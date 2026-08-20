//! The coverage-closure controller: FU-56.
//!
//! Production support planning used to return from `select_contacts_baseline`
//! one line before the legacy pre-routing repair loop
//! (`select_contacts_with_mode`, `lib.rs`), so `COVERAGE_REPAIR_MAX_ATTEMPTS`
//! had no production reader and the immutable [`crate::specialized::RepairPool`]
//! — built, lineage-validated, contact-validated and hashed — sat unused. This
//! module is the controller the pool was built for.
//!
//! **What closes, and what never moves.** Closure is achieved ONLY by adding
//! immutable repair-pool contacts to the plan, never by relaxing a gate: the
//! coverage radius and the registered A3 censor
//! ([`registered_coverage_censor_area_mm2`], `voxel_pitch_mm²` = 0.16 mm²)
//! are read from the SAME registry the pre-write gate reads
//! ([`registered_coverage_request`], shared with
//! `plinth_pipeline::gates::registered_coverage_request`), and
//! [`coverage_patch_blocks_verdict`] mirrors `plinth_verify::defects_for_patches`'s
//! blocking predicate exactly rather than restating it — see
//! `plinth-verify/tests/coverage_gate.rs` for the drift alarm that keeps the
//! two definitions from diverging silently.
//!
//! **The censor decides verdicts, never effort.** That predicate answers "is
//! this model refused?"; the controller's repair worklist is chosen by the
//! uncensored [`coverage_patch_needs_repair`] instead, because a contact
//! nominated for a censored dust patch covers everything else inside the
//! coverage radius, real holes included. The call-site table on
//! [`coverage_patch_blocks_verdict`] classifies every use as one or the other.
//! The one geometric measurement is
//! made over ROUTED contacts, because that is what the gate itself measures;
//! a contact this controller adds but the CCO transcript does not admit
//! contributes zero coverage, which is exactly why closure iterates and
//! re-measures rather than trusting a single pre-routing pass.
//!
//! The controller loop itself lives in [`crate::runner::run_support`], because
//! it needs the sealed run's private typestate machinery
//! (`prepare_support_context`, `prepared_run_for_plan`,
//! `LoweredSupportRun::into_context`) that this module — a sibling, not a
//! descendant, of `runner` — cannot name. This module supplies the parts that
//! do not need that machinery: the typed refusal evidence
//! ([`CoverageClosureRefusal`]) FU-57 persists, the shared blocking predicate
//! and registered request, and the batch-first nomination algorithm
//! ([`nominate_repair_rows`]) that turns one attempt's uncovered patches — the
//! uncensored worklist — into a deterministic, deduplicated set of repair-row
//! ids to append.

use std::collections::{BTreeMap, BTreeSet};

use plinth_core::m0_constants;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::specialized::{RepairRow, RepairRowId, SpecializedContactPlan, SpecializedError};
use crate::{
    CoverageGap, CoverageMeasurementError, CoverageMeasurementRequest, SupportPreparationLimits,
};

/// **The refusal verdict's** per-patch predicate, mirroring
/// `plinth_verify::defects_for_patches` exactly: a patch blocks emission iff
/// its assessment failed, its cut was ambiguous, or its uncovered area is
/// GREATER THAN the censor. Kept as the ONE implementation rather than restated
/// independently in the verifier and the controller —
/// `plinth-verify/tests/coverage_gate.rs` proves the pairing stays exact.
///
/// # Verdict versus work selection: the four call sites, classified
///
/// The censor decides **whether a model is refused**. It must NOT decide
/// **which patches the repair controller works on** — those are different
/// questions, and answering the second one with the censor is a measured
/// regression (`Skulker Zombie` 0.35 mm², `Tavey Badger` 0.21 mm² and
/// `zombie-3-a` 12.65 mm² of REAL, above-censor uncovered area, all emitted
/// before the censor reached the worklist and refused after it). The
/// mechanism: a nominated contact covers everything within the coverage
/// radius, so a repair nominated for a dust patch routinely closes a real
/// hole beside it. Dropping the dust patch from the worklist drops that
/// contact and leaves the real hole open. SPEC-A3-A4-COVERAGE.md §4 says this
/// itself — censored area is "diagnostic only … **not permission for another
/// hard gate to fail**" — so the censor is a rule about verdicts, not about
/// effort.
///
/// Hence two named functions, and every call site is one or the other:
///
/// | call site | kind | predicate |
/// | --- | --- | --- |
/// | `plinth_verify::defects_for_patches` (the pre-write coverage gate) | **verdict** | this one |
/// | `plinth_pipeline::coverage_evidence::islands_from_gate_report` (renders that verdict's islands) | **verdict** | this one |
/// | `plinth_support::runner::run_support`, closure loop exit and refusal | **verdict** | this one |
/// | `plinth_support::runner::run_support`, repair-row nomination worklist | **work selection** | [`coverage_patch_needs_repair`] |
///
/// The controller is BOTH, which is why it is listed twice: it decides when it
/// may stop and what it must refuse (censored), and it decides what to try to
/// fix (uncensored). Adding a call site means picking a row in that table.
///
/// # The censor, and why it is per patch
///
/// `SPEC-A3-A4-COVERAGE.md` §4 states the rule this predicate implements:
///
/// > A patch fails if and only if either:
/// > 1. its uncovered area is greater than `voxel_pitch_mm**2`; or
/// > 2. it touches an ambiguous topology cut.
/// >
/// > Area at or below `voxel_pitch_mm**2` is **censored A3 uncertainty**. It is
/// > diagnostic only […] At the current 0.4 mm pitch the censor is 0.16 mm².
///
/// The granularity is therefore NOT ambiguous and needs no house reading: the
/// spec bounds **each patch**, not an island's total and not a model's total. A
/// patch is one connected group of uncovered source faces, which is the
/// physically motivated unit — a patch smaller than one voxel footprint cannot
/// seat a support, whereas a dozen such patches scattered over a model are a
/// dozen independent sub-resolution unknowns, not one large hole. Aggregation
/// to the island happens AFTER this predicate, in
/// `plinth_verify::defects_for_patches`, and sums the areas of blocking patches
/// only — a censored patch contributes nothing to its island's defect measure.
///
/// `censor_area_mm2` is always `voxel_pitch_mm²` from the constants registry;
/// callers take it from the live measurement request
/// (`CoverageMeasurementRequest::patch_area_floor_mm2`, which
/// [`registered_coverage_request`] fills from
/// [`registered_coverage_censor_area_mm2`]) so this predicate censors at
/// exactly the value the measurement itself used when it set
/// `assessment_failed`. It is a parameter rather than a registry read inside
/// the function so that a test may probe the rule at a chosen censor and so
/// that this stays a `const fn` on the hot per-patch path.
///
/// **The comparison is strictly greater-than**, exactly as written: an
/// uncovered area of exactly `voxel_pitch_mm²` is censored, not fatal.
#[must_use]
pub const fn coverage_patch_blocks_verdict(
    uncovered_area_mm2: f64,
    cut_ambiguous: bool,
    assessment_failed: bool,
    censor_area_mm2: f64,
) -> bool {
    assessment_failed || cut_ambiguous || uncovered_area_mm2 > censor_area_mm2
}

/// **The repair controller's work-selection predicate: the same rule at a zero
/// censor.** A patch needs repair iff the measurement found ANY uncovered area
/// on it, its cut was ambiguous, or its assessment failed.
///
/// This takes no censor argument on purpose. Work selection is uncensored: the
/// controller nominates a repair-pool row for every measured uncovered patch,
/// censored or not, because a contact nominated for sub-censor dust covers
/// everything else inside the coverage radius too, and censoring the worklist
/// was measured to leave real, above-censor holes unrepaired (see the table on
/// [`coverage_patch_blocks_verdict`]). Working a censored patch can only ADD
/// coverage; it can never excuse a hole, because the verdict that decides
/// refusal is [`coverage_patch_blocks_verdict`] and is censored.
///
/// It is one implementation with the verdict rule, called at a zero censor, so
/// the two can never drift on the ambiguity and assessment axes — and
/// `every_patch_that_blocks_the_verdict_also_needs_repair` pins the
/// containment: the worklist is always a superset of the verdict's blocking
/// set, so no refusal can name a patch the controller never tried to fix.
#[must_use]
pub const fn coverage_patch_needs_repair(
    uncovered_area_mm2: f64,
    cut_ambiguous: bool,
    assessment_failed: bool,
) -> bool {
    coverage_patch_blocks_verdict(uncovered_area_mm2, cut_ambiguous, assessment_failed, 0.0)
}

/// Failures in assembling the registered coverage-closure/gate request.
#[derive(Clone, Debug, PartialEq, Error)]
pub enum CoverageClosureSetupError {
    #[error("registered M0 constants are unavailable: {0}")]
    RegisteredConstantsUnavailable(String),
    #[error("the production support resource policy is unavailable: {0}")]
    ResourcePolicyUnavailable(String),
    #[error("the registered coverage-closure request is invalid: {0}")]
    InvalidCoverageRequest(#[source] CoverageMeasurementError),
}

/// The one production implementation of the pre-write gate's own coverage
/// measurement request: radius = `COVERAGE_RADIUS_FACTOR × TARGET_SPACING_MM`,
/// patch-area floor = `VOXEL_PITCH_MM²`, ceilings from
/// [`SupportPreparationLimits::coverage_measurement_limits`].
/// `plinth_pipeline::gates::registered_coverage_request` delegates here so the
/// controller and the gate can never independently drift onto two different
/// radii — the controller closes to exactly what the gate will re-measure.
///
/// # Errors
///
/// Returns [`CoverageClosureSetupError::RegisteredConstantsUnavailable`] or
/// [`CoverageClosureSetupError::ResourcePolicyUnavailable`] if the registry
/// cannot be read, and [`CoverageClosureSetupError::InvalidCoverageRequest`]
/// if the registered values do not form a valid request.
pub fn registered_coverage_request() -> Result<CoverageMeasurementRequest, CoverageClosureSetupError>
{
    let limits = SupportPreparationLimits::production()
        .map_err(|error| CoverageClosureSetupError::ResourcePolicyUnavailable(error.to_string()))?;
    let constants = m0_constants().map_err(|error| {
        CoverageClosureSetupError::RegisteredConstantsUnavailable(error.to_string())
    })?;
    let policy = constants.placeholders();
    let radius_mm = policy.coverage_radius_factor().diagnostic().get()
        * policy.target_spacing_mm().diagnostic().get();
    let patch_area_floor_mm2 = registered_coverage_censor_area_mm2()?;
    let limits = limits
        .coverage_measurement_limits()
        .map_err(CoverageClosureSetupError::InvalidCoverageRequest)?;
    CoverageMeasurementRequest::from_f64(radius_mm, patch_area_floor_mm2, limits)
        .map_err(CoverageClosureSetupError::InvalidCoverageRequest)
}

/// The registered A3 coverage censor: `voxel_pitch_mm²`, one voxel-pitch square.
///
/// `SPEC-A3-A4-COVERAGE.md` §4: "Area at or below `voxel_pitch_mm**2` is
/// **censored A3 uncertainty** … At the current 0.4 mm pitch the censor is
/// 0.16 mm²." The provenance is the registered `VOXEL_PITCH_MM` placeholder
/// (0.4 mm, "Legacy perception working scale inferred from one corpus run"),
/// squared here; 0.16 is never written down as a literal anywhere, so moving
/// the registered pitch moves the censor with it.
///
/// This is the ONE derivation. [`registered_coverage_request`] fills the
/// measurement request from it, the measurement stamps `assessment_failed`
/// with it, and [`coverage_patch_blocks_verdict`] is handed the same value back
/// out of the request — so the measurement and the gate can never censor at
/// two different areas.
///
/// # Errors
///
/// Returns [`CoverageClosureSetupError::RegisteredConstantsUnavailable`] if the
/// constants registry cannot be read.
pub fn registered_coverage_censor_area_mm2() -> Result<f64, CoverageClosureSetupError> {
    let constants = m0_constants().map_err(|error| {
        CoverageClosureSetupError::RegisteredConstantsUnavailable(error.to_string())
    })?;
    Ok(constants
        .placeholders()
        .voxel_pitch_mm()
        .diagnostic()
        .get()
        .powi(2))
}

/// Why closure stopped without admitting every measured island.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CoverageClosureReason {
    /// The registered attempt bound
    /// (`ContactSelectionConfig::coverage_repair_max_attempts`, registered
    /// value 8) was reached with blocking patches still measured.
    AttemptsExhausted,
    /// **No** patch on the worklist anywhere had an unoccupied, unfailed
    /// repair-pool row in its own `(island_id, component_id)`, so the attempt
    /// produced an empty nomination set and closure could not shrink anything
    /// at all. The worklist is the UNCENSORED one
    /// ([`coverage_patch_needs_repair`]), so a run reaches this only when even
    /// the censored dust patches have no row left to try.
    ///
    /// This is deliberately the whole-attempt condition, not a per-patch one:
    /// a model with one row-less island and one island that still has rows
    /// keeps iterating, because refusing the moment ANY island runs dry would
    /// abandon islands that were still closing. The per-island distinction
    /// the owner needs — "never had a row" versus "nominated every attempt
    /// and never closed" — is carried on
    /// [`CoverageClosureIsland::eligible_repair_rows`] and
    /// [`CoverageClosureIsland::nominated_repair_rows`] instead, on EVERY
    /// refusal reason including [`Self::AttemptsExhausted`].
    NoEligibleRepairRows,
    /// The routed-contact coverage measurement itself could not be completed.
    ///
    /// **One producer (task #49 split landing, 2026-08-19).** A short-lived
    /// second producer was withdrawn: SPEC-A3-A4 §6's removal used to refuse
    /// when it left a model face with no eligible carrier, and that refusal was
    /// funnelled here. It no longer refuses — the ownerless faces are disclosed
    /// (`perception::OwnerlessLoadFace`) and the measurement below rules on the
    /// island's real uncovered area — so this variant is again exactly what its
    /// name says: the measurement ran and failed.
    MeasurementRefused(String),
    /// A plan the controller had ALREADY extended with repair-pool contacts
    /// could not be re-planned: one of routing, rewire, brace, structure,
    /// lowering or the re-preparation refused the extended plan.
    ///
    /// **This is a coverage verdict, not a generic sealed-run failure.**
    /// Attempt 0 — the model's own plan, untouched by this controller —
    /// sealed all the way through lowering, so the geometry demonstrably
    /// seals; what failed is the controller's own extension of it. Reporting
    /// it as [`crate::SupportRunError::Failed`] would make a coverage refusal
    /// indistinguishable from a routing failure the model would have had
    /// anyway: it would lose the coverage evidence FU-57 requires, and it
    /// would let the oriented-solid reserve substitute a different geometry
    /// after a coverage verdict, which the terminal-refusal design forbids.
    RepairPlanRefused {
        /// The stage that refused the repair-extended plan.
        phase: crate::SupportRunPhase,
        /// That stage's own error, as it was reported.
        detail: String,
    },
    /// **SPEC-A4 §B.7.** No route on the worklist had a single eligible
    /// repair-pool row at the FIRST repair step, so there was never anything
    /// to transact. The one-winner controller's whole-model version of
    /// [`Self::NoEligibleRepairRows`], separated from it because the batch
    /// loop's variant also fires mid-run and this one cannot.
    NothingToTry,
    /// **SPEC-A4 §B.7.** The honest end of a greedy search: every candidate was
    /// tried and rolled back, or the best remaining candidate's GLOBAL score is
    /// 0.00 mm² and cannot cover anything (§B.3's zero-score guard).
    ///
    /// This is not a claim that the model cannot be repaired. Which routes
    /// remain, and why each one remains, is per-route evidence
    /// ([`RouteRepairOutcome`]) measured from counters the run already keeps —
    /// never a distance claim.
    NoImprovingRepair {
        candidates_tried: usize,
        best_remaining_score_mm2: f64,
    },
    /// **SPEC-A4 §B.7, ruling R6.** The registered bound
    /// (`coverage_repair_max_attempts`, value 8) stopped the search with
    /// candidates left. Under R6 that bound counts COMMITTED steps only;
    /// rejections are bounded by pool exhaustion plus the zero-score guard,
    /// and under ruling R10 there is no failed-tries cap at all.
    CommitBudgetExhausted {
        committed: usize,
        candidates_remaining: usize,
    },
}

/// **SPEC-A4 §E.1 — why one still-uncovered route is still uncovered when the
/// loop stopped. Craig's rulings R7 and R14.**
///
/// Every variant is MEASURED from a counter the run already keeps. **None of
/// them is a distance claim**, and that is the whole of the ruling: the design
/// v1 assigned Warhorse islands 5 and 6 the cause "no row within radius",
/// inferred from worst gaps of 7.09 mm and 15.29 mm against a 6.0 mm radius.
/// Those are two different distances — worst gap measures the distance to the
/// nearest ROUTED CONTACT, reach measures the distance to a CANDIDATE SEAT —
/// and the island diagnosis says the inference was false: 220 of island 5's
/// 300 pool rows sit within the radius, with the nearest candidate seat
/// **0.0972 mm** from the uncovered area. "Nothing reached it" and "nothing
/// could reach it" are not the same statement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RouteRepairOutcome {
    /// Rows existed, were tried, and none passed the accept clauses.
    TriedNoImprovement {
        rows_tried: usize,
        best_marginal_area_mm2: f64,
    },
    /// Rows existed and remained when the commit bound ended the search.
    NotReached { rows_remaining: usize },
    /// A measured defect class: this route cannot be repaired by any row in its
    /// pool under today's rules, and the counter that says so is named.
    Blocked(BlockedCause),
}

/// The five measured defect classes ruling R14 names, and no others.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BlockedCause {
    /// Every eligible row was tried; each was rolled back.
    PoolExhausted {
        pool_rows: usize,
        occupied: usize,
        ledgered: usize,
    },
    /// No eligible row scores above 0.00 mm² against the current uncovered set.
    ///
    /// This IS a measurement — the score — not an inference from a distance.
    /// It is named so it cannot be read as a reach claim.
    /// `nearest_seat_to_uncovered_mm` rides along as CONTEXT: the class is
    /// decided by the score being zero, and the distance is reported beside it,
    /// never as the reason.
    NoCandidateCoversAnything {
        pool_rows: usize,
        rows_within_radius: usize,
        nearest_seat_to_uncovered_mm: f64,
    },
    /// The route never had a pool row of its own.
    NoPoolRows,
    /// **Task #51.** Every candidate arm the router proposed for this route was
    /// refused by the support-to-model clearance standoff. Measured from the
    /// CCO transcript's per-terminal refusal reasons.
    ClearanceMaskUnsatisfiable { terminals: u32, standoff_mm: f64 },
    /// **Task #52.** Every contact this route placed was condemned BEFORE any
    /// arm was tried, because the stub axis re-enters the model further than
    /// the allowance.
    ///
    /// Measured, not inferred: the depths are the initial build's own
    /// `remaining_inside_axis_mm` for the condemned stubs, and `allowance_mm`
    /// is the registered `min_illegal_len_mm` the build compared them against.
    /// A quarantine with no condemned-stub row is NOT counted here — it was
    /// quarantined for something else, and naming it re-entry would be the kind
    /// of claim ruling R14 forbids.
    StubReentry {
        terminals_quarantined: u32,
        depth_min_mm: f64,
        depth_median_mm: f64,
        depth_max_mm: f64,
        allowance_mm: f64,
    },
}

/// A worst routed-contact gap, projected from [`CoverageGap`] for a refusal
/// that must outlive the borrowed verification snapshot it was measured
/// against.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CoverageClosureGap {
    Finite(f64),
    NoRoutedContact,
}

impl From<CoverageGap> for CoverageClosureGap {
    fn from(gap: CoverageGap) -> Self {
        match gap {
            CoverageGap::Finite(value) => Self::Finite(value.get()),
            CoverageGap::NoRoutedContact => Self::NoRoutedContact,
        }
    }
}

/// One measured island still uncovered when closure stopped, aggregated over
/// every blocking patch sharing its `(island_id, component_id)` key exactly
/// as `plinth_verify::defects_for_patches` aggregates by island: the area sum
/// is finite and additive, and the worst gap is the least favourable of the
/// group's (`NoRoutedContact` outranks any finite distance, matching the
/// gate's own reading that "no route reached this patch at all" is the worse
/// evidence).
///
/// This is evidence only. It is never read by `judge_pre_write_gates`, and
/// FU-57 is the row that persists it as the sidecar/card/render triple.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoverageClosureIsland {
    pub island_id: u32,
    pub component_id: u32,
    pub representative_face_id: u32,
    pub sorted_face_ids: Vec<u32>,
    pub uncovered_area_mm2: f64,
    pub worst_gap: CoverageClosureGap,
    pub patch_count: usize,
    /// How many repair-pool rows the controller nominated for this island
    /// over the whole run, summed across attempts.
    ///
    /// **Zero means this island was never even attempted** — it had no
    /// unoccupied, unfailed repair-pool row in its own
    /// `(island_id, component_id)` on any attempt that reached nomination.
    /// A positive count with the island still blocking means the opposite:
    /// rows WERE appended and the CCO transcript did not turn them into
    /// coverage. Those are different problems with different fixes (the pool
    /// or the mesh, versus the router), and before this field the refusal
    /// could not tell them apart.
    #[serde(default)]
    pub nominated_repair_rows: usize,
    /// How many repair-pool rows were still eligible for this island the last
    /// time nomination ran. Zero beside a positive
    /// [`Self::nominated_repair_rows`] means the pool for this island drained.
    #[serde(default)]
    pub eligible_repair_rows: usize,
    /// **SPEC-A4 §E.1, rulings R7 and R14.** Why THIS route is still uncovered,
    /// as a measured class rather than a distance claim.
    ///
    /// `None` from the batch controller, which has no per-route search to
    /// report on; `Some` from the one-winner controller. Additive and
    /// `serde(default)`, so an older sidecar still reads.
    #[serde(default)]
    pub outcome: Option<RouteRepairOutcome>,
}

/// One closure round's measured shape, kept for refusal evidence: how many
/// patches were still blocking when this attempt measured, and how many rows
/// this attempt nominated in response (zero on the terminal attempt, which
/// measures but does not nominate).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageClosureAttempt {
    pub attempt: usize,
    /// Patches that would REFUSE the model on this attempt: the censored
    /// verdict set ([`coverage_patch_blocks_verdict`]). Zero here is what ends
    /// the loop with a sealed authority.
    pub blocking_patch_count: usize,
    /// Patches this attempt was willing to WORK ON: every patch the
    /// measurement found uncovered at all, censored or not
    /// ([`coverage_patch_needs_repair`]). Always at least
    /// [`Self::blocking_patch_count`]; the difference is the sub-censor dust
    /// the controller repairs anyway, because a repair nominated for dust
    /// covers the real hole beside it too.
    #[serde(default)]
    pub repair_target_patch_count: usize,
    pub nominated_rows: usize,
}

/// The typed refusal FU-57 persists: closure could not admit every measured
/// island within the registered attempt bound. Boxed at its one call site
/// ([`crate::SupportRunError::CoverageClosure`]) because it carries the whole
/// per-attempt history and every still-uncovered island's evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoverageClosureRefusal {
    pub attempts: usize,
    pub reason: CoverageClosureReason,
    pub islands: Vec<CoverageClosureIsland>,
    pub history: Vec<CoverageClosureAttempt>,
    /// **SPEC-A4 §A.8, Craig's ruling R10 — the loud report, on the refused
    /// path.** R10 removes the failed-tries cap and replaces it with
    /// visibility, and that trade only holds if the visibility reaches the
    /// record. This is the sidecar half of it.
    ///
    /// `None` from the batch controller, which runs no trials. Additive and
    /// `serde(default)`, so it is not the stage-4 format change.
    #[serde(default)]
    pub repair: Option<crate::coverage_repair::RepairRunReport>,
}

/// One uncovered patch's routing key and exact face membership, projected out
/// of a live `CoverageMeasurementPatchEvidence` so nomination can outlive the
/// borrowed verification snapshot the measurement was taken against. Built
/// only from a live measurement, never assembled as a planning input.
///
/// The controller builds TWO lists of these out of one measurement — the
/// censored verdict set and the uncensored repair worklist — so a value of
/// this type means "a measured uncovered patch", and whether it blocks
/// anything depends on which list it is in. See
/// [`coverage_patch_blocks_verdict`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BlockingPatchKey {
    pub(crate) island_id: u32,
    pub(crate) component_id: u32,
    pub(crate) representative_face_id: u32,
    pub(crate) sorted_face_ids: Vec<u32>,
    pub(crate) uncovered_area_mm2_bits: u64,
    pub(crate) worst_gap: CoverageClosureGap,
}

/// Aggregate one attempt's blocking patches into the per-island evidence
/// [`CoverageClosureRefusal`] carries. One gathered pass, grouped by
/// `(island_id, component_id)`, with each island's nomination history stamped
/// on from `tally` in the same pass.
pub(crate) fn islands_from_blocking(
    patches: &[BlockingPatchKey],
    tally: &BTreeMap<(u32, u32), KeyNomination>,
) -> Vec<CoverageClosureIsland> {
    let mut grouped: BTreeMap<(u32, u32), CoverageClosureIsland> = BTreeMap::new();
    for patch in patches {
        let key = (patch.island_id, patch.component_id);
        let uncovered_area_mm2 = f64::from_bits(patch.uncovered_area_mm2_bits);
        grouped
            .entry(key)
            .and_modify(|island| {
                island.representative_face_id = island
                    .representative_face_id
                    .min(patch.representative_face_id);
                island.sorted_face_ids.extend(patch.sorted_face_ids.iter());
                island.sorted_face_ids.sort_unstable();
                island.sorted_face_ids.dedup();
                island.uncovered_area_mm2 += uncovered_area_mm2;
                island.worst_gap = worse_gap(island.worst_gap, patch.worst_gap);
                island.patch_count += 1;
            })
            .or_insert_with(|| CoverageClosureIsland {
                island_id: patch.island_id,
                component_id: patch.component_id,
                representative_face_id: patch.representative_face_id,
                sorted_face_ids: patch.sorted_face_ids.clone(),
                uncovered_area_mm2,
                worst_gap: patch.worst_gap,
                patch_count: 1,
                nominated_repair_rows: 0,
                eligible_repair_rows: 0,
                outcome: None,
            });
    }
    for (key, island) in &mut grouped {
        let counts = tally.get(key).copied().unwrap_or_default();
        island.nominated_repair_rows = counts.nominated_rows;
        island.eligible_repair_rows = counts.eligible_rows;
    }
    grouped.into_values().collect()
}

fn worse_gap(left: CoverageClosureGap, right: CoverageClosureGap) -> CoverageClosureGap {
    match (left, right) {
        (CoverageClosureGap::NoRoutedContact, _) | (_, CoverageClosureGap::NoRoutedContact) => {
            CoverageClosureGap::NoRoutedContact
        }
        (CoverageClosureGap::Finite(left), CoverageClosureGap::Finite(right)) => {
            CoverageClosureGap::Finite(left.max(right))
        }
    }
}

/// What nomination found for one blocking `(island_id, component_id)` key.
///
/// Kept per key rather than summed, because the two ways a key can fail to
/// close are the two different actions an owner would take (FU-57's card
/// exists to tell them apart): "this key never had a single unoccupied,
/// unfailed repair-pool row" says the pool or the mesh is wrong, while
/// "rows were nominated every attempt and the area never moved" says the
/// transcript is not admitting them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct KeyNomination {
    /// Rows still eligible for this key at the moment nomination ran.
    pub(crate) eligible_rows: usize,
    /// Rows this attempt actually nominated for this key.
    pub(crate) nominated_rows: usize,
}

/// One attempt's nomination outcome: the flat, deduplicated row list to
/// append, plus the per-key accounting the refusal evidence needs.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Nominations {
    pub(crate) rows: Vec<RepairRowId>,
    pub(crate) per_key: BTreeMap<(u32, u32), KeyNomination>,
}

/// Batch-first, deterministic nomination of repair-pool rows for one attempt's
/// patches on the repair worklist.
///
/// **Candidates are scored against the samples the measurement found
/// UNCOVERED, never against every sample on a blocking patch's faces.** A
/// blocking patch is a connected group of faces each of which has at least
/// one uncovered sample; a face can be — and on real geometry usually is —
/// partially covered. Scoring against every sample on those faces sums
/// already-covered area into the score, which lets a candidate sitting beside
/// a large covered region outrank the candidate that actually closes the
/// residual gap, and (because the score set is fixed across attempts) makes
/// the whole 8-round ranking walk useless rows while a viable closer waits
/// below them. `uncovered_samples` is the measurement's own
/// [`crate::CoverageMeasurement::uncovered_sample_indices`], the same
/// discipline the legacy `rank_coverage_repair_rows` (`lib.rs`) applies when
/// it scores against failing sample rows only.
///
/// Occupancy is read ONCE ([`SpecializedContactPlan::occupied_repair_rows`]),
/// and the whole pool is grouped by `(island_id, component_id)` ONCE
/// ([`SpecializedContactPlan::all_repair_rows`]) — never per patch, which is
/// what [`SpecializedContactPlan::eligible_repair_rows`] would cost if called
/// once per blocking patch (it re-derives occupancy on every call). Scoring
/// within one `(island_id, component_id)` group is tiled exactly as
/// `rank_coverage_repair_rows` (`lib.rs`) tiles it: the same registered
/// `coverage_score_matrix_max_entries` budget, clamped to the same 65,536
/// production ceiling, the same `sample_tile × candidate_tile` shape, and the
/// same tie-break (score descending, then pool order ascending) — so ordering
/// stays total and deterministic across repeated runs on the same plan.
///
/// One row is nominated PER patch on the worklist (not one per attempt and not
/// one per key), up to the number of eligible rows the key actually has, so a
/// component with several simultaneous uncovered patches can close more than
/// one of them in a single attempt. Nominated rows are deduplicated by id
/// AND by candidate seat point across the whole attempt, matching
/// `append_regular_repair_row`'s own `DuplicateChildPhysicalSeat` refusal.
///
/// **`repair_targets` is the UNCENSORED worklist**, not the censored verdict
/// set: the caller passes every patch the measurement found uncovered at all
/// ([`coverage_patch_needs_repair`]), because a row nominated for sub-censor
/// dust seats a contact that covers the real hole beside it too. See the
/// call-site table on [`coverage_patch_blocks_verdict`].
///
/// # Errors
///
/// Propagates [`SpecializedError`] from
/// [`SpecializedContactPlan::occupied_repair_rows`].
pub(crate) fn nominate_repair_rows(
    plan: &SpecializedContactPlan,
    repair_targets: &[BlockingPatchKey],
    uncovered_samples: &[usize],
    radius_mm: f64,
    matrix_budget: usize,
    failed: &BTreeSet<RepairRowId>,
) -> Result<Nominations, SpecializedError> {
    let mut excluded: BTreeSet<RepairRowId> = plan
        .occupied_repair_rows()?
        .into_iter()
        .map(|occupancy| occupancy.row_id())
        .collect();
    excluded.extend(failed.iter().copied());

    // A repair-pool row's identity is source-qualified by its OWNER ISLAND
    // (`RepairPool` doc, specialized.rs): the same physical candidate seat
    // legally appears as a distinct `RepairRowId` under every island whose
    // face set contains it, so excluding by row id alone does not exclude an
    // already-occupied seat's OTHER island-owned rows. `append_regular_repair_rows`
    // itself refuses a coincident seat with `DuplicateChildPhysicalSeat`
    // regardless of owner, so nomination must apply that same physical-point
    // exclusion BEFORE proposing a row, not discover it only when the append
    // refuses. One pass over the plan's existing contacts, gathered once.
    let mut claimed_points: Vec<[f64; 3]> = plan
        .contacts()
        .iter()
        .map(|contact| contact.position_mm())
        .collect();

    let mut rows_by_key: BTreeMap<(u32, u32), Vec<&RepairRow>> = BTreeMap::new();
    for row in plan.all_repair_rows() {
        if excluded.contains(&row.id())
            || claimed_points
                .iter()
                .any(|point| *point == row.candidate().point_mm)
        {
            continue;
        }
        rows_by_key
            .entry((row.id().owner_island_id(), row.id().component_id()))
            .or_default()
            .push(row);
    }

    // Only the measurement's uncovered rows are grouped: one gathered pass
    // over the uncovered set (never over every sample), so the scoring matrix
    // below is sized by what still needs covering rather than by the whole
    // island. `uncovered_samples` is ascending, so each group stays ascending
    // and the tiling below is order-stable.
    let samples = plan.coverage_samples();
    let mut samples_by_key: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for &index in uncovered_samples {
        let Some(sample) = samples.get(index) else {
            // The measurement is taken against this plan's own sample slice,
            // so this cannot happen; skipping rather than indexing keeps a
            // future caller mismatch from panicking inside the controller.
            continue;
        };
        samples_by_key
            .entry((sample.island_id, sample.component_id))
            .or_default()
            .push(index);
    }

    let mut patches_by_key: BTreeMap<(u32, u32), Vec<&BlockingPatchKey>> = BTreeMap::new();
    for patch in repair_targets {
        patches_by_key
            .entry((patch.island_id, patch.component_id))
            .or_default()
            .push(patch);
    }

    let radius_sq = radius_mm * radius_mm;
    let matrix_budget = matrix_budget.clamp(1, 65_536);
    let sample_tile = ((matrix_budget as f64).sqrt() as usize).max(1);
    let candidate_tile = (matrix_budget / sample_tile).max(1);

    // `claimed_points` already carries every seat this plan's existing
    // contacts occupy (seeded above); picking below extends it in place so a
    // seat chosen for an earlier key in THIS attempt is excluded from every
    // later key too.
    let mut nominated = Nominations::default();

    for (key, patches) in patches_by_key {
        let Some(candidate_rows) = rows_by_key.get(&key) else {
            // Recorded rather than silently skipped: "this blocking key had
            // zero eligible repair-pool rows" is the distinction the refusal
            // evidence must carry, and it used to leave no trace at all.
            nominated.per_key.insert(key, KeyNomination::default());
            continue;
        };
        let mut sorted_faces: Vec<u32> = patches
            .iter()
            .flat_map(|patch| patch.sorted_face_ids.iter().copied())
            .collect();
        sorted_faces.sort_unstable();
        sorted_faces.dedup();
        let scoring_samples: Vec<usize> = samples_by_key
            .get(&key)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&index| {
                sorted_faces
                    .binary_search(&plan.coverage_samples()[index].face_id)
                    .is_ok()
            })
            .collect();

        let mut scores = vec![0.0_f64; candidate_rows.len()];
        let candidate_indices: Vec<usize> = (0..candidate_rows.len()).collect();
        for candidate_block in candidate_indices.chunks(candidate_tile) {
            for sample_block in scoring_samples.chunks(sample_tile) {
                debug_assert!(candidate_block.len() * sample_block.len() <= matrix_budget);
                for &candidate_offset in candidate_block {
                    let point = candidate_rows[candidate_offset].candidate().point_mm;
                    for &sample_index in sample_block {
                        let sample = &plan.coverage_samples()[sample_index];
                        if squared_distance(point, sample.point_mm) <= radius_sq {
                            scores[candidate_offset] += sample.area_mm2;
                        }
                    }
                }
            }
        }

        let mut ranked: Vec<usize> = candidate_indices.clone();
        ranked.sort_by(|&left, &right| {
            scores[left]
                .total_cmp(&scores[right])
                .reverse()
                .then_with(|| {
                    candidate_rows[left]
                        .order()
                        .cmp(&candidate_rows[right].order())
                })
        });

        let mut picked = 0usize;
        for offset in ranked {
            if picked >= patches.len() {
                break;
            }
            let row = candidate_rows[offset];
            let point = row.candidate().point_mm;
            if claimed_points.contains(&point) {
                continue;
            }
            claimed_points.push(point);
            nominated.rows.push(row.id());
            picked += 1;
        }
        nominated.per_key.insert(
            key,
            KeyNomination {
                eligible_rows: candidate_rows.len(),
                nominated_rows: picked,
            },
        );
    }

    Ok(nominated)
}

/// One route's share of a candidate's GLOBAL score — SPEC-A4 design §D.1's
/// `scored_from`.
///
/// A candidate seat covers whatever falls inside its coverage disc, and the
/// disc does not know about island ids. Often the row's own route is not the
/// largest contributor, and that cross-route effect is the whole justification
/// for keeping the repair worklist uncensored (task #27): a seat owned by a
/// dust route that physically closes the real hole next door is credited for
/// the hole, by name, rather than for the dust.
///
/// Per ROUTE and not per patch, deliberately. A per-patch attribution is not
/// producible without scoring per patch, which multiplies the scoring work by
/// the patch count; per route is one extra accumulator in the same matrix pass.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoverageScoreShare {
    pub island_id: u32,
    pub component_id: u32,
    pub area_mm2: f64,
}

/// One eligible repair-pool row with its GLOBAL marginal uncovered area.
#[derive(Clone, Debug, PartialEq)]
#[allow(
    dead_code,
    reason = "SPEC-A4 design §A.4: the one-winner controller consumes these; it lands in the next commit of this lane and the allow goes with it"
)]
pub(crate) struct RankedRepairRow {
    pub(crate) id: RepairRowId,
    /// The route that OWNS the row. Rule N2 changes how a row is scored, never
    /// which route owns it (SPEC §3's one-route rule is untouched).
    pub(crate) key: (u32, u32),
    pub(crate) seat_mm: [f64; 3],
    /// Pool order, the deterministic tie-break SPEC §6 names.
    pub(crate) order: usize,
    /// **Rule N2**: the summed area weight of every uncovered sample within `R`
    /// of this seat, on ANY route.
    pub(crate) marginal_area_mm2: f64,
    pub(crate) scored_from: Vec<CoverageScoreShare>,
}

/// Every eligible candidate across every worklist route, scored globally.
///
/// This is what replaces "one row per patch, deduplicated". There is no pick
/// cap in it (rule N1) and no per-route filter on the scoring samples
/// (rule N2); the controller picks exactly one winner from it per trial, and
/// re-sorts rather than re-scores after a rejection.
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(
    dead_code,
    reason = "SPEC-A4 design §A.4: the one-winner controller consumes these; it lands in the next commit of this lane and the allow goes with it"
)]
pub(crate) struct RankedRepairCandidates {
    pub(crate) ranked: Vec<RankedRepairRow>,
    pub(crate) per_key: BTreeMap<(u32, u32), KeyNomination>,
    /// Each worklist route's own measured UNCENSORED uncovered area, the
    /// quantity rule N3's serve set watches for change.
    pub(crate) route_uncovered_mm2: BTreeMap<(u32, u32), f64>,
}

/// One candidate seat's GLOBAL score and the routes that produced it.
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(
    dead_code,
    reason = "SPEC-A4 design §A.4: the one-winner controller consumes these; it lands in the next commit of this lane and the allow goes with it"
)]
pub(crate) struct GlobalCandidateScore {
    pub(crate) marginal_area_mm2: f64,
    pub(crate) scored_from: Vec<CoverageScoreShare>,
}

/// **Rule N2's arithmetic, on its own, so it can be proved on its own.**
///
/// One matrix over (all candidate seats) × (all uncovered samples), tiled so
/// that `candidate_block.len() × sample_block.len() ≤ matrix_budget` — the same
/// discipline and the same 65,536 production ceiling
/// (`coverage_score_matrix_max_entries`) today's route-local scoring uses. What
/// is different is one deletion: **the samples are NOT filtered to the
/// candidate's own `(island_id, component_id)`**, because the physical quantity
/// SPEC §6 names is the area the contact actually covers and a contact does not
/// know about island ids.
///
/// The per-route shares are the same additions the total already performs,
/// keyed — there is no second pass over the geometry and no second distance
/// test. That is why §D.1's attribution is per ROUTE and not per patch: per
/// route is one extra accumulator in this pass, per patch would multiply the
/// pass by the patch count.
///
/// Kept separate from [`rank_repair_candidates`] deliberately: the ranking
/// needs a whole `SpecializedContactPlan` (mesh, topology, repair pool,
/// lineage) to say which rows are ELIGIBLE, and none of that is needed to say
/// what a seat COVERS. Splitting them is what lets the cross-route credit that
/// task #27's population depends on be proved from a handful of samples instead
/// of from a model run.
#[allow(
    dead_code,
    reason = "SPEC-A4 design §A.4: the one-winner controller consumes these; it lands in the next commit of this lane and the allow goes with it"
)]
pub(crate) fn score_candidates_globally(
    seats: &[[f64; 3]],
    samples: &[crate::CoverageSample],
    scoring_samples: &[usize],
    radius_mm: f64,
    matrix_budget: usize,
) -> Vec<GlobalCandidateScore> {
    let radius_sq = radius_mm * radius_mm;
    let matrix_budget = matrix_budget.clamp(1, 65_536);
    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "a tile edge is a positive count well below 2^53; the clamp above bounds it"
    )]
    let sample_tile = ((matrix_budget as f64).sqrt() as usize).max(1);
    let candidate_tile = (matrix_budget / sample_tile).max(1);

    let mut totals = vec![0.0_f64; seats.len()];
    let mut shares: Vec<BTreeMap<(u32, u32), f64>> = vec![BTreeMap::new(); seats.len()];
    let candidate_indices: Vec<usize> = (0..seats.len()).collect();
    for candidate_block in candidate_indices.chunks(candidate_tile) {
        for sample_block in scoring_samples.chunks(sample_tile) {
            debug_assert!(candidate_block.len() * sample_block.len() <= matrix_budget);
            for &candidate_offset in candidate_block {
                let point = seats[candidate_offset];
                for &sample_index in sample_block {
                    let Some(sample) = samples.get(sample_index) else {
                        continue;
                    };
                    if squared_distance(point, sample.point_mm) <= radius_sq {
                        totals[candidate_offset] += sample.area_mm2;
                        *shares[candidate_offset]
                            .entry((sample.island_id, sample.component_id))
                            .or_insert(0.0) += sample.area_mm2;
                    }
                }
            }
        }
    }

    totals
        .into_iter()
        .zip(shares)
        .map(|(marginal_area_mm2, share)| GlobalCandidateScore {
            marginal_area_mm2,
            scored_from: share
                .into_iter()
                .map(|((island_id, component_id), area_mm2)| CoverageScoreShare {
                    island_id,
                    component_id,
                    area_mm2,
                })
                .collect(),
        })
        .collect()
}

/// The base order SPEC §6 names: score descending, then lower pool row as the
/// deterministic tie-break.
///
/// Rule N4 re-sorts on top of this after every rejection (see
/// [`choose_winner`]); it never re-scores, because a dropped child plan leaves
/// the uncovered sample set and every candidate seat bit-identical.
#[allow(
    dead_code,
    reason = "SPEC-A4 design §A.4: the one-winner controller consumes these; it lands in the next commit of this lane and the allow goes with it"
)]
pub(crate) fn sort_ranked_rows(ranked: &mut [RankedRepairRow]) {
    ranked.sort_by(|left, right| {
        left.marginal_area_mm2
            .total_cmp(&right.marginal_area_mm2)
            .reverse()
            .then_with(|| left.order.cmp(&right.order))
            .then_with(|| left.key.cmp(&right.key))
    });
}

/// **SPEC-A4 design §A.4 rules N1 and N2 — rank every eligible repair-pool row
/// on the worklist by GLOBAL marginal uncovered sample area.**
///
/// Two rules separate this from [`nominate_repair_rows`], and both are
/// deliberate deletions rather than additions:
///
/// **N1 — there is no per-patch pick cap.** The batch loop stopped at
/// `picked >= patches.len()`, one seat per patch per attempt, which is blind to
/// how large the hole is: the Rotting Warhorse's island 68 carries its whole
/// 11.879 mm² as a SINGLE patch and therefore received one seat per attempt —
/// 8 seats out of 377 — refusing with 361 eligible seats never tried. Under
/// one-winner transactions exactly one row is appended per step anyway, so the
/// cap has no nomination work left to do; all it did was ration attention.
///
/// **N2 — the score is global.** Today's scoring filters the samples to the
/// candidate's own `(island_id, component_id)` and to that key's patch faces.
/// That filter is gone. The physical quantity SPEC §6 names — "marginal
/// uncovered sample area" — is the area the contact actually covers, and a
/// contact does not know about island ids. Route-local scoring under-values
/// exactly the rows task #27 exists to keep.
///
/// **The tiling discipline is unchanged**: one matrix over (all eligible
/// candidates) × (all uncovered samples), chunked so that
/// `candidate_block.len() × sample_block.len() ≤ matrix_budget`, clamped to the
/// same 65,536 production ceiling, with the same `total_cmp` tie-break. Cost is
/// three orders of magnitude below one pipeline pass — the Rotting Warhorse's
/// worst rescore is ~1.5 × 10⁶ squared distances against a single trial's
/// 6.0 × 10⁸ seat evaluations on the Giant Toad — so it needs no new limit and
/// no new registered constant.
///
/// The three exclusions are kept verbatim from [`nominate_repair_rows`]:
/// occupied rows, ledgered (`failed`) rows, and rows whose physical seat is
/// already claimed by an existing contact — the last mirroring
/// `append_regular_repair_rows`' own `DuplicateChildPhysicalSeat` refusal, so
/// the controller never spends a trial to discover a refusal it could see.
///
/// # Errors
///
/// Propagates [`SpecializedError`] from
/// [`SpecializedContactPlan::occupied_repair_rows`].
#[allow(
    dead_code,
    reason = "SPEC-A4 design §A.4: the one-winner controller consumes these; it lands in the next commit of this lane and the allow goes with it"
)]
pub(crate) fn rank_repair_candidates(
    plan: &SpecializedContactPlan,
    repair_targets: &[BlockingPatchKey],
    uncovered_samples: &[usize],
    radius_mm: f64,
    matrix_budget: usize,
    failed: &BTreeSet<RepairRowId>,
) -> Result<RankedRepairCandidates, SpecializedError> {
    let mut excluded: BTreeSet<RepairRowId> = plan
        .occupied_repair_rows()?
        .into_iter()
        .map(|occupancy| occupancy.row_id())
        .collect();
    excluded.extend(failed.iter().copied());

    let claimed_points: Vec<[f64; 3]> = plan
        .contacts()
        .iter()
        .map(|contact| contact.position_mm())
        .collect();

    // Only routes ON the worklist may nominate — ruling R2's reading of §6's
    // "failing route", with the worklist uncensored per task #27.
    let mut route_uncovered_mm2: BTreeMap<(u32, u32), f64> = BTreeMap::new();
    for patch in repair_targets {
        *route_uncovered_mm2
            .entry((patch.island_id, patch.component_id))
            .or_insert(0.0) += f64::from_bits(patch.uncovered_area_mm2_bits);
    }

    let mut eligible_by_key: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    let mut candidates: Vec<&RepairRow> = Vec::new();
    for row in plan.all_repair_rows() {
        let key = (row.id().owner_island_id(), row.id().component_id());
        if !route_uncovered_mm2.contains_key(&key) {
            continue;
        }
        if excluded.contains(&row.id())
            || claimed_points
                .iter()
                .any(|point| *point == row.candidate().point_mm)
        {
            continue;
        }
        *eligible_by_key.entry(key).or_insert(0) += 1;
        candidates.push(row);
    }
    // A worklist route with no eligible row leaves a trace rather than
    // vanishing: "this route never had a single unoccupied, unfailed row" is a
    // different owner action from "rows were tried and none helped", and §E.1
    // reports them as different measured classes.
    for key in route_uncovered_mm2.keys() {
        eligible_by_key.entry(*key).or_insert(0);
    }

    // The scoring sample set is EVERY uncovered sample, on every route — that
    // is rule N2 in one line. Gathered once, ascending, so the tiling is
    // order-stable.
    let samples = plan.coverage_samples();
    let scoring_samples: Vec<usize> = uncovered_samples
        .iter()
        .copied()
        .filter(|&index| index < samples.len())
        .collect();

    let seats: Vec<[f64; 3]> = candidates
        .iter()
        .map(|row| row.candidate().point_mm)
        .collect();
    let scores =
        score_candidates_globally(&seats, samples, &scoring_samples, radius_mm, matrix_budget);

    let mut ranked: Vec<RankedRepairRow> = candidates
        .iter()
        .zip(scores)
        .map(|(row, score)| RankedRepairRow {
            id: row.id(),
            key: (row.id().owner_island_id(), row.id().component_id()),
            seat_mm: row.candidate().point_mm,
            order: row.order(),
            marginal_area_mm2: score.marginal_area_mm2,
            scored_from: score.scored_from,
        })
        .collect();
    sort_ranked_rows(&mut ranked);

    let per_key = eligible_by_key
        .into_iter()
        .map(|(key, eligible_rows)| {
            (
                key,
                KeyNomination {
                    eligible_rows,
                    nominated_rows: 0,
                },
            )
        })
        .collect();

    Ok(RankedRepairCandidates {
        ranked,
        per_key,
        route_uncovered_mm2,
    })
}

/// The rejected seats whose coverage discs deprioritize their neighbours —
/// **SPEC-A4 design §A.4 rule N4, Craig's ruling R13**.
///
/// **The measured defect.** Scores tie on absolute area, and the tie-break is
/// pool order, which is spatially contiguous because the repair pool is built
/// by walking the surface. So a run of equal-scoring candidates is a run of
/// NEIGHBOURING seats, and the measured worst case is **63 seats inside a
/// 1.04 mm box** — sixty-three candidates that are physically one seat, ranked
/// consecutively. Under batch nomination that was invisible: they were appended
/// together and the model was measured once. Under one-winner, rejecting seat
/// `k` and then trying seat `k+1` 16 µm away spends a second full
/// prepare → route → rewire → brace → structure → lower → measure pass to learn
/// what the first pass already proved.
///
/// **The rule.** After a trial on row `r` is rejected, every remaining
/// candidate whose seat lies inside `r`'s own coverage disc — `|seat(c) −
/// seat(r)| ≤ R`, the same `R` the score is computed at — sorts after every
/// candidate outside it. Discs accumulate, and the sort key is the COUNT of
/// rejected discs a seat falls inside, ascending, applied BEFORE score, so
/// "how many times has this neighbourhood already failed" is the first
/// question.
///
/// **Deprioritization is not exclusion.** A deprioritized candidate stays
/// eligible and is tried once every candidate outside the disc has been. Under
/// ruling R10 nothing truncates the order, so completeness is preserved
/// exactly: the loop still ends only at pool exhaustion or the zero-score
/// guard.
///
/// **No new constant.** `R` is `COVERAGE_RADIUS_FACTOR × TARGET_SPACING_MM`,
/// already registered, already the radius the score is computed at, and already
/// the radius the pre-write gate re-measures at. The disc is the rejected
/// seat's own coverage disc, derived from existing geometry, not a knob.
///
/// **Why the coverage disc is the right radius, physically.** Two seats within
/// `R` of each other cover overlapping sample sets by construction. If seat
/// `r`'s trial was rejected because its arm could not route, or because it
/// worsened a noncoverage count, a seat inside `r`'s disc is proposing a
/// near-identical arm to a near-identical set of samples. The disc is the exact
/// boundary of "this is the same proposal". Outside it, the candidate reaches
/// samples `r` could not, and it deserves the pass.
#[derive(Clone, Debug, Default)]
#[allow(
    dead_code,
    reason = "SPEC-A4 design §A.4: the one-winner controller consumes these; it lands in the next commit of this lane and the allow goes with it"
)]
pub(crate) struct DiversityDiscs {
    seats: Vec<[f64; 3]>,
    radius_sq: f64,
}

#[allow(
    dead_code,
    reason = "SPEC-A4 design §A.4 rule N4: the one-winner controller consumes these; it lands in the next commit of this lane"
)]
impl DiversityDiscs {
    pub(crate) fn new(radius_mm: f64) -> Self {
        Self {
            seats: Vec::new(),
            radius_sq: radius_mm * radius_mm,
        }
    }

    /// Record a rejected seat. One disc per rejection, so each later rerank
    /// adds exactly one pass over the remaining candidates.
    pub(crate) fn reject(&mut self, seat_mm: [f64; 3]) {
        self.seats.push(seat_mm);
    }

    /// How many rejected coverage discs contain this seat. This IS the sort
    /// key, and it is also §D.1's `diversity_discs_containing_seat` evidence.
    pub(crate) fn discs_containing(&self, seat_mm: [f64; 3]) -> usize {
        self.seats
            .iter()
            .filter(|&&seat| squared_distance(seat, seat_mm) <= self.radius_sq)
            .count()
    }
}

/// Which candidate the controller tries next, and why it was chosen.
#[derive(Clone, Debug, PartialEq)]
#[allow(
    dead_code,
    reason = "SPEC-A4 design §A.4: the one-winner controller consumes these; it lands in the next commit of this lane and the allow goes with it"
)]
pub(crate) struct ChosenCandidate {
    pub(crate) index: usize,
    /// **Rule N3.** True when the first-trial guarantee chose this route rather
    /// than the global ranking, so the guarantee's cost is measurable instead
    /// of assumed.
    pub(crate) by_first_trial_guarantee: bool,
    pub(crate) diversity_discs_containing_seat: usize,
}

/// **SPEC-A4 design §A.4 rule N3 plus rule N4's order — choose the one winner.**
///
/// **Greedy on a global area score is already area-proportional**, which is the
/// point worth stating rather than adding machinery for: the score IS an area,
/// so a 13.04 mm² route's best seats outscore a 0.21 mm² route's by tens of
/// times, and with N1's cap gone a large hole draws consecutive winners until
/// its marginal areas fall below the next route's. That needs no quota, no
/// per-route budget and no new registered number.
///
/// What greedy does not give is any guarantee for a SMALL route, so there is
/// exactly one, and no more:
///
/// > **First-trial guarantee.** The *serve set* is every worklist route that
/// > (a) has not yet had a trial since its own measured uncovered area last
/// > changed, and (b) has at least one eligible candidate whose GLOBAL score is
/// > at or above the censor. While the serve set is non-empty the winner is the
/// > best candidate of the serve-set route whose best candidate ranks highest,
/// > with ties broken on larger route uncovered area then lower pool row. When
/// > it is empty the winner is simply the highest-ranking eligible candidate.
///
/// **The censor is not a new number.** It is the registered A3 censor already.
/// A route whose best global score is below it cannot, by SPEC §4, change the
/// censored objective on its own, so a trial on it is provably wasted. This is
/// the zero-score stop guard generalized from "covers nothing" to "cannot
/// change the verdict"; nothing is added, raised or relaxed.
///
/// **Checked against rule N2.** The guarantee is what makes the task #27
/// population safe under a single ranking, and global scoring is what makes the
/// guarantee reachable: a dust route's seat qualifies for the serve set on the
/// REAL hole's area, not on its own dust. `Skulker Zombie`'s 0.35 mm² and
/// `Tavey Badger`'s 0.21 mm² both clear the 0.16 mm² threshold on the hole they
/// close, not on the route that owns them.
///
/// **The limit, stated rather than argued away.** A first trial that commits
/// consumes committed-step budget, so on a model with many worklist routes the
/// guarantee can spend the whole registered bound on first trials and never let
/// the largest hole take its second contact. Measured: no currently-emitting
/// model exceeds the bound (maximum 8 routes), and every heavy refuser does
/// (the Awakened Tree has 103). That is open item O-3 and it is Craig's ruling,
/// not an implementation detail.
///
/// `ledgered` holds the rows already tried and rolled back; SPEC §6 requires
/// exact failed rows to be excluded, and they stay excluded across a rollback.
#[allow(
    dead_code,
    reason = "SPEC-A4 design §A.4: the one-winner controller consumes these; it lands in the next commit of this lane and the allow goes with it"
)]
pub(crate) fn choose_winner(
    candidates: &RankedRepairCandidates,
    ledgered: &BTreeSet<RepairRowId>,
    discs: &DiversityDiscs,
    served: &BTreeMap<(u32, u32), u64>,
    censor_area_mm2: f64,
) -> Option<ChosenCandidate> {
    // Rule N4's key, ahead of score, exactly as R13 specifies.
    let rank_key = |row: &RankedRepairRow| {
        (
            discs.discs_containing(row.seat_mm),
            OrderedScore(row.marginal_area_mm2),
            row.order,
            row.key,
        )
    };

    let live = || {
        candidates
            .ranked
            .iter()
            .enumerate()
            .filter(|(_, row)| !ledgered.contains(&row.id))
    };

    // (a) a route not yet tried since its area last changed, AND (b) a best
    // global score at or above the censor. Both halves are measured.
    let in_serve_set = |row: &RankedRepairRow| {
        if row.marginal_area_mm2 < censor_area_mm2 {
            return false;
        }
        let current = candidates
            .route_uncovered_mm2
            .get(&row.key)
            .copied()
            .unwrap_or(0.0)
            .to_bits();
        served.get(&row.key) != Some(&current)
    };

    let best_served =
        live()
            .filter(|(_, row)| in_serve_set(row))
            .min_by(|(_, left), (_, right)| {
                rank_key(left).cmp(&rank_key(right)).then_with(|| {
                    // Ties on the rank key break on LARGER route uncovered area,
                    // then lower pool row — the guarantee serves the biggest
                    // unserved hole first.
                    let area = |row: &RankedRepairRow| {
                        OrderedScore(
                            candidates
                                .route_uncovered_mm2
                                .get(&row.key)
                                .copied()
                                .unwrap_or(0.0),
                        )
                    };
                    area(left).cmp(&area(right))
                })
            });
    if let Some((index, row)) = best_served {
        return Some(ChosenCandidate {
            index,
            by_first_trial_guarantee: true,
            diversity_discs_containing_seat: discs.discs_containing(row.seat_mm),
        });
    }

    let (index, row) =
        live().min_by(|(_, left), (_, right)| rank_key(left).cmp(&rank_key(right)))?;
    Some(ChosenCandidate {
        index,
        by_first_trial_guarantee: false,
        diversity_discs_containing_seat: discs.discs_containing(row.seat_mm),
    })
}

/// A descending-by-area ordering key that is total over every `f64` the score
/// can hold, including the zero a candidate covering nothing carries.
/// `total_cmp` reversed, so `min_by` on it means "largest area first".
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(
    dead_code,
    reason = "SPEC-A4 design §A.4: the one-winner controller consumes these; it lands in the next commit of this lane and the allow goes with it"
)]
struct OrderedScore(f64);

impl Eq for OrderedScore {}
impl PartialOrd for OrderedScore {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for OrderedScore {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0).reverse()
    }
}

fn squared_distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    (left[0] - right[0]).mul_add(
        left[0] - right[0],
        (left[1] - right[1]).mul_add(
            left[1] - right[1],
            (left[2] - right[2]) * (left[2] - right[2]),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CoverageSample;

    fn key(island_id: u32, component_id: u32, faces: &[u32]) -> BlockingPatchKey {
        BlockingPatchKey {
            island_id,
            component_id,
            representative_face_id: faces[0],
            sorted_face_ids: faces.to_vec(),
            uncovered_area_mm2_bits: 1.5_f64.to_bits(),
            worst_gap: CoverageClosureGap::Finite(0.5),
        }
    }

    /// The two predicates differ on EXACTLY the censored band and nowhere
    /// else: sub-censor dust is not a verdict, and it IS work.
    ///
    /// This is the rule the co-owner ruled on 2026-08-14. Censoring the repair
    /// worklist as well as the verdict was measured to lose three models that
    /// had emitted the day before (`Skulker Zombie` 0.35 mm², `Tavey Badger`
    /// 0.21 mm², `zombie-3-a` 12.65 mm² — every one of them REAL, above-censor
    /// uncovered area), because the dust patches the censored worklist dropped
    /// had been nominating contacts that incidentally closed those holes.
    #[test]
    fn work_selection_is_uncensored_where_the_verdict_is_censored() {
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        // Dust: below the censor, so no verdict — but still real measured
        // uncovered area, so still work.
        for area in [f64::MIN_POSITIVE, 0.000_47, 0.06, censor] {
            assert!(
                !coverage_patch_blocks_verdict(area, false, false, censor),
                "{area} mm² is at or below the censor and must not refuse the model"
            );
            assert!(
                coverage_patch_needs_repair(area, false, false),
                "{area} mm² is measured uncovered area and must still be repaired"
            );
        }
        // A patch with no uncovered area at all is neither.
        assert!(!coverage_patch_blocks_verdict(0.0, false, false, censor));
        assert!(!coverage_patch_needs_repair(0.0, false, false));
    }

    /// Containment, on every axis: the uncensored worklist is a SUPERSET of
    /// the censored verdict set, so the controller can never refuse a model
    /// over a patch it declined to work on.
    #[test]
    fn every_patch_that_blocks_the_verdict_also_needs_repair() {
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        for area in [
            0.0,
            f64::MIN_POSITIVE,
            0.000_47,
            censor,
            f64::from_bits(censor.to_bits() + 1),
            0.650_5,
            12.65,
        ] {
            for cut_ambiguous in [false, true] {
                for assessment_failed in [false, true] {
                    if coverage_patch_blocks_verdict(area, cut_ambiguous, assessment_failed, censor)
                    {
                        assert!(
                            coverage_patch_needs_repair(area, cut_ambiguous, assessment_failed),
                            "area={area} cut_ambiguous={cut_ambiguous} \
                             assessment_failed={assessment_failed} blocks the verdict but was \
                             left off the repair worklist"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn blocking_predicate_matches_the_verifier_truth_table() {
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        for (area, cut_ambiguous, assessment_failed, expected) in [
            (0.0, false, false, false),
            (0.0, true, false, true),
            (0.0, false, true, true),
            (1e-12, false, false, false),
            (3.4, false, false, true),
        ] {
            assert_eq!(
                coverage_patch_blocks_verdict(area, cut_ambiguous, assessment_failed, censor),
                expected,
                "area={area} cut_ambiguous={cut_ambiguous} assessment_failed={assessment_failed}"
            );
        }
    }

    /// The registered censor is `voxel_pitch_mm²` and nothing else, and 0.16 is
    /// a CONSEQUENCE of the registered 0.4 mm pitch rather than a number this
    /// code carries. If someone re-registers the pitch, this test moves with it
    /// on the first line and pins the arithmetic on the second.
    #[test]
    fn the_censor_is_the_registered_voxel_pitch_squared_and_never_a_literal() {
        let constants = m0_constants().expect("registered constants");
        let pitch = constants.placeholders().voxel_pitch_mm().diagnostic().get();
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        assert_eq!(censor, pitch * pitch);
        assert!(
            (censor - 0.16).abs() < 1e-12,
            "at the currently registered 0.4 mm pitch the censor must be 0.16 mm², got {censor}"
        );
        // And the measurement request the gate and the controller share is
        // filled from exactly that value, so the measurement's own
        // `assessment_failed` stamp and this predicate censor identically.
        assert_eq!(
            registered_coverage_request()
                .expect("registered request")
                .patch_area_floor_mm2()
                .get(),
            censor
        );
    }

    /// SPEC-A3-A4-COVERAGE.md §4 is written as "GREATER THAN
    /// `voxel_pitch_mm**2`" with "area AT OR BELOW" censored, so the boundary
    /// itself is censored. Pinned because `>` and `>=` differ here on exactly
    /// one real value and nothing else in the suite would notice.
    #[test]
    fn area_exactly_at_the_censor_is_censored_and_the_next_representable_step_is_not() {
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        assert!(
            !coverage_patch_blocks_verdict(censor, false, false, censor),
            "the spec censors area AT OR BELOW the censor, so equality must not block"
        );
        assert!(
            coverage_patch_blocks_verdict(
                f64::from_bits(censor.to_bits() + 1),
                false,
                false,
                censor
            ),
            "one ULP above the censor is above it and must block"
        );
    }

    /// The two models this censor was ruled on, at their real measured areas.
    ///
    /// Deinonychus stops on a single one-triangle patch of 0.00047 mm², three
    /// orders of magnitude below one voxel footprint — sub-resolution dust that
    /// no support can seat on. The censor must clear it. Warhorse's worst
    /// single patch is far above the censor and must still fail: the censor
    /// bounds each patch, so a genuinely uncovered island stays fatal no matter
    /// how the rest of the model's dust is distributed.
    #[test]
    fn the_censor_clears_deinonychus_dust_and_still_fails_a_real_uncovered_patch() {
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        assert!(
            !coverage_patch_blocks_verdict(0.000_47, false, false, censor),
            "Deinonychus' single 0.00047 mm² triangle is below one voxel footprint"
        );
        assert!(
            coverage_patch_blocks_verdict(0.650_5, false, false, censor),
            "a 0.6505 mm² uncovered patch is four voxel footprints and must still fail"
        );
        // Per patch, not per model: thirteen separate sub-censor patches are
        // thirteen sub-resolution unknowns, never one 1.3 mm² hole.
        for _ in 0..13 {
            assert!(!coverage_patch_blocks_verdict(0.1, false, false, censor));
        }
    }

    #[test]
    fn islands_from_blocking_aggregates_by_island_and_component_like_the_verifier() {
        let patches = vec![key(2, 0, &[10, 11]), key(2, 0, &[12]), key(7, 1, &[20])];
        // Island 2 was nominated for twice and still has rows left; island 7
        // never had an eligible row at all. Those are the two different
        // stories the refusal must be able to tell apart.
        let mut tally = BTreeMap::new();
        tally.insert(
            (2, 0),
            KeyNomination {
                eligible_rows: 4,
                nominated_rows: 2,
            },
        );
        tally.insert((7, 1), KeyNomination::default());
        let islands = islands_from_blocking(&patches, &tally);
        assert_eq!(islands.len(), 2);
        let island_two = islands
            .iter()
            .find(|island| island.island_id == 2)
            .expect("island 2 present");
        assert_eq!(island_two.patch_count, 2);
        assert_eq!(island_two.sorted_face_ids, vec![10, 11, 12]);
        assert!((island_two.uncovered_area_mm2 - 3.0).abs() < 1e-9);
        assert_eq!(
            (
                island_two.nominated_repair_rows,
                island_two.eligible_repair_rows
            ),
            (2, 4)
        );
        let island_seven = islands
            .iter()
            .find(|island| island.island_id == 7)
            .expect("island 7 present");
        assert_eq!(
            (
                island_seven.nominated_repair_rows,
                island_seven.eligible_repair_rows
            ),
            (0, 0),
            "an island that never had an eligible repair row must be distinguishable from one \
             that was nominated for and never closed"
        );
    }

    #[test]
    fn worse_gap_prefers_no_routed_contact_over_any_finite_distance() {
        assert_eq!(
            worse_gap(
                CoverageClosureGap::Finite(10.0),
                CoverageClosureGap::NoRoutedContact
            ),
            CoverageClosureGap::NoRoutedContact
        );
        assert_eq!(
            worse_gap(
                CoverageClosureGap::Finite(1.0),
                CoverageClosureGap::Finite(2.0)
            ),
            CoverageClosureGap::Finite(2.0)
        );
    }

    // ---------------------------------------------------------------------
    // SPEC-A4 design §A.4 — the one-winner search rules N2, N3 and N4.
    //
    // These are decisions about ORDER over a list of (seat, route, score, pool
    // order) tuples. Nothing below needs a mesh, a plan or a model run, and
    // that is deliberate: routing them through a `SpecializedContactPlan` would
    // prove the plan builder rather than the rule Craig ruled on.
    // ---------------------------------------------------------------------

    fn sample(
        island_id: u32,
        component_id: u32,
        point_mm: [f64; 3],
        area_mm2: f64,
    ) -> CoverageSample {
        CoverageSample {
            island_id,
            face_id: 0,
            component_id,
            point_mm,
            area_mm2,
        }
    }

    fn ranked_row(
        island_id: u32,
        component_id: u32,
        order: usize,
        seat_mm: [f64; 3],
        marginal_area_mm2: f64,
    ) -> RankedRepairRow {
        RankedRepairRow {
            id: RepairRowId::for_search_rule_test(island_id, component_id, order as u64),
            key: (island_id, component_id),
            seat_mm,
            order,
            marginal_area_mm2,
            scored_from: Vec::new(),
        }
    }

    fn candidates(rows: Vec<RankedRepairRow>) -> RankedRepairCandidates {
        let mut route_uncovered_mm2: BTreeMap<(u32, u32), f64> = BTreeMap::new();
        for row in &rows {
            let entry = route_uncovered_mm2.entry(row.key).or_insert(0.0);
            *entry = entry.max(row.marginal_area_mm2);
        }
        let mut ranked = rows;
        sort_ranked_rows(&mut ranked);
        RankedRepairCandidates {
            ranked,
            per_key: BTreeMap::new(),
            route_uncovered_mm2,
        }
    }

    const R: f64 = 6.0;

    /// **Rule N2, and it is task #27's population in miniature.**
    ///
    /// `Skulker Zombie` refused with 0.35 mm² of REAL, above-censor uncovered
    /// area when the worklist was censored, because the row that would have
    /// closed the hole is owned by a sub-censor DUST route. Here the same
    /// shape: one seat owned by route (9, 0) whose own route carries 0.02 mm²
    /// of dust, sitting 1 mm from a 0.35 mm² hole owned by route (5, 0).
    ///
    /// Global scoring credits it for the hole. The `scored_from` share says so
    /// BY NAME, which is what §D.1 exists for — the sidecar can print "this
    /// contact closed island 5's hole" instead of leaving a reader to infer it.
    #[test]
    fn a_dust_routes_seat_is_credited_for_the_real_hole_it_closes() {
        let samples = vec![
            sample(9, 0, [0.0, 0.0, 0.0], 0.02),
            sample(5, 0, [1.0, 0.0, 0.0], 0.35),
        ];
        let scoring: Vec<usize> = (0..samples.len()).collect();
        let scores = score_candidates_globally(&[[0.0, 0.0, 0.0]], &samples, &scoring, R, 65_536);

        assert!(
            (scores[0].marginal_area_mm2 - 0.37).abs() < 1e-12,
            "the dust route's seat must score the hole it physically covers, \
             not only its own dust: {:?}",
            scores[0]
        );
        let hole_share = scores[0]
            .scored_from
            .iter()
            .find(|share| (share.island_id, share.component_id) == (5, 0))
            .expect("the hole's route must appear in scored_from by name");
        assert!((hole_share.area_mm2 - 0.35).abs() < 1e-12);
    }

    /// **The revert proof for rule N2.** Filter the scoring samples back to the
    /// candidate's own route — which is exactly what production does today
    /// (`nominate_repair_rows`) — and the same seat scores 0.02 mm², an eighth
    /// of the registered censor. Under one global ranking it would sit below
    /// every row on every larger route and never be reached.
    ///
    /// If this test ever fails, the test above has stopped proving rule N2 and
    /// is passing for some other reason.
    #[test]
    fn route_local_scoring_hides_the_hole_the_global_score_finds() {
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        let samples = vec![
            sample(9, 0, [0.0, 0.0, 0.0], 0.02),
            sample(5, 0, [1.0, 0.0, 0.0], 0.35),
        ];
        // The candidate is owned by route (9, 0), so route-local scoring sees
        // only sample 0.
        let route_local: Vec<usize> = samples
            .iter()
            .enumerate()
            .filter(|(_, sample)| (sample.island_id, sample.component_id) == (9, 0))
            .map(|(index, _)| index)
            .collect();
        let scores =
            score_candidates_globally(&[[0.0, 0.0, 0.0]], &samples, &route_local, R, 65_536);
        assert!((scores[0].marginal_area_mm2 - 0.02).abs() < 1e-12);
        assert!(
            scores[0].marginal_area_mm2 < censor,
            "route-local scoring must leave this seat below the censor — that is \
             the measured regression rule N2 exists to undo"
        );
    }

    /// The tiling is a work-splitting device, never a result-changing one. Two
    /// budgets that produce completely different tile shapes must produce
    /// bit-identical scores and bit-identical shares.
    #[test]
    fn the_matrix_budget_splits_the_work_and_never_moves_the_score() {
        let samples: Vec<CoverageSample> = (0..40_u16)
            .map(|index| {
                sample(
                    u32::from(index % 3),
                    0,
                    [f64::from(index) * 0.25, 0.0, 0.0],
                    0.01 * f64::from(index + 1),
                )
            })
            .collect();
        let scoring: Vec<usize> = (0..samples.len()).collect();
        let seats: Vec<[f64; 3]> = (0..7)
            .map(|index| [f64::from(index) * 1.5, 0.0, 0.0])
            .collect();

        let wide = score_candidates_globally(&seats, &samples, &scoring, R, 65_536);
        let narrow = score_candidates_globally(&seats, &samples, &scoring, R, 4);
        assert_eq!(
            wide, narrow,
            "the score is a physical quantity; the tile shape must not appear in it"
        );
    }

    /// **The scores are bit-identical across a rejection**, which is the half of
    /// §A.4's reranking claim that survived v2.1. A rejected trial drops its
    /// child plan, so the uncovered sample set and every candidate seat are
    /// exactly what they were — rescoring would be work with a provably
    /// identical result, and the controller therefore only RE-SORTS.
    #[test]
    fn a_rejection_cannot_move_a_score_so_only_the_order_is_rebuilt() {
        let samples = vec![
            sample(5, 0, [0.0, 0.0, 0.0], 1.25),
            sample(5, 0, [0.4, 0.0, 0.0], 0.75),
        ];
        let scoring: Vec<usize> = (0..samples.len()).collect();
        let seats = [[0.1, 0.0, 0.0], [9.9, 0.0, 0.0]];
        let before = score_candidates_globally(&seats, &samples, &scoring, R, 65_536);
        let after = score_candidates_globally(&seats, &samples, &scoring, R, 65_536);
        assert_eq!(before, after);
        assert_eq!(
            before[0].marginal_area_mm2.to_bits(),
            after[0].marginal_area_mm2.to_bits()
        );
    }

    /// **Rule N4, ruling R13 — design §E.5 case 11, the diversity fixture.**
    ///
    /// THREE equal-scoring candidates, and the third one is what makes this a
    /// test rather than a formality: after the rejection, TWO candidates are
    /// still live — one 0.5 mm from the rejected seat, inside its 6.0 mm
    /// coverage disc, and one 8 mm away, outside it. The inside one has the
    /// LOWER pool order, so score-then-pool-order picks it, and pool order is
    /// spatially contiguous, which is the measured "63 seats inside a 1.04 mm
    /// box" defect. N4 must pick the outside one instead.
    ///
    /// **Written this way deliberately.** With only two candidates the
    /// rejection leaves exactly ONE live row and the assertion passes whatever
    /// the sort key is — it would prove nothing. A sabotage run that zeroed
    /// N4's contribution to the sort key caught the two-row form passing green,
    /// so the fixture is the three-row form and the sabotage now fails as it
    /// should.
    #[test]
    fn after_a_rejection_the_next_winner_is_outside_the_rejected_seats_coverage_disc() {
        let rejected = ranked_row(5, 0, 1, [0.0, 0.0, 0.0], 2.0);
        let inside = ranked_row(5, 0, 2, [0.5, 0.0, 0.0], 2.0);
        let outside = ranked_row(5, 0, 3, [8.0, 0.0, 0.0], 2.0);
        let pool = candidates(vec![rejected.clone(), inside.clone(), outside.clone()]);

        let mut discs = DiversityDiscs::new(R);
        let served = BTreeMap::new();
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");

        // Lowest pool order wins the tie first, exactly as production ranks.
        let first = choose_winner(&pool, &BTreeSet::new(), &discs, &served, censor)
            .expect("a live candidate");
        assert_eq!(pool.ranked[first.index].id, rejected.id);

        // Reject it: ledger the row AND record its coverage disc.
        let mut ledgered = BTreeSet::new();
        ledgered.insert(rejected.id);
        discs.reject(rejected.seat_mm);

        let next = choose_winner(&pool, &ledgered, &discs, &served, censor)
            .expect("two live candidates remain");
        assert_eq!(
            pool.ranked[next.index].id, outside.id,
            "the next winner must be OUTSIDE the rejected seat's own coverage disc, even though \
             the inside one has the lower pool order and the identical score"
        );
        assert_eq!(
            next.diversity_discs_containing_seat, 0,
            "and the record must say the winner was not deprioritized"
        );

        // Completeness: the demoted neighbour is still tried, and the record
        // says it was demoted when it was.
        ledgered.insert(outside.id);
        discs.reject(outside.seat_mm);
        let last = choose_winner(&pool, &ledgered, &discs, &served, censor)
            .expect("deprioritization is not exclusion");
        assert_eq!(pool.ranked[last.index].id, inside.id);
        assert_eq!(
            last.diversity_discs_containing_seat, 1,
            "the instrument must say this seat sat inside exactly one rejected disc"
        );
    }

    /// **The revert proof for rule N4.** Hand the same three rows the same
    /// rejection with an EMPTY disc set — which is the controller without
    /// ruling R13 — and the next winner is the seat 0.5 mm from the one just
    /// rejected, paying a second full pipeline pass to re-prove what the first
    /// pass proved. If this ever fails, the fixture above has stopped testing
    /// N4 and is passing for some other reason.
    #[test]
    fn without_the_diversity_rule_the_next_winner_is_the_neighbouring_seat() {
        let rejected = ranked_row(5, 0, 1, [0.0, 0.0, 0.0], 2.0);
        let inside = ranked_row(5, 0, 2, [0.5, 0.0, 0.0], 2.0);
        let outside = ranked_row(5, 0, 3, [8.0, 0.0, 0.0], 2.0);
        let pool = candidates(vec![rejected.clone(), inside.clone(), outside]);

        let no_discs = DiversityDiscs::new(R);
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        let mut ledgered = BTreeSet::new();
        ledgered.insert(rejected.id);

        let next = choose_winner(&pool, &ledgered, &no_discs, &BTreeMap::new(), censor)
            .expect("two live candidates remain");
        assert_eq!(
            pool.ranked[next.index].id, inside.id,
            "without the disc set the search marches to the next pool order, 0.5 mm away"
        );
    }

    /// **Rule N4's completeness half, pinned rather than assumed.**
    /// Deprioritization is not exclusion: drain a fully-enclosed
    /// neighbourhood and every demoted candidate is eventually tried. Under
    /// ruling R10 nothing truncates the order, which is the ruling's own
    /// consistency argument for letting diversity reorder freely.
    #[test]
    fn every_deprioritized_candidate_is_eventually_tried() {
        // Five seats packed inside one 1 mm box — the measured pathology — plus
        // two well outside it.
        let mut rows = Vec::new();
        for index in 0..5_u32 {
            rows.push(ranked_row(
                5,
                0,
                index as usize,
                [f64::from(index) * 0.25, 0.0, 0.0],
                2.0,
            ));
        }
        rows.push(ranked_row(5, 0, 5, [20.0, 0.0, 0.0], 2.0));
        rows.push(ranked_row(5, 0, 6, [40.0, 0.0, 0.0], 2.0));
        let pool = candidates(rows);
        let total = pool.ranked.len();

        let mut discs = DiversityDiscs::new(R);
        let mut ledgered: BTreeSet<RepairRowId> = BTreeSet::new();
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        let mut demoted_and_tried = 0_usize;

        // Every trial is rejected — the worst case — so the loop ends only at
        // pool exhaustion, which is precisely what R10 says bounds it.
        while let Some(chosen) = choose_winner(&pool, &ledgered, &discs, &BTreeMap::new(), censor) {
            let row = &pool.ranked[chosen.index];
            if chosen.diversity_discs_containing_seat > 0 {
                demoted_and_tried += 1;
            }
            ledgered.insert(row.id);
            discs.reject(row.seat_mm);
        }

        assert_eq!(
            ledgered.len(),
            total,
            "no candidate may fall off the end of the order: deprioritization is not exclusion"
        );
        assert!(
            demoted_and_tried > 0,
            "the fixture must actually exercise demotion, or it proves nothing"
        );
    }

    /// **Rule N3 — the first-trial guarantee.** A 0.21 mm² route (the measured
    /// `Tavey Badger` shape, 1.3× the censor) is served before a 13.04 mm²
    /// route takes its SECOND contact. Greedy alone would never get there.
    #[test]
    fn the_first_trial_guarantee_serves_a_small_route_before_the_big_route_takes_a_second() {
        let big_first = ranked_row(74, 0, 1, [0.0, 0.0, 0.0], 13.041);
        let big_second = ranked_row(74, 0, 2, [30.0, 0.0, 0.0], 13.041);
        let small = ranked_row(3, 0, 3, [60.0, 0.0, 0.0], 0.21);
        let pool = candidates(vec![big_first.clone(), big_second, small.clone()]);
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        let discs = DiversityDiscs::new(R);

        // Nothing served yet: the biggest unserved route goes first.
        let first = choose_winner(&pool, &BTreeSet::new(), &discs, &BTreeMap::new(), censor)
            .expect("a live candidate");
        assert_eq!(pool.ranked[first.index].id, big_first.id);
        assert!(first.by_first_trial_guarantee);

        // Record that route (74, 0) has now been served at its current area,
        // and the small route — not the big route's second seat — wins.
        let mut served = BTreeMap::new();
        served.insert(
            (74_u32, 0_u32),
            pool.route_uncovered_mm2[&(74, 0)].to_bits(),
        );
        let second = choose_winner(&pool, &BTreeSet::new(), &discs, &served, censor)
            .expect("a live candidate");
        assert_eq!(
            pool.ranked[second.index].id, small.id,
            "the guarantee must reach a route whose whole area is 1.6% of the big one's"
        );
        assert!(second.by_first_trial_guarantee);
    }

    /// The serve set's threshold is the registered A3 censor and nothing new. A
    /// route whose best GLOBAL score is below it cannot, by SPEC §4, change the
    /// censored objective on its own, so a trial on it is provably wasted — and
    /// it never gets a guaranteed first trial. It stays in the pool: greedy may
    /// still reach it, and the zero-score guard is what ends the search.
    #[test]
    fn a_route_that_cannot_change_the_verdict_never_enters_the_serve_set() {
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        let real = ranked_row(5, 0, 1, [0.0, 0.0, 0.0], 1.5);
        let dust = ranked_row(9, 0, 2, [40.0, 0.0, 0.0], censor * 0.5);
        let pool = candidates(vec![real.clone(), dust.clone()]);
        let discs = DiversityDiscs::new(R);

        let mut served = BTreeMap::new();
        served.insert((5_u32, 0_u32), pool.route_uncovered_mm2[&(5, 0)].to_bits());

        // The only unserved route is the dust one, and it is below the censor,
        // so the serve set is EMPTY and the winner comes from the global
        // ranking instead — with the flag saying the guarantee did not choose it.
        let chosen = choose_winner(&pool, &BTreeSet::new(), &discs, &served, censor)
            .expect("a live candidate");
        assert_eq!(pool.ranked[chosen.index].id, real.id);
        assert!(!chosen.by_first_trial_guarantee);
    }

    /// A candidate that covers nothing is still ranked — SPEC §6 keeps
    /// zero-score rows eligible — but it can never be a serve-set first trial,
    /// because the zero-score guard (§B.3) is the serve-set threshold's own
    /// special case. The controller's guard is what stops the loop; this pins
    /// that the guard is reachable, i.e. that a zero-scoring row really can
    /// surface as the winner.
    #[test]
    fn a_zero_scoring_candidate_reaches_the_winner_slot_where_the_guard_can_see_it() {
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        let nothing = ranked_row(5, 0, 1, [0.0, 0.0, 0.0], 0.0);
        let pool = candidates(vec![nothing.clone()]);
        let chosen = choose_winner(
            &pool,
            &BTreeSet::new(),
            &DiversityDiscs::new(R),
            &BTreeMap::new(),
            censor,
        )
        .expect("the row is eligible, so it is ranked");
        assert_eq!(pool.ranked[chosen.index].marginal_area_mm2, 0.0);
        assert!(
            !chosen.by_first_trial_guarantee,
            "a route that covers nothing is below the censor and cannot be served"
        );
    }
}
