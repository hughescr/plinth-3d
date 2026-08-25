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
//! and registered request, and the SPEC-A4 §A.4 search rules
//! ([`rank_repair_candidates`], [`rank_eligible_candidates`],
//! [`choose_winner`]) that turn one measurement's uncovered patches — the
//! uncensored worklist — into a deterministic global ranking of repair-pool
//! rows and pick ONE winner from it.
//!
//! **The batch nominator went with the batch loop at §G.1 stage 4.**
//! `nominate_repair_rows` picked one row per patch per attempt and returned a
//! whole append set; there is no longer any caller that appends more than one
//! row, and its two exclusion rules live on verbatim in
//! [`rank_repair_candidates`], which is where its tests were moved.

use std::collections::{BTreeMap, BTreeSet};

use plinth_core::m0_constants;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::specialized::{RepairRowId, SpecializedContactPlan, SpecializedError};
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
/// contact and leaves the real hole open. docs/specs/coverage-repair.md §4 says this
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
/// `docs/specs/coverage-repair.md` §4 states the rule this predicate implements:
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
/// `docs/specs/coverage-repair.md` §4: "Area at or below `voxel_pitch_mm**2` is
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
    /// to transact.
    ///
    /// This is the successor to the retired `NoEligibleRepairRows`, and the
    /// difference is not cosmetic: the batch variant also fired MID-RUN, after
    /// several attempts had each appended a batch, so "no eligible rows" could
    /// mean either "the pool was empty from the start" or "this controller
    /// spent the pool". This one can only mean the first. Which routes remain
    /// and why is per-route evidence ([`RouteRepairOutcome`]), not a
    /// whole-model verdict.
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
    /// Always `Some` from the one-winner controller, which is the only
    /// controller. Kept `Option` and `serde(default)` so a sidecar written
    /// before the search existed still reads.
    #[serde(default)]
    pub outcome: Option<RouteRepairOutcome>,
}

/// The typed refusal FU-57 persists: the coverage search stopped with blocking
/// patches still measured. Boxed at its one call site
/// ([`crate::SupportRunError::CoverageClosure`]) because it carries the whole
/// trial stream and every still-uncovered island's evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoverageClosureRefusal {
    /// **Ruling R6: COMMITTED repair steps**, not passes of a loop. The field
    /// keeps the name of the registered bound it counts against
    /// (`coverage_repair_max_attempts`), and that bound bounds commits.
    pub attempts: usize,
    pub reason: CoverageClosureReason,
    pub islands: Vec<CoverageClosureIsland>,
    /// **SPEC-A4 §D — the trial stream, on the wire.**
    ///
    /// One row per transaction, committed or not, each naming the exact
    /// repair-pool row it proposed, the routes its GLOBAL score came from, both
    /// sides of all three accept clauses, and the outcome.
    ///
    /// **This replaced `history: Vec<CoverageClosureAttempt>` at §G.1 stage 4**,
    /// and the replacement is the point rather than a rename. A batch attempt
    /// row said "289 rows nominated, blocking count 3 → 3": it could not say
    /// which row did anything, because 289 rows moved at once. A step row says
    /// which single contact was tried, what it cost and what it changed. §D:
    /// "there is no inference and no apportionment".
    #[serde(default)]
    pub steps: Vec<crate::coverage_repair::CoverageRepairStepWire>,
    /// **SPEC-A4 §A.8, Craig's ruling R10 — the loud report, on the refused
    /// path.** R10 removes the failed-tries cap and replaces it with
    /// visibility, and that trade only holds if the visibility reaches the
    /// record. This is the sidecar half of it.
    ///
    /// Additive and `serde(default)`, so an older sidecar still reads.
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

pub(crate) fn worse_gap(left: CoverageClosureGap, right: CoverageClosureGap) -> CoverageClosureGap {
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
    /// **Rule N4, carried on the row rather than recomputed.** How many
    /// rejected coverage discs this seat lies inside. [`DiversityDiscs`]
    /// charges exactly ONE new disc per rejection in ONE pass over the
    /// ranking, which is the incremental bookkeeping §A.4 rule N4 budgets;
    /// reading it in a comparator is then O(1) instead of O(rejections).
    pub(crate) diversity_discs: usize,
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
/// **Where each rule lives, and why the split exists.** This function does
/// exactly one thing the plan is needed for — decide which pool rows are
/// ELIGIBLE — and then hands the whole eligible set, unfiltered and
/// untruncated, to [`rank_eligible_candidates`], which is where rules N1 and
/// N2 are actually carried out. The split is not cosmetic: it is what lets
/// both rules be proved by a fixture with two worklist routes and cross-route
/// samples instead of by a model run, and it is the fix for the challenger's
/// finding that the ranking's own rules had no direct test at all. Nothing
/// between the call and the return may filter, truncate or reorder the result;
/// the call IS the tail expression so that there is nowhere for such a step to
/// hide.
///
/// # Errors
///
/// Propagates [`SpecializedError`] from
/// [`SpecializedContactPlan::occupied_repair_rows`].
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

    let mut eligible: Vec<EligibleRepairCandidate> = Vec::new();
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
        eligible.push(EligibleRepairCandidate {
            id: row.id(),
            key,
            seat_mm: row.candidate().point_mm,
            order: row.order(),
        });
    }

    Ok(rank_eligible_candidates(
        &eligible,
        route_uncovered_mm2,
        plan.coverage_samples(),
        uncovered_samples,
        radius_mm,
        matrix_budget,
    ))
}

/// One repair-pool row that survived the three eligibility exclusions, carrying
/// everything the ranking needs and nothing that only the plan knows.
///
/// This type exists so that rules N1 and N2 can be exercised without a
/// `SpecializedContactPlan`. Eligibility is a question about a plan — which
/// rows are occupied, ledgered or already physically claimed. Ranking is not:
/// it is arithmetic over (seat, route, pool order) against a sample list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EligibleRepairCandidate {
    pub(crate) id: RepairRowId,
    /// The route that OWNS the row (SPEC §3's one-route rule, untouched).
    pub(crate) key: (u32, u32),
    pub(crate) seat_mm: [f64; 3],
    /// Pool order, the deterministic tie-break SPEC §6 names.
    pub(crate) order: usize,
}

/// **Rules N1 and N2, both of them, in the one function the production ranking
/// path calls.**
///
/// **N1 — there is no per-patch pick cap.** Every eligible row is scored and
/// every eligible row is ranked. The batch loop stopped at
/// `picked >= patches.len()`, one seat per patch per attempt, which is blind to
/// how large the hole is: the Rotting Warhorse's island 68 carries its whole
/// 11.879 mm² as a SINGLE patch and therefore received one seat per attempt —
/// 8 seats out of 377 — refusing with 361 eligible seats never tried. Under
/// one-winner transactions exactly one row is appended per step anyway, so a
/// nomination cap has no work left to do; all it did was ration attention.
/// **The rule is the `zip` below returning `eligible.len()` rows**, and
/// `ranked_rows_are_never_capped_by_the_patch_or_route_count` is its revert
/// proof.
///
/// **N2 — the score is global.** `scoring_samples` is EVERY uncovered sample on
/// every route, and it is never filtered to the candidate's own
/// `(island_id, component_id)`. The physical quantity SPEC §6 names — "marginal
/// uncovered sample area" — is the area the contact actually covers, and a
/// contact does not know about island ids. Route-local scoring under-values
/// exactly the rows task #27 exists to keep, and
/// `a_dust_routes_row_outranks_the_hole_owners_row_in_the_production_ranking`
/// is its revert proof: it goes red the moment a per-route filter reappears
/// anywhere on this path.
///
/// The only filtering done here is a bounds guard on the sample indices, which
/// is not a rule: an index past the end of the sample list describes no
/// geometry and cannot be scored.
pub(crate) fn rank_eligible_candidates(
    eligible: &[EligibleRepairCandidate],
    route_uncovered_mm2: BTreeMap<(u32, u32), f64>,
    samples: &[crate::CoverageSample],
    uncovered_samples: &[usize],
    radius_mm: f64,
    matrix_budget: usize,
) -> RankedRepairCandidates {
    let scoring_samples: Vec<usize> = uncovered_samples
        .iter()
        .copied()
        .filter(|&index| index < samples.len())
        .collect();
    let seats: Vec<[f64; 3]> = eligible.iter().map(|row| row.seat_mm).collect();
    let scores =
        score_candidates_globally(&seats, samples, &scoring_samples, radius_mm, matrix_budget);

    let mut ranked: Vec<RankedRepairRow> = eligible
        .iter()
        .zip(scores)
        .map(|(row, score)| RankedRepairRow {
            id: row.id,
            key: row.key,
            seat_mm: row.seat_mm,
            order: row.order,
            marginal_area_mm2: score.marginal_area_mm2,
            scored_from: score.scored_from,
            diversity_discs: 0,
        })
        .collect();
    sort_ranked_rows(&mut ranked);

    // A worklist route with no eligible row leaves a trace rather than
    // vanishing: "this route never had a single unoccupied, unfailed row" is a
    // different owner action from "rows were tried and none helped", and §E.1
    // reports them as different measured classes.
    let mut per_key: BTreeMap<(u32, u32), KeyNomination> = route_uncovered_mm2
        .keys()
        .map(|key| (*key, KeyNomination::default()))
        .collect();
    for row in eligible {
        per_key.entry(row.key).or_default().eligible_rows += 1;
    }

    RankedRepairCandidates {
        ranked,
        per_key,
        route_uncovered_mm2,
    }
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
/// candidate outside it, **at equal score**. Discs accumulate, and the count of
/// rejected discs a seat falls inside is the FIRST TIE-BREAK, ascending: "how
/// many times has this neighbourhood already failed" decides every question
/// that area has left tied, and none that it has already answered.
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
/// **AMENDED — the disc count is the FIRST TIE-BREAK, not the first key.**
/// §A.4 states the rule twice and the two sentences do not agree: "it sorts
/// after every candidate outside that disc, **at equal score**", and "the sort
/// key is the count of rejected discs it falls inside, ascending, applied
/// **before** score". Only the first is safe, and the unsafe one was measured
/// doing harm — on `make_overhang_table` 905 of 1,011 ranked rows score
/// 0.00 mm², so with the disc count ahead of score a seat covering NOTHING
/// outranks a live 13.68 mm² seat as soon as the latter falls inside one
/// rejected disc, and the search walks hundreds of full pipeline passes over
/// seats that cannot help. Score is the physical quantity; the disc count is a
/// statement about redundancy, and redundancy may only decide a question that
/// area has left tied. It still closes the whole of the measured defect,
/// because that defect IS a tie: the 63-seats-in-a-1.04 mm-box run ties on
/// absolute area and is ordered by spatially contiguous pool order. Recorded
/// as design addendum §A.4-N4a.
///
/// **Bookkeeping is incremental, as §A.4 ordered.** The count lives on
/// [`RankedRepairRow::diversity_discs`]; [`Self::reject`] charges exactly one
/// new disc in one pass over the ranking and [`Self::recount`] rebuilds the
/// counts after a COMMIT rescores the pool. Nothing recomputes a distance
/// inside a comparator.
#[derive(Clone, Debug, Default)]
pub(crate) struct DiversityDiscs {
    seats: Vec<[f64; 3]>,
    radius_sq: f64,
}

impl DiversityDiscs {
    pub(crate) fn new(radius_mm: f64) -> Self {
        Self {
            seats: Vec::new(),
            radius_sq: radius_mm * radius_mm,
        }
    }

    /// Record a rejected seat and charge its disc to the ranking in ONE pass —
    /// one new disc per rejection, exactly the incremental cost §A.4 budgets.
    ///
    /// Returns the rows this disc deprioritized for the FIRST time, which is
    /// §A.8's `diversity_demotions` instrument: that field counts CANDIDATES,
    /// not demotion events, so a row already inside another rejected disc must
    /// not be counted twice.
    pub(crate) fn reject(
        &mut self,
        seat_mm: [f64; 3],
        candidates: &mut RankedRepairCandidates,
    ) -> Vec<RepairRowId> {
        self.seats.push(seat_mm);
        let mut newly_demoted = Vec::new();
        for row in &mut candidates.ranked {
            if squared_distance(seat_mm, row.seat_mm) <= self.radius_sq {
                row.diversity_discs += 1;
                if row.diversity_discs == 1 {
                    newly_demoted.push(row.id);
                }
            }
        }
        newly_demoted
    }

    /// Recharge every accumulated disc onto a freshly rescored ranking.
    ///
    /// Only a COMMIT rebuilds the ranking, so this runs at most once per
    /// committed step and is bounded by the registered commit bound. A
    /// rejection never calls it — that is what [`Self::reject`]'s single pass
    /// is for.
    pub(crate) fn recount(&self, candidates: &mut RankedRepairCandidates) {
        for row in &mut candidates.ranked {
            row.diversity_discs = self
                .seats
                .iter()
                .filter(|&&seat| squared_distance(seat, row.seat_mm) <= self.radius_sq)
                .count();
        }
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
    served: &BTreeMap<(u32, u32), u64>,
    censor_area_mm2: f64,
) -> Option<ChosenCandidate> {
    // Score first — it is the physical quantity — then rule N4's disc count as
    // the FIRST tie-break, then pool order. See [`DiversityDiscs`] for why the
    // disc count may not lead: §A.4 states the rule both ways, and with the
    // count ahead of score a seat covering nothing outranks a live 13.68 mm²
    // seat (measured on `make_overhang_table`, 905 of 1,011 rows at 0.00 mm²).
    let rank_key = |row: &RankedRepairRow| {
        (
            OrderedScore(row.marginal_area_mm2),
            row.diversity_discs,
            row.order,
            row.key,
        )
    };
    // The same order WITHOUT the unique per-row fields, so that the serve set's
    // own tie-break has something left to decide. `order` and `key` are unique
    // per row, so folding them into the key the routes are compared on is what
    // made §A.4 N3's "larger route uncovered area" arm unreachable.
    let route_rank_key =
        |row: &RankedRepairRow| (OrderedScore(row.marginal_area_mm2), row.diversity_discs);

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
                // Ties on score and disc count break on LARGER route uncovered
                // area, then lower pool row — the guarantee serves the biggest
                // unserved hole first. Reachable because `route_rank_key`
                // deliberately omits `order` and `key`.
                let area = |row: &RankedRepairRow| {
                    OrderedScore(
                        candidates
                            .route_uncovered_mm2
                            .get(&row.key)
                            .copied()
                            .unwrap_or(0.0),
                    )
                };
                route_rank_key(left)
                    .cmp(&route_rank_key(right))
                    .then_with(|| area(left).cmp(&area(right)))
                    .then_with(|| left.order.cmp(&right.order))
                    .then_with(|| left.key.cmp(&right.key))
            });
    if let Some((index, row)) = best_served {
        return Some(ChosenCandidate {
            index,
            by_first_trial_guarantee: true,
            diversity_discs_containing_seat: row.diversity_discs,
        });
    }

    let (index, row) =
        live().min_by(|(_, left), (_, right)| rank_key(left).cmp(&rank_key(right)))?;
    Some(ChosenCandidate {
        index,
        by_first_trial_guarantee: false,
        diversity_discs_containing_seat: row.diversity_discs,
    })
}

/// **§B.3's own quantity: the best GLOBAL score any candidate still LIVE can
/// offer**, and 0.0 when none is live.
///
/// §B.3 words the stop guard as "when the best eligible candidate's global
/// score is 0.00 mm²"; §A.7's pseudocode says "winner". The code took the
/// winner reading and it was measured stopping the search on
/// `make_overhang_table` with 1,008 rows still live and the best of them
/// covering 13.68 mm² — a candidate that can physically help, which is exactly
/// what Craig's ruling 1 forbids giving up on. The guard reads this instead,
/// and the refusal reports this number rather than a hard-coded zero, so a
/// `NoImprovingRepair` that names 0.00 mm² is a measurement and not a claim.
pub(crate) fn best_live_score_mm2(
    candidates: &RankedRepairCandidates,
    ledgered: &BTreeSet<RepairRowId>,
) -> f64 {
    candidates
        .ranked
        .iter()
        .filter(|row| !ledgered.contains(&row.id))
        .map(|row| row.marginal_area_mm2)
        .fold(0.0_f64, f64::max)
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

    /// docs/specs/coverage-repair.md §4 is written as "GREATER THAN
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
            diversity_discs: 0,
        }
    }

    fn eligible(
        island_id: u32,
        component_id: u32,
        order: usize,
        seat_mm: [f64; 3],
    ) -> EligibleRepairCandidate {
        EligibleRepairCandidate {
            id: RepairRowId::for_search_rule_test(island_id, component_id, order as u64),
            key: (island_id, component_id),
            seat_mm,
            order,
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
        let mut pool = candidates(vec![rejected.clone(), inside.clone(), outside.clone()]);

        let mut discs = DiversityDiscs::new(R);
        let served = BTreeMap::new();
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");

        // Lowest pool order wins the tie first, exactly as production ranks.
        let first =
            choose_winner(&pool, &BTreeSet::new(), &served, censor).expect("a live candidate");
        assert_eq!(pool.ranked[first.index].id, rejected.id);

        // Reject it: ledger the row AND charge its coverage disc onto the
        // ranking, which is the one incremental pass §A.4 budgets.
        let mut ledgered = BTreeSet::new();
        ledgered.insert(rejected.id);
        let newly_demoted = discs.reject(rejected.seat_mm, &mut pool);
        assert!(
            newly_demoted.contains(&inside.id),
            "the neighbour inside the disc must be the row this rejection demoted"
        );

        let next =
            choose_winner(&pool, &ledgered, &served, censor).expect("two live candidates remain");
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
        discs.reject(outside.seat_mm, &mut pool);
        let last = choose_winner(&pool, &ledgered, &served, censor)
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
        // The disc set is never charged onto the pool, which is exactly the
        // controller without ruling R13: `diversity_discs` stays 0 everywhere.
        let pool = candidates(vec![rejected.clone(), inside.clone(), outside]);

        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        let mut ledgered = BTreeSet::new();
        ledgered.insert(rejected.id);

        let next = choose_winner(&pool, &ledgered, &BTreeMap::new(), censor)
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
        let mut pool = candidates(rows);
        let total = pool.ranked.len();

        let mut discs = DiversityDiscs::new(R);
        let mut ledgered: BTreeSet<RepairRowId> = BTreeSet::new();
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        let mut demoted_and_tried = 0_usize;

        // Every trial is rejected — the worst case — so the loop ends only at
        // pool exhaustion, which is precisely what R10 says bounds it.
        while let Some(chosen) = choose_winner(&pool, &ledgered, &BTreeMap::new(), censor) {
            let row = pool.ranked[chosen.index].clone();
            if chosen.diversity_discs_containing_seat > 0 {
                demoted_and_tried += 1;
            }
            ledgered.insert(row.id);
            discs.reject(row.seat_mm, &mut pool);
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

        // Nothing served yet: the biggest unserved route goes first.
        let first = choose_winner(&pool, &BTreeSet::new(), &BTreeMap::new(), censor)
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
        let second =
            choose_winner(&pool, &BTreeSet::new(), &served, censor).expect("a live candidate");
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

        let mut served = BTreeMap::new();
        served.insert((5_u32, 0_u32), pool.route_uncovered_mm2[&(5, 0)].to_bits());

        // The only unserved route is the dust one, and it is below the censor,
        // so the serve set is EMPTY and the winner comes from the global
        // ranking instead — with the flag saying the guarantee did not choose it.
        let chosen =
            choose_winner(&pool, &BTreeSet::new(), &served, censor).expect("a live candidate");
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
        let chosen = choose_winner(&pool, &BTreeSet::new(), &BTreeMap::new(), censor)
            .expect("the row is eligible, so it is ranked");
        assert_eq!(pool.ranked[chosen.index].marginal_area_mm2, 0.0);
        assert!(
            !chosen.by_first_trial_guarantee,
            "a route that covers nothing is below the censor and cannot be served"
        );
    }

    // ---------------------------------------------------------------------
    // Rules N1 and N2, exercised through the function the PRODUCTION ranking
    // path calls, on a pool with TWO worklist routes and cross-route samples.
    //
    // The challenger's finding was that neither rule was pinned anywhere the
    // production caller could feel it: re-adding the per-patch pick cap left
    // 544 tests green, and so did re-adding the route-local sample filter,
    // because the only N2 tests called `score_candidates_globally` on
    // hand-built sample lists and could not see a filter added in the caller.
    // Both sabotages now land inside `rank_eligible_candidates`, and both of
    // the tests below go red on them.
    // ---------------------------------------------------------------------

    /// A two-route worklist. Route (5, 0) is the REAL hole — 12 samples, one
    /// patch's worth of area, spread over 3 mm. Route (9, 0) is task #27's
    /// dust: one 0.02 mm² sample, and the row that owns it sits ON the real
    /// hole.
    fn two_route_worklist() -> (Vec<EligibleRepairCandidate>, Vec<CoverageSample>) {
        // The real hole: twelve samples at x = 0 … 11, summing to 4.26 mm².
        let mut samples: Vec<CoverageSample> = (0..12_u32)
            .map(|index| {
                sample(
                    5,
                    0,
                    [f64::from(index), 0.0, 0.0],
                    0.30 + f64::from(index) * 0.01,
                )
            })
            .collect();
        // The dust: one 0.02 mm² sample at the midpoint of the hole.
        samples.push(sample(9, 0, [5.5, 0.0, 0.0], 0.02));

        // Nine rows on the hole's OWN route, seated beyond the hole's far end,
        // so the best of them reaches six of the twelve samples …
        let mut rows: Vec<EligibleRepairCandidate> = (0..9)
            .map(|index| {
                eligible(
                    5,
                    0,
                    index,
                    [
                        12.0 + f64::from(u32::try_from(index).unwrap_or(0)) * 2.0,
                        0.0,
                        0.0,
                    ],
                )
            })
            .collect();
        // … and ONE row owned by the dust route, seated at the hole's midpoint,
        // where its 6.0 mm coverage disc reaches all twelve.
        rows.push(eligible(9, 0, 9, [5.5, 0.0, 0.0]));
        (rows, samples)
    }

    fn worklist_areas() -> BTreeMap<(u32, u32), f64> {
        let mut areas = BTreeMap::new();
        areas.insert((5_u32, 0_u32), 4.26);
        areas.insert((9_u32, 0_u32), 0.02);
        areas
    }

    /// **Rule N1, in the production ranking path.** Every eligible row is
    /// ranked. There are ten of them across two routes and one patch's worth of
    /// area per route, so any cap keyed to the patch count, the route count or
    /// anything else that is not "how many rows are eligible" shortens this
    /// list — which is precisely the `picked >= patches.len()` throttle that
    /// gave the Rotting Warhorse's 11.879 mm² single-patch island 8 seats out
    /// of 377.
    ///
    /// This is the revert proof the challenger asked for: re-adding
    /// `ranked.truncate(route_uncovered_mm2.len())` after the sort turns the
    /// ten into two and this assertion fails.
    #[test]
    fn ranked_rows_are_never_capped_by_the_patch_or_route_count() {
        let (rows, samples) = two_route_worklist();
        let scoring: Vec<usize> = (0..samples.len()).collect();
        let ranked =
            rank_eligible_candidates(&rows, worklist_areas(), &samples, &scoring, R, 65_536);

        assert_eq!(
            ranked.ranked.len(),
            rows.len(),
            "every eligible row must be ranked: rule N1 deletes the per-patch pick cap, and a \
             ranking shorter than the eligible set is that cap by another name"
        );
        assert!(
            ranked.ranked.len() > ranked.route_uncovered_mm2.len(),
            "the fixture must have more eligible rows than routes, or a per-route cap would \
             pass it"
        );
        assert_eq!(ranked.per_key[&(5, 0)].eligible_rows, 9);
        assert_eq!(ranked.per_key[&(9, 0)].eligible_rows, 1);
    }

    /// **Rule N2, in the production ranking path, with two routes and
    /// cross-route samples.** The dust route's row sits on the real hole, so it
    /// must be credited for the hole and outrank every row of the hole's own
    /// route. Under a route-local sample filter it scores its own 0.02 mm² of
    /// dust — an eighth of the registered censor — and sinks to last, which is
    /// exactly the regression that lost `Skulker Zombie`, `Tavey Badger` and
    /// `zombie-3-a`.
    ///
    /// Unlike the pair that scores hand-built sample lists, this goes through
    /// the caller, so a filter re-added anywhere on the ranking path is red.
    #[test]
    fn a_dust_routes_row_outranks_the_hole_owners_row_in_the_production_ranking() {
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        let (rows, samples) = two_route_worklist();
        let scoring: Vec<usize> = (0..samples.len()).collect();
        let ranked =
            rank_eligible_candidates(&rows, worklist_areas(), &samples, &scoring, R, 65_536);

        let winner = &ranked.ranked[0];
        assert_eq!(
            winner.key,
            (9, 0),
            "the seat owned by the 0.02 mm² dust route covers the whole 4.26 mm² hole next \
             door, so global scoring must rank it first; route-local scoring ranks it last"
        );
        assert!(
            winner.marginal_area_mm2 > censor,
            "and it must clear the censor on the hole it closes, not on its own dust: {} mm²",
            winner.marginal_area_mm2
        );
        let hole_share = winner
            .scored_from
            .iter()
            .find(|share| (share.island_id, share.component_id) == (5, 0))
            .expect("the hole's route must be named in scored_from");
        assert!(
            hole_share.area_mm2 > 4.0,
            "the cross-route share is the evidence §D.2 prints: {hole_share:?}"
        );
        let own_share = winner
            .scored_from
            .iter()
            .find(|share| (share.island_id, share.component_id) == (9, 0))
            .expect("the row's own route must be named too — global means EVERY route");
        assert!(
            (own_share.area_mm2 - 0.02).abs() < 1e-12,
            "a filter that keeps one route's samples and drops another's is still route-local, \
             whichever route it keeps: {own_share:?}"
        );
    }

    /// **The order the challenger measured breaking.** With the disc count
    /// ahead of score, a seat covering NOTHING outranks a live 13.68 mm² seat
    /// as soon as the good seat falls inside one rejected disc — measured on
    /// `make_overhang_table`, where 905 of 1,011 ranked rows score 0.00 mm².
    /// Score leads; the disc count decides only what area left tied.
    #[test]
    fn a_seat_that_covers_nothing_never_outranks_a_seat_that_covers_something() {
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        let good = ranked_row(5, 0, 1, [0.0, 0.0, 0.0], 13.678);
        let neighbour = ranked_row(5, 0, 2, [0.5, 0.0, 0.0], 11.288);
        let covers_nothing = ranked_row(5, 0, 3, [80.0, 0.0, 0.0], 0.0);
        let mut pool = candidates(vec![
            good.clone(),
            neighbour.clone(),
            covers_nothing.clone(),
        ]);
        let mut discs = DiversityDiscs::new(R);

        // Reject the best seat. Its disc now contains the 11.288 mm² neighbour
        // and does NOT contain the row 80 mm away that covers nothing.
        let mut ledgered = BTreeSet::new();
        ledgered.insert(good.id);
        discs.reject(good.seat_mm, &mut pool);

        let mut served = BTreeMap::new();
        served.insert((5_u32, 0_u32), pool.route_uncovered_mm2[&(5, 0)].to_bits());
        let next =
            choose_winner(&pool, &ledgered, &served, censor).expect("two live candidates remain");
        assert_eq!(
            pool.ranked[next.index].id, neighbour.id,
            "a demoted 11.288 mm² seat still beats a 0.00 mm² seat: the disc count is a \
             tie-break, and area is the physical quantity"
        );
        assert_eq!(
            next.diversity_discs_containing_seat, 1,
            "and the record must still say it was demoted"
        );

        assert!(
            (best_live_score_mm2(&pool, &ledgered) - 11.288).abs() < 1e-12,
            "§B.3's guard reads the best LIVE score, which is what stops the loop giving up on \
             a candidate that can physically help"
        );
    }

    /// **The composite property the two blocking findings broke between them:
    /// while ANY live row covers something, the winner covers something.**
    ///
    /// This is what makes §B.3's stop guard and §A.7's search agree. With the
    /// disc count ahead of score, a pool that is mostly zero-scoring — 905 of
    /// 1,011 rows on `make_overhang_table` — hands the winner slot to a seat
    /// that covers nothing after a few rejections, and every such winner costs
    /// a full prepare → route → rewire → brace → structure → lower → measure
    /// pass to learn nothing. Rejecting rows one at a time, the property must
    /// hold at every step of the drain, not just at the start.
    #[test]
    fn while_any_live_row_covers_something_the_winner_covers_something() {
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        // Three seats that cover real area, packed inside one coverage disc —
        // the measured "63 seats in a 1.04 mm box" shape — and seventeen that
        // cover nothing, spread far apart so no rejected disc ever reaches them.
        let mut rows: Vec<RankedRepairRow> = (0..3_u32)
            .map(|index| {
                ranked_row(
                    5,
                    0,
                    index as usize,
                    [f64::from(index) * 0.5, 0.0, 0.0],
                    13.678 - f64::from(index),
                )
            })
            .collect();
        for index in 3..20_u32 {
            rows.push(ranked_row(
                5,
                0,
                index as usize,
                [f64::from(index) * 20.0, 0.0, 0.0],
                0.0,
            ));
        }
        let mut pool = candidates(rows);
        let mut discs = DiversityDiscs::new(R);
        let mut ledgered: BTreeSet<RepairRowId> = BTreeSet::new();
        let mut served = BTreeMap::new();
        served.insert((5_u32, 0_u32), pool.route_uncovered_mm2[&(5, 0)].to_bits());

        let mut covering_winners = 0_usize;
        while let Some(chosen) = choose_winner(&pool, &ledgered, &served, censor) {
            let winner = pool.ranked[chosen.index].clone();
            let best_live = best_live_score_mm2(&pool, &ledgered);
            if best_live > 0.0 {
                assert!(
                    winner.marginal_area_mm2 > 0.0,
                    "the best live candidate covers {best_live} mm², so the winner may not be a \
                     seat that covers nothing — that is a full pipeline pass spent to learn what \
                     the score already said"
                );
                covering_winners += 1;
            }
            ledgered.insert(winner.id);
            discs.reject(winner.seat_mm, &mut pool);
        }
        assert_eq!(
            covering_winners, 3,
            "all three covering seats must be tried before any zero-scoring seat is"
        );
    }

    /// **§B.3's guard, at its own words.** The best live score is what says
    /// whether any candidate can still help. It falls to zero only when every
    /// live row covers nothing — and a ledgered row, however large, does not
    /// count, because it has already been tried and rolled back.
    #[test]
    fn the_best_live_score_ignores_ledgered_rows_and_reaches_zero_only_when_nothing_covers() {
        let big = ranked_row(5, 0, 1, [0.0, 0.0, 0.0], 13.678);
        let nothing = ranked_row(5, 0, 2, [80.0, 0.0, 0.0], 0.0);
        let pool = candidates(vec![big.clone(), nothing]);

        assert!((best_live_score_mm2(&pool, &BTreeSet::new()) - 13.678).abs() < 1e-12);
        let mut ledgered = BTreeSet::new();
        ledgered.insert(big.id);
        assert_eq!(
            best_live_score_mm2(&pool, &ledgered),
            0.0,
            "with the only covering row spent, nothing left can change the objective"
        );
    }

    /// **Rule N3's second tie-break, which was unreachable.** `rank_key`
    /// carried `order` and `key`, both unique per row, so the serve set's
    /// "larger route uncovered area" arm could never be consulted and the
    /// SMALLER route won on pool order. Two unserved routes, equal-scoring best
    /// candidates, and the 13.041 mm² route must go first.
    #[test]
    fn the_serve_set_breaks_a_score_tie_on_the_larger_route_not_on_pool_order() {
        let censor = registered_coverage_censor_area_mm2().expect("registered censor");
        let small = ranked_row(3, 0, 1, [0.0, 0.0, 0.0], 2.0);
        let big = ranked_row(74, 0, 2, [40.0, 0.0, 0.0], 2.0);
        let mut pool = candidates(vec![small.clone(), big.clone()]);
        // `candidates` derives each route's area from its best score, which is
        // equal here by construction; state the measured areas instead.
        pool.route_uncovered_mm2.insert((3, 0), 0.21);
        pool.route_uncovered_mm2.insert((74, 0), 13.041);

        let chosen =
            choose_winner(&pool, &BTreeSet::new(), &BTreeMap::new(), censor).expect("a candidate");
        assert!(chosen.by_first_trial_guarantee);
        assert_eq!(
            pool.ranked[chosen.index].id, big.id,
            "the guarantee serves the biggest unserved hole first; pool order decides only \
             what route area has left tied"
        );
    }

    /// **Rule N4's bookkeeping is incremental, and `recount` agrees with it.**
    /// §A.4 ordered "one new disc per rejection, so each rerank adds one pass
    /// over the remaining candidates" — the landed code instead recomputed
    /// `discs_containing` inside a `min_by` comparator and swept the pool a
    /// second time per rejection. The counts now live on the rows; this pins
    /// that charging discs one at a time gives the same answer as recomputing
    /// all of them, so the cheap path cannot drift from the honest one.
    #[test]
    fn charging_one_disc_per_rejection_agrees_with_recomputing_every_disc() {
        let rows: Vec<RankedRepairRow> = (0..8_u32)
            .map(|index| ranked_row(5, 0, index as usize, [f64::from(index), 0.0, 0.0], 2.0))
            .collect();
        let mut incremental = candidates(rows.clone());
        let mut discs = DiversityDiscs::new(R);
        let mut reported: Vec<RepairRowId> = Vec::new();
        for seat in [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [7.0, 0.0, 0.0]] {
            reported.extend(discs.reject(seat, &mut incremental));
        }
        // §A.8's `diversity_demotions` counts CANDIDATES, not demotion events:
        // "how many candidates rule N4 deprioritized". These discs overlap
        // heavily, so reporting a row on every disc that reaches it would
        // inflate the one instrument that says whether ruling R13 did any work.
        let unique: BTreeSet<RepairRowId> = reported.iter().copied().collect();
        assert_eq!(
            unique.len(),
            reported.len(),
            "a candidate may be reported as newly deprioritized exactly once, however many \
             rejected discs later reach it"
        );

        let mut rebuilt = candidates(rows);
        discs.recount(&mut rebuilt);
        assert_eq!(
            incremental, rebuilt,
            "the incremental charge and the full recount must produce the same counts, or the \
             ranking after a commit would disagree with the ranking after a rejection"
        );
        assert!(
            incremental.ranked.iter().any(|row| row.diversity_discs > 1),
            "the fixture must contain a seat inside more than one rejected disc, or it does not \
             exercise accumulation"
        );
    }
}
