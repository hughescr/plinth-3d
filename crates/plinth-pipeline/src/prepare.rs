//! The production `plinth prepare` executor: a folder of `.stl` in, one
//! `.chitubox` per model out, one typed disposition per model either way.
//!
//! Until this module existed the whole end-to-end composition lived in
//! `examples/shared/first_artifact_chain.rs`, where it was reachable only from
//! a test and an artifact-producing example. `crates/plinth-cli` parsed the
//! ratified command line and then printed "execution is not implemented".
//! Design record E4/E5 (`docs/engineering-journal/export-path-design-2026-08-07.md`)
//! names this the missing piece; this is it. The example's shared module is now
//! a thin corpus-fixture wrapper over the functions below, so the code the test
//! gates and the code the CLI runs cannot drift apart (O-5: one implementation
//! per capability).
//!
//! **Nothing here relaxes a gate.** `seal_and_emit` reaches emission only
//! through [`crate::judge_then_emit`], which is the sole public door to the
//! pre-write gate set and takes no override parameter. A gate refusal is
//! terminal for that model, leaves no file at its target path, and projects to
//! [`PipelineExitCode::Gates`].
//!
//! **Batch-first, and where the batch actually is.** The per-model loop is
//! I/O-bound orchestration, not a hot path — every hot path in this chain
//! (decimation probes, voxel work, the support tournament, record assembly)
//! is already one batched call inside its stage crate. Two things are
//! deliberately *not* batched across models and the reasons are recorded here
//! rather than left to be rediscovered:
//!
//! - `plinth_mesh::load_binary_stls` is a real batch API and is still not used,
//!   because it fails the whole slice on the first bad file and holds every
//!   mesh resident at once. This executor's contract is the opposite on both
//!   counts: one unreadable model must not cost the folder its other
//!   artifacts, and a folder of minis must not need all of them in memory
//!   simultaneously.
//! - the run-level policy assembly — resource policy, printer profile, support
//!   limits — is gathered exactly once before the loop and reused, rather than
//!   rebuilt per model as the example chain did.
//!
//! **Determinism.** Sources are enumerated, sorted by path, and processed in
//! that order; the report is built in the same order. No wall clock is read and
//! nothing is seeded from the environment, so two runs over the same folder
//! produce the same report bytes. Models are processed serially: the stage
//! crates are already internally parallel, and running whole models
//! concurrently would put several support tournaments' peak allocations on the
//! machine at once for no ordering benefit.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use plinth_core::{ModelId, PositiveFinite, PrinterProfile};
use plinth_decimate::{
    run as run_decimation, DecimatedModel, DecimationBudget, ProbeConfig, ScaleSanity,
};
use plinth_emit::EmitRequest;
use plinth_hollow::{
    HollowDisposition, HollowError, HollowRequest, HollowStageOutput, HollowedModel,
    SolidDecisionReason,
};
use plinth_mesh::{load_binary_stl, WeldPolicy};
use plinth_orient::{run as run_orientation, OrientRequest, OrientedModel};
use plinth_support::{run_support, SupportRequest};

use crate::{
    hollow_stage, AssumedPrintSize, ModelPreparationOutcome, ModelPreparationReport,
    OrientationPolicy, OutputProfile, PipelineError, PipelineExitCode, PreWriteGate, PrepareReport,
    PrepareRequest, PrepareResourcePolicy, PrepareResourcePolicyError, StageStop,
};

/// The file extension this executor treats as a source model, compared
/// case-insensitively so a folder written by a Windows tool is not silently
/// skipped.
const SOURCE_EXTENSION: &str = "stl";

/// The extension of every artifact this executor writes.
pub const ARTIFACT_EXTENSION: &str = "chitubox";

/// The LOD identity bound into the prepared mesh's delicacy field.
///
/// Owner override O-8 makes the protected, decimated mesh the sole production
/// geometry, and the recorded FaceTable ruling makes delicacy a property of
/// the LOD being scored. This is that LOD: the sealed, hollowed, drilled mesh
/// the support stage plans against, and the only one whose delicacy the
/// support stage may ever read. A search proxy is a different LOD and carries
/// a different name; the two can never be confused because the name is inside
/// the field's own digest.
pub const PREPARED_MESH_LOD: &str = "prepared";

/// The delicacy field the support stage anchors against, measured on the
/// prepared mesh itself.
///
/// This fed `vec![0.0; prepared_faces]` until 2026-08-10. That was not a
/// neutral default: `strut_model_anchor_max_delicacy` rejects an anchor
/// candidate whose face is *more* delicate than the cap, so an all-zeros field
/// asserted that no face on any model is delicate and left the cap structurally
/// unreachable — the support stage carried the machinery to protect delicate
/// geometry and was handed, on every model, a measurement that could never trip
/// it. The owner ruling of 2026-08-10 closed that by building the producer-side
/// evaluator; see
/// `docs/engineering-journal/decisions/2026-08-10-delicacy-producer.md`.
///
/// The field is bound to this mesh at [`PREPARED_MESH_LOD`], which is what
/// makes the per-face weights and the per-face areas the support stage measures
/// two views of one object rather than two meshes silently mixed.
///
/// This is a named function rather than four lines inside `seal_and_emit` so a
/// test can assert against exactly what production computes, not against a
/// second copy of it.
///
/// # Errors
///
/// [`plinth_mesh::DelicacyError`] whenever the field cannot be measured. There
/// is deliberately no fallback: an all-zeros field is not "delicacy unknown",
/// it is the positive claim that nothing on this model is delicate, and this
/// stage does not make claims it has not measured.
pub fn prepared_delicacy_field(
    mesh: &plinth_mesh::Mesh,
) -> Result<plinth_mesh::DelicacyField, plinth_mesh::DelicacyError> {
    plinth_mesh::evaluate_delicacy(
        mesh,
        PREPARED_MESH_LOD,
        plinth_mesh::DelicacySettings::production()?,
        plinth_mesh::DelicacyBuildConfig::production(mesh.tris().len()),
    )
}

/// The production decimation probe configuration, read from the M0 registry.
///
/// `plinth-decimate` states outright that it publishes no default probe
/// configuration and that "the load-to-decimate integration owns supplying a
/// finite profile-derived request". This module is that integration, and this
/// is that request: the pixel pitch comes from the ratified Saturn measurement
/// through [`DecimationBudget::ratified_default`], and the three probe values
/// come from `DECIMATION_PROBE_GRID_PITCH_MM`,
/// `DECIMATION_PROBE_MAX_GRID_BYTES` and
/// `DECIMATION_PROBE_NEAREST_UPPER_BOUND_MM`. They are registered Placeholders
/// carrying exactly the numbers 18 call sites in this workspace already passed
/// by hand, so publishing them moved no digest — but they are read here, not
/// restated.
///
/// # Errors
///
/// [`PrepareResourcePolicyError::Decimation`] when the registry cannot be read
/// or publishes a probe configuration decimation will not execute against.
pub fn registered_decimation_budget() -> Result<DecimationBudget, PrepareResourcePolicyError> {
    let constants = plinth_core::m0_constants().map_err(|error| {
        plinth_decimate::BudgetConfigError::Constants {
            message: error.to_string(),
        }
    })?;
    let placeholders = constants.placeholders();
    let probe = ProbeConfig::new(
        placeholders
            .decimation_probe_grid_pitch_mm()
            .diagnostic()
            .get(),
        usize::try_from(
            placeholders
                .decimation_probe_max_grid_bytes()
                .diagnostic()
                .get(),
        )
        .map_err(|_| plinth_decimate::BudgetConfigError::InvalidGridBytes { value: 0 })?,
        placeholders
            .decimation_probe_nearest_upper_bound_mm()
            .diagnostic()
            .get(),
    );
    let budget = DecimationBudget::ratified_default(probe)?;
    budget.validate_for_execution()?;
    Ok(budget)
}

/// The lift every model in a run is planned and emitted at, read from the
/// selected printer profile's registered `LIFT_MM` rather than restated.
///
/// # Errors
///
/// [`PrepareResourcePolicyError::Lift`] when the profile publishes a lift the
/// support stage cannot accept.
pub fn registered_support_request(
    printer: &PrinterProfile,
) -> Result<SupportRequest, PrepareResourcePolicyError> {
    let lift_mm = PositiveFinite::new(*printer.lift_default_mm().diagnostic())
        .map_err(|_| PrepareResourcePolicyError::Lift)?;
    Ok(SupportRequest { lift_mm })
}

/// Which geometry actually reached the sealed support run, and why.
///
/// This is measured, never assumed. The design record's OQ-1 asked whether
/// `plinth_hollow::run` completes on corpus models; it does, and the answer
/// turned out to matter for a reason OQ-1 did not anticipate, so the state is
/// carried out to the caller rather than buried in a comment.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SupportedGeometry {
    /// The hollow stage decided solid on its own terms and support planning
    /// consumed exactly what it produced. Nothing here is a fallback.
    HollowStageDecidedSolid,
    /// Hollowed, drained and drilled, and the sealed support run accepted that
    /// shell. This is the path the pipeline is ultimately meant to take.
    DrilledShellSealed,
    /// Hollowed, drained and drilled -- and then refused by the sealed support
    /// run, so the oriented solid carried the model instead.
    ///
    /// The refusal string is the runner's own message plus the model-scaled
    /// caps it was measured against, because a refusal without its bound is
    /// not a measurement.
    DrilledShellRefusedBySupport {
        drilled_faces: usize,
        refusal: String,
    },
    /// The hollow stage itself **errored** -- it neither hollowed nor decided
    /// solid -- so the oriented solid carried the model instead.
    ///
    /// This is the case OQ-1 actually anticipated ("a corpus model might
    /// *refuse* to hollow") and which the Sphere never exercised, so the
    /// typed solid-decision fallback OQ-1 recommended for it was never built
    /// until the Weeping Treant hit it. `plinth_hollow::run` converts only the
    /// five ineligibility codes into a typed `HollowStageOutput::Solid`; every
    /// other `HollowError` propagates and arrives here.
    ///
    /// **History, not current behaviour:** the model that first reached this
    /// variant was the Weeping Treant, whose `InvalidShellTopology` was FU-34
    /// and then FU-49. It no longer takes this path — the 2026-08-11
    /// well-composedness repair made its cavity extractable and it hollows —
    /// and no corpus model is known to take it today. The branch is kept and
    /// gated on a *pre-hollowed* fixture instead, whose refusal is designed and
    /// permanent rather than a defect waiting to be fixed; see the test in
    /// `tests/first_artifact.rs`.
    ///
    /// **The provenance caveat, stated rather than hidden (the F14 pattern).**
    /// The reserve is taken through
    /// `HollowedModel::solid_decision_recording_repair`, whose
    /// `SolidDecisionReason` names why a *decision* was made and has no value
    /// meaning "the hollow stage errored" — `NoCavityAfterRepair` and
    /// `BelowThresholdAfterRepair`, added with the repair, distinguish a
    /// repair-caused decision from a property of the geometry, but none of the
    /// five covers an error. (What the constructor *does* carry across the
    /// error is the repair's own measurement, taken off the
    /// `HollowStageFailure`, so this record cannot claim a repair never ran.)
    /// So
    /// the model handed to support planning is stamped `BelowThreshold` by a
    /// stage that never reached a threshold test, and that stamp is an
    /// imprecision this seam knowingly inherits -- the same one the
    /// support-refusal path above already carries, for the same reason: it is
    /// the only handoff `plinth-hollow` exposes to a caller holding an
    /// oriented solid, and it is the one every sealed-run gate and the whole
    /// OD-8 census already travel. **This variant is where the truth lives.**
    /// `refusal` is the typed `HollowError`'s own `Display`, verbatim and
    /// unedited, so the artifact's real provenance is recoverable from the
    /// outcome even though the `HollowedModel`'s own reason field cannot
    /// express it. Anyone reading `hollow_disposition` alone will read a
    /// retained-solid/below-threshold verdict and be misled; read this variant
    /// beside it.
    HollowStageRefused { refusal: String },
}

/// The refused gate attempt, carried whole rather than rendered.
///
/// Both fields exist because a refusal string cannot answer the question the
/// terminal-`Gates` design turns on: *which geometry* was refused. "The
/// drilled shell sealed and the gate refused its plan" and "support refused
/// the drilled shell, the chain fell back to the oriented solid, and the gate
/// refused that" produce the same exit code and very nearly the same message,
/// and they are exactly the substitution the terminal refusal exists to
/// prevent. The gate is likewise carried as an enum value, so a renamed
/// variant is a compile error rather than a silently-permanently-false
/// substring assertion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateStop {
    /// The gate that spoke, when one gate owns the refusal.
    pub gate: Option<PreWriteGate>,
    /// The geometry the refused plan was built on.
    pub geometry: SupportedGeometry,
}

/// Why one model did not reach an artifact.
///
/// The distinction between a gate stop and every other stop is load-bearing,
/// not cosmetic. The oriented-solid reserve answers a **support-run** refusal:
/// the runner could not seal *this geometry*, so a different geometry is a
/// legitimate second attempt. It does not answer a **gate** refusal, which is a
/// physics verdict on a plan that sealed perfectly well; retrying it against
/// the reserve would turn "this model is not adequately supported" into "here
/// is a different artifact", which is precisely the substitution the pre-write
/// gate exists to stop. A gate stop is therefore terminal, and no file is
/// written.
/// The typed error is boxed: `PipelineError` carries whole `CoverageReport`s
/// and stage diagnostics, and an unboxed one would put ~170 bytes of `Err` on
/// every `Result` in this chain.
#[derive(Debug)]
pub struct ChainStop {
    error: Box<PipelineError>,
    gate: Option<GateStop>,
    /// The bounds the refused stage was measured against, when it has any.
    /// A refusal without its bound is not a measurement.
    measured_against: Option<String>,
}

impl ChainStop {
    #[must_use]
    pub fn new(error: PipelineError) -> Self {
        Self {
            error: Box::new(error),
            gate: None,
            measured_against: None,
        }
    }

    #[must_use]
    pub fn refused(error: PipelineError, gate: GateStop) -> Self {
        Self {
            error: Box::new(error),
            gate: Some(gate),
            measured_against: None,
        }
    }

    /// Attach the model-scaled ceilings a stage refused under.
    #[must_use]
    pub fn measured_against(mut self, bounds: String) -> Self {
        self.measured_against = Some(bounds);
        self
    }

    /// The typed stage error behind this stop.
    #[must_use]
    pub fn error(&self) -> &PipelineError {
        &self.error
    }

    /// The frozen per-stage exit-code projection, unchanged by this module.
    #[must_use]
    pub fn exit_code(&self) -> PipelineExitCode {
        self.error.exit_code()
    }

    /// Whether the pre-write physics gates are what stopped this model.
    #[must_use]
    pub fn is_gates(&self) -> bool {
        matches!(*self.error, PipelineError::Gates { .. })
    }

    /// Whether **support planning** is what stopped this model — the one stop
    /// the oriented-solid reserve answers.
    ///
    /// The reserve's claim is "this *geometry* could not be sealed", so only a
    /// refusal by the sealed run or the planner may reach it. A `Lowering`, an
    /// `Emit` (the post-write disk audit refusing the bytes), a `LayerPlan` or
    /// a `DegenerateExtent` says nothing about whether the geometry is
    /// supportable, and re-planning a different geometry after one of those
    /// would write a *substitute* artifact and label it a support refusal.
    ///
    /// **A [`Self::is_coverage_closure_refusal`] stop is excluded (FU-56).**
    /// That refusal means the sealed run built, routed and lowered a real
    /// plan for *this* geometry and could not close its own coverage promise
    /// from the immutable repair pool within the registered attempt bound —
    /// it is a physics verdict on a plan that sealed perfectly well, exactly
    /// like a pre-write gate refusal, not a claim that the geometry itself
    /// could not be sealed. Re-planning the oriented solid after it would be
    /// the same substitution the gate case above already refuses to allow.
    #[must_use]
    pub fn is_support_refusal(&self) -> bool {
        match &*self.error {
            PipelineError::Support { .. } => true,
            PipelineError::SealedRun { source, .. } => !matches!(
                source,
                plinth_support::SupportRunError::CoverageClosure(_)
            ),
            _ => false,
        }
    }

    /// Whether this stop is specifically a coverage-closure refusal (FU-56):
    /// the sealed run's own coverage-closure controller could not admit every
    /// measured island within the registered attempt bound. See
    /// [`Self::is_support_refusal`] for why this is excluded from that check.
    #[must_use]
    pub fn is_coverage_closure_refusal(&self) -> bool {
        matches!(
            &*self.error,
            PipelineError::SealedRun {
                source: plinth_support::SupportRunError::CoverageClosure(_),
                ..
            }
        )
    }

    /// The gate that refused, as a value.
    #[must_use]
    pub const fn gate(&self) -> Option<PreWriteGate> {
        match &self.gate {
            Some(stop) => stop.gate,
            None => None,
        }
    }

    /// The geometry whose sealed plan the gate refused.
    #[must_use]
    pub const fn refused_geometry(&self) -> Option<&SupportedGeometry> {
        match &self.gate {
            Some(stop) => Some(&stop.geometry),
            None => None,
        }
    }

    /// The stop rendered with the source chain the `#[source]` attributes
    /// carry, which `Display` alone drops.
    #[must_use]
    pub fn detail(&self) -> String {
        let mut rendered = self.error.to_string();
        let mut source = std::error::Error::source(&self.error);
        while let Some(cause) = source {
            rendered.push_str(": ");
            rendered.push_str(&cause.to_string());
            source = cause.source();
        }
        if let Some(bounds) = &self.measured_against {
            rendered.push_str(" [");
            rendered.push_str(bounds);
            rendered.push(']');
        }
        rendered
    }
}

impl std::fmt::Display for ChainStop {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.gate {
            Some(stop) => write!(
                formatter,
                "pre-write gates refused on {:?}: {}",
                stop.geometry,
                self.detail()
            ),
            None => formatter.write_str(&self.detail()),
        }
    }
}

/// Everything one model's chain observed, reported rather than asserted so
/// every caller can decide what to do with it.
#[derive(Clone, Debug)]
pub struct ChainOutcome {
    pub model: ModelId,
    pub source_faces: usize,
    pub prepared_faces: usize,
    pub hollow_disposition: HollowDisposition,
    /// What the well-composedness repair did to this model's cavity, when one
    /// ran. Carried out of the chain rather than left inside `plinth-hollow`,
    /// where nothing outside the crate could read it.
    pub cavity_repair: Option<plinth_hollow::CavityRepairEvidence>,
    pub supported_geometry: SupportedGeometry,
    pub routed_contacts: usize,
    pub final_live_members: usize,
    pub snapshot_digest: u64,
    /// The pre-write gates that are named and consulted but not yet ported.
    /// Carried out rather than described: the artifact below was authorized by
    /// one gate out of twelve, and this is the list of the other eleven.
    pub pending_gates: Vec<PreWriteGate>,
    pub primitive_count: usize,
    pub lift_mm: f64,
    pub plate_z_mm: f64,
    pub emit: plinth_emit::EmitReport,
}

/// Everything one model's stage chain needs beyond the model itself, gathered
/// once per run rather than rebuilt per model.
///
/// The printer profile is not a separate field: it is `hollow.printer`, so
/// there is exactly one profile in a run and no way for the hollow stage and
/// the emitter to disagree about which machine they are planning for.
#[derive(Clone, Debug)]
pub struct StagePolicy {
    pub orientation: OrientationPolicy,
    pub hollow: HollowRequest,
    pub support: SupportRequest,
    pub resources: PrepareResourcePolicy,
}

impl StagePolicy {
    /// The printer every stage in this run plans against.
    #[must_use]
    pub const fn printer(&self) -> &PrinterProfile {
        &self.hollow.printer
    }

    /// Assembles the one production policy for a request's own selections.
    ///
    /// # Errors
    ///
    /// [`PrepareResourcePolicyError`] when the ratified authority behind any
    /// stage-owned limit cannot be read or is not executable.
    pub fn for_request(request: &PrepareRequest) -> Result<Self, PrepareResourcePolicyError> {
        let resources = match request.output_profile {
            OutputProfile::Saturn4Ultra => {
                PrepareResourcePolicy::for_saturn4_ultra(request.decimation_budget)?
            }
        };
        Ok(Self {
            orientation: request.orientation.clone(),
            hollow: request.hollow.clone(),
            support: request.support.clone(),
            resources,
        })
    }
}

/// Prepare every source model in a folder.
///
/// Enumerates `*.stl` in `request.input_models_dir` in sorted order, runs the
/// full stage chain on each, and writes `<stem>.chitubox` into
/// `request.output_dir` for every model that passes the pre-write physics
/// gates. **One bad model never aborts the folder**: a load failure, an
/// unorientable model, a stage error and a gate refusal are all per-model
/// dispositions in the returned report, and every other model in the folder is
/// still attempted.
///
/// The returned `Err` is reserved for failures of the *run*, not of a model:
/// an unreadable input folder, an uncreatable output folder, or a resource
/// policy that will not assemble. Nothing has been written when one of those
/// is returned.
///
/// **No model's artifact path survives its own run.** Every model's target is
/// cleared before that model is attempted, so a folder re-run after a model
/// started failing reports the failure *and* the earlier artifact is gone.
/// A stale file beside an ERROR or GATE-REFUSED line is read by the next thing
/// down the line as this run's answer, and that is the hand-off the pre-write
/// gate exists to prevent.
///
/// # Errors
///
/// - [`PipelineError::InputFolder`] when the source folder cannot be read.
/// - [`PipelineError::OutputFolder`] when the destination cannot be created.
/// - [`PipelineError::ResourcePolicy`] when the run's stage limits will not
///   assemble from the ratified authorities.
pub fn prepare(request: PrepareRequest) -> Result<PrepareReport, PipelineError> {
    let sources = enumerate_sources(&request.input_models_dir)?;
    std::fs::create_dir_all(&request.output_dir).map_err(|source| PipelineError::OutputFolder {
        path: request.output_dir.clone(),
        source,
    })?;
    let policy = StagePolicy::for_request(&request)
        .map_err(|source| PipelineError::ResourcePolicy { source })?;

    let mut claimed_targets: BTreeSet<PathBuf> = BTreeSet::new();
    let mut models = Vec::with_capacity(sources.len());
    for source_path in sources {
        let model = ModelId::new(model_stem(&source_path));
        let target = request
            .output_dir
            .join(format!("{}.{ARTIFACT_EXTENSION}", model.as_str()));

        // Two sources in one folder that differ only in extension case would
        // otherwise race for one artifact path and the loser's bytes would be
        // silently replaced by the winner's. Refuse the collision by name.
        if !claimed_targets.insert(target.clone()) {
            models.push(ModelPreparationReport {
                model,
                source_path,
                assumed_print_size: None,
                // Refused by name before it was ever opened, so the pre-pass
                // never saw it.
                repair: None,
                outcome: ModelPreparationOutcome::Failed {
                    exit_code: PipelineExitCode::Input,
                    detail: format!(
                        "two source models in this folder claim the artifact path {}",
                        target.display()
                    ),
                },
            });
            continue;
        }

        models.push(prepare_one(
            &model,
            &source_path,
            &target,
            &policy,
            request.stop_after,
        ));
    }

    Ok(PrepareReport {
        no_cache: request.no_cache,
        models,
    })
}

/// Every `*.stl` in one folder, in sorted path order.
fn enumerate_sources(directory: &Path) -> Result<Vec<PathBuf>, PipelineError> {
    let entries = std::fs::read_dir(directory).map_err(|source| PipelineError::InputFolder {
        path: directory.to_path_buf(),
        source,
    })?;
    let mut sources = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| PipelineError::InputFolder {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let is_source = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case(SOURCE_EXTENSION));
        // `is_file` follows symlinks, which is deliberate: the corpus is
        // provisioned as symlinks into a sibling checkout.
        if is_source && path.is_file() {
            sources.push(path);
        }
    }
    sources.sort();
    Ok(sources)
}

fn model_stem(path: &Path) -> String {
    path.file_stem().map_or_else(
        || path.to_string_lossy().into_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    )
}

/// One model, load to artifact, with its disposition captured either way.
fn prepare_one(
    model: &ModelId,
    source_path: &Path,
    target: &Path,
    policy: &StagePolicy,
    stop_after: Option<StageStop>,
) -> ModelPreparationReport {
    // `repair` is threaded through every disposition rather than attached only
    // to the success path: a model refused *after* the pre-pass edited it must
    // say what was edited, or the refusal is being reported against geometry
    // the reader cannot reconstruct.
    let mut repair: Option<plinth_mesh::RepairEvidence> = None;
    let report = |repair: &Option<plinth_mesh::RepairEvidence>, assumed_print_size, outcome| {
        ModelPreparationReport {
            model: model.clone(),
            source_path: source_path.to_path_buf(),
            assumed_print_size,
            repair: repair.clone(),
            outcome,
        }
    };

    // **Clear the target before anything can fail.** The no-file guarantee is
    // about the *path*, not about one call: `judge_then_emit` removes a stale
    // artifact when a gate refuses, but a model that stops earlier -- load,
    // decimation, scale sanity, orientation, layer planning, the sealed support
    // run -- never reaches that code at all, and would leave the previous run's
    // file sitting at the target beside an ERROR line. The next reader takes it
    // for this run's answer. Clearing here, once, covers every disposition this
    // function can produce including ones it does not have yet.
    //
    // A removal that fails is reported, never swallowed: "refused, and the old
    // file is still there" is the misleading hand-off this exists to stop, so
    // it fails the model by name instead of proceeding.
    if let Err(source) = clear_stale_artifact(target) {
        return report(
            &repair,
            None,
            failure(&ChainStop::new(PipelineError::StaleArtifact {
                model: model.clone(),
                path: target.to_path_buf(),
                source,
            })),
        );
    }

    let mesh = match load_binary_stl(source_path, WeldPolicy::None) {
        Ok(mesh) => mesh,
        Err(source) => {
            return report(
                &repair,
                None,
                failure(&ChainStop::new(PipelineError::Load {
                    model: model.clone(),
                    path: source_path.to_path_buf(),
                    source,
                })),
            )
        }
    };
    let source_faces = mesh.tris().len();

    // **The repair pre-pass.** D-22 as written refused bad geometry and left
    // the owner to fix it in Blender; the owner ruling of 2026-08-10 overturned
    // that and made repair automatic and in-pipeline
    // (`docs/engineering-journal/decisions/2026-08-10-d22-revision-automatic-repair.md`).
    // It sits here, on the loaded input, because that is upstream of every gate
    // in the chain — and it is *only* here. No gate below is softened by a
    // single line of this change; a model whose defects survive the pre-pass is
    // refused exactly as it was before, and the refusal becomes an automation
    // work item rather than a threshold to move.
    //
    // A mesh with none of the three census defect classes comes out of this
    // bit-identical, so a clean model is not perturbed by a stage that had
    // nothing to do.
    //
    // **There is no failure arm here, and there cannot be one.** The pre-pass
    // returns a mesh, never a `Result`: a stage that is mandatory on this chain
    // and able to fail would be a new hard-refusal class invented by a repair,
    // which is the exact inversion of the ruling. An input it cannot work on —
    // over its resource bound, or any internal failure below it — comes back
    // bit-identical with the reason in `RepairEvidence::actions_skipped`, and
    // every stage below judges the model exactly as it did before this stage
    // existed.
    let (mesh, evidence) = plinth_mesh::repair_mesh(&mesh).into_parts();
    repair = Some(evidence);

    let outcome = match run_decimation(&mesh, policy.resources.decimation()) {
        Ok(outcome) => outcome,
        Err(source) => {
            return report(
                &repair,
                None,
                failure(&ChainStop::new(PipelineError::Decimate {
                    model: model.clone(),
                    source,
                })),
            )
        }
    };
    // The assumed print size is recovered from whichever branch the scale
    // sanity check took, so a refused model still reports the size it was
    // refused at. That is the whole point of surfacing it: "this model was
    // treated as 0.4 mm across" is the finding, and it is invisible in a bare
    // stage error.
    let decimated = match outcome.into_decimated_model() {
        Ok(decimated) => decimated,
        Err(scale) => {
            return report(
                &repair,
                Some(AssumedPrintSize::from(scale)),
                failure(&ChainStop::new(PipelineError::ScaleSuspicious {
                    model: model.clone(),
                    longest_axis_mm: scale.longest_axis_mm,
                })),
            )
        }
    };
    let assumed = Some(AssumedPrintSize::from(decimated.audit().trace().scale));
    if stop_after == Some(StageStop::Decimate) {
        return report(
            &repair,
            assumed,
            ModelPreparationOutcome::Stopped {
                after: StageStop::Decimate,
            },
        );
    }

    let oriented = match orient_model(
        model,
        decimated,
        &policy.orientation,
        policy.resources.orient(),
    ) {
        Ok(oriented) => oriented,
        Err(stop) => return report(&repair, assumed, failure(&stop)),
    };
    if stop_after == Some(StageStop::Orient) {
        return report(
            &repair,
            assumed,
            ModelPreparationOutcome::Stopped {
                after: StageStop::Orient,
            },
        );
    }

    match run_from_oriented(model, source_faces, oriented, policy, target, stop_after) {
        // The provenance travels with the artifact. Two of the four
        // `SupportedGeometry` values mean "the chain could not use the geometry
        // it planned and substituted another", and a report that dropped them
        // would make a substituted artifact indistinguishable from the intended
        // drilled shell.
        Ok(Some(outcome)) => report(
            &repair,
            assumed,
            ModelPreparationOutcome::Artifact {
                emit: outcome.emit,
                geometry: outcome.supported_geometry,
                hollow_disposition: outcome.hollow_disposition,
                cavity_repair: outcome.cavity_repair,
            },
        ),
        Ok(None) => report(
            &repair,
            assumed,
            ModelPreparationOutcome::Stopped {
                after: stop_after.expect("a chain that stopped was told where to stop"),
            },
        ),
        Err(stop) => report(&repair, assumed, failure(&stop)),
    }
}

/// Remove anything sitting at one model's artifact path.
///
/// `NotFound` is the ordinary case and is not an error; every other io error
/// is, including the target being a directory, because in both of those the
/// path this model is about to claim is still occupied by something that is not
/// this run's answer.
fn clear_stale_artifact(target: &Path) -> Result<(), std::io::Error> {
    match std::fs::remove_file(target) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Project a stop into the report's serializable disposition.
fn failure(stop: &ChainStop) -> ModelPreparationOutcome {
    if stop.is_gates() {
        ModelPreparationOutcome::Refused {
            gate: stop.gate(),
            geometry: stop.refused_geometry().cloned(),
            violation: stop.detail(),
        }
    } else {
        ModelPreparationOutcome::Failed {
            exit_code: stop.exit_code(),
            detail: stop.detail(),
        }
    }
}

/// Apply the run's orientation policy to one decimated model.
///
/// **Orientation is never inferred, and it is now searched.**
/// [`OrientationPolicy::DenseSweep`] — the default — runs the recorded D-12
/// funnel: the whole candidate grid on the ~2 % proxy, the top
/// `ORIENT_REFINE_TOP_K` on the ~10 % proxy, the top `ORIENT_COMMIT_TOP_K` at
/// the production mesh, where the committed score is computed and only there.
/// The winner arrives with its full evidence attached, so a searched rotation
/// is auditable and re-pinnable rather than a bare matrix. Nothing about the
/// old refusal's *reason* has been softened: a model the sweep cannot measure
/// is still a per-model refusal with a named cause, never a fallback to
/// identity. Identity is a rotation like any other and is chosen here only by
/// winning.
///
/// **A pinned baseline is resolved per model, by name.** `plinth_orient::run`
/// applies whatever rotation the report it is handed states, to whatever mesh
/// it is handed, and has no way to check that the two describe the same object.
/// One run-wide baseline over a folder of different minis would therefore give
/// every one of them the first one's rotation, and a misoriented model seals,
/// gate-passes and emits exactly like a correct one — there is no downstream
/// stage that would catch it. So the policy is keyed by source stem, and a
/// model the map does not name is refused
/// ([`PipelineError::OrientationBaselineMissing`]) rather than given a
/// neighbour's answer.
fn orient_model(
    model: &ModelId,
    decimated: DecimatedModel,
    policy: &OrientationPolicy,
    limits: plinth_orient::OrientSearchLimits,
) -> Result<OrientedModel, ChainStop> {
    match policy {
        OrientationPolicy::DenseSweep => {
            let mut request = plinth_orient::registered_sweep_request().map_err(|source| {
                ChainStop::new(PipelineError::Orient {
                    model: model.clone(),
                    source,
                })
            })?;
            // The run's resource policy owns the work budget; the registry owns
            // every number that can change a score. Neither owns the other's.
            request.limits = limits;
            plinth_orient::search_and_orient(decimated, &request).map_err(|source| {
                ChainStop::new(PipelineError::Orient {
                    model: model.clone(),
                    source,
                })
            })
        }
        OrientationPolicy::Pinned(baselines) => {
            let Some(pinned) = baselines.get(model.as_str()) else {
                return Err(ChainStop::new(PipelineError::OrientationBaselineMissing {
                    model: model.clone(),
                }));
            };
            run_orientation(
                decimated,
                &OrientRequest {
                    pinned: pinned.clone(),
                },
            )
            .map_err(|source| {
                ChainStop::new(PipelineError::Orient {
                    model: model.clone(),
                    source,
                })
            })
        }
    }
}

/// Run the whole chain from an oriented model to a written artifact, taking the
/// real hollow stage and letting the sealed support run decide what it can
/// carry.
///
/// `Ok(None)` means the caller asked to stop before emission via `stop_after`.
///
/// **OQ-1 is answered, and not the way the design record guessed — and then, on
/// the ninth model, answered the design record's way after all.** OQ-1 expected
/// a corpus model might *refuse* to hollow, and recommended the typed
/// solid-decision fallback for that case. `plinth_hollow::run` does not refuse
/// the corpus Sphere at all: it hollows, plans drains, drills, and hands back
/// `AwaitingInternalSupport` — a proved drilled shell paired with the untouched
/// oriented solid. The refusal lands one stage later. The sealed support run
/// rejects that drilled shell during Preparation, so the drilled hollow is not
/// supportable by today's engine even though it is producible. **The Weeping
/// Treant then did exactly what OQ-1 guessed** (2026-08-09): it errored out of
/// `plinth_hollow::run` itself with `InvalidShellTopology`, and none of the
/// ineligibility codes that would have made that a typed `Solid`. So case 0
/// below is OQ-1's own recommendation, built at last. **That is history as of
/// 2026-08-11:** the well-composedness repair closed the treant's refusal and
/// it hollows. Case 0 stays, because the error class it answers has not gone
/// anywhere; what has gone is the corpus model that used to demonstrate it.
///
/// So this seam tries the real thing and records what happened:
///
/// 0. `Err` — the hollow stage neither hollowed nor decided solid. Fall back to
///    the pre-cloned oriented solid and report the typed refusal verbatim in
///    [`SupportedGeometry::HollowStageRefused`], which carries the provenance
///    caveat this inherits. The refusal itself is FU-34, not this seam's to fix.
/// 1. `Solid` — support planning consumes the hollow stage's own output.
/// 2. `AwaitingInternalSupport` — `approve_hollow` yields the drilled shell
///    with its true `HollowedAndDrilled` disposition (the two alternatives,
///    `into_solid_fallback` and `solid_decision`, would both stamp a named
///    verdict nobody reached). Note that A5 internal-support planning does not
///    exist in this workspace, so this shell carries no internal supports.
/// 3. If the sealed run refuses that shell, fall back to the oriented-solid
///    reserve through the same `solid_decision…(BelowThreshold)` handoff that
///    every sealed-run gate and the whole OD-8 census already use — the
///    design record's F14 caveat, inherited deliberately and reported in
///    [`SupportedGeometry`], not hidden. The **shell's** repair measurement
///    travels onto that reserve: the shell is discarded here and it held the
///    only other copy.
///
/// **Cases 0 and 3 are fallbacks and say so.** Neither is the path the pipeline
/// is meant to take; both exist so that a model the engine cannot yet hollow or
/// support as a shell still reaches an artifact, with the reason attached.
///
/// **A pre-write gate refusal is not one of them.** A gate stop is terminal
/// here: the reserve answers "this geometry could not be sealed", not "the
/// sealed plan does not adequately support the model". Retrying a gate refusal
/// against a different geometry would hand the owner a *substitute* artifact
/// for a model the engine just declared unsupported, which is the exact
/// substitution the gate exists to prevent.
///
/// # Errors
///
/// [`ChainStop`] carrying the typed stage error, and — when the pre-write gates
/// are what spoke — the gate and the geometry it judged.
pub fn run_from_oriented(
    model: &ModelId,
    source_faces: usize,
    oriented: OrientedModel,
    policy: &StagePolicy,
    target: &Path,
    stop_after: Option<StageStop>,
) -> Result<Option<ChainOutcome>, ChainStop> {
    // Hold the oriented solid before the hollow stage consumes it. `OrientedModel`
    // is move-only and exposes no from-parts constructor, so a pre-clone of its two
    // `Clone` halves is the only way a caller can keep a reserve across a hollow
    // *error* -- the `HollowedWithSolidReserve` reserve below exists only once the
    // stage has already succeeded. Both clones are dropped unread on every path
    // that does not error.
    let hollow_error_reserve_mesh = oriented.mesh().clone();
    let hollow_error_reserve_history = oriented.history().clone();

    let output = match hollow_stage(oriented, &policy.hollow) {
        Ok(output) => output,
        // OQ-1's anticipated case, reached for the first time by the Weeping
        // Treant (FU-34, then FU-49 -- which is fixed, and the treant now
        // hollows, so this arm currently has no corpus model behind it and is
        // gated on a pre-hollowed fixture instead). Carry the oriented solid
        // through the same
        // `solid_decision(BelowThreshold)` handoff the support-refusal path
        // below, `embed_incidence_census` and every sealed-run gate already
        // use, and record the typed refusal verbatim in `SupportedGeometry`
        // -- see `HollowStageRefused` for the provenance caveat this inherits.
        // A hollow error that is *not* a refusal of this mesh's geometry is
        // this model's failure, full stop. The reserve says "the engine could
        // not hollow this shape"; it cannot say "the operator asked for a wall
        // the stage will not cut" or "the constants registry would not read",
        // and emitting a solid for either would answer a bad request with a
        // plausible artifact instead of the error that names it.
        Err(failure) if !hollow_error_refuses_this_geometry(failure.error()) => {
            return Err(ChainStop::new(PipelineError::Hollow {
                model: model.clone(),
                source: failure.into_error(),
            }))
        }
        Err(failure) => {
            let refusal = failure.error().to_string();
            // The repair's measurement, when a repair had already run before
            // the refusal. Without it this record — the one that decides how
            // the model was treated — would say a repair never happened on
            // every refusal class except the two that name themselves.
            let cavity_repair = failure.cavity_repair();
            let solid = HollowedModel::solid_decision_recording_repair(
                hollow_error_reserve_mesh,
                hollow_error_reserve_history,
                SolidDecisionReason::BelowThreshold,
                cavity_repair,
            )
            .map_err(|source| {
                ChainStop::new(PipelineError::Hollow {
                    model: model.clone(),
                    source,
                })
            })?;
            if stop_after == Some(StageStop::Hollow) {
                return Ok(None);
            }
            return seal_and_emit(
                model,
                source_faces,
                solid,
                SupportedGeometry::HollowStageRefused { refusal },
                policy,
                target,
                stop_after,
            );
        }
    };

    let candidate = match output {
        HollowStageOutput::Solid(solid) => {
            if stop_after == Some(StageStop::Hollow) {
                return Ok(None);
            }
            return seal_and_emit(
                model,
                source_faces,
                *solid,
                SupportedGeometry::HollowStageDecidedSolid,
                policy,
                target,
                stop_after,
            );
        }
        HollowStageOutput::AwaitingInternalSupport(candidate) => candidate,
    };
    if stop_after == Some(StageStop::Hollow) {
        return Ok(None);
    }

    // Keep the reserve before consuming the pair, so a support refusal on the
    // drilled shell still has the oriented solid to fall back to. The repair's
    // measurement comes with it: `create_shell` succeeded on this model, so a
    // repair may well have run, and the substituted record is the one a reader
    // uses to decide how the model was treated.
    let reserve_mesh = candidate.reserve_mesh().clone();
    let reserve_history = candidate.reserve_history().clone();
    let reserve_cavity_repair = candidate.hollowed().history().hollow().cavity_repair();
    let drilled = (*candidate).approve_hollow();
    let drilled_faces = drilled.mesh().tris().len();

    match seal_and_emit(
        model,
        source_faces,
        drilled,
        SupportedGeometry::DrilledShellSealed,
        policy,
        target,
        stop_after,
    ) {
        Ok(outcome) => Ok(outcome),
        // **Only a support refusal reaches the reserve.** The gate speaks about
        // the plan rather than the geometry, and lowering, layer planning, a
        // degenerate extent and the emitter's post-write disk audit speak about
        // neither -- re-planning the oriented solid after one of those would
        // write a different object to the same path and label the unrelated
        // error a support refusal. Everything that is not a sealed-run or
        // planner refusal is this model's `Failed` disposition, unchanged.
        Err(stop) if !stop.is_support_refusal() => Err(stop),
        Err(stop) => {
            let solid = HollowedModel::solid_decision_recording_repair(
                reserve_mesh,
                reserve_history,
                SolidDecisionReason::BelowThreshold,
                reserve_cavity_repair,
            )
            .map_err(|source| {
                ChainStop::new(PipelineError::Hollow {
                    model: model.clone(),
                    source,
                })
            })?;
            seal_and_emit(
                model,
                source_faces,
                solid,
                SupportedGeometry::DrilledShellRefusedBySupport {
                    drilled_faces,
                    refusal: stop.detail(),
                },
                policy,
                target,
                stop_after,
            )
        }
    }
}

/// Whether a hollow-stage error is a refusal of **this mesh's geometry**, and
/// therefore one the oriented-solid reserve is a legitimate answer to.
///
/// OQ-1/FU-34 authorized the reserve for exactly one thing: a model whose shape
/// the hollow stage cannot turn into a shell (`InvalidShellTopology` and its
/// relatives). No corpus model reaches it today — the Weeping Treant, the one
/// that used to, hollows since the 2026-08-11 well-composedness repair — but
/// the classification below is about what an error *means*, not about which
/// model happens to raise it. It did not authorize laundering a *bad request*
/// or a *broken
/// environment* into a successful solid emission — an operator who passes a
/// parse-valid but inadmissible `--wall-mm`, or whose constants registry will
/// not publish, must get the error that names what they did, not an artifact
/// that looks like the run worked.
///
/// Matched exhaustively and deliberately without a `_` arm: a new `HollowError`
/// variant is a compile error here, so nobody gets to add one and have it
/// silently inherit the fallback.
fn hollow_error_refuses_this_geometry(error: &HollowError) -> bool {
    match error {
        // Configuration and infrastructure. None of these is a statement about
        // the mesh, and a solid artifact is not an answer to any of them.
        HollowError::Constants(_)
        | HollowError::PrinterProfileMismatch { .. }
        | HollowError::WallBelowVoxelPitch { .. }
        | HollowError::InvalidAdmissionLimits
        | HollowError::Kernel
        | HollowError::VoxelStack
        | HollowError::RasterWorkMismatch { .. }
        | HollowError::OutputIndexOverflow { .. }
        | HollowError::InvalidArithmetic => false,
        // Resource ceilings. FU-8b: a cap that was reached is a finding to
        // report, never a reason to take a different route to an artifact.
        HollowError::VoxelGridTooLarge
        | HollowError::ResourceAccountingOverflow
        | HollowError::VoxelCellLimitExceeded { .. }
        | HollowError::RasterWorkLimitExceeded { .. }
        | HollowError::RasterSubdivisionLimitExceeded
        | HollowError::TriageAdmission(_) => false,
        // The model does not fit the machine. Emitting the solid would hand
        // back an artifact for a print that cannot be started.
        HollowError::OutsidePrintableEnvelope { .. } => false,
        // Geometry refusals: the shape could not be hollowed, drained or
        // drilled. This is the reserve's own case, FU-34 included.
        HollowError::DrainPlanning(_)
        | HollowError::Drilling(_)
        | HollowError::Triage(_)
        | HollowError::StrictAudit(_)
        | HollowError::Topology(_)
        | HollowError::TopologyValidation(_)
        | HollowError::NotClosedOrOriented
        | HollowError::SolidMesh(_)
        | HollowError::Solid(_)
        | HollowError::OutputMesh(_)
        | HollowError::InvalidShellTopology
        | HollowError::BooleanVolumeMismatch { .. }
        | HollowError::CavityExtractionFailed
        | HollowError::DrainGeometryFailed
        | HollowError::InvalidProductionMesh => true,
        // Unreachable through `plinth_hollow::run`, which converts all five
        // into a typed `HollowStageOutput::Solid` rather than propagating them.
        // Listed as geometry anyway, because if one ever did propagate, "keep
        // the solid" is precisely what it means. The last two are the
        // well-composedness repair's own outcomes and reach the record as
        // `NoCavityAfterRepair` / `BelowThresholdAfterRepair`, which is how a
        // repair-caused solid stays distinguishable from a shape that never
        // had a cavity.
        HollowError::IneligibleBelowBulk
        | HollowError::IneligibleNoCavity
        | HollowError::IneligibleSavings
        | HollowError::CavityRepairRemovedCavity { .. }
        | HollowError::IneligibleSavingsAfterCavityRepair { .. } => true,
    }
}

/// Seal supports, judge the pre-write gates, lower, and emit one `.chitubox`.
///
/// Nothing here creates `target` until every gate has spoken.
///
/// # Errors
///
/// [`ChainStop`] carrying the typed stage error; a gate stop additionally
/// carries the gate and the geometry it judged, and leaves no file at `target`.
pub fn seal_and_emit(
    model: &ModelId,
    source_faces: usize,
    hollowed: HollowedModel,
    supported_geometry: SupportedGeometry,
    policy: &StagePolicy,
    target: &Path,
    stop_after: Option<StageStop>,
) -> Result<Option<ChainOutcome>, ChainStop> {
    let prepared_faces = hollowed.mesh().tris().len();
    let hollow_disposition = hollowed.history().hollow().disposition();
    // Read here, beside the disposition, because the hollowed model is the
    // last place the hollow stage's own report is in hand.
    let cavity_repair = hollowed.history().hollow().cavity_repair();

    let (min_z, max_z) = hollowed
        .mesh()
        .verts()
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), vertex| {
            (lo.min(vertex[2]), hi.max(vertex[2]))
        });
    if !min_z.is_finite() || !max_z.is_finite() {
        return Err(ChainStop::new(PipelineError::DegenerateExtent {
            model: model.clone(),
        }));
    }
    // The pitch and the lift are the profile's own registered constants, read
    // rather than restated. The `+ 1.0` headroom and the 1,000-layer floor are
    // the sealed-run gates' own long-standing plan shape, carried over verbatim
    // from the composition this module was promoted from.
    let layer_pitch_mm = *policy.printer().default_layer_height_mm().value();
    let span_mm = (max_z - min_z) + policy.support.lift_mm.get() + 1.0;
    let layer_count = ((span_mm / layer_pitch_mm).ceil() as usize).max(1_000);
    let layers = policy.printer().layer_plan(layer_count).map_err(|source| {
        ChainStop::new(PipelineError::LayerPlan {
            model: model.clone(),
            source,
        })
    })?;
    let delicacy_field = prepared_delicacy_field(hollowed.mesh()).map_err(|source| {
        ChainStop::new(PipelineError::Delicacy {
            model: model.clone(),
            source,
        })
    })?;
    let delicacy = delicacy_field
        .face_values()
        .iter()
        .map(|value| value.weight())
        .collect::<Vec<_>>();

    let input = policy.resources.support_input(&layers, &delicacy, &[]);
    // A refusal without the bounds it was measured against is not a
    // measurement; carry the model-scaled caps on the failure path, as the
    // census harness does.
    let scaled_caps = format!(
        "faces={prepared_faces} max_grid_bytes={} knn_max_work={}",
        input.max_grid_bytes(),
        input.minima_knn_max_work(),
    );

    let authority = run_support(hollowed, policy.support.clone(), input).map_err(|source| {
        ChainStop::new(PipelineError::SealedRun {
            model: model.clone(),
            source,
        })
        .measured_against(scaled_caps)
    })?;
    if stop_after == Some(StageStop::Support) {
        return Ok(None);
    }

    // The pre-write gate. `judge_then_emit` judges the sealed snapshot while
    // the authority is still alive, and only a passing verdict reaches
    // `into_supported_model` and `plinth_emit::write` -- so a refusal leaves
    // no file at `target` at all. There is no override parameter and there is
    // not going to be one.
    let gated = crate::judge_then_emit(
        model,
        authority,
        &EmitRequest {
            target_path: target.to_path_buf(),
            printer: policy.printer().clone(),
        },
    )
    .map_err(|error| match &error {
        // Carry the gate's own reason, the gate itself, and the geometry it
        // was refusing -- a refusal without the verdict behind it is not a
        // measurement, and a gate verdict without the geometry it judged
        // cannot be told apart from a verdict on the fallback.
        PipelineError::Gates { source, .. } => {
            let gate = source.gate();
            ChainStop::refused(
                error,
                GateStop {
                    gate,
                    geometry: supported_geometry.clone(),
                },
            )
        }
        _ => ChainStop::new(error),
    })?;

    Ok(Some(ChainOutcome {
        model: model.clone(),
        source_faces,
        prepared_faces,
        cavity_repair,
        hollow_disposition,
        supported_geometry,
        routed_contacts: gated.routed_contacts,
        final_live_members: gated.final_live_members,
        snapshot_digest: gated.snapshot_digest,
        pending_gates: gated.gates.pending().to_vec(),
        primitive_count: gated.primitive_count,
        lift_mm: gated.lift_mm,
        plate_z_mm: gated.plate_z_mm,
        emit: gated.emit,
    }))
}

impl From<ScaleSanity> for AssumedPrintSize {
    fn from(scale: ScaleSanity) -> Self {
        Self {
            longest_axis_mm: scale.longest_axis_mm,
            dimensions_mm: scale.dimensions_mm,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GateRefusal;

    /// **Only a geometry refusal may be answered with a solid.**
    ///
    /// The hollow fallback used to swallow every `HollowError` and emit the
    /// oriented solid as a success, so an operator who passed an inadmissible
    /// `--wall-mm`, or whose constants registry would not publish, got a
    /// plausible artifact instead of the error naming what they did. This pins
    /// the classification on one representative of each class; the match itself
    /// is exhaustive with no `_` arm, so a new variant is a compile error and
    /// cannot inherit the fallback by default.
    #[test]
    fn only_a_geometry_refusal_reaches_the_oriented_solid_reserve() {
        // Configuration: the operator's own request, or the machine selection.
        assert!(!hollow_error_refuses_this_geometry(
            &HollowError::WallBelowVoxelPitch {
                wall_mm: 0.25,
                voxel_pitch_mm: 0.5,
            }
        ));
        assert!(!hollow_error_refuses_this_geometry(
            &HollowError::PrinterProfileMismatch {
                expected: "saturn4-ultra".to_owned(),
                actual: "other".to_owned(),
            }
        ));
        // Infrastructure.
        assert!(!hollow_error_refuses_this_geometry(&HollowError::Kernel));
        // A reached resource cap is a finding, never a route to an artifact
        // by another path (FU-8b).
        assert!(!hollow_error_refuses_this_geometry(
            &HollowError::VoxelCellLimitExceeded {
                cells: 2,
                max_cells: 1,
            }
        ));
        assert!(!hollow_error_refuses_this_geometry(
            &HollowError::VoxelGridTooLarge
        ));
        // A model that does not fit the machine is not answered with a solid
        // that also does not fit it.
        assert!(!hollow_error_refuses_this_geometry(
            &HollowError::OutsidePrintableEnvelope { axis: 2 }
        ));

        // The case OQ-1/FU-34 authorized the reserve for, and its relatives.
        assert!(hollow_error_refuses_this_geometry(
            &HollowError::InvalidShellTopology
        ));
        assert!(hollow_error_refuses_this_geometry(
            &HollowError::NotClosedOrOriented
        ));
        assert!(hollow_error_refuses_this_geometry(
            &HollowError::CavityExtractionFailed
        ));
        // The well-composedness repair's own outcomes. `run` converts both into
        // typed solid decisions before they reach here, so this arm is
        // unreachable in practice — but if either ever did propagate, "keep the
        // solid" is exactly what it means, and the alternative classification
        // would turn a repaired-away cavity into a hard failure of the model.
        assert!(hollow_error_refuses_this_geometry(
            &HollowError::CavityRepairRemovedCavity {
                critical_cells: 1,
                repaired_voxels: 2,
                cell_visits: 27,
                full_grid_passes: 2,
            }
        ));
        assert!(hollow_error_refuses_this_geometry(
            &HollowError::IneligibleSavingsAfterCavityRepair {
                critical_cells: 1,
                repaired_voxels: 2,
                cell_visits: 27,
                full_grid_passes: 2,
            }
        ));
    }

    /// **Only a support refusal reaches the reserve either.**
    ///
    /// The guard used to be "anything that is not a gate stop", so a refusal by
    /// the emitter's post-write disk audit, by lowering, or by layer planning
    /// re-planned a *different geometry*, wrote it to the same target, and
    /// labelled the unrelated error a support refusal.
    #[test]
    fn only_a_sealed_run_or_planner_refusal_is_a_support_refusal() {
        let model = ModelId::new("fixture");
        let stop = |error| ChainStop::new(error);

        assert!(stop(PipelineError::SealedRun {
            model: model.clone(),
            source: plinth_support::SupportRunError::InvalidResourceCaps,
        })
        .is_support_refusal());

        for error in [
            PipelineError::DegenerateExtent {
                model: model.clone(),
            },
            PipelineError::Gates {
                model: model.clone(),
                source: GateRefusal::ConstantsUnavailable,
            },
            PipelineError::Hollow {
                model: model.clone(),
                source: HollowError::InvalidShellTopology,
            },
        ] {
            let stop = stop(error);
            assert!(
                !stop.is_support_refusal(),
                "{} must not be answered with a substitute geometry",
                stop.detail()
            );
        }
    }

    /// **A coverage-closure refusal (FU-56) is excluded from
    /// `is_support_refusal`, and is its own named class.**
    ///
    /// The sealed run built and routed a real plan for the drilled shell and
    /// could not close its own coverage promise within the registered attempt
    /// bound; re-planning the oriented solid after it would substitute a
    /// different artifact for a refusal that is about the plan, not the
    /// geometry — the same reasoning `only_a_sealed_run_or_planner_refusal_is_a_support_refusal`
    /// already documents for a pre-write gate stop.
    #[test]
    fn a_coverage_closure_refusal_is_named_and_excluded_from_support_refusal() {
        let model = ModelId::new("fixture");
        let refusal = plinth_support::CoverageClosureRefusal {
            attempts: 8,
            reason: plinth_support::CoverageClosureReason::AttemptsExhausted,
            islands: Vec::new(),
            history: Vec::new(),
        };
        let stop = ChainStop::new(PipelineError::SealedRun {
            model,
            source: plinth_support::SupportRunError::CoverageClosure(Box::new(refusal)),
        });
        assert!(stop.is_coverage_closure_refusal());
        assert!(
            !stop.is_support_refusal(),
            "a coverage-closure refusal must not be answered with a substitute geometry"
        );
    }

    /// The target is cleared idempotently, and a target that cannot be cleared
    /// is reported rather than swallowed.
    #[test]
    fn clearing_a_target_is_idempotent_and_a_failure_is_not_swallowed() {
        let root = std::env::temp_dir().join(format!("plinth-clear-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("scratch");

        // Absent is not an error; the ordinary case is a first run.
        let absent = root.join("absent.chitubox");
        assert!(clear_stale_artifact(&absent).is_ok());

        // Present is removed, and removing it again is still fine.
        let present = root.join("present.chitubox");
        std::fs::write(&present, b"an earlier run").expect("plant");
        assert!(clear_stale_artifact(&present).is_ok());
        assert!(!present.exists());
        assert!(clear_stale_artifact(&present).is_ok());

        // A directory sitting on the target path still occupies it, and that
        // is reported, not ignored.
        let occupied = root.join("occupied.chitubox");
        std::fs::create_dir_all(&occupied).expect("plant a directory");
        assert!(clear_stale_artifact(&occupied).is_err());

        let _ = std::fs::remove_dir_all(&root);
    }
}
