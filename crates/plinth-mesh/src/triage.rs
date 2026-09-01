//! Deliberate component triage before hollowing.
//!
//! This is not a repair pass.  It only removes tightly qualified modelling
//! shards and refuses a closed shell proved to be inside another shell's
//! material.  The latter is the safe response to a pre-hollowed model: carving
//! it again would destroy its wall and emit trapped resin.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;

use plinth_core::{m0_constants, M0ConstantsError};
use thiserror::Error;

use geom_kernel::vec3::{cross, dot, norm, sub};
use geom_kernel::{ray_triangle_classified, RayTriangleClass};

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

/// The absolute ceilings [`TriageTopologyLimits::production`] publishes.
///
/// They live here, in the crate that owns the admission check, because two
/// stages now admit raw input through it — the hollow stage's H0 boundary and
/// the repair pre-pass — and a second private copy of these numbers is a
/// second authority that will eventually disagree with the first.
const PRODUCTION_MAX_RAW_VERTICES: usize = 16 * 1024 * 1024;
const PRODUCTION_MAX_RAW_FACES: usize = 8 * 1024 * 1024;
const PRODUCTION_MAX_RETAINED_SOURCE_BYTES: usize = 512 * 1024 * 1024;
const PRODUCTION_MAX_EDGE_OCCURRENCES: usize = 24 * 1024 * 1024;
const PRODUCTION_MAX_VERTEX_INCIDENCES: usize = 24 * 1024 * 1024;
const PRODUCTION_MAX_CONTAINMENT_PROBE_SLOTS: usize = 256 * 1024 * 1024;

/// Resource limits for the raw-mesh triage admission check.
///
/// These are limits on named inputs and bounded triage working slots, not a
/// claim about process-wide heap consumption.  A caller selects the limits at
/// its resource boundary before starting input-proportional topology or
/// containment work.
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

    /// The absolute production ceiling for one raw mesh entering triage.
    ///
    /// **A cap that scales with its own charge is not a cap.** A caller that
    /// builds these limits out of the very mesh it is about to admit makes
    /// every comparison `n <= n`, so no budget can ever fire — the defect the
    /// workspace already found and fixed once for the orientation search's own
    /// work budget. These six numbers are therefore fixed, absolute, and
    /// independent of the input: they are the values `plinth-hollow`'s
    /// `H0AdmissionLimits::production()` has always admitted raw input with,
    /// stated once here so the hollow stage and the repair pre-pass cannot
    /// drift apart about how large an input is too large to open at all.
    ///
    /// They are resource exclusions and nothing else: they bound named input
    /// counts and bounded working slots, never a physics claim and never a
    /// verdict about geometry. Raising one is a resource decision with an
    /// owner, not a fix for a refused model.
    #[must_use]
    pub const fn production() -> Self {
        Self {
            raw_vertices: PRODUCTION_MAX_RAW_VERTICES,
            raw_faces: PRODUCTION_MAX_RAW_FACES,
            retained_source_buffer_bytes: PRODUCTION_MAX_RETAINED_SOURCE_BYTES,
            edge_occurrences: PRODUCTION_MAX_EDGE_OCCURRENCES,
            vertex_incidences: PRODUCTION_MAX_VERTEX_INCIDENCES,
            containment_probe_slots: PRODUCTION_MAX_CONTAINMENT_PROBE_SLOTS,
        }
    }

    #[must_use]
    pub const fn raw_vertices(self) -> usize {
        self.raw_vertices
    }

    #[must_use]
    pub const fn raw_faces(self) -> usize {
        self.raw_faces
    }

    #[must_use]
    pub const fn retained_source_buffer_bytes(self) -> usize {
        self.retained_source_buffer_bytes
    }

    #[must_use]
    pub const fn edge_occurrences(self) -> usize {
        self.edge_occurrences
    }

    #[must_use]
    pub const fn vertex_incidences(self) -> usize {
        self.vertex_incidences
    }

    #[must_use]
    pub const fn containment_probe_slots(self) -> usize {
        self.containment_probe_slots
    }
}

/// Evidence that a raw mesh is small enough to enter triage.
///
/// It deliberately records source buffer capacity exactly.  It does not turn
/// that number into a whole-process heap claim: later topology structures and
/// allocator overhead are owned and bounded at their respective boundaries.
/// Loading the authoritative constants registry may perform one-time global
/// initialization; admission guarantees only that refusal precedes any
/// input-proportional per-model triage/topology allocation or welded-view work.
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
    /// Measures and admits raw source inputs before input-proportional
    /// triage/topology allocation. The containment bound uses the worst case
    /// of one component per face and therefore reserves `faces * samples`
    /// slots for a single batched query. Authoritative registry initialization
    /// is allowed before this per-model boundary.
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
            retained_source_buffer_bytes(mesh.verts.capacity(), mesh.tris.capacity())?;
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

    /// Consumes the admission capability and starts input-proportional triage
    /// work with the exact settings used for its resource bound.
    pub fn triage(self, weld_policy: WeldPolicy) -> Result<TriagedMesh, TriageError> {
        triage_components(self, weld_policy)
    }

    /// The triage **verdict** alone: which components are shards and which
    /// source faces they own, without building the welded, compacted mesh.
    ///
    /// **Why this exists.** The repair pre-pass runs on every raw input the
    /// pipeline is handed, and it must not adopt a re-indexed mesh: its whole
    /// byte-identity contract is that surviving faces keep the caller's own
    /// numbering. It therefore reads [`TriageSummary::stripped_faces`] and
    /// deletes those rows from its own arrays — so the compacted `TriagedMesh`
    /// [`Self::triage`] returns was allocated at raw size (up to ~459k faces
    /// in the corpus) and dropped unread on every single call.
    ///
    /// **Why skipping [`Self::verify_exact_bit_triage_output`] is not a
    /// loosened check.** That postcondition proves the triaged *mesh* cannot
    /// enlarge the raw vertex/face counts or expand the raw bounds before it
    /// reaches voxel-grid construction. This entry point produces no mesh; it
    /// produces a set of source face indices, and the only operation a caller
    /// can perform with them is deletion. Both postconditions are then
    /// satisfied by construction — deleting rows cannot raise a count and
    /// cannot move a vertex, so no bound can expand — and there is no third
    /// artifact for the check to have covered. The admission limits, which are
    /// the actual cap, ran in [`Self::from_raw_mesh`] and are untouched. A
    /// caller that wants the compacted mesh must still go through
    /// [`Self::triage`] and its verification.
    pub fn triage_summary(self, weld_policy: WeldPolicy) -> Result<TriageSummary, TriageError> {
        let parts = triage_admitted(self.mesh, weld_policy, self.settings)?;
        debug_assert!(
            parts
                .summary
                .stripped_faces
                .last()
                .is_none_or(|face| *face < self.raw_faces),
            "triage named a face outside the admitted raw mesh"
        );
        Ok(parts.summary)
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

fn retained_source_buffer_bytes(
    vertex_capacity: usize,
    face_capacity: usize,
) -> Result<usize, TriageAdmissionError> {
    vertex_capacity
        .checked_mul(std::mem::size_of::<[f64; 3]>())
        .and_then(|vertices| {
            face_capacity
                .checked_mul(std::mem::size_of::<[u32; 3]>())
                .and_then(|faces| vertices.checked_add(faces))
        })
        .ok_or(TriageAdmissionError::ArithmeticOverflow {
            field: "retained source buffer bytes",
        })
}

fn triage_work_counts(
    raw_faces: usize,
    containment_samples: usize,
) -> Result<(usize, usize, usize), TriageAdmissionError> {
    let edge_occurrences = checked_product("edge occurrences", raw_faces, 3)?;
    let vertex_incidences = checked_product("vertex incidences", raw_faces, 3)?;
    let containment_probe_slots =
        checked_product("containment probe slots", raw_faces, containment_samples)?;
    Ok((edge_occurrences, vertex_incidences, containment_probe_slots))
}

fn checked_product(
    field: &'static str,
    left: usize,
    right: usize,
) -> Result<usize, TriageAdmissionError> {
    left.checked_mul(right)
        .ok_or(TriageAdmissionError::ArithmeticOverflow { field })
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
    stripped_faces: Vec<usize>,
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
    /// The **source** face indices the shard criteria removed, ascending.
    ///
    /// [`TriagedMesh`] hands back a welded, compacted mesh, which is the right
    /// answer for the hollow stage and the wrong one for a caller that must
    /// leave every surviving byte of its input alone. This is the same verdict
    /// expressed against the caller's own face numbering, so a repair can drop
    /// exactly these rows instead of adopting a re-indexed mesh — and so the
    /// shard criteria stay in this one place rather than being re-implemented
    /// by whoever needs source indices.
    #[must_use]
    pub fn stripped_faces(&self) -> &[usize] {
        &self.stripped_faces
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
/// after consuming a successful raw-input admission capability.
///
/// Acceptance-stage callers, including H0, must first derive
/// [`TriageTopologyLimits`] from their resource envelope and call
/// [`TriageTopologyAdmission::from_raw_mesh`]. There is no uncapped public
/// triage entry point; downstream integration must carry this capability to
/// this function before performing other input-proportional topology work.
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
    let parts = triage_admitted(mesh, weld_policy, settings)?;
    // `stripped_faces` is exactly the set the criteria removed, in this same
    // face numbering, so the kept rows are read off it rather than from a
    // second copy of the component test.
    let stripped = parts
        .summary
        .stripped_faces
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let kept_faces = parts
        .faces
        .iter()
        .enumerate()
        .filter_map(|(face, triangle)| (!stripped.contains(&face)).then_some(*triangle))
        .collect::<Vec<_>>();
    let compacted = compact_mesh(&parts.verts, &kept_faces, weld_policy)?;
    Ok(TriagedMesh {
        mesh: compacted,
        summary: parts.summary,
    })
}

/// The welded arrays and the shard verdict, before either is turned into a
/// mesh.
///
/// Splitting this out is what lets [`TriageTopologyAdmission::triage_summary`]
/// answer "which faces are dust" without allocating a compacted mesh the
/// caller is going to drop. There is exactly one implementation of the six
/// criteria and it is here; the two entry points differ only in what they do
/// with its answer.
struct TriageParts {
    verts: Vec<[f64; 3]>,
    faces: Vec<[u32; 3]>,
    summary: TriageSummary,
}

fn triage_admitted(
    mesh: &Mesh,
    weld_policy: WeldPolicy,
    settings: TriageSettings,
) -> Result<TriageParts, TriageError> {
    if matches!(weld_policy, WeldPolicy::None) {
        return Err(TriageError::UnweldedInput);
    }
    mark_input_proportional_triage_work_started();
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

    // `components` is indexed by source face, so the stripped rows are read off
    // in the caller's own numbering in one ascending pass -- no second pass and
    // no second copy of the criteria.
    let stripped_faces = (0..faces.len())
        .filter(|face| shards.contains(&components[*face]))
        .collect::<Vec<_>>();
    Ok(TriageParts {
        verts: view.verts,
        faces,
        summary: TriageSummary {
            component_count,
            main_component: main,
            stripped_components: shards.into_iter().collect(),
            stripped_faces,
        },
    })
}

#[cfg(test)]
static INPUT_PROPORTIONAL_TRIAGE_WORK_STARTED: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[inline]
fn mark_input_proportional_triage_work_started() {
    #[cfg(test)]
    INPUT_PROPORTIONAL_TRIAGE_WORK_STARTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

/// Counts of the containment ray caster's work, for evidence that a change to
/// the ray/triangle primitive actually exercised the path it claims to change.
///
/// This exists because of a recorded project failure mode: a byte-identity
/// proof over a model that never fired the changed code path. A verdict table
/// captured across `docs/specs/geometry-canon.md` §1 OP-2 (plan item C5) must
/// be able to say **how many face tests ran**, not only what the verdict was —
/// a corpus model with no nested closed component never builds a classifier at
/// all, and "the verdict did not move" then means nothing.
///
/// Counters are process-global, monotonically increasing, `Relaxed`, and
/// accumulated once per ray from ray-local counts, so the per-face inner loop
/// is untouched. They are instrumentation: nothing in this crate reads them to
/// make a decision, and they never influence a verdict.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContainmentRayCounters {
    /// `ContainmentClassifier::build` calls: one per candidate container.
    pub classifiers_built: u64,
    /// `classify_ray` calls: `containment_samples` x 3 directions per nested
    /// candidate component.
    pub rays_cast: u64,
    /// Ray/triangle tests that survived the per-face bounding-box filter.
    pub face_tests: u64,
    /// Ray/triangle tests that returned a forward crossing.
    pub face_hits: u64,
    /// Ray/triangle tests that returned no crossing.
    pub face_misses: u64,
    /// Ray/triangle tests the triage call-site policy could not sign: the
    /// crossing landed inside `TRIAGE_ORIGIN_EPS` of the query point.
    ///
    /// This bucket is DISJOINT from [`Self::face_near_parallel`]. The two are
    /// different uncertainties: this one means "a crossing exists and its sign
    /// is untrustworthy", the other means "whether a crossing exists at all is
    /// undecidable".
    pub face_ambiguous: u64,
    /// Ray/triangle tests the KERNEL could not decide: `0 < |det| <= DET_EPS`,
    /// [`geom_kernel::RayTriangleClass::NearParallel`].
    ///
    /// **This is the counter the near-parallel adjudication exists to produce**
    /// (`docs/specs/geometry-adjudication.md` §O4 Part 2, §EXT-1 row 1). Before
    /// the retarget, every one of these evaluations was reported as a definite
    /// Miss and silently dropped a real interior crossing out of the parity
    /// count, flipping the containment verdict for every point beyond it. They
    /// now poison their ray to `Ambiguous`, which the 2-of-3 vote excludes.
    ///
    /// A nonzero reading is the measured refusal rate the owner asked for, not
    /// a value to tune away: the §O4 fallback (escalating only above a sliver
    /// area threshold) would introduce a new deciding constant and needs a new
    /// ruling.
    pub face_near_parallel: u64,
    /// Rays short-circuited by the `point_on_triangle` boundary pre-check,
    /// which is triage policy and not part of the ray/triangle primitive.
    pub point_on_triangle_hits: u64,
}

static CLASSIFIERS_BUILT: AtomicU64 = AtomicU64::new(0);
static RAYS_CAST: AtomicU64 = AtomicU64::new(0);
static FACE_TESTS: AtomicU64 = AtomicU64::new(0);
static FACE_HITS: AtomicU64 = AtomicU64::new(0);
static FACE_MISSES: AtomicU64 = AtomicU64::new(0);
static FACE_AMBIGUOUS: AtomicU64 = AtomicU64::new(0);
static FACE_NEAR_PARALLEL: AtomicU64 = AtomicU64::new(0);
static POINT_ON_TRIANGLE_HITS: AtomicU64 = AtomicU64::new(0);

/// Read the process-global containment ray counters.
#[must_use]
pub fn containment_ray_counters() -> ContainmentRayCounters {
    ContainmentRayCounters {
        classifiers_built: CLASSIFIERS_BUILT.load(Ordering::Relaxed),
        rays_cast: RAYS_CAST.load(Ordering::Relaxed),
        face_tests: FACE_TESTS.load(Ordering::Relaxed),
        face_hits: FACE_HITS.load(Ordering::Relaxed),
        face_misses: FACE_MISSES.load(Ordering::Relaxed),
        face_ambiguous: FACE_AMBIGUOUS.load(Ordering::Relaxed),
        face_near_parallel: FACE_NEAR_PARALLEL.load(Ordering::Relaxed),
        point_on_triangle_hits: POINT_ON_TRIANGLE_HITS.load(Ordering::Relaxed),
    }
}

/// One ray's tallies, folded into the globals in a single pass at the end of
/// the ray so the per-face inner loop stays atomic-free.
#[derive(Default)]
struct RayTally {
    rays: u64,
    tests: u64,
    hits: u64,
    misses: u64,
    ambiguous: u64,
    near_parallel: u64,
    point_on_triangle: u64,
}

impl RayTally {
    fn publish(&self) {
        for (counter, value) in [
            (&RAYS_CAST, self.rays),
            (&FACE_TESTS, self.tests),
            (&FACE_HITS, self.hits),
            (&FACE_MISSES, self.misses),
            (&FACE_AMBIGUOUS, self.ambiguous),
            (&FACE_NEAR_PARALLEL, self.near_parallel),
            (&POINT_ON_TRIANGLE_HITS, self.point_on_triangle),
        ] {
            if value != 0 {
                counter.fetch_add(value, Ordering::Relaxed);
            }
        }
    }
}

/// Zero the process-global containment ray counters.
///
/// Process-global, so a caller that needs per-model figures must not run
/// models concurrently with each other or with anything else that triages.
pub fn reset_containment_ray_counters() {
    for counter in [
        &CLASSIFIERS_BUILT,
        &RAYS_CAST,
        &FACE_TESTS,
        &FACE_HITS,
        &FACE_MISSES,
        &FACE_AMBIGUOUS,
        &FACE_NEAR_PARALLEL,
        &POINT_ON_TRIANGLE_HITS,
    ] {
        counter.store(0, Ordering::Relaxed);
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

/// The three sampling ANGLES, as chosen. These literals are the record of the
/// directions this classifier has always sampled; they are deliberately not
/// unit length, and nothing casts a ray along them any more.
///
/// Their measured norms are 1.075_564_155_9, 1.027_921_468_3 and
/// 1.061_765_048_8 (`docs/specs/geometry-adjudication.md` §O4), which is the
/// whole problem: every quantity the kernel returns for a ray is in units of
/// `|direction|`, so `t`, the kernel's `±EPS` acceptance slack and
/// `TRIAGE_ORIGIN_EPS` were all in units of a number that is not a millimetre
/// and differs per direction — and the determinant compared against the
/// ABSOLUTE `DET_EPS` scaled linearly with `|direction|`, so the same geometric
/// ray could land on either side of the undecidable band depending only on how
/// its direction vector happened to be written down.
const RAW_DIRECTIONS: [[f64; 3]; 3] = [
    [1.0, 0.371_390_676_354_103_7, 0.137_503_523_749_935_02],
    [0.211_324_865_405_187_1, 1.0, 0.109_381_654_230_983_1],
    [0.127_412_352_941_232_4, 0.333_333_333_333_333_3, 1.0],
];

/// [`RAW_DIRECTIONS`], each divided componentwise by its own canonical norm
/// (`geom_kernel::vec3::norm`, OP-6) — the vectors actually cast
/// (`docs/specs/geometry-adjudication.md` §O4 Part 2 item 1).
///
/// After this the units are honest: `t` IS millimetres, `TRIAGE_ORIGIN_EPS`
/// IS 1e-9 mm, the kernel's t-slack IS 1e-6 mm, and the determinant is
/// `2A·cos θ` with no direction-scaling term, so the hit/miss boundary no
/// longer depends on the representation of the direction.
///
/// Componentwise scalar division, not a consolidated vector operation: this is
/// three divisions written out, and it trips no inline-geometry guard pattern.
static SAMPLING_DIRECTIONS: LazyLock<[[f64; 3]; 3]> = LazyLock::new(|| {
    RAW_DIRECTIONS.map(|raw| {
        let length = norm(raw);
        [raw[0] / length, raw[1] / length, raw[2] / length]
    })
});

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
        CLASSIFIERS_BUILT.fetch_add(1, Ordering::Relaxed);
        Ok(Self { faces: table })
    }

    fn classify_batch(&self, points: &[[f64; 3]]) -> Result<Vec<ContainmentClass>, TriageError> {
        points.iter().map(|point| self.classify(*point)).collect()
    }

    fn classify(&self, point: [f64; 3]) -> Result<ContainmentClass, TriageError> {
        let mut inside = 0;
        let mut outside = 0;
        for direction in *SAMPLING_DIRECTIONS {
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
        // Ray-local tallies, folded into the process-global instrumentation
        // once per ray so the per-face loop below carries no atomic.
        let mut tally = RayTally::default();
        let mut hits = Vec::new();
        let outcome = self.classify_ray_counted(point, direction, &mut tally, &mut hits);
        tally.publish();
        outcome
    }

    fn classify_ray_counted(
        &self,
        point: [f64; 3],
        direction: [f64; 3],
        tally: &mut RayTally,
        hits: &mut Vec<f64>,
    ) -> Result<ContainmentClass, TriageError> {
        tally.rays = 1;
        for face in &self.faces {
            if point_on_triangle(point, *face)? {
                tally.point_on_triangle = 1;
                return Ok(ContainmentClass::Ambiguous);
            }
            if !ray_can_reach_bbox(point, direction, face.lo, face.hi) {
                continue;
            }
            tally.tests += 1;
            match ray_triangle_distance(
                point,
                direction,
                face.vertices[0],
                face.vertices[1],
                face.vertices[2],
            )? {
                RayHit::Hit(distance) => {
                    tally.hits += 1;
                    hits.push(distance);
                }
                RayHit::Miss => tally.misses += 1,
                RayHit::Ambiguous => {
                    tally.ambiguous += 1;
                    return Ok(ContainmentClass::Ambiguous);
                }
                RayHit::NearParallel => {
                    // Refuse rather than observe a missing crossing. The
                    // kernel could not decide whether this ray crosses this
                    // face; counting it either way would be a guess, and a
                    // guess here flips the containment verdict for every point
                    // beyond the face. The ray is excluded from the 2-of-3
                    // vote instead.
                    tally.near_parallel += 1;
                    return Ok(ContainmentClass::Ambiguous);
                }
            }
        }
        hits.sort_by(f64::total_cmp);
        // GEOMETRY-CANON EXEMPT: docs/specs/geometry-canon.md §1 OP-2 and §4
        // item 1 of §5's flagged uncertainties. This RELATIVE 1e-9 crossing
        // dedup deliberately diverges from the kernel's ABSOLUTE
        // `DEDUP_LOC_MM = 3e-3`, which belongs to the parity engine (OP-5) and
        // is explicitly out of scope for this sweep. Both merge the same-`t`
        // duplicate hits the kernel's shared-edge slack produces, so C5 is
        // safe under either; they differ only on walls thinner than 3 µm,
        // which is an OP-5 question. Marked so the next census does not
        // re-discover it as an unexamined divergence.
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
    /// A crossing exists and its SIGN is untrustworthy: it landed inside
    /// `TRIAGE_ORIGIN_EPS` of the query point. Triage call-site policy.
    Ambiguous,
    /// Whether a crossing exists AT ALL is undecidable: the kernel classified
    /// the determinant into `0 < |det| <= DET_EPS`
    /// ([`RayTriangleClass::NearParallel`]).
    ///
    /// Kept distinct from [`Self::Ambiguous`] because the two are different
    /// uncertainties and are counted separately
    /// (`ContainmentRayCounters::face_ambiguous` vs `face_near_parallel`).
    /// Both poison the ray, so the containment outcome is the same; only the
    /// evidence differs.
    NearParallel,
}

/// Triage's ray/triangle CALL-SITE POLICY, over the canonical geometry.
///
/// The geometry is `geom_kernel::moller::ray_triangle_classified`
/// (`docs/specs/geometry-canon.md` §1 OP-2, plan item C5). This module's own
/// Möller-Trumbore copy is retired: it agreed with the kernel on the algorithm
/// and disagreed with it categorically on four acceptance rules, so the same
/// ray against the same triangle could be a kernel hit and a triage miss —
/// and a shared-edge crossing could miss BOTH adjacent faces, silently
/// flipping parity for every point beyond it.
///
/// **What is geometry and lives in the kernel:** the determinant band
/// (`DET_EPS = 1e-14`), the barycentric acceptance region with its
/// `EPS = 1e-6` shared-edge slack, and the ray parameter `t`.
///
/// **What is triage policy and lives here:**
/// * the disposition of the kernel's undecidable class. `DET_EPS` is NOT a
///   verdict for an interior crossing (`docs/specs/geometry-adjudication.md`
///   §O4): a ray nearly parallel to a face's plane, with its origin close to
///   that plane, pierces the face INTERIOR at a real finite `t` that no
///   adjacent face contains. The pinned `ray_triangle` calls that a Miss, the
///   crossing vanishes from the parity count, and the verdict flips for every
///   point beyond it. `ray_triangle_classified` surfaces the band instead, and
///   this call site maps it to [`RayHit::NearParallel`] — uncertainty routes to
///   an ambiguous ray, which the 2-of-3 vote excludes. No new constant is
///   introduced.
/// * `TRIAGE_ORIGIN_EPS` — a crossing whose distance from the query point is
///   too small to sign. Canon §1 OP-2 keeps this explicitly, as call-site
///   policy rather than a geometry rule.
/// * half-ray semantics — a crossing behind the query point is not a crossing.
///   This is parity policy, not geometry: the classifier counts forward
///   crossings only, and FU-43 (commit ee58444) is the record of what happens
///   when a real behind-origin hit is allowed to poison a ray.
/// * the non-finite refusal — the kernel propagates non-finite input by
///   contract (canon §1 OP-2), so `TriageError::RayArithmetic` is raised here.
///
/// **Deleted with no replacement:** the old `EDGE_EPS = 1e-10` band, which
/// mapped a hit within 1e-10 barycentric of an edge to `Ambiguous` and
/// poisoned the whole component. That is exactly the shared-edge case the
/// kernel's slack now answers correctly; keeping it would re-refuse what the
/// slack just fixed (canon §1 OP-2, "EDGE_EPS dies with no replacement").
fn ray_triangle_distance(
    origin: [f64; 3],
    direction: [f64; 3],
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
) -> Result<RayHit, TriageError> {
    // A6 RECONSTRUCTION, declared as the kernel's contract requires: this
    // classifier holds vertices, not stored edges, so it builds `e1`/`e2` with
    // `geom_kernel::vec3::sub` — the same subtraction the kernel's own surface
    // build uses.
    let e1 = sub(b, a);
    let e2 = sub(c, a);
    // `t_max = INFINITY`: a parity ray is unbounded forward.
    //
    // UNITS: `direction` is one of `SAMPLING_DIRECTIONS`, each of which is
    // `RAW_DIRECTIONS`'s chosen angle divided by its own canonical norm, so
    // `|direction| = 1` and `t` is MILLIMETRES. The kernel's ±`EPS` acceptance
    // slack is therefore 1e-6 mm and `TRIAGE_ORIGIN_EPS` below is genuinely
    // 1e-9 mm. This is a change from the pre-normalization code, where all
    // three were in units of `|direction|` (1.0756, 1.0279, 1.0618) and
    // differed per direction.
    //
    // The classified call is the decision-feeding form: the pinned
    // `ray_triangle` folds the undecidable determinant band into `hit = false`
    // for oracle parity, and this call site decides a triage verdict.
    let class = ray_triangle_classified(origin, direction, a, e1, e2, f64::INFINITY);
    let hit = match class {
        RayTriangleClass::Hit(hit) => hit,
        // A definite miss, either because the determinant is well clear of the
        // band or because the ray provably does not reach this triangle at all.
        // Both are decided answers; see `RayTriangleClass::Miss`.
        RayTriangleClass::Miss => return Ok(RayHit::Miss),
        // `det == 0.0` exactly. A genuine prune, per the kernel's corrected
        // degenerate-input contract: no interior crossing exists, and the
        // crossings a ray exactly in this plane does make are edge crossings,
        // registered on the adjacent non-parallel faces.
        RayTriangleClass::ExactParallel => return Ok(RayHit::Miss),
        // `0 < |det| <= DET_EPS`. Undecidable at this scale, so this ray
        // cannot contribute a trustworthy parity bit.
        RayTriangleClass::NearParallel => return Ok(RayHit::NearParallel),
    };
    if !hit.t.is_finite() {
        return Err(TriageError::RayArithmetic);
    }
    // Triage policy, not a geometry rule: a crossing this close to the query
    // point has an untrustworthy sign, so the parity it would contribute is
    // untrustworthy too. Millimetres (see the units note above).
    const TRIAGE_ORIGIN_EPS: f64 = 1e-9;
    if hit.t.abs() <= TRIAGE_ORIGIN_EPS {
        return Ok(RayHit::Ambiguous);
    }
    if hit.t < 0.0 {
        // Half-ray semantics. The kernel accepts `t >= -EPS` so that a
        // crossing sitting exactly at the origin registers; triage's own
        // `TRIAGE_ORIGIN_EPS` band above already covers that case, and covers
        // it more conservatively (1e-9 mm against the kernel's 1e-6 mm
        // slack), so everything reaching here is a genuine behind-the-origin
        // crossing and is not counted.
        return Ok(RayHit::Miss);
    }
    Ok(RayHit::Hit(hit.t))
}

#[allow(
    clippy::suspicious_operation_groupings,
    reason = "`dot_aa * dot_bb - dot_ab * dot_ab` is the determinant of the \
              2x2 Gram matrix of the triangle's two edge vectors, not a \
              mis-typed `dot_aa * dot_ab`"
)]
fn point_on_triangle(point: [f64; 3], face: ContainmentFace) -> Result<bool, TriageError> {
    if (0..3).any(|axis| point[axis] < face.lo[axis] - 1e-10 || point[axis] > face.hi[axis] + 1e-10)
    {
        return Ok(false);
    }
    let edge_a = sub(face.vertices[1], face.vertices[0]);
    let edge_b = sub(face.vertices[2], face.vertices[0]);
    let normal = cross(edge_a, edge_b);
    // Canonical length: `geom_kernel::vec3::norm` (`dot(n, n).sqrt()`).
    // docs/specs/geometry-canon.md §1 OP-6 retires the `hypot` chain that was
    // here — one formula project-wide, not two that differ by 2 ULP on 20 % of
    // values. The chain's only real advantage is range beyond ~1e±150 mm,
    // unreachable on a 218 mm plate; the one input class that now answers
    // differently is a cross product whose squared magnitude overflows, which
    // the `is_finite` refusal below already handles the same way.
    let normal_length = norm(normal);
    if !normal_length.is_finite() {
        return Err(TriageError::RayArithmetic);
    }
    if normal_length <= 1e-15 {
        return Ok(false);
    }
    let plane_distance = dot(sub(point, face.vertices[0]), normal).abs() / normal_length;
    if plane_distance > 1e-10 {
        return Ok(false);
    }
    let p = sub(point, face.vertices[0]);
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

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard, PoisonError};

    use super::*;

    /// Serializes the tests that READ a process-global counter against the
    /// tests that INCREMENT one.
    ///
    /// `INPUT_PROPORTIONAL_TRIAGE_WORK_STARTED` and the containment ray
    /// counters are process-global by design — they measure the whole
    /// process's work — so a test asserting `before` / `before + 1` around its
    /// own call races every concurrently scheduled test that triages or builds
    /// a classifier. This is a test-isolation defect, not a claim about the
    /// code under test: measured at 1 failure in 6 parallel runs of this
    /// module, passing 6 of 6 under `--test-threads=1`. Every test that
    /// touches either counter takes this guard.
    static GLOBAL_COUNTER_TESTS: Mutex<()> = Mutex::new(());

    fn global_counter_guard() -> MutexGuard<'static, ()> {
        GLOBAL_COUNTER_TESTS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

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
    fn every_cap_minus_one_refuses_before_input_proportional_triage_work_starts() {
        let _guard = global_counter_guard();
        use std::sync::atomic::Ordering;

        let closed = crate::fixtures::make_box_default().expect("box");
        let open = Mesh::new(closed.verts().to_vec(), closed.tris()[1..].to_vec()).expect("open");
        let exact = exact_limits(&open);
        let cases = [
            (
                "raw vertices",
                TriageTopologyLimits::new(
                    exact.raw_vertices - 1,
                    usize::MAX,
                    usize::MAX,
                    usize::MAX,
                    usize::MAX,
                    usize::MAX,
                ),
            ),
            (
                "raw faces",
                TriageTopologyLimits::new(
                    usize::MAX,
                    exact.raw_faces - 1,
                    usize::MAX,
                    usize::MAX,
                    usize::MAX,
                    usize::MAX,
                ),
            ),
            (
                "retained source buffer bytes",
                TriageTopologyLimits::new(
                    usize::MAX,
                    usize::MAX,
                    exact.retained_source_buffer_bytes - 1,
                    usize::MAX,
                    usize::MAX,
                    usize::MAX,
                ),
            ),
            (
                "edge occurrences",
                TriageTopologyLimits::new(
                    usize::MAX,
                    usize::MAX,
                    usize::MAX,
                    exact.edge_occurrences - 1,
                    usize::MAX,
                    usize::MAX,
                ),
            ),
            (
                "vertex incidences",
                TriageTopologyLimits::new(
                    usize::MAX,
                    usize::MAX,
                    usize::MAX,
                    usize::MAX,
                    exact.vertex_incidences - 1,
                    usize::MAX,
                ),
            ),
            (
                "containment probe slots",
                TriageTopologyLimits::new(
                    usize::MAX,
                    usize::MAX,
                    usize::MAX,
                    usize::MAX,
                    usize::MAX,
                    exact.containment_probe_slots - 1,
                ),
            ),
        ];
        let before = INPUT_PROPORTIONAL_TRIAGE_WORK_STARTED.load(Ordering::Relaxed);
        for (field, rejected) in cases {
            assert!(matches!(
                TriageTopologyAdmission::from_raw_mesh(&open, rejected),
                Err(TriageAdmissionError::LimitExceeded {
                    field: actual,
                    ..
                }) if actual == field
            ));
            assert_eq!(
                INPUT_PROPORTIONAL_TRIAGE_WORK_STARTED.load(Ordering::Relaxed),
                before,
                "{field} refusal must precede input-proportional topology work"
            );
        }

        let admission = TriageTopologyAdmission::from_raw_mesh(&open, exact)
            .expect("the exact cap admits the actual path");
        triage_components(admission, WeldPolicy::ExactBits).expect("open soup remains triageable");
        assert_eq!(
            INPUT_PROPORTIONAL_TRIAGE_WORK_STARTED.load(Ordering::Relaxed),
            before + 1
        );
    }

    #[test]
    fn raw_work_arithmetic_overflow_is_typed() {
        assert!(matches!(
            retained_source_buffer_bytes(usize::MAX, 1),
            Err(TriageAdmissionError::ArithmeticOverflow {
                field: "retained source buffer bytes"
            })
        ));
        assert!(matches!(
            checked_product("edge occurrences", usize::MAX / 3 + 1, 3),
            Err(TriageAdmissionError::ArithmeticOverflow {
                field: "edge occurrences"
            })
        ));
        assert!(matches!(
            checked_product("vertex incidences", usize::MAX / 3 + 1, 3),
            Err(TriageAdmissionError::ArithmeticOverflow {
                field: "vertex incidences"
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
        let _guard = global_counter_guard();
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

    // -----------------------------------------------------------------
    // FU-43: the local ray classifier's half-ray semantics.
    //
    // Characterization (commit ee58444) pinned a defect in
    // `ray_triangle_distance` (this file): it treated *every*
    // `distance <= 1e-9` as `RayHit::Ambiguous` -- not only genuine
    // near-origin degeneracies, but any real intersection strictly behind
    // the ray origin, however far behind. `ContainmentClassifier::
    // classify_ray` then short-circuited on the first `Ambiguous` face it
    // met, discarding every hit already gathered (including a genuine
    // forward crossing) and reporting the whole ray `Ambiguous`.
    //
    // The consumer fix (this commit) narrows the guard to a symmetric band,
    // `|distance| <= 1e-9`, and treats a hit strictly behind the origin
    // outside that band as a definite `Miss`. The three tests below, now
    // updated from the ground truth they characterized to the fixed
    // behavior, establish the fix's effect in increasing scope:
    //   1. one intersection, isolated: a real hit 5 mm behind the origin
    //      reads `Miss`, not `Ambiguous`.
    //   2. a full ray against a real closed shell: a point at a symmetric
    //      octahedron's own centroid -- unambiguously interior material --
    //      now reads `Inside` on all three fixed sampling directions,
    //      because each direction's mirror-image behind-origin face now
    //      reads `Miss` instead of pre-empting the real forward crossing.
    //   3. the pipeline's actual verdict: nesting a small box at that same
    //      centroid, `triage_components` now returns `Err(TriageError::
    //      PreHollowed(_))` for the genuine sealed cavity it used to admit
    //      silently. This is the final-verdict-level fix, not only a
    //      helper-level one -- see that test's doc comment for the exact
    //      arithmetic.
    //
    // None of this touches `geom_kernel`: that crate's finite-segment
    // negative-epsilon and parity handling is a separate, intentional
    // contract and is out of scope here. This module's ray-triangle test is
    // its own independent implementation.

    /// A regular octahedron -- the L1 ball `|x| + |y| + |z| <= r` -- centred
    /// on the origin, with every face wound outward. Not a corpus shape; it
    /// is the smallest closed mesh whose faces are *not* axis-aligned, which
    /// is exactly the property that lets a face's loose bounding-box filter
    /// (`ray_can_reach_bbox`) admit a face that sits behind the ray origin.
    /// An axis-aligned box does not have this problem: each face's bounding
    /// box is degenerate along its own normal axis, so the same filter
    /// rejects every face behind the origin exactly rather than loosely (see
    /// `triage_batches_bounded_samples_and_refuses_a_pre_hollowed_shell` in
    /// `tests/topology_triage.rs`, which is the axis-aligned control this
    /// module's own octahedron tests are contrasted against).
    fn octahedron_faces(r: f64) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        let verts = vec![
            [r, 0.0, 0.0],
            [-r, 0.0, 0.0],
            [0.0, r, 0.0],
            [0.0, -r, 0.0],
            [0.0, 0.0, r],
            [0.0, 0.0, -r],
        ];
        let raw = [
            [0u32, 2, 4],
            [2, 1, 4],
            [1, 3, 4],
            [3, 0, 4],
            [2, 0, 5],
            [1, 2, 5],
            [3, 1, 5],
            [0, 3, 5],
        ];
        // Orient every face outward: for a convex polytope centred at the
        // origin, the outward normal always makes an acute angle with its
        // own vertices.
        let tris = raw
            .into_iter()
            .map(|face| {
                let [a, b, c] = face.map(|index| verts[index as usize]);
                let normal = cross(sub(b, a), sub(c, a));
                if dot(normal, a) > 0.0 {
                    face
                } else {
                    [face[0], face[2], face[1]]
                }
            })
            .collect();
        (verts, tris)
    }

    /// Appends a `box_size`-mm cube centred on the origin to an existing
    /// vertex/face soup and welds the result into a `Mesh`. Corner order
    /// (`x` outer, `y`, `z` inner) and face winding mirror
    /// `fixtures::BOX_FACES` exactly, so this is the same box shape those
    /// fixtures use, just centred at the origin instead of on top of another
    /// solid.
    fn join_centered_box(
        mut verts: Vec<[f64; 3]>,
        mut faces: Vec<[u32; 3]>,
        box_size: f64,
    ) -> Mesh {
        let box_base = u32::try_from(verts.len()).expect("small fixture");
        let half = box_size / 2.0;
        for x in [-half, half] {
            for y in [-half, half] {
                for z in [-half, half] {
                    verts.push([x, y, z]);
                }
            }
        }
        const BOX_FACES: [[u32; 3]; 12] = [
            [1, 3, 0],
            [4, 1, 0],
            [0, 3, 2],
            [2, 4, 0],
            [1, 7, 3],
            [5, 1, 4],
            [5, 7, 1],
            [3, 7, 2],
            [6, 4, 2],
            [2, 7, 6],
            [6, 5, 4],
            [7, 5, 6],
        ];
        faces.extend(
            BOX_FACES.map(|face| [box_base + face[0], box_base + face[1], box_base + face[2]]),
        );
        Mesh::new(verts, faces).expect("closed octahedron + nested box")
    }

    /// Tier 1: one intersection, isolated from the classifier entirely.
    ///
    /// The triangle sits on the plane `x = -5`; the ray starts at the origin
    /// and points along `+x`. The intersection is real (the triangle spans
    /// `y` and `z` well past where the ray crosses the plane, so `u`, `v`,
    /// and `1 - u - v` all sit far inside the triangle) and lies exactly 5 mm
    /// *behind* the ray's origin.
    #[test]
    fn a_real_hit_behind_the_ray_origin_is_a_miss_not_a_crossing() {
        let origin = [0.0, 0.0, 0.0];
        let direction = [1.0, 0.0, 0.0];
        let a = [-5.0, -5.0, -5.0];
        let b = [-5.0, 5.0, -5.0];
        let c = [-5.0, 0.0, 5.0];

        // Ground truth from the plane equation alone, independent of
        // `ray_triangle_distance`'s own epsilon handling: this triangle's
        // plane is only ever reached at `t = -5`.
        let normal = cross(sub(b, a), sub(c, a));
        let plane_t = -dot(sub(origin, a), normal) / dot(direction, normal);
        assert!(
            (plane_t + 5.0).abs() < 1e-9,
            "hand-derived intersection parameter must be -5 (5 mm behind the origin)"
        );

        // Half-ray semantics: a ray only tests intersections ahead of its
        // own origin, so a hit 5 mm behind it is not a crossing at all.
        //
        // FROM (pinned by FU-43 characterization, commit ee58444): the
        // `distance <= 1e-9` guard caught this real, far-behind hit exactly
        // as it would catch a genuine near-origin degeneracy, so it read
        // `Ambiguous`.
        // TO (this fix): `ray_triangle_distance` now only treats
        // `|distance| <= 1e-9` as a near-origin degeneracy; a hit 5 mm
        // behind the origin is well outside that band and reads as a
        // definite `Miss`.
        assert_eq!(
            ray_triangle_distance(origin, direction, a, b, c).expect("finite"),
            RayHit::Miss,
            "a real intersection 5 mm behind the ray origin is not a crossing; half-ray \
             semantics read it as Miss"
        );
    }

    // -----------------------------------------------------------------
    // C5: `ray_triangle_distance` is now triage POLICY over
    // `geom_kernel::ray_triangle` (docs/specs/geometry-canon.md §1 OP-2).
    // These tests pin what the new arrangement answers, rule by rule. They
    // are not characterizations of the retired copy; each states the rule it
    // exercises and why the kernel's answer is the correct one.
    // -----------------------------------------------------------------

    /// The unit triangle in `z = 0`, and a ray straight down onto it from
    /// `(x, y, 1)`. Every C5 acceptance-rule test below aims at this target,
    /// so `u` and `v` are just the query's own `x` and `y`.
    fn unit_triangle_z0() -> ([f64; 3], [f64; 3], [f64; 3]) {
        ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
    }

    fn straight_down(x: f64, y: f64) -> ([f64; 3], [f64; 3]) {
        ([x, y, 1.0], [0.0, 0.0, -1.0])
    }

    /// Rule 1, barycentric slack (`EPS = 1e-6`). A ray passing 5e-7 outside a
    /// triangle's edge registers a crossing.
    ///
    /// The slack is not a tolerance on "is this really inside" — it is what
    /// makes a crossing through an edge SHARED by two faces register on at
    /// least one of them. A zero-slack test, which is what this module used
    /// to have, can miss both, and every point beyond that ray then has the
    /// wrong parity. The accepted cost is this: a ray just outside a
    /// silhouette edge registers on both adjacent faces, at the same `t`, and
    /// the crossing dedup merges the pair back to one.
    #[test]
    fn a_ray_within_the_barycentric_slack_of_an_edge_registers_a_crossing() {
        let (a, b, c) = unit_triangle_z0();
        let (origin, direction) = straight_down(-5.0e-7, 0.25);
        assert_eq!(
            ray_triangle_distance(origin, direction, a, b, c).expect("finite"),
            RayHit::Hit(1.0),
            "u = -5e-7 is inside the kernel's 1e-6 barycentric slack, so this is a crossing"
        );

        // And the slack is bounded: twice outside it is a definite miss.
        let (origin, direction) = straight_down(-2.0e-6, 0.25);
        assert_eq!(
            ray_triangle_distance(origin, direction, a, b, c).expect("finite"),
            RayHit::Miss,
            "u = -2e-6 is outside the slack and must not be a crossing"
        );
    }

    /// Rule 2, the retired `EDGE_EPS = 1e-10` band. A crossing 5e-11 inside an
    /// edge is a definite `Hit`.
    ///
    /// It used to be `Ambiguous`, and one `Ambiguous` face made the whole
    /// component `Ambiguous` and the whole model refused. Refusing a crossing
    /// for being near an edge is precisely what the kernel's slack now
    /// answers correctly, so keeping the band would re-refuse the fix.
    #[test]
    fn a_crossing_just_inside_an_edge_is_a_definite_hit_not_ambiguous() {
        let (a, b, c) = unit_triangle_z0();
        let (origin, direction) = straight_down(0.25, 5.0e-11);
        assert_eq!(
            ray_triangle_distance(origin, direction, a, b, c).expect("finite"),
            RayHit::Hit(1.0),
            "v = 5e-11 is interior; edge proximity is not a reason to refuse to count it"
        );
    }

    /// Rule 3, the determinant guard. A triangle whose determinant lands in
    /// the band `[1e-14, 1e-12)` is now measured rather than skipped.
    ///
    /// This module used to refuse to look below `1e-12`; the kernel prunes at
    /// `1e-14`. The 100x-wide band between them is real geometry that was
    /// never measured. Here the triangle is scaled so that
    /// `det = dot(e1, cross(dv, e2))` sits inside it.
    #[test]
    fn a_determinant_in_the_retired_hundredfold_band_is_measured_not_skipped() {
        // det for a `-z` ray against a `z = 0` triangle with legs `s` is
        // `s^2`, so a leg of `10^-6.5` mm puts `|det|` at 1e-13 — inside
        // `[1e-14, 1e-12)`, the band this module used to refuse to look at.
        let scale = 1.0e-7_f64 * (10.0_f64).sqrt();
        let a = [0.0, 0.0, 0.0];
        let b = [scale, 0.0, 0.0];
        let c = [0.0, scale, 0.0];
        let e1 = sub(b, a);
        let e2 = sub(c, a);
        let determinant = dot(e1, cross([0.0, 0.0, -1.0], e2));
        assert!(
            (1.0e-14..1.0e-12).contains(&determinant.abs()),
            "fixture must sit in the retired band; |det| = {}",
            determinant.abs()
        );
        let (origin, direction) = straight_down(scale / 4.0, scale / 4.0);
        let outcome = ray_triangle_distance(origin, direction, a, b, c).expect("finite");
        // `t` is 1 to within the rounding a 3e-7 mm triangle can carry; the
        // claim under test is that the crossing is MEASURED at all.
        assert!(
            matches!(outcome, RayHit::Hit(t) if (t - 1.0).abs() < 1.0e-9),
            "a triangle this small is still a triangle; the kernel prunes at 1e-14, not 1e-12; \
             got {outcome:?}"
        );
    }

    /// The shared-edge property the slack exists for, stated as a test on two
    /// adjacent faces rather than on one.
    ///
    /// Two triangles meet along the diagonal from `(1,0,0)` to `(0,1,0)`. A
    /// ray aimed exactly at the midpoint of that shared edge must register on
    /// at least one of them: if it registers on neither, parity flips for
    /// every point beyond it, silently.
    #[test]
    fn a_crossing_through_a_shared_edge_registers_on_at_least_one_face() {
        let lower = ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let upper = ([1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]);
        let (origin, direction) = straight_down(0.5, 0.5);
        let on_lower =
            ray_triangle_distance(origin, direction, lower.0, lower.1, lower.2).expect("finite");
        let on_upper =
            ray_triangle_distance(origin, direction, upper.0, upper.1, upper.2).expect("finite");
        let crossings = [on_lower, on_upper]
            .iter()
            .filter(|outcome| matches!(outcome, RayHit::Hit(_)))
            .count();
        assert!(
            crossings >= 1,
            "a ray through a shared edge must be counted by at least one of the two faces; \
             lower = {on_lower:?}, upper = {on_upper:?}"
        );
    }

    /// Triage policy survives the primitive swap, in both directions.
    ///
    /// `TRIAGE_ORIGIN_EPS` is call-site policy the canon keeps deliberately:
    /// a crossing this close to the query point has an untrustworthy sign.
    /// The half-ray rule is policy too, and it is the FU-43 fix — the kernel
    /// accepts `t >= -EPS`, and triage must NOT read that slack as licence to
    /// count a crossing behind the query point.
    #[test]
    fn triage_call_site_policy_still_owns_the_origin_band_and_half_ray_rule() {
        let (a, b, c) = unit_triangle_z0();

        // A crossing 5e-10 in front of the query point: inside the origin
        // band, so unsignable.
        let origin = [0.25, 0.25, 5.0e-10];
        assert_eq!(
            ray_triangle_distance(origin, [0.0, 0.0, -1.0], a, b, c).expect("finite"),
            RayHit::Ambiguous,
            "a crossing within TRIAGE_ORIGIN_EPS of the query point cannot be signed"
        );

        // A crossing 1 mm behind it: definite, and not counted.
        let origin = [0.25, 0.25, -1.0];
        assert_eq!(
            ray_triangle_distance(origin, [0.0, 0.0, -1.0], a, b, c).expect("finite"),
            RayHit::Miss,
            "half-ray semantics: a crossing behind the query point is not a crossing"
        );
    }

    /// The sampling directions the classifier casts are unit length; the
    /// literals they came from are not.
    ///
    /// This is the whole point of the `RAW_DIRECTIONS`/`SAMPLING_DIRECTIONS`
    /// split (`docs/specs/geometry-adjudication.md` §O4 Part 2 item 1). The raw
    /// norms are stated here as MEASURED values, so a future edit to the angles
    /// that quietly reintroduced a unit-length literal would be visible rather
    /// than silently making the split look pointless.
    #[test]
    fn the_sampling_directions_are_unit_length_so_t_is_millimetres() {
        let raw_norms = RAW_DIRECTIONS.map(norm);
        for (index, measured) in raw_norms.iter().enumerate() {
            let expected = [1.075_564_155_9, 1.027_921_468_3, 1.061_765_048_8][index];
            assert!(
                (measured - expected).abs() < 5.0e-11,
                "raw direction {index} norm moved: measured {measured}, recorded {expected}"
            );
            assert!(
                (measured - 1.0).abs() > 0.02,
                "the raw literals are deliberately NOT unit length; direction {index} is \
                 {measured}, so the split has nothing to fix"
            );
        }
        for (index, direction) in SAMPLING_DIRECTIONS.iter().enumerate() {
            let length = norm(*direction);
            // One division per component, so the error is a couple of ULP of
            // 1.0, not a tolerance on geometry.
            assert!(
                (length - 1.0).abs() <= 4.0 * f64::EPSILON,
                "sampling direction {index} must be unit length so `t` is millimetres; \
                 got {length}"
            );
        }
    }

    /// The §O4 counterexample, on the exact literals the adjudication pinned.
    ///
    /// `det = dot(e1, cross(dv, e2)) = -8e-15` — nonzero, and inside
    /// `DET_EPS = 1e-14`. Every product in that chain is exact in f64
    /// (doublings of a stored value and products with zero), so this is not a
    /// rounding accident: the ray genuinely pierces the face INTERIOR at
    /// `t = 1e-16 / 2e-15 ~ 0.05`, at barycentric `u = 0.225, v = 0.25`, and no
    /// adjacent face contains that crossing point.
    ///
    /// The right answer for a parity classifier is therefore neither `Hit` nor
    /// `Miss`: the arithmetic cannot decide at this scale, so the ray must not
    /// contribute a parity bit at all.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "the exactness IS the claim: every product in this determinant chain is a \
                  doubling of a stored value or a product with zero, so `-8e-15` is the \
                  arithmetic's answer and not a rounded one. A tolerance here would let the \
                  fixture drift out of the band it is built to sit in."
    )]
    fn the_o4_counterexample_is_undecidable_not_a_crossing_and_not_a_miss() {
        let a = [0.0, 0.0, 0.0];
        let b = [2.0, 0.0, 0.0];
        let c = [0.0, 2.0, 0.0];
        let origin = [0.4, 0.5, -1.0e-16];
        let direction = [1.0, 0.0, 2.0e-15];

        // The fixture proves itself: the determinant really is in the band.
        let determinant = dot(sub(b, a), cross(direction, sub(c, a)));
        assert_eq!(
            determinant, -8.0e-15,
            "the pinned construction must produce exactly this determinant"
        );
        assert!(
            determinant != 0.0 && determinant.abs() <= 1.0e-14,
            "nonzero and inside DET_EPS: the undecidable band, not the exact-parallel prune"
        );

        assert_eq!(
            ray_triangle_distance(origin, direction, a, b, c).expect("finite"),
            RayHit::NearParallel,
            "a determinant inside DET_EPS says the crossing is undecidable at this scale; \
             triage must say so rather than assert a Miss"
        );
    }

    /// The same undecidable band, reached through the classifier, poisoning
    /// its ray — and counted where the run report can see it.
    ///
    /// The origin is moved from the pinned `z = -1e-16` to `z = -1e-9` for one
    /// reason: at `-1e-16` the query point lies ON the triangle's plane and the
    /// `point_on_triangle` pre-check answers `Ambiguous` before any ray is
    /// cast, so the pinned literals cannot exercise this path. `det` depends
    /// only on `e1`, `e2` and `dv`, so moving the origin along `z` leaves the
    /// band membership exactly as pinned above.
    ///
    /// A counter that cannot fire is worthless, so this asserts the tally
    /// moved, not merely that the verdict did.
    #[test]
    fn an_undecidable_face_poisons_its_ray_and_the_tally_records_it() {
        let verts = vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]];
        let faces = vec![[0_u32, 1, 2]];
        let classifier = ContainmentClassifier::build(&verts, &faces, &[0]).expect("build");

        let origin = [0.4, 0.5, -1.0e-9];
        let direction = [1.0, 0.0, 2.0e-15];
        let mut tally = RayTally::default();
        let mut hits = Vec::new();
        let outcome = classifier
            .classify_ray_counted(origin, direction, &mut tally, &mut hits)
            .expect("finite");

        assert_eq!(
            outcome,
            ContainmentClass::Ambiguous,
            "an undecidable face must poison its ray; the 2-of-3 vote then excludes it"
        );
        assert_eq!(
            tally.near_parallel, 1,
            "the undecidable evaluation must be counted -- this is the measured refusal rate \
             the owner asked for"
        );
        assert_eq!(
            tally.ambiguous, 0,
            "the kernel's undecidable band is a DIFFERENT uncertainty from triage's \
             origin band and must not be folded into it"
        );
        assert_eq!(tally.hits, 0, "no crossing was established");
        assert_eq!(tally.misses, 0, "and none was ruled out either");
        assert_eq!(tally.tests, 1, "exactly one face was evaluated");
        assert!(
            hits.is_empty(),
            "an undecidable evaluation contributes no crossing to the parity list"
        );
    }

    /// The counters are instrumentation, and instrumentation that reports
    /// zero work while work happened is worse than none.
    ///
    /// A verdict table whose probe rows all read `ray_tests = 0` would look
    /// exactly like "nothing changed". This asserts the counters actually
    /// move on a mesh that provably reaches the ray caster.
    #[test]
    fn the_containment_ray_counters_report_the_work_that_actually_ran() {
        let _guard = global_counter_guard();
        let (verts, faces) = octahedron_faces(10.0);
        let face_ids = (0..faces.len()).collect::<Vec<_>>();
        let classifier = ContainmentClassifier::build(&verts, &faces, &face_ids).expect("build");
        reset_containment_ray_counters();
        assert_eq!(
            containment_ray_counters(),
            ContainmentRayCounters::default()
        );
        let before = ContainmentClassifier::build(&verts, &faces, &face_ids).expect("build");
        drop(before);
        classifier
            .classify_batch(&[[0.0, 0.0, 0.0]])
            .expect("finite");
        let counters = containment_ray_counters();
        assert_eq!(counters.classifiers_built, 1, "one build after the reset");
        assert_eq!(
            counters.rays_cast, 3,
            "three sampling directions, one point"
        );
        assert!(
            counters.face_tests > 0,
            "the ray caster ran against real faces; counters reporting zero would make every \
             verdict table a false negative"
        );
        assert_eq!(
            counters.face_tests,
            counters.face_hits
                + counters.face_misses
                + counters.face_ambiguous
                + counters.face_near_parallel,
            "every counted face test lands in exactly one outcome bucket -- including the \
             kernel's undecidable band, which is a bucket of its own and must not be lost \
             into the miss count"
        );
    }

    /// Tier 2: a full closed shell, and today's actual parity consequence.
    ///
    /// The three fixed sampling directions in `ContainmentClassifier::
    /// classify` all have strictly positive `x`, `y`, and `z` components.
    /// For the octahedron's own centroid, each direction's forward crossing
    /// (the near face, in the direction's own positive octant) has a
    /// mirror-image triangle in the *opposite* octant sitting on the same
    /// infinite line -- a real intersection, behind the origin -- because
    /// the octahedron is centrally symmetric.
    #[test]
    fn octahedron_centroid_reads_ambiguous_though_every_direction_has_exactly_one_forward_crossing()
    {
        let _guard = global_counter_guard();
        let (verts, faces) = octahedron_faces(10.0);
        let face_ids = (0..faces.len()).collect::<Vec<_>>();
        let classifier = ContainmentClassifier::build(&verts, &faces, &face_ids).expect("build");

        const DIRECTIONS: [[f64; 3]; 3] = [
            [1.0, 0.371_390_676_354_103_7, 0.137_503_523_749_935_02],
            [0.211_324_865_405_187_1, 1.0, 0.109_381_654_230_983_1],
            [0.127_412_352_941_232_4, 0.333_333_333_333_333_3, 1.0],
        ];
        for direction in DIRECTIONS {
            // Half-ray semantics: exactly one forward crossing (the near
            // face) per direction, an odd count, so each direction alone
            // reports `Inside`.
            //
            // FROM (pinned by FU-43 characterization, commit ee58444): the
            // mirror face's behind-origin hit was visited in the same
            // `classify_ray` loop as the real forward crossing, and the
            // first `Ambiguous` face short-circuited the whole ray before
            // the forward hit's parity could be used.
            // TO (this fix): the mirror face's hit is strictly behind the
            // origin, so `ray_triangle_distance` now reports it `Miss`
            // instead of `Ambiguous`; `classify_ray` never short-circuits,
            // and the one real forward crossing gives an odd parity count.
            assert_eq!(
                classifier
                    .classify_ray([0.0, 0.0, 0.0], direction)
                    .expect("finite"),
                ContainmentClass::Inside,
                "direction {direction:?}: the mirror face's behind-origin hit is now Miss, so \
                 only the real forward crossing counts"
            );
        }

        // FROM: the classifier's own majority rule needs 2 of 3 directions
        // to agree; with all three individually Ambiguous the vote was 0
        // Inside / 0 Outside, so the octahedron's own centroid -- 10 mm
        // from the nearest face along every axis, unambiguously interior
        // material -- read Ambiguous rather than Inside.
        // TO: all three directions now agree Inside, so the majority vote
        // is unanimous.
        assert_eq!(
            classifier.classify([0.0, 0.0, 0.0]).expect("finite"),
            ContainmentClass::Inside,
            "a point deep inside a closed, watertight shell reads Inside"
        );
    }

    /// Tier 3: the actual triage verdict, and the load-bearing consequence.
    ///
    /// A 2 mm box is nested at the octahedron's centroid. It is provably,
    /// deeply interior: every box corner satisfies `|x| + |y| + |z| <= 3`,
    /// against the octahedron's own boundary at `|x| + |y| + |z| = 10` --
    /// 7 mm of clearance on the tightest direction, and no sample point sits
    /// near any triangle's edge. This is exactly the
    /// "sealed cavity" shape triage exists to refuse; compare
    /// `triage_batches_bounded_samples_and_refuses_a_pre_hollowed_shell` in
    /// `tests/topology_triage.rs`, the axis-aligned version of this same
    /// scenario, which the classifier already got right before this fix
    /// because an axis-aligned box's own bounding-box filter rejects its
    /// behind-origin faces exactly rather than loosely.
    ///
    /// The box is 2 mm across (`hollow_shard_max_dim_mm` is 1.0 mm) and does
    /// not touch the octahedron's vertex set, so it cannot be, and is not,
    /// classified as a shard; it reaches the containment/cavity check.
    ///
    /// FROM (pinned by FU-43 characterization, commit ee58444): two of the
    /// box's eight corners (`[-1,-1,-1]` and `[-1,-1,1]`, confirmed by
    /// direct probing of `ContainmentClassifier::classify` during
    /// characterization) landed on the behind-origin-hit failure the two
    /// tests above pin and read `Ambiguous` instead of `Inside`.
    /// `hollow_containment_samples` is 32 but the box only contributes 8
    /// vertices, so all 8 are sampled, and the containment fraction was
    /// 6/8 = 0.75 -- under `hollow_containment_min_fraction` (0.9). Because
    /// the fraction missed the threshold, `triage_components` returned
    /// `Ok`: it silently accepted the shell.
    ///
    /// TO (this fix): all 8 corners now read `Inside` (each several
    /// millimetres clear of both the octahedron's boundary and any triangle
    /// edge), a fraction of 8/8 = 1.0 >= 0.9, and triage refuses with
    /// `TriageError::PreHollowed` -- the same outcome the axis-aligned
    /// control case already got. This is the final-verdict-level fix FU-43's
    /// characterization pass identified: an already-hollow model shaped like
    /// this is now refused instead of being silently admitted and
    /// re-hollowed downstream, which is exactly the wall-destroying,
    /// resin-trapping failure this module's own doc comment says triage
    /// exists to prevent.
    #[test]
    fn triage_accepts_a_pre_hollowed_octahedron_shell_the_containment_fraction_should_refuse() {
        let _guard = global_counter_guard();
        let (verts, faces) = octahedron_faces(10.0);
        let mesh = join_centered_box(verts, faces, 2.0);
        let limits = TriageTopologyLimits::new(
            usize::MAX,
            usize::MAX,
            usize::MAX,
            usize::MAX,
            usize::MAX,
            usize::MAX,
        );
        let admission = TriageTopologyAdmission::from_raw_mesh(&mesh, limits).expect("admits");
        let result = triage_components(admission, WeldPolicy::ExactBits);
        assert!(
            matches!(
                &result,
                Err(TriageError::PreHollowed(refusal)) if refusal.cavity_components().len() == 1
            ),
            "this closed mesh is a genuine sealed cavity (containment fraction 8/8 = 1.0 >= \
             0.9) and half-ray semantics now refuse it as PreHollowed; got {result:?}; see this \
             test's doc comment for the containment-fraction arithmetic"
        );
    }
}
