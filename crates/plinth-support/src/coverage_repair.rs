//! SPEC-A4 §B and §C: what one repair step must prove before it may be kept.
//!
//! The spec's own words, from `SPEC-A3-A4-COVERAGE.md` §6:
//!
//! > One winner at a time enters a full construct, rewire, brace/size, and
//! > verify transaction. It is accepted only when:
//! > 1. the coverage objective strictly improves lexicographically in this
//! >    order: total failing area, maximum failing-patch area, failing-patch
//! >    count, worst gap; and
//! > 2. every noncoverage hard-gate count is componentwise no worse.
//!
//! This module is those two clauses plus the third the design adds, and
//! nothing else: no search, no nomination, no transaction. The transaction
//! itself lives in [`crate::runner`], because it needs the sealed run's private
//! typestate machinery.
//!
//! # Why there is a clause 3
//!
//! §4's censor is **per patch**, and §6's objective sums over **failing**
//! patches. Those two facts together leave a hole a greedy search finds first:
//! a repair that splits one 0.30 mm² failing patch into two 0.15 mm² censored
//! patches drops the total failing area to zero, drops the maximum
//! failing-patch area to zero, drops the failing-patch count to zero — and the
//! model **seals** — without covering one extra square millimetre. That is
//! "converting residual uncovered area into success" by a route §6 does not
//! name.
//!
//! Under the old batch loop the move was hard to reach, because a 48-row append
//! masked it. One winner at a time makes it reachable in a single step, so the
//! guard lands with the controller and not after it.
//!
//! [`RepairGateVector::uncensored_uncovered_mm2`] is what closes it, and it is
//! robust as a **property**, not as a hope: the sample set and its area weights
//! are frozen at attempt 0 for the whole run, the sum never groups by patch,
//! never applies the censor and never reads an island id. So the only way to
//! reduce it is to move at least one sample from uncovered to covered, which is
//! coverage by definition. The metric and the goal are the same thing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::coverage_closure::{CoverageClosureGap, CoverageScoreShare};
use crate::specialized::{
    coverage_key, FrozenSpecializedGateVector, RepairRowId, SpecializedContactPlan,
};
use crate::CoverageObjective;

/// The noncoverage counts a committed repair step may never worsen — SPEC-A4
/// §6 clause 2, as this codebase can measure it today.
///
/// Every component is a count the run already keeps. Nothing here is derived,
/// estimated, or inferred from a distance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoncoverageVector {
    /// Regular terminals the CCO transcript retired with no winner
    /// (`FinalizedDispositionKind::UnroutedNoWinner`).
    ///
    /// Uncovered area follows these, so a repair that unroutes two terminals to
    /// route one is a regression **even when the area happens to fall**. This
    /// is the component that rejects the measured Crawler Zombie plateau: over
    /// its five flat rounds this count climbs 7 → 11 (design §B-O7).
    pub unrouted_no_winner: u32,
    /// Terminals skipped as quarantined (`FinalizedDispositionKind::SkippedQuarantined`).
    ///
    /// A quarantined contact is condemned before any arm is proposed and can
    /// never become routed coverage, so a step that quarantines more than it
    /// started with has made the plan worse in the one currency that matters.
    pub quarantined: u32,
    /// Specialized-minimum and island-birth contacts left unrouted.
    ///
    /// SPEC §6 calls this case "a hard failure". **This design adopts only the
    /// trial half of that reading**, deliberately and not silently: a TRIAL
    /// whose child plan leaves one unrouted is rejected and ledgered, because
    /// §7 says a discarded clone never becomes the accepted state. The
    /// accepted-state half — refusing a model outright — is NOT adopted here,
    /// because task #49 landed the measurement without the refusal
    /// (`SealedSupportAuthority::unrouted_specialized_contacts`, in its own
    /// words: "§6 calls this case a hard failure; this change measures and
    /// ledgers it without refusing on it"), and turning it on would refuse an
    /// uncounted set of corpus models. Comparing it componentwise here is real
    /// work — a step may never INCREASE it — without making that unmeasured
    /// behaviour change. Open item O-5.
    pub unrouted_specialized: u32,
    /// The plan's own frozen load-violation count, from
    /// `SpecializedGateEvidence::accepted`.
    pub load_violations: u32,
    /// The plan's own frozen unsupported-specialized count, same source.
    pub unsupported_specialized: u32,
}

/// The components of [`NoncoverageVector`], exhaustively, so a ledger row can
/// name WHICH count regressed instead of saying "something got worse".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoncoverageComponent {
    UnroutedNoWinner,
    Quarantined,
    UnroutedSpecialized,
    LoadViolations,
    UnsupportedSpecialized,
}

impl NoncoverageComponent {
    /// Every component. Exhaustive by construction: the match in
    /// [`NoncoverageVector::first_regression`] has no wildcard arm, so adding a
    /// component without deciding what clause 2 compares for it is a compile
    /// error rather than a silent hole.
    pub const ALL: [Self; 5] = [
        Self::UnroutedNoWinner,
        Self::Quarantined,
        Self::UnroutedSpecialized,
        Self::LoadViolations,
        Self::UnsupportedSpecialized,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::UnroutedNoWinner => "unrouted_no_winner",
            Self::Quarantined => "quarantined",
            Self::UnroutedSpecialized => "unrouted_specialized",
            Self::LoadViolations => "load_violations",
            Self::UnsupportedSpecialized => "unsupported_specialized",
        }
    }
}

impl NoncoverageVector {
    #[must_use]
    pub const fn component(&self, component: NoncoverageComponent) -> u32 {
        match component {
            NoncoverageComponent::UnroutedNoWinner => self.unrouted_no_winner,
            NoncoverageComponent::Quarantined => self.quarantined,
            NoncoverageComponent::UnroutedSpecialized => self.unrouted_specialized,
            NoncoverageComponent::LoadViolations => self.load_violations,
            NoncoverageComponent::UnsupportedSpecialized => self.unsupported_specialized,
        }
    }

    /// SPEC-A4 §6 clause 2: componentwise no worse.
    ///
    /// Returns the FIRST component that got worse, in
    /// [`NoncoverageComponent::ALL`] order, so the rejection can be named in
    /// the ledger with its before and after. `None` means clause 2 holds.
    #[must_use]
    pub fn first_regression(&self, before: &Self) -> Option<NoncoverageComponent> {
        NoncoverageComponent::ALL
            .into_iter()
            .find(|&component| self.component(component) > before.component(component))
    }

    /// The plan-frozen half of the vector, read off the **accepted** plan.
    ///
    /// Reading the pre-strip routed plan instead would compare a load partition
    /// SPEC §6 says must not exist by this point (task #49 removes every
    /// unrouted regular contact before rewire, bracing and repartition).
    #[must_use]
    pub fn from_accepted_plan(plan: &SpecializedContactPlan) -> Self {
        let accepted = plan.gate_evidence().accepted;
        Self {
            unrouted_no_winner: 0,
            quarantined: 0,
            unrouted_specialized: 0,
            load_violations: saturating_u32(accepted.load_violation_count),
            unsupported_specialized: saturating_u32(accepted.unsupported_specialized_count),
        }
    }

    /// Fill the transcript-derived half. Separated from
    /// [`Self::from_accepted_plan`] only because the two sources live behind
    /// different borrows in the runner; the controller always sets both.
    pub const fn with_transcript_counts(
        mut self,
        unrouted_no_winner: u32,
        quarantined: u32,
        unrouted_specialized: u32,
    ) -> Self {
        self.unrouted_no_winner = unrouted_no_winner;
        self.quarantined = quarantined;
        self.unrouted_specialized = unrouted_specialized;
        self
    }
}

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// One side of the accept test: everything a repair step is judged on, measured
/// at the same instant from the same lowered run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RepairGateVector {
    /// SPEC §6 clause 1's censored, lexicographic coverage objective.
    pub coverage: CoverageObjective,
    /// SPEC §6 clause 2's counts.
    pub noncoverage: NoncoverageVector,
    /// The design's clause 3: total UNCENSORED uncovered sample area, mm².
    pub uncensored_uncovered_mm2: f64,
}

/// Why a trial was not committed. Every variant is a MEASUREMENT, and each one
/// carries the numbers that produced it so the ledger can state the case rather
/// than assert it.
#[derive(Clone, Debug, PartialEq)]
pub enum RepairStepOutcome {
    Committed,
    /// Measured, and the censored objective did not strictly improve.
    NoImprovement {
        before: CoverageObjective,
        after: CoverageObjective,
    },
    /// Measured, and the UNCENSORED total uncovered area did not strictly
    /// fall — the anti-gaming rejection. A step that reaches THIS having passed
    /// clause 1 is a patch split caught in the act.
    NoAreaCovered {
        before_mm2: f64,
        after_mm2: f64,
    },
    /// Measured, and a noncoverage count got worse. The component is named.
    Regression {
        component: NoncoverageComponent,
        before: u32,
        after: u32,
    },
    /// The trial could not be built at all: Preparation, Routing, Rewire,
    /// Brace, Structure or Lowering refused the child plan. Today this kills
    /// the model (`RepairPlanRefused`); here it rolls back and the search goes
    /// on. Task #47 lands here.
    StageRefusal {
        phase: crate::SupportRunPhase,
        detail: String,
    },
    /// `append_regular_repair_rows` refused the row: occupied, duplicate
    /// physical seat, or unknown. Today this kills the model too.
    AppendRefused {
        detail: String,
    },
    /// The routed measurement itself refused on the trial.
    MeasurementRefused {
        detail: String,
    },
}

impl RepairStepOutcome {
    /// The rejection-histogram bucket, for §A.8's loud per-run report.
    #[must_use]
    pub const fn class(&self) -> &'static str {
        match self {
            Self::Committed => "committed",
            Self::NoImprovement { .. } => "no_improvement",
            Self::NoAreaCovered { .. } => "no_area_covered",
            Self::Regression { .. } => "regression",
            Self::StageRefusal { .. } => "stage_refusal",
            Self::AppendRefused { .. } => "append_refused",
            Self::MeasurementRefused { .. } => "measurement_refused",
        }
    }
}

/// **SPEC-A4 §6 clause 1 + clause 2, plus the design's clause 3, in one place.**
///
/// The three clauses are evaluated in the order the spec writes them and the
/// FIRST failure is what is reported, so a ledger row names the reason a reader
/// would name.
///
/// Clause 1 reuses `specialized::coverage_key` rather than restating the
/// lexicographic order: one implementation, and `numeric_cmp`'s infinity
/// handling (a route with no routed contact has a `+inf` worst gap) comes with
/// it. Note that clause 1 is STRICT (`<`) while clause 2 is non-strict (`<=`) —
/// the strictness is this design's, not `gate_not_worse`'s, and the drift test
/// beside this function asserts the two agree on the ORDER, never on the
/// strictness.
#[must_use]
pub fn repair_step_outcome(
    after: &RepairGateVector,
    before: &RepairGateVector,
) -> RepairStepOutcome {
    let after_key = coverage_key(FrozenSpecializedGateVector::coverage_only(after.coverage));
    let before_key = coverage_key(FrozenSpecializedGateVector::coverage_only(before.coverage));
    if after_key >= before_key {
        return RepairStepOutcome::NoImprovement {
            before: before.coverage,
            after: after.coverage,
        };
    }
    if let Some(component) = after.noncoverage.first_regression(&before.noncoverage) {
        return RepairStepOutcome::Regression {
            component,
            before: before.noncoverage.component(component),
            after: after.noncoverage.component(component),
        };
    }
    // Strict `<` on an f64 sum with no epsilon and no new constant, and it is
    // safe for a stated reason rather than by habit: the sum is over a subset
    // of a FIXED multiset of sample area weights, so any real coverage change
    // removes at least one whole weight from the subset. The difference is
    // either exactly zero — nothing was covered — or at least one sample's
    // area, which is many orders of magnitude above f64 summation noise at
    // these magnitudes. There is no near-tie to adjudicate.
    if after.uncensored_uncovered_mm2 >= before.uncensored_uncovered_mm2 {
        return RepairStepOutcome::NoAreaCovered {
            before_mm2: before.uncensored_uncovered_mm2,
            after_mm2: after.uncensored_uncovered_mm2,
        };
    }
    RepairStepOutcome::Committed
}

/// **SPEC-A4 §D.1 — one repair transaction, as it goes into the evidence
/// stream.**
///
/// Because a step changes the plan by exactly ONE contact, the difference
/// between its before- and after-measurement IS that contact's contribution.
/// There is no inference and no apportionment. That is the whole of §D, and it
/// is what the batch loop could never produce: an attempt that appended 266
/// rows and moved the area by 0.4 mm² could not say which row did it, or that
/// 265 rows did nothing.
///
/// **Deliberately NOT `Serialize`, and it is a disclosed deviation from
/// §D.2.** Stage 4 landed the half of the FU-57 format change the door forced
/// — the schema string is `plinth.coverage-refusal.v2` and `attempts` now
/// carries committed steps — but putting this STREAM on the wire needs
/// `RepairRowId` to gain a public wire encoding it has never had, and needs the
/// resulting payload re-frozen against measured corpus values. That is a
/// model-run obligation, not a code one. The stream is produced, ledgered,
/// PRINTED, and exposed on the sealed authority; persisting it is still owed.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverageRepairStep {
    /// Every transaction attempted, committed or not, from 1.
    pub trial: usize,
    /// Which commit this was, if it committed. `None` for a rejection.
    pub commit: Option<usize>,
    /// The winner, by exact pool identity, and where it sits.
    pub row_id: RepairRowId,
    pub seat_mm: [f64; 3],
    /// The route the row is drawn from …
    pub source_island_id: u32,
    pub source_component_id: u32,
    /// … and every route whose uncovered samples contributed to its GLOBAL
    /// score, with each route's share. Often the row's own route is NOT the
    /// largest contributor, and that cross-route effect is the uncensored
    /// worklist's whole justification.
    pub scored_from: Vec<CoverageScoreShare>,
    pub marginal_area_mm2: f64,
    /// Whether rule N3's first-trial guarantee chose this route rather than the
    /// global ranking. Makes the guarantee's cost measurable.
    pub chosen_by_first_trial_guarantee: bool,
    /// How many rejected coverage discs this seat lay inside when it was
    /// chosen (rule N4). **This is the instrument that says whether
    /// diversity-on-rejection did any work on this model** — a nonzero value
    /// means the seat was deprioritized and tried anyway, which is the
    /// completeness half of ruling R13.
    pub diversity_discs_containing_seat: usize,
    pub objective_before: CoverageObjective,
    /// `None` when the trial never reached a measurement.
    pub objective_after: Option<CoverageObjective>,
    /// Clause 3's quantity on both sides. A rejected step whose failing area
    /// fell to zero while THIS did not move is a patch split caught in the act,
    /// and the record names it as one.
    pub uncensored_before_mm2: f64,
    pub uncensored_after_mm2: Option<f64>,
    pub gates_before: NoncoverageVector,
    pub gates_after: Option<NoncoverageVector>,
    /// EVIDENCE ONLY. How many model faces task #49's carried-load repartition
    /// charged to no contact, on both sides. A step that creates ownerless
    /// faces is visible in the record; it is not rejected for it, because load
    /// eligibility is provenance and the routed coverage measurement is what
    /// rules on the face.
    pub ownerless_faces_before: usize,
    pub ownerless_faces_after: Option<usize>,
    /// EVIDENCE ONLY, and it is the class §B-O7 measured but deliberately did
    /// not act on: whether this trial's own appended contact was condemned at
    /// quarantine before any arm was proposed. On Crawler Zombie's five-round
    /// plateau every one of 50 appended contacts was, which is why nothing
    /// moved. Rejecting early on it would be a guess about a plan that had not
    /// been routed — `unrouted_no_winner` demonstrably moved 7 → 11 while those
    /// contacts were being appended — so the class is recorded and sized here
    /// rather than acted on.
    pub own_contact_quarantined: Option<bool>,
    /// Every route whose evidence moved, and by how much. Routes that did not
    /// move are omitted, so a committed step that touched one route carries one
    /// row.
    pub route_deltas: Vec<RouteDelta>,
    pub outcome: RepairStepOutcome,
}

/// One route's before/after evidence across a single transaction.
///
/// **Both quantities, deliberately.** `failing_*` is the CENSORED verdict set —
/// what SPEC §4 will refuse the model for — and `uncensored_*` is the task #27
/// worklist, a superset. §D.2's line "closed island 5's 0.36 mm² patch" is a
/// statement about the failing area, and it cannot be produced from the
/// uncensored area alone. The pair is also what makes the patch-splitting move
/// (§B.5 clause 3) VISIBLE even when it is rejected: a step whose failing area
/// fell to zero while the uncensored area did not move is a split caught in the
/// act.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteDelta {
    pub island_id: u32,
    pub component_id: u32,
    /// The CENSORED verdict area on this route, before and after.
    pub failing_area_before_mm2: f64,
    pub failing_area_after_mm2: f64,
    /// The UNCENSORED worklist area on this route, before and after.
    pub uncensored_area_before_mm2: f64,
    pub uncensored_area_after_mm2: f64,
    /// How many CENSORED verdict patches this route carried, before and after.
    pub failing_patches_before: usize,
    pub failing_patches_after: usize,
    /// How many UNCENSORED worklist patches this route carried, before and
    /// after. A step that halves one patch into two without covering anything
    /// moves this and not the area.
    pub patches_before: usize,
    pub patches_after: usize,
    /// The worst routed-contact gap over this route's UNCENSORED patches, on
    /// both sides. `None` when the route carried no patch on that side, which
    /// is a different statement from "the gap was zero" and is recorded as one.
    pub worst_gap_before: Option<CoverageClosureGap>,
    pub worst_gap_after: Option<CoverageClosureGap>,
}

/// **SPEC-A4 §A.8 — the loud per-run report, which is the price of having no
/// failed-tries cap (Craig's ruling R10).**
///
/// R10 removes the cap and replaces it with visibility, and that trade only
/// holds if the visibility is real. Every field here is printed on every path,
/// sealed or refused.
///
/// > **These are counters that REPORT; none of them DECIDES.** No branch in the
/// > controller reads any of them except `committed` against the registered
/// > bound, which is rulings R6 and R11's own rule. That distinction is the
/// > whole of the physics-first doctrine here: a number that steers is a cap, a
/// > number that is printed is evidence.
///
/// **Wall time is evidence, never a bound.** It is measured and printed and
/// nothing compares it to anything. A machine that runs the corpus slowly must
/// produce the same verdicts as one that runs it fast, or the verdicts were
/// never about the model.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RepairRunReport {
    /// Every transaction attempted, committed or not.
    pub trials: usize,
    /// How many became the accepted plan.
    pub commits: usize,
    /// The [`RepairStepOutcome`] histogram, by class name, ascending by name so
    /// two runs of the same model print the same line.
    pub rejections_by_class: BTreeMap<String, usize>,
    /// The repair loop's own elapsed wall time.
    pub wall_time_total_ms: u64,
    /// Minimum / median / maximum per trial, so one pathological trial is
    /// visible against many cheap ones instead of being averaged away.
    pub wall_time_per_trial_min_ms: u64,
    pub wall_time_per_trial_median_ms: u64,
    pub wall_time_per_trial_max_ms: u64,
    /// Eligible rows never tried when the loop stopped.
    pub candidates_remaining: usize,
    /// How many candidates rule N4 deprioritized …
    pub diversity_demotions: usize,
    /// … and how many of those were tried anyway. **This pair is the instrument
    /// that says whether ruling R13 worked**, and whether it kept its
    /// completeness promise.
    pub diversity_demotions_tried: usize,
}

impl RepairRunReport {
    /// Fold one finished trial's wall time in. Kept as a method so the min /
    /// median / max are derived in one place from the same sample list rather
    /// than three times from three.
    pub fn record_trial_times(&mut self, mut per_trial_ms: Vec<u64>) {
        self.wall_time_total_ms = per_trial_ms.iter().sum();
        if per_trial_ms.is_empty() {
            return;
        }
        per_trial_ms.sort_unstable();
        self.wall_time_per_trial_min_ms = per_trial_ms[0];
        self.wall_time_per_trial_max_ms = per_trial_ms[per_trial_ms.len() - 1];
        self.wall_time_per_trial_median_ms = per_trial_ms[per_trial_ms.len() / 2];
    }

    /// Print the summary line where an operator watching a long run will see
    /// it, on every path — sealed or refused.
    ///
    /// **Deliberately unconditional, and deliberately not behind a verbosity
    /// switch.** Ruling R10 removed the failed-tries cap and put visibility in
    /// its place; a cost that is only printed when someone asks for it is a
    /// cost that is hidden, and the trade R10 made would not hold.
    ///
    /// **Disclosed deviation from design §A.8.** The design names three
    /// destinations: the run log at INFO, the FU-57 sidecar, and the run
    /// report's per-model line. Two of the three are here — this line, and
    /// [`crate::CoverageClosureRefusal::repair`] / the sealed authority's own
    /// copy for the sidecar. `plinth-support` has no logging dependency (its
    /// whole dependency list is `geom_kernel, plinth-core, plinth-hollow,
    /// plinth-layers, plinth-mesh, rayon, serde, thiserror`), so this goes to
    /// stderr rather than to a logger. The run report's per-model line is a
    /// `plinth-pipeline` wiring step and travels with §G.1's stage 4.
    pub fn log_summary(&self) {
        eprintln!("{}", self.summary_line());
    }

    /// The one INFO line an operator watching a long run must see move.
    /// Deliberately one line and deliberately unconditional: a cost that is
    /// only printed when someone asks for it is a cost that is hidden.
    #[must_use]
    pub fn summary_line(&self) -> String {
        let histogram = self
            .rejections_by_class
            .iter()
            .map(|(class, count)| format!("{class}={count}"))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "coverage-repair: trials={} commits={} remaining={} \
             wall_total={}ms wall/trial min={}ms med={}ms max={}ms \
             diversity_demotions={} of_which_tried={} [{histogram}]",
            self.trials,
            self.commits,
            self.candidates_remaining,
            self.wall_time_total_ms,
            self.wall_time_per_trial_min_ms,
            self.wall_time_per_trial_median_ms,
            self.wall_time_per_trial_max_ms,
            self.diversity_demotions,
            self.diversity_demotions_tried,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::specialized::gate_not_worse;

    fn objective(
        failing_area: f64,
        max_patch: f64,
        count: usize,
        worst_gap: f64,
    ) -> CoverageObjective {
        CoverageObjective {
            failing_uncovered_area_mm2: failing_area,
            max_patch_area_mm2: max_patch,
            failing_patch_count: count,
            worst_gap_mm: worst_gap,
        }
    }

    fn vector(coverage: CoverageObjective, uncensored: f64) -> RepairGateVector {
        RepairGateVector {
            coverage,
            noncoverage: NoncoverageVector::default(),
            uncensored_uncovered_mm2: uncensored,
        }
    }

    /// **The anti-gaming fixture (design §E.5 case 8).** Without it clause 3 is
    /// untested, and a controller that omitted clause 3 would still pass every
    /// other test in this suite.
    ///
    /// The move: a 0.30 mm² failing patch is split into two 0.15 mm² censored
    /// patches. Nothing is covered — the uncensored total does not move by one
    /// bit — and yet ALL FOUR components of the censored objective go to zero,
    /// because neither half is a failing patch any more. Clause 1 passes. The
    /// model would seal.
    #[test]
    fn splitting_a_failing_patch_into_censored_halves_passes_clause_one_and_is_rejected_by_clause_three(
    ) {
        let censor = crate::coverage_closure::registered_coverage_censor_area_mm2()
            .expect("registered censor");
        let before_area = 0.30;
        let half = before_area / 2.0;
        assert!(
            before_area > censor && half <= censor,
            "the fixture must straddle the registered censor {censor} mm², \
             or it is not testing the hole it was written for"
        );

        let before = vector(objective(before_area, before_area, 1, 4.0), before_area);
        // After the split: no failing patch remains, so the censored objective
        // is all zeros — and the uncensored area is bit-identical.
        let after = vector(objective(0.0, 0.0, 0, 0.0), before_area);

        assert!(
            coverage_key(FrozenSpecializedGateVector::coverage_only(after.coverage))
                < coverage_key(FrozenSpecializedGateVector::coverage_only(before.coverage)),
            "the split really does improve clause 1 — that is the whole problem"
        );
        assert_eq!(
            repair_step_outcome(&after, &before),
            RepairStepOutcome::NoAreaCovered {
                before_mm2: before_area,
                after_mm2: before_area,
            },
            "clause 3 must reject a step that covered no sample"
        );
    }

    /// The revert proof for clause 3: delete the guard and the fixture above
    /// commits. Modelled here by evaluating clauses 1 and 2 alone, which is
    /// exactly what the controller would be without clause 3.
    #[test]
    fn without_clause_three_the_patch_split_would_commit() {
        let before = vector(objective(0.30, 0.30, 1, 4.0), 0.30);
        let after = vector(objective(0.0, 0.0, 0, 0.0), 0.30);
        let clauses_one_and_two_only =
            coverage_key(FrozenSpecializedGateVector::coverage_only(after.coverage))
                < coverage_key(FrozenSpecializedGateVector::coverage_only(before.coverage))
                && after
                    .noncoverage
                    .first_regression(&before.noncoverage)
                    .is_none();
        assert!(
            clauses_one_and_two_only,
            "if this ever fails, the anti-gaming fixture above has stopped \
             testing clause 3 and is passing for some other reason"
        );
    }

    /// A real repair — one that actually covers samples — passes all three.
    #[test]
    fn a_step_that_covers_real_area_commits() {
        let before = vector(objective(1.60, 1.60, 1, 7.0), 1.85);
        let after = vector(objective(0.40, 0.40, 1, 5.0), 0.62);
        assert_eq!(
            repair_step_outcome(&after, &before),
            RepairStepOutcome::Committed
        );
    }

    /// Clause 1 rejects a step that only tidies sub-censor dust: the uncensored
    /// total falls, so clause 3 is happy, and the censored objective does not
    /// move at all. This is the censor doing its job — §4 calls censored area
    /// "diagnostic only", so it may not justify committing a contact.
    #[test]
    fn a_step_that_only_tidies_dust_fails_clause_one() {
        let before = vector(objective(1.60, 1.60, 1, 7.0), 1.85);
        let after = vector(objective(1.60, 1.60, 1, 7.0), 1.79);
        assert!(matches!(
            repair_step_outcome(&after, &before),
            RepairStepOutcome::NoImprovement { .. }
        ));
    }

    /// **The measured Crawler Zombie plateau, as a unit test (design §B-O7).**
    /// Over its five flat rounds the model covered nothing and `unrouted_no_winner`
    /// climbed 7 → 11. Clause 1 rejects it first, and if a future change ever
    /// made the coverage move, clause 2 would still name the component.
    #[test]
    fn the_measured_plateau_is_rejected_and_the_regression_is_named() {
        let plateau = objective(
            16.625_364_190_080_948,
            16.433_943_572_110_365,
            2,
            f64::INFINITY,
        );
        let before = RepairGateVector {
            coverage: plateau,
            noncoverage: NoncoverageVector {
                unrouted_no_winner: 7,
                ..NoncoverageVector::default()
            },
            uncensored_uncovered_mm2: 16.825_113_205_629_89,
        };
        let after = RepairGateVector {
            coverage: plateau,
            noncoverage: NoncoverageVector {
                unrouted_no_winner: 11,
                ..NoncoverageVector::default()
            },
            uncensored_uncovered_mm2: 16.825_113_205_629_89,
        };
        assert!(matches!(
            repair_step_outcome(&after, &before),
            RepairStepOutcome::NoImprovement { .. }
        ));
        // And the same state, had the coverage improved by a hair, is still a
        // rejection — by clause 2, with the component named.
        let improved = RepairGateVector {
            coverage: objective(16.0, 16.0, 2, f64::INFINITY),
            uncensored_uncovered_mm2: 16.0,
            ..after
        };
        assert_eq!(
            repair_step_outcome(&improved, &before),
            RepairStepOutcome::Regression {
                component: NoncoverageComponent::UnroutedNoWinner,
                before: 7,
                after: 11,
            }
        );
    }

    /// Every component of clause 2 is live: none is declared and then never
    /// compared. Written as a loop over `ALL` so a component added without a
    /// comparison fails here instead of passing silently.
    #[test]
    fn every_noncoverage_component_can_reject_a_step() {
        let coverage_before = objective(1.60, 1.60, 1, 7.0);
        let coverage_after = objective(0.40, 0.40, 1, 5.0);
        for component in NoncoverageComponent::ALL {
            let before = RepairGateVector {
                coverage: coverage_before,
                noncoverage: NoncoverageVector::default(),
                uncensored_uncovered_mm2: 1.85,
            };
            let mut worse = NoncoverageVector::default();
            match component {
                NoncoverageComponent::UnroutedNoWinner => worse.unrouted_no_winner = 1,
                NoncoverageComponent::Quarantined => worse.quarantined = 1,
                NoncoverageComponent::UnroutedSpecialized => worse.unrouted_specialized = 1,
                NoncoverageComponent::LoadViolations => worse.load_violations = 1,
                NoncoverageComponent::UnsupportedSpecialized => worse.unsupported_specialized = 1,
            }
            let after = RepairGateVector {
                coverage: coverage_after,
                noncoverage: worse,
                uncensored_uncovered_mm2: 0.62,
            };
            assert_eq!(
                repair_step_outcome(&after, &before),
                RepairStepOutcome::Regression {
                    component,
                    before: 0,
                    after: 1,
                },
                "component {} does not reject a step that worsens it",
                component.name()
            );
        }
    }

    /// **The drift alarm §B.4 asks for**, and it asserts the thing that is
    /// actually true rather than the thing that sounds true.
    ///
    /// `gate_not_worse` is the merge tournament's NON-STRICT rule; the A4
    /// accept test is STRICT. They are not the same predicate and must not be
    /// asserted equal. What must hold is that they agree on the ORDER: whenever
    /// `coverage_key(a) < coverage_key(b)`, `a` is not-worse than `b` under
    /// `gate_not_worse` too. If someone re-orders `coverage_key`'s components,
    /// or changes `gate_not_worse`'s coverage half, this fails.
    #[test]
    fn the_a4_accept_test_and_gate_not_worse_agree_on_the_order() {
        let vectors = [
            objective(0.0, 0.0, 0, 0.0),
            objective(0.0, 0.0, 0, 3.0),
            objective(0.30, 0.30, 1, 4.0),
            objective(0.30, 0.30, 2, 4.0),
            objective(0.30, 0.25, 2, 4.0),
            objective(1.60, 1.60, 1, 7.0),
            objective(1.60, 1.60, 1, f64::INFINITY),
            objective(16.625, 16.433, 2, f64::INFINITY),
        ];
        for after in vectors {
            for before in vectors {
                let a = FrozenSpecializedGateVector::coverage_only(after);
                let b = FrozenSpecializedGateVector::coverage_only(before);
                if coverage_key(a) < coverage_key(b) {
                    assert!(
                        gate_not_worse(a, b),
                        "a strictly better A4 coverage key must never be 'worse' \
                         under the tournament's own rule: {after:?} vs {before:?}"
                    );
                }
            }
        }
    }

    /// A `+inf` worst gap — "this route has no routed contact at all" — must
    /// order as worse than any finite distance, not as a NaN and not as best.
    /// This is the one place the reuse of `coverage_key` earns its keep.
    #[test]
    fn no_routed_contact_orders_worse_than_any_finite_gap() {
        let finite = FrozenSpecializedGateVector::coverage_only(objective(1.0, 1.0, 1, 15.29));
        let none =
            FrozenSpecializedGateVector::coverage_only(objective(1.0, 1.0, 1, f64::INFINITY));
        assert!(coverage_key(finite) < coverage_key(none));
    }
}
