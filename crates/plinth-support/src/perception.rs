//! Bound, batch-first generic support perception for the frozen pre-P8 path.
//!
//! The older [`crate::perceive_contacts`] leaf remains the sole authority for
//! exterior/down-facing/near-plate/column-ray classification.  This module
//! adds the orchestration which turns that evidence into islands, dyadic
//! seats, print-layer births and local minima.  Its result is deliberately
//! opaque: downstream specialized placement can consume the evidence, but it
//! cannot manufacture or replay it against another mesh or layer schedule.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use geom_kernel::{
    NativeSurface, NativeSurfaceConfig, PlaneSliceLimits, PointCloudIndex, PointQueryError,
    RadiusQueryLimits,
};
use plinth_core::LayerPlan;
use plinth_mesh::{mesh_fingerprint, FaceTopology, Mesh, WeldPolicy, WeldedView};
use thiserror::Error;

use crate::dyadic::{build_candidate_payload, CandidatePayloadConfig, DyadicError};
use crate::{
    perceive_contacts_on_surface, validate_perception_input, BuildFrame, ContactCandidate,
    ContactCandidateId, ContactCandidatePool, SupportIsland, SupportPerceptionConfig,
    SupportPerceptionError,
};

const LOOP_MERGE_MM: f64 = geom_kernel::PLANE_SLICE_MERGE_TOLERANCE_MM;
const BIRTH_MIN_AREA_MM2: f64 = 1.0e-3;

/// Explicit, result-inert refusal limits for generic perception.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenericPerceptionLimits {
    pub max_planes: usize,
    pub max_triangle_plane_tests: usize,
    pub max_slice_intersections: usize,
    pub max_slice_output_bytes: usize,
    pub max_loop_vertices: usize,
    pub max_minima_vertices: usize,
    pub max_island_neighbor_pairs: usize,
    pub max_island_neighbor_bytes: usize,
    pub max_overlap_pairs: usize,
    pub max_overlap_bytes: usize,
    pub max_overlap_vertex_tests: usize,
    pub max_overlap_edge_tests: usize,
}

impl GenericPerceptionLimits {
    #[must_use]
    pub const fn production() -> Self {
        Self {
            max_planes: 20_000,
            max_triangle_plane_tests: 100_000_000,
            max_slice_intersections: 100_000_000,
            max_slice_output_bytes: 2 * 1024 * 1024 * 1024,
            max_loop_vertices: 100_000_000,
            max_minima_vertices: 20_000_000,
            max_island_neighbor_pairs: 100_000_000,
            max_island_neighbor_bytes: 2 * 1024 * 1024 * 1024,
            max_overlap_pairs: 100_000_000,
            max_overlap_bytes: 2 * 1024 * 1024 * 1024,
            max_overlap_vertex_tests: 100_000_000,
            max_overlap_edge_tests: 100_000_000,
        }
    }
}

/// A specialized owner known before generic load partitioning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SpecializedOwnerReservation {
    pub owner_id: u32,
    pub face_id: u32,
}

/// Closed physical-contact owner vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LoadOwner {
    Specialized(u32),
    GenericIsland(u32),
}

/// One already-placed physical contact presented to the load finalizer.
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalContact {
    pub contact_id: u64,
    pub owner: LoadOwner,
    pub point_mm: [f64; 3],
}

#[derive(Clone, Debug, PartialEq)]
struct LoadEligibility {
    face_id: u32,
    eligible_owners: Vec<LoadOwner>,
    centroid_mm: [f64; 3],
    area_mm2: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FinalizedFaceLoad {
    pub face_id: u32,
    pub contact_id: u64,
    pub owner: LoadOwner,
    pub carried_area_mm2: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FinalizedLoadPartition {
    authority: AuthorityFingerprint,
    contacts_fingerprint: u64,
    rows: Vec<FinalizedFaceLoad>,
}

impl FinalizedLoadPartition {
    #[must_use]
    pub fn rows(&self) -> &[FinalizedFaceLoad] {
        &self.rows
    }
}

/// One canonical solid boundary or hole at a real print layer.
#[derive(Clone, Debug, PartialEq)]
pub struct SliceLoop {
    points_xy_mm: Vec<[f64; 2]>,
    signed_area_mm2: f64,
    hole: bool,
}

impl SliceLoop {
    #[must_use]
    pub fn points_xy_mm(&self) -> &[[f64; 2]] {
        &self.points_xy_mm
    }
    #[must_use]
    pub const fn signed_area_mm2(&self) -> f64 {
        self.signed_area_mm2
    }
    #[must_use]
    pub const fn is_hole(&self) -> bool {
        self.hole
    }
}

/// One solid cross-section component with its owned void boundaries.
#[derive(Clone, Debug, PartialEq)]
pub struct SliceSolid {
    outer: SliceLoop,
    holes: Vec<SliceLoop>,
}

impl SliceSolid {
    #[must_use]
    pub const fn outer(&self) -> &SliceLoop {
        &self.outer
    }
    #[must_use]
    pub fn holes(&self) -> &[SliceLoop] {
        &self.holes
    }
    #[must_use]
    pub fn area_mm2(&self) -> f64 {
        self.outer.signed_area_mm2.abs()
            - self
                .holes
                .iter()
                .map(|hole| hole.signed_area_mm2.abs())
                .sum::<f64>()
    }
}

/// First printable appearance of one solid slice component.
#[derive(Clone, Debug, PartialEq)]
pub struct IslandBirth {
    pub layer_index: usize,
    pub z_mm: f64,
    pub centroid_xy_mm: [f64; 2],
    pub area_mm2: f64,
    footprint: SliceSolid,
}

impl IslandBirth {
    #[must_use]
    pub const fn footprint(&self) -> &SliceSolid {
        &self.footprint
    }
}

/// A welded surface-height minimum and its complete birth-band region.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalZMinimum {
    pub position_mm: [f64; 3],
    pub normal: [f64; 3],
    region_points_mm: Vec<[f64; 3]>,
}

impl LocalZMinimum {
    #[must_use]
    pub fn region_points_mm(&self) -> &[[f64; 3]] {
        &self.region_points_mm
    }
}

/// Opaque generic-perception authority.
#[derive(Clone, Debug, PartialEq)]
pub struct GenericContactPlan {
    authority: AuthorityFingerprint,
    islands: Vec<SupportIsland>,
    candidates: ContactCandidatePool,
    slice_layers: Vec<Vec<SliceSolid>>,
    island_births: Vec<IslandBirth>,
    local_z_minima: Vec<LocalZMinimum>,
    load_eligibility: Vec<LoadEligibility>,
    slice_effective_threads: usize,
    slice_participating_threads: usize,
}

impl GenericContactPlan {
    #[must_use]
    pub fn islands(&self) -> &[SupportIsland] {
        &self.islands
    }
    #[must_use]
    pub fn candidates(&self) -> &ContactCandidatePool {
        &self.candidates
    }
    #[must_use]
    pub fn slice_layers(&self) -> &[Vec<SliceSolid>] {
        &self.slice_layers
    }
    #[must_use]
    pub fn island_births(&self) -> &[IslandBirth] {
        &self.island_births
    }
    #[must_use]
    pub fn local_z_minima(&self) -> &[LocalZMinimum] {
        &self.local_z_minima
    }
    #[must_use]
    pub const fn slice_thread_evidence(&self) -> (usize, usize) {
        (
            self.slice_effective_threads,
            self.slice_participating_threads,
        )
    }
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut hash = Fnv::new();
        hash.bytes(self.authority.mesh.as_bytes());
        hash.u64(self.authority.native);
        hash.u64(self.authority.config);
        hash.u64(self.authority.frame);
        hash.u64(self.authority.layers);
        hash.u64(self.authority.specialized);
        hash.u64(self.candidates.digest());
        for island in &self.islands {
            hash.u32(island.id);
            for &face in &island.face_ids {
                hash.u32(face);
            }
        }
        for birth in &self.island_births {
            hash.usize(birth.layer_index);
            hash.f64(birth.z_mm);
            hash.f64(birth.area_mm2);
            for point in birth.footprint.outer.points_xy_mm() {
                hash.f64(point[0]);
                hash.f64(point[1]);
            }
            for hole in &birth.footprint.holes {
                for point in hole.points_xy_mm() {
                    hash.f64(point[0]);
                    hash.f64(point[1]);
                }
            }
        }
        for minimum in &self.local_z_minima {
            for value in minimum.position_mm.iter().chain(minimum.normal.iter()) {
                hash.f64(*value);
            }
            for point in &minimum.region_points_mm {
                for value in point {
                    hash.f64(*value);
                }
            }
        }
        for layer in &self.slice_layers {
            for solid in layer {
                hash.f64(solid.area_mm2());
                for point in solid.outer.points_xy_mm() {
                    hash.f64(point[0]);
                    hash.f64(point[1]);
                }
                for hole in &solid.holes {
                    for point in hole.points_xy_mm() {
                        hash.f64(point[0]);
                        hash.f64(point[1]);
                    }
                }
            }
        }
        for row in &self.load_eligibility {
            hash.u32(row.face_id);
            for value in row.centroid_mm {
                hash.f64(value);
            }
            hash.f64(row.area_mm2);
            for owner in &row.eligible_owners {
                match owner {
                    LoadOwner::Specialized(id) => {
                        hash.u8(0);
                        hash.u32(*id);
                    }
                    LoadOwner::GenericIsland(id) => {
                        hash.u8(1);
                        hash.u32(*id);
                    }
                }
            }
        }
        hash.finish()
    }

    /// Reject replay against any different geometry, native setup, frame,
    /// result-changing configuration or layer schedule.
    #[allow(
        clippy::too_many_arguments,
        reason = "validation deliberately restates every independently stale-able authority input"
    )]
    pub fn validate_binding(
        &self,
        mesh: &Mesh,
        config: &SupportPerceptionConfig<'_>,
        frame: BuildFrame,
        layers: &LayerPlan,
        candidate_config: &CandidatePayloadConfig,
        specialized: &[SpecializedOwnerReservation],
        limits: GenericPerceptionLimits,
    ) -> Result<(), GenericPerceptionError> {
        if config.plate_z_mm != frame.plate_z_mm() {
            return Err(GenericPerceptionError::FrameMismatch);
        }
        let surface = build_surface(mesh, config)?;
        let current = authority_fingerprint(
            mesh,
            &surface,
            config,
            frame,
            layers,
            candidate_config,
            specialized,
            limits,
        )?;
        if current != self.authority {
            return Err(GenericPerceptionError::StaleAuthority);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AuthorityFingerprint {
    mesh: String,
    native: u64,
    config: u64,
    frame: u64,
    layers: u64,
    specialized: u64,
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum GenericPerceptionError {
    #[error(transparent)]
    Perception(#[from] SupportPerceptionError),
    #[error(transparent)]
    Dyadic(#[from] DyadicError),
    #[error("generic perception could not build topology: {0}")]
    Topology(String),
    #[error("generic perception could not build native surface: {0}")]
    Surface(String),
    #[error("generic perception could not query the island point index: {0}")]
    PointIndex(String),
    #[error("generic perception could not slice print layers: {0}")]
    Slice(String),
    #[error("slice layer {layer} contains an open or branched segment seam")]
    OpenSliceSeam { layer: usize },
    #[error("slice loop vertex count {required} exceeds cap {limit}")]
    LoopVertexLimit { required: usize, limit: usize },
    #[error("welded minimum vertex count {required} exceeds cap {limit}")]
    MinimaVertexLimit { required: usize, limit: usize },
    #[error("specialized owner reservation names face {face_id} outside the mesh")]
    SpecializedFaceOutOfRange { face_id: u32 },
    #[error("specialized owner reservation duplicates owner {owner_id} on face {face_id}")]
    DuplicateSpecializedReservation { owner_id: u32, face_id: u32 },
    #[error("the layer schedule does not cover the support mesh z range")]
    LayerScheduleCoverage,
    #[error("support perception plate and build frame disagree")]
    FrameMismatch,
    #[error("island-neighbor query requires {required} output pairs; limit is {limit}")]
    IslandNeighborPairLimit { required: usize, limit: usize },
    #[error("island-neighbor query requires {required} output bytes; limit is {limit}")]
    IslandNeighborByteLimit { required: usize, limit: usize },
    #[error("slice overlap requires {required} candidate pairs; limit is {limit}")]
    OverlapPairLimit { required: usize, limit: usize },
    #[error("slice overlap requires {required} bytes; limit is {limit}")]
    OverlapByteLimit { required: usize, limit: usize },
    #[error("slice overlap requires at least {required} sampled vertex tests; limit is {limit}")]
    OverlapVertexTestLimit { required: usize, limit: usize },
    #[error("slice overlap requires at least {required} sampled edge tests; limit is {limit}")]
    OverlapEdgeTestLimit { required: usize, limit: usize },
    #[error("physical contact {contact_id} is non-finite")]
    NonFinitePhysicalContact { contact_id: u64 },
    #[error("physical contact id {contact_id} is duplicated")]
    DuplicatePhysicalContact { contact_id: u64 },
    #[error("support face {face_id} has no eligible physical contact")]
    MissingPhysicalContact { face_id: u32 },
    #[error("generic perception produced an island without a legal dyadic candidate: {island_id}")]
    NoLegalCandidate { island_id: u32 },
    #[error("generic perception authority is stale or belongs to another input")]
    StaleAuthority,
}

/// Prepare the complete current generic evidence in batched native calls.
pub fn prepare_generic_contacts(
    mesh: &Mesh,
    config: &SupportPerceptionConfig<'_>,
    frame: BuildFrame,
    layers: &LayerPlan,
    candidate_config: &CandidatePayloadConfig,
    specialized: &[SpecializedOwnerReservation],
    limits: GenericPerceptionLimits,
) -> Result<GenericContactPlan, GenericPerceptionError> {
    if config.plate_z_mm != frame.plate_z_mm() {
        return Err(GenericPerceptionError::FrameMismatch);
    }
    let (topology, delicacy) = validate_perception_input(mesh, config)?;
    let surface = build_surface(mesh, config)?;
    let raw = perceive_contacts_on_surface(mesh, config, &topology, delicacy, &surface)?;
    let islands = connected_islands(&raw, &topology, &surface, limits)?;
    let payload = build_candidate_payload(mesh, &islands, candidate_config)?;
    let geometry = surface.geometry();
    let candidates = payload
        .rows()
        .iter()
        .enumerate()
        .map(|(row, candidate)| ContactCandidate {
            id: ContactCandidateId(row as u64),
            face_id: candidate.face_id,
            point_mm: candidate.point_mm,
            normal: candidate.normal,
            supported_area_mm2: geometry.areas()[candidate.face_id as usize],
            overhang_excess: -candidate.normal[2],
            delicacy: delicacy[candidate.face_id as usize],
        })
        .collect::<Vec<_>>();
    for island in &islands {
        if !candidates
            .iter()
            .any(|candidate| island.face_ids.binary_search(&candidate.face_id).is_ok())
        {
            return Err(GenericPerceptionError::NoLegalCandidate {
                island_id: island.id,
            });
        }
    }
    let candidates = ContactCandidatePool { candidates };

    let (layer_indices, planes) = layer_midplanes(mesh, layers)?;
    let sliced = surface
        .slice_z_batch(
            &planes,
            PlaneSliceLimits {
                max_planes: limits.max_planes,
                max_triangle_plane_tests: limits.max_triangle_plane_tests,
                max_intersections: limits.max_slice_intersections,
                max_output_bytes: limits.max_slice_output_bytes,
            },
        )
        .map_err(|error| GenericPerceptionError::Slice(error.to_string()))?;
    let mut loop_vertices = 0usize;
    let mut slice_layers = Vec::with_capacity(planes.len());
    for layer in 0..planes.len() {
        let loops = stitch_layer(layer, sliced.plane(layer))?;
        loop_vertices = loop_vertices
            .checked_add(
                loops
                    .iter()
                    .map(|solid| {
                        solid.outer.points_xy_mm.len()
                            + solid
                                .holes
                                .iter()
                                .map(|hole| hole.points_xy_mm.len())
                                .sum::<usize>()
                    })
                    .sum::<usize>(),
            )
            .ok_or(GenericPerceptionError::LoopVertexLimit {
                required: usize::MAX,
                limit: limits.max_loop_vertices,
            })?;
        if loop_vertices > limits.max_loop_vertices {
            return Err(GenericPerceptionError::LoopVertexLimit {
                required: loop_vertices,
                limit: limits.max_loop_vertices,
            });
        }
        slice_layers.push(loops);
    }
    let island_births = find_births(&layer_indices, &planes, &slice_layers, limits)?;
    let local_z_minima = find_local_minima(mesh, &surface, layers, &island_births, limits)?;
    let load_eligibility = build_load_eligibility(mesh, &surface, &islands, specialized)?;
    let authority = authority_fingerprint(
        mesh,
        &surface,
        config,
        frame,
        layers,
        candidate_config,
        specialized,
        limits,
    )?;
    Ok(GenericContactPlan {
        authority,
        islands,
        candidates,
        slice_layers,
        island_births,
        local_z_minima,
        load_eligibility,
        slice_effective_threads: sliced.effective_threads,
        slice_participating_threads: sliced.participating_threads,
    })
}

fn build_surface(
    mesh: &Mesh,
    config: &SupportPerceptionConfig<'_>,
) -> Result<NativeSurface, GenericPerceptionError> {
    NativeSurface::from_indexed(
        mesh.verts(),
        mesh.tris(),
        NativeSurfaceConfig {
            grid_pitch_mm: config.surface_grid_pitch_mm.get(),
            max_grid_bytes: config.max_grid_bytes,
        },
    )
    .map_err(|error| GenericPerceptionError::Surface(error.to_string()))
}

fn connected_islands(
    raw: &ContactCandidatePool,
    topology: &FaceTopology,
    surface: &NativeSurface,
    limits: GenericPerceptionLimits,
) -> Result<Vec<SupportIsland>, GenericPerceptionError> {
    let mut unseen = raw
        .candidates()
        .iter()
        .map(|candidate| candidate.face_id as usize)
        .collect::<BTreeSet<_>>();
    if unseen.is_empty() {
        return Ok(Vec::new());
    }
    let radius = plinth_core::m0_constants()
        .map_err(|error| GenericPerceptionError::Topology(error.to_string()))?
        .placeholders()
        .island_merge_radius_mm()
        .diagnostic()
        .get();
    let centers = surface.geometry().centroids();
    let face_ids = unseen.iter().copied().collect::<Vec<_>>();
    let points = face_ids
        .iter()
        .map(|face| centers[*face])
        .collect::<Vec<_>>();
    let index = PointCloudIndex::new(&points)
        .map_err(|error| GenericPerceptionError::PointIndex(error.to_string()))?;
    // Gather every island-closing query, invoke the maintained spatial index
    // once, then scatter the CSR into source-face adjacency.
    let nearby = index
        .radius_neighbors_batch(
            &points,
            radius,
            RadiusQueryLimits {
                max_output_pairs: limits.max_island_neighbor_pairs,
                max_output_bytes: limits.max_island_neighbor_bytes,
            },
        )
        .map_err(|error| match error {
            PointQueryError::RadiusOutputPairLimit { required, limit } => {
                GenericPerceptionError::IslandNeighborPairLimit { required, limit }
            }
            PointQueryError::RadiusOutputByteLimit { required, limit } => {
                GenericPerceptionError::IslandNeighborByteLimit { required, limit }
            }
            other => GenericPerceptionError::PointIndex(other.to_string()),
        })?;
    let mut spatial = BTreeMap::<usize, Vec<usize>>::new();
    for (row, &face) in face_ids.iter().enumerate() {
        let range = nearby
            .row_range(row)
            .expect("one point-index row exists per gathered face");
        spatial.insert(
            face,
            nearby.point_ids[range]
                .iter()
                .map(|point_id| face_ids[*point_id as usize])
                .filter(|other| *other != face)
                .collect(),
        );
    }
    let mut groups = Vec::new();
    while let Some(&start) = unseen.first() {
        unseen.remove(&start);
        let mut queue = VecDeque::from([start]);
        let mut faces = Vec::new();
        while let Some(face) = queue.pop_front() {
            faces.push(face as u32);
            let mut neighbors = topology
                .neighbors(face)
                .unwrap_or(&[])
                .iter()
                .map(|value| *value as usize)
                .filter(|value| unseen.contains(value))
                .collect::<Vec<_>>();
            neighbors.extend(
                spatial[&face]
                    .iter()
                    .copied()
                    .filter(|other| unseen.contains(other)),
            );
            neighbors.sort_unstable();
            neighbors.dedup();
            for neighbor in neighbors {
                if unseen.remove(&neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
        faces.sort_unstable();
        groups.push(faces);
    }
    groups.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    Ok(groups
        .into_iter()
        .enumerate()
        .map(|(id, face_ids)| SupportIsland {
            id: id as u32,
            face_ids,
        })
        .collect())
}

fn layer_midplanes(
    mesh: &Mesh,
    layers: &LayerPlan,
) -> Result<(Vec<usize>, Vec<f64>), GenericPerceptionError> {
    let zmin = mesh
        .verts()
        .iter()
        .map(|point| point[2])
        .fold(f64::INFINITY, f64::min);
    let zmax = mesh
        .verts()
        .iter()
        .map(|point| point[2])
        .fold(f64::NEG_INFINITY, f64::max);
    if layers.count() == 0
        || layers.z0_mm() > zmin
        || layers.z_of(layers.count() - 1).unwrap_or(f64::NEG_INFINITY)
            + layers.heights_mm()[layers.count() - 1]
            < zmax
    {
        return Err(GenericPerceptionError::LayerScheduleCoverage);
    }
    let indexed = (0..layers.count())
        .filter_map(|index| {
            let z = layers.z_of(index)? + layers.heights_mm()[index] * 0.5;
            (z >= zmin && z < zmax).then_some((index, z))
        })
        .collect::<Vec<_>>();
    Ok(indexed.into_iter().unzip())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PointKey(i64, i64);

fn point_key(point: [f64; 2]) -> PointKey {
    PointKey(
        (point[0] / LOOP_MERGE_MM).round() as i64,
        (point[1] / LOOP_MERGE_MM).round() as i64,
    )
}

fn stitch_layer(
    layer: usize,
    intersections: &[geom_kernel::PlaneIntersection],
) -> Result<Vec<SliceSolid>, GenericPerceptionError> {
    let mut points = BTreeMap::<PointKey, [f64; 2]>::new();
    let mut edges = BTreeSet::<(PointKey, PointKey)>::new();
    for event in intersections {
        let a = [event.segment[0][0], event.segment[0][1]];
        let b = [event.segment[1][0], event.segment[1][1]];
        let ka = point_key(a);
        let kb = point_key(b);
        if ka == kb {
            continue;
        }
        points.entry(ka).or_insert(a);
        points.entry(kb).or_insert(b);
        edges.insert(if ka < kb { (ka, kb) } else { (kb, ka) });
    }
    let mut adjacency = BTreeMap::<PointKey, Vec<PointKey>>::new();
    for &(a, b) in &edges {
        adjacency.entry(a).or_default().push(b);
        adjacency.entry(b).or_default().push(a);
    }
    if adjacency.values().any(|neighbors| neighbors.len() != 2) {
        return Err(GenericPerceptionError::OpenSliceSeam { layer });
    }
    for neighbors in adjacency.values_mut() {
        neighbors.sort_unstable();
    }
    let mut unused = edges;
    let mut raw = Vec::<Vec<[f64; 2]>>::new();
    while let Some(&(start, next)) = unused.first() {
        let mut keys = vec![start];
        let mut previous = start;
        let mut current = next;
        unused.remove(&(start, next));
        while current != start {
            keys.push(current);
            let neighbors = &adjacency[&current];
            let following = if neighbors[0] == previous {
                neighbors[1]
            } else {
                neighbors[0]
            };
            let edge = if current < following {
                (current, following)
            } else {
                (following, current)
            };
            if !unused.remove(&edge) {
                return Err(GenericPerceptionError::OpenSliceSeam { layer });
            }
            previous = current;
            current = following;
        }
        raw.push(keys.into_iter().map(|key| points[&key]).collect());
    }
    let mut loops = Vec::new();
    for (index, points_xy) in raw.iter().enumerate() {
        let probe = points_xy[0];
        let depth = raw
            .iter()
            .enumerate()
            .filter(|(other, polygon)| *other != index && point_in_polygon(probe, polygon))
            .count();
        let hole = depth % 2 == 1;
        let mut canonical = canonicalize_loop(points_xy.clone(), hole);
        let area = signed_area(&canonical);
        if area.abs() < BIRTH_MIN_AREA_MM2 {
            continue;
        }
        let desired_positive = !hole;
        if (area > 0.0) != desired_positive {
            canonical.reverse();
            canonical = canonicalize_loop(canonical, hole);
        }
        loops.push(SliceLoop {
            signed_area_mm2: signed_area(&canonical),
            points_xy_mm: canonical,
            hole,
        });
    }
    loops.sort_by(|a, b| {
        a.hole
            .cmp(&b.hole)
            .then_with(|| b.signed_area_mm2.abs().total_cmp(&a.signed_area_mm2.abs()))
            .then_with(|| point_key(a.points_xy_mm[0]).cmp(&point_key(b.points_xy_mm[0])))
    });
    let mut solids = loops
        .iter()
        .filter(|loop_| !loop_.hole)
        .cloned()
        .map(|outer| SliceSolid {
            outer,
            holes: Vec::new(),
        })
        .collect::<Vec<_>>();
    for hole in loops.into_iter().filter(|loop_| loop_.hole) {
        let probe = hole.points_xy_mm[0];
        let owner = solids
            .iter()
            .enumerate()
            .filter(|(_, solid)| point_in_polygon(probe, solid.outer.points_xy_mm()))
            .min_by(|(_, left), (_, right)| {
                left.outer
                    .signed_area_mm2
                    .abs()
                    .total_cmp(&right.outer.signed_area_mm2.abs())
            })
            .map(|(index, _)| index)
            .ok_or(GenericPerceptionError::OpenSliceSeam { layer })?;
        solids[owner].holes.push(hole);
    }
    for solid in &mut solids {
        solid.holes.sort_by(|left, right| {
            right
                .signed_area_mm2
                .abs()
                .total_cmp(&left.signed_area_mm2.abs())
                .then_with(|| {
                    point_key(left.points_xy_mm[0]).cmp(&point_key(right.points_xy_mm[0]))
                })
        });
    }
    Ok(solids)
}

fn canonicalize_loop(mut points: Vec<[f64; 2]>, _hole: bool) -> Vec<[f64; 2]> {
    let first = points
        .iter()
        .enumerate()
        .min_by_key(|(_, point)| point_key(**point))
        .map(|(index, _)| index)
        .unwrap_or(0);
    points.rotate_left(first);
    points
}

fn signed_area(points: &[[f64; 2]]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    (0..points.len())
        .map(|i| {
            let next = (i + 1) % points.len();
            points[i][0] * points[next][1] - points[next][0] * points[i][1]
        })
        .sum::<f64>()
        * 0.5
}

fn point_in_polygon(point: [f64; 2], polygon: &[[f64; 2]]) -> bool {
    let mut inside = false;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        if (a[1] > point[1]) != (b[1] > point[1])
            && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}

fn segments_intersect(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let ccw = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        (r[1] - p[1]) * (q[0] - p[0]) > (q[1] - p[1]) * (r[0] - p[0])
    };
    ccw(a, c, d) != ccw(b, c, d) && ccw(a, b, c) != ccw(a, b, d)
}

fn find_births(
    layer_indices: &[usize],
    planes: &[f64],
    layers: &[Vec<SliceSolid>],
    limits: GenericPerceptionLimits,
) -> Result<Vec<IslandBirth>, GenericPerceptionError> {
    let mut births = Vec::new();
    let mut previous: &[SliceSolid] = &[];
    let mut operation_budget = OverlapOperationBudget::new(limits);
    for (layer_index, layer) in layers.iter().enumerate() {
        let overlaps = overlap_candidates(
            layer,
            previous,
            limits.max_overlap_pairs,
            limits.max_overlap_bytes,
        )?;
        for (current_index, solid) in layer.iter().enumerate() {
            let mut overlapped = false;
            if let Some(prior) = overlaps.get(&current_index) {
                for index in prior {
                    if solids_overlap(solid, &previous[*index], &mut operation_budget)? {
                        overlapped = true;
                        break;
                    }
                }
            }
            if !overlapped {
                let count = solid.outer.points_xy_mm.len() as f64;
                let centroid = [
                    solid
                        .outer
                        .points_xy_mm
                        .iter()
                        .map(|point| point[0])
                        .sum::<f64>()
                        / count,
                    solid
                        .outer
                        .points_xy_mm
                        .iter()
                        .map(|point| point[1])
                        .sum::<f64>()
                        / count,
                ];
                births.push(IslandBirth {
                    layer_index: layer_indices[layer_index],
                    z_mm: planes[layer_index],
                    centroid_xy_mm: centroid,
                    area_mm2: solid.area_mm2(),
                    footprint: solid.clone(),
                });
            }
        }
        previous = layer;
    }
    Ok(births)
}

fn overlap_candidates(
    current: &[SliceSolid],
    previous: &[SliceSolid],
    max_pairs: usize,
    max_bytes: usize,
) -> Result<BTreeMap<usize, Vec<usize>>, GenericPerceptionError> {
    if current.is_empty() || previous.is_empty() {
        return Ok(BTreeMap::new());
    }
    let upper = current.len().checked_mul(previous.len()).ok_or(
        GenericPerceptionError::OverlapPairLimit {
            required: usize::MAX,
            limit: max_pairs,
        },
    )?;
    if upper > max_pairs {
        return Err(GenericPerceptionError::OverlapPairLimit {
            required: upper,
            limit: max_pairs,
        });
    }
    let bytes = upper
        .checked_mul(2 * std::mem::size_of::<usize>() + std::mem::size_of::<f64>())
        .ok_or(GenericPerceptionError::OverlapByteLimit {
            required: usize::MAX,
            limit: max_bytes,
        })?;
    if bytes > max_bytes {
        return Err(GenericPerceptionError::OverlapByteLimit {
            required: bytes,
            limit: max_bytes,
        });
    }
    let bbox = |solid: &SliceSolid| {
        let xs = solid.outer.points_xy_mm.iter().map(|point| point[0]);
        let ys = solid.outer.points_xy_mm.iter().map(|point| point[1]);
        let min_x = xs.clone().fold(f64::INFINITY, f64::min);
        let max_x = xs.fold(f64::NEG_INFINITY, f64::max);
        let min_y = ys.clone().fold(f64::INFINITY, f64::min);
        let max_y = ys.fold(f64::NEG_INFINITY, f64::max);
        (
            [0.5 * (min_x + max_x), 0.5 * (min_y + max_y), 0.0],
            0.5 * ((max_x - min_x).hypot(max_y - min_y)),
        )
    };
    let previous_boxes = previous.iter().map(bbox).collect::<Vec<_>>();
    let current_boxes = current.iter().map(bbox).collect::<Vec<_>>();
    let radius = previous_boxes
        .iter()
        .map(|(_, radius)| *radius)
        .fold(0.0, f64::max)
        + current_boxes
            .iter()
            .map(|(_, radius)| *radius)
            .fold(0.0, f64::max);
    let index = PointCloudIndex::new(
        &previous_boxes
            .iter()
            .map(|(center, _)| *center)
            .collect::<Vec<_>>(),
    )
    .map_err(|error| GenericPerceptionError::PointIndex(error.to_string()))?;
    let nearby = index
        .radius_neighbors_batch(
            &current_boxes
                .iter()
                .map(|(center, _)| *center)
                .collect::<Vec<_>>(),
            radius,
            RadiusQueryLimits {
                max_output_pairs: upper,
                max_output_bytes: current
                    .len()
                    .checked_add(1)
                    .and_then(|rows| rows.checked_mul(std::mem::size_of::<usize>()))
                    .and_then(|offsets| {
                        upper
                            .checked_mul(std::mem::size_of::<u64>() + std::mem::size_of::<f64>())
                            .and_then(|pairs| offsets.checked_add(pairs))
                    })
                    .unwrap_or(usize::MAX),
            },
        )
        .map_err(|error| GenericPerceptionError::PointIndex(error.to_string()))?;
    if nearby.point_ids.len() > max_pairs {
        return Err(GenericPerceptionError::OverlapPairLimit {
            required: nearby.point_ids.len(),
            limit: max_pairs,
        });
    }
    Ok((0..current.len())
        .map(|row| {
            let range = nearby.row_range(row).expect("one row per current solid");
            (
                row,
                nearby.point_ids[range]
                    .iter()
                    .map(|id| *id as usize)
                    .collect(),
            )
        })
        .collect())
}

fn solid_contains(solid: &SliceSolid, point: [f64; 2]) -> bool {
    point_in_polygon(point, solid.outer.points_xy_mm())
        && !solid
            .holes
            .iter()
            .any(|hole| point_in_polygon(point, hole.points_xy_mm()))
}

struct OverlapOperationBudget {
    vertex_tests: usize,
    edge_tests: usize,
    max_vertex_tests: usize,
    max_edge_tests: usize,
}

impl OverlapOperationBudget {
    fn new(limits: GenericPerceptionLimits) -> Self {
        Self {
            vertex_tests: 0,
            edge_tests: 0,
            max_vertex_tests: limits.max_overlap_vertex_tests,
            max_edge_tests: limits.max_overlap_edge_tests,
        }
    }

    fn vertex(&mut self) -> Result<(), GenericPerceptionError> {
        self.vertex_tests = self.vertex_tests.saturating_add(1);
        if self.vertex_tests > self.max_vertex_tests {
            return Err(GenericPerceptionError::OverlapVertexTestLimit {
                required: self.vertex_tests,
                limit: self.max_vertex_tests,
            });
        }
        Ok(())
    }

    fn edge(&mut self) -> Result<(), GenericPerceptionError> {
        self.edge_tests = self.edge_tests.saturating_add(1);
        if self.edge_tests > self.max_edge_tests {
            return Err(GenericPerceptionError::OverlapEdgeTestLimit {
                required: self.edge_tests,
                limit: self.max_edge_tests,
            });
        }
        Ok(())
    }
}

fn solid_bbox(solid: &SliceSolid) -> [f64; 4] {
    let mut bbox = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for point in &solid.outer.points_xy_mm {
        bbox[0] = bbox[0].min(point[0]);
        bbox[1] = bbox[1].max(point[0]);
        bbox[2] = bbox[2].min(point[1]);
        bbox[3] = bbox[3].max(point[1]);
    }
    bbox
}

fn solids_overlap(
    a: &SliceSolid,
    b: &SliceSolid,
    budget: &mut OverlapOperationBudget,
) -> Result<bool, GenericPerceptionError> {
    let aa = solid_bbox(a);
    let bb = solid_bbox(b);
    if aa[1] < bb[0] || bb[1] < aa[0] || aa[3] < bb[2] || bb[3] < aa[2] {
        return Ok(false);
    }
    for (source, target) in [(&a.outer, &b.outer), (&b.outer, &a.outer)] {
        let step = (source.points_xy_mm.len() / 40).max(1);
        for index in (0..source.points_xy_mm.len()).step_by(step) {
            budget.vertex()?;
            if point_in_polygon(source.points_xy_mm[index], target.points_xy_mm()) {
                return Ok(true);
            }
        }
    }
    let left = &a.outer;
    let right = &b.outer;
    let left_step = (left.points_xy_mm.len() / 60).max(1);
    let right_step = (right.points_xy_mm.len() / 60).max(1);
    for i in (0..left.points_xy_mm.len()).step_by(left_step) {
        for j in (0..right.points_xy_mm.len()).step_by(right_step) {
            budget.edge()?;
            if segments_intersect(
                left.points_xy_mm[i],
                left.points_xy_mm[(i + left_step) % left.points_xy_mm.len()],
                right.points_xy_mm[j],
                right.points_xy_mm[(j + right_step) % right.points_xy_mm.len()],
            ) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn find_local_minima(
    mesh: &Mesh,
    surface: &NativeSurface,
    layers: &LayerPlan,
    births: &[IslandBirth],
    limits: GenericPerceptionLimits,
) -> Result<Vec<LocalZMinimum>, GenericPerceptionError> {
    let welded = WeldedView::build(mesh, WeldPolicy::ExactBits)
        .map_err(|error| GenericPerceptionError::Topology(error.to_string()))?;
    if welded.verts().len() > limits.max_minima_vertices {
        return Err(GenericPerceptionError::MinimaVertexLimit {
            required: welded.verts().len(),
            limit: limits.max_minima_vertices,
        });
    }
    let mut adjacency = vec![BTreeSet::<usize>::new(); welded.verts().len()];
    let mut incident = vec![Vec::<usize>::new(); welded.verts().len()];
    for (face, triangle) in mesh.tris().iter().enumerate() {
        let ids = triangle.map(|id| welded.map()[id as usize] as usize);
        for &(a, b) in &[(ids[0], ids[1]), (ids[1], ids[2]), (ids[2], ids[0])] {
            adjacency[a].insert(b);
            adjacency[b].insert(a);
        }
        for id in ids {
            incident[id].push(face);
        }
    }
    let constants = plinth_core::m0_constants()
        .map_err(|error| GenericPerceptionError::Topology(error.to_string()))?;
    let eps = constants.placeholders().minima_eps_mm().diagnostic().get();
    let connect_layers = constants
        .placeholders()
        .minima_connect_layers()
        .diagnostic()
        .get()
        .ceil() as usize;
    let normals = surface.geometry().normals();
    let mut candidates = (0..welded.verts().len())
        .filter(|&vertex| {
            adjacency[vertex]
                .iter()
                .all(|neighbor| welded.verts()[vertex][2] <= welded.verts()[*neighbor][2] + eps)
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| {
        welded.verts()[*a][2]
            .total_cmp(&welded.verts()[*b][2])
            .then_with(|| a.cmp(b))
    });
    let mut globally_visited = vec![false; welded.verts().len()];
    let mut minima = Vec::new();
    for start in candidates {
        if globally_visited[start] {
            continue;
        }
        let z = welded.verts()[start][2];
        let layer_index = layers.index_at(z).unwrap_or(0);
        let tolerance = layers.heights_mm()[layer_index..]
            .iter()
            .take(connect_layers)
            .sum::<f64>();
        let mut stack = vec![start];
        let mut seen = BTreeSet::from([start]);
        let mut local = vec![start];
        let mut reaches_lower = false;
        'flood: while let Some(vertex) = stack.pop() {
            for &neighbor in &adjacency[vertex] {
                if seen.contains(&neighbor) || welded.verts()[neighbor][2] > z + tolerance {
                    continue;
                }
                let nz = welded.verts()[neighbor][2];
                if nz < z - eps {
                    reaches_lower = true;
                    break 'flood;
                }
                seen.insert(neighbor);
                local.push(neighbor);
                stack.push(neighbor);
            }
        }
        for &vertex in &local {
            globally_visited[vertex] = true;
        }
        if reaches_lower {
            continue;
        }
        let representative = start;
        let mut normal = [0.0; 3];
        for &face in &incident[representative] {
            let source = mesh.tris()[face];
            let ids = source.map(|id| welded.map()[id as usize] as usize);
            let corner = ids
                .iter()
                .position(|id| *id == representative)
                .expect("incident face contains welded vertex");
            let p = welded.verts()[representative];
            let a = welded.verts()[ids[(corner + 1) % 3]];
            let b = welded.verts()[ids[(corner + 2) % 3]];
            let va = [a[0] - p[0], a[1] - p[1], a[2] - p[2]];
            let vb = [b[0] - p[0], b[1] - p[1], b[2] - p[2]];
            let la = (va[0] * va[0] + va[1] * va[1] + va[2] * va[2]).sqrt();
            let lb = (vb[0] * vb[0] + vb[1] * vb[1] + vb[2] * vb[2]).sqrt();
            let cosine =
                ((va[0] * vb[0] + va[1] * vb[1] + va[2] * vb[2]) / (la * lb)).clamp(-1.0, 1.0);
            let angle = cosine.acos();
            for (axis, value) in normal.iter_mut().enumerate() {
                *value += angle * normals[face][axis];
            }
        }
        let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if length > 0.0 {
            for value in &mut normal {
                *value /= length;
            }
        }
        minima.push(LocalZMinimum {
            position_mm: welded.verts()[representative],
            normal,
            region_points_mm: local.iter().map(|id| welded.verts()[*id]).collect(),
        });
    }
    // Independent filter A: nearest face, queried once for every minimum.
    minima = filter_minima_to_exterior_faces(
        surface.geometry().centroids(),
        surface.exterior_face_mask(),
        minima,
    )?;

    // Independent filter B: keep a true birth, or an inadequately carried
    // non-birth. The adequacy probes are gathered and sliced in one batch.
    let birth_match_layers = constants
        .placeholders()
        .minima_birth_match_layers()
        .diagnostic()
        .get()
        .ceil() as usize;
    let is_birth = minima
        .iter()
        .map(|minimum| {
            let z_tolerance = layer_tolerance(layers, minimum.position_mm[2], birth_match_layers);
            births.iter().any(|birth| {
                (birth.z_mm - minimum.position_mm[2]).abs() <= z_tolerance
                    && sampled_region_points(minimum)
                        .iter()
                        .any(|point| birth_xy_ok(&birth.footprint, [point[0], point[1]], 0.2))
            })
        })
        .collect::<Vec<_>>();
    let suspects = minima
        .iter()
        .enumerate()
        .filter(|(index, _)| !is_birth[*index])
        .collect::<Vec<_>>();
    let probe_heights = suspects
        .iter()
        .map(|(_, minimum)| {
            let index = layers
                .index_at(minimum.position_mm[2])
                .expect("validated layer schedule covers every minimum");
            minimum.position_mm[2] - layers.heights_mm()[index]
        })
        .collect::<Vec<_>>();
    let adequacy_slices = surface
        .slice_z_batch(
            &probe_heights,
            PlaneSliceLimits {
                max_planes: limits.max_planes,
                max_triangle_plane_tests: limits.max_triangle_plane_tests,
                max_intersections: limits.max_slice_intersections,
                max_output_bytes: limits.max_slice_output_bytes,
            },
        )
        .map_err(|error| GenericPerceptionError::Slice(error.to_string()))?;
    let threshold = constants
        .placeholders()
        .minima_support_area_tips()
        .diagnostic()
        .get()
        * std::f64::consts::PI
        * constants
            .placeholders()
            .tip_radius_default_mm()
            .diagnostic()
            .get()
            .powi(2);
    let mut adequate = vec![false; minima.len()];
    for (probe_row, (minimum_index, minimum)) in suspects.into_iter().enumerate() {
        let solids = stitch_layer(probe_row, adequacy_slices.plane(probe_row))?;
        let point = [minimum.position_mm[0], minimum.position_mm[1]];
        let carried = carried_outer_area(&solids, point);
        adequate[minimum_index] = carried >= threshold;
    }
    Ok(minima
        .into_iter()
        .enumerate()
        .filter(|(index, _)| is_birth[*index] || !adequate[*index])
        .map(|(_, minimum)| minimum)
        .collect())
}

fn filter_minima_to_exterior_faces(
    face_centroids: &[[f64; 3]],
    exterior: &[bool],
    mut minima: Vec<LocalZMinimum>,
) -> Result<Vec<LocalZMinimum>, GenericPerceptionError> {
    if minima.is_empty()
        || !exterior.iter().any(|value| *value)
        || !exterior.iter().any(|value| !*value)
    {
        return Ok(minima);
    }
    let queries = minima
        .iter()
        .map(|minimum| minimum.position_mm)
        .collect::<Vec<_>>();
    let nearest = PointCloudIndex::new(face_centroids)
        .map_err(|error| GenericPerceptionError::PointIndex(error.to_string()))?
        .knn_batch(&queries, 1)
        .map_err(|error| GenericPerceptionError::PointIndex(error.to_string()))?;
    minima = minima
        .into_iter()
        .enumerate()
        .filter(|(row, _)| {
            let range = nearest
                .row_range(*row)
                .expect("one all-face nearest row per minimum");
            exterior[nearest.point_ids[range.start] as usize]
        })
        .map(|(_, minimum)| minimum)
        .collect();
    Ok(minima)
}

fn carried_outer_area(solids: &[SliceSolid], point: [f64; 2]) -> f64 {
    // Python Path2D.discrete tests each positive outer loop independently.
    // A hollow cavity does not subtract from the load-bearing outer component
    // for this one adequacy predicate.
    solids
        .iter()
        .filter(|solid| point_in_polygon(point, solid.outer.points_xy_mm()))
        .map(|solid| solid.outer.signed_area_mm2)
        .fold(0.0, f64::max)
}

fn layer_tolerance(layers: &LayerPlan, z: f64, count: usize) -> f64 {
    let index = layers.index_at(z).unwrap_or(0);
    layers.heights_mm()[index..].iter().take(count).sum()
}

fn sampled_region_points(minimum: &LocalZMinimum) -> Vec<[f64; 3]> {
    let mut points = vec![minimum.position_mm];
    let region = &minimum.region_points_mm;
    if region.len() <= 64 {
        points.extend(region.iter().copied());
    } else {
        points.extend((0..64).map(|index| region[index * (region.len() - 1) / 63]));
    }
    points
}

fn birth_xy_ok(solid: &SliceSolid, point: [f64; 2], reach: f64) -> bool {
    if solid_contains(solid, point) {
        return true;
    }
    std::iter::once(&solid.outer)
        .chain(solid.holes.iter())
        .flat_map(|loop_| {
            (0..loop_.points_xy_mm.len()).map(move |index| {
                point_segment_distance(
                    point,
                    loop_.points_xy_mm[index],
                    loop_.points_xy_mm[(index + 1) % loop_.points_xy_mm.len()],
                )
            })
        })
        .fold(f64::INFINITY, f64::min)
        <= reach
}

fn point_segment_distance(point: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let denominator = ab[0] * ab[0] + ab[1] * ab[1];
    if denominator < 1.0e-15 {
        return (point[0] - a[0]).hypot(point[1] - a[1]);
    }
    let t = (((point[0] - a[0]) * ab[0] + (point[1] - a[1]) * ab[1]) / denominator).clamp(0.0, 1.0);
    (point[0] - (a[0] + t * ab[0])).hypot(point[1] - (a[1] + t * ab[1]))
}

fn build_load_eligibility(
    mesh: &Mesh,
    surface: &NativeSurface,
    islands: &[SupportIsland],
    specialized: &[SpecializedOwnerReservation],
) -> Result<Vec<LoadEligibility>, GenericPerceptionError> {
    let mut owners = BTreeMap::<u32, BTreeSet<LoadOwner>>::new();
    let mut specialized_pairs = BTreeSet::new();
    for reservation in specialized {
        if reservation.face_id as usize >= mesh.tris().len() {
            return Err(GenericPerceptionError::SpecializedFaceOutOfRange {
                face_id: reservation.face_id,
            });
        }
        if !specialized_pairs.insert((reservation.face_id, reservation.owner_id)) {
            return Err(GenericPerceptionError::DuplicateSpecializedReservation {
                owner_id: reservation.owner_id,
                face_id: reservation.face_id,
            });
        }
        owners
            .entry(reservation.face_id)
            .or_default()
            .insert(LoadOwner::Specialized(reservation.owner_id));
    }
    for island in islands {
        for &face in &island.face_ids {
            owners
                .entry(face)
                .or_default()
                .insert(LoadOwner::GenericIsland(island.id));
        }
    }
    Ok(owners
        .into_iter()
        .map(|(face_id, set)| LoadEligibility {
            face_id,
            eligible_owners: set.into_iter().collect(),
            centroid_mm: surface.geometry().centroids()[face_id as usize],
            area_mm2: surface.geometry().areas()[face_id as usize],
        })
        .collect())
}

/// Finalize the true unique-face load partition only after physical contacts
/// exist. Crate-private by design: generic perception cannot invent contacts.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "consumed by the next specialized-contact integration"
    )
)]
pub(crate) fn finalize_load_partition(
    plan: &GenericContactPlan,
    mesh: &Mesh,
    contacts: &[PhysicalContact],
) -> Result<FinalizedLoadPartition, GenericPerceptionError> {
    let fingerprint = mesh_fingerprint(mesh)
        .map_err(|error| GenericPerceptionError::Topology(error.to_string()))?;
    if fingerprint != plan.authority.mesh {
        return Err(GenericPerceptionError::StaleAuthority);
    }
    let mut ids = BTreeSet::new();
    for contact in contacts {
        if !ids.insert(contact.contact_id) {
            return Err(GenericPerceptionError::DuplicatePhysicalContact {
                contact_id: contact.contact_id,
            });
        }
        if contact.point_mm.iter().any(|value| !value.is_finite()) {
            return Err(GenericPerceptionError::NonFinitePhysicalContact {
                contact_id: contact.contact_id,
            });
        }
    }
    let mut by_owner = BTreeMap::<LoadOwner, Vec<&PhysicalContact>>::new();
    for contact in contacts {
        by_owner.entry(contact.owner).or_default().push(contact);
    }
    for values in by_owner.values_mut() {
        values.sort_by_key(|contact| contact.contact_id);
    }
    let mut nearest = BTreeMap::<(usize, LoadOwner), (&PhysicalContact, f64)>::new();
    for (owner, owner_contacts) in &by_owner {
        let face_rows = plan
            .load_eligibility
            .iter()
            .enumerate()
            .filter(|(_, eligibility)| eligibility.eligible_owners.binary_search(owner).is_ok())
            .collect::<Vec<_>>();
        if face_rows.is_empty() {
            continue;
        }
        let index = PointCloudIndex::new(
            &owner_contacts
                .iter()
                .map(|contact| contact.point_mm)
                .collect::<Vec<_>>(),
        )
        .map_err(|error| GenericPerceptionError::PointIndex(error.to_string()))?;
        let result = index
            .knn_batch(
                &face_rows
                    .iter()
                    .map(|(_, eligibility)| eligibility.centroid_mm)
                    .collect::<Vec<_>>(),
                1,
            )
            .map_err(|error| GenericPerceptionError::PointIndex(error.to_string()))?;
        for (query, (row, _)) in face_rows.into_iter().enumerate() {
            let range = result.row_range(query).expect("one nearest row per face");
            let point = result.point_ids[range.start] as usize;
            nearest.insert(
                (row, *owner),
                (owner_contacts[point], result.squared_distances[range.start]),
            );
        }
    }
    let mut rows = Vec::with_capacity(plan.load_eligibility.len());
    for (row, eligibility) in plan.load_eligibility.iter().enumerate() {
        let (contact, _) = eligibility
            .eligible_owners
            .iter()
            .filter_map(|owner| nearest.get(&(row, *owner)).copied())
            .min_by(|(left, left_distance), (right, right_distance)| {
                left_distance
                    .total_cmp(right_distance)
                    .then_with(|| left.contact_id.cmp(&right.contact_id))
            })
            .ok_or(GenericPerceptionError::MissingPhysicalContact {
                face_id: eligibility.face_id,
            })?;
        rows.push(FinalizedFaceLoad {
            face_id: eligibility.face_id,
            contact_id: contact.contact_id,
            owner: contact.owner,
            carried_area_mm2: eligibility.area_mm2,
        });
    }
    let mut contact_hash = Fnv::new();
    let mut canonical_contacts = contacts.iter().collect::<Vec<_>>();
    canonical_contacts.sort_by_key(|contact| contact.contact_id);
    for contact in canonical_contacts {
        contact_hash.u64(contact.contact_id);
        hash_owner(&mut contact_hash, contact.owner);
        for value in contact.point_mm {
            contact_hash.f64(value);
        }
    }
    Ok(FinalizedLoadPartition {
        authority: plan.authority.clone(),
        contacts_fingerprint: contact_hash.finish(),
        rows,
    })
}

/// Extend the one generic load authority with the faces served by finalized
/// specialized physical contacts, then invoke the sole load finalizer.  The
/// extension is crate-private so callers cannot manufacture eligible load
/// routes or bypass the generic plan's mesh binding.
pub(crate) fn finalize_specialized_load_partition(
    plan: &GenericContactPlan,
    mesh: &Mesh,
    surface: &NativeSurface,
    contacts: &[PhysicalContact],
    specialized_owner_faces: &[(u32, Vec<u32>)],
) -> Result<FinalizedLoadPartition, GenericPerceptionError> {
    let fingerprint = mesh_fingerprint(mesh)
        .map_err(|error| GenericPerceptionError::Topology(error.to_string()))?;
    if fingerprint != plan.authority.mesh {
        return Err(GenericPerceptionError::StaleAuthority);
    }
    let mut extended = plan.clone();
    for (owner_id, faces) in specialized_owner_faces {
        let owner = LoadOwner::Specialized(*owner_id);
        for &face_id in faces {
            if face_id as usize >= mesh.tris().len() {
                return Err(GenericPerceptionError::SpecializedFaceOutOfRange { face_id });
            }
            if let Some(row) = extended
                .load_eligibility
                .iter_mut()
                .find(|row| row.face_id == face_id)
            {
                match row.eligible_owners.binary_search(&owner) {
                    Ok(_) => {}
                    Err(index) => row.eligible_owners.insert(index, owner),
                }
            } else {
                extended.load_eligibility.push(LoadEligibility {
                    face_id,
                    eligible_owners: vec![owner],
                    centroid_mm: surface.geometry().centroids()[face_id as usize],
                    area_mm2: surface.geometry().areas()[face_id as usize],
                });
            }
        }
    }
    extended
        .load_eligibility
        .sort_by_key(|eligibility| eligibility.face_id);
    finalize_load_partition(&extended, mesh, contacts)
}

#[cfg_attr(
    not(test),
    allow(dead_code, reason = "used by the crate-private load finalizer")
)]
fn hash_owner(hash: &mut Fnv, owner: LoadOwner) {
    match owner {
        LoadOwner::Specialized(id) => {
            hash.u8(0);
            hash.u32(id);
        }
        LoadOwner::GenericIsland(id) => {
            hash.u8(1);
            hash.u32(id);
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the fingerprint must bind every result-shaping input without hiding an authority source"
)]
fn authority_fingerprint(
    mesh: &Mesh,
    surface: &NativeSurface,
    config: &SupportPerceptionConfig<'_>,
    frame_value: BuildFrame,
    layers: &LayerPlan,
    candidate_config: &CandidatePayloadConfig,
    specialized: &[SpecializedOwnerReservation],
    limits: GenericPerceptionLimits,
) -> Result<AuthorityFingerprint, GenericPerceptionError> {
    let mesh = mesh_fingerprint(mesh)
        .map_err(|error| GenericPerceptionError::Topology(error.to_string()))?;
    let mut native = Fnv::new();
    native.f64(config.surface_grid_pitch_mm.get());
    native.usize(config.max_grid_bytes);
    for value in surface
        .geometry()
        .normals()
        .iter()
        .flatten()
        .chain(surface.geometry().areas().iter())
    {
        native.f64(*value);
    }
    for value in surface.exterior_face_mask() {
        native.u8(u8::from(*value));
    }
    let mut result_config = Fnv::new();
    result_config.f64(config.column_pitch_mm.get());
    result_config.f64(config.overhang_angle_deg.get());
    result_config.f64(config.bridge_free_gap_mm.get());
    result_config.f64(config.near_plate_mm.get());
    if let Some(delicacy) = config.delicacy_by_face {
        for value in delicacy {
            result_config.f64(*value);
        }
    }
    let (depth, virtual_cells, candidates, compact_bytes, working_cells, edge_bits) =
        candidate_config.authority_fields();
    result_config.u32(depth);
    result_config.usize(virtual_cells);
    result_config.usize(candidates);
    result_config.usize(compact_bytes);
    result_config.usize(working_cells);
    result_config.u64(edge_bits);
    result_config.f64(config.plate_z_mm.get());
    result_config.usize(limits.max_planes);
    result_config.usize(limits.max_triangle_plane_tests);
    result_config.usize(limits.max_slice_intersections);
    result_config.usize(limits.max_slice_output_bytes);
    result_config.usize(limits.max_loop_vertices);
    result_config.usize(limits.max_minima_vertices);
    result_config.usize(limits.max_island_neighbor_pairs);
    result_config.usize(limits.max_island_neighbor_bytes);
    result_config.usize(limits.max_overlap_pairs);
    result_config.usize(limits.max_overlap_bytes);
    result_config.usize(limits.max_overlap_vertex_tests);
    result_config.usize(limits.max_overlap_edge_tests);
    let mut frame = Fnv::new();
    frame.f64(frame_value.plate_z_mm().get());
    frame.f64(frame_value.lift_mm().get());
    let mut layer_hash = Fnv::new();
    layer_hash.f64(layers.z0_mm());
    for value in layers.heights_mm() {
        layer_hash.f64(*value);
    }
    let mut specialized_hash = Fnv::new();
    let mut canonical_specialized = specialized.to_vec();
    canonical_specialized.sort_unstable();
    for reservation in canonical_specialized {
        specialized_hash.u32(reservation.owner_id);
        specialized_hash.u32(reservation.face_id);
    }
    Ok(AuthorityFingerprint {
        mesh,
        native: native.finish(),
        config: result_config.finish(),
        frame: frame.finish(),
        layers: layer_hash.finish(),
        specialized: specialized_hash.finish(),
    })
}

struct Fnv(u64);
impl Fnv {
    const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    fn u8(&mut self, value: u8) {
        self.bytes(&[value]);
    }
    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }
    fn usize(&mut self, value: usize) {
        self.u64(value as u64);
    }
    fn f64(&mut self, value: f64) {
        self.u64(value.to_bits());
    }
    const fn finish(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_core::{Finite, PositiveFinite};
    use plinth_mesh::{make_overhang_table, make_raking_blade, make_thin_fin, make_toed_slab};

    const SWEEP_CHILD: &str = "PLINTH_PERCEPTION_SWEEP_CHILD";
    const SWEEP_LINE: &str = "PLINTH_PERCEPTION_SWEEP=";

    fn perception_config<'a>(delicacy: &'a [f64]) -> SupportPerceptionConfig<'a> {
        SupportPerceptionConfig::current(
            Finite::new(0.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            64 * 1024 * 1024,
            Some(delicacy),
        )
        .unwrap()
    }

    fn candidate_config() -> CandidatePayloadConfig {
        CandidatePayloadConfig::current(8, 1_000_000, 3_000_000, 256 * 1024 * 1024, 65_536).unwrap()
    }

    fn layer_plan() -> LayerPlan {
        LayerPlan::new(0.0, vec![0.05; 1_000]).unwrap()
    }

    fn frame() -> BuildFrame {
        BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap())
    }

    fn covering_layer_plan(mesh: &Mesh) -> LayerPlan {
        let zmin = mesh
            .verts()
            .iter()
            .map(|point| point[2])
            .fold(f64::INFINITY, f64::min);
        let zmax = mesh
            .verts()
            .iter()
            .map(|point| point[2])
            .fold(f64::NEG_INFINITY, f64::max);
        let count = ((zmax - zmin) / 0.05).ceil() as usize + 1;
        LayerPlan::new(zmin, vec![0.05; count]).unwrap()
    }

    #[test]
    fn frozen_birth_overlap_ignores_retained_negative_winding_holes_like_python() {
        let square =
            |x0: f64, y0: f64, x1: f64, y1: f64| vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
        let outer = SliceLoop {
            points_xy_mm: square(0.0, 0.0, 4.0, 4.0),
            signed_area_mm2: 16.0,
            hole: false,
        };
        let hole = SliceLoop {
            points_xy_mm: square(1.0, 1.0, 2.0, 2.0),
            signed_area_mm2: -1.0,
            hole: true,
        };
        let inside_prior_hole = SliceLoop {
            points_xy_mm: vec![[1.1, 1.1], [1.9, 1.1], [1.9, 1.3], [1.1, 1.8]],
            signed_area_mm2: 0.36,
            hole: false,
        };
        let disjoint_asymmetric = SliceLoop {
            points_xy_mm: vec![[5.1, 1.1], [5.9, 1.1], [5.9, 1.3], [5.1, 1.8]],
            signed_area_mm2: 0.36,
            hole: false,
        };
        let first = SliceSolid {
            outer: outer.clone(),
            holes: vec![hole],
        };
        assert_eq!(first.holes.len(), 1, "hole remains retained as evidence");
        let births = find_births(
            &[0, 1],
            &[0.025, 0.075],
            &[
                vec![first.clone()],
                vec![
                    first,
                    SliceSolid {
                        outer: inside_prior_hole,
                        holes: vec![],
                    },
                    SliceSolid {
                        outer: disjoint_asymmetric,
                        holes: vec![],
                    },
                ],
            ],
            GenericPerceptionLimits {
                max_overlap_pairs: 16,
                max_overlap_bytes: 4096,
                ..GenericPerceptionLimits::production()
            },
        )
        .unwrap();
        assert_eq!(births.len(), 2);
        assert_eq!(births[0].centroid_xy_mm, [2.0, 2.0]);
        assert!(
            !births
                .iter()
                .any(|birth| birth.centroid_xy_mm == [1.5, 1.325]),
            "a current outer inside the prior outer is overlapping in the live Python predicate, even when it lies in a retained hole"
        );
        // Boundary-point mean, matching frozen Path2D.discrete behavior;
        // deliberately asymmetric so an area centroid cannot pass.
        assert_eq!(births[1].centroid_xy_mm, [5.5, 1.325]);
    }

    #[test]
    fn hollow_adequacy_uses_positive_outer_area_even_when_point_is_in_hole() {
        let solid = SliceSolid {
            outer: SliceLoop {
                points_xy_mm: vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]],
                signed_area_mm2: 16.0,
                hole: false,
            },
            holes: vec![SliceLoop {
                points_xy_mm: vec![[1.0, 1.0], [1.0, 3.0], [3.0, 3.0], [3.0, 1.0]],
                signed_area_mm2: -4.0,
                hole: true,
            }],
        };
        assert_eq!(carried_outer_area(&[solid], [2.0, 2.0]), 16.0);
    }

    #[test]
    fn exterior_filter_uses_one_all_face_query_and_global_face_id_tie() {
        let minimum = LocalZMinimum {
            position_mm: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, -1.0],
            region_points_mm: vec![[0.0, 0.0, 0.0]],
        };
        let centroids = [[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        assert_eq!(
            filter_minima_to_exterior_faces(&centroids, &[false, true], vec![minimum.clone()])
                .unwrap(),
            Vec::<LocalZMinimum>::new()
        );
        assert_eq!(
            filter_minima_to_exterior_faces(&centroids, &[true, false], vec![minimum.clone()])
                .unwrap(),
            vec![minimum]
        );
    }

    #[test]
    fn island_neighbor_dense_output_accepts_exact_caps_and_rejects_cap_plus_one() {
        let mesh = plinth_mesh::make_box([0.01, 0.01, 0.01], [0.0; 3]).unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let config = perception_config(&delicacy);
        let surface = build_surface(&mesh, &config).unwrap();
        let topology = plinth_mesh::build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let raw = ContactCandidatePool {
            candidates: (0..mesh.tris().len())
                .map(|face| ContactCandidate {
                    id: ContactCandidateId(face as u64),
                    face_id: face as u32,
                    point_mm: surface.geometry().centroids()[face],
                    normal: surface.geometry().normals()[face],
                    supported_area_mm2: surface.geometry().areas()[face],
                    overhang_excess: 1.0,
                    delicacy: 0.0,
                })
                .collect(),
        };
        let count = mesh.tris().len();
        let pairs = count * count;
        let bytes = (count + 1) * std::mem::size_of::<usize>()
            + pairs * (std::mem::size_of::<u64>() + std::mem::size_of::<f64>());
        let exact = GenericPerceptionLimits {
            max_island_neighbor_pairs: pairs,
            max_island_neighbor_bytes: bytes,
            ..GenericPerceptionLimits::production()
        };
        assert_eq!(
            connected_islands(&raw, &topology, &surface, exact)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            connected_islands(
                &raw,
                &topology,
                &surface,
                GenericPerceptionLimits {
                    max_island_neighbor_pairs: pairs - 1,
                    ..exact
                }
            ),
            Err(GenericPerceptionError::IslandNeighborPairLimit {
                required: pairs,
                limit: pairs - 1,
            })
        );
        assert_eq!(
            connected_islands(
                &raw,
                &topology,
                &surface,
                GenericPerceptionLimits {
                    max_island_neighbor_bytes: bytes - 1,
                    ..exact
                }
            ),
            Err(GenericPerceptionError::IslandNeighborByteLimit {
                required: bytes,
                limit: bytes - 1,
            })
        );
    }

    #[test]
    fn overlap_sampling_enforces_exact_vertex_and_edge_operation_caps() {
        let solid = |points_xy_mm: Vec<[f64; 2]>| SliceSolid {
            outer: SliceLoop {
                signed_area_mm2: signed_area(&points_xy_mm),
                points_xy_mm,
                hole: false,
            },
            holes: vec![],
        };
        // Bboxes overlap, while the triangles have a deterministic gap.
        let a = solid(vec![[0.0, 0.0], [2.0, 0.0], [0.0, 2.0]]);
        let b = solid(vec![[2.0, 2.0], [2.0, 0.5], [0.5, 2.0]]);
        let mut exact = OverlapOperationBudget {
            vertex_tests: 0,
            edge_tests: 0,
            max_vertex_tests: 6,
            max_edge_tests: 9,
        };
        assert!(!solids_overlap(&a, &b, &mut exact).unwrap());
        assert_eq!((exact.vertex_tests, exact.edge_tests), (6, 9));

        let mut vertex_short = OverlapOperationBudget {
            max_vertex_tests: 5,
            ..exact
        };
        vertex_short.vertex_tests = 0;
        vertex_short.edge_tests = 0;
        assert_eq!(
            solids_overlap(&a, &b, &mut vertex_short),
            Err(GenericPerceptionError::OverlapVertexTestLimit {
                required: 6,
                limit: 5,
            })
        );
        let mut edge_short = OverlapOperationBudget {
            vertex_tests: 0,
            edge_tests: 0,
            max_vertex_tests: 6,
            max_edge_tests: 8,
        };
        assert_eq!(
            solids_overlap(&a, &b, &mut edge_short),
            Err(GenericPerceptionError::OverlapEdgeTestLimit {
                required: 9,
                limit: 8,
            })
        );
    }

    #[test]
    fn frozen_segment_vector_deduplicates_shared_owner_and_stitches_one_loop() {
        use geom_kernel::{PlaneIntersection, PlaneIntersectionKind};
        let edge = |face_id, a: [f64; 2], b: [f64; 2]| PlaneIntersection {
            face_id,
            kind: PlaneIntersectionKind::Basic,
            segment: [[a[0], a[1], 0.5], [b[0], b[1], 0.5]],
        };
        let events = vec![
            edge(0, [0.0, 0.0], [2.0, 0.0]),
            edge(1, [2.0, 0.0], [2.0, 2.0]),
            edge(2, [2.0, 2.0], [0.0, 2.0]),
            edge(3, [0.0, 2.0], [0.0, 0.0]),
            // Frozen native slicing deliberately retains duplicate positive
            // edge owners. Loop assembly owns this deterministic dedup.
            edge(4, [2.0, 0.0], [0.0, 0.0]),
        ];
        let loops = stitch_layer(7, &events).unwrap();
        assert_eq!(loops.len(), 1);
        assert_eq!(
            loops[0].outer().points_xy_mm(),
            &[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]
        );
        assert_eq!(loops[0].area_mm2(), 4.0);
        assert!(loops[0].holes().is_empty());
    }

    #[test]
    fn unique_face_partition_is_finalized_from_physical_contacts() {
        let mesh = make_overhang_table().unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let plan = prepare_generic_contacts(
            &mesh,
            &perception_config(&delicacy),
            frame(),
            &layer_plan(),
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(),
        )
        .unwrap();
        let contacts = plan
            .islands()
            .iter()
            .flat_map(|island| {
                [
                    PhysicalContact {
                        contact_id: u64::from(island.id) + 100,
                        owner: LoadOwner::GenericIsland(island.id),
                        point_mm: [0.0, 0.0, 0.0],
                    },
                    PhysicalContact {
                        contact_id: u64::from(island.id) + 10,
                        owner: LoadOwner::GenericIsland(island.id),
                        point_mm: [0.0, 0.0, 0.0],
                    },
                ]
            })
            .collect::<Vec<_>>();
        let partition = finalize_load_partition(&plan, &mesh, &contacts).unwrap();
        let unique = partition
            .rows()
            .iter()
            .map(|row| row.face_id)
            .collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), partition.rows().len());
        assert!(partition
            .rows()
            .iter()
            .all(|row| row.carried_area_mm2 > 0.0));
        assert!(partition.rows().iter().all(|row| row.contact_id < 100));
        assert_eq!(
            partition
                .rows()
                .iter()
                .map(|row| row.carried_area_mm2)
                .sum::<f64>(),
            plan.load_eligibility
                .iter()
                .map(|row| row.area_mm2)
                .sum::<f64>()
        );
    }

    #[test]
    fn box_full_birth_minimum_and_solid_area_vector_is_frozen() {
        let mesh = plinth_mesh::make_box_default().unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let plan = prepare_generic_contacts(
            &mesh,
            &perception_config(&delicacy),
            frame(),
            &layer_plan(),
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(),
        )
        .unwrap();
        assert_eq!(plan.island_births().len(), 1);
        let birth = &plan.island_births()[0];
        assert_eq!(birth.layer_index, 0);
        assert_eq!(birth.z_mm, 0.025);
        assert_eq!(birth.centroid_xy_mm, [0.0, 0.0]);
        assert_eq!(birth.area_mm2, 100.0);
        assert!(birth.footprint().holes().is_empty());
        assert_eq!(plan.local_z_minima().len(), 1);
        let minimum = &plan.local_z_minima()[0];
        assert_eq!(minimum.position_mm, [-5.0, -5.0, 0.0]);
        let diagonal = -1.0 / 3.0_f64.sqrt();
        assert_eq!(minimum.normal, [diagonal, diagonal, diagonal]);
        let mut region = minimum.region_points_mm().to_vec();
        region.sort_by(|left, right| {
            left[0]
                .total_cmp(&right[0])
                .then_with(|| left[1].total_cmp(&right[1]))
                .then_with(|| left[2].total_cmp(&right[2]))
        });
        assert_eq!(
            region,
            vec![
                [-5.0, -5.0, 0.0],
                [-5.0, 5.0, 0.0],
                [5.0, -5.0, 0.0],
                [5.0, 5.0, 0.0],
            ]
        );
    }

    #[test]
    fn overlap_caps_have_exact_cap_and_cap_plus_one_boundaries() {
        let loop_ = SliceLoop {
            points_xy_mm: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            signed_area_mm2: 1.0,
            hole: false,
        };
        let solid = SliceSolid {
            outer: loop_,
            holes: vec![],
        };
        let rows = vec![solid.clone(), solid];
        assert!(overlap_candidates(&rows, &rows, 4, 96).is_ok());
        assert_eq!(
            overlap_candidates(&rows, &rows, 3, 96),
            Err(GenericPerceptionError::OverlapPairLimit {
                required: 4,
                limit: 3,
            })
        );
        assert_eq!(
            overlap_candidates(&rows, &rows, 4, 95),
            Err(GenericPerceptionError::OverlapByteLimit {
                required: 96,
                limit: 95,
            })
        );
    }

    #[test]
    fn table_toe_raking_and_thin_fin_are_bound() {
        for mesh in [
            make_overhang_table().unwrap(),
            make_toed_slab().unwrap(),
            make_raking_blade().unwrap(),
            make_thin_fin().unwrap(),
        ] {
            let delicacy = vec![0.0; mesh.tris().len()];
            let plan = prepare_generic_contacts(
                &mesh,
                &perception_config(&delicacy),
                frame(),
                &layer_plan(),
                &candidate_config(),
                &[],
                GenericPerceptionLimits::production(),
            )
            .expect("required semantic fixture perception");
            plan.validate_binding(
                &mesh,
                &perception_config(&delicacy),
                frame(),
                &layer_plan(),
                &candidate_config(),
                &[],
                GenericPerceptionLimits::production(),
            )
            .unwrap();
            assert_ne!(plan.digest(), 0);
        }
    }

    #[test]
    fn frozen_live_fixture_birth_and_filtered_minimum_counts_match_python() {
        let table = make_overhang_table().unwrap();
        let table_delicacy = vec![0.0; table.tris().len()];
        let table_plan = prepare_generic_contacts(
            &table,
            &perception_config(&table_delicacy),
            frame(),
            &layer_plan(),
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(),
        )
        .unwrap();
        assert_eq!(table_plan.island_births().len(), 4);
        assert_eq!(table_plan.local_z_minima().len(), 4);

        let toe = make_toed_slab().unwrap();
        let toe_delicacy = vec![0.0; toe.tris().len()];
        let toe_plan = prepare_generic_contacts(
            &toe,
            &perception_config(&toe_delicacy),
            frame(),
            &layer_plan(),
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(),
        )
        .unwrap();
        assert_eq!(toe_plan.local_z_minima().len(), 3);
        let toe_z = toe_plan
            .local_z_minima()
            .iter()
            .map(|minimum| minimum.position_mm[2])
            .collect::<Vec<_>>();
        assert!(toe_z.iter().any(|z| (*z - 9.0).abs() < 1.0e-3));
        assert!(toe_z.iter().any(|z| (*z - 11.0).abs() < 1.0e-3));

        let raking = make_raking_blade().unwrap();
        let raking_delicacy = vec![0.0; raking.tris().len()];
        let raking_plan = prepare_generic_contacts(
            &raking,
            &perception_config(&raking_delicacy),
            frame(),
            &layer_plan(),
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(),
        )
        .unwrap();
        assert!(raking_plan.island_births().len() >= 5);
        assert!(raking_plan.local_z_minima().len() <= 2);
        assert!(raking_plan.island_births().len() > raking_plan.local_z_minima().len() + 3);
    }

    #[test]
    fn required_sphere_fixture_is_loaded_and_binding_rejects_another_mesh() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/tests/fixtures/chitubox/Sphere.stl");
        assert!(path.is_file(), "required Sphere.stl fixture is missing");
        let mesh = plinth_mesh::load_binary_stl(&path, WeldPolicy::None)
            .expect("required Sphere.stl must parse");
        let delicacy = vec![0.0; mesh.tris().len()];
        let layers = covering_layer_plan(&mesh);
        let plan = prepare_generic_contacts(
            &mesh,
            &perception_config(&delicacy),
            frame(),
            &layers,
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(),
        )
        .expect("Sphere perception");
        let other = plinth_mesh::make_box_default().unwrap();
        let other_delicacy = vec![0.0; other.tris().len()];
        assert_eq!(
            plan.validate_binding(
                &other,
                &perception_config(&other_delicacy),
                frame(),
                &layers,
                &candidate_config(),
                &[],
                GenericPerceptionLimits::production(),
            ),
            Err(GenericPerceptionError::StaleAuthority)
        );
    }

    #[test]
    fn authority_stale_matrix_binds_every_result_shaping_input() {
        let mesh = plinth_mesh::make_box_default().unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let config = perception_config(&delicacy);
        let layers = layer_plan();
        let candidates = candidate_config();
        let reservations = [SpecializedOwnerReservation {
            owner_id: 1,
            face_id: 0,
        }];
        let limits = GenericPerceptionLimits::production();
        let plan = prepare_generic_contacts(
            &mesh,
            &config,
            frame(),
            &layers,
            &candidates,
            &reservations,
            limits,
        )
        .unwrap();
        assert!(plan
            .validate_binding(
                &mesh,
                &config,
                frame(),
                &layers,
                &candidates,
                &reservations,
                limits,
            )
            .is_ok());

        let other_mesh = plinth_mesh::make_box([11.0, 10.0, 10.0], [0.0; 3]).unwrap();
        let other_delicacy = vec![0.0; other_mesh.tris().len()];
        assert_eq!(
            plan.validate_binding(
                &other_mesh,
                &perception_config(&other_delicacy),
                frame(),
                &layers,
                &candidates,
                &reservations,
                limits,
            ),
            Err(GenericPerceptionError::StaleAuthority)
        );
        let changed_config = SupportPerceptionConfig::current(
            Finite::new(0.0).unwrap(),
            PositiveFinite::new(2.0).unwrap(),
            PositiveFinite::new(0.5).unwrap(),
            64 * 1024 * 1024,
            Some(&delicacy),
        )
        .unwrap();
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &changed_config,
                frame(),
                &layers,
                &candidates,
                &reservations,
                limits,
            ),
            Err(GenericPerceptionError::StaleAuthority)
        );
        let changed_frame =
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(6.0).unwrap());
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &config,
                changed_frame,
                &layers,
                &candidates,
                &reservations,
                limits,
            ),
            Err(GenericPerceptionError::StaleAuthority)
        );
        let mut changed_heights = layers.heights_mm().to_vec();
        changed_heights[10] = 0.051;
        let changed_layers = LayerPlan::new(0.0, changed_heights).unwrap();
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &config,
                frame(),
                &changed_layers,
                &candidates,
                &reservations,
                limits,
            ),
            Err(GenericPerceptionError::StaleAuthority)
        );
        let changed_candidates =
            CandidatePayloadConfig::current(7, 1_000_000, 3_000_000, 256 * 1024 * 1024, 65_536)
                .unwrap();
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &config,
                frame(),
                &layers,
                &changed_candidates,
                &reservations,
                limits,
            ),
            Err(GenericPerceptionError::StaleAuthority)
        );
        let changed_reservations = [SpecializedOwnerReservation {
            owner_id: 2,
            face_id: 0,
        }];
        assert_eq!(
            plan.validate_binding(
                &mesh,
                &config,
                frame(),
                &layers,
                &candidates,
                &changed_reservations,
                limits,
            ),
            Err(GenericPerceptionError::StaleAuthority)
        );
        for changed_limits in [
            GenericPerceptionLimits {
                max_island_neighbor_pairs: limits.max_island_neighbor_pairs - 1,
                ..limits
            },
            GenericPerceptionLimits {
                max_island_neighbor_bytes: limits.max_island_neighbor_bytes - 1,
                ..limits
            },
            GenericPerceptionLimits {
                max_overlap_pairs: limits.max_overlap_pairs - 1,
                ..limits
            },
            GenericPerceptionLimits {
                max_overlap_bytes: limits.max_overlap_bytes - 1,
                ..limits
            },
            GenericPerceptionLimits {
                max_overlap_vertex_tests: limits.max_overlap_vertex_tests - 1,
                ..limits
            },
            GenericPerceptionLimits {
                max_overlap_edge_tests: limits.max_overlap_edge_tests - 1,
                ..limits
            },
        ] {
            assert_eq!(
                plan.validate_binding(
                    &mesh,
                    &config,
                    frame(),
                    &layers,
                    &candidates,
                    &reservations,
                    changed_limits,
                ),
                Err(GenericPerceptionError::StaleAuthority)
            );
        }
        let wrong_plate = BuildFrame::new(
            Finite::new(-1.0).unwrap(),
            PositiveFinite::new(5.0).unwrap(),
        );
        assert_eq!(
            prepare_generic_contacts(
                &mesh,
                &config,
                wrong_plate,
                &layers,
                &candidates,
                &reservations,
                limits,
            ),
            Err(GenericPerceptionError::FrameMismatch)
        );
    }

    #[test]
    fn generic_plan_digest_is_identical_at_one_four_and_sixteen_threads() {
        if std::env::var(SWEEP_CHILD).as_deref() != Ok("1") {
            let rows = [1usize, 4, 16]
                .into_iter()
                .map(|threads| {
                    let output = std::process::Command::new(
                        std::env::current_exe().expect("current test executable"),
                    )
                    .args([
                        "--exact",
                        "perception::tests::generic_plan_digest_is_identical_at_one_four_and_sixteen_threads",
                        "--nocapture",
                    ])
                    .env("PLINTH_THREADS", threads.to_string())
                    .env(SWEEP_CHILD, "1")
                    .output()
                    .expect("spawn thread-sweep child");
                    assert!(
                        output.status.success(),
                        "thread child {threads} failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let line = stdout
                        .lines()
                        .find_map(|line| line.strip_prefix(SWEEP_LINE))
                        .expect("thread child emitted evidence");
                    let values = line
                        .split(',')
                        .map(|value| value.parse::<u64>().unwrap())
                        .collect::<Vec<_>>();
                    assert_eq!(values[1], threads as u64);
                    values
                })
                .collect::<Vec<_>>();
            assert_eq!(rows[0][0], rows[1][0]);
            assert_eq!(rows[0][0], rows[2][0]);
            return;
        }

        // One thousand real in-bounds slice tasks make participation evidence
        // robust even when the whole workspace test suite is concurrently
        // busy; a tiny sphere can legitimately be completed by one worker.
        let mesh = plinth_mesh::make_box([10.0, 10.0, 50.0], [0.0; 3]).unwrap();
        let delicacy = vec![0.0; mesh.tris().len()];
        let config = SupportPerceptionConfig::current(
            Finite::new(0.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            64 * 1024 * 1024,
            Some(&delicacy),
        )
        .unwrap();
        let layers = LayerPlan::new(0.0, vec![0.05; 1_001]).unwrap();
        let plan = prepare_generic_contacts(
            &mesh,
            &config,
            frame(),
            &layers,
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(),
        )
        .expect("Sphere perception");
        let (effective, participating) = plan.slice_thread_evidence();
        println!("{SWEEP_LINE}{},{effective},{participating}", plan.digest());
    }
}
