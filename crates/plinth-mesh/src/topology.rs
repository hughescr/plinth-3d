//! Immutable face topology over an explicit welded view of a source mesh.
//!
//! The source [`Mesh`](crate::Mesh) is never reordered or repaired here.  A
//! topology view owns only the vertex-to-welded-vertex map needed to make its
//! declared policy true.  Its sidecar digest intentionally excludes allocator
//! estimates and that derived map: schema v4 identifies the four topology
//! arrays plus the canonical mesh fingerprint, and nothing else.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{Mesh, WeldPolicy};

pub const FACE_TOPOLOGY_SCHEMA: &str = "support-placement/face-topology";
pub const FACE_TOPOLOGY_SCHEMA_VERSION: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopologySchema {
    V4,
}

impl TopologySchema {
    #[must_use]
    pub const fn version(self) -> u32 {
        match self {
            Self::V4 => FACE_TOPOLOGY_SCHEMA_VERSION,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyDigest {
    schema: TopologySchema,
    hex: String,
}

impl TopologyDigest {
    #[must_use]
    pub const fn schema(&self) -> TopologySchema {
        self.schema
    }
    #[must_use]
    pub fn as_hex(&self) -> &str {
        &self.hex
    }
}

pub const TOPOLOGY_CUT_BOUNDARY: u8 = 1;
pub const TOPOLOGY_CUT_NONMANIFOLD: u8 = 2;
pub const TOPOLOGY_CUT_SAME_DIRECTION: u8 = 4;
pub const TOPOLOGY_CUT_DUPLICATE: u8 = 8;
pub const TOPOLOGY_CUT_POINT_ONLY: u8 = 16;
pub const TOPOLOGY_CUT_DEGENERATE: u8 = 32;
const TOPOLOGY_KNOWN_CUT_MASK: u8 = TOPOLOGY_CUT_BOUNDARY
    | TOPOLOGY_CUT_NONMANIFOLD
    | TOPOLOGY_CUT_SAME_DIRECTION
    | TOPOLOGY_CUT_DUPLICATE
    | TOPOLOGY_CUT_POINT_ONLY
    | TOPOLOGY_CUT_DEGENERATE;

#[derive(Clone, Debug, Error, PartialEq)]
pub enum TopologyError {
    #[error("topology coordinate at vertex {vertex} axis {axis} is not finite")]
    NonFiniteVertex { vertex: usize, axis: usize },
    #[error("epsilon welding requires a positive finite tolerance, got {tolerance}")]
    InvalidWeldTolerance { tolerance: f64 },
    #[error("epsilon welding grid cannot represent vertex {vertex} axis {axis} coordinate {coordinate} at tolerance {tolerance}")]
    GridCellOutOfRange {
        vertex: usize,
        axis: usize,
        coordinate: f64,
        tolerance: f64,
    },
    #[error("topology vertex count exceeds u32")]
    VertexCountOverflow,
    #[error("topology face count exceeds u32")]
    FaceCountOverflow,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum TopologyValidationError {
    #[error("unsupported face-topology schema or version")]
    UnsupportedSchema,
    #[error("face-topology indptr has invalid length or bounds")]
    InvalidIndptr,
    #[error("face-topology CSR contains an invalid, duplicate, or asymmetric edge")]
    InvalidCsr,
    #[error("face-topology component ids do not match the CSR")]
    InvalidComponents,
    #[error("face-topology contains an unknown cut flag")]
    UnknownCutFlag,
    #[error("face-topology content digest does not match its immutable arrays")]
    DigestMismatch,
    #[error("face-topology source vertex map is invalid")]
    InvalidWeldMap,
}

/// The canonicalized source-geometry identity used by topology.
///
/// This deliberately canonicalizes only signed zero.  Generic content
/// digests retain signed-zero bits; topology is the sole exception because a
/// sign-bit-only seam must not fragment face adjacency.
pub fn mesh_fingerprint(mesh: &Mesh) -> Result<String, TopologyError> {
    let mut hasher = Sha256::new();
    hash_tagged_bytes(
        &mut hasher,
        b"vertices",
        &(mesh.verts().len() as u64).to_le_bytes(),
    );
    for (vertex, point) in mesh.verts().iter().enumerate() {
        for (axis, coordinate) in point.iter().enumerate() {
            if !coordinate.is_finite() {
                return Err(TopologyError::NonFiniteVertex { vertex, axis });
            }
            let canonical = if *coordinate == 0.0 { 0.0 } else { *coordinate };
            hasher.update(canonical.to_bits().to_le_bytes());
        }
    }
    hash_tagged_bytes(
        &mut hasher,
        b"triangles",
        &(mesh.tris().len() as u64).to_le_bytes(),
    );
    for triangle in mesh.tris() {
        for index in triangle {
            hasher.update(index.to_le_bytes());
        }
    }
    Ok(hex_digest(hasher))
}

/// An immutable topology authority constructed once for one explicit view.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceTopology {
    indptr: Vec<u32>,
    indices: Vec<u32>,
    component_ids: Vec<u32>,
    cut_flags: Vec<u8>,
    source_vertex_to_welded: Vec<u32>,
    welded_vertex_count: u32,
    weld_policy: WeldPolicy,
    mesh_fingerprint: String,
    content_digest: TopologyDigest,
}

impl FaceTopology {
    #[must_use]
    pub fn indptr(&self) -> &[u32] {
        &self.indptr
    }
    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }
    #[must_use]
    pub fn component_ids(&self) -> &[u32] {
        &self.component_ids
    }
    #[must_use]
    pub fn cut_flags(&self) -> &[u8] {
        &self.cut_flags
    }
    #[must_use]
    pub fn source_vertex_to_welded(&self) -> &[u32] {
        &self.source_vertex_to_welded
    }
    #[must_use]
    pub const fn welded_vertex_count(&self) -> u32 {
        self.welded_vertex_count
    }
    #[must_use]
    pub const fn weld_policy(&self) -> WeldPolicy {
        self.weld_policy
    }
    #[must_use]
    pub fn mesh_fingerprint(&self) -> &str {
        &self.mesh_fingerprint
    }
    #[must_use]
    pub fn content_digest(&self) -> &str {
        self.content_digest.as_hex()
    }
    #[must_use]
    pub const fn topology_digest(&self) -> &TopologyDigest {
        &self.content_digest
    }
    #[must_use]
    pub fn n_faces(&self) -> usize {
        self.cut_flags.len()
    }
    #[must_use]
    pub fn neighbors(&self, face: usize) -> Option<&[u32]> {
        let start = usize::try_from(*self.indptr.get(face)?).ok()?;
        let end = usize::try_from(*self.indptr.get(face + 1)?).ok()?;
        self.indices.get(start..end)
    }
}

/// Build the topology once.  The returned view records, and genuinely applies,
/// the supplied welding policy; the source mesh is left unchanged.
pub fn build_face_topology(
    mesh: &Mesh,
    weld_policy: WeldPolicy,
) -> Result<FaceTopology, TopologyError> {
    let fingerprint = mesh_fingerprint(mesh)?;
    let welded = WeldedView::build(mesh, weld_policy)?;
    let n_faces = mesh.tris().len();
    u32::try_from(n_faces).map_err(|_| TopologyError::FaceCountOverflow)?;

    let mut cut_flags = vec![0_u8; n_faces];
    let mut canonical_faces = Vec::with_capacity(n_faces);
    for triangle in mesh.tris() {
        canonical_faces.push([
            welded.map[triangle[0] as usize],
            welded.map[triangle[1] as usize],
            welded.map[triangle[2] as usize],
        ]);
    }

    let mut triangles_by_key: BTreeMap<[u32; 3], Vec<usize>> = BTreeMap::new();
    for (face, triangle) in canonical_faces.iter().enumerate() {
        let mut key = *triangle;
        key.sort_unstable();
        triangles_by_key.entry(key).or_default().push(face);
    }
    for faces in triangles_by_key.values() {
        if faces.len() > 1 {
            for &face in faces {
                cut_flags[face] |= TOPOLOGY_CUT_DUPLICATE;
            }
        }
    }

    #[derive(Clone, Copy)]
    struct EdgeOccurrence {
        face: usize,
        forward: bool,
        degenerate: bool,
    }
    let mut edges: BTreeMap<(u32, u32), Vec<EdgeOccurrence>> = BTreeMap::new();
    for (face, triangle) in canonical_faces.iter().enumerate() {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            edges
                .entry((a.min(b), a.max(b)))
                .or_default()
                .push(EdgeOccurrence {
                    face,
                    forward: a < b,
                    degenerate: a == b,
                });
        }
    }

    let mut adjacency = vec![Vec::<u32>::new(); n_faces];
    let mut accepted_edges = Vec::<(usize, usize)>::new();
    for occurrences in edges.values() {
        let degenerate = occurrences.iter().any(|occurrence| occurrence.degenerate);
        let duplicate = occurrences
            .iter()
            .any(|occurrence| cut_flags[occurrence.face] & TOPOLOGY_CUT_DUPLICATE != 0);
        let valid_pair = occurrences.len() == 2
            && occurrences[0].face != occurrences[1].face
            && occurrences[0].forward != occurrences[1].forward
            && !degenerate
            && !duplicate;
        if valid_pair {
            let a = occurrences[0].face;
            let b = occurrences[1].face;
            adjacency[a].push(b as u32);
            adjacency[b].push(a as u32);
            accepted_edges.push((a, b));
            continue;
        }
        for occurrence in occurrences {
            if occurrences.len() == 1 {
                cut_flags[occurrence.face] |= TOPOLOGY_CUT_BOUNDARY;
            }
            if degenerate {
                cut_flags[occurrence.face] |= TOPOLOGY_CUT_DEGENERATE;
            }
            if duplicate {
                cut_flags[occurrence.face] |= TOPOLOGY_CUT_DUPLICATE;
            }
            if occurrences.len() == 2
                && occurrences[0].face != occurrences[1].face
                && occurrences[0].forward == occurrences[1].forward
            {
                cut_flags[occurrence.face] |= TOPOLOGY_CUT_SAME_DIRECTION;
            } else if occurrences.len() != 1
                && !degenerate
                && !duplicate
                && !(occurrences.len() == 2
                    && occurrences[0].face != occurrences[1].face
                    && occurrences[0].forward == occurrences[1].forward)
            {
                cut_flags[occurrence.face] |= TOPOLOGY_CUT_NONMANIFOLD;
            }
        }
    }

    // An accepted manifold edge connects the two incident face nodes at both
    // endpoint fans.  More than one local fan at a vertex is a point-only cut.
    let mut incidence: BTreeMap<u32, BTreeMap<usize, BTreeSet<usize>>> = BTreeMap::new();
    for (face, triangle) in canonical_faces.iter().enumerate() {
        for vertex in triangle {
            incidence
                .entry(*vertex)
                .or_default()
                .entry(face)
                .or_default();
        }
    }
    for (a, b) in &accepted_edges {
        let ta = canonical_faces[*a];
        let tb = canonical_faces[*b];
        for vertex in [ta[0], ta[1], ta[2]] {
            if tb.contains(&vertex) {
                let fan = incidence.get_mut(&vertex).expect("face incidence inserted");
                fan.get_mut(a).expect("face incidence inserted").insert(*b);
                fan.get_mut(b).expect("face incidence inserted").insert(*a);
            }
        }
    }
    for fan in incidence.values() {
        let mut unseen: BTreeSet<usize> = fan.keys().copied().collect();
        let mut components = 0;
        while let Some(&start) = unseen.iter().next() {
            components += 1;
            let mut queue = VecDeque::from([start]);
            unseen.remove(&start);
            while let Some(face) = queue.pop_front() {
                for neighbor in &fan[&face] {
                    if unseen.remove(neighbor) {
                        queue.push_back(*neighbor);
                    }
                }
            }
        }
        if components > 1 {
            for face in fan.keys() {
                cut_flags[*face] |= TOPOLOGY_CUT_POINT_ONLY;
            }
        }
    }

    for row in &mut adjacency {
        row.sort_unstable();
        row.dedup();
    }
    let mut indptr = Vec::with_capacity(n_faces + 1);
    indptr.push(0);
    let mut indices = Vec::new();
    for row in &adjacency {
        indices.extend(row);
        indptr.push(u32::try_from(indices.len()).map_err(|_| TopologyError::FaceCountOverflow)?);
    }
    let component_ids = component_labels(&adjacency)?;
    let mut topology = FaceTopology {
        indptr,
        indices,
        component_ids,
        cut_flags,
        source_vertex_to_welded: welded.map,
        welded_vertex_count: welded.vertex_count,
        weld_policy,
        mesh_fingerprint: fingerprint,
        content_digest: TopologyDigest {
            schema: TopologySchema::V4,
            hex: String::new(),
        },
    };
    topology.content_digest = topology_digest(&topology);
    debug_assert!(validate_face_topology(&topology).is_ok());
    Ok(topology)
}

/// Expensive diagnostic validation, intentionally not a normal hot-path API.
pub fn validate_face_topology(topology: &FaceTopology) -> Result<(), TopologyValidationError> {
    if FACE_TOPOLOGY_SCHEMA.is_empty()
        || topology.content_digest.schema() != TopologySchema::V4
        || topology.content_digest.schema().version() != FACE_TOPOLOGY_SCHEMA_VERSION
    {
        return Err(TopologyValidationError::UnsupportedSchema);
    }
    let n = topology.cut_flags.len();
    if topology.indptr.len() != n + 1
        || topology.indptr.first() != Some(&0)
        || topology
            .indptr
            .last()
            .copied()
            .map(usize::try_from)
            .transpose()
            .ok()
            .flatten()
            != Some(topology.indices.len())
    {
        return Err(TopologyValidationError::InvalidIndptr);
    }
    if topology
        .indptr
        .windows(2)
        .any(|window| window[0] > window[1])
    {
        return Err(TopologyValidationError::InvalidIndptr);
    }
    let mut adjacency = vec![Vec::new(); n];
    for (face, row) in adjacency.iter_mut().enumerate() {
        let neighbors = topology
            .neighbors(face)
            .ok_or(TopologyValidationError::InvalidIndptr)?;
        let mut previous = None;
        for &neighbor in neighbors {
            let neighbor = neighbor as usize;
            if neighbor >= n || neighbor == face || previous.is_some_and(|prior| prior >= neighbor)
            {
                return Err(TopologyValidationError::InvalidCsr);
            }
            previous = Some(neighbor);
            row.push(neighbor as u32);
        }
    }
    for (face, neighbors) in adjacency.iter().enumerate() {
        if neighbors
            .iter()
            .any(|neighbor| !adjacency[*neighbor as usize].contains(&(face as u32)))
        {
            return Err(TopologyValidationError::InvalidCsr);
        }
    }
    if topology.component_ids
        != component_labels(&adjacency).map_err(|_| TopologyValidationError::InvalidComponents)?
    {
        return Err(TopologyValidationError::InvalidComponents);
    }
    if topology
        .cut_flags
        .iter()
        .any(|flag| *flag & !TOPOLOGY_KNOWN_CUT_MASK != 0)
    {
        return Err(TopologyValidationError::UnknownCutFlag);
    }
    if topology
        .source_vertex_to_welded
        .iter()
        .any(|vertex| *vertex >= topology.welded_vertex_count)
    {
        return Err(TopologyValidationError::InvalidWeldMap);
    }
    if topology.content_digest != topology_digest(topology) {
        return Err(TopologyValidationError::DigestMismatch);
    }
    Ok(())
}

pub(crate) struct WeldedView {
    pub(crate) verts: Vec<[f64; 3]>,
    pub(crate) map: Vec<u32>,
    pub(crate) vertex_count: u32,
}

impl WeldedView {
    pub(crate) fn build(mesh: &Mesh, policy: WeldPolicy) -> Result<Self, TopologyError> {
        for (vertex, point) in mesh.verts().iter().enumerate() {
            for (axis, value) in point.iter().enumerate() {
                if !value.is_finite() {
                    return Err(TopologyError::NonFiniteVertex { vertex, axis });
                }
            }
        }
        match policy {
            WeldPolicy::None => {
                let map = (0..mesh.verts().len())
                    .map(|index| {
                        u32::try_from(index).map_err(|_| TopologyError::VertexCountOverflow)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Self {
                    verts: mesh.verts().to_vec(),
                    vertex_count: map.len() as u32,
                    map,
                })
            }
            WeldPolicy::ExactBits => exact_weld(mesh.verts()),
            WeldPolicy::Epsilon(tolerance) => epsilon_weld(mesh.verts(), tolerance),
        }
    }
}

fn exact_weld(points: &[[f64; 3]]) -> Result<WeldedView, TopologyError> {
    let mut ids = BTreeMap::<[u64; 3], u32>::new();
    let mut verts = Vec::new();
    let mut map = Vec::with_capacity(points.len());
    for point in points {
        let key = point.map(|coordinate| {
            if coordinate == 0.0 {
                0.0_f64.to_bits()
            } else {
                coordinate.to_bits()
            }
        });
        let id = match ids.get(&key) {
            Some(id) => *id,
            None => {
                let id =
                    u32::try_from(verts.len()).map_err(|_| TopologyError::VertexCountOverflow)?;
                ids.insert(key, id);
                verts.push(*point);
                id
            }
        };
        map.push(id);
    }
    Ok(WeldedView {
        vertex_count: verts.len() as u32,
        verts,
        map,
    })
}

fn epsilon_weld(points: &[[f64; 3]], tolerance: f64) -> Result<WeldedView, TopologyError> {
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err(TopologyError::InvalidWeldTolerance { tolerance });
    }
    let mut parent = (0..points.len()).collect::<Vec<_>>();
    let mut cells = BTreeMap::<(i64, i64, i64), Vec<usize>>::new();
    for (index, point) in points.iter().enumerate() {
        let cell = grid_cell(*point, tolerance, index)?;
        for dx in -1_i64..=1 {
            for dy in -1_i64..=1 {
                for dz in -1_i64..=1 {
                    let Some(x) = cell.0.checked_add(dx) else {
                        continue;
                    };
                    let Some(y) = cell.1.checked_add(dy) else {
                        continue;
                    };
                    let Some(z) = cell.2.checked_add(dz) else {
                        continue;
                    };
                    if let Some(candidates) = cells.get(&(x, y, z)) {
                        for &other in candidates {
                            let delta = [
                                point[0] - points[other][0],
                                point[1] - points[other][1],
                                point[2] - points[other][2],
                            ];
                            if delta[0].hypot(delta[1]).hypot(delta[2]) <= tolerance {
                                union(&mut parent, index, other);
                            }
                        }
                    }
                }
            }
        }
        cells.entry(cell).or_default().push(index);
    }
    let mut roots = BTreeMap::<usize, u32>::new();
    let mut verts = Vec::new();
    let mut map = Vec::with_capacity(points.len());
    for (index, point) in points.iter().enumerate() {
        let root = find(&mut parent, index);
        let id = match roots.get(&root) {
            Some(id) => *id,
            None => {
                let id =
                    u32::try_from(verts.len()).map_err(|_| TopologyError::VertexCountOverflow)?;
                roots.insert(root, id);
                verts.push(*point);
                id
            }
        };
        map.push(id);
    }
    Ok(WeldedView {
        vertex_count: verts.len() as u32,
        verts,
        map,
    })
}

fn grid_cell(
    point: [f64; 3],
    tolerance: f64,
    vertex: usize,
) -> Result<(i64, i64, i64), TopologyError> {
    let values = [
        point[0] / tolerance,
        point[1] / tolerance,
        point[2] / tolerance,
    ];
    for (axis, value) in values.iter().enumerate() {
        if !value.is_finite() || *value < (i64::MIN + 1) as f64 || *value > (i64::MAX - 1) as f64 {
            return Err(TopologyError::GridCellOutOfRange {
                vertex,
                axis,
                coordinate: point[axis],
                tolerance,
            });
        }
    }
    Ok((
        values[0].floor() as i64,
        values[1].floor() as i64,
        values[2].floor() as i64,
    ))
}

fn find(parent: &mut [usize], node: usize) -> usize {
    if parent[node] != node {
        let root = find(parent, parent[node]);
        parent[node] = root;
    }
    parent[node]
}
fn union(parent: &mut [usize], left: usize, right: usize) {
    let left = find(parent, left);
    let right = find(parent, right);
    if left != right {
        parent[right] = left.min(right);
        parent[left] = left.min(right);
    }
}

fn component_labels(adjacency: &[Vec<u32>]) -> Result<Vec<u32>, TopologyError> {
    let mut labels = vec![u32::MAX; adjacency.len()];
    let mut label = 0_u32;
    for start in 0..adjacency.len() {
        if labels[start] != u32::MAX {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        labels[start] = label;
        while let Some(face) = queue.pop_front() {
            for &neighbor in &adjacency[face] {
                let neighbor = neighbor as usize;
                if labels[neighbor] == u32::MAX {
                    labels[neighbor] = label;
                    queue.push_back(neighbor);
                }
            }
        }
        label = label
            .checked_add(1)
            .ok_or(TopologyError::FaceCountOverflow)?;
    }
    Ok(labels)
}

fn topology_digest(topology: &FaceTopology) -> TopologyDigest {
    let mut hasher = Sha256::new();
    hasher.update(b"plinth/topology-digest\0");
    hasher.update(TopologySchema::V4.version().to_le_bytes());
    hash_bytes(&mut hasher, topology.mesh_fingerprint.as_bytes());
    hash_u32_array(&mut hasher, &topology.indptr);
    hash_u32_array(&mut hasher, &topology.indices);
    hash_u32_array(&mut hasher, &topology.component_ids);
    hash_bytes(&mut hasher, &topology.cut_flags);
    TopologyDigest {
        schema: TopologySchema::V4,
        hex: hex_digest(hasher),
    }
}

fn hash_u32_array(hasher: &mut Sha256, values: &[u32]) {
    hasher.update((values.len() as u64).to_le_bytes());
    for value in values {
        hasher.update(value.to_le_bytes());
    }
}
fn hash_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}
fn hash_tagged_bytes(hasher: &mut Sha256, tag: &[u8], bytes: &[u8]) {
    hasher.update((tag.len() as u64).to_le_bytes());
    hasher.update(tag);
    hash_bytes(hasher, bytes);
}
fn hex_digest(hasher: Sha256) -> String {
    let bytes = hasher.finalize();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("string formatting");
    }
    output
}
