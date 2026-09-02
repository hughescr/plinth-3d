//! Sealed support preparation through initial physical-contact construction.
//!
//! This is deliberately not a public `run` entry point. It consumes the move-only hollowing
//! hand-off, executes the accepted planning leaves in their frozen order, and seals their
//! authority together for the later controller. Each native leaf remains batch-first: perception
//! gathers its rays and slices, specialized planning batches projections and gates, and initial
//! construction makes one axis-interval batch.

use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
use std::cell::Cell;

use geom_kernel::vec3::{dot, sub};
use geom_kernel::{
    KnnBatchLimits, KnnBatchPlan, NativeSurface, NativeSurfaceConfig, PenetrationBatchLimits,
    SurfaceBuildError,
};
use plinth_core::{m0_constants, Finite, LayerPlan, PositiveFinite};
use plinth_hollow::{HollowHistory, HollowedModel};
use plinth_mesh::{mesh_fingerprint, FaceTopology, Mesh};
use thiserror::Error;

use crate::coverage_measurement::CoverageMeasurementResourceSummary;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MeasurementCallCounts {
    normalized_route_projection_reserves: usize,
    coverage_route_planner_allocations: usize,
    coverage_route_queries: usize,
    coverage_assessment_traversals: usize,
    /// Full validations of the named-island sidecar. One snapshot has one
    /// sidecar, so one controller pass over it is one validation; anything
    /// above that is the same proof re-run per routed contact.
    named_island_validations: usize,
    /// Slice elements the coverage preflight examines while answering its
    /// per-route-key questions -- how many route entries carry a key, how many
    /// memberships carry it, how many coverage samples query it.
    ///
    /// This counts the *shape* of that work, not its wall clock. The sealed
    /// per-route rescans it replaced examined `O(M^2 + K(M + S))` elements for
    /// `M` route entries, `K` distinct route keys and `S` coverage samples;
    /// one gathered sorted pass examines `O(M + S)`.
    preflight_route_key_scans: usize,
}

#[cfg(test)]
thread_local! {
    static MEASUREMENT_CALL_COUNTS: Cell<MeasurementCallCounts> = const {
        Cell::new(MeasurementCallCounts {
            normalized_route_projection_reserves: 0,
            coverage_route_planner_allocations: 0,
            coverage_route_queries: 0,
            coverage_assessment_traversals: 0,
            named_island_validations: 0,
            preflight_route_key_scans: 0,
        })
    };
}

#[cfg(test)]
fn reset_measurement_call_counts() {
    MEASUREMENT_CALL_COUNTS.with(|counts| counts.set(MeasurementCallCounts::default()));
}

#[cfg(test)]
fn measurement_call_counts() -> MeasurementCallCounts {
    MEASUREMENT_CALL_COUNTS.with(|counts| counts.get())
}

#[cfg(test)]
fn record_measurement_call(update: impl FnOnce(&mut MeasurementCallCounts)) {
    MEASUREMENT_CALL_COUNTS.with(|counts| {
        let mut current = counts.get();
        update(&mut current);
        counts.set(current);
    });
}

use crate::{
    anchor::{
        AnchorGeometryAuthority, AnchorSearchConfig, AnchorSearchError, SurfaceAnchorPool,
        SurfaceAnchorPoolMemo,
    },
    assess_coverage_with_mode,
    brace::{
        brace_fixed_point_current_parity, BraceAdditionKind, BraceError, BraceFixedPointPlan,
        BraceFixedPointTermination, SealedTerminalDisposition,
    },
    cco::{
        CcoController, CcoError, FinalizedAnchorTerminalRoute, FinalizedCcoTranscript,
        FinalizedDispositionKind, RouteCcoWorkMeter, TerminalCcoRequest, VerifiedRoutingAuthority,
    },
    coverage_measurement_assessment_resource_plan,
    dyadic::CandidatePayloadConfig,
    forest::{
        ContactEnd, Forest, Member, MemberKey, MemberKind, MemberSpec, Owner, TerminalLedgerState,
    },
    initial::{
        build_initial_contacts, ContactConstructionLimits, InitialContactBuild, InitialContactError,
    },
    joints::{
        plan_joint_balls_filtered, runner_planning_limits, FilteredLiveView, JointConfig,
        JointError, JointPlan,
    },
    lower::{lower_current, LoweredSupportPlan, LoweringConfig, LoweringError},
    perception::{
        prepare_generic_contacts, FinalizedContactLoads, GenericContactPlan,
        GenericPerceptionError, GenericPerceptionLimits, SpecializedOwnerReservation,
    },
    raft::{plan_bottom_connected, RaftLimits, RaftPlan},
    rewire::{optimize as optimize_rewire, RewireError, RewireReport, RewireRequest},
    select_contacts_baseline,
    specialized::{
        fill_route_points_nearest, plan_route_points_nearest, prepare_specialized_contacts,
        CoverageRoutePoint, NearestWitnessRetention, RepairRowId, SpecializedBatchWork,
        SpecializedContactPlan, SpecializedError, SpecializedLimits,
    },
    transient::{transient_report, TransientError, TransientReport},
    validate_perception_input, BuildFrame, ContactSelectionConfig, ContactSelectionError,
    CoverageAssessmentLimits, CoverageAssessmentMode, CoverageGap, CoverageMeasurement,
    CoverageMeasurementError, CoverageMeasurementEvidence, CoverageMeasurementIslandEvidence,
    CoverageMeasurementPatchEvidence, CoverageMeasurementRequest, CoverageMeasurementResourcePlan,
    CoverageSample, SelectedContacts, SupportError, SupportHistory, SupportIsland,
    SupportPerceptionConfig, SupportPerceptionError, SupportReport, SupportRequest, SupportedModel,
};

/// Explicit caller-owned inputs for one sealed support run. No policy defaults
/// are selected here; all limits are supplied by the caller.
#[derive(Clone, Copy, Debug)]
pub struct SupportPreparationInput<'a> {
    layers: &'a LayerPlan,
    delicacy_by_face: &'a [f64],
    column_pitch_mm: PositiveFinite,
    surface_grid_pitch_mm: PositiveFinite,
    max_grid_bytes: usize,
    candidate_config: CandidatePayloadConfig,
    reservations: &'a [SpecializedOwnerReservation],
    perception_limits: GenericPerceptionLimits,
    coverage_samples_max_virtual_cells: usize,
    coverage_score_matrix_max_entries: usize,
    specialized_limits: SpecializedLimits,
    construction_limits: ContactConstructionLimits,
    brace_penetration_limits: PenetrationBatchLimits,
    raft_limits: RaftLimits,
    max_primitives: usize,
    max_primitive_bytes: usize,
}

/// Production-owned, result-inert resource authority for support preparation.
///
/// This is deliberately separate from the borrowed model evidence accepted by
/// [`SupportPreparationInput`]. The pipeline can select this one authority
/// without copying support caps, then bind the current model's batches only
/// when it is ready to execute the stage.
///
/// Two of these caps are **model-scaled**, and this type therefore carries the
/// *rule* rather than one finished number: `knn_max_work` and `max_grid_bytes`
/// are `max(floor, per_face × support_mesh_face_count)`, materialized by
/// [`Self::input`] once the face count is known. A cap's job is to refuse work
/// that is superlinear in model size, not to refuse a large model, and a flat
/// constant cannot tell those apart — it fires on honest linear work at 429k
/// faces while admitting a real quadratic at 24. All four registry constants
/// behind the rule are read here from the one M0 publication
/// (`SupportResourceCapPolicy`); this crate invents no second numeric owner.
#[derive(Clone, Copy, Debug)]
pub struct SupportPreparationLimits {
    column_pitch_mm: PositiveFinite,
    surface_grid_pitch_mm: PositiveFinite,
    /// `SUPPORT_PARITY_GRID_MAX_BYTES_FLOOR`; never the whole cap on its own.
    max_grid_bytes: usize,
    /// `SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE`.
    grid_bytes_per_face: usize,
    /// `SUPPORT_KNN_MAX_WORK_PER_FACE`. It scales exactly one batch, the
    /// perception minima-to-face kNN, because that is the only kNN batch with a
    /// measurement of record. `SUPPORT_KNN_MAX_WORK_FLOOR` is carried inside
    /// both `KnnBatchLimits::max_work` fields below, and for the specialized
    /// coverage-route batch the floor is the whole cap.
    knn_max_work_per_face: usize,
    candidate_config: CandidatePayloadConfig,
    perception_limits: GenericPerceptionLimits,
    coverage_samples_max_virtual_cells: usize,
    coverage_score_matrix_max_entries: usize,
    specialized_limits: SpecializedLimits,
    construction_limits: ContactConstructionLimits,
    brace_penetration_limits: PenetrationBatchLimits,
    raft_limits: RaftLimits,
    max_primitives: usize,
    max_primitive_bytes: usize,
}

/// Materializes one model-scaled cap: `max(floor, per_face × face_count)`.
///
/// Saturating, so an absurd face count yields `usize::MAX` rather than a
/// wrapped-around tiny cap; the caller's own `validate` refuses `usize::MAX`,
/// which keeps "unbounded" unreachable through this path.
#[must_use]
const fn model_scaled_cap(floor: usize, per_face: usize, face_count: usize) -> usize {
    let scaled = per_face.saturating_mul(face_count);
    if scaled > floor {
        scaled
    } else {
        floor
    }
}

impl SupportPreparationLimits {
    /// The single established production cap set for support preparation.
    pub fn production() -> Result<Self, SupportPreparationLimitsError> {
        let pitch = PositiveFinite::new(1.0)
            .map_err(|error| SupportPreparationLimitsError::Pitch(error.to_string()))?;
        let constants = plinth_core::m0_constants().map_err(|error| {
            SupportPreparationLimitsError::RegisteredConstantsUnavailable(error.to_string())
        })?;
        let caps = constants.support_resource_cap_policy();
        // Every cap this function states has exactly one registered owner, and
        // this is the read of it. Before this change the four scaled caps below
        // were read here and the remaining twenty-two were restated as literals
        // whose own registry notes named this very function -- two numeric
        // owners for one number, free to drift apart silently. They now have
        // one owner each, and it is not this function.
        //
        // `diagnostic()` on a `Placeholder` cap is the sanctioned read: a
        // resource cap admits or refuses, it never contributes to an accepted
        // result, so it is not on the accept path the wrapper guards.
        let registered = constants.placeholders();
        let knn_max_work_floor = cap_usize(caps.knn_max_work_floor().diagnostic().get())?;
        let knn_max_work_per_face = cap_usize(*caps.knn_max_work_per_face().value())?;
        let grid_bytes_floor = cap_usize(caps.parity_grid_max_bytes_floor().diagnostic().get())?;
        let grid_bytes_per_face = cap_usize(*caps.parity_grid_max_bytes_per_face().value())?;
        let knn = KnnBatchLimits {
            max_source_points: registered_cap(registered.support_knn_max_source_points())?,
            max_query_points: registered_cap(registered.support_knn_max_query_points())?,
            max_neighbors: registered_cap(registered.support_knn_max_neighbors())?,
            max_logical_peak_bytes: registered_cap(
                registered.support_knn_max_logical_peak_bytes(),
            )?,
            max_work: knn_max_work_floor,
        };
        Ok(Self {
            column_pitch_mm: pitch,
            surface_grid_pitch_mm: pitch,
            max_grid_bytes: grid_bytes_floor,
            grid_bytes_per_face,
            knn_max_work_per_face,
            // The five dyadic candidate-payload numbers below are the one group
            // in this function with no registry owner to read: no constant in
            // the M0 publication declares them. They are left stated here, and
            // that gap is the finding, not this call.
            candidate_config: CandidatePayloadConfig::current(
                8,
                1_000_000,
                3_000_000,
                256 * 1024 * 1024,
                65_536,
            )
            .map_err(|error| SupportPreparationLimitsError::Candidate(error.to_string()))?,
            perception_limits: GenericPerceptionLimits::production(knn),
            coverage_samples_max_virtual_cells: registered_cap(
                registered.support_preparation_coverage_samples_max_virtual_cells(),
            )?,
            coverage_score_matrix_max_entries: registered_cap(
                registered.support_preparation_coverage_score_matrix_max_entries(),
            )?,
            specialized_limits: SpecializedLimits::production(
                knn,
                SpecializedBatchWork::registered()
                    .map_err(|error| SupportPreparationLimitsError::BatchWork(error.to_string()))?,
            ),
            construction_limits: ContactConstructionLimits::new(
                registered_cap(registered.support_construction_max_contacts())?,
                registered_cap(registered.support_construction_max_load_rows())?,
                registered_cap(registered.support_construction_max_work_items())?,
                registered_cap(registered.support_construction_max_scratch_bytes())?,
            )
            .ok_or(SupportPreparationLimitsError::Construction)?,
            brace_penetration_limits: PenetrationBatchLimits {
                max_rows: registered_cap(registered.support_brace_penetration_max_rows())?,
                max_total_work: registered_cap(
                    registered.support_brace_penetration_max_total_work(),
                )?,
                max_total_hits: registered_cap(
                    registered.support_brace_penetration_max_total_hits(),
                )?,
                max_total_samples: registered_cap(
                    registered.support_brace_penetration_max_total_samples(),
                )?,
                max_total_intervals: registered_cap(
                    registered.support_brace_penetration_max_total_intervals(),
                )?,
                max_scratch_bytes: registered_cap(
                    registered.support_brace_penetration_max_scratch_bytes(),
                )?,
            },
            raft_limits: RaftLimits::try_new(
                registered_cap(registered.support_raft_max_candidates())?,
                registered_cap(registered.support_raft_max_roots())?,
                registered_cap(registered.support_raft_max_primitives())?,
                registered_cap(registered.support_raft_max_bytes())?,
            )
            .ok_or(SupportPreparationLimitsError::Raft)?,
            max_primitives: registered_cap(registered.support_preparation_max_primitives())?,
            max_primitive_bytes: registered_cap(
                registered.support_preparation_max_primitive_bytes(),
            )?,
        })
    }

    /// The coverage-verification ceilings implied by this same policy.
    ///
    /// The pre-write coverage gate re-measures the routed-contact projection
    /// this policy's own specialized stage already routed, and then runs the
    /// same `assess_coverage` traversal that stage ran. So it is held to that
    /// stage's registered coverage ceilings rather than to a second set of
    /// numbers invented for the verifier — there is exactly one owner for each
    /// of these caps and it is not this function.
    ///
    /// Each ceiling is taken from the registered owner of the quantity it
    /// bounds, and no cap is asked to do a second job:
    ///
    /// - route memberships and indexed points are both the coverage-route kNN
    ///   batch's source points, because `measure_coverage` pushes exactly one
    ///   `CoverageRoutePoint` per membership and hands that vector to the
    ///   kernel as the source cloud;
    /// - query rows are that batch's query points, one per coverage sample;
    /// - distance work is that batch's `max_work`, the same `source × query`
    ///   product the kernel charges;
    /// - the route kNN batch's own logical peak is that batch's own
    ///   `max_logical_peak_bytes`, its registered owner — it is deliberately
    ///   NOT the coverage scratch ceiling below, which bounds a different and
    ///   larger quantity. Charging the batch against the scratch ceiling held
    ///   it to a quarter of its registered ceiling and made a support run the
    ///   specialized stage was permitted to complete read as an unmeasurable
    ///   physics refusal at the gate;
    /// - assessment work and the *measurement's* retained peak are the
    ///   specialized stage's own `max_coverage_operations` /
    ///   `max_coverage_scratch_bytes`. The retained peak is the sum of the
    ///   nearest vector, the normalized route projection and the larger of the
    ///   route and assessment phase peaks, so it legitimately bounds the kNN
    ///   peak as one of its summands; that aggregate statement is this
    ///   ceiling's own job, not a second charge of the batch's.
    ///
    /// # Errors
    ///
    /// Returns [`CoverageMeasurementError::InvalidLimits`] if any registered
    /// ceiling is zero.
    pub fn coverage_measurement_limits(
        self,
    ) -> Result<crate::CoverageMeasurementLimits, crate::CoverageMeasurementError> {
        let knn = self.specialized_limits.coverage_route_knn;
        crate::CoverageMeasurementLimits::new(
            knn.max_source_points,
            knn.max_source_points,
            knn.max_query_points,
            knn.max_work,
            self.specialized_limits.max_coverage_scratch_bytes,
            self.specialized_limits.max_coverage_operations,
        )?
        .with_route_knn_peak_bytes(knn.max_logical_peak_bytes)
    }

    /// Binds this authority to one model, materializing the two model-scaled
    /// caps against that model's own face count.
    ///
    /// The face count is `delicacy_by_face.len()`: the sidecar is required and
    /// is length-checked against the support mesh at the perception boundary
    /// (`SupportPerceptionError::DelicacyLength`), so it is the face count by
    /// contract rather than by convention. A zero-length sidecar therefore
    /// yields exactly the two floors, which are the caps as they stood before
    /// this rule existed.
    #[must_use]
    pub const fn input<'a>(
        self,
        layers: &'a LayerPlan,
        delicacy_by_face: &'a [f64],
        reservations: &'a [SpecializedOwnerReservation],
    ) -> SupportPreparationInput<'a> {
        let face_count = delicacy_by_face.len();
        let knn_max_work = model_scaled_cap(
            self.perception_limits.minima_knn.max_work,
            self.knn_max_work_per_face,
            face_count,
        );
        let mut perception_limits = self.perception_limits;
        perception_limits.minima_knn.max_work = knn_max_work;
        // The specialized coverage-route kNN batch shares the `KnnBatchLimits`
        // *shape* with the minima batch, but it is deliberately NOT scaled.
        // `SUPPORT_KNN_WORK_PER_FACE_OBSERVED` (696) was measured on the
        // minima-to-face batch alone; the coverage-route batch has no
        // measurement of record, so scaling it by that coefficient would be an
        // undisclosed third cap resting on someone else's number. It stays at
        // the flat `SUPPORT_KNN_MAX_WORK_FLOOR` until it is measured.
        let specialized_limits = self.specialized_limits;
        SupportPreparationInput {
            layers,
            delicacy_by_face,
            column_pitch_mm: self.column_pitch_mm,
            surface_grid_pitch_mm: self.surface_grid_pitch_mm,
            max_grid_bytes: model_scaled_cap(
                self.max_grid_bytes,
                self.grid_bytes_per_face,
                face_count,
            ),
            candidate_config: self.candidate_config,
            reservations,
            perception_limits,
            coverage_samples_max_virtual_cells: self.coverage_samples_max_virtual_cells,
            coverage_score_matrix_max_entries: self.coverage_score_matrix_max_entries,
            specialized_limits,
            construction_limits: self.construction_limits,
            brace_penetration_limits: self.brace_penetration_limits,
            raft_limits: self.raft_limits,
            max_primitives: self.max_primitives,
            max_primitive_bytes: self.max_primitive_bytes,
        }
    }
}

impl SupportPreparationInput<'_> {
    #[must_use]
    pub const fn max_grid_bytes(&self) -> usize {
        self.max_grid_bytes
    }
    /// The materialized minima-kNN work cap for this model. Read-only: it is
    /// execution authority, so it is observable for measurement but never
    /// writable, and it never enters a semantic or result digest.
    #[must_use]
    pub const fn minima_knn_max_work(&self) -> usize {
        self.perception_limits.minima_knn.max_work
    }
    /// The specialized coverage-route kNN work cap for this model. Read-only
    /// on the same terms as [`Self::minima_knn_max_work`], and unlike it this
    /// one is *not* model-scaled: it is here so a test can pin that fact
    /// rather than let a third scaled cap reappear undisclosed.
    #[must_use]
    pub const fn coverage_route_knn_max_work(&self) -> usize {
        self.specialized_limits.coverage_route_knn.max_work
    }
    #[must_use]
    pub const fn max_primitives(&self) -> usize {
        self.max_primitives
    }
    #[must_use]
    pub const fn max_primitive_bytes(&self) -> usize {
        self.max_primitive_bytes
    }

    fn validate(&self) -> bool {
        [
            self.max_grid_bytes,
            self.coverage_samples_max_virtual_cells,
            self.coverage_score_matrix_max_entries,
            self.max_primitives,
            self.max_primitive_bytes,
            self.brace_penetration_limits.max_rows,
            self.brace_penetration_limits.max_total_work,
            self.brace_penetration_limits.max_total_hits,
            self.brace_penetration_limits.max_total_samples,
            self.brace_penetration_limits.max_total_intervals,
            self.brace_penetration_limits.max_scratch_bytes,
        ]
        .iter()
        .all(|value| *value != 0 && *value != usize::MAX)
    }
}

/// Narrows one registered `u64` cap constant to this platform's `usize`.
fn cap_usize(value: u64) -> Result<usize, SupportPreparationLimitsError> {
    usize::try_from(value).map_err(|_| SupportPreparationLimitsError::CapUnrepresentable { value })
}

/// Reads one registered non-zero resource cap out of the M0 publication.
///
/// `diagnostic()` is the sanctioned `Placeholder` read for this family: a
/// resource cap admits or refuses a batch and never contributes a number to an
/// accepted result, so it is not on the accept path the wrapper guards. The
/// registry declares each of these as `NonZeroU64`, so the zero a
/// `ContactConstructionLimits::new` or `RaftLimits::try_new` would reject
/// cannot arrive from here.
fn registered_cap(
    cap: &plinth_core::Placeholder<std::num::NonZeroU64>,
) -> Result<usize, SupportPreparationLimitsError> {
    cap_usize(cap.diagnostic().get())
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SupportPreparationLimitsError {
    #[error("support production pitch is invalid: {0}")]
    Pitch(String),
    #[error("registered M0 constants are unavailable: {0}")]
    RegisteredConstantsUnavailable(String),
    #[error("registered support resource cap {value} is unrepresentable as usize")]
    CapUnrepresentable { value: u64 },
    #[error("registered support batch-work coefficients are unusable: {0}")]
    BatchWork(String),
    #[error("support production candidate limits are invalid: {0}")]
    Candidate(String),
    #[error("support production construction limits are invalid")]
    Construction,
    #[error("support production raft limits are invalid")]
    Raft,
}

/// Public phase result without exposing private runner typestates.
#[derive(Debug, Error)]
pub enum SupportRunError {
    #[error("support run derived a non-finite plate frame")]
    InvalidDerivedFrame,
    #[error("support run requires non-zero explicit resource caps")]
    InvalidResourceCaps,
    /// FU-truthful-errors: `detail` used to not exist. Every `Failed` was
    /// constructed either by `.map_err(|_| ..)`, which threw the real
    /// underlying error away, or (on the coverage-closure controller's first
    /// attempt specifically) by discarding an error that WAS in hand — see
    /// `repair_plan_or_stage_failure`. `detail` is that error's message,
    /// so a `Failed` now says what actually happened instead of only which
    /// phase it happened in.
    #[error("sealed support run failed during {phase:?}: {detail}")]
    Failed {
        phase: SupportRunPhase,
        detail: String,
    },
    /// FU-56: the coverage-closure controller could not admit every measured
    /// island. Boxed because
    /// [`crate::coverage_closure::CoverageClosureRefusal`] carries the whole
    /// trial stream and every still-uncovered island's evidence, and
    /// an unboxed variant would put that weight on every `Result` in this
    /// crate's error path. Distinct from [`Self::Failed`]: a `Failed` phase
    /// means the sealed run itself could not be built at all, where this
    /// means the run built and routed fine but the planner's own coverage
    /// promise was not kept and could not be repaired from the immutable
    /// repair pool within the bound.
    #[error("coverage closure refused: {0:?}")]
    CoverageClosure(Box<crate::coverage_closure::CoverageClosureRefusal>),
}

// `Serialize`/`Deserialize` because FU-57 persists
// `CoverageClosureReason::RepairPlanRefused`, which names the stage that
// refused a repair-extended plan, into the coverage sidecar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SupportRunPhase {
    Preparation,
    Routing,
    Rewire,
    Brace,
    Structure,
    Lowering,
    /// FU-56: the routed-contact coverage re-measurement the closure
    /// controller runs after `Lowering` succeeds. Distinct from `Lowering`
    /// itself so a measurement failure is never misreported as a lowering
    /// failure.
    CoverageMeasurement,
}

/// Opaque, move-only final support authority. It admits only borrowed
/// verification evidence or final consuming scene conversion.
pub struct FinalSupportAuthority<'a> {
    lowered: LoweredSupportRun<'a>,
    ownerless_islands: Vec<OwnerlessIslandDisclosure>,
    abandoned_contacts: Vec<AbandonedContactDisclosure>,
    grown_contact_tips: Vec<GrownContactTipDisclosure>,
    /// **Task #71** — per-contact carried area and emitted tip size.
    emitted_contacts: Vec<EmittedContactDisclosure>,
    /// **SPEC-A4 §A.8, ruling R10 — the loud report, on the SEALED path.**
    /// §A.8 says "every run reports, on every path — sealed or refused", and a
    /// model that seals after eleven trials and two commits must say so as
    /// plainly as one that refuses.
    repair_report: crate::coverage_repair::RepairRunReport,
    /// **SPEC-A4 §D — the trial stream, in memory.**
    ///
    /// One row per transaction, committed or not, each naming the exact
    /// repair-pool row it proposed, the routes its GLOBAL score came from, both
    /// sides of all three accept clauses, and the outcome. Because a step
    /// changes the plan by exactly ONE contact, the before/after difference IS
    /// that contact's contribution — no inference and no apportionment.
    repair_steps: Vec<crate::coverage_repair::CoverageRepairStep>,
    /// **The coverage the run could NOT close, on a run that sealed anyway**
    /// (owner ruling, 2026-09-01).
    ///
    /// `None` on a run whose accepted layout covers every island — the ordinary
    /// case, and the one whose bytes and digest are unchanged by this field
    /// existing.
    ///
    /// `Some` when the repair controller ran out of candidates, or of budget,
    /// with blocking patches still uncovered. That used to be
    /// `SupportRunError::CoverageClosure` and no file at all. The ruling is
    /// that an unsupported region is a WARNING and never a refusal: a file with
    /// a named gap can be opened, inspected, hand-supported in the slicer, and
    /// read as evidence about which parts of the placement logic worked. A
    /// refusal hands the owner nothing and teaches nothing.
    ///
    /// **What it carries is what the refusal carried** — the same
    /// [`crate::coverage_closure::CoverageClosureRefusal`] type, with its
    /// islands, their uncovered mm², their blocking cause and the repair
    /// ledger — so nothing was weakened on the way to becoming a warning.
    ///
    /// **The islands describe the EMITTED run.** They are re-measured on the
    /// geometry this authority actually seals, not copied from the state the
    /// controller was tracking when it gave up; see `seal_uncovered`.
    uncovered: Option<Box<crate::coverage_closure::CoverageClosureRefusal>>,
}

/// One rung of the PR-8 growth ladder, as the run report prints it.
///
/// UNITS: `radius_mm` is a RADIUS. Both are given because the registry speaks
/// radius and the slicer UI speaks diameter, and a number carried between them
/// without halving or doubling lands 2x wrong.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TipGrowthRungDisclosure {
    radius_mm: f64,
    diameter_mm: f64,
    attested: bool,
}

impl TipGrowthRungDisclosure {
    /// The tip RADIUS this rung proposed, mm.
    #[must_use]
    pub const fn radius_mm(&self) -> f64 {
        self.radius_mm
    }
    /// The same rung as a DIAMETER, mm — exactly twice the radius.
    #[must_use]
    pub const fn diameter_mm(&self) -> f64 {
        self.diameter_mm
    }
    /// Whether PR-8 attested at that size.
    #[must_use]
    pub const fn attested(&self) -> bool {
        self.attested
    }
}

/// **One contact this run refused, and the load that refusal abandons.**
///
/// PR-8 refuses a contact whose tip is not measurably in the solid. Where that
/// is curable by growing the tip, the run grows it (see
/// [`GrownContactTipDisclosure`]); this type is what is left when every remedy
/// the stage has was tried and none worked.
///
/// **Why the load is the headline number.** A refused contact is quarantined:
/// it stops carrying, no neighbour is told, none is grown to take its place, and
/// the coverage measurement's 6 mm radius cannot see the hole a single claw tip
/// leaves. The area is therefore genuinely abandoned, and printing it is the
/// difference between a support that was refused and a support that vanished.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbandonedContactDisclosure {
    contact_id: u32,
    seat_mm: [f64; 3],
    abandoned_area_mm2: f64,
    refusal: String,
    law_tip_radius_mm: f64,
    growth_ceiling_radius_mm: f64,
    growth_ceiling_constant: String,
    growth_ladder_entered: bool,
    growth_rungs: Vec<TipGrowthRungDisclosure>,
    reseat: String,
}

impl AbandonedContactDisclosure {
    /// The physical contact this refusal is about.
    #[must_use]
    pub const fn contact_id(&self) -> u32 {
        self.contact_id
    }
    /// Where it stood, in mm — the contact point itself, not the stub's outer
    /// end.
    #[must_use]
    pub const fn seat_mm(&self) -> [f64; 3] {
        self.seat_mm
    }
    /// The routed load this refusal abandons, mm².
    #[must_use]
    pub const fn abandoned_area_mm2(&self) -> f64 {
        self.abandoned_area_mm2
    }
    /// Which PR-8/OD-9 screen refused it, by name.
    #[must_use]
    pub fn refusal(&self) -> &str {
        &self.refusal
    }
    /// The tip RADIUS the load law asked for, mm — the size PR-8 refused.
    #[must_use]
    pub const fn law_tip_radius_mm(&self) -> f64 {
        self.law_tip_radius_mm
    }
    /// The registered maximum the growth ladder stopped at, mm RADIUS. Stated
    /// so the disclosure names the limit and not only the failure.
    ///
    /// It is the SMALLER of the two registered limits — see
    /// [`Self::growth_ceiling_constant`] for which one this is.
    #[must_use]
    pub const fn growth_ceiling_radius_mm(&self) -> f64 {
        self.growth_ceiling_radius_mm
    }

    /// **WHICH registered constant stopped the ladder**, by its registry id.
    ///
    /// `TIP_RADIUS_MAX_GROWN_SCAR_MM` is Craig's measured field ceiling on the
    /// scar a grow-for-attachment rescue may leave — 0.400 mm RADIUS
    /// (0.800 mm DIAMETER), an external requirement carrying field data rather
    /// than physics. `TIP_RADIUS_MAX_LEGACY_MM` is the load law's own clamp
    /// maximum, 1.000 mm radius (2.000 mm diameter).
    ///
    /// The name is printed rather than left to be inferred from the number,
    /// because two limits that differ by 0.6 mm are not distinguishable by eye
    /// in a log, and "which rule refused my support" is the question this line
    /// exists to answer.
    #[must_use]
    pub fn growth_ceiling_constant(&self) -> &str {
        &self.growth_ceiling_constant
    }

    /// **Whether the growth ladder was walked for this contact at all** — the
    /// one fact that tells a refusal the registered maximum STOPPED from a
    /// refusal it merely stood beside.
    ///
    /// `false` means no rung was ever proposed: the seat is buried, and burial
    /// is a mesh fault no tip radius can move. The ceiling reported above is
    /// still the one growth would have used, so both disclosure paths quote the
    /// same limit, but it did not refuse this contact and a reader must not be
    /// left to infer that it did.
    ///
    /// `true` with an EMPTY [`Self::growth_rungs`] is the opposite case and a
    /// real policy refusal: the ladder ran and the ceiling left it no legal
    /// rung, because the load law had already sized this tip at or above the
    /// ceiling.
    #[must_use]
    pub const fn growth_ladder_entered(&self) -> bool {
        self.growth_ladder_entered
    }
    /// Every larger tip that was tried, ascending, and what each measured. A
    /// refused contact never reaches the descending refinement scan — the scan
    /// needs a bracket, and there is none without a success — so this list is
    /// the ladder alone.
    ///
    /// Empty means one of two different things, and
    /// [`Self::growth_ladder_entered`] is what tells them apart: with the
    /// ladder entered, there was no larger legal tip to try — the load law had
    /// already sized this one at or above the ceiling; without it, no rung was
    /// ever proposed, because the refusal is one no tip radius can move.
    #[must_use]
    pub fn growth_rungs(&self) -> &[TipGrowthRungDisclosure] {
        &self.growth_rungs
    }
    /// Why re-seating was not among the remedies. An initial contact's legal
    /// seat set has exactly one member, the point perception chose, so there is
    /// no alternative seat for this stage to try.
    #[must_use]
    pub fn reseat(&self) -> &str {
        &self.reseat
    }
}

/// **One contact whose tip this run grew, on its own initiative, so that it
/// would attest.**
///
/// The emitted radius is the smallest radius the run MEASURED as satisfying BOTH
/// of the tip's duties — the registered load law and PR-8 attachment. The
/// ascending ladder brackets it and the descending refinement scan finds it
/// inside that bracket, so it is the honest minimum for this contact rather than
/// the first rung that happened to work. It is always at least the law radius,
/// so growth can only ever move a tip in the conservative direction.
///
/// **The seat is part of the disclosure, not decoration.** A grown tip is a
/// larger scar than the load law asked for, in a place the operator did not
/// choose. Printing only a contact id says a scar exists somewhere; printing the
/// seat says WHERE, which is the difference between a record and a record
/// somebody can act on. The refusal disclosure already carried it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrownContactTipDisclosure {
    contact_id: u32,
    seat_mm: [f64; 3],
    law_tip_radius_mm: f64,
    emitted_tip_radius_mm: f64,
    emitted_tip_diameter_mm: f64,
    kept_area_mm2: f64,
    growth_rungs: Vec<TipGrowthRungDisclosure>,
}

impl GrownContactTipDisclosure {
    #[must_use]
    pub const fn contact_id(&self) -> u32 {
        self.contact_id
    }
    /// Where the grown tip stands, in mm — the contact point itself, not the
    /// stub's outer end. Same convention as
    /// [`AbandonedContactDisclosure::seat_mm`].
    #[must_use]
    pub const fn seat_mm(&self) -> [f64; 3] {
        self.seat_mm
    }
    /// What the load law asked for, mm RADIUS. PR-8 refused this size.
    #[must_use]
    pub const fn law_tip_radius_mm(&self) -> f64 {
        self.law_tip_radius_mm
    }
    /// What the run actually built, mm RADIUS.
    #[must_use]
    pub const fn emitted_tip_radius_mm(&self) -> f64 {
        self.emitted_tip_radius_mm
    }
    /// The same tip as a DIAMETER, mm — what the slicer UI would show.
    #[must_use]
    pub const fn emitted_tip_diameter_mm(&self) -> f64 {
        self.emitted_tip_diameter_mm
    }
    /// The load this growth KEPT, mm² — the area that would otherwise have been
    /// abandoned at quarantine.
    #[must_use]
    pub const fn kept_area_mm2(&self) -> f64 {
        self.kept_area_mm2
    }
    /// Every radius tried, in the order it was MEASURED: the ascending ladder
    /// that bracketed the answer, then the descending refinement scan that
    /// found the smallest attesting radius inside that bracket. The cure is
    /// therefore the smallest attesting entry, not necessarily the last one.
    #[must_use]
    pub fn growth_rungs(&self) -> &[TipGrowthRungDisclosure] {
        &self.growth_rungs
    }
}

fn growth_rungs(
    attempts: &[crate::designed_embed::TipGrowthAttempt],
) -> Vec<TipGrowthRungDisclosure> {
    attempts
        .iter()
        .map(|attempt| TipGrowthRungDisclosure {
            radius_mm: attempt.radius_mm,
            diameter_mm: attempt.radius_mm * 2.0,
            attested: attempt.attested,
        })
        .collect()
}

/// Project the initial build's PR-8 evidence into the two public disclosures.
///
/// Batch-first and allocation-bounded by construction: one pass over each of the
/// two evidence lists, which are the refusals and the cures, not the contacts.
/// On a healthy model both are empty and this costs two empty vectors.
fn contact_attestation_disclosures(
    initial: &crate::initial::InitialContactBuild,
) -> (
    Vec<AbandonedContactDisclosure>,
    Vec<GrownContactTipDisclosure>,
) {
    let abandoned = initial
        .unattested()
        .iter()
        .map(|stub| AbandonedContactDisclosure {
            contact_id: stub.terminal,
            seat_mm: stub.seat_mm,
            abandoned_area_mm2: stub.abandoned_mm2,
            refusal: match stub.refusal {
                crate::designed_embed::ContactAttestationRefusal::BuriedSurface { .. } => {
                    "buried_surface"
                }
                crate::designed_embed::ContactAttestationRefusal::NotAttached { .. } => {
                    "not_attached"
                }
                // The UNDECIDABLE cause gets its own string, and it is the only
                // `UnusableGeometry` cause that can reach a PLACED stub's
                // disclosure at all (`initial::attest_initial_contacts` still
                // collects the rest into a total build refusal). It is named
                // apart because the operator's next move differs: an
                // undecidable ray means this SEAT could not be judged and a
                // seat a fraction of a millimetre away could have been, so the
                // gap is a placement-search result and points at the placement
                // code — where a genuinely unusable geometry would point at the
                // producer of the evidence.
                crate::designed_embed::ContactAttestationRefusal::UnusableGeometry {
                    cause: crate::designed_embed::DesignedMaskError::NearParallelWitness { .. },
                    ..
                } => "undecidable_geometry",
                crate::designed_embed::ContactAttestationRefusal::UnusableGeometry { .. } => {
                    "unusable_geometry"
                }
            }
            .to_owned(),
            law_tip_radius_mm: stub.remedy.law_radius_mm,
            growth_ceiling_radius_mm: stub.remedy.growth_ceiling_radius_mm,
            growth_ceiling_constant: stub.remedy.growth_ceiling_constant.to_owned(),
            growth_ladder_entered: stub.remedy.ladder_entered,
            growth_rungs: growth_rungs(&stub.remedy.growth),
            reseat: match stub.remedy.reseat {
                crate::initial::ReseatAvailability::NoAlternativeSeatAtThisStage => {
                    "no_alternative_seat_at_this_stage"
                }
            }
            .to_owned(),
        })
        .collect();
    let grown = initial
        .grown()
        .iter()
        .map(|contact| GrownContactTipDisclosure {
            contact_id: contact.terminal,
            seat_mm: contact.seat_mm,
            law_tip_radius_mm: contact.law_radius_mm,
            emitted_tip_radius_mm: contact.emitted_radius_mm,
            emitted_tip_diameter_mm: contact.emitted_radius_mm * 2.0,
            kept_area_mm2: contact.carried_mm2,
            growth_rungs: growth_rungs(&contact.attempts),
        })
        .collect();
    (abandoned, grown)
}

/// Task #49 — one island the sealing attempt's ACCEPTED contact set leaves with
/// no owning contact, disclosed beside what the run's own coverage measurement
/// says that island actually leaves uncovered.
///
/// **Why both numbers, and why neither is the other.**
/// [`Self::ownerless_face_area_mm2`] is provenance arithmetic: the total area of
/// the faces whose every eligible load owner lost every contact, so nothing is
/// sized to carry them. [`Self::measured_uncovered_area_mm2`] is a measurement:
/// what the routed layout, arms and all, leaves uncovered on this island under
/// the registered coverage request. They answer different questions and they
/// are computed from different relations — eligibility is `(owner, face)`
/// provenance, coverage is distance — so neither bounds the other.
///
/// **What CAN close the gap between them, measured rather than assumed.** The
/// coverage measurement matches a sample only against routed contacts of its
/// OWN `(island_id, component_id)` route (`specialized.rs`,
/// `fill_route_points_nearest`), so a *neighbouring island's* arm never counts
/// as coverage here, however close it stands. What can cover an ownerless face
/// is a surviving contact of a DIFFERENT OWNER on the SAME island and
/// component: one island carries several load owners, and only the owners a
/// face is eligible for can be charged for it.
///
/// On the four corpus models measured 2026-08-19 the two numbers were EQUAL to
/// six decimals — the ownerless faces were also entirely uncovered — so do not
/// read a small measured area as evidence that something else is holding the
/// face. Read it as what it is: an area small enough that the registered A3
/// censor forgave it, which is why the model still EMITTED. The disclosure
/// exists so that "forgave it" is a thing the run says out loud.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerlessIslandDisclosure {
    island_id: Option<u32>,
    ownerless_face_count: usize,
    ownerless_face_area_mm2: f64,
    measured_uncovered_area_mm2: Option<f64>,
}

impl OwnerlessIslandDisclosure {
    /// The named island these faces belong to, or `None` for a face that
    /// belongs to no island at all — a face reached only through a specialized
    /// reservation. Reported rather than dropped, so no area goes missing.
    #[must_use]
    pub const fn island_id(&self) -> Option<u32> {
        self.island_id
    }
    /// How many of this island's faces the accepted partition charges to
    /// nobody. Never the island's whole face count unless every one of its
    /// contacts went unrouted.
    #[must_use]
    pub const fn ownerless_face_count(&self) -> usize {
        self.ownerless_face_count
    }
    /// Their total area, summed from the load eligibility's own per-face areas.
    #[must_use]
    pub const fn ownerless_face_area_mm2(&self) -> f64 {
        self.ownerless_face_area_mm2
    }
    /// What the sealing attempt's routed-coverage measurement reports uncovered
    /// for this island, summed over its patches, UNCENSORED.
    ///
    /// `Some(0.0)` means the measurement ran over this island and found nothing
    /// uncovered. `None` means no measurement of this island exists to report:
    /// the ownerless faces belong to no named island, so the coverage
    /// measurement — which groups its patches BY island — has no row that could
    /// be attributed to them. Those are different facts and a run report that
    /// printed `0.000000` for both would be claiming a measurement it never
    /// took.
    #[must_use]
    pub const fn measured_uncovered_area_mm2(&self) -> Option<f64> {
        self.measured_uncovered_area_mm2
    }
}

/// Join the accepted set's ownerless faces to the sealing attempt's own
/// coverage measurement, one island per row.
///
/// Batch-first: the island membership is inverted ONCE into a face -> island
/// map and the measurement is summed ONCE into an island -> area map, so this
/// is linear in the plan's face count plus the patch face count, never a scan
/// per ownerless face.
///
/// Split from [`ownerless_island_disclosures`] so the join itself can be
/// exercised on constructed inputs: the caller below gathers three real slices
/// out of the run and this decides every number the report prints.
///
/// **Known imprecision, in the conservative direction.** `repair_targets` keys
/// its patches by `(island_id, component_id)` while an ownerless face carries
/// only its island, so an island split over several components sums ALL its
/// components' uncovered area into the one disclosure row. That over-states the
/// measured figure — it can never hide uncovered area — and is recorded here
/// rather than papered over.
fn join_ownerless_island_disclosures(
    ownerless: &[crate::perception::OwnerlessLoadFace],
    islands: &[crate::SupportIsland],
    repair_targets: &[crate::coverage_closure::BlockingPatchKey],
) -> Vec<OwnerlessIslandDisclosure> {
    if ownerless.is_empty() {
        return Vec::new();
    }
    let mut island_of_face: BTreeMap<u32, u32> = BTreeMap::new();
    for island in islands {
        for &face_id in &island.face_ids {
            island_of_face.insert(face_id, island.id);
        }
    }
    let mut measured: BTreeMap<u32, f64> = BTreeMap::new();
    for key in repair_targets {
        *measured.entry(key.island_id).or_insert(0.0) +=
            f64::from_bits(key.uncovered_area_mm2_bits);
    }
    let mut rows: BTreeMap<Option<u32>, (usize, f64)> = BTreeMap::new();
    for face in ownerless {
        let entry = rows
            .entry(island_of_face.get(&face.face_id).copied())
            .or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 += face.area_mm2;
    }
    rows.into_iter()
        .map(
            |(island_id, (ownerless_face_count, ownerless_face_area_mm2))| {
                OwnerlessIslandDisclosure {
                    island_id,
                    ownerless_face_count,
                    ownerless_face_area_mm2,
                    // `Some(0.0)` and `None` are different claims: the first is
                    // "the measurement covers this island and found nothing",
                    // the second is "this island is not a thing the
                    // measurement grouped by, so no figure exists".
                    measured_uncovered_area_mm2: island_id
                        .map(|id| measured.get(&id).copied().unwrap_or(0.0)),
                }
            },
        )
        .collect()
}

/// Gather the three real slices the disclosure joins, out of the run that is
/// about to seal.
fn ownerless_island_disclosures(
    lowered: &LoweredSupportRun<'_>,
    repair_targets: &[crate::coverage_closure::BlockingPatchKey],
) -> Vec<OwnerlessIslandDisclosure> {
    let routed = &lowered.structured.braced.rewired.routed;
    let ownerless = &routed.accepted.ownerless_faces;
    if ownerless.is_empty() {
        return Vec::new();
    }
    let plan = routed.accepted.plan(&routed.plan);
    join_ownerless_island_disclosures(ownerless, plan.islands(), repair_targets)
}

impl FinalSupportAuthority<'_> {
    pub fn verification_snapshot(
        &self,
    ) -> Result<SupportVerificationSnapshot<'_>, SupportVerificationError> {
        self.lowered
            .verification_snapshot()
            .map_err(|_| SupportVerificationError::AuthorityMismatch)
    }

    /// How many repair contacts this run KEPT: zero when the baseline layout
    /// already covered every island.
    ///
    /// **Re-derived at §G.1 stage 4, from rounds to COMMITS.** It used to be
    /// `closure_history.len() - 1` — one per pass of the batch loop, each of
    /// which appended a whole nomination batch, so "3 rounds" could mean three
    /// contacts or two hundred and sixty-six. Under one-winner transactions a
    /// committed step IS one contact, so this counts contacts that earned their
    /// place. Rejected trials are not rounds and are not counted here; they are
    /// in [`Self::repair_steps`] and in the §A.8 report.
    #[must_use]
    pub fn coverage_repair_rounds(&self) -> usize {
        self.repair_report.commits
    }

    /// **SPEC-A4 §A.8, ruling R10** — the trial count, the commit count, the
    /// rejection histogram, the wall time and the diversity-demotion pair for
    /// the run that sealed.
    ///
    /// These counters REPORT and none of them DECIDES: no branch of the
    /// one-winner loop reads any of them except `committed` against the
    /// registered bound, which is rulings R6 and R11's own rule.
    #[must_use]
    pub const fn repair_report(&self) -> &crate::coverage_repair::RepairRunReport {
        &self.repair_report
    }

    /// **SPEC-A4 §D.1** — every transaction this run attempted, in order.
    ///
    /// Empty when the model sealed on its baseline layout, which is a true
    /// statement about that model rather than a missing record: it never
    /// entered a transaction.
    #[must_use]
    pub fn repair_steps(&self) -> &[crate::coverage_repair::CoverageRepairStep] {
        &self.repair_steps
    }

    /// **The coverage this run left open, on a run that emitted anyway.**
    ///
    /// `None` means the accepted layout covered every island — not that the
    /// question went unasked. A caller must not report a `Some` run as fully
    /// covered: the artifact is real, and so is the gap in it.
    #[must_use]
    pub fn uncovered_coverage(
        &self,
    ) -> Option<&crate::coverage_closure::CoverageClosureRefusal> {
        self.uncovered.as_deref()
    }

    /// SPEC-A3-A4 §6's removal ledger for the attempt that sealed: every
    /// regular contact the cadence did not route, with its exact
    /// candidate-pool row, its reason, and the carried area the pre-removal
    /// partition was charging it.
    ///
    /// This is what makes the clause observable from outside the crate. It is
    /// **not** empty on a healthy corpus model — see the task #49 record for
    /// the measured share of each model's carried area that was being charged
    /// to contacts with no support at all.
    #[must_use]
    pub fn unrouted_regular_removals(&self) -> &[UnroutedRegularRemoval] {
        &self
            .lowered
            .structured
            .braced
            .rewired
            .routed
            .accepted
            .removals
    }

    /// Every unrouted demand-bearing (specialized minimum or island-birth)
    /// contact the sealing attempt found. §6 calls this case a hard failure;
    /// this change measures and ledgers it without refusing on it.
    #[must_use]
    pub fn unrouted_specialized_contacts(&self) -> &[UnroutedSpecializedContact] {
        &self
            .lowered
            .structured
            .braced
            .rewired
            .routed
            .accepted
            .specialized_unrouted
    }

    /// **PR-8 grow-to-attest — every contact the run REFUSED after trying every
    /// remedy it has, with the load that refusal abandons.**
    ///
    /// A quarantined initial contact stops carrying, nothing is told to grow in
    /// its place, and the coverage measurement works at a 6 mm radius, far too
    /// coarse to notice a single missing claw tip. Before this disclosure a run
    /// could drop a routed 2.98 mm² of Deinonychus toe load and say nothing at
    /// all. Now it says so, per contact, with the seat, the area, and every
    /// remedy it tried on the way.
    ///
    /// **Empty is the ordinary outcome and it is the point.** A non-empty list
    /// means this run emitted an artifact with a load nothing carries.
    #[must_use]
    pub fn abandoned_contacts(&self) -> &[AbandonedContactDisclosure] {
        &self.abandoned_contacts
    }

    /// **PR-8 grow-to-attest — every contact whose tip the run GREW on its own
    /// initiative to make it attest.**
    ///
    /// Each row is a support that would have been quarantined, and a load that
    /// would have been abandoned, at the tip radius the load law asked for. The
    /// emitted radius is the smallest rung of the growth ladder that satisfies
    /// both of the tip's duties, so it is the honest minimum for that contact
    /// rather than a concession to either duty.
    #[must_use]
    pub fn grown_contact_tips(&self) -> &[GrownContactTipDisclosure] {
        &self.grown_contact_tips
    }

    /// Task #49 — every island the accepted contact set leaves with no owning
    /// contact, with what the run's own coverage measurement says that island
    /// actually leaves uncovered. See [`OwnerlessIslandDisclosure`].
    ///
    /// **Empty on the ordinary model, and that is the point.** A non-empty list
    /// means this run emitted an artifact over an island nothing is charged to
    /// carry, and the run report prints it for exactly that reason: the verdict
    /// forgave it under the registered censor, and a forgiveness nobody can see
    /// is a forgiveness nobody can audit.
    #[must_use]
    pub fn ownerless_islands(&self) -> &[OwnerlessIslandDisclosure] {
        &self.ownerless_islands
    }

    /// **Task #71** — per-contact carried area and emitted tip size, for every
    /// contact the sealing attempt routed.
    ///
    /// Projected in [`seal_authority`] from the SEALING attempt's own transcript
    /// and forest, for the same reason the abandoned-contact and grown-tip lists
    /// are: a disclosure paired with a different attempt's geometry would name
    /// areas and radii belonging to a plan this run did not emit.
    #[must_use]
    pub fn emitted_contacts(&self) -> &[EmittedContactDisclosure] {
        &self.emitted_contacts
    }

    pub fn into_supported_model(self) -> Result<SupportedModel, SupportRunError> {
        self.lowered
            .into_supported_model()
            .map_err(|error| SupportRunError::Failed {
                phase: SupportRunPhase::Lowering,
                detail: format!(
                    "materializing the final emittable scene from the sealed authority failed: \
                 {error:?}"
                ),
            })
    }
}

/// Record a stage failure for the census, when the census is compiled in.
///
/// A free function rather than an inline `census_record!` at each call site so
/// the `error` binding is consumed in BOTH feature configurations — otherwise
/// the ordinary (census-off) build warns on six unused closure parameters.
fn note_stage_failure<E: std::fmt::Debug>(stage: &'static str, error: &E) {
    crate::census_record!(crate::census::note_stage_error(stage, error));
    #[cfg(not(feature = "census"))]
    let _ = (stage, error);
}

/// Build the sealed authority from the lowered run that closed.
///
/// One function rather than two inline blocks so the zero-trial seal and the
/// seal-after-commit can never disagree about what a SEALED run discloses. The
/// two joins here are the reason:
///
/// * Task #49's ownerless faces are joined to `repair_targets` — the SAME
///   attempt's uncensored measurement of how much area each island truly leaves
///   uncovered. Joining them anywhere else would pair a disclosure with a
///   different run's measurement.
/// * PR-8's abandoned contacts and grown tips are projected from the SEALING
///   attempt's own initial build, for the same reason: a disclosure paired with
///   a different attempt's attestation would name loads and radii belonging to
///   geometry this run did not emit.
fn seal_authority<'a>(
    lowered: LoweredSupportRun<'a>,
    repair_targets: &[crate::coverage_closure::BlockingPatchKey],
    repair_report: crate::coverage_repair::RepairRunReport,
    repair_steps: Vec<crate::coverage_repair::CoverageRepairStep>,
) -> FinalSupportAuthority<'a> {
    let ownerless_islands = ownerless_island_disclosures(&lowered, repair_targets);
    let (abandoned_contacts, grown_contact_tips) =
        contact_attestation_disclosures(&lowered.structured.braced.rewired.routed.initial);
    let emitted_contacts = emitted_contact_disclosures(&lowered);
    FinalSupportAuthority {
        lowered,
        ownerless_islands,
        abandoned_contacts,
        grown_contact_tips,
        emitted_contacts,
        repair_report,
        repair_steps,
        uncovered: None,
    }
}

/// **Emit the model the controller could not finish covering (owner ruling,
/// 2026-09-01).**
///
/// The controller reaches this with an accepted `(plan, context)` and no lowered
/// run: the accepted run was decomposed back into its two halves so the next
/// trial could borrow the context, and every trial since then has been rolled
/// back. So the artifact has to be re-derived, by running the same stage chain
/// on the same accepted plan. That is one extra chain pass, paid once, only on
/// the path that used to produce nothing at all.
///
/// **The coverage evidence attached to the result is RE-MEASURED on this run,
/// not carried over from the controller's own bookkeeping.** The controller's
/// `measured` describes the state it was tracking; this describes the file. If
/// the two ever disagree, the file's own measurement is the true one, and it is
/// the one an operator will be looking at in the slicer. Only the parts of the
/// refusal that are statements about the SEARCH — the reason it stopped, the
/// attempt count, the repair ledger and the trial steps — are carried through
/// unchanged, because those are facts about the search and the search really
/// did happen.
///
/// **It can still fail, and then it refuses exactly as before.** If the chain
/// will not lower the accepted plan, or the coverage of the re-lowered run
/// cannot be measured, there is no artifact to hand anybody and the original
/// closure refusal is returned untouched. Emitting a file the run cannot
/// describe would be worse than refusing: the whole value of the warning is
/// that it is accurate.
#[allow(
    clippy::too_many_arguments,
    reason = "every argument is one thing the emitted artifact needs and nothing owns jointly: \
              the refusal being downgraded, the two halves of the accepted run, the steering and \
              the request they must be re-lowered under, and the two ledger fields that are \
              MOVED out of the caller's ledger. Bundling them would only move the same list one \
              line up, exactly as `refuse` above says of its own"
)]
fn seal_uncovered<'a>(
    mut refusal: Box<crate::coverage_closure::CoverageClosureRefusal>,
    context: SupportRunContext<'a>,
    plan: SpecializedContactPlan,
    steering: &crate::legalization::LegalizationSteering,
    coverage_request: CoverageMeasurementRequest,
    report: crate::coverage_repair::RepairRunReport,
    steps: Vec<crate::coverage_repair::CoverageRepairStep>,
    tally: &BTreeMap<(u32, u32), crate::coverage_closure::KeyNomination>,
) -> Result<FinalSupportAuthority<'a>, SupportRunError> {
    let ChainOutcome::Lowered(lowered) = run_stage_chain(context, plan, None, Some(steering))
    else {
        crate::coverage_repair::emit_run_line(
            "coverage: the accepted layout could not be re-lowered for emission, so the \
             coverage-closure refusal stands and no file is written",
        );
        return Err(SupportRunError::CoverageClosure(refusal));
    };
    let Ok(measured) = measure_for_controller(&lowered, coverage_request) else {
        crate::coverage_repair::emit_run_line(
            "coverage: the re-lowered layout could not be measured, so the coverage-closure \
             refusal stands and no file is written rather than one whose gap cannot be named",
        );
        return Err(SupportRunError::CoverageClosure(refusal));
    };
    // **The warning must describe the FILE.** The refusal's islands are the
    // richer record — each one carries the blocking cause and the route outcome
    // the search measured for it — so they are kept when the re-lowered run
    // blocks on the same set, which is what a deterministic chain on an
    // unchanged plan produces. When the two disagree, the re-measurement wins
    // and the cause detail is lost, because a cause attached to the wrong
    // island is worse than no cause at all, and the operator is going to be
    // looking at the file rather than at the search.
    let remeasured =
        crate::coverage_closure::islands_from_blocking(&measured.blocking, tally);
    let same_islands = |left: &[crate::coverage_closure::CoverageClosureIsland],
                        right: &[crate::coverage_closure::CoverageClosureIsland]| {
        left.len() == right.len()
            && left.iter().zip(right).all(|(one, other)| {
                one.island_id == other.island_id
                    && one.component_id == other.component_id
                    && one.sorted_face_ids == other.sorted_face_ids
            })
    };
    if !same_islands(&refusal.islands, &remeasured) {
        crate::coverage_repair::emit_run_line(&format!(
            "coverage: the emitted layout blocks on {} island(s), not the {} the search stopped \
             on; the warning reports the emitted ones and drops the search's per-island cause",
            remeasured.len(),
            refusal.islands.len(),
        ));
        refusal.islands = remeasured;
    }
    let mut authority = seal_authority(*lowered, &measured.repair_targets, report, steps);
    authority.uncovered = Some(refusal);
    Ok(authority)
}

/// **Task #71's projection** — one row per ROUTED contact of the sealing
/// attempt, carrying what it is charged with and how big the tip that carries
/// it came out.
///
/// The three sources are the three that own the three facts, and none of them
/// is re-derived: the finalized transcript says which terminals routed, the
/// ACCEPTED plan (task #49's reduced child, after every removal and
/// repartition) says what each one carries, and the final forest says what
/// radius its member ended up at. Reading the pre-strip plan instead would
/// report a load partition SPEC §6 says must not exist by this point.
///
/// Batch-first: the contacts are indexed once and the transcript is walked
/// once, so this is O(rows log contacts) and never O(rows x contacts). No
/// surface is projected and no geometry is recomputed.
///
/// A transcript row whose contact or member cannot be resolved contributes a
/// row with `None` in every field it could not read, rather than being dropped:
/// a contact that emitted and could not be described is exactly the thing this
/// disclosure exists to make visible, and dropping it would restore the
/// silence. **Nothing is invented to fill the gap.** An unresolved join used to
/// be disclosed as `0.0 mm² at the origin`, which a reader cannot tell from a
/// contact that genuinely carries nothing at a genuine seat — and this
/// artifact's own sidecar already carries a real `carried_area_mm2 == 0` row.
/// Zero is a legal area to read and an illegal one to invent, exactly as it is
/// a legal radius to read and an illegal one to invent.
fn emitted_contact_disclosures(lowered: &LoweredSupportRun<'_>) -> Vec<EmittedContactDisclosure> {
    let routed = &lowered.structured.braced.rewired.routed;
    let forest = &lowered.structured.braced.plan.forest;
    let accepted = routed.accepted.plan(&routed.plan);
    let carried_by_id: BTreeMap<u64, (f64, [f64; 3])> = accepted
        .contacts()
        .iter()
        .map(|contact| {
            (
                contact.contact_id(),
                (contact.carried_area_mm2(), contact.position_mm()),
            )
        })
        .collect();
    routed
        .transcript
        .rows()
        .iter()
        .filter(|row| row.kind() == FinalizedDispositionKind::Routed)
        .map(|row| {
            let contact_id = row.physical_contact_id();
            let carried = carried_by_id.get(&contact_id).copied();
            let tip_radius_mm = forest
                .terminal_key(row.terminal_id())
                .and_then(|key| forest.member(key))
                .map(|member| member.terminal_radius_mm().get().get());
            EmittedContactDisclosure {
                contact_id,
                position_mm: carried.map(|(_, position)| position),
                carried_area_mm2: carried.map(|(area, _)| area),
                tip_radius_mm,
                tip_diameter_mm: tip_radius_mm.map(|radius| radius * 2.0),
            }
        })
        .collect()
}

/// One measurement, split the two ways SPEC-A3-A4 splits it, plus the two
/// scalars the accept test reads. Owned rather than borrowed: the measurement
/// borrows the lowered run, and the controller must outlive it to decide.
struct MeasuredState {
    /// The CENSORED verdict set. Empty means the model seals.
    blocking: Vec<crate::coverage_closure::BlockingPatchKey>,
    /// The UNCENSORED repair worklist, a superset of `blocking` (task #27).
    repair_targets: Vec<crate::coverage_closure::BlockingPatchKey>,
    uncovered_samples: Vec<usize>,
    objective: crate::CoverageObjective,
    uncensored_uncovered_mm2: f64,
}

/// Take one routed coverage measurement against the registered gate request.
///
/// SPEC-A4 §1's expensive reading, deliberately: the verification inside a
/// transaction is a FULL routed measurement at
/// `registered_coverage_request` — the same request
/// `plinth_pipeline::gates::registered_coverage_request` hands the pre-write
/// gate. A cheap local re-measure around the new seat is not excluded by §6's
/// text and is refused here for one reason: the controller must close to
/// exactly what the gate will re-measure. Closing to a narrower promise than
/// the gate keeps is the forgery FU-8b's doctrine forbids.
fn measure_for_controller(
    lowered: &LoweredSupportRun<'_>,
    coverage_request: crate::coverage_measurement::CoverageMeasurementRequest,
) -> Result<MeasuredState, (SupportRunPhase, String)> {
    let snapshot = lowered.verification_snapshot().map_err(|error| {
        note_stage_failure("CoverageMeasurement", &error);
        (SupportRunPhase::CoverageMeasurement, format!("{error:?}"))
    })?;
    let measurement = snapshot
        .measure_coverage(coverage_request)
        .map_err(|error| {
            note_stage_failure("CoverageMeasurement", &error);
            (SupportRunPhase::CoverageMeasurement, error.to_string())
        })?;
    let censor_area_mm2 = coverage_request.patch_area_floor_mm2().get();
    let patches = measurement.evidence().patches();
    let mut blocking = Vec::new();
    let mut repair_targets = Vec::with_capacity(patches.len());
    for patch in patches {
        let uncovered_area_mm2 = patch.uncovered_area_mm2().get();
        if !crate::coverage_closure::coverage_patch_needs_repair(
            uncovered_area_mm2,
            patch.cut_ambiguous(),
            patch.assessment_failed(),
        ) {
            continue;
        }
        let key = crate::coverage_closure::BlockingPatchKey {
            island_id: patch.island_id(),
            component_id: patch.component_id(),
            representative_face_id: patch.representative_face_id(),
            sorted_face_ids: patch.sorted_face_ids().to_vec(),
            uncovered_area_mm2_bits: uncovered_area_mm2.to_bits(),
            worst_gap: patch.worst_gap().into(),
        };
        if crate::coverage_closure::coverage_patch_blocks_verdict(
            uncovered_area_mm2,
            patch.cut_ambiguous(),
            patch.assessment_failed(),
            censor_area_mm2,
        ) {
            blocking.push(key.clone());
        }
        repair_targets.push(key);
    }
    Ok(MeasuredState {
        blocking,
        repair_targets,
        uncovered_samples: measurement.uncovered_sample_indices().to_vec(),
        objective: measurement.objective(),
        uncensored_uncovered_mm2: measurement.uncensored_uncovered_mm2(),
    })
}

/// Either a fully lowered run, or the context handed back with the phase and
/// detail that refused it.
enum ChainOutcome<'a> {
    Lowered(Box<LoweredSupportRun<'a>>),
    Refused(Box<ChainRefusal<'a>>),
    /// **Stage B — the third arm.** The pass did not fail; it learned something
    /// and stopped. It produced no lowered run, therefore no sealed authority,
    /// therefore nothing that can reach an artifact — which is what makes "a
    /// pass that was abandoned may never reach output" true by construction
    /// rather than by a check (design §1.3 obligation 2).
    ///
    /// Produced ONLY by the routing and bracing stages, only when the abandon
    /// rule is satisfied, and only when the CALLER SUPPLIED A STEERING SET. The
    /// stage macro is untouched: the two stages that can abandon are written
    /// out longhand beside it, so no other stage can grow this arm by accident.
    ///
    /// **That last condition is what keeps the trial path bit-identical, by
    /// construction rather than by a downstream match.** The retry wraps the
    /// ATTEMPT-0 chain call only (design §4 Stage B piece 4); the repair loop's
    /// trial call passes `None`, so a trial's disturbing brace refusal is still
    /// one `ChainOutcome::Refused`, one ledger row and the next candidate —
    /// which is the C3 precedence rule.
    Abandoned(Box<ChainAbandon<'a>>),
}

/// The context handed back, with what the abandoned pass learned.
struct ChainAbandon<'a> {
    context: SupportRunContext<'a>,
    abandon: crate::legalization::LegalizationAbandon,
}

/// The context handed back, with the stage that refused the plan.
/// Boxed for the same reason the lowered run is: both payloads carry the one
/// mesh/topology/`NativeSurface` context, so an unboxed enum would size every
/// stack slot to the larger of them.
struct ChainRefusal<'a> {
    context: SupportRunContext<'a>,
    stop: RefusedStage,
}

/// Which stage refused a plan, and what it said — separated from the context it
/// travels with so the two things the controller DOES with it can be proved.
///
/// **FU-truthful-errors, ported here at §G.1 stage 4.** The retired
/// `repair_plan_or_stage_failure` used to discard `error` outright whenever
/// `attempt == 0`, returning a bare `SupportRunError::Failed(phase)` that could
/// not say what actually failed — exactly the attempt most likely to hold the
/// real bug, because every other attempt already kept its error. The two
/// projections below are the successors to that function's two branches, and
/// `a_stage_refusal_keeps_its_detail_at_attempt_zero_and_on_a_trial` pins that
/// both carry the same detail.
struct RefusedStage {
    phase: SupportRunPhase,
    detail: String,
}

impl RefusedStage {
    /// **Attempt 0 runs the model's OWN plan**, so a stage failure there is the
    /// model's and not this controller's. It is not a rollback, it is not a
    /// coverage refusal, and it is not eligible to be one (§A.6, §H.7).
    fn into_attempt_zero_failure(self) -> SupportRunError {
        SupportRunError::Failed {
            phase: self.phase,
            detail: self.detail,
        }
    }

    /// **A TRIAL's plan carries a contact this controller appended**, so its
    /// stage failure is the controller's own doing — and under §A.5 it is one
    /// ledger row and the next candidate, not the end of the model. Task #47's
    /// Giant Bat, which loses a whole model to one arm's
    /// `Brace(ClearanceViolation(1))`, lands exactly here.
    fn into_trial_rollback(self) -> crate::coverage_repair::RepairStepOutcome {
        crate::coverage_repair::RepairStepOutcome::StageRefusal {
            phase: self.phase,
            detail: self.detail,
        }
    }
}

/// Drive one plan through the whole stage chain, and hand the context back on
/// every failure path.
///
/// This is what the SPEC-A4 §A.3 seam bought. Before it, a stage that failed
/// CONSUMED the one mesh/topology/`NativeSurface` context the machine is
/// allowed to hold and the model died — which is why task #47's Giant Bat loses
/// an entire model to one arm's `Brace(ClearanceViolation(1))`. Here the same
/// failure returns the context, the trial rolls back, and the search goes on.
/// `announce` carries the judge's stage stream and NOTHING else: when it is
/// `Some`, each stage prints one line before it starts. Attempt 0 passes a
/// clock because that is the silence an operator sits through — minutes of it
/// on a big model, with the repair loop's own entry line still to come. Every
/// repair TRIAL passes `None`, because six lines times five hundred trials is
/// not visibility, it is noise that buries the progress line R10 paid for.
///
/// It is a parameter rather than a flag read from somewhere, so the choice is
/// visible at both call sites and no branch below can reach it.
fn run_stage_chain<'a>(
    context: SupportRunContext<'a>,
    plan: SpecializedContactPlan,
    announce: Option<&crate::coverage_repair::StageClock>,
    // **Stage B.** `None` on the trial path, by design (§4 Stage B piece 4).
    steering: Option<&crate::legalization::LegalizationSteering>,
) -> ChainOutcome<'a> {
    macro_rules! stage {
        ($expr:expr, $stage:literal, $phase:expr) => {{
            if let Some(clock) = announce {
                clock.entering($stage);
            }
            match $expr {
                Ok(value) => value,
                Err((context, error)) => {
                    note_stage_failure($stage, &error);
                    return ChainOutcome::Refused(Box::new(ChainRefusal {
                        context,
                        stop: RefusedStage {
                            phase: $phase,
                            detail: format!("{error:?}"),
                        },
                    }));
                }
            }
        }};
    }
    let prepared = stage!(
        prepared_run_for_plan(context, plan),
        "Preparation",
        SupportRunPhase::Preparation
    );
    // Routing and Brace are the two stages that can ABANDON, so they are
    // written out beside the macro rather than through it. The macro at the top
    // of this function is deliberately unchanged: an abandoned pass is an
    // outcome only these two stages may produce, and a stage that grew it by
    // accident would be a stage that silently stopped producing artifacts.
    if let Some(clock) = announce {
        clock.entering("Routing");
    }
    let routed = match prepared.route_initial_cco(steering) {
        Ok(value) => value,
        Err((context, error)) => {
            note_stage_failure("Routing", &error);
            return match error {
                SupportRoutingError::SteeringSpent { terminal } if steering.is_some() => {
                    ChainOutcome::Abandoned(Box::new(ChainAbandon {
                        context,
                        abandon: crate::legalization::LegalizationAbandon::SteeringSpent {
                            terminal_id: terminal,
                        },
                    }))
                }
                error => ChainOutcome::Refused(Box::new(ChainRefusal {
                    context,
                    stop: RefusedStage {
                        phase: SupportRunPhase::Routing,
                        detail: format!("{error:?}"),
                    },
                })),
            };
        }
    };
    let rewired = stage!(routed.rewire(), "Rewire", SupportRunPhase::Rewire);
    if let Some(clock) = announce {
        clock.entering("Brace");
    }
    let braced = match rewired.brace() {
        Ok(value) => value,
        Err((context, error)) => {
            note_stage_failure("Brace", &error);
            return match error {
                SupportBraceError::SealedRoutedTerminalCondemned(_class, abandon)
                    if steering.is_some() =>
                {
                    ChainOutcome::Abandoned(Box::new(ChainAbandon {
                        context,
                        abandon: *abandon,
                    }))
                }
                error => ChainOutcome::Refused(Box::new(ChainRefusal {
                    context,
                    stop: RefusedStage {
                        phase: SupportRunPhase::Brace,
                        detail: format!("{error:?}"),
                    },
                })),
            };
        }
    };
    let structured = stage!(braced.structure(), "Structure", SupportRunPhase::Structure);
    let lowered = stage!(structured.lower(), "Lowering", SupportRunPhase::Lowering);
    ChainOutcome::Lowered(Box::new(lowered))
}

/// **Stage B — what the legalization retry spent, and the one rule that stops
/// it.**
///
/// Every field is printed on every path and read by no branch, per the no-cap
/// doctrine: the mechanism is uncapped, so the only honest control on it is
/// that an operator can see exactly what it did.
#[derive(Clone, Debug, Default)]
struct LegalizationTally {
    /// Passes through the attempt-0 chain, including the first and including
    /// the one that lowered.
    passes: usize,
    /// Rounds opened. A voided round is counted here as well as in
    /// `voided_rounds`: it was opened, it was paid for, and it earned nothing.
    confirmation_rounds: usize,
    voided_rounds: usize,
    /// Arms a round has driven, across all rounds — **including arms measured
    /// inside a round that was later voided.** It is a cost, not a credit: the
    /// passes were spent whether or not the round they belonged to survived to
    /// earn anything. What a retirement rests on is the round's own sealed
    /// rows, never this counter.
    round_arms_measured: usize,
    /// **Stage D.** One row per ABANDONED pass, in pass order, and one row per
    /// round opened, in open order. They are the same facts the lines above
    /// stream, retained so they can reach the evidence sidecar instead of
    /// living only in a log an operator has to have kept.
    disturbances: Vec<crate::coverage_repair::LegalizationDisturbance>,
    rounds: Vec<OpenRoundRecord>,
    /// Index into `rounds` of the round that is open right now, if any. One
    /// round at a time is §1.3's own rule — the steering set is frozen for a
    /// round's duration — so this is an index and not a set.
    open_round: Option<usize>,
}

/// A round's Stage D row while it is still being driven. `outcome` is unknown
/// until the round is voided or a pass lowers, and an `Option` is the honest
/// way to hold "not decided yet".
#[derive(Clone, Debug)]
struct OpenRoundRecord {
    terminal_id: u32,
    opened_at_pass: usize,
    no_goods_at_open: usize,
    arms_measured: usize,
    outcome: Option<crate::coverage_repair::ConfirmationRoundOutcome>,
}

impl LegalizationTally {
    /// **Stage D — the loop's own record, projected onto the wire.**
    ///
    /// `unrouted_no_winner` is read off the LOWERED run's sealed transcript and
    /// is the only outside input: it decides nothing, it names where a round's
    /// terminal stood when the run lowered. A round that was voided already
    /// carries its outcome and this cannot overwrite it.
    fn disclose(
        &self,
        steering: &crate::legalization::LegalizationSteering,
        unrouted_no_winner: &BTreeSet<u32>,
    ) -> crate::coverage_repair::LegalizationDisclosure {
        use crate::coverage_repair::ConfirmationRoundOutcome;
        crate::coverage_repair::LegalizationDisclosure {
            passes: self.passes,
            no_goods: steering.len(),
            no_goods_by_terminal: steering.counts_by_terminal(),
            confirmation_rounds: self.confirmation_rounds,
            voided_rounds: self.voided_rounds,
            round_arms_measured: self.round_arms_measured,
            disturbances: self.disturbances.clone(),
            rounds: self
                .rounds
                .iter()
                .map(
                    |round| crate::coverage_repair::ConfirmationRoundDisclosure {
                        terminal_id: round.terminal_id,
                        opened_at_pass: round.opened_at_pass,
                        arms_measured: round.arms_measured,
                        no_goods_at_open: round.no_goods_at_open,
                        outcome: round.outcome.unwrap_or(
                            if unrouted_no_winner.contains(&round.terminal_id) {
                                ConfirmationRoundOutcome::TerminalUnroutedAtLowering
                            } else {
                                ConfirmationRoundOutcome::TerminalRoutedAtLowering
                            },
                        ),
                    },
                )
                .collect(),
        }
    }

    /// Mark the open round voided and close its Stage D row.
    fn void_open_round(&mut self) {
        self.voided_rounds += 1;
        if let Some(index) = self.open_round.take() {
            self.rounds[index].outcome =
                Some(crate::coverage_repair::ConfirmationRoundOutcome::Voided);
        }
    }

    fn render(&self, steering: &crate::legalization::LegalizationSteering) -> String {
        let per_terminal = steering
            .counts_by_terminal()
            .into_iter()
            .map(|(terminal, count)| format!("t{terminal}={count}"))
            .collect::<Vec<_>>();
        format!(
            "passes={} no_goods={} per_terminal=[{}] confirmation_rounds={} voided_rounds={} \
             round_arms_measured={}",
            self.passes,
            steering.len(),
            if per_terminal.is_empty() {
                "none".to_owned()
            } else {
                per_terminal.join(",")
            },
            self.confirmation_rounds,
            self.voided_rounds,
            self.round_arms_measured,
        )
    }

    /// Fold one abandoned pass into the steering, or refuse by name.
    ///
    /// **The abandon rule, and it is the weak and correct one** (§1.3
    /// obligation 2): *every abandoned pass added at least one no-good that was
    /// not already in the set.* It is what makes the loop terminate — the
    /// context universe is finite because the set never feeds a context digest,
    /// and the set grows strictly, so the passes are bounded by the number of
    /// distinct `(terminal, prefix, arm)` triples the run can produce.
    ///
    /// A pass that would abandon while learning nothing new **must refuse by
    /// name rather than loop.** That is design §4 Stage B revert proof 3, and
    /// it is a real refusal rather than a cap: it fires on a pass that made no
    /// progress, never on a pass that made some.
    ///
    /// **The rule is stated once and holds on EVERY arm.** Progress is one of
    /// three things and nothing else: a no-good entered the set, a round
    /// advanced its rank, or a round OPENED at a context that had none. Voiding
    /// a round is not progress — it destroys work rather than making any — so a
    /// pass whose only effect was a void refuses here as well. Both arms below
    /// are written against that single sentence, because the first version of
    /// this function enforced it on the `Condemned` arm alone and left the
    /// `SteeringSpent` arm free to void and re-open a round forever without the
    /// set ever growing.
    fn apply(
        &mut self,
        steering: &mut crate::legalization::LegalizationSteering,
        abandon: crate::legalization::LegalizationAbandon,
    ) -> Result<(), SupportRunError> {
        match abandon {
            crate::legalization::LegalizationAbandon::Condemned {
                learned,
                own_arm_terminals,
                cascade_victims,
                measured,
            } => {
                let round_terminal = steering.round().map(|round| round.terminal_id);
                // **A COUNT and not a flag**, because Stage D's row reports how
                // many no-goods this pass added that were not already in the
                // set. The abandon rule reads the same value the flag carried —
                // `learned_count > 0` is exactly the old `grew` — so the rule
                // is unchanged and the disclosure is not a second tally.
                let mut learned_count = 0_usize;
                let mut round_advanced = false;
                let mut voided = false;
                for (no_good, physics) in learned.into_iter().zip(measured) {
                    if round_terminal == Some(no_good.terminal_id()) {
                        // **The round's own arm, measured.** §1.3: refusals
                        // measured during the round are recorded AGAINST THE
                        // ROUND and not added to the steering set. The set is
                        // frozen for the round's duration, which is what makes
                        // the round replayable and what stops the round from
                        // feeding itself.
                        let Some(round) = steering.round_mut() else {
                            unreachable!("round_terminal was just read from the same round");
                        };
                        round
                            .measured
                            .push(physics.at_rank(round.arm_rank, no_good.arm()));
                        round.arm_rank += 1;
                        self.round_arms_measured += 1;
                        // **Stage D — the round LENGTH, read off the round's own
                        // rows.** Not a second tally: the round is the source,
                        // and this row carries what it holds so the record
                        // survives the round being dropped.
                        let measured_now = round.measured.len();
                        if let Some(index) = self.open_round {
                            self.rounds[index].arms_measured = measured_now;
                        }
                        round_advanced = true;
                        crate::coverage_repair::emit_run_line(&format!(
                            "legalize: round t{} measured {} — advancing to rank {}",
                            round.terminal_id,
                            round
                                .measured
                                .last()
                                .map_or_else(String::new, crate::legalization::RoundArm::render),
                            round.arm_rank,
                        ));
                        continue;
                    }
                    // Somebody ELSE'S arm was condemned. If a round is open,
                    // the world moved for a reason other than that terminal's
                    // own choice, so the round is VOID: §1.3. It is paid for,
                    // it earned nothing, and it is re-entered later against the
                    // new set.
                    voided |= round_terminal.is_some();
                    if steering.learn(no_good) {
                        learned_count += 1;
                    }
                }
                // **The abandon rule, and a round advancing IS progress.** A
                // round pass measures an arm nobody had measured in this world,
                // and its rank strictly increases, so the loop is no less
                // bounded than a steering pass that grows the set. What is
                // forbidden is a pass that did neither.
                //
                // **Voiding a round is NOT the third kind of progress**, and the
                // rule is evaluated BEFORE the void below for that reason. A
                // void destroys work rather than making any: it hands the next
                // pass the same steering set and no open round, which is the
                // world this pass already started from. §1.3 lets the round be
                // re-entered *"after the set grew"*, so a void is only ever
                // finite when it travels with a no-good the set did not have.
                // Reaching this with `voided` true and `learned_count` zero
                // means a terminal's own arm was condemned while carrying the
                // exact `(terminal, prefix, arm)` triple that should have
                // suppressed it — a defect in the steering channel, not a
                // property of the model, and the same class as the spent-during
                // -its-own-round refusal below.
                if learned_count == 0 && !round_advanced {
                    return Err(SupportRunError::Failed {
                        phase: SupportRunPhase::Brace,
                        detail: format!(
                            "legalization pass {} abandoned without learning a new no-good \
                             (own-arm terminals {own_arm_terminals:?}, cascade victims \
                             {cascade_victims:?}, would have voided an open round: {voided}); \
                             the abandon rule forbids another pass because this one made no \
                             progress",
                            self.passes
                        ),
                    });
                }
                if voided {
                    let round = steering
                        .round()
                        .map(|round| (round.terminal_id, round.arm_rank, round.measured.len()));
                    self.void_open_round();
                    steering.set_round(None);
                    crate::coverage_repair::emit_run_line(&format!(
                        "legalize: round VOID {round:?} — another terminal's own arm was \
                         condemned during it; it retires nothing and is re-entered later",
                    ));
                }
                // **Stage D — which terminal was disturbed, per pass.** Written
                // AFTER the abandon rule has let the pass continue, so a pass
                // that refused by name above leaves no row for a pass that never
                // continued.
                self.disturbances
                    .push(crate::coverage_repair::LegalizationDisturbance {
                        pass: self.passes,
                        own_arm_terminals,
                        cascade_victims,
                        no_goods_learned: learned_count,
                        round_arm_measured: round_advanced,
                        voided_a_round: voided,
                        steering_spent_terminal: None,
                    });
                Ok(())
            }
            crate::legalization::LegalizationAbandon::SteeringSpent { terminal_id } => {
                // **The only thing allowed to earn a retirement opens here.**
                // §1.3: a terminal with a no-good for every arm it can still
                // generate does NOT retire. It enters a round — the steering
                // set held fixed, its remaining arms driven one per pass in the
                // tournament's own order, and the retirement earned only when
                // the last one has been measured in the world it is deciding
                // about.
                if let Some(open) = steering.round() {
                    if open.terminal_id == terminal_id {
                        // The round is open for this very terminal and it still
                        // reports itself spent. A round forces a rank and
                        // suppresses nothing, so the tournament cannot have
                        // reached the spent branch — this is a defect, not a
                        // physics outcome, and it refuses rather than looping.
                        return Err(SupportRunError::Failed {
                            phase: SupportRunPhase::Routing,
                            detail: format!(
                                "terminal {terminal_id} reported its steering spent while its \
                                 own confirmation round was open at rank {}; a round \
                                 suppresses nothing, so this is a defect in the steering \
                                 channel and not a property of the model",
                                open.arm_rank
                            ),
                        });
                    }
                    // **A second terminal is spent while another's round runs,
                    // and this pass made NO progress — so it refuses by name.**
                    //
                    // The abandon rule is the same one the `Condemned` arm
                    // applies, stated once and applied on every arm: a pass
                    // that neither learned a no-good nor advanced a round may
                    // not run another. A spent report learns nothing — it
                    // carries no arm and no measurement — so displacing an open
                    // round here would void the work already paid for and
                    // re-open at rank 0 with the set unchanged. Design §1.3
                    // admits exactly two ways into a round: *"a context whose
                    // steering set is full, or re-entered after the set grew,
                    // both of which are finite events"*. A displacement is
                    // neither, and it is a true fixed point rather than a slow
                    // path: the earlier terminal in the cadence is forced by
                    // its own round and routes, the later one reports itself
                    // spent, and the next pass repeats it in the same world.
                    //
                    // This refuses BEFORE it touches anything — no VOID line,
                    // no counter, no round row, no disturbance row — for the
                    // reason the `Condemned` arm states above: a pass that
                    // refuses by name leaves no row for a pass that never
                    // continued.
                    return Err(SupportRunError::Failed {
                        phase: SupportRunPhase::Routing,
                        detail: format!(
                            "legalization pass {} would displace terminal {}'s confirmation \
                             round at rank {} because terminal {terminal_id} also reported its \
                             steering spent; a spent report learns no no-good and advances no \
                             round, so the abandon rule forbids another pass — the two \
                             terminals hold each other and the set can never grow",
                            self.passes, open.terminal_id, open.arm_rank
                        ),
                    });
                }
                self.confirmation_rounds += 1;
                // **Stage D — the round row opens with the round.** The
                // no-good count is read HERE and not at the end because §1.3
                // freezes the set for the round's duration: this is the set the
                // round measured against.
                self.open_round = Some(self.rounds.len());
                self.rounds.push(OpenRoundRecord {
                    terminal_id,
                    opened_at_pass: self.passes,
                    no_goods_at_open: steering.len(),
                    arms_measured: 0,
                    outcome: None,
                });
                self.disturbances
                    .push(crate::coverage_repair::LegalizationDisturbance {
                        pass: self.passes,
                        own_arm_terminals: Vec::new(),
                        cascade_victims: Vec::new(),
                        no_goods_learned: 0,
                        round_arm_measured: false,
                        // A spent report never voids a round: the only pass
                        // that could have is the displacement above, and it
                        // refuses instead of continuing.
                        voided_a_round: false,
                        steering_spent_terminal: Some(terminal_id),
                    });
                steering.set_round(Some(crate::legalization::ConfirmationRound {
                    terminal_id,
                    arm_rank: 0,
                    measured: Vec::new(),
                }));
                crate::coverage_repair::emit_run_line(&format!(
                    "legalize: round OPEN t{terminal_id} at rank 0 — re-measuring every arm \
                     under the frozen set, one per pass, in the tournament's own order; \
                     UNCAPPED",
                ));
                Ok(())
            }
        }
    }
}

/// Everything one route accumulated across the whole search, so §E.1's
/// per-route outcome is a read of counters rather than an inference.
#[derive(Clone, Debug, Default)]
struct RouteSearchRecord {
    rows_tried: usize,
    best_marginal_area_mm2: f64,
    /// Trials whose OWN appended contact ended `UnroutedNoWinner` with every
    /// feasible arm refused by the clearance standoff (task #51).
    clearance_refused_terminals: u32,
    /// Trials whose OWN appended contact was condemned at quarantine with a
    /// measured stub-axis re-entry depth (task #52). Depths in mm, in trial
    /// order.
    stub_reentry_depths_mm: Vec<f64>,
}

/// The registered numbers §E.1's two defect classes are measured AGAINST.
///
/// Both are read from the same registry every other reader reads; neither is
/// restated, moved or interpreted here. `standoff_mm` is the support-to-model
/// clearance standoff the CCO's own clearance survival uses; `allowance_mm` is
/// the `min_illegal_len_mm` the initial build compares a stub's remaining
/// inside-axis overlap against before condemning it.
#[derive(Clone, Copy, Debug)]
struct BlockedCauseScales {
    standoff_mm: f64,
    allowance_mm: f64,
}

impl BlockedCauseScales {
    fn read() -> Result<Self, SupportRunError> {
        let constants = m0_constants().map_err(|error| SupportRunError::Failed {
            phase: SupportRunPhase::Preparation,
            detail: format!("binding the registered blocked-cause scales failed: {error}"),
        })?;
        let placeholders = constants.placeholders();
        Ok(Self {
            standoff_mm: placeholders.standoff_mm().diagnostic().get(),
            allowance_mm: placeholders.min_illegal_len_mm().diagnostic().get(),
        })
    }
}

/// **SPEC-A4 §A — the one-winner transactional coverage-repair controller.**
///
/// One repair step is ONE repair-pool row appended to the accepted plan, the
/// resulting child plan taken all the way through
/// `prepared_run_for_plan → route_initial_cco → rewire → brace → structure →
/// lower`, and one routed coverage measurement taken against the lowered
/// result at the registered gate request. The step then either COMMITS (the
/// child becomes the accepted plan) or ROLLS BACK (the child is dropped and the
/// row is ledgered). That is SPEC §6's "full construct, rewire, brace/size, and
/// verify transaction".
///
/// **This is the ONE door to the sealed run — §G.1 stage 4 landed, and the
/// batch loop that used to own this name is deleted.** It appended a whole
/// nomination batch per attempt, and the three things that cost are why the
/// spec asks for one winner at a time:
///
/// * **Nothing was attributable.** An attempt that appended 266 rows and moved
///   the area by 0.4 mm² could not say which row did it. Here the plan changes
///   by exactly one contact, so the before/after difference IS that contact's
///   contribution — no inference and no apportionment (§D).
/// * **Nothing was reversible.** `plan = plan.append…(rows)` overwrote the
///   parent binding on the same line. Here the plan is a persistent value and
///   the child is a DERIVED value, so rolling back is "do not rebind `plan`"
///   (§A.5). There is no undo log and nothing to rebuild.
/// * **Nothing was protected.** A row that made a noncoverage gate worse was
///   kept. Here clause 2 rejects it and the ledger names the component.
///
/// # What bounds this loop, and what does not
///
/// `coverage_repair_max_attempts` (registered value 8) bounds **COMMITTED steps
/// only** — Craig's ruling R6. **There is NO failed-tries cap** — ruling R10:
/// the loop gives up only when no candidate can physically help, which is pool
/// exhaustion plus §B.3's zero-score guard. A counter may never decide an
/// outcome; that is the physics-first doctrine, and a trial ceiling is exactly
/// a counter deciding an outcome. What bounds the COST instead is rule N4
/// (ruling R13) attacking the measured mechanism — 63 near-identical seats
/// inside a 1.04 mm box, each costing a full pipeline pass to re-prove — and
/// [`crate::coverage_repair::RepairRunReport`] making the residual loud instead
/// of hidden (§A.8).
///
/// # Attempt 0 is not a trial
///
/// Attempt 0 runs the model's own plan, and its stage failures stay
/// [`SupportRunError::Failed`], exactly as they are today and exactly as
/// `a_stage_error_is_not_discarded_at_attempt_zero` pins. Only a plan this
/// controller extended can produce a rollback.
///
/// # Errors
///
/// [`SupportRunError::Failed`] for an attempt-0 stage failure or a bad
/// resource cap; [`SupportRunError::CoverageClosure`] when the search stops
/// with blocking patches left, carrying the measured stop reason (§B.7), the
/// per-route outcomes (§E.1) and the §A.8 report.
#[allow(
    clippy::too_many_lines,
    reason = "SPEC-A4 §A.7's loop is one decision procedure; splitting it would put \
              the accept/rollback pairing behind a call boundary, which is the exact \
              seam a reviewer has to see whole"
)]
pub fn run_support<'a>(
    hollowed: HollowedModel,
    request: SupportRequest,
    input: SupportPreparationInput<'a>,
) -> Result<FinalSupportAuthority<'a>, SupportRunError> {
    if !input.validate() {
        return Err(SupportRunError::InvalidResourceCaps);
    }
    let min_z = hollowed
        .mesh()
        .verts()
        .iter()
        .map(|point| point[2])
        .reduce(f64::min)
        .ok_or(SupportRunError::InvalidDerivedFrame)?;
    let plate_z_mm = Finite::new(min_z - request.lift_mm.get())
        .map_err(|_| SupportRunError::InvalidDerivedFrame)?;
    let perception_config = SupportPerceptionConfig::current(
        plate_z_mm,
        input.column_pitch_mm,
        input.surface_grid_pitch_mm,
        input.max_grid_bytes,
        Some(input.delicacy_by_face),
    )
    .map_err(|error| SupportRunError::Failed {
        phase: SupportRunPhase::Preparation,
        detail: format!("binding registered perception thresholds failed: {error}"),
    })?;
    let selection_config = ContactSelectionConfig::current(
        Some(input.delicacy_by_face),
        input.coverage_samples_max_virtual_cells,
        input.coverage_score_matrix_max_entries,
        input.max_grid_bytes,
    )
    .map_err(|error| SupportRunError::Failed {
        phase: SupportRunPhase::Preparation,
        detail: format!("binding registered contact-selection policy failed: {error}"),
    })?;
    // R6 and R11: the registered value does not move and is not re-read
    // differently. Only its MEANING changes — it counts committed steps.
    let commit_budget = selection_config.coverage_repair_max_attempts();
    let matrix_budget = selection_config.coverage_score_matrix_max_entries;
    let scales = BlockedCauseScales::read()?;
    let authority = SupportPreparationAuthority::new(
        input.layers,
        perception_config,
        input.candidate_config,
        input.reservations,
        input.perception_limits,
        selection_config,
        input.specialized_limits,
        input.construction_limits,
        input.brace_penetration_limits,
        input.raft_limits,
        input.max_primitives,
        input.max_primitive_bytes,
    );

    let coverage_request =
        crate::coverage_closure::registered_coverage_request().map_err(|error| {
            SupportRunError::Failed {
                phase: SupportRunPhase::Preparation,
                detail: format!("binding the registered coverage-closure request failed: {error}"),
            }
        })?;
    let radius_mm = coverage_request.radius_mm().get();
    let censor_area_mm2 = coverage_request.patch_area_floor_mm2().get();

    let (mut context, plan) =
        prepare_support_context(hollowed, request, authority).map_err(|error| {
            note_stage_failure("Preparation", &error);
            SupportRunError::Failed {
                phase: SupportRunPhase::Preparation,
                detail: error.to_string(),
            }
        })?;

    // ---------------------------------------------------------------- attempt 0
    //
    // The judge's streaming-silence note. Attempt 0 is the long quiet stretch:
    // Preparation, Routing, Rewire, Brace, Structure and Lowering all run here
    // before the repair loop's own entry line can exist, and on a big model
    // that is four to five minutes of nothing. Every line below is print-only.
    let stage_clock = crate::coverage_repair::StageClock::start("support");

    // ------------------------------------------- Stage B: the legalization retry
    //
    // Design §4 Stage B piece 4. ONE loop, wrapping the ATTEMPT-0 chain call and
    // not the trial call — the trial path keeps today's
    // `restore_prepared_digests` / `into_trial_rollback` behaviour exactly.
    //
    // **The loop holds the no-good set**, which is precisely where the
    // finiteness hazard is: if the set reached any digest in the context tuple,
    // every context would be fresh on every pass, every no-good would miss
    // forever, and each abandoned pass would satisfy the abandon rule with a
    // genuinely new triple while making no progress. It is a local, borrowed
    // out and never stored — and `a_growing_no_good_set_moves_no_context_digest`
    // is the standing proof.
    //
    // **There is no cap here and there is none below.** Craig's authorization
    // item 4 is that the confirmation round is uncapped in engine semantics and
    // loudly reported. The only ceiling anywhere is the test/eval harness's
    // wall clock, which lives outside the engine and records "stopped at T with
    // state X" rather than passing off a truncated search as an answer. Every
    // pass streams its entry line before it starts, so a run killed inside pass
    // 40 still leaves a log that says which pass, which terminal, and how many
    // no-goods.
    let mut steering = crate::legalization::LegalizationSteering::default();
    let mut legalization = LegalizationTally::default();
    // §A.5, finding S4, applied to this loop for the same reason the trial loop
    // applies it: `prepared_run_for_plan` stamps five digest fields on the
    // context, so a pass abandoned mid-chain hands back a context stamped by
    // that pass. The next pass re-stamps them, but anything read BETWEEN passes
    // would be reading a plan that does not exist.
    let prepared_digests = context.prepared_digests();
    let attempt_zero = loop {
        legalization.passes += 1;
        let pass_started = std::time::Instant::now();
        crate::coverage_repair::emit_run_line(&format!(
            "legalize: pass {} entering no_goods={} round={}",
            legalization.passes,
            steering.len(),
            steering.round().map_or_else(
                || "none".to_owned(),
                |round| format!("t{}@rank{}", round.terminal_id, round.arm_rank),
            ),
        ));
        match run_stage_chain(context, plan.clone(), Some(&stage_clock), Some(&steering)) {
            ChainOutcome::Lowered(lowered) => {
                crate::coverage_repair::emit_run_line(&format!(
                    "legalize: pass {} LOWERED in {} ms — {}",
                    legalization.passes,
                    elapsed_ms(pass_started),
                    legalization.render(&steering),
                ));
                break lowered;
            }
            // §A.6 and §H.7: attempt 0 runs the model's OWN plan, so its stage
            // failure is the model's, not this controller's. It is not a
            // rollback, it is not a coverage refusal, and it is not eligible to
            // be one. An abandoned pass is not one of these — it is the arm
            // below.
            ChainOutcome::Refused(refusal) => {
                return Err(refusal.stop.into_attempt_zero_failure());
            }
            ChainOutcome::Abandoned(abandoned) => {
                let ChainAbandon {
                    context: returned,
                    abandon,
                } = *abandoned;
                context = returned;
                context.restore_prepared_digests(prepared_digests);
                crate::coverage_repair::emit_run_line(&format!(
                    "legalize: pass {} ABANDONED in {} ms — {}",
                    legalization.passes,
                    elapsed_ms(pass_started),
                    abandon.render(),
                ));
                legalization.apply(&mut steering, abandon)?;
            }
        }
    };
    crate::coverage_repair::emit_run_line(&format!("legalize: {}", legalization.render(&steering)));
    // **Stage D — the record, taken once, at the one point every later path
    // shares.** The loop is over, so the tally is final; the run has lowered,
    // so its sealed transcript can say where a round's terminal ended. Every
    // `RepairRunReport` this function builds below carries this same value, so
    // an emitted artifact and a coverage refusal disclose the same rescue.
    // Disclosure only: no branch below reads a field of it.
    let legalization_disclosure =
        legalization.disclose(&steering, &attempt_zero.unrouted_no_winner_terminals());
    stage_clock.entering("CoverageMeasurement");
    let mut measured = match measure_for_controller(&attempt_zero, coverage_request) {
        Ok(measured) => measured,
        Err((_phase, detail)) => {
            // §A.8 says every run reports on EVERY path. This one refuses
            // before a single trial, so the report is all zeros — and printing
            // a line of zeros is the point: an operator sees that the repair
            // loop was never entered rather than seeing nothing at all.
            let report = crate::coverage_repair::RepairRunReport {
                legalization: legalization_disclosure,
                ..Default::default()
            };
            report.log_summary();
            return Err(SupportRunError::CoverageClosure(Box::new(
                crate::coverage_closure::CoverageClosureRefusal {
                    attempts: 0,
                    reason: crate::coverage_closure::CoverageClosureReason::MeasurementRefused(
                        detail,
                    ),
                    islands: Vec::new(),
                    steps: Vec::new(),
                    repair: Some(report),
                },
            )));
        }
    };

    if measured.blocking.is_empty() {
        // The zero-cost path: a model whose baseline layout already covers
        // never enters a transaction, so no rule in §A, §B, §C, §D or §E can
        // reach it and its digest is bit-identical to the pre-controller
        // engine's by construction rather than by luck.
        let report = crate::coverage_repair::RepairRunReport {
            legalization: legalization_disclosure,
            ..Default::default()
        };
        report.log_summary();
        return Ok(seal_authority(
            *attempt_zero,
            &measured.repair_targets,
            report,
            Vec::new(),
        ));
    }

    // ------------------------------------------------------------- enter repair
    let gates_at_attempt_zero = attempt_zero.noncoverage_vector();
    let ownerless_at_attempt_zero = attempt_zero.ownerless_face_count();
    // The plan carried forward is the ACCEPTED one — task #49's reduced child
    // when anything was stripped. Carrying the pre-strip plan would re-admit
    // contacts SPEC §6 removes before rewire, bracing and repartition.
    let (mut plan, mut context) = attempt_zero.into_plan_and_context();

    let mut accepted_objective = measured.objective;
    let mut accepted_uncensored_mm2 = measured.uncensored_uncovered_mm2;
    let mut accepted_gates = gates_at_attempt_zero;
    let mut accepted_ownerless = ownerless_at_attempt_zero;
    let mut committed = 0_usize;

    let mut ledger = RepairLedger::new(radius_mm, legalization_disclosure);
    // Rule N3's serve-set memory: the route's own uncovered area, by bits, at
    // the moment it was last given a trial. A commit moves the area, which puts
    // the route back in the serve set — that is "since its own measured
    // uncovered area last changed", implemented rather than approximated.
    let mut served: BTreeMap<(u32, u32), u64> = BTreeMap::new();
    let mut route_records: BTreeMap<(u32, u32), RouteSearchRecord> = BTreeMap::new();
    let mut tally: BTreeMap<(u32, u32), crate::coverage_closure::KeyNomination> = BTreeMap::new();

    let mut candidates = rank_or_refuse(&plan, &measured, radius_mm, matrix_budget, &mut ledger)?;
    stamp_tally(&mut tally, &candidates);

    // ---------------------------------------------------- task #67: be loud DURING
    //
    // The measured case for this is one line of a real run log: `zombie-3-a`
    // spent **412 055 ms across 20 trials** and printed NOTHING until it
    // finished (`…/scratchpad/a4-wire-out/run-candidate.log`). Ruling R10 paid
    // for the removal of the failed-tries cap with visibility, and visibility
    // that arrives only after the seven minutes are over is not the thing R10
    // bought.
    //
    // The entry line is emitted before the first trial so that a run killed
    // during trial 1 still leaves a non-empty log — "trials=0" is a real
    // statement about a loop that has been entered, and it is the one an
    // operator needs when a model appears to hang at Routing.
    let loop_started = std::time::Instant::now();
    let mut pacer = crate::coverage_repair::ProgressPacer::started_at(loop_started);
    crate::coverage_repair::emit_run_line(
        &ledger
            .report
            .progress_line(ledger.remaining(&candidates), 0),
    );

    // **The three ends of this loop, under the owner's ruling of 2026-09-01.**
    //
    // Each of them used to be `return Err(refuse(...))` and therefore no file
    // at all. They still build exactly the same refusal — the reason, the
    // committed count, the islands, the ledger and the trial stream, all
    // unweakened — and then hand it to `seal_uncovered`, which re-derives the
    // accepted layout, re-measures what it leaves open, and seals it as an
    // artifact carrying the refusal AS A WARNING. If that cannot be done the
    // refusal is returned and the behaviour is exactly what it was.
    //
    // A macro rather than a function because every one of these ends has to
    // MOVE `context` and `plan` out of the loop, which only a `return` in the
    // loop body may do.
    macro_rules! emit_with_gap_or_refuse {
        ($reason:expr) => {{
            let refusal = refuse(
                $reason,
                committed,
                &measured,
                &candidates,
                &plan,
                &mut ledger,
                &route_records,
                &tally,
                scales,
                radius_mm,
            );
            return match refusal {
                SupportRunError::CoverageClosure(closure) => seal_uncovered(
                    closure,
                    context,
                    plan,
                    &steering,
                    coverage_request,
                    ledger.report,
                    ledger.steps,
                    &tally,
                ),
                other => Err(other),
            };
        }};
    }

    loop {
        // Paced from the top of the iteration, which is the one place every
        // path through the body reaches — including the several `continue`s a
        // rejected trial takes. Reading the ledger's own live report means the
        // streamed counts are the reported counts.
        let now = std::time::Instant::now();
        if pacer.due(ledger.report.trials, now) {
            let remaining = ledger.remaining(&candidates);
            let elapsed_ms = elapsed_ms(loop_started);
            crate::coverage_repair::emit_run_line(
                &ledger.report.progress_line(remaining, elapsed_ms),
            );
        }

        // §B.3's own quantity, measured over every row still live, not read off
        // the winner. It feeds both the stop guard below and every refusal that
        // names a remaining score, so no refusal can claim 0.00 mm² while a row
        // that covers 13.68 mm² is still on the ranking.
        let best_live_mm2 =
            crate::coverage_closure::best_live_score_mm2(&candidates, &ledger.ledgered);
        let chosen = crate::coverage_closure::choose_winner(
            &candidates,
            &ledger.ledgered,
            &served,
            censor_area_mm2,
        );
        let Some(chosen) = chosen else {
            // Every candidate on the ranking has been ledgered. Under ruling
            // R10 that is one of exactly two honest ends, and it is measured:
            // the pool really did drain.
            emit_with_gap_or_refuse!(
                if candidates.ranked.is_empty() && ledger.report.trials == 0 {
                    crate::coverage_closure::CoverageClosureReason::NothingToTry
                } else {
                    crate::coverage_closure::CoverageClosureReason::NoImprovingRepair {
                        candidates_tried: ledger.report.trials,
                        best_remaining_score_mm2: best_live_mm2,
                    }
                }
            );
        };
        let winner = candidates.ranked[chosen.index].clone();

        // §B.3's zero-score stop guard, BEFORE the transaction and not after
        // it. `eligible_repair_rows` keeps zero-score candidates eligible on
        // purpose; under batch nomination that was harmless padding, and under
        // one-winner it would spend a FULL prepare → route → rewire → brace →
        // structure → lower → measure pass to discover that a contact covering
        // nothing improves nothing.
        //
        // **It reads the BEST LIVE candidate, which is §B.3's own wording**
        // ("when the best eligible candidate's global score is 0.00 mm²"), not
        // §A.7's looser "winner". The winner reading was measured stopping
        // `make_overhang_table` after three rejections with 1,008 rows live and
        // the best of them covering 13.68 mm² — a candidate that can physically
        // help, which is what Craig's ruling 1 forbids abandoning.
        if best_live_mm2 == 0.0 {
            emit_with_gap_or_refuse!(
                crate::coverage_closure::CoverageClosureReason::NoImprovingRepair {
                    candidates_tried: ledger.report.trials,
                    best_remaining_score_mm2: best_live_mm2,
                }
            );
        }

        // The ONE counter that decides anything, and rulings R6 and R11 are
        // what make it legitimate: it bounds COMMITTED steps.
        if committed >= commit_budget {
            emit_with_gap_or_refuse!(
                crate::coverage_closure::CoverageClosureReason::CommitBudgetExhausted {
                    committed,
                    candidates_remaining: candidates
                        .ranked
                        .iter()
                        .filter(|row| !ledger.ledgered.contains(&row.id))
                        .count(),
                }
            );
        }

        // ------------------------------------------------------------ one trial
        let started = std::time::Instant::now();
        ledger.report.trials += 1;
        if chosen.diversity_discs_containing_seat > 0 {
            ledger.report.diversity_demotions_tried += 1;
        }
        let route_area_bits = candidates
            .route_uncovered_mm2
            .get(&winner.key)
            .copied()
            .unwrap_or(0.0)
            .to_bits();
        served.insert(winner.key, route_area_bits);
        {
            let record = route_records.entry(winner.key).or_default();
            record.rows_tried += 1;
            record.best_marginal_area_mm2 =
                record.best_marginal_area_mm2.max(winner.marginal_area_mm2);
        }

        let digests = context.prepared_digests();
        let before = crate::coverage_repair::RepairGateVector {
            coverage: accepted_objective,
            noncoverage: accepted_gates,
            uncensored_uncovered_mm2: accepted_uncensored_mm2,
        };
        let mut step = crate::coverage_repair::CoverageRepairStep {
            trial: ledger.report.trials,
            commit: None,
            row_id: winner.id,
            seat_mm: winner.seat_mm,
            source_island_id: winner.key.0,
            source_component_id: winner.key.1,
            scored_from: winner.scored_from.clone(),
            marginal_area_mm2: winner.marginal_area_mm2,
            chosen_by_first_trial_guarantee: chosen.by_first_trial_guarantee,
            diversity_discs_containing_seat: chosen.diversity_discs_containing_seat,
            objective_before: accepted_objective,
            objective_after: None,
            uncensored_before_mm2: accepted_uncensored_mm2,
            uncensored_after_mm2: None,
            gates_before: accepted_gates,
            gates_after: None,
            ownerless_faces_before: accepted_ownerless,
            ownerless_faces_after: None,
            own_contact_quarantined: None,
            own_contact_condemned: None,
            own_contact_no_winner: None,
            route_deltas: Vec::new(),
            outcome: crate::coverage_repair::RepairStepOutcome::Committed,
        };

        // The one row this trial appends, through the one-row form that already
        // exists and is documented byte-identical to the batch on the same id.
        // A one-winner step HAS exactly one row by construction, so this is the
        // batch, not a scalar-per-item defect.
        let child = plan
            .begin_regular_edit()
            .and_then(|edit| edit.append_regular_repair_row(winner.id));
        let child = match child {
            Ok(child) => child,
            Err(error) => {
                // Today this kills the model. Here it is one ledger row and the
                // next candidate.
                step.outcome = crate::coverage_repair::RepairStepOutcome::AppendRefused {
                    detail: format!("{error:?}"),
                };
                ledger.reject(&mut candidates, step, &winner, started);
                continue;
            }
        };
        // The child's own contact id, so the trial can read what the cadence
        // did with ITS contact rather than with the plan as a whole.
        let trial_contact_id = child.contacts().last().map(|contact| contact.contact_id());

        // `None`: a trial's six stage lines times hundreds of trials would bury
        // the progress line rather than reveal anything. The loop's own paced
        // line is the trial-scale instrument.
        match run_stage_chain(context, child, None, None) {
            // Unreachable while the trial path passes `None`, and deliberately
            // conservative if it ever stops: a trial that abandoned is rolled
            // back exactly as a trial that refused, because the retry loop
            // wraps the attempt-0 chain call and NOT this one (design §4
            // Stage B piece 4). Nothing here learns, and nothing here retries.
            ChainOutcome::Abandoned(abandon) => {
                let ChainAbandon {
                    context: returned,
                    abandon,
                } = *abandon;
                context = returned;
                context.restore_prepared_digests(digests);
                step.outcome = crate::coverage_repair::RepairStepOutcome::AppendRefused {
                    detail: format!("trial abandoned without a retry: {}", abandon.render()),
                };
                ledger.reject(&mut candidates, step, &winner, started);
            }
            ChainOutcome::Refused(refusal) => {
                let ChainRefusal {
                    context: returned,
                    stop,
                } = *refusal;
                context = returned;
                // §A.5, finding S4: `prepared_run_for_plan` stamps five digest
                // fields on the context, so a trial refused mid-chain hands
                // back a context stamped with the REJECTED CHILD's digests.
                // The retired batch loop self-healed only because its next call
                // was always `prepared_run_for_plan`; this controller reads
                // evidence BETWEEN trials, and evidence read off a stale digest
                // would describe a plan that does not exist.
                context.restore_prepared_digests(digests);
                step.outcome = stop.into_trial_rollback();
                ledger.reject(&mut candidates, step, &winner, started);
            }
            ChainOutcome::Lowered(trial) => {
                let disposition = trial_contact_id.map(|id| trial.trial_contact_disposition(id));
                step.own_contact_quarantined = disposition.map(|disposition| {
                    matches!(disposition, TrialContactDisposition::Quarantined { .. })
                });
                step.own_contact_condemned = disposition.map(|disposition| {
                    matches!(
                        disposition,
                        TrialContactDisposition::Quarantined { .. }
                            | TrialContactDisposition::UnroutedNoWinner(_)
                    )
                });
                // The VALUE the cadence sealed, carried straight onto the step.
                // Not recomputed here and not summarised on the way: this is
                // the same `OwnContactNoWinnerEvidence` the disposition holds.
                step.own_contact_no_winner =
                    disposition.and_then(|disposition| match disposition {
                        TrialContactDisposition::UnroutedNoWinner(evidence) => Some(evidence),
                        _ => None,
                    });
                record_disposition(&mut route_records, winner.key, disposition);

                let trial_measured = match measure_for_controller(&trial, coverage_request) {
                    Ok(trial_measured) => trial_measured,
                    Err((_phase, detail)) => {
                        context = trial.into_context();
                        context.restore_prepared_digests(digests);
                        step.outcome =
                            crate::coverage_repair::RepairStepOutcome::MeasurementRefused {
                                detail,
                            };
                        ledger.reject(&mut candidates, step, &winner, started);
                        continue;
                    }
                };
                let trial_gates = trial.noncoverage_vector();
                let trial_ownerless = trial.ownerless_face_count();
                let after = crate::coverage_repair::RepairGateVector {
                    coverage: trial_measured.objective,
                    noncoverage: trial_gates,
                    uncensored_uncovered_mm2: trial_measured.uncensored_uncovered_mm2,
                };
                step.objective_after = Some(trial_measured.objective);
                step.uncensored_after_mm2 = Some(trial_measured.uncensored_uncovered_mm2);
                step.gates_after = Some(trial_gates);
                step.ownerless_faces_after = Some(trial_ownerless);
                step.route_deltas = route_deltas(&measured, &trial_measured);

                let outcome = crate::coverage_repair::repair_step_outcome(&after, &before);
                if outcome != crate::coverage_repair::RepairStepOutcome::Committed {
                    context = trial.into_context();
                    context.restore_prepared_digests(digests);
                    step.outcome = outcome;
                    ledger.reject(&mut candidates, step, &winner, started);
                    continue;
                }

                // ---------------------------------------------------- COMMIT
                committed += 1;
                step.commit = Some(committed);
                step.outcome = crate::coverage_repair::RepairStepOutcome::Committed;
                ledger.commit(step, started);

                if trial_measured.blocking.is_empty() {
                    ledger.finish(Some(&candidates));
                    ledger.report.log_summary();
                    return Ok(seal_authority(
                        *trial,
                        &trial_measured.repair_targets,
                        ledger.report,
                        ledger.steps,
                    ));
                }

                accepted_objective = trial_measured.objective;
                accepted_uncensored_mm2 = trial_measured.uncensored_uncovered_mm2;
                accepted_gates = trial_gates;
                accepted_ownerless = trial_ownerless;
                measured = trial_measured;
                let (new_plan, new_context) = trial.into_plan_and_context();
                plan = new_plan;
                context = new_context;
                // A COMMIT is the only thing that moves the uncovered sample
                // set, so it is the only thing that needs the whole matrix
                // rescored. A rejection re-sorts instead (rule N4), which
                // `choose_winner` does on every call from the disc set.
                candidates =
                    rank_or_refuse(&plan, &measured, radius_mm, matrix_budget, &mut ledger)?;
                // The ranking was rebuilt from scratch, so rule N4's accumulated
                // discs must be recharged onto it. This is the ONLY sweep that
                // costs rows × discs, and it runs at most once per COMMIT, which
                // the registered bound already limits.
                ledger.recount_discs(&mut candidates);
                stamp_tally(&mut tally, &candidates);
            }
        }
    }
}

fn elapsed_ms(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Everything the search accumulates across trials that is not the accepted
/// state: which rows are spent, which neighbourhoods have already failed, the
/// trial stream, and the §A.8 counters.
///
/// One struct because a rollback touches all of it at once, and because
/// splitting it back into five parameters is what made `reject` an
/// eight-argument function.
struct RepairLedger {
    report: crate::coverage_repair::RepairRunReport,
    /// SPEC §6's "exact failed rows are excluded". Deliberately survives a
    /// rollback — that is the spec's own word — and it is filled with LEDGERED
    /// rows, which is what the batch loop's identically-named set never was.
    ledgered: BTreeSet<RepairRowId>,
    discs: crate::coverage_closure::DiversityDiscs,
    /// Every candidate rule N4 has ever deprioritized, by identity.
    ///
    /// A SET and not a counter, because §A.8 asks "how many candidates rule N4
    /// deprioritized" and one candidate can fall inside several rejected discs.
    /// Counting demotions instead of candidates would inflate the instrument
    /// that is supposed to say whether ruling R13 did any work.
    demoted: BTreeSet<RepairRowId>,
    /// **Why each rejection happened, searchable by seat.**
    ///
    /// The disc beside it remembers WHERE a rejection happened and nothing
    /// else, so every rejection class deprioritizes a neighbourhood
    /// identically — a clause-1 rejection, a clause-2 veto and a stage refusal
    /// are indistinguishable to every later decision. This remembers the
    /// reason as well, in the shape
    /// [`crate::coverage_repair::RepairVetoIndex`] documents.
    ///
    /// **Nothing reads it to decide anything.** It is folded on every
    /// rejection and disclosed at the end; no branch in this controller
    /// consults it, and the search this ledger records is the search it
    /// recorded before the field existed.
    vetoes: crate::coverage_repair::RepairVetoIndex,
    steps: Vec<crate::coverage_repair::CoverageRepairStep>,
    /// The same step rows, appended to disk as they land — see
    /// [`crate::coverage_repair::RepairTrialTrail`]. `None` unless
    /// [`crate::coverage_repair::TRIAL_TRAIL_DIR_ENV`] asked for one.
    trail: Option<crate::coverage_repair::RepairTrialTrail>,
    per_trial_ms: Vec<u64>,
}

impl RepairLedger {
    /// **Stage D — the legalization record is a PASSENGER on this report.**
    ///
    /// It is taken once, before the first trial, and it is never written again:
    /// nothing in the repair loop can change what the attempt-0 loop already
    /// did. Passing it at construction is what makes that true by shape rather
    /// than by discipline, and it is why every refusal this ledger builds
    /// carries the same record the sealed authority would have.
    fn new(radius_mm: f64, legalization: crate::coverage_repair::LegalizationDisclosure) -> Self {
        Self {
            report: crate::coverage_repair::RepairRunReport {
                legalization,
                ..Default::default()
            },
            ledgered: BTreeSet::new(),
            discs: crate::coverage_closure::DiversityDiscs::new(radius_mm),
            demoted: BTreeSet::new(),
            // The SAME radius the discs are built with, so "near" means one
            // thing across both memories rather than two things that agree
            // today.
            vetoes: crate::coverage_repair::RepairVetoIndex::new(radius_mm),
            steps: Vec::new(),
            trail: crate::coverage_repair::RepairTrialTrail::from_env(),
            per_trial_ms: Vec::new(),
        }
    }

    /// **Roll one trial back.** The plan is a persistent value and the child was
    /// a DERIVED value, so rolling back is "do not rebind `plan`" — there is no
    /// undo log to replay and nothing to rebuild. What is left to do is bookkeeping:
    /// ledger the row so SPEC §6's exclusion holds, add the rejected seat's own
    /// coverage disc so rule N4 stops the search walking a 16 µm step to the
    /// same proposal, and record the step so the ledger can state the case.
    fn reject(
        &mut self,
        candidates: &mut crate::coverage_closure::RankedRepairCandidates,
        step: crate::coverage_repair::CoverageRepairStep,
        winner: &crate::coverage_closure::RankedRepairRow,
        started: std::time::Instant,
    ) {
        *self
            .report
            .rejections_by_class
            .entry(step.outcome.class().to_owned())
            .or_insert(0) += 1;
        self.ledgered.insert(winner.id);
        // ONE pass over the ranking per rejection, which is exactly the
        // incremental cost §A.4 rule N4 budgets: one new disc, one sweep, with
        // the count carried on the row so no comparator ever recomputes a
        // distance. On the Warhorse's 6,777 eligible rows that is 6,777 squared
        // distances, three orders of magnitude below the pipeline pass the
        // rejection just paid for.
        // A ledgered row is spent, so demoting it is not work rule N4 did — and
        // the rejected seat lies inside its own disc, so without this filter the
        // instrument would count one phantom demotion per rejection.
        let newly_demoted = self.discs.reject(winner.seat_mm, candidates);
        self.demoted.extend(
            newly_demoted
                .into_iter()
                .filter(|id| !self.ledgered.contains(id)),
        );
        // WHY, beside where. `from_step` reads the step this rejection already
        // filled — the outcome class, clause 2's named component when clause 2
        // is what fired, every hard-gate component that rose whichever clause
        // fired, and whether the trial's own contact was condemned. Nothing is
        // re-measured, and `None` is impossible here because a committed step
        // never reaches this function; the `if let` states that rather than
        // asserting it.
        if let Some(veto) = crate::coverage_repair::RepairVeto::from_step(&step) {
            self.vetoes.record(veto);
        }
        self.record_trial(step, started);
    }

    /// The two things every finished trial does, committed or not: join the
    /// in-memory stream, and reach the live trail on disk.
    ///
    /// One place, so a trial can never be in the ledger and missing from the
    /// trail — which would make the trail a second, quieter tally of the same
    /// thing rather than the same rows written sooner.
    fn record_trial(
        &mut self,
        step: crate::coverage_repair::CoverageRepairStep,
        started: std::time::Instant,
    ) {
        if let Some(trail) = self.trail.as_mut() {
            trail.append(&step);
        }
        self.steps.push(step);
        self.per_trial_ms.push(elapsed_ms(started));
    }

    /// Record a committed step, which needs no disc and spends no row: the row
    /// became a contact, so `occupied_repair_rows` excludes it from the rescore
    /// without anything being added to `ledgered`.
    fn commit(
        &mut self,
        step: crate::coverage_repair::CoverageRepairStep,
        started: std::time::Instant,
    ) {
        self.report.commits += 1;
        self.record_trial(step, started);
    }

    /// The whole trial stream, projected onto SPEC-A4 §D.2's wire form.
    ///
    /// Every trial, committed or not. A refusal that carried only its commits
    /// would print an empty list for the model that spent 45 trials and kept
    /// none of them, which is the exact case the record exists for.
    fn wire_steps(&self) -> Vec<crate::coverage_repair::CoverageRepairStepWire> {
        self.steps
            .iter()
            .map(crate::coverage_repair::CoverageRepairStep::to_wire)
            .collect()
    }

    /// Recharge rule N4's accumulated discs onto a ranking that a COMMIT has
    /// just rebuilt, and fold whatever that newly deprioritizes into the §A.8
    /// instrument.
    fn recount_discs(&mut self, candidates: &mut crate::coverage_closure::RankedRepairCandidates) {
        self.discs.recount(candidates);
        for row in &candidates.ranked {
            if row.diversity_discs > 0 && !self.ledgered.contains(&row.id) {
                self.demoted.insert(row.id);
            }
        }
    }

    /// **Eligible rows still untried, right now.**
    ///
    /// Task #67: the streamed progress line and the final report must not be
    /// two counts of the same thing. [`Self::finish`] fills
    /// `candidates_remaining` by calling THIS, and the progress line prints
    /// what THIS returns, so `remaining=` mid-run and `remaining=` at the end
    /// are the same expression over the same two collections.
    fn remaining(&self, candidates: &crate::coverage_closure::RankedRepairCandidates) -> usize {
        candidates
            .ranked
            .iter()
            .filter(|row| !self.ledgered.contains(&row.id))
            .count()
    }

    /// Fill the §A.8 counters that are only knowable when the search stops.
    ///
    /// `candidates` is `None` on the one path where the ranking itself refused,
    /// so there is no ranking to count remaining rows on. Everything else — the
    /// demotions and the wall times — is knowable there and is still filled.
    ///
    /// On that one path `candidates_remaining` therefore keeps its default 0,
    /// and that 0 is NOT a count of anything: it is the absence of a ranking.
    /// It cannot contradict a streamed line, because the loop is never entered
    /// on that path and no progress line is ever emitted — but a reader of the
    /// summary alone cannot tell that 0 from "every eligible row was tried",
    /// and §A.8.1 says so rather than leaving the reader to find out.
    fn finish(&mut self, candidates: Option<&crate::coverage_closure::RankedRepairCandidates>) {
        if let Some(candidates) = candidates {
            self.report.candidates_remaining = self.remaining(candidates);
        }
        self.report.diversity_demotions = self.demoted.len();
        // Both disclosures are folds of state kept incrementally, so this is a
        // projection at the end and not a pass over the trials.
        self.report.vetoes = self.vetoes.disclosure();
        self.report.trail = self
            .trail
            .as_ref()
            .map(crate::coverage_repair::RepairTrialTrail::disclosure);
        let per_trial = std::mem::take(&mut self.per_trial_ms);
        self.report.record_trial_times(per_trial);
    }
}

fn record_disposition(
    route_records: &mut BTreeMap<(u32, u32), RouteSearchRecord>,
    key: (u32, u32),
    disposition: Option<TrialContactDisposition>,
) {
    let record = route_records.entry(key).or_default();
    match disposition {
        Some(TrialContactDisposition::UnroutedNoWinner(evidence))
            if evidence.every_feasible_arm_refused_by_clearance =>
        {
            record.clearance_refused_terminals =
                record.clearance_refused_terminals.saturating_add(1);
        }
        Some(TrialContactDisposition::Quarantined {
            remaining_inside_axis_mm: Some(depth),
        }) => record.stub_reentry_depths_mm.push(depth),
        _ => {}
    }
}

/// One route's folded evidence on one side of a transaction.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct RouteFold {
    area_mm2: f64,
    patches: usize,
    worst_gap: Option<crate::coverage_closure::CoverageClosureGap>,
}

/// Per-route before/after evidence for one transaction. Routes that did not
/// move are omitted, so a committed step that touched one route carries one row.
///
/// **Both patch lists are folded, not one.** `MeasuredState::blocking` is the
/// CENSORED verdict set and `repair_targets` is the UNCENSORED worklist; §D.1's
/// `RouteDelta` names both, and only the censored side can say "this contact
/// closed island 5's failing patch". Folding only the uncensored side — which
/// is what this function did — silently dropped that sentence from the record.
fn route_deltas(
    before: &MeasuredState,
    after: &MeasuredState,
) -> Vec<crate::coverage_repair::RouteDelta> {
    fn fold(
        patches: &[crate::coverage_closure::BlockingPatchKey],
    ) -> BTreeMap<(u32, u32), RouteFold> {
        let mut grouped: BTreeMap<(u32, u32), RouteFold> = BTreeMap::new();
        for patch in patches {
            let entry = grouped
                .entry((patch.island_id, patch.component_id))
                .or_default();
            entry.area_mm2 += f64::from_bits(patch.uncovered_area_mm2_bits);
            entry.patches += 1;
            entry.worst_gap = Some(match entry.worst_gap {
                None => patch.worst_gap,
                Some(seen) => crate::coverage_closure::worse_gap(seen, patch.worst_gap),
            });
        }
        grouped
    }
    let failing_before = fold(&before.blocking);
    let failing_after = fold(&after.blocking);
    let left = fold(&before.repair_targets);
    let right = fold(&after.repair_targets);
    let mut keys: BTreeSet<(u32, u32)> = left.keys().copied().collect();
    keys.extend(right.keys().copied());
    keys.extend(failing_before.keys().copied());
    keys.extend(failing_after.keys().copied());
    keys.into_iter()
        .filter_map(|key| {
            let before_route = left.get(&key).copied().unwrap_or_default();
            let after_route = right.get(&key).copied().unwrap_or_default();
            let failing_left = failing_before.get(&key).copied().unwrap_or_default();
            let failing_right = failing_after.get(&key).copied().unwrap_or_default();
            let unmoved = before_route.area_mm2.to_bits() == after_route.area_mm2.to_bits()
                && before_route.patches == after_route.patches
                && before_route.worst_gap == after_route.worst_gap
                && failing_left.area_mm2.to_bits() == failing_right.area_mm2.to_bits()
                && failing_left.patches == failing_right.patches;
            if unmoved {
                return None;
            }
            Some(crate::coverage_repair::RouteDelta {
                island_id: key.0,
                component_id: key.1,
                failing_area_before_mm2: failing_left.area_mm2,
                failing_area_after_mm2: failing_right.area_mm2,
                uncensored_area_before_mm2: before_route.area_mm2,
                uncensored_area_after_mm2: after_route.area_mm2,
                failing_patches_before: failing_left.patches,
                failing_patches_after: failing_right.patches,
                patches_before: before_route.patches,
                patches_after: after_route.patches,
                worst_gap_before: before_route.worst_gap,
                worst_gap_after: after_route.worst_gap,
            })
        })
        .collect()
}

fn stamp_tally(
    tally: &mut BTreeMap<(u32, u32), crate::coverage_closure::KeyNomination>,
    candidates: &crate::coverage_closure::RankedRepairCandidates,
) {
    for (key, counts) in &candidates.per_key {
        let entry = tally.entry(*key).or_default();
        entry.eligible_rows = counts.eligible_rows;
    }
}

/// **§A.8 applies here too.** This is the one exit from the loop that is not
/// `refuse`, and it used to discard the report entirely (`repair: None`) and
/// print nothing — so a run that had already spent trials could refuse silently
/// with its whole ledger thrown away. It now finishes and prints the report on
/// this path like every other, which is what "every run reports, on every path"
/// means.
///
/// `candidates_remaining` stays 0 here and that is honest rather than
/// convenient: the RANKING itself refused, so there is no ranking to count rows
/// on.
fn rank_or_refuse(
    plan: &SpecializedContactPlan,
    measured: &MeasuredState,
    radius_mm: f64,
    matrix_budget: usize,
    ledger: &mut RepairLedger,
) -> Result<crate::coverage_closure::RankedRepairCandidates, SupportRunError> {
    crate::coverage_closure::rank_repair_candidates(
        plan,
        &measured.repair_targets,
        &measured.uncovered_samples,
        radius_mm,
        matrix_budget,
        &ledger.ledgered,
    )
    .map_err(|error| {
        note_stage_failure("Preparation", &error);
        ledger.finish(None);
        ledger.report.log_summary();
        let steps = ledger.wire_steps();
        SupportRunError::CoverageClosure(Box::new(
            crate::coverage_closure::CoverageClosureRefusal {
                // Ruling R6: the field named for the registered bound carries
                // what that bound bounds. The ledger's own commit counter is
                // the one place that number lives.
                attempts: ledger.report.commits,
                reason: crate::coverage_closure::CoverageClosureReason::RepairPlanRefused {
                    phase: SupportRunPhase::Preparation,
                    detail: format!("{error:?}"),
                },
                islands: Vec::new(),
                steps,
                repair: Some(ledger.report.clone()),
            },
        ))
    })
}

/// Build the refusal: the measured stop reason, the per-route outcomes §E.1
/// names, and the §A.8 report.
#[allow(
    clippy::too_many_arguments,
    reason = "every argument is one measured input to the refusal; bundling them into a \
              struct would only move the same list one line up"
)]
fn refuse(
    reason: crate::coverage_closure::CoverageClosureReason,
    committed: usize,
    measured: &MeasuredState,
    candidates: &crate::coverage_closure::RankedRepairCandidates,
    plan: &SpecializedContactPlan,
    ledger: &mut RepairLedger,
    route_records: &BTreeMap<(u32, u32), RouteSearchRecord>,
    tally: &BTreeMap<(u32, u32), crate::coverage_closure::KeyNomination>,
    scales: BlockedCauseScales,
    radius_mm: f64,
) -> SupportRunError {
    ledger.finish(Some(candidates));
    ledger.report.log_summary();
    let steps = ledger.wire_steps();

    let mut islands = crate::coverage_closure::islands_from_blocking(&measured.blocking, tally);
    for island in &mut islands {
        island.outcome = Some(route_outcome(
            (island.island_id, island.component_id),
            candidates,
            plan,
            &ledger.ledgered,
            route_records,
            measured,
            scales,
            radius_mm,
        ));
    }
    SupportRunError::CoverageClosure(Box::new(crate::coverage_closure::CoverageClosureRefusal {
        // Ruling R6: the registered bound counts COMMITTED steps, so this
        // is the honest number to carry in a field named for the bound.
        attempts: committed,
        reason,
        islands,
        steps,
        repair: Some(ledger.report.clone()),
    }))
}

/// **SPEC-A4 §E.1's two MEASURED defect classes, decided from one route's own
/// search record and nothing else.**
///
/// These two `return`s are the only construction sites of
/// [`crate::coverage_closure::BlockedCause::ClearanceMaskUnsatisfiable`] (task
/// #51) and [`crate::coverage_closure::BlockedCause::StubReentry`] (task #52)
/// in the workspace, and they are split out of [`route_outcome`] so they can be
/// PROVEN. Reaching them through the loop needs geometry that reproduces #51 or
/// #52 — real minis do, no procedural fixture does, and a class that decides
/// how a refusal reads must not wait on a defect being reproducible before
/// anybody can pin which class it names. Everything the decision reads is in the
/// two arguments, so `mod tests` builds the record that
/// [`record_disposition`] would build from a run's own
/// [`TrialContactDisposition`] values and asserts WHICH class comes back.
///
/// **A class is claimed only when EVERY trial the route spent landed in it.**
/// `None` means the route's trials did not all land in one class — including
/// the route that spent no trial at all, and the quarantine whose re-entry
/// depth was never measured — and the caller then reports the search outcome
/// rather than picking a favourite. Splitting this out does not move that rule:
/// the two equality tests below are the ones that were inline, unchanged.
fn measured_defect_class(
    record: &RouteSearchRecord,
    scales: BlockedCauseScales,
) -> Option<crate::coverage_closure::BlockedCause> {
    use crate::coverage_closure::BlockedCause;

    if record.rows_tried == 0 {
        return None;
    }
    if u32::try_from(record.rows_tried).unwrap_or(u32::MAX) == record.clearance_refused_terminals {
        return Some(BlockedCause::ClearanceMaskUnsatisfiable {
            terminals: record.clearance_refused_terminals,
            standoff_mm: scales.standoff_mm,
        });
    }
    if record.stub_reentry_depths_mm.len() == record.rows_tried {
        let mut depths = record.stub_reentry_depths_mm.clone();
        depths.sort_by(f64::total_cmp);
        return Some(BlockedCause::StubReentry {
            terminals_quarantined: u32::try_from(depths.len()).unwrap_or(u32::MAX),
            depth_min_mm: depths[0],
            depth_median_mm: depths[depths.len() / 2],
            depth_max_mm: depths[depths.len() - 1],
            allowance_mm: scales.allowance_mm,
        });
    }
    None
}

/// **SPEC-A4 §E.1, rulings R7 and R14 — why THIS route is still uncovered.**
///
/// Every branch reads a counter this run already kept. None of them is a
/// distance claim: the design's v1 assigned Warhorse islands 5 and 6 "no row
/// within radius" from worst gaps of 7.09 mm and 15.29 mm, and the island
/// diagnosis showed 220 of 300 pool rows sit INSIDE the radius with the nearest
/// candidate seat 0.0972 mm from the hole. "Nothing reached it" and "nothing
/// could reach it" are different statements and only the first was measured.
#[allow(
    clippy::too_many_arguments,
    reason = "the classes are decided by six independent measured inputs; passing fewer \
              would mean deciding a class from less evidence than the run has"
)]
fn route_outcome(
    key: (u32, u32),
    candidates: &crate::coverage_closure::RankedRepairCandidates,
    plan: &SpecializedContactPlan,
    ledgered: &BTreeSet<RepairRowId>,
    route_records: &BTreeMap<(u32, u32), RouteSearchRecord>,
    measured: &MeasuredState,
    scales: BlockedCauseScales,
    radius_mm: f64,
) -> crate::coverage_closure::RouteRepairOutcome {
    use crate::coverage_closure::{BlockedCause, RouteRepairOutcome};

    let pool_rows = plan
        .all_repair_rows()
        .iter()
        .filter(|row| (row.id().owner_island_id(), row.id().component_id()) == key)
        .count();
    if pool_rows == 0 {
        return RouteRepairOutcome::Blocked(BlockedCause::NoPoolRows);
    }

    let route_rows: Vec<&crate::coverage_closure::RankedRepairRow> = candidates
        .ranked
        .iter()
        .filter(|row| row.key == key)
        .collect();
    let remaining = route_rows
        .iter()
        .filter(|row| !ledgered.contains(&row.id))
        .count();
    let record = route_records.get(&key).cloned().unwrap_or_default();

    // The two measured defect classes, each claimed only when EVERY trial this
    // route spent landed in it. A route that mixed causes reports the search
    // outcome instead of picking a favourite.
    if let Some(cause) = measured_defect_class(&record, scales) {
        return RouteRepairOutcome::Blocked(cause);
    }

    // The zero-score class. It IS a measurement — the score — and the distance
    // rides along as CONTEXT so a reader can see how close the pool sits
    // without the class ever being decided by it.
    let best_score = route_rows
        .iter()
        .map(|row| row.marginal_area_mm2)
        .fold(0.0_f64, f64::max);
    if best_score == 0.0 {
        let samples = plan.coverage_samples();
        let mut nearest_sq = f64::INFINITY;
        let mut rows_within_radius = 0_usize;
        for row in &route_rows {
            let mut row_nearest_sq = f64::INFINITY;
            for &index in &measured.uncovered_samples {
                let Some(sample) = samples.get(index) else {
                    continue;
                };
                let delta = sub(row.seat_mm, sample.point_mm);
                // Canon C7 (`docs/specs/geometry-canon.md` §2 step 2(c)): the
                // retired `mul_add` chain is a fused, therefore different,
                // value from the kernel's, and it is compared against
                // `radius_mm * radius_mm` two lines below.
                let distance_sq = dot(delta, delta);
                row_nearest_sq = row_nearest_sq.min(distance_sq);
            }
            if row_nearest_sq <= radius_mm * radius_mm {
                rows_within_radius += 1;
            }
            nearest_sq = nearest_sq.min(row_nearest_sq);
        }
        return RouteRepairOutcome::Blocked(BlockedCause::NoCandidateCoversAnything {
            pool_rows,
            rows_within_radius,
            nearest_seat_to_uncovered_mm: nearest_sq.sqrt(),
        });
    }

    if remaining == 0 && record.rows_tried > 0 {
        return RouteRepairOutcome::Blocked(BlockedCause::PoolExhausted {
            pool_rows,
            occupied: pool_rows.saturating_sub(route_rows.len()),
            ledgered: route_rows.len(),
        });
    }
    if record.rows_tried > 0 {
        return RouteRepairOutcome::TriedNoImprovement {
            rows_tried: record.rows_tried,
            best_marginal_area_mm2: record.best_marginal_area_mm2,
        };
    }
    RouteRepairOutcome::NotReached {
        rows_remaining: remaining,
    }
}

/// One physical contact admitted by the finalized CCO transcript for future
/// coverage verification. It is a borrowed view, never a new planning input.
#[derive(Clone, Copy, Debug)]
pub struct RoutedPhysicalContactEvidence<'a> {
    contact: &'a crate::specialized::SpecializedPhysicalContact,
}

/// **Task #71 — what each emitted contact actually carries, and how big the tip
/// that carries it is.**
///
/// The gap this closes, in the words of the task: *artifact bytes can move
/// while every disclosed field stays identical*. A run report gives a record
/// count, a coverage verdict and a repair ledger; none of them is a function of
/// the load partition or of the tip sizing, so two runs whose per-contact
/// carried area and tip radii differ can print the same summary and write
/// different files. A reader could see the bytes move and had nothing to read
/// that would say why.
///
/// **UNITS: `tip_radius_mm` is a RADIUS and `tip_diameter_mm` is a DIAMETER.**
/// Both are given for the same reason [`TipGrowthRungDisclosure`] gives both:
/// the registry speaks radius and the slicer UI speaks diameter, and a number
/// carried between them without halving or doubling lands 2x wrong.
///
/// **The tip is read at the end that TOUCHES THE MODEL, in the final live
/// view** — the terminal's own registered member, at its terminalward end,
/// which is where a contact stub meets the surface. It is the member the run
/// went on to emit, not the one it planned: it is read after growth, after
/// rewire and after the brace stage's resize.
///
/// **It is the MEMBER's tip radius, and the artifact's TIP record is not the
/// same number.** `lower.rs`'s `lower_stub` builds the contact ball at
/// `tip_ball_factor` (registered, 1.5) times this radius, so a reader diffing
/// this field straight against a `SegmentType::TIP` record in the `.chitubox`
/// lands 1.5x off — the same class of error the UNITS paragraph above warns
/// about, one factor over. The relation is exact and is pinned end to end in
/// `plinth-pipeline`'s `prepare_folder` gate, which reads the TIP records back
/// off the written file and compares them to these radii scaled by the
/// registered factor.
///
/// **This is evidence, and the pre-write `Load` gate READS IT.**
/// `plinth_pipeline::judge_emitted_load_law` reads `carried_area_mm2` and
/// `tip_radius_mm` off every row here and judges
/// `pi * tip_radius_mm^2 >= carried_area_mm2 / X_ALLOWED` on each.
///
/// **The area-law verdict is a WARNING, not a refusal** (owner ruling,
/// 2026-08-29): a violating contact is recorded with its evidence and the
/// model still emits, so the gate can measure the corpus while the load model
/// is redesigned. It flips to hard refusal once the plans are fixed. What the
/// gate still REFUSES on is a record it cannot read at all -- a `Routed` row
/// with either field missing, or a row count that disagrees with the sealed
/// snapshot. Two consequences a future editor must carry:
///
/// * `None` is load-bearing. The gate refuses a `Routed` row it cannot
///   describe, so filling either field with a legal-looking `0.0` to make a
///   join succeed would convert a refusal into a pass -- and, under warning
///   mode, into a silent pass. The paragraph on `carried_area_mm2` explains
///   why zero is legal to READ and illegal to INVENT; the gate is now what
///   enforces it.
/// * The row count is load-bearing. The gate cross-checks it against the sealed
///   snapshot's own routed-contact projection, so dropping an undescribable row
///   instead of emitting a `None` one would be caught, not absorbed.
///
/// It still carries `tip_diameter_mm` and the seat for the older reason: so a
/// byte movement can be attributed instead of argued about.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmittedContactDisclosure {
    /// The physical contact id, which is also its terminal id.
    pub contact_id: u64,
    /// Where the contact sits on the model, mm. `None` when the transcript row
    /// names a contact the accepted plan no longer holds — see
    /// [`Self::carried_area_mm2`], which is `None` in exactly the same case and
    /// for the same reason.
    pub position_mm: Option<[f64; 3]>,
    /// The model area this contact is charged to carry after every removal and
    /// repartition the sealing attempt performed, mm². `None` when the
    /// transcript row's contact could not be found in the accepted plan — a
    /// state this refuses to paper over with a zero at the origin, because
    /// `0.0 mm²` is a legal area to READ (a routed contact charged with nothing
    /// is ordinary, and this artifact's own sidecar carries such rows) and an
    /// illegal one to INVENT. A reader must be able to tell "carries nothing"
    /// from "could not be described".
    pub carried_area_mm2: Option<f64>,
    /// The RADIUS of the member end that touches the model, mm. `None` when the
    /// terminal's member is not in the final live view — a state this refuses
    /// to paper over with a zero, because zero is a legal radius to read and an
    /// illegal one to invent.
    pub tip_radius_mm: Option<f64>,
    /// The same measurement as a DIAMETER, mm. Exactly `2 x tip_radius_mm`.
    pub tip_diameter_mm: Option<f64>,
}

/// Immutable geometry of one member in the exact final live support view.
///
/// This is verification evidence, not a planning input: it has no public
/// constructor and deliberately exposes neither a forest nor any mutation
/// seam.  `p0` is terminalward, `p1` is rootward, and their radii retain that
/// same orientation.
#[derive(Clone, Debug, PartialEq)]
pub struct FinalLiveMemberEvidence {
    key: MemberKey,
    p0_mm: [f64; 3],
    p1_mm: [f64; 3],
    r0_mm: f64,
    r1_mm: f64,
    kind: MemberKind,
    model_contact_end: Option<ContactEnd>,
}

impl FinalLiveMemberEvidence {
    fn from_member(key: MemberKey, member: &Member) -> Self {
        Self {
            key,
            p0_mm: member.p0_mm().map(Finite::get),
            p1_mm: member.p1_mm().map(Finite::get),
            r0_mm: member.terminal_radius_mm().get().get(),
            r1_mm: member.rootward_radius_mm().get().get(),
            kind: member.kind().clone(),
            model_contact_end: member.model_contact_end(),
        }
    }

    #[must_use]
    pub const fn key(&self) -> MemberKey {
        self.key
    }
    #[must_use]
    pub const fn p0_mm(&self) -> [f64; 3] {
        self.p0_mm
    }
    #[must_use]
    pub const fn p1_mm(&self) -> [f64; 3] {
        self.p1_mm
    }
    #[must_use]
    pub const fn terminal_radius_mm(&self) -> f64 {
        self.r0_mm
    }
    #[must_use]
    pub const fn rootward_radius_mm(&self) -> f64 {
        self.r1_mm
    }
    #[must_use]
    pub const fn kind(&self) -> &MemberKind {
        &self.kind
    }
    #[must_use]
    pub const fn model_contact_end(&self) -> Option<ContactEnd> {
        self.model_contact_end
    }

    #[cfg(test)]
    pub(crate) fn transit_test_fixture(
        key: [u32; 2],
        endpoints_mm: [[f64; 3]; 2],
        radii_mm: [f64; 2],
        kind: MemberKind,
        model_contact_end: Option<ContactEnd>,
    ) -> Self {
        Self {
            key: MemberKey::transit_test_fixture(key[0], key[1]),
            p0_mm: endpoints_mm[0],
            p1_mm: endpoints_mm[1],
            r0_mm: radii_mm[0],
            r1_mm: radii_mm[1],
            kind,
            model_contact_end,
        }
    }
}

impl RoutedPhysicalContactEvidence<'_> {
    #[must_use]
    pub const fn physical_contact_id(&self) -> u64 {
        self.contact.contact_id()
    }

    #[must_use]
    pub const fn position_mm(&self) -> [f64; 3] {
        self.contact.position_mm()
    }

    #[must_use]
    pub const fn normal(&self) -> [f64; 3] {
        self.contact.normal()
    }

    #[must_use]
    pub const fn carried_area_mm2(&self) -> f64 {
        self.contact.carried_area_mm2()
    }
}

/// Source-qualified logical route carried by one finalized physical seat.
/// It is deliberately private to the verification boundary: callers may not
/// forge route membership or turn this evidence back into a planning input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RoutedLogicalRoute {
    island_id: u32,
    face_id: u32,
    component_id: u32,
    provenance: RoutedRouteProvenance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum RoutedRouteProvenance {
    RegularGeneric,
    RegularSyntheticIsland,
    SpecializedMinimum,
    SpecializedIslandBirth,
}

/// Private, normalized route projection parallel to the routed-contact
/// cadence. This keeps public physical evidence copyable while retaining the
/// exact logical authority needed by a later batched measurement pass.
#[derive(Debug)]
struct RoutedRouteMembership<'a> {
    contact: RoutedPhysicalContactEvidence<'a>,
    logical_routes: Vec<RoutedLogicalRoute>,
}

/// Rejection from the sealed verification boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum SupportVerificationError {
    #[error("sealed verification snapshot integrity digest is stale")]
    SnapshotDigestMismatch,
    #[error("sealed verification snapshot no longer matches its mesh or topology authority")]
    TopologyMeshMismatch,
    #[error("sealed verification snapshot no longer matches final support authority")]
    AuthorityMismatch,
}

/// Borrow-only verification evidence from one validated final lowering run.
///
/// This exposes neither planning state nor mutable routing state. Its contact
/// iterator contains exactly the contacts whose terminal disposition is
/// `Routed`; quarantined and no-winner terminals are intentionally absent.
pub struct SupportVerificationSnapshot<'a> {
    mesh: &'a Mesh,
    topology: &'a FaceTopology,
    // The final snapshot borrows the single native surface retained by the
    // support context.  It never reconstructs an index from mesh arrays.
    surface: &'a NativeSurface,
    coverage_samples: &'a [CoverageSample],
    // The canonical planner-owned registry is retained separately from
    // samples. A source face can be a member of several named islands.
    named_islands: &'a [SupportIsland],
    routed_contacts: Vec<RoutedPhysicalContactEvidence<'a>>,
    routed_route_memberships: Vec<RoutedRouteMembership<'a>>,
    final_live_members: Vec<FinalLiveMemberEvidence>,
    mesh_digest: &'a str,
    topology_digest: &'a str,
    plan_digest: u64,
    load_digest: u64,
    geometry_digest: u64,
    forest_digest: u64,
    transcript_digest: u64,
    regular_digest: u64,
    coverage_digest: u64,
    named_islands_digest: u64,
    lowered_primitives_digest: u64,
    routed_projection_digest: u64,
    final_live_members_digest: u64,
    snapshot_digest: u64,
}

impl std::fmt::Debug for SupportVerificationSnapshot<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SupportVerificationSnapshot")
            .field("mesh_digest", &self.mesh_digest)
            .field("topology_digest", &self.topology_digest)
            .field("routed_contacts", &self.routed_contacts)
            .field("final_live_members", &self.final_live_members)
            .field("plan_digest", &self.plan_digest)
            .field("load_digest", &self.load_digest)
            .field("geometry_digest", &self.geometry_digest)
            .field("forest_digest", &self.forest_digest)
            .field("transcript_digest", &self.transcript_digest)
            .field("lowered_primitives_digest", &self.lowered_primitives_digest)
            .field("routed_projection_digest", &self.routed_projection_digest)
            .field("final_live_members_digest", &self.final_live_members_digest)
            .field("snapshot_digest", &self.snapshot_digest)
            .finish()
    }
}

/// Scalar accounting gathered from sealed evidence before measurement creates
/// its normalized projection or route planner maps.  The physical-seat count
/// intentionally uses bounded comparison scans: route membership is canonical
/// within a contact, but transcript cadence does not promise adjacent equal
/// route keys.  The scan is at most quadratic in the caller-capped logical
/// membership count and short-circuits on every cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CoverageMeasurementPreflight {
    memberships: usize,
    sequential_membership_peak: usize,
    indexed_points: usize,
    sequential_indexed_peak: usize,
    query_rows: usize,
    distance_work: usize,
    route_retained_base_bytes: usize,
    route_knn_peak_bytes: usize,
    island_row_count: usize,
    assessment: crate::CoverageAssessmentResourcePlan,
    route_plan_count: usize,
    logical_peak_bytes: usize,
}

impl<'a> SupportVerificationSnapshot<'a> {
    fn measurement_preflight(
        &self,
        limits: crate::CoverageMeasurementLimits,
        knn_limits: KnnBatchLimits,
        island_index: &IslandFaceIndex<'_>,
    ) -> Result<CoverageMeasurementPreflight, CoverageMeasurementError> {
        let mut memberships = 0usize;
        let mut sequential_membership_peak = 0usize;
        let mut indexed_points = 0usize;
        let mut sequential_indexed_peak = 0usize;
        let mut query_rows = 0usize;
        let mut distance_work = 0usize;
        let mut source_route_count = 0usize;
        let mut route_plan_count = 0usize;
        let mut max_knn_peak = 0usize;
        let mut max_query_count = 0usize;

        // Batch-first (D-37).  Refuse the route-entry total against its own
        // ceiling before reserving any scratch for it, so the cap still
        // refuses ahead of the allocation exactly as the per-route counter
        // below does.
        let total_routes = self
            .routed_route_memberships
            .iter()
            .try_fold(0usize, |total, membership| {
                total.checked_add(membership.logical_routes.len())
            })
            .ok_or(CoverageMeasurementError::ArithmeticOverflow {
                operation: "route memberships",
            })?;
        if total_routes > limits.max_route_memberships() {
            return Err(CoverageMeasurementError::ResourceLimit {
                resource: "route memberships",
            });
        }
        // One sorted column per input set, gathered once.  Every per-key
        // question below is then a pair of binary searches into a column,
        // where it used to be a fresh scan of every membership or every
        // coverage sample, repeated for each distinct key, on top of an
        // `O(M²)` sealed-slice prefix scan for the first-occurrence test.
        let route_key_rows =
            sorted_routed_route_key_rows(&self.routed_route_memberships, total_routes)?;
        let sample_keys = sorted_sample_route_keys(self.coverage_samples)?;
        let mut seen_keys: BTreeSet<(u32, u32)> = BTreeSet::new();

        for membership in &self.routed_route_memberships {
            for route in &membership.logical_routes {
                memberships = memberships.checked_add(1).ok_or(
                    CoverageMeasurementError::ArithmeticOverflow {
                        operation: "route memberships",
                    },
                )?;
                if memberships > limits.max_route_memberships() {
                    return Err(CoverageMeasurementError::ResourceLimit {
                        resource: "route memberships",
                    });
                }

                // Process a route key only once, on its first occurrence in
                // this same iteration order.  Canonical per-contact routes do
                // not make transcript-wide equal keys adjacent, so the order
                // is the transcript's and the retained set is what makes one
                // pass enough.
                let key = (route.island_id, route.component_id);
                if !seen_keys.insert(key) {
                    continue;
                }
                let key_rows =
                    &route_key_rows[sorted_key_range(&route_key_rows, &key, |row| (row.0, row.1))];
                #[cfg(test)]
                record_measurement_call(|counts| {
                    counts.preflight_route_key_scans += key_rows.len();
                });
                // The run's length is every route entry carrying this key; the
                // number of distinct membership indices inside it is every
                // membership carrying it, and the rows are sorted by that
                // index within the run.
                let route_memberships = key_rows.len();
                sequential_membership_peak = sequential_membership_peak.max(route_memberships);
                let source_count = usize::from(!key_rows.is_empty())
                    + key_rows
                        .windows(2)
                        .filter(|pair| pair[0].2 != pair[1].2)
                        .count();
                indexed_points = indexed_points.checked_add(source_count).ok_or(
                    CoverageMeasurementError::ArithmeticOverflow {
                        operation: "indexed points",
                    },
                )?;
                if indexed_points > limits.max_indexed_points() {
                    return Err(CoverageMeasurementError::ResourceLimit {
                        resource: "indexed points",
                    });
                }
                source_route_count = source_route_count.checked_add(1).ok_or(
                    CoverageMeasurementError::ArithmeticOverflow {
                        operation: "route keys",
                    },
                )?;
                let queries = sorted_key_range(&sample_keys, &key, |row| *row).len();
                let work = source_count.checked_mul(queries).ok_or(
                    CoverageMeasurementError::ArithmeticOverflow {
                        operation: "distance work",
                    },
                )?;
                distance_work = distance_work.checked_add(work).ok_or(
                    CoverageMeasurementError::ArithmeticOverflow {
                        operation: "distance work",
                    },
                )?;
                if distance_work > limits.max_distance_work() {
                    return Err(CoverageMeasurementError::ResourceLimit {
                        resource: "distance work",
                    });
                }
                if queries != 0 {
                    route_plan_count = route_plan_count.checked_add(1).ok_or(
                        CoverageMeasurementError::ArithmeticOverflow {
                            operation: "route plans",
                        },
                    )?;
                    query_rows = query_rows.checked_add(queries).ok_or(
                        CoverageMeasurementError::ArithmeticOverflow {
                            operation: "query rows",
                        },
                    )?;
                    if query_rows > limits.max_query_rows() {
                        return Err(CoverageMeasurementError::ResourceLimit {
                            resource: "query rows",
                        });
                    }
                    max_query_count = max_query_count.max(queries);
                    sequential_indexed_peak = sequential_indexed_peak.max(source_count);
                    let knn =
                        KnnBatchPlan::new(source_count, queries, 1, knn_limits).map_err(|_| {
                            CoverageMeasurementError::ResourceLimit {
                                resource: "route kNN preflight",
                            }
                        })?;
                    max_knn_peak = max_knn_peak.max(knn.logical_peak_bytes());
                }
            }
        }

        // Batch-first (D-37).  Both counts below used to be quadratic scalar
        // scans over the coverage samples — a per-sample rescan of the sample
        // prefix, and a per-sample linear walk of the named islands ending in a
        // linear `Vec::contains` over that island's faces.  On a flagship model
        // (~4e5 samples) that is ~1e10-1e11 comparisons on every production
        // emission, because the pre-write gate now runs this preflight per
        // artifact.  Both are now one gathered pass: the distinct route keys
        // are run starts in the sample key column already sorted above, and a
        // single sorted island index answers every sample by two binary
        // searches.
        let query_route_count = distinct_sorted_keys(&sample_keys);
        let island_row_count = self
            .coverage_samples
            .iter()
            .try_fold(0usize, |total, sample| {
                if island_index.contains(sample.island_id, sample.face_id) {
                    total
                        .checked_add(1)
                        .ok_or(CoverageMeasurementError::ArithmeticOverflow {
                            operation: "island evidence rows",
                        })
                } else {
                    Ok(total)
                }
            })?;
        let assessment = coverage_measurement_assessment_resource_plan(
            self.coverage_samples,
            self.topology,
            island_row_count,
        )
        .map_err(|_| CoverageMeasurementError::AssessmentRefusal {
            reason: "assessment preflight",
        })?;
        if assessment.operations > limits.max_assessment_work() {
            return Err(CoverageMeasurementError::ResourceLimit {
                resource: "assessment work",
            });
        }
        let route_plans = route_plan_count
            .checked_mul(std::mem::size_of::<crate::specialized::CoverageRouteKnnPlan>())
            .ok_or(CoverageMeasurementError::ArithmeticOverflow {
                operation: "route plan bytes",
            })?;
        let route_retained_base_bytes = [
            source_route_count
                .checked_mul(std::mem::size_of::<crate::specialized::CoverageRoute>()),
            source_route_count.checked_mul(std::mem::size_of::<(
                crate::specialized::CoverageRoute,
                usize,
            )>()),
            indexed_points.checked_mul(std::mem::size_of::<u64>()),
            query_route_count.checked_mul(std::mem::size_of::<(
                crate::specialized::CoverageRoute,
                usize,
            )>()),
            Some(route_plans),
            sequential_indexed_peak.checked_mul(std::mem::size_of::<u64>()),
            max_query_count.checked_mul(std::mem::size_of::<usize>()),
        ]
        .into_iter()
        .try_fold(0usize, |total, bytes| {
            bytes
                .ok_or(CoverageMeasurementError::ArithmeticOverflow {
                    operation: "route retained bytes",
                })
                .and_then(|bytes| {
                    total
                        .checked_add(bytes)
                        .ok_or(CoverageMeasurementError::ArithmeticOverflow {
                            operation: "route retained bytes",
                        })
                })
        })?;
        let nearest_bytes = self
            .coverage_samples
            .len()
            .checked_mul(std::mem::size_of::<f64>())
            .ok_or(CoverageMeasurementError::ArithmeticOverflow {
                operation: "nearest vector bytes",
            })?;
        let normalized_route_bytes = memberships
            .checked_mul(std::mem::size_of::<CoverageRoutePoint>())
            .ok_or(CoverageMeasurementError::ArithmeticOverflow {
                operation: "normalized route bytes",
            })?;
        let route_peak = route_retained_base_bytes.checked_add(max_knn_peak).ok_or(
            CoverageMeasurementError::ArithmeticOverflow {
                operation: "route peak bytes",
            },
        )?;
        let logical_peak_bytes = nearest_bytes
            .checked_add(normalized_route_bytes)
            .and_then(|base| base.checked_add(route_peak.max(assessment.phase_peak_bytes)))
            .ok_or(CoverageMeasurementError::ArithmeticOverflow {
                operation: "logical peak bytes",
            })?;
        if logical_peak_bytes > limits.max_logical_peak_bytes() {
            return Err(CoverageMeasurementError::ResourceLimit {
                resource: "logical peak bytes",
            });
        }
        Ok(CoverageMeasurementPreflight {
            memberships,
            sequential_membership_peak,
            indexed_points,
            sequential_indexed_peak,
            query_rows,
            distance_work,
            route_retained_base_bytes,
            route_knn_peak_bytes: max_knn_peak,
            island_row_count,
            assessment,
            route_plan_count,
            logical_peak_bytes,
        })
    }

    /// Measures the sealed routed-contact projection without making an
    /// acceptance decision or exposing route authority.
    pub fn measure_coverage(
        &self,
        request: CoverageMeasurementRequest,
    ) -> Result<CoverageMeasurement, CoverageMeasurementError> {
        self.validate_integrity()
            .map_err(|_| CoverageMeasurementError::SnapshotBinding)?;
        let limits = request.limits();
        let knn_limits = KnnBatchLimits {
            max_source_points: limits.max_indexed_points(),
            max_query_points: limits.max_query_rows(),
            max_neighbors: 1,
            // The batch's own registered ceiling, not the measurement's
            // retained-peak ceiling. The latter still bounds this peak as one
            // of its summands in `measurement_preflight`; what it must not do
            // is stand in for the batch's own owner.
            max_logical_peak_bytes: limits.max_route_knn_peak_bytes(),
            max_work: limits.max_distance_work(),
        };
        // Built once and threaded through both the preflight's count and the
        // real measurement below — not rebuilt for the second use.
        let island_index = IslandFaceIndex::build(self.named_islands)?;
        // This complete scalar pass must precede every measurement allocation.
        // In particular, it counts exact physical seats per route rather than
        // using a worst-case membership bound, so a request at an exact cap is
        // accepted when the later allocating planner agrees.
        let preflight = self.measurement_preflight(limits, knn_limits, &island_index)?;
        let mut points = Vec::new();
        #[cfg(test)]
        record_measurement_call(|counts| counts.normalized_route_projection_reserves += 1);
        points
            .try_reserve_exact(preflight.memberships)
            .map_err(|_| CoverageMeasurementError::ResourceLimit {
                resource: "normalized route projection",
            })?;
        for membership in &self.routed_route_memberships {
            for route in &membership.logical_routes {
                points.push(CoverageRoutePoint {
                    physical_id: membership.contact.physical_contact_id(),
                    island_id: route.island_id,
                    component_id: route.component_id,
                    position_mm: membership.contact.position_mm(),
                });
            }
        }
        #[cfg(test)]
        record_measurement_call(|counts| counts.coverage_route_planner_allocations += 1);
        // FU-8d: the measurement path evaluates no merge proposals, so it has
        // nothing to answer from retained witnesses.  It keeps asking the
        // kernel for one neighbour per query, and its charged logical peak —
        // which `measurement_preflight` above states at the same `k` — is
        // unchanged by FU-8d.
        let planned = plan_route_points_nearest(
            &points,
            self.coverage_samples,
            knn_limits,
            NearestWitnessRetention::NearestOnly,
        )
        .map_err(|_| CoverageMeasurementError::ResourceLimit {
            resource: "route kNN preflight",
        })?;
        debug_assert_eq!(points.len(), preflight.memberships);
        let (
            memberships,
            sequential_membership_peak,
            indexed_points,
            indexed_point_peak,
            query_rows,
            distance_work,
        ) = planned.measurement_counts();
        if !measurement_plan_matches_preflight(preflight, &planned) {
            return Err(CoverageMeasurementError::AssessmentRefusal {
                reason: "route preflight mismatch",
            });
        }
        let mut nearest = vec![f64::INFINITY; self.coverage_samples.len()];
        #[cfg(test)]
        record_measurement_call(|counts| counts.coverage_route_queries += 1);
        fill_route_points_nearest(&points, self.coverage_samples, &mut nearest, planned, None)
            .map_err(|_| CoverageMeasurementError::KernelRefusal {
                operation: "route kNN query",
            })?;
        #[cfg(test)]
        record_measurement_call(|counts| counts.coverage_assessment_traversals += 1);
        let mut assessment = assess_coverage_with_mode(
            self.coverage_samples,
            self.topology,
            &nearest,
            request.radius_mm().get(),
            request.patch_area_floor_mm2().get(),
            CoverageAssessmentLimits {
                max_scratch_bytes: limits.max_logical_peak_bytes(),
                max_operations: limits.max_assessment_work(),
            },
            CoverageAssessmentMode::MeasurementEvidence,
        )
        .map_err(|_| CoverageMeasurementError::AssessmentRefusal {
            reason: "coverage traversal",
        })?;
        // The same single sorted membership index the preflight built, reused
        // rather than rebuilt: this loop asks the identical question once per
        // sample.
        let mut island_rows = Vec::with_capacity(preflight.island_row_count);
        for sample in self.coverage_samples {
            if island_index.contains(sample.island_id, sample.face_id) {
                island_rows.push((sample.island_id, sample.component_id, sample.face_id));
            }
        }
        island_rows.sort_unstable();
        island_rows.dedup();
        let mut islands = Vec::with_capacity(island_rows.len());
        let mut start = 0;
        while start < island_rows.len() {
            let (island_id, component_id, representative_face_id) = island_rows[start];
            let mut end = start + 1;
            while end < island_rows.len()
                && (island_rows[end].0, island_rows[end].1) == (island_id, component_id)
            {
                end += 1;
            }
            let faces = island_rows[start..end]
                .iter()
                .map(|row| row.2)
                .collect::<Vec<_>>();
            islands.push(CoverageMeasurementIslandEvidence::new(
                island_id,
                component_id,
                representative_face_id,
                faces.into_boxed_slice(),
            ));
            start = end;
        }
        islands.sort_unstable_by_key(|island| {
            (
                island.island_id(),
                island.component_id(),
                island.representative_face_id(),
            )
        });
        let uncovered_sample_indices = std::mem::take(&mut assessment.uncovered_sample_indices);
        // SPEC-A4 §6 clause 1 and §B.5 clause 3. Both quantities are already
        // computed by the assessment above and were both being discarded; the
        // objective is a struct copy and the uncensored total is one pass over
        // the uncovered rows this measurement already owns. No new geometry
        // work, no new allocation, no new charge.
        let objective = assessment.objective;
        let uncensored_uncovered_mm2 = uncovered_sample_indices
            .iter()
            .filter_map(|&index| self.coverage_samples().get(index))
            .map(|sample| sample.area_mm2)
            .sum();
        let mut patches = assessment
            .patches
            .into_iter()
            .map(|patch| {
                let gap = if patch.worst_gap_mm.is_infinite() {
                    CoverageGap::NoRoutedContact
                } else {
                    CoverageGap::Finite(Finite::new(patch.worst_gap_mm).map_err(|_| {
                        CoverageMeasurementError::AssessmentRefusal {
                            reason: "non-finite patch gap",
                        }
                    })?)
                };
                Ok(CoverageMeasurementPatchEvidence::new(
                    patch.island_id,
                    patch.component_id,
                    patch.representative_face_id,
                    patch.sorted_face_ids.into_boxed_slice(),
                    Finite::new(patch.uncovered_area_mm2).map_err(|_| {
                        CoverageMeasurementError::AssessmentRefusal {
                            reason: "non-finite patch area",
                        }
                    })?,
                    patch.cut_ambiguous,
                    patch.assessment_failed,
                    gap,
                ))
            })
            .collect::<Result<Vec<_>, CoverageMeasurementError>>()?;
        patches.sort_unstable_by_key(|patch| {
            (
                patch.island_id(),
                patch.component_id(),
                patch.representative_face_id(),
            )
        });
        Ok(CoverageMeasurement::new(
            self.snapshot_digest,
            request,
            CoverageMeasurementResourcePlan::from_summary(CoverageMeasurementResourceSummary {
                route_membership_sum: memberships,
                sequential_route_membership_peak: sequential_membership_peak,
                indexed_point_count: indexed_points,
                sequential_indexed_point_peak: indexed_point_peak,
                query_row_count: query_rows,
                distance_work,
                logical_peak_bytes: preflight.logical_peak_bytes,
                assessment_work: preflight.assessment.operations,
                assessment_scratch_bytes: preflight.assessment.scratch_bytes,
                assessment_evidence_bytes: preflight.assessment.evidence_bytes,
                assessment_phase_peak_bytes: preflight.assessment.phase_peak_bytes,
            }),
            CoverageMeasurementEvidence::new(
                islands.into_boxed_slice(),
                patches.into_boxed_slice(),
            ),
            uncovered_sample_indices,
            objective,
            uncensored_uncovered_mm2,
        ))
    }
    #[must_use]
    pub const fn mesh(&self) -> &'a Mesh {
        self.mesh
    }

    #[must_use]
    pub const fn topology(&self) -> &'a FaceTopology {
        self.topology
    }

    #[must_use]
    pub fn coverage_samples(&self) -> &'a [CoverageSample] {
        self.coverage_samples
    }

    /// The exact retained native query surface bound to this snapshot.
    #[must_use]
    pub const fn native_surface(&self) -> &'a NativeSurface {
        self.surface
    }

    #[must_use]
    pub fn final_live_members(
        &self,
    ) -> impl ExactSizeIterator<Item = &FinalLiveMemberEvidence> + '_ {
        self.final_live_members.iter()
    }

    pub fn routed_contacts(
        &self,
    ) -> impl ExactSizeIterator<Item = RoutedPhysicalContactEvidence<'a>> + '_ {
        self.routed_contacts.iter().copied()
    }

    #[must_use]
    pub const fn digest(&self) -> u64 {
        self.snapshot_digest
    }

    /// Minimal, fully integrity-valid snapshot for bounded transit/SCAR unit
    /// tests. Production construction remains exclusively on the lowering
    /// path above.
    #[cfg(test)]
    pub(crate) fn transit_test_fixture(
        mesh: &'a Mesh,
        topology: &'a FaceTopology,
        surface: &'a NativeSurface,
        mesh_digest: &'a str,
        final_live_members: Vec<FinalLiveMemberEvidence>,
    ) -> Self {
        let routed_contacts = Vec::new();
        let routed_route_memberships = Vec::new();
        let coverage_samples: &'a [CoverageSample] = &[];
        let named_islands: &'a [SupportIsland] = &[];
        let final_live_members_digest = final_live_members_digest(&final_live_members);
        let geometry_digest = AnchorGeometryAuthority::new(mesh, topology, surface)
            .expect("transit test geometry authority")
            .digest();
        let mut snapshot = Self {
            mesh,
            topology,
            surface,
            coverage_samples,
            named_islands,
            routed_contacts,
            routed_route_memberships,
            final_live_members,
            mesh_digest,
            topology_digest: topology.content_digest(),
            plan_digest: 0,
            load_digest: 0,
            geometry_digest,
            forest_digest: 0,
            transcript_digest: 0,
            regular_digest: 0,
            coverage_digest: coverage_samples_digest(coverage_samples),
            named_islands_digest: named_islands_digest(named_islands),
            lowered_primitives_digest: 0,
            routed_projection_digest: routed_projection_digest(&[]),
            final_live_members_digest,
            snapshot_digest: 0,
        };
        snapshot.snapshot_digest = verification_snapshot_digest(&snapshot);
        snapshot
    }

    /// Rechecks the immutable bindings without reconstructing any geometry,
    /// route, or coverage decision.
    pub fn validate_integrity(&self) -> Result<(), SupportVerificationError> {
        let mesh_digest = mesh_fingerprint(self.mesh)
            .map_err(|_| SupportVerificationError::TopologyMeshMismatch)?;
        let geometry = AnchorGeometryAuthority::new(self.mesh, self.topology, self.surface)
            .map_err(|_| SupportVerificationError::TopologyMeshMismatch)?;
        if mesh_digest != self.mesh_digest
            || self.topology.mesh_fingerprint() != self.mesh_digest
            || self.topology.content_digest() != self.topology_digest
            || geometry.digest() != self.geometry_digest
            || plinth_mesh::validate_face_topology(self.topology).is_err()
            || coverage_samples_digest(self.coverage_samples) != self.coverage_digest
            || named_islands_digest(self.named_islands) != self.named_islands_digest
        {
            return Err(SupportVerificationError::SnapshotDigestMismatch);
        }
        // The sidecar is proved well-formed and indexed exactly once per
        // snapshot, then threaded through every membership check below. It was
        // re-proved per routed contact inside `routed_logical_routes`, and each
        // of those re-proofs was followed by a linear membership walk of the
        // same sidecar.
        let island_index = IslandFaceIndex::build_named(self.named_islands, self.topology)
            .map_err(|_| SupportVerificationError::SnapshotDigestMismatch)?;
        if self.routed_contacts.len() != self.routed_route_memberships.len()
            || self.routed_contacts.iter().any(|contact| {
                !self
                    .routed_route_memberships
                    .iter()
                    .any(|membership| membership.contact.contact == contact.contact)
            })
            || !validate_routed_memberships(
                &self.routed_route_memberships,
                &island_index,
                self.topology,
            )
            || routed_projection_digest(&self.routed_route_memberships)
                != self.routed_projection_digest
            || final_live_members_digest(&self.final_live_members) != self.final_live_members_digest
            || verification_snapshot_digest(self) != self.snapshot_digest
        {
            return Err(SupportVerificationError::SnapshotDigestMismatch);
        }
        Ok(())
    }
}

fn measurement_plan_matches_preflight(
    preflight: CoverageMeasurementPreflight,
    planned: &crate::specialized::CoverageRouteNearestPlan,
) -> bool {
    let (
        memberships,
        sequential_membership_peak,
        indexed_points,
        indexed_point_peak,
        query_rows,
        distance_work,
    ) = planned.measurement_counts();
    (
        memberships,
        sequential_membership_peak,
        indexed_points,
        indexed_point_peak,
        query_rows,
        distance_work,
        planned.route_plan_count(),
        planned.retained_base_bytes(),
        planned.max_knn_peak_bytes(),
    ) == (
        preflight.memberships,
        preflight.sequential_membership_peak,
        preflight.indexed_points,
        preflight.sequential_indexed_peak,
        preflight.query_rows,
        preflight.distance_work,
        preflight.route_plan_count,
        preflight.route_retained_base_bytes,
        preflight.route_knn_peak_bytes,
    )
}

/// Complete typed caller authority for the sealed preparation stage.
///
/// Construction is crate-private and every field remains immutable. Result-changing values live
/// in their existing registered config types; this type introduces no support-physics defaults.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SupportPreparationAuthority<'a> {
    layers: &'a LayerPlan,
    perception_config: SupportPerceptionConfig<'a>,
    candidate_config: CandidatePayloadConfig,
    reservations: &'a [SpecializedOwnerReservation],
    perception_limits: GenericPerceptionLimits,
    selection_config: ContactSelectionConfig<'a>,
    specialized_limits: SpecializedLimits,
    construction_limits: ContactConstructionLimits,
    brace_penetration_limits: PenetrationBatchLimits,
    raft_limits: RaftLimits,
    max_primitives: usize,
    max_primitive_bytes: usize,
}

impl<'a> SupportPreparationAuthority<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        layers: &'a LayerPlan,
        perception_config: SupportPerceptionConfig<'a>,
        candidate_config: CandidatePayloadConfig,
        reservations: &'a [SpecializedOwnerReservation],
        perception_limits: GenericPerceptionLimits,
        selection_config: ContactSelectionConfig<'a>,
        specialized_limits: SpecializedLimits,
        construction_limits: ContactConstructionLimits,
        brace_penetration_limits: PenetrationBatchLimits,
        raft_limits: RaftLimits,
        max_primitives: usize,
        max_primitive_bytes: usize,
    ) -> Self {
        Self {
            layers,
            perception_config,
            candidate_config,
            reservations,
            perception_limits,
            selection_config,
            specialized_limits,
            construction_limits,
            brace_penetration_limits,
            raft_limits,
            max_primitives,
            max_primitive_bytes,
        }
    }
}

/// Stable evidence retained at the sealed preparation boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedSupportDigests {
    mesh: String,
    generic: u64,
    regular: u64,
    plan: u64,
    loads: u64,
    geometry: u64,
    initial_forest: u64,
    /// PR-8 — the contact-attestation transcript.
    ///
    /// This is retained ALONGSIDE `initial_forest` rather than folded into it
    /// because the forest digest structurally cannot see an attestation
    /// regression: an attested contact and an unattested-but-still-live one
    /// are the same geometry, and a refusal that silently stopped firing
    /// leaves no geometric trace at all. Without a separate digest, deleting
    /// the owner's governing requirement would move nothing.
    contact_attestation: u64,
    prepared: u64,
}

impl PreparedSupportDigests {
    pub(crate) fn mesh(&self) -> &str {
        &self.mesh
    }
    pub(crate) const fn generic(&self) -> u64 {
        self.generic
    }
    pub(crate) const fn regular(&self) -> u64 {
        self.regular
    }
    pub(crate) const fn plan(&self) -> u64 {
        self.plan
    }
    pub(crate) const fn loads(&self) -> u64 {
        self.loads
    }
    pub(crate) const fn geometry(&self) -> u64 {
        self.geometry
    }
    pub(crate) const fn initial_forest(&self) -> u64 {
        self.initial_forest
    }
    pub(crate) const fn contact_attestation(&self) -> u64 {
        self.contact_attestation
    }
    pub(crate) const fn prepared(&self) -> u64 {
        self.prepared
    }
}

/// Opaque, move-only hand-off to the later support controller.
///
/// The surface and topology are retained rather than reconstructed from raw arrays. The anchor
/// authority itself borrows those values, so safe Rust cannot store it self-referentially; the
/// sealed geometry digest proves that the retained pair is the one used for initial construction.
/// Immutable authority prepared once for a support run.
///
/// This intentionally owns the one retained mesh/topology/native-surface trio and all generic
/// and regular planning authority.  Later route state borrows nothing from a replacement geometry
/// or a rerun preparation pass.
pub(crate) struct SupportRunContext<'a> {
    mesh: Mesh,
    hollow_history: HollowHistory,
    request: SupportRequest,
    frame: BuildFrame,
    authority: SupportPreparationAuthority<'a>,
    topology: FaceTopology,
    surface: NativeSurface,
    generic: GenericContactPlan,
    regular: SelectedContacts,
    brace_penetration_limits: PenetrationBatchLimits,
    raft_limits: RaftLimits,
    digests: PreparedSupportDigests,
    /// **Task #36 Stage C (design §2.4 item 1) — the run-invariant anchor pool.**
    ///
    /// `SurfaceAnchorPool::from_geometry` reads the retained geometry trio
    /// above and the perception config's delicacy sidecar, and neither moves
    /// for the whole run. It was nevertheless rebuilt twice per attempt —
    /// once in the Routing stage, once in Rewire — and the coverage-closure
    /// controller makes many attempts against this one context.
    ///
    /// The slot starts empty and is filled by whichever stage asks first, so
    /// a first-build refusal is still raised by the stage that has always
    /// raised it. It holds owned rows and borrows nothing, which is why a
    /// context that owns the `NativeSurface` those rows describe can hold it
    /// without becoming self-referential.
    anchor_pool: Option<SurfaceAnchorPoolMemo>,
}

impl<'a> SupportRunContext<'a> {
    /// Lend the anchor-pool slot out for one stage.
    ///
    /// A stage holds `&SupportRunContext` for its whole body — `geometry`,
    /// `pool` and `authority` all borrow it — so it cannot also hold
    /// `&mut self.anchor_pool`. Moving the slot OUT before the shared borrow
    /// begins is what makes the two coexist, with no interior mutability
    /// anywhere in the retained authority (the property
    /// `validate_against_hoisted` and the FU-32 J1 digest threading both rest
    /// on). The caller returns it with [`Self::restore_anchor_pool`] on every
    /// exit, refusal included.
    fn take_anchor_pool(&mut self) -> Option<SurfaceAnchorPoolMemo> {
        self.anchor_pool.take()
    }

    /// The other half of [`Self::take_anchor_pool`]. Losing the memo would
    /// cost a rebuild and nothing else, so this is a performance obligation
    /// rather than a correctness one — but every exit honours it.
    fn restore_anchor_pool(&mut self, memo: Option<SurfaceAnchorPoolMemo>) {
        self.anchor_pool = memo;
    }

    pub(crate) const fn mesh(&self) -> &Mesh {
        &self.mesh
    }
    pub(crate) const fn hollow_history(&self) -> &HollowHistory {
        &self.hollow_history
    }
    pub(crate) const fn request(&self) -> &SupportRequest {
        &self.request
    }
    pub(crate) const fn frame(&self) -> BuildFrame {
        self.frame
    }
    pub(crate) const fn layers(&self) -> &LayerPlan {
        self.authority.layers
    }
    pub(crate) const fn generic(&self) -> &GenericContactPlan {
        &self.generic
    }
    pub(crate) const fn regular(&self) -> &SelectedContacts {
        &self.regular
    }
    pub(crate) const fn digests(&self) -> &PreparedSupportDigests {
        &self.digests
    }

    pub(crate) const fn brace_penetration_limits(&self) -> PenetrationBatchLimits {
        self.brace_penetration_limits
    }

    pub(crate) const fn raft_limits(&self) -> RaftLimits {
        self.raft_limits
    }

    pub(crate) const fn lowering_limits(&self) -> (usize, usize) {
        (
            self.authority.max_primitives,
            self.authority.max_primitive_bytes,
        )
    }

    /// Rebind the retained immutable geometry without cloning or replacing either component.
    pub(crate) fn geometry(&self) -> Result<AnchorGeometryAuthority<'_>, AnchorSearchError> {
        let geometry = AnchorGeometryAuthority::new(&self.mesh, &self.topology, &self.surface)?;
        if geometry.digest() != self.digests.geometry {
            return Err(AnchorSearchError::TopologyMeshMismatch);
        }
        Ok(geometry)
    }

    /// SPEC-A4 §A.5 — the five plan-dependent digest fields
    /// [`prepared_run_for_plan`] stamps, captured so a rejected trial can be
    /// rolled back to the accepted state's own preparation.
    pub(crate) const fn prepared_digests(&self) -> PreparedDigestSnapshot {
        PreparedDigestSnapshot {
            plan: self.digests.plan,
            loads: self.digests.loads,
            initial_forest: self.digests.initial_forest,
            contact_attestation: self.digests.contact_attestation,
            prepared: self.digests.prepared,
        }
    }

    /// Re-stamp exactly those five fields.
    ///
    /// A trial rejected at Routing, Rewire, Brace, Structure or Lowering hands
    /// this context back stamped with the REJECTED CHILD's digests. Today that
    /// self-heals only because the next call is always
    /// [`prepared_run_for_plan`]; under one-winner transactions the controller
    /// assembles refusal evidence BETWEEN trials, and evidence read from a
    /// stale digest would describe a plan that does not exist. So the restore
    /// is explicit rather than an invariant nobody states.
    pub(crate) const fn restore_prepared_digests(&mut self, snapshot: PreparedDigestSnapshot) {
        self.digests.plan = snapshot.plan;
        self.digests.loads = snapshot.loads;
        self.digests.initial_forest = snapshot.initial_forest;
        self.digests.contact_attestation = snapshot.contact_attestation;
        self.digests.prepared = snapshot.prepared;
    }
}

/// The five digest fields [`prepared_run_for_plan`] stamps on the context, as a
/// value. SPEC-A4 §A.5's rollback restores exactly these and nothing else:
/// every other piece of per-attempt state either lives in the plan (which is a
/// persistent value, so rollback is "do not rebind it") or is rebuilt inside
/// the next `prepared_run_for_plan`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PreparedDigestSnapshot {
    plan: u64,
    loads: u64,
    initial_forest: u64,
    contact_attestation: u64,
    prepared: u64,
}

/// Opaque by design.
///
/// The context exists because exactly one mesh, topology and `NativeSurface`
/// grid may be on the machine at a time (`runner.rs`'s memory-critical seam), so
/// there is nothing here anyone should want printed. It carries a `Debug` at all
/// only because SPEC-A4 §A.3's recoverable stage seam returns
/// `Err((SupportRunContext, E))`, and `Result::expect` needs the whole `Err` to
/// be printable.
impl std::fmt::Debug for SupportRunContext<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SupportRunContext")
            .field("prepared_digest", &self.digests.prepared)
            .finish_non_exhaustive()
    }
}

/// Opaque, move-only hand-off to the later support controller.
///
/// Its context is immutable and its two prepared plan artifacts are transferred exactly once into
/// the routed state. This preserves the established preparation API without retaining a second
/// geometry/index authority.
pub(crate) struct PreparedSupportRun<'a> {
    context: SupportRunContext<'a>,
    plan: SpecializedContactPlan,
    initial: InitialContactBuild,
}

impl<'a> PreparedSupportRun<'a> {
    pub(crate) const fn mesh(&self) -> &Mesh {
        self.context.mesh()
    }
    pub(crate) const fn hollow_history(&self) -> &HollowHistory {
        self.context.hollow_history()
    }
    pub(crate) const fn request(&self) -> &SupportRequest {
        self.context.request()
    }
    pub(crate) const fn frame(&self) -> BuildFrame {
        self.context.frame()
    }
    pub(crate) const fn layers(&self) -> &LayerPlan {
        self.context.layers()
    }
    pub(crate) const fn generic(&self) -> &GenericContactPlan {
        self.context.generic()
    }
    pub(crate) const fn regular(&self) -> &SelectedContacts {
        self.context.regular()
    }
    pub(crate) const fn plan(&self) -> &SpecializedContactPlan {
        &self.plan
    }
    pub(crate) fn loads(&self) -> Result<&FinalizedContactLoads, SpecializedError> {
        self.plan.finalized_loads()
    }
    pub(crate) const fn initial(&self) -> &InitialContactBuild {
        &self.initial
    }
    pub(crate) const fn digests(&self) -> &PreparedSupportDigests {
        self.context.digests()
    }
    pub(crate) fn geometry(&self) -> Result<AnchorGeometryAuthority<'_>, AnchorSearchError> {
        self.context.geometry()
    }

    /// Consume the prepared authority in the one sealed initial-terminal CCO
    /// transaction.  The frozen initial routing order is the only cadence;
    /// all route-shaped authority is rechecked before the first native query
    /// or forest transition.
    #[allow(
        clippy::result_large_err,
        reason = "the Ok variant carries the same SupportRunContext by construction: \
                  it is MOVED through the typestate either way, so boxing the Err \
                  would allocate on every refusal and save nothing"
    )]
    pub(crate) fn route_initial_cco(
        self,
        // **Stage B.** The pass's steering, borrowed. `None` is every pass of
        // every model that has never abandoned, and it is also every trial of
        // the repair loop: the retry wraps the ATTEMPT-0 chain call only
        // (design §4 Stage B piece 4), so the trial path keeps today's
        // `restore_prepared_digests` / `into_trial_rollback` behaviour exactly.
        steering: Option<&crate::legalization::LegalizationSteering>,
    ) -> Result<RoutedSupportRun<'a>, (SupportRunContext<'a>, SupportRoutingError)> {
        let limits = match RouteCcoWorkLimits::registered() {
            Ok(limits) => limits,
            Err(error) => return Err((self.context, error)),
        };
        self.route_initial_cco_with_limits(limits, steering)
    }

    /// The routing transaction, with its execution caps supplied rather than
    /// read.
    ///
    /// FU-32 J5 split this out so the caps arrive as a parameter, exactly as
    /// every other stage's limits do — which is also what lets a test drive the
    /// REAL routing path into the new refusal on real geometry, instead of
    /// proving the arithmetic in isolation and trusting the wiring by
    /// inspection. Production has one caller and it passes the registered caps.
    #[allow(
        clippy::result_large_err,
        reason = "the Ok variant carries the same SupportRunContext by construction: \
                  it is MOVED through the typestate either way, so boxing the Err \
                  would allocate on every refusal and save nothing"
    )]
    fn route_initial_cco_with_limits(
        self,
        limits: RouteCcoWorkLimits,
        steering: Option<&crate::legalization::LegalizationSteering>,
    ) -> Result<RoutedSupportRun<'a>, (SupportRunContext<'a>, SupportRoutingError)> {
        let PreparedSupportRun {
            mut context,
            plan,
            initial,
        } = self;
        // Stage C: lend the run-invariant anchor pool to the stage, and take
        // it back on both exits. See `SupportRunContext::take_anchor_pool`.
        let mut anchor_pool = context.take_anchor_pool();
        let routed =
            route_initial_cco_state(&context, &mut anchor_pool, plan, initial, limits, steering);
        context.restore_anchor_pool(anchor_pool);
        match routed {
            Ok(state) => Ok(RoutedSupportRun { context, state }),
            Err(error) => Err((context, error)),
        }
    }
}

/// SPEC-A4 §A.3 — the routing cadence, factored so the stage above can hand the
/// context back on the error path.
///
/// **Why a split rather than a hand-written unwind at every `?`.** The design
/// budgeted this as "the single largest piece of hand-written work in this
/// change", because `geometry`, `pool` and `authority` all borrow `context` and
/// each early return would have to end those borrows at exactly the right
/// point. Taking `&SupportRunContext` here does that once, structurally: every
/// borrow ends when this function returns, so the caller owns the context again
/// with no lifetime reasoning per exit and no chance of an exit that forgets.
/// Nothing about WHAT the stage does moves — the body below is the previous
/// body, `?` for `?`.
///
/// It also cannot leak an inconsistent typestate, which is the reason v1's
/// `Result<T, (Self, E)>` was rejected: `initial` is consumed here and a
/// half-grown forest is never handed back to anyone.
fn route_initial_cco_state(
    context: &SupportRunContext<'_>,
    anchor_pool: &mut Option<SurfaceAnchorPoolMemo>,
    plan: SpecializedContactPlan,
    mut initial: InitialContactBuild,
    limits: RouteCcoWorkLimits,
    steering: Option<&crate::legalization::LegalizationSteering>,
) -> Result<RoutedPlanState, SupportRoutingError> {
    {
        // Bind these three retained fields directly so the independent
        // initial forest remains available for the serial transaction below.
        let geometry =
            AnchorGeometryAuthority::new(&context.mesh, &context.topology, &context.surface)?;
        if geometry.digest() != context.digests.geometry {
            return Err(SupportRoutingError::GeometryDigestMismatch);
        }
        let loads = plan.finalized_loads()?;
        context.validate_initial_cco_preflight(&plan, &initial, &geometry, loads)?;

        // Constructing the exterior pool is a bounded, fail-closed resource
        // staging step. It uses the same required S0 sidecar that produced
        // the contact plan; no runner-local anchor policy exists.
        let delicacy = context
            .authority
            .perception_config
            .delicacy_by_face
            .ok_or(SupportRoutingError::MissingAnchorDelicacy)?;
        let anchor_config = AnchorSearchConfig::current()?;
        CcoController::validate_initial_route_config()?;
        // Task #36 Stage C: built here on the first attempt, borrowed on
        // every later one. The rows are a function of `geometry` and
        // `delicacy` alone, both fixed for the run, and
        // `VerifiedRoutingAuthority::compute` below still re-hashes every row
        // against this pool's sealed digest before the first terminal.
        let pool = SurfaceAnchorPool::memoized(anchor_pool, &geometry, delicacy)?;

        // FU-32 phase J1.  `plan`, `loads`, `pool` and `geometry` are immutable
        // for the whole cadence below, in types with no interior mutability, so
        // their digests are hashed ONCE here and threaded — the G2
        // `VerifiedMeshFingerprint` precedent.  Every staleness comparison
        // stays at its own site; only the re-hashing behind them is gone.
        let authority = VerifiedRoutingAuthority::compute(&plan, loads, &pool, &geometry)?;

        let mut controller = CcoController::default();
        let mut cadence = initial.routing_order().specialized_terminals().to_vec();
        cadence.extend_from_slice(initial.routing_order().regular_terminals());

        // FU-32 phase J5 (design D3, F8's debt).  Routing used to be entirely
        // uncharged -- no Limit, no counter, no clock -- so an instance too big
        // to route could only be stopped by an external kill.  Charge the real
        // Theta(N x P) work now, from the shape, BEFORE the first terminal:
        // `cadence` entries each classify all `pool.rows()` exterior samples
        // and retain at most one refusal row per sample.  The charge lands here
        // rather than in J1/J2 on purpose -- pricing the loop before the
        // redundant hashing was removed would have written the defect into a
        // budget.
        let charge = RouteCcoWorkCharge::compute(cadence.len(), pool.rows().len())?;
        charge.preflight(limits)?;
        crate::census_record!(crate::census::record_route_cco_charge(
            charge.cadence(),
            charge.pool_rows(),
            charge.seat_evaluations(),
            charge.seat_refusal_rows(),
        ));

        for terminal in cadence {
            let ledger = initial
                .forest()
                .terminal_ledger(terminal)
                .ok_or(SupportRoutingError::MissingTerminalLedger { terminal })?;
            if ledger.state == TerminalLedgerState::Quarantined {
                controller.advance_skipped_terminal(initial.forest(), ledger)?;
                continue;
            }

            let request = TerminalCcoRequest::bind(initial.forest(), terminal, context.frame)?;
            // FU-32 J1: the token's contact index replaces an O(|contacts|)
            // linear scan run once per cadence entry.
            let contact = authority
                .contact(u64::from(terminal))
                .ok_or(SupportRoutingError::MissingPhysicalContact { terminal })?;
            controller
                .grow_terminal_with_finalized_anchor(
                    initial.forest_mut(),
                    &request,
                    FinalizedAnchorTerminalRoute {
                        authority: &authority,
                        contact,
                        contact_id: u64::from(terminal),
                        config: anchor_config,
                        // PR-8/OD-9 (F3). The seat attestation claims ONE PROCESS
                        // RESOLUTION, and a layer height is a property of this run
                        // rather than of the registry, so it travels on the route.
                        process_resolution_mm: crate::contact::ContactPolicy::process_resolution_of(
                            context.layers(),
                        ),
                    },
                    steering,
                )
                .map_err(|error| match error {
                    // **Stage B.** Lifted out of the transparent `Cco` wrapper at
                    // the stage boundary for the same reason Stage A lifted the two
                    // brace refusals: the retry loop matches one arm rather than
                    // parsing a string, and an operator reads WHICH refusal
                    // happened without unwrapping.
                    CcoError::SteeringSpent { terminal_id } => SupportRoutingError::SteeringSpent {
                        terminal: terminal_id,
                    },
                    other => SupportRoutingError::Cco(other),
                })?;
        }
        // FU-32 J5 — the receipt, held against the charge above before the
        // transcript is sealed. `executed <= charged` on both resources, both
        // recorded (the G6c idiom: a meter beside the charge, never instead of
        // it).
        let meter = controller.route_work_meter();
        charge.covers_execution(meter)?;
        crate::census_record!(crate::census::record_route_cco_execution(
            meter.seat_evaluations,
            meter.seat_refusal_rows,
        ));

        let transcript = controller.finalize_transcript(&initial, &plan, loads, &geometry)?;
        transcript.validate_integrity()?;
        // PR-8/OD-9 (F3). The CCO model-anchor family's seat-attestation chain,
        // extending the stub family's sealed head. Held on the route state, not
        // in the CCO transcript: the transcript records what the attestation
        // DID (a refusal appears as an `anchor_refusals` row) while this records
        // that it RAN and what it measured, and those regress independently.
        let contact_attestation_chain = extend_contact_attestation_chain(
            context.digests.contact_attestation,
            CONTACT_ATTESTATION_STAGE_ROUTE_CCO,
            controller.contact_attestation_digest(),
        );
        // SPEC-A3-A4 §6, at the point the clause names: the cadence has
        // sealed its transcript and NOTHING downstream has run, so this is
        // the last moment before "rewire, bracing, carried-load repartition,
        // or coverage judgment".
        let accepted = accepted_contact_set_after_routing(&plan, &transcript, initial.forest())?;
        Ok(RoutedPlanState {
            plan,
            accepted,
            initial,
            controller,
            transcript,
            contact_attestation_chain,
        })
    }
}

/// F3 — the stage tags of the contact-attestation chain, in run order.
///
/// The chain starts at `PreparedSupportDigests::contact_attestation`, the stub
/// family's sealed head, and is extended once per stage that mints a model
/// contact. Tags are distinct so two stages producing the same transcript are
/// not the same link.
const CONTACT_ATTESTATION_STAGE_ROUTE_CCO: u8 = 0x10;
const CONTACT_ATTESTATION_STAGE_REWIRE_CCO: u8 = 0x11;
const CONTACT_ATTESTATION_STAGE_BRACE_RUNG: u8 = 0x12;

/// F3 — extend the run's contact-attestation chain by one stage.
///
/// The chain is what makes "all three families are attested" checkable at
/// runtime. A forest digest cannot see an attestation regression in ANY of the
/// three families, for the same reason in each case: an attested contact and an
/// unattested one are the same geometry, and a refused CANDIDATE leaves no
/// geometric trace at all.
fn extend_contact_attestation_chain(head: u64, stage: u8, transcript: u64) -> u64 {
    let mut hash = Fnv::new();
    hash.u64(head);
    hash.u64(u64::from(stage));
    hash.u64(transcript);
    hash.finish()
}

impl SupportRunContext<'_> {
    fn validate_initial_cco_preflight(
        &self,
        plan: &SpecializedContactPlan,
        initial: &InitialContactBuild,
        geometry: &AnchorGeometryAuthority<'_>,
        loads: &FinalizedContactLoads,
    ) -> Result<(), SupportRoutingError> {
        // This repeats the planner's complete immutable binding before CCO
        // can make any query or forest mutation.  In particular it covers
        // candidate/refusal limits, specialized resource limits, and the
        // plan-owned finalized load artifact rather than accepting digests as
        // a substitute for the real authority.
        plan.validate_binding(
            &self.mesh,
            &self.authority.perception_config,
            self.frame,
            self.authority.layers,
            &self.authority.candidate_config,
            self.authority.reservations,
            self.authority.perception_limits,
            &self.generic,
            &self.regular,
            &self.authority.selection_config,
            self.authority.specialized_limits,
        )?;
        if plan.digest() != self.digests.plan
            || plan.finalized_load_plan_digest() != plan.digest()
            || loads.digest() != self.digests.loads
        {
            return Err(SupportRoutingError::PlanOrLoadDigestMismatch);
        }
        geometry.validate_mesh(&self.mesh)?;
        if geometry.digest() != self.digests.geometry {
            return Err(SupportRoutingError::GeometryDigestMismatch);
        }
        if initial.plan_digest() != plan.digest()
            || initial.load_digest() != loads.digest()
            || initial.native_digest() != geometry.digest()
            || initial.initial_forest_digest() != Forest::new().digest()
            || initial.post_quarantine_forest_digest() != self.digests.initial_forest
            || initial.forest().digest() != self.digests.initial_forest
            // PR-8. The attestation transcript is checked here for the same
            // reason it is retained separately: the two forest-digest clauses
            // above cannot detect an attestation that stopped happening.
            || initial.contact_attestation_digest() != self.digests.contact_attestation
            || !initial.all_terminals_registered_before_quarantine()
        {
            return Err(SupportRoutingError::InitialAuthorityMismatch);
        }

        let routing = initial.routing_order();
        let mut expected = BTreeSet::new();
        for terminal in routing
            .specialized_terminals()
            .iter()
            .chain(routing.regular_terminals())
            .copied()
        {
            if !expected.insert(terminal)
                || !initial
                    .terminals()
                    .iter()
                    .any(|evidence| evidence.terminal == terminal)
                || plan
                    .contacts()
                    .iter()
                    .filter(|contact| contact.contact_id() == u64::from(terminal))
                    .count()
                    != 1
                || initial.forest().terminal_ledger(terminal).is_none()
            {
                return Err(SupportRoutingError::TerminalNamespaceMismatch { terminal });
            }
        }
        if expected.len() != initial.terminals().len() || expected.len() != plan.contacts().len() {
            return Err(SupportRoutingError::TerminalNamespaceCardinalityMismatch);
        }
        Ok(())
    }
}

/// Route-only state, formed by consuming prepared plan state once.
///
/// Keeping every mutable/terminal artifact together makes the one-way transition explicit:
/// generic preparation and retained native geometry stay in `SupportRunContext`, while routing
/// owns the specialized plan, initial forest, controller, and finalized transcript.
struct RoutedPlanState {
    /// The plan the cadence ROUTED. It is the authority the sealed transcript
    /// and the initial construction are pinned to, and it never changes after
    /// routing — `accepted` is derived from it, not in place of it.
    plan: SpecializedContactPlan,
    /// SPEC-A3-A4 §6's accepted contact set: `plan` minus every unrouted
    /// regular contact, with the carried load repartitioned over the
    /// survivors.
    accepted: AcceptedContactSet,
    initial: InitialContactBuild,
    controller: CcoController,
    transcript: FinalizedCcoTranscript,
    /// PR-8/OD-9 (F3) — the run's contact-attestation chain after the routing
    /// stage: the stub family's sealed head extended by the CCO model-anchor
    /// seat transcript.
    contact_attestation_chain: u64,
}

/// Why a contact is not in the accepted contact set: the reason half of the
/// row/reason pair SPEC-A3-A4 §6 requires be ledgered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnroutedContactReason {
    /// Quarantined before the cadence ran. The initial quarantine record is
    /// its complete evidence; the transcript deliberately keeps no route.
    Quarantined,
    /// The cadence ran the terminal and no seat won, so the stub was retired.
    NoWinner,
}

/// One regular contact removed from the accepted contact set by SPEC-A3-A4
/// §6, ledgered with its exact row and reason.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnroutedRegularRemoval {
    contact_id: u64,
    terminal_id: u32,
    /// The exact candidate-pool row identity this contact occupied. §6 opens
    /// with "A4 preserves exact candidate-pool row identity throughout", so
    /// the ledger names the row rather than only the physical contact.
    route: crate::specialized::RegularContactRoute,
    reason: UnroutedContactReason,
    /// What the PRE-removal load partition was charging this contact — the
    /// model area that no support was carrying, and that the repartition over
    /// the survivors now has to place somewhere else.
    carried_area_mm2: f64,
}

impl UnroutedRegularRemoval {
    #[must_use]
    pub const fn contact_id(&self) -> u64 {
        self.contact_id
    }
    #[must_use]
    pub const fn terminal_id(&self) -> u32 {
        self.terminal_id
    }
    #[must_use]
    pub const fn route(&self) -> crate::specialized::RegularContactRoute {
        self.route
    }
    #[must_use]
    pub const fn reason(&self) -> UnroutedContactReason {
        self.reason
    }
    #[must_use]
    pub const fn carried_area_mm2(&self) -> f64 {
        self.carried_area_mm2
    }
}

/// One unrouted DEMAND-BEARING contact — a specialized minimum or an
/// island-birth obligation that the cadence did not route.
///
/// §6's next sentence says such a contact "is a hard failure". It is detected
/// and ledgered here, and it is deliberately NOT converted into a refusal in
/// this change; see [`AcceptedContactSet`] for the measurement that decides
/// that, which is an owner's call rather than this lane's.
#[derive(Clone, Debug, PartialEq)]
pub struct UnroutedSpecializedContact {
    contact_id: u64,
    terminal_id: u32,
    reason: UnroutedContactReason,
    kinds: Vec<crate::specialized::SpecializedDemandKind>,
}

impl UnroutedSpecializedContact {
    #[must_use]
    pub const fn contact_id(&self) -> u64 {
        self.contact_id
    }
    #[must_use]
    pub const fn terminal_id(&self) -> u32 {
        self.terminal_id
    }
    #[must_use]
    pub const fn reason(&self) -> UnroutedContactReason {
        self.reason
    }
    #[must_use]
    pub fn kinds(&self) -> &[crate::specialized::SpecializedDemandKind] {
        &self.kinds
    }
}

/// SPEC-A3-A4 §6's accepted contact set, derived once at the end of routing.
///
/// > Before rewire, bracing, carried-load repartition, or coverage judgment,
/// > every unrouted regular contact is ledgered with its exact row/reason and
/// > removed from the accepted contact set. An unrouted specialized minimum or
/// > island-birth contact is a hard failure.
///
/// **What this type carries and why it is not simply the routed plan.** The
/// routed plan is the authority the sealed transcript, the initial
/// construction and the forest are all pinned to; re-deriving it in place
/// would break every one of those seals, which is what they are for. So the
/// removal produces a CHILD plan — the accepted contact set — and both are
/// kept: `RoutedPlanState::plan` answers "what was routed", this answers
/// "what is accepted". Every downstream consumer that §6 names (rewire and
/// brace preflight binding, the carried-load repartition, and the coverage
/// judgment's snapshot) reads this one.
///
/// `reduced` is `None` exactly when nothing was removed. That is not an
/// optimisation of convenience: `begin_regular_edit` builds its own
/// `NativeSurface`, and a model with no unrouted regular contact must pay
/// nothing and move no digest.
struct AcceptedContactSet {
    reduced: Option<SpecializedContactPlan>,
    removals: Vec<UnroutedRegularRemoval>,
    specialized_unrouted: Vec<UnroutedSpecializedContact>,
    /// Task #49, the split landing. Every model face the ACCEPTED set's own
    /// carried-load repartition charges to no contact — the faces of an island
    /// whose every contact went unrouted.
    ///
    /// **This is a disclosure, not a refusal, and the distinction is the whole
    /// ruling.** Load eligibility is provenance, not reach, so "no eligible
    /// owner has a contact" is not a measurement that the face is unsupported;
    /// the measurement that decides that is the routed-coverage judgment §6
    /// sequences AFTER this removal, which reports each island's uncovered area
    /// and rules on it under the registered A3 censor. Refusing here would
    /// pre-empt that measurement with a set-membership count — a bare tally
    /// deciding an outcome, which the physics-first doctrine forbids.
    ///
    /// Read beside `UnroutedRegularRemoval::carried_area_mm2`: that is area the
    /// PRE-removal partition charged to a contact with no support under it.
    /// Removing the contact moves that area onto whichever surviving contact is
    /// nearest and eligible, or — for these faces — onto nothing at all. Both
    /// directions size the surviving arms for what they truly carry.
    ownerless_faces: Vec<crate::perception::OwnerlessLoadFace>,
    /// The accepted plan's digest, sealed at derivation and re-checked at
    /// every preflight below.
    digest: u64,
}

impl AcceptedContactSet {
    /// The accepted contact set itself: the reduced child when anything was
    /// removed, otherwise the routed plan unchanged.
    fn plan<'p>(&'p self, routed: &'p SpecializedContactPlan) -> &'p SpecializedContactPlan {
        self.reduced.as_ref().unwrap_or(routed)
    }
}

#[derive(Debug, Error)]
pub(crate) enum AcceptedContactSetError {
    #[error(transparent)]
    Specialized(#[from] SpecializedError),
    #[error(transparent)]
    Unrouted(#[from] UnroutedTerminalError),
    #[error("the SPEC-A4 §6 accepted contact set's sealed digest is stale")]
    DigestMismatch,
    #[error("the SPEC-A4 §6 removal ledger does not reproduce from the sealed transcript")]
    LedgerMismatch,
    #[error("the SPEC-A4 §6 accepted contact set is not the routed plan minus its ledgered rows")]
    ContactSetMismatch,
}

/// Recompute-from-state tripwire for the accepted contact set (the F6 idiom
/// used at the transcript and controller boundaries).
///
/// It never trusts the stored value: the removal ledger is re-derived from
/// the sealed transcript and the live forest ledger, and the accepted plan is
/// re-checked to be exactly the routed plan minus those rows, contact by
/// contact. Only `carried_area_mm2` is allowed to differ, because the
/// repartition over the survivors is precisely what §6 orders after the
/// removal.
#[allow(
    clippy::float_cmp,
    reason = "a ledger check that the accepted set is the surviving planned \
              contacts VERBATIM.  Both sides are the same stored values; any \
              difference at all, including one ULP, is the mismatch this is \
              looking for"
)]
fn validate_accepted_contact_set(
    state: &RoutedPlanState,
    forest: &Forest,
) -> Result<(), AcceptedContactSetError> {
    let accepted = state.accepted.plan(&state.plan);
    if accepted.digest() != state.accepted.digest
        || accepted.finalized_load_plan_digest() != accepted.digest()
    {
        return Err(AcceptedContactSetError::DigestMismatch);
    }
    accepted.finalized_loads()?;

    let unrouted = finalized_unrouted_terminals(&state.transcript, forest)?;
    let routed_by_id = state
        .plan
        .contacts()
        .iter()
        .map(|contact| (contact.contact_id(), contact))
        .collect::<BTreeMap<_, _>>();
    let mut expected_removals = Vec::new();
    let mut expected_specialized = Vec::new();
    for terminal in unrouted {
        let contact = routed_by_id
            .get(&terminal.contact_id)
            .copied()
            .ok_or(AcceptedContactSetError::LedgerMismatch)?;
        if contact.demands().is_empty() {
            let route = contact
                .primary_regular()
                .ok_or(AcceptedContactSetError::LedgerMismatch)?
                .route();
            expected_removals.push(UnroutedRegularRemoval {
                contact_id: terminal.contact_id,
                terminal_id: terminal.terminal_id,
                route,
                reason: terminal.reason,
                carried_area_mm2: contact.carried_area_mm2(),
            });
        } else {
            expected_specialized.push(UnroutedSpecializedContact {
                contact_id: terminal.contact_id,
                terminal_id: terminal.terminal_id,
                reason: terminal.reason,
                kinds: contact
                    .demands()
                    .iter()
                    .map(crate::specialized::SpecializedDemand::kind)
                    .collect(),
            });
        }
    }
    if expected_removals != state.accepted.removals
        || expected_specialized != state.accepted.specialized_unrouted
    {
        return Err(AcceptedContactSetError::LedgerMismatch);
    }

    let removed = state
        .accepted
        .removals
        .iter()
        .map(|removal| removal.contact_id)
        .collect::<BTreeSet<_>>();
    if removed.len() != state.accepted.removals.len()
        || accepted.contacts().len() + removed.len() != state.plan.contacts().len()
    {
        return Err(AcceptedContactSetError::ContactSetMismatch);
    }
    let mut survivors = state
        .plan
        .contacts()
        .iter()
        .filter(|contact| !removed.contains(&contact.contact_id()));
    for kept in accepted.contacts() {
        let original = survivors
            .next()
            .ok_or(AcceptedContactSetError::ContactSetMismatch)?;
        if kept.contact_id() != original.contact_id()
            || kept.position_mm() != original.position_mm()
            || kept.normal() != original.normal()
            || kept.seat_provenance() != original.seat_provenance()
            || kept.primary_regular() != original.primary_regular()
            || kept.regular_routes() != original.regular_routes()
            || kept.demands() != original.demands()
        {
            return Err(AcceptedContactSetError::ContactSetMismatch);
        }
    }
    if survivors.next().is_some() {
        return Err(AcceptedContactSetError::ContactSetMismatch);
    }
    Ok(())
}

/// Apply SPEC-A3-A4 §6's removal clause at the one point the clause names.
///
/// Called from the routing stage after the transcript is sealed and before
/// anything downstream runs, so no rewire, brace, load repartition or
/// coverage judgment has yet seen a contact that never routed.
///
/// The unrouted set is read from [`finalized_unrouted_terminals`] — the same
/// function the structure stage's live-view filter reads — so the accepted
/// contact set and the live view can never disagree about which terminals
/// routed.
fn accepted_contact_set_after_routing(
    plan: &SpecializedContactPlan,
    transcript: &FinalizedCcoTranscript,
    forest: &Forest,
) -> Result<AcceptedContactSet, SupportRoutingError> {
    let unrouted = finalized_unrouted_terminals(transcript, forest)?;
    // One pass over the contacts, indexed once: the classification below is
    // O(unrouted log contacts), never O(unrouted x contacts).
    let contacts_by_id = plan
        .contacts()
        .iter()
        .map(|contact| (contact.contact_id(), contact))
        .collect::<BTreeMap<_, _>>();
    if contacts_by_id.len() != plan.contacts().len() {
        return Err(SupportRoutingError::TerminalNamespaceCardinalityMismatch);
    }
    let mut removals = Vec::new();
    let mut specialized_unrouted = Vec::new();
    for terminal in unrouted {
        let contact = contacts_by_id.get(&terminal.contact_id).copied().ok_or(
            SupportRoutingError::MissingPhysicalContact {
                terminal: terminal.terminal_id,
            },
        )?;
        if !contact.demands().is_empty() {
            specialized_unrouted.push(UnroutedSpecializedContact {
                contact_id: terminal.contact_id,
                terminal_id: terminal.terminal_id,
                reason: terminal.reason,
                kinds: contact
                    .demands()
                    .iter()
                    .map(crate::specialized::SpecializedDemand::kind)
                    .collect(),
            });
            continue;
        }
        let route = contact
            .primary_regular()
            .ok_or(SupportRoutingError::MissingPhysicalContact {
                terminal: terminal.terminal_id,
            })?
            .route();
        removals.push(UnroutedRegularRemoval {
            contact_id: terminal.contact_id,
            terminal_id: terminal.terminal_id,
            route,
            reason: terminal.reason,
            carried_area_mm2: contact.carried_area_mm2(),
        });
    }
    if removals.is_empty() {
        return Ok(AcceptedContactSet {
            ownerless_faces: plan.load_partition().ownerless_faces().to_vec(),
            reduced: None,
            removals,
            specialized_unrouted,
            digest: plan.digest(),
        });
    }
    // ONE edit for the whole unrouted set: `derive_child` replays the load
    // finalizer once, so the carried-load repartition §6 sequences after the
    // removal happens exactly once per run.
    let ids = removals
        .iter()
        .map(|removal| removal.contact_id)
        .collect::<Vec<_>>();
    // The repartition can now leave a face with no owning contact at all —
    // every load owner eligible for it lost every contact it had. That is not
    // an error: the face is charged to nobody, the survivors are sized for
    // what they truly carry, and `perception::OwnerlessLoadFace` carries the
    // disclosure out. See `AcceptedContactSet::ownerless_faces`.
    let reduced = plan
        .begin_regular_edit()?
        .remove_demand_free_regular_rows(&ids)
        .map_err(SupportRoutingError::Specialized)?;
    let digest = reduced.digest();
    Ok(AcceptedContactSet {
        ownerless_faces: reduced.load_partition().ownerless_faces().to_vec(),
        reduced: Some(reduced),
        removals,
        specialized_unrouted,
        digest,
    })
}

/// Opaque route-stage authority for the normalizer. It retains the exact prepared context and
/// sealed CCO state; no caller can replay an initial terminal after this value exists.
pub(crate) struct RoutedSupportRun<'a> {
    context: SupportRunContext<'a>,
    state: RoutedPlanState,
}

impl<'a> RoutedSupportRun<'a> {
    #[must_use]
    pub(crate) const fn mesh(&self) -> &Mesh {
        self.context.mesh()
    }

    #[must_use]
    pub(crate) const fn hollow_history(&self) -> &HollowHistory {
        self.context.hollow_history()
    }

    #[must_use]
    pub(crate) const fn request(&self) -> &SupportRequest {
        self.context.request()
    }

    #[must_use]
    pub(crate) const fn frame(&self) -> BuildFrame {
        self.context.frame()
    }

    #[must_use]
    pub(crate) const fn plan(&self) -> &SpecializedContactPlan {
        &self.state.plan
    }

    #[must_use]
    pub(crate) const fn initial(&self) -> &InitialContactBuild {
        &self.state.initial
    }

    #[must_use]
    pub(crate) const fn digests(&self) -> &PreparedSupportDigests {
        self.context.digests()
    }

    #[must_use]
    pub(crate) const fn forest(&self) -> &Forest {
        self.state.initial.forest()
    }

    #[must_use]
    pub(crate) const fn controller(&self) -> &CcoController {
        &self.state.controller
    }

    #[must_use]
    pub(crate) const fn transcript(&self) -> &FinalizedCcoTranscript {
        &self.state.transcript
    }

    /// SPEC-A4 §6's accepted contact set — the routed plan minus every
    /// unrouted regular contact, with the load repartitioned over the
    /// survivors.
    #[must_use]
    pub(crate) fn accepted_plan(&self) -> &SpecializedContactPlan {
        self.state.accepted.plan(&self.state.plan)
    }

    /// The §6 ledger: every removed regular contact, with its exact row and
    /// reason.
    #[must_use]
    pub(crate) fn unrouted_regular_removals(&self) -> &[UnroutedRegularRemoval] {
        &self.state.accepted.removals
    }

    /// Task #49: the faces the accepted set charges to NO contact — see
    /// [`AcceptedContactSet::ownerless_faces`].
    #[must_use]
    pub(crate) fn ownerless_faces(&self) -> &[crate::perception::OwnerlessLoadFace] {
        &self.state.accepted.ownerless_faces
    }

    /// Every unrouted DEMAND-BEARING contact this run found.
    #[must_use]
    pub(crate) fn unrouted_specialized_contacts(&self) -> &[UnroutedSpecializedContact] {
        &self.state.accepted.specialized_unrouted
    }

    /// Recover the context from a routed run that a test decided not to carry
    /// further. Production never abandons a routed run: it either continues to
    /// the next stage or takes the context back off a refusal.
    #[cfg(test)]
    pub(crate) fn into_context_for_test(self) -> SupportRunContext<'a> {
        self.context
    }

    /// Consume the sealed CCO route state in the one deterministic rewire
    /// transaction.  This rebinds the retained native surface and gathers the
    /// exterior anchor rows once; it never reconstructs mesh, topology, or a
    /// native index.
    #[allow(
        clippy::result_large_err,
        reason = "the Ok variant carries the same SupportRunContext by construction: \
                  it is MOVED through the typestate either way, so boxing the Err \
                  would allocate on every refusal and save nothing"
    )]
    pub(crate) fn rewire(
        self,
    ) -> Result<RewiredSupportRun<'a>, (SupportRunContext<'a>, SupportRewireError)> {
        let RoutedSupportRun { mut context, state } = self;
        // Stage C, the second of the two sites: same lend-and-return.
        let mut anchor_pool = context.take_anchor_pool();
        let rewired = rewire_state(&context, &mut anchor_pool, state);
        context.restore_anchor_pool(anchor_pool);
        match rewired {
            Ok(state) => Ok(RewiredSupportRun { context, state }),
            Err(error) => Err((context, error)),
        }
    }
}

/// SPEC-A4 §A.3 — the rewire transaction, factored for context recovery. See
/// [`route_initial_cco_state`] for why the split is the seam rather than a
/// hand-written unwind per `?`.
fn rewire_state(
    context: &SupportRunContext<'_>,
    anchor_pool: &mut Option<SurfaceAnchorPoolMemo>,
    mut state: RoutedPlanState,
) -> Result<RewiredPlanState, SupportRewireError> {
    {
        let geometry = context.geometry()?;
        let loads = state.plan.finalized_loads()?;
        context.validate_rewire_preflight(&state, &geometry, loads)?;

        let delicacy = context
            .authority
            .perception_config
            .delicacy_by_face
            .ok_or(SupportRewireError::MissingAnchorDelicacy)?;
        let anchor_config = AnchorSearchConfig::current()?;
        // The pool is the sole gather pass for rewire. `optimize` receives
        // its immutable contiguous rows and performs its own batched native
        // candidate/foot checks; there is no per-member runner query.
        //
        // Task #36 Stage C: the gather itself is the Routing stage's gather,
        // over the same fixed geometry and sidecar. `validate_against` below
        // is untouched and still re-hashes every row against the pool's own
        // sealed digest, so a memo whose contents moved refuses here.
        let pool = SurfaceAnchorPool::memoized(anchor_pool, &geometry, delicacy)?;
        pool.validate_against(&geometry)?;
        let report = optimize_rewire(
            state.initial.forest_mut(),
            RewireRequest {
                frame: context.frame,
                anchor_samples: pool.rows(),
                delicacy_by_face: Some(delicacy),
                anchor_config,
                geometry: &geometry,
                // PR-8/OD-9 (F3). See the routing stage: the seat attestation
                // claims one process resolution, a property of this run.
                process_resolution_mm: crate::contact::ContactPolicy::process_resolution_of(
                    context.layers(),
                ),
            },
        )?;
        state
            .initial
            .forest()
            .validate()
            .map_err(RewireError::from)?;
        let post_rewire_forest_digest = state.initial.forest().digest();
        // PR-8/OD-9 (F3). Rewire is the SECOND place the model-anchor family
        // proposes a seat, so the chain gains a link here too.
        let contact_attestation_chain = extend_contact_attestation_chain(
            state.contact_attestation_chain,
            CONTACT_ATTESTATION_STAGE_REWIRE_CCO,
            report.contact_attestation_digest,
        );

        Ok(RewiredPlanState {
            routed: state,
            rewire_report_digest: rewire_report_digest(&report),
            report,
            post_rewire_forest_digest,
            contact_attestation_chain,
        })
    }
}

/// Route state after deterministic topology rewire. The prior plan, initial
/// construction, controller, and finalized CCO transcript remain sealed in
/// place; this state adds only the rewire evidence and exact final digest.
struct RewiredPlanState {
    routed: RoutedPlanState,
    report: RewireReport,
    rewire_report_digest: u64,
    post_rewire_forest_digest: u64,
    /// PR-8/OD-9 (F3) — the contact-attestation chain after the rewire stage.
    contact_attestation_chain: u64,
}

/// Opaque hand-off after the rewire stage. Later stages can inspect evidence,
/// but cannot replay CCO or rewire the same routed state twice.
pub(crate) struct RewiredSupportRun<'a> {
    context: SupportRunContext<'a>,
    state: RewiredPlanState,
}

impl<'a> RewiredSupportRun<'a> {
    #[must_use]
    pub(crate) const fn mesh(&self) -> &Mesh {
        self.context.mesh()
    }

    #[must_use]
    pub(crate) const fn hollow_history(&self) -> &HollowHistory {
        self.context.hollow_history()
    }

    #[must_use]
    pub(crate) const fn request(&self) -> &SupportRequest {
        self.context.request()
    }

    #[must_use]
    pub(crate) const fn frame(&self) -> BuildFrame {
        self.context.frame()
    }

    #[must_use]
    pub(crate) const fn plan(&self) -> &SpecializedContactPlan {
        &self.state.routed.plan
    }

    #[must_use]
    pub(crate) const fn initial(&self) -> &InitialContactBuild {
        &self.state.routed.initial
    }

    #[must_use]
    pub(crate) const fn digests(&self) -> &PreparedSupportDigests {
        self.context.digests()
    }

    #[must_use]
    pub(crate) const fn forest(&self) -> &Forest {
        self.state.routed.initial.forest()
    }

    #[must_use]
    pub(crate) const fn controller(&self) -> &CcoController {
        &self.state.routed.controller
    }

    #[must_use]
    pub(crate) const fn transcript(&self) -> &FinalizedCcoTranscript {
        &self.state.routed.transcript
    }

    #[must_use]
    pub(crate) const fn rewire_report(&self) -> &RewireReport {
        &self.state.report
    }

    #[must_use]
    pub(crate) const fn post_rewire_forest_digest(&self) -> u64 {
        self.state.post_rewire_forest_digest
    }

    #[must_use]
    pub(crate) const fn rewire_report_digest(&self) -> u64 {
        self.state.rewire_report_digest
    }

    /// Consume the sealed rewire state in the frozen bracing fixed point.
    /// The retained surface, plate frame, and caller-owned batch resource are
    /// used directly; no geometry/index or bracing policy is reconstructed.
    #[allow(
        clippy::result_large_err,
        reason = "the Ok variant carries the same SupportRunContext by construction: \
                  it is MOVED through the typestate either way, so boxing the Err \
                  would allocate on every refusal and save nothing"
    )]
    pub(crate) fn brace(
        self,
    ) -> Result<BracedSupportRun<'a>, (SupportRunContext<'a>, SupportBraceError)> {
        let RewiredSupportRun { context, state } = self;
        match brace_state(&context, state) {
            Ok(state) => Ok(BracedSupportRun { context, state }),
            Err(error) => Err((context, error)),
        }
    }
}

/// **Stage B — join the condemnation class to the routing cadence's own record.**
///
/// Stage A's class answers *which terminals had their own chosen arm condemned*.
/// It cannot answer *what that arm was*, because a `MemberKey` in the
/// post-rewire forest is not the tournament's record of its own choice. The
/// sealed disposition is: `CcoController::sealed_routed_arm` returns the five
/// prefix digests the terminal's tournament read and the four fields it sealed
/// about the winner. One no-good per steerable terminal, minted from those two
/// halves.
///
/// **A terminal that is not sealed `Routed` mints nothing**, and neither does a
/// cascade victim. A victim's own arm was never measured illegal — its stub was
/// re-parented onto somebody else's arm and that arm was condemned. Giving it a
/// no-good would cost it a legal arm per pass for another member's failure
/// (§1.3), so victims are reported as victims and steer nothing.
///
/// **The Stage A escalation this stage inherits, disclosed rather than
/// guarded.** `cco.rs`'s rewire mints members owning `Owner::Contact(root.slot())`
/// — a MEMBER slot in the terminal-id namespace — so a numeric collision on a
/// walked rootward path could in principle attribute a rewire member to an
/// innocent sealed terminal. It cannot produce a wrong ANSWER: a mis-keyed
/// no-good may only suppress a choice, never retire a terminal, and the
/// confirmation round re-measures every arm it suppressed. The owner of every
/// arm that mints a no-good is therefore PRINTED, so the collision is a thing a
/// run log can be read for rather than a thing a structural invariant refuses —
/// and a structural invariant here would risk refusing a legitimate plate-descent
/// chain.
fn mint_no_goods(
    controller: &CcoController,
    class: &crate::brace::SealedCondemnationClass,
) -> crate::legalization::LegalizationAbandon {
    let mut learned = Vec::new();
    let mut own_arm_terminals = Vec::new();
    let mut cascade_victims = Vec::new();
    let mut measured = Vec::new();
    for terminal in &class.disturbed {
        if terminal.own_chosen_arms.is_empty() {
            cascade_victims.push(terminal.terminal_id);
            continue;
        }
        let Some((route, winner)) = controller.sealed_routed_arm(terminal.terminal_id) else {
            // Disturbed, its own arm condemned, and yet the record does not
            // seal it as `Routed` with a winner. Stage A already refused to
            // quarantine it; there is simply no arm to steer away from, so it
            // is disclosed as a victim rather than silently dropped.
            cascade_victims.push(terminal.terminal_id);
            continue;
        };
        own_arm_terminals.push(terminal.terminal_id);
        learned.push(crate::legalization::NoGood::new(
            terminal.terminal_id,
            crate::legalization::PrefixKey::of(route),
            crate::legalization::ArmIdentity::of(winner),
        ));
        // The MEASURED physics behind this refusal, taken from the class's own
        // rows rather than recomputed: the worst (most negative) gap among this
        // terminal's condemned own arms, with the radii the resize assigned to
        // that member. A steering pass discards it; a confirmation round keeps
        // it, because a retirement must show measured millimetres and not a
        // count (§1.3 verifier addition 1).
        //
        // Absent only if the class named an arm with no refusal row, which
        // `classify_sealed_condemnation` builds them together so cannot happen
        // — the zeroed row would be a visible lie rather than a silent one.
        let worst = class
            .measured_refusals
            .iter()
            .filter(|row| terminal.own_chosen_arms.contains(&row.member))
            .min_by(|left, right| left.gap_mm.total_cmp(&right.gap_mm));
        measured.push(worst.map_or(
            crate::legalization::ArmRefusalPhysics {
                gap_mm: f64::NAN,
                distance_clearance_mm: f64::NAN,
                axis_transit_illegal: false,
                terminal_radius_mm: f64::NAN,
                rootward_radius_mm: f64::NAN,
            },
            |row| crate::legalization::ArmRefusalPhysics {
                gap_mm: row.gap_mm,
                distance_clearance_mm: row.distance_clearance_mm,
                axis_transit_illegal: row.axis_transit_illegal,
                terminal_radius_mm: row.terminal_radius_mm,
                rootward_radius_mm: row.rootward_radius_mm,
            },
        ));
    }
    crate::legalization::LegalizationAbandon::Condemned {
        learned,
        own_arm_terminals,
        cascade_victims,
        measured,
    }
}

/// **Stage B — print every condemned member's OWNER, on both brace refusals.**
///
/// Stage A splits a disturbed terminal's condemned members into the arms it
/// OWNS and the carriers it does not, and the split decides everything: an own
/// arm mints a no-good and the loop retries, a carrier steers nothing and the
/// run stops. So the split has to be readable from a run log, because the one
/// fact that says whether a stop is honest physics or a provenance defect is
/// who owns the carrier.
///
/// **The specific hazard this discloses.** `cco`'s rewire mints members owning
/// `Owner::Contact(root.slot())` — a MEMBER slot in the terminal-id namespace.
/// A member that is really this terminal's own route, minted for it by rewire,
/// therefore wears an owner that is not `Contact(terminal)` and is classified
/// as somebody else's arm. That would turn a steerable refusal into an
/// unsteerable one, and the run would refuse looking exactly like a model whose
/// physics cannot be satisfied. One printed `owner()` separates the two.
fn disclose_class_provenance(
    forest: &Forest,
    sealed: &[crate::brace::SealedTerminalDisposition],
    class: &crate::brace::SealedCondemnationClass,
) {
    let owners = |keys: &[MemberKey]| {
        if keys.is_empty() {
            return "none".to_owned();
        }
        keys.iter()
            .map(|key| {
                forest.member(*key).map_or_else(
                    || format!("slot{}g{}=absent", key.slot(), key.generation()),
                    |member| {
                        format!(
                            "slot{}g{}={:?}",
                            key.slot(),
                            key.generation(),
                            member.owner()
                        )
                    },
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    let sealed_ids = sealed
        .iter()
        .map(|row| row.terminal_id)
        .collect::<BTreeSet<_>>();
    for terminal in &class.disturbed {
        // The terminal's whole rootward path, with each member's owner. This
        // answers both questions a carrier raises at once: is the rewire root
        // its owner names ON this terminal's route, and who owns that root.
        let mut path = Vec::new();
        let mut cursor = Some(terminal.terminal_member);
        while let Some(key) = cursor {
            let Some(member) = forest.member(key) else {
                path.push(format!("slot{}g{}=absent", key.slot(), key.generation()));
                break;
            };
            let owner = member.owner();
            let named_terminal = match owner {
                Owner::Contact(id) => {
                    if sealed_ids.contains(id) {
                        "sealed-terminal"
                    } else {
                        // `cco`'s rewire owns by MEMBER slot, so a
                        // `Contact(x)` whose x the sealed record does not name
                        // is a rewire root and not a terminal at all.
                        "not-a-sealed-terminal"
                    }
                }
                Owner::ModelAnchor(_) | Owner::Generated { .. } => "n/a",
            };
            path.push(format!(
                "slot{}g{}={owner:?}({named_terminal})",
                key.slot(),
                key.generation(),
            ));
            cursor = forest.parent_of(key);
        }
        // **The physical discriminator, and it beats owner archaeology.**
        // "Somebody else's arm" means an arm that also carries somebody else.
        // For each condemned carrier, walk its subtree and name every terminal
        // whose stub hangs off it. One stub — this terminal's — and the carrier
        // exists to carry this terminal and nothing else. More than one, and
        // the refusal is §1.3's cascade victim exactly as written.
        let stubs = forest.terminals_by_member();
        for carrier in &terminal.cascade_carriers {
            let mut carried = Vec::new();
            let mut frontier = vec![*carrier];
            while let Some(key) = frontier.pop() {
                if let Some(id) = stubs.get(&key) {
                    carried.push(*id);
                }
                frontier.extend(forest.children_of(key));
            }
            carried.sort_unstable();
            crate::coverage_repair::emit_run_line(&format!(
                "legalize: carrier slot{}g{} of t{} carries terminals {carried:?}",
                carrier.slot(),
                carrier.generation(),
                terminal.terminal_id,
            ));
        }
        crate::coverage_repair::emit_run_line(&format!(
            "legalize: provenance t{} sealed={:?} own_arms=[{}] carriers=[{}] rootward=[{}]",
            terminal.terminal_id,
            terminal.sealed_kind,
            owners(&terminal.own_chosen_arms),
            owners(&terminal.cascade_carriers),
            path.join(" -> "),
        ));
    }
}

/// SPEC-A4 §A.3 — the bracing fixed point, factored for context recovery. This
/// is the stage task #47 turns on: the Giant Bat's
/// `Brace(ClearanceViolation(1))` is a whole model today and becomes one
/// ledgered rollback once the controller can take the context back.
fn brace_state(
    context: &SupportRunContext<'_>,
    state: RewiredPlanState,
) -> Result<BracedPlanState, SupportBraceError> {
    {
        let geometry = context.geometry()?;
        let loads = state.routed.plan.finalized_loads()?;
        context.validate_brace_preflight(&state, &geometry, loads)?;

        // **Stage A.** The sealed record, handed to the brace stage so a sweep
        // that would flip a sealed terminal's live state abandons the pass
        // instead of writing on top of a record it cannot change. Every sealed
        // disposition travels, not the `Routed` ones only (design §4 Stage
        // A(a)); the transcript is already held here, so this is a projection
        // rather than a new authority.
        let sealed = state
            .routed
            .transcript
            .rows()
            .iter()
            .map(|row| SealedTerminalDisposition {
                terminal_id: row.terminal_id(),
                kind: row.kind(),
            })
            .collect::<Vec<_>>();
        let plan = match brace_fixed_point_current_parity(
            state.routed.initial.forest(),
            geometry.surface(),
            // #36 step 3. The face adjacency the seat-zone freeze walks, taken
            // from the one authority that already validated it against this
            // exact surface and mesh — the runner constructs no second
            // topology (`SupportRunContext::topology`).
            geometry.topology(),
            context.frame.plate_z_mm(),
            context.brace_penetration_limits(),
            // PR-1/PR-2 need the run's OWN process resolution: the seat floor
            // and the clearance margin are both one layer, and a layer is a
            // property of this run's plan rather than of the registry.
            crate::contact::ContactPolicy::process_resolution_of(context.layers()),
            &sealed,
        ) {
            Ok(plan) => plan,
            // Stage A's two outcomes are lifted out of the generic
            // `BraceError` wrapper and named at the stage boundary, so an
            // operator reads WHICH refusal happened without unwrapping a
            // transparent error, and so the retry loop Stage B adds can match
            // one arm rather than parse a string.
            Err(BraceError::SealedRoutedTerminalCondemned(class)) => {
                // **Stage B — the mint.** The class says WHICH terminals had
                // their own chosen arm condemned; the routing cadence's sealed
                // record says WHAT each of them chose and in which world. The
                // two are joined here, at the only point where both exist.
                disclose_class_provenance(state.routed.initial.forest(), &sealed, &class);
                let abandon = mint_no_goods(&state.routed.controller, &class);
                return Err(SupportBraceError::SealedRoutedTerminalCondemned(
                    class,
                    Box::new(abandon),
                ));
            }
            Err(BraceError::SealedSweepLearnedNothing(class)) => {
                // The same disclosure on the refusing branch. This is the arm
                // where knowing the owners matters MOST: the pass is about to
                // stop for good, and whether that is honest physics or a
                // provenance defect is exactly what an `owner()` says.
                disclose_class_provenance(state.routed.initial.forest(), &sealed, &class);
                return Err(SupportBraceError::SealedSweepLearnedNothing(class));
            }
            Err(error) => return Err(error.into()),
        };
        plan.forest.validate().map_err(BraceError::from)?;
        let report = transient_report(&plan.forest, context.frame.plate_z_mm())?;
        let post_brace_forest_digest = plan.forest.digest();
        let post_brace_report_digest = transient_report_digest(&report);
        let brace_plan_digest = brace_fixed_point_plan_digest(&plan);
        // PR-8/OD-9 (F3). The third and last family: the brace model rung. With
        // this link the chain covers all three families that mint a model
        // contact -- stub, CCO model anchor, brace model rung -- which is what
        // "the contact attestation is still running" has to mean to be
        // checkable at all.
        let contact_attestation_chain = extend_contact_attestation_chain(
            state.contact_attestation_chain,
            CONTACT_ATTESTATION_STAGE_BRACE_RUNG,
            plan.contact_attestation_digest,
        );

        Ok(BracedPlanState {
            rewired: state,
            plan,
            report,
            brace_plan_digest,
            post_brace_forest_digest,
            post_brace_report_digest,
            contact_attestation_chain,
        })
    }
}

/// Route and rewire evidence plus the complete frozen brace fixed-point plan.
/// This opaque hand-off deliberately preserves both earlier stages while
/// making only the brace plan's final forest authoritative for later stages.
struct BracedPlanState {
    rewired: RewiredPlanState,
    plan: BraceFixedPointPlan,
    report: TransientReport,
    brace_plan_digest: u64,
    post_brace_forest_digest: u64,
    post_brace_report_digest: u64,
    /// PR-8/OD-9 (F3) — the run's complete contact-attestation chain: the stub
    /// family's sealed head, then the CCO model anchor at routing, then at
    /// rewire, then the brace model rung. All three families that mint a model
    /// contact are links in it.
    contact_attestation_chain: u64,
}

impl BracedPlanState {
    /// Rebind and validate all retained provenance without another native mutation.
    fn validate_integrity(&self, context: &SupportRunContext<'_>) -> Result<(), SupportBraceError> {
        let geometry = context.geometry()?;
        let loads = self.rewired.routed.plan.finalized_loads()?;
        context.validate_brace_preflight(&self.rewired, &geometry, loads)?;
        self.plan.forest.validate().map_err(BraceError::from)?;
        if self.brace_plan_digest != brace_fixed_point_plan_digest(&self.plan) {
            return Err(SupportBraceError::BracePlanDigestMismatch);
        }
        // PR-8/OD-9 (F3). The chain is recomputed from its own links rather
        // than trusted, so a state that was assembled with a stale or missing
        // family transcript fails closed here rather than shipping.
        if self.contact_attestation_chain
            != extend_contact_attestation_chain(
                self.rewired.contact_attestation_chain,
                CONTACT_ATTESTATION_STAGE_BRACE_RUNG,
                self.plan.contact_attestation_digest,
            )
        {
            return Err(SupportBraceError::ContactAttestationChainMismatch);
        }
        if self.post_brace_forest_digest != self.plan.forest.digest() {
            return Err(SupportBraceError::FinalForestDigestMismatch);
        }
        let fresh = transient_report(&self.plan.forest, context.frame.plate_z_mm())?;
        if fresh != self.report
            || self.post_brace_report_digest != transient_report_digest(&self.report)
        {
            return Err(SupportBraceError::TransientReportDigestMismatch);
        }
        Ok(())
    }
}

pub(crate) struct BracedSupportRun<'a> {
    context: SupportRunContext<'a>,
    state: BracedPlanState,
}

impl BracedSupportRun<'_> {
    #[must_use]
    pub(crate) const fn mesh(&self) -> &Mesh {
        self.context.mesh()
    }

    #[must_use]
    pub(crate) const fn frame(&self) -> BuildFrame {
        self.context.frame()
    }

    #[must_use]
    pub(crate) const fn brace_penetration_limits(&self) -> PenetrationBatchLimits {
        self.context.brace_penetration_limits()
    }

    #[must_use]
    pub(crate) const fn plan(&self) -> &SpecializedContactPlan {
        &self.state.rewired.routed.plan
    }

    #[must_use]
    pub(crate) const fn initial(&self) -> &InitialContactBuild {
        &self.state.rewired.routed.initial
    }

    #[must_use]
    pub(crate) const fn digests(&self) -> &PreparedSupportDigests {
        self.context.digests()
    }

    #[must_use]
    pub(crate) const fn controller(&self) -> &CcoController {
        &self.state.rewired.routed.controller
    }

    #[must_use]
    pub(crate) const fn transcript(&self) -> &FinalizedCcoTranscript {
        &self.state.rewired.routed.transcript
    }
    #[must_use]
    pub(crate) const fn forest(&self) -> &Forest {
        &self.state.plan.forest
    }

    #[must_use]
    pub(crate) const fn brace_plan(&self) -> &BraceFixedPointPlan {
        &self.state.plan
    }

    #[must_use]
    pub(crate) const fn transient_report(&self) -> &TransientReport {
        &self.state.report
    }

    #[must_use]
    pub(crate) const fn brace_plan_digest(&self) -> u64 {
        self.state.brace_plan_digest
    }

    #[must_use]
    pub(crate) const fn post_brace_forest_digest(&self) -> u64 {
        self.state.post_brace_forest_digest
    }

    #[must_use]
    pub(crate) const fn post_brace_report_digest(&self) -> u64 {
        self.state.post_brace_report_digest
    }

    #[must_use]
    pub(crate) const fn rewire_report(&self) -> &RewireReport {
        &self.state.rewired.report
    }

    #[must_use]
    pub(crate) const fn rewire_report_digest(&self) -> u64 {
        self.state.rewired.rewire_report_digest
    }

    #[must_use]
    pub(crate) const fn post_rewire_forest_digest(&self) -> u64 {
        self.state.rewired.post_rewire_forest_digest
    }

    pub(crate) fn validate_integrity(&self) -> Result<(), SupportBraceError> {
        self.state.validate_integrity(&self.context)
    }

    #[cfg(test)]
    fn test_corrupt_brace_plan_additions(&mut self) {
        let key = self
            .state
            .plan
            .forest
            .live_members()
            .into_iter()
            .next()
            .expect("required corpus has a live member");
        let member = self
            .state
            .plan
            .forest
            .member(key)
            .expect("live key resolves")
            .clone();
        self.state.plan.additions.push(crate::brace::BraceAddition {
            key,
            kind: BraceAdditionKind::CrossLink,
            member: MemberSpec {
                p0_mm: member.p0_mm(),
                p1_mm: member.p1_mm(),
                r0_mm: member.terminal_radius_mm(),
                r1_mm: member.rootward_radius_mm(),
                kind: member.kind().clone(),
                owner: member.owner().clone(),
                model_contact_end: member.model_contact_end(),
                carried_mm2: member.carried_mm2(),
            },
        });
    }

    #[cfg(test)]
    fn test_corrupt_brace_plan_iterations(&mut self) {
        self.state.plan.iterations = self.state.plan.iterations.wrapping_add(1);
    }

    #[cfg(test)]
    fn test_corrupt_brace_plan_termination(&mut self, termination: BraceFixedPointTermination) {
        self.state.plan.termination = termination;
    }
}

/// Complete post-brace semantic planning authority.  It owns the one final
/// forest plus the one filtered live view consumed by both independent
/// planning leaves; it deliberately performs no lowering or emission.
pub(crate) struct StructuredSupportRun<'a> {
    context: SupportRunContext<'a>,
    braced: BracedPlanState,
    live_view: FilteredLiveView,
    joints: JointPlan,
    raft: RaftPlan,
}

/// Opaque final lowering hand-off. It retains the exact structured authority until the sole
/// supported-scene constructor has accepted the primitive stream and its report.
pub(crate) struct LoweredSupportRun<'a> {
    structured: StructuredSupportRun<'a>,
    lowered: LoweredSupportPlan,
}

impl<'a> BracedSupportRun<'a> {
    /// Consume the braced forest exactly once into the joint and raft semantic
    /// planners.  All predecessor bindings are preflighted before gathering
    /// the view, and both outputs are rebound to that exact view afterward.
    #[allow(
        clippy::result_large_err,
        reason = "the Ok variant carries the same SupportRunContext by construction: \
                  it is MOVED through the typestate either way, so boxing the Err \
                  would allocate on every refusal and save nothing"
    )]
    pub(crate) fn structure(
        self,
    ) -> Result<StructuredSupportRun<'a>, (SupportRunContext<'a>, SupportStructureError)> {
        let BracedSupportRun {
            context,
            state: braced,
        } = self;
        match structure_parts(&context, &braced) {
            Ok((live_view, joints, raft)) => Ok(StructuredSupportRun {
                context,
                braced,
                live_view,
                joints,
                raft,
            }),
            Err(error) => Err((context, error)),
        }
    }
}

/// SPEC-A4 §A.3 — the joint and raft planners, factored for context recovery.
/// `braced` is borrowed rather than moved because the structured typestate
/// keeps it verbatim; nothing here mutates it.
fn structure_parts(
    context: &SupportRunContext<'_>,
    braced: &BracedPlanState,
) -> Result<(FilteredLiveView, JointPlan, RaftPlan), SupportStructureError> {
    {
        braced.validate_integrity(context)?;
        braced.plan.forest.validate().map_err(BraceError::from)?;
        let unrouted = finalized_unrouted_terminal_ids(
            &braced.rewired.routed.transcript,
            &braced.plan.forest,
        )?;
        let constants = m0_constants()
            .map_err(|error| SupportStructureError::RegisteredConstants(error.to_string()))?;
        let joint = JointConfig::current(
            context.frame.plate_z_mm(),
            *constants.placeholders().joint_ball_factor().diagnostic(),
        );
        let (view_limits, staging) = runner_planning_limits(joint);
        let live_view = FilteredLiveView::current(&braced.plan.forest, &unrouted, view_limits)?;
        live_view.validate(&braced.plan.forest)?;
        let joints = plan_joint_balls_filtered(&braced.plan.forest, &live_view, joint, staging)?;
        let raft = plan_bottom_connected(
            &braced.plan.forest,
            &live_view,
            context.frame.plate_z_mm(),
            context.raft_limits(),
        )?;
        joints.validate_filtered(&braced.plan.forest, &live_view, joint)?;
        raft.validate(&braced.plan.forest, &live_view, context.frame.plate_z_mm())?;
        Ok((live_view, joints, raft))
    }
}

impl<'a> StructuredSupportRun<'a> {
    #[must_use]
    pub(crate) const fn forest(&self) -> &Forest {
        &self.braced.plan.forest
    }

    #[must_use]
    pub(crate) const fn live_view(&self) -> &FilteredLiveView {
        &self.live_view
    }

    #[must_use]
    pub(crate) const fn joints(&self) -> &JointPlan {
        &self.joints
    }

    #[must_use]
    pub(crate) const fn raft(&self) -> &RaftPlan {
        &self.raft
    }

    #[must_use]
    pub(crate) const fn frame(&self) -> BuildFrame {
        self.context.frame()
    }

    #[must_use]
    pub(crate) const fn brace_termination(&self) -> BraceFixedPointTermination {
        self.braced.plan.termination
    }

    pub(crate) fn validate_integrity(&self) -> Result<(), SupportStructureError> {
        self.braced.validate_integrity(&self.context)?;
        self.braced
            .plan
            .forest
            .validate()
            .map_err(BraceError::from)?;
        self.live_view.validate(&self.braced.plan.forest)?;
        let joint = JointConfig::current(
            self.context.frame.plate_z_mm(),
            *m0_constants()
                .map_err(|error| SupportStructureError::RegisteredConstants(error.to_string()))?
                .placeholders()
                .joint_ball_factor()
                .diagnostic(),
        );
        self.joints
            .validate_filtered(&self.braced.plan.forest, &self.live_view, joint)?;
        self.raft.validate(
            &self.braced.plan.forest,
            &self.live_view,
            self.context.frame.plate_z_mm(),
        )?;
        Ok(())
    }

    /// Lower the one final structured authority. No semantic planning stage is repeated.
    #[allow(
        clippy::result_large_err,
        reason = "the Ok variant carries the same SupportRunContext by construction: \
                  it is MOVED through the typestate either way, so boxing the Err \
                  would allocate on every refusal and save nothing"
    )]
    pub(crate) fn lower(
        self,
    ) -> Result<LoweredSupportRun<'a>, (SupportRunContext<'a>, SupportLoweringError)> {
        // SPEC-A4 §A.3. Lowering needs no split: it mutates nothing, so the
        // fallible half runs on `&self` and the context is moved out of `self`
        // only on the error path, after every borrow has ended.
        match self.lowered_plan() {
            Ok(lowered) => Ok(LoweredSupportRun {
                structured: self,
                lowered,
            }),
            Err(error) => Err((self.context, error)),
        }
    }

    fn lowered_plan(&self) -> Result<LoweredSupportPlan, SupportLoweringError> {
        self.validate_integrity()?;
        let (max_primitives, max_primitive_bytes) = self.context.lowering_limits();
        let config = LoweringConfig::current(
            self.context.frame(),
            self.context.layers(),
            max_primitives,
            max_primitive_bytes,
        )?;
        let lowered = lower_current(
            &self.braced.plan.forest,
            &self.live_view,
            &self.joints,
            &self.raft,
            config,
        )?;
        validate_lowered_output(self, &lowered, config)?;
        Ok(lowered)
    }
}

/// What the routing cadence did with the one contact a repair trial appended.
///
/// Evidence, never a decision: nothing in the accept test reads it. Design
/// §B-O7.6 records why an early rejection on `Quarantined` is deliberately NOT
/// implemented — over the measured plateau `unrouted_no_winner` moved 7 → 11
/// while those contacts were being appended, so a quarantined contact
/// demonstrably changes OTHER terminals' outcomes, and refusing before the plan
/// is routed would be a guess about a plan nobody routed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TrialContactDisposition {
    Routed,
    /// Condemned before any arm was proposed. `remaining_inside_axis_mm` is the
    /// measured stub-axis re-entry depth when the initial build's condemned
    /// ledger holds one; `None` means the quarantine had a different cause and
    /// this refuses to name one it did not measure.
    Quarantined {
        remaining_inside_axis_mm: Option<f64>,
    },
    /// Routed and retired with no winning arm. The payload is the transcript
    /// row's OWN sealed `NoWinnerEvidence`, projected — it carries the
    /// `every_feasible_arm_refused_by_clearance` flag this variant has always
    /// held, plus the sibling counts the same row already holds beside it.
    UnroutedNoWinner(crate::coverage_repair::OwnContactNoWinnerEvidence),
    /// The contact never appeared in the cadence at all.
    NotInCadence,
}

/// **Project one transcript row's sealed no-winner reason onto the wire — a
/// READ, in the strict sense.**
///
/// Every value below is copied out of the `NoWinnerEvidence` the routing
/// cadence already built and sealed onto the row; the only arithmetic is
/// `Vec::len` on lists this function borrows and does not walk. No candidate is
/// re-counted, no clearance test is re-run, and no count is invented for a
/// variant that does not carry one — a field the sealed reason has no place for
/// comes out `None`, never `0`. That distinction is why the disclosure is
/// trustworthy at all: a `0` a reader could not tell from "unrecorded" would be
/// a second measurement wearing the first one's clothes.
///
/// `None` in, `None` stage out: a finalized `UnroutedNoWinner` row is always
/// sealed with a reason, but this refuses to name one it was not given, and it
/// reports `every_feasible_arm_refused_by_clearance: false` there — byte for
/// byte what the `matches!` this replaced returned on the same input.
fn disclose_no_winner_evidence(
    reason: Option<&crate::cco::NoWinnerEvidence>,
) -> crate::coverage_repair::OwnContactNoWinnerEvidence {
    use crate::coverage_repair::{OwnContactNoWinnerEvidence, OwnContactNoWinnerStage};

    match reason {
        None => OwnContactNoWinnerEvidence {
            every_feasible_arm_refused_by_clearance: false,
            stage: None,
            candidates_generated: None,
            candidates_feasible: None,
            anchor_seat_refusals_recorded: 0,
            anchor_foot_refusals_recorded: None,
        },
        Some(crate::cco::NoWinnerEvidence::NoCandidatesGenerated {
            anchor_seat_refusals,
        }) => OwnContactNoWinnerEvidence {
            every_feasible_arm_refused_by_clearance: false,
            stage: Some(OwnContactNoWinnerStage::NoCandidatesGenerated),
            candidates_generated: None,
            candidates_feasible: None,
            anchor_seat_refusals_recorded: anchor_seat_refusals.len(),
            anchor_foot_refusals_recorded: None,
        },
        Some(crate::cco::NoWinnerEvidence::NoFeasibleCandidate {
            candidates_generated,
            anchor_seat_refusals,
            anchor_refusals,
        }) => OwnContactNoWinnerEvidence {
            every_feasible_arm_refused_by_clearance: false,
            stage: Some(OwnContactNoWinnerStage::NoFeasibleCandidate),
            candidates_generated: Some(*candidates_generated),
            candidates_feasible: None,
            anchor_seat_refusals_recorded: anchor_seat_refusals.len(),
            anchor_foot_refusals_recorded: Some(anchor_refusals.len()),
        },
        Some(crate::cco::NoWinnerEvidence::AllFeasibleCandidatesRefusedByClearance {
            candidates_generated,
            candidates_feasible,
            anchor_seat_refusals,
            anchor_refusals,
        }) => OwnContactNoWinnerEvidence {
            every_feasible_arm_refused_by_clearance: true,
            stage: Some(OwnContactNoWinnerStage::AllFeasibleCandidatesRefusedByClearance),
            candidates_generated: Some(*candidates_generated),
            candidates_feasible: Some(*candidates_feasible),
            anchor_seat_refusals_recorded: anchor_seat_refusals.len(),
            anchor_foot_refusals_recorded: Some(anchor_refusals.len()),
        },
        Some(crate::cco::NoWinnerEvidence::ConfirmationRoundExhausted {
            candidates_generated,
            candidates_feasible,
            anchor_seat_refusals,
            anchor_refusals,
            ..
        }) => OwnContactNoWinnerEvidence {
            // **FALSE, and the distinction is load-bearing.** This flag means
            // task #51's shape — every feasible arm refused by the CANDIDACY
            // clearance standoff. A round's arms passed that standoff; what
            // refused them is the post-resize brace pass, two stages later, on
            // radii candidacy could not predict. Setting it here would let a
            // Stage B retirement be counted as evidence for the mask defect.
            every_feasible_arm_refused_by_clearance: false,
            stage: Some(OwnContactNoWinnerStage::ConfirmationRoundExhausted),
            candidates_generated: Some(*candidates_generated),
            candidates_feasible: Some(*candidates_feasible),
            anchor_seat_refusals_recorded: anchor_seat_refusals.len(),
            anchor_foot_refusals_recorded: Some(anchor_refusals.len()),
        },
    }
}

impl<'a> LoweredSupportRun<'a> {
    /// Recycle the retained mesh/topology/`NativeSurface`/geometry context out
    /// of a fully lowered (but coverage-blocked) run, for the coverage-closure
    /// controller's next attempt.
    ///
    /// This is the memory-critical seam (FU-56): the controller re-runs
    /// `prepared_run_for_plan` against a repair-extended plan up to
    /// `COVERAGE_REPAIR_MAX_ATTEMPTS` times, and the context this returns is
    /// MOVED, never cloned, between attempts — cloning it would put one
    /// `NativeSurface` grid per attempt on the machine at once. It discards
    /// the routed/rewired/braced/structured/lowered planning state, which the
    /// next attempt rebuilds from the (now repair-extended) plan.
    pub(crate) fn into_context(self) -> SupportRunContext<'a> {
        self.structured.context
    }

    /// **SPEC-A4 §6 clause 2's vector, measured off this run.**
    ///
    /// Every component is a count the run already keeps, so this is a read,
    /// not a computation: one pass over the CCO transcript for the two
    /// disposition counts (O(cadence) integer compares — about 1,200 on the
    /// Giant Toad), a length read for the unrouted specialized list task #49
    /// built, and two field reads off the plan's frozen gate evidence. Against
    /// a trial that costs `cadence × pool_rows` seat evaluations — 6.0 × 10⁸ on
    /// that same toad — the whole check is ~10³ operations. It needs no new
    /// geometry call, no new allocation and no new registered limit.
    ///
    /// The plan half is read off the **accepted** (reduced) plan, never the
    /// routed one, because SPEC §6 strips the unrouted regular contacts before
    /// the carried-load repartition and comparing the pre-strip partition would
    /// compare a state the spec says must not exist.
    pub(crate) fn noncoverage_vector(&self) -> crate::coverage_repair::NoncoverageVector {
        let routed = &self.structured.braced.rewired.routed;
        let mut unrouted_no_winner = 0_u32;
        let mut quarantined = 0_u32;
        for row in routed.transcript.rows() {
            match row.kind() {
                crate::cco::FinalizedDispositionKind::Routed => {}
                crate::cco::FinalizedDispositionKind::SkippedQuarantined => {
                    quarantined = quarantined.saturating_add(1);
                }
                crate::cco::FinalizedDispositionKind::UnroutedNoWinner => {
                    unrouted_no_winner = unrouted_no_winner.saturating_add(1);
                }
            }
        }
        let unrouted_specialized =
            u32::try_from(routed.accepted.specialized_unrouted.len()).unwrap_or(u32::MAX);
        crate::coverage_repair::NoncoverageVector::from_accepted_plan(
            routed.accepted.plan(&routed.plan),
        )
        .with_transcript_counts(unrouted_no_winner, quarantined, unrouted_specialized)
    }

    /// Task #49's ownerless faces, as EVIDENCE on a repair step.
    ///
    /// Deliberately not a clause-2 component. #49's own ruling is that load
    /// eligibility is provenance, not reach, so an ownerless face is not a
    /// measurement that anything is unsupported — the measurement that decides
    /// that is the routed-coverage judgment this controller runs. Making a
    /// set-membership tally a hard component would let a bare count veto a step
    /// the physical measurement approves, which is exactly what the
    /// physics-first doctrine forbids. So it travels in the record and decides
    /// nothing.
    pub(crate) fn ownerless_face_count(&self) -> usize {
        self.structured
            .braced
            .rewired
            .routed
            .accepted
            .ownerless_faces
            .len()
    }

    /// **Stage D — every terminal this run's SEALED transcript retired with no
    /// winner.**
    ///
    /// One pass over the finalized rows, comparing the disposition kind the
    /// cadence already sealed; the same read [`Self::noncoverage_vector`] makes
    /// to count them, keeping the ids instead of a total. It is a batch read of
    /// a list the run already holds — no geometry call, no per-terminal
    /// lookup, and nothing derived.
    ///
    /// **It names a place, not a cause.** `UnroutedNoWinner` has four sealed
    /// causes and only one of them is a confirmation round; which one applies
    /// to a given terminal is inside its own `NoWinnerEvidence`, where design
    /// §1.3 put it. Stage D's round row uses this to say where the round's
    /// terminal stood at lowering and claims nothing further.
    fn unrouted_no_winner_terminals(&self) -> BTreeSet<u32> {
        self.structured
            .braced
            .rewired
            .routed
            .transcript
            .rows()
            .iter()
            .filter(|row| row.kind() == crate::cco::FinalizedDispositionKind::UnroutedNoWinner)
            .map(crate::cco::FinalizedCcoTranscriptRow::terminal_id)
            .collect()
    }

    /// **What the CCO did with the ONE contact a trial appended.**
    ///
    /// This is the per-trial evidence design §B-O7.6 requires and §E.1 measures
    /// its blocked causes from. Every branch is a read of a record the run
    /// already keeps — the finalized transcript's disposition for that exact
    /// `physical_contact_id`, and, for a quarantined one, the initial build's
    /// own condemned-stub row with its recorded re-entry depth. Nothing here is
    /// inferred from a distance, which is ruling R14's whole rule.
    ///
    /// The design's §B-O7 measurement is the reason it exists: on Crawler
    /// Zombie's five-round plateau **every one of 50 appended contacts was
    /// condemned at quarantine**, so the routed contact count did not move and
    /// no measured quantity could. Without this read a trial can only report
    /// "nothing improved"; with it the record says why.
    pub(crate) fn trial_contact_disposition(
        &self,
        physical_contact_id: u64,
    ) -> TrialContactDisposition {
        let routed = &self.structured.braced.rewired.routed;
        let Some(row) = routed
            .transcript
            .rows()
            .iter()
            .find(|row| row.physical_contact_id() == physical_contact_id)
        else {
            return TrialContactDisposition::NotInCadence;
        };
        match row.kind() {
            crate::cco::FinalizedDispositionKind::Routed => TrialContactDisposition::Routed,
            crate::cco::FinalizedDispositionKind::SkippedQuarantined => {
                // The finalized transcript deliberately keeps no quarantine
                // REASON — only two frozen digests. The reason lives where it
                // was measured: the initial build's condemned-stub ledger,
                // keyed by terminal, carrying the exact remaining-inside-axis
                // overlap that condemned it. A quarantine with no condemned row
                // was quarantined for something other than stub re-entry, and
                // this says so with `None` rather than guessing.
                let terminal = row.terminal_id();
                let remaining_inside_axis_mm = routed
                    .initial
                    .condemned()
                    .iter()
                    .find(|stub| stub.terminal == terminal)
                    .map(|stub| stub.remaining_inside_axis_mm);
                TrialContactDisposition::Quarantined {
                    remaining_inside_axis_mm,
                }
            }
            crate::cco::FinalizedDispositionKind::UnroutedNoWinner => {
                TrialContactDisposition::UnroutedNoWinner(disclose_no_winner_evidence(
                    row.no_winner_reason(),
                ))
            }
        }
    }

    /// SPEC-A4 §A.2 — recover BOTH the accepted plan and the context from a
    /// successful run, so a COMMITTED transactional step needs no third plan
    /// copy alive beside the two the run already owns.
    ///
    /// **The plan handed back is the ACCEPTED one, never the routed one.**
    /// Task #49 put an `AcceptedContactSet` in front of the routed plan: when
    /// any unrouted regular contact was stripped, the accepted set is a reduced
    /// child, and carrying the pre-strip plan forward would re-admit contacts
    /// SPEC §6 removed before rewire, bracing and repartition. Cloning is what
    /// the reduced branch costs, and it costs it only when something was
    /// actually removed — `AcceptedContactSet::reduced` is `None` otherwise and
    /// the routed plan is moved out whole.
    pub(crate) fn into_plan_and_context(self) -> (SpecializedContactPlan, SupportRunContext<'a>) {
        let routed = self.structured.braced.rewired.routed;
        let plan = routed.accepted.reduced.unwrap_or(routed.plan);
        (plan, self.structured.context)
    }

    #[must_use]
    pub(crate) const fn digest(&self) -> plinth_core::ContentDigest {
        self.lowered.digest()
    }

    /// Borrow the final, validated run as narrow coverage-verification evidence.
    /// No coverage judgment is performed here.
    pub(crate) fn verification_snapshot(
        &self,
    ) -> Result<SupportVerificationSnapshot<'_>, SupportLoweringError> {
        self.structured.validate_integrity()?;
        let (max_primitives, max_primitive_bytes) = self.structured.context.lowering_limits();
        let config = LoweringConfig::current(
            self.structured.context.frame(),
            self.structured.context.layers(),
            max_primitives,
            max_primitive_bytes,
        )?;
        validate_lowered_output(&self.structured, &self.lowered, config)?;

        let context = &self.structured.context;
        let routed = &self.structured.braced.rewired.routed;
        // SPEC-A4 §6: coverage judgment is the last of the four consumers the
        // clause names, and the snapshot is its input, so the snapshot
        // projects the ACCEPTED contact set. `routed_plan` stays beside it
        // because the transcript, the initial construction and the forest are
        // all sealed to the plan that was ROUTED; comparing those seals
        // against the accepted set would compare a seal to something it was
        // never taken over.
        validate_accepted_contact_set(routed, &self.structured.braced.plan.forest)
            .map_err(|_| SupportVerificationError::AuthorityMismatch)?;
        let routed_plan = &routed.plan;
        let plan = routed.accepted.plan(routed_plan);
        let transcript = &routed.transcript;
        let routed_loads = routed_plan
            .finalized_loads()
            .map_err(|_| SupportVerificationError::AuthorityMismatch)?;
        let loads = plan
            .finalized_loads()
            .map_err(|_| SupportVerificationError::AuthorityMismatch)?;
        let geometry = context
            .geometry()
            .map_err(|_| SupportVerificationError::TopologyMeshMismatch)?;
        transcript
            .validate_integrity()
            .map_err(|_| SupportVerificationError::AuthorityMismatch)?;
        if routed_plan.digest() != context.digests.plan()
            || routed_loads.digest() != context.digests.loads()
            || geometry.digest() != context.digests.geometry()
            || transcript.plan_digest() != routed_plan.digest()
            || transcript.finalized_load_digest() != routed_loads.digest()
            || transcript.geometry_digest() != geometry.digest()
        {
            return Err(SupportLoweringError::Verification(
                SupportVerificationError::AuthorityMismatch,
            ));
        }

        // Gather the immutable plan rows once, then scatter the transcript's
        // stable cadence order into the sealed projection. This intentionally
        // never retries or reclassifies a terminal.
        let contacts_by_id = plan
            .contacts()
            .iter()
            .map(|contact| (contact.contact_id(), contact))
            .collect::<BTreeMap<_, _>>();
        if contacts_by_id.len() != plan.contacts().len() {
            return Err(SupportVerificationError::AuthorityMismatch.into());
        }
        let mut routed_contacts = Vec::new();
        let mut routed_route_memberships = Vec::new();
        let mut routed_ids = BTreeSet::new();
        // One proof and one index for the whole projection, not one per routed
        // contact: the sidecar is the same slice for every row below.
        let island_index = IslandFaceIndex::build_named(plan.islands(), &context.topology)
            .map_err(|_| SupportVerificationError::AuthorityMismatch)?;
        for row in transcript.rows() {
            if row.kind() != FinalizedDispositionKind::Routed {
                continue;
            }
            let physical_contact_id = row.physical_contact_id();
            let terminal_id = row.terminal_id();
            let contact = contacts_by_id
                .get(&physical_contact_id)
                .copied()
                .ok_or(SupportVerificationError::AuthorityMismatch)?;
            let ledger = self
                .structured
                .braced
                .plan
                .forest
                .terminal_ledger(terminal_id)
                .ok_or(SupportVerificationError::AuthorityMismatch)?;
            if physical_contact_id != u64::from(terminal_id)
                || !routed_ids.insert(physical_contact_id)
                || ledger.state != TerminalLedgerState::Consumed
            {
                return Err(SupportVerificationError::AuthorityMismatch.into());
            }
            let logical_routes = routed_logical_routes(contact, &island_index, &context.topology)
                .map_err(|_| SupportVerificationError::AuthorityMismatch)?;
            let evidence = RoutedPhysicalContactEvidence { contact };
            routed_contacts.push(evidence);
            routed_route_memberships.push(RoutedRouteMembership {
                contact: evidence,
                logical_routes,
            });
        }
        let routed_projection_digest = routed_projection_digest(&routed_route_memberships);
        // This is the sole member gather for verification.  The filtered view
        // was already used to validate joints, raft, and lowering; retired,
        // quarantined, and otherwise excluded slots cannot enter this slice.
        let final_live_members = self
            .structured
            .live_view
            .keys()
            .iter()
            .map(|key| {
                self.structured
                    .braced
                    .plan
                    .forest
                    .member(*key)
                    .map(|member| FinalLiveMemberEvidence::from_member(*key, member))
                    .ok_or(SupportVerificationError::AuthorityMismatch)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if final_live_members
            .iter()
            .map(FinalLiveMemberEvidence::key)
            .ne(self.lowered.member_keys().iter().copied())
        {
            return Err(SupportVerificationError::AuthorityMismatch.into());
        }
        let final_live_members_digest = final_live_members_digest(&final_live_members);
        let snapshot = SupportVerificationSnapshot {
            mesh: context.mesh(),
            topology: &context.topology,
            surface: &context.surface,
            coverage_samples: plan.coverage_samples(),
            named_islands: plan.islands(),
            routed_contacts,
            routed_route_memberships,
            final_live_members,
            mesh_digest: context.digests.mesh(),
            topology_digest: context.topology.content_digest(),
            plan_digest: plan.digest(),
            load_digest: loads.digest(),
            geometry_digest: geometry.digest(),
            forest_digest: self.structured.braced.plan.forest.digest(),
            transcript_digest: transcript.digest(),
            regular_digest: context.digests.regular(),
            coverage_digest: coverage_samples_digest(plan.coverage_samples()),
            named_islands_digest: named_islands_digest(plan.islands()),
            lowered_primitives_digest: self.lowered.digest().0,
            routed_projection_digest,
            final_live_members_digest,
            snapshot_digest: 0,
        };
        let snapshot_digest = verification_snapshot_digest(&snapshot);
        Ok(SupportVerificationSnapshot {
            snapshot_digest,
            ..snapshot
        })
    }

    /// Consume the final opaque lowering result into the public supported scene.
    pub(crate) fn into_supported_model(self) -> Result<SupportedModel, SupportLoweringError> {
        let LoweredSupportRun {
            structured,
            lowered,
        } = self;
        structured.validate_integrity()?;
        let (max_primitives, max_primitive_bytes) = structured.context.lowering_limits();
        let config = LoweringConfig::current(
            structured.context.frame(),
            structured.context.layers(),
            max_primitives,
            max_primitive_bytes,
        )?;
        validate_lowered_output(&structured, &lowered, config)?;
        let primitives = lowered
            .validate(
                &structured.braced.plan.forest,
                &structured.live_view,
                structured.context.frame(),
            )?
            .to_vec();
        let support = SupportReport {
            support_primitive_count: lowered.support_primitive_count(),
            raft_primitive_count: lowered.raft_primitive_count(),
        };
        let StructuredSupportRun { context, .. } = structured;
        let SupportRunContext {
            mesh,
            hollow_history,
            frame,
            ..
        } = context;
        SupportedModel::from_parts(
            mesh,
            primitives,
            SupportHistory::from_reports(hollow_history, support, frame),
        )
        .map_err(SupportLoweringError::Scene)
    }
}

fn validate_lowered_output(
    structured: &StructuredSupportRun<'_>,
    lowered: &LoweredSupportPlan,
    config: LoweringConfig,
) -> Result<(), SupportLoweringError> {
    let balls = structured.joints.validate_filtered(
        &structured.braced.plan.forest,
        &structured.live_view,
        config.joint_config(),
    )?;
    structured.raft.validate(
        &structured.braced.plan.forest,
        &structured.live_view,
        structured.context.frame().plate_z_mm(),
    )?;
    let primitives = lowered.validate(
        &structured.braced.plan.forest,
        &structured.live_view,
        structured.context.frame(),
    )?;
    let support_count = lowered.support_primitive_count();
    let raft_count = lowered.raft_primitive_count();
    let member_count = support_count
        .checked_sub(balls.len())
        .ok_or(SupportLoweringError::PrimitiveCountMismatch)?;
    if raft_count != structured.raft.primitives().len()
        || primitives.len().checked_sub(support_count) != Some(raft_count)
    {
        return Err(SupportLoweringError::PrimitiveCountMismatch);
    }
    if primitives[..member_count].iter().any(|primitive| {
        matches!(
            primitive.segment_kind,
            crate::SupportSegmentKind::JointSphere
                | crate::SupportSegmentKind::RaftPad
                | crate::SupportSegmentKind::RaftBridge
        )
    }) || primitives[member_count..support_count]
        .iter()
        .any(|primitive| primitive.segment_kind != crate::SupportSegmentKind::JointSphere)
        || primitives[support_count..].iter().any(|primitive| {
            !matches!(
                primitive.segment_kind,
                crate::SupportSegmentKind::RaftPad | crate::SupportSegmentKind::RaftBridge
            )
        })
    {
        return Err(SupportLoweringError::PrimitiveOrderMismatch);
    }
    Ok(())
}

fn finalized_unrouted_terminal_ids(
    transcript: &FinalizedCcoTranscript,
    forest: &Forest,
) -> Result<Vec<u32>, SupportStructureError> {
    Ok(finalized_unrouted_terminals(transcript, forest)?
        .into_iter()
        .map(|unrouted| unrouted.terminal_id)
        .collect())
}

/// Every terminal the sealed cadence did NOT route, with the reason, in
/// transcript order.
///
/// The single source of truth for "unrouted", used twice and deliberately not
/// implemented twice: the structure stage filters these terminals out of the
/// live view, and the routing stage removes their regular contacts from the
/// accepted contact set (SPEC-A3-A4 §6). Two implementations could disagree
/// about which terminals those are, which is exactly the class of divergence
/// the transcript/ledger cross-checks below exist to catch.
fn finalized_unrouted_terminals(
    transcript: &FinalizedCcoTranscript,
    forest: &Forest,
) -> Result<Vec<UnroutedTerminal>, UnroutedTerminalError> {
    transcript.validate_integrity()?;
    let mut transcript_terminals = BTreeSet::new();
    let mut unrouted = Vec::new();
    for row in transcript.rows() {
        let terminal = row.terminal_id();
        let ledger = forest
            .terminal_ledger(terminal)
            .ok_or(UnroutedTerminalError::TerminalEvidenceMismatch { terminal })?;
        if !transcript_terminals.insert(terminal)
            || ledger.original_key != row.original_key()
            || row.physical_contact_id() != u64::from(terminal)
        {
            return Err(UnroutedTerminalError::TerminalEvidenceMismatch { terminal });
        }
        match row.kind() {
            FinalizedDispositionKind::Routed if ledger.state == TerminalLedgerState::Consumed => {}
            FinalizedDispositionKind::SkippedQuarantined
                if ledger.state == TerminalLedgerState::Quarantined =>
            {
                unrouted.push(UnroutedTerminal {
                    terminal_id: terminal,
                    contact_id: row.physical_contact_id(),
                    reason: UnroutedContactReason::Quarantined,
                });
            }
            FinalizedDispositionKind::UnroutedNoWinner
                if ledger.state == TerminalLedgerState::Retired =>
            {
                unrouted.push(UnroutedTerminal {
                    terminal_id: terminal,
                    contact_id: row.physical_contact_id(),
                    reason: UnroutedContactReason::NoWinner,
                });
            }
            _ => return Err(UnroutedTerminalError::TerminalEvidenceMismatch { terminal }),
        }
    }
    if forest
        .terminal_ledger_iter()
        .any(|entry| !transcript_terminals.contains(&entry.contact))
    {
        return Err(UnroutedTerminalError::TerminalLedgerCardinalityMismatch);
    }
    Ok(unrouted)
}

/// One terminal the sealed cadence did not route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UnroutedTerminal {
    terminal_id: u32,
    contact_id: u64,
    reason: UnroutedContactReason,
}

#[derive(Debug, Error)]
pub(crate) enum UnroutedTerminalError {
    #[error(transparent)]
    Cco(#[from] CcoError),
    #[error(
        "finalized transcript terminal {terminal} does not match final forest lifecycle evidence"
    )]
    TerminalEvidenceMismatch { terminal: u32 },
    #[error("finalized transcript and final forest have different terminal namespaces")]
    TerminalLedgerCardinalityMismatch,
}

impl From<UnroutedTerminalError> for SupportStructureError {
    fn from(error: UnroutedTerminalError) -> Self {
        match error {
            UnroutedTerminalError::Cco(cco) => Self::Cco(cco),
            UnroutedTerminalError::TerminalEvidenceMismatch { terminal } => {
                Self::TerminalEvidenceMismatch { terminal }
            }
            UnroutedTerminalError::TerminalLedgerCardinalityMismatch => {
                Self::TerminalLedgerCardinalityMismatch
            }
        }
    }
}

impl From<UnroutedTerminalError> for SupportRoutingError {
    fn from(error: UnroutedTerminalError) -> Self {
        match error {
            UnroutedTerminalError::Cco(cco) => Self::Cco(cco),
            UnroutedTerminalError::TerminalEvidenceMismatch { terminal } => {
                Self::TerminalEvidenceMismatch { terminal }
            }
            UnroutedTerminalError::TerminalLedgerCardinalityMismatch => {
                Self::TerminalLedgerCardinalityMismatch
            }
        }
    }
}

impl SupportRunContext<'_> {
    fn validate_brace_preflight(
        &self,
        state: &RewiredPlanState,
        geometry: &AnchorGeometryAuthority<'_>,
        loads: &FinalizedContactLoads,
    ) -> Result<(), SupportBraceError> {
        state.routed.plan.validate_binding(
            &self.mesh,
            &self.authority.perception_config,
            self.frame,
            self.authority.layers,
            &self.authority.candidate_config,
            self.authority.reservations,
            self.authority.perception_limits,
            &self.generic,
            &self.regular,
            &self.authority.selection_config,
            self.authority.specialized_limits,
        )?;
        if self.generic.digest() != self.digests.generic
            || self.regular.digest() != self.digests.regular
            || state.routed.plan.digest() != self.digests.plan
            || state.routed.plan.finalized_load_plan_digest() != state.routed.plan.digest()
            || loads.digest() != self.digests.loads
            || prepared_digest(&self.digests, self.frame, self.authority) != self.digests.prepared
            || self.brace_penetration_limits != self.authority.brace_penetration_limits
        {
            return Err(SupportBraceError::PlanOrLoadDigestMismatch);
        }
        geometry.validate_mesh(&self.mesh)?;
        if geometry.digest() != self.digests.geometry
            || state.routed.initial.plan_digest() != state.routed.plan.digest()
            || state.routed.initial.load_digest() != loads.digest()
            || state.routed.initial.native_digest() != geometry.digest()
        {
            return Err(SupportBraceError::GeometryDigestMismatch);
        }
        if state.routed.initial.initial_forest_digest() != Forest::new().digest()
            || state.routed.initial.post_quarantine_forest_digest() != self.digests.initial_forest
            || state.routed.initial.contact_attestation_digest() != self.digests.contact_attestation
            || !state
                .routed
                .initial
                .all_terminals_registered_before_quarantine()
        {
            return Err(SupportBraceError::InitialAuthorityMismatch);
        }
        state.routed.transcript.validate_integrity()?;
        if state.routed.transcript.plan_digest() != state.routed.plan.digest()
            || state.routed.transcript.finalized_load_digest() != loads.digest()
            || state.routed.transcript.geometry_digest() != geometry.digest()
            || state.routed.transcript.initial_forest_digest()
                != state.routed.initial.initial_forest_digest()
            || state
                .routed
                .transcript
                .initial_post_quarantine_forest_digest()
                != state.routed.initial.post_quarantine_forest_digest()
            // FU-32 J2 (§D2.2/F6) -- REAL tripwire: recompute-from-state and
            // compare.  `sealed_digest_matches_rebuilt` rebuilds both digest
            // accumulators from scratch (never the cached fields) and also
            // cross-checks the cached pair, so a controller mutated after
            // seal AND a controller whose incremental accumulators drifted
            // both fail here.
            || !state
                .routed
                .controller
                .sealed_digest_matches_rebuilt(state.routed.transcript.final_controller_digest())
        {
            return Err(SupportBraceError::TranscriptDigestMismatch);
        }
        state
            .routed
            .initial
            .forest()
            .validate()
            .map_err(RewireError::from)?;
        if state.post_rewire_forest_digest != state.routed.initial.forest().digest()
            || rewire_report_digest(&state.report) != state.rewire_report_digest
        {
            return Err(SupportBraceError::RewireDigestMismatch);
        }
        // SPEC-A4 §6: bracing is the second stage the clause names.
        validate_accepted_contact_set(&state.routed, state.routed.initial.forest())?;
        Ok(())
    }

    fn validate_rewire_preflight(
        &self,
        state: &RoutedPlanState,
        geometry: &AnchorGeometryAuthority<'_>,
        loads: &FinalizedContactLoads,
    ) -> Result<(), SupportRewireError> {
        state.plan.validate_binding(
            &self.mesh,
            &self.authority.perception_config,
            self.frame,
            self.authority.layers,
            &self.authority.candidate_config,
            self.authority.reservations,
            self.authority.perception_limits,
            &self.generic,
            &self.regular,
            &self.authority.selection_config,
            self.authority.specialized_limits,
        )?;
        if self.generic.digest() != self.digests.generic
            || self.regular.digest() != self.digests.regular
            || state.plan.digest() != self.digests.plan
            || state.plan.finalized_load_plan_digest() != state.plan.digest()
            || loads.digest() != self.digests.loads
            || prepared_digest(&self.digests, self.frame, self.authority) != self.digests.prepared
        {
            return Err(SupportRewireError::PlanOrLoadDigestMismatch);
        }
        geometry.validate_mesh(&self.mesh)?;
        if geometry.digest() != self.digests.geometry
            || state.initial.plan_digest() != state.plan.digest()
            || state.initial.load_digest() != loads.digest()
            || state.initial.native_digest() != geometry.digest()
        {
            return Err(SupportRewireError::GeometryDigestMismatch);
        }
        if state.initial.initial_forest_digest() != Forest::new().digest()
            || state.initial.post_quarantine_forest_digest() != self.digests.initial_forest
            || state.initial.contact_attestation_digest() != self.digests.contact_attestation
            || !state.initial.all_terminals_registered_before_quarantine()
        {
            return Err(SupportRewireError::InitialAuthorityMismatch);
        }
        state.transcript.validate_integrity()?;
        if state.transcript.plan_digest() != state.plan.digest()
            || state.transcript.finalized_load_digest() != loads.digest()
            || state.transcript.geometry_digest() != geometry.digest()
            || state.transcript.initial_forest_digest() != state.initial.initial_forest_digest()
            || state.transcript.initial_post_quarantine_forest_digest()
                != state.initial.post_quarantine_forest_digest()
            || state.transcript.final_forest_digest() != state.initial.forest().digest()
            // FU-32 J2 (§D2.2/F6) -- REAL tripwire: recompute-from-state and
            // compare, exactly as at the brace boundary above.
            || !state
                .controller
                .sealed_digest_matches_rebuilt(state.transcript.final_controller_digest())
        {
            return Err(SupportRewireError::TranscriptDigestMismatch);
        }
        state
            .initial
            .forest()
            .validate()
            .map_err(RewireError::from)?;
        // SPEC-A4 §6: rewire is the first stage the clause names, so the
        // accepted contact set is proved here before it runs.
        validate_accepted_contact_set(state, state.initial.forest())?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub(crate) enum SupportPreparationError {
    #[error(transparent)]
    PerceptionInput(#[from] SupportPerceptionError),
    #[error(transparent)]
    Perception(#[from] GenericPerceptionError),
    #[error(transparent)]
    Selection(#[from] ContactSelectionError),
    #[error(transparent)]
    Specialized(#[from] SpecializedError),
    #[error(transparent)]
    Surface(#[from] SurfaceBuildError),
    #[error(transparent)]
    Geometry(#[from] AnchorSearchError),
    #[error(transparent)]
    Initial(#[from] InitialContactError),
    #[error("support preparation perception and selection delicacy authorities disagree")]
    DelicacyAuthorityMismatch,
    #[error("support preparation selection resource authority is invalid")]
    InvalidSelectionResources,
    #[error("support preparation brace penetration resource authority is invalid")]
    InvalidBracePenetrationResources,
    #[error("support preparation raft resource authority is invalid")]
    InvalidRaftResources,
    #[error("support preparation lowering resource authority is invalid")]
    InvalidLoweringResources,
    #[error("support preparation layer schedule does not cover the model z range")]
    LayerScheduleCoverage,
    #[error("support preparation reservation names face {face_id} outside the model")]
    ReservationFaceOutOfRange { face_id: u32 },
    #[error("support preparation duplicates owner {owner_id} on face {face_id}")]
    DuplicateReservation { owner_id: u32, face_id: u32 },
    #[error("support preparation mesh fingerprint failed: {0}")]
    MeshFingerprint(String),
}

#[derive(Debug, Error)]
pub(crate) enum SupportRoutingError {
    #[error(transparent)]
    Specialized(#[from] SpecializedError),
    #[error(transparent)]
    Anchor(#[from] AnchorSearchError),
    #[error(transparent)]
    Cco(#[from] CcoError),
    #[error("sealed CCO routing requires the prepared face-order delicacy authority")]
    MissingAnchorDelicacy,
    #[error("sealed CCO routing found stale plan or finalized-load digest authority")]
    PlanOrLoadDigestMismatch,
    #[error("sealed CCO routing found stale mesh, topology, or native-surface digest authority")]
    GeometryDigestMismatch,
    #[error("sealed CCO routing found stale initial-forest authority")]
    InitialAuthorityMismatch,
    /// **Stage B.** Every arm terminal {terminal} can still take at its current
    /// context carries a steering no-good, so the pass stopped rather than
    /// letting a winnerless terminal retire on a count of them. The controller
    /// opens a confirmation round: every one of those arms re-measured under
    /// the frozen set, one per pass, in the tournament's own order.
    ///
    /// It is an error rather than a named termination because the routing pass
    /// it stopped holds a half-grown forest that no caller may reach by
    /// ignoring a field — the same reason Stage A's two refusals are errors.
    #[error(
        "sealed CCO routing terminal {terminal} has a steering no-good for every arm it can \
         still take; the pass stops so a confirmation round can re-measure them"
    )]
    SteeringSpent { terminal: u32 },
    #[error(
        "sealed CCO routing terminal {terminal} is absent or mismatched across the physical namespace"
    )]
    TerminalNamespaceMismatch { terminal: u32 },
    #[error("sealed CCO routing physical/terminal namespaces have unequal cardinality")]
    TerminalNamespaceCardinalityMismatch,
    #[error("sealed CCO routing terminal {terminal} lacks a ledger entry")]
    MissingTerminalLedger { terminal: u32 },
    #[error("sealed CCO routing terminal {terminal} lacks its exact physical contact")]
    MissingPhysicalContact { terminal: u32 },
    #[error(
        "finalized transcript terminal {terminal} does not match final forest lifecycle evidence"
    )]
    TerminalEvidenceMismatch { terminal: u32 },
    #[error("finalized transcript and final forest have different terminal namespaces")]
    TerminalLedgerCardinalityMismatch,
    /// FU-32 J5 (design D3). The routing cadence's demand exceeds a registered
    /// routing cap. The first typed resource refusal this stage has ever had:
    /// before FU-32 it was entirely uncharged (design F8), so an over-large
    /// instance could only be stopped by a wall-clock kill from outside.
    #[error("sealed CCO routing {resource} requires {required}, limit is {limit}")]
    Limit {
        resource: &'static str,
        required: usize,
        limit: usize,
    },
    /// FU-32 J5 (design D3), the G6c idiom: the cadence executed more than the
    /// preflight charged. The charge is a worst case the executing walks cannot
    /// exceed by construction (every grown terminal classifies each pool row
    /// exactly once, and a sample enters a terminal's refusal list at most
    /// once), so this is unreachable today; it exists so a future edit that
    /// lets the charging and executing sides disagree refuses loudly here
    /// instead of quietly overspending an admitted charge.
    #[error("sealed CCO routing executed {executed} {resource}; the preflight charged {charged}")]
    ChargeDrift {
        resource: &'static str,
        charged: usize,
        executed: usize,
    },
    /// FU-32 J5. The cadence-by-pool product does not fit a `usize`.
    #[error("sealed CCO routing resource arithmetic overflow for {0}")]
    ResourceOverflow(&'static str),
    #[error("sealed CCO routing registered constants are unavailable: {0}")]
    RegisteredConstantsUnavailable(String),
}

/// FU-32 J5 (design D3) — the routing cadence's registered execution caps.
///
/// Read from the one M0 publication, never redeclared here. `diagnostic()` is
/// the sanctioned `Placeholder` read for a resource cap: a cap admits or
/// refuses, it never contributes to an accepted result, so it is not on the
/// accept path the wrapper guards (the same reasoning
/// [`SupportPreparationLimits::production`] records).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RouteCcoWorkLimits {
    max_seat_evaluations: usize,
    max_seat_refusal_rows: usize,
}

impl RouteCcoWorkLimits {
    fn registered() -> Result<Self, SupportRoutingError> {
        let placeholders = plinth_core::m0_constants()
            .map_err(|error| {
                SupportRoutingError::RegisteredConstantsUnavailable(error.to_string())
            })?
            .placeholders();
        Ok(Self {
            max_seat_evaluations: cap_from_registry(
                placeholders
                    .support_route_cco_max_seat_evaluations()
                    .diagnostic()
                    .get(),
                "seat evaluations",
            )?,
            max_seat_refusal_rows: cap_from_registry(
                placeholders
                    .support_route_cco_max_seat_refusal_rows()
                    .diagnostic()
                    .get(),
                "seat refusal rows",
            )?,
        })
    }

    #[cfg(test)]
    pub(crate) const fn for_test(
        max_seat_evaluations: usize,
        max_seat_refusal_rows: usize,
    ) -> Self {
        Self {
            max_seat_evaluations,
            max_seat_refusal_rows,
        }
    }
}

fn cap_from_registry(value: u64, resource: &'static str) -> Result<usize, SupportRoutingError> {
    usize::try_from(value).map_err(|_| SupportRoutingError::ResourceOverflow(resource))
}

/// FU-32 J5 (design D3) — what the sealed routing cadence is charged, computed
/// from the instance shape before the first terminal runs.
///
/// The cadence's remaining honest work, once J1 and J2 removed the redundant
/// hashing, is a `Θ(N·P)` seat search: each grown terminal's
/// `discover_finalized_anchor_seats` classifies EVERY exterior anchor-pool row
/// exactly once, and retains one `AnchorSeatRefusal` row for at most each row
/// it refuses. Both worst cases are therefore `cadence × pool_rows`, and both
/// are knowable before any work is done — which is what makes them chargeable.
/// A quarantined terminal spends neither, so the executed totals land at or
/// under the charge and never above it.
///
/// Deliberately NOT charged: the controller digest and the immutable-authority
/// digests. After J1 and J2 they are O(1) and O(delta) respectively, and
/// pricing them would price an implementation detail rather than the work
/// (design D3; the same call G3 made when it declined to charge the gate's
/// executed prefix).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RouteCcoWorkCharge {
    cadence: usize,
    pool_rows: usize,
    seat_evaluations: usize,
    seat_refusal_rows: usize,
}

impl RouteCcoWorkCharge {
    /// The instance's demand. `cadence` counts every entry of the frozen
    /// routing order (quarantined entries included: whether a terminal is
    /// quarantined is not knowable from the shape, so the charge assumes none
    /// are).
    pub(crate) fn compute(cadence: usize, pool_rows: usize) -> Result<Self, SupportRoutingError> {
        let product = cadence
            .checked_mul(pool_rows)
            .ok_or(SupportRoutingError::ResourceOverflow("seat evaluations"))?;
        Ok(Self {
            cadence,
            pool_rows,
            seat_evaluations: product,
            seat_refusal_rows: product,
        })
    }

    pub(crate) const fn seat_evaluations(self) -> usize {
        self.seat_evaluations
    }

    pub(crate) const fn seat_refusal_rows(self) -> usize {
        self.seat_refusal_rows
    }

    pub(crate) const fn cadence(self) -> usize {
        self.cadence
    }

    pub(crate) const fn pool_rows(self) -> usize {
        self.pool_rows
    }

    /// Refuse before the cadence starts if the instance demands more than the
    /// registered caps admit.
    pub(crate) fn preflight(self, limits: RouteCcoWorkLimits) -> Result<(), SupportRoutingError> {
        for (resource, required, limit) in [
            (
                "seat evaluations",
                self.seat_evaluations,
                limits.max_seat_evaluations,
            ),
            (
                "seat refusal rows",
                self.seat_refusal_rows,
                limits.max_seat_refusal_rows,
            ),
        ] {
            if required > limit {
                return Err(SupportRoutingError::Limit {
                    resource,
                    required,
                    limit,
                });
            }
        }
        Ok(())
    }

    /// The receipt: hold the counted execution against what was charged. The
    /// `charge_covers_execution` shape (`initial.rs`), and the reason the
    /// meter is beside the charge rather than instead of it — a run that
    /// outruns its charge refuses here rather than silently spending more than
    /// it was admitted for.
    pub(crate) fn covers_execution(
        self,
        meter: RouteCcoWorkMeter,
    ) -> Result<(), SupportRoutingError> {
        for (resource, charged, executed) in [
            (
                "seat evaluations",
                self.seat_evaluations,
                meter.seat_evaluations,
            ),
            (
                "seat refusal rows",
                self.seat_refusal_rows,
                meter.seat_refusal_rows,
            ),
        ] {
            if executed > charged {
                return Err(SupportRoutingError::ChargeDrift {
                    resource,
                    charged,
                    executed,
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub(crate) enum SupportRewireError {
    #[error(transparent)]
    Specialized(#[from] SpecializedError),
    #[error(transparent)]
    Anchor(#[from] AnchorSearchError),
    #[error(transparent)]
    Cco(#[from] CcoError),
    #[error(transparent)]
    Rewire(#[from] RewireError),
    #[error("sealed rewire requires the prepared face-order delicacy authority")]
    MissingAnchorDelicacy,
    #[error(
        "sealed rewire found stale generic, regular, plan, or finalized-load digest authority"
    )]
    PlanOrLoadDigestMismatch,
    #[error("sealed rewire found stale mesh, topology, or native-surface digest authority")]
    GeometryDigestMismatch,
    #[error("sealed rewire found stale initial-contact construction authority")]
    InitialAuthorityMismatch,
    #[error("sealed rewire found stale finalized CCO transcript authority")]
    TranscriptDigestMismatch,
    #[error(transparent)]
    AcceptedContactSet(#[from] AcceptedContactSetError),
}

#[derive(Debug, Error)]
pub(crate) enum SupportBraceError {
    #[error(transparent)]
    Specialized(#[from] SpecializedError),
    #[error(transparent)]
    Anchor(#[from] AnchorSearchError),
    #[error(transparent)]
    Cco(#[from] CcoError),
    #[error(transparent)]
    Rewire(#[from] RewireError),
    #[error(transparent)]
    Brace(#[from] BraceError),
    #[error(transparent)]
    Transient(#[from] TransientError),
    #[error("sealed brace found stale generic, regular, plan, or finalized-load digest authority")]
    PlanOrLoadDigestMismatch,
    #[error("sealed brace found stale mesh, topology, or native-surface digest authority")]
    GeometryDigestMismatch,
    #[error("sealed brace found stale initial-contact construction authority")]
    InitialAuthorityMismatch,
    #[error("sealed brace found stale finalized CCO transcript authority")]
    TranscriptDigestMismatch,
    #[error("sealed brace found stale rewire forest or report digest authority")]
    RewireDigestMismatch,
    #[error(transparent)]
    AcceptedContactSet(#[from] AcceptedContactSetError),
    #[error("sealed brace plan's final forest digest is stale")]
    FinalForestDigestMismatch,
    #[error(
        "the contact-attestation chain does not reproduce from its own per-family transcripts"
    )]
    ContactAttestationChainMismatch,
    #[error("sealed brace fixed-point plan digest is stale")]
    BracePlanDigestMismatch,
    #[error("sealed brace plan's transient report or report digest is stale")]
    TransientReportDigestMismatch,
    /// **Stage A.** The bracing pass measured a sealed-`Routed` terminal's own
    /// chosen arm inside the model and abandoned rather than minting the
    /// terminal pair the accounting forbids. The whole disturbed class travels
    /// with it — every affected terminal, its own arms, its cascade carriers,
    /// and every measured refusal in millimetres.
    ///
    /// **Stage B adds the second payload**: the same refusal read as an
    /// instruction for the next pass. The class is the physics; the abandon is
    /// what the controller does about it. They are separate fields because a
    /// caller that only reports has no business reading the steering, and a
    /// caller that steers must not be able to invent the physics.
    #[error("{0}")]
    SealedRoutedTerminalCondemned(
        Box<crate::brace::SealedCondemnationClass>,
        Box<crate::legalization::LegalizationAbandon>,
    ),
    /// **Stage A.** The bracing pass would have disturbed sealed terminals, but
    /// no sealed-`Routed` terminal's own chosen arm was condemned, so the pass
    /// has learned nothing it could steer with.
    #[error("{0}")]
    SealedSweepLearnedNothing(Box<crate::brace::SealedCondemnationClass>),
}

#[derive(Debug, Error)]
pub(crate) enum SupportStructureError {
    #[error(transparent)]
    Brace(#[from] SupportBraceError),
    #[error(transparent)]
    BraceLeaf(#[from] BraceError),
    #[error(transparent)]
    Cco(#[from] CcoError),
    #[error(transparent)]
    Joint(#[from] JointError),
    #[error(transparent)]
    Raft(#[from] SupportError),
    #[error("registered joint-ball factor is unavailable: {0}")]
    RegisteredConstants(String),
    #[error(
        "finalized transcript terminal {terminal} does not match final forest lifecycle evidence"
    )]
    TerminalEvidenceMismatch { terminal: u32 },
    #[error("finalized transcript and final forest have different terminal namespaces")]
    TerminalLedgerCardinalityMismatch,
}

#[derive(Debug, Error)]
pub(crate) enum SupportLoweringError {
    #[error(transparent)]
    Structure(#[from] SupportStructureError),
    #[error(transparent)]
    Lowering(#[from] LoweringError),
    #[error(transparent)]
    Joint(#[from] JointError),
    #[error(transparent)]
    Raft(#[from] SupportError),
    #[error(transparent)]
    Verification(#[from] SupportVerificationError),
    #[error(transparent)]
    Scene(SupportError),
    #[error("sealed lowering primitive counts do not match semantic authority")]
    PrimitiveCountMismatch,
    #[error("sealed lowering primitive order is not member, joint balls, then raft")]
    PrimitiveOrderMismatch,
}

/// Execute the sealed preparation pipeline through the one batched initial-contact build.
pub(crate) fn prepare_support_run<'a>(
    hollowed: HollowedModel,
    request: SupportRequest,
    authority: SupportPreparationAuthority<'a>,
) -> Result<PreparedSupportRun<'a>, SupportPreparationError> {
    let (context, plan) = prepare_support_context(hollowed, request, authority)?;
    // The single-attempt entry has nothing to recover the context FOR: it is
    // the whole run, so a preparation failure ends it. Dropping the context
    // here is what this function has always done.
    prepared_run_for_plan(context, plan).map_err(|(_recovered_context, error)| error)
}

/// Part (a) of the sealed-preparation split (FU-56): everything that does NOT
/// change when the coverage-closure controller appends repair rows to the
/// specialized plan. Validation, the generic/baseline-regular/specialized
/// contact plan, and the retained mesh/topology/`NativeSurface`/geometry
/// trio are all built exactly once per model, here — never once per closure
/// attempt, which is the memory-critical property `into_context` (below)
/// preserves: cloning this instead of recycling it would put one
/// `NativeSurface` grid on the machine per attempt.
///
/// The returned plan is the FIRST specialized plan for this model, with no
/// repair rows appended yet; [`prepared_run_for_plan`] is what turns a
/// context and a (possibly repair-extended) plan into a runnable
/// [`PreparedSupportRun`].
pub(crate) fn prepare_support_context<'a>(
    hollowed: HollowedModel,
    request: SupportRequest,
    authority: SupportPreparationAuthority<'a>,
) -> Result<(SupportRunContext<'a>, SpecializedContactPlan), SupportPreparationError> {
    let (mesh, hollow_history) = hollowed.into_parts();

    // Validate every independently stale-able caller input before the first native query. The
    // returned topology is the exact S0 topology authority retained for initial construction.
    let (topology, perception_delicacy) =
        validate_perception_input(&mesh, &authority.perception_config)?;
    validate_authority_pre_query(&mesh, perception_delicacy, authority)?;

    let frame = BuildFrame::new(authority.perception_config.plate_z_mm, request.lift_mm);
    let generic = prepare_generic_contacts(
        &mesh,
        &authority.perception_config,
        frame,
        authority.layers,
        &authority.candidate_config,
        authority.reservations,
        authority.perception_limits,
    )?;
    let regular = if generic.islands().is_empty() {
        SelectedContacts::new(Vec::new(), Vec::new())
    } else {
        select_contacts_baseline(
            &mesh,
            generic.candidates(),
            generic.islands(),
            &authority.selection_config,
        )?
    };
    let plan = prepare_specialized_contacts(
        &mesh,
        &authority.perception_config,
        frame,
        authority.layers,
        &authority.candidate_config,
        authority.reservations,
        authority.perception_limits,
        &generic,
        &regular,
        &authority.selection_config,
        authority.specialized_limits,
    )?;

    let surface = NativeSurface::from_indexed(
        mesh.verts(),
        mesh.tris(),
        NativeSurfaceConfig {
            grid_pitch_mm: authority.perception_config.surface_grid_pitch_mm.get(),
            max_grid_bytes: authority.perception_config.max_grid_bytes,
        },
    )?;
    let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface)?;
    let geometry_digest = geometry.digest();
    let mesh_digest = mesh_fingerprint(&mesh)
        .map_err(|error| SupportPreparationError::MeshFingerprint(error.to_string()))?;

    let digests = PreparedSupportDigests {
        mesh: mesh_digest,
        generic: generic.digest(),
        regular: regular.digest(),
        // Plan-dependent digests are not known until a plan is bound by
        // `prepared_run_for_plan`; they are filled in there, every attempt.
        plan: 0,
        loads: 0,
        geometry: geometry_digest,
        initial_forest: 0,
        contact_attestation: 0,
        prepared: 0,
    };

    Ok((
        SupportRunContext {
            mesh,
            hollow_history,
            request,
            frame,
            authority,
            topology,
            surface,
            generic,
            regular,
            brace_penetration_limits: authority.brace_penetration_limits,
            raft_limits: authority.raft_limits,
            digests,
            // Stage C: empty until the Routing stage of the first attempt
            // asks for it, so the first build's refusals stay where they
            // have always been raised.
            anchor_pool: None,
        },
        plan,
    ))
}

/// Part (b) of the sealed-preparation split (FU-56): bind one specialized
/// plan (the baseline, or a closure attempt's repair-extended child) to a
/// retained context, building the initial-contact construction and every
/// plan-dependent digest fresh. Every existing single-attempt caller is
/// `prepare_support_context` then this, which is exactly what
/// [`prepare_support_run`] does, so no first-attempt-closes digest moves.
#[allow(
    clippy::result_large_err,
    reason = "the Ok variant carries the same SupportRunContext by construction: \
              it is MOVED through the typestate either way, so boxing the Err \
              would allocate on every refusal and save nothing"
)]
pub(crate) fn prepared_run_for_plan<'a>(
    mut context: SupportRunContext<'a>,
    plan: SpecializedContactPlan,
) -> Result<PreparedSupportRun<'a>, (SupportRunContext<'a>, SupportPreparationError)> {
    let initial = match initial_contacts_for_plan(&context, &plan) {
        Ok(initial) => initial,
        Err(error) => return Err((context, error)),
    };

    let loads_digest = match plan.finalized_loads() {
        Ok(loads) => loads.digest(),
        Err(error) => return Err((context, error.into())),
    };
    context.digests.plan = plan.digest();
    context.digests.loads = loads_digest;
    context.digests.initial_forest = initial.post_quarantine_forest_digest();
    context.digests.contact_attestation = initial.contact_attestation_digest();
    context.digests.prepared = prepared_digest(&context.digests, context.frame, context.authority);

    Ok(PreparedSupportRun {
        context,
        plan,
        initial,
    })
}

/// SPEC-A4 §A.3 — the initial-contact build, factored out so
/// [`prepared_run_for_plan`] can hand the context back when a trial's plan
/// cannot be prepared at all.
fn initial_contacts_for_plan(
    context: &SupportRunContext<'_>,
    plan: &SpecializedContactPlan,
) -> Result<InitialContactBuild, SupportPreparationError> {
    let geometry = context.geometry()?;
    Ok(build_initial_contacts(
        plan,
        plan.finalized_loads()?,
        &context.mesh,
        &context.authority.perception_config,
        context.frame,
        context.authority.layers,
        &context.authority.candidate_config,
        context.authority.reservations,
        context.authority.perception_limits,
        &context.generic,
        &context.regular,
        &context.authority.selection_config,
        context.authority.specialized_limits,
        &geometry,
        context.authority.construction_limits,
    )?)
}

fn validate_authority_pre_query(
    mesh: &Mesh,
    perception_delicacy: &[f64],
    authority: SupportPreparationAuthority<'_>,
) -> Result<(), SupportPreparationError> {
    let penetration = authority.brace_penetration_limits;
    if penetration.max_rows == 0
        || penetration.max_total_work == 0
        || penetration.max_total_hits == 0
        || penetration.max_total_samples == 0
        || penetration.max_total_intervals == 0
        || penetration.max_scratch_bytes == 0
    {
        return Err(SupportPreparationError::InvalidBracePenetrationResources);
    }
    if !authority.raft_limits.is_valid() {
        return Err(SupportPreparationError::InvalidRaftResources);
    }
    if authority.max_primitives == 0 || authority.max_primitive_bytes == 0 {
        return Err(SupportPreparationError::InvalidLoweringResources);
    }
    if authority.selection_config.delicacy_by_face != Some(perception_delicacy) {
        return Err(SupportPreparationError::DelicacyAuthorityMismatch);
    }
    if authority
        .selection_config
        .coverage_samples_max_virtual_cells
        == 0
        || authority
            .selection_config
            .coverage_samples_max_compact_payload_bytes
            == 0
        || authority
            .selection_config
            .coverage_samples_max_working_cells
            == 0
        || authority.selection_config.coverage_samples_max_peak_bytes == 0
        || authority.selection_config.coverage_score_matrix_max_entries == 0
        || authority.selection_config.surface_max_grid_bytes == 0
    {
        return Err(SupportPreparationError::InvalidSelectionResources);
    }
    let (z_min, z_max) = mesh.bounds_z();
    let layers = authority.layers;
    if layers.count() == 0
        || layers.z0_mm() > z_min
        || layers.z_of(layers.count() - 1).unwrap_or(f64::NEG_INFINITY)
            + layers.heights_mm()[layers.count() - 1]
            < z_max
    {
        return Err(SupportPreparationError::LayerScheduleCoverage);
    }
    let mut seen = BTreeSet::new();
    for reservation in authority.reservations {
        if reservation.face_id as usize >= mesh.tris().len() {
            return Err(SupportPreparationError::ReservationFaceOutOfRange {
                face_id: reservation.face_id,
            });
        }
        if !seen.insert((reservation.face_id, reservation.owner_id)) {
            return Err(SupportPreparationError::DuplicateReservation {
                owner_id: reservation.owner_id,
                face_id: reservation.face_id,
            });
        }
    }
    Ok(())
}

fn prepared_digest(
    digests: &PreparedSupportDigests,
    frame: BuildFrame,
    authority: SupportPreparationAuthority<'_>,
) -> u64 {
    let mut hash = Fnv::new();
    hash.bytes(digests.mesh.as_bytes());
    hash.u64(digests.generic);
    hash.u64(digests.regular);
    hash.u64(digests.plan);
    hash.u64(digests.loads);
    hash.u64(digests.geometry);
    hash.u64(digests.initial_forest);
    hash.u64(digests.contact_attestation);
    hash.f64(frame.plate_z_mm().get());
    hash.f64(frame.lift_mm().get());
    hash.f64(authority.layers.z0_mm());
    for value in authority.layers.heights_mm() {
        hash.f64(*value);
    }
    let (depth, virtual_cells, candidates, compact_bytes, working_cells, edge_bits) =
        authority.candidate_config.authority_fields();
    hash.u32(depth);
    hash.usize(virtual_cells);
    hash.usize(candidates);
    hash.usize(compact_bytes);
    hash.usize(working_cells);
    hash.u64(edge_bits);
    let penetration = authority.brace_penetration_limits;
    hash.usize(penetration.max_rows);
    hash.usize(penetration.max_total_work);
    hash.usize(penetration.max_total_hits);
    hash.usize(penetration.max_total_samples);
    hash.usize(penetration.max_total_intervals);
    hash.usize(penetration.max_scratch_bytes);
    for value in authority.raft_limits.authority_fingerprint() {
        hash.usize(value);
    }
    hash.usize(authority.max_primitives);
    hash.usize(authority.max_primitive_bytes);
    for reservation in authority.reservations {
        hash.u32(reservation.owner_id);
        hash.u32(reservation.face_id);
    }
    hash.finish()
}

struct Fnv(u64);

impl Fnv {
    const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn byte(&mut self, value: u8) {
        self.0 = (self.0 ^ u64::from(value)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    fn bytes(&mut self, values: &[u8]) {
        for value in values {
            self.byte(*value);
        }
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

fn coverage_samples_digest(samples: &[CoverageSample]) -> u64 {
    let mut hash = Fnv::new();
    for sample in samples {
        hash.u32(sample.island_id);
        hash.u32(sample.face_id);
        hash.u32(sample.component_id);
        for value in sample.point_mm {
            hash.f64(value);
        }
        hash.f64(sample.area_mm2);
    }
    hash.finish()
}

fn named_islands_digest(islands: &[SupportIsland]) -> u64 {
    let mut hash = Fnv::new();
    hash.usize(islands.len());
    for island in islands {
        hash.u32(island.id);
        hash.usize(island.face_ids.len());
        for &face_id in &island.face_ids {
            hash.u32(face_id);
        }
    }
    hash.finish()
}

/// The `(island_id, component_id)` route key of every coverage sample, sorted
/// but not deduplicated, in one gathered pass.
///
/// Sorted, it answers both questions the preflight asks of the sample set --
/// how many distinct keys the samples query, and how many samples query one
/// given key -- without a second pass over the samples for either.
///
/// Batch-first (D-37): the per-sample rescan of the sample prefix this
/// replaces was `O(samples²)`, which the pre-write gate turned into a
/// per-emission cost. The scratch is one `(u32, u32)` per sample and is
/// released before any measurement allocation is charged.
fn sorted_sample_route_keys(
    samples: &[CoverageSample],
) -> Result<Vec<(u32, u32)>, CoverageMeasurementError> {
    let mut keys: Vec<(u32, u32)> = Vec::new();
    keys.try_reserve_exact(samples.len())
        .map_err(|_| CoverageMeasurementError::ResourceLimit {
            resource: "sample route keys",
        })?;
    keys.extend(
        samples
            .iter()
            .map(|sample| (sample.island_id, sample.component_id)),
    );
    keys.sort_unstable();
    #[cfg(test)]
    record_measurement_call(|counts| counts.preflight_route_key_scans += keys.len());
    Ok(keys)
}

/// The number of distinct route keys in an already-sorted key column.
///
/// Counting run starts is one pass and no allocation; `dedup` would be a
/// second write over the same column to learn the same number.
fn distinct_sorted_keys<T: PartialEq>(sorted: &[T]) -> usize {
    usize::from(!sorted.is_empty()) + sorted.windows(2).filter(|pair| pair[0] != pair[1]).count()
}

/// The half-open index range of an already-sorted key column that carries
/// `key`, found by two binary searches rather than a scan.
fn sorted_key_range<T, K: Ord>(
    sorted: &[T],
    key: &K,
    project: impl Fn(&T) -> K,
) -> std::ops::Range<usize> {
    let start = sorted.partition_point(|row| project(row) < *key);
    let end = sorted.partition_point(|row| project(row) <= *key);
    start..end
}

/// One `(island_id, component_id, membership_index)` row per logical route,
/// sorted.
///
/// Batch-first (D-37). Sorted, one run of this column answers the two
/// per-key questions the preflight used to answer by rescanning every
/// membership: the run's length is the number of route entries carrying the
/// key, and the number of distinct membership indices inside the run is the
/// number of memberships carrying it. Both were `O(M)` scans repeated for
/// every one of the `K` distinct keys, on top of an `O(M²)` sealed-slice
/// prefix scan that decided which routes were first occurrences.
///
/// The caller has already refused a route-entry total above its own
/// `max_route_memberships` ceiling, so this scratch is bounded by an admitted
/// cap before it is reserved.
fn sorted_routed_route_key_rows(
    memberships: &[RoutedRouteMembership<'_>],
    total_routes: usize,
) -> Result<Vec<(u32, u32, usize)>, CoverageMeasurementError> {
    let mut rows: Vec<(u32, u32, usize)> = Vec::new();
    rows.try_reserve_exact(total_routes)
        .map_err(|_| CoverageMeasurementError::ResourceLimit {
            resource: "routed projection key rows",
        })?;
    for (membership_index, membership) in memberships.iter().enumerate() {
        rows.extend(
            membership
                .logical_routes
                .iter()
                .map(|route| (route.island_id, route.component_id, membership_index)),
        );
    }
    rows.sort_unstable();
    #[cfg(test)]
    record_measurement_call(|counts| counts.preflight_route_key_scans += rows.len());
    Ok(rows)
}

/// One sorted island-id → ascending-face-ids membership index over the named
/// island sidecar, built once per measurement and asked once per coverage
/// sample by two binary searches.
///
/// This replaces a linear `named_islands` walk ending in a linear
/// `Vec::contains` on that island's faces, evaluated once per sample.
struct IslandFaceIndex<'a> {
    islands: Vec<(u32, &'a [u32])>,
}

impl<'a> IslandFaceIndex<'a> {
    /// # Errors
    ///
    /// Returns [`CoverageMeasurementError::SnapshotBinding`] if the sidecar
    /// does not satisfy the two invariants the binary searches rest on —
    /// unique island ids and strictly ascending face ids. Both are already
    /// proved by [`validate_named_islands`] through `validate_integrity`,
    /// which every measurement entry point runs first; this is the fail-closed
    /// restatement of that precondition at the place that depends on it,
    /// rather than a silent wrong answer if the two ever drift apart.
    fn build(islands: &'a [SupportIsland]) -> Result<Self, CoverageMeasurementError> {
        let mut rows: Vec<(u32, &'a [u32])> = Vec::new();
        rows.try_reserve_exact(islands.len()).map_err(|_| {
            CoverageMeasurementError::ResourceLimit {
                resource: "island membership index",
            }
        })?;
        rows.extend(
            islands
                .iter()
                .map(|island| (island.id, island.face_ids.as_slice())),
        );
        rows.sort_unstable_by_key(|(id, _)| *id);
        if rows.windows(2).any(|pair| pair[0].0 >= pair[1].0)
            || rows
                .iter()
                .any(|(_, faces)| faces.windows(2).any(|pair| pair[0] >= pair[1]))
        {
            return Err(CoverageMeasurementError::SnapshotBinding);
        }
        Ok(Self { islands: rows })
    }

    /// Validates the named-island sidecar against the topology **once** and
    /// indexes it, producing the value every later membership question is
    /// asked of.
    ///
    /// The sidecar is one per snapshot and does not change while a snapshot is
    /// being checked, so proving it well-formed is a once-per-snapshot job.
    /// Before this constructor existed, `routed_logical_routes` re-proved it
    /// per routed contact and then answered membership with a linear island
    /// walk ending in a linear `Vec::contains`, so one `validate_integrity`
    /// over `N` routed contacts walked the whole sidecar `2N + 1` times.
    ///
    /// # Errors
    ///
    /// Returns [`SupportVerificationError::AuthorityMismatch`] if the sidecar
    /// is not a valid named-island set for this topology, or if it does not
    /// satisfy the two invariants the binary searches rest on.
    fn build_named(
        islands: &'a [SupportIsland],
        topology: &FaceTopology,
    ) -> Result<Self, SupportVerificationError> {
        if !validate_named_islands(islands, topology) {
            return Err(SupportVerificationError::AuthorityMismatch);
        }
        Self::build(islands).map_err(|_| SupportVerificationError::AuthorityMismatch)
    }

    fn contains(&self, island_id: u32, face_id: u32) -> bool {
        self.islands
            .binary_search_by_key(&island_id, |(id, _)| *id)
            .is_ok_and(|position| self.islands[position].1.binary_search(&face_id).is_ok())
    }
}

fn validate_named_islands(islands: &[SupportIsland], topology: &FaceTopology) -> bool {
    #[cfg(test)]
    record_measurement_call(|counts| counts.named_island_validations += 1);
    let face_count = topology.component_ids().len();
    let mut ids = BTreeSet::new();
    islands.iter().all(|island| {
        !island.face_ids.is_empty()
            && ids.insert(island.id)
            && island.face_ids.windows(2).all(|faces| faces[0] < faces[1])
            && island
                .face_ids
                .iter()
                .all(|&face_id| (face_id as usize) < face_count)
    })
}

/// The logical routes one routed contact projects onto, checked against the
/// already-validated named-island index.
///
/// `islands` carries the proof that the sidecar is a well-formed named-island
/// set for `topology` ([`IslandFaceIndex::build_named`]), so this function
/// neither re-proves it nor walks it: each route's membership is two binary
/// searches.
fn routed_logical_routes(
    contact: &crate::specialized::SpecializedPhysicalContact,
    islands: &IslandFaceIndex<'_>,
    topology: &FaceTopology,
) -> Result<Vec<RoutedLogicalRoute>, SupportVerificationError> {
    let mut routes = BTreeSet::new();
    for route in contact.regular_routes() {
        routes.insert(RoutedLogicalRoute {
            island_id: route.owner_island_id(),
            face_id: route.coverage_face_id(),
            component_id: route.component_id(),
            provenance: match route.source_kind() {
                crate::specialized::RepairRowSourceKind::Generic => {
                    RoutedRouteProvenance::RegularGeneric
                }
                crate::specialized::RepairRowSourceKind::SyntheticIsland => {
                    RoutedRouteProvenance::RegularSyntheticIsland
                }
            },
        });
    }
    for demand in contact.demands() {
        routes.insert(RoutedLogicalRoute {
            island_id: demand.owner_island_id(),
            face_id: demand.coverage_face_id(),
            component_id: demand.coverage_component_id(),
            provenance: match demand.kind() {
                crate::specialized::SpecializedDemandKind::Minimum => {
                    RoutedRouteProvenance::SpecializedMinimum
                }
                crate::specialized::SpecializedDemandKind::IslandBirth => {
                    RoutedRouteProvenance::SpecializedIslandBirth
                }
            },
        });
    }
    if routes.is_empty() {
        return Err(SupportVerificationError::AuthorityMismatch);
    }
    let routes = routes.into_iter().collect::<Vec<_>>();
    if routes.iter().any(|route| {
        topology.component_ids().get(route.face_id as usize) != Some(&route.component_id)
            || !islands.contains(route.island_id, route.face_id)
    }) {
        return Err(SupportVerificationError::AuthorityMismatch);
    }
    Ok(routes)
}

fn validate_routed_memberships(
    memberships: &[RoutedRouteMembership<'_>],
    islands: &IslandFaceIndex<'_>,
    topology: &FaceTopology,
) -> bool {
    let mut contact_ids = BTreeSet::new();
    memberships.iter().all(|membership| {
        contact_ids.insert(membership.contact.physical_contact_id())
            && routed_logical_routes(membership.contact.contact, islands, topology)
                .is_ok_and(|routes| routes == membership.logical_routes)
    })
}

fn routed_projection_digest(memberships: &[RoutedRouteMembership<'_>]) -> u64 {
    let mut hash = Fnv::new();
    hash.usize(memberships.len());
    for membership in memberships {
        let contact = membership.contact;
        hash.u64(contact.physical_contact_id());
        for value in contact.position_mm() {
            hash.f64(value);
        }
        for value in contact.normal() {
            hash.f64(value);
        }
        hash.f64(contact.carried_area_mm2());
        hash.usize(membership.logical_routes.len());
        for route in &membership.logical_routes {
            hash.u32(route.island_id);
            hash.u32(route.face_id);
            hash.u32(route.component_id);
            hash.byte(match route.provenance {
                RoutedRouteProvenance::RegularGeneric => 0,
                RoutedRouteProvenance::RegularSyntheticIsland => 1,
                RoutedRouteProvenance::SpecializedMinimum => 2,
                RoutedRouteProvenance::SpecializedIslandBirth => 3,
            });
        }
    }
    hash.finish()
}

fn final_live_members_digest(members: &[FinalLiveMemberEvidence]) -> u64 {
    let mut hash = Fnv::new();
    hash.usize(members.len());
    for member in members {
        hash.u32(member.key.slot());
        hash.u32(member.key.generation());
        for value in member.p0_mm {
            hash.f64(value);
        }
        for value in member.p1_mm {
            hash.f64(value);
        }
        hash.f64(member.r0_mm);
        hash.f64(member.r1_mm);
        match &member.kind {
            MemberKind::Stub => hash.byte(0),
            MemberKind::Shoot => hash.byte(1),
            MemberKind::Trunk => hash.byte(2),
            MemberKind::Strut => hash.byte(3),
            MemberKind::Brace => hash.byte(4),
            MemberKind::Other(label) => {
                hash.byte(5);
                hash.bytes(label.as_bytes());
            }
        }
        hash.byte(match member.model_contact_end {
            None => 0,
            Some(ContactEnd::Terminalward) => 1,
            Some(ContactEnd::Rootward) => 2,
        });
    }
    hash.finish()
}

fn verification_snapshot_digest(snapshot: &SupportVerificationSnapshot<'_>) -> u64 {
    let mut hash = Fnv::new();
    hash.bytes(snapshot.mesh_digest.as_bytes());
    hash.bytes(snapshot.topology_digest.as_bytes());
    hash.u64(snapshot.plan_digest);
    hash.u64(snapshot.load_digest);
    hash.u64(snapshot.geometry_digest);
    hash.u64(snapshot.forest_digest);
    hash.u64(snapshot.transcript_digest);
    hash.u64(snapshot.regular_digest);
    hash.u64(snapshot.coverage_digest);
    hash.u64(snapshot.named_islands_digest);
    hash.u64(snapshot.lowered_primitives_digest);
    hash.u64(snapshot.routed_projection_digest);
    hash.u64(snapshot.final_live_members_digest);
    hash.usize(snapshot.coverage_samples.len());
    hash.usize(snapshot.named_islands.len());
    hash.usize(snapshot.routed_contacts.len());
    hash.usize(snapshot.final_live_members.len());
    hash.finish()
}

fn rewire_report_digest(report: &RewireReport) -> u64 {
    let mut hash = Fnv::new();
    hash.f64(report.initial_volume_ml);
    hash.f64(report.final_volume_ml);
    hash.usize(report.curve.len());
    for (accepted, volume_ml) in &report.curve {
        hash.u64(*accepted);
        hash.f64(*volume_ml);
    }
    hash.u64(report.accepted);
    hash.u64(report.rejected);
    hash.u64(report.sweeps);
    hash.u64(report.move_evaluations);
    hash.u64(report.cco_candidate_batches);
    // **`seat_zone_members_refrozen` is DELIBERATELY NOT folded here** (second
    // review, C5/C6). It is disclosure, and it would be a defensible thing to
    // digest — two runs reaching the same forest, one having re-cut every
    // seated exemption and one having re-cut none, are not the same run, and a
    // re-size writes no `Member` so the forest digest cannot separate them.
    //
    // Folding it would move every sealed rewire-report digest in the corpus,
    // and this lane is authorized to move exactly one fingerprint, the CCO
    // transcript's — the same argument `brace_fixed_point_plan_digest` makes
    // for keeping `final_pass_refusals` out. Anyone re-sealing the corpus for
    // another reason should fold it then.
    hash.u64(match report.termination {
        crate::rewire::RewireTermination::FullSweepNoAcceptance => 0,
        crate::rewire::RewireTermination::RollingSavingsConverged => 1,
        crate::rewire::RewireTermination::DeterministicWorkBudget => 2,
    });
    hash.finish()
}

fn transient_report_digest(report: &TransientReport) -> u64 {
    let mut hash = Fnv::new();
    hash.usize(report.members.len());
    for row in &report.members {
        hash.u32(row.member.slot());
        hash.u32(row.member.generation());
        hash.f64(row.worst_span_mm.get());
        hash.usize(row.restraint_zs_mm.len());
        for z in &row.restraint_zs_mm {
            hash.f64(z.get());
        }
        hash.usize(row.free_gaps_mm.len());
        for gap in &row.free_gaps_mm {
            hash.f64(gap.lo_mm.get());
            hash.f64(gap.hi_mm.get());
        }
        hash.usize(row.n_terminals);
        hash.f64(row.worst_span_interval_mm.lo_mm.get());
        hash.f64(row.worst_span_interval_mm.hi_mm.get());
        hash.usize(row.worst_span_path.len());
        for key in &row.worst_span_path {
            hash.u32(key.slot());
            hash.u32(key.generation());
        }
    }
    hash.finish()
}

fn brace_fixed_point_plan_digest(plan: &BraceFixedPointPlan) -> u64 {
    let mut hash = Fnv::new();
    hash.u64(plan.forest.digest());
    hash.usize(plan.additions.len());
    for addition in &plan.additions {
        hash.u32(addition.key.slot());
        hash.u32(addition.key.generation());
        hash.u64(match addition.kind {
            BraceAdditionKind::CrossLink => 0,
            BraceAdditionKind::ModelRung => 1,
        });
        hash_member_spec(&mut hash, &addition.member);
    }
    hash.usize(plan.iterations);
    hash.u64(match plan.termination {
        BraceFixedPointTermination::Converged => 0,
        BraceFixedPointTermination::IterationLimit => 1,
        BraceFixedPointTermination::Unplaceable => 2,
        BraceFixedPointTermination::ContactRefused => 3,
    });
    // PR-9. The refused keys are digested, not just the variant: two runs that
    // both end in `ContactRefused` having refused DIFFERENT contacts are not
    // the same run, and the forest digest cannot tell them apart either, since
    // a quarantined member is absent from both.
    hash.usize(plan.contact_refusals.len());
    for key in &plan.contact_refusals {
        hash.u32(key.slot());
        hash.u32(key.generation());
    }
    // PR-8/OD-9 (F3). The model-rung seat-attestation transcript, for the same
    // reason: a rung the attestation REFUSED never entered the forest, so its
    // absence there is indistinguishable from a rung that merely lost its
    // tournament. Only the transcript separates them, and only if it is
    // digested.
    hash.u64(plan.contact_attestation_digest);
    hash.finish()
}

fn hash_member_spec(hash: &mut Fnv, spec: &MemberSpec) {
    for point in [spec.p0_mm, spec.p1_mm] {
        for coordinate in point {
            hash.f64(coordinate.get());
        }
    }
    hash.f64(spec.r0_mm.get().get());
    hash.f64(spec.r1_mm.get().get());
    match &spec.kind {
        MemberKind::Stub => hash.u64(0),
        MemberKind::Shoot => hash.u64(1),
        MemberKind::Trunk => hash.u64(2),
        MemberKind::Strut => hash.u64(3),
        MemberKind::Brace => hash.u64(4),
        MemberKind::Other(name) => {
            hash.u64(5);
            hash.usize(name.len());
            hash.bytes(name.as_bytes());
        }
    }
    match &spec.owner {
        Owner::Contact(id) => {
            hash.u64(0);
            hash.u32(*id);
        }
        Owner::ModelAnchor(id) => {
            hash.u64(1);
            hash.u32(*id);
        }
        Owner::Generated { family, ordinal } => {
            hash.u64(2);
            hash.usize(family.len());
            hash.bytes(family.as_bytes());
            hash.u32(*ordinal);
        }
    }
    hash.u64(match spec.model_contact_end {
        None => 0,
        Some(ContactEnd::Terminalward) => 1,
        Some(ContactEnd::Rootward) => 2,
    });
    hash.f64(spec.carried_mm2.get());
}

#[cfg(test)]
mod tests {
    use std::{mem::size_of, path::PathBuf, process::Command};

    use crate::CoverageMeasurementLimits;
    use plinth_core::{Finite, PositiveFinite};
    use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
    use plinth_hollow::{HollowedModel, SolidDecisionReason};
    use plinth_mesh::{make_box, make_overhang_table};
    use plinth_orient::{
        BaselineSelection, OrientationHistory, OrientationProvenance, OrientationReport,
        RigidTransform,
    };

    use super::*;
    use geom_kernel::KnnBatchLimits;

    /// The two questions the coverage measurement asks once per sample,
    /// answered at a scale a per-sample rescan cannot reach.
    ///
    /// 200,000 samples is a fraction of a flagship model's coverage set and
    /// runs in milliseconds through a sort/dedup and a pair of binary
    /// searches. The forms these replaced — a rescan of the sample prefix per
    /// sample, and a linear island walk ending in a linear `Vec::contains` per
    /// sample — are ~4e10 comparisons on this same fixture, so a revert to
    /// either does not merely slow this test down, it stops it finishing.
    /// That is the regression worth pinning: the pre-write gate made these run
    /// on every production emission (D-37).
    #[test]
    fn coverage_sample_keys_and_island_membership_are_answered_at_corpus_scale() {
        const SAMPLES: u32 = 200_000;
        const ISLANDS: u32 = 64;
        const FACES_PER_ISLAND: u32 = 1_000;
        const COMPONENTS: u32 = 7;

        let islands = (0..ISLANDS)
            .map(|id| SupportIsland {
                id,
                face_ids: (0..FACES_PER_ISLAND)
                    .map(|face| id * FACES_PER_ISLAND + face)
                    .collect(),
            })
            .collect::<Vec<_>>();
        let samples = (0..SAMPLES)
            .map(|index| {
                let island_id = index % ISLANDS;
                CoverageSample {
                    island_id,
                    face_id: island_id * FACES_PER_ISLAND + index % FACES_PER_ISLAND,
                    component_id: index % COMPONENTS,
                    point_mm: [0.0; 3],
                    area_mm2: 1.0,
                }
            })
            .collect::<Vec<_>>();

        // `ISLANDS` and `COMPONENTS` are coprime, so the index walks every
        // pair: the distinct route keys are the whole product.
        assert_eq!(
            distinct_sorted_keys(&sorted_sample_route_keys(&samples).expect("sample route keys")),
            (ISLANDS * COMPONENTS) as usize,
        );

        let index = IslandFaceIndex::build(&islands).expect("well-formed island sidecar");
        assert!(
            samples
                .iter()
                .all(|sample| index.contains(sample.island_id, sample.face_id)),
            "every sample names a face of its own island",
        );
        assert!(
            !index.contains(ISLANDS, 0),
            "an island id that is not in the sidecar has no faces",
        );
        assert!(
            !index.contains(0, FACES_PER_ISLAND),
            "a face belonging to another island is not a member of this one",
        );
    }

    /// The membership index refuses a sidecar its binary searches cannot
    /// answer, rather than reporting a confident wrong membership.
    ///
    /// `validate_named_islands` already proves both invariants through
    /// `validate_integrity`, which runs before every measurement; this is the
    /// fail-closed restatement at the point that depends on them.
    #[test]
    fn the_island_membership_index_refuses_a_sidecar_it_cannot_binary_search() {
        let unsorted_faces = vec![SupportIsland {
            id: 0,
            face_ids: vec![3, 1],
        }];
        assert_eq!(
            IslandFaceIndex::build(&unsorted_faces).err(),
            Some(CoverageMeasurementError::SnapshotBinding),
        );

        let duplicate_ids = vec![
            SupportIsland {
                id: 4,
                face_ids: vec![0],
            },
            SupportIsland {
                id: 4,
                face_ids: vec![1],
            },
        ];
        assert_eq!(
            IslandFaceIndex::build(&duplicate_ids).err(),
            Some(CoverageMeasurementError::SnapshotBinding),
        );
    }

    /// Every ceiling the pre-write coverage gate measures under comes from the
    /// registered owner of the quantity it bounds — in particular the route
    /// kNN batch is charged to its own `max_logical_peak_bytes` and not to the
    /// specialized stage's coverage *scratch* ceiling.
    ///
    /// The two are not interchangeable and the substitution was not
    /// conservative bookkeeping: the scratch ceiling is a quarter of the
    /// batch's, so a support run the specialized stage was permitted to
    /// complete could come back from the gate as `Unmeasurable`, which reads
    /// as a physics refusal.
    #[test]
    fn the_coverage_gate_charges_the_route_knn_batch_to_its_own_registered_ceiling() {
        let production = SupportPreparationLimits::production().expect("registered support policy");
        let route_knn = production.specialized_limits.coverage_route_knn;
        let scratch_bytes = production.specialized_limits.max_coverage_scratch_bytes;
        let limits = production
            .coverage_measurement_limits()
            .expect("registered coverage ceilings");

        assert_eq!(
            limits.max_route_knn_peak_bytes(),
            route_knn.max_logical_peak_bytes,
            "the route kNN batch must be held to its own registered ceiling",
        );
        assert_eq!(
            limits.max_logical_peak_bytes(),
            scratch_bytes,
            "the measurement's retained peak keeps the coverage scratch ceiling",
        );
        assert_ne!(
            limits.max_route_knn_peak_bytes(),
            limits.max_logical_peak_bytes(),
            "these are two different registered caps; if they ever coincide, \
             this test stops proving that one is not standing in for the other",
        );
    }

    const GRID_BYTES: usize = 64 * 1024 * 1024;
    const TEST_KNN_LIMITS: KnnBatchLimits = KnnBatchLimits {
        max_source_points: 20_000_000,
        max_query_points: 20_000_000,
        max_neighbors: 1,
        max_logical_peak_bytes: 2 * 1024 * 1024 * 1024,
        max_work: 100_000_000,
    };
    const SWEEP_CHILD: &str = "PLINTH_RUNNER_PREPARE_SWEEP_CHILD";
    const SWEEP_LINE: &str = "RUNNER_PREPARE_SWEEP=";

    fn required_runner_corpus_path() -> PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/baseline/make_overhang_table.prepped.stl")
    }

    fn load_required_runner_corpus() -> Mesh {
        let path = required_runner_corpus_path();
        assert!(
            path.is_file(),
            "RequiredRunnerCorpusMissing: {}",
            path.display()
        );
        plinth_mesh::load_binary_stl(&path, plinth_mesh::WeldPolicy::None).unwrap_or_else(|error| {
            panic!(
                "RequiredRunnerCorpusUnreadable: {}: {error}",
                path.display()
            )
        })
    }

    fn procedural_overhang_for_error_case() -> Mesh {
        make_overhang_table().expect("procedural overhang error-case fixture")
    }

    fn hollowed_fixture(mesh: Mesh) -> HollowedModel {
        let budget =
            DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
                .expect("registered decimation budget");
        let decimated = run_decimation(&mesh, budget)
            .expect("required fixture decimation")
            .into_decimated_model()
            .expect("required fixture scale");
        let (mesh, decimation) = decimated.into_parts();
        let orientation = OrientationHistory::from_reports(
            decimation,
            OrientationReport {
                provenance: OrientationProvenance::Pinned {
                    pinned_report: PathBuf::from("runner-required-fixture.pipeline.json"),
                },
                selection: BaselineSelection::Identity,
                transform: RigidTransform::identity(),
            },
        );
        HollowedModel::solid_decision(mesh, orientation, SolidDecisionReason::BelowThreshold)
            .expect("required fixture enters hollow hand-off")
    }

    fn layers() -> LayerPlan {
        LayerPlan::new(0.0, vec![0.05; 1_000]).expect("covering fixture layers")
    }

    fn candidate_config() -> CandidatePayloadConfig {
        CandidatePayloadConfig::current(8, 1_000_000, 3_000_000, 256 * 1024 * 1024, 65_536)
            .expect("registered candidate config")
    }

    // This is caller-owned test runtime capacity for the named corpus.  It
    // is deliberately finite and not a production/default policy.
    fn brace_penetration_limits() -> PenetrationBatchLimits {
        PenetrationBatchLimits {
            max_rows: 100_000,
            max_total_work: 1_000_000_000,
            max_total_hits: 1_000_000_000,
            max_total_samples: 10_000_000_000,
            max_total_intervals: 1_000_000_000,
            max_scratch_bytes: 100_000_000_000,
        }
    }

    /// #36 — the face adjacency for the SAME indexed mesh
    /// `remote_brace_surface` builds its surface from.
    fn remote_brace_topology() -> plinth_mesh::FaceTopology {
        let (vertices, faces) = remote_brace_parts();
        let mesh = plinth_mesh::Mesh::new(vertices, faces).expect("remote brace fixture mesh");
        plinth_mesh::build_face_topology(&mesh, plinth_mesh::WeldPolicy::ExactBits)
            .expect("remote brace fixture topology")
    }

    fn remote_brace_parts() -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        let vertices = vec![
            [100.0, 100.0, 100.0],
            [110.0, 100.0, 100.0],
            [110.0, 110.0, 100.0],
            [100.0, 110.0, 100.0],
            [100.0, 100.0, 110.0],
            [110.0, 100.0, 110.0],
            [110.0, 110.0, 110.0],
            [100.0, 110.0, 110.0],
        ];
        let faces = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        (vertices, faces)
    }

    fn remote_brace_surface() -> NativeSurface {
        let (vertices, faces) = remote_brace_parts();
        NativeSurface::from_indexed(
            &vertices,
            &faces,
            NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: 4 * 1024 * 1024,
            },
        )
        .expect("remote brace fixture builds")
    }

    fn brace_fixture_member(height_mm: f64, ordinal: u32) -> crate::forest::Member {
        crate::forest::Member::new(MemberSpec {
            p0_mm: [
                Finite::new(ordinal as f64 * 2.0).unwrap(),
                Finite::new(0.0).unwrap(),
                Finite::new(height_mm).unwrap(),
            ],
            p1_mm: [
                Finite::new(ordinal as f64 * 2.0).unwrap(),
                Finite::new(0.0).unwrap(),
                Finite::new(0.0).unwrap(),
            ],
            r0_mm: crate::forest::TerminalEndRadius::new(PositiveFinite::new(0.5).unwrap()),
            r1_mm: crate::forest::RootwardEndRadius::new(PositiveFinite::new(0.5).unwrap()),
            kind: MemberKind::Trunk,
            owner: Owner::Generated {
                family: "runner-brace-test".to_owned(),
                ordinal,
            },
            model_contact_end: None,
            carried_mm2: Finite::new(0.0).unwrap(),
        })
    }

    fn authority<'a>(
        delicacy: &'a [f64],
        layers: &'a LayerPlan,
    ) -> SupportPreparationAuthority<'a> {
        let perception = SupportPerceptionConfig::current(
            Finite::new(0.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            GRID_BYTES,
            Some(delicacy),
        )
        .unwrap();
        let selection =
            ContactSelectionConfig::current_defaults(Some(delicacy), GRID_BYTES).unwrap();
        SupportPreparationAuthority::new(
            layers,
            perception,
            candidate_config(),
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
            selection,
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work()),
            ContactConstructionLimits::production(),
            brace_penetration_limits(),
            RaftLimits::new(100_000, 100_000, 200_000, 128 * 1024 * 1024),
            200_000,
            128 * 1024 * 1024,
        )
    }

    /// Drop the recovered context from a stage refusal.
    ///
    /// SPEC-A4 §A.3's recoverable seam returns `(SupportRunContext, E)` so a
    /// transactional trial can take the context back and try the next
    /// candidate. A test that asserts on the ERROR wants only the error, and
    /// dropping the context here is exactly what the pre-seam signature did
    /// implicitly at every one of these sites.
    fn stage_error<T, E>(result: Result<T, (SupportRunContext<'_>, E)>) -> Result<T, E> {
        result.map_err(|(_recovered_context, error)| error)
    }

    fn with_prepared_table<T>(use_prepared: impl FnOnce(PreparedSupportRun<'_>) -> T) -> T {
        let hollowed = hollowed_fixture(load_required_runner_corpus());
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        prepare_fixture(hollowed, &delicacy, &layers, use_prepared)
    }

    fn with_braced_table<T>(use_braced: impl FnOnce(BracedSupportRun<'_>) -> T) -> T {
        with_prepared_table(|prepared| {
            let braced = prepared
                .route_initial_cco(None)
                .expect("required corpus routes")
                .rewire()
                .expect("required corpus rewires")
                .brace()
                .expect("required corpus braces");
            use_braced(braced)
        })
    }

    fn with_structured_limits<T>(
        max_primitives: usize,
        max_primitive_bytes: usize,
        use_structured: impl FnOnce(StructuredSupportRun<'_>) -> T,
    ) -> T {
        let hollowed = hollowed_fixture(load_required_runner_corpus());
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        let mut authority = authority(&delicacy, &layers);
        authority.max_primitives = max_primitives;
        authority.max_primitive_bytes = max_primitive_bytes;
        let prepared = prepare_support_run(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).unwrap(),
            },
            authority,
        )
        .expect("required fixture preparation");
        let structured = prepared
            .route_initial_cco(None)
            .expect("required corpus routes")
            .rewire()
            .expect("required corpus rewires")
            .brace()
            .expect("required corpus braces")
            .structure()
            .expect("required corpus structures");
        use_structured(structured)
    }

    fn prepare_fixture<T>(
        hollowed: HollowedModel,
        delicacy: &[f64],
        layers: &LayerPlan,
        use_prepared: impl FnOnce(PreparedSupportRun<'_>) -> T,
    ) -> T {
        let prepared = prepare_support_run(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).unwrap(),
            },
            authority(delicacy, layers),
        )
        .expect("required fixture preparation");
        use_prepared(prepared)
    }

    fn assert_mixed_snapshot_projects_only_routed(
        braced: BracedSupportRun<'_>,
        expected_non_routed_kind: FinalizedDispositionKind,
    ) {
        let expected_routed_ids = braced
            .transcript()
            .rows()
            .iter()
            .filter(|row| row.kind() == FinalizedDispositionKind::Routed)
            .map(|row| row.physical_contact_id())
            .collect::<Vec<_>>();
        let non_routed_ids = braced
            .transcript()
            .rows()
            .iter()
            .filter(|row| row.kind() == expected_non_routed_kind)
            .map(|row| row.physical_contact_id())
            .collect::<Vec<_>>();
        assert!(
            !expected_routed_ids.is_empty(),
            "mixed fixture must retain at least one routed terminal"
        );
        assert!(
            !non_routed_ids.is_empty(),
            "mixed fixture must retain the requested non-routed disposition"
        );

        let lowered = braced
            .structure()
            .expect("mixed fixture structures")
            .lower()
            .expect("mixed fixture lowers");
        let snapshot = lowered
            .verification_snapshot()
            .expect("mixed fixture snapshots");
        let snapshot_routed_ids = snapshot
            .routed_contacts()
            .map(|contact| contact.physical_contact_id())
            .collect::<Vec<_>>();
        assert_eq!(snapshot_routed_ids, expected_routed_ids);

        let snapshot_routed_id_set = snapshot_routed_ids.into_iter().collect::<BTreeSet<_>>();
        let non_routed_id_set = non_routed_ids.into_iter().collect::<BTreeSet<_>>();
        assert!(
            snapshot_routed_id_set.is_disjoint(&non_routed_id_set),
            "snapshot must exclude the requested non-routed contacts"
        );
    }

    fn rejected_rewire_preflight(routed: &RoutedSupportRun<'_>) -> SupportRewireError {
        let forest_digest = routed.forest().digest();
        let geometry = routed
            .context
            .geometry()
            .expect("retained geometry rebinds");
        let loads = routed
            .state
            .plan
            .finalized_loads()
            .expect("retained plan owns finalized loads");
        let error = routed
            .context
            .validate_rewire_preflight(&routed.state, &geometry, loads)
            .expect_err("corrupt route evidence must fail before rewire mutation");
        assert_eq!(routed.forest().digest(), forest_digest);
        error
    }

    fn rejected_brace_preflight(rewired: &RewiredSupportRun<'_>) -> SupportBraceError {
        let forest_digest = rewired.forest().digest();
        let geometry = rewired
            .context
            .geometry()
            .expect("retained geometry rebinds");
        let loads = rewired
            .state
            .routed
            .plan
            .finalized_loads()
            .expect("retained plan owns finalized loads");
        let error = rewired
            .context
            .validate_brace_preflight(&rewired.state, &geometry, loads)
            .expect_err("corrupt rewire evidence must fail before brace mutation");
        assert_eq!(rewired.forest().digest(), forest_digest);
        error
    }

    fn translated_mesh(mesh: Mesh, offset: [f64; 3]) -> Mesh {
        Mesh::new(
            mesh.verts()
                .iter()
                .map(|point| std::array::from_fn(|axis| point[axis] + offset[axis]))
                .collect(),
            mesh.tris().to_vec(),
        )
        .expect("translated test mesh remains valid")
    }

    fn merge_meshes(parts: &[Mesh]) -> Mesh {
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for part in parts {
            let offset = u32::try_from(vertices.len()).expect("test mesh vertex namespace");
            vertices.extend_from_slice(part.verts());
            triangles.extend(
                part.tris()
                    .iter()
                    .map(|triangle| triangle.map(|vertex| vertex + offset)),
            );
        }
        Mesh::new(vertices, triangles).expect("merged test mesh remains valid")
    }

    fn prepared_digest_for_mesh(mesh: Mesh) -> u64 {
        let hollowed = hollowed_fixture(mesh);
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        prepare_support_run(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).unwrap(),
            },
            authority(&delicacy, &layers),
        )
        .expect("mesh preparation")
        .digests()
        .prepared()
    }

    #[test]
    fn required_named_corpus_fixture_is_loaded_from_disk() {
        let mesh = load_required_runner_corpus();
        assert!(!mesh.verts().is_empty());
        assert!(!mesh.tris().is_empty());
    }

    #[test]
    fn changed_corpus_geometry_changes_prepared_digest() {
        let corpus = load_required_runner_corpus();
        let original_digest = prepared_digest_for_mesh(corpus.clone());
        let translated_vertices = corpus
            .verts()
            .iter()
            .map(|vertex| [vertex[0] + 0.125, vertex[1], vertex[2]])
            .collect();
        let changed = Mesh::new(translated_vertices, corpus.tris().to_vec())
            .expect("translated corpus remains a valid mesh");
        let changed_digest = prepared_digest_for_mesh(changed);
        assert_ne!(original_digest, changed_digest);
    }

    #[test]
    fn prepares_required_fixture_and_retains_plan_owned_authority() {
        with_prepared_table(|prepared| {
            assert!(!prepared.plan().contacts().is_empty());
            assert_eq!(
                prepared.loads().unwrap().digest(),
                prepared.digests().loads()
            );
            assert_eq!(prepared.plan().digest(), prepared.digests().plan());
            assert_eq!(prepared.initial().plan_digest(), prepared.plan().digest());
            assert_eq!(
                prepared.initial().load_digest(),
                prepared.loads().unwrap().digest()
            );
            assert_eq!(
                prepared.geometry().unwrap().digest(),
                prepared.digests().geometry()
            );
            assert_eq!(
                prepared.initial().post_quarantine_forest_digest(),
                prepared.digests().initial_forest()
            );
            assert_eq!(prepared.frame().lift_mm(), prepared.request().lift_mm);
            assert_eq!(
                prepared.mesh().tris().len(),
                prepared
                    .hollow_history()
                    .orientation()
                    .decimation()
                    .report()
                    .output_faces()
            );
        });
    }

    #[test]
    fn routes_required_corpus_in_frozen_specialized_then_regular_cadence() {
        with_prepared_table(|prepared| {
            let expected = prepared
                .initial()
                .routing_order()
                .specialized_terminals()
                .iter()
                .chain(prepared.initial().routing_order().regular_terminals())
                .copied()
                .collect::<Vec<_>>();
            let plan_digest = prepared.digests().plan();
            let loads_digest = prepared.digests().loads();
            let geometry_digest = prepared.digests().geometry();
            let routed = prepared
                .route_initial_cco(None)
                .expect("required corpus routes through sealed CCO transaction");
            let transcript = routed.transcript();
            transcript
                .validate_integrity()
                .expect("runner retains an internally sealed transcript");
            assert_eq!(transcript.plan_digest(), plan_digest);
            assert_eq!(transcript.finalized_load_digest(), loads_digest);
            assert_eq!(transcript.geometry_digest(), geometry_digest);
            assert_eq!(transcript.rows().len(), expected.len());
            assert_eq!(
                transcript
                    .rows()
                    .iter()
                    .map(|row| row.terminal_id())
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(transcript.final_forest_digest(), routed.forest().digest());
            assert_eq!(
                transcript.final_controller_digest(),
                routed.controller().digest()
            );
        });
    }

    #[test]
    fn route_consumes_prepared_state_once_and_retains_split_authority_digests() {
        with_prepared_table(|prepared| {
            let expected_prepared = prepared.digests().prepared();
            let expected_plan = prepared.plan().digest();
            let expected_loads = prepared.loads().unwrap().digest();
            let expected_initial_forest = prepared.initial().forest().digest();

            // `route_initial_cco` takes `self`; retaining this one resulting handle is the only
            // available route transition and proves the plan/initial pair moved intact.
            let routed = prepared
                .route_initial_cco(None)
                .expect("prepared state is consumed by exactly one CCO route");

            assert_eq!(routed.digests().prepared(), expected_prepared);
            assert_eq!(routed.plan().digest(), expected_plan);
            assert_eq!(routed.initial().forest().digest(), routed.forest().digest());
            assert_eq!(
                routed.transcript().plan_digest(),
                routed.plan().digest(),
                "specialized plan moved into routed plan state"
            );
            assert_eq!(routed.transcript().finalized_load_digest(), expected_loads);
            assert_eq!(
                routed.transcript().final_forest_digest(),
                routed.forest().digest()
            );
            assert_eq!(
                routed.transcript().final_controller_digest(),
                routed.controller().digest()
            );
            assert_ne!(expected_initial_forest, routed.forest().digest());
        });
    }

    #[test]
    fn rewires_required_corpus_with_sealed_route_evidence_and_named_termination() {
        with_prepared_table(|prepared| {
            let routed = prepared
                .route_initial_cco(None)
                .expect("required corpus routes before rewire");
            let plan_digest = routed.plan().digest();
            let loads_digest = routed.transcript().finalized_load_digest();
            let geometry_digest = routed.transcript().geometry_digest();
            let transcript_digest = routed.transcript().digest();
            let rewired = routed
                .rewire()
                .expect("required corpus rewires through retained authority");

            rewired
                .forest()
                .validate()
                .expect("rewire leaves a valid final forest");
            assert_eq!(rewired.plan().digest(), plan_digest);
            assert_eq!(rewired.transcript().digest(), transcript_digest);
            assert_eq!(rewired.transcript().plan_digest(), plan_digest);
            assert_eq!(rewired.transcript().finalized_load_digest(), loads_digest);
            assert_eq!(rewired.transcript().geometry_digest(), geometry_digest);
            assert_eq!(
                rewired.post_rewire_forest_digest(),
                rewired.forest().digest(),
                "rewired state retains the exact post-rewire forest digest"
            );
            assert!(matches!(
                rewired.rewire_report().termination,
                crate::rewire::RewireTermination::FullSweepNoAcceptance
                    | crate::rewire::RewireTermination::RollingSavingsConverged
                    | crate::rewire::RewireTermination::DeterministicWorkBudget
            ));
        });
    }

    #[test]
    fn braces_required_corpus_with_retained_rewire_evidence_and_named_termination() {
        with_prepared_table(|prepared| {
            let expected_limits = prepared.context.brace_penetration_limits();
            let rewired = prepared
                .route_initial_cco(None)
                .expect("required corpus routes before rewire")
                .rewire()
                .expect("required corpus rewires before brace");
            let rewire_forest_digest = rewired.post_rewire_forest_digest();
            let expected_rewire_report_digest = rewired.rewire_report_digest();
            let transcript_digest = rewired.transcript().digest();
            let braced = rewired
                .brace()
                .expect("required corpus braces through retained authority");

            braced
                .validate_integrity()
                .expect("braced final forest and transient report remain sealed");
            assert_eq!(braced.transcript().digest(), transcript_digest);
            assert_eq!(braced.brace_penetration_limits(), expected_limits);
            assert_eq!(braced.post_rewire_forest_digest(), rewire_forest_digest);
            assert_eq!(
                rewire_report_digest(braced.rewire_report()),
                expected_rewire_report_digest
            );
            assert_eq!(braced.post_brace_forest_digest(), braced.forest().digest());
            assert_eq!(
                braced.post_brace_report_digest(),
                transient_report_digest(braced.transient_report())
            );
            assert_eq!(
                braced.brace_plan().termination,
                // **MOVED BY #51: `Unplaceable` -> `Converged`, 2026-08-21.**
                //
                // ATTRIBUTED, not assumed. Re-running this fixture with the
                // mask convergence in place but PR-7's seat suppressed still
                // reads `Converged`, and re-running it with the retired flat
                // inset pair reads `Unplaceable`, so the cause is the mask
                // (#51 Piece B) and not the seating (Piece A).
                //
                // The mechanism: with the flat rootward inset the model-anchor
                // family could not clear on any geometry, so this corpus routed
                // more of its load down plate descents, and the brace fixed
                // point ran out of placeable rungs. With the family admitted
                // the routed forest is different and the fixed point converges.
                // `Converged` is the better of the two named outcomes; the
                // corpus still does not reach the frozen third-pass
                // IterationLimit branch.
                crate::brace::BraceFixedPointTermination::Converged
            );
        });
    }

    #[test]
    fn production_brace_leaf_reaches_converged_and_unplaceable_named_outcomes() {
        let plate = Finite::new(0.0).unwrap();
        let converged = brace_fixed_point_current_parity(
            &Forest::new(),
            &remote_brace_surface(),
            &remote_brace_topology(),
            plate,
            brace_penetration_limits(),
            0.05,
            &[],
        )
        .expect("empty forest is a valid production brace input");
        assert_eq!(converged.termination, BraceFixedPointTermination::Converged);

        let mut forest = Forest::new();
        forest.add_member(brace_fixture_member(30.0, 0));
        let unplaceable = brace_fixed_point_current_parity(
            &forest,
            &remote_brace_surface(),
            &remote_brace_topology(),
            plate,
            brace_penetration_limits(),
            0.05,
            &[],
        )
        .expect("isolated long trunk remains a valid production brace input");
        assert_eq!(
            unplaceable.termination,
            BraceFixedPointTermination::Unplaceable
        );

        let mut linked = Forest::new();
        linked.add_member(brace_fixture_member(30.0, 0));
        linked.add_member(brace_fixture_member(30.0, 1));
        let linked_plan = brace_fixed_point_current_parity(
            &linked,
            &remote_brace_surface(),
            &remote_brace_topology(),
            plate,
            brace_penetration_limits(),
            0.05,
            &[],
        )
        .expect("paired long trunks remain a valid production brace input");
        assert_eq!(
            linked_plan.termination,
            BraceFixedPointTermination::Converged
        );
        assert!(!linked_plan.additions.is_empty());
    }

    #[test]
    fn stale_rewire_report_digest_fails_closed_before_brace_mutation() {
        with_prepared_table(|prepared| {
            let mut rewired = prepared
                .route_initial_cco(None)
                .expect("required corpus routes")
                .rewire()
                .expect("required corpus rewires");
            rewired.state.rewire_report_digest = rewired.state.rewire_report_digest.wrapping_add(1);
            assert!(matches!(
                rejected_brace_preflight(&rewired),
                SupportBraceError::RewireDigestMismatch
            ));
            assert!(matches!(
                stage_error(rewired.brace()),
                Err(SupportBraceError::RewireDigestMismatch)
            ));
        });
    }

    #[test]
    fn stale_brace_transient_report_digest_fails_closed_on_revalidation() {
        with_prepared_table(|prepared| {
            let mut braced = prepared
                .route_initial_cco(None)
                .expect("required corpus routes")
                .rewire()
                .expect("required corpus rewires")
                .brace()
                .expect("required corpus braces");
            braced.state.post_brace_report_digest =
                braced.state.post_brace_report_digest.wrapping_add(1);
            assert!(matches!(
                braced.validate_integrity(),
                Err(SupportBraceError::TransientReportDigestMismatch)
            ));
        });
    }

    #[test]
    fn brace_plan_digest_fails_closed_for_addition_and_iteration_corruption() {
        with_braced_table(|mut braced| {
            braced.test_corrupt_brace_plan_additions();
            assert!(matches!(
                braced.validate_integrity(),
                Err(SupportBraceError::BracePlanDigestMismatch)
            ));
        });
        with_braced_table(|mut braced| {
            braced.test_corrupt_brace_plan_iterations();
            assert!(matches!(
                braced.validate_integrity(),
                Err(SupportBraceError::BracePlanDigestMismatch)
            ));
        });
    }

    #[test]
    fn brace_plan_digest_covers_each_named_termination_variant() {
        let base = BraceFixedPointPlan {
            forest: Forest::new(),
            additions: Vec::new(),
            iterations: 1,
            termination: BraceFixedPointTermination::Converged,
            contact_refusals: Vec::new(),
            contact_attestation_digest: 0,
            final_pass_refusals: crate::brace::FinalPassRefusalAccounting::default(),
            seat_zone_members_refrozen: 0,
        };
        let digest = brace_fixed_point_plan_digest(&base);

        // The refusal accounting is DISCLOSURE, and deliberately outside this
        // digest: every row it carries is a pure function of the forest and the
        // surface the digest already covers, and this lane is authorized to
        // move exactly one sealed fingerprint — the CCO transcript's. Pinned
        // here so the exclusion reads as a decision rather than an oversight.
        let mut disclosed = base.clone();
        disclosed.final_pass_refusals = crate::brace::FinalPassRefusalAccounting {
            refused_arms: 3,
            pipeline_violations: 1,
            rows: Vec::new(),
        };
        assert_eq!(
            brace_fixed_point_plan_digest(&disclosed),
            digest,
            "refusal accounting must not move the sealed brace-plan digest"
        );
        // C5/C6, the same decision for the same reason. The re-size count is
        // disclosure the controller now carries instead of discarding; folding
        // it would move every sealed brace-plan digest in the corpus, and this
        // lane may move exactly one fingerprint. Pinned so the exclusion reads
        // as a decision rather than an oversight.
        let mut counted = base.clone();
        counted.seat_zone_members_refrozen = 7;
        assert_eq!(
            brace_fixed_point_plan_digest(&counted),
            digest,
            "the seat-zone re-size count must not move the sealed brace-plan \
             digest either"
        );
        for termination in [
            BraceFixedPointTermination::Converged,
            BraceFixedPointTermination::IterationLimit,
            BraceFixedPointTermination::Unplaceable,
        ] {
            let mut plan = base.clone();
            plan.termination = termination;
            if termination == BraceFixedPointTermination::Converged {
                assert_eq!(brace_fixed_point_plan_digest(&plan), digest);
            } else {
                assert_ne!(brace_fixed_point_plan_digest(&plan), digest);
            }
        }
        for (seeded, corrupted) in [
            (
                BraceFixedPointTermination::IterationLimit,
                BraceFixedPointTermination::Converged,
            ),
            (
                BraceFixedPointTermination::Converged,
                BraceFixedPointTermination::IterationLimit,
            ),
            (
                BraceFixedPointTermination::Converged,
                BraceFixedPointTermination::Unplaceable,
            ),
        ] {
            with_braced_table(|mut braced| {
                // Seed a complete, internally coherent test-only plan record
                // for the alternate variant, then prove changing to each
                // named outcome is detected by the sealed plan digest.
                braced.state.plan.termination = seeded;
                braced.state.brace_plan_digest = brace_fixed_point_plan_digest(&braced.state.plan);
                braced.test_corrupt_brace_plan_termination(corrupted);
                assert!(matches!(
                    braced.validate_integrity(),
                    Err(SupportBraceError::BracePlanDigestMismatch)
                ));
            });
        }
    }

    /// FU-32 J5 (design D3, M1). The treant's stated demand, pinned: the
    /// charge is the cadence-by-pool product on BOTH resources, and the two
    /// registered caps admit it. This is the number the registry notes quote,
    /// held against the registry's own values so a cap edit and a note can
    /// never drift apart silently.
    #[test]
    fn fu32_j5_routing_charge_prices_the_measured_treant_shape_under_the_registered_caps() {
        // FU-32 design M1, measured at bfa5b3b: cadence 553,
        // pool_rows 364,223 (single specimen, n = 1, per R4/FU-23).
        let charge = RouteCcoWorkCharge::compute(553, 364_223).expect("treant shape is finite");
        assert_eq!(charge.seat_evaluations(), 201_415_319);
        assert_eq!(charge.seat_refusal_rows(), 201_415_319);

        let registered = RouteCcoWorkLimits::registered().expect("registered routing caps");
        assert_eq!(
            registered,
            RouteCcoWorkLimits::for_test(536_870_912, 536_870_912),
            "the registered routing caps are next_power_of_two(2 x 201,415,319) on both resources"
        );
        assert!(charge.preflight(registered).is_ok());
    }

    /// The seat-search arm alone. Deleting the evaluation check must fail
    /// THIS test and not its sibling, so the two arms are proven independent
    /// rather than one of them being carried by the other.
    #[test]
    fn fu32_j5_routing_refuses_an_over_demand_seat_search_on_the_required_corpus() {
        with_prepared_table(|prepared| {
            let refused =
                stage_error(prepared.route_initial_cco_with_limits(
                    RouteCcoWorkLimits::for_test(1, usize::MAX),
                    None,
                ));
            match refused {
                Err(SupportRoutingError::Limit {
                    resource: "seat evaluations",
                    required,
                    limit: 1,
                }) => assert!(
                    required > 1,
                    "the required corpus must demand a real seat search"
                ),
                Err(other) => panic!("expected a seat-evaluation limit refusal, got {other:?}"),
                Ok(_) => panic!("an over-demand seat search must refuse"),
            }
        });
    }

    /// The retained-evidence arm alone, with the seat-search cap wide open.
    #[test]
    fn fu32_j5_routing_refuses_an_over_demand_refusal_evidence_budget_on_the_required_corpus() {
        with_prepared_table(|prepared| {
            let refused =
                stage_error(prepared.route_initial_cco_with_limits(
                    RouteCcoWorkLimits::for_test(usize::MAX, 1),
                    None,
                ));
            match refused {
                Err(SupportRoutingError::Limit {
                    resource: "seat refusal rows",
                    required,
                    limit: 1,
                }) => assert!(
                    required > 1,
                    "the required corpus must charge real refusal evidence"
                ),
                Err(other) => panic!("expected a refusal-row limit refusal, got {other:?}"),
                Ok(_) => panic!("an over-demand refusal budget must refuse"),
            }
        });
    }

    /// FU-32 J5 — the meter is really fed by the walk, and the receipt really
    /// holds on real geometry: the required corpus routes, spends a nonzero
    /// number of seat evaluations, and lands at or under the charge on both
    /// resources. A meter left unwired reads zero and fails here.
    #[test]
    fn fu32_j5_the_routing_meter_counts_the_real_pool_walk_and_stays_under_its_charge() {
        // The charge this instance is priced at, recovered from the refusing
        // arm above rather than recomputed from a formula this test owns.
        let charged =
            with_prepared_table(|prepared| {
                match stage_error(prepared.route_initial_cco_with_limits(
                    RouteCcoWorkLimits::for_test(1, usize::MAX),
                    None,
                )) {
                    Err(SupportRoutingError::Limit { required, .. }) => required,
                    Err(other) => panic!("expected a limit refusal, got {other:?}"),
                    Ok(_) => panic!("an over-demand seat search must refuse"),
                }
            });

        with_prepared_table(|prepared| {
            let routed = prepared
                .route_initial_cco(None)
                .expect("required corpus routes");
            let meter = routed.state.controller.route_work_meter();
            assert!(
                meter.seat_evaluations > 0,
                "the routing cadence must meter the seat search it executed"
            );
            assert!(meter.seat_evaluations <= charged);
            assert!(meter.seat_refusal_rows <= charged);
            assert!(
                meter.seat_refusal_rows <= meter.seat_evaluations,
                "a sample can refuse at most once per terminal"
            );
        });
    }

    /// FU-32 J5 — the receipt's own arms, both of them: an execution that
    /// outruns its charge refuses with the resource that outran it. Unreachable
    /// in production by construction, which is exactly why it is tested here.
    #[test]
    fn fu32_j5_the_routing_receipt_refuses_execution_that_outruns_its_charge() {
        let charge = RouteCcoWorkCharge::compute(4, 10).expect("small shape is finite");
        assert!(charge
            .covers_execution(RouteCcoWorkMeter {
                seat_evaluations: 40,
                seat_refusal_rows: 40,
            })
            .is_ok());
        assert!(matches!(
            charge.covers_execution(RouteCcoWorkMeter {
                seat_evaluations: 41,
                seat_refusal_rows: 0,
            }),
            Err(SupportRoutingError::ChargeDrift {
                resource: "seat evaluations",
                charged: 40,
                executed: 41,
            })
        ));
        assert!(matches!(
            charge.covers_execution(RouteCcoWorkMeter {
                seat_evaluations: 0,
                seat_refusal_rows: 41,
            }),
            Err(SupportRoutingError::ChargeDrift {
                resource: "seat refusal rows",
                charged: 40,
                executed: 41,
            })
        ));
    }

    /// FU-32 J5 — an instance whose cadence-by-pool product is not
    /// representable refuses as an overflow rather than wrapping to a small,
    /// admissible charge.
    #[test]
    fn fu32_j5_an_unrepresentable_routing_product_refuses_instead_of_wrapping() {
        assert!(matches!(
            RouteCcoWorkCharge::compute(usize::MAX, 2),
            Err(SupportRoutingError::ResourceOverflow("seat evaluations"))
        ));
    }

    /// **THE OLD FORM OF THIS TEST WENT VACUOUS WITH THE TWO-TIER RULING, AND
    /// THAT IS MEASURED.**
    ///
    /// It capped brace penetration ROWS at one and asserted the stage refused,
    /// because the required corpus used to reach the brace interval batch with
    /// more than one admitted row. It no longer reaches it with ANY row.
    /// `brace::axis_illegal_rows` admits a row only when its clearance sits in
    /// `[0, 0.1)` or its midpoint is inside the model, and that window is
    /// expressed in CLEARANCE, so it moves when the demand does: at
    /// `STANDOFF_MM` it meant "true gap in [0.600, 0.700)", and at
    /// `FINAL_CLEARANCE_GAP_MM` it means "true gap in [0.300, 0.400)". This
    /// corpus has occupants of the first band and none of the second. Capping
    /// rows at one now SUCCEEDS, capping them at zero is refused as an invalid
    /// caller authority, and capping samples at one also succeeds — the batch
    /// is simply never called.
    ///
    /// A test that can go quiet rather than red is worse than no test, so the
    /// two things it used to conflate are now separated and neither can go
    /// vacuous:
    ///
    /// * the CAP itself is pinned in `brace.rs` by
    ///   `a_capped_penetration_batch_refuses_rather_than_skipping_the_transit_screen`,
    ///   on a fixture that CONSTRUCTS an admitted row instead of hoping for
    ///   one; and
    /// * the runner BOUNDARY — that the caller's own authority is what the
    ///   brace stage spends — is this test, which reads the value back out at
    ///   the far side of a successful stage.
    #[test]
    fn the_callers_brace_penetration_authority_is_the_one_the_brace_stage_spends() {
        let hollowed = hollowed_fixture(load_required_runner_corpus());
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        let mut authored = authority(&delicacy, &layers);
        // Distinctive, generous, and unmistakably the CALLER's: no default and
        // no derived bound could produce these by coincidence.
        let expected = PenetrationBatchLimits {
            max_rows: 7_777_777,
            max_total_work: 7_777_778,
            max_total_hits: 7_777_779,
            max_total_samples: 7_777_780,
            max_total_intervals: 7_777_781,
            max_scratch_bytes: 7_777_782,
        };
        authored.brace_penetration_limits = expected;
        let braced = prepare_support_run(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).unwrap(),
            },
            authored,
        )
        .expect("a generous brace authority is a valid caller authority")
        .route_initial_cco(None)
        .expect("required corpus routes")
        .rewire()
        .expect("required corpus rewires")
        .brace()
        .expect("required corpus braces under a generous authority");
        assert_eq!(
            braced.brace_penetration_limits(),
            expected,
            "the brace stage must spend the CALLER's penetration authority, \
             not a default reconstructed inside the runner"
        );
    }

    #[test]
    fn stale_routed_digest_fails_closed_before_rewire_mutation() {
        with_prepared_table(|prepared| {
            let mut routed = prepared
                .route_initial_cco(None)
                .expect("required corpus routes before stale authority check");
            routed.context.digests.plan = routed.context.digests.plan.wrapping_add(1);
            assert!(matches!(
                stage_error(routed.rewire()),
                Err(SupportRewireError::PlanOrLoadDigestMismatch)
            ));
        });
    }

    #[test]
    fn stale_initial_forest_digest_fails_closed_before_rewire_mutation() {
        with_prepared_table(|prepared| {
            let mut routed = prepared
                .route_initial_cco(None)
                .expect("required corpus routes");
            routed.state.initial.test_corrupt_initial_forest_digest();
            assert!(matches!(
                rejected_rewire_preflight(&routed),
                SupportRewireError::InitialAuthorityMismatch
            ));
            assert!(matches!(
                stage_error(routed.rewire()),
                Err(SupportRewireError::InitialAuthorityMismatch)
            ));
        });
    }

    /// PR-8. The attestation transcript is checked independently of every
    /// forest digest, because a regression in which the attestation stops
    /// happening moves NO geometry and every forest digest would stay put.
    #[test]
    fn stale_contact_attestation_digest_fails_closed_before_rewire_mutation() {
        with_prepared_table(|prepared| {
            let mut routed = prepared
                .route_initial_cco(None)
                .expect("required corpus routes");
            // Non-vacuity: the forest digests are untouched by this seam, so
            // the rejection below can only have come from the attestation
            // clause.
            let before = routed.state.initial.post_quarantine_forest_digest();
            routed
                .state
                .initial
                .test_corrupt_contact_attestation_digest();
            assert_eq!(
                routed.state.initial.post_quarantine_forest_digest(),
                before,
                "the seam must move no geometry"
            );
            assert!(matches!(
                rejected_rewire_preflight(&routed),
                SupportRewireError::InitialAuthorityMismatch
            ));
            assert!(matches!(
                stage_error(routed.rewire()),
                Err(SupportRewireError::InitialAuthorityMismatch)
            ));
        });
    }

    #[test]
    fn stale_post_quarantine_digest_fails_closed_before_rewire_mutation() {
        with_prepared_table(|prepared| {
            let mut routed = prepared
                .route_initial_cco(None)
                .expect("required corpus routes");
            routed
                .state
                .initial
                .test_corrupt_post_quarantine_forest_digest();
            assert!(matches!(
                rejected_rewire_preflight(&routed),
                SupportRewireError::InitialAuthorityMismatch
            ));
            assert!(matches!(
                stage_error(routed.rewire()),
                Err(SupportRewireError::InitialAuthorityMismatch)
            ));
        });
    }

    #[test]
    fn missing_initial_registration_evidence_fails_closed_before_rewire_mutation() {
        with_prepared_table(|prepared| {
            let mut routed = prepared
                .route_initial_cco(None)
                .expect("required corpus routes");
            routed
                .state
                .initial
                .test_clear_terminal_registration_evidence();
            assert!(matches!(
                rejected_rewire_preflight(&routed),
                SupportRewireError::InitialAuthorityMismatch
            ));
            assert!(matches!(
                stage_error(routed.rewire()),
                Err(SupportRewireError::InitialAuthorityMismatch)
            ));
        });
    }

    #[test]
    fn transcript_initial_forest_binding_fails_closed_before_rewire_mutation() {
        with_prepared_table(|prepared| {
            let mut routed = prepared
                .route_initial_cco(None)
                .expect("required corpus routes");
            routed.state.transcript.test_rebind_initial_forest_digest();
            assert!(matches!(
                rejected_rewire_preflight(&routed),
                SupportRewireError::TranscriptDigestMismatch
            ));
            assert!(matches!(
                stage_error(routed.rewire()),
                Err(SupportRewireError::TranscriptDigestMismatch)
            ));
        });
    }

    #[test]
    fn transcript_post_quarantine_binding_fails_closed_before_rewire_mutation() {
        with_prepared_table(|prepared| {
            let mut routed = prepared
                .route_initial_cco(None)
                .expect("required corpus routes");
            routed
                .state
                .transcript
                .test_rebind_initial_post_quarantine_forest_digest();
            assert!(matches!(
                rejected_rewire_preflight(&routed),
                SupportRewireError::TranscriptDigestMismatch
            ));
            assert!(matches!(
                stage_error(routed.rewire()),
                Err(SupportRewireError::TranscriptDigestMismatch)
            ));
        });
    }

    /// SPEC-A4 §A.3, and it is the property task #47 dies for today: a stage
    /// that REFUSES must hand the context back, and the context it hands back
    /// must still be a usable authority for the next trial.
    ///
    /// The refusal used here is the registered routing work limit driven to 1,
    /// so the stage does real work and fails deep inside the cadence rather
    /// than at a preflight. Before this seam the context was consumed by the
    /// failing stage and the whole model died with it.
    #[test]
    fn a_refused_stage_returns_the_context_and_the_next_trial_can_still_use_it() {
        with_prepared_table(|prepared| {
            let plan = prepared.plan.clone();
            let (context, error) = prepared
                .route_initial_cco_with_limits(RouteCcoWorkLimits::for_test(1, usize::MAX), None)
                .err()
                .expect("an over-demand seat search must refuse");
            assert!(
                matches!(error, SupportRoutingError::Limit { .. }),
                "expected the seat-evaluation limit refusal, got {error:?}"
            );
            // The recovered context is not a corpse: it prepares and routes
            // the very same plan at the registered limits.
            let prepared_again = stage_error(prepared_run_for_plan(context, plan))
                .expect("the recovered context prepares the next trial");
            stage_error(prepared_again.route_initial_cco(None))
                .expect("the recovered context routes at the registered limits");
        });
    }

    /// SPEC-A4 §A.5's rollback obligation: a rejected trial may not leave its
    /// own digests visible on the context.
    ///
    /// `prepared_run_for_plan` stamps five plan-dependent digest fields. A
    /// trial rejected at Routing, Rewire, Brace, Structure or Lowering hands
    /// back a context stamped with the REJECTED CHILD's values, and under
    /// one-winner transactions the controller reads evidence between trials —
    /// so a stale stamp would describe a plan that does not exist. This drives
    /// the whole cycle on real geometry: accepted plan, child plan with one
    /// appended repair row, refusal, restore.
    #[test]
    fn a_rejected_trial_does_not_leave_its_digests_on_the_context() {
        with_prepared_table(|prepared| {
            let accepted_plan = prepared.plan.clone();
            let accepted_digests = prepared.context.prepared_digests();
            let context = prepared.route_initial_cco(None).map_or_else(
                |(context, _error)| context,
                |routed| {
                    routed.rewire().map_or_else(
                        |(context, _error)| context,
                        |rewired| {
                            rewired.brace().map_or_else(
                                |(context, _error)| context,
                                |braced| {
                                    braced.structure().map_or_else(
                                        |(context, _error)| context,
                                        |structured| {
                                            structured.lower().map_or_else(
                                                |(context, _error)| context,
                                                LoweredSupportRun::into_context,
                                            )
                                        },
                                    )
                                },
                            )
                        },
                    )
                },
            );
            assert_eq!(
                context.prepared_digests(),
                accepted_digests,
                "a run that never re-prepared must still carry the accepted stamp"
            );

            // Now a real child: one repair row appended, which is exactly what
            // a trial is.
            let occupied: std::collections::BTreeSet<_> = accepted_plan
                .occupied_repair_rows()
                .expect("the fixture plan reports its occupied repair rows")
                .into_iter()
                .map(|occupancy| occupancy.row_id())
                .collect();
            let row = accepted_plan
                .all_repair_rows()
                .iter()
                .map(crate::specialized::RepairRow::id)
                .find(|id| !occupied.contains(id))
                .expect("the required corpus fixture has an unoccupied repair row");
            let child = accepted_plan
                .begin_regular_edit()
                .and_then(|edit| edit.append_regular_repair_rows(&[row]))
                .expect("appending one repair row derives a child plan");
            let trial = stage_error(prepared_run_for_plan(context, child))
                .expect("the child plan prepares");
            let trial_digests = trial.context.prepared_digests();
            assert_ne!(
                trial_digests, accepted_digests,
                "a child plan with an extra contact must stamp different digests, \
                 or this test proves nothing"
            );

            // Reject the trial the way the controller will, and restore.
            let mut context = trial.route_initial_cco(None).map_or_else(
                |(context, _error)| context,
                |routed| routed.into_context_for_test(),
            );
            assert_eq!(
                context.prepared_digests(),
                trial_digests,
                "before the restore the context carries the REJECTED child's stamp — \
                 that is the defect §A.5 exists to close"
            );
            context.restore_prepared_digests(accepted_digests);
            assert_eq!(
                context.prepared_digests(),
                accepted_digests,
                "after the restore the context is the accepted state's again"
            );
        });
    }

    #[test]
    fn stale_initial_authority_is_refused_before_cco_routing() {
        with_prepared_table(|mut prepared| {
            prepared.initial.test_corrupt_initial_forest_digest();
            assert!(matches!(
                stage_error(prepared.route_initial_cco(None)),
                Err(SupportRoutingError::InitialAuthorityMismatch)
            ));
        });
    }

    /// SPEC-A3-A4 §6, task #49 — **the split landing's first half.** A model
    /// half buried under a blocker has every one of that half's terminals
    /// quarantined, so after the §6 removal the buried islands have no contact
    /// left at all and the carried-load repartition has faces no owner can
    /// carry.
    ///
    /// **That is not an error, and this test is what pins it.** The routing
    /// stage completes, the accepted contact set is built, and the faces the
    /// repartition cannot place are DISCLOSED — each with the area the load
    /// eligibility gave it — instead of taking the model out. Refusing here
    /// would decide a support-adequacy question from a set-membership tally:
    /// eligibility is provenance, not reach, so "every eligible owner lost its
    /// contacts" is not a measurement that these faces are unsupported. The
    /// measurement that rules on them is the routed-coverage judgment §6
    /// sequences AFTER this removal.
    ///
    /// **Revert-proof.** At `9a9b459` this same fixture returned
    /// `Err(SupportRoutingError::UnownedFaceAfterUnroutedRemoval { .. })`, so
    /// the `expect` below fails on the pre-change tree; and the disclosure
    /// assertion fails on any tree that completes routing without recording
    /// what it could not place.
    ///
    /// The measured emission cost of the refusal — three corpus models — is
    /// recorded at `run_support`'s Routing arm.
    #[test]
    fn a_fully_buried_half_discloses_its_ownerless_faces_instead_of_refusing() {
        let buried = make_overhang_table().expect("buried table fixture");
        let blocker =
            make_box([30.0, 30.0, 1.0], [0.0, 0.0, 16.56]).expect("burial blocker fixture");
        let live = translated_mesh(
            make_overhang_table().expect("live table fixture"),
            [100.0, 0.0, 0.0],
        );
        let hollowed = hollowed_fixture(merge_meshes(&[buried, blocker, live]));
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        prepare_fixture(hollowed, &delicacy, &layers, |prepared| {
            let routed = prepared
                .route_initial_cco(None)
                .expect("a buried half no longer takes the routing stage out");
            let ownerless = routed.ownerless_faces();
            assert!(
                !ownerless.is_empty(),
                "the buried half's islands lost every contact, so the repartition had faces \
                 it could not place; they must be disclosed, not silently dropped"
            );
            // The disclosure carries a real measurement, not a bare count: each
            // face keeps the eligibility's own area, which is the quantity a
            // reader needs to weigh it against the coverage verdict.
            assert!(
                ownerless
                    .iter()
                    .all(|face| face.area_mm2.is_finite() && face.area_mm2 > 0.0),
                "every disclosed face must carry its own finite, positive area: {ownerless:?}"
            );
            // Nothing is double-counted: a face is charged to a contact or to
            // nobody, never both.
            let charged = routed
                .accepted_plan()
                .load_partition()
                .rows()
                .iter()
                .map(|row| row.face_id)
                .collect::<BTreeSet<_>>();
            assert!(
                ownerless
                    .iter()
                    .all(|face| !charged.contains(&face.face_id)),
                "a disclosed face must appear in no partition row"
            );
        });
    }

    /// Task #49 — **the run report's disclosure is BUILT, on real run data,
    /// and it accounts for every ownerless face.**
    ///
    /// `ownerless_island_disclosures` is the only producer of the list the
    /// chain carries out to the run report and the CLI prints. Before this
    /// test, no test anywhere asserted a NON-EMPTY disclosure: forcing the
    /// function to `return Vec::new()` left the whole suite green, because the
    /// one end-to-end fixture that reaches it takes the coverage-refusal arm
    /// and never looks at the list. That made the ruling's actual deliverable
    /// — "the run report must DISCLOSE ownerless islands so nothing is hidden"
    /// — a behaviour with no guard at all.
    ///
    /// So this drives the REAL fixture to a REAL `LoweredSupportRun` and calls
    /// the production join on it. Sabotage measured, not asserted: with the
    /// body replaced by `Vec::new()`, this fails on the first assertion.
    ///
    /// **Why the fixture stops at `lower` instead of sealing.** A sealing run
    /// with a non-empty disclosure is not constructible from a procedural
    /// fixture, and the reason is a measurement, not an inconvenience. The
    /// coverage measurement matches a sample only against routed contacts of
    /// its own `(island_id, component_id)` route (`specialized.rs`,
    /// `fill_route_points_nearest`), so an island that loses EVERY owner
    /// measures fully uncovered — its whole area is blocking, and the run
    /// refuses at the coverage judgment rather than sealing. Only an island
    /// whose ownerless area is dust below the registered censor seals, and on
    /// the four corpus models that dust is 0.0005–0.05 mm², an area no
    /// procedural mesh feature survives decimation at. The corpus models are
    /// the fixture for the sealing case; this is the fixture for the join.
    #[test]
    fn the_ownerless_disclosure_is_built_from_the_run_and_accounts_for_every_face() {
        let buried = make_overhang_table().expect("buried table fixture");
        let blocker =
            make_box([30.0, 30.0, 1.0], [0.0, 0.0, 16.56]).expect("burial blocker fixture");
        let live = translated_mesh(
            make_overhang_table().expect("live table fixture"),
            [100.0, 0.0, 0.0],
        );
        let hollowed = hollowed_fixture(merge_meshes(&[buried, blocker, live]));
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        prepare_fixture(hollowed, &delicacy, &layers, |prepared| {
            let lowered = prepared
                .route_initial_cco(None)
                .expect("a buried half no longer takes the routing stage out")
                .rewire()
                .expect("fixture rewires")
                .brace()
                .expect("fixture braces")
                .structure()
                .expect("fixture structures")
                .lower()
                .expect("fixture lowers");
            let ownerless = lowered
                .structured
                .braced
                .rewired
                .routed
                .accepted
                .ownerless_faces
                .clone();
            assert!(
                !ownerless.is_empty(),
                "the fixture must actually leave faces ownerless for this to mean anything"
            );

            // No measurement supplied: the disclosure still has to be made,
            // because the provenance half of it does not depend on coverage.
            let disclosed = ownerless_island_disclosures(&lowered, &[]);
            assert!(
                !disclosed.is_empty(),
                "the run charges {} face(s) to nobody and disclosed none of them",
                ownerless.len()
            );
            // Every ownerless face lands in exactly one row, and its area
            // lands with it. This is the "nothing is hidden" claim as
            // arithmetic: counts and areas must reconcile to the bit.
            let disclosed_faces: usize =
                disclosed.iter().map(|row| row.ownerless_face_count()).sum();
            assert_eq!(
                disclosed_faces,
                ownerless.len(),
                "every ownerless face must appear in exactly one disclosure row"
            );
            let disclosed_area: f64 = disclosed
                .iter()
                .map(|row| row.ownerless_face_area_mm2())
                .sum();
            let true_area: f64 = ownerless.iter().map(|face| face.area_mm2).sum();
            assert!(
                (disclosed_area - true_area).abs() < 1e-9,
                "the disclosed area must be the faces' own area: {disclosed_area} vs {true_area}"
            );
            // A row that names an island really does name the island those
            // faces belong to — the join is not attributing area to whatever
            // island sorts first.
            let plan = lowered
                .structured
                .braced
                .rewired
                .routed
                .accepted
                .plan(&lowered.structured.braced.rewired.routed.plan);
            let ownerless_ids = ownerless
                .iter()
                .map(|face| face.face_id)
                .collect::<BTreeSet<_>>();
            for row in &disclosed {
                let Some(island_id) = row.island_id() else {
                    // No island, so no coverage row exists to report. `None`,
                    // never a zero that would read as a clean measurement.
                    assert_eq!(
                        row.measured_uncovered_area_mm2(),
                        None,
                        "an islandless row has no measurement to state: {row:?}"
                    );
                    continue;
                };
                let island = plan
                    .islands()
                    .iter()
                    .find(|island| island.id == island_id)
                    .expect("a disclosed island id is one of the plan's islands");
                let counted = island
                    .face_ids
                    .iter()
                    .filter(|face_id| ownerless_ids.contains(face_id))
                    .count();
                assert_eq!(
                    counted,
                    row.ownerless_face_count(),
                    "island {island_id}'s row must count that island's own ownerless faces"
                );
                assert_eq!(
                    row.measured_uncovered_area_mm2(),
                    Some(0.0),
                    "with no patches supplied the measurement found nothing, which is a \
                     different claim from having taken no measurement: {row:?}"
                );
            }

            // And the measured half is joined from the attempt's own patches,
            // not invented: hand the join one uncensored patch on a disclosed
            // island and the row must carry exactly that area.
            let target_island = disclosed
                .iter()
                .find_map(OwnerlessIslandDisclosure::island_id)
                .expect("the fixture's ownerless faces belong to a named island");
            let patch = crate::coverage_closure::BlockingPatchKey {
                island_id: target_island,
                component_id: 0,
                representative_face_id: 0,
                sorted_face_ids: Vec::new(),
                uncovered_area_mm2_bits: 0.004_024_703_349_f64.to_bits(),
                worst_gap: crate::coverage_closure::CoverageClosureGap::NoRoutedContact,
            };
            let joined = ownerless_island_disclosures(&lowered, std::slice::from_ref(&patch));
            let row = joined
                .iter()
                .find(|row| row.island_id() == Some(target_island))
                .expect("the same island is disclosed either way");
            assert_eq!(
                row.measured_uncovered_area_mm2(),
                Some(0.004_024_703_349),
                "the measured half must be the attempt's own uncovered area, to the bit"
            );
        });
    }

    /// Task #49 — the disclosure join itself, on constructed inputs, so every
    /// rule it applies is pinned without needing a run that exhibits all of
    /// them at once: grouping by island, the islandless row, the summed
    /// measurement, and `None` versus `Some(0.0)`.
    #[test]
    fn the_disclosure_join_groups_by_island_and_states_what_it_did_not_measure() {
        let ownerless = vec![
            crate::perception::OwnerlessLoadFace {
                face_id: 7,
                area_mm2: 0.25,
            },
            crate::perception::OwnerlessLoadFace {
                face_id: 9,
                area_mm2: 0.5,
            },
            // Belongs to no island at all.
            crate::perception::OwnerlessLoadFace {
                face_id: 99,
                area_mm2: 1.5,
            },
            crate::perception::OwnerlessLoadFace {
                face_id: 11,
                area_mm2: 2.0,
            },
        ];
        let islands = vec![
            crate::SupportIsland {
                id: 3,
                face_ids: vec![7, 9, 12],
            },
            crate::SupportIsland {
                id: 4,
                face_ids: vec![11],
            },
        ];
        let patch =
            |island_id, component_id, area: f64| crate::coverage_closure::BlockingPatchKey {
                island_id,
                component_id,
                representative_face_id: 0,
                sorted_face_ids: Vec::new(),
                uncovered_area_mm2_bits: area.to_bits(),
                worst_gap: crate::coverage_closure::CoverageClosureGap::NoRoutedContact,
            };
        // Island 3 is measured over two components; island 4 has no patch at
        // all, which is a measurement that found nothing.
        let targets = vec![patch(3, 0, 0.125), patch(3, 1, 0.375)];
        let rows = join_ownerless_island_disclosures(&ownerless, &islands, &targets);

        assert_eq!(rows.len(), 3, "one row per island, plus the islandless row");
        let row = |island_id| {
            *rows
                .iter()
                .find(|row| row.island_id() == island_id)
                .unwrap_or_else(|| panic!("a row for {island_id:?}"))
        };

        let three = row(Some(3));
        assert_eq!(three.ownerless_face_count(), 2);
        assert!((three.ownerless_face_area_mm2() - 0.75).abs() < 1e-12);
        // Both components' uncovered area, summed. Over-attributing across
        // components is the known imprecision recorded on the join; it can
        // only ever report MORE uncovered area, never less.
        assert_eq!(three.measured_uncovered_area_mm2(), Some(0.5));

        let four = row(Some(4));
        assert_eq!(four.ownerless_face_count(), 1);
        assert!((four.ownerless_face_area_mm2() - 2.0).abs() < 1e-12);
        assert_eq!(
            four.measured_uncovered_area_mm2(),
            Some(0.0),
            "an island the measurement covered and found clean says so with a zero"
        );

        let islandless = row(None);
        assert_eq!(islandless.ownerless_face_count(), 1);
        assert!((islandless.ownerless_face_area_mm2() - 1.5).abs() < 1e-12);
        assert_eq!(
            islandless.measured_uncovered_area_mm2(),
            None,
            "no island means no coverage row; a zero here would claim a measurement \
             that was never taken"
        );

        assert!(
            join_ownerless_island_disclosures(&[], &islands, &targets).is_empty(),
            "no ownerless face means nothing to disclose"
        );
    }

    /// SPEC-A3-A4 §6, task #49 — the removal clause, end to end on a fixture
    /// that keeps both unrouted dispositions and still seals.
    ///
    /// Every assertion here is on the MECHANISM, not on a value that happens
    /// to hold today: the ledger is exactly the transcript's unrouted regular
    /// rows with their reasons; the accepted contact set is the routed plan
    /// minus exactly those ids; no removed id survives; and the carried area
    /// they held is repartitioned onto the survivors rather than dropped.
    #[test]
    fn unrouted_regular_contacts_leave_the_accepted_contact_set_with_their_load_repartitioned() {
        let table = load_required_runner_corpus();
        let boundary_blocker =
            make_box([30.0, 30.0, 1.0], [0.0, 0.0, 16.55]).expect("no-winner blocker fixture");
        let hollowed = hollowed_fixture(merge_meshes(&[table, boundary_blocker]));
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        prepare_fixture(hollowed, &delicacy, &layers, |prepared| {
            let routed = prepared
                .route_initial_cco(None)
                .expect("no-winner fixture seals through CCO");

            let expected: Vec<(u64, UnroutedContactReason)> = routed
                .transcript()
                .rows()
                .iter()
                .filter_map(|row| match row.kind() {
                    FinalizedDispositionKind::Routed => None,
                    FinalizedDispositionKind::SkippedQuarantined => Some((
                        row.physical_contact_id(),
                        UnroutedContactReason::Quarantined,
                    )),
                    FinalizedDispositionKind::UnroutedNoWinner => {
                        Some((row.physical_contact_id(), UnroutedContactReason::NoWinner))
                    }
                })
                .filter(|(contact_id, _)| {
                    routed
                        .plan()
                        .contacts()
                        .iter()
                        .find(|contact| contact.contact_id() == *contact_id)
                        .is_some_and(|contact| contact.demands().is_empty())
                })
                .collect();
            assert!(
                expected
                    .iter()
                    .any(|(_, reason)| *reason == UnroutedContactReason::NoWinner),
                "fixture must retain a real no-winner regular terminal"
            );

            let ledgered = routed
                .unrouted_regular_removals()
                .iter()
                .map(|removal| (removal.contact_id(), removal.reason()))
                .collect::<Vec<_>>();
            assert_eq!(ledgered, expected);

            // Every ledgered row names the exact candidate-pool row the
            // removed contact occupied, as §6's first sentence requires.
            for removal in routed.unrouted_regular_removals() {
                let contact = routed
                    .plan()
                    .contacts()
                    .iter()
                    .find(|contact| contact.contact_id() == removal.contact_id())
                    .expect("ledgered contact is in the routed plan");
                assert_eq!(
                    Some(removal.route()),
                    contact.primary_regular().map(|primary| primary.route())
                );
            }

            let removed_ids = routed
                .unrouted_regular_removals()
                .iter()
                .map(UnroutedRegularRemoval::contact_id)
                .collect::<BTreeSet<_>>();
            let accepted = routed.accepted_plan();
            assert_eq!(
                accepted.contacts().len() + removed_ids.len(),
                routed.plan().contacts().len()
            );
            assert!(accepted
                .contacts()
                .iter()
                .all(|contact| !removed_ids.contains(&contact.contact_id())));
            assert_ne!(accepted.digest(), routed.plan().digest());

            // The load is repartitioned, not discarded: the removed contacts
            // were holding real model area and the survivors now hold it.
            let routed_total = carried_area_total(routed.plan());
            let accepted_total = carried_area_total(accepted);
            let removed_area = routed
                .unrouted_regular_removals()
                .iter()
                .map(UnroutedRegularRemoval::carried_area_mm2)
                .sum::<f64>();
            assert!(
                removed_area > 0.0,
                "the fixture's unrouted contacts were charged real model area"
            );
            assert!(
                (routed_total - accepted_total).abs() < 1e-9,
                "carried area is repartitioned over the survivors, not dropped: \
                 {routed_total} vs {accepted_total}"
            );
        });
    }

    /// SPEC-A3-A4 §6, task #49 — **the strip moves the sizing INPUT in the
    /// safe direction. Nothing in production reads that input yet.**
    ///
    /// This is the load-safety half of the ruling, and it is the reason the
    /// removal is worth having on its own merits, independently of §6's text.
    ///
    /// **The defect it pins.** Before the removal was wired, the carried-load
    /// repartition charged model area to contacts the cadence never routed —
    /// contacts with no arm under them at all. On the procedural toed slab,
    /// measured 2026-08-19, that was **93% of the whole load area**. Every
    /// surviving arm was therefore sized against a share of the model it was
    /// not the one actually holding: the phantom credit made the real
    /// carriers look LIGHTLY loaded, and an arm sized for less load than it
    /// carries is the unsafe direction of the error.
    ///
    /// **The invariant, and why it is an invariant rather than a number that
    /// happens to hold.** A face is charged to the nearest contact eligible
    /// for it. Deleting contacts cannot unseat a winner — if survivor `S` was
    /// nearest among the whole set it is still nearest among any subset
    /// containing it — so no survivor can LOSE a face to the strip, and every
    /// face whose winner was struck goes to a surviving eligible contact or to
    /// nobody. Hence: every survivor's carried area is non-decreasing, and the
    /// removed contacts' area (whatever is not ownerless) lands on survivors.
    /// Both are asserted below, together with the strict increase that proves
    /// the fixture exercises the path at all.
    ///
    /// **What this does NOT claim, withdrawn 2026-08-19 after review.** An
    /// earlier version of this doc and of `b048acd`'s commit message said
    /// "member sizing sees the increase". That is false, and this branch's own
    /// measurement says so: all four models with non-empty removals emit
    /// BYTE-IDENTICAL artifacts to `develop`, and `develop` has no accepted
    /// contact set at all. No printed geometry moved, so no arm was re-sized.
    ///
    /// The reason is the read order, and it is worth naming the lines. Every
    /// geometric stage reads the ROUTED plan's fold: `route_initial_cco` sets
    /// the terminal radii from `plan.finalized_loads()` BEFORE the accepted set
    /// is derived at the end of that same transaction; `rewire` reads
    /// `state.plan.finalized_loads()`; `brace` and its re-validation read
    /// `state.routed.plan.finalized_loads()`. The ACCEPTED plan's fold is read
    /// in exactly two places — `validate_accepted_contact_set`, a tripwire, and
    /// the verification snapshot the coverage judgment projects. Neither sizes
    /// anything.
    ///
    /// So the safety benefit is real at PLAN level and LATENT in production: it
    /// reaches a printed part only when `PreWriteGate::Load` is ported (task
    /// #59) or when a sizing stage is moved onto the accepted fold. Wiring that
    /// is explicitly not this lane's work. What this test does is pin the INPUT
    /// so that whichever of those lands, it lands on a quantity that is already
    /// correct: the assertion runs over `finalized_loads()`, the tip-sized
    /// fold, so the tip radius a sizer WOULD compute is checked beside the area
    /// and neither is allowed to fall.
    ///
    /// Revert-proof: at `9a9b459` and on `develop` the accepted plan is the
    /// routed plan (no removal, or a removal that refused), so no survivor
    /// increases and the strict-increase assertion fails.
    #[test]
    fn stripping_phantom_credit_only_ever_raises_a_survivors_load_and_its_tip_radius() {
        let table = load_required_runner_corpus();
        let boundary_blocker =
            make_box([30.0, 30.0, 1.0], [0.0, 0.0, 16.55]).expect("no-winner blocker fixture");
        let hollowed = hollowed_fixture(merge_meshes(&[table, boundary_blocker]));
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        prepare_fixture(hollowed, &delicacy, &layers, |prepared| {
            let routed = prepared
                .route_initial_cco(None)
                .expect("no-winner fixture seals through CCO");
            assert!(
                !routed.unrouted_regular_removals().is_empty(),
                "the fixture must actually strip something for this to mean anything"
            );

            let before = routed
                .plan()
                .finalized_loads()
                .expect("the routed plan's own tip-sized load fold");
            let accepted = routed.accepted_plan();
            let after = accepted
                .finalized_loads()
                .expect("the accepted plan's own tip-sized load fold");

            let mut raised = 0usize;
            for contact in accepted.contacts() {
                let id = contact.contact_id();
                let before_leaf = before
                    .leaf_for(id)
                    .expect("a survivor was in the routed fold too");
                let after_leaf = after
                    .leaf_for(id)
                    .expect("a survivor is in the accepted fold");
                // The sizing INPUT never falls. This is the whole safety
                // claim, and its whole extent: an arm may learn it carries
                // more, never less. Nothing in production reads this fold yet
                // — see the doc above.
                assert!(
                    after_leaf.carried_area_mm2() >= before_leaf.carried_area_mm2(),
                    "survivor {id} lost load to the strip: {} -> {}",
                    before_leaf.carried_area_mm2(),
                    after_leaf.carried_area_mm2()
                );
                // And the tip radius this fold already carries moves with it,
                // never against it. That is the sizing OUTPUT a consumer of
                // this fold would get — not one any production stage reads
                // today.
                assert!(
                    after_leaf.tip_radius_mm().get() >= before_leaf.tip_radius_mm().get(),
                    "survivor {id}'s tip shrank while its load grew: {} -> {}",
                    before_leaf.tip_radius_mm().get(),
                    after_leaf.tip_radius_mm().get()
                );
                if after_leaf.carried_area_mm2() > before_leaf.carried_area_mm2() {
                    raised += 1;
                }
            }
            assert!(
                raised > 0,
                "the stripped contacts' area has to land on a survivor; no survivor's load rose"
            );
        });
    }

    fn carried_area_total(plan: &SpecializedContactPlan) -> f64 {
        plan.contacts()
            .iter()
            .map(crate::specialized::SpecializedPhysicalContact::carried_area_mm2)
            .sum()
    }

    /// The snapshot the coverage judgment reads is taken over the ACCEPTED
    /// contact set, which is what puts §6's removal ahead of "coverage
    /// judgment" as the clause orders it.
    #[test]
    fn the_verification_snapshot_projects_the_accepted_contact_set() {
        let table = load_required_runner_corpus();
        let boundary_blocker =
            make_box([30.0, 30.0, 1.0], [0.0, 0.0, 16.55]).expect("no-winner blocker fixture");
        let hollowed = hollowed_fixture(merge_meshes(&[table, boundary_blocker]));
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        prepare_fixture(hollowed, &delicacy, &layers, |prepared| {
            let routed = prepared
                .route_initial_cco(None)
                .expect("no-winner fixture seals through CCO");
            let accepted_digest = routed.accepted_plan().digest();
            let routed_digest = routed.plan().digest();
            assert_ne!(accepted_digest, routed_digest);
            let lowered = routed
                .rewire()
                .expect("fixture rewires")
                .brace()
                .expect("fixture braces")
                .structure()
                .expect("fixture structures")
                .lower()
                .expect("fixture lowers");
            let snapshot = lowered.verification_snapshot().expect("fixture snapshots");
            assert_eq!(snapshot.plan_digest, accepted_digest);
            snapshot
                .validate_integrity()
                .expect("snapshot rebinds over the accepted contact set");
        });
    }

    #[test]
    fn routes_real_no_winner_terminal_with_sealed_evidence() {
        let table = load_required_runner_corpus();
        let boundary_blocker =
            make_box([30.0, 30.0, 1.0], [0.0, 0.0, 16.55]).expect("no-winner blocker fixture");
        let hollowed = hollowed_fixture(merge_meshes(&[table, boundary_blocker]));
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        prepare_fixture(hollowed, &delicacy, &layers, |prepared| {
            let routed = prepared
                .route_initial_cco(None)
                .expect("no-winner fixture seals through CCO");
            let row = routed
                .transcript()
                .rows()
                .iter()
                .find(|row| row.kind() == crate::cco::FinalizedDispositionKind::UnroutedNoWinner)
                .expect("fixture retains a real no-winner terminal");
            assert!(row.route().is_some());
            let reason = row
                .no_winner_reason()
                .expect("no-winner transcript row retains a reason");
            assert!(!reason.anchor_seat_refusals().is_empty());
            let braced = routed
                .rewire()
                .expect("no-winner fixture rewires")
                .brace()
                .expect("no-winner fixture braces");
            assert_mixed_snapshot_projects_only_routed(
                braced,
                FinalizedDispositionKind::UnroutedNoWinner,
            );
        });
    }

    #[test]
    fn initial_routing_is_demand_bearing_before_regular_with_exact_ids() {
        with_prepared_table(|prepared| {
            let routing = prepared.initial().routing_order();
            let mut demanded = prepared
                .plan()
                .contacts()
                .iter()
                .filter(|contact| !contact.demands().is_empty())
                .map(|contact| u32::try_from(contact.contact_id()).unwrap())
                .collect::<Vec<_>>();
            let mut regular = prepared
                .plan()
                .contacts()
                .iter()
                .filter(|contact| contact.demands().is_empty())
                .map(|contact| u32::try_from(contact.contact_id()).unwrap())
                .collect::<Vec<_>>();
            demanded.sort_unstable();
            regular.sort_unstable();
            assert!(routing
                .specialized_terminals()
                .iter()
                .all(|terminal| { demanded.binary_search(terminal).is_ok() }));
            assert!(routing
                .regular_terminals()
                .iter()
                .all(|terminal| { regular.binary_search(terminal).is_ok() }));
            let mut routed = routing
                .specialized_terminals()
                .iter()
                .chain(routing.regular_terminals())
                .copied()
                .collect::<Vec<_>>();
            routed.sort_unstable();
            let mut physical_ids = prepared
                .plan()
                .contacts()
                .iter()
                .map(|contact| u32::try_from(contact.contact_id()).unwrap())
                .collect::<Vec<_>>();
            physical_ids.sort_unstable();
            let mut terminal_ids = prepared
                .initial()
                .terminals()
                .iter()
                .map(|row| row.terminal)
                .collect::<Vec<_>>();
            terminal_ids.sort_unstable();
            assert_eq!(routed, physical_ids);
            assert_eq!(terminal_ids, physical_ids);
        });
    }

    #[test]
    fn canonical_empty_regular_selection_reaches_specialized_planning() {
        let hollowed = hollowed_fixture(make_box([10.0; 3], [0.0, 0.0, 0.0]).unwrap());
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        let prepared = prepare_support_run(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).unwrap(),
            },
            authority(&delicacy, &layers),
        )
        .expect("empty regular authority is canonical");
        assert!(prepared.generic().islands().is_empty());
        assert!(prepared.regular().contacts().is_empty());
        assert!(prepared.regular().coverage_samples().is_empty());
        assert_eq!(
            prepared.initial().queried_stubs(),
            prepared.plan().contacts().len()
        );
    }

    #[test]
    fn mismatched_authority_is_rejected_before_native_preparation() {
        let hollowed = hollowed_fixture(procedural_overhang_for_error_case());
        let perception_delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let mut selection_delicacy = perception_delicacy.clone();
        selection_delicacy[0] = 0.25;
        let layers = layers();
        let perception = SupportPerceptionConfig::current(
            Finite::new(0.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            GRID_BYTES,
            Some(&perception_delicacy),
        )
        .unwrap();
        let selection =
            ContactSelectionConfig::current_defaults(Some(&selection_delicacy), GRID_BYTES)
                .unwrap();
        let result = prepare_support_run(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).unwrap(),
            },
            SupportPreparationAuthority::new(
                &layers,
                perception,
                candidate_config(),
                &[],
                GenericPerceptionLimits::production(TEST_KNN_LIMITS),
                selection,
                SpecializedLimits::production(
                    TEST_KNN_LIMITS,
                    crate::specialized::test_batch_work(),
                ),
                ContactConstructionLimits::production(),
                brace_penetration_limits(),
                RaftLimits::new(100_000, 100_000, 200_000, 128 * 1024 * 1024),
                200_000,
                128 * 1024 * 1024,
            ),
        );
        assert!(matches!(
            result,
            Err(SupportPreparationError::DelicacyAuthorityMismatch)
        ));
    }

    #[test]
    fn uncovered_layers_and_stale_reservations_fail_typed() {
        let hollowed = hollowed_fixture(procedural_overhang_for_error_case());
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let short_layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let result = prepare_support_run(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).unwrap(),
            },
            authority(&delicacy, &short_layers),
        );
        assert!(matches!(
            result,
            Err(SupportPreparationError::LayerScheduleCoverage)
        ));

        let hollowed = hollowed_fixture(procedural_overhang_for_error_case());
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        let mut authority = authority(&delicacy, &layers);
        let stale = [SpecializedOwnerReservation {
            owner_id: 7,
            face_id: u32::MAX,
        }];
        authority.reservations = &stale;
        let result = prepare_support_run(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).unwrap(),
            },
            authority,
        );
        assert!(matches!(
            result,
            Err(SupportPreparationError::ReservationFaceOutOfRange { .. })
        ));
    }

    #[test]
    fn braced_run_creates_one_shared_view_for_joint_and_raft_semantics() {
        with_braced_table(|braced| {
            let structured = braced.structure().expect("required corpus structures");
            structured
                .validate_integrity()
                .expect("structured plans retain the exact shared authority");
            assert_eq!(
                structured.joints().digest(),
                structured.joints().digest(),
                "joint semantic digest is stable"
            );
            assert_eq!(
                structured.raft().digest(),
                structured.raft().digest(),
                "raft semantic digest is stable"
            );
            assert!(matches!(
                structured.brace_termination(),
                BraceFixedPointTermination::Converged
                    | BraceFixedPointTermination::IterationLimit
                    | BraceFixedPointTermination::Unplaceable
            ));
            assert_eq!(
                structured.live_view().digest(),
                structured.live_view().digest(),
                "the opaque view remains one sealed identity"
            );
        });
    }

    #[test]
    fn structured_run_lowers_the_required_corpus_into_a_supported_model() {
        with_braced_table(|braced| {
            let lowered = braced
                .structure()
                .expect("required corpus structures")
                .lower()
                .expect("required corpus lowers");
            let digest = lowered.digest();
            let model = lowered
                .into_supported_model()
                .expect("lowered corpus constructs the public scene");
            assert_eq!(
                model.primitives().len(),
                model.history().support().support_primitive_count
                    + model.history().support().raft_primitive_count
            );
            assert_ne!(digest.0, 0, "lowered primitive evidence is retained");
        });
    }

    #[test]
    fn lowered_run_snapshot_exposes_only_transcript_routed_contacts_in_cadence_order() {
        with_braced_table(|braced| {
            let lowered = braced
                .structure()
                .expect("required corpus structures")
                .lower()
                .expect("required corpus lowers");
            let expected = lowered
                .structured
                .braced
                .rewired
                .routed
                .transcript
                .rows()
                .iter()
                .filter(|row| row.kind() == FinalizedDispositionKind::Routed)
                .map(|row| row.physical_contact_id())
                .collect::<Vec<_>>();
            let snapshot = lowered
                .verification_snapshot()
                .expect("validated final authority snapshots");
            assert_eq!(
                snapshot
                    .routed_contacts()
                    .map(|contact| contact.physical_contact_id())
                    .collect::<Vec<_>>(),
                expected,
                "quarantined and no-winner transcript rows are absent"
            );
            assert_eq!(
                snapshot.coverage_samples(),
                lowered
                    .structured
                    .braced
                    .rewired
                    .routed
                    .plan
                    .coverage_samples()
            );
            assert_eq!(
                snapshot.routed_route_memberships.len(),
                snapshot.routed_contacts.len(),
                "each physical seat retains one private logical-route projection"
            );
            assert!(
                snapshot
                    .routed_route_memberships
                    .iter()
                    .all(|membership| !membership.logical_routes.is_empty()),
                "routed seats cannot be measured from pose alone"
            );
            assert!(validate_routed_memberships(
                &snapshot.routed_route_memberships,
                &IslandFaceIndex::build_named(snapshot.named_islands, snapshot.topology)
                    .expect("the fixture snapshot's named-island sidecar is well-formed"),
                snapshot.topology,
            ));
            let expected_members = lowered
                .structured
                .live_view
                .keys()
                .iter()
                .map(|key| {
                    let member = lowered
                        .structured
                        .braced
                        .plan
                        .forest
                        .member(*key)
                        .expect("final live view never contains a tombstoned key");
                    (
                        *key,
                        member.p0_mm().map(Finite::get),
                        member.p1_mm().map(Finite::get),
                        member.terminal_radius_mm().get().get(),
                        member.rootward_radius_mm().get().get(),
                        member.kind().clone(),
                        member.model_contact_end(),
                    )
                })
                .collect::<Vec<_>>();
            let actual_members = snapshot
                .final_live_members()
                .map(|member| {
                    (
                        member.key(),
                        member.p0_mm(),
                        member.p1_mm(),
                        member.terminal_radius_mm(),
                        member.rootward_radius_mm(),
                        member.kind().clone(),
                        member.model_contact_end(),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(actual_members, expected_members);
            assert_eq!(
                actual_members
                    .iter()
                    .map(|member| member.0)
                    .collect::<Vec<_>>(),
                lowered.lowered.member_keys(),
                "the sealed member order is exactly the lowered member order"
            );
            snapshot
                .validate_integrity()
                .expect("borrowed snapshot remains bound to retained authority");
        });
    }

    #[test]
    fn verification_snapshot_digest_input_list_is_pinned() {
        // Pins the exact FNV input list of `verification_snapshot_digest`
        // over a minimal deterministic fixture. Widening the input set (as
        // commit 908d6df did by adding `final_live_members_digest` and the
        // final live member count) must flip this pinned value first,
        // forcing a deliberate refresh of downstream frozen goldens such as
        // plinth-verify's `coverage_gate_overhang_table.json`.
        let mesh = make_box([1.0, 1.0, 1.0], [0.0, 0.0, 0.0]).expect("pinned fixture mesh");
        let topology = plinth_mesh::build_face_topology(&mesh, plinth_mesh::WeldPolicy::None)
            .expect("pinned fixture topology");
        let surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: 4 * 1024 * 1024,
            },
        )
        .expect("pinned fixture native surface");
        let mesh_digest = mesh_fingerprint(&mesh).expect("pinned fixture fingerprint");
        let members = vec![FinalLiveMemberEvidence::transit_test_fixture(
            [1, 0],
            [[0.25, 0.5, 0.5], [0.25, 0.5, -1.0]],
            [0.1, 0.2],
            MemberKind::Stub,
            Some(ContactEnd::Terminalward),
        )];
        let snapshot = SupportVerificationSnapshot::transit_test_fixture(
            &mesh,
            &topology,
            &surface,
            &mesh_digest,
            members,
        );
        snapshot
            .validate_integrity()
            .expect("pinned fixture snapshot integrity");
        assert_eq!(snapshot.digest(), 11_833_545_896_309_364_349);
    }

    #[test]
    fn verification_snapshot_digest_corruption_fails_closed() {
        with_braced_table(|braced| {
            let lowered = braced
                .structure()
                .expect("required corpus structures")
                .lower()
                .expect("required corpus lowers");
            let mut snapshot = lowered
                .verification_snapshot()
                .expect("validated final authority snapshots");
            snapshot.plan_digest = snapshot.plan_digest.wrapping_add(1);
            assert_eq!(
                snapshot.validate_integrity(),
                Err(SupportVerificationError::SnapshotDigestMismatch)
            );
            assert_eq!(
                snapshot.measure_coverage(
                    CoverageMeasurementRequest::from_f64(
                        1.0,
                        1.0,
                        CoverageMeasurementLimits::new(
                            usize::MAX,
                            usize::MAX,
                            usize::MAX,
                            usize::MAX,
                            usize::MAX,
                            usize::MAX,
                        )
                        .expect("nonzero measurement limits"),
                    )
                    .expect("finite measurement request"),
                ),
                Err(CoverageMeasurementError::SnapshotBinding)
            );
        });
    }

    #[test]
    fn verification_snapshot_member_evidence_corruption_fails_closed() {
        with_braced_table(|braced| {
            let lowered = braced
                .structure()
                .expect("required corpus structures")
                .lower()
                .expect("required corpus lowers");
            let mut snapshot = lowered
                .verification_snapshot()
                .expect("validated final authority snapshots");
            let member = snapshot
                .final_live_members
                .first_mut()
                .expect("corpus emits final live members");
            member.p0_mm[0] += 1.0;
            assert_eq!(
                snapshot.validate_integrity(),
                Err(SupportVerificationError::SnapshotDigestMismatch)
            );
        });
    }

    #[test]
    fn verification_snapshot_rejects_rebound_native_surface() {
        with_braced_table(|braced| {
            let lowered = braced
                .structure()
                .expect("required corpus structures")
                .lower()
                .expect("required corpus lowers");
            let mut snapshot = lowered
                .verification_snapshot()
                .expect("validated final authority snapshots");
            let rebound = NativeSurface::from_indexed(
                snapshot.mesh.verts(),
                snapshot.mesh.tris(),
                NativeSurfaceConfig {
                    grid_pitch_mm: 0.25,
                    max_grid_bytes: usize::MAX,
                },
            )
            .expect("test surface with a distinct query configuration");
            snapshot.surface = &rebound;
            assert_eq!(
                snapshot.validate_integrity(),
                Err(SupportVerificationError::SnapshotDigestMismatch)
            );
        });
    }

    #[test]
    fn verification_snapshot_rejects_wrong_missing_and_duplicate_route_projections() {
        with_braced_table(|braced| {
            let lowered = braced
                .structure()
                .expect("required corpus structures")
                .lower()
                .expect("required corpus lowers");

            let mut wrong = lowered
                .verification_snapshot()
                .expect("validated final authority snapshots");
            wrong.routed_route_memberships[0].logical_routes[0].face_id = u32::MAX;
            assert_eq!(
                wrong.validate_integrity(),
                Err(SupportVerificationError::SnapshotDigestMismatch)
            );

            let mut missing = lowered
                .verification_snapshot()
                .expect("validated final authority snapshots");
            missing.routed_route_memberships[0].logical_routes.clear();
            assert_eq!(
                missing.validate_integrity(),
                Err(SupportVerificationError::SnapshotDigestMismatch)
            );

            let mut duplicate = lowered
                .verification_snapshot()
                .expect("validated final authority snapshots");
            let route = duplicate.routed_route_memberships[0].logical_routes[0];
            duplicate.routed_route_memberships[0]
                .logical_routes
                .push(route);
            assert_eq!(
                duplicate.validate_integrity(),
                Err(SupportVerificationError::SnapshotDigestMismatch)
            );
        });
    }

    #[test]
    fn distinct_provenance_rows_remain_two_memberships_and_one_indexed_seat() {
        let regular = RoutedLogicalRoute {
            island_id: 3,
            face_id: 7,
            component_id: 11,
            provenance: RoutedRouteProvenance::RegularGeneric,
        };
        let specialized = RoutedLogicalRoute {
            provenance: RoutedRouteProvenance::SpecializedMinimum,
            ..regular
        };
        assert_ne!(regular, specialized);
        assert_eq!(BTreeSet::from([regular, specialized]).len(), 2);

        let points = [regular, specialized]
            .into_iter()
            .map(|route| CoverageRoutePoint {
                physical_id: 9,
                island_id: route.island_id,
                component_id: route.component_id,
                position_mm: [0.0, 0.0, 0.0],
            })
            .collect::<Vec<_>>();
        let samples = [CoverageSample {
            island_id: 3,
            face_id: 7,
            component_id: 11,
            point_mm: [0.0, 0.0, 0.0],
            area_mm2: 1.0,
        }];
        let planned = plan_route_points_nearest(
            &points,
            &samples,
            TEST_KNN_LIMITS,
            NearestWitnessRetention::NearestOnly,
        )
        .expect("distinct provenance rows share one indexed seat");
        assert_eq!(planned.measurement_counts(), (2, 2, 1, 1, 1, 1));
    }

    #[test]
    fn coverage_route_plan_bills_planned_elements_not_vec_overallocation() {
        let points = [
            CoverageRoutePoint {
                physical_id: 1,
                island_id: 3,
                component_id: 11,
                position_mm: [0.0, 0.0, 0.0],
            },
            CoverageRoutePoint {
                physical_id: 2,
                island_id: 5,
                component_id: 13,
                position_mm: [1.0, 0.0, 0.0],
            },
        ];
        let samples = [CoverageSample {
            island_id: 3,
            face_id: 7,
            component_id: 11,
            point_mm: [0.0, 0.0, 0.0],
            area_mm2: 1.0,
        }];
        let planned = plan_route_points_nearest(
            &points,
            &samples,
            TEST_KNN_LIMITS,
            NearestWitnessRetention::NearestOnly,
        )
        .expect("tiny fixture route plan");
        let expected = 2 * size_of::<crate::specialized::CoverageRoute>()
            + 2 * size_of::<(crate::specialized::CoverageRoute, usize)>()
            + 2 * size_of::<u64>()
            + size_of::<(crate::specialized::CoverageRoute, usize)>()
            + size_of::<crate::specialized::CoverageRouteKnnPlan>()
            + size_of::<u64>()
            + size_of::<usize>();
        assert_eq!(planned.route_plan_count(), 1);
        assert_eq!(planned.retained_base_bytes(), expected);
    }

    #[test]
    fn coverage_route_plan_mismatch_is_detected() {
        let points = [CoverageRoutePoint {
            physical_id: 1,
            island_id: 3,
            component_id: 11,
            position_mm: [0.0, 0.0, 0.0],
        }];
        let samples = [CoverageSample {
            island_id: 3,
            face_id: 7,
            component_id: 11,
            point_mm: [0.0, 0.0, 0.0],
            area_mm2: 1.0,
        }];
        let planned = plan_route_points_nearest(
            &points,
            &samples,
            TEST_KNN_LIMITS,
            NearestWitnessRetention::NearestOnly,
        )
        .expect("tiny fixture route plan");
        let (memberships, membership_peak, indexed_points, indexed_peak, query_rows, distance_work) =
            planned.measurement_counts();
        let preflight = CoverageMeasurementPreflight {
            memberships,
            sequential_membership_peak: membership_peak,
            indexed_points,
            sequential_indexed_peak: indexed_peak,
            query_rows,
            distance_work,
            route_retained_base_bytes: planned.retained_base_bytes(),
            route_knn_peak_bytes: planned.max_knn_peak_bytes(),
            island_row_count: 0,
            assessment: crate::CoverageAssessmentResourcePlan {
                scratch_bytes: 0,
                evidence_bytes: 0,
                phase_peak_bytes: 0,
                operations: 0,
            },
            route_plan_count: planned.route_plan_count() + 1,
            logical_peak_bytes: 0,
        };
        assert!(!measurement_plan_matches_preflight(preflight, &planned));
    }

    #[test]
    fn coverage_membership_cap_minus_one_refuses_before_projection_reserve() {
        with_braced_table(|braced| {
            let lowered = braced
                .structure()
                .expect("required corpus structures")
                .lower()
                .expect("required corpus lowers");
            let snapshot = lowered
                .verification_snapshot()
                .expect("validated final authority snapshots");
            let membership_count = snapshot
                .routed_route_memberships
                .iter()
                .try_fold(0usize, |total, membership| {
                    total.checked_add(membership.logical_routes.len())
                })
                .expect("fixture membership count fits usize");
            assert!(
                membership_count > 1,
                "fixture has multiple logical memberships"
            );
            let limits = CoverageMeasurementLimits::new(
                membership_count - 1,
                usize::MAX,
                usize::MAX,
                usize::MAX,
                usize::MAX,
                usize::MAX,
            )
            .expect("all ceilings are nonzero");
            let request = CoverageMeasurementRequest::from_f64(1.0, 1.0, limits)
                .expect("finite measurement request");
            reset_measurement_call_counts();
            assert_eq!(
                snapshot.measure_coverage(request),
                Err(CoverageMeasurementError::ResourceLimit {
                    resource: "route memberships",
                })
            );
            assert_eq!(
                measurement_call_counts().normalized_route_projection_reserves,
                0,
                "the capped request must not reserve the normalized projection"
            );
        });
    }

    /// One snapshot has one named-island sidecar, so one integrity pass proves
    /// it well-formed exactly once.
    ///
    /// The defect this pins: `routed_logical_routes` re-proved the sidecar for
    /// every routed contact and then answered each route's membership with a
    /// linear island walk ending in a linear `Vec::contains`, so a single
    /// `validate_integrity` over `N` routed contacts walked the whole sidecar
    /// `2N + 1` times -- and the pre-write coverage gate runs it on every
    /// production emission. Reverting the fix puts the count back to
    /// `1 + N` validations here, which this equality refuses.
    #[test]
    fn snapshot_integrity_proves_the_island_sidecar_exactly_once() {
        with_braced_table(|braced| {
            let lowered = braced
                .structure()
                .expect("required corpus structures")
                .lower()
                .expect("required corpus lowers");
            let snapshot = lowered
                .verification_snapshot()
                .expect("validated final authority snapshots");
            assert!(
                snapshot.routed_route_memberships.len() > 1,
                "the fixture must route more than one contact for this count to bite",
            );
            reset_measurement_call_counts();
            snapshot
                .validate_integrity()
                .expect("the fixture snapshot is intact");
            assert_eq!(
                measurement_call_counts().named_island_validations,
                1,
                "the sidecar is proved once per snapshot, not once per routed contact",
            );
        });
    }

    /// The coverage preflight's per-route-key work stays linear in its own
    /// inputs.
    ///
    /// The defect this pins is asymptotic, so the assertion is a bound rather
    /// than a pinned number. For `M` route entries, `K` distinct route keys
    /// and `S` coverage samples, the preflight used to run an `O(M^2)`
    /// sealed-slice prefix scan to decide first occurrences, then, per
    /// distinct key, a fresh scan of every membership *twice* and of every
    /// coverage sample once -- `O(M^2 + K(M + S))`, on every production
    /// emission, because the pre-write gate runs this preflight per artifact.
    /// One sorted column per input set answers all of it in `O(M + S)` element
    /// examinations plus binary searches.
    ///
    /// The bound below is deliberately generous -- four times the input size --
    /// so it fails only on a return to superlinear scanning, not on an honest
    /// change of constant factor.
    #[test]
    fn the_coverage_preflight_key_work_stays_linear_in_its_inputs() {
        with_braced_table(|braced| {
            let lowered = braced
                .structure()
                .expect("required corpus structures")
                .lower()
                .expect("required corpus lowers");
            let snapshot = lowered
                .verification_snapshot()
                .expect("validated final authority snapshots");
            let route_entries = snapshot
                .routed_route_memberships
                .iter()
                .map(|membership| membership.logical_routes.len())
                .sum::<usize>();
            let samples = snapshot.coverage_samples.len();
            assert!(
                route_entries > 1 && samples > 1,
                "the fixture must carry routes and samples for this bound to bite",
            );
            let unlimited = CoverageMeasurementLimits::new(
                usize::MAX,
                usize::MAX,
                usize::MAX,
                usize::MAX,
                usize::MAX,
                usize::MAX,
            )
            .expect("nonzero limits");
            let island_index =
                IslandFaceIndex::build(snapshot.named_islands).expect("well-formed fixture");
            reset_measurement_call_counts();
            snapshot
                .measurement_preflight(
                    unlimited,
                    KnnBatchLimits {
                        max_source_points: usize::MAX,
                        max_query_points: usize::MAX,
                        max_neighbors: 1,
                        max_logical_peak_bytes: usize::MAX,
                        max_work: usize::MAX,
                    },
                    &island_index,
                )
                .expect("fixture preflight");
            let scans = measurement_call_counts().preflight_route_key_scans;
            let bound = 4 * (route_entries + samples);
            assert!(
                scans <= bound,
                "the preflight examined {scans} elements for {route_entries} route entries \
                 and {samples} coverage samples; the linear bound is {bound}",
            );
        });
    }

    #[test]
    fn coverage_every_cap_preflights_before_all_measurement_allocations() {
        with_braced_table(|braced| {
            let lowered = braced
                .structure()
                .expect("required corpus structures")
                .lower()
                .expect("required corpus lowers");
            let snapshot = lowered
                .verification_snapshot()
                .expect("validated final authority snapshots");
            let unlimited = CoverageMeasurementLimits::new(
                usize::MAX,
                usize::MAX,
                usize::MAX,
                usize::MAX,
                usize::MAX,
                usize::MAX,
            )
            .expect("nonzero limits");
            let island_index =
                IslandFaceIndex::build(snapshot.named_islands).expect("well-formed fixture");
            let summary = snapshot
                .measurement_preflight(
                    unlimited,
                    KnnBatchLimits {
                        max_source_points: usize::MAX,
                        max_query_points: usize::MAX,
                        max_neighbors: 1,
                        max_logical_peak_bytes: usize::MAX,
                        max_work: usize::MAX,
                    },
                    &island_index,
                )
                .expect("fixture preflight");
            let exact = CoverageMeasurementLimits::new(
                summary.memberships,
                summary.indexed_points,
                summary.query_rows,
                summary.distance_work,
                summary.logical_peak_bytes,
                summary.assessment.operations,
            )
            .expect("fixture resources are nonzero");
            assert!(
                [
                    summary.memberships,
                    summary.indexed_points,
                    summary.query_rows,
                    summary.distance_work,
                    summary.logical_peak_bytes,
                    summary.assessment.operations,
                ]
                .into_iter()
                .all(|value| value > 1),
                "fixture must exercise valid cap-minus-one requests"
            );
            assert!(snapshot
                .measure_coverage(
                    CoverageMeasurementRequest::from_f64(1.0, 1.0, exact)
                        .expect("valid exact request"),
                )
                .is_ok());

            for (
                resource,
                membership_cap,
                indexed_cap,
                query_cap,
                work_cap,
                peak_cap,
                assessment_cap,
            ) in [
                (
                    "route memberships",
                    summary.memberships - 1,
                    summary.indexed_points,
                    summary.query_rows,
                    summary.distance_work,
                    summary.logical_peak_bytes,
                    summary.assessment.operations,
                ),
                (
                    "indexed points",
                    summary.memberships,
                    summary.indexed_points - 1,
                    summary.query_rows,
                    summary.distance_work,
                    summary.logical_peak_bytes,
                    summary.assessment.operations,
                ),
                (
                    "query rows",
                    summary.memberships,
                    summary.indexed_points,
                    summary.query_rows - 1,
                    summary.distance_work,
                    summary.logical_peak_bytes,
                    summary.assessment.operations,
                ),
                (
                    "distance work",
                    summary.memberships,
                    summary.indexed_points,
                    summary.query_rows,
                    summary.distance_work - 1,
                    summary.logical_peak_bytes,
                    summary.assessment.operations,
                ),
                (
                    "logical peak bytes",
                    summary.memberships,
                    summary.indexed_points,
                    summary.query_rows,
                    summary.distance_work,
                    summary.logical_peak_bytes - 1,
                    summary.assessment.operations,
                ),
                (
                    "assessment work",
                    summary.memberships,
                    summary.indexed_points,
                    summary.query_rows,
                    summary.distance_work,
                    summary.logical_peak_bytes,
                    summary.assessment.operations - 1,
                ),
            ] {
                reset_measurement_call_counts();
                let limits = CoverageMeasurementLimits::new(
                    membership_cap,
                    indexed_cap,
                    query_cap,
                    work_cap,
                    peak_cap,
                    assessment_cap,
                )
                .expect("cap-minus-one remains nonzero");
                assert_eq!(
                    snapshot.measure_coverage(
                        CoverageMeasurementRequest::from_f64(1.0, 1.0, limits)
                            .expect("valid capped request"),
                    ),
                    Err(CoverageMeasurementError::ResourceLimit { resource })
                );
                // This test is about MEASUREMENT work: no cap may be reached
                // after the measurement has already reserved or queried
                // anything. It names those four counters rather than comparing
                // the whole struct, so that adding a counter for a different
                // kind of quantity cannot silently widen what it claims.
                let counts = measurement_call_counts();
                assert_eq!(
                    (
                        counts.normalized_route_projection_reserves,
                        counts.coverage_route_planner_allocations,
                        counts.coverage_route_queries,
                        counts.coverage_assessment_traversals,
                    ),
                    (0, 0, 0, 0),
                    "the capped request must refuse before any measurement work",
                );
                // The snapshot's own integrity proof is NOT measurement work:
                // `measure_coverage` runs it before it looks at a single cap,
                // and it costs one pass over the named-island sidecar. Pinned
                // at exactly one here for the same reason
                // `snapshot_integrity_proves_the_island_sidecar_exactly_once`
                // pins it: it used to run once per routed contact as well.
                assert_eq!(counts.named_island_validations, 1);
            }
        });
    }

    #[test]
    fn coverage_logical_peak_cap_preflights_duplicate_face_evidence_before_query() {
        with_braced_table(|braced| {
            let lowered = braced
                .structure()
                .expect("required corpus structures")
                .lower()
                .expect("required corpus lowers");
            let snapshot = lowered
                .verification_snapshot()
                .expect("validated final authority snapshots");
            assert!(
                snapshot
                    .coverage_samples
                    .windows(2)
                    .any(|rows| rows[0].face_id == rows[1].face_id),
                "fixture retains multiple samples for one source face"
            );
            let points = snapshot
                .routed_route_memberships
                .iter()
                .flat_map(|membership| {
                    membership
                        .logical_routes
                        .iter()
                        .map(move |route| CoverageRoutePoint {
                            physical_id: membership.contact.physical_contact_id(),
                            island_id: route.island_id,
                            component_id: route.component_id,
                            position_mm: membership.contact.position_mm(),
                        })
                })
                .collect::<Vec<_>>();
            let planned = plan_route_points_nearest(
                &points,
                snapshot.coverage_samples,
                TEST_KNN_LIMITS,
                NearestWitnessRetention::NearestOnly,
            )
            .expect("fixture route preflight");
            let island_rows = snapshot
                .coverage_samples
                .iter()
                .filter(|sample| {
                    snapshot.named_islands.iter().any(|island| {
                        island.id == sample.island_id && island.face_ids.contains(&sample.face_id)
                    })
                })
                .count();
            let assessment = coverage_measurement_assessment_resource_plan(
                snapshot.coverage_samples,
                snapshot.topology,
                island_rows,
            )
            .expect("assessment preflight");
            let logical_peak = snapshot
                .coverage_samples
                .len()
                .checked_mul(size_of::<f64>())
                .and_then(|bytes| bytes.checked_add(points.len() * size_of::<CoverageRoutePoint>()))
                .and_then(|base| {
                    planned
                        .retained_base_bytes()
                        .checked_add(planned.max_knn_peak_bytes())
                        .and_then(|route_peak| {
                            base.checked_add(route_peak.max(assessment.phase_peak_bytes))
                        })
                })
                .expect("fixture logical peak fits usize");
            let (memberships, _, indexed, _, rows, distance_work) = planned.measurement_counts();
            let request_for = |max_logical_peak_bytes| {
                CoverageMeasurementRequest::from_f64(
                    0.000_001,
                    0.000_001,
                    CoverageMeasurementLimits::new(
                        memberships.max(1),
                        indexed.max(1),
                        rows.max(1),
                        distance_work.max(1),
                        max_logical_peak_bytes,
                        assessment.operations.max(1),
                    )
                    .expect("nonzero fixture limits"),
                )
                .expect("finite measurement request")
            };
            let measurement = snapshot
                .measure_coverage(request_for(logical_peak))
                .expect("exact logical peak passes");
            for patch in measurement.evidence().patches() {
                assert!(patch
                    .sorted_face_ids()
                    .windows(2)
                    .all(|faces| faces[0] < faces[1]));
            }
            reset_measurement_call_counts();
            assert_eq!(
                snapshot.measure_coverage(request_for(logical_peak - 1)),
                Err(CoverageMeasurementError::ResourceLimit {
                    resource: "logical peak bytes",
                })
            );
            assert_eq!(
                measurement_call_counts().coverage_route_queries,
                0,
                "the cap-minus-one request must not query or traverse coverage"
            );
        });
    }

    #[test]
    fn lowering_cap_minus_one_fails_before_any_supported_model_is_constructed() {
        let (count, bytes) = with_structured_limits(200_000, 128 * 1024 * 1024, |structured| {
            let lowered = structured.lower().expect("uncapped corpus lowers");
            let count = lowered
                .lowered
                .support_primitive_count()
                .checked_add(lowered.lowered.raft_primitive_count())
                .expect("fixture count fits usize");
            (count, count * size_of::<crate::SupportPrimitive>())
        });
        assert!(count > 1, "required corpus has more than one primitive");
        with_structured_limits(count - 1, bytes, |structured| {
            assert!(matches!(
                stage_error(structured.lower()),
                Err(SupportLoweringError::Lowering(
                    LoweringError::PrimitiveLimit
                ))
            ));
        });
        with_structured_limits(count, bytes - 1, |structured| {
            assert!(matches!(
                stage_error(structured.lower()),
                Err(SupportLoweringError::Lowering(
                    LoweringError::PrimitiveBytesLimit
                ))
            ));
        });
    }

    #[test]
    fn runner_thread_sweep_child() {
        if std::env::var_os(SWEEP_CHILD).is_none() {
            return;
        }
        with_prepared_table(|prepared| {
            let requested = std::env::var("PLINTH_THREADS")
                .unwrap()
                .parse::<usize>()
                .unwrap();
            let thread_evidence = prepared.plan().generic_slice_thread_evidence();
            assert_eq!(thread_evidence.0, requested);
            assert!((1..=requested).contains(&thread_evidence.1));
            let routed = prepared
                .route_initial_cco(None)
                .expect("thread sweep routes the required corpus");
            let rewired = routed
                .rewire()
                .expect("thread sweep rewires the required corpus");
            let braced = rewired
                .brace()
                .expect("thread sweep braces the required corpus");
            let structured = braced
                .structure()
                .expect("thread sweep structures required corpus");
            let lineage = format!(
                "{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                structured.context.digests().prepared(),
                structured.context.digests().plan(),
                structured.context.digests().loads(),
                structured.context.digests().initial_forest(),
                structured.braced.rewired.routed.initial.queried_stubs(),
                structured.braced.rewired.routed.initial.condemned().len(),
                structured
                    .braced
                    .rewired
                    .routed
                    .transcript
                    .final_forest_digest(),
                structured
                    .braced
                    .rewired
                    .routed
                    .transcript
                    .final_controller_digest(),
                structured.braced.rewired.routed.transcript.digest(),
                rewire_report_digest(&structured.braced.rewired.report),
                structured.braced.rewired.post_rewire_forest_digest,
                structured.braced.post_brace_forest_digest,
                structured.braced.post_brace_report_digest,
                structured.braced.plan.iterations,
                structured.braced.plan.additions.len(),
                structured.live_view.digest(),
                structured.joints.digest(),
                structured.raft.digest().0,
                match structured.braced.plan.termination {
                    crate::brace::BraceFixedPointTermination::Converged => 0,
                    crate::brace::BraceFixedPointTermination::IterationLimit => 1,
                    crate::brace::BraceFixedPointTermination::Unplaceable => 2,
                    crate::brace::BraceFixedPointTermination::ContactRefused => 3,
                },
            );
            let lowered = structured
                .lower()
                .expect("thread sweep lowers required corpus");
            let lower_digest = lowered.digest();
            let snapshot = lowered
                .verification_snapshot()
                .expect("thread sweep snapshots validated final authority");
            let snapshot_digest = snapshot.digest();
            let coverage = format!(
                "{:?}",
                snapshot.measure_coverage(
                    CoverageMeasurementRequest::from_f64(
                        1.0,
                        1.0,
                        CoverageMeasurementLimits::new(
                            usize::MAX,
                            usize::MAX,
                            usize::MAX,
                            usize::MAX,
                            usize::MAX,
                            usize::MAX,
                        )
                        .expect("nonzero thread-sweep measurement limits"),
                    )
                    .expect("thread sweep measures sealed snapshot"),
                ) // `Debug` contains every private request, resource, evidence,
                  // and binding field; comparing it across child processes keeps
                  // this process boundary from reducing the check to a digest.
            );
            let routed_contact_count = snapshot.routed_contacts().len();
            snapshot
                .validate_integrity()
                .expect("thread sweep snapshot remains sealed");
            drop(snapshot);
            let model = lowered
                .into_supported_model()
                .expect("thread sweep constructs supported model");
            println!(
                "{SWEEP_LINE}{lineage}:{}:{}:{}:{}:{}:{}:{}:{}:{coverage}",
                model.primitives().len(),
                model.history().support().support_primitive_count,
                model.history().support().raft_primitive_count,
                lower_digest.0,
                snapshot_digest,
                routed_contact_count,
                model.lift_mm().get().to_bits(),
                model.plate_z_mm().get().to_bits(),
            );
        });
    }

    #[test]
    fn actual_one_four_sixteen_thread_complete_lowered_lineage_is_digest_identical() {
        if std::env::var_os(SWEEP_CHILD).is_some() {
            return;
        }
        let executable = std::env::current_exe().expect("current plinth-support test binary");
        let signatures = [1, 4, 16]
            .into_iter()
            .map(|threads| {
                let output = Command::new(&executable)
                    .arg("--exact")
                    .arg("runner::tests::runner_thread_sweep_child")
                    .arg("--nocapture")
                    .env(SWEEP_CHILD, "1")
                    .env("PLINTH_THREADS", threads.to_string())
                    .output()
                    .expect("runner thread-sweep child starts");
                assert!(
                    output.status.success(),
                    "runner {threads}-thread child failed:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                String::from_utf8(output.stdout)
                    .expect("test output is UTF-8")
                    .split_once(SWEEP_LINE)
                    .and_then(|(_, signature)| signature.lines().next())
                    .map(str::to_owned)
                    .expect("runner child emits digest evidence")
            })
            .collect::<Vec<_>>();
        assert_eq!(signatures[0], signatures[1]);
        assert_eq!(signatures[0], signatures[2]);
    }

    // ------------------------------------------------------------------
    // Malformed preparation resource caps.
    //
    // These live in-crate by necessity, not by preference.
    // `SupportPreparationInput`'s fields are private and its only public
    // constructor is `SupportPreparationLimits::production().input(..)`, which
    // by construction always yields valid caps. There is therefore no public
    // way to build a malformed input, so an external test cannot reach
    // `SupportRunError::InvalidResourceCaps` at all — see
    // `tests/compile_fail/support_preparation_input_*.rs`, which prove exactly
    // that. In-crate struct-update syntax off a production input is the only
    // way to exercise the refusal; adding a public malformed-input constructor
    // would reopen the hole those compile-fail cases seal.
    //
    // Known boundary, recorded rather than widened here: `validate()` guards
    // the eleven values below only. `candidate_config`, `perception_limits`,
    // `specialized_limits`, `construction_limits` and `raft_limits` carry their
    // own caps and are validated by their own constructors, not re-checked at
    // this boundary.
    // ------------------------------------------------------------------

    type CapMutator = for<'a> fn(SupportPreparationInput<'a>, usize) -> SupportPreparationInput<'a>;

    /// The two probes every guarded cap must refuse. `usize::MAX` is refused as
    /// firmly as zero: unbounded is not a budget.
    const CAP_PROBES: [usize; 2] = [0, usize::MAX];

    fn production_input<'a>(
        layers: &'a LayerPlan,
        delicacy_by_face: &'a [f64],
    ) -> SupportPreparationInput<'a> {
        SupportPreparationLimits::production()
            .expect("production support preparation limits")
            .input(layers, delicacy_by_face, &[])
    }

    fn top_level_cap_mutators() -> [(&'static str, CapMutator); 5] {
        [
            ("max_grid_bytes", |base, probe| SupportPreparationInput {
                max_grid_bytes: probe,
                ..base
            }),
            ("coverage_samples_max_virtual_cells", |base, probe| {
                SupportPreparationInput {
                    coverage_samples_max_virtual_cells: probe,
                    ..base
                }
            }),
            ("coverage_score_matrix_max_entries", |base, probe| {
                SupportPreparationInput {
                    coverage_score_matrix_max_entries: probe,
                    ..base
                }
            }),
            ("max_primitives", |base, probe| SupportPreparationInput {
                max_primitives: probe,
                ..base
            }),
            ("max_primitive_bytes", |base, probe| {
                SupportPreparationInput {
                    max_primitive_bytes: probe,
                    ..base
                }
            }),
        ]
    }

    fn nested_penetration_cap_mutators() -> [(&'static str, CapMutator); 6] {
        [
            ("brace_penetration_limits.max_rows", |base, probe| {
                SupportPreparationInput {
                    brace_penetration_limits: PenetrationBatchLimits {
                        max_rows: probe,
                        ..base.brace_penetration_limits
                    },
                    ..base
                }
            }),
            ("brace_penetration_limits.max_total_work", |base, probe| {
                SupportPreparationInput {
                    brace_penetration_limits: PenetrationBatchLimits {
                        max_total_work: probe,
                        ..base.brace_penetration_limits
                    },
                    ..base
                }
            }),
            ("brace_penetration_limits.max_total_hits", |base, probe| {
                SupportPreparationInput {
                    brace_penetration_limits: PenetrationBatchLimits {
                        max_total_hits: probe,
                        ..base.brace_penetration_limits
                    },
                    ..base
                }
            }),
            (
                "brace_penetration_limits.max_total_samples",
                |base, probe| SupportPreparationInput {
                    brace_penetration_limits: PenetrationBatchLimits {
                        max_total_samples: probe,
                        ..base.brace_penetration_limits
                    },
                    ..base
                },
            ),
            (
                "brace_penetration_limits.max_total_intervals",
                |base, probe| SupportPreparationInput {
                    brace_penetration_limits: PenetrationBatchLimits {
                        max_total_intervals: probe,
                        ..base.brace_penetration_limits
                    },
                    ..base
                },
            ),
            (
                "brace_penetration_limits.max_scratch_bytes",
                |base, probe| SupportPreparationInput {
                    brace_penetration_limits: PenetrationBatchLimits {
                        max_scratch_bytes: probe,
                        ..base.brace_penetration_limits
                    },
                    ..base
                },
            ),
        ]
    }

    fn support_request() -> SupportRequest {
        SupportRequest {
            lift_mm: PositiveFinite::new(5.0).expect("positive lift"),
        }
    }

    /// One full `run_support` call with the given cap mutation applied, used
    /// sparingly: the pure `validate()` assertions carry the per-cap table and
    /// this proves only the wiring from that predicate to the typed refusal.
    fn run_support_with_mutated_cap(mutate: CapMutator, probe: usize) -> SupportRunError {
        let hollowed = hollowed_fixture(procedural_overhang_for_error_case());
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        let input = mutate(production_input(&layers, &delicacy), probe);
        run_support(hollowed, support_request(), input)
            .err()
            .expect("malformed caps must refuse the run")
    }

    #[test]
    fn every_top_level_cap_is_refused_at_zero_and_at_usize_max() {
        let layers = layers();
        let base = production_input(&layers, &[]);
        assert!(
            base.validate(),
            "positive control: unmutated production caps must validate"
        );

        for (name, mutate) in top_level_cap_mutators() {
            for probe in CAP_PROBES {
                assert!(
                    !mutate(base, probe).validate(),
                    "{name} = {probe} must be refused"
                );
            }
        }

        // Two representative cases prove the predicate is actually wired to the
        // typed error at the public run boundary.
        let (_, zero_grid_bytes) = top_level_cap_mutators()[0];
        assert!(matches!(
            run_support_with_mutated_cap(zero_grid_bytes, 0),
            SupportRunError::InvalidResourceCaps
        ));
        let (_, unbounded_primitives) = top_level_cap_mutators()[3];
        assert!(matches!(
            run_support_with_mutated_cap(unbounded_primitives, usize::MAX),
            SupportRunError::InvalidResourceCaps
        ));
    }

    #[test]
    fn every_nested_penetration_cap_is_refused_at_zero_and_at_usize_max() {
        let layers = layers();
        let base = production_input(&layers, &[]);

        for (name, mutate) in nested_penetration_cap_mutators() {
            for probe in CAP_PROBES {
                assert!(
                    !mutate(base, probe).validate(),
                    "{name} = {probe} must be refused"
                );
            }
        }

        // The nested caps are the newly guarded surface: the previous inline
        // check never descended into `brace_penetration_limits`.
        let (_, zero_scratch_bytes) = nested_penetration_cap_mutators()[5];
        assert!(matches!(
            run_support_with_mutated_cap(zero_scratch_bytes, 0),
            SupportRunError::InvalidResourceCaps
        ));
    }

    #[test]
    fn usize_max_is_refused_as_firmly_as_zero() {
        let layers = layers();
        let base = production_input(&layers, &[]);

        let unbounded = top_level_cap_mutators()
            .into_iter()
            .chain(nested_penetration_cap_mutators())
            .filter(|(_, mutate)| mutate(base, usize::MAX).validate())
            .map(|(name, _)| name)
            .collect::<Vec<_>>();
        assert!(
            unbounded.is_empty(),
            "these caps accepted an unbounded budget: {unbounded:?}"
        );
        assert_eq!(
            top_level_cap_mutators().len() + nested_penetration_cap_mutators().len(),
            11,
            "the guarded cap table must stay in step with SupportPreparationInput::validate"
        );
    }

    #[test]
    fn production_limits_pass_validation_and_are_thread_setting_inert() {
        let layers = layers();
        assert!(production_input(&layers, &[]).validate());

        let caps = [1, 4, 16].map(|_| {
            let limits = SupportPreparationLimits::production().expect("production limits");
            (
                [
                    limits.max_grid_bytes,
                    limits.coverage_samples_max_virtual_cells,
                    limits.coverage_score_matrix_max_entries,
                    limits.max_primitives,
                    limits.max_primitive_bytes,
                    limits.brace_penetration_limits.max_rows,
                    limits.brace_penetration_limits.max_total_work,
                    limits.brace_penetration_limits.max_total_hits,
                    limits.brace_penetration_limits.max_total_samples,
                    limits.brace_penetration_limits.max_total_intervals,
                    limits.brace_penetration_limits.max_scratch_bytes,
                ],
                [
                    limits.column_pitch_mm.get().to_bits(),
                    limits.surface_grid_pitch_mm.get().to_bits(),
                ],
            )
        });
        assert!(caps.windows(2).all(|pair| pair[0] == pair[1]));
    }

    /// Every production support resource cap that the M0 registry owns is
    /// **read** from the registry, never restated here.
    ///
    /// The defect this pins is two owners for one number. Before the fix,
    /// `production()` read four caps from `SupportResourceCapPolicy` and wrote
    /// the other twenty-two as literals -- literals whose own registry notes
    /// name `SupportPreparationLimits::production` as the site they describe
    /// (`SUPPORT_RAFT_MAX_BYTES`, `SUPPORT_CONSTRUCTION_MAX_CONTACTS`, the six
    /// `SUPPORT_BRACE_PENETRATION_*`, and the rest). Nothing held the two
    /// copies together, and each of these caps can end a run as a refusal.
    ///
    /// This test compares the shipped authority field-for-field against the
    /// registry, so a literal that drifts from its registered owner -- in
    /// either direction -- is a failure rather than a silent divergence.
    #[test]
    fn every_registered_production_resource_cap_is_read_from_the_registry() {
        let limits = SupportPreparationLimits::production().expect("production limits");
        let constants = plinth_core::m0_constants().expect("registered M0 constants");
        let registered = constants.placeholders();
        let cap = |value: &plinth_core::Placeholder<std::num::NonZeroU64>| {
            usize::try_from(value.diagnostic().get()).expect("a registered cap fits usize")
        };

        // The kNN batch shape. `max_work` is the registered floor of the
        // measured per-face rule and is asserted through the scaled-cap
        // policy, not here.
        let knn = limits.perception_limits.minima_knn;
        assert_eq!(
            (
                knn.max_source_points,
                knn.max_query_points,
                knn.max_neighbors,
                knn.max_logical_peak_bytes,
            ),
            (
                cap(registered.support_knn_max_source_points()),
                cap(registered.support_knn_max_query_points()),
                cap(registered.support_knn_max_neighbors()),
                cap(registered.support_knn_max_logical_peak_bytes()),
            ),
            "the production kNN batch shape is the registered one",
        );

        assert_eq!(
            (
                limits.coverage_samples_max_virtual_cells,
                limits.coverage_score_matrix_max_entries,
                limits.max_primitives,
                limits.max_primitive_bytes,
            ),
            (
                cap(registered.support_preparation_coverage_samples_max_virtual_cells()),
                cap(registered.support_preparation_coverage_score_matrix_max_entries()),
                cap(registered.support_preparation_max_primitives()),
                cap(registered.support_preparation_max_primitive_bytes()),
            ),
            "the coverage-sample and emitted-primitive caps are the registered ones",
        );

        assert_eq!(
            limits.construction_limits,
            ContactConstructionLimits::new(
                cap(registered.support_construction_max_contacts()),
                cap(registered.support_construction_max_load_rows()),
                cap(registered.support_construction_max_work_items()),
                cap(registered.support_construction_max_scratch_bytes()),
            )
            .expect("registered construction caps are nonzero"),
            "the initial-contact construction caps are the registered ones",
        );

        assert_eq!(
            limits.brace_penetration_limits,
            PenetrationBatchLimits {
                max_rows: cap(registered.support_brace_penetration_max_rows()),
                max_total_work: cap(registered.support_brace_penetration_max_total_work()),
                max_total_hits: cap(registered.support_brace_penetration_max_total_hits()),
                max_total_samples: cap(registered.support_brace_penetration_max_total_samples()),
                max_total_intervals: cap(registered.support_brace_penetration_max_total_intervals()),
                max_scratch_bytes: cap(registered.support_brace_penetration_max_scratch_bytes()),
            },
            "the brace penetration-batch caps are the registered ones",
        );

        assert_eq!(
            limits.raft_limits,
            RaftLimits::try_new(
                cap(registered.support_raft_max_candidates()),
                cap(registered.support_raft_max_roots()),
                cap(registered.support_raft_max_primitives()),
                cap(registered.support_raft_max_bytes()),
            )
            .expect("registered raft caps are nonzero"),
            "the raft-plate caps are the registered ones",
        );
    }

    #[test]
    fn refusal_precedes_any_geometry_work() {
        // Baseline: with valid caps this short layer plan fails later, during
        // preparation, with a distinguishable typed error.
        let hollowed = hollowed_fixture(procedural_overhang_for_error_case());
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let short_layers = LayerPlan::new(0.0, vec![0.05; 10]).expect("short layer plan");
        assert!(matches!(
            run_support(
                hollowed,
                support_request(),
                production_input(&short_layers, &delicacy),
            ),
            Err(SupportRunError::Failed {
                phase: SupportRunPhase::Preparation,
                ..
            })
        ));

        // Same model and same doomed layer plan, malformed caps: the cap
        // refusal wins, so it must have happened before any of that work.
        let hollowed = hollowed_fixture(procedural_overhang_for_error_case());
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let (_, zero_grid_bytes) = top_level_cap_mutators()[0];
        let input = zero_grid_bytes(production_input(&short_layers, &delicacy), 0);
        assert!(matches!(
            run_support(hollowed, support_request(), input),
            Err(SupportRunError::InvalidResourceCaps)
        ));
    }

    /// **FU-truthful-errors, ported to the one-winner controller at §G.1 stage
    /// 4 — INTENT UNCHANGED, and one half of it re-derived rather than
    /// re-asserted.**
    ///
    /// The retired `repair_plan_or_stage_failure` used to discard `error`
    /// outright whenever `attempt == 0`, returning a bare
    /// `SupportRunError::Failed(phase)` that could not say what actually
    /// failed — exactly the attempt most likely to hold the real bug, because
    /// every OTHER attempt already kept its error. The claim under test is the
    /// same one it always was: **attempt 0 keeps the same detail a repair
    /// attempt does**, so a future edit that reintroduces the discard fails
    /// here instead of only being found by re-running hours of instrumentation
    /// on real models.
    ///
    /// **What moved, and why it is not a weaker test.** Under the batch loop
    /// the attempt-≥1 half of the pair produced a
    /// `CoverageClosureReason::RepairPlanRefused` that KILLED THE MODEL. Under
    /// one-winner transactions the same stage failure is a ledgered rollback
    /// and the search goes on (§A.5, task #47), so the second projection is
    /// `RepairStepOutcome::StageRefusal` rather than a whole-run refusal. That
    /// is the from→to: same phase, same detail, different consequence — and the
    /// consequence changing is the fix, not a regression.
    ///
    /// Both projections are read off the ONE value `run_stage_chain` produces,
    /// so a future edit that lets the two arms disagree about the detail cannot
    /// pass.
    /// **The abandon rule — design §4 Stage B revert proof 3.**
    ///
    /// # Why this test has to exist
    ///
    /// The legalization loop is UNCAPPED, on Craig's authorization item 4, and
    /// it should be: a cap would answer "how long may this search run" with a
    /// number nobody measured. What makes an uncapped loop terminate instead is
    /// a real invariant — *every abandoned pass added at least one no-good that
    /// was not already in the set* — plus the finiteness obligation that keeps
    /// the set out of every context digest.
    ///
    /// So the loop must refuse, by name, on a pass that abandoned while
    /// learning nothing. Without this rule the same candidate is chosen and
    /// condemned forever and the run never ends, never emits and never says
    /// why. With it, a stuck loop stops on its second pass with the terminals
    /// named.
    ///
    /// **Sabotage-shaped:** drop the `grew` check and this test goes red on the
    /// `Ok`. It fires on a pass that made no progress and never on one that
    /// made some, which is why it is a refusal and not a cap.
    #[test]
    fn an_abandoned_pass_that_learns_nothing_new_refuses_instead_of_looping() {
        let no_good = crate::legalization::NoGood::new(
            113,
            prefix_fixture(4),
            crate::legalization::ArmIdentity::live(
                crate::cco::CandidateFamily::PlateDescent,
                7,
                1.25,
                3,
            ),
        );
        let condemned = || crate::legalization::LegalizationAbandon::Condemned {
            learned: vec![no_good],
            own_arm_terminals: vec![113],
            cascade_victims: Vec::new(),
            measured: vec![crate::legalization::ArmRefusalPhysics {
                gap_mm: -0.05,
                distance_clearance_mm: 0.280_044,
                axis_transit_illegal: true,
                terminal_radius_mm: 0.372_136,
                rootward_radius_mm: 0.372_136,
            }],
        };

        let mut steering = crate::legalization::LegalizationSteering::default();
        let mut tally = LegalizationTally {
            passes: 1,
            ..LegalizationTally::default()
        };
        tally
            .apply(&mut steering, condemned())
            .expect("the first pass learns a no-good the set did not have, so it may retry");
        assert_eq!(steering.len(), 1);

        // The identical refusal a second time. The world did not move, so
        // another pass would choose and condemn the identical arm forever.
        tally.passes = 2;
        let refusal = tally
            .apply(&mut steering, condemned())
            .expect_err("a pass that abandoned while learning nothing must refuse by name");
        let SupportRunError::Failed { phase, detail } = refusal else {
            panic!("the abandon rule must refuse as a named stage failure, got {refusal:?}");
        };
        assert_eq!(phase, SupportRunPhase::Brace);
        assert!(
            detail.contains("without learning a new no-good") && detail.contains("113"),
            "the refusal must name the rule and the terminals it was about: {detail}"
        );
        assert_eq!(steering.len(), 1, "a refused pass adds nothing");
    }

    /// A pass whose whole disturbed class is cascade victims learns nothing it
    /// can steer with, so it refuses by the same rule rather than retrying
    /// against an unchanged set. Victims steer nothing — §1.3 — and "nothing"
    /// has to include "nothing that licenses another pass".
    #[test]
    fn a_pass_that_disturbed_only_cascade_victims_refuses_rather_than_retrying() {
        let mut steering = crate::legalization::LegalizationSteering::default();
        let mut tally = LegalizationTally {
            passes: 1,
            ..LegalizationTally::default()
        };
        let refusal = tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::Condemned {
                    learned: Vec::new(),
                    own_arm_terminals: Vec::new(),
                    cascade_victims: vec![7, 9],
                    measured: Vec::new(),
                },
            )
            .expect_err("victims steer nothing, so this pass learned nothing");
        assert!(
            format!("{refusal:?}").contains("cascade victims [7, 9]"),
            "the refusal must name the victims: {refusal:?}"
        );
        assert_eq!(steering.len(), 0);
    }

    /// **A spent steering opens a round; it never retires anything — design
    /// §1.3 and §4 Stage B piece 5.**
    ///
    /// # Why this test has to exist
    ///
    /// The previous revision of the design let `k` recorded no-goods retire a
    /// terminal, on the argument that `k` measured negative margins are better
    /// evidence than an arithmetic count. They are better evidence of the wrong
    /// proposition: each margin was measured in the world that existed when its
    /// pass ran, and every abandoned pass replaces that world, so `k` true
    /// measurements can be harvested from `k` different worlds and still say
    /// nothing about whether an arm is legal in THIS one. That is the finding
    /// that made the second review return FLAWED.
    ///
    /// So the only thing a spent steering may do is open a round, and the round
    /// is what earns a retirement by re-measuring.
    ///
    /// **Sabotage-shaped:** make the spent branch retire, or count no-goods,
    /// and this goes red on the open round.
    #[test]
    fn a_spent_steering_opens_a_round_at_rank_zero_and_retires_nothing() {
        let mut steering = crate::legalization::LegalizationSteering::default();
        let mut tally = LegalizationTally {
            passes: 4,
            ..LegalizationTally::default()
        };
        tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::SteeringSpent { terminal_id: 113 },
            )
            .expect("a spent steering opens a round rather than stopping the run");
        let round = steering
            .round()
            .expect("the round is the only thing allowed to earn a retirement");
        assert_eq!(round.terminal_id, 113);
        assert_eq!(
            round.arm_rank, 0,
            "a round starts at the tournament's first arm"
        );
        assert!(
            round.measured.is_empty(),
            "an opening round has measured nothing; a retirement must be earned by rows this \
             round actually took"
        );
        assert_eq!(tally.confirmation_rounds, 1);
        assert_eq!(tally.voided_rounds, 0);
    }

    /// **A round's own refusals are recorded AGAINST THE ROUND, never added to
    /// the steering set — design §1.3.**
    ///
    /// # Why this test has to exist
    ///
    /// The set is held fixed for the round's duration, and that is what makes
    /// the round replayable: "arm rank k" means the same arm on every pass with
    /// the same prefix. If a round's own refusals fed the set, the slate would
    /// shrink underneath the round it is being counted through, the ranks would
    /// stop naming the same arms, and the round would be measuring a moving
    /// target while claiming to be one round.
    ///
    /// The rank advancing is also what makes a round pass count as PROGRESS
    /// under the abandon rule: it measured an arm nobody had measured in this
    /// world, even though it grew no set.
    #[test]
    fn a_rounds_own_refusal_advances_the_rank_and_leaves_the_steering_set_alone() {
        let arm = crate::legalization::ArmIdentity::live(
            crate::cco::CandidateFamily::ModelAnchor,
            98,
            2.5,
            1,
        );
        let no_good = crate::legalization::NoGood::new(113, prefix_fixture(4), arm);
        let mut steering = crate::legalization::LegalizationSteering::default();
        steering.set_round(Some(crate::legalization::ConfirmationRound {
            terminal_id: 113,
            arm_rank: 0,
            measured: Vec::new(),
        }));
        let mut tally = LegalizationTally::default();

        tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::Condemned {
                    learned: vec![no_good],
                    own_arm_terminals: vec![113],
                    cascade_victims: Vec::new(),
                    measured: vec![crate::legalization::ArmRefusalPhysics {
                        gap_mm: -0.05,
                        distance_clearance_mm: 0.280_044,
                        axis_transit_illegal: true,
                        terminal_radius_mm: 0.372_136,
                        rootward_radius_mm: 0.372_136,
                    }],
                },
            )
            .expect("a round measuring its own arm is progress, so the pass may go on");

        assert_eq!(
            steering.len(),
            0,
            "a round's refusal must NOT enter the steering set; the set is frozen for the \
             round's duration and that is what makes the ranks name the same arms"
        );
        let round = steering.round().expect("the round is still open");
        assert_eq!(round.arm_rank, 1, "the round advanced to its next arm");
        assert_eq!(round.measured.len(), 1);
        assert_eq!(round.measured[0].rank, 0);
        assert_eq!(round.measured[0].arm, arm);
        assert!(round.measured[0].axis_transit_illegal);
        assert!((round.measured[0].distance_clearance_mm - 0.280_044).abs() < 1e-9);
        assert_eq!(tally.round_arms_measured, 1);
        assert_eq!(tally.voided_rounds, 0);
    }

    /// **Another terminal's arm condemned during a round VOIDS it — design
    /// §1.3.**
    ///
    /// # Why this test has to exist
    ///
    /// The round's claim is *"nothing this terminal could have chosen was
    /// legal, in one world"*. If some other terminal's own arm is condemned
    /// while the round runs, the world moved for a reason that was not this
    /// terminal's choice, so the arms measured so far were measured in worlds
    /// the remaining ones will not be measured in. Counting them as one round
    /// would be exactly the harvest-from-k-worlds error the round exists to
    /// prevent — with a round's name on it.
    ///
    /// A void round retires nothing, is reported as void, and is re-entered
    /// later against the new set.
    #[test]
    fn another_terminals_refusal_during_a_round_voids_it_and_retires_nothing() {
        let mut steering = crate::legalization::LegalizationSteering::default();
        steering.set_round(Some(crate::legalization::ConfirmationRound {
            terminal_id: 113,
            arm_rank: 2,
            measured: Vec::new(),
        }));
        let mut tally = LegalizationTally::default();

        tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::Condemned {
                    learned: vec![crate::legalization::NoGood::new(
                        177,
                        prefix_fixture(9),
                        crate::legalization::ArmIdentity::live(
                            crate::cco::CandidateFamily::PlateDescent,
                            3,
                            1.0,
                            2,
                        ),
                    )],
                    own_arm_terminals: vec![177],
                    cascade_victims: Vec::new(),
                    measured: vec![crate::legalization::ArmRefusalPhysics {
                        gap_mm: -0.2,
                        distance_clearance_mm: -0.2,
                        axis_transit_illegal: false,
                        terminal_radius_mm: 0.4,
                        rootward_radius_mm: 0.4,
                    }],
                },
            )
            .expect("the void is not a failure; the pass abandons on the new no-good as usual");

        assert!(
            steering.round().is_none(),
            "the round is void: the world moved for a reason other than terminal 113's own \
             choice"
        );
        assert_eq!(tally.voided_rounds, 1);
        assert_eq!(
            steering.len(),
            1,
            "the OTHER terminal's refusal is an ordinary steering no-good and does enter the set"
        );
        assert_eq!(
            tally.round_arms_measured, 0,
            "a voided round measured nothing that counts; it is paid for and earns nothing"
        );
    }

    /// **A MIXED pass — the round advances and another terminal's arm is
    /// condemned in the same pass — keeps the arm it measured and still voids.**
    ///
    /// # Why this test has to exist
    ///
    /// The abandon rule now runs BEFORE the void block rather than after it, so
    /// that a refusing pass destroys nothing on its way out. That reorder puts
    /// three mutations in one pass in a fixed order — the round's own arm is
    /// recorded inside the loop, the rule is read, then the round is voided —
    /// and the mixed pass is the only shape where all three happen at once. If
    /// the void ever moved ahead of the loop's bookkeeping, the arm this pass
    /// measured would vanish from the round's row and the run would under-report
    /// what it spent, silently, on the one pass that did the most work.
    ///
    /// The behaviour itself is pre-existing and unchanged: the arm is measured,
    /// the row keeps it, the round is voided by the OTHER terminal's refusal, and
    /// the pass goes on because it learned a no-good the set did not have.
    ///
    /// **Sabotage-shaped:** move the void block above the `for` loop and the
    /// `arms_measured` reading goes to 0 while everything else stays green.
    #[test]
    fn a_pass_that_both_advances_a_round_and_voids_it_keeps_the_arm_it_measured() {
        let mut steering = crate::legalization::LegalizationSteering::default();
        let mut tally = LegalizationTally {
            passes: 1,
            ..LegalizationTally::default()
        };
        tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::SteeringSpent { terminal_id: 113 },
            )
            .expect("a spent steering opens a round");
        tally.passes = 2;
        tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::Condemned {
                    learned: vec![
                        crate::legalization::NoGood::new(
                            113,
                            prefix_fixture(40),
                            crate::legalization::ArmIdentity::live(
                                crate::cco::CandidateFamily::ModelAnchor,
                                0,
                                2.5,
                                1,
                            ),
                        ),
                        crate::legalization::NoGood::new(
                            177,
                            prefix_fixture(9),
                            crate::legalization::ArmIdentity::live(
                                crate::cco::CandidateFamily::PlateDescent,
                                3,
                                1.0,
                                2,
                            ),
                        ),
                    ],
                    own_arm_terminals: vec![113, 177],
                    cascade_victims: Vec::new(),
                    measured: vec![
                        crate::legalization::ArmRefusalPhysics {
                            gap_mm: -0.05,
                            distance_clearance_mm: 0.280_044,
                            axis_transit_illegal: true,
                            terminal_radius_mm: 0.372_136,
                            rootward_radius_mm: 0.372_136,
                        },
                        crate::legalization::ArmRefusalPhysics {
                            gap_mm: -0.2,
                            distance_clearance_mm: -0.2,
                            axis_transit_illegal: false,
                            terminal_radius_mm: 0.4,
                            rootward_radius_mm: 0.4,
                        },
                    ],
                },
            )
            .expect("the pass learned a no-good the set did not have, so it may retry");

        assert!(steering.round().is_none(), "the other terminal voided it");
        assert_eq!(tally.voided_rounds, 1);
        assert_eq!(
            tally.rounds[0].arms_measured, 1,
            "the arm this pass measured is on the round's row; the void may not erase what \
             the run already spent"
        );
        assert_eq!(
            tally.rounds[0].outcome,
            Some(crate::coverage_repair::ConfirmationRoundOutcome::Voided)
        );
        assert_eq!(tally.round_arms_measured, 1);
        assert_eq!(
            steering.len(),
            1,
            "the round's own refusal stays out of the frozen set; the other terminal's enters it"
        );
        let disturbance = tally
            .disturbances
            .last()
            .expect("the pass continued, so it left a row");
        assert!(disturbance.round_arm_measured, "it advanced a round");
        assert!(disturbance.voided_a_round, "and it voided one");
        assert_eq!(disturbance.no_goods_learned, 1);
    }

    /// **Voiding a round is not progress: a pass that only voids REFUSES, and
    /// the round it would have destroyed is left standing.**
    ///
    /// # Why this test has to exist
    ///
    /// The abandon rule's own sentence is *"what is forbidden is a pass that did
    /// neither"* — neither learned a no-good nor advanced a round. The first
    /// version of the rule carried a third escape: a pass that voided a round
    /// was accepted even when it learned nothing, which is the same hole the
    /// spent arm carried, one arm over. It cycles the same way: void the round,
    /// re-open it against the unchanged set, void it again.
    ///
    /// Reaching it needs a defect rather than hard physics — a terminal's own
    /// arm condemned while the exact `(terminal, prefix, arm)` triple that
    /// should have suppressed it is already in the set — which is precisely why
    /// it must refuse by name instead of spinning: a defect that announces
    /// itself is cheaper than one that looks like a slow model.
    ///
    /// **Sabotage-shaped:** put `&& !voided` back on the rule and this goes red
    /// on the `expect_err`; move the rule back below the void block and it goes
    /// red on `voided_rounds`, which is the half that says a refusing pass
    /// destroys nothing on its way out.
    #[test]
    fn a_pass_that_only_voids_a_round_refuses_and_leaves_the_round_standing() {
        let elsewhere = crate::legalization::NoGood::new(
            177,
            prefix_fixture(9),
            crate::legalization::ArmIdentity::live(
                crate::cco::CandidateFamily::PlateDescent,
                3,
                1.0,
                2,
            ),
        );
        let condemned_elsewhere = || crate::legalization::LegalizationAbandon::Condemned {
            learned: vec![elsewhere],
            own_arm_terminals: vec![177],
            cascade_victims: Vec::new(),
            measured: vec![crate::legalization::ArmRefusalPhysics {
                gap_mm: -0.2,
                distance_clearance_mm: -0.2,
                axis_transit_illegal: false,
                terminal_radius_mm: 0.4,
                rootward_radius_mm: 0.4,
            }],
        };
        let mut steering = crate::legalization::LegalizationSteering::default();
        let mut tally = LegalizationTally {
            passes: 1,
            ..LegalizationTally::default()
        };
        tally
            .apply(&mut steering, condemned_elsewhere())
            .expect("the first pass learns a no-good the set did not have");
        tally.passes = 2;
        tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::SteeringSpent { terminal_id: 113 },
            )
            .expect("a spent steering opens a round");

        // The identical refusal, now with a round open. The set cannot grow on
        // it, so the only thing this pass would do is destroy the round.
        tally.passes = 3;
        let refusal = tally
            .apply(&mut steering, condemned_elsewhere())
            .expect_err("a pass whose only effect is a void made no progress and must refuse");
        let SupportRunError::Failed { phase, detail } = &refusal else {
            panic!("the abandon rule refuses as a named stage failure, not {refusal:?}");
        };
        assert_eq!(*phase, SupportRunPhase::Brace);
        assert!(
            detail.contains("made no progress") && detail.contains("voided an open round: true"),
            "the refusal must say the pass would have voided a round and still made no \
             progress: {detail}"
        );

        assert_eq!(
            tally.voided_rounds, 0,
            "a refusing pass destroys nothing on its way out — the rule is read BEFORE the void"
        );
        let round = steering
            .round()
            .expect("the round is still open; the refusal did not void it");
        assert_eq!(round.terminal_id, 113);
        assert_eq!(round.arm_rank, 0);
        assert!(
            tally.rounds[0].outcome.is_none(),
            "the round's row is still open, not stamped Voided"
        );
        assert_eq!(
            tally.disturbances.len(),
            2,
            "pass 1 learned and pass 2 opened a round; pass 3 refused and wrote nothing"
        );
        assert_eq!(steering.len(), 1, "a refused pass adds nothing");
    }

    /// **Stage D — a run that never legalized discloses ZEROS, not silence.**
    ///
    /// # Why this test has to exist
    ///
    /// The attempt-0 loop runs on every model, so `passes` is at least 1 on
    /// every live run. That is the only thing separating "this model needed no
    /// rescue" from "this file was written before Stage D existed", where
    /// `#[serde(default)]` supplies a 0. Writing nothing for the quiet model,
    /// or normalizing the 0 away, destroys the discriminator — and this is the
    /// same rule `persist_sealed_repair_ledger` already follows when it writes
    /// a ledger of zeros rather than no file.
    #[test]
    fn a_run_that_never_legalized_discloses_one_pass_and_no_rounds() {
        let steering = crate::legalization::LegalizationSteering::default();
        let tally = LegalizationTally {
            passes: 1,
            ..LegalizationTally::default()
        };
        let disclosure = tally.disclose(&steering, &BTreeSet::new());
        assert_eq!(disclosure.passes, 1, "the attempt-0 loop always runs once");
        assert_eq!(disclosure.no_goods, 0);
        assert!(disclosure.no_goods_by_terminal.is_empty());
        assert_eq!(disclosure.confirmation_rounds, 0);
        assert_eq!(disclosure.round_arms_measured, 0);
        assert!(disclosure.disturbances.is_empty());
        assert!(disclosure.rounds.is_empty());
        assert_ne!(
            disclosure,
            crate::coverage_repair::LegalizationDisclosure::default(),
            "a live run must be distinguishable from a file that predates Stage D, and \
             `passes` is the whole of that distinction"
        );
    }

    /// **Stage D — the round row carries the terminal, the length and where
    /// that terminal ended, and it is built through the same `apply` the loop
    /// drives.**
    ///
    /// # Why this test has to exist
    ///
    /// Design §4 Stage D asks the sidecar to disclose *which terminal was
    /// disturbed, the round length, and the outcome*. The length is the number
    /// of arms the round actually drove — read off the round's own `measured`
    /// rows so there is one source, not a parallel counter that agrees today.
    /// The outcome is decided by a read of the LOWERED run's sealed transcript,
    /// which is why both readings are asserted here from the same tally.
    #[test]
    fn the_round_row_carries_the_terminal_the_length_and_where_it_ended() {
        let mut steering = crate::legalization::LegalizationSteering::default();
        let mut tally = LegalizationTally {
            passes: 3,
            ..LegalizationTally::default()
        };
        tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::SteeringSpent { terminal_id: 113 },
            )
            .expect("a spent steering opens a round");
        tally.passes = 4;
        for rank in 0..2 {
            tally
                .apply(
                    &mut steering,
                    crate::legalization::LegalizationAbandon::Condemned {
                        learned: vec![crate::legalization::NoGood::new(
                            113,
                            prefix_fixture(40 + rank),
                            crate::legalization::ArmIdentity::live(
                                crate::cco::CandidateFamily::ModelAnchor,
                                usize::try_from(rank).expect("rank fits"),
                                2.5,
                                1,
                            ),
                        )],
                        own_arm_terminals: vec![113],
                        cascade_victims: Vec::new(),
                        measured: vec![crate::legalization::ArmRefusalPhysics {
                            gap_mm: -0.05,
                            distance_clearance_mm: 0.280_044,
                            axis_transit_illegal: true,
                            terminal_radius_mm: 0.372_136,
                            rootward_radius_mm: 0.372_136,
                        }],
                    },
                )
                .expect("a round measuring its own arm is progress");
            tally.passes += 1;
        }

        let retired = tally.disclose(&steering, &BTreeSet::from([113]));
        assert_eq!(retired.rounds.len(), 1, "one round opened, one row");
        let row = &retired.rounds[0];
        assert_eq!(row.terminal_id, 113);
        assert_eq!(row.opened_at_pass, 3);
        assert_eq!(
            row.arms_measured, 2,
            "the round LENGTH is the arms it drove, read off the round's own rows"
        );
        assert_eq!(
            row.no_goods_at_open, 0,
            "the set the round measured against is the one frozen when it opened"
        );
        assert_eq!(
            row.outcome,
            crate::coverage_repair::ConfirmationRoundOutcome::TerminalUnroutedAtLowering
        );
        assert_eq!(retired.round_arms_measured, 2);
        assert_eq!(
            retired.no_goods, 0,
            "a round's own refusals never enter the steering set"
        );

        let routed = tally.disclose(&steering, &BTreeSet::new());
        assert_eq!(
            routed.rounds[0].outcome,
            crate::coverage_repair::ConfirmationRoundOutcome::TerminalRoutedAtLowering,
            "the same round, with the terminal absent from the sealed unrouted list, ended \
             the other way — the forced arm survived the brace pass"
        );
    }

    /// **Stage D — a VOIDED round keeps its void outcome, whatever the sealed
    /// transcript later says about its terminal.**
    ///
    /// # Why this test has to exist
    ///
    /// A void round retires nothing (§1.3). Its terminal can still end the run
    /// `UnroutedNoWinner` for one of the three older reasons, and a disclosure
    /// that let the transcript read overwrite the void would report a round
    /// that ended in a retirement it is forbidden to have earned.
    ///
    /// **Sabotage-shaped:** drop the `outcome.unwrap_or(...)` guard so the
    /// transcript read wins, and this goes red on a round whose terminal is in
    /// the unrouted set.
    #[test]
    fn a_voided_round_keeps_its_void_outcome_even_when_its_terminal_ends_unrouted() {
        let mut steering = crate::legalization::LegalizationSteering::default();
        let mut tally = LegalizationTally {
            passes: 2,
            ..LegalizationTally::default()
        };
        tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::SteeringSpent { terminal_id: 113 },
            )
            .expect("a spent steering opens a round");
        tally.passes = 3;
        tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::Condemned {
                    learned: vec![crate::legalization::NoGood::new(
                        177,
                        prefix_fixture(9),
                        crate::legalization::ArmIdentity::live(
                            crate::cco::CandidateFamily::PlateDescent,
                            3,
                            1.0,
                            2,
                        ),
                    )],
                    own_arm_terminals: vec![177],
                    cascade_victims: Vec::new(),
                    measured: vec![crate::legalization::ArmRefusalPhysics {
                        gap_mm: -0.2,
                        distance_clearance_mm: -0.2,
                        axis_transit_illegal: false,
                        terminal_radius_mm: 0.4,
                        rootward_radius_mm: 0.4,
                    }],
                },
            )
            .expect("the void is not a failure");

        let disclosure = tally.disclose(&steering, &BTreeSet::from([113, 177]));
        assert_eq!(disclosure.voided_rounds, 1);
        assert_eq!(
            disclosure.rounds[0].outcome,
            crate::coverage_repair::ConfirmationRoundOutcome::Voided,
            "a void round earned nothing, and the transcript may not re-label it"
        );
        assert_eq!(disclosure.rounds[0].arms_measured, 0);
    }

    /// **A second spent terminal REFUSES BY NAME; it may not displace an open
    /// round.**
    ///
    /// # Why this test has to exist
    ///
    /// This is the loop's termination invariant, and it used to hold on one arm
    /// of `apply` only. `Condemned` enforced it; `SteeringSpent` voided the open
    /// round, re-opened at rank 0 with `measured` empty and returned `Ok`, so a
    /// pass that learned no no-good AND advanced no round was accepted as
    /// progress.
    ///
    /// It is a true fixed point rather than a slow path. Take terminals T and U,
    /// T before U in the cadence, both with a no-good on every arm. Pass A: no
    /// round, T reports spent, round(T) opens. Pass B: T is forced and routes —
    /// a round suppresses nothing — the cadence reaches U, U reports spent,
    /// round(T) is voided and round(U) opens at rank 0. Pass C: T is no longer
    /// forced, T reports spent first, round(U) is voided and round(T) re-opens
    /// at rank 0. Pass D is pass B, in the same world: the steering set never
    /// grew, the routing cadence is bit-identical across replays (precondition
    /// S-1), and no round ever measures an arm. Production has no wall clock, so
    /// the model would hang with no artifact and no named reason.
    ///
    /// Design §1.3 admits two ways into a round — *"a context whose steering set
    /// is full, or re-entered after the set grew, both of which are finite
    /// events"* — and the void rule it states covers only another terminal's arm
    /// being REFUSED. A displacement is neither, so it refuses.
    ///
    /// **Sabotage-shaped:** put the void-and-re-open back in place of the
    /// refusal and this goes red on the `expect_err`; delete only the tally
    /// assertions' subject — bump `confirmation_rounds` before returning the
    /// error — and it goes red on the second half, which is the half that says a
    /// refused pass leaves no row behind.
    #[test]
    fn a_second_spent_terminal_refuses_by_name_instead_of_displacing_an_open_round() {
        let mut steering = crate::legalization::LegalizationSteering::default();
        let mut tally = LegalizationTally {
            passes: 3,
            ..LegalizationTally::default()
        };
        tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::SteeringSpent { terminal_id: 113 },
            )
            .expect("a spent steering opens a round when none is open");
        tally.passes = 4;
        tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::Condemned {
                    learned: vec![crate::legalization::NoGood::new(
                        113,
                        prefix_fixture(40),
                        crate::legalization::ArmIdentity::live(
                            crate::cco::CandidateFamily::ModelAnchor,
                            0,
                            2.5,
                            1,
                        ),
                    )],
                    own_arm_terminals: vec![113],
                    cascade_victims: Vec::new(),
                    measured: vec![crate::legalization::ArmRefusalPhysics {
                        gap_mm: -0.05,
                        distance_clearance_mm: 0.280_044,
                        axis_transit_illegal: true,
                        terminal_radius_mm: 0.372_136,
                        rootward_radius_mm: 0.372_136,
                    }],
                },
            )
            .expect("the round measured its own arm, which is progress");
        tally.passes = 5;

        let error = tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::SteeringSpent { terminal_id: 177 },
            )
            .expect_err("a spent report that would displace an open round must refuse by name");
        let SupportRunError::Failed { phase, detail } = &error else {
            panic!("the refusal is a named stage failure, not {error:?}");
        };
        assert_eq!(*phase, SupportRunPhase::Routing);
        for named in ["pass 5", "terminal 113", "rank 1", "terminal 177"] {
            assert!(
                detail.contains(named),
                "refusing BY NAME means a reader learns who held whom; {named:?} is missing \
                 from {detail:?}"
            );
        }

        // **The refusal leaves no row behind**, which is the same rule the
        // `Condemned` arm states: a pass that never continued may not appear in
        // the record as one that did.
        assert_eq!(
            tally.confirmation_rounds, 1,
            "no round opened on the refusing pass"
        );
        assert_eq!(tally.voided_rounds, 0, "no round was voided; none may be");
        assert_eq!(tally.rounds.len(), 1, "one round row, from pass 3");
        assert_eq!(tally.rounds[0].terminal_id, 113);
        assert!(
            tally.rounds[0].outcome.is_none(),
            "the open round is still open — the refusal did not close it"
        );
        assert_eq!(
            tally.disturbances.len(),
            2,
            "pass 3 opened a round and pass 4 measured an arm; pass 5 refused and wrote nothing"
        );
        assert_eq!(tally.round_arms_measured, 1);
        let round = steering
            .round()
            .expect("the open round is untouched by the refusal");
        assert_eq!(round.terminal_id, 113);
        assert_eq!(round.arm_rank, 1);
        assert_eq!(round.measured.len(), 1);
        assert_eq!(
            steering.len(),
            0,
            "a round's own refusal never enters the set, and a spent report carries no arm to \
             learn — which is exactly why this pass had to refuse"
        );
    }

    /// **A voided round keeps the arms it measured; the round that replaces it
    /// starts from zero; and the run-level counter is the sum of both.**
    ///
    /// # Why this test has to exist
    ///
    /// Design §1.3 obligation 3 is PER ROUND: *"the number of passes attributed
    /// to that round must equal the number of arms it measured"*. Three
    /// quantities look like they could disagree after a void, and all three are
    /// asserted here at once so nobody has to reason about which one a
    /// disclosure meant:
    ///
    /// * the voided round's own row keeps its 2 arms — they were paid for;
    /// * the re-entered round starts at rank 0 with nothing measured, because
    ///   the set it measures against is a different one and its earlier
    ///   measurements were taken in worlds it will not be measured in again;
    /// * `round_arms_measured` is the RUN's cost across all rounds, voided ones
    ///   included, and so reads 2 while the open round names none.
    ///
    /// The re-entry is legal precisely because the set GREW — `no_goods_at_open`
    /// is asserted to prove it, and that is what makes the re-entry one of §1.3's
    /// finite events rather than a second way to spin.
    ///
    /// **Sabotage-shaped:** make `void_open_round` clear the row's
    /// `arms_measured`, or make the re-opened round inherit the voided round's
    /// `measured` rows, and one of the three readings goes red while the other
    /// two stay green.
    #[test]
    fn a_voided_round_keeps_its_arms_and_the_re_entered_round_starts_from_zero() {
        let mut steering = crate::legalization::LegalizationSteering::default();
        let mut tally = LegalizationTally {
            passes: 1,
            ..LegalizationTally::default()
        };
        tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::SteeringSpent { terminal_id: 113 },
            )
            .expect("a spent steering opens a round");
        for rank in 0..2_usize {
            tally.passes += 1;
            tally
                .apply(
                    &mut steering,
                    crate::legalization::LegalizationAbandon::Condemned {
                        learned: vec![crate::legalization::NoGood::new(
                            113,
                            prefix_fixture(40 + u64::try_from(rank).expect("rank fits")),
                            crate::legalization::ArmIdentity::live(
                                crate::cco::CandidateFamily::ModelAnchor,
                                rank,
                                2.5,
                                1,
                            ),
                        )],
                        own_arm_terminals: vec![113],
                        cascade_victims: Vec::new(),
                        measured: vec![crate::legalization::ArmRefusalPhysics {
                            gap_mm: -0.05,
                            distance_clearance_mm: 0.280_044,
                            axis_transit_illegal: true,
                            terminal_radius_mm: 0.372_136,
                            rootward_radius_mm: 0.372_136,
                        }],
                    },
                )
                .expect("the round measured its own arm, which is progress");
        }
        tally.passes += 1;
        tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::Condemned {
                    learned: vec![crate::legalization::NoGood::new(
                        177,
                        prefix_fixture(9),
                        crate::legalization::ArmIdentity::live(
                            crate::cco::CandidateFamily::PlateDescent,
                            3,
                            1.0,
                            2,
                        ),
                    )],
                    own_arm_terminals: vec![177],
                    cascade_victims: Vec::new(),
                    measured: vec![crate::legalization::ArmRefusalPhysics {
                        gap_mm: -0.2,
                        distance_clearance_mm: -0.2,
                        axis_transit_illegal: false,
                        terminal_radius_mm: 0.4,
                        rootward_radius_mm: 0.4,
                    }],
                },
            )
            .expect(
                "another terminal's own arm voids the round; the set grows and the pass goes on",
            );
        assert!(steering.round().is_none(), "the round is void");
        tally.passes += 1;
        tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::SteeringSpent { terminal_id: 113 },
            )
            .expect(
                "the same terminal re-enters a round against the GROWN set — §1.3's second, \
                 finite way in, and no round is open to displace",
            );

        let disclosure = tally.disclose(&steering, &BTreeSet::from([113]));
        assert_eq!(disclosure.rounds.len(), 2, "one voided, one re-entered");
        assert_eq!(
            disclosure.rounds[0].arms_measured, 2,
            "the voided round keeps the arms it drove; they were paid for"
        );
        assert_eq!(
            disclosure.rounds[0].outcome,
            crate::coverage_repair::ConfirmationRoundOutcome::Voided
        );
        assert_eq!(
            disclosure.rounds[0].no_goods_at_open, 0,
            "the first round froze an empty set"
        );
        assert_eq!(
            disclosure.rounds[1].arms_measured, 0,
            "the re-entered round has measured nothing yet, and it is the only thing a \
             retirement of terminal 113 could rest on"
        );
        assert_eq!(
            disclosure.rounds[1].no_goods_at_open, 1,
            "the re-entry is legal because the SET GREW — that is what makes it finite"
        );
        assert_eq!(
            disclosure.round_arms_measured, 2,
            "the run-level counter is a COST across all rounds, voided ones included; it is \
             not the open round's length and no retirement may read it"
        );
        assert_eq!(disclosure.voided_rounds, 1);
        assert_eq!(disclosure.confirmation_rounds, 2);
        let round = steering.round().expect("the re-entered round is open");
        assert_eq!(round.arm_rank, 0);
        assert!(
            round.measured.is_empty(),
            "the re-entered round inherits nothing: its arms are re-measured under the new set"
        );
    }

    /// **Stage D — each abandoned pass discloses WHICH terminal was disturbed,
    /// and keeps Stage A's own-arm / cascade-victim split.**
    ///
    /// # Why this test has to exist
    ///
    /// Stage A's whole result is that the mismatch class is reported in full
    /// and that a cascade victim steers nothing. A disclosure that folded the
    /// two lists into one count would put back exactly the confusion Stage A
    /// removed: a reader could not tell a one-terminal class from a cascade,
    /// which is the difference between "this model's physics is hard" and
    /// "one re-parenting disturbed a crowd".
    #[test]
    fn each_abandoned_pass_discloses_which_terminal_was_disturbed() {
        let mut steering = crate::legalization::LegalizationSteering::default();
        let mut tally = LegalizationTally {
            passes: 1,
            ..LegalizationTally::default()
        };
        tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::Condemned {
                    learned: vec![crate::legalization::NoGood::new(
                        113,
                        prefix_fixture(1),
                        crate::legalization::ArmIdentity::live(
                            crate::cco::CandidateFamily::ModelAnchor,
                            0,
                            2.5,
                            1,
                        ),
                    )],
                    own_arm_terminals: vec![113],
                    cascade_victims: vec![7, 9],
                    measured: vec![crate::legalization::ArmRefusalPhysics {
                        gap_mm: -0.05,
                        distance_clearance_mm: 0.280_044,
                        axis_transit_illegal: true,
                        terminal_radius_mm: 0.372_136,
                        rootward_radius_mm: 0.372_136,
                    }],
                },
            )
            .expect("the pass learned a new no-good, so it may go on");

        let disclosure = tally.disclose(&steering, &BTreeSet::new());
        assert_eq!(disclosure.disturbances.len(), 1);
        let row = &disclosure.disturbances[0];
        assert_eq!(row.pass, 1);
        assert_eq!(row.own_arm_terminals, vec![113]);
        assert_eq!(
            row.cascade_victims,
            vec![7, 9],
            "victims steer nothing, and a reader who cannot see them cannot tell a \
             one-terminal class from a cascade"
        );
        assert_eq!(row.no_goods_learned, 1);
        assert!(!row.round_arm_measured);
        assert!(!row.voided_a_round);
        assert_eq!(row.steering_spent_terminal, None);
        assert_eq!(
            disclosure.no_goods_by_terminal,
            BTreeMap::from([(113_u32, 1_usize)]),
            "the no-goods consulted, keyed by the terminal that carries them"
        );
    }

    /// **Stage D — a pass that refuses by the abandon rule leaves NO row.**
    ///
    /// # Why this test has to exist
    ///
    /// A disturbance row says "this pass abandoned and the loop went on". The
    /// pass that trips the abandon rule does not go on; it ends the run by
    /// name. A row for it would claim a pass the loop never took, and the count
    /// of rows would stop matching the count of abandoned passes.
    #[test]
    fn a_pass_that_refuses_by_the_abandon_rule_leaves_no_disturbance_row() {
        let mut steering = crate::legalization::LegalizationSteering::default();
        let mut tally = LegalizationTally {
            passes: 1,
            ..LegalizationTally::default()
        };
        tally
            .apply(
                &mut steering,
                crate::legalization::LegalizationAbandon::Condemned {
                    learned: Vec::new(),
                    own_arm_terminals: Vec::new(),
                    cascade_victims: vec![7],
                    measured: Vec::new(),
                },
            )
            .expect_err("victims steer nothing, so this pass learned nothing");
        assert!(
            tally
                .disclose(&steering, &BTreeSet::new())
                .disturbances
                .is_empty(),
            "the refusing pass did not abandon-and-continue, so it has no row"
        );
    }

    /// **Stage D — the record survives a sidecar written before it existed.**
    ///
    /// # Why this test has to exist
    ///
    /// `persist_sealed_repair_ledger`'s doc comment claims the embedded report
    /// "deserializes back into the original typed report exactly". Adding a
    /// required field would break that claim for every `.coverage.repair.json`
    /// already on disk. `#[serde(default)]` keeps it, and the default it
    /// supplies — `passes: 0` — is the marker that says the file is older than
    /// Stage D rather than a run that never legalized.
    #[test]
    fn a_report_written_before_stage_d_still_deserializes_and_reads_as_pre_stage_d() {
        let legacy = serde_json::json!({
            "trials": 0,
            "commits": 0,
            "rejections_by_class": {},
            "wall_time_total_ms": 0,
            "wall_time_per_trial_min_ms": 0,
            "wall_time_per_trial_median_ms": 0,
            "wall_time_per_trial_max_ms": 0,
            "candidates_remaining": 0,
            "diversity_demotions": 0,
            "diversity_demotions_tried": 0
        });
        let report: crate::coverage_repair::RepairRunReport =
            serde_json::from_value(legacy).expect("a pre-Stage-D report must still deserialize");
        assert_eq!(
            report.legalization.passes, 0,
            "zero passes cannot come from a live run; it is the pre-Stage-D marker"
        );

        let steering = crate::legalization::LegalizationSteering::default();
        let live = crate::coverage_repair::RepairRunReport {
            legalization: LegalizationTally {
                passes: 6,
                confirmation_rounds: 1,
                round_arms_measured: 2,
                ..LegalizationTally::default()
            }
            .disclose(&steering, &BTreeSet::new()),
            ..crate::coverage_repair::RepairRunReport::default()
        };
        let round_tripped: crate::coverage_repair::RepairRunReport =
            serde_json::from_value(serde_json::to_value(&live).expect("serializes"))
                .expect("round-trips");
        assert_eq!(round_tripped, live);
        assert_eq!(round_tripped.legalization.passes, 6);
    }

    fn prefix_fixture(forest_digest_before: u64) -> crate::legalization::PrefixKey {
        crate::legalization::PrefixKey::of(&crate::cco::TerminalRouteEvidence {
            physical_contact_id: 0,
            terminal_id: 0,
            original_key: crate::forest::MemberKey::transit_test_fixture(0, 0),
            plan_digest: 1,
            loads_digest: 2,
            geometry_digest: 3,
            forest_digest_before,
            controller_digest_before: 5,
        })
    }

    #[test]
    fn a_stage_refusal_keeps_its_detail_at_attempt_zero_and_on_a_trial() {
        let stop = || RefusedStage {
            phase: SupportRunPhase::Routing,
            detail: format!("{:?}", "distinctive-marker-9f3c1e"),
        };

        let attempt_zero = stop().into_attempt_zero_failure();
        let SupportRunError::Failed { phase, detail } = attempt_zero else {
            panic!("expected SupportRunError::Failed at attempt 0, got {attempt_zero:?}");
        };
        assert_eq!(phase, SupportRunPhase::Routing);
        assert!(
            detail.contains("distinctive-marker-9f3c1e"),
            "attempt 0 must carry the real error in its detail, got: {detail:?}"
        );

        let trial = stop().into_trial_rollback();
        let crate::coverage_repair::RepairStepOutcome::StageRefusal {
            phase: trial_phase,
            detail: trial_detail,
        } = &trial
        else {
            panic!("a trial's stage refusal must be a ledgered rollback, got {trial:?}");
        };
        assert_eq!(*trial_phase, SupportRunPhase::Routing);
        assert_eq!(
            *trial_detail, detail,
            "the trial rollback must carry the SAME detail attempt 0 does; a divergence here \
             means one of the two paths is losing the error again"
        );
        assert_eq!(
            trial.class(),
            "stage_refusal",
            "the §A.8 histogram must be able to count this rejection by its own class"
        );
    }

    /// **§D.1's `RouteDelta` carries the CENSORED and the UNCENSORED side, and
    /// they are different numbers.**
    ///
    /// The first landing kept only the uncensored pair, so §D.2's promised
    /// sentence — "closed island 5's 0.36 mm² patch" — could not be produced
    /// from the record at all, and the patch-splitting move §B.5 guards against
    /// was invisible in the evidence even when the guard caught it.
    ///
    /// Route (5, 0) here carries one 0.36 mm² failing patch that the step
    /// closes, plus 0.05 mm² of sub-censor dust that it does not touch. Read
    /// only the uncensored side and the route looks like it went 0.41 → 0.05;
    /// read the censored side and it says the hole closed.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local exact pins over hand-written fixture values. See \
                  docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn a_route_delta_states_the_censored_and_the_uncensored_side_separately() {
        fn patch(area_mm2: f64, gap: f64) -> crate::coverage_closure::BlockingPatchKey {
            crate::coverage_closure::BlockingPatchKey {
                island_id: 5,
                component_id: 0,
                representative_face_id: 0,
                sorted_face_ids: vec![0],
                uncovered_area_mm2_bits: area_mm2.to_bits(),
                worst_gap: crate::coverage_closure::CoverageClosureGap::Finite(gap),
            }
        }
        fn state(
            blocking: Vec<crate::coverage_closure::BlockingPatchKey>,
            repair_targets: Vec<crate::coverage_closure::BlockingPatchKey>,
        ) -> MeasuredState {
            MeasuredState {
                blocking,
                repair_targets,
                uncovered_samples: Vec::new(),
                objective: crate::CoverageObjective {
                    failing_uncovered_area_mm2: 0.0,
                    max_patch_area_mm2: 0.0,
                    failing_patch_count: 0,
                    worst_gap_mm: 0.0,
                },
                uncensored_uncovered_mm2: 0.0,
            }
        }
        let dust = patch(0.05, 0.4);
        let hole = patch(0.36, 1.5);
        let before = state(vec![hole.clone()], vec![hole, dust.clone()]);
        let after = state(Vec::new(), vec![dust]);

        let deltas = route_deltas(&before, &after);
        assert_eq!(deltas.len(), 1, "one route moved: {deltas:?}");
        let delta = deltas[0];
        assert!((delta.failing_area_before_mm2 - 0.36).abs() < 1e-12);
        assert_eq!(delta.failing_area_after_mm2, 0.0);
        assert_eq!(delta.failing_patches_before, 1);
        assert_eq!(delta.failing_patches_after, 0);
        assert!((delta.uncensored_area_before_mm2 - 0.41).abs() < 1e-12);
        assert!((delta.uncensored_area_after_mm2 - 0.05).abs() < 1e-12);
        assert_ne!(
            delta.failing_area_before_mm2, delta.uncensored_area_before_mm2,
            "the fixture must separate the two sides, or it proves nothing"
        );
        assert_eq!(
            delta.worst_gap_before,
            Some(crate::coverage_closure::CoverageClosureGap::Finite(1.5)),
            "the worst gap is the WORST over the route's patches, not the last one seen"
        );
        assert_eq!(
            delta.worst_gap_after,
            Some(crate::coverage_closure::CoverageClosureGap::Finite(0.4))
        );
    }

    // ---------------------------------------------------------------------
    // SPEC-A4 §E.1 — the two MEASURED defect classes, each pinned to the exact
    // class it names.
    //
    // `ClearanceMaskUnsatisfiable` (task #51) and `StubReentry` (task #52) had
    // no fixture at all: their construction sites were reachable only through a
    // model whose geometry reproduces #51 or #52, and no procedural fixture
    // does. A refusal's class is what a reader acts on — "#51 is blocking this
    // island" is a different instruction from "the pool ran out" — so the class
    // is pinned here from the same per-trial evidence the loop folds, rather
    // than left to the day a mini reproduces the defect.
    //
    // The chain under test is the production one end to end minus the CCO:
    // `TrialContactDisposition` (what the finalized transcript said the trial's
    // OWN appended contact did) -> `record_disposition` -> `RouteSearchRecord`
    // -> `measured_defect_class`. Nothing below re-implements the rule; the
    // rule is called.
    // ---------------------------------------------------------------------

    /// The route key every case below uses. It is the raking blade's refusing
    /// island, so the two files talk about the same shape of thing.
    const CLASS_ROUTE: (u32, u32) = (5, 1);

    /// Fold a run's per-trial dispositions exactly as the loop folds them: the
    /// loop counts EVERY trial it spends — `rows_tried` is incremented before
    /// the stage chain runs — and then, only for a trial that reached a lowered
    /// run, records what the cadence did with that trial's own contact. A
    /// `None` here is a trial that never got that far (a stage refusal), which
    /// is exactly what the production path passes.
    fn folded_route_record(dispositions: &[Option<TrialContactDisposition>]) -> RouteSearchRecord {
        let mut records: BTreeMap<(u32, u32), RouteSearchRecord> = BTreeMap::new();
        for disposition in dispositions {
            records.entry(CLASS_ROUTE).or_default().rows_tried += 1;
            record_disposition(&mut records, CLASS_ROUTE, *disposition);
        }
        records.remove(&CLASS_ROUTE).unwrap_or_default()
    }

    fn clearance_refused() -> Option<TrialContactDisposition> {
        Some(TrialContactDisposition::UnroutedNoWinner(
            disclose_no_winner_evidence(Some(
                &crate::cco::NoWinnerEvidence::AllFeasibleCandidatesRefusedByClearance {
                    candidates_generated: 40,
                    candidates_feasible: 12,
                    anchor_seat_refusals: Vec::new(),
                    anchor_refusals: Vec::new(),
                },
            )),
        ))
    }

    fn quarantined_at(depth_mm: f64) -> Option<TrialContactDisposition> {
        Some(TrialContactDisposition::Quarantined {
            remaining_inside_axis_mm: Some(depth_mm),
        })
    }

    /// **Task #51's class, named.** Every trial this route spent ended with its
    /// contact unrouted because every feasible arm the router proposed was
    /// refused by the clearance standoff. The route reports
    /// `ClearanceMaskUnsatisfiable` and nothing else, it carries the count of
    /// terminals that were refused, and it carries the REGISTERED standoff it
    /// was measured against rather than a restated number.
    #[test]
    fn a_route_whose_every_trial_was_clearance_refused_reports_task_51s_class() {
        let scales = BlockedCauseScales::read().expect("the registered blocked-cause scales");
        let record = folded_route_record(&[clearance_refused(); 3]);
        assert_eq!(record.rows_tried, 3);

        assert_eq!(
            measured_defect_class(&record, scales),
            Some(
                crate::coverage_closure::BlockedCause::ClearanceMaskUnsatisfiable {
                    terminals: 3,
                    standoff_mm: scales.standoff_mm,
                }
            ),
            "three trials, three clearance refusals: this island reports task #51's class \
             exactly, not 'one of the six'"
        );
        assert!(
            scales.standoff_mm > 0.0,
            "a class carrying a zero standoff would be carrying a default, not a registered \
             scale: {scales:?}"
        );
    }

    /// **Task #52's class, named — and its three depths are the sorted
    /// order statistics, not the arrival order.** The depths are fed out of
    /// order on purpose: a class that reported them as they arrived would pass
    /// a same-order fixture and mislabel every real one.
    #[test]
    fn a_route_whose_every_trial_was_quarantined_reports_task_52s_class() {
        let scales = BlockedCauseScales::read().expect("the registered blocked-cause scales");
        let record = folded_route_record(&[
            quarantined_at(0.90),
            quarantined_at(0.20),
            quarantined_at(0.50),
        ]);
        assert_eq!(record.rows_tried, 3);

        assert_eq!(
            measured_defect_class(&record, scales),
            Some(crate::coverage_closure::BlockedCause::StubReentry {
                terminals_quarantined: 3,
                depth_min_mm: 0.20,
                depth_median_mm: 0.50,
                depth_max_mm: 0.90,
                allowance_mm: scales.allowance_mm,
            }),
            "three trials, three measured re-entry depths: this island reports task #52's class \
             exactly, with the depths ordered smallest / median / largest"
        );
        assert!(
            scales.allowance_mm > 0.0,
            "the allowance the depths are read against must be the registered one: {scales:?}"
        );
    }

    /// **A route that mixed causes reports NO defect class.** One clearance
    /// refusal and one quarantine is not "mostly #51": neither cause accounts
    /// for the whole search, so the route falls through to the search outcome
    /// the caller computes from the pool. Without this, the first `if` would
    /// happily claim #51 on a route where it explained half the trials.
    #[test]
    fn a_route_that_mixed_causes_claims_neither_defect_class() {
        let scales = BlockedCauseScales::read().expect("the registered blocked-cause scales");
        let record = folded_route_record(&[clearance_refused(), quarantined_at(0.30)]);
        assert_eq!(
            (record.rows_tried, record.clearance_refused_terminals),
            (2, 1)
        );
        assert_eq!(record.stub_reentry_depths_mm.len(), 1);

        assert_eq!(
            measured_defect_class(&record, scales),
            None,
            "two trials, two different causes: neither class may be claimed, because neither \
             one is what stopped this route"
        );
    }

    /// **A quarantine whose re-entry depth was never measured is not task
    /// #52.** `trial_contact_disposition` returns `Quarantined { None }` when
    /// the initial build's condemned ledger holds no row for that terminal —
    /// the contact was quarantined for something other than stub re-entry.
    /// Naming #52 there would be inventing a cause, and the depths the class
    /// carries would have nothing behind them.
    #[test]
    fn a_quarantine_with_no_measured_depth_does_not_become_task_52s_class() {
        let scales = BlockedCauseScales::read().expect("the registered blocked-cause scales");
        let record = folded_route_record(&[
            Some(TrialContactDisposition::Quarantined {
                remaining_inside_axis_mm: None,
            }),
            Some(TrialContactDisposition::Quarantined {
                remaining_inside_axis_mm: None,
            }),
        ]);
        assert_eq!(record.rows_tried, 2);
        assert!(
            record.stub_reentry_depths_mm.is_empty(),
            "a quarantine with no condemned-stub row records no depth"
        );

        assert_eq!(
            measured_defect_class(&record, scales),
            None,
            "these contacts were quarantined for a reason this run did not measure; #52 is a \
             claim about stub-axis re-entry and there is no re-entry depth here to make it with"
        );
    }

    /// **Trials that never reached a lowered run claim nothing either**, and
    /// neither does a route that spent no trial at all. Both are `rows_tried`
    /// without dispositions, and both must fall through — a route the search
    /// never reached has no defect to report.
    #[test]
    fn trials_without_a_disposition_claim_no_defect_class() {
        let scales = BlockedCauseScales::read().expect("the registered blocked-cause scales");

        let untried = folded_route_record(&[]);
        assert_eq!(untried.rows_tried, 0);
        assert_eq!(
            measured_defect_class(&untried, scales),
            None,
            "a route the search never spent a trial on cannot report a measured defect"
        );

        // A stage refusal: the trial was spent, but the chain never produced a
        // cadence to read a disposition off.
        let stage_refused = folded_route_record(&[None, None]);
        assert_eq!(stage_refused.rows_tried, 2);
        assert_eq!(
            measured_defect_class(&stage_refused, scales),
            None,
            "two trials refused by a stage say nothing about clearance or stub re-entry"
        );

        // And the routed trial that simply did not improve is not a defect
        // class either — it is the search outcome.
        let routed = folded_route_record(&[Some(TrialContactDisposition::Routed)]);
        assert_eq!(
            measured_defect_class(&routed, scales),
            None,
            "a contact that ROUTED and did not improve coverage is not blocked by #51 or #52"
        );
    }

    /// **The `UnroutedNoWinner` that was refused for some OTHER reason is not
    /// task #51.** The disposition carries the distinction as a bool, and the
    /// class may be claimed only from the `true` half — otherwise every
    /// no-winner terminal in the workspace would be reported as a clearance
    /// mask defect and #51's own instrument would be worthless.
    #[test]
    fn an_unrouted_terminal_refused_for_another_reason_is_not_task_51s_class() {
        let scales = BlockedCauseScales::read().expect("the registered blocked-cause scales");
        let record = folded_route_record(&[Some(TrialContactDisposition::UnroutedNoWinner(
            disclose_no_winner_evidence(Some(&crate::cco::NoWinnerEvidence::NoFeasibleCandidate {
                candidates_generated: 40,
                anchor_seat_refusals: Vec::new(),
                anchor_refusals: Vec::new(),
            })),
        ))]);
        assert_eq!(
            (record.rows_tried, record.clearance_refused_terminals),
            (1, 0)
        );

        assert_eq!(
            measured_defect_class(&record, scales),
            None,
            "no winner for some other reason is not the clearance mask being unsatisfiable"
        );
    }

    /// **The disclosure is a READ of the sealed reason, one variant at a time.**
    ///
    /// The archived design evidence (`docs/plans/support-clearance.md`
    /// §"Evidence") sized a class it could not name:
    /// 37 of `zombie-3-a`'s 89 rejected trials ended `UnroutedNoWinner`, and
    /// which of them were the `every_feasible_arm_refused_by_clearance` sub-class
    /// was unmeasured because the run computed the flag and threw it away. This
    /// pins the projection that now keeps it.
    ///
    /// **What could silently go wrong, and what each case catches.** The whole
    /// value of the disclosure is that it re-measures nothing, so the failure
    /// mode is not a crash — it is a plausible number in the wrong field. So
    /// every case below reads a DIFFERENT sealed reason and checks the exact
    /// values that came out of it, including which fields must stay `None`:
    ///
    /// * `NoCandidatesGenerated` carries no candidate counts, so writing `0`
    ///   there would turn "unrecorded" into "measured zero";
    /// * `NoFeasibleCandidate` counts what was generated but never reached the
    ///   clearance test, so a feasible count there would be invented;
    /// * only `AllFeasibleCandidatesRefusedByClearance` may set the flag, which
    ///   is the whole distinction task #51's instrument rests on;
    /// * the seat and foot refusal counts must come from the two DIFFERENT
    ///   lists — swapping them is the classic projection defect, and the
    ///   asymmetric lengths below (nine seat rows against no foot row) catch it
    ///   while still keeping `Some(0)` distinct from `None`.
    #[test]
    fn the_no_winner_disclosure_reads_each_sealed_reason_without_inventing_a_count() {
        use crate::coverage_repair::OwnContactNoWinnerStage;

        let seat_rows = |count: usize| -> Vec<crate::anchor::AnchorSeatRefusal> {
            (0..count)
                .map(|index| crate::anchor::AnchorSeatRefusal {
                    sample_index: index,
                    source_sample_face_id: u32::try_from(index).expect("a small index"),
                    reason: crate::anchor::AnchorSeatRefusalReason::TooShort,
                    first_hit_t: None,
                })
                .collect()
        };

        let none_generated = disclose_no_winner_evidence(Some(
            &crate::cco::NoWinnerEvidence::NoCandidatesGenerated {
                anchor_seat_refusals: seat_rows(5),
            },
        ));
        assert_eq!(
            none_generated,
            crate::coverage_repair::OwnContactNoWinnerEvidence {
                every_feasible_arm_refused_by_clearance: false,
                stage: Some(OwnContactNoWinnerStage::NoCandidatesGenerated),
                candidates_generated: None,
                candidates_feasible: None,
                anchor_seat_refusals_recorded: 5,
                anchor_foot_refusals_recorded: None,
            },
            "an empty anchor batch records five seat refusals and no candidate count at all"
        );

        let none_feasible =
            disclose_no_winner_evidence(Some(&crate::cco::NoWinnerEvidence::NoFeasibleCandidate {
                candidates_generated: 40,
                anchor_seat_refusals: seat_rows(9),
                anchor_refusals: Vec::new(),
            }));
        assert_eq!(
            none_feasible,
            crate::coverage_repair::OwnContactNoWinnerEvidence {
                every_feasible_arm_refused_by_clearance: false,
                stage: Some(OwnContactNoWinnerStage::NoFeasibleCandidate),
                candidates_generated: Some(40),
                candidates_feasible: None,
                anchor_seat_refusals_recorded: 9,
                anchor_foot_refusals_recorded: Some(0),
            },
            "arms were proposed and none was feasible, so there is no feasible count to read \
             and the flag stays false"
        );

        let clearance = disclose_no_winner_evidence(Some(
            &crate::cco::NoWinnerEvidence::AllFeasibleCandidatesRefusedByClearance {
                candidates_generated: 40,
                candidates_feasible: 12,
                anchor_seat_refusals: seat_rows(9),
                anchor_refusals: Vec::new(),
            },
        ));
        assert_eq!(
            clearance,
            crate::coverage_repair::OwnContactNoWinnerEvidence {
                every_feasible_arm_refused_by_clearance: true,
                stage: Some(OwnContactNoWinnerStage::AllFeasibleCandidatesRefusedByClearance),
                candidates_generated: Some(40),
                candidates_feasible: Some(12),
                anchor_seat_refusals_recorded: 9,
                anchor_foot_refusals_recorded: Some(0),
            },
            "task #51's shape: twelve feasible arms existed and the standoff refused every one"
        );

        // A row with no sealed reason at all. The `matches!` this projection
        // replaced returned `false` on exactly this input, and the flag must not
        // move because the disclosure grew fields around it.
        let unreasoned = disclose_no_winner_evidence(None);
        assert_eq!(
            unreasoned,
            crate::coverage_repair::OwnContactNoWinnerEvidence {
                every_feasible_arm_refused_by_clearance: false,
                stage: None,
                candidates_generated: None,
                candidates_feasible: None,
                anchor_seat_refusals_recorded: 0,
                anchor_foot_refusals_recorded: None,
            },
            "a row that named no reason gets no stage, and the flag reads false as it always did"
        );
    }

    /// **The disclosure changed no decision — the flag still drives the fold,
    /// and only from the clearance stage.**
    ///
    /// `record_disposition` is the one place the flag is READ, and it is the
    /// seam an evidence-only change could break without any test noticing: fold
    /// on the wrong stage and task #51's instrument starts counting terminals
    /// that were never near the clearance test. The three stages are folded
    /// here through the production function and only one of them may count.
    #[test]
    fn only_the_clearance_stage_folds_into_task_51s_counter() {
        let no_candidates = folded_route_record(&[Some(
            TrialContactDisposition::UnroutedNoWinner(disclose_no_winner_evidence(Some(
                &crate::cco::NoWinnerEvidence::NoCandidatesGenerated {
                    anchor_seat_refusals: Vec::new(),
                },
            ))),
        )]);
        assert_eq!(
            (
                no_candidates.rows_tried,
                no_candidates.clearance_refused_terminals
            ),
            (1, 0),
            "an empty anchor batch is not the clearance mask refusing anything"
        );

        let clearance = folded_route_record(&[clearance_refused(); 3]);
        assert_eq!(
            (clearance.rows_tried, clearance.clearance_refused_terminals),
            (3, 3),
            "and every clearance-refused trial still counts, exactly as before the disclosure"
        );
    }

    fn ledger_row(order: usize, seat_mm: [f64; 3]) -> crate::coverage_closure::RankedRepairRow {
        crate::coverage_closure::RankedRepairRow {
            id: crate::specialized::RepairRowId::for_search_rule_test(3, 0, order as u64),
            key: (3, 0),
            seat_mm,
            order,
            marginal_area_mm2: 0.46,
            scored_from: Vec::new(),
            diversity_discs: 0,
        }
    }

    fn ledger_step(
        trial: usize,
        row: &crate::coverage_closure::RankedRepairRow,
        outcome: crate::coverage_repair::RepairStepOutcome,
        gates_after: Option<crate::coverage_repair::NoncoverageVector>,
        own_contact_condemned: Option<bool>,
    ) -> crate::coverage_repair::CoverageRepairStep {
        let objective = crate::CoverageObjective {
            failing_uncovered_area_mm2: 0.46,
            max_patch_area_mm2: 0.36,
            failing_patch_count: 2,
            worst_gap_mm: 4.0,
        };
        crate::coverage_repair::CoverageRepairStep {
            trial,
            commit: None,
            row_id: row.id,
            seat_mm: row.seat_mm,
            source_island_id: row.key.0,
            source_component_id: row.key.1,
            scored_from: Vec::new(),
            marginal_area_mm2: row.marginal_area_mm2,
            chosen_by_first_trial_guarantee: false,
            diversity_discs_containing_seat: 0,
            objective_before: objective,
            objective_after: Some(objective),
            uncensored_before_mm2: 0.46,
            uncensored_after_mm2: Some(0.46),
            gates_before: crate::coverage_repair::NoncoverageVector {
                unrouted_no_winner: 7,
                quarantined: 2,
                unrouted_specialized: 1,
                load_violations: 3,
                unsupported_specialized: 4,
            },
            gates_after,
            ownerless_faces_before: 0,
            ownerless_faces_after: Some(0),
            own_contact_quarantined: Some(false),
            own_contact_condemned,
            own_contact_no_winner: None,
            route_deltas: Vec::new(),
            outcome,
        }
    }

    /// **The ledger keeps WHY, beside the two things it already kept.**
    ///
    /// Before this, `reject` did the same two things for every rejection class
    /// — spend the row, drop a disc at its seat — so a clause-1 rejection, a
    /// clause-2 veto and a stage refusal were indistinguishable to everything
    /// downstream. The outcome class went to the step stream, which no branch
    /// reads and which only reaches disk when the search stops.
    ///
    /// What this pins:
    ///
    /// * the OLD bookkeeping is untouched — the row is still ledgered, the
    ///   disc is still charged to the whole ranking, and the §A.8 histogram
    ///   still counts by class;
    /// * the NEW memory holds the class, the hard-gate component that rose
    ///   underneath a clause-1 rejection (the measured majority case: 176 of
    ///   321 trials on the 2026-08-28 baseline), and the condemned own
    ///   contact;
    /// * a COMMIT teaches it nothing, which is what keeps the memory from
    ///   learning to avoid the seats that work.
    ///
    /// The revert this guards is the cheap one: fold the veto in
    /// `RepairLedger::finish` from the step stream instead of at the rejection,
    /// and a run that never terminates records nothing again.
    #[test]
    fn the_repair_ledger_records_why_a_trial_was_rejected_not_only_that_it_was() {
        let rejected = ledger_row(0, [0.0, 0.0, 0.0]);
        let neighbour = ledger_row(1, [1.0, 0.0, 0.0]);
        let distant = ledger_row(2, [100.0, 0.0, 0.0]);
        let mut candidates = crate::coverage_closure::RankedRepairCandidates {
            ranked: vec![rejected.clone(), neighbour.clone(), distant.clone()],
            ..Default::default()
        };
        let mut ledger = RepairLedger::new(
            6.0,
            crate::coverage_repair::LegalizationDisclosure::default(),
        );

        // Clause 1 rejected it, and `unrouted_no_winner` rose underneath.
        ledger.report.trials += 1;
        let step = ledger_step(
            1,
            &rejected,
            crate::coverage_repair::RepairStepOutcome::NoImprovement {
                before: crate::CoverageObjective {
                    failing_uncovered_area_mm2: 0.46,
                    max_patch_area_mm2: 0.36,
                    failing_patch_count: 2,
                    worst_gap_mm: 4.0,
                },
                after: crate::CoverageObjective {
                    failing_uncovered_area_mm2: 0.46,
                    max_patch_area_mm2: 0.36,
                    failing_patch_count: 2,
                    worst_gap_mm: 4.0,
                },
            },
            Some(crate::coverage_repair::NoncoverageVector {
                unrouted_no_winner: 8,
                quarantined: 2,
                unrouted_specialized: 1,
                load_violations: 3,
                unsupported_specialized: 4,
            }),
            Some(true),
        );
        ledger.reject(&mut candidates, step, &rejected, std::time::Instant::now());

        // OLD bookkeeping, unchanged.
        assert!(ledger.ledgered.contains(&rejected.id), "the row is spent");
        assert_eq!(
            candidates
                .ranked
                .iter()
                .map(|row| row.diversity_discs)
                .collect::<Vec<_>>(),
            vec![1, 1, 0],
            "the disc is still charged to every row inside it, and to nothing outside it"
        );
        assert_eq!(ledger.report.rejections_by_class["no_improvement"], 1);

        // NEW memory: the class, the component that moved, and the seat it
        // moved at.
        assert_eq!(ledger.vetoes.len(), 1);
        let near = ledger.vetoes.counts_near(rejected.seat_mm);
        assert_eq!(near.total, 1);
        assert_eq!(
            near.class(crate::coverage_repair::RepairVetoClass::NoImprovement),
            1
        );
        assert_eq!(
            near.gate_rise(crate::coverage_repair::NoncoverageComponent::UnroutedNoWinner),
            1,
            "the rise clause 1 hid is what the ledger could not say before"
        );
        assert_eq!(near.own_contact_condemned, 1);
        assert_eq!(
            ledger.vetoes.counts_near(distant.seat_mm).total,
            0,
            "and it is asked by seat, at the disc's own radius"
        );

        // A COMMIT is not a veto.
        ledger.report.trials += 1;
        let mut committed = ledger_step(
            2,
            &neighbour,
            crate::coverage_repair::RepairStepOutcome::Committed,
            Some(crate::coverage_repair::NoncoverageVector {
                unrouted_no_winner: 7,
                quarantined: 2,
                unrouted_specialized: 1,
                load_violations: 3,
                unsupported_specialized: 4,
            }),
            Some(false),
        );
        committed.commit = Some(1);
        ledger.commit(committed, std::time::Instant::now());
        assert_eq!(
            ledger.vetoes.len(),
            1,
            "a commit teaches the memory nothing"
        );

        // And what it learned reaches the report an operator reads.
        ledger.finish(Some(&candidates));
        let disclosure = &ledger.report.vetoes;
        assert_eq!(disclosure.total, 1);
        assert_eq!(disclosure.by_class["no_improvement"], 1);
        assert_eq!(disclosure.gate_rises_by_component["unrouted_no_winner"], 1);
        assert!(
            !disclosure
                .gate_rises_by_component
                .contains_key("quarantined"),
            "a component that never moved is absent, not zero: {disclosure:?}"
        );
        assert_eq!(disclosure.own_contact_condemned, 1);
        assert_eq!(
            ledger.report.trail, None,
            "no trail directory was asked for, so no file was opened"
        );
        assert!(ledger
            .report
            .summary_line()
            .contains("gate_rises[unrouted_no_winner=1]"));
    }
}
