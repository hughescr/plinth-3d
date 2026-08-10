//! Folder-in, one-artifact-per-model pipeline data boundary.
//!
//! This checkpoint intentionally contains no loader, cache, stage runner, or emit loop. Behavior
//! lands on the parallel `plinth-pipeline` implementation branch with this signature:
//!
//! `plinth_pipeline::prepare(PrepareRequest) -> Result<PrepareReport, PipelineError>`
//!
//! A full run has one `Artifact` outcome per source model. Stage stops are a typed testing
//! outcome, not a stringly alternate state machine. Cache bypass has exactly one data flag,
//! `no_cache`, which is wired only from the future CLI spelling `--no-cache`.
#![forbid(unsafe_code)]

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "baseline", rename_all = "snake_case")]
pub enum OrientationPolicy {
    #[default]
    LegacyCoarse,
    Pinned(PinnedBaseline),
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
    Artifact { emit: EmitReport },
    Stopped { after: StageStop },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelPreparationReport {
    pub model: ModelId,
    pub source_path: PathBuf,
    pub outcome: ModelPreparationOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareReport {
    pub no_cache: bool,
    pub models: Vec<ModelPreparationReport>,
}

impl PrepareReport {
    /// Full-run artifacts in deterministic model order. A stopped model contributes no artifact.
    pub fn artifacts(&self) -> impl Iterator<Item = &Path> {
        self.models.iter().filter_map(|model| match &model.outcome {
            ModelPreparationOutcome::Artifact { emit } => Some(emit.artifact_path.as_path()),
            ModelPreparationOutcome::Stopped { .. } => None,
        })
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
            Self::InputFolder { .. } | Self::Decimate { .. } | Self::Orient { .. } => {
                PipelineExitCode::Input
            }
            Self::Hollow { .. } => PipelineExitCode::Hollow,
            Self::Support { .. } => PipelineExitCode::Supports,
            Self::Emit { .. } => PipelineExitCode::Emit,
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
        let policy = OrientationPolicy::Pinned(PinnedBaseline {
            pipeline_report: PathBuf::from("baseline/Sphere.pipeline.json"),
        });
        let wire = serde_json::to_string(&policy).expect("policy serializes");
        assert_eq!(
            wire,
            r#"{"kind":"pinned","baseline":{"pipeline_report":"baseline/Sphere.pipeline.json"}}"#
        );
        assert_eq!(
            serde_json::from_str::<OrientationPolicy>(&wire).expect("policy deserializes"),
            policy
        );
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

    #[test]
    fn report_distinguishes_one_artifact_from_an_explicit_stage_stop() {
        let report = PrepareReport {
            no_cache: true,
            models: vec![
                ModelPreparationReport {
                    model: ModelId::new("mini-a"),
                    source_path: PathBuf::from("models/mini-a.stl"),
                    outcome: ModelPreparationOutcome::Artifact {
                        emit: EmitReport {
                            artifact_path: PathBuf::from("out/mini-a.chitubox"),
                            support_record_count: 4,
                            artifact_bytes: 2048,
                        },
                    },
                },
                ModelPreparationReport {
                    model: ModelId::new("mini-b"),
                    source_path: PathBuf::from("models/mini-b.stl"),
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
        assert_eq!(wire["models"][1]["outcome"]["after"], "hollow");
    }
}
