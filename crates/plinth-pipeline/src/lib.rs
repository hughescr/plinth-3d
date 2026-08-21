//! Folder-in, one-artifact-per-model pipeline.
//!
//! The promised signature is now real and lives in [`prepare`]:
//!
//! `plinth_pipeline::prepare(PrepareRequest) -> Result<PrepareReport, PipelineError>`
//!
//! A full run has one outcome per source model, and **one bad model never
//! aborts the folder**: a load failure, an unorientable model, a stage error
//! and a pre-write gate refusal are all per-model dispositions in
//! [`PrepareReport`]. The returned `Err` is reserved for failures of the run
//! itself. Stage stops remain a typed testing outcome, not a stringly alternate
//! state machine. Cache bypass has exactly one data flag, `no_cache`, wired
//! only from the CLI spelling `--no-cache`; no cache exists yet, so the flag is
//! recorded in the report and changes nothing, which is the honest state rather
//! than a faked one (design record OQ-5).
#![forbid(unsafe_code)]

pub mod coverage_evidence;
pub mod gates;
pub mod prepare;

pub use coverage_evidence::{
    CoverageRefusalEvidence, EvidenceSource, UncoveredIslandSummary, COVERAGE_RENDER_MAX_FACES,
    COVERAGE_RENDER_MAX_ISLANDS,
};
// Re-exported so a caller of `UncoveredIslandSummary::worst_gap` (the CLI
// card is the one today) can match on it without taking a direct
// `plinth-support` dependency of its own for one field's type.
pub use plinth_support::CoverageClosureGap;
// Same reason, for the two PR-8 contact disclosures. They are already the
// element types of public `ContactAttestationDisclosure` fields, so they are
// part of this crate's surface either way; without the re-export the CLI can
// print them but cannot NAME them, which is what forces the run report's
// refusal wording to live inline in a `println!` instead of in a line builder a
// test can read back.
pub use plinth_support::{AbandonedContactDisclosure, GrownContactTipDisclosure};
// Same reason again, for SPEC-A4 §A.8's report. It is the type of a public
// field on three of [`ModelPreparationOutcome`]'s four variants, so it is part
// of this crate's surface either way — and §A.8's third destination is the
// CLI's per-model line, which cannot NAME the type it prints without this.
pub use plinth_support::RepairRunReport;
pub use prepare::{
    prepare, registered_decimation_budget, registered_support_request, ChainOutcome, ChainStop,
    GateStop, StagePolicy, SupportedGeometry, ARTIFACT_EXTENSION,
};

// `judge_pre_write_gates` is deliberately absent: it takes a caller-supplied
// coverage request, which is an override in all but name, so `judge_then_emit`
// is the only public way to reach the gate set.
pub use gates::{
    judge_then_emit, registered_coverage_request, GateRefusal, GatedEmission, PreWriteGate,
    PreWriteGateReport,
};

use plinth_core::{m0_constants, Finite, ModelId, PrinterProfile};
use plinth_decimate::{BudgetConfigError, DecimationBudget, DecimationError};
use plinth_emit::{EmitError, EmitReport};
use plinth_hollow::{
    drilling::DrillingLimits, H0AdmissionLimits, HollowDisposition, HollowError, HollowRequest,
    HollowStageFailure, HollowStageOutput,
};
use plinth_orient::{OrientError, OrientSearchLimits, OrientedModel, PinnedBaseline};
use plinth_support::{
    SupportError, SupportPreparationInput, SupportPreparationLimits, SupportPreparationLimitsError,
    SupportRequest,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Runs the complete hollow boundary for one already-oriented model.
///
/// The returned typestate is the only value support planning can consume: it
/// contains either a proved drilled hollow or the preserved oriented solid
/// with an explicit fallback disposition. Planned but undrilled shells have
/// no conversion through this boundary.
///
/// The failure carries the well-composedness repair's measurement when the
/// repair had already run, because the caller's refusal record is built from
/// this value and would otherwise say a repair never happened.
pub fn hollow_stage(
    model: OrientedModel,
    request: &HollowRequest,
) -> Result<HollowStageOutput, HollowStageFailure> {
    plinth_hollow::run(model, request)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HollowStageProjection {
    pub disposition: HollowDisposition,
    pub awaiting_internal_support: bool,
    pub oriented_solid_reserve_retained: bool,
}

#[must_use]
pub fn project_hollow_stage(output: &HollowStageOutput) -> HollowStageProjection {
    let awaiting = matches!(output, HollowStageOutput::AwaitingInternalSupport(_));
    HollowStageProjection {
        disposition: output.report().disposition(),
        awaiting_internal_support: awaiting,
        oriented_solid_reserve_retained: awaiting,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(i32)]
pub enum PipelineExitCode {
    Ok = 0,
    Input = 1,
    Hollow = 2,
    Gates = 3,
    Emit = 4,
    Supports = 5,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageStop {
    Decimate,
    Orient,
    Hollow,
    Support,
}

/// The orientation authority selected for a preparation run.
///
/// **The dense sweep is the default** (`04-migration-plan.md:1659-1669, 1721`).
/// Arbitrary input has no frozen baseline report, and the previous default —
/// `LegacyCoarse`, a named policy with no implementation — meant that
/// `plinth prepare` refused every model it was ever given. That variant is
/// **deleted**, not left standing beside the search: a dead second orientation
/// authority is one more thing to keep honest for no benefit, and reproducing
/// the retired 13-candidate objective is the job of a library driver and
/// `plinth calibrate`, not of a policy the CLI can select.
///
/// **A pinned baseline belongs to one model, so that arm is a map and not a
/// single value.** `plinth_orient::run` reads a rotation out of a baseline
/// pipeline report and applies it to whatever mesh it is handed; it does not
/// and cannot check that the report and the mesh describe the same object. A
/// run-wide baseline over a heterogeneous folder would therefore hand every
/// model the *first* model's rotation, and a misoriented mini still seals,
/// still passes the gates and still emits — a plausible artifact and an
/// unprintable one. Keying by source stem is how the proven composition has
/// always resolved it (`corpus/baseline/<stem>.pipeline.json`), and a model the
/// map does not name is a per-model refusal
/// ([`PipelineError::OrientationBaselineMissing`]), never a borrowed rotation.
/// The dense sweep has no such hazard: it measures the mesh in front of it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "baselines", rename_all = "snake_case")]
pub enum OrientationPolicy {
    /// Search: the recorded D-12 funnel over the whole candidate grid, scoring
    /// all four terms of §3.8 and committing at the production mesh.
    #[default]
    DenseSweep,
    /// One baseline per source stem — the `<stem>` of `<stem>.stl`, which is
    /// also the `<stem>` of the `<stem>.chitubox` the run writes.
    Pinned(BTreeMap<String, PinnedBaseline>),
}

impl OrientationPolicy {
    /// The pinned policy for a run of exactly one named model.
    ///
    /// Spelled out as a constructor because the one-model case is the common
    /// one and building the map by hand at every call site is how a caller ends
    /// up reaching for one shared baseline again.
    #[must_use]
    pub fn pinned_for(stem: &str, baseline: PinnedBaseline) -> Self {
        Self::Pinned(BTreeMap::from([(stem.to_owned(), baseline)]))
    }

    /// The baseline this policy pins for one model, if it pins one.
    #[must_use]
    pub fn baseline_for(&self, stem: &str) -> Option<&PinnedBaseline> {
        match self {
            Self::DenseSweep => None,
            Self::Pinned(baselines) => baselines.get(stem),
        }
    }
}

/// The output printer profiles currently admitted by preparation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputProfile {
    Saturn4Ultra,
}

impl OutputProfile {
    /// The sole CLI spelling accepted by the future `prepare --output` parser.
    pub const SATURN4_ULTRA_CLI_VALUE: &'static str = "saturn4-ultra";

    /// Resolves this typed selection through the ratified M0 printer authority.
    pub fn resolve(self) -> Result<&'static PrinterProfile, OutputProfileError> {
        let constants = m0_constants().map_err(|_| OutputProfileError::ConstantsUnavailable)?;
        match self {
            Self::Saturn4Ultra => Ok(constants.saturn4_ultra()),
        }
    }
}

impl TryFrom<&str> for OutputProfile {
    type Error = OutputProfileError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            Self::SATURN4_ULTRA_CLI_VALUE => Ok(Self::Saturn4Ultra),
            _ => Err(OutputProfileError::Unknown {
                value: value.into(),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum OutputProfileError {
    #[error("unknown output profile {value}")]
    Unknown { value: String },
    #[error("the ratified M0 printer authority is unavailable")]
    ConstantsUnavailable,
}

/// The one production resource-policy assembly for a preparation run.
///
/// Numerical caps stay owned by their stage crates. Decimation has no
/// production probe default by design, so an already finite, bounded budget is
/// required here before any stage can execute.
///
/// **Orientation's recorded precondition is discharged by the `orient` field**,
/// exactly as the dense-sweep design's §6 asks. `plinth_orient::OrientSearchLimits`
/// is five deterministic work-unit budgets — candidates, face- and
/// vertex-direction products, section breakpoints and working bytes — charged
/// exactly from counts before any buffer is allocated, refusing with both the
/// demand and the cap and never trimming the candidate set to fit. There is no
/// wall clock anywhere in it (O-4).
///
/// The first version of this landing derived those caps per rung from the very
/// counts they were then compared against, which made every comparison
/// structurally `n·a ≤ n·b` with `a < b` — no budget could fire for any mesh,
/// and orientation was the one stage in the pipeline with no effective work
/// bound at all. The caps are therefore **absolute**: fixed numbers derived once
/// from the largest model this workspace can run end to end, assembled here
/// before any mesh has been read, and handed to the stage. A cap that scales
/// with its own charge is not a cap.
#[derive(Clone, Copy, Debug)]
pub struct PrepareResourcePolicy {
    decimation: DecimationBudget,
    hollow: H0AdmissionLimits,
    drilling: DrillingLimits,
    support: SupportPreparationLimits,
    orient: OrientSearchLimits,
}

impl PrepareResourcePolicy {
    /// Selects the single currently supported printer policy without exposing
    /// stage-local numerical limits to the pipeline or CLI.
    pub fn for_saturn4_ultra(
        decimation: DecimationBudget,
    ) -> Result<Self, PrepareResourcePolicyError> {
        OutputProfile::Saturn4Ultra.resolve()?;
        decimation.validate_for_execution()?;
        let support = SupportPreparationLimits::production()?;
        Ok(Self {
            decimation,
            hollow: H0AdmissionLimits::production(),
            drilling: DrillingLimits::production(),
            support,
            orient: OrientSearchLimits::production(),
        })
    }

    #[must_use]
    pub const fn decimation(self) -> DecimationBudget {
        self.decimation
    }

    #[must_use]
    pub const fn hollow(self) -> H0AdmissionLimits {
        self.hollow
    }

    #[must_use]
    pub const fn drilling(self) -> DrillingLimits {
        self.drilling
    }

    /// The orientation search's work budget, run-wide and absolute.
    #[must_use]
    pub const fn orient(self) -> OrientSearchLimits {
        self.orient
    }

    #[must_use]
    pub const fn support_input<'a>(
        self,
        layers: &'a plinth_core::LayerPlan,
        delicacy_by_face: &'a [f64],
        reservations: &'a [plinth_support::perception::SpecializedOwnerReservation],
    ) -> SupportPreparationInput<'a> {
        self.support.input(layers, delicacy_by_face, reservations)
    }
}

#[derive(Debug, Error)]
pub enum PrepareResourcePolicyError {
    #[error(transparent)]
    Profile(#[from] OutputProfileError),
    #[error("decimation execution budget is invalid")]
    Decimation(#[source] BudgetConfigError),
    #[error("support preparation limits are invalid")]
    Support(#[source] SupportPreparationLimitsError),
    /// The selected printer profile publishes a lift the support stage cannot
    /// plan against. The profile is the sole authority for it, so there is no
    /// substitute value to fall back to.
    #[error(
        "the selected printer profile's registered lift is not a positive finite millimetre value"
    )]
    Lift,
}

impl From<BudgetConfigError> for PrepareResourcePolicyError {
    fn from(error: BudgetConfigError) -> Self {
        Self::Decimation(error)
    }
}

impl From<SupportPreparationLimitsError> for PrepareResourcePolicyError {
    fn from(error: SupportPreparationLimitsError) -> Self {
        Self::Support(error)
    }
}

/// One batch request: every source STL in `input_models_dir` maps to one output stem.
///
/// # Units contract: source coordinates ARE final printed millimetres
///
/// Every STL under `input_models_dir` is read on the assumption that one
/// mesh unit is one millimetre of cured resin at final print size — this
/// chain does not scale on the way in, and nothing downstream of it expects
/// to be rescaled on the way out. That assumption is load-bearing for every
/// stage this request drives: the decimation deviation budget, the hollow
/// wall thickness, support tip diameters, and support member/contact sizing
/// are all registered in millimetres and are all measured against the
/// geometry exactly as loaded. A slicer rescale applied to the emitted
/// `.chitubox` — or a source model exported at the wrong unit scale in the
/// first place — silently invalidates every one of those commitments at
/// once, without producing any error this chain can raise: the geometry
/// changes size after the only stage that measured it against a millimetre
/// bound.
///
/// `decimation_budget`'s scale-sanity check (the registered
/// `DECIMATION_SCALE_ACCEPTANCE_MM`, an inclusive `[10, 300]` mm
/// longest-bbox-axis window) is the one automated backstop in this path, and
/// it catches only gross unit errors — a model left in metres, centimetres,
/// or an arbitrary CAD unit. It does not catch a model that is wrong by a
/// plausible scale factor: 1.5x, 2x, or a percent-level manual
/// scale-to-fit all land inside a still-plausible 10-300 mm mini size and
/// pass silently, with every registered millimetre bound downstream then
/// judging the wrong physical object. Confirm the units contract on the
/// source models before calling [`prepare`], not after.
#[derive(Clone, Debug, PartialEq)]
pub struct PrepareRequest {
    pub input_models_dir: PathBuf,
    pub output_dir: PathBuf,
    pub orientation: OrientationPolicy,
    pub output_profile: OutputProfile,
    pub decimation_budget: DecimationBudget,
    pub hollow: HollowRequest,
    pub support: SupportRequest,
    /// The sole cache-bypass control, corresponding only to CLI `--no-cache`.
    pub no_cache: bool,
    pub stop_after: Option<StageStop>,
}

// `Eq` is gone, for two independent reasons that both land here: the wall
// fields (`Finite` wraps an `f64`, `PartialEq` only) and, as of FU-57,
// `Refused`/`Failed`'s `CoverageRefusalEvidence`, whose islands hold `f64`
// uncovered-area and gap measurements. Nothing keys a map or a set on an
// outcome — the derive was reachable only because every field happened to be
// integral — and a report is compared for equality in tests, never keyed on.
/// **PR-8 grow-to-attest, as the run report carries it.**
///
/// A tip is sized by the load law, and PR-8 probes attachment with a ray ring at
/// that same tip radius, so a load-sized thin tip seated near a model silhouette
/// edge can find nothing but air and be refused for being SMALL. Where that is
/// curable the run grows the tip until it attests and builds the grown one;
/// where it is not, the contact is refused and the load it carried is abandoned
/// with no neighbour told and nothing grown in its place.
///
/// Both halves are reported because both are decisions the run made on its own:
///
/// * `grown_contact_tips` — a support the run RESIZED, with the load it kept.
/// * `abandoned_contacts` — a support that is NOT THERE, with the routed load in
///   mm² that nothing now carries. The coverage measurement works at a 6 mm
///   radius and cannot see a single missing claw tip, so if this list does not
///   say it, nothing does.
///
/// Both empty is the ordinary outcome on a healthy model.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactAttestationDisclosure {
    #[serde(default)]
    pub abandoned_contacts: Vec<plinth_support::AbandonedContactDisclosure>,
    #[serde(default)]
    pub grown_contact_tips: Vec<plinth_support::GrownContactTipDisclosure>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelPreparationOutcome {
    /// A `.chitubox` was written, and **what was written is stated rather than
    /// assumed**.
    ///
    /// `geometry` is the same field [`Self::Refused`] carries and it is
    /// load-bearing for the same reason. Two of the four
    /// [`prepare::SupportedGeometry`] values are *substitutions*: the chain
    /// planned the drilled shell, could not seal or could not hollow it, and
    /// carried the oriented solid instead. A report that said only `EMITTED`
    /// would make an artifact produced by substitution indistinguishable from
    /// the intended drilled shell — the reader would have no way to tell that
    /// the object on the plate is not the object the pipeline set out to make.
    /// `hollow_disposition` is the hollow stage's own verdict beside it, with
    /// the caveat [`prepare::SupportedGeometry::HollowStageRefused`] records:
    /// on the fallback paths the disposition is stamped by a stage that never
    /// reached a threshold test, and `geometry` is where the truth lives.
    ///
    /// `cavity_repair` is the well-composedness repair's own measurement, and
    /// it is here because the alternative is that the repair is invisible
    /// outside `plinth-hollow`. It says how many critical 2×2×2 cells the
    /// cavity mask held, how many cavity voxels were handed back to material to
    /// remove them, and what that cost in cell visits and whole-grid passes.
    /// `None` means no repair result was recorded for this model — nothing in
    /// this run reached cavity selection, so there is nothing to report. It is
    /// **not** "a repair may have run and its measurement was dropped": the
    /// substitution paths (a hollow-stage refusal, and a support refusal on a
    /// shell that was built) carry the repair's measurement onto the record
    /// they produce, which is what makes `None` a claim rather than a silence.
    ///
    /// `wall_mm` (FU-37) and `realized_wall` (FU-55) are the shell's defining
    /// parameter and what the engine actually delivered against it. They are
    /// here because otherwise they are not anywhere: `HollowReport` carries
    /// both, but the report dies inside `seal_and_emit` and nothing on disk
    /// serializes one, so `plinth prepare --wall-mm 3.0` and `--wall-mm 2.5`
    /// produced runs whose printed report, serialized report and `.chitubox`
    /// artifact were all metadata-indistinguishable — the exact
    /// indistinguishability FU-37 was filed to fix. What these two fields
    /// close is **this report**, printed and serialized; the `.chitubox` wire
    /// format still carries no wall (`plinth_emit::write` embeds geometry,
    /// placement and support records only), which is recorded as a remaining
    /// gap rather than claimed closed.
    ///
    /// `wall_mm` is `None` only where no hollow request stood behind the
    /// model; `realized_wall` is `None` on every path that never built a
    /// shell to measure — including the two solid-reserve substitutions,
    /// whose artifact is the oriented solid and for which a realized wall
    /// would describe a shell that is not in the file — and it carries its
    /// own [`plinth_hollow::RealizedWallClaim`] so a query bound is never
    /// read as a measurement.
    ///
    /// `filled_back_chambers_ml` is every cavity pocket the drilling stage
    /// turned back into solid material because no legal drain reached it,
    /// each stated in millilitres. It exists so that giving up on a chamber
    /// is a number in the report rather than a silence. An empty list means
    /// nothing was given up; a list with entries means that much resin volume
    /// the wall thickness would have allowed is still solid in the printed
    /// model, and the run report prints one line per entry.
    ///
    /// The alternative to filling a chamber back is shipping a sealed void
    /// full of liquid resin, which is never acceptable, so this is a loss the
    /// engine takes deliberately — and a loss taken deliberately has to be
    /// stated.
    Artifact {
        emit: EmitReport,
        geometry: prepare::SupportedGeometry,
        hollow_disposition: HollowDisposition,
        #[serde(default)]
        cavity_repair: Option<plinth_hollow::CavityRepairEvidence>,
        #[serde(default)]
        wall_mm: Option<Finite>,
        #[serde(default)]
        realized_wall: Option<plinth_hollow::RealizedWall>,
        /// 2026-08-13 owner ruling: the savings-gate refusal's own
        /// measurement, when `hollow_disposition` names one
        /// (`SolidDecisionReason::BelowThreshold` /
        /// `BelowThresholdAfterRepair`). `None` on every other disposition,
        /// including a hollowed one — see
        /// [`plinth_hollow::SavingsShortfall`]. `default` for the same
        /// reason `realized_wall` is: a report frozen before this field
        /// existed has to keep deserializing.
        #[serde(default)]
        savings_shortfall: Option<plinth_hollow::SavingsShortfall>,
        #[serde(default)]
        filled_back_chambers_ml: Vec<Finite>,
        /// Task #49. Islands this run's accepted contact set left with no
        /// owning contact, each with its ownerless face count and area and
        /// what the run's own coverage measurement found uncovered there.
        /// Empty on the ordinary model; a non-empty list says this artifact
        /// covers an island nothing is charged to carry, which the registered
        /// censor forgave rather than nobody noticed. `default` for the same
        /// reason the fields above are: a report frozen before this field
        /// existed has to keep deserializing.
        #[serde(default)]
        ownerless_islands: Vec<plinth_support::OwnerlessIslandDisclosure>,
        /// PR-8 grow-to-attest, both halves. See
        /// [`ContactAttestationDisclosure`]. Boxed because two more vectors
        /// inline would make this variant the reason the enum is oversized for
        /// every other variant that has to be moved with it; `default` for the
        /// same reason the fields above are, so a report frozen before this
        /// existed keeps deserializing.
        #[serde(default)]
        contacts: Box<ContactAttestationDisclosure>,
        /// **SPEC-A4 §A.8's third destination — what the one-winner coverage
        /// repair controller spent to produce this artifact.** Trials,
        /// commits, the rejection histogram by class, the per-trial wall-time
        /// order statistics, and rule N4's demotion pair.
        ///
        /// It is here because ruling R10 deleted the failed-tries cap and put
        /// loud reporting in its place. A cost that only ever reaches stderr
        /// is a cost nobody can compare between two runs, and the trade R10
        /// made would not hold. `trials=0 commits=0` is the ordinary model
        /// that covered on its baseline layout and paid nothing.
        ///
        /// `None` means the sealed run kept no repair ledger. Since §G.1
        /// stage 4 no production path produces that; `default` for the same
        /// reason the fields above are, so a report frozen before this field
        /// existed keeps deserializing. Boxed for the same reason `contacts`
        /// is: inline it is over a hundred bytes that every other variant of
        /// this enum has to be moved with.
        #[serde(default)]
        repair_report: Option<Box<plinth_support::RepairRunReport>>,
        /// **SPEC-A4 §D.2 — where this artifact's EXACT attribution was
        /// written.**
        ///
        /// `repair_report` above says what the controller spent; this says
        /// where to read what it BOUGHT, contact by contact: which repair-pool
        /// row was appended on which trial, which routes its global score came
        /// from, and which route's failing patch it closed. The stream is too
        /// large to inline in a run report — a model that spends thousands of
        /// trials carries thousands of rows — so the report carries the path,
        /// the counts and any write error, and the file carries §D.
        ///
        /// `None` means the sealed run wrote no ledger; since this landed no
        /// production path produces that. `default` for the same reason the
        /// fields above are.
        #[serde(default)]
        repair_ledger: Option<crate::coverage_evidence::SealedRepairLedger>,
    },
    Stopped {
        after: StageStop,
    },
    /// The pre-write physics gates refused this model.
    ///
    /// **There is no file at this model's target path** — not this run's, and
    /// not an earlier run's either. `judge_then_emit` owns that invariant and
    /// enforces it about the path, not about one call.
    ///
    /// `geometry` is not decoration: "the drilled shell sealed and the gate
    /// refused its plan" and "support refused the drilled shell, the chain
    /// fell back to the oriented solid, and the gate refused that" are the
    /// substitution the terminal refusal exists to prevent, and only this
    /// field tells them apart.
    Refused {
        gate: Option<PreWriteGate>,
        geometry: Option<prepare::SupportedGeometry>,
        violation: String,
        /// FU-57: the sidecar/card/render evidence for a coverage refusal,
        /// when `gate` is [`PreWriteGate::UncoveredIslands`]. `#[serde(default)]`
        /// keeps every report serialized before this field existed readable.
        /// `None` either because the refusal was not a coverage refusal or
        /// because evidence persistence itself found nothing to write --
        /// never because writing it failed silently: a write failure is
        /// recorded inside `CoverageRefusalEvidence::write_error` instead, so
        /// this stays `Some` with the failure named.
        /// Boxed: `CoverageRefusalEvidence` carries the full ranked island
        /// list, and an unboxed field would put that weight on every
        /// `Refused` value, including a non-coverage gate refusal.
        #[serde(default)]
        evidence: Option<Box<coverage_evidence::CoverageRefusalEvidence>>,
        /// **SPEC-A4 §A.8's third destination on the REFUSED path** — what
        /// the one-winner coverage-repair controller spent before the gate
        /// refused this model. Same fields, same meaning and same reason as
        /// [`Self::Artifact`]'s copy.
        ///
        /// It is here because §A.8 says the report goes to three places "on
        /// every path — sealed or refused", and because the refused path is
        /// the expensive one: ruling R10 deleted the failed-tries cap, so a
        /// model that refuses runs its candidate pool all the way out. A run
        /// report that states the spend for the cheap path and nothing for
        /// the costly one is not the visibility R10 traded the cap for.
        ///
        /// `None` means no controller ledger exists for this refusal — the
        /// model never reached a sealed run at all. `default` for the same
        /// reason `evidence` is.
        #[serde(default)]
        repair_report: Option<Box<plinth_support::RepairRunReport>>,
    },
    /// The model did not reach the gates at all. `exit_code` is the frozen
    /// per-stage projection of the typed error behind `detail`.
    Failed {
        exit_code: PipelineExitCode,
        detail: String,
        /// FU-57: populated only when this failure is the FU-56
        /// coverage-closure controller's refusal (`SupportRunError::
        /// CoverageClosure`) -- the one `Failed` disposition that is also
        /// "a coverage refusal" in the ruling's sense. Every other `Failed`
        /// disposition carries `None`. `#[serde(default)]` for the same
        /// reason as `Refused::evidence`; boxed for the same reason too.
        #[serde(default)]
        evidence: Option<Box<coverage_evidence::CoverageRefusalEvidence>>,
        /// **SPEC-A4 §A.8's third destination on the FAILED path** — what the
        /// one-winner coverage-repair controller spent before this failure.
        ///
        /// Two failure classes reach it, and they are the two that matter
        /// most to ruling R10's trade:
        ///
        /// 1. the controller's own refusal (`SupportRunError::CoverageClosure`
        ///    — the disposition FU-56 gave the coverage verdict), where the
        ///    ledger comes off the typed refusal, because there is no sealed
        ///    authority on that path; and
        /// 2. a model that SEALED and then failed to lower or to emit, where
        ///    it comes off the authority before it was handed over.
        ///
        /// `None` on every other failure, and that is a measurement, not a
        /// gap: a model that failed at or before attempt 0 never entered the
        /// repair loop, and writing `trials=0` for it would claim the loop ran
        /// and spent nothing. `default` for the same reason `evidence` is.
        #[serde(default)]
        repair_report: Option<Box<plinth_support::RepairRunReport>>,
    },
}

impl ModelPreparationOutcome {
    /// The FU-57 coverage-refusal evidence, when this outcome carries any --
    /// from either refusal source, without the caller needing to match on
    /// which variant it landed in.
    #[must_use]
    pub fn coverage_evidence(&self) -> Option<&coverage_evidence::CoverageRefusalEvidence> {
        match self {
            Self::Refused { evidence, .. } | Self::Failed { evidence, .. } => evidence.as_deref(),
            Self::Artifact { .. } | Self::Stopped { .. } => None,
        }
    }

    /// **SPEC-A4 §A.8's third destination, read the same way on every
    /// disposition** — what the one-winner coverage-repair controller spent on
    /// this model, whether the model emitted, was refused by a gate, or
    /// failed.
    ///
    /// One accessor rather than three match arms at every reader, for the
    /// reason `coverage_evidence` above is one: §A.8 says the report is
    /// reported "on every path", and a reader that has to remember which
    /// variants carry it is a reader that will forget one. That is exactly the
    /// defect this method was added to close — the `Artifact` arm printed the
    /// spend and the `Refused` and `Failed` arms printed nothing, on the path
    /// where ruling R10 removed the cap.
    ///
    /// `Stopped` carries none because a run told to stop before the sealed
    /// stage never ran a controller. A run told to stop AFTER it did still
    /// prints the operator's stderr line; carrying it into the report as well
    /// is not wired, and is disclosed rather than claimed.
    #[must_use]
    pub fn repair_report(&self) -> Option<&plinth_support::RepairRunReport> {
        match self {
            Self::Artifact { repair_report, .. }
            | Self::Refused { repair_report, .. }
            | Self::Failed { repair_report, .. } => repair_report.as_deref(),
            Self::Stopped { .. } => None,
        }
    }

    /// Whether this model's disposition is a **coverage refusal** — the
    /// pre-write coverage gate refusing it, or the FU-56 closure controller
    /// refusing to emit an under-covered plan.
    ///
    /// Both are the same safety verdict wearing two dispositions, and this is
    /// the predicate the run-level signal keys on. See
    /// [`PrepareReport::exit_code`] for why that matters.
    #[must_use]
    pub fn is_coverage_refusal(&self) -> bool {
        match self {
            Self::Refused { gate, evidence, .. } => {
                *gate == Some(PreWriteGate::UncoveredIslands) || evidence.is_some()
            }
            Self::Failed { evidence, .. } => evidence.is_some(),
            Self::Artifact { .. } | Self::Stopped { .. } => false,
        }
    }
}

impl ModelPreparationOutcome {
    /// This model's contribution to the run's exit status.
    #[must_use]
    pub const fn exit_code(&self) -> PipelineExitCode {
        match self {
            Self::Artifact { .. } | Self::Stopped { .. } => PipelineExitCode::Ok,
            Self::Refused { .. } => PipelineExitCode::Gates,
            Self::Failed { exit_code, .. } => *exit_code,
        }
    }
}

/// The size this run assumed the model was, recorded because it is the single
/// most common reason a plausible model produces an implausible result.
///
/// It is the decimation stage's own scale evidence, not a second measurement:
/// a model whose longest axis falls outside `DECIMATION_SCALE_ACCEPTANCE_MM`
/// is refused before any search runs, and the refusal is meaningless without
/// the number it was refused at. Reported on the success path too, so an
/// operator can see that a mini they believe is 32 mm tall was in fact
/// processed as 3.2 mm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssumedPrintSize {
    pub longest_axis_mm: f64,
    pub dimensions_mm: [f64; 3],
}

/// `Eq` is deliberately absent throughout the report types below:
/// [`AssumedPrintSize`] carries millimetre quantities, and a report is compared
/// for equality in tests, never keyed on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelPreparationReport {
    pub model: ModelId,
    pub source_path: PathBuf,
    /// `None` only when the model failed before decimation measured it.
    pub assumed_print_size: Option<AssumedPrintSize>,
    /// What the automatic repair pre-pass did to this model's loaded input,
    /// per defect class and per action.
    ///
    /// `None` only when the model failed before it was loaded. An
    /// all-zero-count evidence and a `None` are different statements and the
    /// report keeps them apart: the first says the pre-pass looked and found
    /// nothing, the second says it never ran.
    ///
    /// **This is where a silent geometry change would otherwise hide.** The
    /// 2026-08-10 ruling made repair automatic; an automatic repair that is
    /// not reported is an unrecorded edit to the owner's model, so every
    /// action carries its own row here and the run report prints a line for
    /// any model the pre-pass touched.
    #[serde(default)]
    pub repair: Option<plinth_mesh::RepairEvidence>,
    pub outcome: ModelPreparationOutcome,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareReport {
    pub no_cache: bool,
    pub models: Vec<ModelPreparationReport>,
}

impl PrepareReport {
    /// Full-run artifacts in deterministic model order. A stopped, refused or
    /// failed model contributes no artifact.
    pub fn artifacts(&self) -> impl Iterator<Item = &Path> {
        self.models.iter().filter_map(|model| match &model.outcome {
            ModelPreparationOutcome::Artifact { emit, .. } => Some(emit.artifact_path.as_path()),
            ModelPreparationOutcome::Stopped { .. }
            | ModelPreparationOutcome::Refused { .. }
            | ModelPreparationOutcome::Failed { .. } => None,
        })
    }

    /// The run's exit status: [`PipelineExitCode::Gates`] if **any** model in
    /// the run was refused by the pre-write physics gates, and otherwise the
    /// code of the first model, in the run's own deterministic order, that did
    /// not reach an artifact.
    ///
    /// **`Gates` is sticky, and it is the only code that is.** The exit code is
    /// the sole machine-observable safety signal a calling script has, and
    /// `Gates = 3` is the one code with a documented meaning callers act on:
    /// "the physics gates refused a model, there is no artifact for it, and the
    /// refusal is the finding". First-in-order alone would let that signal be
    /// masked by an unrelated earlier disposition — a macOS `._sidecar` that
    /// sorts before every real model and fails to load would turn a folder
    /// containing a gate refusal into exit 1, which reads as "one file was
    /// unreadable" and hides the refusal completely. Every other code stays
    /// first-in-order, because there is no total severity ranking over them
    /// (`Hollow = 2` is not "worse" than `Input = 1`, it is a different
    /// verdict) and inventing one would be a second contract.
    ///
    /// **A coverage refusal is sticky too, at second priority (2026-08-12).**
    /// FU-56 created a second disposition for the same safety verdict: the
    /// closure controller refusing to emit an under-covered plan lands as
    /// `Failed` with [`PipelineExitCode::Supports`], not as `Refused` with
    /// `Gates`. Before this rule that verdict was first-in-order and could be
    /// masked by an unrelated earlier `Input` failure — the exact masking the
    /// `Gates` rule above exists to prevent, reintroduced through a new door
    /// for the very refusal class FU-57 exists to make visible. So a run
    /// containing a coverage refusal exits with that refusal's code unless a
    /// gate refusal outranks it. This **adds** a bound on which signal may be
    /// masked; it loosens nothing, moves no threshold, and does not invent a
    /// severity ranking over the remaining codes, which stay first-in-order
    /// exactly as before.
    #[must_use]
    pub fn exit_code(&self) -> PipelineExitCode {
        let mut first_non_ok = None;
        let mut coverage = None;
        for model in &self.models {
            let code = model.outcome.exit_code();
            if code == PipelineExitCode::Gates {
                return PipelineExitCode::Gates;
            }
            if code != PipelineExitCode::Ok {
                if model.outcome.is_coverage_refusal() && coverage.is_none() {
                    coverage = Some(code);
                }
                if first_non_ok.is_none() {
                    first_non_ok = Some(code);
                }
            }
        }
        coverage
            .or(first_non_ok)
            .unwrap_or(PipelineExitCode::Ok)
    }

    /// Whether the pre-write physics gates refused any model in this run.
    #[must_use]
    pub fn has_gate_refusal(&self) -> bool {
        self.models
            .iter()
            .any(|model| matches!(model.outcome, ModelPreparationOutcome::Refused { .. }))
    }

    /// Whether **any** model in this run was refused on coverage, by either
    /// the pre-write coverage gate or the FU-56 closure controller.
    ///
    /// [`Self::has_gate_refusal`] answers only the first of those, because it
    /// is about the `Refused` disposition rather than about coverage; a
    /// caller counting the run's coverage refusals wants this one.
    #[must_use]
    pub fn has_coverage_refusal(&self) -> bool {
        self.models
            .iter()
            .any(|model| model.outcome.is_coverage_refusal())
    }
}

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("cannot enumerate input model folder {path}")]
    InputFolder {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot create output folder {path}")]
    OutputFolder {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The run's stage limits will not assemble from the ratified authorities,
    /// so no model in it can be planned. This is a run failure, not a model's.
    #[error("the production resource policy for this run is unavailable")]
    ResourcePolicy {
        #[source]
        source: PrepareResourcePolicyError,
    },
    #[error("model {model} could not be read as a binary STL from {path}")]
    Load {
        model: ModelId,
        path: PathBuf,
        #[source]
        source: plinth_mesh::MeshIoError,
    },
    /// Decimation refused to authorize a production mesh because the model's
    /// longest axis is outside `DECIMATION_SCALE_ACCEPTANCE_MM`.
    ///
    /// Separate from [`Self::Decimate`] because it is not a failure of the
    /// search: it is the search declining to run on a model whose assumed
    /// print size says it is not a print-scale object. The size is on the
    /// error because a scale refusal without its measurement is unactionable.
    #[error(
        "model {model} is outside the print-scale window decimation will authorize \
         (longest axis {longest_axis_mm} mm)"
    )]
    ScaleSuspicious {
        model: ModelId,
        longest_axis_mm: f64,
    },
    /// The run's pinned-orientation policy names no baseline for this model.
    ///
    /// A pinned baseline is one mesh's recorded rotation. Applying it to a
    /// different mesh is not an approximation of that model's orientation, it
    /// is another model's answer — and a misoriented model still seals, still
    /// passes the gates and still emits, so nothing downstream would catch it.
    /// A folder whose policy does not name this stem is therefore a per-model
    /// refusal, exactly like an unported policy.
    #[error(
        "model {model} has no pinned orientation baseline in this run's policy; a pinned \
         baseline records one mesh's rotation and is never applied to another model"
    )]
    OrientationBaselineMissing { model: ModelId },
    /// An artifact left at this model's target path by an earlier run could not
    /// be removed before the run started.
    ///
    /// Reported rather than ignored: the whole reason the target is cleared up
    /// front is that a stale file beside a failure disposition is read as this
    /// run's answer. A refusal or an error announced while the old file is
    /// still sitting there is the same misleading hand-off, so a failed removal
    /// fails the model by name instead.
    #[error("model {model} could not have an earlier run's artifact at {path} cleared")]
    StaleArtifact {
        model: ModelId,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// FU-57: an earlier run's coverage-refusal evidence (the
    /// `<model>.coverage.json` sidecar or a `<model>.coverage.island-*.svg`
    /// render) sitting at this model's target could not be removed before
    /// this run started.
    ///
    /// Reported rather than ignored, for exactly [`Self::StaleArtifact`]'s
    /// reason: stale evidence beside a fresh disposition is read as this
    /// run's answer, so a failed removal fails the model by name instead of
    /// proceeding with an evidence trail that may no longer be true.
    #[error(
        "model {model} could not have an earlier run's coverage-refusal evidence at {path} cleared"
    )]
    StaleCoverageEvidence {
        model: ModelId,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The prepared mesh has no finite Z extent, so no layer plan spans it.
    #[error("model {model} has no finite z extent to plan layers over")]
    DegenerateExtent { model: ModelId },
    /// The prepared mesh's delicacy field could not be evaluated, so the
    /// support stage has no measurement of which faces are delicate. There is
    /// no substitute: an all-zeros field is not "no delicacy", it is the claim
    /// that nothing on the model is delicate, and this stage does not make
    /// claims it has not measured.
    #[error("model {model} could not have its prepared-mesh delicacy field evaluated")]
    Delicacy {
        model: ModelId,
        #[source]
        source: plinth_mesh::DelicacyError,
    },
    /// The layer plan support planning plans against could not be built.
    #[error("model {model} could not be given a layer plan for support planning")]
    LayerPlan {
        model: ModelId,
        #[source]
        source: plinth_core::LayerPlanError,
    },
    // **There is deliberately no `Repair` variant.** The automatic repair
    // pre-pass has no failure disposition at all: `plinth_mesh::repair_mesh`
    // returns a mesh, not a `Result`. An unrepairable defect is recorded in
    // [`plinth_mesh::RepairEvidence`] and left for the unchanged closed-solid
    // gate to judge, which is the whole shape of the 2026-08-10 ruling; an
    // input the pass cannot work on at all — over its resource bound (FU-45),
    // or any internal failure below it — has every action **skipped and
    // recorded** in `RepairEvidence::actions_skipped` with the mesh returned
    // bit-identical; and a plan that would empty the mesh is rolled back the
    // same way (FU-46). This enum used to carry a `Repair` variant, and it was
    // reachable: it made a stage added to *help* a model into a stage that
    // could refuse one, which is the inversion the ruling forbids.
    #[error("model {model} failed production decimation")]
    Decimate {
        model: ModelId,
        #[source]
        source: DecimationError,
    },
    #[error("model {model} failed orientation policy")]
    Orient {
        model: ModelId,
        #[source]
        source: OrientError,
    },
    #[error("model {model} failed hollowing or drilling")]
    Hollow {
        model: ModelId,
        #[source]
        source: HollowError,
    },
    #[error("model {model} failed support or raft planning")]
    Support {
        model: ModelId,
        #[source]
        source: SupportError,
    },
    /// The sealed support run refused this model, so there was never a plan for
    /// the pre-write gates to judge.
    ///
    /// Distinct from [`Self::Support`] only in the error type it carries:
    /// `run_support` reports a run *phase* (`Preparation`, `Routing`, …), where
    /// `SupportError` reports a planner verdict. Both project to
    /// [`PipelineExitCode::Supports`].
    #[error("model {model} failed its sealed support run")]
    SealedRun {
        model: ModelId,
        #[source]
        source: plinth_support::SupportRunError,
    },
    /// Lowering a gate-passed authority into the emittable scene failed.
    ///
    /// This is the support stage's own last step, so it projects to the same
    /// exit code; it is a separate variant only because it carries the runner's
    /// error type rather than the planner's.
    #[error("model {model} failed to lower its sealed support authority")]
    Lowering {
        model: ModelId,
        #[source]
        source: plinth_support::SupportRunError,
    },
    /// The pre-write physics gates refused this model.
    ///
    /// This is the only producer of [`PipelineExitCode::Gates`], and it is
    /// always returned *before* `plinth_emit::write` is reached, so a model
    /// that lands here has no artifact on disk. The refusal carries the gate
    /// and, for a blocking verdict, the full `CoverageReport` behind it.
    #[error("model {model} was refused by the pre-write physics gates")]
    Gates {
        model: ModelId,
        #[source]
        source: GateRefusal,
    },
    #[error("model {model} failed final ChituBox emission")]
    Emit {
        model: ModelId,
        #[source]
        source: EmitError,
    },
}

impl PipelineError {
    #[must_use]
    pub const fn exit_code(&self) -> PipelineExitCode {
        match self {
            Self::InputFolder { .. }
            | Self::OutputFolder { .. }
            | Self::ResourcePolicy { .. }
            | Self::Load { .. }
            | Self::ScaleSuspicious { .. }
            | Self::OrientationBaselineMissing { .. }
            | Self::DegenerateExtent { .. }
            | Self::Decimate { .. }
            | Self::Orient { .. } => PipelineExitCode::Input,
            Self::Delicacy { .. } | Self::LayerPlan { .. } => PipelineExitCode::Supports,
            Self::Hollow { .. } => PipelineExitCode::Hollow,
            Self::Gates { .. } => PipelineExitCode::Gates,
            Self::Support { .. } | Self::SealedRun { .. } | Self::Lowering { .. } => {
                PipelineExitCode::Supports
            }
            // A stale artifact that will not clear is a failure of this model's
            // emission path: the one place the run owns the target file. FU-57's
            // coverage evidence lives beside the same target and is cleared under
            // the same discipline, so it projects to the same code.
            Self::Emit { .. } | Self::StaleArtifact { .. } | Self::StaleCoverageEvidence { .. } => {
                PipelineExitCode::Emit
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_core::{m0_constants, Finite};
    use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
    use plinth_hollow::{
        drains::{CandidateDiagnostic, CandidateUnavailable, DrainPlanningRefusal, HoleRole},
        drilling::DrillingRefusal,
        HollowDisposition, HollowFailureCategory, HollowStageOutput, InternalSupportRefusalReason,
        SolidDecisionReason,
    };
    use plinth_mesh::{load_binary_stl, make_box, Mesh, WeldPolicy};
    use plinth_orient::{run as run_orientation, OrientRequest, PinnedBaseline};

    /// The recorded M8 default, as a fact about the type rather than a comment.
    ///
    /// This is the standing product defect closing: the previous default named
    /// a search that did not exist, so `plinth prepare` refused every model it
    /// was ever handed. Nothing selects `legacy_coarse` any more because the
    /// variant is gone; a persisted policy still carrying that tag now fails to
    /// deserialize by name, which is the intended loud failure.
    #[test]
    fn arbitrary_input_orientation_defaults_to_the_dense_sweep() {
        assert_eq!(OrientationPolicy::default(), OrientationPolicy::DenseSweep);
        assert_eq!(
            serde_json::to_string(&OrientationPolicy::default()).expect("policy serializes"),
            r#"{"kind":"dense_sweep"}"#
        );
        assert!(
            serde_json::from_str::<OrientationPolicy>(r#"{"kind":"legacy_coarse"}"#).is_err(),
            "the deleted policy must not silently deserialize into anything"
        );
    }

    #[test]
    fn pinned_orientation_retains_the_existing_typed_baseline_authority() {
        let policy = OrientationPolicy::pinned_for(
            "Sphere",
            PinnedBaseline {
                pipeline_report: PathBuf::from("baseline/Sphere.pipeline.json"),
            },
        );
        let wire = serde_json::to_string(&policy).expect("policy serializes");
        assert_eq!(
            wire,
            r#"{"kind":"pinned","baselines":{"Sphere":{"pipeline_report":"baseline/Sphere.pipeline.json"}}}"#
        );
        assert_eq!(
            serde_json::from_str::<OrientationPolicy>(&wire).expect("policy deserializes"),
            policy
        );
    }

    /// A pinned policy answers **by name**, and refuses to answer for a model
    /// it does not name.
    ///
    /// This is the whole defence against a heterogeneous folder receiving one
    /// model's rotation for every model: `plinth_orient::run` applies whatever
    /// baseline it is handed to whatever mesh it is handed, and a misoriented
    /// model seals, gate-passes and emits like any other, so the binding has to
    /// happen here or nowhere.
    #[test]
    fn a_pinned_policy_resolves_per_model_and_lends_no_model_anothers_baseline() {
        let baseline = |name: &str| PinnedBaseline {
            pipeline_report: PathBuf::from(format!("baseline/{name}.pipeline.json")),
        };
        let policy = OrientationPolicy::Pinned(BTreeMap::from([
            ("mini-a".to_owned(), baseline("mini-a")),
            ("mini-b".to_owned(), baseline("mini-b")),
        ]));
        assert_eq!(policy.baseline_for("mini-a"), Some(&baseline("mini-a")));
        assert_eq!(policy.baseline_for("mini-b"), Some(&baseline("mini-b")));
        assert_eq!(
            policy.baseline_for("mini-c"),
            None,
            "an unnamed model must get no baseline at all, never a neighbour's"
        );
        assert_eq!(OrientationPolicy::DenseSweep.baseline_for("mini-a"), None);
    }

    #[test]
    fn output_profile_accepts_only_the_exact_saturn_cli_spelling() {
        assert_eq!(
            OutputProfile::try_from("saturn4-ultra").expect("ratified profile"),
            OutputProfile::Saturn4Ultra
        );
        for value in ["Saturn4-Ultra", "saturn4_ultra", "saturn4-ultra "] {
            assert_eq!(
                OutputProfile::try_from(value),
                Err(OutputProfileError::Unknown {
                    value: value.to_owned(),
                })
            );
        }
    }

    #[test]
    fn output_profile_resolves_the_ratified_build_volume_authority() {
        let profile = OutputProfile::Saturn4Ultra
            .resolve()
            .expect("M0 profile authority");
        assert_eq!(profile.id().as_str(), "saturn4-ultra");
        assert_eq!(
            profile.build_volume_mm(),
            m0_constants()
                .expect("M0 constants")
                .saturn4_ultra()
                .build_volume_mm()
        );
        assert_eq!(
            serde_json::to_string(&OutputProfile::Saturn4Ultra).expect("profile serializes"),
            r#""saturn4_ultra""#
        );
    }

    fn finite_decimation_budget() -> DecimationBudget {
        DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
            .expect("finite explicit decimation budget")
    }

    #[test]
    fn saturn_resource_policy_assembles_only_stage_owned_production_limits() {
        let policy = PrepareResourcePolicy::for_saturn4_ultra(finite_decimation_budget())
            .expect("production resource policy");
        assert_eq!(policy.hollow(), H0AdmissionLimits::production());
        assert_eq!(policy.drilling(), DrillingLimits::production());
        assert_eq!(policy.decimation(), finite_decimation_budget());

        let layers = plinth_core::LayerPlan::new(0.0, vec![0.05]).expect("layer plan");
        // An empty delicacy sidecar is a zero-face model, so the two
        // model-scaled support caps come out at exactly their floors.
        let input = policy.support_input(&layers, &[], &[]);
        assert_eq!(input.max_grid_bytes(), 64 * 1024 * 1024);
        assert_eq!(input.minima_knn_max_work(), 100_000_000);
        assert_eq!(input.max_primitives(), 200_000);
        assert_eq!(input.max_primitive_bytes(), 128 * 1024 * 1024);

        // The pipeline must carry the scaling through, not flatten it: the same
        // policy bound to a 429,120-face model yields the scaled caps.
        let delicacy = vec![0.0_f64; 429_120];
        let scaled = policy.support_input(&layers, &delicacy, &[]);
        assert_eq!(scaled.max_grid_bytes(), 512 * 429_120);
        assert_eq!(scaled.minima_knn_max_work(), 2_048 * 429_120);
    }

    #[test]
    fn resource_policy_refuses_unbounded_decimation_before_stage_execution() {
        let budget = DecimationBudget::ratified_default(ProbeConfig::new(1.0, usize::MAX, 100.0))
            .expect("registered pixel pitch");
        assert!(matches!(
            PrepareResourcePolicy::for_saturn4_ultra(budget),
            Err(PrepareResourcePolicyError::Decimation(
                BudgetConfigError::InvalidGridBytes { value: usize::MAX }
            ))
        ));
    }

    #[test]
    fn resource_policy_is_thread_setting_inert_at_one_four_and_sixteen() {
        let policies = [1, 4, 16].map(|_| {
            PrepareResourcePolicy::for_saturn4_ultra(finite_decimation_budget())
                .expect("production resource policy")
        });
        assert!(policies.windows(2).all(|pair| {
            pair[0].decimation() == pair[1].decimation()
                && pair[0].hollow() == pair[1].hollow()
                && pair[0].drilling() == pair[1].drilling()
        }));
    }

    fn baseline(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/baseline")
            .join(name)
    }

    /// A model oriented by **its own** pinned baseline.
    ///
    /// Which baseline is not a detail: hollowing and drain drilling are
    /// rotation-dependent (an axis-aligned voxel grid and build-frame drains),
    /// so a fixture oriented by another model's rotation produces a prepared
    /// mesh that is neither the sealed fixture's nor what production makes. Any
    /// assertion about "the prepared mesh" of a fixture is about a different
    /// solid if this argument is wrong.
    fn oriented_by(mesh: &Mesh, baseline_report: &str) -> OrientedModel {
        let budget =
            DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
                .expect("registered transition-test budget");
        let decimated = run_decimation(mesh, budget)
            .expect("transition-test decimation")
            .into_decimated_model()
            .expect("transition-test mesh is in contract");
        run_orientation(
            decimated,
            &OrientRequest {
                pinned: PinnedBaseline {
                    pipeline_report: baseline(baseline_report),
                },
            },
        )
        .expect("strict pinned orientation")
    }

    /// The Sphere-baseline shorthand the hollow-stage transition tests use.
    ///
    /// Those tests are about the hollow stage's *dispositions* on a 20 mm ball,
    /// not about reproducing a sealed fixture, so one pinned rotation is the
    /// point rather than a substitution.
    fn oriented(mesh: Mesh) -> OrientedModel {
        oriented_by(&mesh, "Sphere.pipeline.json")
    }

    fn hollow_request() -> HollowRequest {
        HollowRequest::with_registered_wall_default(
            m0_constants()
                .expect("registered constants")
                .saturn4_ultra()
                .clone(),
        )
        .expect("registered wall default")
    }

    /// The wall the caller states must reach the geometry, not merely be
    /// carried. The Sphere fixture is about 20 mm across, so a 40 mm wall
    /// leaves no voxel further than the wall from the surface: the stage must
    /// decide `Solid { NoCavity }` where the same model at the registered
    /// default hollows and drills. Only a request whose wall reaches the
    /// extraction can produce that difference.
    #[test]
    fn hollow_stage_hollows_against_the_wall_the_request_states() {
        let sphere = load_binary_stl(baseline("Sphere.prepped.stl"), WeldPolicy::None)
            .expect("committed Sphere fixture");
        let thick = HollowRequest {
            wall_mm: plinth_core::PositiveFinite::new(40.0).expect("positive finite wall"),
            ..hollow_request()
        };
        let output = hollow_stage(oriented(sphere.clone()), &thick)
            .expect("an ineligible model is a decision, not an error");
        assert_eq!(
            project_hollow_stage(&output).disposition,
            HollowDisposition::Solid {
                reason: SolidDecisionReason::NoCavity
            }
        );
        assert!(matches!(
            hollow_stage(oriented(sphere), &hollow_request()).expect("registered default wall"),
            HollowStageOutput::AwaitingInternalSupport(_)
        ));
    }

    #[test]
    fn hollow_stage_preserves_history_and_only_hands_off_proved_drilled_mesh() {
        let sphere = load_binary_stl(baseline("Sphere.prepped.stl"), WeldPolicy::None)
            .expect("committed Sphere fixture");
        let oriented = oriented(sphere);
        let reserve_allocation = oriented.mesh().verts().as_ptr();
        let expected_reserve_mesh = oriented.mesh().clone();
        let expected_history = oriented.history().clone();
        let output = hollow_stage(oriented, &hollow_request()).expect("Sphere hollow transition");
        let projection = project_hollow_stage(&output);
        assert!(projection.awaiting_internal_support);
        assert!(projection.oriented_solid_reserve_retained);
        let HollowStageOutput::AwaitingInternalSupport(candidate) = output else {
            panic!("eligible Sphere must await A5")
        };
        assert_eq!(
            candidate.reserve_mesh().verts().as_ptr(),
            reserve_allocation
        );
        assert_eq!(candidate.reserve_mesh(), &expected_reserve_mesh);
        assert_eq!(candidate.reserve_history(), &expected_history);
        let evidence = candidate
            .hollowed()
            .history()
            .hollow()
            .drilling_evidence()
            .expect("complete drilling proof retained");
        assert!(evidence
            .holes()
            .iter()
            .any(|hole| hole.role() == plinth_hollow::drains::HoleRole::Vent));
        assert_eq!(evidence.cavity_voxels(), evidence.drainable_voxels());
        let model = (*candidate).approve_hollow();

        assert_eq!(
            model.history().hollow().disposition(),
            HollowDisposition::HollowedAndDrilled
        );
        assert!(!model.history().hollow().cavities().is_empty());
        assert!(!model.history().hollow().drains().is_empty());
        assert_eq!(model.history().orientation(), &expected_history);
        assert_eq!(
            model.history().orientation().decimation().trace(),
            expected_history.decimation().trace()
        );
    }

    #[test]
    fn hollow_stage_records_legitimate_solid_decision_without_calling_it_a_fallback() {
        let oriented = oriented(make_box([20.0, 8.0, 8.0], [0.0; 3]).expect("thin closed box"));
        let allocation = oriented.mesh().verts().as_ptr();
        let expected_mesh = oriented.mesh().clone();
        let expected_history = oriented.history().clone();
        let output = hollow_stage(oriented, &hollow_request()).expect("solid decision");
        let HollowStageOutput::Solid(model) = output else {
            panic!("below-threshold box must remain solid")
        };

        assert_eq!(model.mesh().verts().as_ptr(), allocation);
        assert_eq!(model.mesh(), &expected_mesh);
        assert_eq!(model.history().orientation(), &expected_history);
        assert_eq!(
            model.history().hollow().disposition(),
            HollowDisposition::Solid {
                reason: SolidDecisionReason::BelowThreshold,
            }
        );
        assert!(model.history().hollow().cavities().is_empty());
        assert!(model.history().hollow().drains().is_empty());
    }

    #[test]
    fn successful_hollow_reserve_is_consumed_once_by_exact_a5_refusal() {
        let sphere = load_binary_stl(baseline("Sphere.prepped.stl"), WeldPolicy::None)
            .expect("committed Sphere fixture");
        let oriented = oriented(sphere);
        let allocation = oriented.mesh().verts().as_ptr();
        let expected_mesh = oriented.mesh().clone();
        let expected_history = oriented.history().clone();
        let output = hollow_stage(oriented, &hollow_request()).expect("Sphere hollow transition");
        let HollowStageOutput::AwaitingInternalSupport(candidate) = output else {
            panic!("eligible Sphere must retain the A5 reserve")
        };
        let model = (*candidate)
            .into_solid_fallback(InternalSupportRefusalReason::A5GateFailure)
            .expect("authorized A5 fallback");
        assert_eq!(model.mesh().verts().as_ptr(), allocation);
        assert_eq!(model.mesh(), &expected_mesh);
        assert_eq!(model.history().orientation(), &expected_history);
        assert_eq!(
            model.history().hollow().disposition(),
            HollowDisposition::SolidFallback {
                reason: InternalSupportRefusalReason::A5GateFailure,
            }
        );
    }

    #[test]
    fn operational_hollow_failures_project_to_frozen_hollow_exit() {
        let cases = [
            HollowError::DrainPlanning(DrainPlanningRefusal::Constants),
            HollowError::DrainPlanning(DrainPlanningRefusal::Kernel),
            HollowError::DrainPlanning(DrainPlanningRefusal::NoExit {
                lobe_id: 7,
                diagnostics: Vec::new(),
            }),
            HollowError::DrainPlanning(DrainPlanningRefusal::NoVentSite {
                diagnostics: Vec::new(),
            }),
            HollowError::DrainPlanning(DrainPlanningRefusal::ResourceAccountingOverflow),
            HollowError::Drilling(DrillingRefusal::Boolean),
            HollowError::Drilling(DrillingRefusal::InvalidDrilledTopology),
            HollowError::Drilling(DrillingRefusal::BoundsChanged),
            HollowError::Drilling(DrillingRefusal::VolumeBalance {
                error_mm3: 2.0,
                tolerance_mm3: 1.0,
            }),
            HollowError::Drilling(DrillingRefusal::BlockedOpening { hole: 0 }),
            HollowError::Drilling(DrillingRefusal::TrappedCavity { trapped_voxels: 1 }),
            HollowError::Drilling(DrillingRefusal::MissingRequiredRole),
        ];
        for source in cases {
            assert!(matches!(
                source.category(),
                HollowFailureCategory::DrainPlanning | HollowFailureCategory::Drilling
            ));
            let error = PipelineError::Hollow {
                model: ModelId::new("fixture"),
                source,
            };
            assert_eq!(error.exit_code(), PipelineExitCode::Hollow);
        }
    }

    #[test]
    fn hollow_request_rejects_any_profile_other_than_the_registered_authority() {
        let mut wire = serde_json::to_value(
            m0_constants()
                .expect("registered constants")
                .saturn4_ultra(),
        )
        .expect("profile serializes");
        wire["id"] = serde_json::Value::String("unregistered-profile".to_owned());
        let printer = serde_json::from_value(wire).expect("alternate profile remains well-formed");
        let request = HollowRequest {
            printer,
            ..hollow_request()
        };
        let model = oriented(make_box([20.0, 8.0, 8.0], [0.0; 3]).expect("thin closed box"));
        let failure = hollow_stage(model, &request).expect_err("profile mismatch must refuse");
        assert_eq!(
            failure.error().category(),
            HollowFailureCategory::PrinterProfile
        );
        // A refusal raised before cavity selection makes no repair claim: there
        // is nothing to report, as opposed to something that was dropped.
        assert_eq!(failure.cavity_repair(), None);
        let wrapped = PipelineError::Hollow {
            model: ModelId::new("fixture"),
            source: failure.into_error(),
        };
        assert_eq!(wrapped.exit_code(), PipelineExitCode::Hollow);
    }

    #[test]
    fn drain_refusal_diagnostics_survive_pipeline_error_projection() {
        let diagnostic = CandidateDiagnostic {
            role: HoleRole::Vent,
            lobe_id: None,
            site: 12,
            direction: [
                Finite::new(1.0).unwrap(),
                Finite::new(0.0).unwrap(),
                Finite::new(0.0).unwrap(),
            ],
            reason: CandidateUnavailable::CrossingCount { crossings: 3 },
        };
        let error = PipelineError::Hollow {
            model: ModelId::new("fixture"),
            source: HollowError::DrainPlanning(DrainPlanningRefusal::NoVentSite {
                diagnostics: vec![diagnostic.clone()],
            }),
        };
        assert_eq!(error.exit_code(), PipelineExitCode::Hollow);
        let PipelineError::Hollow {
            source: HollowError::DrainPlanning(DrainPlanningRefusal::NoVentSite { diagnostics }),
            ..
        } = error
        else {
            panic!("typed diagnostic was collapsed")
        };
        assert_eq!(diagnostics, vec![diagnostic]);
    }

    fn size(longest_axis_mm: f64) -> Option<AssumedPrintSize> {
        Some(AssumedPrintSize {
            longest_axis_mm,
            dimensions_mm: [longest_axis_mm, 1.0, 1.0],
        })
    }

    #[test]
    fn report_distinguishes_one_artifact_from_an_explicit_stage_stop() {
        let report = PrepareReport {
            no_cache: true,
            models: vec![
                ModelPreparationReport {
                    model: ModelId::new("mini-a"),
                    source_path: PathBuf::from("models/mini-a.stl"),
                    assumed_print_size: size(32.0),
                    repair: None,
                    outcome: ModelPreparationOutcome::Artifact {
                        emit: EmitReport {
                            artifact_path: PathBuf::from("out/mini-a.chitubox"),
                            support_record_count: 4,
                            artifact_bytes: 2048,
                        },
                        geometry: SupportedGeometry::DrilledShellSealed,
                        hollow_disposition: HollowDisposition::HollowedAndDrilled,
                        cavity_repair: None,
                        wall_mm: None,
                        realized_wall: None,
                        savings_shortfall: None,
                        filled_back_chambers_ml: Vec::new(),
                        ownerless_islands: Vec::new(),
                        contacts: Box::default(),
                        repair_report: None,
                        repair_ledger: None,
                    },
                },
                ModelPreparationReport {
                    model: ModelId::new("mini-b"),
                    source_path: PathBuf::from("models/mini-b.stl"),
                    assumed_print_size: size(28.0),
                    repair: None,
                    outcome: ModelPreparationOutcome::Stopped {
                        after: StageStop::Hollow,
                    },
                },
            ],
        };
        assert_eq!(
            report.artifacts().collect::<Vec<_>>(),
            vec![Path::new("out/mini-a.chitubox")]
        );
        let wire = serde_json::to_value(&report).expect("prepare report serializes");
        assert_eq!(wire["no_cache"], true);
        assert_eq!(wire["models"][0]["outcome"]["kind"], "artifact");
        assert_eq!(
            wire["models"][0]["assumed_print_size"]["longest_axis_mm"],
            32.0
        );
        assert_eq!(wire["models"][1]["outcome"]["after"], "hollow");
        // Neither an artifact nor an explicit stop is a nonzero exit.
        assert_eq!(report.exit_code(), PipelineExitCode::Ok);
    }

    /// A refusal and a failure are different dispositions with different exit
    /// codes, and the run's code is the first non-Ok one in model order.
    #[test]
    fn a_gate_refusal_reports_gates_and_a_stage_failure_reports_its_own_stage() {
        // SPEC-A4 §A.8's third destination on the refused path. The gate
        // refused a model the repair controller had already spent 45 rolled-
        // back transactions on, and the run report has to say so: this is the
        // path ruling R10 deleted the failed-tries cap on.
        let spent = plinth_support::RepairRunReport {
            trials: 45,
            commits: 0,
            ..plinth_support::RepairRunReport::default()
        };
        let refused = ModelPreparationOutcome::Refused {
            gate: Some(PreWriteGate::UncoveredIslands),
            geometry: Some(SupportedGeometry::DrilledShellSealed),
            violation: "nine uncovered islands".to_owned(),
            evidence: None,
            repair_report: Some(Box::new(spent.clone())),
        };
        assert_eq!(refused.exit_code(), PipelineExitCode::Gates);
        assert_eq!(refused.repair_report(), Some(&spent));
        // A model that never reached a sealed run carries no ledger, and that
        // is a measurement rather than a gap — "the loop never ran" is not
        // "the loop ran and spent nothing".
        let failed = ModelPreparationOutcome::Failed {
            exit_code: PipelineExitCode::Input,
            detail: "unreadable".to_owned(),
            evidence: None,
            repair_report: None,
        };
        assert_eq!(failed.exit_code(), PipelineExitCode::Input);
        assert_eq!(failed.repair_report(), None);

        let model = |name: &str, outcome| ModelPreparationReport {
            model: ModelId::new(name),
            source_path: PathBuf::from(format!("models/{name}.stl")),
            assumed_print_size: size(30.0),
            repair: None,
            outcome,
        };
        let report = PrepareReport {
            no_cache: false,
            models: vec![
                model(
                    "emitted",
                    ModelPreparationOutcome::Artifact {
                        emit: EmitReport {
                            artifact_path: PathBuf::from("out/emitted.chitubox"),
                            support_record_count: 1,
                            artifact_bytes: 1,
                        },
                        geometry: SupportedGeometry::DrilledShellSealed,
                        hollow_disposition: HollowDisposition::HollowedAndDrilled,
                        cavity_repair: None,
                        wall_mm: None,
                        realized_wall: None,
                        savings_shortfall: None,
                        filled_back_chambers_ml: Vec::new(),
                        ownerless_islands: Vec::new(),
                        contacts: Box::default(),
                        repair_report: None,
                        repair_ledger: None,
                    },
                ),
                model("refused", refused),
                model("failed", failed),
            ],
        };
        assert_eq!(report.exit_code(), PipelineExitCode::Gates);
        assert_eq!(
            report.artifacts().collect::<Vec<_>>(),
            vec![Path::new("out/emitted.chitubox")],
            "a refused or failed model must contribute no artifact"
        );

        let wire = serde_json::to_value(&report).expect("prepare report serializes");
        assert_eq!(wire["models"][1]["outcome"]["kind"], "refused");
        assert_eq!(wire["models"][1]["outcome"]["gate"], "uncovered_islands");
        assert_eq!(
            wire["models"][1]["outcome"]["geometry"]["kind"],
            "drilled_shell_sealed"
        );
        // §A.8's third destination has to survive the wire, not just the
        // struct: the run report on disk is the machine-readable half of the
        // owner's summary, and a refused model's spend is the figure two runs
        // get compared on.
        assert_eq!(wire["models"][1]["outcome"]["repair_report"]["trials"], 45);
        assert_eq!(wire["models"][1]["outcome"]["repair_report"]["commits"], 0);
        assert_eq!(wire["models"][2]["outcome"]["exit_code"], "input");
        assert!(
            wire["models"][2]["outcome"]["repair_report"].is_null(),
            "a model that never reached a sealed run must not claim a repair ledger"
        );
        assert_eq!(
            wire["models"][0]["outcome"]["geometry"]["kind"], "drilled_shell_sealed",
            "a successful artifact must carry the geometry it was built from"
        );
        assert_eq!(
            wire["models"][0]["outcome"]["hollow_disposition"]["kind"],
            "hollowed_and_drilled"
        );
    }

    /// **A gate refusal cannot be masked by an earlier, unrelated failure.**
    ///
    /// The named case: a macOS `._sidecar` sorts before every real model and
    /// fails to load, so a first-in-order exit code would report `1` for a
    /// folder in which the physics gates refused a mini. Exit `3` is the only
    /// machine-observable safety signal a calling script has, and it is the one
    /// code with a documented meaning callers act on, so it is sticky.
    #[test]
    fn a_gate_refusal_anywhere_in_a_run_outranks_an_earlier_unrelated_failure() {
        let model = |name: &str, outcome| ModelPreparationReport {
            model: ModelId::new(name),
            source_path: PathBuf::from(format!("models/{name}.stl")),
            assumed_print_size: None,
            repair: None,
            outcome,
        };
        let sidecar = model(
            "._mini",
            ModelPreparationOutcome::Failed {
                exit_code: PipelineExitCode::Input,
                detail: "not a binary STL".to_owned(),
                evidence: None,
                repair_report: None,
            },
        );
        let refused = model(
            "mini",
            ModelPreparationOutcome::Refused {
                gate: Some(PreWriteGate::UncoveredIslands),
                geometry: Some(SupportedGeometry::DrilledShellSealed),
                violation: "nine uncovered islands".to_owned(),
                evidence: None,
                repair_report: None,
            },
        );

        let report = PrepareReport {
            no_cache: false,
            models: vec![sidecar.clone(), refused.clone()],
        };
        assert_eq!(
            report.exit_code(),
            PipelineExitCode::Gates,
            "the refusal sorts second and was masked by the sidecar's exit 1"
        );
        assert!(report.has_gate_refusal());

        // Order-independent, which is the point: the same run enumerated the
        // other way round must report the same verdict.
        let reversed = PrepareReport {
            no_cache: false,
            models: vec![refused, sidecar.clone()],
        };
        assert_eq!(reversed.exit_code(), PipelineExitCode::Gates);

        // And with no refusal in it, the run is first-in-order as before --
        // `Gates` is the only sticky code, not a new severity ranking.
        let no_refusal = PrepareReport {
            no_cache: false,
            models: vec![
                sidecar,
                model(
                    "other",
                    ModelPreparationOutcome::Failed {
                        exit_code: PipelineExitCode::Supports,
                        detail: "sealed run refused".to_owned(),
                        evidence: None,
                        repair_report: None,
                    },
                ),
            ],
        };
        assert_eq!(no_refusal.exit_code(), PipelineExitCode::Input);
        assert!(!no_refusal.has_gate_refusal());
    }

    /// Every new stop this executor can produce lands on the frozen exit-code
    /// projection, not on a new one.
    #[test]
    fn every_prepare_stage_error_projects_onto_the_frozen_exit_codes() {
        let model = || ModelId::new("fixture");
        let cases: Vec<(PipelineError, PipelineExitCode)> = vec![
            (
                PipelineError::OutputFolder {
                    path: PathBuf::from("out"),
                    source: std::io::Error::other("denied"),
                },
                PipelineExitCode::Input,
            ),
            (
                PipelineError::ScaleSuspicious {
                    model: model(),
                    longest_axis_mm: 0.4,
                },
                PipelineExitCode::Input,
            ),
            (
                // The dense sweep's own refusal, which replaced the deleted
                // unported-policy refusal on this same exit code.
                PipelineError::Orient {
                    model: model(),
                    source: plinth_orient::OrientError::Search(
                        plinth_orient::SearchError::NoCandidateSurvived {
                            tier: plinth_orient::search::SearchTier::Commit,
                            first: "fixture".to_owned(),
                        },
                    ),
                },
                PipelineExitCode::Input,
            ),
            (
                PipelineError::OrientationBaselineMissing { model: model() },
                PipelineExitCode::Input,
            ),
            (
                PipelineError::StaleArtifact {
                    model: model(),
                    path: PathBuf::from("out/fixture.chitubox"),
                    source: std::io::Error::other("denied"),
                },
                PipelineExitCode::Emit,
            ),
            (
                PipelineError::StaleCoverageEvidence {
                    model: model(),
                    path: PathBuf::from("out/fixture.coverage.json"),
                    source: std::io::Error::other("denied"),
                },
                PipelineExitCode::Emit,
            ),
            (
                PipelineError::DegenerateExtent { model: model() },
                PipelineExitCode::Input,
            ),
            (
                PipelineError::SealedRun {
                    model: model(),
                    source: plinth_support::SupportRunError::InvalidResourceCaps,
                },
                PipelineExitCode::Supports,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(error.exit_code(), expected, "{error}");
        }
    }

    /// The one policy assembly reads the registry rather than restating it, and
    /// reads the *same* numbers every hand-written call site in this workspace
    /// already passed — so publishing them moved no decimation digest.
    #[test]
    fn the_registered_production_budget_is_the_budget_every_gate_already_used() {
        let registered = registered_decimation_budget().expect("registered production budget");
        assert_eq!(registered, finite_decimation_budget());
        assert_eq!(registered.probe.surface.grid_pitch_mm, 1.0);
        assert_eq!(registered.probe.surface.max_grid_bytes, 64 * 1024 * 1024);
        assert_eq!(registered.probe.nearest_upper_bound_mm, 10_000.0);
    }

    /// The lift the whole run plans and emits at is the profile's registered
    /// one, not a literal restated at the CLI boundary.
    #[test]
    fn the_run_lift_is_the_profiles_own_registered_placeholder() {
        let printer = OutputProfile::Saturn4Ultra
            .resolve()
            .expect("M0 profile authority");
        let request = registered_support_request(printer).expect("registered lift");
        assert_eq!(request.lift_mm.get(), 5.0);
        assert_eq!(
            request.lift_mm.get(),
            *printer.lift_default_mm().diagnostic()
        );
    }

    /// The prepared mesh that reaches `seal_and_emit`, for one fixture, oriented
    /// by that fixture's **own** pinned baseline.
    fn prepared_mesh(mesh: &Mesh, baseline_report: &str) -> Mesh {
        prepared_hollow(mesh, baseline_report).mesh().clone()
    }

    /// The hollowed model that reaches the support stage, for one fixture,
    /// oriented by that fixture's own pinned baseline.
    fn prepared_hollow(mesh: &Mesh, baseline_report: &str) -> plinth_hollow::HollowedModel {
        match hollow_stage(oriented_by(mesh, baseline_report), &hollow_request())
            .expect("hollow stage")
        {
            HollowStageOutput::AwaitingInternalSupport(candidate) => candidate.approve_hollow(),
            HollowStageOutput::Solid(model) => *model,
        }
    }

    /// The real delicacy field is not a zero field on production geometry.
    ///
    /// This is the defect the 2026-08-10 owner ruling closed, asserted as a
    /// property rather than left to inspection: `prepare` fed
    /// `vec![0.0; prepared_faces]` to the support stage, so
    /// `strut_model_anchor_max_delicacy` could never be reached and the
    /// delicate-face protection was structurally dead. A field that measured
    /// zero everywhere would pass every other test in this workspace and
    /// silently restore exactly that state.
    #[test]
    fn the_prepared_delicacy_field_measures_real_geometry() {
        let fin = prepared_mesh(
            &plinth_mesh::make_thin_fin().expect("procedural thin fin"),
            "make_thin_fin.pipeline.json",
        );
        let field = prepare::prepared_delicacy_field(&fin).expect("thin fin delicacy");
        assert_eq!(field.face_values().len(), fin.tris().len());
        assert_eq!(field.face_table().lod(), prepare::PREPARED_MESH_LOD);
        let peak = field
            .face_values()
            .iter()
            .map(|value| value.weight())
            .fold(0.0_f64, f64::max);
        assert!(
            peak > 0.0,
            "the 0.6 mm thin fin measured no delicacy anywhere: the producer is a zero field again"
        );
    }

    /// The three fixtures the committed prepare path seals, each with the
    /// pinned baseline that is **its own**.
    fn committed_prepare_fixtures() -> [(&'static str, Mesh, &'static str); 3] {
        [
            (
                "make_toed_slab.prepped.stl",
                load_binary_stl(baseline("make_toed_slab.prepped.stl"), WeldPolicy::None)
                    .expect("committed toed slab"),
                "make_toed_slab.pipeline.json",
            ),
            (
                "Sphere.prepped.stl",
                load_binary_stl(baseline("Sphere.prepped.stl"), WeldPolicy::None)
                    .expect("committed sphere"),
                "Sphere.pipeline.json",
            ),
            (
                "make_overhang_table (procedural)",
                plinth_mesh::make_overhang_table().expect("procedural overhang table"),
                "make_overhang_table.pipeline.json",
            ),
        ]
    }

    /// Why closing the zero-field defect moved no committed expectation, part
    /// one: no fixture reaches the anchor cap.
    ///
    /// `strut_model_anchor_max_delicacy` (0.5) rejects an anchor candidate whose
    /// face is *more* delicate than the cap. Every model the committed prepare
    /// fixtures seal stays under it on every face, so no anchor candidate that
    /// was admissible under the old zero field is inadmissible under the real
    /// one.
    ///
    /// **Each fixture is oriented by its own pinned baseline**, and that is
    /// load-bearing rather than tidy: hollowing and drain drilling are
    /// rotation-dependent, so measuring the toed slab's delicacy on a mesh
    /// hollowed under the Sphere's rotation is measuring a solid the pipeline
    /// never makes. This guard used to do exactly that for two of its three
    /// fixtures, and could have stayed green while covered geometry crossed the
    /// cap.
    ///
    /// **This is not the whole neutrality argument**, and it never was — see
    /// `closing_the_zero_field_defect_moves_no_sealed_support_digest` for the
    /// contact-selection tie-break, which real delicacy values can move without
    /// any face reaching the cap.
    ///
    /// This is a statement about these fixtures, not about all geometry. The
    /// Weeping Treant carries 13.6 % of its faces above the cap and is
    /// deliberately not claimed here — it is not one of the models these
    /// fixtures seal.
    #[test]
    fn the_committed_prepare_fixtures_stay_under_the_anchor_delicacy_cap() {
        let cap = m0_constants()
            .expect("registered constants")
            .placeholders()
            .strut_model_anchor_max_delicacy()
            .diagnostic()
            .get();
        assert_eq!(cap, 0.5);

        for (name, mesh, baseline_report) in committed_prepare_fixtures() {
            let prepared = prepared_mesh(&mesh, baseline_report);
            let field = prepare::prepared_delicacy_field(&prepared).expect("delicacy field");
            let peak = field
                .face_values()
                .iter()
                .map(|value| value.weight())
                .fold(0.0_f64, f64::max);
            assert!(
                peak <= cap,
                "{name} peaks at delicacy {peak}, above the {cap} anchor cap: closing the \
                 zero-field defect is no longer decision-neutral for it, and the artifacts it \
                 produces need the recorded re-pin protocol rather than this assertion"
            );
        }
    }

    /// What closing the zero-field defect actually changed, measured per
    /// fixture instead of argued.
    ///
    /// The anchor cap is **not** the support stage's only delicacy-driven
    /// decision, and the 2026-08-10 decision record claimed it was. Contact
    /// selection sorts each island's eligible candidates by exact Z and then
    /// breaks ties on **delicacy** (`crates/plinth-support/src/lib.rs:1281`,
    /// "exact geometric ties retain a deterministic least-scar preference").
    /// Exact Z ties are common on a flat underside, so real delicacy values can
    /// reorder contact selection — and with it support geometry and artifact
    /// bytes — without any face coming near 0.5. A cap argument cannot cover
    /// that channel, because the channel never consults the cap.
    ///
    /// So this is the recorded re-pin protocol's own instrument rather than an
    /// argument: each fixture is sealed twice on the same hollowed geometry,
    /// once with the measured field and once with the zero field production used
    /// to feed, and the two outcomes are compared. A refusal is an outcome too —
    /// the Sphere's drilled shell reaches no sealed run under either field, and
    /// production answers that with its reserve solid, so two identical refusals
    /// are two identical decisions.
    ///
    /// **The measured answer is that the divergence is real for two of the
    /// three.** The cap argument held — no fixture crosses 0.5 — and the runs
    /// moved anyway, through the tie-break. That is recorded here as a per-model
    /// verdict rather than re-pinned: no committed expectation broke (every
    /// harness that pins a support digest feeds the zero field itself, so each
    /// remains exactly reproducible by the harness that produced it), but those
    /// pinned digests describe support runs production no longer performs. The
    /// re-pin question is FU-38's and belongs to the owner.
    ///
    /// This guard pins the **verdict**, not the digests: a fixture flipping in
    /// either direction is a change in what the record has to say, and a support
    /// change that moves a digest without changing neutrality is not.
    #[test]
    fn the_zero_field_divergence_is_measured_per_committed_fixture() {
        let printer = OutputProfile::Saturn4Ultra
            .resolve()
            .expect("M0 profile authority");
        let support = registered_support_request(printer).expect("registered lift");
        let limits = SupportPreparationLimits::production().expect("production support limits");

        for (name, mesh, baseline_report) in committed_prepare_fixtures() {
            let prepared = prepared_mesh(&mesh, baseline_report);
            // The production layer plan, derived exactly as `prepare` derives
            // it, so this comparison is about the field and not about a
            // different plan.
            let (min_z, max_z) = prepared
                .verts()
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), vertex| {
                    (lo.min(vertex[2]), hi.max(vertex[2]))
                });
            let pitch = *printer.default_layer_height_mm().value();
            let span = (max_z - min_z) + support.lift_mm.get() + 1.0;
            let layers = printer
                .layer_plan(((span / pitch).ceil() as usize).max(1_000))
                .expect("layer plan");
            let measured = prepare::prepared_delicacy_field(&prepared)
                .expect("delicacy field")
                .face_values()
                .iter()
                .map(|value| value.weight())
                .collect::<Vec<_>>();
            let zeros = vec![0.0_f64; prepared.tris().len()];
            assert!(
                measured.iter().any(|value| *value > 0.0),
                "{name}: a field that is already zeros would make this comparison vacuous"
            );

            // A refusal is an outcome too, and two identical refusals are
            // identical outcomes: the Sphere's drilled shell does not reach a
            // sealed run under either field, and production answers that with
            // its reserve solid rather than with a different support plan.
            let outcome = |field: &[f64]| -> Result<u64, String> {
                let authority = plinth_support::run_support(
                    prepared_hollow(&mesh, baseline_report),
                    support.clone(),
                    limits.input(&layers, field, &[]),
                )
                .map_err(|error| error.to_string())?;
                let snapshot = authority
                    .verification_snapshot()
                    .expect("borrow sealed verification snapshot");
                snapshot.validate_integrity().expect("snapshot integrity");
                Ok(snapshot.digest())
            };

            // Measured 2026-08-10 on this tip. `true` means the measured field
            // and the zero field produce the same sealed outcome.
            let recorded_neutral = match name {
                // Re-recorded 2026-08-10 by the cavity-field exterior fix,
                // true -> false, with its attribution rather than an edited
                // expectation. Until that fix the Sphere's cavity reached to
                // within 0.5 mm of its outer surface at the six points where
                // it is tangent to its own bounding box, and the drilled shell
                // that produced was refused during lowering under *both*
                // fields — two identical refusals, which is what made the
                // fixture read as neutral. With the wall no longer breached
                // the drilled shell **seals** under both fields, and the two
                // sealed digests differ (measured 6219979372923930719 against
                // 1114015346968956405), so the Sphere now says exactly what
                // `make_toed_slab` and `make_overhang_table` already said.
                // This does not change FU-38's substance: the divergence it
                // records is the same contact-selection tie-break, and no
                // pinned digest anywhere is re-frozen by this line.
                //
                // Re-recorded a THIRD time, 2026-08-12, by FU-56 (the
                // coverage-closure controller), false -> true, with this
                // attribution rather than an edited expectation, exactly as
                // this row's own protocol requires. `run_support` is now the
                // coverage-closure controller: it measures ROUTED coverage
                // and, when blocking, appends immutable repair-pool contacts
                // for up to 8 attempts before refusing. Measured on THIS
                // fixture under BOTH fields: the baseline is coverage-blocked
                // (as it always secretly was — nothing checked before FU-56),
                // four repair rounds are appended identically under either
                // field, and by the fifth routing attempt the repair-extended
                // plan is geometry CCO refuses (`SupportRoutingError::Cco(InvalidGeometry)`)
                // — under BOTH fields, at the SAME attempt, with the
                // byte-identical error string "sealed support run failed
                // during Routing". That is a real, deterministic new fact
                // about this fixture, not noise: a highly symmetric sphere's
                // repair-pool nomination converges to the same geometric wall
                // regardless of which field ordered the tie-break, so the two
                // runs now agree for a reason FU-38's original tie-break
                // finding never covered. This is FU-56's territory, not
                // FU-38's substance being edited — the contact-selection
                // tie-break this row records is still real and still moves
                // both digests during the four successful repair rounds; it
                // is simply no longer the LAST word for this one fixture. No
                // pinned digest anywhere is re-frozen by this line, and
                // nothing here touches the coverage radius, the patch-area
                // floor, or the zero-tolerance verdict.
                //
                // Re-recorded a FOURTH time, 2026-08-12, by the FU-56 review
                // fixes, true -> false, with this attribution rather than an
                // edited expectation. Two changes land together and both are
                // measured on this fixture:
                //
                //  1. Nomination now scores candidate repair rows against the
                //     samples the measurement found UNCOVERED, not against
                //     every sample on a blocking patch's faces. On this
                //     fixture that is the difference between repairs doing
                //     nothing and repairs working: the blocking patch count
                //     now measures 174, 174, **109**, 109 across attempts
                //     0..3 under both fields — the first time in this
                //     project's record that an appended repair-pool contact
                //     has been observed to reduce measured uncovered area at
                //     all.
                //  2. A stage failure on a plan this controller had already
                //     extended is now the coverage refusal it actually is
                //     (`CoverageClosureReason::RepairPlanRefused`), not a
                //     bare `Failed(Routing)`. The Sphere's fifth routing
                //     attempt still hits `Cco(InvalidGeometry)` under both
                //     fields, so the shape of the ending is unchanged; what
                //     changed is that it now carries its islands, its
                //     per-attempt history and its evidence.
                //
                // The divergence itself is real and is FU-38's own tie-break,
                // still visible: under the measured field island 1 ends with
                // 477 nominated repair rows and 26 still eligible, under the
                // zero field 478 and 27. The delicacy field orders the
                // candidate pool, so the two runs drain it by one row
                // differently — the same contact-selection tie-break this row
                // has always measured, now observable through the refusal's
                // own per-island evidence rather than through a sealed
                // digest. No pinned digest anywhere is re-frozen by this
                // line, and nothing here touches the coverage radius, the
                // patch-area floor, or the zero-tolerance verdict.
                //
                // Re-recorded a FIFTH time, 2026-08-13, by the owner ruling
                // on the wall target and minimum wall, false -> true, with
                // this attribution rather than an edited expectation.
                //
                // **What changed and what did not.** The Sphere did NOT seal
                // before this ruling and does not seal after it — it has
                // been refusing under BOTH fields since FU-56, and it still
                // is. What the ruling moved is WHICH refusal. Measured on
                // this fixture at `develop` (7f8e4dd) and at this tip, same
                // process, same settings:
                //
                //   develop: measured -> Err("sealed support run failed
                //            during Routing"), zeros -> Err("coverage
                //            closure refused: ... RepairPlanRefused
                //            { phase: Routing, detail: Cco(InvalidGeometry) }
                //            ..."). Two refusals, DIFFERENT text, so the
                //            outcomes compared unequal: divergent.
                //   this tip: both fields -> Err("sealed support run failed
                //            during Lowering"). One refusal, identical text
                //            under both: neutral.
                //
                // The mechanism: `outcome` runs `prepared_hollow`, so it
                // consumes the drilled shell, and zeroing
                // `HOLLOW_CAVITY_MARCH_MARGIN_MM` cuts the Sphere's cavity at
                // the requested 2.5 mm instead of 3.366 mm. That is a
                // different shell — 1,749 mm3 of cavity against 1,221 — and
                // the support chain on it now stops at Lowering, before it
                // ever reaches the coverage-closure repair rounds where
                // FU-38's contact-selection tie-break was the last thing
                // still separating the two fields.
                //
                // **FU-38's finding is not contradicted and is not edited.**
                // The tie-break is still measured, and still divergent, on
                // both fixtures that reach that far — `make_toed_slab` and
                // `make_overhang_table`, whose rows are untouched. What this
                // row now records is that the Sphere stops earlier than the
                // stage the tie-break lives in, so it can no longer witness
                // it. That is the same kind of statement its THIRD re-record
                // made, for a different reason.
                //
                // **The earlier refusal is recorded as an open question, not
                // silently absorbed:** a shell that stops at Lowering where
                // it used to reach Routing is a fact about the support chain
                // on a thinner-walled sphere, and it belongs to whoever owns
                // the Sphere's coverage closure. No pinned digest anywhere is
                // re-frozen by this line, and nothing here touches the
                // coverage radius, the patch-area floor, the zero-tolerance
                // verdict, or any registered constant.
                "Sphere.prepped.stl" => true,
                // Both move, and neither crosses the anchor cap (the guard
                // above proves that): this is the contact-selection tie-break,
                // not the cap.
                "make_toed_slab.prepped.stl" => false,
                // Re-recorded 2026-08-12, by FU-56, false -> true, with this
                // attribution: `make_overhang_table`'s island 0 (component 4,
                // face 51) has no repair-pool candidate the CCO transcript
                // ever admits at the registered coverage radius
                // (`plinth-support/tests/support_runner_integration.rs`,
                // `public_runner_refuses_overhang_table_with_a_genuinely_unclosable_island`),
                // under EITHER field: both runs exhaust the registered
                // 8-attempt bound and refuse
                // `SupportRunError::CoverageClosure(AttemptsExhausted)` with
                // the same one blocking island, because the nomination
                // scoring for this fixture's single blocking key is dominated
                // by geometric distance to the uncovered samples, not by the
                // delicacy tie-break this row otherwise measures. FU-38's
                // tie-break finding is unaffected on every fixture that
                // reaches a sealed run at all; this fixture no longer does.
                // No pinned digest anywhere is re-frozen by this line, and
                // nothing here touches the coverage radius, the patch-area
                // floor, or the zero-tolerance verdict.
                //
                // Re-recorded again, 2026-08-12, by the FU-56 review fixes,
                // true -> false, with this attribution rather than an edited
                // expectation. The refusal above is GONE: with nomination
                // scored against the measurement's uncovered samples instead
                // of against every sample on the blocking patch's faces, this
                // fixture's island 0 closes from its own repair pool and the
                // run seals under both fields
                // (`plinth-support/tests/support_runner_integration.rs`,
                // `public_runner_closes_overhang_table_only_after_appending_repair_contacts`).
                // Two sealed runs mean two sealed digests, and they differ —
                // which is exactly FU-38's original contact-selection
                // tie-break finding for this fixture, restored to visibility
                // rather than newly discovered. No pinned digest anywhere is
                // re-frozen by this line, and nothing here touches the
                // coverage radius, the patch-area floor, or the
                // zero-tolerance verdict.
                "make_overhang_table (procedural)" => false,
                other => panic!("unrecorded fixture {other}"),
            };
            assert_eq!(
                outcome(&measured) == outcome(&zeros),
                recorded_neutral,
                "{name}: the zero-field divergence changed status. The record
                 (decisions/2026-08-10-delicacy-producer.md and FU-38) states this fixture's
                 verdict; a flip means the record is now wrong and the recorded re-pin protocol
                 is owed a fresh from→to attribution rather than an edited expectation here"
            );
        }
    }
}
