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
    #[error(
        "gate Load refused emission: {} of {judged} emitted contact(s) do not satisfy the \
         registered support area law pi * tip_radius_mm^2 >= carried_area_mm2 / X_ALLOWED, \
         evaluated per contact on the final numbers with the registered X_ALLOWED = {x_allowed}:\n{}",
        violations.len(),
        render_load_law_violations(violations)
    )]
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
    /// describe every contact the run routed.
    ///
    /// Fail-closed against the way a record-reading gate goes silently vacuous.
    /// The two counts come from two independent walks of the same finalized
    /// transcript — the sealed snapshot's routed-contact projection and the
    /// authority's own emitted-contact disclosure — so they must agree exactly.
    /// If they do not, some contact shipped without a row to judge, and a gate
    /// that judged the remaining rows would report a safety it did not check.
    #[error(
        "gate Load refused emission: the sealed run routed {routed} contact(s) but its own \
         emitted-contact record describes {described}; the area law cannot clear a record that \
         does not cover every contact the run shipped"
    )]
    LoadRecordIncomplete { routed: usize, described: usize },
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
            | Self::LoadRecordIncomplete { .. }
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
            | Self::LoadRecordIncomplete { .. }
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
            | Self::LoadRecordIncomplete { .. }
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
            | Self::LoadRecordIncomplete { .. }
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
/// `X_ALLOWED` is the registered `Derived<f64>` of 12, whose own registry note
/// states this law verbatim ("pi\*r^2 >= carried\_area/X with X = X\_ALLOWED =
/// 12"), derived as `floor(lower(X_STAR_CONE)/2)` from the measured
/// arm-capacity plate.
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
/// its only other argument is a count it cross-checks rather than obeys. There
/// is nothing here to pass a lenient value to, which is why it can be public
/// where the gate runner cannot.
///
/// # Fail-closed on the record as well as on the arithmetic
///
/// A `Routed` row whose carried area or tip radius is `None` is itself a
/// refusal — see [`LoadLawFault`]. So is a record that describes fewer contacts
/// than the snapshot says the run routed, which is how a record-reading gate
/// would otherwise go quietly vacuous.
///
/// An empty record clears only when the run routed nothing; `routed_contacts`
/// is what makes that statement checkable rather than assumed.
///
/// # Errors
///
/// - [`GateRefusal::LoadRecordIncomplete`] when the record does not describe
///   every routed contact.
/// - [`GateRefusal::ConstantsUnavailable`] /
///   [`GateRefusal::UnusableRegisteredAllowance`] when the registry cannot
///   supply an allowance the law can be evaluated at.
/// - [`GateRefusal::LoadLawViolated`], naming every refused contact, when a
///   shipped tip is smaller than the area law demands or a shipped contact
///   cannot be described.
pub fn judge_emitted_load_law(
    emitted_contacts: &[EmittedContactDisclosure],
    routed_contacts: usize,
) -> Result<(), GateRefusal> {
    if emitted_contacts.len() != routed_contacts {
        return Err(GateRefusal::LoadRecordIncomplete {
            routed: routed_contacts,
            described: emitted_contacts.len(),
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
                Err(GateRefusal::Refused {
                    gate,
                    report: Box::new(report),
                })
            }
        }
        // The area law on the run's own emitted-contact record. No measurement
        // is taken here and no stage is re-run: `judge_emitted_load_law` reads
        // scalars the sealing attempt already wrote down. The routed count is
        // read off the sealed snapshot, which walks the same finalized
        // transcript independently, so the two must agree or the record is
        // refused rather than judged in part.
        PreWriteGate::Load => judge_emitted_load_law(
            authority.emitted_contacts(),
            snapshot.routed_contacts().count(),
        )
        .map(|()| GateVerdict::Cleared),
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
    for gate in PreWriteGate::ALL {
        match judge(gate, artifact, snapshot, request, authority)? {
            GateVerdict::Passed(report) => coverage = Some(*report),
            GateVerdict::Cleared => {}
            GateVerdict::NotYetPorted => pending.push(gate),
        }
    }
    let coverage = coverage.ok_or(GateRefusal::Unmeasurable {
        gate: PreWriteGate::UncoveredIslands,
        source: CoverageVerificationError::SnapshotBinding,
    })?;
    Ok(PreWriteGateReport { coverage, pending })
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
/// - [`PipelineError::Emit`] (exit code 4) when the write or its disk audit
///   fails.
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
        // own registry note states this law verbatim.
        assert!(
            (x_allowed() - 12.0).abs() < 1.0e-12,
            "X_ALLOWED moved; the area law's anchor changed and that is a finding, not a value \
             to refresh here"
        );
    }

    #[test]
    fn a_tip_exactly_at_the_area_law_clears_and_one_hair_under_it_refuses() {
        let carried = 6.0_f64;
        let exact = required_radius_mm(carried);

        // AT the law: `pi*r^2 >= A/X` holds with equality, so the row clears.
        judge_emitted_load_law(&[described(1, carried, exact)], 1)
            .expect("a tip sized exactly to the law satisfies the law");

        // One hair UNDER it: the same row, refused. This is the boundary the
        // whole gate turns on, so it is pinned from both sides.
        let refusal = judge_emitted_load_law(&[described(1, carried, exact * 0.999)], 1)
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
            2,
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
        assert!(text.contains("X_ALLOWED = 12"), "{text}");
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
        let refusal = judge_emitted_load_law(&[no_area], 1).expect_err("an unstated load refuses");
        assert_eq!(
            refusal.load_law_violations().expect("rows")[0].fault,
            LoadLawFault::CarriedAreaNotDescribed
        );

        let mut no_tip = described(8, 3.0, 0.5);
        no_tip.tip_radius_mm = None;
        let refusal = judge_emitted_load_law(&[no_tip], 1).expect_err("an unstated tip refuses");
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
        let refusal = judge_emitted_load_law(&[described(9, 0.0, 0.0)], 1)
            .expect_err("a tip of no cross-section carries nothing through nothing");
        assert_eq!(
            refusal.load_law_violations().expect("rows")[0].fault,
            LoadLawFault::TipRadiusUnusable
        );

        let refusal = judge_emitted_load_law(&[described(10, -1.0, 0.5)], 1)
            .expect_err("a negative area is not an area");
        assert_eq!(
            refusal.load_law_violations().expect("rows")[0].fault,
            LoadLawFault::CarriedAreaUnusable
        );

        let refusal = judge_emitted_load_law(&[described(11, f64::NAN, 0.5)], 1)
            .expect_err("NaN must never compare its way past the law");
        assert_eq!(
            refusal.load_law_violations().expect("rows")[0].fault,
            LoadLawFault::CarriedAreaUnusable
        );

        let refusal = judge_emitted_load_law(&[described(12, 3.0, f64::INFINITY)], 1)
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
        let refusal = judge_emitted_load_law(&[described(1, 0.1, 0.5)], 4)
            .expect_err("a partial record cannot clear the run that produced it");
        assert!(matches!(
            refusal,
            GateRefusal::LoadRecordIncomplete {
                routed: 4,
                described: 1
            }
        ));
        assert_eq!(refusal.gate(), Some(PreWriteGate::Load));

        // Including the fully vacuous case: no rows at all for a run that
        // routed contacts.
        assert!(matches!(
            judge_emitted_load_law(&[], 3).expect_err("an empty record cannot clear a routed run"),
            GateRefusal::LoadRecordIncomplete {
                routed: 3,
                described: 0
            }
        ));

        // A run that genuinely routed nothing has nothing to judge and clears.
        judge_emitted_load_law(&[], 0).expect("no routed contacts, no load to check");
    }
}
