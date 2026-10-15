//! Immutable face topology over an explicit welded view of a source mesh.
//!
//! The source [`Mesh`](crate::Mesh) is never reordered or repaired here.  A
//! topology view owns only the vertex-to-welded-vertex map needed to make its
//! declared policy true.  Its sidecar digest intentionally excludes allocator
//! estimates and that derived map: schema v4 identifies the four topology
//! arrays plus the canonical mesh fingerprint, and nothing else.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::hash::{BuildHasherDefault, Hasher};

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
    // Kept on the mesh: the mesh cannot change, so a repeat is the same string.
    if let Some(kept) = mesh.memo().fingerprint().get() {
        return Ok(kept.clone());
    }
    let fingerprint = compute_mesh_fingerprint(mesh)?;
    let _ = mesh.memo().fingerprint().set(fingerprint.clone());
    Ok(fingerprint)
}

fn compute_mesh_fingerprint(mesh: &Mesh) -> Result<String, TopologyError> {
    let mut hasher = Sha256::new();
    hash_tagged_bytes(
        &mut hasher,
        b"vertices",
        &(mesh.verts().len() as u64).to_le_bytes(),
    );
    // The byte stream is the same as feeding the hasher one value at a time;
    // it is staged in blocks only because a `Sha256::update` call per eight
    // bytes costs more than the hashing does.
    // Whole chunks are written into one fixed buffer with fixed-width stores
    // and handed to the hasher in one `update` each; the bytes are the same
    // stream, in the same order, as the value-at-a-time form.
    const VERTICES_PER_CHUNK: usize = 256;
    const TRIANGLES_PER_CHUNK: usize = 512;
    let mut bytes = [0_u8; VERTICES_PER_CHUNK * 24];
    for (chunk_index, chunk) in mesh.verts().chunks(VERTICES_PER_CHUNK).enumerate() {
        let mut used = 0_usize;
        for (offset, point) in chunk.iter().enumerate() {
            for (axis, coordinate) in point.iter().enumerate() {
                if !coordinate.is_finite() {
                    return Err(TopologyError::NonFiniteVertex {
                        vertex: chunk_index * VERTICES_PER_CHUNK + offset,
                        axis,
                    });
                }
                let canonical = if *coordinate == 0.0 { 0.0 } else { *coordinate };
                bytes[used..used + 8].copy_from_slice(&canonical.to_bits().to_le_bytes());
                used += 8;
            }
        }
        hasher.update(&bytes[..used]);
    }
    hash_tagged_bytes(
        &mut hasher,
        b"triangles",
        &(mesh.tris().len() as u64).to_le_bytes(),
    );
    let mut bytes = [0_u8; TRIANGLES_PER_CHUNK * 12];
    for chunk in mesh.tris().chunks(TRIANGLES_PER_CHUNK) {
        let mut used = 0_usize;
        for triangle in chunk {
            for index in triangle {
                bytes[used..used + 4].copy_from_slice(&index.to_le_bytes());
                used += 4;
            }
        }
        hasher.update(&bytes[..used]);
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

impl FaceTopology {
    /// Bytes this value holds on the heap and inline, from the actual buffer
    /// capacities: the struct itself, every vector, and the fingerprint
    /// string. `None` on arithmetic overflow.
    pub(crate) fn allocated_bytes(&self) -> Option<usize> {
        let mut total = std::mem::size_of::<Self>();
        for bytes in [
            self.indptr.capacity().checked_mul(std::mem::size_of::<u32>())?,
            self.indices.capacity().checked_mul(std::mem::size_of::<u32>())?,
            self.component_ids.capacity().checked_mul(std::mem::size_of::<u32>())?,
            self.cut_flags.capacity(),
            self.source_vertex_to_welded
                .capacity()
                .checked_mul(std::mem::size_of::<u32>())?,
            self.mesh_fingerprint.capacity(),
            self.point_only_vertices
                .capacity()
                .checked_mul(std::mem::size_of::<PointOnlyVertex>())?,
        ] {
            total = total.checked_add(bytes)?;
        }
        for vertex in &self.point_only_vertices {
            total = total.checked_add(
                vertex
                    .fans
                    .capacity()
                    .checked_mul(std::mem::size_of::<Vec<u32>>())?,
            )?;
            for fan in &vertex.fans {
                total = total.checked_add(fan.capacity().checked_mul(std::mem::size_of::<u32>())?)?;
            }
        }
        Some(total)
    }
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
pub fn build_face_topology(
    mesh: &Mesh,
    weld_policy: WeldPolicy,
) -> Result<FaceTopology, TopologyError> {
    // The exact-bits topology of a mesh is a pure function of its arrays and a
    // mesh cannot change, so it is built once and each later call takes a copy
    // of the kept value (a few flat vectors) instead of building it again.
    if weld_policy != WeldPolicy::ExactBits {
        return build_face_topology_uncached(mesh, weld_policy);
    }
    if let Some(kept) = mesh.memo().exact_topology().get() {
        return Ok(FaceTopology::clone(kept));
    }
    let built = build_face_topology_uncached(mesh, weld_policy)?;
    let _ = mesh
        .memo()
        .exact_topology()
        .set(std::sync::Arc::new(built.clone()));
    Ok(built)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged as a \
          class, and the one architecture item the class carried (geom_kernel's 32-bit BVH index \
          width) is now a named, enforced limit. TODO(cast-width-audit): retire site by site.  See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn build_face_topology_uncached(
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
    //
    // Grouping is a sort of `(unordered vertex triple, face)` pairs.  Nothing
    // below reads the groups in a particular order: each adds to a count or
    // sets a flag.
    let mut triangles_by_key: Vec<([u32; 3], usize)> = canonical_faces
        .iter()
        .enumerate()
        .map(|(face, triangle)| {
            let mut key = *triangle;
            key.sort_unstable();
            (key, face)
        })
        .collect();
    triangles_by_key.sort_unstable();
    let mut duplicate_face = vec![false; n_faces];
    let mut counts = TopologyCounts::default();
    let mut group_start = 0;
    while group_start < triangles_by_key.len() {
        let key = triangles_by_key[group_start].0;
        let group_end = group_start
            + triangles_by_key[group_start..].partition_point(|(other, _)| *other == key);
        let faces = &triangles_by_key[group_start..group_end];
        if faces.len() > 1 {
            counts.duplicate_face_groups += 1;
            counts.duplicate_faces += faces.len();
            for &(_, face) in faces {
                duplicate_face[face] = true;
                cut_flags[face] |= TOPOLOGY_CUT_DUPLICATE;
            }
        }
        group_start = group_end;
    }

    #[derive(Clone, Copy)]
    struct EdgeOccurrence {
        face: usize,
        forward: bool,
        degenerate: bool,
    }
    // Every edge occurrence, grouped by undirected edge in ascending
    // `(low, high)` key order, each group in the order its occurrences were
    // met (face order, then edge order within a face).  That is the order the
    // two passes below visit edges in, and the first pass's flag propagation
    // depends on it, so the sort carries the occurrence's position as the
    // final key.
    let mut keyed_occurrences: Vec<((u32, u32), usize, EdgeOccurrence)> =
        Vec::with_capacity(n_faces * 3);
    for (face, triangle) in canonical_faces.iter().enumerate() {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            keyed_occurrences.push((
                (a.min(b), a.max(b)),
                keyed_occurrences.len(),
                EdgeOccurrence {
                    face,
                    forward: a < b,
                    degenerate: a == b,
                },
            ));
        }
    }
    keyed_occurrences.sort_unstable_by_key(|(key, position, _)| (*key, *position));
    let sorted_occurrences: Vec<EdgeOccurrence> = keyed_occurrences
        .iter()
        .map(|(_, _, occurrence)| *occurrence)
        .collect();
    let mut edge_groups: Vec<std::ops::Range<usize>> = Vec::new();
    let mut group_start = 0;
    while group_start < keyed_occurrences.len() {
        let key = keyed_occurrences[group_start].0;
        let group_end = group_start
            + keyed_occurrences[group_start..].partition_point(|(other, _, _)| *other == key);
        edge_groups.push(group_start..group_end);
        group_start = group_end;
    }
    drop(keyed_occurrences);
    let edges = || {
        edge_groups
            .iter()
            .map(|range| &sorted_occurrences[range.clone()])
    };

    // First pass: cut flags and adjacency.  A face that meets a duplicate
    // across one edge is itself cut there, and `cut_flags` carries that
    // outward through the pass, so an edge reached later sees a face already
    // marked ambiguous and refuses to accept it as a manifold partner.  That
    // propagation only ever severs adjacency, never restores it, so it is kept
    // exactly as it stands; what it must not do is decide a *count*, which is
    // why the counting pass below reads `duplicate_face` instead.
    let mut adjacency = vec![Vec::<u32>::new(); n_faces];
    let mut accepted_edges = Vec::<(usize, usize)>::new();
    for occurrences in edges() {
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
    for occurrences in edges() {
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
    //
    // Fans are the connected components of the faces around one welded vertex
    // under the accepted edges that both contain that vertex.  They are found
    // with a union-find over one slot per (vertex, face) pair, laid out as a
    // CSR keyed by welded vertex with each vertex's faces ascending.  Vertices
    // are visited in ascending id and a vertex's fans in ascending smallest
    // face, with ascending members: the order the earlier ordered-map walk
    // produced.
    let welded_vertex_total = welded.vertex_count as usize;
    let mut face_start = vec![0_usize; welded_vertex_total + 1];
    for triangle in &canonical_faces {
        for (corner, vertex) in triangle.iter().enumerate() {
            if !triangle[..corner].contains(vertex) {
                face_start[*vertex as usize + 1] += 1;
            }
        }
    }
    for vertex in 0..welded_vertex_total {
        face_start[vertex + 1] += face_start[vertex];
    }
    let mut faces_at = vec![0_u32; face_start[welded_vertex_total]];
    let mut fill = face_start.clone();
    for (face, triangle) in canonical_faces.iter().enumerate() {
        for (corner, vertex) in triangle.iter().enumerate() {
            if !triangle[..corner].contains(vertex) {
                faces_at[fill[*vertex as usize]] = face as u32;
                fill[*vertex as usize] += 1;
            }
        }
    }
    drop(fill);
    let slot_of = |vertex: u32, face: usize| -> usize {
        let start = face_start[vertex as usize];
        let end = face_start[vertex as usize + 1];
        start
            + faces_at[start..end]
                .binary_search(&(face as u32))
                .expect("face incidence inserted")
    };
    fn find(parent: &mut [u32], mut slot: usize) -> usize {
        while parent[slot] as usize != slot {
            let grandparent = parent[parent[slot] as usize];
            parent[slot] = grandparent;
            slot = grandparent as usize;
        }
        slot
    }
    let mut parent: Vec<u32> = (0..faces_at.len() as u32).collect();
    for (a, b) in &accepted_edges {
        let ta = canonical_faces[*a];
        let tb = canonical_faces[*b];
        for vertex in [ta[0], ta[1], ta[2]] {
            if tb.contains(&vertex) {
                let root_a = find(&mut parent, slot_of(vertex, *a));
                let root_b = find(&mut parent, slot_of(vertex, *b));
                if root_a != root_b {
                    parent[root_a.max(root_b)] = root_a.min(root_b) as u32;
                }
            }
        }
    }
    let mut point_only_vertices = Vec::new();
    for vertex in 0..welded_vertex_total {
        let slots = face_start[vertex]..face_start[vertex + 1];
        // A slot is the root of its fan exactly when it is its own parent.
        let fan_count = slots
            .clone()
            .filter(|slot| parent[*slot] as usize == *slot)
            .count();
        if fan_count <= 1 {
            continue;
        }
        // The fans are materialized rather than merely counted. Counting
        // answers "is this a pinch"; a repair additionally has to know which
        // faces belong to which sheet, and deriving that a second time
        // elsewhere is exactly the duplicated-capability hazard O-5 forbids.
        //
        // A root always lies in this vertex's own slot range (edges only
        // union slots of one vertex), so `root - first_slot` indexes a lookup
        // table of one entry per slot.  A linear scan over the roots seen so
        // far would be quadratic in the fan count on a vertex with many fans.
        let first_slot = slots.start;
        let mut fan_of_root: Vec<u32> = vec![u32::MAX; slots.len()];
        let mut fans: Vec<Vec<u32>> = Vec::with_capacity(fan_count);
        for slot in slots {
            let face = faces_at[slot];
            cut_flags[face as usize] |= TOPOLOGY_CUT_POINT_ONLY;
            let root = find(&mut parent, slot);
            let entry = &mut fan_of_root[root - first_slot];
            if *entry == u32::MAX {
                *entry = fans.len() as u32;
                fans.push(vec![face]);
            } else {
                fans[*entry as usize].push(face);
            }
        }
        point_only_vertices.push(PointOnlyVertex {
            welded_vertex: vertex as u32,
            fans,
        });
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
            WeldPolicy::ExactBits => {
                // Pure function of the vertex array, which a mesh cannot
                // change: built once, later calls copy the kept view.
                if let Some(kept) = mesh.memo().exact_weld().get() {
                    return Ok(WeldedView::clone(kept));
                }
                let built = exact_weld(mesh.verts())?;
                let _ = mesh
                    .memo()
                    .exact_weld()
                    .set(std::sync::Arc::new(built.clone()));
                Ok(built)
            }
            WeldPolicy::Epsilon(tolerance) => epsilon_weld(mesh.verts(), tolerance),
        }
    }

    /// Bytes this value holds, struct and buffers, from actual capacities.
    pub(crate) fn allocated_bytes(&self) -> Option<usize> {
        std::mem::size_of::<Self>()
            .checked_add(
                self.verts
                    .capacity()
                    .checked_mul(std::mem::size_of::<[f64; 3]>())?,
            )?
            .checked_add(self.map.capacity().checked_mul(std::mem::size_of::<u32>())?)
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
    /// Multiply-xor mixing over the three words of a coordinate key.  A weld
    /// table needs speed and spread, not collision resistance: the keys are
    /// the mesh's own coordinates.
    #[derive(Default)]
    struct WeldKeyHasher(u64);

    impl Hasher for WeldKeyHasher {
        fn finish(&self) -> u64 {
            self.0
        }

        fn write(&mut self, bytes: &[u8]) {
            // A `[u64; 3]` key reaches here as one 24-byte slice.
            let mut words = bytes.chunks_exact(8);
            for word in words.by_ref() {
                let mut buffer = [0_u8; 8];
                buffer.copy_from_slice(word);
                self.write_u64(u64::from_le_bytes(buffer));
            }
            for byte in words.remainder() {
                self.write_u64(u64::from(*byte));
            }
        }

        fn write_u64(&mut self, word: u64) {
            let mixed = (self.0 ^ word).wrapping_mul(0x9E37_79B9_7F4A_7C15);
            self.0 = mixed ^ (mixed >> 29);
        }
    }

    // Only `get` and `insert` touch this map and ids are handed out in
    // first-occurrence order, so the result does not depend on the hasher.
    let mut ids =
        HashMap::<[u64; 3], u32, BuildHasherDefault<WeldKeyHasher>>::with_capacity_and_hasher(
            points.len() / 4,
            BuildHasherDefault::default(),
        );
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

/// A fixed staging buffer in front of a [`Sha256`]: the digest sees the same
/// bytes in the same order, in few large `update` calls instead of many tiny
/// ones.
struct HashBlock {
    bytes: [u8; 8192],
    used: usize,
}

impl HashBlock {
    const fn new() -> Self {
        Self {
            bytes: [0; 8192],
            used: 0,
        }
    }

    fn push(&mut self, hasher: &mut Sha256, piece: &[u8]) {
        if self.used + piece.len() > self.bytes.len() {
            self.flush(hasher);
        }
        self.bytes[self.used..self.used + piece.len()].copy_from_slice(piece);
        self.used += piece.len();
    }

    fn flush(&mut self, hasher: &mut Sha256) {
        hasher.update(&self.bytes[..self.used]);
        self.used = 0;
    }
}

fn hash_u32_array(hasher: &mut Sha256, values: &[u32]) {
    hasher.update((values.len() as u64).to_le_bytes());
    let mut block = HashBlock::new();
    for value in values {
        block.push(hasher, &value.to_le_bytes());
    }
    block.flush(hasher);
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

/// The ordered-map builder this file's production builder replaced, as its
/// test oracle.
#[cfg(test)]
mod reference;
