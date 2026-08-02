//! Bound, batch-first generic support perception for the frozen pre-P8 path.
//!
//! The older [`crate::perceive_contacts`] leaf remains the sole authority for
//! exterior/down-facing/near-plate/column-ray classification.  This module
//! adds the orchestration which turns that evidence into islands, dyadic
//! seats, print-layer births and local minima.  Its result is deliberately
//! opaque: downstream specialized placement can consume the evidence, but it
//! cannot manufacture or replay it against another mesh or layer schedule.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use geom_kernel::{NativeSurface, NativeSurfaceConfig, PlaneSliceLimits, PointCloudIndex};
use plinth_core::LayerPlan;
use plinth_mesh::{
    build_face_topology, mesh_fingerprint, FaceTopology, Mesh, WeldPolicy, WeldedView,
};
use thiserror::Error;

use crate::dyadic::{build_candidate_payload, CandidatePayloadConfig, DyadicError};
use crate::{
    perceive_contacts, ContactCandidate, ContactCandidateId, ContactCandidatePool, SupportIsland,
    SupportPerceptionConfig, SupportPerceptionError,
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
        }
    }
}

/// A specialized owner known before generic load partitioning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SpecializedOwnerReservation {
    pub owner_id: u32,
    pub face_id: u32,
}

/// Closed owner vocabulary used by the unique-face load partition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LoadOwner {
    Specialized(u32),
    GenericIsland(u32),
}

/// One face appears exactly once in this evidence stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceLoadOwner {
    pub face_id: u32,
    pub owner: LoadOwner,
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

/// First printable appearance of one solid slice component.
#[derive(Clone, Debug, PartialEq)]
pub struct IslandBirth {
    pub layer_index: usize,
    pub z_mm: f64,
    pub centroid_xy_mm: [f64; 2],
    pub area_mm2: f64,
    footprint: SliceLoop,
}

impl IslandBirth {
    #[must_use]
    pub const fn footprint(&self) -> &SliceLoop {
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
    slice_layers: Vec<Vec<SliceLoop>>,
    island_births: Vec<IslandBirth>,
    local_z_minima: Vec<LocalZMinimum>,
    load_partition: Vec<FaceLoadOwner>,
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
    pub fn slice_layers(&self) -> &[Vec<SliceLoop>] {
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
    pub fn load_partition(&self) -> &[FaceLoadOwner] {
        &self.load_partition
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
            for point in birth.footprint.points_xy_mm() {
                hash.f64(point[0]);
                hash.f64(point[1]);
            }
        }
        for minimum in &self.local_z_minima {
            for value in minimum.position_mm.iter().chain(minimum.normal.iter()) {
                hash.f64(*value);
            }
        }
        for row in &self.load_partition {
            hash.u32(row.face_id);
            match row.owner {
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
        hash.finish()
    }

    /// Reject replay against any different geometry, native setup, frame,
    /// result-changing configuration or layer schedule.
    pub fn validate_binding(
        &self,
        mesh: &Mesh,
        config: &SupportPerceptionConfig<'_>,
        layers: &LayerPlan,
    ) -> Result<(), GenericPerceptionError> {
        let surface = build_surface(mesh, config)?;
        let current = authority_fingerprint(mesh, &surface, config, layers)?;
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
    #[error("generic perception produced an island without a legal dyadic candidate: {island_id}")]
    NoLegalCandidate { island_id: u32 },
    #[error("generic perception authority is stale or belongs to another input")]
    StaleAuthority,
}

/// Prepare the complete current generic evidence in batched native calls.
pub fn prepare_generic_contacts(
    mesh: &Mesh,
    config: &SupportPerceptionConfig<'_>,
    layers: &LayerPlan,
    candidate_config: &CandidatePayloadConfig,
    specialized: &[SpecializedOwnerReservation],
    limits: GenericPerceptionLimits,
) -> Result<GenericContactPlan, GenericPerceptionError> {
    let raw = perceive_contacts(mesh, config)?;
    let topology = build_face_topology(mesh, WeldPolicy::ExactBits)
        .map_err(|error| GenericPerceptionError::Topology(error.to_string()))?;
    let surface = build_surface(mesh, config)?;
    let islands = connected_islands(&raw, &topology, &surface)?;
    let payload = build_candidate_payload(mesh, &islands, candidate_config)?;
    let delicacy = config
        .delicacy_by_face
        .ok_or(SupportPerceptionError::MissingDelicacySidecar)?;
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
                    .map(|loop_| loop_.points_xy_mm.len())
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
    let island_births = find_births(&layer_indices, &planes, &slice_layers);
    let local_z_minima = find_local_minima(mesh, &surface, layers, limits.max_minima_vertices)?;
    let load_partition = partition_load(mesh, &islands, specialized)?;
    let authority = authority_fingerprint(mesh, &surface, config, layers)?;
    Ok(GenericContactPlan {
        authority,
        islands,
        candidates,
        slice_layers,
        island_births,
        local_z_minima,
        load_partition,
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
        .radius_neighbors_batch(&points, radius)
        .map_err(|error| GenericPerceptionError::PointIndex(error.to_string()))?;
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
) -> Result<Vec<SliceLoop>, GenericPerceptionError> {
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
    Ok(loops)
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

fn polygons_overlap(a: &[[f64; 2]], b: &[[f64; 2]]) -> bool {
    if a.iter().any(|point| point_in_polygon(*point, b))
        || b.iter().any(|point| point_in_polygon(*point, a))
    {
        return true;
    }
    (0..a.len()).any(|ai| {
        (0..b.len()).any(|bi| {
            segments_intersect(a[ai], a[(ai + 1) % a.len()], b[bi], b[(bi + 1) % b.len()])
        })
    })
}

fn segments_intersect(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    fn orient(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    }
    let ab_c = orient(a, b, c);
    let ab_d = orient(a, b, d);
    let cd_a = orient(c, d, a);
    let cd_b = orient(c, d, b);
    let on_segment = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        q[0] >= p[0].min(r[0]) - LOOP_MERGE_MM
            && q[0] <= p[0].max(r[0]) + LOOP_MERGE_MM
            && q[1] >= p[1].min(r[1]) - LOOP_MERGE_MM
            && q[1] <= p[1].max(r[1]) + LOOP_MERGE_MM
    };
    if ab_c.abs() <= LOOP_MERGE_MM && on_segment(a, c, b)
        || ab_d.abs() <= LOOP_MERGE_MM && on_segment(a, d, b)
        || cd_a.abs() <= LOOP_MERGE_MM && on_segment(c, a, d)
        || cd_b.abs() <= LOOP_MERGE_MM && on_segment(c, b, d)
    {
        return true;
    }
    (ab_c > 0.0) != (ab_d > 0.0) && (cd_a > 0.0) != (cd_b > 0.0)
}

fn find_births(
    layer_indices: &[usize],
    planes: &[f64],
    layers: &[Vec<SliceLoop>],
) -> Vec<IslandBirth> {
    let mut births = Vec::new();
    let mut previous: Vec<&SliceLoop> = Vec::new();
    for (layer_index, layer) in layers.iter().enumerate() {
        let current = layer.iter().filter(|loop_| !loop_.hole).collect::<Vec<_>>();
        for loop_ in &current {
            if !previous
                .iter()
                .any(|prior| polygons_overlap(loop_.points_xy_mm(), prior.points_xy_mm()))
            {
                let centroid = polygon_centroid(loop_.points_xy_mm());
                births.push(IslandBirth {
                    layer_index: layer_indices[layer_index],
                    z_mm: planes[layer_index],
                    centroid_xy_mm: centroid,
                    area_mm2: loop_.signed_area_mm2.abs(),
                    footprint: (*loop_).clone(),
                });
            }
        }
        previous = current;
    }
    births
}

fn polygon_centroid(points: &[[f64; 2]]) -> [f64; 2] {
    let mut cross_sum = 0.0;
    let mut x = 0.0;
    let mut y = 0.0;
    for i in 0..points.len() {
        let j = (i + 1) % points.len();
        let cross = points[i][0] * points[j][1] - points[j][0] * points[i][1];
        cross_sum += cross;
        x += (points[i][0] + points[j][0]) * cross;
        y += (points[i][1] + points[j][1]) * cross;
    }
    if cross_sum.abs() <= f64::EPSILON {
        let count = points.len() as f64;
        return [
            points.iter().map(|point| point[0]).sum::<f64>() / count,
            points.iter().map(|point| point[1]).sum::<f64>() / count,
        ];
    }
    [x / (3.0 * cross_sum), y / (3.0 * cross_sum)]
}

fn find_local_minima(
    mesh: &Mesh,
    surface: &NativeSurface,
    layers: &LayerPlan,
    cap: usize,
) -> Result<Vec<LocalZMinimum>, GenericPerceptionError> {
    let welded = WeldedView::build(mesh, WeldPolicy::ExactBits)
        .map_err(|error| GenericPerceptionError::Topology(error.to_string()))?;
    if welded.verts().len() > cap {
        return Err(GenericPerceptionError::MinimaVertexLimit {
            required: welded.verts().len(),
            limit: cap,
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
    let exterior = surface.exterior_face_mask();
    let normals = surface.geometry().normals();
    let mut candidates = (0..welded.verts().len())
        .filter(|&vertex| {
            adjacency[vertex]
                .iter()
                .all(|neighbor| welded.verts()[vertex][2] <= welded.verts()[*neighbor][2] + eps)
                && incident[vertex].iter().any(|face| exterior[*face])
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
        let mut queue = VecDeque::from([start]);
        let mut local = BTreeSet::from([start]);
        let mut reaches_lower = false;
        while let Some(vertex) = queue.pop_front() {
            for &neighbor in &adjacency[vertex] {
                let nz = welded.verts()[neighbor][2];
                if nz < z - eps {
                    reaches_lower = true;
                }
                if nz >= z - eps && nz <= z + tolerance && local.insert(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
        for &vertex in &local {
            globally_visited[vertex] = true;
        }
        if reaches_lower {
            continue;
        }
        let representative = *local
            .iter()
            .min_by(|a, b| {
                welded.verts()[**a][2]
                    .total_cmp(&welded.verts()[**b][2])
                    .then_with(|| a.cmp(b))
            })
            .expect("minimum region contains its seed");
        let mut normal = [0.0; 3];
        let mut count = 0.0;
        for &face in &incident[representative] {
            if exterior[face] {
                for (axis, value) in normal.iter_mut().enumerate() {
                    *value += normals[face][axis];
                }
                count += 1.0;
            }
        }
        if count > 0.0 {
            let length =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            if length > 0.0 {
                for value in &mut normal {
                    *value /= length;
                }
            }
        }
        minima.push(LocalZMinimum {
            position_mm: welded.verts()[representative],
            normal,
            region_points_mm: local.iter().map(|id| welded.verts()[*id]).collect(),
        });
    }
    Ok(minima)
}

fn partition_load(
    mesh: &Mesh,
    islands: &[SupportIsland],
    specialized: &[SpecializedOwnerReservation],
) -> Result<Vec<FaceLoadOwner>, GenericPerceptionError> {
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
        .filter_map(|(face_id, set)| {
            set.first()
                .copied()
                .map(|owner| FaceLoadOwner { face_id, owner })
        })
        .collect())
}

fn authority_fingerprint(
    mesh: &Mesh,
    surface: &NativeSurface,
    config: &SupportPerceptionConfig<'_>,
    layers: &LayerPlan,
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
    let mut frame = Fnv::new();
    frame.f64(config.plate_z_mm.get());
    let mut layer_hash = Fnv::new();
    layer_hash.f64(layers.z0_mm());
    for value in layers.heights_mm() {
        layer_hash.f64(*value);
    }
    Ok(AuthorityFingerprint {
        mesh,
        native: native.finish(),
        config: result_config.finish(),
        frame: frame.finish(),
        layers: layer_hash.finish(),
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
    fn frozen_loop_birth_and_hole_vectors_are_not_self_generated() {
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
        let moved = SliceLoop {
            points_xy_mm: square(10.0, 0.0, 11.0, 1.0),
            signed_area_mm2: 1.0,
            hole: false,
        };
        let births = find_births(
            &[0, 1],
            &[0.025, 0.075],
            &[vec![outer.clone(), hole], vec![outer, moved]],
        );
        assert_eq!(births.len(), 2);
        assert_eq!(births[0].centroid_xy_mm, [2.0, 2.0]);
        assert_eq!(births[1].centroid_xy_mm, [10.5, 0.5]);
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
            loops[0].points_xy_mm(),
            &[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]
        );
        assert_eq!(loops[0].signed_area_mm2(), 4.0);
        assert!(!loops[0].is_hole());
    }

    #[test]
    fn unique_face_partition_reserves_specialized_owner_without_double_counting() {
        let mesh = plinth_mesh::make_box_default().unwrap();
        let rows = partition_load(
            &mesh,
            &[
                SupportIsland {
                    id: 4,
                    face_ids: vec![0, 1],
                },
                SupportIsland {
                    id: 8,
                    face_ids: vec![1, 2],
                },
            ],
            &[SpecializedOwnerReservation {
                owner_id: 2,
                face_id: 1,
            }],
        )
        .unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].owner, LoadOwner::Specialized(2));
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
                &layer_plan(),
                &candidate_config(),
                &[],
                GenericPerceptionLimits::production(),
            )
            .expect("required semantic fixture perception");
            plan.validate_binding(&mesh, &perception_config(&delicacy), &layer_plan())
                .unwrap();
            assert_ne!(plan.digest(), 0);
        }
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
            &layers,
            &candidate_config(),
            &[],
            GenericPerceptionLimits::production(),
        )
        .expect("Sphere perception");
        let other = plinth_mesh::make_box_default().unwrap();
        let other_delicacy = vec![0.0; other.tris().len()];
        assert_eq!(
            plan.validate_binding(&other, &perception_config(&other_delicacy), &layers),
            Err(GenericPerceptionError::StaleAuthority)
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
                    if threads > 1 {
                        assert!(values[2] > 1, "parallel slice participation was vacuous");
                    }
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
