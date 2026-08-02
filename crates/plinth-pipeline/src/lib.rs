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

use plinth_core::ModelId;
use plinth_decimate::{DecimationBudget, DecimationError};
use plinth_emit::{EmitError, EmitReport};
use plinth_hollow::{HollowError, HollowRequest, HollowedModel};
use plinth_orient::{OrientError, OrientedModel};
use plinth_support::{SupportError, SupportRequest};
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
) -> Result<HollowedModel, HollowError> {
    plinth_hollow::run(model, request)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageStop {
    Decimate,
    Orient,
    Hollow,
    Support,
}

/// One batch request: every source STL in `input_models_dir` maps to one output stem.
#[derive(Clone, Debug, PartialEq)]
pub struct PrepareRequest {
    pub input_models_dir: PathBuf,
    pub output_dir: PathBuf,
    pub pinned_baseline_dir: PathBuf,
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
    #[error("model {model} failed pinned orientation")]
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

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_core::m0_constants;
    use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
    use plinth_hollow::{HollowDisposition, SolidFallbackReason};
    use plinth_mesh::{load_binary_stl, make_box, Mesh, WeldPolicy};
    use plinth_orient::{run as run_orientation, OrientRequest, PinnedBaseline};

    fn baseline(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/baseline")
            .join(name)
    }

    fn oriented(mesh: Mesh) -> OrientedModel {
        let budget =
            DecimationBudget::ratified_default(ProbeConfig::new(1.0, usize::MAX, 10_000.0))
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
        HollowRequest {
            printer: m0_constants()
                .expect("registered constants")
                .saturn4_ultra()
                .clone(),
        }
    }

    #[test]
    fn hollow_stage_preserves_history_and_only_hands_off_proved_drilled_mesh() {
        let sphere = load_binary_stl(baseline("Sphere.prepped.stl"), WeldPolicy::None)
            .expect("committed Sphere fixture");
        let oriented = oriented(sphere);
        let expected_history = oriented.history().clone();
        let model = hollow_stage(oriented, &hollow_request()).expect("Sphere hollow transition");

        assert_eq!(
            model.history().hollow().disposition,
            HollowDisposition::HollowedAndDrilled
        );
        assert!(!model.history().hollow().cavities.is_empty());
        assert!(!model.history().hollow().drains.is_empty());
        assert_eq!(model.history().orientation(), &expected_history);
        assert_eq!(
            model.history().orientation().decimation().trace(),
            expected_history.decimation().trace()
        );
    }

    #[test]
    fn hollow_stage_retains_original_oriented_solid_for_explicit_fallback() {
        let oriented = oriented(make_box([20.0, 8.0, 8.0], [0.0; 3]).expect("thin closed box"));
        let allocation = oriented.mesh().verts().as_ptr();
        let expected_mesh = oriented.mesh().clone();
        let expected_history = oriented.history().clone();
        let model = hollow_stage(oriented, &hollow_request()).expect("explicit solid fallback");

        assert_eq!(model.mesh().verts().as_ptr(), allocation);
        assert_eq!(model.mesh(), &expected_mesh);
        assert_eq!(model.history().orientation(), &expected_history);
        assert_eq!(
            model.history().hollow().disposition,
            HollowDisposition::SolidFallback {
                reason: SolidFallbackReason::IneligibleForHollowing,
            }
        );
        assert!(model.history().hollow().cavities.is_empty());
        assert!(model.history().hollow().drains.is_empty());
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
