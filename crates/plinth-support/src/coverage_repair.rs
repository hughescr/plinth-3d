//! SPEC-A4 §B and §C: what one repair step must prove before it may be kept.
//!
//! The spec's own words, from `docs/specs/coverage-repair.md` §6:
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
use std::path::{Path, PathBuf};

use geom_kernel::vec3::{dot, sub};
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

    /// This component's position in [`Self::ALL`], which is also its bit in
    /// [`NoncoverageComponentSet`] and its slot in
    /// [`RepairVetoCounts::gate_rise`]. One numbering, defined once.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::UnroutedNoWinner => 0,
            Self::Quarantined => 1,
            Self::UnroutedSpecialized => 2,
            Self::LoadViolations => 3,
            Self::UnsupportedSpecialized => 4,
        }
    }

    const fn bit(self) -> u8 {
        1_u8 << self.index()
    }
}

/// **A set of [`NoncoverageComponent`]s, as five bits.**
///
/// [`NoncoverageVector::first_regression`] names the ONE component clause 2
/// vetoes on. This is the set behind it, and the difference is the whole
/// reason it exists: **the clause-2 veto is not where the hard gates actually
/// move.** MEASURED 2026-08-28 over the four fixtures that enter the repair
/// loop (`docs/evidence/remediation-2026-08-26/repair-search-baseline.md`),
/// across 1,062 trials the clause-2 veto fired **zero** times, while a
/// hard-gate count ROSE on **176 of the 321** trials that left a step stream —
/// always by exactly +1, and only ever `unrouted_no_winner` (93) or
/// `quarantined` (83); the other three components never moved once. Clause 1
/// rejected every one of those 176 first, so the rise was measured and thrown
/// away unnamed.
///
/// **Five bits and not a `BTreeSet`** because the ranking reads this per
/// candidate row: a set that allocates would put a heap call where a
/// comparator goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoncoverageComponentSet(u8);

impl NoncoverageComponentSet {
    /// No component moved. The value a trial that never reached a measurement
    /// carries, which is why [`RepairVeto::gates_measured`] exists beside it.
    pub const EMPTY: Self = Self(0);

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn contains(self, component: NoncoverageComponent) -> bool {
        self.0 & component.bit() != 0
    }

    pub const fn insert(&mut self, component: NoncoverageComponent) {
        self.0 |= component.bit();
    }

    /// How many components are in the set. Named `count` rather than `len`
    /// because this is a bit population, not a container length.
    #[must_use]
    pub const fn count(self) -> usize {
        self.0.count_ones() as usize
    }

    /// The members, in [`NoncoverageComponent::ALL`] order.
    pub fn iter(self) -> impl Iterator<Item = NoncoverageComponent> {
        NoncoverageComponent::ALL
            .into_iter()
            .filter(move |&component| self.contains(component))
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

    /// **Every component that rose, not just the first — the same comparison,
    /// unfolded.**
    ///
    /// [`Self::first_regression`] answers clause 2's question ("may this step
    /// be kept?"), so it stops at the first `>`. This answers the ledger's
    /// question ("what did this trial do to the hard gates?"), which is asked
    /// on trials clause 1 has ALREADY rejected — where clause 2 never runs and
    /// the rise is real all the same. See [`NoncoverageComponentSet`] for the
    /// measurement that makes the distinction load-bearing.
    ///
    /// The two are the same predicate over the same order: the set is empty
    /// exactly when `first_regression` is `None`, and `first_regression` is
    /// always the set's first member. A test beside this pins both directions,
    /// so the day a component is added the two cannot drift apart.
    #[must_use]
    pub fn risen_components(&self, before: &Self) -> NoncoverageComponentSet {
        let mut risen = NoncoverageComponentSet::EMPTY;
        for component in NoncoverageComponent::ALL {
            if self.component(component) > before.component(component) {
                risen.insert(component);
            }
        }
        risen
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
    /// **Measured, and the trial's OWN appended contact never became an arm.**
    ///
    /// The plan that was measured is the accepted plan plus a contact the
    /// pipeline threw away — quarantined at attestation, or routed and retired
    /// with no winning arm — so the thing the trial proposed was never
    /// actually tested. Reporting that as [`Self::NoImprovement`] said "the
    /// improvement was measured and it was not one", which is a claim nobody
    /// made: no improvement was ever put in front of the measurement.
    ///
    /// **MEASURED, 2026-09-01, three models, 132 rejected trials.** Round 25
    /// 61 of 61, `make_raking_blade` 19 of 19, `Morel base` 52 of 55 — and
    /// `Morel base`'s three NON-condemned trials are exactly its three
    /// commits. Every one of the 132 was previously filed as `no_improvement`.
    OwnContactCondemned {
        condemnation: OwnContactCondemnation,
    },
}

/// **What the pipeline did with the ONE contact a repair trial appended**, in
/// the shape that goes on the wire.
///
/// This is `crate::runner::TrialContactDisposition`'s two CONDEMNED arms and
/// nothing else: `Routed` is not a condemnation and `NotInCadence` means the
/// contact never appeared, which is a different statement from being thrown
/// away. It is a projection of a value the routing cadence already sealed —
/// the same rule every field of [`OwnContactNoWinnerEvidence`] travels under —
/// so nothing here re-measures a quarantine or re-runs a route.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnContactCondemnation {
    /// Condemned before any arm was proposed. `remaining_inside_axis_mm` is the
    /// measured stub-axis re-entry depth when the initial build's condemned
    /// ledger holds one; `None` means the quarantine had a different cause and
    /// this refuses to name one it did not measure.
    Quarantined {
        remaining_inside_axis_mm: Option<f64>,
    },
    /// Routed and retired with no winning arm, carrying the transcript row's
    /// own sealed reason.
    UnroutedNoWinner(OwnContactNoWinnerEvidence),
}

impl RepairStepOutcome {
    /// The rejection-histogram bucket, for §A.8's loud per-run report.
    ///
    /// Every bucket but `committed` is read off [`RepairVetoClass::name`]
    /// rather than spelled again here, so the histogram in the report and the
    /// tally in the veto memory cannot end up with two vocabularies for one
    /// set of outcomes.
    #[must_use]
    pub const fn class(&self) -> &'static str {
        match self.veto_class() {
            Some(class) => class.name(),
            None => "committed",
        }
    }

    /// Which veto class this outcome is, or `None` for a COMMIT — which is not
    /// a veto and must never be folded into the veto memory as one.
    #[must_use]
    pub const fn veto_class(&self) -> Option<RepairVetoClass> {
        match self {
            Self::Committed => None,
            Self::NoImprovement { .. } => Some(RepairVetoClass::NoImprovement),
            Self::NoAreaCovered { .. } => Some(RepairVetoClass::NoAreaCovered),
            Self::Regression { .. } => Some(RepairVetoClass::Regression),
            Self::StageRefusal { .. } => Some(RepairVetoClass::StageRefusal),
            Self::AppendRefused { .. } => Some(RepairVetoClass::AppendRefused),
            Self::MeasurementRefused { .. } => Some(RepairVetoClass::MeasurementRefused),
            Self::OwnContactCondemned { .. } => Some(RepairVetoClass::OwnContactCondemned),
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
/// **This is the in-memory row; [`CoverageRepairStepWire`] is the one that goes
/// on disk.** The only difference between them is `row_id`: here it is the
/// opaque, unforgeable [`RepairRowId`] the plan editor accepts, and there it is
/// [`crate::specialized::RepairRowWire`], the same identity as four numbers a
/// reader can parse and nothing can act on. §D.2's persistence obligation is
/// discharged through [`Self::to_wire`]; the opaque id's construction rule is
/// not weakened to do it.
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
    /// **EVIDENCE ONLY, and the field that names the dominant cost of a repair
    /// search.** `true` when this trial's own appended contact was CONDEMNED —
    /// the cadence quarantined it, or retired it with no winner — so no arm was
    /// built at that seat and the row's `marginal_area_mm2`, which is a
    /// PREDICTION of what an arm there would cover, was never delivered.
    ///
    /// It is strictly wider than `own_contact_quarantined`, which names only
    /// the quarantine half, and the difference is not academic. MEASURED,
    /// `zombie-3-a` (`trials=92 commits=3`, the two persisted sidecars that
    /// carry this field agreeing exactly, and two older ones agreeing on
    /// everything they do carry): of 89 rejected trials, **52 were quarantined
    /// and 37 more were unrouted with no winner** — that is **89 of 89**
    /// rejections, every one a full prepare → route → rewire → brace →
    /// structure → lower → measure pass spent to learn that no arm was built
    /// at that seat. Reading the narrow field alone leaves **42 %** of that
    /// waste unnamed. The remaining three steps are the three commits, whose
    /// contacts were not condemned.
    ///
    /// **This paragraph previously said 34, "86 of 89" and 38 %, attributed to
    /// `e9e160f`.** Those numbers are not reproducible: the field did not exist
    /// at `e9e160f`, whose persisted sidecar carries `null` here, and the runs
    /// that do carry it measure 37. The error looks like subtracting the three
    /// committed trials twice. Corrected against the persisted
    /// `*.coverage.repair.json` sidecars rather than re-derived by argument.
    ///
    /// **The two halves are different problems and only one of them is
    /// understood.** The 52 are condemned before any arm is proposed. The 37
    /// end at ROUTING with no winning arm, before any brace, resize or
    /// path-level state exists, so the "acceptance depends on post-brace state"
    /// argument that covers the 52 says nothing about them.
    ///
    /// **Which of the 37 are the `every_feasible_arm_refused_by_clearance`
    /// sub-class: MEASURED 2026-08-22, and it is ALL of them.** 37 true, 0
    /// false, every one at stage
    /// `AllFeasibleCandidatesRefusedByClearance` — not one had an empty anchor
    /// batch and not one ran out of feasible arms (42 to 106 proposed, 37 to 102
    /// feasible, never zero). Arms existed, most were feasible, and the
    /// clearance standoff refused every one, which is task #51's shape end to
    /// end. [`Self::own_contact_no_winner`] is the field that says so; the run
    /// is recorded in the archived design evidence
    /// (`docs/plans/support-clearance.md` §"Evidence").
    ///
    /// **Nothing reads it to decide anything, and a ranking rule that did was
    /// built and MEASURED HARMFUL.** Deprioritizing every candidate inside a
    /// condemned seat's coverage disc sank the seats that WORK along with the
    /// ones that do not: on `zombie-3-a` the first commit, which lands at trial
    /// 6 without it, had not landed by trial 14 with it. The reason is a scale
    /// mismatch and it is measured — the candidates in one cluster sit
    /// **0.03 to 0.46 mm** apart while the disc is the registered
    /// 6.0 mm coverage radius, so one condemnation swallows the whole
    /// neighbourhood, committing rows included. The rule was reverted rather
    /// than tuned, because tuning it means inventing a radius, and the honest
    /// remedy is a cheap PHYSICS screen on the seat itself — see the lane
    /// report and task #52.
    pub own_contact_condemned: Option<bool>,
    /// **EVIDENCE ONLY — the OTHER half of `own_contact_condemned`, named.**
    ///
    /// `Some` exactly when this trial's own appended contact reached the
    /// routing cadence and came out `UnroutedNoWinner`; `None` on every other
    /// step, including a trial that never reached a measurement at all, and
    /// `own_contact_condemned` is what tells those two apart.
    ///
    /// See [`OwnContactNoWinnerEvidence`] for what the fields are and for the
    /// rule that every one of them is READ off the transcript row the run
    /// already wrote, never recomputed here.
    pub own_contact_no_winner: Option<OwnContactNoWinnerEvidence>,
    /// Every route whose evidence moved, and by how much. Routes that did not
    /// move are omitted, so a committed step that touched one route carries one
    /// row.
    pub route_deltas: Vec<RouteDelta>,
    pub outcome: RepairStepOutcome,
}

/// **Where the router stopped, for ONE contact that got no arm.**
///
/// This is the vocabulary `crate::cco::NoWinnerEvidence` already uses, carried
/// out under its own names. It is a DISCLOSURE of which variant the routing
/// cadence sealed on that terminal, not a second classification of it: the
/// mapping is one variant to one value and there is no judgment in between.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnContactNoWinnerStage {
    /// The anchor batch was empty — no arm was ever proposed to refuse.
    NoCandidatesGenerated,
    /// Arms were proposed and every one of them failed feasibility before the
    /// clearance stage was reached.
    NoFeasibleCandidate,
    /// Arms were proposed, some were feasible, and the clearance standoff
    /// refused every feasible one. This is task #51's shape.
    AllFeasibleCandidatesRefusedByClearance,
    /// **A confirmation round drove every remaining arm, one per pass, under a
    /// frozen steering set, and the exact post-brace check measured every one
    /// of them illegal.** Distinct from the reason above because the refusal
    /// happened at a DIFFERENT check in a DIFFERENT stage: that one is the
    /// candidacy-time clearance standoff, this one is the post-resize brace
    /// pass, and conflating them would let a Stage B retirement be read as
    /// task #51's mask defect. The measured millimetres live in the sealed
    /// `NoWinnerEvidence`, per design §1.3 — this field is disclosure of a
    /// record that already exists.
    ConfirmationRoundExhausted,
}

/// **EVIDENCE ONLY. Why the router built no arm at this trial's own seat.**
///
/// The design record (`docs/plans/support-clearance.md` §"Evidence", the
/// archived probe logs) measured that on
/// `zombie-3-a` **37 of 89** rejected trials ended `UnroutedNoWinner` on their
/// own appended contact — a class that ends at ROUTING, before any brace,
/// resize or path-level state exists — and recorded that the run computed
/// `every_feasible_arm_refused_by_clearance` but never persisted it, so nobody
/// could say which of the 37 were task #51's shape. This type is that
/// disclosure and nothing more.
///
/// **Every field is a READ, never a derivation.** The routing cadence seals a
/// `crate::cco::NoWinnerEvidence` onto the transcript row when a terminal
/// retires with no winner, and that value already holds the candidate counts
/// and the retained refusal rows. `crate::runner` projects them out at the one
/// site that reads the row. Nothing here recounts candidates, re-runs a
/// clearance test, or infers a count from a distance.
///
/// **Nothing reads it to decide anything.** No accept clause, no ranking rule
/// and no gate takes this value as an input; it travels on the step and is
/// written to the sealed repair ledger. That is the physics-first doctrine's
/// own line: a number that steers is a cap, a number that is printed is
/// evidence.
///
/// **The `Option`s say "the record does not hold this", never "zero".** A
/// `NoCandidatesGenerated` row has no `candidates_generated` field to read —
/// its variant name IS the count — so this discloses `None` rather than
/// writing a 0 the cadence never wrote.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnContactNoWinnerEvidence {
    /// **The judge's flag, persisted by name.** Exactly the value
    /// `crate::runner::TrialContactDisposition::UnroutedNoWinner` has always
    /// carried: `true` when the sealed reason is
    /// [`OwnContactNoWinnerStage::AllFeasibleCandidatesRefusedByClearance`].
    /// It is redundant with `stage` on purpose — `stage` is the wider
    /// disclosure this change adds, and this is the field the measurement was
    /// ordered on.
    pub every_feasible_arm_refused_by_clearance: bool,
    /// Which sealed reason the row carried. `None` means the transcript row was
    /// `UnroutedNoWinner` with no reason attached at all, which is a different
    /// statement from any of the three stages and is recorded as one.
    pub stage: Option<OwnContactNoWinnerStage>,
    /// How many arms the router proposed at this terminal. `None` on the
    /// `NoCandidatesGenerated` stage, which carries no such field.
    pub candidates_generated: Option<usize>,
    /// How many of those survived feasibility. `None` on every stage but
    /// `AllFeasibleCandidatesRefusedByClearance`, which is the only one that
    /// reached the clearance test and therefore the only one that counted them.
    pub candidates_feasible: Option<usize>,
    /// How many anchor-seat refusal rows the cadence RETAINED for this
    /// terminal — a length of the list it already holds, not a re-tally of
    /// anything. Every stage carries this list.
    pub anchor_seat_refusals_recorded: usize,
    /// How many anchor-foot refusal rows it retained. `None` on
    /// `NoCandidatesGenerated`, which has no foot stage to refuse at.
    pub anchor_foot_refusals_recorded: Option<usize>,
}

/// **Stage D — what the attempt-0 legalization retry did, on disk.**
///
/// Design authority: `docs/specs/seal-semantics.md`
/// §4 Stage D, authorized by Craig 2026-08-23. *"The no-good ledger, the pass
/// count and the confirmation-round record join the coverage refusal sidecar …
/// **This is disclosure, not the record**."*
///
/// **It is not a repair-controller counter.** It travels on
/// [`RepairRunReport`], which is SPEC-A4 §A.8's report on the ONE-WINNER
/// COVERAGE-REPAIR controller, because that report is the one value both
/// evidence files already embed verbatim — the refusal sidecar through
/// `CoverageClosureRefusal::repair` and the sealed ledger through its own
/// `report` field. The mechanism it describes is a different one at a
/// different time: the legalization retry runs at **attempt 0**, before the
/// first repair trial exists, and none of its numbers is an input to any
/// repair rule. Reading `passes` as a repair count would be a category error.
///
/// **`passes == 0` means the file predates Stage D.** The attempt-0 loop runs
/// at least once on every run that reaches this report, so a live run always
/// writes `passes >= 1`; the field is `#[serde(default)]` so an older
/// `.coverage.repair.json` still deserializes, and it deserializes to zero.
/// Do not "fix" that zero to one — it is the only thing telling an old file
/// from a run that never legalized.
///
/// **Nothing here is truncated and nothing here is capped.** Craig's
/// authorization item 4 is that the confirmation round is uncapped in engine
/// semantics and loudly reported; a "first N rounds" or "first N arms" clip in
/// the disclosure would be that cap wearing a reporting hat.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalizationDisclosure {
    /// Passes through the attempt-0 chain, including the first and including
    /// the one that lowered. A run that never abandoned reports `1`.
    pub passes: usize,
    /// How many steering no-goods the set held when the loop left it.
    pub no_goods: usize,
    /// **The no-goods consulted, per terminal.** The same projection the run
    /// log prints, keyed by terminal id so a reader can see which terminal the
    /// mechanism spent itself on rather than only how much it spent.
    pub no_goods_by_terminal: BTreeMap<u32, usize>,
    /// Rounds OPENED. A voided round is counted here too: it was opened, it
    /// was paid for, and it earned nothing.
    pub confirmation_rounds: usize,
    pub voided_rounds: usize,
    /// Arms a round has driven, across all rounds — **including arms measured
    /// inside a round that was later voided.** It is a COST, not a credit: the
    /// passes were spent whether or not the round they belonged to survived to
    /// earn anything. What a retirement rests on is the round's own sealed
    /// rows inside `NoWinnerEvidence::ConfirmationRoundExhausted`, never this
    /// counter.
    pub round_arms_measured: usize,
    /// One row per ABANDONED pass — **which terminal was disturbed**, in pass
    /// order. This is Stage A's classification, persisted.
    pub disturbances: Vec<LegalizationDisturbance>,
    /// One row per confirmation round opened, in the order they opened.
    pub rounds: Vec<ConfirmationRoundDisclosure>,
}

/// One abandoned legalization pass, as the loop classified it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalizationDisturbance {
    /// Which pass this was, counting from one.
    pub pass: usize,
    /// Terminals whose OWN chosen arm the brace pass measured inside the
    /// model. These are the terminals that steer.
    pub own_arm_terminals: Vec<u32>,
    /// Terminals disturbed purely by re-parenting — their own arm was never
    /// measured illegal. **They steer nothing** (design §1.3); they are here
    /// because a reader who cannot see them cannot tell a one-terminal class
    /// from a cascade.
    pub cascade_victims: Vec<u32>,
    /// How many no-goods this pass added that were NOT already in the set.
    /// The abandon rule's own number: a pass that added none and advanced no
    /// round refused by name instead of looping, so a row reading zero here
    /// carries `round_arm_measured` or `voided_a_round`.
    pub no_goods_learned: usize,
    /// Whether this pass measured an arm for an open confirmation round.
    pub round_arm_measured: bool,
    /// Whether this pass voided an open round.
    pub voided_a_round: bool,
    /// Set when the pass abandoned because a terminal reported every arm it
    /// can still generate carries a no-good — the event that OPENS a round.
    pub steering_spent_terminal: Option<u32>,
}

/// One confirmation round, as the loop drove it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmationRoundDisclosure {
    /// The terminal the round measured.
    pub terminal_id: u32,
    /// The pass on which it opened, counting from one.
    pub opened_at_pass: usize,
    /// **The round length — how many arms it actually drove**, one per pass,
    /// in the tournament's own order.
    pub arms_measured: usize,
    /// How many no-goods the steering set held at the moment the round opened.
    /// The set is frozen for the round's duration, so this is the set the
    /// round measured against.
    pub no_goods_at_open: usize,
    pub outcome: ConfirmationRoundOutcome,
}

/// **What became of one confirmation round, named for what was OBSERVED.**
///
/// Deliberately not `Retired` / `Legalized`. `UnroutedNoWinner` has four
/// sealed causes and only one of them is a round, so "this round retired the
/// terminal" is an inference, and a field name is the wrong place to smuggle
/// one. The mechanism record lives where design §1.3 put it — inside the
/// sealed `NoWinnerEvidence::ConfirmationRoundExhausted`, with the measured
/// millimetres. This says only where the terminal stood in the transcript the
/// lowering pass sealed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmationRoundOutcome {
    /// Another terminal's own arm was condemned while this round ran (design
    /// §1.3, and it is the ONLY way a round is voided): the set grew, so the
    /// round is re-entered later against the new set. A second terminal
    /// reporting its own steering spent cannot void a round — that pass learns
    /// nothing and advances nothing, so it refuses by name instead. The round
    /// retires nothing.
    Voided,
    /// The round was still open when a pass lowered, and the lowered run's
    /// sealed transcript carries this terminal as `UnroutedNoWinner`.
    TerminalUnroutedAtLowering,
    /// The round was still open when a pass lowered, and the lowered run's
    /// sealed transcript does NOT carry this terminal as `UnroutedNoWinner` —
    /// the forced arm survived the brace pass.
    TerminalRoutedAtLowering,
}

impl CoverageRepairStep {
    /// Project this step onto the wire — SPEC-A4 §D.2's persistence, with the
    /// opaque row identity replaced by its readable name.
    ///
    /// Every other field is carried VERBATIM. There is no summarisation and no
    /// rounding here on purpose: the whole claim of §D is that the before/after
    /// pair IS the contact's contribution, and a persisted row that had been
    /// condensed on its way to disk would be a second measurement of the same
    /// step rather than the step.
    #[must_use]
    pub fn to_wire(&self) -> CoverageRepairStepWire {
        CoverageRepairStepWire {
            trial: self.trial,
            commit: self.commit,
            row_id: self.row_id.wire(),
            seat_mm: self.seat_mm,
            source_island_id: self.source_island_id,
            source_component_id: self.source_component_id,
            scored_from: self.scored_from.clone(),
            marginal_area_mm2: self.marginal_area_mm2,
            chosen_by_first_trial_guarantee: self.chosen_by_first_trial_guarantee,
            diversity_discs_containing_seat: self.diversity_discs_containing_seat,
            objective_before: self.objective_before,
            objective_after: self.objective_after,
            uncensored_before_mm2: self.uncensored_before_mm2,
            uncensored_after_mm2: self.uncensored_after_mm2,
            gates_before: self.gates_before,
            gates_after: self.gates_after,
            ownerless_faces_before: self.ownerless_faces_before,
            ownerless_faces_after: self.ownerless_faces_after,
            own_contact_quarantined: self.own_contact_quarantined,
            own_contact_condemned: self.own_contact_condemned,
            own_contact_no_winner: self.own_contact_no_winner,
            route_deltas: self.route_deltas.clone(),
            outcome: self.outcome.clone(),
        }
    }
}

/// **SPEC-A4 §D.1 on the wire** — one repair transaction, as it is written to
/// disk and read back.
///
/// Field-for-field [`CoverageRepairStep`], with the opaque [`RepairRowId`]
/// replaced by [`crate::specialized::RepairRowWire`]. That single substitution
/// is what lets this type be `Deserialize` without handing a reader the right
/// to construct a pool identity the planner never froze.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoverageRepairStepWire {
    /// Every transaction attempted, committed or not, from 1.
    pub trial: usize,
    /// Which commit this was, if it committed. `None` for a rejection.
    pub commit: Option<usize>,
    /// The winner, by exact pool identity.
    pub row_id: crate::specialized::RepairRowWire,
    pub seat_mm: [f64; 3],
    pub source_island_id: u32,
    pub source_component_id: u32,
    /// Every route whose uncovered samples contributed to this row's GLOBAL
    /// score, with each route's share. This is the field §D.2's cross-route
    /// sentence is made of, and the reason the record can say "0.36 mm² of the
    /// 0.46 mm² came from island 5" instead of inferring it.
    pub scored_from: Vec<CoverageScoreShare>,
    pub marginal_area_mm2: f64,
    pub chosen_by_first_trial_guarantee: bool,
    pub diversity_discs_containing_seat: usize,
    pub objective_before: CoverageObjective,
    pub objective_after: Option<CoverageObjective>,
    pub uncensored_before_mm2: f64,
    pub uncensored_after_mm2: Option<f64>,
    pub gates_before: NoncoverageVector,
    pub gates_after: Option<NoncoverageVector>,
    pub ownerless_faces_before: usize,
    pub ownerless_faces_after: Option<usize>,
    pub own_contact_quarantined: Option<bool>,
    /// Verbatim from [`CoverageRepairStep::own_contact_condemned`], which is
    /// the only place this field is explained. It named "rule N4c" until
    /// 2026-08-21; that rule was built, MEASURED HARMFUL and reverted, so it is
    /// not in the tree and the name pointed at nothing.
    pub own_contact_condemned: Option<bool>,
    /// Verbatim from [`CoverageRepairStep::own_contact_no_winner`], which is
    /// the only place this field is explained. It splits the `UnroutedNoWinner`
    /// half of `own_contact_condemned` into the stages the routing cadence
    /// already sealed, so the class the design record sized at 37 of 89 (`docs/plans/support-clearance.md`
    /// §"Evidence") can be read off the file instead of guessed at.
    pub own_contact_no_winner: Option<OwnContactNoWinnerEvidence>,
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
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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

// ---------------------------------------------------------------------------
// THE VETO MEMORY — what a rejection LEARNED, kept in a shape a later decision
// can read.
// ---------------------------------------------------------------------------

/// **Which class of veto ended a trial**, as one small `Copy` value.
///
/// [`RepairStepOutcome`] carries the class AND the measurement that produced
/// it — two `CoverageObjective`s, or a `String` of stage detail. That is the
/// right shape for a ledger row and the wrong shape for a memory that is
/// consulted per candidate: this is the discriminant alone, so a tally can
/// index it and a comparator can compare it without touching a heap.
///
/// The variants are [`RepairStepOutcome`]'s minus `Committed`, which is not a
/// veto. [`RepairStepOutcome::veto_class`] is the only conversion, and
/// [`RepairStepOutcome::class`] now reads its name from [`Self::name`], so the
/// §A.8 histogram and this tally are one vocabulary by construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairVetoClass {
    NoImprovement,
    NoAreaCovered,
    Regression,
    StageRefusal,
    AppendRefused,
    MeasurementRefused,
    OwnContactCondemned,
}

impl RepairVetoClass {
    /// Every class, exhaustively. The match in [`Self::index`] has no wildcard
    /// arm, so a class added without a slot in [`RepairVetoCounts`] is a
    /// compile error rather than a silently uncounted rejection.
    pub const ALL: [Self; 7] = [
        Self::NoImprovement,
        Self::NoAreaCovered,
        Self::Regression,
        Self::StageRefusal,
        Self::AppendRefused,
        Self::MeasurementRefused,
        Self::OwnContactCondemned,
    ];

    /// How many classes there are — the width of [`RepairVetoCounts`]'s
    /// per-class array, taken from `ALL` so the two cannot disagree.
    pub const COUNT: usize = Self::ALL.len();

    /// This class's slot in [`RepairVetoCounts`], and its position in
    /// [`Self::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::NoImprovement => 0,
            Self::NoAreaCovered => 1,
            Self::Regression => 2,
            Self::StageRefusal => 3,
            Self::AppendRefused => 4,
            Self::MeasurementRefused => 5,
            Self::OwnContactCondemned => 6,
        }
    }

    /// The name the §A.8 histogram already prints for this class.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NoImprovement => "no_improvement",
            Self::NoAreaCovered => "no_area_covered",
            Self::Regression => "regression",
            Self::StageRefusal => "stage_refusal",
            Self::AppendRefused => "append_refused",
            Self::MeasurementRefused => "measurement_refused",
            Self::OwnContactCondemned => "own_contact_condemned",
        }
    }
}

/// **One rejection, reduced to what it TAUGHT.**
///
/// Before this type, `RepairLedger::reject` did the same two things for every
/// rejection class — ledger the row so it is never retried, and drop a
/// diversity disc at its seat — and the outcome class, the component that
/// regressed and the measured before/after went to the step stream, which no
/// branch reads. A search that vetoed 119 strict improvements on one model
/// could not answer "and on what?" without re-reading a file it only writes
/// when it stops.
///
/// **This is a RECORD, not a rule.** Nothing in the controller reads it to
/// choose a candidate or to accept a trial, and that inertness is the point of
/// landing it on its own: the ranking change that will consult it is a
/// separate, measurable step, and it must be able to assume the memory beneath
/// it did not move the search. The physics-first doctrine's line applies
/// unchanged — a number that steers is a cap, a number that is recorded is
/// evidence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RepairVeto {
    /// The trial that produced it, from 1 — the same number
    /// [`CoverageRepairStep::trial`] carries, so a veto and its step row are
    /// the same event under two shapes.
    pub trial: usize,
    /// Where the rejected candidate sat. This is the key the memory is
    /// searched by, and it is the same seat the diversity disc is centred on.
    pub seat_mm: [f64; 3],
    pub class: RepairVetoClass,
    /// The component clause 2 named. `Some` exactly when `class` is
    /// [`RepairVetoClass::Regression`] — clause 2 is the only clause that
    /// names one.
    pub clause_two_component: Option<NoncoverageComponent>,
    /// **Every hard-gate component that ROSE across this trial**, whichever
    /// clause did the vetoing. On a clause-1 rejection this is the rise the
    /// old ledger threw away: see [`NoncoverageComponentSet`] for the
    /// measurement (176 of 321 trials, always +1, never more than one
    /// component, and never one of the other three).
    pub gates_risen: NoncoverageComponentSet,
    /// Whether the gates were measured on both sides at all. `false` means the
    /// trial never reached a routed measurement, so `gates_risen` is EMPTY
    /// because nothing was compared — not because nothing rose. Those two must
    /// not look alike.
    pub gates_measured: bool,
    /// Whether the trial's own appended contact was condemned — quarantined,
    /// or retired with no winner — so no arm was ever built at that seat.
    /// `None` when the trial never reached a routed measurement.
    ///
    /// The baseline's cross-tab is why this travels on the veto and not only
    /// on the step: on the dust fixture, all 274 trials fall into two of four
    /// cells — no trial ever raised a gate without its own contact condemned,
    /// and no condemned trial ever committed (222 condemned, 222 rejected,
    /// against 4 commits none of which were condemned).
    pub own_contact_condemned: Option<bool>,
}

impl RepairVeto {
    /// **Read one rejected step for what it learned. The ONLY conversion.**
    ///
    /// `None` for a COMMIT: a committed step is not a veto, and folding one in
    /// would poison the memory with the seats that WORK — which is precisely
    /// how the reverted disc-first ranking rule went wrong (see
    /// [`CoverageRepairStep::own_contact_condemned`], "MEASURED HARMFUL").
    ///
    /// Every field is READ off the step the runner already filled. Nothing
    /// here re-measures a gate, re-runs a clause or infers a count.
    #[must_use]
    pub fn from_step(step: &CoverageRepairStep) -> Option<Self> {
        let class = step.outcome.veto_class()?;
        let clause_two_component = match &step.outcome {
            RepairStepOutcome::Regression { component, .. } => Some(*component),
            _ => None,
        };
        let (gates_risen, gates_measured) = match step.gates_after {
            Some(after) => (after.risen_components(&step.gates_before), true),
            None => (NoncoverageComponentSet::EMPTY, false),
        };
        Some(Self {
            trial: step.trial,
            seat_mm: step.seat_mm,
            class,
            clause_two_component,
            gates_risen,
            gates_measured,
            own_contact_condemned: step.own_contact_condemned,
        })
    }
}

/// **A fold of vetoes, as fixed-width counters** — the answer shape for "has a
/// candidate like this one already been vetoed, and on what?".
///
/// `Copy`, heap-free and fixed-size on purpose: this is what a ranking rule
/// gets back per candidate row, and a `BTreeMap` per row per re-sort would put
/// an allocation inside the comparator. [`RepairVetoDisclosure`] is the named,
/// serialized shape, built ONCE per run for the report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RepairVetoCounts {
    /// How many vetoes are folded in here.
    pub total: usize,
    by_class: [usize; RepairVetoClass::COUNT],
    gate_rises: [usize; NoncoverageComponent::ALL.len()],
    /// Of `total`, how many reached a routed measurement, so a reader can tell
    /// "no gate rose" from "no gate was measured".
    pub gates_measured: usize,
    /// Of `total`, how many had their own appended contact condemned.
    pub own_contact_condemned: usize,
}

impl RepairVetoCounts {
    /// Fold one veto in. O(1), no branch on the seat: the caller decides which
    /// vetoes are near enough to count.
    pub fn fold(&mut self, veto: &RepairVeto) {
        self.total += 1;
        self.by_class[veto.class.index()] += 1;
        for component in veto.gates_risen.iter() {
            self.gate_rises[component.index()] += 1;
        }
        if veto.gates_measured {
            self.gates_measured += 1;
        }
        if veto.own_contact_condemned == Some(true) {
            self.own_contact_condemned += 1;
        }
    }

    /// How many of these vetoes ended in `class`.
    #[must_use]
    pub const fn class(&self, class: RepairVetoClass) -> usize {
        self.by_class[class.index()]
    }

    /// How many of these vetoes raised `component`. A trial can raise more
    /// than one, so these do not sum to `total`.
    #[must_use]
    pub const fn gate_rise(&self, component: NoncoverageComponent) -> usize {
        self.gate_rises[component.index()]
    }
}

/// [`RepairVetoCounts`] under its names, for the report and the sidecar.
///
/// Zero entries are omitted, the same way `rejections_by_class` omits classes
/// that never fired: a map that prints five zeros says less than one that
/// prints the two components that moved.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairVetoDisclosure {
    pub total: usize,
    /// Veto count by [`RepairVetoClass::name`] — the same keys
    /// `rejections_by_class` uses, minus `committed`, which is not a veto.
    pub by_class: BTreeMap<String, usize>,
    /// **The instrument the 2026-08-28 baseline had to derive by hand from a
    /// step stream, printed by the run itself**: how many rejected trials
    /// raised each hard-gate component, whichever clause vetoed them.
    pub gate_rises_by_component: BTreeMap<String, usize>,
    pub gates_measured: usize,
    pub own_contact_condemned: usize,
}

/// **Every rejection this search has made, searchable by seat.**
///
/// The question a later ranking rule asks is "has a candidate LIKE this one
/// already been vetoed, and on what?", and "like this one" means "seated near
/// it" — the same relation, at the same registered coverage radius, that the
/// diversity disc already uses. What the disc keeps is a COUNT; what this
/// keeps is the reason.
///
/// **The index is a uniform grid of one radius per cell**, so a query touches
/// 27 cells instead of every veto ever made: on the censored 741-trial fixture
/// a linear memory would cost 3,415 rows × 741 vetoes per re-sort, which is
/// the per-item hot path this design exists to avoid. Recording is O(log n)
/// for the cell lookup and nothing else.
///
/// **It decides nothing today.** No branch in the controller reads it. It is
/// built, folded and disclosed so that the ranking change which will read it
/// can be measured against a search this did not move.
#[derive(Clone, Debug)]
pub struct RepairVetoIndex {
    /// The disc's own containment test, squared once.
    radius_sq: f64,
    /// One cell per radius, so every veto within `radius` of a seat lies in
    /// the seat's own cell or one of its 26 neighbours.
    cell_mm: f64,
    vetoes: Vec<RepairVeto>,
    by_cell: BTreeMap<[i64; 3], Vec<usize>>,
    totals: RepairVetoCounts,
}

/// The grid axis is clamped before it is cast, so a seat at any coordinate a
/// mesh can hold lands in a cell rather than in undefined behaviour. Model
/// coordinates are millimetres in the tens; these bounds are ten orders of
/// magnitude past anything a plate can hold and exist only to make the cast
/// total.
const CELL_INDEX_MIN: f64 = -1.0e15;
const CELL_INDEX_MAX: f64 = 1.0e15;

impl RepairVetoIndex {
    /// `radius_mm` is the registered coverage radius the diversity discs are
    /// built with, so "near" means the same thing in both memories.
    ///
    /// A radius that is not positive and finite would make the grid meaningless,
    /// so the cell edge falls back to 1 mm and the containment test keeps the
    /// radius it was given: the index then degrades to an honest linear scan
    /// over one cell rather than inventing a neighbourhood.
    #[must_use]
    pub fn new(radius_mm: f64) -> Self {
        let cell_mm = if radius_mm.is_finite() && radius_mm > 0.0 {
            radius_mm
        } else {
            1.0
        };
        Self {
            radius_sq: radius_mm * radius_mm,
            cell_mm,
            vetoes: Vec::new(),
            by_cell: BTreeMap::new(),
            totals: RepairVetoCounts::default(),
        }
    }

    /// Remember one rejection.
    pub fn record(&mut self, veto: RepairVeto) {
        self.totals.fold(&veto);
        let cell = self.cell_of(veto.seat_mm);
        let index = self.vetoes.len();
        self.vetoes.push(veto);
        self.by_cell.entry(cell).or_default().push(index);
    }

    /// Every veto recorded, in trial order.
    #[must_use]
    pub fn vetoes(&self) -> &[RepairVeto] {
        &self.vetoes
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.vetoes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vetoes.is_empty()
    }

    /// The whole run's fold, maintained incrementally as vetoes arrive.
    #[must_use]
    pub const fn totals(&self) -> RepairVetoCounts {
        self.totals
    }

    /// **The batch query, which is the real one.**
    ///
    /// One call answers a whole ranking, filling `out` with one
    /// [`RepairVetoCounts`] per seat, in the seats' own order. A ranking that
    /// asked seat by seat would be the per-item hot-path call this codebase
    /// treats as a defect.
    ///
    /// It takes an ITERATOR of seats rather than a slice so the caller's rows
    /// can be read where they already live — `rows.iter().map(|row|
    /// row.seat_mm)` — with no seat vector built per re-sort; and `out` is a
    /// buffer the caller keeps, so after the first call the answer costs no
    /// allocation at all.
    pub fn counts_near_batch(
        &self,
        seats: impl IntoIterator<Item = [f64; 3]>,
        out: &mut Vec<RepairVetoCounts>,
    ) {
        out.clear();
        let seats = seats.into_iter();
        out.reserve(seats.size_hint().0);
        if self.vetoes.is_empty() {
            out.extend(seats.map(|_| RepairVetoCounts::default()));
            return;
        }
        out.extend(seats.map(|seat| self.counts_near(seat)));
    }

    /// One seat's fold — the convenience form of [`Self::counts_near_batch`],
    /// for a test or a single lookup. A ranking uses the batch.
    #[must_use]
    pub fn counts_near(&self, seat_mm: [f64; 3]) -> RepairVetoCounts {
        let mut counts = RepairVetoCounts::default();
        let [cell_x, cell_y, cell_z] = self.cell_of(seat_mm);
        for x in -1..=1_i64 {
            for y in -1..=1_i64 {
                for z in -1..=1_i64 {
                    let Some(bucket) = self.by_cell.get(&[
                        cell_x.saturating_add(x),
                        cell_y.saturating_add(y),
                        cell_z.saturating_add(z),
                    ]) else {
                        continue;
                    };
                    for &index in bucket {
                        let veto = &self.vetoes[index];
                        if seat_distance_sq(seat_mm, veto.seat_mm) <= self.radius_sq {
                            counts.fold(veto);
                        }
                    }
                }
            }
        }
        counts
    }

    /// The run's fold under its names, for [`RepairRunReport`].
    #[must_use]
    pub fn disclosure(&self) -> RepairVetoDisclosure {
        let mut by_class = BTreeMap::new();
        for class in RepairVetoClass::ALL {
            let count = self.totals.class(class);
            if count > 0 {
                by_class.insert(class.name().to_owned(), count);
            }
        }
        let mut gate_rises_by_component = BTreeMap::new();
        for component in NoncoverageComponent::ALL {
            let count = self.totals.gate_rise(component);
            if count > 0 {
                gate_rises_by_component.insert(component.name().to_owned(), count);
            }
        }
        RepairVetoDisclosure {
            total: self.totals.total,
            by_class,
            gate_rises_by_component,
            gates_measured: self.totals.gates_measured,
            own_contact_condemned: self.totals.own_contact_condemned,
        }
    }

    fn cell_of(&self, point: [f64; 3]) -> [i64; 3] {
        [
            self.cell_axis(point[0]),
            self.cell_axis(point[1]),
            self.cell_axis(point[2]),
        ]
    }

    /// A non-finite coordinate lands in cell 0 and is then rejected by the
    /// distance test, which is `false` on every NaN comparison. It cannot
    /// match, and it cannot make a finite seat match either.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the value is clamped to ±1e15 on the line above, which is inside i64"
    )]
    fn cell_axis(&self, value: f64) -> i64 {
        let quotient = value / self.cell_mm;
        if !quotient.is_finite() {
            return 0;
        }
        quotient.floor().clamp(CELL_INDEX_MIN, CELL_INDEX_MAX) as i64
    }
}

/// The diversity disc's own containment arithmetic, to the term order — the
/// two memories must agree on what "near" means, and a second formula would be
/// a second definition.
///
/// Canon item C7 (`docs/specs/geometry-canon.md` §2 step 2(c)): the term order
/// is now the kernel's `dot(sub, sub)`, not a `mul_add` chain. The comment
/// above was already the right argument; it was being made against a formula
/// the rest of the workspace does not use.
fn seat_distance_sq(left: [f64; 3], right: [f64; 3]) -> f64 {
    let d = sub(left, right);
    dot(d, d)
}

// ---------------------------------------------------------------------------
// THE TRIAL TRAIL — the step stream, on disk WHILE the search is still running.
// ---------------------------------------------------------------------------

/// **Where the live trial trail is written, if anywhere.** Names a DIRECTORY;
/// each support run in the process opens its own file inside it.
pub const TRIAL_TRAIL_DIR_ENV: &str = "PLINTH_REPAIR_TRIAL_TRAIL_DIR";

/// The schema, in the file NAME, so every line of the file can be one step row
/// and `jq -s` over the file is the same array shape a `.coverage.repair.json`
/// carries under `steps`.
const TRIAL_TRAIL_SCHEMA: &str = "repair-trial-trail.v1";

/// **The trial stream, appended as it happens.**
///
/// `RepairLedger::steps` reaches disk only at a terminal state — a seal or a
/// refusal — so the run that most needs reading is the one that writes
/// nothing. That is not hypothetical: the 2026-08-28 baseline could tabulate
/// the per-component hard-gate rise for every fixture that finished, and for
/// the 741-trial fixture that was killed at 2,068 s it could only record "per
/// component figures are NOT available: a killed process writes no step
/// stream".
///
/// This closes that hole and nothing else. One JSON row per trial, one line
/// each, written and flushed as the trial lands — so a killed run, a runaway
/// run and a wedged run all leave the same readable trail a finished run does,
/// in the same [`CoverageRepairStepWire`] shape, so the same `jq` reads it.
///
/// **Off unless asked for.** With [`TRIAL_TRAIL_DIR_ENV`] unset there is no
/// file, no formatting and no syscall; a production run is byte-for-byte the
/// run it was.
///
/// **Cost, per trial: one serialization of ONE row and one `write_all`.** The
/// whole ledger is never re-serialized — that would be quadratic in the trial
/// count, on a loop whose whole problem is that it runs too many trials.
/// Against a measured median trial of 437 ms to 1,981 ms, a row of a few
/// kilobytes is not on the hot path.
///
/// **A write failure is data, not control flow** — the rule
/// `SealedRepairLedger` already follows. The first failure is recorded, said
/// once on the run log, and the file is dropped; the search continues
/// unchanged, because evidence that cannot be written is not a reason to
/// refuse a model.
#[derive(Debug)]
pub struct RepairTrialTrail {
    path: PathBuf,
    file: Option<std::fs::File>,
    rows: usize,
    write_error: Option<String>,
}

/// What the trail did, for the report — including where it is, so an operator
/// reading a summary can find the file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairTrailDisclosure {
    /// The file, rendered rather than carried as a `PathBuf`. A `PathBuf` that
    /// is not UTF-8 refuses to serialize, and this value is embedded in the
    /// sealed ledger and the refusal sidecar — so a path nobody can encode
    /// would take a whole model's evidence file down with it. The path here is
    /// for a human to find the file with; `Display` is what a human reads.
    pub path: String,
    /// Rows that reached the file. Equal to the trial count on a healthy run;
    /// a shortfall names a write that failed rather than hiding it.
    pub rows: usize,
    pub write_error: Option<String>,
}

impl RepairTrialTrail {
    /// Open a trail if [`TRIAL_TRAIL_DIR_ENV`] names a directory, else `None`.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let directory = std::env::var_os(TRIAL_TRAIL_DIR_ENV)?;
        if directory.is_empty() {
            return None;
        }
        Some(Self::open_in(Path::new(&directory)))
    }

    /// Open this run's own trail file inside `directory`.
    ///
    /// The name carries the process id and a per-process sequence number, so
    /// several support runs — including several running at once — never
    /// interleave their trials into one file, and a reader never has to guess
    /// which model a `trial=3` row belongs to.
    #[must_use]
    pub fn open_in(directory: &Path) -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT_TRAIL: AtomicUsize = AtomicUsize::new(0);

        let sequence = NEXT_TRAIL.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            "{TRIAL_TRAIL_SCHEMA}.{}.{sequence}.jsonl",
            std::process::id()
        ));
        let opened = std::fs::create_dir_all(directory).and_then(|()| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
        });
        let mut trail = Self {
            path,
            file: None,
            rows: 0,
            write_error: None,
        };
        match opened {
            Ok(file) => {
                trail.file = Some(file);
                emit_run_line(&format!(
                    "coverage-repair trail: writing one row per trial to {}",
                    trail.path.display()
                ));
            }
            Err(error) => trail.fail(format!("could not be opened: {error}")),
        }
        trail
    }

    /// Append ONE finished trial. Called once per trial, committed or not.
    pub fn append(&mut self, step: &CoverageRepairStep) {
        if self.file.is_none() {
            return;
        }
        match self.write_row(step) {
            Ok(()) => self.rows += 1,
            Err(message) => self.fail(message),
        }
    }

    #[must_use]
    pub fn disclosure(&self) -> RepairTrailDisclosure {
        RepairTrailDisclosure {
            path: self.path.display().to_string(),
            rows: self.rows,
            write_error: self.write_error.clone(),
        }
    }

    fn write_row(&mut self, step: &CoverageRepairStep) -> Result<(), String> {
        use std::io::Write as _;

        let Some(file) = self.file.as_mut() else {
            return Ok(());
        };
        let mut line = serde_json::to_vec(&step.to_wire())
            .map_err(|error| format!("trial {} could not be serialized: {error}", step.trial))?;
        line.push(b'\n');
        // One write of one whole line, then a flush: a line that is still in a
        // buffer when the process is killed is a line that was never written,
        // and this file exists FOR the process that is killed.
        file.write_all(&line)
            .and_then(|()| file.flush())
            .map_err(|error| format!("trial {} could not be written: {error}", step.trial))
    }

    /// Record the FIRST failure, say it once, and stop writing.
    fn fail(&mut self, message: String) {
        self.file = None;
        if self.write_error.is_some() {
            return;
        }
        let message = format!("{}: {message}", self.path.display());
        emit_run_line(&format!("coverage-repair trail: {message}"));
        self.write_error = Some(message);
    }
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
    /// **Stage D — what the attempt-0 legalization retry did.**
    ///
    /// A passenger, not a counter of this controller: see
    /// [`LegalizationDisclosure`] for why it rides here and for what
    /// `passes == 0` means. `#[serde(default)]` so a `.coverage.repair.json`
    /// or refusal sidecar written before Stage D still deserializes into this
    /// type, which is the claim `persist_sealed_repair_ledger`'s doc comment
    /// makes about the embedded report.
    #[serde(default)]
    pub legalization: LegalizationDisclosure,
    /// **What the rejections LEARNED, folded over the whole run.**
    ///
    /// `rejections_by_class` says how many trials were thrown away. This says
    /// what they were thrown away ON: the same classes, plus the hard-gate
    /// component each rejected trial actually raised, which no earlier report
    /// could state without a step stream and a hand-written `jq` pass —
    /// and which a killed run therefore could not state at all.
    ///
    /// A counter, like every other field here: no branch reads it.
    /// `#[serde(default)]` so a sidecar written before this field still
    /// deserializes, and it deserializes to "no vetoes recorded".
    #[serde(default)]
    pub vetoes: RepairVetoDisclosure,
    /// Where the live trial trail was written, and whether every row reached
    /// it. `None` when [`TRIAL_TRAIL_DIR_ENV`] was unset, which is the
    /// production default and means no file was opened.
    #[serde(default)]
    pub trail: Option<RepairTrailDisclosure>,
    /// **What the attestation pre-screen removed from the candidate pool, and
    /// on what cause.**
    ///
    /// A counter like every other field here: no branch reads it. It is the
    /// only record of a candidate the loop never spent a trial on, so a pool
    /// that screened down to nothing says so out loud instead of arriving at
    /// `NothingToTry` with no explanation.
    /// `#[serde(default)]` so a sidecar written before the screen existed
    /// still deserializes, and it deserializes to "no screening pass ran".
    #[serde(default)]
    pub prescreen: RepairPrescreenDisclosure,
}

/// **The attestation pre-screen's ledger, summed over every ranking pass.**
///
/// `passes` is how many times the pool was screened — once per `rank_or_refuse`
/// call, so once at entry plus once per COMMIT. `offered` and the drop counts
/// are SUMS over those passes, which means a row that is still unusable after
/// a commit is counted again: each pass really did screen it and really did
/// drop it, and a de-duplicated total would be a different measurement wearing
/// this one's name. Divide by `passes` for the per-pass figure.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairPrescreenDisclosure {
    pub passes: usize,
    /// Eligible rows handed to the screen, summed over passes.
    pub offered: usize,
    /// Of those, how many the screen refused, summed over passes.
    pub dropped: usize,
    /// The drops by [`crate::designed_embed::DesignedMaskError`] cause name,
    /// ascending by name so two runs of the same model print the same line.
    /// Zero entries are omitted, the same way `rejections_by_class` omits
    /// classes that never fired.
    pub dropped_by_cause: BTreeMap<String, usize>,
    /// **What the screen MEASURED and deliberately let through**, by name.
    ///
    /// The screen refuses on one class only. `buried_surface` and
    /// `not_attached` are verdicts about the seat meeting the solid, which the
    /// routed member's own approach can change and which `initial.rs`'s
    /// grow-to-attest ladder exists to move; a screen that dropped candidates
    /// on them would be outrunning its evidence. They are counted because the
    /// question "why did the screen keep a seat the trial then condemned?" has
    /// to be answerable from the run log rather than from a rebuild.
    pub observed_not_acted_on: BTreeMap<String, usize>,
}

impl RepairPrescreenDisclosure {
    /// Fold ONE screening pass in. The only mutation site.
    pub fn fold_pass(
        &mut self,
        offered: usize,
        dropped_by_cause: &BTreeMap<String, usize>,
        observed_not_acted_on: &BTreeMap<String, usize>,
    ) {
        self.passes += 1;
        self.offered += offered;
        for (cause, count) in dropped_by_cause {
            self.dropped += count;
            *self.dropped_by_cause.entry(cause.clone()).or_insert(0) += count;
        }
        for (cause, count) in observed_not_acted_on {
            *self.observed_not_acted_on.entry(cause.clone()).or_insert(0) += count;
        }
    }
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
    /// plinth-layers, plinth-mesh, rayon, serde, serde_json, thiserror` —
    /// `serde_json` joined it for [`RepairTrialTrail`], and it is a serializer,
    /// not a logger), so this goes to stderr rather than to a logger. The run report's per-model line is a
    /// `plinth-pipeline` wiring step and travels with §G.1's stage 4.
    pub fn log_summary(&self) {
        emit_run_line(&self.summary_line());
    }

    /// **The refusal-class histogram, rendered once.**
    ///
    /// Both the live progress line and the final summary print this, and both
    /// call THIS function rather than formatting the map themselves. Task #67
    /// requires the streamed counts and the reported counts to come from one
    /// source; two format loops over the same map would be two sources that
    /// happen to agree today.
    #[must_use]
    fn histogram_text(&self) -> String {
        self.rejections_by_class
            .iter()
            .map(|(class, count)| format!("{class}={count}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// **Task #67 — the line the loop streams WHILE it is running.**
    ///
    /// Every count on it is read straight off `self`, which is the very
    /// [`RepairRunReport`] [`Self::summary_line`] prints at the end: `trials`
    /// and `commits` are incremented by the ledger as each trial lands, and
    /// `rejections_by_class` is the same map, through the same
    /// [`Self::histogram_text`]. There is no second tally, so on those three a
    /// progress line and the summary that follows it cannot disagree by
    /// construction.
    ///
    /// The two values that are NOT read off `self` are the two that cannot be:
    /// `remaining` is a property of the live ranking (the ledger computes it
    /// with the same expression its `finish` uses), and `elapsed_ms` is a
    /// wall-clock read. Wall time is evidence and never a bound — the same rule
    /// the summary's own wall times travel under.
    ///
    /// `remaining` is therefore the same EXPRESSION on both lines, but only the
    /// summary's field is optional: `finish` fills `candidates_remaining` only
    /// where a ranking exists to count, so a summary printed from a run whose
    /// ranking itself refused carries 0. No progress line exists on that path
    /// to contradict it — the loop is never entered — and §A.8.1 records the
    /// exception rather than claiming an invariant that has one.
    ///
    /// It is deliberately a DIFFERENT prefix from the summary (`…progress:`
    /// versus `…:`), so a reader scanning a log can tell a mid-run line from
    /// the final one without counting fields.
    #[must_use]
    pub fn progress_line(&self, remaining: usize, elapsed_ms: u64) -> String {
        format!(
            "coverage-repair progress: trials={} commits={} remaining={} \
             elapsed={}ms [{}]",
            self.trials,
            self.commits,
            remaining,
            elapsed_ms,
            self.histogram_text(),
        )
    }

    /// The one INFO line an operator watching a long run must see move.
    /// Deliberately one line and deliberately unconditional: a cost that is
    /// only printed when someone asks for it is a cost that is hidden.
    #[must_use]
    pub fn summary_line(&self) -> String {
        let histogram = self.histogram_text();
        format!(
            "coverage-repair: trials={} commits={} remaining={} \
             wall_total={}ms wall/trial min={}ms med={}ms max={}ms \
             diversity_demotions={} of_which_tried={} [{histogram}]{}{}{}",
            self.trials,
            self.commits,
            self.candidates_remaining,
            self.wall_time_total_ms,
            self.wall_time_per_trial_min_ms,
            self.wall_time_per_trial_median_ms,
            self.wall_time_per_trial_max_ms,
            self.diversity_demotions,
            self.diversity_demotions_tried,
            self.gate_rise_text(),
            self.prescreen_text(),
            self.trail_text(),
        )
    }

    /// **What the pre-screen cost the pool, on the one line an operator always
    /// sees.**
    ///
    /// Printed on every run that screened at all, drops or none — a screen
    /// whose cost is only visible when it fires is a screen nobody can tell
    /// from an absent one, and `offered=N dropped=0` is the statement that
    /// says the pool was measured and kept. The cause list is omitted when it
    /// is empty, for the same reason `gate_rise_text` omits an empty rise.
    fn prescreen_text(&self) -> String {
        if self.prescreen.passes == 0 {
            return String::new();
        }
        let named = |counts: &BTreeMap<String, usize>, label: &str| {
            if counts.is_empty() {
                return String::new();
            }
            let body = counts
                .iter()
                .map(|(cause, count)| format!("{cause}={count}"))
                .collect::<Vec<_>>()
                .join(" ");
            format!(" {label}[{body}]")
        };
        format!(
            " prescreen offered={} dropped={} passes={}{}{}",
            self.prescreen.offered,
            self.prescreen.dropped,
            self.prescreen.passes,
            named(&self.prescreen.dropped_by_cause, "dropped_by"),
            named(&self.prescreen.observed_not_acted_on, "kept_despite"),
        )
    }

    /// **The hard-gate rise, on the one line an operator always sees.**
    ///
    /// Empty when nothing rose, so a run that vetoed cleanly prints the line
    /// it always printed. When something did rise, the components are named
    /// here rather than left to be recovered from a sidecar — the 2026-08-28
    /// baseline had to derive exactly this from a step stream, and the run
    /// that most needed it never wrote one.
    fn gate_rise_text(&self) -> String {
        if self.vetoes.gate_rises_by_component.is_empty() {
            return String::new();
        }
        let components = self
            .vetoes
            .gate_rises_by_component
            .iter()
            .map(|(component, count)| format!("{component}={count}"))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            " gate_rises[{components}] of_measured={} own_contact_condemned={}",
            self.vetoes.gates_measured, self.vetoes.own_contact_condemned,
        )
    }

    /// Where the live trail went, when there was one, so the operator reading
    /// the summary knows the file exists and whether it is complete.
    fn trail_text(&self) -> String {
        let Some(trail) = self.trail.as_ref() else {
            return String::new();
        };
        let error = trail
            .write_error
            .as_ref()
            .map_or_else(String::new, |error| format!(" trail_error={error}"));
        format!(" trail={} rows={}{error}", trail.path, trail.rows)
    }
}

/// **The judge's streaming-silence note — a run says which stage it is in,
/// while it is still in it.**
///
/// MEASURED, `e9e160f`, release binary: `Giant Bat.stl` printed **nothing at
/// all** to stderr for 75.77 s and then failed; on the bigger models the first
/// line an operator sees is the repair loop's own entry line, four to five
/// minutes in. Everything before it — orientation, hollowing, drilling,
/// preparation, routing, rewire, bracing, structure, lowering — is silent, so a
/// run that is working and a run that is wedged look identical.
///
/// **This is print-only, and that is a rule, not an accident.** No branch reads
/// a stage line, no stage line is compared to anything, and the clock it prints
/// is wall time — evidence, never a bound, the same rule
/// [`RepairRunReport`]'s own wall times travel under. A machine that runs the
/// corpus slowly must produce the same verdicts as one that runs it fast.
///
/// It deliberately goes through [`emit_run_line`], which is the repair loop's
/// own write: one flushed stderr write, one test mirror, so a killed run has
/// still said where it got to and a test can see it. A second write path would
/// be a second thing to keep flushed.
///
/// **The scope is on the line because the clocks are different.** Each clock
/// times its own scope from its own start — the pipeline's is per model, the
/// support run's is per support run — so a reader never has to guess which zero
/// an elapsed is measured from.
#[derive(Debug)]
pub struct StageClock {
    started: std::time::Instant,
    scope: String,
}

impl StageClock {
    #[must_use]
    pub fn start(scope: impl Into<String>) -> Self {
        Self {
            started: std::time::Instant::now(),
            scope: scope.into(),
        }
    }

    /// Announce that `stage` is being entered, with this clock's elapsed.
    pub fn entering(&self, stage: &str) {
        let elapsed_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        emit_run_line(&format!(
            "stage: {} entering {stage} elapsed={elapsed_ms}ms",
            self.scope
        ));
    }
}

/// **Task #67 — the one write the repair loop's live evidence goes through, and
/// it is FLUSHED.**
///
/// Both the streamed progress lines and the final summary call this. Two
/// obligations meet here:
///
/// 1. **A killed run must still have said something.** The whole point of
///    streaming is the operator who watches a model spend 412 seconds (the
///    measured `zombie-3-a` figure) and today sees nothing until it ends. A
///    line that is buffered when the process dies is a line that was never
///    printed, so the flush is explicit rather than inherited from stderr's
///    current unbuffered-ness. If stderr is ever wrapped in a buffered writer
///    upstream, this still holds.
/// 2. **The write must be observable by a test.** The capture below is the
///    only seam, it is off unless a test installs it, and it never replaces
///    the stderr write — it is an additional mirror, so production behaviour
///    is the same whether or not anybody is looking.
pub(crate) fn emit_run_line(line: &str) {
    capture::mirror(line);
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    // A failed write to a closed stderr must not take a support run down: the
    // line is evidence, and evidence that cannot be printed is not a reason to
    // refuse a model.
    let _ = writeln!(err, "{line}");
    let _ = err.flush();
}

/// The test-only mirror behind [`emit_run_line`].
///
/// `plinth-support`'s controller tests live in an **integration** crate
/// (`tests/a4_transactional_controller.rs`), so `#[cfg(test)]` inside this
/// library is not enabled for them and cannot be the seam. This is therefore a
/// real, public, opt-in recorder rather than a conditional one — and being
/// real is what makes it honest: the production path is identical whether a
/// recorder is installed or not, because installing one only ADDS a mirror.
///
/// Thread-local, because the repair loop emits from the thread that called
/// `run_support`; the stage chain's own rayon work never emits.
pub mod capture {
    use std::cell::RefCell;

    thread_local! {
        static LINES: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
    }

    pub(super) fn mirror(line: &str) {
        LINES.with(|lines| {
            if let Some(lines) = lines.borrow_mut().as_mut() {
                lines.push(line.to_owned());
            }
        });
    }

    /// Record every run line this thread emits for as long as the guard lives.
    ///
    /// Nesting is not supported and does not need to be: a test installs one.
    #[derive(Debug)]
    pub struct RunLogCapture {
        _private: (),
    }

    impl RunLogCapture {
        /// Start recording on this thread, discarding anything a previous
        /// guard left behind.
        #[must_use]
        pub fn install() -> Self {
            LINES.with(|lines| *lines.borrow_mut() = Some(Vec::new()));
            Self { _private: () }
        }

        /// Every line emitted on this thread since `install`, in order.
        #[must_use]
        pub fn lines(&self) -> Vec<String> {
            LINES.with(|lines| lines.borrow().clone().unwrap_or_default())
        }
    }

    impl Drop for RunLogCapture {
        fn drop(&mut self) {
            LINES.with(|lines| *lines.borrow_mut() = None);
        }
    }
}

/// **Task #67 — when the loop is due to say where it has got to.**
///
/// > **This paces a PRINT. It decides nothing.** No branch in the controller
/// > reads it, no candidate is admitted or refused by it, and a run whose
/// > progress lines are all suppressed reaches the same verdict on the same
/// > model. That is why the two cadence numbers below are plain `const`s in
/// > this module and are NOT registry candidates: the physics-first doctrine's
/// > rule is about numbers that steer outcomes, and these steer a line of
/// > text.
///
/// The cadence is "every 30 trials or 60 seconds, whichever comes first",
/// which is sized off measured trial costs, not guessed: the dust fixture's
/// median trial is 1 917 ms (30 trials ≈ 58 s) and `zombie-3-a`'s is 18 698 ms
/// (60 s ≈ 3 trials). One rule therefore covers both the fast loop, where the
/// trial count trips first, and the slow one, where the clock does.
#[derive(Debug)]
pub struct ProgressPacer {
    trials_per_line: usize,
    interval: std::time::Duration,
    trials_at_last_line: usize,
    last_line_at: std::time::Instant,
}

/// Trials between progress lines when the loop is fast.
const PROGRESS_TRIALS_PER_LINE: usize = 30;
/// Seconds between progress lines when the loop is slow.
const PROGRESS_INTERVAL_SECS: u64 = 60;

impl ProgressPacer {
    /// Start the pacer at `now`, having just emitted the loop's entry line.
    #[must_use]
    pub fn started_at(now: std::time::Instant) -> Self {
        Self {
            trials_per_line: PROGRESS_TRIALS_PER_LINE,
            interval: std::time::Duration::from_secs(PROGRESS_INTERVAL_SECS),
            trials_at_last_line: 0,
            last_line_at: now,
        }
    }

    /// Whether a line is due, and — when it is — remember that it was emitted.
    ///
    /// Takes `trials` and `now` rather than reading them, so the cadence can be
    /// proved in a unit test at any speed without a sleep.
    pub fn due(&mut self, trials: usize, now: std::time::Instant) -> bool {
        let by_trials = trials.saturating_sub(self.trials_at_last_line) >= self.trials_per_line;
        let by_clock = now.duration_since(self.last_line_at) >= self.interval;
        if by_trials || by_clock {
            self.trials_at_last_line = trials;
            self.last_line_at = now;
            return true;
        }
        false
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

    /// One fully populated step, with every `Option` on its `Some` arm and both
    /// vectors non-default, so a field dropped in [`CoverageRepairStep::to_wire`]
    /// cannot hide behind a default value that happens to match.
    fn populated_step() -> CoverageRepairStep {
        CoverageRepairStep {
            trial: 7,
            commit: Some(3),
            row_id: RepairRowId::for_search_rule_test(5, 1, 118),
            seat_mm: [10.4, 7.6, 19.0],
            source_island_id: 3,
            source_component_id: 0,
            scored_from: vec![
                CoverageScoreShare {
                    island_id: 5,
                    component_id: 1,
                    area_mm2: 0.36,
                },
                CoverageScoreShare {
                    island_id: 3,
                    component_id: 0,
                    area_mm2: 0.10,
                },
            ],
            marginal_area_mm2: 0.46,
            chosen_by_first_trial_guarantee: true,
            diversity_discs_containing_seat: 2,
            objective_before: objective(0.46, 0.36, 2, 4.0),
            objective_after: Some(objective(0.10, 0.10, 1, 2.0)),
            uncensored_before_mm2: 0.46,
            uncensored_after_mm2: Some(0.10),
            gates_before: NoncoverageVector {
                unrouted_no_winner: 7,
                quarantined: 2,
                unrouted_specialized: 1,
                load_violations: 3,
                unsupported_specialized: 4,
            },
            gates_after: Some(NoncoverageVector {
                unrouted_no_winner: 6,
                quarantined: 2,
                unrouted_specialized: 1,
                load_violations: 3,
                unsupported_specialized: 4,
            }),
            ownerless_faces_before: 8,
            ownerless_faces_after: Some(5),
            own_contact_quarantined: Some(false),
            own_contact_condemned: Some(false),
            own_contact_no_winner: Some(OwnContactNoWinnerEvidence {
                every_feasible_arm_refused_by_clearance: true,
                stage: Some(OwnContactNoWinnerStage::AllFeasibleCandidatesRefusedByClearance),
                candidates_generated: Some(40),
                candidates_feasible: Some(12),
                anchor_seat_refusals_recorded: 118,
                anchor_foot_refusals_recorded: Some(12),
            }),
            route_deltas: vec![RouteDelta {
                island_id: 5,
                component_id: 1,
                failing_area_before_mm2: 0.36,
                failing_area_after_mm2: 0.0,
                uncensored_area_before_mm2: 0.36,
                uncensored_area_after_mm2: 0.0,
                failing_patches_before: 1,
                failing_patches_after: 0,
                patches_before: 1,
                patches_after: 0,
                worst_gap_before: Some(CoverageClosureGap::NoRoutedContact),
                worst_gap_after: None,
            }],
            outcome: RepairStepOutcome::Committed,
        }
    }

    /// **SPEC-A4 §D.2's persistence obligation, and the reason it was owed:
    /// `RepairRowId` had no wire encoding.**
    ///
    /// Now it has one, and this pins both halves of the rule the encoding was
    /// designed around:
    ///
    /// * the projection is LOSSLESS — every field of the step survives to the
    ///   wire row and back through a JSON round trip, so a sealed-run ledger on
    ///   disk is the step and not a summary of it;
    /// * the projection is ONE-WAY — the wire row names the pool identity as
    ///   four numbers and a provenance, and there is no path back to a
    ///   `RepairRowId`, which is what keeps "only the specialized planner may
    ///   construct one" true after §D.2 is discharged.
    ///
    /// The revert this guards: dropping a field from `to_wire` (the cheap way to
    /// make a large ledger smaller) or deriving `Deserialize` straight onto
    /// `RepairRowId` (the cheap way to skip the wire type). The first fails the
    /// equality below; the second is what the wire type exists instead of.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn a_repair_step_reaches_the_wire_losslessly_and_names_its_row_without_granting_it() {
        let step = populated_step();
        let wire = step.to_wire();

        let json = serde_json::to_string(&wire).expect("the wire step serializes");
        let parsed: CoverageRepairStepWire =
            serde_json::from_str(&json).expect("the wire step round-trips");
        assert_eq!(parsed, wire, "the wire row must survive JSON unchanged");

        // The row identity, field by field, against the id the step actually
        // carries: a `to_wire` that stamped a placeholder identity would still
        // round-trip, and this is what catches it.
        assert_eq!(
            parsed.row_id,
            crate::specialized::RepairRowWire {
                owner_island_id: 5,
                candidate_id: 118,
                face_id: 0,
                component_id: 1,
                source_kind: crate::specialized::RepairRowSource::Generic,
            },
            "the wire row must carry the pool identity, not a placeholder"
        );

        // Every other field, verbatim. Listed rather than folded into one
        // comparison so a failure names which field the projection dropped.
        assert_eq!(parsed.trial, step.trial);
        assert_eq!(parsed.commit, step.commit);
        assert_eq!(parsed.seat_mm, step.seat_mm);
        assert_eq!(parsed.source_island_id, step.source_island_id);
        assert_eq!(parsed.source_component_id, step.source_component_id);
        assert_eq!(parsed.scored_from, step.scored_from);
        assert_eq!(parsed.marginal_area_mm2, step.marginal_area_mm2);
        assert_eq!(
            parsed.chosen_by_first_trial_guarantee,
            step.chosen_by_first_trial_guarantee
        );
        assert_eq!(
            parsed.diversity_discs_containing_seat,
            step.diversity_discs_containing_seat
        );
        assert_eq!(parsed.objective_before, step.objective_before);
        assert_eq!(parsed.objective_after, step.objective_after);
        assert_eq!(parsed.uncensored_before_mm2, step.uncensored_before_mm2);
        assert_eq!(parsed.uncensored_after_mm2, step.uncensored_after_mm2);
        assert_eq!(parsed.gates_before, step.gates_before);
        assert_eq!(parsed.gates_after, step.gates_after);
        assert_eq!(parsed.ownerless_faces_before, step.ownerless_faces_before);
        assert_eq!(parsed.ownerless_faces_after, step.ownerless_faces_after);
        assert_eq!(parsed.own_contact_quarantined, step.own_contact_quarantined);
        assert_eq!(parsed.own_contact_condemned, step.own_contact_condemned);
        assert_eq!(parsed.own_contact_no_winner, step.own_contact_no_winner);
        assert_eq!(parsed.route_deltas, step.route_deltas);
        assert_eq!(parsed.outcome, step.outcome);
    }

    /// **The 37 must be readable off the FILE, field by field.**
    ///
    /// The archived design evidence (`docs/plans/support-clearance.md`
    /// §"Evidence") measured that 37 of `zombie-3-a`'s
    /// 89 rejected trials ended `UnroutedNoWinner` on their own appended
    /// contact, and recorded that nobody could say which of them were task #51's
    /// `every_feasible_arm_refused_by_clearance` sub-class — the run computed the
    /// flag and the sidecar dropped it. This reads the JSON a sealed run writes.
    ///
    /// **The revert this guards** is the one-line kind and it is exactly the one
    /// that measurement was bitten by: leave the field off `to_wire`, or off the wire struct,
    /// or let the runner stop filling it, and every digest, every gate verdict
    /// and every artifact byte stays identical while the class goes back to
    /// being unmeasurable. Each assertion below is on a distinct field, so a
    /// failure names which one the projection dropped rather than saying only
    /// that something moved.
    ///
    /// The `Option`s are asserted as VALUES, not merely as present: `None` here
    /// means "the sealed reason had no such field", and a projection that wrote
    /// `0` instead would be inventing a count the routing cadence never took.
    #[test]
    fn the_persisted_step_names_why_no_arm_was_built_at_its_own_seat() {
        let wire = populated_step().to_wire();
        let value = serde_json::to_value(&wire).expect("the wire step serializes");

        let no_winner = &value["own_contact_no_winner"];
        assert!(
            !no_winner.is_null(),
            "a step whose own contact got no arm must say so on the wire: {value}"
        );
        assert_eq!(
            no_winner["every_feasible_arm_refused_by_clearance"],
            serde_json::json!(true),
            "the flag the run already computes must reach the file under its own name: \
             {no_winner}"
        );
        assert_eq!(
            no_winner["stage"], "all_feasible_candidates_refused_by_clearance",
            "and the sealed reason's own variant name must travel with it: {no_winner}"
        );
        assert_eq!(
            no_winner["candidates_generated"],
            serde_json::json!(40),
            "how many arms were proposed is evidence the cadence already held: {no_winner}"
        );
        assert_eq!(
            no_winner["candidates_feasible"],
            serde_json::json!(12),
            "and how many of them were feasible is what makes 'clearance refused every one' a \
             measurement instead of a label: {no_winner}"
        );
        assert_eq!(
            no_winner["anchor_seat_refusals_recorded"],
            serde_json::json!(118)
        );
        assert_eq!(
            no_winner["anchor_foot_refusals_recorded"],
            serde_json::json!(12)
        );
    }

    /// **A stage that recorded no such count must write `null`, never `0`.**
    ///
    /// `NoCandidatesGenerated` has no `candidates_generated` field to read — its
    /// variant name IS the count — and a projection that filled the hole with a
    /// zero would make "the cadence proposed nothing" indistinguishable from
    /// "the cadence proposed nothing and we know it", which is the same
    /// distinction §A.8.1 refuses to blur for `candidates_remaining`.
    #[test]
    fn a_stage_that_recorded_no_count_discloses_null_rather_than_zero() {
        let mut step = populated_step();
        step.own_contact_no_winner = Some(OwnContactNoWinnerEvidence {
            every_feasible_arm_refused_by_clearance: false,
            stage: Some(OwnContactNoWinnerStage::NoCandidatesGenerated),
            candidates_generated: None,
            candidates_feasible: None,
            anchor_seat_refusals_recorded: 7,
            anchor_foot_refusals_recorded: None,
        });
        let value = serde_json::to_value(step.to_wire()).expect("the wire step serializes");
        let no_winner = &value["own_contact_no_winner"];

        assert_eq!(no_winner["stage"], "no_candidates_generated");
        assert!(
            no_winner["candidates_generated"].is_null(),
            "an unrecorded count is null: {no_winner}"
        );
        assert!(
            no_winner["candidates_feasible"].is_null(),
            "an unrecorded count is null: {no_winner}"
        );
        assert!(
            no_winner["anchor_foot_refusals_recorded"].is_null(),
            "this stage never reached a foot to refuse at: {no_winner}"
        );
        assert_eq!(
            no_winner["anchor_seat_refusals_recorded"],
            serde_json::json!(7),
            "the list it DOES hold is disclosed by its length: {no_winner}"
        );
    }

    /// **A step whose own contact was not `UnroutedNoWinner` writes `null`, and
    /// the key is still there.** The reader has to be able to tell "this trial
    /// is not in the 37" from "this file predates the disclosure", and only a
    /// present key can say the first.
    #[test]
    fn a_step_outside_the_no_winner_class_still_carries_the_key() {
        let mut step = populated_step();
        step.own_contact_no_winner = None;
        let value = serde_json::to_value(step.to_wire()).expect("the wire step serializes");

        assert!(
            value
                .as_object()
                .expect("a step is a JSON object")
                .contains_key("own_contact_no_winner"),
            "the key must be written even when there is nothing to say: {value}"
        );
        assert!(value["own_contact_no_winner"].is_null());
    }

    /// **§D.2's cross-route share must be readable off the FILE, not only off
    /// the struct.** The sidecar sentence the design asks for — "scored 0.46 mm²
    /// of which 0.36 mm² came from island 5" — is made of `scored_from`, and a
    /// serializer that flattened it to a single number would still round-trip a
    /// `Vec` of one element. This reads the JSON itself.
    #[test]
    fn the_persisted_step_carries_the_per_route_shares_its_score_came_from() {
        let wire = populated_step().to_wire();
        let value = serde_json::to_value(&wire).expect("the wire step serializes");

        let shares = value["scored_from"]
            .as_array()
            .expect("scored_from is an array on the wire");
        assert_eq!(
            shares.len(),
            2,
            "both contributing routes must reach the file: {value}"
        );
        assert_eq!(shares[0]["island_id"], 5);
        assert_eq!(shares[0]["area_mm2"], 0.36);
        assert_eq!(shares[1]["island_id"], 3);
        assert_eq!(shares[1]["area_mm2"], 0.10);
        // The row's OWN route is island 3 with the smaller share, which is the
        // cross-route effect §D names: the largest contributor is not the route
        // the row was drawn from.
        assert_eq!(value["source_island_id"], 3);
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

    /// A report mid-run: two trials landed, one of each measured refusal class
    /// the dust fixture produces, nothing committed yet.
    fn mid_run_report() -> RepairRunReport {
        let mut report = RepairRunReport {
            trials: 47,
            commits: 2,
            ..RepairRunReport::default()
        };
        report
            .rejections_by_class
            .insert("no_improvement".to_owned(), 44);
        report
            .rejections_by_class
            .insert("stage_refusal".to_owned(), 1);
        report
    }

    /// **Task #67's one-source rule, held where it can actually break.**
    ///
    /// The requirement is not "the two lines look similar" — it is that the
    /// streamed numbers and the summarised numbers are the SAME numbers. So
    /// this asserts the shared substrings rather than a golden line: every
    /// `class=count` pair the summary prints appears verbatim in the progress
    /// line, and so do the trial and commit counts. A future edit that gives
    /// either line its own tally of the same quantity fails here.
    #[test]
    fn a_progress_line_and_the_summary_read_the_same_counters() {
        let report = mid_run_report();
        let progress = report.progress_line(849, 366_035);
        let summary = report.summary_line();

        assert!(progress.contains("trials=47"), "{progress}");
        assert!(summary.contains("trials=47"), "{summary}");
        assert!(progress.contains("commits=2"), "{progress}");
        assert!(summary.contains("commits=2"), "{summary}");
        for class in ["no_improvement=44", "stage_refusal=1"] {
            assert!(
                progress.contains(class),
                "progress lost {class}: {progress}"
            );
            assert!(summary.contains(class), "summary lost {class}: {summary}");
        }
        assert_eq!(report.histogram_text(), "no_improvement=44 stage_refusal=1");
        // The remaining count is the loop's, not the report's stale copy: a
        // progress line printed before `finish` must still say a true number.
        assert!(progress.contains("remaining=849"), "{progress}");
        assert!(progress.contains("elapsed=366035ms"), "{progress}");
        // And the two lines are distinguishable in a log at a glance.
        assert!(
            progress.starts_with("coverage-repair progress:"),
            "{progress}"
        );
        assert!(summary.starts_with("coverage-repair: "), "{summary}");
    }

    /// **The cadence: 30 trials or 60 seconds, whichever comes FIRST.**
    ///
    /// Proved with an injected clock rather than a sleep, so the test is not a
    /// timing test and cannot flake on a busy machine (task #37's class of
    /// defect, not repeated here).
    #[test]
    fn the_pacer_fires_on_thirty_trials_or_sixty_seconds_whichever_comes_first() {
        let start = std::time::Instant::now();
        let mut pacer = ProgressPacer::started_at(start);

        // Neither condition met.
        assert!(!pacer.due(29, start + std::time::Duration::from_secs(59)));
        // Trial count trips it, well inside the interval.
        assert!(pacer.due(30, start + std::time::Duration::from_secs(59)));
        // Having fired, it re-arms from THIS trial count, not from zero.
        assert!(!pacer.due(59, start + std::time::Duration::from_secs(60)));
        assert!(pacer.due(60, start + std::time::Duration::from_secs(60)));

        // The clock arm, on a loop far too slow to reach 30 trials: the
        // measured `zombie-3-a` trial is 18 698 ms, so three trials is a
        // minute.
        let mut slow = ProgressPacer::started_at(start);
        assert!(!slow.due(2, start + std::time::Duration::from_millis(37_396)));
        assert!(slow.due(3, start + std::time::Duration::from_millis(60_000)));
        assert!(!slow.due(4, start + std::time::Duration::from_millis(78_698)));
        assert!(slow.due(5, start + std::time::Duration::from_millis(120_001)));
    }

    /// The capture seam records what `emit_run_line` writes, in order, and
    /// stops recording when the guard drops — otherwise a test that installs
    /// one would leak lines into whatever runs next on the same thread.
    #[test]
    fn the_run_log_capture_records_in_order_and_only_while_installed() {
        let recorded = {
            let capture = capture::RunLogCapture::install();
            emit_run_line("first");
            emit_run_line("second");
            capture.lines()
        };
        assert_eq!(recorded, vec!["first".to_owned(), "second".to_owned()]);

        emit_run_line("after the guard dropped");
        let capture = capture::RunLogCapture::install();
        assert!(capture.lines().is_empty());
    }

    // -----------------------------------------------------------------------
    // THE VETO MEMORY
    // -----------------------------------------------------------------------

    fn gates(
        unrouted_no_winner: u32,
        quarantined: u32,
        unrouted_specialized: u32,
        load_violations: u32,
        unsupported_specialized: u32,
    ) -> NoncoverageVector {
        NoncoverageVector {
            unrouted_no_winner,
            quarantined,
            unrouted_specialized,
            load_violations,
            unsupported_specialized,
        }
    }

    /// **Clause 2's question and the ledger's question are the same predicate,
    /// asked for different reasons — so they must never disagree.**
    ///
    /// `first_regression` stops at the first `>` because clause 2 only needs to
    /// know THAT the step may not be kept. `risen_components` keeps going
    /// because the ledger needs to know WHAT moved, on trials clause 1 already
    /// rejected and clause 2 therefore never saw.
    ///
    /// The revert this guards: reimplementing either one against a different
    /// component order, or letting a FALL count as a rise. Both directions are
    /// asserted for every component, so adding a sixth component without
    /// deciding what it means here fails rather than passes quietly.
    #[test]
    fn every_component_that_rose_is_named_and_the_first_of_them_is_clause_twos() {
        let before = gates(7, 2, 1, 3, 4);

        assert_eq!(
            before.risen_components(&before),
            NoncoverageComponentSet::EMPTY
        );
        assert!(before.risen_components(&before).is_empty());
        assert_eq!(before.first_regression(&before), None);

        // One component at a time, in both instruments.
        for component in NoncoverageComponent::ALL {
            let mut after = before;
            match component {
                NoncoverageComponent::UnroutedNoWinner => after.unrouted_no_winner += 1,
                NoncoverageComponent::Quarantined => after.quarantined += 1,
                NoncoverageComponent::UnroutedSpecialized => after.unrouted_specialized += 1,
                NoncoverageComponent::LoadViolations => after.load_violations += 1,
                NoncoverageComponent::UnsupportedSpecialized => {
                    after.unsupported_specialized += 1;
                }
            }
            let risen = after.risen_components(&before);
            assert_eq!(risen.count(), 1, "{component:?}");
            assert!(risen.contains(component), "{component:?}");
            assert_eq!(
                risen.iter().collect::<Vec<_>>(),
                vec![component],
                "the set iterates in ALL order"
            );
            assert_eq!(
                after.first_regression(&before),
                Some(component),
                "clause 2 must name the one component that moved"
            );
        }

        // Several at once: the set holds all of them, and clause 2 names the
        // first in `ALL` order — which is the ONLY thing clause 2 can say.
        let after = gates(8, 2, 1, 4, 4);
        let risen = after.risen_components(&before);
        assert_eq!(risen.count(), 2);
        assert!(risen.contains(NoncoverageComponent::UnroutedNoWinner));
        assert!(risen.contains(NoncoverageComponent::LoadViolations));
        assert_eq!(
            after.first_regression(&before),
            Some(NoncoverageComponent::UnroutedNoWinner)
        );
        assert_eq!(
            risen.iter().next(),
            after.first_regression(&before),
            "clause 2's component is the set's first member, always"
        );

        // A component that FELL is not a rise, and it does not mask one.
        let after = gates(6, 3, 1, 3, 4);
        let risen = after.risen_components(&before);
        assert_eq!(
            risen.iter().collect::<Vec<_>>(),
            vec![NoncoverageComponent::Quarantined]
        );
    }

    /// **One vocabulary for the outcome classes, by construction.**
    ///
    /// The §A.8 histogram keys and the veto tally's keys are the same strings
    /// because `RepairStepOutcome::class` reads `RepairVetoClass::name`. This
    /// pins that, and pins that `committed` has no veto class: a commit folded
    /// into the veto memory would teach the search to avoid the seats that
    /// WORK, which is the failure mode the reverted disc-first rule was
    /// measured to have.
    #[test]
    fn the_histogram_and_the_veto_tally_name_the_same_classes() {
        let sample = |class: RepairVetoClass| -> RepairStepOutcome {
            match class {
                RepairVetoClass::NoImprovement => RepairStepOutcome::NoImprovement {
                    before: objective(0.46, 0.36, 2, 4.0),
                    after: objective(0.46, 0.36, 2, 4.0),
                },
                RepairVetoClass::NoAreaCovered => RepairStepOutcome::NoAreaCovered {
                    before_mm2: 0.46,
                    after_mm2: 0.46,
                },
                RepairVetoClass::Regression => RepairStepOutcome::Regression {
                    component: NoncoverageComponent::Quarantined,
                    before: 2,
                    after: 3,
                },
                RepairVetoClass::StageRefusal => RepairStepOutcome::StageRefusal {
                    phase: crate::SupportRunPhase::Routing,
                    detail: "refused".to_owned(),
                },
                RepairVetoClass::AppendRefused => RepairStepOutcome::AppendRefused {
                    detail: "occupied".to_owned(),
                },
                RepairVetoClass::MeasurementRefused => RepairStepOutcome::MeasurementRefused {
                    detail: "refused".to_owned(),
                },
                RepairVetoClass::OwnContactCondemned => RepairStepOutcome::OwnContactCondemned {
                    condemnation: OwnContactCondemnation::Quarantined {
                        remaining_inside_axis_mm: Some(0.12),
                    },
                },
            }
        };

        for (slot, class) in RepairVetoClass::ALL.into_iter().enumerate() {
            assert_eq!(class.index(), slot, "ALL order is the slot order");
            let outcome = sample(class);
            assert_eq!(outcome.veto_class(), Some(class));
            assert_eq!(outcome.class(), class.name());
        }
        assert_eq!(RepairVetoClass::COUNT, RepairVetoClass::ALL.len());
        assert_eq!(RepairStepOutcome::Committed.veto_class(), None);
        assert_eq!(RepairStepOutcome::Committed.class(), "committed");
    }

    /// **A rejected step is read for what it LEARNED, and a commit teaches
    /// nothing.**
    ///
    /// The clause-1 case is the one the 2026-08-28 baseline says is the whole
    /// distribution: 1,062 trials, zero clause-2 vetoes, and a hard-gate rise
    /// on 176 of the 321 trials that left a step stream. The old ledger saw
    /// that rise and kept none of it. This is where it is kept.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "the seat is CARRIED, not computed: the assertion is that the veto holds the \
                  step's own coordinates bit for bit. See \
                  docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn a_rejection_keeps_its_class_its_component_and_the_gate_that_moved() {
        // Clause 1 rejected it, and a gate rose underneath — the measured
        // majority case.
        let mut step = populated_step();
        step.commit = None;
        step.outcome = RepairStepOutcome::NoImprovement {
            before: objective(0.46, 0.36, 2, 4.0),
            after: objective(0.46, 0.36, 2, 4.0),
        };
        step.gates_before = gates(7, 2, 1, 3, 4);
        step.gates_after = Some(gates(8, 2, 1, 3, 4));
        step.own_contact_condemned = Some(true);

        let veto = RepairVeto::from_step(&step).expect("a rejected step is a veto");
        assert_eq!(veto.trial, step.trial);
        assert_eq!(veto.seat_mm, step.seat_mm);
        assert_eq!(veto.class, RepairVetoClass::NoImprovement);
        assert_eq!(
            veto.clause_two_component, None,
            "clause 2 never ran, so it named nothing — and the memory must not invent a name"
        );
        assert!(veto.gates_measured);
        assert_eq!(
            veto.gates_risen.iter().collect::<Vec<_>>(),
            vec![NoncoverageComponent::UnroutedNoWinner],
            "the rise clause 1 hid is what this memory is for"
        );
        assert_eq!(veto.own_contact_condemned, Some(true));

        // Clause 2's own case names its component.
        step.outcome = RepairStepOutcome::Regression {
            component: NoncoverageComponent::UnroutedNoWinner,
            before: 7,
            after: 8,
        };
        let veto = RepairVeto::from_step(&step).expect("a rejected step is a veto");
        assert_eq!(veto.class, RepairVetoClass::Regression);
        assert_eq!(
            veto.clause_two_component,
            Some(NoncoverageComponent::UnroutedNoWinner)
        );

        // A trial that never reached a measurement says so, rather than
        // reporting "no gate rose".
        step.outcome = RepairStepOutcome::AppendRefused {
            detail: "occupied".to_owned(),
        };
        step.gates_after = None;
        step.own_contact_condemned = None;
        let veto = RepairVeto::from_step(&step).expect("a rejected step is a veto");
        assert!(!veto.gates_measured, "nothing was compared");
        assert!(veto.gates_risen.is_empty());
        assert_eq!(veto.own_contact_condemned, None);

        // And a COMMIT is not a veto at all.
        let mut committed = populated_step();
        committed.outcome = RepairStepOutcome::Committed;
        assert_eq!(RepairVeto::from_step(&committed), None);
    }

    fn veto_at(seat_mm: [f64; 3], class: RepairVetoClass) -> RepairVeto {
        let mut risen = NoncoverageComponentSet::EMPTY;
        risen.insert(NoncoverageComponent::Quarantined);
        RepairVeto {
            trial: 1,
            seat_mm,
            class,
            clause_two_component: None,
            gates_risen: risen,
            gates_measured: true,
            own_contact_condemned: Some(true),
        }
    }

    /// **"Has a candidate like this one already been vetoed, and on what?" —
    /// answered by seat, at the disc's own radius.**
    ///
    /// The seats are placed against a 6 mm radius: one on top of the query,
    /// one just inside, one just outside, and one far away. The pair either
    /// side of the boundary is the point — a memory that answered "yes"
    /// everywhere would be as useless as one that answered "no".
    #[test]
    fn the_veto_memory_answers_by_seat_and_names_the_reason() {
        let mut index = RepairVetoIndex::new(6.0);
        assert!(index.is_empty());
        assert_eq!(index.counts_near([0.0, 0.0, 0.0]).total, 0);

        index.record(veto_at([0.0, 0.0, 0.0], RepairVetoClass::NoImprovement));
        index.record(veto_at([5.9, 0.0, 0.0], RepairVetoClass::NoImprovement));
        index.record(veto_at([6.1, 0.0, 0.0], RepairVetoClass::Regression));
        index.record(veto_at([100.0, 0.0, 0.0], RepairVetoClass::StageRefusal));

        assert_eq!(index.len(), 4);
        assert_eq!(index.vetoes().len(), 4);

        let near = index.counts_near([0.0, 0.0, 0.0]);
        assert_eq!(near.total, 2, "the row at 6.1 mm is outside the radius");
        assert_eq!(near.class(RepairVetoClass::NoImprovement), 2);
        assert_eq!(near.class(RepairVetoClass::Regression), 0);
        assert_eq!(near.gate_rise(NoncoverageComponent::Quarantined), 2);
        assert_eq!(near.gate_rise(NoncoverageComponent::UnroutedNoWinner), 0);
        assert_eq!(near.gates_measured, 2);
        assert_eq!(near.own_contact_condemned, 2);

        // The whole-run fold is every veto, wherever it sits.
        let totals = index.totals();
        assert_eq!(totals.total, 4);
        assert_eq!(totals.class(RepairVetoClass::StageRefusal), 1);

        // The batch form is the one a ranking calls, and it must answer
        // exactly what the single-seat form answers, in order.
        let seats = [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [-500.0, 0.0, 0.0]];
        let mut out = Vec::new();
        index.counts_near_batch(seats.iter().copied(), &mut out);
        assert_eq!(out.len(), seats.len());
        for (answer, seat) in out.iter().zip(seats) {
            assert_eq!(*answer, index.counts_near(seat));
        }
        assert_eq!(out[1].total, 1);
        assert_eq!(out[2].total, 0, "a seat with no veto near it says zero");

        // An empty memory answers every seat, not none of them: `out` is
        // filled by position, so a caller can zip it against its rows.
        let empty = RepairVetoIndex::new(6.0);
        empty.counts_near_batch(seats.iter().copied(), &mut out);
        assert_eq!(out, vec![RepairVetoCounts::default(); seats.len()]);
    }

    /// **The grid must find exactly what a linear scan finds.**
    ///
    /// The index exists to make the query cheap, and a cheap query that misses
    /// a veto is worse than the linear scan it replaced — it would answer "no
    /// candidate like this has been vetoed" about a neighbourhood the search
    /// has already spent itself on. So the fast answer is checked against the
    /// slow one over 400 seats and 200 vetoes drawn from a fixed generator,
    /// including seats far outside every occupied cell.
    ///
    /// The revert this guards is the cell arithmetic: a neighbourhood of 1
    /// cell instead of 27, a cell edge smaller than the radius, or a `floor`
    /// that truncates toward zero and folds the two cells either side of the
    /// origin into one. Each of those passes the test above and fails this.
    #[test]
    fn the_grid_finds_exactly_what_a_linear_scan_finds() {
        // A plain LCG: deterministic, no dependency, and the same sequence on
        // every machine.
        let mut state = 0x2545_F491_4F6C_DD1D_u64;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            #[expect(
                clippy::cast_precision_loss,
                reason = "test-local coordinate spread; the value is a seat position, not a count"
            )]
            let unit = (state >> 11) as f64 / (1_u64 << 53) as f64;
            // GEOM-EXEMPT(scalar affine map, not vector algebra) —
            // GEOMETRY-CANON EXEMPT: canon §2 step 3 (MAY-STAY, test code).
            // This is a test-local LCG mapping a unit scalar onto [-20, 20] mm;
            // there is no vector, no tolerance comparison and no verdict, so
            // the no-FMA rule (canon §1 OP-7) has nothing to bite on. Listed in
            // tests/no_private_vector_algebra.rs so the `.mul_add(` matcher
            // stays a hard failure everywhere else.
            unit.mul_add(40.0, -20.0)
        };

        let radius_mm = 6.0;
        let mut index = RepairVetoIndex::new(radius_mm);
        let mut recorded: Vec<RepairVeto> = Vec::new();
        for trial in 0..200 {
            let mut veto = veto_at([next(), next(), next()], RepairVetoClass::NoImprovement);
            veto.trial = trial;
            recorded.push(veto);
            index.record(veto);
        }

        let seats: Vec<[f64; 3]> = (0..400)
            .map(|seat| {
                if seat % 40 == 0 {
                    // Deliberately outside every occupied cell.
                    [next() + 1_000.0, next(), next()]
                } else {
                    [next(), next(), next()]
                }
            })
            .collect();
        let mut fast = Vec::new();
        index.counts_near_batch(seats.iter().copied(), &mut fast);

        for (seat, answer) in seats.iter().zip(&fast) {
            let mut slow = RepairVetoCounts::default();
            for veto in &recorded {
                if seat_distance_sq(*seat, veto.seat_mm) <= radius_mm * radius_mm {
                    slow.fold(veto);
                }
            }
            assert_eq!(*answer, slow, "the grid and the scan disagree at {seat:?}");
        }
        // …and the fixture is not vacuous: some seats really do have vetoes
        // near them, and some really do not.
        assert!(fast.iter().any(|counts| counts.total > 0));
        assert!(fast.iter().any(|counts| counts.total == 0));
    }

    /// The disclosure names what MOVED and stays quiet about what did not —
    /// the same rule `rejections_by_class` follows, so five zeros never crowd
    /// out the two components that actually rose.
    #[test]
    fn the_disclosure_names_only_the_classes_and_components_that_moved() {
        let mut index = RepairVetoIndex::new(6.0);
        assert_eq!(index.disclosure(), RepairVetoDisclosure::default());

        let mut first = veto_at([0.0, 0.0, 0.0], RepairVetoClass::NoImprovement);
        first.gates_risen = NoncoverageComponentSet::EMPTY;
        first
            .gates_risen
            .insert(NoncoverageComponent::UnroutedNoWinner);
        index.record(first);
        index.record(veto_at([50.0, 0.0, 0.0], RepairVetoClass::NoImprovement));
        let mut never_measured = veto_at([80.0, 0.0, 0.0], RepairVetoClass::AppendRefused);
        never_measured.gates_risen = NoncoverageComponentSet::EMPTY;
        never_measured.gates_measured = false;
        never_measured.own_contact_condemned = None;
        index.record(never_measured);

        let disclosure = index.disclosure();
        assert_eq!(disclosure.total, 3);
        assert_eq!(
            disclosure.by_class,
            BTreeMap::from([
                ("no_improvement".to_owned(), 2),
                ("append_refused".to_owned(), 1),
            ])
        );
        assert_eq!(
            disclosure.gate_rises_by_component,
            BTreeMap::from([
                ("unrouted_no_winner".to_owned(), 1),
                ("quarantined".to_owned(), 1),
            ]),
            "the three components that never moved are absent, not zero"
        );
        assert_eq!(disclosure.gates_measured, 2);
        assert_eq!(disclosure.own_contact_condemned, 2);

        // And it reaches the operator's one line, where a rise is otherwise
        // invisible until a sidecar is read.
        let report = RepairRunReport {
            trials: 3,
            vetoes: disclosure,
            ..RepairRunReport::default()
        };
        let summary = report.summary_line();
        assert!(
            summary.contains("gate_rises[quarantined=1 unrouted_no_winner=1]"),
            "{summary}"
        );
        assert!(summary.contains("of_measured=2"), "{summary}");
        assert!(summary.contains("own_contact_condemned=2"), "{summary}");
        // A run where nothing rose prints the line it always printed.
        assert!(!RepairRunReport::default()
            .summary_line()
            .contains("gate_rises"));
    }

    // -----------------------------------------------------------------------
    // THE TRIAL TRAIL
    // -----------------------------------------------------------------------

    /// A directory of this test's own, under the system temp dir, removed on
    /// drop so a passing run leaves nothing behind and a failing one leaves
    /// the file to look at.
    struct TrailDir(PathBuf);

    impl TrailDir {
        fn new(name: &str) -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let unique = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "plinth-trail-{name}-{}-{unique}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            Self(path)
        }
    }

    impl Drop for TrailDir {
        fn drop(&mut self) {
            if !std::thread::panicking() {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    /// **The trail is readable BEFORE the search ends — that is the whole
    /// point of it.**
    ///
    /// Nothing here calls a terminal path: two trials are appended and the
    /// file is read while the "search" is still notionally running, because
    /// the run this exists for is the one that never reaches a terminal
    /// path. The 741-trial fixture was killed at 2,068 s and its per-component
    /// figures are simply missing from the 2026-08-28 baseline for exactly
    /// this reason.
    ///
    /// The rows are asserted to be the SAME rows the sidecar would carry —
    /// `CoverageRepairStepWire`, one per line — so the `jq` written against a
    /// finished run reads a killed one unchanged.
    #[test]
    fn the_trail_is_on_disk_and_complete_before_the_search_stops() {
        let directory = TrailDir::new("live");
        let mut trail = RepairTrialTrail::open_in(&directory.0);
        assert_eq!(trail.disclosure().write_error, None);

        let first = populated_step();
        let mut second = populated_step();
        second.trial = 8;
        second.commit = None;
        second.outcome = RepairStepOutcome::NoImprovement {
            before: objective(0.46, 0.36, 2, 4.0),
            after: objective(0.46, 0.36, 2, 4.0),
        };
        trail.append(&first);
        trail.append(&second);

        let disclosure = trail.disclosure();
        assert_eq!(disclosure.rows, 2);
        assert_eq!(disclosure.write_error, None);

        let text = std::fs::read_to_string(&disclosure.path).expect("the trail is on disk");
        let rows: Vec<CoverageRepairStepWire> = text
            .lines()
            .map(|line| serde_json::from_str(line).expect("every line is one wire step"))
            .collect();
        assert_eq!(rows, vec![first.to_wire(), second.to_wire()]);

        // One file per run, named for the process and the run within it, so
        // two searches in one process never interleave their trials.
        let other = RepairTrialTrail::open_in(&directory.0);
        assert_ne!(other.disclosure().path, disclosure.path);
    }

    /// **A trail that cannot be written is data, not control flow.**
    ///
    /// The directory here is a path under an existing FILE, so it cannot be
    /// created. Nothing panics, the error is kept and disclosed, and every
    /// later append is a silent no-op — because evidence that cannot be
    /// written is not a reason to refuse a model.
    #[test]
    fn a_trail_that_cannot_be_opened_is_recorded_and_never_refuses() {
        let directory = TrailDir::new("blocked");
        std::fs::create_dir_all(&directory.0).expect("the parent exists");
        let blocker = directory.0.join("not-a-directory");
        std::fs::write(&blocker, b"file").expect("the blocker is written");

        let mut trail = RepairTrialTrail::open_in(&blocker.join("inside"));
        let opened = trail.disclosure();
        assert!(opened.write_error.is_some(), "{opened:?}");
        assert_eq!(opened.rows, 0);

        trail.append(&populated_step());
        let after = trail.disclosure();
        assert_eq!(after.rows, 0, "an append with no file writes nothing");
        assert_eq!(
            after.write_error, opened.write_error,
            "the FIRST failure is the one that is reported"
        );

        // And the report says so on the one line an operator always sees.
        let report = RepairRunReport {
            trail: Some(after),
            ..RepairRunReport::default()
        };
        assert!(report.summary_line().contains("trail_error="));
    }
}
