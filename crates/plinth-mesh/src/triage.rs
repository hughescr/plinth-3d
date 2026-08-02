//! Deliberate component triage before hollowing.
//!
//! This is not a repair pass.  It only removes tightly qualified modelling
//! shards and refuses a closed shell proved to be inside another shell's
//! material.  The latter is the safe response to a pre-hollowed model: carving
//! it again would destroy its wall and emit trapped resin.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use plinth_core::{m0_constants, M0ConstantsError};
use thiserror::Error;

use crate::topology::{TopologyError, WeldedView};
use crate::{Mesh, MeshError, WeldPolicy};

/// Typed triage settings read from the sole atomic M0 constants publication.
/// They remain diagnostic-only Placeholder values; this stage uses them to
/// classify input, never to claim accept-safe calibration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TriageSettings {
    shard_max_faces: usize,
    shard_max_dim_mm: f64,
    shard_max_volume_mm3: f64,
    containment_samples: usize,
    containment_min_fraction: f64,
}

impl TriageSettings {
    fn load() -> Result<Self, TriageError> {
        let constants = m0_constants().map_err(TriageError::Constants)?;
        let placeholders = constants.placeholders();
        Ok(Self {
            shard_max_faces: usize::try_from(
                placeholders.hollow_shard_max_faces().diagnostic().get(),
            )
            .map_err(|_| TriageError::ConstantCountOverflow {
                id: "HOLLOW_SHARD_MAX_FACES",
            })?,
            shard_max_dim_mm: placeholders.hollow_shard_max_dim_mm().diagnostic().get(),
            shard_max_volume_mm3: placeholders
                .hollow_shard_max_volume_mm3()
                .diagnostic()
                .get(),
            containment_samples: usize::try_from(
                placeholders.hollow_containment_samples().diagnostic().get(),
            )
            .map_err(|_| TriageError::ConstantCountOverflow {
                id: "HOLLOW_CONTAINMENT_SAMPLES",
            })?,
            containment_min_fraction: placeholders
                .hollow_containment_min_fraction()
                .diagnostic()
                .get(),
        })
    }

    #[must_use]
    pub const fn shard_max_faces(self) -> usize {
        self.shard_max_faces
    }
    #[must_use]
    pub const fn shard_max_dim_mm(self) -> f64 {
        self.shard_max_dim_mm
    }
    #[must_use]
    pub const fn shard_max_volume_mm3(self) -> f64 {
        self.shard_max_volume_mm3
    }
    #[must_use]
    pub const fn containment_samples(self) -> usize {
        self.containment_samples
    }
    #[must_use]
    pub const fn containment_min_fraction(self) -> f64 {
        self.containment_min_fraction
    }
}

pub fn triage_settings() -> Result<TriageSettings, TriageError> {
    TriageSettings::load()
}

/// Resource limits for the allocation-free raw-mesh triage admission check.
///
/// These are limits on named inputs and bounded triage working slots, not a
/// claim about process-wide heap consumption.  A caller selects the limits at
/// its resource boundary before starting any topology or containment work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TriageTopologyLimits {
    raw_vertices: usize,
    raw_faces: usize,
    retained_source_buffer_bytes: usize,
    edge_occurrences: usize,
    vertex_incidences: usize,
    containment_probe_slots: usize,
}

impl TriageTopologyLimits {
    #[must_use]
    pub const fn new(
        raw_vertices: usize,
        raw_faces: usize,
        retained_source_buffer_bytes: usize,
        edge_occurrences: usize,
        vertex_incidences: usize,
        containment_probe_slots: usize,
    ) -> Self {
        Self {
            raw_vertices,
            raw_faces,
            retained_source_buffer_bytes,
            edge_occurrences,
            vertex_incidences,
            containment_probe_slots,
        }
    }
}

/// Allocation-free evidence that a raw mesh is small enough to enter triage.
///
/// It deliberately records source buffer capacity exactly.  It does not turn
/// that number into a whole-process heap claim: later topology structures and
/// allocator overhead are owned and bounded at their respective boundaries.
#[derive(Debug, PartialEq)]
pub struct TriageTopologyAdmission<'mesh> {
    mesh: &'mesh Mesh,
    settings: TriageSettings,
    raw_vertices: usize,
    raw_faces: usize,
    retained_source_buffer_bytes: usize,
    edge_occurrences: usize,
    vertex_incidences: usize,
    containment_probe_slots: usize,
    raw_bounds: RawMeshBounds,
}

impl<'mesh> TriageTopologyAdmission<'mesh> {
    /// Measures and admits raw source inputs without allocating.  The
    /// containment bound uses the worst case of one component per face and
    /// therefore reserves `faces * samples` slots for a single batched query.
    pub fn from_raw_mesh(
        mesh: &'mesh Mesh,
        limits: TriageTopologyLimits,
    ) -> Result<Self, TriageAdmissionError> {
        // Admission and execution share this exact settings value.  Callers
        // cannot substitute a smaller sample count after the bound is proven.
        let settings = TriageSettings::load().map_err(|error| match error {
            TriageError::Constants(source) => TriageAdmissionError::Constants(source),
            TriageError::ConstantCountOverflow { id } => {
                TriageAdmissionError::ConstantCountOverflow { id }
            }
            _ => unreachable!("loading settings has no mesh or topology inputs"),
        })?;
        let raw_vertices = mesh.verts().len();
        let raw_faces = mesh.tris().len();
        let retained_source_buffer_bytes =
            mesh.retained_buffer_bytes()
                .ok_or(TriageAdmissionError::ArithmeticOverflow {
                    field: "retained source buffer bytes",
                })?;
        let (edge_occurrences, vertex_incidences, containment_probe_slots) =
            triage_work_counts(raw_faces, settings.containment_samples())?;

        enforce_limit("raw vertices", raw_vertices, limits.raw_vertices)?;
        enforce_limit("raw faces", raw_faces, limits.raw_faces)?;
        enforce_limit(
            "retained source buffer bytes",
            retained_source_buffer_bytes,
            limits.retained_source_buffer_bytes,
        )?;
        enforce_limit(
            "edge occurrences",
            edge_occurrences,
            limits.edge_occurrences,
        )?;
        enforce_limit(
            "vertex incidences",
            vertex_incidences,
            limits.vertex_incidences,
        )?;
        enforce_limit(
            "containment probe slots",
            containment_probe_slots,
            limits.containment_probe_slots,
        )?;

        Ok(Self {
            mesh,
            settings,
            raw_vertices,
            raw_faces,
            retained_source_buffer_bytes,
            edge_occurrences,
            vertex_incidences,
            containment_probe_slots,
            raw_bounds: RawMeshBounds::from_mesh(mesh),
        })
    }

    /// Proves the output fed to voxel-grid construction cannot enlarge the
    /// raw mesh's vertex/face inputs or its axis-aligned bounds.  It is only
    /// meaningful for the exact-bit triage path; epsilon welding is allowed
    /// to move vertices and must establish a separate downstream invariant.
    pub fn verify_exact_bit_triage_output(
        &self,
        triaged: &TriagedMesh,
    ) -> Result<(), TriageAdmissionError> {
        if !matches!(triaged.mesh().weld_policy(), WeldPolicy::ExactBits) {
            return Err(TriageAdmissionError::OutputNotExactBitWelded);
        }
        if triaged.mesh().verts().len() > self.raw_vertices {
            return Err(TriageAdmissionError::TriagedCountIncreased {
                field: "vertices",
                raw: self.raw_vertices,
                triaged: triaged.mesh().verts().len(),
            });
        }
        if triaged.mesh().tris().len() > self.raw_faces {
            return Err(TriageAdmissionError::TriagedCountIncreased {
                field: "faces",
                raw: self.raw_faces,
                triaged: triaged.mesh().tris().len(),
            });
        }
        if !self
            .raw_bounds
            .contains(RawMeshBounds::from_mesh(triaged.mesh()))
        {
            return Err(TriageAdmissionError::TriagedBoundsExpanded);
        }
        Ok(())
    }

    #[must_use]
    pub const fn raw_vertices(&self) -> usize {
        self.raw_vertices
    }
    #[must_use]
    pub const fn raw_faces(&self) -> usize {
        self.raw_faces
    }
    #[must_use]
    pub const fn retained_source_buffer_bytes(&self) -> usize {
        self.retained_source_buffer_bytes
    }
    #[must_use]
    pub const fn edge_occurrences(&self) -> usize {
        self.edge_occurrences
    }
    #[must_use]
    pub const fn vertex_incidences(&self) -> usize {
        self.vertex_incidences
    }
    #[must_use]
    pub const fn containment_probe_slots(&self) -> usize {
        self.containment_probe_slots
    }

    /// Consumes the admission capability and starts the allocation-bearing
    /// triage operation with the exact settings used for its resource bound.
    pub fn triage(self, weld_policy: WeldPolicy) -> Result<TriagedMesh, TriageError> {
        triage_components(self, weld_policy)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RawMeshBounds {
    low: [f64; 3],
    high: [f64; 3],
}

impl RawMeshBounds {
    fn from_mesh(mesh: &Mesh) -> Self {
        let mut low = [f64::INFINITY; 3];
        let mut high = [f64::NEG_INFINITY; 3];
        for vertex in mesh.verts() {
            for axis in 0..3 {
                low[axis] = low[axis].min(vertex[axis]);
                high[axis] = high[axis].max(vertex[axis]);
            }
        }
        Self { low, high }
    }

    fn contains(self, other: Self) -> bool {
        (0..3).all(|axis| self.low[axis] <= other.low[axis] && other.high[axis] <= self.high[axis])
    }
}

fn enforce_limit(
    field: &'static str,
    actual: usize,
    limit: usize,
) -> Result<(), TriageAdmissionError> {
    if actual > limit {
        return Err(TriageAdmissionError::LimitExceeded {
            field,
            actual,
            limit,
        });
    }
    Ok(())
}

fn triage_work_counts(
    raw_faces: usize,
    containment_samples: usize,
) -> Result<(usize, usize, usize), TriageAdmissionError> {
    let edge_occurrences =
        raw_faces
            .checked_mul(3)
            .ok_or(TriageAdmissionError::ArithmeticOverflow {
                field: "edge occurrences",
            })?;
    let vertex_incidences =
        raw_faces
            .checked_mul(3)
            .ok_or(TriageAdmissionError::ArithmeticOverflow {
                field: "vertex incidences",
            })?;
    let containment_probe_slots = raw_faces.checked_mul(containment_samples).ok_or(
        TriageAdmissionError::ArithmeticOverflow {
            field: "containment probe slots",
        },
    )?;
    Ok((edge_occurrences, vertex_incidences, containment_probe_slots))
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum TriageAdmissionError {
    #[error("M0 constants publication failed during triage admission: {0}")]
    Constants(&'static M0ConstantsError),
    #[error("M0 constant {id} does not fit this platform's usize during triage admission")]
    ConstantCountOverflow { id: &'static str },
    #[error("triage admission arithmetic overflowed while calculating {field}")]
    ArithmeticOverflow { field: &'static str },
    #[error("triage admission {field} {actual} exceeds limit {limit}")]
    LimitExceeded {
        field: &'static str,
        actual: usize,
        limit: usize,
    },
    #[error("triaged mesh was not exact-bit welded")]
    OutputNotExactBitWelded,
    #[error("triaged {field} count {triaged} exceeds raw count {raw}")]
    TriagedCountIncreased {
        field: &'static str,
        raw: usize,
        triaged: usize,
    },
    #[error("triaged mesh bounds exceed the raw voxel-grid input bounds")]
    TriagedBoundsExpanded,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreHollowed {
    cavity_components: Vec<ComponentDescription>,
}

impl PreHollowed {
    #[must_use]
    pub fn cavity_components(&self) -> &[ComponentDescription] {
        &self.cavity_components
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentDescription {
    component_id: usize,
    face_count: usize,
    dims_mm: [f64; 3],
}

impl ComponentDescription {
    #[must_use]
    pub const fn component_id(&self) -> usize {
        self.component_id
    }
    #[must_use]
    pub const fn face_count(&self) -> usize {
        self.face_count
    }
    #[must_use]
    pub const fn dims_mm(&self) -> [f64; 3] {
        self.dims_mm
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriageSummary {
    component_count: usize,
    main_component: usize,
    stripped_components: Vec<usize>,
}

impl TriageSummary {
    #[must_use]
    pub const fn component_count(&self) -> usize {
        self.component_count
    }
    #[must_use]
    pub const fn main_component(&self) -> usize {
        self.main_component
    }
    #[must_use]
    pub fn stripped_components(&self) -> &[usize] {
        &self.stripped_components
    }
}

/// A triage-stage mesh whose policy is truthful: its returned arrays are
/// really welded, unlike the source `Mesh` loaded from an STL.
#[derive(Clone, Debug, PartialEq)]
pub struct TriagedMesh {
    mesh: Mesh,
    summary: TriageSummary,
}

impl TriagedMesh {
    #[must_use]
    pub const fn mesh(&self) -> &Mesh {
        &self.mesh
    }
    #[must_use]
    pub const fn summary(&self) -> &TriageSummary {
        &self.summary
    }
    #[must_use]
    pub fn into_mesh(self) -> Mesh {
        self.mesh
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum TriageError {
    #[error(transparent)]
    Admission(#[from] TriageAdmissionError),
    #[error(transparent)]
    Topology(#[from] TopologyError),
    #[error(transparent)]
    Mesh(#[from] MeshError),
    #[error("M0 constants publication failed: {0}")]
    Constants(&'static M0ConstantsError),
    #[error("M0 constant {id} does not fit this platform's usize")]
    ConstantCountOverflow { id: &'static str },
    #[error("triage requires an actual welding policy, not WeldPolicy::None")]
    UnweldedInput,
    #[error("pre-hollowed input: {0:?}")]
    PreHollowed(PreHollowed),
    #[error("ray-parity arithmetic failed for containment")]
    RayArithmetic,
}

/// Weld, classify, strip only measured shards, and refuse sealed cavities
/// after consuming a successful allocation-free admission capability.
///
/// `WeldPolicy::ExactBits` mirrors the legacy `merge_vertices()` behavior;
/// callers may deliberately select an epsilon weld instead.  `None` is
/// rejected rather than quietly labelling an unwelded triangle soup as welded.
pub fn triage_components(
    admission: TriageTopologyAdmission<'_>,
    weld_policy: WeldPolicy,
) -> Result<TriagedMesh, TriageError> {
    let triaged = triage_components_admitted(admission.mesh, weld_policy, admission.settings)?;
    if matches!(weld_policy, WeldPolicy::ExactBits) {
        admission.verify_exact_bit_triage_output(&triaged)?;
    }
    Ok(triaged)
}

fn triage_components_admitted(
    mesh: &Mesh,
    weld_policy: WeldPolicy,
    settings: TriageSettings,
) -> Result<TriagedMesh, TriageError> {
    if matches!(weld_policy, WeldPolicy::None) {
        return Err(TriageError::UnweldedInput);
    }
    mark_triage_work_started();
    let view = WeldedView::build(mesh, weld_policy)?;
    let faces = remap_faces(mesh, &view.map);
    let components = face_components(&faces);
    let component_count = components
        .iter()
        .copied()
        .max()
        .map_or(0, |value| value + 1);
    debug_assert!(component_count > 0, "Mesh rejects empty triangle arrays");
    let details = ComponentDetail::collect(&view.verts, &faces, &components, component_count);
    let main = details
        .iter()
        .enumerate()
        .max_by(|(left_id, left), (right_id, right)| {
            left.diagonal()
                .total_cmp(&right.diagonal())
                .then_with(|| left.face_ids.len().cmp(&right.face_ids.len()))
                .then_with(|| left_id.cmp(right_id))
        })
        .map(|(id, _)| id)
        .expect("nonempty mesh has a component");

    let foreign_edges = components_with_foreign_edge(&faces, &components);
    let main_vertices = details[main].vertices.clone();
    let mut shards = BTreeSet::new();
    for (component, detail) in details.iter().enumerate() {
        if component == main {
            continue;
        }
        if !foreign_edges.contains(&component)
            && detail.touches_any(&main_vertices)
            && detail.inside_bbox(&details[main])
            && detail.face_ids.len() <= settings.shard_max_faces()
            && detail.max_dim() <= settings.shard_max_dim_mm()
            && detail.is_watertight(&faces)
            && detail.signed_volume(&view.verts, &faces).abs() <= settings.shard_max_volume_mm3()
        {
            shards.insert(component);
        }
    }

    let remaining = (0..component_count)
        .filter(|component| !shards.contains(component))
        .collect::<Vec<_>>();
    let closed = remaining
        .iter()
        .copied()
        .filter(|component| *component != main && details[*component].is_watertight(&faces))
        .collect::<Vec<_>>();
    let mut cavities = BTreeSet::new();
    // Gather all candidate boundary probes for one container before running a
    // single batched parity query, then scatter the boolean answers back.
    for container in remaining
        .iter()
        .copied()
        .filter(|component| details[*component].is_watertight(&faces))
    {
        let candidates = closed
            .iter()
            .copied()
            .filter(|candidate| {
                *candidate != container && details[*candidate].inside_bbox(&details[container])
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            continue;
        }
        let mut gathered = Vec::new();
        let mut ranges = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let start = gathered.len();
            gathered.extend(
                details[candidate].sample_points(&view.verts, settings.containment_samples()),
            );
            ranges.push((candidate, start, gathered.len()));
        }
        let classifier =
            ContainmentClassifier::build(&view.verts, &faces, &details[container].face_ids)?;
        let inside = classifier.classify_batch(&gathered)?;
        for (candidate, start, end) in ranges {
            let samples = end - start;
            if samples > 0
                && inside[start..end]
                    .iter()
                    .filter(|value| **value == ContainmentClass::Inside)
                    .count() as f64
                    / samples as f64
                    >= settings.containment_min_fraction()
            {
                cavities.insert(candidate);
            }
        }
    }
    if !cavities.is_empty() {
        let descriptions = cavities
            .into_iter()
            .map(|component| ComponentDescription {
                component_id: component,
                face_count: details[component].face_ids.len(),
                dims_mm: details[component].dims(),
            })
            .collect();
        return Err(TriageError::PreHollowed(PreHollowed {
            cavity_components: descriptions,
        }));
    }

    let kept_faces = faces
        .iter()
        .enumerate()
        .filter_map(|(face, triangle)| (!shards.contains(&components[face])).then_some(*triangle))
        .collect::<Vec<_>>();
    let compacted = compact_mesh(&view.verts, &kept_faces, weld_policy)?;
    Ok(TriagedMesh {
        mesh: compacted,
        summary: TriageSummary {
            component_count,
            main_component: main,
            stripped_components: shards.into_iter().collect(),
        },
    })
}

#[cfg(test)]
static TRIAGE_WORK_STARTED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[inline]
fn mark_triage_work_started() {
    #[cfg(test)]
    TRIAGE_WORK_STARTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

fn remap_faces(mesh: &Mesh, map: &[u32]) -> Vec<[u32; 3]> {
    mesh.tris()
        .iter()
        .map(|triangle| {
            [
                map[triangle[0] as usize],
                map[triangle[1] as usize],
                map[triangle[2] as usize],
            ]
        })
        .collect()
}

fn face_components(faces: &[[u32; 3]]) -> Vec<usize> {
    let mut by_edge = BTreeMap::<(u32, u32), Vec<usize>>::new();
    for (face, triangle) in faces.iter().enumerate() {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            by_edge.entry((a.min(b), a.max(b))).or_default().push(face);
        }
    }
    let mut adjacency = vec![Vec::new(); faces.len()];
    for occurrences in by_edge.values() {
        if occurrences.len() == 2 && occurrences[0] != occurrences[1] {
            adjacency[occurrences[0]].push(occurrences[1]);
            adjacency[occurrences[1]].push(occurrences[0]);
        }
    }
    let mut labels = vec![usize::MAX; faces.len()];
    let mut next = 0;
    for start in 0..faces.len() {
        if labels[start] != usize::MAX {
            continue;
        }
        labels[start] = next;
        let mut queue = VecDeque::from([start]);
        while let Some(face) = queue.pop_front() {
            for &neighbor in &adjacency[face] {
                if labels[neighbor] == usize::MAX {
                    labels[neighbor] = next;
                    queue.push_back(neighbor);
                }
            }
        }
        next += 1;
    }
    labels
}

fn components_with_foreign_edge(faces: &[[u32; 3]], components: &[usize]) -> BTreeSet<usize> {
    let mut by_edge = BTreeMap::<(u32, u32), BTreeSet<usize>>::new();
    for (face, triangle) in faces.iter().enumerate() {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            by_edge
                .entry((a.min(b), a.max(b)))
                .or_default()
                .insert(components[face]);
        }
    }
    by_edge
        .into_values()
        .filter(|owners| owners.len() > 1)
        .flatten()
        .collect()
}

#[derive(Clone, Debug)]
struct ComponentDetail {
    face_ids: Vec<usize>,
    vertices: BTreeSet<u32>,
    lo: [f64; 3],
    hi: [f64; 3],
}

impl ComponentDetail {
    fn collect(
        verts: &[[f64; 3]],
        faces: &[[u32; 3]],
        labels: &[usize],
        count: usize,
    ) -> Vec<Self> {
        let mut result = (0..count)
            .map(|_| Self {
                face_ids: Vec::new(),
                vertices: BTreeSet::new(),
                lo: [f64::INFINITY; 3],
                hi: [f64::NEG_INFINITY; 3],
            })
            .collect::<Vec<_>>();
        for (face, triangle) in faces.iter().enumerate() {
            let detail = &mut result[labels[face]];
            detail.face_ids.push(face);
            for vertex in triangle {
                detail.vertices.insert(*vertex);
                for (axis, coordinate) in verts[*vertex as usize].iter().enumerate() {
                    detail.lo[axis] = detail.lo[axis].min(*coordinate);
                    detail.hi[axis] = detail.hi[axis].max(*coordinate);
                }
            }
        }
        result
    }
    fn dims(&self) -> [f64; 3] {
        [
            self.hi[0] - self.lo[0],
            self.hi[1] - self.lo[1],
            self.hi[2] - self.lo[2],
        ]
    }
    fn max_dim(&self) -> f64 {
        self.dims().into_iter().fold(0.0, f64::max)
    }
    fn diagonal(&self) -> f64 {
        self.dims()
            .into_iter()
            .map(|value| value * value)
            .sum::<f64>()
            .sqrt()
    }
    fn touches_any(&self, other: &BTreeSet<u32>) -> bool {
        self.vertices.iter().any(|vertex| other.contains(vertex))
    }
    fn inside_bbox(&self, other: &Self) -> bool {
        (0..3).all(|axis| {
            self.lo[axis] >= other.lo[axis] - 1e-9 && self.hi[axis] <= other.hi[axis] + 1e-9
        })
    }
    fn is_watertight(&self, faces: &[[u32; 3]]) -> bool {
        let mut counts = BTreeMap::<(u32, u32), usize>::new();
        for &face in &self.face_ids {
            let triangle = faces[face];
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                *counts.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        counts.values().all(|count| *count == 2)
    }
    fn signed_volume(&self, verts: &[[f64; 3]], faces: &[[u32; 3]]) -> f64 {
        self.face_ids
            .iter()
            .map(|face| {
                let [a, b, c] = faces[*face];
                dot(
                    verts[a as usize],
                    cross(verts[b as usize], verts[c as usize]),
                ) / 6.0
            })
            .sum()
    }
    fn sample_points(&self, verts: &[[f64; 3]], limit: usize) -> Vec<[f64; 3]> {
        let ordered = self.vertices.iter().copied().collect::<Vec<_>>();
        let count = ordered.len().min(limit);
        (0..count)
            .map(|sample| verts[ordered[sample * ordered.len() / count] as usize])
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContainmentClass {
    Inside,
    Outside,
    Ambiguous,
}

#[derive(Clone, Copy, Debug)]
struct ContainmentFace {
    vertices: [[f64; 3]; 3],
    lo: [f64; 3],
    hi: [f64; 3],
}

/// A container-specific, immutable classifier. It builds the triangle table
/// once, accepts only gathered batches, and scatters classifications in input
/// order. There is deliberately no public scalar query API.
struct ContainmentClassifier {
    faces: Vec<ContainmentFace>,
}

impl ContainmentClassifier {
    fn build(
        verts: &[[f64; 3]],
        faces: &[[u32; 3]],
        container_faces: &[usize],
    ) -> Result<Self, TriageError> {
        let mut table = Vec::with_capacity(container_faces.len());
        for &face in container_faces {
            let [a, b, c] = faces[face];
            let vertices = [verts[a as usize], verts[b as usize], verts[c as usize]];
            let mut lo = [f64::INFINITY; 3];
            let mut hi = [f64::NEG_INFINITY; 3];
            for point in &vertices {
                for axis in 0..3 {
                    lo[axis] = lo[axis].min(point[axis]);
                    hi[axis] = hi[axis].max(point[axis]);
                }
            }
            if !lo.iter().chain(hi.iter()).all(|value| value.is_finite()) {
                return Err(TriageError::RayArithmetic);
            }
            table.push(ContainmentFace { vertices, lo, hi });
        }
        Ok(Self { faces: table })
    }

    fn classify_batch(&self, points: &[[f64; 3]]) -> Result<Vec<ContainmentClass>, TriageError> {
        points.iter().map(|point| self.classify(*point)).collect()
    }

    fn classify(&self, point: [f64; 3]) -> Result<ContainmentClass, TriageError> {
        const DIRECTIONS: [[f64; 3]; 3] = [
            [1.0, 0.371_390_676_354_103_7, 0.137_503_523_749_935_02],
            [0.211_324_865_405_187_1, 1.0, 0.109_381_654_230_983_1],
            [0.127_412_352_941_232_4, 0.333_333_333_333_333_3, 1.0],
        ];
        let mut inside = 0;
        let mut outside = 0;
        for direction in DIRECTIONS {
            match self.classify_ray(point, direction)? {
                ContainmentClass::Inside => inside += 1,
                ContainmentClass::Outside => outside += 1,
                ContainmentClass::Ambiguous => {}
            }
        }
        if inside >= 2 {
            Ok(ContainmentClass::Inside)
        } else if outside >= 2 {
            Ok(ContainmentClass::Outside)
        } else {
            Ok(ContainmentClass::Ambiguous)
        }
    }

    fn classify_ray(
        &self,
        point: [f64; 3],
        direction: [f64; 3],
    ) -> Result<ContainmentClass, TriageError> {
        let mut hits = Vec::new();
        for face in &self.faces {
            if point_on_triangle(point, *face)? {
                return Ok(ContainmentClass::Ambiguous);
            }
            if !ray_can_reach_bbox(point, direction, face.lo, face.hi) {
                continue;
            }
            match ray_triangle_distance(
                point,
                direction,
                face.vertices[0],
                face.vertices[1],
                face.vertices[2],
            )? {
                RayHit::Hit(distance) => hits.push(distance),
                RayHit::Miss => {}
                RayHit::Ambiguous => return Ok(ContainmentClass::Ambiguous),
            }
        }
        hits.sort_by(f64::total_cmp);
        hits.dedup_by(|left, right| {
            (*left - *right).abs() <= 1e-9 * left.abs().max(right.abs()).max(1.0)
        });
        Ok(if hits.len() % 2 == 1 {
            ContainmentClass::Inside
        } else {
            ContainmentClass::Outside
        })
    }
}

fn ray_can_reach_bbox(origin: [f64; 3], direction: [f64; 3], lo: [f64; 3], hi: [f64; 3]) -> bool {
    (0..3).all(|axis| {
        if direction[axis] > 0.0 {
            hi[axis] >= origin[axis] - 1e-12
        } else if direction[axis] < 0.0 {
            lo[axis] <= origin[axis] + 1e-12
        } else {
            origin[axis] >= lo[axis] - 1e-12 && origin[axis] <= hi[axis] + 1e-12
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RayHit {
    Hit(f64),
    Miss,
    Ambiguous,
}

fn ray_triangle_distance(
    origin: [f64; 3],
    direction: [f64; 3],
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
) -> Result<RayHit, TriageError> {
    let edge_a = subtract(b, a);
    let edge_b = subtract(c, a);
    let p = cross(direction, edge_b);
    let determinant = dot(edge_a, p);
    if !determinant.is_finite() {
        return Err(TriageError::RayArithmetic);
    }
    if determinant.abs() < 1e-12 {
        return Ok(RayHit::Miss);
    }
    let inverse = determinant.recip();
    let offset = subtract(origin, a);
    let u = dot(offset, p) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return Ok(RayHit::Miss);
    }
    let q = cross(offset, edge_a);
    let v = dot(direction, q) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return Ok(RayHit::Miss);
    }
    let distance = dot(edge_b, q) * inverse;
    if !distance.is_finite() {
        return Err(TriageError::RayArithmetic);
    }
    if distance <= 1e-9 {
        return Ok(RayHit::Ambiguous);
    }
    const EDGE_EPS: f64 = 1e-10;
    if u <= EDGE_EPS || v <= EDGE_EPS || 1.0 - u - v <= EDGE_EPS {
        return Ok(RayHit::Ambiguous);
    }
    Ok(RayHit::Hit(distance))
}

fn point_on_triangle(point: [f64; 3], face: ContainmentFace) -> Result<bool, TriageError> {
    if (0..3).any(|axis| point[axis] < face.lo[axis] - 1e-10 || point[axis] > face.hi[axis] + 1e-10)
    {
        return Ok(false);
    }
    let edge_a = subtract(face.vertices[1], face.vertices[0]);
    let edge_b = subtract(face.vertices[2], face.vertices[0]);
    let normal = cross(edge_a, edge_b);
    let normal_length = normal[0].hypot(normal[1]).hypot(normal[2]);
    if !normal_length.is_finite() {
        return Err(TriageError::RayArithmetic);
    }
    if normal_length <= 1e-15 {
        return Ok(false);
    }
    let plane_distance = dot(subtract(point, face.vertices[0]), normal).abs() / normal_length;
    if plane_distance > 1e-10 {
        return Ok(false);
    }
    let p = subtract(point, face.vertices[0]);
    let dot_aa = dot(edge_a, edge_a);
    let dot_ab = dot(edge_a, edge_b);
    let dot_bb = dot(edge_b, edge_b);
    let dot_ap = dot(edge_a, p);
    let dot_bp = dot(edge_b, p);
    let determinant = dot_aa * dot_bb - dot_ab * dot_ab;
    if !determinant.is_finite() {
        return Err(TriageError::RayArithmetic);
    }
    if determinant.abs() <= 1e-20 {
        return Ok(false);
    }
    let u = (dot_bb * dot_ap - dot_ab * dot_bp) / determinant;
    let v = (dot_aa * dot_bp - dot_ab * dot_ap) / determinant;
    Ok(u >= -1e-10 && v >= -1e-10 && u + v <= 1.0 + 1e-10)
}

fn compact_mesh(
    verts: &[[f64; 3]],
    faces: &[[u32; 3]],
    policy: WeldPolicy,
) -> Result<Mesh, TriageError> {
    let mut map = BTreeMap::new();
    let mut compact = Vec::new();
    let mut output = Vec::with_capacity(faces.len());
    for triangle in faces {
        let mut remapped = [0_u32; 3];
        for (corner, vertex) in triangle.iter().enumerate() {
            let id = match map.get(vertex) {
                Some(id) => *id,
                None => {
                    let id = u32::try_from(compact.len())
                        .map_err(|_| TopologyError::VertexCountOverflow)?;
                    map.insert(*vertex, id);
                    compact.push(verts[*vertex as usize]);
                    id
                }
            };
            remapped[corner] = id;
        }
        output.push(remapped);
    }
    Ok(Mesh::new_welded(compact, output, policy)?)
}

fn subtract(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}
fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}
fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exact_limits(mesh: &Mesh) -> TriageTopologyLimits {
        let faces = mesh.tris().len();
        let samples = TriageSettings::load()
            .expect("settings")
            .containment_samples();
        TriageTopologyLimits::new(
            mesh.verts().len(),
            faces,
            mesh.retained_buffer_bytes().expect("small mesh"),
            faces * 3,
            faces * 3,
            faces * samples,
        )
    }

    #[test]
    fn cap_minus_one_refuses_before_real_triage_work_starts() {
        use std::sync::atomic::Ordering;

        let closed = crate::fixtures::make_box_default().expect("box");
        let open = Mesh::new(closed.verts().to_vec(), closed.tris()[1..].to_vec()).expect("open");
        let exact = exact_limits(&open);
        let rejected = TriageTopologyLimits::new(
            open.verts().len() - 1,
            exact.raw_faces,
            exact.retained_source_buffer_bytes,
            exact.edge_occurrences,
            exact.vertex_incidences,
            exact.containment_probe_slots,
        );
        let before = TRIAGE_WORK_STARTED.load(Ordering::Relaxed);
        assert!(matches!(
            TriageTopologyAdmission::from_raw_mesh(&open, rejected),
            Err(TriageAdmissionError::LimitExceeded {
                field: "raw vertices",
                ..
            })
        ));
        assert_eq!(
            TRIAGE_WORK_STARTED.load(Ordering::Relaxed),
            before,
            "refusal must precede the first allocation-bearing topology operation"
        );

        let admission = TriageTopologyAdmission::from_raw_mesh(&open, exact)
            .expect("the exact cap admits the actual path");
        triage_components(admission, WeldPolicy::ExactBits).expect("open soup remains triageable");
        assert_eq!(TRIAGE_WORK_STARTED.load(Ordering::Relaxed), before + 1);
    }

    #[test]
    fn raw_work_arithmetic_overflow_is_typed() {
        assert!(matches!(
            triage_work_counts(usize::MAX / 3 + 1, 1),
            Err(TriageAdmissionError::ArithmeticOverflow {
                field: "edge occurrences"
            })
        ));
        assert!(matches!(
            triage_work_counts(2, usize::MAX),
            Err(TriageAdmissionError::ArithmeticOverflow {
                field: "containment probe slots"
            })
        ));
    }

    #[test]
    fn classifier_builds_one_reused_face_table_and_classifies_a_gathered_batch() {
        let mesh = crate::fixtures::make_box_default().expect("box");
        let face_ids = (0..mesh.tris().len()).collect::<Vec<_>>();
        let classifier = ContainmentClassifier::build(mesh.verts(), mesh.tris(), &face_ids)
            .expect("one table build");
        assert_eq!(classifier.faces.len(), face_ids.len());
        assert_eq!(
            classifier
                .classify_batch(&[[0.0, 0.0, 5.0], [20.0, 0.0, 5.0], [5.0, 0.0, 5.0]])
                .expect("ordered batch"),
            [
                ContainmentClass::Inside,
                ContainmentClass::Outside,
                ContainmentClass::Ambiguous,
            ],
            "inside, outside, and boundary/ray-edge ambiguity never share a boolean path"
        );
    }

    #[test]
    fn sampling_more_than_the_authoritative_limit_is_bounded_and_deterministic() {
        let settings = TriageSettings::load().expect("M0 settings");
        let detail = ComponentDetail {
            face_ids: Vec::new(),
            vertices: (0..65_u32).collect(),
            lo: [0.0; 3],
            hi: [1.0; 3],
        };
        let points = (0..65)
            .map(|value| [value as f64, 0.0, 0.0])
            .collect::<Vec<_>>();
        let first = detail.sample_points(&points, settings.containment_samples());
        let second = detail.sample_points(&points, settings.containment_samples());
        assert_eq!(first.len(), settings.containment_samples());
        assert_eq!(first, second);
    }
}
