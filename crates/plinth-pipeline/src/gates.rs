//! The pre-write physics gate: the only door between a sealed support run and
//! a `.chitubox` file.
//!
//! Until this module existed nothing consulted a physics verdict before
//! writing an artifact. `plinth_emit::write` took no verdict,
//! `plinth_verify::verify_coverage` had no non-test caller,
//! `EMIT_REQUIRE_PASSED_GATES_DEFAULT` had no reader, and
//! [`PipelineExitCode::Gates`] was reserved and unproduced. A model that could
//! not be supported still reached disk, and the only gate that ran ran *after*
//! the write, on the bytes.
//!
//! **There is no override.** `03-physics-and-constants.md` retires
//! `PERF_GATE_EMIT_ON_VERIFY` — "the false compatibility switch does not
//! survive: Rust always refuses blocking defects and canonical
//! `EMIT_REQUIRE_PASSED_GATES_DEFAULT=true` is the sole emission gate" — so
//! this module takes no force flag, no allow-list and no waiver, and
//! `CoverageReport::accepted()` it consults is itself non-overridable.
//!
//! **The refusal happens before the file exists.** The gate runs against the
//! sealed verification snapshot, which borrows the authority; only once every
//! gate has spoken is the authority consumed by `into_supported_model`, and
//! only then is `plinth_emit::write` called. A refused model therefore leaves
//! no path on disk at all — not a truncated one, not an empty one.
//!
//! That claim is about the *path*, not merely about this call, so
//! [`judge_then_emit`] also removes anything already sitting at the target on
//! every refusal. Two ways an artifact could otherwise survive a refusal, both
//! of which were reachable: `plinth_emit::write` used to save the project
//! before auditing it, so a disk-audit rejection left the file it had just
//! refused (it now stages and renames, and cleans up); and a *stale* artifact
//! from an earlier run was never touched, so a model that emitted yesterday
//! and is gate-refused today would still have had yesterday's file at the
//! target. The acceptance test is "the output path does not exist", and it is
//! the path this stage owns.
//!
//! [`PipelineExitCode::Gates`]: crate::PipelineExitCode::Gates

use plinth_core::{m0_constants, ArtifactId, ModelId};
use plinth_emit::{write as emit_write, EmitReport, EmitRequest};
use plinth_support::{
    CoverageMeasurementError, CoverageMeasurementRequest, EmittedContactDisclosure,
    FinalSupportAuthority, SupportVerificationError, SupportVerificationSnapshot,
};
use plinth_verify::{verify_coverage, CoverageReport, CoverageVerificationError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::PipelineError;

/// The twelve gates a candidate must pass before emission.
///
/// The list is `docs/engineering-journal/04-migration-plan.md` §"The defect class this
/// milestone exists to make impossible" verbatim and in its stated order.
/// [`Self::UncoveredIslands`] and [`Self::Load`] have ported implementations;
/// the rest are named here rather than left implicit, so that "which gates does
/// emission actually consult?" has one answer in the type system instead of
/// silent omissions. No production comment states the ported set as a count:
/// [`PreWriteGateReport::pending`] is filled from the arm [`judge`] actually
/// took, so a reader is told what ran rather than what a doc claims ran.
/// [`judge`] matches this enum exhaustively and takes no `_` arm, so a
/// thirteenth gate is a compile error, not a gap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreWriteGate {
    UncoveredIslands,
    Clearance,
    Load,
    UnsupportedMinima,
    IslandBirths,
    RegularContactInvariants,
    UnroutedSpecializedContacts,
    Connectivity,
    Printability,
    Murray,
    BallBreach,
    PhysicalContactInvariant,
}

impl PreWriteGate {
    /// Every gate, in the guide's order. Emission consults all of them.
    pub const ALL: [Self; 12] = [
        Self::UncoveredIslands,
        Self::Clearance,
        Self::Load,
        Self::UnsupportedMinima,
        Self::IslandBirths,
        Self::RegularContactInvariants,
        Self::UnroutedSpecializedContacts,
        Self::Connectivity,
        Self::Printability,
        Self::Murray,
        Self::BallBreach,
        Self::PhysicalContactInvariant,
    ];
}

/// What one gate said.
#[derive(Clone, Debug, PartialEq)]
enum GateVerdict {
    /// The gate ran against this snapshot and found nothing blocking.
    Passed(Box<CoverageReport>),
    /// The gate ran against the run's own recorded evidence and found nothing
    /// blocking.
    ///
    /// It carries no [`CoverageReport`] because it measures no coverage: it
    /// reads the scalars the run already wrote down and checks that the record
    /// satisfies the invariant that gate owns. A gate that reached this verdict
    /// is absent from [`PreWriteGateReport::pending`] exactly as a
    /// [`Self::Passed`] one is.
    Cleared,
    /// The gate is named and wired into the refusal path, but its verifier has
    /// not been ported yet. It cannot pass or fail a model; it is reported so
    /// that a caller can see the gate set is incomplete.
    NotYetPorted,
    /// [`PreWriteGate::Load`] ran, read the run's own emitted-contact record,
    /// and found one or more contacts that do not satisfy the registered
    /// support area law — and, per the 2026-08-29 owner ruling putting the
    /// gate in WARNING MODE while the load-model redesign is underway, did
    /// NOT refuse emission for it.
    ///
    /// Carries exactly the evidence a hard refusal would have carried
    /// ([`GateRefusal::LoadLawViolated`]'s own `judged`, `x_allowed` and
    /// `violations`), unweakened: the arithmetic that decides a row is a
    /// violation is the same arithmetic [`judge_emitted_load_law`] always
    /// ran, and this verdict changes only what happens to that answer, never
    /// how it is computed. A gate that reached this verdict is absent from
    /// [`PreWriteGateReport::pending`] exactly as a [`Self::Passed`] one is —
    /// it ran, and it found something to report.
    ///
    /// **Every row it carries is a MEASURED violation.** Warning mode is
    /// scoped to [`LoadLawFaultClass::RunShowsItIsUnsafe`]; a record with even
    /// one row the run could not describe never reaches this verdict, because
    /// such a run has not shown it is safe. [`judge_load_gate`] is where that
    /// is decided, and it is the only place this verdict is constructed on the
    /// production path. **A caller must not report this as a pass** — see
    /// `examples/first_artifact.rs`, which used to and therefore described an
    /// artifact carrying live area-law violations as fully gated.
    Warned(LoadWarnings),
    /// [`PreWriteGate::UncoveredIslands`] ran, measured this snapshot, found
    /// the coverage verdict NOT accepted, and — per the owner ruling of
    /// 2026-09-01 — did NOT refuse emission for it.
    ///
    /// **The ruling, in the owner's own words:** "if a region can't be
    /// supported, then EMIT THE MODEL ANYWAYS with the supports that *do* work
    /// in place, and a loud warning about the fact some area(s) are
    /// unsupported"; and "Partial success will let me then look at the model
    /// and fix it manually to print, or have a chance at refining the algorithm
    /// understanding where it failed. With *nothing* emitted, I can't see
    /// what's working and what isn't working." An emitted file is evidence
    /// about the ENGINE as well as a deliverable, and a refusal destroys that
    /// evidence.
    ///
    /// It carries the whole [`CoverageReport`] the hard refusal carried,
    /// unweakened: the measurement that decides whether a patch is covered is
    /// the measurement [`verify_coverage`] always took, and this verdict
    /// changes only what is done with the answer. **A caller must not report
    /// this as a pass** — the model emitted with named, measured gaps in it,
    /// and [`PreWriteGateReport::coverage_warned`] is how a caller tells the
    /// two apart.
    CoverageWarned(Box<CoverageReport>),
}

/// The full evidence [`PreWriteGate::Load`] found while in WARNING MODE,
/// carried out to the caller rather than swallowed by a `Cleared` verdict
/// that would otherwise say nothing happened.
///
/// **This is the measuring instrument for the load-model redesign** (owner
/// ruling, 2026-08-29): the gate lands recording every violation instead of
/// refusing on it, so the corpus can be watched while the load-partition
/// defect (task #68) is fixed, and the gate can be flipped back to hard
/// refusal once it is. Every field here is exactly what
/// [`GateRefusal::LoadLawViolated`] would have carried on the hard-refusal
/// path — nothing is summarized or rounded on the way into warning mode.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoadWarnings {
    /// How many emitted contacts the gate judged.
    pub judged: usize,
    /// The registered allowance the law was evaluated at.
    pub x_allowed: f64,
    /// Every contact whose shipped tip does not satisfy the area law, in the
    /// order the record held them. Empty is not a value this verdict is
    /// constructed with — an empty violation set is [`GateVerdict::Cleared`],
    /// never [`GateVerdict::Warned`] — so a caller that sees
    /// [`PreWriteGateReport::load_warnings`] return `Some` knows the vector
    /// inside it is non-empty without re-checking its length.
    pub violations: Vec<LoadLawViolation>,
}

impl std::fmt::Display for LoadWarnings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "gate Load found {} of {} emitted contact(s) that do not satisfy the registered \
             support area law pi * tip_radius_mm^2 >= carried_area_mm2 / X_ALLOWED, evaluated \
             per contact on the final numbers with the registered X_ALLOWED = {} (WARNING MODE: \
             recorded, emission not refused):\n{}",
            self.violations.len(),
            self.judged,
            self.x_allowed,
            render_load_law_violations(&self.violations)
        )
    }
}

/// The gate set's verdict on one sealed run.
///
/// `pending` is not decoration: it is the honest statement of how much of the
/// named gate set actually reaches a verdict, carried out to the caller rather
/// than described in a comment. A gate absent from it ran.
#[derive(Clone, Debug, PartialEq)]
pub struct PreWriteGateReport {
    coverage: CoverageReport,
    pending: Vec<PreWriteGate>,
    load_warnings: Option<LoadWarnings>,
    /// True when the coverage gate reached [`GateVerdict::CoverageWarned`] —
    /// it measured, it did not accept, and it emitted anyway. The measurement
    /// itself is in `coverage`, which is the same report either way; this is
    /// the one bit that says which disposition it got.
    coverage_warned: bool,
}

impl PreWriteGateReport {
    /// The one gate with a ported implementation, and its full evidence.
    #[must_use]
    pub const fn coverage(&self) -> &CoverageReport {
        &self.coverage
    }

    /// The gates that are named and refused-through but not yet ported,
    /// ascending in [`PreWriteGate::ALL`] order.
    #[must_use]
    pub fn pending(&self) -> &[PreWriteGate] {
        &self.pending
    }

    /// [`PreWriteGate::Load`]'s WARNING MODE evidence, when it found anything
    /// to report. `None` means either the gate found no violation, or (while
    /// `Load` remains in [`PreWriteGateReport::pending`]) that it has not
    /// been ported — carried the same way [`Self::pending`] tells the two
    /// apart, by checking membership there first.
    #[must_use]
    pub const fn load_warnings(&self) -> Option<&LoadWarnings> {
        self.load_warnings.as_ref()
    }

    /// Whether the coverage gate MEASURED this model as not covered and let it
    /// emit anyway (owner ruling, 2026-09-01). `false` means the coverage
    /// verdict was accepted. A caller reporting "all gates passed" must check
    /// this first: the gate ran, and it found something.
    #[must_use]
    pub const fn coverage_warned(&self) -> bool {
        self.coverage_warned
    }
}

/// Why one emitted contact's own record does not satisfy the registered
/// support area law.
///
/// One fault is arithmetic and four are record integrity. The record-integrity
/// faults refuse for the same reason the arithmetic one does:
/// [`EmittedContactDisclosure`]'s own documentation says a `None` field means
/// the run "could not be described", and an emitted contact whose load or whose
/// tip the run cannot state is not an emitted contact this gate can clear.
/// Reading a missing operand as a legal-looking `0.0` would turn "unknown" into
/// "passes" — the exact substitution that disclosure refuses to make when it is
/// written, and this gate will not make when it is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadLawFault {
    /// Both operands were read and the law is not satisfied: the emitted tip's
    /// cross-section is smaller than the carried area divided by `X_ALLOWED`.
    /// This is the under-sized support arm the law exists to catch.
    TipSmallerThanTheAreaLawDemands,
    /// The run routed this contact and its own record states no carried area
    /// for it, so there is no load to size the tip against.
    CarriedAreaNotDescribed,
    /// The run routed this contact and its own record states no emitted tip
    /// radius for it, so there is no tip to judge.
    TipRadiusNotDescribed,
    /// The carried area was stated and is not an area: negative, or not finite.
    CarriedAreaUnusable,
    /// The tip radius was stated and is not a radius a load can pass through:
    /// zero, negative, or not finite. Zero is refused rather than divided by —
    /// a contact carrying anything through a tip of no cross-section carries it
    /// through nothing, and `0 >= 0` would clear such a row on the arithmetic.
    TipRadiusUnusable,
}

impl LoadLawFault {
    /// Which of the two readings this fault is — the distinction WARNING MODE
    /// turns on.
    ///
    /// Matched exhaustively and deliberately without a `_` arm: a sixth fault
    /// added to [`LoadLawFault`] without a decision here is a compile error,
    /// not a fault that silently inherits whichever class the wildcard named.
    /// That is the whole point of putting the split on the fault type instead
    /// of writing a predicate at the dispatch — see [`judge_load_gate`].
    #[must_use]
    pub const fn class(self) -> LoadLawFaultClass {
        match self {
            Self::TipSmallerThanTheAreaLawDemands => LoadLawFaultClass::RunShowsItIsUnsafe,
            Self::CarriedAreaNotDescribed
            | Self::TipRadiusNotDescribed
            | Self::CarriedAreaUnusable
            | Self::TipRadiusUnusable => LoadLawFaultClass::RunCannotShowItIsSafe,
        }
    }
}

/// The two readings a [`LoadLawFault`] can have, and the ONLY thing WARNING
/// MODE is allowed to decide on.
///
/// The owner's ruling putting [`PreWriteGate::Load`] in warning mode
/// (2026-08-29) was scoped: *warning mode reports exactly what hard refusal
/// would have refused, unweakened; a fault meaning "this run SHOWS IT IS
/// UNSAFE" may warn, a fault meaning "this run CANNOT SHOW IT IS SAFE" must
/// keep refusing.* Those are two different sentences about the evidence, and
/// this enum is them:
///
/// * [`Self::RunShowsItIsUnsafe`] — both operands were read, the arithmetic
///   ran, and it answered "the tip is too small". The run is describable and
///   the answer is a measurement. Warning mode may downgrade this, because a
///   reader still gets the true number.
/// * [`Self::RunCannotShowItIsSafe`] — a row's operand is absent or is not a
///   quantity, so the law was never evaluated on that contact at all. There is
///   no measurement to report and no reader can act on one. Downgrading this
///   would ship a support the run cannot describe, and would additionally
///   REWARD the omission: a row with no operands is dropped by the ratchet's
///   own `filter_map`, so an undescribable contact improves both the worst
///   tip-area ratio and the total scar area it should have worsened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadLawFaultClass {
    /// The law was evaluated and the row failed it.
    RunShowsItIsUnsafe,
    /// The law could not be evaluated on this row at all.
    RunCannotShowItIsSafe,
}

/// One emitted contact the [`PreWriteGate::Load`] gate refuses, carrying
/// everything the run's own record said about it.
///
/// Every field is read, never derived, except [`Self::required_tip_radius_mm`],
/// which is the area law solved for the radius it demands — the one number an
/// operator needs in order to see how far under-sized the arm is.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoadLawViolation {
    /// The physical contact id, which is also its terminal id.
    pub contact_id: u64,
    /// Where the contact sits on the model, mm, as the record stated it.
    pub position_mm: Option<[f64; 3]>,
    /// The model area this contact is charged to carry after every SPEC-A3-A4
    /// §6 removal and repartition, mm², as the record stated it.
    pub carried_area_mm2: Option<f64>,
    /// The RADIUS of the member end that touches the model in the final live
    /// forest, mm, as the record stated it. This is the tip the run shipped:
    /// after PR-8 grow-to-attest, after rewire, bracing and lowering.
    pub emitted_tip_radius_mm: Option<f64>,
    /// `sqrt(carried_area_mm2 / (pi * X_ALLOWED))` — the smallest tip radius
    /// the area law admits for this contact's carried area. `None` when the
    /// carried area was not usable, in which case no radius can be demanded.
    pub required_tip_radius_mm: Option<f64>,
    /// Which of the five ways this row fails.
    pub fault: LoadLawFault,
}

impl std::fmt::Display for LoadLawViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "contact {} {}: ", self.contact_id, self.seat())?;
        match self.fault {
            LoadLawFault::TipSmallerThanTheAreaLawDemands => {
                let area = self.carried_area_mm2.unwrap_or(f64::NAN);
                let emitted = self.emitted_tip_radius_mm.unwrap_or(f64::NAN);
                let required = self.required_tip_radius_mm.unwrap_or(f64::NAN);
                write!(
                    f,
                    "carries {area:.4} mm^2 through an emitted tip of radius {emitted:.4} mm \
                     (cross-section {:.6} mm^2), but the area law demands a tip radius of at \
                     least {required:.4} mm (cross-section {:.6} mm^2). The emitted tip is \
                     {:.2}x too small in radius",
                    std::f64::consts::PI * emitted * emitted,
                    std::f64::consts::PI * required * required,
                    required / emitted,
                )
            }
            LoadLawFault::CarriedAreaNotDescribed => write!(
                f,
                "the run routed this contact and then could not state the model area it carries, \
                 so the area law has no load to size its tip against. An emitted contact the run \
                 cannot describe is not an emitted contact this gate can clear"
            ),
            LoadLawFault::TipRadiusNotDescribed => write!(
                f,
                "the run routed this contact and then could not state the radius of the tip it \
                 shipped, so the area law has no tip to judge. An emitted contact the run cannot \
                 describe is not an emitted contact this gate can clear"
            ),
            LoadLawFault::CarriedAreaUnusable => write!(
                f,
                "the record states a carried area of {:?} mm^2, which is not an area",
                self.carried_area_mm2
            ),
            LoadLawFault::TipRadiusUnusable => write!(
                f,
                "the record states an emitted tip radius of {:?} mm, which is not a radius a load \
                 can pass through",
                self.emitted_tip_radius_mm
            ),
        }
    }
}

impl LoadLawViolation {
    /// The seat, rendered for an operator who has to find this arm on the
    /// model. Says so plainly when the record did not state one.
    fn seat(&self) -> String {
        self.position_mm.map_or_else(
            || "at a seat the record does not state".to_owned(),
            |[x, y, z]| format!("at ({x:.3}, {y:.3}, {z:.3}) mm"),
        )
    }
}

/// One refused row per line, indented, in the order the record held them.
fn render_load_law_violations(violations: &[LoadLawViolation]) -> String {
    violations
        .iter()
        .map(|violation| format!("  {violation}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// How many of these rows are ones the run could not describe at all.
fn unreadable_row_count(violations: &[LoadLawViolation]) -> usize {
    violations
        .iter()
        .filter(|violation| violation.fault.class() == LoadLawFaultClass::RunCannotShowItIsSafe)
        .count()
}

/// [`GateRefusal::LoadLawViolated`]'s message.
///
/// Written as a function rather than a `#[error]` format string because it has
/// a conditional clause: a refusal that includes rows the run could not
/// describe must SAY SO in its first sentence, so a reader can tell "these
/// arms are under-sized" from "this record cannot be judged" without decoding
/// per-row prose. That second reading is the one WARNING MODE must never
/// downgrade, and a message that did not distinguish it is part of why the
/// downgrade went unnoticed.
fn render_load_law_refusal(
    judged: usize,
    x_allowed: f64,
    violations: &[LoadLawViolation],
) -> String {
    let unreadable = unreadable_row_count(violations);
    let integrity = if unreadable == 0 {
        String::new()
    } else {
        format!(
            "; {unreadable} of them the run could not describe, so the law was never evaluated \
             on that row and this run cannot show it is safe (WARNING MODE does not apply to a \
             record-integrity fault, only to a measured area-law violation)"
        )
    };
    format!(
        "gate Load refused emission: {} of {judged} emitted contact(s) do not satisfy the \
         registered support area law pi * tip_radius_mm^2 >= carried_area_mm2 / X_ALLOWED, \
         evaluated per contact on the final numbers with the registered X_ALLOWED = \
         {x_allowed}{integrity}:\n{}",
        violations.len(),
        render_load_law_violations(violations)
    )
}

/// One contact id the sealed snapshot's routed-contact walk and the run's own
/// emitted-contact record disagree about, with both multiplicities.
///
/// Both counts are carried rather than a verdict word, because the same shape
/// expresses every way the two walks can disagree — a routed contact with no
/// row (`described == 0`), a row naming a contact the run never routed
/// (`routed == 0`), and a row written twice (`described > 1`) — and a reader
/// gets the raw numbers instead of this module's summary of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadRecordIdMismatch {
    /// The physical contact id the two walks disagree about.
    pub contact_id: u64,
    /// How many times the sealed snapshot's routed-contact projection names
    /// it. Normally exactly 1; 0 means the record describes a contact the run
    /// did not route.
    pub routed: usize,
    /// How many times the run's own emitted-contact record names it. Normally
    /// exactly 1; 0 means a routed contact shipped with no row to judge, and
    /// more than 1 means a duplicated row.
    pub described: usize,
}

impl LoadRecordIdMismatch {
    /// The run routed this contact and its own record does not describe it: it
    /// shipped with no row for the area law to judge.
    #[must_use]
    pub const fn is_missing(&self) -> bool {
        self.routed > 0 && self.described == 0
    }

    /// The record names this contact more than once. The extra rows are what
    /// let a count-only check pass while a different contact went missing.
    #[must_use]
    pub const fn is_duplicated(&self) -> bool {
        self.described > 1
    }

    /// The record describes a contact the sealed run did not route at all.
    #[must_use]
    pub const fn is_unrouted(&self) -> bool {
        self.routed == 0 && self.described > 0
    }
}

impl std::fmt::Display for LoadRecordIdMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            contact_id,
            routed,
            described,
        } = *self;
        if self.is_missing() {
            write!(
                f,
                "contact {contact_id}: MISSING -- the sealed run routed it {routed} time(s) and \
                 its own record does not describe it at all, so it shipped with no row for the \
                 area law to judge"
            )
        } else if self.is_unrouted() {
            write!(
                f,
                "contact {contact_id}: UNROUTED -- the record describes it {described} time(s) \
                 and the sealed run did not route it, so the record describes a support this run \
                 did not ship"
            )
        } else if self.is_duplicated() {
            write!(
                f,
                "contact {contact_id}: DUPLICATED -- the sealed run routed it {routed} time(s) \
                 and its own record describes it {described} time(s); the extra row(s) pad the \
                 record's length in place of a contact that is missing from it"
            )
        } else {
            write!(
                f,
                "contact {contact_id}: routed {routed} time(s), described {described} time(s)"
            )
        }
    }
}

/// [`GateRefusal::LoadRecordMismatch`]'s message: the two totals, why counting
/// them is not enough, and one line per disagreeing contact id.
fn render_load_record_mismatch(
    routed: usize,
    described: usize,
    mismatches: &[LoadRecordIdMismatch],
) -> String {
    format!(
        "gate Load refused emission: the sealed run routed {routed} contact(s) and its own \
         emitted-contact record describes {described}, and the two do not identify the same \
         contacts. Comparing only the two counts is not a completeness proof -- a record that \
         names one contact twice in place of another has exactly the right length and leaves a \
         shipped contact unjudged -- so the check is on the identities, and these {} do not \
         agree:\n{}",
        mismatches.len(),
        mismatches
            .iter()
            .map(|mismatch| format!("  {mismatch}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// Why emission was refused. Every variant maps to
/// [`PipelineExitCode::Gates`](crate::PipelineExitCode::Gates).
#[derive(Clone, Debug, PartialEq, Error)]
pub enum GateRefusal {
    /// The registered emission authorization is not enabled.
    ///
    /// This is the read of `EMIT_REQUIRE_PASSED_GATES_DEFAULT`, and it can only
    /// ever *refuse*: the retired `PERF_GATE_EMIT_ON_VERIFY` was the switch
    /// that let a run emit without a verdict, and it does not survive. A
    /// registry that ever published `false` here would be describing an
    /// emission mode this pipeline does not implement, so it refuses instead of
    /// quietly skipping the gates.
    #[error(
        "the registered emission authorization is not enabled; \
         PERF_GATE_EMIT_ON_VERIFY is retired and unverified emission does not exist"
    )]
    EmissionAuthorizationNotEnabled,
    /// The ratified constants registry could not be read at all.
    #[error("the ratified M0 constants authority is unavailable to the pre-write gates")]
    ConstantsUnavailable,
    /// The production support resource policy could not be assembled, so the
    /// gate has no ceilings to measure under.
    #[error("the production support resource policy is unavailable to the pre-write gates: {0}")]
    ResourcePolicyUnavailable(String),
    /// The measurement request could not be built from the registry.
    #[error("the pre-write coverage measurement request is invalid")]
    InvalidCoverageRequest(#[source] CoverageMeasurementError),
    /// The sealed snapshot refused its own integrity check.
    #[error("the sealed support snapshot refused integrity validation before the gates ran")]
    SnapshotIntegrity(#[source] SupportVerificationError),
    /// A gate could not reach a verdict. Fail-closed: an unmeasurable gate
    /// refuses emission exactly as a failed one does.
    #[error("gate {gate:?} could not be measured")]
    Unmeasurable {
        gate: PreWriteGate,
        #[source]
        source: CoverageVerificationError,
    },
    /// A gate reached a verdict and it was blocking.
    #[error("gate {gate:?} refused emission with {} blocking defect(s)", report.defects().len())]
    Refused {
        gate: PreWriteGate,
        report: Box<CoverageReport>,
    },
    /// [`PreWriteGate::Load`] refused: one or more contacts this run actually
    /// emitted do not satisfy the registered support area law at the tip they
    /// actually shipped.
    ///
    /// Every refused row is named, with the numbers the record itself held, so
    /// a reader who has never opened this file can see which arm is under-sized
    /// and by how much.
    #[error("{}", render_load_law_refusal(*judged, *x_allowed, violations))]
    LoadLawViolated {
        /// How many emitted contacts were judged, so the refused rows can be
        /// read against the size of the set they came from.
        judged: usize,
        /// The registered allowance the law was evaluated at, named in the
        /// refusal so a reader never has to guess which anchor was used.
        x_allowed: f64,
        /// Every refused row, in the order the record held them.
        violations: Vec<LoadLawViolation>,
    },
    /// [`PreWriteGate::Load`] refused: the run's emitted-contact record does not
    /// describe every contact the run routed, ONCE EACH, BY IDENTITY.
    ///
    /// Fail-closed against the way a record-reading gate goes silently vacuous.
    /// The two lists come from two independent walks of the same finalized
    /// transcript — the sealed snapshot's routed-contact projection and the
    /// authority's own emitted-contact disclosure — so they must identify the
    /// same contacts. If they do not, some contact shipped without a row to
    /// judge, and a gate that judged the remaining rows would report a safety
    /// it did not check.
    ///
    /// **This used to compare only the two COUNTS, and a count is not a
    /// completeness proof.** A record that named contact 17 twice where 17 and
    /// 18 were routed had the right length, satisfied the check, and left
    /// contact 18 unjudged — the gate then reported on a set that was missing
    /// a shipped support. Counting can only catch a record of the wrong SIZE;
    /// it is blind to a record of the right size describing the wrong things,
    /// which is the failure a duplicated or substituted row actually is. The
    /// comparison is now between the two multisets of contact ids, and
    /// [`Self::mismatches`] names every id the two walks disagree about.
    #[error("{}", render_load_record_mismatch(*routed, *described, mismatches))]
    LoadRecordMismatch {
        /// How many contacts the sealed snapshot says the run routed.
        routed: usize,
        /// How many rows the run's own emitted-contact record holds.
        described: usize,
        /// Every contact id the two walks disagree about, ascending by id.
        /// Never empty on this variant: an agreeing pair of walks does not
        /// produce it.
        mismatches: Vec<LoadRecordIdMismatch>,
    },
    /// The registry published an allowance the area law cannot be evaluated
    /// against. Fail-closed for the same reason every other unmeasurable gate
    /// is: a law with no usable threshold has not been checked.
    #[error(
        "the registered {constant} is {value}, which is not a usable allowance; the support area \
         law cannot be evaluated and emission is refused"
    )]
    UnusableRegisteredAllowance { constant: &'static str, value: f64 },
}

impl GateRefusal {
    /// The gate that refused, when one gate owns the refusal.
    #[must_use]
    pub const fn gate(&self) -> Option<PreWriteGate> {
        match self {
            Self::Unmeasurable { gate, .. } | Self::Refused { gate, .. } => Some(*gate),
            Self::LoadLawViolated { .. }
            | Self::LoadRecordMismatch { .. }
            | Self::UnusableRegisteredAllowance { .. } => Some(PreWriteGate::Load),
            Self::EmissionAuthorizationNotEnabled
            | Self::ConstantsUnavailable
            | Self::ResourcePolicyUnavailable(_)
            | Self::InvalidCoverageRequest(_)
            | Self::SnapshotIntegrity(_) => None,
        }
    }

    /// The measurement error behind an UNMEASURABLE **coverage** gate, when
    /// that is what this refusal is.
    ///
    /// FU-57 keeps evidence on every remaining coverage refusal, and an
    /// unmeasurable coverage gate is one: fail-closed, model refused, the
    /// coverage gate named, no artifact. There is no [`CoverageReport`] to
    /// persist — that is what unmeasurable means — but there is a reason, and
    /// the reason is the evidence. Scoped to gates that actually measure
    /// coverage, so a future non-coverage gate's measurement failure does not
    /// silently start writing a coverage sidecar.
    #[must_use]
    pub fn unmeasurable_coverage_reason(&self) -> Option<String> {
        match self {
            Self::Unmeasurable {
                gate: PreWriteGate::UncoveredIslands,
                source,
            } => Some(source.to_string()),
            Self::Unmeasurable { .. }
            | Self::Refused { .. }
            | Self::LoadLawViolated { .. }
            | Self::LoadRecordMismatch { .. }
            | Self::UnusableRegisteredAllowance { .. }
            | Self::EmissionAuthorizationNotEnabled
            | Self::ConstantsUnavailable
            | Self::ResourcePolicyUnavailable(_)
            | Self::InvalidCoverageRequest(_)
            | Self::SnapshotIntegrity(_) => None,
        }
    }

    /// The full evidence behind a blocking verdict, when there is one.
    #[must_use]
    pub fn report(&self) -> Option<&CoverageReport> {
        match self {
            Self::Refused { report, .. } => Some(report),
            Self::EmissionAuthorizationNotEnabled
            | Self::ConstantsUnavailable
            | Self::ResourcePolicyUnavailable(_)
            | Self::InvalidCoverageRequest(_)
            | Self::SnapshotIntegrity(_)
            | Self::LoadLawViolated { .. }
            | Self::LoadRecordMismatch { .. }
            | Self::UnusableRegisteredAllowance { .. }
            | Self::Unmeasurable { .. } => None,
        }
    }

    /// Every emitted contact [`PreWriteGate::Load`] refused, when that is what
    /// this refusal is.
    ///
    /// The evidence half of the load refusal, in the same shape FU-57 keeps
    /// evidence on a coverage refusal: the message is the reading, and this is
    /// the rows behind it, so a caller can persist or re-render them without
    /// parsing prose.
    #[must_use]
    pub fn load_law_violations(&self) -> Option<&[LoadLawViolation]> {
        match self {
            Self::LoadLawViolated { violations, .. } => Some(violations),
            Self::EmissionAuthorizationNotEnabled
            | Self::ConstantsUnavailable
            | Self::ResourcePolicyUnavailable(_)
            | Self::InvalidCoverageRequest(_)
            | Self::SnapshotIntegrity(_)
            | Self::LoadRecordMismatch { .. }
            | Self::UnusableRegisteredAllowance { .. }
            | Self::Refused { .. }
            | Self::Unmeasurable { .. } => None,
        }
    }

    /// Every row of a load refusal that the run could not describe, so the law
    /// was never evaluated on it — the subset [`LoadLawFaultClass`] calls
    /// [`LoadLawFaultClass::RunCannotShowItIsSafe`].
    ///
    /// This is the evidence half of the WARNING MODE split: a caller deciding
    /// what to do with a `LoadLawViolated` reads this rather than re-deriving
    /// the classification from the fault codes, so there is one definition of
    /// "unreadable" in this crate and [`judge_load_gate`] uses the same one.
    #[must_use]
    pub fn unreadable_load_rows(&self) -> Vec<LoadLawViolation> {
        self.load_law_violations()
            .unwrap_or_default()
            .iter()
            .filter(|violation| violation.fault.class() == LoadLawFaultClass::RunCannotShowItIsSafe)
            .copied()
            .collect()
    }

    /// Every contact id the routed-contact walk and the emitted-contact record
    /// disagree about, when that is what this refusal is.
    ///
    /// The identity half of [`Self::LoadRecordMismatch`]: the message is the
    /// reading, and this is the rows behind it, so a caller can persist or
    /// re-render them without parsing prose.
    #[must_use]
    pub fn load_record_mismatches(&self) -> Option<&[LoadRecordIdMismatch]> {
        match self {
            Self::LoadRecordMismatch { mismatches, .. } => Some(mismatches),
            Self::EmissionAuthorizationNotEnabled
            | Self::ConstantsUnavailable
            | Self::ResourcePolicyUnavailable(_)
            | Self::InvalidCoverageRequest(_)
            | Self::SnapshotIntegrity(_)
            | Self::LoadLawViolated { .. }
            | Self::UnusableRegisteredAllowance { .. }
            | Self::Refused { .. }
            | Self::Unmeasurable { .. } => None,
        }
    }
}

/// The one coverage measurement the pre-write gate makes.
///
/// Both geometric thresholds are the *planner's own*, read from the same
/// registered constants `plinth-support` builds its `ContactSelectionConfig`
/// from: the radius is `COVERAGE_RADIUS_FACTOR × TARGET_SPACING_MM` and the
/// patch-area floor is `VOXEL_PITCH_MM²`, exactly as
/// `specialized::consolidate` derives them for its own coverage question. The
/// gate therefore measures the promise the planner made, and introduces no
/// second, independently-chosen coverage physics. The ceilings come from the
/// same production support policy, through
/// [`SupportPreparationLimits::coverage_measurement_limits`].
///
/// **A thin delegation to `plinth_support::registered_coverage_request`
/// (FU-56).** That is now the one implementation: the coverage-closure
/// controller and this gate must measure under the identical request, or
/// closing coverage in the controller would not guarantee the gate agrees.
/// This wrapper exists only to keep this crate's own [`GateRefusal`] mapping
/// at the door, rather than leaking `plinth-support`'s setup error type into
/// the gate's public error surface.
///
/// # Errors
///
/// Returns [`GateRefusal::ConstantsUnavailable`] /
/// [`GateRefusal::ResourcePolicyUnavailable`] if the registry cannot be read
/// and [`GateRefusal::InvalidCoverageRequest`] if the registered values do not
/// form a valid request.
pub fn registered_coverage_request() -> Result<CoverageMeasurementRequest, GateRefusal> {
    plinth_support::registered_coverage_request().map_err(|error| match error {
        plinth_support::CoverageClosureSetupError::RegisteredConstantsUnavailable(_) => {
            GateRefusal::ConstantsUnavailable
        }
        plinth_support::CoverageClosureSetupError::ResourcePolicyUnavailable(detail) => {
            GateRefusal::ResourcePolicyUnavailable(detail)
        }
        plinth_support::CoverageClosureSetupError::InvalidCoverageRequest(source) => {
            GateRefusal::InvalidCoverageRequest(source)
        }
    })
}

/// Does the run's own emitted-contact record name exactly the contacts the
/// sealed snapshot says it routed, once each?
///
/// `None` when the two walks agree. `Some` carries every contact id they
/// disagree about with both multiplicities, ascending by id — deterministic
/// because it is built through a `BTreeMap`, so a refusal message is
/// reproducible rather than hash-ordered.
///
/// **Why multisets and not sets.** A set comparison would answer "which ids
/// appear on only one side" and would be silent about a record that names a
/// routed contact TWICE, which is the exact substitution the count-only check
/// admitted. Comparing multiplicities catches it, and it costs one integer per
/// id.
///
/// The routed side is counted rather than assumed unique. The sealed
/// snapshot's own construction already refuses a duplicate routed id with
/// `SupportVerificationError::AuthorityMismatch`, so on the production path
/// every routed count is 1 — but this function is reachable from the public
/// [`judge_emitted_load_law`], and a completeness check that trusted its input
/// to be a set would be the same class of assumption the count check was.
fn load_record_identity_mismatches(
    emitted_contacts: &[EmittedContactDisclosure],
    routed_contact_ids: &[u64],
) -> Option<Vec<LoadRecordIdMismatch>> {
    let mut counts: std::collections::BTreeMap<u64, (usize, usize)> =
        std::collections::BTreeMap::new();
    for id in routed_contact_ids {
        counts.entry(*id).or_insert((0, 0)).0 += 1;
    }
    for contact in emitted_contacts {
        counts.entry(contact.contact_id).or_insert((0, 0)).1 += 1;
    }
    let mismatches: Vec<LoadRecordIdMismatch> = counts
        .into_iter()
        .filter(|(_, (routed, described))| routed != described)
        .map(|(contact_id, (routed, described))| LoadRecordIdMismatch {
            contact_id,
            routed,
            described,
        })
        .collect();
    (!mismatches.is_empty()).then_some(mismatches)
}

/// **[`PreWriteGate::Load`] — the whole of it.**
///
/// # The invariant
///
/// Every contact the run actually emitted satisfies the registered area law at
/// the tip it actually shipped:
///
/// ```text
/// pi * tip_radius_mm^2  >=  carried_area_mm2 / X_ALLOWED
/// ```
///
/// evaluated per contact on the FINAL numbers — after SPEC-A3-A4 §6 removal and
/// repartition, after PR-8 grow-to-attest, after rewire, bracing and lowering.
/// `X_ALLOWED` is the registered design load ratio: 18 since Craig's ruling of
/// 2026-09-30 (a Measured owner decision, bounded below the plate-5 cone and
/// plate-6 cylinder snap edges), and 12 = `floor(lower(X_STAR_CONE)/2)` before
/// it. The gate reads it; it never types it.
///
/// # What establishes it
///
/// [`FinalSupportAuthority::emitted_contacts`] — one row per `Routed` finalized
/// transcript row, each carrying the contact id, the seat, the carried area
/// read off the ACCEPTED plan (post-repartition) and the tip radius read off
/// the FINAL live forest member. Both operands are scalars the run already
/// wrote down, and the join between them was already performed by the producer.
/// This gate performs one multiply, one divide and one compare per row. It
/// touches no mesh, no surface, no spatial index, no kernel query and no
/// planning stage; it re-derives no geometry and re-runs nothing.
///
/// That independence is the point. The producer sizes tips from
/// `FrozenTipLoadPolicy`'s legacy stress form, built from four placeholders the
/// registry itself labels a legacy diagnostic that is potentially about four
/// times low; this gate judges the tip that came out of that against the
/// measured `X_ALLOWED` anchor instead. It is the restoration of the deleted
/// auditor's only support hard bound — `max_x = A / (pi * r_tip^2)`, pinned at
/// `max_x <= 12` by `04-migration-plan.md`'s acceptance gate — which had no
/// replacement after that crate was deleted.
///
/// # Why it takes no threshold
///
/// `judge_pre_write_gates` is private because a caller-supplied coverage radius
/// would be an override in all but name. This function has no such seam: it
/// takes no allowance, reads `X_ALLOWED` from the ratified registry itself, and
/// its only other argument is the routed-contact IDENTITY LIST it cross-checks
/// rather than obeys. There is nothing here to pass a lenient value to, which
/// is why it can be public where the gate runner cannot.
///
/// # Fail-closed on the record as well as on the arithmetic
///
/// A `Routed` row whose carried area or tip radius is `None` is itself a
/// refusal — see [`LoadLawFault`]. So is a record that does not name exactly
/// the routed contacts, once each, which is how a record-reading gate would
/// otherwise go quietly vacuous.
///
/// **The completeness check is on IDENTITIES, not on counts.** It used to
/// compare `emitted_contacts.len()` against a routed-contact count, and a
/// length is blind to the failure that matters: a record naming contact 17
/// twice where 17 and 18 were routed has the right length, cleared the old
/// check, and left contact 18 with no row judged at all. The two multisets of
/// contact ids are compared instead, and every id they disagree about is named
/// in the refusal — see [`LoadRecordIdMismatch`].
///
/// An empty record clears only when the run routed nothing;
/// `routed_contact_ids` is what makes that statement checkable rather than
/// assumed.
///
/// # Errors
///
/// - [`GateRefusal::LoadRecordMismatch`] when the record does not name exactly
///   the routed contacts, once each.
/// - [`GateRefusal::ConstantsUnavailable`] /
///   [`GateRefusal::UnusableRegisteredAllowance`] when the registry cannot
///   supply an allowance the law can be evaluated at.
/// - [`GateRefusal::LoadLawViolated`], naming every refused contact, when a
///   shipped tip is smaller than the area law demands or a shipped contact
///   cannot be described.
pub fn judge_emitted_load_law(
    emitted_contacts: &[EmittedContactDisclosure],
    routed_contact_ids: &[u64],
) -> Result<(), GateRefusal> {
    if let Some(mismatches) = load_record_identity_mismatches(emitted_contacts, routed_contact_ids)
    {
        return Err(GateRefusal::LoadRecordMismatch {
            routed: routed_contact_ids.len(),
            described: emitted_contacts.len(),
            mismatches,
        });
    }
    let x_allowed = *m0_constants()
        .map_err(|_| GateRefusal::ConstantsUnavailable)?
        .evidence()
        .x_allowed()
        .value();
    if !x_allowed.is_finite() || x_allowed <= 0.0 {
        return Err(GateRefusal::UnusableRegisteredAllowance {
            constant: "X_ALLOWED",
            value: x_allowed,
        });
    }
    let violations: Vec<LoadLawViolation> = emitted_contacts
        .iter()
        .filter_map(|contact| load_law_violation(contact, x_allowed))
        .collect();
    if violations.is_empty() {
        Ok(())
    } else {
        Err(GateRefusal::LoadLawViolated {
            judged: emitted_contacts.len(),
            x_allowed,
            violations,
        })
    }
}

/// What [`PreWriteGate::Load`] decided when it did NOT refuse.
///
/// A refusal is the `Err` half of [`judge_load_gate`]'s result; this is the two
/// non-refusing readings, kept apart so a caller cannot mistake one for the
/// other. `Warned` is not `Cleared` with a note attached — it is a gate that
/// found violations and was ruled non-blocking, and anything that reports it as
/// a pass is describing an artifact it has not read.
#[derive(Clone, Debug, PartialEq)]
pub enum LoadGateDisposition {
    /// Every row read cleanly and every row satisfies the area law.
    Cleared,
    /// Every row read cleanly and one or more do not satisfy the area law.
    /// Non-blocking per the 2026-08-29 owner ruling; the evidence is carried
    /// out whole.
    Warned(LoadWarnings),
}

impl From<LoadGateDisposition> for GateVerdict {
    fn from(disposition: LoadGateDisposition) -> Self {
        match disposition {
            LoadGateDisposition::Cleared => Self::Cleared,
            LoadGateDisposition::Warned(warnings) => Self::Warned(warnings),
        }
    }
}

/// **[`PreWriteGate::Load`]'s WHOLE verdict, WARNING MODE included.**
///
/// [`judge_emitted_load_law`] answers the physics question — does every row
/// read, and does every row satisfy the law. This answers the *disposition*
/// question: given that answer, does the run emit, warn, or refuse. The gate
/// dispatch in [`judge`] holds no logic of its own and calls only this, so
/// every branch below is reachable from a test that has no
/// [`FinalSupportAuthority`] to build.
///
/// # The split WARNING MODE turns on
///
/// The owner's 2026-08-29 ruling put this gate in warning mode *while the load
/// model is redesigned*, and scoped it: **a fault meaning "this run SHOWS IT IS
/// UNSAFE" may warn; a fault meaning "this run CANNOT SHOW IT IS SAFE" must
/// keep refusing.** That is [`LoadLawFaultClass`], and this function is the one
/// place it is applied.
///
/// A [`GateRefusal::LoadLawViolated`] can carry BOTH kinds of row at once, so
/// the test is over the whole set, not over the first row or over the variant:
/// **if even one row is [`LoadLawFaultClass::RunCannotShowItIsSafe`], the whole
/// refusal stands.** A run with one undescribable contact and forty measured
/// violations has not shown it is safe, whatever the other forty say.
///
/// ## What this replaces, and what it cost
///
/// The dispatch used to downgrade EVERY `LoadLawViolated` to a warning without
/// looking at the fault, while the comment above it asserted the opposite —
/// "the gate stays fail-closed on a record it cannot read at all". Measured
/// consequence: with one contact's tip radius made undescribable, `plinth
/// prepare` printed the warning and then `result EMITTED ... gate-refused=0`.
/// The unreadable support shipped.
///
/// The ratchet then REWARDED it. `ratchet_harness.rs` drops rows with missing
/// operands through a `filter_map`, so an undescribable contact leaves both the
/// worst tip-area ratio and the total scar area — measured on two models, total
/// scar area IMPROVED by exactly the vanished tip's own cross-section
/// (0.0078540 mm² each). Warning on a record-integrity fault does not merely
/// fail to report the defect; it makes the defect read as an improvement.
///
/// # Errors
///
/// Every [`GateRefusal`] [`judge_emitted_load_law`] can return, except a
/// [`GateRefusal::LoadLawViolated`] whose rows are ALL
/// [`LoadLawFaultClass::RunShowsItIsUnsafe`] — that one, and only that one,
/// becomes [`LoadGateDisposition::Warned`].
pub fn judge_load_gate(
    emitted_contacts: &[EmittedContactDisclosure],
    routed_contact_ids: &[u64],
) -> Result<LoadGateDisposition, GateRefusal> {
    match judge_emitted_load_law(emitted_contacts, routed_contact_ids) {
        Ok(()) => Ok(LoadGateDisposition::Cleared),
        Err(GateRefusal::LoadLawViolated {
            judged,
            x_allowed,
            violations,
        }) => {
            if violations.iter().any(|violation| {
                violation.fault.class() == LoadLawFaultClass::RunCannotShowItIsSafe
            }) {
                // NOT a warning. The set contains at least one row the law was
                // never evaluated on, so this run cannot show it is safe, and
                // warning mode is scoped to the other reading. The refusal is
                // returned exactly as it arrived -- every row, measured and
                // unreadable alike, so the operator sees the whole record.
                Err(GateRefusal::LoadLawViolated {
                    judged,
                    x_allowed,
                    violations,
                })
            } else {
                Ok(LoadGateDisposition::Warned(LoadWarnings {
                    judged,
                    x_allowed,
                    violations,
                }))
            }
        }
        Err(other) => Err(other),
    }
}

/// Relative slack on the area-law comparison.
///
/// **Arithmetic hygiene, not a physics allowance.** The producer sizes a tip by
/// taking a square root of the same ratio this gate squares back up, and that
/// round trip is not exact in binary floating point: a tip sized to sit exactly
/// ON the law lands a few units in the last place under it when the gate
/// recomputes `pi * r^2`. Refusing that would vanish a support that satisfies
/// the law — the failure `plinth_support`'s own delegation note warns about,
/// that "a strict reimplementation would refuse a contact one ULP low".
///
/// Eight ULP is about 1.8e-15 relative, and it cannot mask a defect: the
/// smallest genuinely under-sized contact this gate has been measured to refuse
/// was 59% over the law, thirteen orders of magnitude above this. It is also far
/// tighter than the engine's own `1.0e-6` tolerance on the realized safety
/// factor, so the gate does not inherit the producer's slack — a contact this
/// clears is one the producer's own predicate clears too.
const AREA_LAW_ARITHMETIC_SLACK: f64 = 8.0 * f64::EPSILON;

/// The area law on ONE recorded row: `Some` when the row does not satisfy it.
///
/// The comparison is written in the law's own terms — `pi * r^2` against
/// `A / X` — rather than as `r` against the solved radius, so the code and the
/// registered note read the same. The solved radius is carried alongside it
/// purely so the refusal can say how far under-sized the arm is.
fn load_law_violation(
    contact: &EmittedContactDisclosure,
    x_allowed: f64,
) -> Option<LoadLawViolation> {
    let row = |fault, required_tip_radius_mm| LoadLawViolation {
        contact_id: contact.contact_id,
        position_mm: contact.position_mm,
        carried_area_mm2: contact.carried_area_mm2,
        emitted_tip_radius_mm: contact.tip_radius_mm,
        required_tip_radius_mm,
        fault,
    };
    let Some(carried_area_mm2) = contact.carried_area_mm2 else {
        return Some(row(LoadLawFault::CarriedAreaNotDescribed, None));
    };
    if !carried_area_mm2.is_finite() || carried_area_mm2 < 0.0 {
        return Some(row(LoadLawFault::CarriedAreaUnusable, None));
    }
    let required = (carried_area_mm2 / (std::f64::consts::PI * x_allowed)).sqrt();
    let Some(tip_radius_mm) = contact.tip_radius_mm else {
        return Some(row(LoadLawFault::TipRadiusNotDescribed, Some(required)));
    };
    if !tip_radius_mm.is_finite() || tip_radius_mm <= 0.0 {
        return Some(row(LoadLawFault::TipRadiusUnusable, Some(required)));
    }
    let shipped_section_mm2 = std::f64::consts::PI * tip_radius_mm * tip_radius_mm;
    let demanded_section_mm2 = carried_area_mm2 / x_allowed;
    (shipped_section_mm2 < demanded_section_mm2 * (1.0 - AREA_LAW_ARITHMETIC_SLACK)).then(|| {
        row(
            LoadLawFault::TipSmallerThanTheAreaLawDemands,
            Some(required),
        )
    })
}

/// Judge one gate against the sealed snapshot.
///
/// Matched exhaustively and deliberately without a `_` arm: adding a gate to
/// [`PreWriteGate`] without deciding what it does here is a compile error.
///
/// `authority` is the run's own RECORDED EVIDENCE, and that is the only thing a
/// gate may take from it here. It is passed by shared reference, so nothing in
/// this function can consume it into a scene or re-run a stage against it; the
/// borrow admits exactly the disclosure lists the run already wrote down and
/// the sealed snapshot, which is passed separately because the coverage gate
/// measures against it.
fn judge(
    gate: PreWriteGate,
    artifact: &ArtifactId,
    snapshot: &SupportVerificationSnapshot<'_>,
    request: CoverageMeasurementRequest,
    authority: &FinalSupportAuthority<'_>,
) -> Result<GateVerdict, GateRefusal> {
    match gate {
        PreWriteGate::UncoveredIslands => {
            let report = verify_coverage(artifact.clone(), snapshot, request)
                .map_err(|source| GateRefusal::Unmeasurable { gate, source })?;
            if report.accepted() {
                Ok(GateVerdict::Passed(Box::new(report)))
            } else {
                // WARNING MODE (owner ruling, 2026-09-01). This was
                // `GateRefusal::Refused`, and it was the last thing standing
                // between a measured-but-imperfect layout and a file on disk.
                // The measurement is unchanged and it is carried out whole;
                // only the disposition moved. See
                // [`GateVerdict::CoverageWarned`].
                Ok(GateVerdict::CoverageWarned(Box::new(report)))
            }
        }
        // The area law on the run's own emitted-contact record, WARNING MODE
        // included. This arm holds NO decision of its own: it gathers the two
        // things the load gate reads -- the authority's emitted-contact
        // disclosure and the sealed snapshot's independent routed-contact
        // identity walk -- and hands both to `judge_load_gate`, which owns
        // the whole verdict, including which faults may warn and which must
        // still refuse. Every branch of the load gate is therefore reachable
        // from a test without a `FinalSupportAuthority`, which is what this
        // arm's previous inline `match` was not: the branch that downgraded a
        // refusal to a warning could only be entered through a real sealed
        // run, so nothing exercised it and it shipped downgrading faults it
        // must not have.
        PreWriteGate::Load => judge_load_gate(
            authority.emitted_contacts(),
            &snapshot
                .routed_contacts()
                .map(|contact| contact.physical_contact_id())
                .collect::<Vec<_>>(),
        )
        .map(GateVerdict::from),
        // TODO(gate-port): each of these is a live gate in the frozen current
        // verifier and is listed as binding by `docs/engineering-journal/04-migration-plan.md`
        // §"The defect class this milestone exists to make impossible". None
        // has a ported Rust implementation yet, so none can reach a verdict.
        // They are named here, in the refusal path, so that the gap is a
        // reported value rather than an omission — see
        // `PreWriteGateReport::pending`.
        PreWriteGate::Clearance
        | PreWriteGate::UnsupportedMinima
        | PreWriteGate::IslandBirths
        | PreWriteGate::RegularContactInvariants
        | PreWriteGate::UnroutedSpecializedContacts
        | PreWriteGate::Connectivity
        | PreWriteGate::Printability
        | PreWriteGate::Murray
        | PreWriteGate::BallBreach
        | PreWriteGate::PhysicalContactInvariant => Ok(GateVerdict::NotYetPorted),
    }
}

/// Run every pre-write gate against one sealed snapshot.
///
/// **Deliberately private.** It takes a caller-supplied
/// [`CoverageMeasurementRequest`], and a caller-supplied radius is an override
/// in all but name — pass a large enough one and every layout "covers". Public,
/// it would have been the override [`judge_then_emit`] refuses to be, and
/// [`PreWriteGateReport`] is constructible only here, so leaving it public
/// would have made the gate set's verdict forgeable from outside the crate.
/// The one public entry point is `judge_then_emit`, which derives the request
/// from the registry.
///
/// # Errors
///
/// Returns the first [`GateRefusal`] in [`PreWriteGate::ALL`] order. The
/// authorization read and the snapshot integrity check precede every gate, so
/// an unauthorized or incoherent run refuses before any measurement runs.
fn judge_pre_write_gates(
    artifact: &ArtifactId,
    snapshot: &SupportVerificationSnapshot<'_>,
    request: CoverageMeasurementRequest,
    authority: &FinalSupportAuthority<'_>,
) -> Result<PreWriteGateReport, GateRefusal> {
    // The sole read of EMIT_REQUIRE_PASSED_GATES_DEFAULT. It is not an
    // override: `true` runs the gates, and anything else refuses outright.
    let authorized = *m0_constants()
        .map_err(|_| GateRefusal::ConstantsUnavailable)?
        .placeholders()
        .emit_require_passed_gates_default()
        .diagnostic();
    if !authorized {
        return Err(GateRefusal::EmissionAuthorizationNotEnabled);
    }

    snapshot
        .validate_integrity()
        .map_err(GateRefusal::SnapshotIntegrity)?;

    let mut coverage = None;
    let mut pending = Vec::new();
    let mut load_warnings = None;
    let mut coverage_warned = false;
    for gate in PreWriteGate::ALL {
        match judge(gate, artifact, snapshot, request, authority)? {
            GateVerdict::Passed(report) => coverage = Some(*report),
            GateVerdict::Cleared => {}
            GateVerdict::NotYetPorted => pending.push(gate),
            GateVerdict::Warned(warnings) => load_warnings = Some(warnings),
            GateVerdict::CoverageWarned(report) => {
                coverage = Some(*report);
                coverage_warned = true;
            }
        }
    }
    let coverage = coverage.ok_or(GateRefusal::Unmeasurable {
        gate: PreWriteGate::UncoveredIslands,
        source: CoverageVerificationError::SnapshotBinding,
    })?;
    Ok(PreWriteGateReport {
        coverage,
        pending,
        load_warnings,
        coverage_warned,
    })
}

/// Everything one gated emission produced, measured rather than assumed.
#[derive(Clone, Debug)]
pub struct GatedEmission {
    pub gates: PreWriteGateReport,
    /// Task #49. Every island this run's accepted contact set leaves with no
    /// owning contact, read off the authority before it is consumed. Empty on
    /// the ordinary model; non-empty means the artifact below covers an island
    /// nothing is charged to carry, forgiven by the registered censor on the
    /// measurement this same run took.
    pub ownerless_islands: Vec<plinth_support::OwnerlessIslandDisclosure>,
    /// PR-8 grow-to-attest. Every contact this run refused after every remedy,
    /// with the routed load that refusal abandons, and every contact whose tip
    /// the run grew so that it would attest. Both read off the authority before
    /// it is consumed. Both empty on the ordinary model.
    pub abandoned_contacts: Vec<plinth_support::AbandonedContactDisclosure>,
    pub grown_contact_tips: Vec<plinth_support::GrownContactTipDisclosure>,
    /// **SPEC-A4 §A.8's third destination is NOT read here**, deliberately —
    /// see `prepare::seal_and_emit`, which reads the repair ledger off the
    /// authority one frame out, before it hands the authority to
    /// [`judge_then_emit`].
    ///
    /// It used to be read here, and that put the ledger on the SEALED path
    /// only: a gate refusal returns `Err` from this function and the clone
    /// died with it, so the refused path — the path ruling R10 deleted the
    /// failed-tries cap on, the expensive one — reported nothing. One read,
    /// one frame out, reaches both dispositions.
    pub routed_contacts: usize,
    pub final_live_members: usize,
    pub snapshot_digest: u64,
    pub primitive_count: usize,
    pub lift_mm: f64,
    pub plate_z_mm: f64,
    pub emit: EmitReport,
}

/// The stage runner: judge, then lower, then write — in that order, once.
///
/// This is the sequence `prepare()` will call per model, and it is the reason
/// `PipelineExitCode::Gates` can now be produced. The gate reads the snapshot
/// while the authority is still alive; only a passing verdict reaches
/// `into_supported_model`, which consumes it; only then does
/// `plinth_emit::write` create the target path. A gate refusal therefore
/// returns with **no file written at `request.target_path`**.
///
/// **It takes no gate parameter of any kind**, deliberately: not a force flag,
/// not a waiver, and not the coverage measurement request either. A
/// caller-supplied radius would be an override in all but name — pass a large
/// enough one and every layout "covers" — so the request is derived here from
/// the registry, through [`registered_coverage_request`]. The one knob is what
/// the constants say, and the constants are the ratified authority. The
/// request-taking [`judge_pre_write_gates`] is private for the same reason,
/// which makes this the only public door to the gate set.
///
/// **On every error path `request.target_path` is left nonexistent**, including
/// a path that was already occupied when this was called. See the module
/// documentation: the invariant is about the path, not about this call, and a
/// stale artifact at a refused model's target is exactly the misleading
/// hand-off the gate exists to prevent.
///
/// # Errors
///
/// - [`PipelineError::Gates`] (exit code 3) when a gate refuses or cannot be
///   measured, before any write.
/// - [`PipelineError::Lowering`] (exit code 5) when lowering the passed
///   authority fails.
/// - [`PipelineError::Emit`] (exit code 4) when the write fails or the written
///   file cannot be read back (its bytes are kept beside the target; a file
///   that reads back but fails another audit check is EMITTED, with the
///   findings on its report).
pub fn judge_then_emit(
    model: &ModelId,
    authority: FinalSupportAuthority<'_>,
    request: &EmitRequest,
) -> Result<GatedEmission, PipelineError> {
    match judged_emission(model, authority, request) {
        Ok(emission) => Ok(emission),
        Err(error) => {
            // The invariant is "a refused model leaves no path on disk", and
            // this stage owns the path. `plinth_emit::write` already cleans up
            // after itself; what only this scope can clean up is an artifact
            // an *earlier* run left at the same target, which a gate refusal
            // never reaches `write` to remove.
            let _ = std::fs::remove_file(&request.target_path);
            Err(error)
        }
    }
}

fn judged_emission(
    model: &ModelId,
    authority: FinalSupportAuthority<'_>,
    request: &EmitRequest,
) -> Result<GatedEmission, PipelineError> {
    let artifact = ArtifactId::new(model.as_str());
    let coverage = registered_coverage_request().map_err(|source| PipelineError::Gates {
        model: model.clone(),
        source,
    })?;
    // The snapshot borrows the authority, so this scope must close before the
    // authority can be consumed below. That ordering is the whole point: the
    // verdict is reached while the run is still un-lowered and un-written.
    // Task #49: read while the authority is still alive — `into_supported_model`
    // below consumes it, and the supported scene carries no §6 ledger.
    let ownerless_islands = authority.ownerless_islands().to_vec();
    let abandoned_contacts = authority.abandoned_contacts().to_vec();
    let grown_contact_tips = authority.grown_contact_tips().to_vec();
    // SPEC-A4 §A.8's repair ledger is read by the CALLER, before it hands the
    // authority over — see `GatedEmission::routed_contacts`'s neighbouring
    // doc. Reading it here would only reach the sealed path.
    let (gates, routed_contacts, final_live_members, snapshot_digest) = {
        let snapshot =
            authority
                .verification_snapshot()
                .map_err(|source| PipelineError::Gates {
                    model: model.clone(),
                    source: GateRefusal::SnapshotIntegrity(source),
                })?;
        let gates = judge_pre_write_gates(&artifact, &snapshot, coverage, &authority).map_err(
            |source| PipelineError::Gates {
                model: model.clone(),
                source,
            },
        )?;
        (
            gates,
            snapshot.routed_contacts().count(),
            snapshot.final_live_members().count(),
            snapshot.digest(),
        )
    };

    let supported = authority
        .into_supported_model()
        .map_err(|source| PipelineError::Lowering {
            model: model.clone(),
            source,
        })?;

    let emit = emit_write(&supported, request).map_err(|source| PipelineError::Emit {
        model: model.clone(),
        source,
    })?;

    Ok(GatedEmission {
        gates,
        ownerless_islands,
        abandoned_contacts,
        grown_contact_tips,
        routed_contacts,
        final_live_members,
        snapshot_digest,
        primitive_count: supported.primitives().len(),
        lift_mm: supported.lift_mm().get(),
        plate_z_mm: supported.plate_z_mm().get(),
        emit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_support::SupportPreparationLimits;

    #[test]
    fn every_named_gate_appears_exactly_once_in_the_consulted_set() {
        let mut sorted = PreWriteGate::ALL.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            PreWriteGate::ALL.len(),
            "a gate is listed twice, so one of them is never consulted"
        );
        assert_eq!(PreWriteGate::ALL[0], PreWriteGate::UncoveredIslands);
    }

    #[test]
    fn the_registered_emission_authorization_is_enabled_and_readable() {
        // The constant now has a reader, and this is the assertion that it
        // says what `03-physics-and-constants.md` says it must.
        assert!(
            *m0_constants()
                .expect("registered constants")
                .placeholders()
                .emit_require_passed_gates_default()
                .diagnostic(),
            "EMIT_REQUIRE_PASSED_GATES_DEFAULT is the sole emission gate and must be true"
        );
    }

    /// **FU-57 guard: evidence is never a gate input.** The sidecar/card/
    /// render writer lives in `prepare.rs`, at the two call sites that
    /// already hold a refusal; this module must not import that module, or
    /// "evidence, never an input" stops being true by construction and
    /// starts being true only by convention. `include_str!` embeds this
    /// file's own source at compile time, so the guard cannot go stale by
    /// reading the wrong copy.
    ///
    /// The forbidden module name is assembled at runtime, deliberately never
    /// spelled out as one literal anywhere in this file -- spelling it out
    /// would make this very doc comment trip its own guard the moment
    /// `include_str!` embeds it.
    #[test]
    fn gates_module_source_never_imports_the_evidence_module() {
        let forbidden = ["coverage", "_", "evidence"].concat();
        let source = include_str!("gates.rs");
        assert!(
            !source.contains(&forbidden),
            "gates.rs must never import the FU-57 evidence writer -- judge_pre_write_gates \
             stays private and unchanged, and the verdict must be decided before it is \
             explained, never the other way round"
        );
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn the_registered_coverage_request_is_the_planners_own_radius_and_floor() {
        let limits = SupportPreparationLimits::production().expect("production limits");
        let request = registered_coverage_request().expect("registered request");
        let constants = m0_constants().expect("registered constants");
        let policy = constants.placeholders();
        assert_eq!(
            request.radius_mm().get(),
            policy.coverage_radius_factor().diagnostic().get()
                * policy.target_spacing_mm().diagnostic().get()
        );
        assert_eq!(
            request.patch_area_floor_mm2().get(),
            policy.voxel_pitch_mm().diagnostic().get().powi(2)
        );
        // And the ceilings are the support policy's, not this module's.
        assert_eq!(
            request.limits(),
            limits
                .coverage_measurement_limits()
                .expect("registered ceilings")
        );
    }

    // ---- PreWriteGate::Load -------------------------------------------

    /// The registered allowance, read exactly as the gate reads it.
    fn x_allowed() -> f64 {
        *m0_constants()
            .expect("registered constants")
            .evidence()
            .x_allowed()
            .value()
    }

    /// The smallest tip radius the area law admits for `carried_area_mm2`.
    fn required_radius_mm(carried_area_mm2: f64) -> f64 {
        (carried_area_mm2 / (std::f64::consts::PI * x_allowed())).sqrt()
    }

    /// One recorded row, fully described.
    fn described(
        contact_id: u64,
        carried_area_mm2: f64,
        tip_radius_mm: f64,
    ) -> EmittedContactDisclosure {
        EmittedContactDisclosure {
            contact_id,
            position_mm: Some([1.5, -2.5, 3.5]),
            carried_area_mm2: Some(carried_area_mm2),
            tip_radius_mm: Some(tip_radius_mm),
            tip_diameter_mm: Some(tip_radius_mm * 2.0),
        }
    }

    #[test]
    fn the_registered_allowance_is_the_measured_anchor_the_law_names() {
        // The gate's threshold is not a hand-typed twin: it is X_ALLOWED, whose
        // own registry note states this law verbatim. Re-pinned 12 -> 18 on
        // 2026-09-30: Craig ruled the design load ratio to 18 (the registry's
        // X_ALLOWED note records the ruling and the margins it leaves).
        assert!(
            (x_allowed() - 18.0).abs() < 1.0e-12,
            "X_ALLOWED moved; the area law's anchor changed and that is a finding, not a value \
             to refresh here"
        );
    }

    #[test]
    fn a_tip_exactly_at_the_area_law_clears_and_one_hair_under_it_refuses() {
        let carried = 6.0_f64;
        let exact = required_radius_mm(carried);

        // AT the law: `pi*r^2 >= A/X` holds with equality, so the row clears.
        judge_emitted_load_law(&[described(1, carried, exact)], &[1])
            .expect("a tip sized exactly to the law satisfies the law");

        // One hair UNDER it: the same row, refused. This is the boundary the
        // whole gate turns on, so it is pinned from both sides.
        let refusal = judge_emitted_load_law(&[described(1, carried, exact * 0.999)], &[1])
            .expect_err("a tip below the law's demand must refuse emission");
        let violations = refusal
            .load_law_violations()
            .expect("a load refusal carries its rows");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].contact_id, 1);
        assert_eq!(
            violations[0].fault,
            LoadLawFault::TipSmallerThanTheAreaLawDemands
        );
        assert_eq!(refusal.gate(), Some(PreWriteGate::Load));
    }

    #[test]
    fn the_refusal_names_the_contact_the_seat_the_load_and_both_radii() {
        // The bar is that someone who has never read this code can act on the
        // message, so the message itself is asserted, not merely the verdict.
        let carried = 12.0_f64;
        let refusal = judge_emitted_load_law(
            &[described(0, 0.5, 0.2), described(4_242, carried, 0.05)],
            &[0, 4_242],
        )
        .expect_err("an under-sized tip must refuse");
        let text = refusal.to_string();
        assert!(text.contains("contact 4242"), "{text}");
        assert!(text.contains("(1.500, -2.500, 3.500) mm"), "{text}");
        assert!(text.contains("12.0000 mm^2"), "{text}");
        assert!(text.contains("0.0500 mm"), "{text}");
        assert!(
            text.contains(&format!("{:.4} mm", required_radius_mm(carried))),
            "the demanded radius must appear: {text}"
        );
        // The message prints the registered value, whatever it is (18 since
        // Craig's ruling of 2026-09-30); it must never be a typed 12.
        assert!(
            text.contains(&format!("X_ALLOWED = {}", x_allowed())),
            "{text}"
        );
        assert!(text.contains("X_ALLOWED = 18"), "{text}");
        assert!(
            text.contains("1 of 2 emitted contact(s)"),
            "the refused rows must be readable against the set they came from: {text}"
        );
        assert!(
            !text.contains("contact 0"),
            "a compliant contact must not be named as a violation: {text}"
        );
    }

    #[test]
    fn a_routed_contact_the_run_could_not_describe_is_refused_not_papered_over() {
        // `EmittedContactDisclosure` says `None` means "could not be
        // described". Reading either operand as `0.0` would clear the row on
        // the arithmetic, which is exactly the silence the disclosure refuses
        // to write and the gate refuses to read.
        let mut no_area = described(7, 3.0, 0.5);
        no_area.carried_area_mm2 = None;
        let refusal =
            judge_emitted_load_law(&[no_area], &[7]).expect_err("an unstated load refuses");
        assert_eq!(
            refusal.load_law_violations().expect("rows")[0].fault,
            LoadLawFault::CarriedAreaNotDescribed
        );

        let mut no_tip = described(8, 3.0, 0.5);
        no_tip.tip_radius_mm = None;
        let refusal = judge_emitted_load_law(&[no_tip], &[8]).expect_err("an unstated tip refuses");
        assert_eq!(
            refusal.load_law_violations().expect("rows")[0].fault,
            LoadLawFault::TipRadiusNotDescribed
        );
    }

    #[test]
    fn a_zero_or_nonfinite_operand_refuses_rather_than_clearing_on_arithmetic() {
        // A zero radius carrying zero area satisfies `0 >= 0`. That is the
        // arithmetic clearing a contact with no tip at all, so the record check
        // runs first and refuses it.
        let refusal = judge_emitted_load_law(&[described(9, 0.0, 0.0)], &[9])
            .expect_err("a tip of no cross-section carries nothing through nothing");
        assert_eq!(
            refusal.load_law_violations().expect("rows")[0].fault,
            LoadLawFault::TipRadiusUnusable
        );

        let refusal = judge_emitted_load_law(&[described(10, -1.0, 0.5)], &[10])
            .expect_err("a negative area is not an area");
        assert_eq!(
            refusal.load_law_violations().expect("rows")[0].fault,
            LoadLawFault::CarriedAreaUnusable
        );

        let refusal = judge_emitted_load_law(&[described(11, f64::NAN, 0.5)], &[11])
            .expect_err("NaN must never compare its way past the law");
        assert_eq!(
            refusal.load_law_violations().expect("rows")[0].fault,
            LoadLawFault::CarriedAreaUnusable
        );

        let refusal = judge_emitted_load_law(&[described(12, 3.0, f64::INFINITY)], &[12])
            .expect_err("an infinite radius is not a measurement");
        assert_eq!(
            refusal.load_law_violations().expect("rows")[0].fault,
            LoadLawFault::TipRadiusUnusable
        );
    }

    #[test]
    fn a_record_that_does_not_cover_every_routed_contact_refuses() {
        // The way a record-reading gate goes silently vacuous: judge the rows
        // that are there and say nothing about the ones that are not.
        let refusal = judge_emitted_load_law(&[described(1, 0.1, 0.5)], &[1, 2, 3, 4])
            .expect_err("a partial record cannot clear the run that produced it");
        let GateRefusal::LoadRecordMismatch {
            routed,
            described,
            ref mismatches,
        } = refusal
        else {
            panic!("a partial record refuses as a record mismatch: {refusal:?}");
        };
        assert_eq!((routed, described), (4, 1));
        assert_eq!(
            mismatches
                .iter()
                .map(|mismatch| mismatch.contact_id)
                .collect::<Vec<_>>(),
            vec![2, 3, 4],
            "the refusal must NAME the contacts that shipped without a row, not merely say the \
             counts disagreed"
        );
        assert!(mismatches.iter().all(LoadRecordIdMismatch::is_missing));
        assert_eq!(refusal.gate(), Some(PreWriteGate::Load));

        // Including the fully vacuous case: no rows at all for a run that
        // routed contacts.
        let refusal = judge_emitted_load_law(&[], &[10, 11, 12])
            .expect_err("an empty record cannot clear a routed run");
        assert_eq!(
            refusal
                .load_record_mismatches()
                .expect("a record mismatch carries its rows")
                .iter()
                .map(|mismatch| mismatch.contact_id)
                .collect::<Vec<_>>(),
            vec![10, 11, 12]
        );

        // A run that genuinely routed nothing has nothing to judge and clears.
        judge_emitted_load_law(&[], &[]).expect("no routed contacts, no load to check");
    }

    #[test]
    fn a_duplicated_row_is_refused_where_counting_rows_cleared_it() {
        // THE DEFECT THIS TEST EXISTS FOR. Two routed contacts, 17 and 18; a
        // record that names 17 twice and 18 not at all. The count check the
        // gate used to run compares 2 against 2 and passes, and contact 18 --
        // a support this run shipped -- is never judged by the area law at
        // all. Both rows below are law-satisfying, so nothing except the
        // identity check can refuse this record.
        let record = [described(17, 0.1, 0.5), described(17, 0.1, 0.5)];
        let routed = [17_u64, 18];
        assert_eq!(
            record.len(),
            routed.len(),
            "the premise: the OLD count-only check saw two rows for two routed contacts and \
             cleared this record"
        );

        let refusal = judge_emitted_load_law(&record, &routed)
            .expect_err("a record that names one contact twice in place of another must refuse");
        let mismatches = refusal
            .load_record_mismatches()
            .expect("a record mismatch carries its rows");
        assert_eq!(mismatches.len(), 2, "{mismatches:?}");
        assert_eq!(
            mismatches[0],
            LoadRecordIdMismatch {
                contact_id: 17,
                routed: 1,
                described: 2
            }
        );
        assert!(mismatches[0].is_duplicated());
        assert_eq!(
            mismatches[1],
            LoadRecordIdMismatch {
                contact_id: 18,
                routed: 1,
                described: 0
            }
        );
        assert!(mismatches[1].is_missing());

        // The operator has to be able to act on it without reading this file.
        let text = refusal.to_string();
        assert!(text.contains("contact 17"), "{text}");
        assert!(text.contains("DUPLICATED"), "{text}");
        assert!(text.contains("contact 18"), "{text}");
        assert!(text.contains("MISSING"), "{text}");

        // And a row for a contact the run never routed is caught by the same
        // comparison, from the other side.
        let refusal =
            judge_emitted_load_law(&[described(17, 0.1, 0.5), described(99, 0.1, 0.5)], &[17])
                .expect_err("the record must not describe a support the run did not ship");
        let mismatches = refusal
            .load_record_mismatches()
            .expect("a record mismatch carries its rows");
        assert_eq!(mismatches.len(), 1);
        assert!(mismatches[0].is_unrouted());
        assert_eq!(mismatches[0].contact_id, 99);
    }

    // ---- WARNING MODE's fault-class split ------------------------------
    //
    // These call `judge_load_gate`, which IS the dispatch: `judge`'s
    // `PreWriteGate::Load` arm calls it and holds no decision of its own.
    // Testing `judge_emitted_load_law` instead -- which every test above this
    // point does -- can never reach the warn-or-refuse branch, and that is
    // exactly how a dispatch that warned on an unreadable record shipped past
    // three sabotage lanes and a cross-family challenger.

    /// Every way the run can fail to describe a row, with the fault each one
    /// must produce. Named as data so a new record-integrity fault added to
    /// [`LoadLawFault`] without a case here is visible as a gap in one place.
    fn undescribable_rows() -> Vec<(&'static str, EmittedContactDisclosure, LoadLawFault)> {
        let mut carried_area_missing = described(1, 3.0, 0.5);
        carried_area_missing.carried_area_mm2 = None;
        let mut tip_radius_missing = described(1, 3.0, 0.5);
        tip_radius_missing.tip_radius_mm = None;
        vec![
            (
                "carried area missing",
                carried_area_missing,
                LoadLawFault::CarriedAreaNotDescribed,
            ),
            (
                "carried area negative",
                described(1, -1.0, 0.5),
                LoadLawFault::CarriedAreaUnusable,
            ),
            (
                "carried area non-finite",
                described(1, f64::NAN, 0.5),
                LoadLawFault::CarriedAreaUnusable,
            ),
            (
                "tip radius missing",
                tip_radius_missing,
                LoadLawFault::TipRadiusNotDescribed,
            ),
            (
                "tip radius zero",
                described(1, 3.0, 0.0),
                LoadLawFault::TipRadiusUnusable,
            ),
            (
                "tip radius non-finite",
                described(1, 3.0, f64::INFINITY),
                LoadLawFault::TipRadiusUnusable,
            ),
        ]
    }

    #[test]
    fn every_load_law_fault_is_classified_and_only_the_measured_one_may_warn() {
        // The split itself, stated as data rather than inferred from the
        // dispatch's behaviour. `LoadLawFault::class` matches exhaustively, so
        // a sixth fault cannot reach the dispatch unclassified.
        assert_eq!(
            LoadLawFault::TipSmallerThanTheAreaLawDemands.class(),
            LoadLawFaultClass::RunShowsItIsUnsafe
        );
        for (label, _, fault) in undescribable_rows() {
            assert_eq!(
                fault.class(),
                LoadLawFaultClass::RunCannotShowItIsSafe,
                "{label}: a row the run could not describe cannot show the run is safe"
            );
        }
    }

    #[test]
    fn the_dispatch_refuses_every_record_integrity_fault_and_never_warns_on_one() {
        for (label, row, expected_fault) in undescribable_rows() {
            let refusal = match judge_load_gate(&[row], &[1]) {
                Ok(disposition) => panic!(
                    "{label}: the gate dispatch answered {disposition:?}. A record the run \
                     cannot read must REFUSE -- warning mode is scoped to a MEASURED area-law \
                     violation, and this row was never measured at all"
                ),
                Err(refusal) => refusal,
            };
            let violations = refusal
                .load_law_violations()
                .unwrap_or_else(|| panic!("{label}: the refusal must carry its row"));
            assert_eq!(violations.len(), 1, "{label}");
            assert_eq!(violations[0].fault, expected_fault, "{label}");
            assert_eq!(
                violations[0].fault.class(),
                LoadLawFaultClass::RunCannotShowItIsSafe,
                "{label}"
            );
            assert_eq!(refusal.unreadable_load_rows().len(), 1, "{label}");
            assert_eq!(refusal.gate(), Some(PreWriteGate::Load), "{label}");
            let text = refusal.to_string();
            assert!(
                text.contains("1 of them the run could not describe"),
                "{label}: the refusal must say the record could not be read: {text}"
            );
        }
    }

    #[test]
    fn the_dispatch_refuses_a_mixed_record_even_though_most_rows_are_measured() {
        // The case a per-row or first-row test would miss: forty measured
        // violations and one unreadable row. The run has not shown it is safe,
        // whatever the forty say, so the whole refusal stands -- and it still
        // carries every row, measured and unreadable alike.
        let mut record: Vec<EmittedContactDisclosure> = (0_u64..40)
            .map(|id| described(id, 6.0, required_radius_mm(6.0) * 0.5))
            .collect();
        let mut unreadable = described(40, 3.0, 0.5);
        unreadable.tip_radius_mm = None;
        record.push(unreadable);
        let routed: Vec<u64> = (0..=40).collect();

        let refusal = judge_load_gate(&record, &routed)
            .expect_err("one unreadable row in a set of forty measured ones still refuses");
        let violations = refusal
            .load_law_violations()
            .expect("the refusal carries every row");
        assert_eq!(
            violations.len(),
            41,
            "the refusal must carry the measured rows too, not just the unreadable one"
        );
        assert_eq!(refusal.unreadable_load_rows().len(), 1);
        let text = refusal.to_string();
        assert!(
            text.contains("1 of them the run could not describe"),
            "the message must say the record cannot be judged, not only that tips are small: \
             {text}"
        );
    }

    #[test]
    fn the_dispatch_warns_and_does_not_refuse_a_measured_area_law_violation() {
        // The other half of the ruling, pinned so a fix to the above cannot
        // quietly turn warning mode back into a hard refusal.
        let carried = 6.0_f64;
        let starved = required_radius_mm(carried) * 0.5;
        let disposition = judge_load_gate(
            &[
                described(1, carried, required_radius_mm(carried)),
                described(2, carried, starved),
            ],
            &[1, 2],
        )
        .expect("a measured area-law violation warns; it does not refuse");
        let LoadGateDisposition::Warned(warnings) = disposition else {
            panic!("a record with a violating row must WARN, not clear: {disposition:?}");
        };
        assert_eq!(warnings.judged, 2);
        assert_eq!(warnings.violations.len(), 1);
        assert_eq!(warnings.violations[0].contact_id, 2);
        assert_eq!(
            warnings.violations[0].fault,
            LoadLawFault::TipSmallerThanTheAreaLawDemands
        );

        // And a clean record clears rather than warning.
        assert_eq!(
            judge_load_gate(&[described(1, carried, required_radius_mm(carried))], &[1])
                .expect("a clean record clears"),
            LoadGateDisposition::Cleared
        );
    }

    #[test]
    fn the_dispatch_refuses_a_record_mismatch_rather_than_warning_on_it() {
        // `LoadRecordMismatch` is the other "cannot show it is safe" answer,
        // and warning mode must not reach it either -- including when the rows
        // that ARE present also carry measured violations.
        let carried = 6.0_f64;
        let starved = required_radius_mm(carried) * 0.5;
        let refusal = judge_load_gate(
            &[
                described(17, carried, starved),
                described(17, carried, starved),
            ],
            &[17, 18],
        )
        .expect_err("a duplicated row refuses; it is not downgraded to a warning");
        assert!(matches!(refusal, GateRefusal::LoadRecordMismatch { .. }));
    }
}
