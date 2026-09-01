//! Immutable face topology over an explicit welded view of a source mesh.
//!
//! The source [`Mesh`](crate::Mesh) is never reordered or repaired here.  A
//! topology view owns only the vertex-to-welded-vertex map needed to make its
//! declared policy true.  Its sidecar digest intentionally excludes allocator
//! estimates and that derived map: schema v4 identifies the four topology
//! arrays plus the canonical mesh fingerprint, and nothing else.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use geom_kernel::vec3::{cross, dot, norm, sub};
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

/// Counts established by Plinth's canonical face-topology construction.
///
/// `boundary_edges` and `nonmanifold_edges` count undirected edge incidences.
/// `duplicate_faces` and `duplicate_face_groups` count faces and exact
/// duplicate groups: a group is a set of two or more faces whose welded vertex
/// triples are equal as unordered sets, and `duplicate_faces` counts every
/// member of every such group.
///
/// Those four are not counts of faces bearing a cut flag.  A face can carry
/// `TOPOLOGY_CUT_DUPLICATE` because it meets a duplicate across one ambiguous
/// edge without being a duplicate itself, so the flag is deliberately broader
/// than `duplicate_faces`.
///
/// The three `*_cut_faces` fields *are* face counts, one per cut class that no
/// edge count above covers, taken in one pass over the finished `cut_flags`
/// array: a face is counted in a class exactly when it carries that class's
/// flag, and a face carrying two classes is counted in both.  They exist
/// because `validate_closed_mesh` refuses **any** nonzero cut flag while the
/// counts a producer could compare against described only two of the six
/// classes: a stage could hand on a mesh whose pinch, same-direction or
/// degenerate cut count had gone up, with every count it published unmoved.
/// `TOPOLOGY_CUT_BOUNDARY` needs no field — a face carries it exactly when one
/// of its edges is a boundary edge — and `TOPOLOGY_CUT_NONMANIFOLD` sits beside
/// `nonmanifold_edges`, which is the incidence form of the same evidence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TopologyCounts {
    pub boundary_edges: usize,
    pub nonmanifold_edges: usize,
    pub duplicate_faces: usize,
    pub duplicate_face_groups: usize,
    /// Faces flagged `TOPOLOGY_CUT_POINT_ONLY`: their vertex fan falls into
    /// more than one component, i.e. the surface pinches to a point there.
    pub point_only_cut_faces: usize,
    /// Faces flagged `TOPOLOGY_CUT_SAME_DIRECTION`: they meet exactly one other
    /// face across an edge both traverse the same way, so the two cannot be
    /// consistently oriented.
    pub same_direction_cut_faces: usize,
    /// Faces flagged `TOPOLOGY_CUT_DEGENERATE`: some edge of the face collapses
    /// to a single welded vertex.
    pub degenerate_cut_faces: usize,
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

/// The cut classes that refuse a mesh at the closed-solid gate
/// (`plinth_hollow::validate_closed_mesh`).
///
/// This is **not** [`TOPOLOGY_KNOWN_CUT_MASK`] with a bit subtracted: each
/// class is written out so that a new cut class added to the topology in the
/// future is a deliberate decision made here, not one silently inherited from
/// the "every known flag" mask.
///
/// `TOPOLOGY_CUT_POINT_ONLY` is deliberately absent. Per the owner ruling of
/// 2026-08-12 (`docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`,
/// §4, FU-47): a point-touch is not a modelling defect — resin cannot flow
/// through a point, so a vertex fan that falls into more than one component
/// there is a legitimate fingertip touch, not a hole in the solid. The flag
/// is still measured (`TopologyCounts::point_only_cut_faces` is untouched)
/// and still gates decimation's no-regression check; it simply no longer
/// refuses the closed-solid gate. Closure and orientation are not relaxed by
/// this mask, and no threshold moves.
pub const TOPOLOGY_CUT_CLOSED_SOLID_MASK: u8 = TOPOLOGY_CUT_BOUNDARY
    | TOPOLOGY_CUT_NONMANIFOLD
    | TOPOLOGY_CUT_SAME_DIRECTION
    | TOPOLOGY_CUT_DUPLICATE
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
    #[error("closed-component certificate requires a validated topology")]
    InvalidCertificateTopology,
    #[error("closed-component certificate topology does not describe this mesh")]
    CertificateMeshMismatch,
    #[error("closed-component certificate requires every component to be closed and oriented")]
    CertificateNotClosedOrOriented,
    #[error("closed-component {component} volume arithmetic is not finite")]
    CertificateVolumeArithmetic { component: u32 },
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
    #[error("face-topology pinch fans disagree with its point-only cut flags")]
    InvalidPointOnlyFans,
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

/// One pinch vertex — a welded vertex whose incident faces form more than one
/// locally-connected fan — together with those fans.
///
/// This is the same fact `TOPOLOGY_CUT_POINT_ONLY` records, carried at the
/// resolution a repair needs: the flag says *this face touches a pinch*, which
/// cannot tell a repair which copy of the vertex each face should follow. The
/// fan partition is computed exactly once, inside
/// [`build_face_topology`]'s own incidence pass, so no consumer re-derives a
/// second (possibly disagreeing) notion of a fan — owner override O-5.
///
/// Ordering is total and deterministic: `fans` are sorted by their smallest
/// face id, each fan's faces ascend, and
/// [`FaceTopology::point_only_vertices`] ascends by welded vertex id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointOnlyVertex {
    welded_vertex: u32,
    fans: Vec<Vec<u32>>,
}

impl PointOnlyVertex {
    /// The welded vertex the fans meet at.
    #[must_use]
    pub const fn welded_vertex(&self) -> u32 {
        self.welded_vertex
    }

    /// The disjoint face fans meeting at this vertex, at least two of them.
    #[must_use]
    pub fn fans(&self) -> &[Vec<u32>] {
        &self.fans
    }
}

/// An immutable topology authority constructed once for one explicit view.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceTopology {
    indptr: Vec<u32>,
    indices: Vec<u32>,
    component_ids: Vec<u32>,
    cut_flags: Vec<u8>,
    counts: TopologyCounts,
    point_only_vertices: Vec<PointOnlyVertex>,
    source_vertex_to_welded: Vec<u32>,
    welded_vertex_count: u32,
    weld_policy: WeldPolicy,
    mesh_fingerprint: String,
    content_digest: TopologyDigest,
}

/// One closed, consistently oriented face component certified from the
/// canonical topology arrays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClosedComponent {
    component_id: u32,
    face_count: usize,
    signed_volume_mm3: f64,
}

impl ClosedComponent {
    #[must_use]
    pub const fn component_id(self) -> u32 {
        self.component_id
    }

    #[must_use]
    pub const fn face_count(self) -> usize {
        self.face_count
    }

    #[must_use]
    pub const fn signed_volume_mm3(self) -> f64 {
        self.signed_volume_mm3
    }
}

/// Canonical closed-component signs and volumes for one exact mesh/topology
/// pair. Containment is deliberately not inferred here; callers use the
/// existing triage containment authority when they need that evidence.
#[derive(Clone, Debug, PartialEq)]
pub struct ClosedComponentCertificate {
    components: Vec<ClosedComponent>,
}

impl ClosedComponentCertificate {
    #[must_use]
    pub fn components(&self) -> &[ClosedComponent] {
        &self.components
    }

    #[must_use]
    pub fn signed_volume_mm3(&self) -> f64 {
        self.components
            .iter()
            .map(|component| component.signed_volume_mm3)
            .sum()
    }
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
    /// Edge-incidence and duplicate-face counts established by this topology
    /// view's one canonical construction.  Consumers must use these rather
    /// than re-auditing raw triangle indices locally.
    #[must_use]
    pub const fn counts(&self) -> TopologyCounts {
        self.counts
    }
    /// Every pinch vertex and its fans, ascending by welded vertex id.
    ///
    /// Empty exactly when no face carries `TOPOLOGY_CUT_POINT_ONLY`. The
    /// content digest deliberately does not cover this list: it is a
    /// re-derivable projection of the same incidence pass that wrote the cut
    /// flags the digest already hashes, and schema v4's identity is the four
    /// topology arrays plus the mesh fingerprint.
    #[must_use]
    pub fn point_only_vertices(&self) -> &[PointOnlyVertex] {
        &self.point_only_vertices
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
#[expect(
    clippy::cast_possible_truncation,
    reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged as a \
          class, and the one architecture item the class carried (geom_kernel's 32-bit BVH index \
          width) is now a named, enforced limit. TODO(cast-width-audit): retire site by site.  See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
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

    // The duplicate-face set is established once, in full, before any edge is
    // classified: `duplicate_face[f]` says that `f` is itself one of two or
    // more faces sharing a welded vertex triple.  Everything that must not
    // depend on iteration order reads this frozen snapshot rather than
    // `cut_flags`, which the edge pass below is still writing.
    let mut triangles_by_key: BTreeMap<[u32; 3], Vec<usize>> = BTreeMap::new();
    for (face, triangle) in canonical_faces.iter().enumerate() {
        let mut key = *triangle;
        key.sort_unstable();
        triangles_by_key.entry(key).or_default().push(face);
    }
    let mut duplicate_face = vec![false; n_faces];
    let mut counts = TopologyCounts::default();
    for faces in triangles_by_key.values() {
        if faces.len() > 1 {
            counts.duplicate_face_groups += 1;
            counts.duplicate_faces += faces.len();
            for &face in faces {
                duplicate_face[face] = true;
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

    // First pass: cut flags and adjacency.  A face that meets a duplicate
    // across one edge is itself cut there, and `cut_flags` carries that
    // outward through the pass, so an edge reached later sees a face already
    // marked ambiguous and refuses to accept it as a manifold partner.  That
    // propagation only ever severs adjacency, never restores it, so it is kept
    // exactly as it stands; what it must not do is decide a *count*, which is
    // why the counting pass below reads `duplicate_face` instead.
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

    // Second pass: the counts, taken only once every flag has been written and
    // taken against the frozen duplicate-face set rather than the flags the
    // pass above was still writing.  Counting inside that pass made an edge's
    // verdict depend on how many earlier edges had already spread
    // `TOPOLOGY_CUT_DUPLICATE` onto one of its faces, so a genuinely
    // non-manifold edge reached late fell out of the total: deterministic in
    // `BTreeMap` key order, but a floor on the damage rather than a count of
    // it.  Every exclusion below is now reported somewhere — a degenerate edge
    // and a duplicate face each carry their own named cut, and duplicate faces
    // are counted in this same struct — so excluding them from
    // `nonmanifold_edges` no longer hides them.
    for occurrences in edges.values() {
        let degenerate = occurrences.iter().any(|occurrence| occurrence.degenerate);
        let duplicate = occurrences
            .iter()
            .any(|occurrence| duplicate_face[occurrence.face]);
        if occurrences.len() == 1 {
            counts.boundary_edges += 1;
        }
        if occurrences.len() > 2 && !degenerate && !duplicate {
            counts.nonmanifold_edges += 1;
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
    let mut point_only_vertices = Vec::new();
    for (vertex, fan) in &incidence {
        let mut unseen: BTreeSet<usize> = fan.keys().copied().collect();
        // The fans are materialized rather than merely counted. Counting
        // answers "is this a pinch"; a repair additionally has to know which
        // faces belong to which sheet, and deriving that a second time
        // elsewhere is exactly the duplicated-capability hazard O-5 forbids.
        let mut fans: Vec<Vec<u32>> = Vec::new();
        while let Some(&start) = unseen.iter().next() {
            let mut members = Vec::new();
            let mut queue = VecDeque::from([start]);
            unseen.remove(&start);
            while let Some(face) = queue.pop_front() {
                members.push(face as u32);
                for neighbor in &fan[&face] {
                    if unseen.remove(neighbor) {
                        queue.push_back(*neighbor);
                    }
                }
            }
            members.sort_unstable();
            fans.push(members);
        }
        if fans.len() > 1 {
            for face in fan.keys() {
                cut_flags[*face] |= TOPOLOGY_CUT_POINT_ONLY;
            }
            // `unseen` is a `BTreeSet`, so the fans are already discovered in
            // ascending smallest-face order; sorting says so rather than
            // relying on it.
            fans.sort_by(|left, right| left[0].cmp(&right[0]));
            point_only_vertices.push(PointOnlyVertex {
                welded_vertex: *vertex,
                fans,
            });
        }
    }

    // Third pass: the per-class face counts, taken once, after the point-only
    // pass has written the last flag any face can acquire.  One scan over the
    // finished array — the classes are read from the same authority that
    // `validate_closed_mesh` reads, so a producer comparing these counts is
    // comparing the quantity the gate downstream actually refuses on.
    for flags in &cut_flags {
        if flags & TOPOLOGY_CUT_POINT_ONLY != 0 {
            counts.point_only_cut_faces += 1;
        }
        if flags & TOPOLOGY_CUT_SAME_DIRECTION != 0 {
            counts.same_direction_cut_faces += 1;
        }
        if flags & TOPOLOGY_CUT_DEGENERATE != 0 {
            counts.degenerate_cut_faces += 1;
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
        counts,
        point_only_vertices,
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
#[expect(
    clippy::cast_possible_truncation,
    reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged as a \
          class, and the one architecture item the class carried (geom_kernel's 32-bit BVH index \
          width) is now a named, enforced limit. TODO(cast-width-audit): retire site by site.  See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
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
    validate_point_only_vertices(topology, n)?;
    if topology.content_digest != topology_digest(topology) {
        return Err(TopologyValidationError::DigestMismatch);
    }
    Ok(())
}

/// The pinch-fan list must describe the same pinches the cut flags do, in the
/// one total order its consumers are entitled to rely on.
fn validate_point_only_vertices(
    topology: &FaceTopology,
    face_count: usize,
) -> Result<(), TopologyValidationError> {
    let mut previous_vertex = None;
    let mut fan_faces = BTreeSet::new();
    for pinch in &topology.point_only_vertices {
        if pinch.welded_vertex >= topology.welded_vertex_count
            || previous_vertex.is_some_and(|prior| prior >= pinch.welded_vertex)
        {
            return Err(TopologyValidationError::InvalidPointOnlyFans);
        }
        previous_vertex = Some(pinch.welded_vertex);
        if pinch.fans.len() < 2 {
            return Err(TopologyValidationError::InvalidPointOnlyFans);
        }
        let mut previous_first = None;
        for fan in &pinch.fans {
            let Some(first) = fan.first().copied() else {
                return Err(TopologyValidationError::InvalidPointOnlyFans);
            };
            if previous_first.is_some_and(|prior| prior >= first) {
                return Err(TopologyValidationError::InvalidPointOnlyFans);
            }
            previous_first = Some(first);
            let mut previous_face = None;
            for &face in fan {
                if face as usize >= face_count
                    || previous_face.is_some_and(|prior| prior >= face)
                    || topology.cut_flags[face as usize] & TOPOLOGY_CUT_POINT_ONLY == 0
                {
                    return Err(TopologyValidationError::InvalidPointOnlyFans);
                }
                previous_face = Some(face);
                fan_faces.insert(face);
            }
        }
    }
    // **The cross-check has to compare two independently derived quantities.**
    // The per-face check above only proves the fans are a *subset* of the
    // flagged faces, and the boolean check below only proves "any flag" agrees
    // with "any pinch"; a flagged face belonging to no fan would slip past both
    // as long as one pinch existed. That is exactly the disagreement that
    // matters, because the repair pre-pass drives edits from
    // `point_only_vertices` while the gate refuses on `cut_flags` — one of them
    // naming a face the other does not is the two representations describing
    // different damage. So the flag side is counted from `cut_flags` and the
    // fan side from the fans, over distinct faces because a face can carry the
    // flag from more than one pinch vertex.
    let flagged_faces = topology
        .cut_flags
        .iter()
        .filter(|flags| **flags & TOPOLOGY_CUT_POINT_ONLY != 0)
        .count();
    if flagged_faces != fan_faces.len() {
        return Err(TopologyValidationError::InvalidPointOnlyFans);
    }
    if (flagged_faces != 0) != !topology.point_only_vertices.is_empty() {
        return Err(TopologyValidationError::InvalidPointOnlyFans);
    }
    Ok(())
}

/// Certify the signed volume of every closed, consistently oriented face
/// component described by `topology`.
///
/// Each component uses one of its own vertices as the tetrahedral reference
/// origin, so translating the complete mesh does not inject a large global
/// coordinate into every volume term. This function never repairs, rewinds,
/// welds, or performs containment classification.
///
/// **What "closed" means here is [`TOPOLOGY_CUT_CLOSED_SOLID_MASK`], the same
/// definition the closed-solid gate uses.** Per the owner ruling of
/// 2026-08-12 (FU-47,
/// `docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`,
/// §4) a point-touch is not a defect, and a certificate is where that ruling
/// has to hold or it does not hold at all: `plinth_hollow`'s
/// `validate_oriented_solid` passes the gate and then immediately certifies,
/// so a certificate refusing what the gate admits refuses the model anyway,
/// one line later and under a less accurate name. Nothing about closure or
/// orientation is relaxed — a point-touch mesh is closed and consistently
/// oriented, it simply has more face components than it has solids, and each
/// of those components is closed, which is exactly what this function
/// integrates over.
#[expect(
    clippy::cast_possible_truncation,
    reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged as a \
          class, and the one architecture item the class carried (geom_kernel's 32-bit BVH index \
          width) is now a named, enforced limit. TODO(cast-width-audit): retire site by site.  See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
pub fn certify_closed_components(
    mesh: &Mesh,
    topology: &FaceTopology,
) -> Result<ClosedComponentCertificate, TopologyError> {
    validate_face_topology(topology).map_err(|_| TopologyError::InvalidCertificateTopology)?;
    if topology.mesh_fingerprint != mesh_fingerprint(mesh)?
        || topology.component_ids.len() != mesh.tris().len()
    {
        return Err(TopologyError::CertificateMeshMismatch);
    }
    if topology
        .cut_flags
        .iter()
        .any(|flags| *flags & TOPOLOGY_CUT_CLOSED_SOLID_MASK != 0)
    {
        return Err(TopologyError::CertificateNotClosedOrOriented);
    }
    let component_count = topology
        .component_ids
        .iter()
        .copied()
        .max()
        .map_or(0_usize, |component| component as usize + 1);
    let mut origins = vec![None; component_count];
    let mut face_counts = vec![0_usize; component_count];
    for (face, triangle) in mesh.tris().iter().enumerate() {
        let component = topology.component_ids[face] as usize;
        origins[component].get_or_insert(mesh.verts()[triangle[0] as usize]);
        face_counts[component] = face_counts[component]
            .checked_add(1)
            .ok_or(TopologyError::FaceCountOverflow)?;
    }
    let mut volumes = vec![0.0_f64; component_count];
    for (face, triangle) in mesh.tris().iter().enumerate() {
        let component = topology.component_ids[face] as usize;
        let origin = origins[component].expect("every component labels at least one face");
        let a_point = mesh.verts()[triangle[0] as usize];
        let b_point = mesh.verts()[triangle[1] as usize];
        let c_point = mesh.verts()[triangle[2] as usize];
        // `geom_kernel::vec3` (docs/specs/geometry-canon.md §1 OP-9, plan item
        // C8a). Byte-neutral: the retired inline forms had exactly the kernel's
        // subtraction, cross-product operand order and left-to-right dot
        // association.
        let a = sub(a_point, origin);
        let b = sub(b_point, origin);
        let c = sub(c_point, origin);
        let term = dot(a, cross(b, c)) / 6.0;
        let next = volumes[component] + term;
        if !term.is_finite() || !next.is_finite() {
            return Err(TopologyError::CertificateVolumeArithmetic {
                component: component as u32,
            });
        }
        volumes[component] = next;
    }
    let components = volumes
        .into_iter()
        .enumerate()
        .map(|(component, signed_volume_mm3)| ClosedComponent {
            component_id: component as u32,
            face_count: face_counts[component],
            signed_volume_mm3,
        })
        .collect();
    Ok(ClosedComponentCertificate { components })
}

/// A deterministic vertex-only projection of a mesh under an explicit weld
/// policy.
///
/// This view never mutates or replaces the source [`Mesh`]. It exists for
/// algorithms which require shared positional indexing but must retain the
/// source triangle soup as their oracle representation. `map()[source_id]`
/// is the corresponding ID in `verts()`; welded IDs are assigned by the first
/// source-order occurrence of each equivalence class.
#[derive(Clone, Debug, PartialEq)]
pub struct WeldedView {
    pub(crate) verts: Vec<[f64; 3]>,
    pub(crate) map: Vec<u32>,
    pub(crate) vertex_count: u32,
}

impl WeldedView {
    /// Builds one explicit vertex-only weld view without changing `mesh`.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged \
              as a class, and the one architecture item the class carried (geom_kernel's 32-bit BVH \
              index width) is now a named, enforced limit. TODO(cast-width-audit): retire site by \
              site.  See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    pub fn build(mesh: &Mesh, policy: WeldPolicy) -> Result<Self, TopologyError> {
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

    /// Welded vertices in deterministic first-occurrence order.
    #[must_use]
    pub fn verts(&self) -> &[[f64; 3]] {
        &self.verts
    }

    /// Source vertex ID to welded vertex ID.
    #[must_use]
    pub fn map(&self) -> &[u32] {
        &self.map
    }

    /// Number of welded vertices.
    #[must_use]
    pub const fn vertex_count(&self) -> u32 {
        self.vertex_count
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged as a \
          class, and the one architecture item the class carried (geom_kernel's 32-bit BVH index \
          width) is now a named, enforced limit. TODO(cast-width-audit): retire site by site.  See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
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
                verts.push(canonical_zero_point(*point));
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

#[expect(
    clippy::cast_possible_truncation,
    reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged as a \
          class, and the one architecture item the class carried (geom_kernel's 32-bit BVH index \
          width) is now a named, enforced limit. TODO(cast-width-audit): retire site by site.  See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
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
                            // Canonical distance: `geom_kernel::vec3::norm`
                            // over `sub` (docs/specs/geometry-canon.md §1
                            // OP-6, triage rule (c) — the `hypot` chain that
                            // was here is the retired third formula). This
                            // comparison decides whether two vertices weld, so
                            // it is a tolerance decision and had to move: two
                            // spellings of "distance" disagreeing by 2 ULP at
                            // a weld threshold is exactly the class of
                            // divergence the sweep exists to remove. Input
                            // class now answered differently: vertex pairs
                            // whose separation is within 2 ULP of `tolerance`
                            // may weld where they did not, or the reverse.
                            let delta = sub(*point, points[other]);
                            if norm(delta) <= tolerance {
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
                verts.push(canonical_zero_point(*point));
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

#[expect(
    clippy::cast_possible_truncation,
    reason = "float-to-integer casts on lattice or step arithmetic: Rust's `as` saturates at the integer \
          bounds and maps NaN to 0, and every site in this function is range-checked before the cast \
          or clamped into range after it (read site by site, 2026-08-26). See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn grid_cell(
    point: [f64; 3],
    tolerance: f64,
    vertex: usize,
) -> Result<(i64, i64, i64), TopologyError> {
    // The upper endpoint is exclusive: `+2^63` cannot be represented by an
    // `i64`, while `-2^63` is its valid inclusive lower endpoint.
    const GRID_CELL_MIN: f64 = -9_223_372_036_854_775_808.0;
    const GRID_CELL_MAX_EXCLUSIVE: f64 = 9_223_372_036_854_775_808.0;
    let values = [
        point[0] / tolerance,
        point[1] / tolerance,
        point[2] / tolerance,
    ];
    for (axis, value) in values.iter().enumerate() {
        let cell = value.floor();
        if !(GRID_CELL_MIN..GRID_CELL_MAX_EXCLUSIVE).contains(&cell) {
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

fn canonical_zero_point(point: [f64; 3]) -> [f64; 3] {
    point.map(|coordinate| if coordinate == 0.0 { 0.0 } else { coordinate })
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
