//! The production `plinth prepare` executor: a folder of `.stl` in, one
//! `.chitubox` per model out, one typed disposition per model either way.
//!
//! Until this module existed the whole end-to-end composition lived in
//! `examples/shared/first_artifact_chain.rs`, where it was reachable only from
//! a test and an artifact-producing example. `crates/plinth-cli` parsed the
//! ratified command line and then printed "execution is not implemented".
//! Design record E4/E5 (`docs/architecture.md`)
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

use plinth_core::{Finite, ModelId, PositiveFinite, PrinterProfile};
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
use plinth_support::{run_support, SupportRequest, SupportRunError};

use crate::coverage_evidence::{self, RenderMesh};
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
/// `docs/architecture.md`.
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
    /// not a measurement. `stage` is the same fact as a value rather than a
    /// substring of that message, so a reader (or a downstream tool) can tell
    /// "the planner never got started" from "the sealed run died in Lowering"
    /// without parsing prose.
    ///
    /// `stage` is `Option` for wire compatibility only: a report or coverage
    /// sidecar serialized before the field existed still deserializes, with
    /// `None` meaning "this record predates the field", never "no stage
    /// refused it". Every value this pipeline writes is `Some`.
    DrilledShellRefusedBySupport {
        drilled_faces: usize,
        #[serde(default)]
        stage: Option<ShellRefusalStage>,
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
    /// **The provenance caveat, stated rather than hidden (the F14 pattern),
    /// and now closed.** The reserve is taken through
    /// `HollowedModel::solid_decision_recording_repair`, recording
    /// [`plinth_hollow::SolidDecisionReason::HollowStageRefused`] — the
    /// reason that says what happened, where this used to stamp
    /// `BelowThreshold` and claim a savings measurement nobody took. (The
    /// constructor still carries the repair's own measurement across the
    /// error, taken off the `HollowStageFailure`, so this record cannot claim
    /// a repair never ran.)
    ///
    /// **This path now shares the remedy the support-refusal path above
    /// carries.** That path records
    /// [`plinth_hollow::SolidDecisionReason::DrilledShellRefused`] for a shell
    /// that existed and was refused later; this one records
    /// [`plinth_hollow::SolidDecisionReason::HollowStageRefused`] for a shell
    /// that never existed at all, because the hollow stage's own `run` call
    /// errored. Distinct reasons for distinct facts, so a reader (or a
    /// downstream tool) matching on `SolidDecisionReason` alone can no longer
    /// conflate the two. **This variant is where the detail lives.**
    /// `refusal` is the typed `HollowError`'s own `Display`, verbatim and
    /// unedited, so the artifact's real provenance is recoverable from the
    /// outcome even though `HollowStageRefused` alone has no room for it.
    HollowStageRefused { refusal: String },
}

/// Which stage refused a drilled shell, as a value rather than a substring of
/// a rendered message.
///
/// Only a support refusal ever reaches the oriented-solid reserve (see
/// [`ChainStop::is_support_refusal`]), so these are the only two stages that
/// can speak here, and a stop that is not a support refusal has no stage of
/// this kind at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum ShellRefusalStage {
    /// The support planner refused the shell — [`PipelineError::Support`].
    SupportPlanning,
    /// The sealed support run refused it — [`PipelineError::SealedRun`].
    /// `phase` is the runner's own phase when the refusal named one; it is
    /// `None` for the two setup refusals (`InvalidDerivedFrame`,
    /// `InvalidResourceCaps`), which happen before any phase runs.
    SealedRun {
        #[serde(default)]
        phase: Option<plinth_support::SupportRunPhase>,
    },
}

impl std::fmt::Display for ShellRefusalStage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SupportPlanning => formatter.write_str("support planning"),
            Self::SealedRun { phase: Some(phase) } => {
                write!(formatter, "the sealed support run during {phase:?}")
            }
            Self::SealedRun { phase: None } => formatter.write_str("the sealed support run"),
        }
    }
}

/// A drilled shell that **existed** and was refused, kept so the fact survives
/// whatever happens to the oriented solid that replaced it.
///
/// The substitution succeeds far more often than it fails, and when it
/// succeeds [`SupportedGeometry::DrilledShellRefusedBySupport`] carries this
/// story into the report and the coverage sidecar. When it *fails* — the
/// substituted solid refused or errored in its own right — that geometry
/// record dies with the outcome, and without this the final error says only
/// what the solid did. A reader of the Giant Bat's, the treant's or the
/// swallowing toad's report then cannot tell whether a shell was ever built,
/// which is the difference between "this model cannot be hollowed" and "this
/// model was hollowed and the support engine could not carry it".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrilledShellRefusal {
    /// The face count of the shell that was actually built and drilled.
    pub drilled_faces: usize,
    /// Where it was refused.
    pub stage: ShellRefusalStage,
    /// That refusal's own rendered detail, bounds included.
    pub detail: String,
}

impl DrilledShellRefusal {
    /// Read the record off the stop that refused the shell.
    ///
    /// # Panics
    ///
    /// Never in this pipeline: the only caller is the substitution seam, which
    /// is reached only when [`ChainStop::is_support_refusal`] holds, and every
    /// such stop has a stage. A stop without one would mean the guard and this
    /// function disagree about what a support refusal is, and a record naming
    /// the wrong stage is worse than no record.
    #[must_use]
    fn from_stop(drilled_faces: usize, stop: &ChainStop) -> Self {
        let stage = stop
            .shell_refusal_stage()
            .expect("only a support refusal reaches the oriented-solid reserve");
        // **No SPEC-A4 §A.8 spend travels here, and that is a measurement
        // rather than an omission.** The substitution seam does run the
        // repair controller twice — once on the drilled shell, then again on
        // the oriented solid — but no stop that reaches this function can
        // carry a repair ledger, by construction:
        //
        // * a stop reaches here only when [`ChainStop::is_support_refusal`]
        //   holds, and that predicate explicitly EXCLUDES
        //   `SupportRunError::CoverageClosure` — the one refusal class the
        //   controller attaches a ledger to. A coverage-closure refusal of
        //   the shell is terminal and keeps its own report;
        // * every other class it admits (`Support`, and `SealedRun` with
        //   `Failed` / `InvalidDerivedFrame` / `InvalidResourceCaps`) is a
        //   failure at or before attempt 0, where the repair loop was never
        //   entered and no ledger exists to lose;
        // * and a shell that SEALED and then failed to lower or emit is not
        //   a support refusal either, so it keeps its own ledger on its own
        //   `Failed` disposition.
        //
        // `debug_assert` rather than prose alone, so the day one of those
        // three facts changes this stops being true quietly.
        debug_assert!(
            stop.repair_report().is_none(),
            "a stop carrying a repair ledger reached the substitution seam; the shell's \
             §A.8 spend would be dropped here"
        );
        Self {
            drilled_faces,
            stage,
            detail: stop.detail(),
        }
    }

    /// The sentence carried into a later failure's evidence.
    #[must_use]
    fn sentence(&self) -> String {
        format!(
            "a hollow shell was built and drilled ({} faces) and refused at {}: {}; \
             the oriented solid was substituted for it and then failed as stated above",
            self.drilled_faces, self.stage, self.detail
        )
    }
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
    /// FU-57: the sidecar/card/render evidence, when this stop is a coverage
    /// refusal from either source. Attached by [`Self::with_evidence`] at the
    /// point in `seal_and_emit` where the refusal is first known, and
    /// projected into the report by [`failure`]. Boxed: `CoverageRefusalEvidence`
    /// carries the full ranked island list, so an unboxed field would put
    /// that weight on every `ChainStop`, including the overwhelming majority
    /// that never carry one.
    evidence: Option<Box<crate::coverage_evidence::CoverageRefusalEvidence>>,
    /// The drilled shell this model had, when a shell was built and refused
    /// before the geometry behind this stop was tried. Attached by
    /// [`Self::after_refused_shell`] on the substitution seam's own error
    /// paths, and rendered by [`Self::detail`] so it reaches the report
    /// whichever disposition the second failure lands in. Boxed for
    /// [`Self::evidence`]'s reason: it carries a rendered refusal string, and
    /// the overwhelming majority of stops never carry one.
    refused_shell: Option<Box<DrilledShellRefusal>>,
    /// **SPEC-A4 §A.8's third destination, on the refused path.** What the
    /// one-winner coverage-repair controller spent before this stop was
    /// reached: trials, commits, the rejection histogram by class, the
    /// per-trial wall-time order statistics, and rule N4's demotion pair.
    ///
    /// Ruling R10 deleted the failed-tries cap and put loud reporting in its
    /// place, and §A.8 says the report goes to three places **on every path —
    /// sealed or refused**. The refused path is the expensive one: it is where
    /// the pool runs all the way out (45 one-row transactions on the raking
    /// blade, committing none). A report that only reaches the sealed path
    /// leaves that spend nowhere a reader can compare it, and the trade R10
    /// made would not hold.
    ///
    /// Attached by [`Self::with_repair_report`] at the two points in
    /// `seal_and_emit` where a stop is first known to follow a controller run,
    /// and projected into `Refused` / `Failed` by [`failure`]. `None` means no
    /// controller ledger exists for this stop — a model that failed before or
    /// at attempt 0 never entered the repair loop, and claiming `trials=0`
    /// for it would say "the loop ran and spent nothing" about a loop that
    /// never ran. Boxed for [`Self::evidence`]'s reason.
    repair_report: Option<Box<plinth_support::RepairRunReport>>,
}

impl ChainStop {
    #[must_use]
    pub fn new(error: PipelineError) -> Self {
        Self {
            error: Box::new(error),
            gate: None,
            measured_against: None,
            evidence: None,
            refused_shell: None,
            repair_report: None,
        }
    }

    #[must_use]
    pub fn refused(error: PipelineError, gate: GateStop) -> Self {
        Self {
            error: Box::new(error),
            gate: Some(gate),
            measured_against: None,
            evidence: None,
            refused_shell: None,
            repair_report: None,
        }
    }

    /// Attach the model-scaled ceilings a stage refused under.
    #[must_use]
    pub fn measured_against(mut self, bounds: String) -> Self {
        self.measured_against = Some(bounds);
        self
    }

    /// Attach the FU-57 coverage-refusal evidence this stop carries.
    #[must_use]
    pub fn with_evidence(
        mut self,
        evidence: crate::coverage_evidence::CoverageRefusalEvidence,
    ) -> Self {
        self.evidence = Some(Box::new(evidence));
        self
    }

    /// The coverage-refusal evidence attached to this stop, when any.
    #[must_use]
    pub fn evidence(&self) -> Option<&crate::coverage_evidence::CoverageRefusalEvidence> {
        self.evidence.as_deref()
    }

    /// Attach what SPEC-A4 §A.8's repair controller spent before this stop.
    ///
    /// Takes the `Option` rather than the value, because the caller's question
    /// is "does a controller ledger exist for this stop" and answering it at
    /// the call site is what keeps "no ledger" from being written as an
    /// all-zero one.
    #[must_use]
    pub fn with_repair_report(
        mut self,
        repair_report: Option<plinth_support::RepairRunReport>,
    ) -> Self {
        self.repair_report = repair_report.map(Box::new);
        self
    }

    /// What SPEC-A4 §A.8's repair controller spent before this stop, when a
    /// controller ran at all.
    #[must_use]
    pub fn repair_report(&self) -> Option<&plinth_support::RepairRunReport> {
        self.repair_report.as_deref()
    }

    /// Record that a drilled shell was built and refused before the geometry
    /// behind this stop was tried.
    ///
    /// Attached on **every** error path of the substitution seam, so a
    /// substituted solid that fails in its own right cannot report as a model
    /// that was never hollowed at all.
    #[must_use]
    pub fn after_refused_shell(mut self, refused_shell: DrilledShellRefusal) -> Self {
        self.refused_shell = Some(Box::new(refused_shell));
        self
    }

    /// The drilled shell that was built and refused before this stop's
    /// geometry was tried, when there was one.
    #[must_use]
    pub fn refused_shell(&self) -> Option<&DrilledShellRefusal> {
        self.refused_shell.as_deref()
    }

    /// Which stage refused this model, when the stop is a support refusal —
    /// the only class the oriented-solid reserve answers.
    ///
    /// Derived from the typed error rather than from its rendered message, so
    /// a reworded `Display` cannot silently change the recorded stage. `None`
    /// for every other stop, which by [`Self::is_support_refusal`] cannot
    /// reach the reserve at all.
    #[must_use]
    pub fn shell_refusal_stage(&self) -> Option<ShellRefusalStage> {
        if !self.is_support_refusal() {
            return None;
        }
        match &*self.error {
            PipelineError::Support { .. } => Some(ShellRefusalStage::SupportPlanning),
            PipelineError::SealedRun { source, .. } => Some(ShellRefusalStage::SealedRun {
                phase: match source {
                    plinth_support::SupportRunError::Failed { phase, .. } => Some(*phase),
                    plinth_support::SupportRunError::InvalidDerivedFrame
                    | plinth_support::SupportRunError::InvalidResourceCaps
                    | plinth_support::SupportRunError::CoverageClosure(_) => None,
                },
            }),
            _ => None,
        }
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
    /// `Emit` (a write failure, or a written file that cannot be read back), a `LayerPlan` or
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
            PipelineError::SealedRun { source, .. } => {
                !matches!(source, plinth_support::SupportRunError::CoverageClosure(_))
            }
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
    /// carry, which `Display` alone drops — and, when a drilled shell was
    /// built and refused before this geometry was tried, that fact beside it.
    ///
    /// The shell record is appended here rather than at the report boundary
    /// because [`failure`] projects a stop into `Refused { violation }` or
    /// `Failed { detail }` through this one function: appending it here is
    /// what makes the record reach the run report on **both** dispositions,
    /// and what makes it impossible to add a third and lose it again.
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
        if let Some(refused_shell) = &self.refused_shell {
            rendered.push_str(" -- ");
            rendered.push_str(&refused_shell.sentence());
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
    /// FU-37: the wall this model's hollow attempt requested, read off the
    /// same `HollowReport` as `cavity_repair` and for the same reason —
    /// `seal_and_emit` is the last place that report is in hand, and a
    /// measurement that stops here reaches no run output at all.
    pub wall: Option<plinth_core::Finite>,
    /// FU-55: what the shell actually delivered against that wall, when a
    /// shell was built and measured, with its
    /// [`plinth_hollow::RealizedWallClaim`] attached.
    pub realized_wall: Option<plinth_hollow::RealizedWall>,
    /// 2026-08-13 owner ruling: when this model's hollow disposition is
    /// `SolidDecisionReason::BelowThreshold` or
    /// `BelowThresholdAfterRepair`, what the cavity would have saved and the
    /// registered minimum it fell short of — read off the same
    /// `HollowReport` as `wall` and `realized_wall`, and for the same
    /// reason. `None` on every other disposition, including a hollowed one.
    pub savings_shortfall: Option<plinth_hollow::SavingsShortfall>,
    /// Every cavity pocket the drilling stage filled back to solid because no
    /// legal drain reached it, in millilitres. Read off the same
    /// `HollowReport` as `cavity_repair`, for the same reason: this is the
    /// last place that report is in hand, and a loss nobody can see is a loss
    /// nobody will fix.
    pub filled_back_chambers_ml: Vec<plinth_core::Finite>,
    pub supported_geometry: SupportedGeometry,
    /// Task #49: the islands this run's accepted contact set left with no
    /// owning contact, carried out to the run report rather than left inside
    /// the support crate where nothing could print it.
    pub ownerless_islands: Vec<plinth_support::OwnerlessIslandDisclosure>,
    /// **SPEC-A4 §A.8's third destination, and §G.1 stage 4's pipeline
    /// wiring.** What the one-winner coverage-repair controller spent on this
    /// model: trials, commits, the rejection histogram by class, the wall time,
    /// and rule N4's demotion pair. Carried out for the same reason as the
    /// field above — the trade ruling R10 made (no failed-tries cap, loud
    /// reporting instead) only holds while the cost reaches a reader.
    ///
    /// **`Some` on every model the production door seals**, including one that
    /// needed no repair at all, whose report reads `trials=0 commits=0`. That
    /// is the marker: `None` here means the sealed run came from a controller
    /// that keeps no repair ledger, which since stage 4 is no production path.
    pub repair_report: Option<plinth_support::RepairRunReport>,
    /// **SPEC-A4 §D.2 on the SEALED path** — where this model's exact
    /// attribution was written, and whether the write succeeded.
    ///
    /// The file itself carries the stream; this row is the receipt, so a reader
    /// of the run report alone knows the path to open and whether anything
    /// stopped it from being written. `None` only on a report shape that never
    /// reached a sealed run.
    pub repair_ledger: Option<crate::coverage_evidence::SealedRepairLedger>,
    /// PR-8 grow-to-attest: the contacts this run refused with the load each
    /// abandons, and the contacts whose tips it grew to keep them. Carried out
    /// to the run report for the same reason as the field above -- a support
    /// that vanished with nobody told is exactly what this closes.
    pub abandoned_contacts: Vec<plinth_support::AbandonedContactDisclosure>,
    pub grown_contact_tips: Vec<plinth_support::GrownContactTipDisclosure>,
    pub routed_contacts: usize,
    pub final_live_members: usize,
    pub snapshot_digest: u64,
    /// The pre-write gates that are named and consulted but not yet ported.
    /// Carried out rather than described: the artifact below was authorized by
    /// one gate out of twelve, and this is the list of the other eleven.
    pub pending_gates: Vec<PreWriteGate>,
    /// [`PreWriteGate::Load`]'s WARNING MODE evidence (owner ruling,
    /// 2026-08-29), when it found a contact whose shipped tip does not
    /// satisfy the registered support area law. `None` means the gate found
    /// nothing to report -- it is never `None` because the gate did not run;
    /// while `Load` is ported it always reaches a verdict. Carried out here
    /// for the same reason `pending_gates` is: an artifact that emitted with
    /// live area-law violations recorded against it is a different artifact
    /// from one that emitted clean, and nothing else in the report would say
    /// so.
    pub load_warnings: Option<crate::gates::LoadWarnings>,
    /// **WARNING MODE (owner ruling, 2026-09-01).** What the coverage gate
    /// measured this artifact as leaving unsupported, and why the repair
    /// search stopped. `None` means the coverage verdict was accepted.
    pub coverage_warning: Option<crate::coverage_evidence::CoverageGapWarning>,
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
    /// Which support engine this run uses. See [`crate::Placement`].
    pub placement: crate::Placement,
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
            placement: request.placement,
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
                orientation: None,
                outcome: ModelPreparationOutcome::Failed {
                    exit_code: PipelineExitCode::Input,
                    detail: format!(
                        "two source models in this folder claim the artifact path {}",
                        target.display()
                    ),
                    evidence: None,
                    // Refused by name before it was opened: no stage ran, so
                    // no controller ran, so there is nothing §A.8 could
                    // report. `None` is the measurement here.
                    repair_report: None,
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
    // Every disposition BELOW the orientation stage: the model never got a
    // rotation, so the field says the record does not hold one rather than
    // claiming identity, which is a rotation the stage can also choose.
    let report = |repair: &Option<plinth_mesh::RepairEvidence>, assumed_print_size, outcome| {
        ModelPreparationReport {
            model: model.clone(),
            source_path: source_path.to_path_buf(),
            assumed_print_size,
            repair: repair.clone(),
            orientation: None,
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

    // **FU-57: the same discipline extended to coverage-refusal evidence.**
    // A `<model>.coverage.json` and its renders from a run that refused this
    // model are read by the next reader as *this* run's answer the moment
    // this run's own disposition sits beside them without touching them --
    // the exact misleading hand-off `clear_stale_artifact` above exists to
    // prevent, and evidence that a model now emits cleanly must not survive
    // to sit beside its fresh artifact. A removal that fails fails the model
    // by name, the same as a stale artifact does.
    if let Err(source) = clear_stale_coverage_evidence(target) {
        return report(
            &repair,
            None,
            failure(&ChainStop::new(PipelineError::StaleCoverageEvidence {
                model: model.clone(),
                path: target.to_path_buf(),
                source,
            })),
        );
    }

    // **The judge's streaming-silence note, model side.** Everything from here
    // to the support run's own first line is silent today: load, mesh repair,
    // decimation, the dense orientation sweep and the hollow-and-drill stage.
    // MEASURED at `e9e160f`: `Giant Bat.stl` printed nothing to stderr for
    // 75.77 s and then failed, and on the big models the first line an operator
    // sees is the repair loop's, four to five minutes in.
    //
    // Print-only, through `plinth-support`'s own flushed write so a killed run
    // has still said where it got to. No branch reads a stage line; the elapsed
    // it carries is wall time, which is evidence and never a bound.
    let clock = plinth_support::StageClock::start(format!("model {model}"));
    clock.entering("Load");
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
    // (`docs/architecture.md`).
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
    clock.entering("MeshRepair");
    let (mesh, evidence) = plinth_mesh::repair_mesh(&mesh).into_parts();
    repair = Some(evidence);

    clock.entering("Decimation");
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

    clock.entering("Orientation");
    let oriented = match orient_model(
        model,
        decimated,
        &policy.orientation,
        policy.resources.orient(),
    ) {
        Ok(oriented) => oriented,
        Err(stop) => return report(&repair, assumed, failure(&stop)),
    };
    // **The chosen orientation, read once and carried on every disposition
    // from here down.** The closure is SHADOWED rather than given a fourth
    // parameter so that the eleven call sites above and below stay as they
    // are, and so that "no orientation was chosen yet" is enforced by which
    // closure is in scope instead of by remembering to pass `None`.
    let orientation = Some(crate::OrientationDisclosure::from_report(
        oriented.history().orientation(),
    ));
    let report = |repair: &Option<plinth_mesh::RepairEvidence>, assumed_print_size, outcome| {
        ModelPreparationReport {
            model: model.clone(),
            source_path: source_path.to_path_buf(),
            assumed_print_size,
            repair: repair.clone(),
            orientation: orientation.clone(),
            outcome,
        }
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
        Ok(Some(outcome)) => report(&repair, assumed, artifact_outcome(outcome)),
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

/// Remove any FU-57 coverage-refusal evidence left at one model's target from
/// an earlier run: the sidecar `<model>.coverage.json` and every
/// `<model>.coverage.island-*.svg`.
///
/// **Matched by prefix/suffix on the directory listing, not shell-style
/// globbing** -- a model name may itself contain glob metacharacters (`*`,
/// `[`, `]`), and this must still match exactly the files
/// [`coverage_evidence::render_path`] can have written for this target and
/// nothing else.
///
/// `NotFound` is the ordinary case (no earlier refusal, or an output
/// directory that does not exist yet) and is not an error; any other io
/// error is, for the same reason `clear_stale_artifact` treats one as fatal
/// to the model: evidence from a run this model's own disposition is about
/// to sit beside is read as this run's answer.
fn clear_stale_coverage_evidence(target: &Path) -> Result<(), std::io::Error> {
    let sidecar = coverage_evidence::sidecar_path(target);
    match std::fs::remove_file(&sidecar) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    // **SPEC-A4 §D.2's sealed-run twin, cleared for exactly the reason the
    // sidecar above is.** `<model>.coverage.repair.json` names which contact
    // closed which hole on the run that wrote it. Left behind, the next reader
    // takes one run's attribution for another's — and a repair ledger is more
    // misleading stale than a refusal is, because it describes a plan that was
    // printed.
    let ledger = coverage_evidence::repair_ledger_path(target);
    match std::fs::remove_file(&ledger) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let Some(parent) = target.parent() else {
        return Ok(());
    };
    let Some(prefix) = coverage_evidence::render_file_prefix(target) else {
        return Ok(());
    };
    let entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name.starts_with(&prefix) && name.ends_with(".svg") {
            match std::fs::remove_file(entry.path()) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
    }
    Ok(())
}

/// Project a stop into the report's serializable disposition.
///
/// **SPEC-A4 §A.8's third destination travels through here on the refused
/// path**, into whichever of the two non-emitting dispositions the stop lands
/// in — a pre-write gate refusal is `Refused`, a controller refusal or a
/// post-seal lowering/emit failure is `Failed`, and the controller spent the
/// same trials either way.
fn failure(stop: &ChainStop) -> ModelPreparationOutcome {
    let repair_report = stop.repair_report().cloned().map(Box::new);
    if stop.is_gates() {
        ModelPreparationOutcome::Refused {
            gate: stop.gate(),
            geometry: stop.refused_geometry().cloned(),
            violation: stop.detail(),
            evidence: stop.evidence().cloned().map(Box::new),
            repair_report,
        }
    } else {
        ModelPreparationOutcome::Failed {
            exit_code: stop.exit_code(),
            detail: stop.detail(),
            evidence: stop.evidence().cloned().map(Box::new),
            repair_report,
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
            // LANE 3 (docs/plans/slice-sweep-core-2026-09-02.md §12): the
            // orientation stage is shared by both `--placement` engines, so
            // wiring the real sweep-demand measurer here fixes the double-
            // charged-flat-pose defect for legacy and sweep placement alike.
            let demand = plinth_sweep::SweepDemand::production();
            plinth_orient::search_and_orient(decimated, &request, Some(&demand)).map_err(|source| {
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
///    the pre-cloned oriented solid through `solid_decision…`, recording
///    [`SolidDecisionReason::HollowStageRefused`] — the reason that says what
///    happened, where this used to stamp `BelowThreshold` and claim a savings
///    measurement nobody took — and report the typed refusal verbatim in
///    [`SupportedGeometry::HollowStageRefused`]. The refusal itself is FU-34,
///    not this seam's to fix.
/// 1. `Solid` — support planning consumes the hollow stage's own output.
/// 2. `AwaitingInternalSupport` — `approve_hollow` yields the drilled shell
///    with its true `HollowedAndDrilled` disposition (the two alternatives,
///    `into_solid_fallback` and `solid_decision`, would both stamp a named
///    verdict nobody reached). Note that A5 internal-support planning does not
///    exist in this workspace, so this shell carries no internal supports.
/// 3. If the sealed run refuses that shell, fall back to the oriented-solid
///    reserve through `solid_decision…`, recording
///    [`SolidDecisionReason::DrilledShellRefused`] — the reason that says what
///    happened, where this used to stamp `BelowThreshold` and claim a savings
///    measurement nobody took. The stage that refused the shell and its own
///    detail are reported in [`SupportedGeometry`], not hidden. The **shell's**
///    repair measurement travels onto that reserve: the shell is discarded here
///    and it held the only other copy. If the substituted solid then fails or
///    is refused in its own right, the shell's refusal travels on the failure
///    too ([`ChainStop::after_refused_shell`]), so "was this model ever
///    hollowed?" stays answerable from the report.
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

    // Its own clock, scoped to the hollow-and-drill stage it times. This is the
    // longest silent stretch on a big model before the sealed run starts.
    let clock = plinth_support::StageClock::start(format!("model {model}"));
    clock.entering("Hollow");
    let output = match hollow_stage(oriented, &policy.hollow) {
        Ok(output) => output,
        // OQ-1's anticipated case, reached for the first time by the Weeping
        // Treant (FU-34, then FU-49 -- which is fixed, and the treant now
        // hollows, so this arm currently has no corpus model behind it and is
        // gated on a pre-hollowed fixture instead). Carry the oriented solid
        // through `solid_decision(HollowStageRefused)`, recording the typed
        // refusal verbatim in `SupportedGeometry::HollowStageRefused` --
        // where this used to stamp `BelowThreshold` and claim a savings
        // measurement nobody took, the same imprecision the support-refusal
        // path below no longer carries either.
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
            // FU-37: the wall this attempt requested, carried onto the
            // reserve exactly as the repair measurement is above.
            let wall = Some(
                Finite::new(policy.hollow.wall_mm.get())
                    .expect("a PositiveFinite wall is always finite"),
            );
            let solid = substitute_solid_for_hollow_stage_failure(
                model,
                hollow_error_reserve_mesh,
                hollow_error_reserve_history,
                cavity_repair,
                wall,
            )?;
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
    // FU-37: the wall this drilled shell was requested at, carried onto the
    // reserve the same way `reserve_cavity_repair` is just above.
    let reserve_wall = candidate.hollowed().history().hollow().wall();
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
            // **The shell existed, and everything after this line is about the
            // solid.** Take the record of what happened to the shell first, as
            // a value: this stop is the last thing that knows a shell was
            // built at all, and both of the paths below can lose it -- the
            // successful one by emitting a solid, the failing one by returning
            // the solid's own error.
            let refused_shell = DrilledShellRefusal::from_stop(drilled_faces, &stop);
            let solid = substitute_solid_for_refused_shell(
                model,
                reserve_mesh,
                reserve_history,
                reserve_cavity_repair,
                reserve_wall,
                &refused_shell,
            )?;
            seal_and_emit(
                model,
                source_faces,
                solid,
                SupportedGeometry::DrilledShellRefusedBySupport {
                    drilled_faces,
                    stage: Some(refused_shell.stage),
                    refusal: refused_shell.detail.clone(),
                },
                policy,
                target,
                stop_after,
            )
            // The substituted solid failed or was refused in its own right.
            // Without this the report would say only what the *solid* did, and
            // whether this model was ever hollowed would not be recoverable
            // from the run at all.
            .map_err(|stop| stop.after_refused_shell(refused_shell))
        }
    }
}

/// The oriented solid substituted for a drilled shell a later stage refused.
///
/// One function, so the reason recorded on the substitute cannot drift from
/// the record of what actually happened to the shell: both come from here.
///
/// **The reason is [`SolidDecisionReason::DrilledShellRefused`], and it used
/// to be `BelowThreshold`.** That older stamp said the savings gate had
/// measured this model's cavity and found it under the registered minimum —
/// next to a report line stating that the model's half-million-face drilled
/// shell had just been refused. Nothing on this path measures a saving: the
/// cavity passed every hollow-stage gate, the shell was built and drilled, and
/// a later stage would not carry it. The stage that refused it and that
/// refusal's own detail travel beside the reason, in
/// [`SupportedGeometry::DrilledShellRefusedBySupport`] on the emitted record
/// and in [`ChainStop::after_refused_shell`] when the solid fails too.
fn substitute_solid_for_refused_shell(
    model: &ModelId,
    mesh: plinth_mesh::Mesh,
    orientation: plinth_orient::OrientationHistory,
    cavity_repair: Option<plinth_hollow::CavityRepairEvidence>,
    wall: Option<Finite>,
    refused_shell: &DrilledShellRefusal,
) -> Result<HollowedModel, ChainStop> {
    HollowedModel::solid_decision_recording_repair(
        mesh,
        orientation,
        SolidDecisionReason::DrilledShellRefused,
        cavity_repair,
        wall,
    )
    .map_err(|source| {
        ChainStop::new(PipelineError::Hollow {
            model: model.clone(),
            source,
        })
        .after_refused_shell(refused_shell.clone())
    })
}

/// The oriented solid substituted for a hollow stage that errored out
/// entirely -- case 0, never a shell.
///
/// The counterpart to [`substitute_solid_for_refused_shell`] for the path
/// where no shell was ever built: `hollow_stage` itself returned an `Err`
/// that refuses this mesh's geometry, so there is no [`DrilledShellRefusal`]
/// to carry -- only the pre-cloned oriented solid and the typed error's own
/// detail, which the caller records verbatim in
/// [`SupportedGeometry::HollowStageRefused`].
///
/// **The reason is [`SolidDecisionReason::HollowStageRefused`], and it used
/// to be `BelowThreshold`.** That older stamp said the savings gate had
/// measured this model's cavity and found it under the registered minimum --
/// next to a report line stating that the hollow stage itself had errored
/// before it ever produced a cavity to measure. Nothing on this path measures
/// a saving: the stage never got that far.
fn substitute_solid_for_hollow_stage_failure(
    model: &ModelId,
    mesh: plinth_mesh::Mesh,
    orientation: plinth_orient::OrientationHistory,
    cavity_repair: Option<plinth_hollow::CavityRepairEvidence>,
    wall: Option<Finite>,
) -> Result<HollowedModel, ChainStop> {
    HollowedModel::solid_decision_recording_repair(
        mesh,
        orientation,
        SolidDecisionReason::HollowStageRefused,
        cavity_repair,
        wall,
    )
    .map_err(|source| {
        ChainStop::new(PipelineError::Hollow {
            model: model.clone(),
            source,
        })
    })
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
        | HollowError::CavityDistanceIndexLimitExceeded { .. }
        // FU-54: the well-composedness repair's whole-grid re-selection loop
        // hit its admitted `max_cavity_repair_passes` ceiling. A cap that was
        // reached is a finding to report, never a reason to take a different
        // route to an artifact -- the same FU-8b rule as every other ceiling
        // in this group, and this one is unreachable through
        // `plinth_hollow::run` today for the same structural reason
        // `CavityRepairRemovedCavity` is: the module only ever removes
        // cavity, so a real mask converges in 1-2 passes and 8 is 4x
        // headroom. Listed here rather than with the geometry group below
        // because it names a resource ceiling, not a shape.
        | HollowError::CavityRepairPassLimitExceeded { .. }
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
        // Unreachable through `plinth_hollow::run`, which converts all seven
        // into a typed `HollowStageOutput::Solid` rather than propagating them.
        // Listed as geometry anyway, because if one ever did propagate, "keep
        // the solid" is precisely what it means. `IneligibleCavityBelowDrainThroat`
        // is FU-47's resin-flow connectivity rule (2026-08-12 ruling, §4): a
        // cavity that nowhere admits the registered drain bore is left as
        // solid material rather than cut into an undrainable trap. The middle
        // two are the well-composedness repair's own outcomes and reach the
        // record as `NoCavityAfterRepair` / `BelowThresholdAfterRepair`, which
        // is how a repair-caused solid stays distinguishable from a shape that
        // never had a cavity. `RealizedWallBelowSpec` is FU-55's own check: a
        // shell was built and its realized wall measured under the requested
        // one, and `run` converts it into `ShellUnderWalled` before it ever
        // reaches here — "keep the solid" is exactly the ruled behaviour.
        HollowError::IneligibleBelowBulk
        | HollowError::IneligibleNoCavity
        | HollowError::IneligibleCavityBelowDrainThroat { .. }
        | HollowError::IneligibleSavings { .. }
        | HollowError::CavityRepairRemovedCavity { .. }
        | HollowError::IneligibleSavingsAfterCavityRepair { .. }
        | HollowError::RealizedWallBelowSpec { .. } => true,
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
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "float-to-integer casts on lattice or step arithmetic: Rust's `as` saturates at the integer \
          bounds and maps NaN to 0, and every site in this function is range-checked before the cast \
          or clamped into range after it (read site by site, 2026-08-26). See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
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
    // last place the hollow stage's own report is in hand — the sealed run
    // consumes it a few lines below and the report is gone with it. FU-37's
    // requested wall and FU-55's realized wall come off the same report for
    // exactly that reason: leaving them there is how a landed, ruled
    // measurement reaches no output of an actual run.
    let cavity_repair = hollowed.history().hollow().cavity_repair();
    let wall = hollowed.history().hollow().wall();
    let realized_wall = hollowed.history().hollow().realized_wall();
    let savings_shortfall = hollowed.history().hollow().savings_shortfall();
    // Chambers the drilling stage could not drain and filled back to solid.
    // Same reason as the three lines above: this is the last place the hollow
    // stage's report exists, and resin the engine gave up on has to reach the
    // run report or nobody ever learns it was given up on.
    let filled_back_chambers_ml: Vec<Finite> = hollowed
        .history()
        .hollow()
        .drilling_evidence()
        .map(|evidence| {
            evidence
                .refilled_pockets()
                .iter()
                .map(plinth_hollow::drilling::RefilledPocketEvidence::volume_ml)
                .collect()
        })
        .unwrap_or_default();

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

    // **The placement selector.** Both engines end at the same
    // `SupportedModel` -> `plinth_emit::write` boundary, but they do NOT share
    // the pre-write gate set: `judge_then_emit` judges a
    // `FinalSupportAuthority`'s verification snapshot, and the sweep produces
    // no authority. The sweep path therefore emits with ZERO gates run, and
    // says so by reporting every gate as pending — the same disclosure channel
    // the legacy path uses for a gate that is not yet ported. See the sweep
    // path's own comment.
    if policy.placement == crate::Placement::Sweep {
        return sweep_and_emit(
            model,
            source_faces,
            prepared_faces,
            hollowed,
            supported_geometry,
            SweepStageInputs {
                min_z,
                max_z,
                layer_pitch_mm,
                lift_mm: policy.support.lift_mm,
                max_primitives: input.max_primitives(),
                max_primitive_bytes: input.max_primitive_bytes(),
                printer: policy.printer(),
            },
            HollowCarryOut {
                hollow_disposition,
                cavity_repair,
                wall,
                realized_wall,
                savings_shortfall,
                filled_back_chambers_ml,
            },
            target,
            stop_after,
        );
    }

    // A refusal without the bounds it was measured against is not a
    // measurement; carry the model-scaled caps on the failure path, as the
    // census harness does.
    let scaled_caps = format!(
        "faces={prepared_faces} max_grid_bytes={} knn_max_work={}",
        input.max_grid_bytes(),
        input.minima_knn_max_work(),
    );

    // FU-57: a coverage-closure refusal (FU-56) is only knowable from
    // `run_support`'s `Err`, by which point the consumed
    // `HollowedModel` and its mesh are gone. The render evidence needs the
    // mesh, so this is the last point it can be captured -- see
    // [`RenderMesh`]'s own doc for why this is a real, documented per-model
    // cost rather than a lazy one.
    let render_mesh = RenderMesh::from_mesh(hollowed.mesh());

    // **SPEC-A4 design §G.1 stage 4 — the production door.** This is the one
    // production call site of the sealed support run, and from here it is the
    // one-winner transactional controller: one repair-pool row per step, taken
    // all the way through the pipeline and either committed or rolled back,
    // with the §A.8 report carried out below. The batch loop this call site
    // used to reach is deleted; there is no second controller to reach.
    // Its own clock, scoped to the sealed run, because that is the span it
    // times; `run_support` starts a second one for the stage chain inside it,
    // and both lines say which scope they are measured from.
    let clock = plinth_support::StageClock::start(format!("model {model}"));
    clock.entering("SealedSupportRun");
    let sealed = run_support(hollowed, policy.support.clone(), input);
    let authority = sealed.map_err(|source| {
        // FU-57: only the coverage-closure refusal carries evidence this row
        // persists. Every other `SupportRunError` is a stage failure with
        // nothing coverage-shaped to rank.
        let evidence = if let SupportRunError::CoverageClosure(refusal) = &source {
            // **"A refused model leaves no path on disk" is a claim about the
            // path, and this refusal now owns it.** The coverage verdict used
            // to be spoken only by the pre-write gate, and `judge_then_emit`
            // cleared an earlier run's artifact on its way out. FU-56 moved
            // the verdict one stage earlier, to a stage that never reaches
            // `judge_then_emit` — so without this, a model that emitted
            // yesterday and is coverage-refused today would leave yesterday's
            // `.chitubox` sitting at the target beside today's refusal, where
            // the next reader takes it for this run's answer. Best-effort for
            // the same reason the gate's own clearing is: this is cleanup on a
            // path that has already decided to refuse.
            let _ = std::fs::remove_file(target);
            Some(coverage_evidence::persist_closure_refusal(
                model,
                target,
                &render_mesh,
                Some(&supported_geometry),
                refusal,
            ))
        } else {
            None
        };
        // **SPEC-A4 §A.8's third destination, refused half.** The controller
        // refused, and this is the disposition that carries what it spent
        // doing so. It comes off the typed refusal rather than off the
        // authority, because on this path there IS no authority — the run
        // returned `Err` and its ledger lives in the refusal. Ruling R10
        // deleted the failed-tries cap here specifically, so this is the
        // expensive path and the one whose cost most needs a reader.
        //
        // Every other `SupportRunError` is a stage failure at or before
        // attempt 0, where the repair loop was never entered: `None`, not an
        // all-zero report, because "the loop ran and spent nothing" and "the
        // loop never ran" are different facts.
        let repair_report = match &source {
            SupportRunError::CoverageClosure(refusal) => refusal.repair.clone(),
            _ => None,
        };
        let stop = ChainStop::new(PipelineError::SealedRun {
            model: model.clone(),
            source,
        })
        .measured_against(scaled_caps)
        .with_repair_report(repair_report);
        match evidence {
            Some(evidence) => stop.with_evidence(evidence),
            None => stop,
        }
    })?;
    if stop_after == Some(StageStop::Support) {
        return Ok(None);
    }

    // **SPEC-A4 §A.8's third destination, sealed half — read HERE and not
    // inside `judge_then_emit`.** The authority is about to be handed over and
    // consumed, and the lowered scene keeps no repair ledger, so this is the
    // last frame in which the ledger exists. Reading it one frame in, inside
    // `judged_emission`, is what put the report on the sealed path only: a
    // gate refusal returns `Err` from there and the read died with it, so the
    // path ruling R10 deleted the failed-tries cap on reported nothing about
    // what it spent. One read here reaches the artifact, the gate refusal and
    // a post-seal lowering or emit failure alike.
    let repair_report = Some(authority.repair_report().clone());

    // **WARNING MODE (owner ruling, 2026-09-01), read at the same last frame
    // and for the same reason.** When the coverage-repair controller ran out of
    // candidates or of budget it used to return `Err` and no file existed; it
    // now seals the accepted layout and carries its own give-up reason on the
    // authority. That reason dies with the authority one frame from here, and
    // it is the half of the warning the gate's measurement cannot supply: the
    // gate says WHAT is uncovered, this says WHY the engine stopped trying.
    let uncovered_closure = authority.uncovered_coverage().cloned().map(Box::new);

    // **SPEC-A4 §D.2, the SEALED half — and this line is the whole of it.**
    //
    // The step stream lives on the authority and dies with it one frame from
    // here, so this is the last moment §D's exact attribution exists at all.
    // Until now nothing wrote it: `clear_stale_coverage_evidence` deleted any
    // stale sidecar on the way in and a sealing model left NOTHING, so "which
    // contact closed which hole" was answerable for a refused model and not for
    // one whose plan was about to be printed.
    //
    // Written before the pre-write gate deliberately. The controller had
    // already sealed and already spent by the time the gate speaks, and a gate
    // refusal does not un-spend it; a reader of a gate-refused model gets the
    // refusal sidecar AND the ledger of the run that produced the plan it
    // refused.
    //
    // A write failure is data, never control flow (the FU-57 rule): the model
    // is not un-emitted because an evidence file could not be written.
    let repair_ledger = coverage_evidence::persist_sealed_repair_ledger(
        model,
        target,
        authority.repair_report(),
        authority.repair_steps(),
        authority.emitted_contacts(),
    );

    // The pre-write gate. `judge_then_emit` judges the sealed snapshot while
    // the authority is still alive, and only a passing verdict reaches
    // `into_supported_model` and `plinth_emit::write` -- so a refusal leaves
    // no file at `target` at all. There is no override parameter and there is
    // not going to be one.
    clock.entering("PreWriteGateAndEmit");
    let gated = match crate::judge_then_emit(
        model,
        authority,
        &EmitRequest {
            target_path: target.to_path_buf(),
            printer: policy.printer().clone(),
        },
    ) {
        Ok(gated) => gated,
        // A `match` rather than `map_err` so the one repair-ledger read above
        // can be MOVED into whichever of the two paths runs, instead of being
        // cloned once for a closure that usually never fires.
        Err(error) => {
            let stop = match &error {
                // Carry the gate's own reason, the gate itself, and the
                // geometry it was refusing -- a refusal without the verdict
                // behind it is not a measurement, and a gate verdict without
                // the geometry it judged cannot be told apart from a verdict
                // on the fallback.
                PipelineError::Gates { source, .. } => {
                    let gate = source.gate();
                    // FU-57: a blocking coverage verdict carries the full
                    // `CoverageReport` on `GateRefusal::Refused`, and an
                    // UNMEASURABLE coverage gate is still a coverage refusal —
                    // fail-closed, the model `Refused`, the coverage gate
                    // named — so it keeps evidence too, as a reason-only
                    // sidecar naming the measurement failure. That is exactly
                    // what the controller's own equivalent failure
                    // (`CoverageClosureReason::MeasurementRefused`) has always
                    // written; before this the identical error produced a
                    // sidecar one stage earlier and complete silence here.
                    // Gate outcomes that are not about coverage at all
                    // (unauthorized emission, an unavailable registry, a
                    // pending-but-unported gate) persist nothing, because
                    // there is no coverage refusal to explain.
                    let evidence = source.report().map_or_else(
                        || {
                            source.unmeasurable_coverage_reason().map(|reason| {
                                coverage_evidence::persist_unmeasurable_gate(
                                    model,
                                    target,
                                    &render_mesh,
                                    gate,
                                    Some(&supported_geometry),
                                    &reason,
                                )
                            })
                        },
                        |report| {
                            Some(coverage_evidence::persist_gate_refusal(
                                model,
                                target,
                                &render_mesh,
                                gate,
                                Some(&supported_geometry),
                                report,
                            ))
                        },
                    );
                    let stop = ChainStop::refused(
                        error,
                        GateStop {
                            gate,
                            geometry: supported_geometry.clone(),
                        },
                    );
                    match evidence {
                        Some(evidence) => stop.with_evidence(evidence),
                        None => stop,
                    }
                }
                _ => ChainStop::new(error),
            };
            // §A.8 again, and this is the half the challenger measured
            // missing. The controller had already run and already spent when
            // the gate refused, or when lowering or the emit audit failed
            // afterwards. Attached to every one of those stops, not only the
            // coverage-gate one: the spend is the same fact whichever stage
            // spoke next.
            return Err(stop.with_repair_report(repair_report));
        }
    };

    // **WARNING MODE (owner ruling, 2026-09-01), and FU-57 kept alive across
    // it.** The coverage gate used to REFUSE an uncovered model, and the
    // refusal path is where the sidecar and the per-island SVG renders were
    // written. Now that the same model EMITS, that evidence would have vanished
    // at exactly the moment it became most useful: the owner has a file he can
    // open, and the renders are what tell him which part of it to look at. So
    // the same writer runs here, on the emitted path, from the same gate report
    // and the same pre-support render mesh.
    //
    // A write failure is data, never control flow (the FU-57 rule): the model
    // is not un-emitted because an evidence file could not be written, and the
    // failure is carried on the evidence value itself.
    let coverage_warning = gated
        .gates
        .coverage_warned()
        .then(|| {
            crate::coverage_evidence::CoverageGapWarning::from_gate_report(
                gated.gates.coverage(),
                uncovered_closure.as_deref(),
            )
        })
        .flatten();
    if coverage_warning.is_some() {
        let _ = coverage_evidence::persist_gate_refusal(
            model,
            target,
            &render_mesh,
            Some(crate::PreWriteGate::UncoveredIslands),
            Some(&supported_geometry),
            gated.gates.coverage(),
        );
    }

    Ok(Some(ChainOutcome {
        model: model.clone(),
        source_faces,
        prepared_faces,
        cavity_repair,
        wall,
        realized_wall,
        savings_shortfall,
        filled_back_chambers_ml,
        hollow_disposition,
        supported_geometry,
        ownerless_islands: gated.ownerless_islands,
        // The same single read as the refused path above — see the comment at
        // the `judge_then_emit` call.
        repair_report,
        repair_ledger: Some(repair_ledger),
        abandoned_contacts: gated.abandoned_contacts,
        grown_contact_tips: gated.grown_contact_tips,
        routed_contacts: gated.routed_contacts,
        final_live_members: gated.final_live_members,
        snapshot_digest: gated.snapshot_digest,
        pending_gates: gated.gates.pending().to_vec(),
        load_warnings: gated.gates.load_warnings().cloned(),
        // WARNING MODE (owner ruling, 2026-09-01). Built from the GATE's own
        // measurement of the sealed snapshot, which is the measurement that
        // describes the bytes about to be written, and named with the reason
        // the repair search gave up when it was the repair search that got the
        // run here. `coverage_warned()` is the gate's own bit; the island list
        // is derived from the report either way, so a report that names no
        // uncovered island produces no warning and cannot cry wolf.
        coverage_warning,
        primitive_count: gated.primitive_count,
        lift_mm: gated.lift_mm,
        plate_z_mm: gated.plate_z_mm,
        emit: gated.emit,
    }))
}

/// The frame numbers the sweep path needs, gathered at the fork so
/// `sweep_and_emit` does not re-measure the mesh a second time.
struct SweepStageInputs<'a> {
    min_z: f64,
    max_z: f64,
    layer_pitch_mm: f64,
    lift_mm: PositiveFinite,
    max_primitives: usize,
    max_primitive_bytes: usize,
    printer: &'a PrinterProfile,
}

/// The hollow stage's own measurements, read once at the fork because the
/// hollowed model is consumed below and its report goes with it.
struct HollowCarryOut {
    hollow_disposition: HollowDisposition,
    cavity_repair: Option<plinth_hollow::CavityRepairEvidence>,
    wall: Option<Finite>,
    realized_wall: Option<plinth_hollow::RealizedWall>,
    savings_shortfall: Option<plinth_hollow::SavingsShortfall>,
    filled_back_chambers_ml: Vec<Finite>,
}

/// Place a support field with the slice-sweep core and emit one `.chitubox`.
///
/// # What this path does NOT do, and why that is stated here rather than
/// discovered
///
/// **No pre-write gate runs.** `judge_then_emit` judges a
/// `FinalSupportAuthority`'s verification snapshot — routed contacts, live
/// members, the sealed layout — and the sweep produces none of those. Rather
/// than fake a snapshot for it to judge, this path emits with zero gates run
/// and reports **all twelve** as `pending_gates`, which is the run report's
/// existing channel for "this artifact was authorized on the gates that ran,
/// not on all twelve". The design of record (§11) predicted the gates would
/// need no change; they do, and this is the deviation, disclosed.
///
/// **No repair ledger, no ownerless-island list, no abandoned/grown contacts.**
/// Those are measurements the legacy controller takes. The sweep has no repair
/// loop at all (the up-sweep terminates with the demand set empty or a named
/// residual), so `None`/empty here means "this engine never ran that search",
/// not "the search found nothing".
///
/// Every shortfall the sweep DOES find is printed in the disclosure block
/// below and is a warning on an emitted file, never a refusal (owner ruling
/// 2026-09-01).
#[expect(
    clippy::too_many_arguments,
    reason = "the sweep fork carries the frame numbers and the hollow report the caller \
          already read; grouping them further would only rename the same nine facts"
)]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "one layer-count cast on a span the mesh bbox already proved finite and positive; \
          Rust's `as` saturates at the integer bounds and maps NaN to 0, and the result is \
          clamped to at least 1 below"
)]
fn sweep_and_emit(
    model: &ModelId,
    source_faces: usize,
    prepared_faces: usize,
    hollowed: HollowedModel,
    supported_geometry: SupportedGeometry,
    inputs: SweepStageInputs<'_>,
    hollow: HollowCarryOut,
    target: &Path,
    stop_after: Option<StageStop>,
) -> Result<Option<ChainOutcome>, ChainStop> {
    let clock = plinth_support::StageClock::start(format!("model {model}"));
    clock.entering("SliceSweepPlacement");

    // The sweep's own frame: `plate_z = min_z - lift`, exactly as
    // `crates/plinth-support/src/runner.rs`:2178-2186 computes it, and a layer
    // plan that stops at the MODEL TOP rather than carrying the sealed-run
    // gates' `+1.0 mm` headroom and 1,000-layer minimum. Slicing 830 empty
    // planes for a disc is waste, not error.
    let plate_z = inputs.min_z - inputs.lift_mm.get();
    let sweep_layer_count = (((inputs.max_z - plate_z) / inputs.layer_pitch_mm).ceil() as usize)
        .saturating_add(1)
        .max(1);
    let sweep_layers = plinth_core::LayerPlan::new(
        plate_z,
        vec![inputs.layer_pitch_mm; sweep_layer_count],
    )
    .map_err(|source| {
        ChainStop::new(PipelineError::LayerPlan {
            model: model.clone(),
            source,
        })
    })?;

    let policy = plinth_sweep::SweepPolicy::from_registry().map_err(|source| {
        ChainStop::new(PipelineError::Sweep {
            model: model.clone(),
            source,
        })
    })?;
    // The drains this model was drilled with, handed to placement so the field
    // is built AROUND the resin's way out rather than across it. The hollow
    // stage runs after orientation, so these coordinates and the mesh the sweep
    // is about to slice are already one frame and no transform is applied here
    // -- if that ordering ever changes, this is the line that has to change
    // with it.
    let drains: Vec<plinth_sweep::DrainSpec> = hollowed
        .history()
        .hollow()
        .drains()
        .iter()
        .map(|drain| {
            plinth_sweep::DrainSpec::from_bore(
                [
                    drain.entrance_mm[0].get(),
                    drain.entrance_mm[1].get(),
                    drain.entrance_mm[2].get(),
                ],
                [
                    drain.exit_mm[0].get(),
                    drain.exit_mm[1].get(),
                    drain.exit_mm[2].get(),
                ],
                drain.radius_mm.get(),
            )
        })
        .collect();
    let started = std::time::Instant::now();
    let plan = plinth_sweep::run_sweep_with_drains(
        hollowed.mesh(),
        &sweep_layers,
        &policy,
        &drains,
    )
    .map_err(|source| {
        ChainStop::new(PipelineError::Sweep {
            model: model.clone(),
            source,
        })
    })?;
    let sweep_seconds = started.elapsed().as_secs_f64();

    let frame = plinth_support::BuildFrame::new(
        Finite::new(plate_z).unwrap_or_else(|_| {
            // `plate_z` came out of a finite mesh extent minus a positive
            // finite lift; the sweep already refused a non-finite bbox.
            Finite::new(0.0).expect("zero is finite")
        }),
        inputs.lift_mm,
    );
    let emission = plinth_sweep::to_primitives(
        &plan,
        frame,
        &sweep_layers,
        inputs.max_primitives,
        inputs.max_primitive_bytes,
    )
    .map_err(|source| {
        ChainStop::new(PipelineError::SweepLowering {
            model: model.clone(),
            source,
        })
    })?;

    print_sweep_disclosure(
        model,
        &plan,
        &emission,
        sweep_seconds,
        inputs.printer,
        &sweep_layers,
    );

    if stop_after == Some(StageStop::Support) {
        return Ok(None);
    }

    // The raft the lowering appended after every support record
    // (fix/sweep-raft, 2026-09-29): its pads and bridges are counted as raft,
    // everything before them as support.
    let raft_primitive_count = emission.raft_pads + emission.raft_bridges;
    let primitive_count = emission.primitives.len();
    let (mesh, hollow_history) = hollowed.into_parts();
    let history = plinth_support::SupportHistory::from_reports(
        hollow_history,
        plinth_support::SupportReport {
            support_primitive_count: primitive_count.saturating_sub(raft_primitive_count),
            raft_primitive_count,
        },
        frame,
    );
    let supported =
        plinth_support::SupportedModel::from_parts(mesh, emission.primitives, history).map_err(
            |source| {
                ChainStop::new(PipelineError::Support {
                    model: model.clone(),
                    source,
                })
            },
        )?;

    clock.entering("EmitWithoutGates");
    // A failed write leaves no file AT THE TARGET: `plinth_emit::write` clears
    // it on every failure path. An artifact an EARLIER run left at the same
    // target is cleared the way the gated path clears it, so a failed model
    // never leaves yesterday's answer sitting beside today's failure. The disk
    // audit no longer fails a file that reads back (its findings ride on the
    // report as WARNINGS), and a file that cannot be read back keeps its bytes
    // BESIDE the target (`plinth_emit::failed_audit_path`), which this removal
    // does not touch (fix/round100-emit, 2026-09-29).
    let emit = match plinth_emit::write(
        &supported,
        &EmitRequest {
            target_path: target.to_path_buf(),
            printer: inputs.printer.clone(),
        },
    ) {
        Ok(report) => report,
        Err(source) => {
            let _ = std::fs::remove_file(target);
            return Err(ChainStop::new(PipelineError::Emit {
                model: model.clone(),
                source,
            }));
        }
    };
    // THE SEAT TRIM as the file holds it (fix/tip-trim, 2026-09-28).
    let trim = &emit.seat_trim;
    #[expect(
        clippy::cast_precision_loss,
        reason = "a resin volume in thousandths of a mm3 is far inside f64's exact range"
    )]
    let removed_mm3 = trim.removed_volume_milli_mm3 as f64 / 1000.0;
    println!(
        "[sweep {model}] seat trim in the file: {} records cut in one batched call; {} cut by \
         their seat plane, {} further cut by the model solid; {removed_mm3:.3} mm3 of support \
         removed; of the records the trim would have removed entirely, {} keep only their part \
         outside the model, {} lie WHOLLY INSIDE the model and are left out of the file (a \
         WARNING of a seat that failed), and {} were left untrimmed because the re-cut could not \
         run",
        trim.records_trimmed,
        trim.records_cut_by_seat_plane,
        trim.records_cut_by_model,
        trim.records_kept_outside_model_after_empty_trim,
        trim.records_dropped_wholly_inside_model,
        trim.records_left_whole_after_empty_trim,
    );
    if let Some(error) = &trim.batch_error {
        println!(
            "[sweep {model}] WARNING the seat trim did not run ({error}); every tip and first \
             shaft segment in this file is UNTRIMMED"
        );
    }

    Ok(Some(ChainOutcome {
        model: model.clone(),
        source_faces,
        prepared_faces,
        cavity_repair: hollow.cavity_repair,
        wall: hollow.wall,
        realized_wall: hollow.realized_wall,
        savings_shortfall: hollow.savings_shortfall,
        filled_back_chambers_ml: hollow.filled_back_chambers_ml,
        hollow_disposition: hollow.hollow_disposition,
        supported_geometry,
        // Empty because this engine never ran the searches that fill them, not
        // because those searches found nothing. See this function's own doc.
        ownerless_islands: Vec::new(),
        repair_report: None,
        repair_ledger: None,
        abandoned_contacts: Vec::new(),
        grown_contact_tips: Vec::new(),
        routed_contacts: plan.struts.len(),
        final_live_members: plan.struts.len(),
        snapshot_digest: sweep_digest_u64(&plan.report.sweep_digest),
        // **Every gate is pending on this path**, because none of them ran.
        pending_gates: PreWriteGate::ALL.to_vec(),
        load_warnings: None,
        coverage_warning: None,
        primitive_count,
        lift_mm: supported.lift_mm().get(),
        plate_z_mm: supported.plate_z_mm().get(),
        emit,
    }))
}

/// A stable `u64` view of the sweep's own content digest, so the run report's
/// `snapshot_digest` field names THIS field rather than being left at zero.
fn sweep_digest_u64(digest: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in digest.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Print the sweep's disclosure block and its full summary.
///
/// **This goes to stdout, not into the serialized `.pipeline.json` report.**
/// The design of record's §13 field list is printed in full and verbatim
/// below; giving it a typed home inside `ModelPreparationOutcome::Artifact`
/// means a serde surface and a golden-report migration that LANE 2 did not
/// take, and pretending it landed in the JSON would be worse than saying where
/// it did land.
fn print_sweep_disclosure(
    model: &ModelId,
    plan: &plinth_sweep::SweepPlan,
    emission: &plinth_sweep::SweepEmission,
    sweep_seconds: f64,
    printer: &PrinterProfile,
    layers: &plinth_core::LayerPlan,
) {
    let report = &plan.report;
    println!("[sweep {model}] {}", report.render_disclosure());
    println!(
        "[sweep {model}] arms {} (plate {}, model {}, arm {}, interior {}); \
         seat spacing mean {:.3} mm p95 {:.3} mm max {:.3} mm; \
         tip radius min {:.4} p50 {:.4} max {:.4} mm (DIAMETER min {:.4} p50 {:.4} max {:.4} mm), \
         {} at the minimum, {} at the maximum, {} above the owner's growth ceiling; \
         arms dropped at the minimum load {} (kept by the span test {}, removal passes rolled \
         back {})",
        report.struts_total,
        report.landings.plate,
        report.landings.model,
        report.landings.strut,
        report.landings.interior,
        report.seat_spacing.mean_mm,
        report.seat_spacing.p95_mm,
        report.seat_spacing.max_mm,
        report.tip_radius.min_mm,
        report.tip_radius.p50_mm,
        report.tip_radius.max_mm,
        2.0 * report.tip_radius.min_mm,
        2.0 * report.tip_radius.p50_mm,
        2.0 * report.tip_radius.max_mm,
        report.tip_radius.count_at_min,
        report.tip_radius.count_at_max,
        report.tip_radius.count_above_owner_growth_ceiling,
        report.counters.strut_removals,
        report.counters.strut_removals_refused_by_span,
        report.counters.removal_passes_rolled_back,
    );
    // The tip SEAT, published per run rather than assumed. The `a >= r_tip`
    // guarantee is the last line: the cut disc the seat plane actually takes
    // through each ball, against the tip it has to be no narrower than.
    {
        use plinth_support::SweepLowering;
        let depth = SweepLowering::distribution(&emission.tip_seat_depth_mm);
        let cut_min = emission
            .tip_cut_radius_mm
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        let tip_min = emission
            .tip_radius_mm
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        let below = emission.tips_below_area_guarantee_count();
        let (depth_min, depth_p50, depth_max) = depth.unwrap_or((f64::NAN, f64::NAN, f64::NAN));
        println!(
            "[sweep {model}] tip seat depth min {depth_min:.4} p50 {depth_p50:.4} \
             max {depth_max:.4} mm over {} seated tips; {} at the law minimum; \
             cut-disc radius min {cut_min:.4} mm against \
             tip radius min {tip_min:.4} mm; tips below the a>=r_tip guarantee: {below}",
            emission.tip_seat_depth_mm.len(),
            emission.tips_at_law_minimum,
        );
        // THE SEAT TRIM and the FOOTPRINT RULE (fix/tip-trim, 2026-09-28).
        let disc =
            plinth_support::SweepLowering::distribution(&emission.tip_contact_disc_radius_mm)
                .unwrap_or((f64::NAN, f64::NAN, f64::NAN));
        let offset =
            plinth_support::SweepLowering::distribution(&emission.tip_raster_seat_offset_mm)
                .unwrap_or((f64::NAN, f64::NAN, f64::NAN));
        let tip_radius = &emission.tip_radius_mm;
        let below_at_face = emission
            .tip_contact_disc_radius_mm
            .iter()
            .zip(tip_radius)
            .filter(|(disc, tip)| {
                **disc < **tip * (1.0 - plinth_support::SweepLowering::AREA_GUARANTEE_TOLERANCE)
            })
            .count();
        let footprint = &report.seat_footprint;
        println!(
            "[sweep {model}] seat trim: {} tips (and their first shaft segments) trimmed to the \
             support side of their seat face and out of the model, and {} further Member spans \
             of those shafts that reach the model side of the seat plane, {} of them on the raster \
             seat's plane for want of a measured face; contact disc at the face min {:.4} p50 \
             {:.4} max {:.4} mm, {below_at_face} below its own tip radius; raster seat from the \
             face min {:+.4} p50 {:+.4} max {:+.4} mm (positive = below a down-facing face). \
             Footprint: {} discs graded, {} on their face as placed, {} seats MOVED along the \
             face onto it (longest {:.4} mm; {} members joined to a moved axis moved with it, \
             by up to {:.4} mm), {} on a knife edge, {} left in place because every on-face \
             seat broke a rule of the finished field; moves refused by the lean law {}, by an \
             unbraced run's limit {}, by the drain keep-out {} ({} member checks); {} disc \
             samples + {} search samples over {} layers, {} round(s)",
            emission.tips_trimmed_to_seat,
            emission.shaft_members_trimmed_to_seat,
            emission.tips_trimmed_without_a_measured_face,
            disc.0,
            disc.1,
            disc.2,
            offset.0,
            offset.1,
            offset.2,
            footprint.tips_graded,
            footprint.tips_on_face,
            footprint.tips_moved_onto_face,
            footprint.max_move_mm,
            footprint.members_following_a_moved_host,
            footprint.max_follow_mm,
            footprint.knife_edges.len(),
            footprint.moves_refused.len(),
            footprint.moves_refused_lean,
            footprint.moves_refused_slenderness,
            footprint.moves_refused_drain,
            footprint.members_checked,
            footprint.disc_samples,
            footprint.search_samples,
            footprint.layers_rasterised,
            footprint.rounds,
        );
        for (arm, fraction) in &footprint.knife_edges {
            println!(
                "[sweep {model}] WARNING tip of arm {arm} sits on a KNIFE EDGE: no seat within \
                 the search radius along its face holds its whole contact disc, so it keeps its \
                 seat, trimmed, with {:.1}% of its disc on the face",
                fraction * 100.0
            );
        }
        for (arm, fraction) in &footprint.moves_refused {
            println!(
                "[sweep {model}] WARNING tip of arm {arm} hangs off its face and every seat \
                 along the face that holds its whole contact disc would break the lean law, an \
                 unbraced run's limit or the drain keep-out for it or a member joined to it, so \
                 it keeps its seat, trimmed, with {:.1}% of its disc on the face",
                fraction * 100.0
            );
        }
        // THE HEAD-JOIN RULE (2026-09-29).
        let rule = &report.head_join;
        println!(
            "[sweep {model}] head joins: {} joins on support arms examined, {} with a sum-sized \
             section within {:.3} mm of the arm's seat plane and of the model ({} near the plane \
             only, off the model); {} landers moved below the seat zone ({} down the \
             landing chain, {} at the {:.3} mm hard limit because no join held the {:.3} mm \
             placement margin; largest drop {} layers; smallest solid clearance {:.4} mm; \
             {:.4} mm2 handed on), {} resting members re-landed with them, {} clearance rows, \
             {} section-to-model rows ({} model-edge rows against the filled sections; {} \
             sections, {} in the emitted census, near only by their interior), {} candidates \
             admitted off the model, \
             {} candidates refused because the member's base disc would reach under the plate, \
             {} pass(es){}; kept in the seat zone: no clear join {}, a cross tied to a mover {}, \
             not a support arm {}, no model surface {}; {} unmeasured",
            rule.joins_examined,
            rule.joins_in_seat_zone_before,
            rule.clearance_mm,
            rule.joins_off_the_model,
            rule.landers_moved,
            rule.landers_moved_down_the_chain,
            rule.landers_at_hard_limit,
            rule.clearance_mm,
            rule.standoff_mm,
            rule.max_drop_layers,
            rule.min_lander_clearance_mm,
            rule.load_moved_mm2,
            rule.dependants_relanded,
            rule.clearance_rows,
            rule.model_rows,
            rule.interior_rows,
            rule.sections_near_by_interior,
            rule.census_near_by_interior,
            rule.candidates_off_the_model,
            rule.candidates_below_plate,
            rule.passes,
            if rule.enabled { "" } else { " (RULE OFF)" },
            rule.kept_no_clear_join,
            rule.kept_carrying_a_tie,
            rule.kept_not_an_arm,
            rule.kept_surface_unavailable,
            rule.joins_unmeasured,
        );
        // THE LIP-SEAT RULE (2026-09-29).
        let lip = &report.lip_seat;
        println!(
            "[sweep {model}] lip seats: {} seated arms graded, {} met their seat face beyond the \
             angle at which the seat trim keeps their section (deepest cut into a shaft outside \
             its ball {:.4} mm, widest approach {:.1} deg); {} re-landed at a smaller angle to \
             the same seat{}; kept: {} carrying another member, {} with no admissible descent; \
             candidates {} priced, {} refused by the path test, {} by the plate rule, {} by the \
             lowering's own measurement, {} by the solid's clearance from the model{}, {} by a new \
             plate root touching another, {} verified; put back once the field was sized: {} \
             whose host chain came out overloaded, {} whose root came to lap another ({} sized \
             checks; unexplained by any re-landing: {} newly overloaded members, {} new root \
             pairs; {} arms the judging sizing had inserted withdrawn with them, {} of those \
             landed on a move put back); {} pass(es); {} members resting on a re-landed arm \
             moved with it; footprint: {} on-face tips searched for a seat further along the \
             same face, {} moved there ({} taken back as still cut once every move's follow was \
             drawn, then {} other moves taken back because the field left broke a rule for a \
             member they displace, over {} re-checks), {} slides of those refused because the \
             moved shaft would still be cut; on the emitted field {} graded, {} still cut",
            lip.arms_graded,
            lip.arms_over_limit_before,
            lip.max_bite_before_mm,
            lip.max_approach_before_deg,
            lip.arms_relanded,
            if lip.enabled { "" } else { " (RULE OFF)" },
            lip.kept_carrying,
            lip.kept_no_candidate,
            lip.candidates_priced,
            lip.candidates_refused_path,
            lip.candidates_refused_plate,
            lip.candidates_refused_measured,
            lip.candidates_refused_clearance,
            if lip.clearance_unmeasured {
                " (WARNING: the model surface could not be built, so the clearance went \
                 unmeasured)"
            } else {
                ""
            },
            lip.candidates_refused_roots,
            lip.candidates_verified,
            lip.moves_put_back_host_load,
            lip.moves_put_back_roots,
            lip.sizing_checks,
            lip.new_overloads_unattributed,
            lip.new_root_pairs_unattributed,
            lip.inserted_arms_withdrawn,
            lip.inserted_on_a_put_back_move,
            lip.passes,
            lip.children_moved_with_host,
            report.seat_footprint.tips_graded_for_section,
            report.seat_footprint.tips_moved_for_section,
            report.seat_footprint.section_moves_taken_back,
            report.seat_footprint.moves_taken_back_limits,
            report.seat_footprint.settle_rechecks,
            report.seat_footprint.moves_refused_section,
            lip.arms_graded_after,
            lip.over_limit_after.len(),
        );
        for warning in &lip.over_limit_after {
            println!(
                "[sweep {model}] WARNING arm {} meets its seat face at {:.1} deg (its limit {:.1} \
                 deg), and the seat trim cuts {:.4} mm into its shaft outside the tip ball{}{}{}",
                warning.arm,
                warning.approach_deg,
                warning.limit_deg,
                warning.bite_mm,
                if warning.carries_someone {
                    "; another member rests on it, so it was not re-landed"
                } else {
                    ""
                },
                if warning.relanded {
                    "; it was re-landed by this rule and a later pass cut it again"
                } else {
                    ""
                },
                if warning.on_plate {
                    "; it stands on the plate (horizontal rings down to its boss)"
                } else {
                    ""
                },
            );
        }
        for (lander, trunk) in &rule.in_seat_zone_after {
            println!(
                "[sweep {model}] WARNING member {lander} joins arm {trunk} near its head and the \
                 section carrying both loads comes within {:.3} mm of arm {trunk}'s seat plane \
                 and of the model, so arm {trunk}'s contact with the model is wider than its own \
                 load needs",
                rule.clearance_mm
            );
        }
    }
    println!(
        "[sweep {model}] peak layer area {:.4} mm2; total scar {:.4} mm2 (invariant \
         peak/X_ALLOWED = {:.4} mm2, ratio {:.4}); plate reaction {:.4} mm2; \
         conservation residual {:.3e}; residual demand {} cells / {:.4} mm2; \
         uncarried load {:.4} mm2 (misrouted {:.4} mm2); unrooted landings {}",
        report.peak_layer_area_mm2,
        report.total_scar_mm2,
        report.peak_layer_area_mm2 / report.x_allowed,
        if report.peak_layer_area_mm2 > 0.0 {
            report.total_scar_mm2 / (report.peak_layer_area_mm2 / report.x_allowed)
        } else {
            f64::NAN
        },
        report.plate_reaction_mm2,
        report.conservation_residual,
        report.residual_demand_cells,
        report.residual_demand_mm2,
        report.uncarried_load_mm2,
        report.misrouted_load_mm2,
        report.unrooted_landings,
    );
    println!(
        "[sweep {model}] raster {} cells/layer at {:.4} mm pitch over {} layers \
         ({} coarsening steps); kappa used {:.7}, first loss {:.7}, audit gate {:.7}, \
         will pass audit span check: {}; u_from_vertical_max {:.5}; \
         tau_vertical_over_tau_used_max {:.4}; max tau at binding reach {:.4} mm; \
         chamfer non-convergences {}; route non-convergences {}; \
         arms inserted by load {} of {} ({} insertion rounds, {} layers round-bound-exhausted \
         [UNDER-support risk], {} layers geometry-exhausted [over the limit, more rounds would \
         not help]); descent: {} arms leaned of {} (max sideways move {:.4} mm per layer),          {} of those merged onto another arm, {} cheaper leans refused for want of a clear path,          {} merges undone because the trunk could not grow enough;          registered-but-unread target spacing {:.3} mm",
        report.cells_per_layer,
        report.achieved_pitch_mm,
        report.layers,
        report.coarsening_steps,
        report.reach.kappa_used,
        report.reach.kappa_first_loss,
        report.reach.kappa_audit_gate,
        report.will_pass_audit_span_check,
        report.u_from_vertical_max,
        report.tau_vertical_over_tau_used_max,
        report.max_tau_at_binding_reach_mm,
        report.chamfer_not_converged,
        report.route_not_converged,
        report.arms_inserted_by_load,
        report.struts_total,
        report.insertion_rounds,
        report.insertion_rounds_exhausted,
        report.insertion_geometry_exhausted,
        report.arms_leaned,
        report.struts_total,
        report.lean_max_step_mm,
        report.arms_merged,
        report.lean_paths_blocked,
        report.merges_refused_by_radius,
        report.reach.target_spacing_mm_registered_but_unused,
    );
    // THE RESIN IS MEASURED FROM THE EMITTED RECORDS (2026-09-28). Until then
    // this line published the sweep's LEDGER volume as "support volume", and
    // the file held 34-51 % less on the taper spans because the lowering drew
    // one straight taper where the ledger stepped. The emitted figure leads;
    // the ledger stays beside it, labelled as the sizing figure.
    println!(
        "[sweep {model}] resin: support volume as emitted {:.3} mm3 ({:.3} mm3 in tapers and \
         chained members, {:.3} mm3 in the raft, the rest tip balls and root flares) over {} \
         arms; sizing ledger \
         {:.3} mm3 (the area law's shafts, not what is drawn); drawn profile: {} members in \
         more than one span, {} Member frusta chained, {} arms WITHOUT a profile, {} members \
         raised above their ledger to meet a thicker branch, {} arm junctions inverted in the \
         ledger itself; \
         plate contacts {} \
         holding {:.4} mm2 of root area; junctions {} at a parent's top / {} MID-SHAFT \
         ({} chosen mid-shaft by the descent, before un-merging and removal); \
         {} lander loads on an arm's head booked into the shaft below its contact \
         ({:.4} mm2), {} kept at a one-slot arm's contact; \
         shaft index {} spans; {} tip-column roots offered, carrying {:.4} mm2 that no other \
         carrier could have taken",
        emission.drawn_volume_mm3,
        emission.drawn_shaft_volume_mm3,
        emission.raft_volume_mm3,
        report.struts_total,
        report.support_volume_mm3,
        emission.arms_drawn_in_spans,
        emission.profile_member_frusta,
        emission.arms_without_profile,
        emission.members_raised_to_a_branch,
        emission.arm_junctions_inverted_in_ledger,
        report.landings.plate,
        report.plate_contact_area_mm2,
        report.junctions_at_parent_top,
        report.junctions_mid_shaft,
        report.descent_junctions_mid_shaft,
        report.counters.contact_merges_moved_below,
        report.contact_merge_load_moved_mm2,
        report.counters.contact_merges_kept_at_single_slot,
        report.shaft_spans_recorded,
        report.tip_column_roots_offered,
        report.tip_column_root_load_mm2,
    );
    // **THE CONGESTION LANE, PUBLISHED PER RUN** (owner ruling, 2026-09-06:
    // "spread some of the load back upwards to higher-landing supports"). The
    // footprint's crowding before and after, what each remedy did, and how much
    // section actually left the model's own silhouette.
    let jam = &report.congestion_lane;
    println!(
        "[sweep {model}] congestion lane: {} plate roots; congestion {:.4} -> {:.4} at lane exit \
         -> {:.4} on the finished field (under the silhouette {:.4} -> {:.4} -> {:.4}); \
         overlapping root pairs {} -> {} -> {}; cells under \
         3+ roots {} -> {} (deepest stack {} -> {}); remedies over {} round(s): {} roots \
         leaned outward, {} flank arms inserted of {} wanted for {:.4} mm2 of stacked disc \
         area after the lean, at shelf layer {} ({} cover cells), {} round(s) with no flank \
         arm because the previous ones left the stack no smaller, \
         {:.4} mm2 of section moved outside the silhouette, {:.4} mm2 of load still carried to \
         the plate by flank arms ({} of them re-landed off the plate); flank root radius \
         estimate {:.4} mm used (mean placed root {:.4} mm, hex-cell area law {:.4} mm), \
         {} flank roots still overlapping after sizing; {} outward cells refused, \
         {} flank seats refused{}",
        jam.after.all.roots,
        jam.before.all.ratio,
        jam.at_lane_exit.all.ratio,
        jam.after.all.ratio,
        jam.before.under_silhouette.ratio,
        jam.at_lane_exit.under_silhouette.ratio,
        jam.after.under_silhouette.ratio,
        jam.before.all.overlap_pairs,
        jam.at_lane_exit.all.overlap_pairs,
        jam.after.all.overlap_pairs,
        jam.before.all.stack_max,
        jam.after.all.stack_max,
        jam.before.all.stack_depth_max,
        jam.after.all.stack_depth_max,
        jam.rounds_used,
        jam.arms_leaned,
        jam.arms_inserted,
        jam.flank_arms_wanted,
        jam.flank_stacked_area_mm2.iter().sum::<f64>(),
        jam.shelf_layer,
        jam.shelf_cover_cells,
        jam.flank_rounds_not_relieving,
        jam.section_moved_outside_silhouette_mm2,
        jam.flank_load_still_at_plate_mm2,
        jam.flank_arms_moved_off_plate,
        jam.flank_radius_used_mm,
        jam.flank_radius_mean_placed_mm,
        jam.flank_radius_area_law_mm,
        jam.flank_roots_overlapping_after_sizing,
        jam.outward_cells_refused,
        jam.flank_seats_refused,
        if !jam.enabled {
            " [LANE DISABLED]"
        } else if jam.still_congested {
            " [STILL CONGESTED: a WARNING on an emitted file, never a refusal]"
        } else {
            ""
        },
    );
    // **THE SLENDERNESS LANE, PUBLISHED PER RUN** (owner ruling, 2026-09-05).
    // The ratio in force, what was over the limit before any remedy, what each
    // remedy did, and the worst run left standing against its own limit — so
    // the operator can see what the constraint decided instead of inferring it
    // from an arm count.
    let lane = &report.slenderness_lane;
    println!(
        "[sweep {model}] slenderness lane: S = {:.1} in force; brace reach {:.3} mm at the \
         thinnest brace, {:.3} mm at the thickest; {} arms over the limit before remedies \
         ({} runs); remedies: {} merges forced, {} |X| braces added, {} members thickened \
         (root raised, tip kept), {} masts PLACED, {} warnings; {} forced merges put back by \
         the radius ceiling and {} descent merges put back beside them, both before any tie; \
         crosses (each holds the sway within 45 deg of its own plane): {} for a run no cross \
         held, {} for a direction the crosses on the member did not hold, \
         {} to a partner beyond the nearest, {} bands planned, {} refused as overlapping a \
         cross on the same pair, {} taken out again as not needed; thicken refused: \
         {} past the maximum radius, {} for a root disc that would touch another, {} trunks \
         raised under a thickened member; {} plate root discs grown to the root a cross makes \
         the file draw, {} of them then lapping another root; {} brace bands refused, {} masts REFUSED for want of \
         an admissible plate cell ({} cells refused by the braced-panel check), {} masts \
         extended to a later tie, {} mast roots still lapping another root after sizing; {} \
         tie passes, {}; longest unbraced run remaining {:.3} mm against its own limit of {:.3} mm{}",
        lane.ratio,
        lane.thin_brace_reach_mm,
        lane.thick_brace_reach_mm,
        lane.arms_over_limit_before,
        lane.runs_over_limit_before,
        lane.merges_forced,
        lane.braces_added,
        lane.members_thickened,
        lane.masts_dropped,
        lane.warnings.len(),
        lane.merges_undone_by_radius,
        lane.descent_merges_put_back,
        lane.crosses_first_plane,
        lane.crosses_further_plane,
        lane.crosses_to_a_later_partner,
        lane.cross_bands_planned,
        lane.crosses_refused_overlapping,
        lane.crosses_pruned,
        lane.thicken_refused_radius,
        lane.thicken_refused_disc,
        lane.trunks_raised_under_thickening,
        lane.cross_roots_grown,
        lane.cross_roots_lapping,
        lane.brace_bands_refused,
        lane.mast_cells_refused,
        lane.mast_cells_refused_by_panel,
        lane.masts_extended,
        lane.mast_roots_overlapping_after_sizing,
        lane.tie_passes,
        if lane.fixed_point {
            "fixed point reached"
        } else {
            "NOT at a fixed point when the pass bound ran out (a WARNING, never a refusal)"
        },
        lane.longest_run_mm,
        lane.longest_run_limit_mm,
        if lane.longest_run_mm > lane.longest_run_limit_mm {
            " [OVER: a WARNING on an emitted file, never a refusal]"
        } else {
            ""
        },
    );
    // MEMBER CLEARANCE AND JOINT CREDITING (fix/member-clearance, 2026-09-29):
    // what the solid tests refused, what the joint rule saved, and which masts
    // the rules moved or refused. Each is a path-hit witness.
    println!(
        "[sweep {model}] member clearance: brace bands refused {} for coming within the final \
         gap of the model, {} for entering a member they are not joined to; mast cells refused \
         {} by the model, {} by a member (member tests measured on the drawn spans: {} \
         refused where the closest-axes reading admitted, {} admitted where it refused); \
         {} for needing a mast thicker than the member it \
         braces; {} masts placed at a cell needing a thinner mast than the nearest; {} crosses \
         built at a layer the member was already tied at (planes credited together there); \
         {} thickenings applied over a root lap that stood before the growth",
        lane.brace_bands_refused_by_model,
        lane.brace_bands_refused_by_member,
        lane.mast_cells_refused_by_model,
        lane.mast_cells_refused_by_member,
        lane.member_refusals_beyond_the_closest_axes,
        lane.member_admissions_the_closest_axes_refused,
        lane.mast_cells_refused_thicker_than_member,
        lane.masts_moved_to_a_thinner_cell,
        lane.crosses_at_an_existing_tie_layer,
        lane.thicken_over_existing_lap,
    );
    // FREE SHAFT (fix/brace-not-in-flare, 2026-09-30): bands whose bottom fell
    // inside a root flare or the raft. Path-hit witnesses.
    println!(
        "[sweep {model}] free shaft: {} brace bands moved up out of a root flare or the raft, \
         {} dropped",
        lane.brace_bands_moved_above_flare, lane.brace_bands_dropped_in_flare,
    );
    for warning in lane.warnings.iter().take(20) {
        println!(
            "[sweep {model}]   unbraced_run_over_limit: arm {} layers {}..{} runs {:.3} mm \
             against a limit of {:.3} mm",
            warning.strut_id,
            warning.k_lo,
            warning.k_top,
            warning.length_mm,
            warning.limit_mm,
        );
    }
    println!(
        "[sweep {model}] slenderness p50 {:.1}:1 max {:.1}:1; overloaded arms {}; \
         model sections over X_ALLOWED {}; peel field {:?} (area {:?}, tilt {:?}); \
         path hits: reach {} vertical {} spread {} arm-share {} injections {} max-combines {} \
         raster {} coarsening {} kappa {}/{} lift {} creep-cells {} creep-steps {}",
        report.slenderness_p50,
        report.slenderness_max,
        report.overloaded_struts.len(),
        report.model_section_overload.len(),
        report.peel_field_mode,
        report.peel_field_stats.beta_area_low_span,
        report.peel_field_stats.beta_tilt_span,
        report.counters.reach_tests,
        report.counters.vertical_inheritance_hits,
        report.counters.multi_carrier_split_hits,
        report.counters.strut_share_reads,
        report.counters.landing_injections,
        report.counters.max_combine_hits,
        report.counters.rasteriser_entries,
        report.counters.coarsening_loop_hits,
        report.counters.kappa_placement_reads,
        report.counters.kappa_audit_gate_reads,
        report.counters.lift_assertions,
        report.counters.creep_cells_weighted,
        report.counters.creep_steps_taken,
    );
    println!(
        "[sweep {model}] raft: {} pads ({} grown wider than the native pad to cover their foot, \
         {} feet sharing a pad), {} bridges, {:.4} mm3",
        emission.raft_pads,
        emission.raft_pads_grown,
        emission.raft_feet_sharing_a_pad,
        emission.raft_bridges,
        emission.raft_volume_mm3,
    );
    println!(
        "[sweep {model}] lowering: {} arms offered, {} emitted, {} dropped zero-length, \
         {} dropped invalid geometry, {} plate arms WITHOUT a root flare; \
         {} emit primitives; plate_z {:.4} mm; layer height {:.4} mm; \
         plan layers {} at {:.4} mm; printer {}; sweep wall time {:.2} s; digest {}",
        emission.arms_offered,
        emission.arms_emitted(),
        emission.arms_dropped_zero_length,
        emission.arms_dropped_invalid_geometry,
        emission.arms_without_root_flare,
        emission.primitives.len(),
        plan.plate_z_mm,
        plan.layer_height_mm,
        layers.heights_mm().len(),
        layers.heights_mm().first().copied().unwrap_or_default(),
        printer.id().as_str(),
        sweep_seconds,
        report.sweep_digest,
    );
    for overloaded in report.overloaded_struts.iter().take(20) {
        println!(
            "[sweep {model}] WARNING overloaded arm {} at layer {}: carries {:.4} mm2, \
             realized X {:.3} against X_ALLOWED {}",
            overloaded.strut_id,
            overloaded.layer,
            overloaded.carried_mm2,
            overloaded.realized_x,
            report.x_allowed,
        );
    }
    print_overloaded_arms(model, plan);
    // PER-EVENT SIZING (rule a′, 2026-09-29): what the order change did, and
    // its path-hit witnesses.
    let per_event = &report.per_event;
    println!(
        "[sweep {model}] per-event sizing: {}; {} events (largest {:.4} mm2), largest plate \
         reaction of one event {:.4} mm2, largest uncarried of one event {:.4} mm2, per-event \
         conservation residual {:.3e}; heaviest member {} carries {:.4} mm2 (the max-field \
         ledger it replaced: {:.4} mm2); {} routed entries, {} child hand-offs, {} model \
         hand-backs, {} same-event catches summed (largest gain {:.4} mm2), {:.4} mm2 on stale \
         landings; {} insertion triggers over the limit after {} pass(es)",
        if per_event.enabled {
            "ON"
        } else {
            "OFF (max-field ledger; sabotage)"
        },
        per_event.events,
        per_event.max_event_mm2,
        per_event.max_event_plate_mm2,
        per_event.max_event_uncarried_mm2,
        per_event.conservation_residual,
        per_event.worst_member_id,
        per_event.worst_member_mm2,
        per_event.worst_member_max_field_mm2,
        per_event.routed_entries,
        per_event.child_feeds,
        per_event.model_continuations,
        per_event.same_event_catches,
        per_event.same_event_catch_gain_max_mm2,
        per_event.stale_landing_mm2,
        per_event.triggers_over_limit,
        per_event.passes,
    );
    let lane = &report.congestion_lane;
    if lane.flank_relief_evaluated {
        println!(
            "[sweep {model}] flank relief: with the flank arms stacked {:.4} mm2, overloaded \
             arm-layers {}, uncarried {:.4} mm2; without them {:.4} mm2, {}, {:.4} mm2; {} flank \
             arms withdrawn",
            lane.flank_relief_with[0],
            lane.flank_relief_with[1],
            lane.flank_relief_with[2],
            lane.flank_relief_without[0],
            lane.flank_relief_without[1],
            lane.flank_relief_without[2],
            lane.flank_arms_withdrawn,
        );
    }
    // THE RIGID-BODY READING: a warning, never sizing.
    let rigid = &report.rigid_share;
    println!(
        "[sweep {model}] rigid-body share (diagnostic, sizes nothing): {}; {} events solved over \
         {} members in {} trees; heaviest share {:.4} mm2 on arm {}; {} arms above the limit; \
         largest compression {:.4} mm2; residual {:.3e}",
        if rigid.ran { "ran" } else { "not run" },
        rigid.events_solved,
        rigid.members,
        rigid.trees,
        rigid.max_share_mm2,
        rigid.max_share_id,
        rigid.over_limit.len(),
        rigid.compressive_max_mm2,
        rigid.conservation_residual,
    );
    for warning in rigid.over_limit.iter().take(20) {
        println!(
            "[sweep {model}] WARNING rigid-body share on arm {}: {:.4} mm2 at the event of layer \
             {} would realize X {:.3} at the largest legal radius (X_ALLOWED {}); it is sized for \
             {:.4} mm2 by its worst single event (root radius {:.4} mm; {} members land on it; it \
             lands on {:?}). A body that has cured stiff shares its peel by stiffness; check this \
             arm.",
            warning.strut_id,
            warning.share_mm2,
            warning.event_layer,
            warning.realized_x,
            report.x_allowed,
            warning.sized_mm2,
            warning.root_radius_mm,
            warning.members_landed_on_it,
            warning.landing,
        );
    }
    for section in report.model_section_overload.iter().take(20) {
        println!(
            "[sweep {model}] WARNING model section at layer {}: {:.4} mm2 of material carries \
             {:.4} mm2, realized X {:.3}. The engine supports models; it does not redesign them.",
            section.layer, section.component_area_mm2, section.carried_mm2, section.realized_x,
        );
    }
    if report.residual_demand_cells > 0 {
        println!(
            "[sweep {model}] WARNING {} demand cells ({:.4} mm2) no legal arm could reach. \
             This region is BARE on an emitted file.",
            report.residual_demand_cells, report.residual_demand_mm2,
        );
    }
}

/// One line per OVERLOADED ARM (not per arm-layer): what the member is and what
/// hangs on it, so an overload in a run log can be traced to its cause without
/// re-running the engine. Stdout only; the emitted file does not depend on it.
///
/// Batch-first: one pass over the overload list and one over the struts.
fn print_overloaded_arms(model: &ModelId, plan: &plinth_sweep::SweepPlan) {
    use std::collections::BTreeMap;
    // id -> (entries, lowest layer, highest layer, most carried mm2)
    let mut arms: BTreeMap<u32, (usize, usize, usize, f64)> = BTreeMap::new();
    for entry in &plan.report.overloaded_struts {
        let slot = arms
            .entry(entry.strut_id)
            .or_insert((0, usize::MAX, 0, 0.0));
        slot.0 += 1;
        slot.1 = slot.1.min(entry.layer);
        slot.2 = slot.2.max(entry.layer);
        slot.3 = slot.3.max(entry.carried_mm2);
    }
    if arms.is_empty() {
        return;
    }
    // id -> (children landed on it, their summed carried load at their own
    // bottom, braces or masts seated on it)
    let mut hangers: BTreeMap<u32, (usize, f64, usize)> = BTreeMap::new();
    for strut in &plan.struts {
        if let plinth_sweep::Landing::Strut { id, .. } = strut.landing {
            if arms.contains_key(&id) {
                let slot = hangers.entry(id).or_default();
                slot.0 += 1;
                slot.1 += strut.carried_mm2.first().copied().unwrap_or(0.0);
            }
        }
        if let Some(parent) = strut.seat_on_strut {
            if arms.contains_key(&parent) {
                hangers.entry(parent).or_default().2 += 1;
            }
        }
    }
    for (id, (entries, low, high, most)) in &arms {
        let Some(strut) = plan.struts.get(*id as usize) else {
            continue;
        };
        let (children, child_load, seated) = hangers.get(id).copied().unwrap_or_default();
        println!(
            "[sweep {model}] OVERLOADED ARM {id}: {entries} arm-layers over the limit, layers \
             {low}..{high}, at most {most:.4} mm2; {:?} k {}..{}, landing {:?}, cell {} at \
             ({:.3}, {:.3}) mm, inserted for load {}, moved by a remedy {}; {children} members \
             landed on it carrying {child_load:.4} mm2 at their own bottoms, {seated} braces or \
             masts seated on it; root radius {:.4} mm",
            strut.kind,
            strut.k_bottom,
            strut.k_top,
            strut.landing,
            strut.cell,
            strut.center_mm[0],
            strut.center_mm[1],
            strut.inserted_by_load,
            strut.straight_fallback.is_some(),
            strut.radius_mm.first().copied().unwrap_or(0.0),
        );
    }
}

/// The last hop of a successful chain: everything the run measured, moved into
/// the outcome the run report serializes and the CLI prints.
///
/// Split out of `run_model`'s match arm by task #49 so this hop can be pinned
/// on constructed inputs. It is a pure move — every field the chain measured
/// has to arrive at the report, and a field quietly replaced by an empty or
/// defaulted value here is invisible in a run log but changes what the report
/// claims. `a_chain_outcomes_disclosure_reaches_the_run_report` is the guard.
fn artifact_outcome(outcome: ChainOutcome) -> ModelPreparationOutcome {
    ModelPreparationOutcome::Artifact {
        emit: Box::new(outcome.emit),
        geometry: outcome.supported_geometry,
        hollow_disposition: outcome.hollow_disposition,
        cavity_repair: outcome.cavity_repair.map(Box::new),
        // FU-37 / FU-55: the wall the run asked for and the wall it got, so
        // the report on disk can tell two otherwise identical runs apart.
        wall_mm: outcome.wall,
        realized_wall: outcome.realized_wall,
        // 2026-08-13 owner ruling: legible on disk without re-deriving it —
        // "saved X mm3 against a registered Y mm3 minimum" for any model whose
        // disposition is a savings refusal.
        savings_shortfall: outcome.savings_shortfall,
        filled_back_chambers_ml: outcome.filled_back_chambers_ml,
        // Task #49: the artifact emitted, and this is the list of islands
        // nothing is charged to carry under it.
        ownerless_islands: outcome.ownerless_islands,
        // SPEC-A4 §A.8: the artifact emitted, and this is what the repair
        // controller spent to get it there.
        repair_report: outcome.repair_report.map(Box::new),
        // SPEC-A4 §D.2: and this is where to read what it BOUGHT — which
        // contact closed which hole, on the model that emitted.
        repair_ledger: outcome.repair_ledger.map(Box::new),
        // PR-8: the artifact emitted, and these are the loads it does not
        // carry and the tips it grew in order to carry the rest.
        contacts: Box::new(crate::ContactAttestationDisclosure {
            abandoned_contacts: outcome.abandoned_contacts,
            grown_contact_tips: outcome.grown_contact_tips,
        }),
        // Disclosure half of the pre-write gate's #1 finding: this artifact
        // was authorized on the gates that ran, not on all twelve, and this
        // is the list of the ones that did not.
        pending_gates: outcome.pending_gates,
        // WARNING MODE (owner ruling, 2026-08-29): the artifact emitted, and
        // this is the area-law evidence `PreWriteGate::Load` recorded rather
        // than refused on. `None` means no violation was found. Boxed on the
        // way into the run report's own enum -- see that field's doc.
        load_warnings: outcome.load_warnings.map(Box::new),
        // WARNING MODE (owner ruling, 2026-09-01): the artifact emitted, and
        // this is the coverage the gate measured it as NOT having. `None`
        // means the coverage verdict was accepted, never that it went unasked.
        coverage_warning: outcome.coverage_warning.map(Box::new),
    }
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
        // FU-54: the same rule for the cavity-repair pass ceiling. It reads
        // as a resource cap, not a shape, even though it fires downstream of
        // a partial repair the same way the two repair-caused geometry
        // refusals below do.
        assert!(!hollow_error_refuses_this_geometry(
            &HollowError::CavityRepairPassLimitExceeded {
                passes: 8,
                max_passes: 8,
                critical_cells: 1,
                repaired_voxels: 2,
                cell_visits: 27,
            }
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
        // FU-47's resin-flow connectivity rule (2026-08-12 ruling, §4): a
        // cavity that nowhere admits the registered drain bore is a geometry
        // finding, and the reserve is a legitimate answer to it -- `run`
        // converts it into `SolidDecisionReason::CavityBelowDrainThroat`
        // before it reaches here, same as the other four Ineligible* cases.
        assert!(hollow_error_refuses_this_geometry(
            &HollowError::IneligibleCavityBelowDrainThroat {
                throat_radius_mm: 1.5,
            }
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
                saved_mm3: 100.0,
                threshold_mm3: 1500.0,
            }
        ));
        // FU-55: the realized-wall check's own refusal. `run` converts this
        // into `ShellUnderWalled` before it ever reaches here, but if it ever
        // did propagate, "keep the solid" is what it means — a hollow
        // refusal is not a model rejection.
        assert!(hollow_error_refuses_this_geometry(
            &HollowError::RealizedWallBelowSpec {
                realized_wall_mm: 0.27,
                minimum_wall_mm: 2.0,
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
            reason: plinth_support::CoverageClosureReason::NoImprovingRepair {
                candidates_tried: 8,
                best_remaining_score_mm2: 0.0,
            },
            islands: Vec::new(),
            steps: Vec::new(),
            repair: None,
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

    /// **SPEC-A4 §A.8's third destination survives the projection into BOTH
    /// non-emitting dispositions.**
    ///
    /// `failure()` is the one funnel every stop passes through on its way to
    /// the run report, and until 2026-08-20 it dropped the repair ledger on
    /// the way — `Refused` and `Failed` had nowhere to put it, so the report
    /// stated what the controller spent only when the model EMITTED. §A.8
    /// says "on every path — sealed or refused", and the refused path is the
    /// one ruling R10 deleted the failed-tries cap on.
    ///
    /// The two shapes are pinned separately because they are reached by
    /// different attachment points in `seal_and_emit`: a pre-write gate
    /// refusal reads the ledger off the still-live authority, and a
    /// controller refusal reads it off the typed refusal, there being no
    /// authority on that path.
    #[test]
    fn a_refused_and_a_failed_disposition_both_carry_what_the_controller_spent() {
        let model = ModelId::new("fixture");
        let spent = plinth_support::RepairRunReport {
            trials: 45,
            commits: 0,
            candidates_remaining: 0,
            ..plinth_support::RepairRunReport::default()
        };

        // The gate refused a model the controller had already spent on.
        let gate_stop = ChainStop::refused(
            PipelineError::Gates {
                model: model.clone(),
                source: GateRefusal::ConstantsUnavailable,
            },
            GateStop {
                gate: Some(crate::PreWriteGate::UncoveredIslands),
                geometry: SupportedGeometry::DrilledShellSealed,
            },
        )
        .with_repair_report(Some(spent.clone()));
        assert_eq!(gate_stop.repair_report(), Some(&spent));
        let refused = failure(&gate_stop);
        assert!(matches!(refused, ModelPreparationOutcome::Refused { .. }));
        assert_eq!(refused.repair_report(), Some(&spent));

        // The controller itself refused, and its ledger came off the refusal.
        let closure_stop = ChainStop::new(PipelineError::SealedRun {
            model: model.clone(),
            source: plinth_support::SupportRunError::CoverageClosure(Box::new(
                plinth_support::CoverageClosureRefusal {
                    attempts: 0,
                    reason: plinth_support::CoverageClosureReason::NoImprovingRepair {
                        candidates_tried: 8,
                        best_remaining_score_mm2: 0.0,
                    },
                    islands: Vec::new(),
                    steps: Vec::new(),
                    repair: Some(spent.clone()),
                },
            )),
        })
        .with_repair_report(Some(spent.clone()));
        let failed = failure(&closure_stop);
        assert!(matches!(failed, ModelPreparationOutcome::Failed { .. }));
        assert_eq!(failed.repair_report(), Some(&spent));

        // And a stop from before any controller ran carries no ledger, which
        // is a measurement: "the loop never ran" is not "the loop ran and
        // spent nothing".
        let early = failure(&ChainStop::new(PipelineError::DegenerateExtent { model }));
        assert_eq!(early.repair_report(), None);
    }

    /// A shell that was built and drilled, and then refused, is a stop of a
    /// known shape: the sealed run failed during `Lowering`. Used by both
    /// tests below.
    fn shell_refusal_stop(model: &ModelId) -> ChainStop {
        ChainStop::new(PipelineError::SealedRun {
            model: model.clone(),
            source: SupportRunError::Failed {
                phase: plinth_support::SupportRunPhase::Lowering,
                detail: "root run 280 is shorter than the registered minimum".to_owned(),
            },
        })
        .measured_against("faces=496052 max_grid_bytes=67108864 knn_max_work=4000000".to_owned())
    }

    /// A closed oriented solid to substitute, built the cheap procedural way.
    fn oriented_solid_parts() -> (plinth_mesh::Mesh, plinth_orient::OrientationHistory) {
        let mesh =
            plinth_mesh::make_box([20.0, 20.0, 20.0], [0.0, 0.0, 0.0]).expect("procedural box");
        let budget =
            DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
                .expect("ratified decimation budget");
        let (mesh, decimation) = run_decimation(&mesh, budget)
            .expect("decimate the procedural box")
            .into_decimated_model()
            .expect("print-scale procedural box")
            .into_parts();
        let orientation = plinth_orient::OrientationHistory::from_reports(
            decimation,
            plinth_orient::OrientationReport {
                provenance: plinth_orient::OrientationProvenance::Pinned {
                    pinned_report: PathBuf::from("substitution-fixture.pipeline.json"),
                },
                selection: plinth_orient::BaselineSelection::Identity,
                transform: plinth_orient::RigidTransform::identity(),
            },
        );
        (mesh, orientation)
    }

    /// **The substituted solid says what happened to the shell, and does not
    /// claim a savings measurement nobody took.**
    ///
    /// This path used to hand `solid_decision_recording_repair` a hardcoded
    /// `SolidDecisionReason::BelowThreshold`, so a model whose 496,052-face
    /// drilled shell had just been refused reported `Solid { reason:
    /// BelowThreshold }` — "the cavity would have saved too little" — beside
    /// the line saying the shell existed and was refused. Nothing on this path
    /// measures a saving.
    #[test]
    fn the_substituted_solid_records_the_shell_refusal_not_a_savings_verdict() {
        let model = ModelId::new("fixture");
        let stop = shell_refusal_stop(&model);
        let refused_shell = DrilledShellRefusal::from_stop(496_052, &stop);
        assert_eq!(
            refused_shell.stage,
            ShellRefusalStage::SealedRun {
                phase: Some(plinth_support::SupportRunPhase::Lowering)
            },
            "the stage comes off the typed error, not out of the message"
        );

        let (mesh, orientation) = oriented_solid_parts();
        let solid = substitute_solid_for_refused_shell(
            &model,
            mesh,
            orientation,
            None,
            None,
            &refused_shell,
        )
        .expect("the oriented solid substitutes");

        let disposition = solid.history().hollow().disposition();
        assert_eq!(
            disposition,
            HollowDisposition::Solid {
                reason: SolidDecisionReason::DrilledShellRefused
            },
            "the substitution must record that a drilled shell was refused"
        );
        assert_ne!(
            disposition,
            HollowDisposition::Solid {
                reason: SolidDecisionReason::BelowThreshold
            },
            "nothing on this path measured a saving, so nothing may report one"
        );
        // The savings-gate refusal's own measurement is `None` here, which is
        // the same claim stated the other way: there was no savings verdict.
        assert!(solid.history().hollow().savings_shortfall().is_none());
    }

    /// **The hollow-stage-error fallback (case 0) says the stage failed, and
    /// does not claim a savings measurement nobody took.**
    ///
    /// The #33 fix (`5d5b249`) closed this imprecision for the support-refusal
    /// path (case 3) and named this one -- "the hollow stage erroring out
    /// entirely" -- a known remaining gap in `SupportedGeometry::
    /// HollowStageRefused`'s own doc. This test closes it: no shell is ever
    /// built on this path, so `Solid { reason: BelowThreshold }` -- "the
    /// savings gate measured this cavity and found it under the registered
    /// minimum" -- was never true here either.
    #[test]
    fn the_hollow_stage_error_fallback_records_the_stage_failed_not_a_savings_verdict() {
        let model = ModelId::new("fixture");
        let (mesh, orientation) = oriented_solid_parts();
        let solid =
            substitute_solid_for_hollow_stage_failure(&model, mesh, orientation, None, None)
                .expect("the oriented solid substitutes");

        let disposition = solid.history().hollow().disposition();
        assert_eq!(
            disposition,
            HollowDisposition::Solid {
                reason: SolidDecisionReason::HollowStageRefused
            },
            "the fallback must record that the hollow stage itself failed"
        );
        assert_ne!(
            disposition,
            HollowDisposition::Solid {
                reason: SolidDecisionReason::BelowThreshold
            },
            "nothing on this path measured a saving, so nothing may report one"
        );
        // Distinct from case 3's reason too: no shell ever existed here for a
        // later stage to refuse.
        assert_ne!(
            disposition,
            HollowDisposition::Solid {
                reason: SolidDecisionReason::DrilledShellRefused
            },
            "this path never built a shell at all, so it is not the shell-refusal case"
        );
        assert!(solid.history().hollow().savings_shortfall().is_none());
    }

    /// **A geometry record written before `stage` existed still reads.**
    ///
    /// The field is `#[serde(default)]` for exactly this: the coverage sidecar
    /// and the serialized run report are both read by things this workspace
    /// does not rebuild, and a required field would turn every stored record
    /// into a parse error. `None` says the record predates the field; it never
    /// says no stage refused the shell.
    #[test]
    fn a_geometry_record_written_before_the_stage_field_still_deserializes() {
        let stored = r#"{"kind":"drilled_shell_refused_by_support",
                         "drilled_faces":496052,"refusal":"an older run's message"}"#;
        let geometry: SupportedGeometry =
            serde_json::from_str(stored).expect("a stored record still reads");
        assert_eq!(
            geometry,
            SupportedGeometry::DrilledShellRefusedBySupport {
                drilled_faces: 496_052,
                stage: None,
                refusal: "an older run's message".to_owned(),
            }
        );

        // And a record this pipeline writes round-trips with its stage.
        let written = SupportedGeometry::DrilledShellRefusedBySupport {
            drilled_faces: 496_052,
            stage: Some(ShellRefusalStage::SealedRun {
                phase: Some(plinth_support::SupportRunPhase::Preparation),
            }),
            refusal: "this run's message".to_owned(),
        };
        let json = serde_json::to_string(&written).expect("serialize");
        assert_eq!(
            serde_json::from_str::<SupportedGeometry>(&json).expect("round trip"),
            written
        );
    }

    /// **A shell that was built and refused stays on the record when the
    /// solid substituted for it fails too.**
    ///
    /// The retry used to return the solid's own error and nothing else, so for
    /// the Giant Bat, the treant and the swallowing toad it was not knowable
    /// from the report whether a shell had ever been built. Both dispositions
    /// `failure` can produce read the same `detail()`, so both are pinned.
    #[test]
    fn a_failure_after_substitution_keeps_the_shell_refusal_record() {
        let model = ModelId::new("fixture");
        let refused_shell = DrilledShellRefusal::from_stop(496_052, &shell_refusal_stop(&model));

        // The substituted solid's own failure, at a different stage.
        let stop = ChainStop::new(PipelineError::SealedRun {
            model: model.clone(),
            source: SupportRunError::Failed {
                phase: plinth_support::SupportRunPhase::Routing,
                detail: "the corrupt endpoint mask refused the route".to_owned(),
            },
        })
        .after_refused_shell(refused_shell);

        let detail = stop.detail();
        assert!(
            detail.contains("the corrupt endpoint mask refused the route"),
            "the solid's own failure is still what this stop is about: {detail}"
        );
        assert!(
            detail.contains(
                "a hollow shell was built and drilled (496052 faces) and refused at \
                 the sealed support run during Lowering"
            ),
            "the shell that was built and refused must survive the substitution: {detail}"
        );
        assert!(
            detail.contains("root run 280 is shorter than the registered minimum"),
            "the shell's own refusal detail travels with it: {detail}"
        );
        assert_eq!(
            stop.refused_shell().map(|shell| shell.drilled_faces),
            Some(496_052),
            "the record is a value, not only a sentence"
        );

        let ModelPreparationOutcome::Failed { detail, .. } = failure(&stop) else {
            panic!("a sealed-run failure is this model's Failed disposition");
        };
        assert!(
            detail.contains("a hollow shell was built and drilled"),
            "the run report is where the record has to arrive: {detail}"
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

    /// Task #49 — **an ownerless-island disclosure the chain measured reaches
    /// the run report the operator reads.**
    ///
    /// The ruling this branch implements is "the run report must DISCLOSE
    /// ownerless islands so nothing is hidden". The producer is pinned in
    /// `plinth-support`; this pins the last hop, where the measurement becomes
    /// the reported outcome. Without it, `ownerless_islands: Vec::new()` here
    /// would emit an artifact that silently claims every face it covers has
    /// something sized to carry it — and every existing assertion, which is
    /// `is_empty()` on a healthy fixture, would still pass.
    ///
    /// The disclosure is deserialized rather than constructed because its
    /// fields are private to `plinth-support`, and that is the same route a
    /// frozen report on disk takes back in, so the numbers below are the
    /// numbers a reader sees.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn a_chain_outcomes_disclosure_reaches_the_run_report() {
        let disclosure: plinth_support::OwnerlessIslandDisclosure = serde_json::from_str(
            r#"{"island_id":3,"ownerless_face_count":1,
                "ownerless_face_area_mm2":0.00046970077,
                "measured_uncovered_area_mm2":0.00046970077}"#,
        )
        .expect("a disclosure deserializes exactly as a frozen report carries it");
        let islandless: plinth_support::OwnerlessIslandDisclosure = serde_json::from_str(
            r#"{"island_id":null,"ownerless_face_count":2,
                "ownerless_face_area_mm2":0.5,
                "measured_uncovered_area_mm2":null}"#,
        )
        .expect("an islandless disclosure states that it has no measurement");
        // PR-8 grow-to-attest, same last hop and the same failure mode: an
        // `abandoned_contacts: Vec::new()` here would emit an artifact that
        // silently claims every routed load is carried, and every `is_empty()`
        // assertion on a healthy fixture would still pass.
        let abandoned: plinth_support::AbandonedContactDisclosure = serde_json::from_str(
            r#"{"contact_id":41,"seat_mm":[1.5,-2.25,3.0],
                "abandoned_area_mm2":2.98,"refusal":"not_attached",
                "law_tip_radius_mm":0.05,"growth_ceiling_radius_mm":0.4,
                "growth_ceiling_constant":"TIP_RADIUS_MAX_GROWN_SCAR_MM",
                "growth_ladder_entered":true,
                "growth_rungs":[
                    {"radius_mm":0.07071067811865475,"diameter_mm":0.1414213562373095,
                     "attested":false},
                    {"radius_mm":0.4,"diameter_mm":0.8,"attested":false}],
                "reseat":"no_alternative_seat_at_this_stage"}"#,
        )
        .expect("an abandoned-contact disclosure deserializes as a frozen report carries it");
        let grown: plinth_support::GrownContactTipDisclosure = serde_json::from_str(
            r#"{"contact_id":7,"seat_mm":[4.125,-0.5,12.75],
                "law_tip_radius_mm":0.05,
                "emitted_tip_radius_mm":0.08132887446985768,
                "emitted_tip_diameter_mm":0.16265774893971535,
                "kept_area_mm2":0.42,
                "growth_rungs":[
                    {"radius_mm":0.07071067811865475,"diameter_mm":0.1414213562373095,
                     "attested":false},
                    {"radius_mm":0.1,"diameter_mm":0.2,"attested":true},
                    {"radius_mm":0.0901825229575319,"diameter_mm":0.1803650459150638,
                     "attested":true},
                    {"radius_mm":0.08132887446985768,"diameter_mm":0.16265774893971535,
                     "attested":true},
                    {"radius_mm":0.0733444308898817,"diameter_mm":0.1466888617797634,
                     "attested":false}]}"#,
        )
        .expect("a grown-tip disclosure deserializes as a frozen report carries it");

        let outcome = ChainOutcome {
            model: ModelId::new("deinonychus"),
            source_faces: 1,
            prepared_faces: 1,
            hollow_disposition: HollowDisposition::Solid {
                reason: plinth_hollow::SolidDecisionReason::BelowThreshold,
            },
            cavity_repair: None,
            wall: None,
            realized_wall: None,
            savings_shortfall: None,
            filled_back_chambers_ml: Vec::new(),
            supported_geometry: SupportedGeometry::HollowStageDecidedSolid,
            ownerless_islands: vec![disclosure, islandless],
            // SPEC-A4 §A.8: what the one-winner controller spent. Distinct,
            // non-default numbers so a field quietly replaced by `None` or by
            // `Default::default()` on the hop below is visible rather than
            // indistinguishable from a model that paid nothing.
            repair_report: Some(plinth_support::RepairRunReport {
                trials: 45,
                commits: 3,
                candidates_remaining: 1_008,
                diversity_demotions: 44,
                diversity_demotions_tried: 44,
                ..plinth_support::RepairRunReport::default()
            }),
            repair_ledger: Some(crate::coverage_evidence::SealedRepairLedger {
                path: std::path::PathBuf::from("deinonychus.coverage.repair.json"),
                commits: 3,
                trials: 45,
                steps: 45,
                write_error: None,
            }),
            abandoned_contacts: vec![abandoned.clone()],
            grown_contact_tips: vec![grown.clone()],
            routed_contacts: 0,
            final_live_members: 0,
            snapshot_digest: 0,
            // Non-empty and non-default on purpose: this is the field the
            // guard exists to protect. An empty `Vec::new()` here could not
            // tell "the eleven pending gates survived the hop" apart from
            // "the field was dropped and defaulted to empty" -- see
            // `crate::PreWriteGate::ALL` for the full twelve.
            pending_gates: vec![
                PreWriteGate::Clearance,
                PreWriteGate::Load,
                PreWriteGate::BallBreach,
            ],
            // WARNING MODE. Non-`None` and non-default on purpose, for the
            // same reason `pending_gates` above is: an empty/`None` value
            // here could not tell "the gate's evidence survived the hop"
            // apart from "the field was dropped and defaulted away".
            load_warnings: Some(crate::gates::LoadWarnings {
                judged: 90,
                x_allowed: 12.0,
                violations: vec![crate::gates::LoadLawViolation {
                    contact_id: 19,
                    position_mm: Some([-15.0, -5.625, 15.938]),
                    carried_area_mm2: Some(450.0),
                    emitted_tip_radius_mm: Some(0.05),
                    required_tip_radius_mm: Some(3.454_942_539_921_643_5),
                    fault: crate::gates::LoadLawFault::TipSmallerThanTheAreaLawDemands,
                }],
            }),
            // WARNING MODE (owner ruling, 2026-09-01). Non-`None` for the same
            // reason as the two fields above: this hop is the only thing
            // between the gate's measurement of an unsupported region and the
            // report a reader sees, and a dropped field would read back as "the
            // file covers everything".
            coverage_warning: Some(crate::coverage_evidence::CoverageGapWarning {
                islands: vec![crate::coverage_evidence::UncoveredIslandSummary {
                    island_id: 4,
                    uncovered_area_mm2: 3.5,
                    worst_gap: plinth_support::CoverageClosureGap::NoRoutedContact,
                    face_ids: vec![11, 12],
                    search_outcome: None,
                }],
                total_uncovered_mm2: 3.5,
                closure_reason: Some("NoImprovingRepair".to_owned()),
                closure_attempts: 2,
            }),
            primitive_count: 0,
            lift_mm: 5.0,
            plate_z_mm: 0.0,
            emit: plinth_emit::EmitReport {
                artifact_path: std::path::PathBuf::from("deinonychus.chitubox"),
                support_record_count: 0,
                artifact_bytes: 0,
                seat_trim: Box::default(),
                audit_findings: Vec::new(),
            },
        };

        let ModelPreparationOutcome::Artifact {
            ownerless_islands,
            contacts,
            repair_report,
            pending_gates,
            load_warnings,
            coverage_warning,
            ..
        } = artifact_outcome(outcome)
        else {
            panic!("a completed chain reports an artifact");
        };
        // WARNING MODE (owner ruling, 2026-08-29): the Load gate's evidence
        // must reach the report unchanged, not get dropped on this hop and
        // silently read back as "the gate found nothing".
        assert_eq!(
            load_warnings,
            Some(Box::new(crate::gates::LoadWarnings {
                judged: 90,
                x_allowed: 12.0,
                violations: vec![crate::gates::LoadLawViolation {
                    contact_id: 19,
                    position_mm: Some([-15.0, -5.625, 15.938]),
                    carried_area_mm2: Some(450.0),
                    emitted_tip_radius_mm: Some(0.05),
                    required_tip_radius_mm: Some(3.454_942_539_921_643_5),
                    fault: crate::gates::LoadLawFault::TipSmallerThanTheAreaLawDemands,
                }],
            })),
            "the Load gate's warning-mode evidence must reach the run report unchanged"
        );
        // WARNING MODE (owner ruling, 2026-09-01), same guard one field over:
        // the coverage gap the artifact carries must reach the report whole.
        // A dropped field here reads back as a fully covered model, which is
        // the exact silence the ruling traded a refusal for.
        assert_eq!(
            coverage_warning.as_deref(),
            Some(&crate::coverage_evidence::CoverageGapWarning {
                islands: vec![crate::coverage_evidence::UncoveredIslandSummary {
                    island_id: 4,
                    uncovered_area_mm2: 3.5,
                    worst_gap: plinth_support::CoverageClosureGap::NoRoutedContact,
                    face_ids: vec![11, 12],
                    search_outcome: None,
                }],
                total_uncovered_mm2: 3.5,
                closure_reason: Some("NoImprovingRepair".to_owned()),
                closure_attempts: 2,
            }),
            "the coverage gap an emitted artifact carries must reach the run report unchanged"
        );
        // The disclosure this guard exists to protect: the pending-gate list
        // must reach the report unchanged, not get dropped on this hop and
        // silently read back as "every gate ran".
        assert_eq!(
            pending_gates,
            vec![
                PreWriteGate::Clearance,
                PreWriteGate::Load,
                PreWriteGate::BallBreach,
            ],
            "the gates that did not run must reach the run report unchanged"
        );
        // SPEC-A4 §A.8's third destination. Ruling R10 traded the failed-tries
        // cap for loud reporting, and a report that is dropped on this hop is
        // exactly the silence the ruling refused. Asserted as the whole value,
        // so a hop that kept `trials` and forgot the diversity pair fails here.
        assert_eq!(
            repair_report.as_deref(),
            Some(&plinth_support::RepairRunReport {
                trials: 45,
                commits: 3,
                candidates_remaining: 1_008,
                diversity_demotions: 44,
                diversity_demotions_tried: 44,
                ..plinth_support::RepairRunReport::default()
            }),
            "what the repair controller spent must reach the run report unchanged"
        );
        // PR-8. The refused contact's ABANDONED LOAD and the ladder that failed
        // to save it must both survive the hop: an operator who cannot see the
        // 2.98 mm² cannot tell a refused support from one that vanished.
        let crate::ContactAttestationDisclosure {
            abandoned_contacts,
            grown_contact_tips,
        } = *contacts;
        assert_eq!(abandoned_contacts, vec![abandoned]);
        assert_eq!(abandoned_contacts[0].contact_id(), 41);
        assert_eq!(abandoned_contacts[0].abandoned_area_mm2(), 2.98);
        assert_eq!(abandoned_contacts[0].refusal(), "not_attached");
        assert_eq!(abandoned_contacts[0].growth_rungs().len(), 2);
        assert!(abandoned_contacts[0]
            .growth_rungs()
            .iter()
            .all(|rung| !rung.attested()));
        assert_eq!(abandoned_contacts[0].growth_ceiling_radius_mm(), 0.4);
        // The bound is NAMED all the way through to the run report, so the
        // ABANDONED LOAD line can say WHICH rule stopped the rescue.
        assert_eq!(
            abandoned_contacts[0].growth_ceiling_constant(),
            "TIP_RADIUS_MAX_GROWN_SCAR_MM"
        );
        // ...and whether that named maximum actually stopped anything. This row
        // walked the ladder, so it did. A row that never entered the ladder
        // carries `false` here and the run report says in words that the
        // maximum is not the reason its support is missing.
        assert!(abandoned_contacts[0].growth_ladder_entered());
        assert_eq!(
            abandoned_contacts[0].reseat(),
            "no_alternative_seat_at_this_stage"
        );
        // And the cure's own row: WHERE the scar is, what the law asked for,
        // what was built, in BOTH units, and the load the growth kept.
        assert_eq!(grown_contact_tips, vec![grown]);
        assert_eq!(grown_contact_tips[0].seat_mm(), [4.125, -0.5, 12.75]);
        assert_eq!(grown_contact_tips[0].law_tip_radius_mm(), 0.05);
        assert_eq!(
            grown_contact_tips[0].emitted_tip_radius_mm(),
            0.081_328_874_469_857_68
        );
        assert_eq!(
            grown_contact_tips[0].emitted_tip_diameter_mm(),
            0.162_657_748_939_715_35
        );
        assert_eq!(grown_contact_tips[0].kept_area_mm2(), 0.42);
        // The record carries the ladder AND the scan, so the LAST rung is a
        // failed scan step and the cure is the smallest ATTESTING one. A report
        // that only ever asserted `last().attested()` would go green again the
        // day the scan is deleted.
        assert_eq!(grown_contact_tips[0].growth_rungs().len(), 5);
        assert!(!grown_contact_tips[0]
            .growth_rungs()
            .last()
            .unwrap()
            .attested());
        let smallest_attesting_mm = grown_contact_tips[0]
            .growth_rungs()
            .iter()
            .filter(|rung| rung.attested())
            .map(plinth_support::TipGrowthRungDisclosure::radius_mm)
            .fold(f64::INFINITY, f64::min);
        assert_eq!(
            smallest_attesting_mm,
            grown_contact_tips[0].emitted_tip_radius_mm(),
            "the emitted tip must be the SMALLEST radius the record shows \
             attesting, over the ladder and the scan together"
        );
        assert_eq!(
            ownerless_islands,
            vec![disclosure, islandless],
            "the report must carry the run's own disclosure, unedited and in order"
        );
        // Both numbers, and the difference between "measured nothing" and "no
        // measurement exists", survive the hop. These are what the CLI prints.
        assert_eq!(ownerless_islands[0].island_id(), Some(3));
        assert_eq!(ownerless_islands[0].ownerless_face_count(), 1);
        assert_eq!(
            ownerless_islands[0].measured_uncovered_area_mm2(),
            Some(0.000_469_700_77)
        );
        assert_eq!(ownerless_islands[1].island_id(), None);
        assert_eq!(ownerless_islands[1].measured_uncovered_area_mm2(), None);
    }
}
