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

pub mod gates;
pub mod prepare;

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

use plinth_core::{m0_constants, ModelId, PrinterProfile};
use plinth_decimate::{BudgetConfigError, DecimationBudget, DecimationError};
use plinth_emit::{EmitError, EmitReport};
use plinth_hollow::{
    drilling::DrillingLimits, H0AdmissionLimits, HollowDisposition, HollowError, HollowRequest,
    HollowStageOutput,
};
use plinth_orient::{OrientError, OrientedModel, PinnedBaseline};
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
pub fn hollow_stage(
    model: OrientedModel,
    request: &HollowRequest,
) -> Result<HollowStageOutput, HollowError> {
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
/// Arbitrary input has no frozen baseline report, so the public preparation
/// default is the legacy coarse policy. Its search implementation lands later;
/// this boundary only makes that future work explicit rather than silently
/// requiring a baseline artifact for every input.
///
/// **A pinned baseline belongs to one model, so the policy is a map and not a
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "baselines", rename_all = "snake_case")]
pub enum OrientationPolicy {
    #[default]
    LegacyCoarse,
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
            Self::LegacyCoarse => None,
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
/// required here before any stage can execute. Orientation is intentionally
/// absent: the later LegacyCoarse search must introduce its own typed bound
/// before it is connected to this policy.
#[derive(Clone, Copy, Debug)]
pub struct PrepareResourcePolicy {
    decimation: DecimationBudget,
    hollow: H0AdmissionLimits,
    drilling: DrillingLimits,
    support: SupportPreparationLimits,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    Artifact {
        emit: EmitReport,
        geometry: prepare::SupportedGeometry,
        hollow_disposition: HollowDisposition,
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
    },
    /// The model did not reach the gates at all. `exit_code` is the frozen
    /// per-stage projection of the typed error behind `detail`.
    Failed {
        exit_code: PipelineExitCode,
        detail: String,
    },
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
    #[must_use]
    pub fn exit_code(&self) -> PipelineExitCode {
        let mut first_non_ok = None;
        for model in &self.models {
            let code = model.outcome.exit_code();
            if code == PipelineExitCode::Gates {
                return PipelineExitCode::Gates;
            }
            if code != PipelineExitCode::Ok && first_non_ok.is_none() {
                first_non_ok = Some(code);
            }
        }
        first_non_ok.unwrap_or(PipelineExitCode::Ok)
    }

    /// Whether the pre-write physics gates refused any model in this run.
    #[must_use]
    pub fn has_gate_refusal(&self) -> bool {
        self.models
            .iter()
            .any(|model| matches!(model.outcome, ModelPreparationOutcome::Refused { .. }))
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
    /// The run's orientation policy has no implementation that can orient this
    /// model, and orientation is never inferred.
    ///
    /// `plinth-orient` deliberately does not search: it applies the rotation a
    /// pinned baseline states. [`OrientationPolicy::LegacyCoarse`] names the
    /// native coarse search of `02-target-architecture.md` §6, which is not
    /// ported. Refusing here is the whole point — silently choosing identity
    /// would be inferring an orientation, and identity is a rotation like any
    /// other.
    #[error(
        "model {model} cannot be oriented: the legacy-coarse orientation search is not ported, \
         and orientation is never inferred. Only OrientationPolicy::Pinned can orient a model \
         today, and it needs a baseline pipeline report naming the rotation to apply"
    )]
    OrientationPolicyUnavailable { model: ModelId },
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
    /// The prepared mesh has no finite Z extent, so no layer plan spans it.
    #[error("model {model} has no finite z extent to plan layers over")]
    DegenerateExtent { model: ModelId },
    /// The layer plan support planning plans against could not be built.
    #[error("model {model} could not be given a layer plan for support planning")]
    LayerPlan {
        model: ModelId,
        #[source]
        source: plinth_core::LayerPlanError,
    },
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
            | Self::OrientationPolicyUnavailable { .. }
            | Self::OrientationBaselineMissing { .. }
            | Self::DegenerateExtent { .. }
            | Self::Decimate { .. }
            | Self::Orient { .. } => PipelineExitCode::Input,
            Self::LayerPlan { .. } => PipelineExitCode::Supports,
            Self::Hollow { .. } => PipelineExitCode::Hollow,
            Self::Gates { .. } => PipelineExitCode::Gates,
            Self::Support { .. } | Self::SealedRun { .. } | Self::Lowering { .. } => {
                PipelineExitCode::Supports
            }
            // A stale artifact that will not clear is a failure of this model's
            // emission path: the one place the run owns the target file.
            Self::Emit { .. } | Self::StaleArtifact { .. } => PipelineExitCode::Emit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_core::{m0_constants, Finite};
    use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
    use plinth_hollow::{
        drains::{CandidateDiagnostic, DrainPlanningRefusal, HoleRole},
        drilling::DrillingRefusal,
        HollowDisposition, HollowFailureCategory, HollowStageOutput, InternalSupportRefusalReason,
        SolidDecisionReason,
    };
    use plinth_mesh::{load_binary_stl, make_box, Mesh, WeldPolicy};
    use plinth_orient::{run as run_orientation, OrientRequest, PinnedBaseline};

    #[test]
    fn arbitrary_input_orientation_defaults_to_legacy_coarse() {
        assert_eq!(
            OrientationPolicy::default(),
            OrientationPolicy::LegacyCoarse
        );
        assert_eq!(
            serde_json::to_string(&OrientationPolicy::default()).expect("policy serializes"),
            r#"{"kind":"legacy_coarse"}"#
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
        assert_eq!(OrientationPolicy::LegacyCoarse.baseline_for("mini-a"), None);
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

    fn oriented(mesh: Mesh) -> OrientedModel {
        let budget =
            DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
                .expect("registered transition-test budget");
        let decimated = run_decimation(&mesh, budget)
            .expect("transition-test decimation")
            .into_decimated_model()
            .expect("transition-test mesh is in contract");
        run_orientation(
            decimated,
            &OrientRequest {
                pinned: PinnedBaseline {
                    pipeline_report: baseline("Sphere.pipeline.json"),
                },
            },
        )
        .expect("strict pinned orientation")
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
        let error = hollow_stage(model, &request).expect_err("profile mismatch must refuse");
        assert_eq!(error.category(), HollowFailureCategory::PrinterProfile);
        let wrapped = PipelineError::Hollow {
            model: ModelId::new("fixture"),
            source: error,
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
            crossings: 3,
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
                    outcome: ModelPreparationOutcome::Artifact {
                        emit: EmitReport {
                            artifact_path: PathBuf::from("out/mini-a.chitubox"),
                            support_record_count: 4,
                            artifact_bytes: 2048,
                        },
                        geometry: SupportedGeometry::DrilledShellSealed,
                        hollow_disposition: HollowDisposition::HollowedAndDrilled,
                    },
                },
                ModelPreparationReport {
                    model: ModelId::new("mini-b"),
                    source_path: PathBuf::from("models/mini-b.stl"),
                    assumed_print_size: size(28.0),
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
        let refused = ModelPreparationOutcome::Refused {
            gate: Some(PreWriteGate::UncoveredIslands),
            geometry: Some(SupportedGeometry::DrilledShellSealed),
            violation: "nine uncovered islands".to_owned(),
        };
        assert_eq!(refused.exit_code(), PipelineExitCode::Gates);
        let failed = ModelPreparationOutcome::Failed {
            exit_code: PipelineExitCode::Input,
            detail: "unreadable".to_owned(),
        };
        assert_eq!(failed.exit_code(), PipelineExitCode::Input);

        let model = |name: &str, outcome| ModelPreparationReport {
            model: ModelId::new(name),
            source_path: PathBuf::from(format!("models/{name}.stl")),
            assumed_print_size: size(30.0),
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
        assert_eq!(wire["models"][2]["outcome"]["exit_code"], "input");
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
            outcome,
        };
        let sidecar = model(
            "._mini",
            ModelPreparationOutcome::Failed {
                exit_code: PipelineExitCode::Input,
                detail: "not a binary STL".to_owned(),
            },
        );
        let refused = model(
            "mini",
            ModelPreparationOutcome::Refused {
                gate: Some(PreWriteGate::UncoveredIslands),
                geometry: Some(SupportedGeometry::DrilledShellSealed),
                violation: "nine uncovered islands".to_owned(),
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
                PipelineError::OrientationPolicyUnavailable { model: model() },
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
}
