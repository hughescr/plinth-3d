//! Design phase E4: `plinth_pipeline::prepare` over a real folder.
//!
//! `tests/first_artifact.rs` proves the *chain* joins up on one model. This
//! proves the **executor** does: a folder of source `.stl` in, one
//! `.chitubox` per model that passes the pre-write gates out, and one typed
//! disposition per model either way.
//!
//! Every fixture here is a **committed** file under `corpus/baseline/`, never
//! a provisioned corpus model — so this test runs in a fresh clone with
//! nothing symlinked. `corpus/baseline/*.prepped.stl` are checked in;
//! `corpus/models/` is not.
//!
//! **A finding this test surfaced, recorded rather than papered over — and
//! rewritten twice since.** It used to read: no checked-in `.prepped.stl` is
//! refused by the pre-write gates, because both
//! `make_overhang_table.prepped.stl` and `Sphere.prepped.stl` seal and emit
//! when run through the whole chain, while the *procedural*
//! `make_overhang_table()` and the provisioned `corpus/models/Sphere.stl` are
//! refused for uncovered islands (`tests/first_artifact.rs`). That stopped
//! being true of `Sphere.prepped.stl` on 2026-08-10 (the cavity-field
//! exterior fix stopped its cavity breaching the wall at its poles, so its
//! drilled shell seals and then fails coverage on its own merits), and moved
//! a second time on 2026-08-12 when FU-53 landed (the exact nearest-surface
//! cavity selection plus `HOLLOW_CAVITY_MARCH_MARGIN_MM` shrinks the cavity
//! honestly, and on this doubly-decimated, doubly-oriented fixture the
//! resulting support demand exceeds the sealed run's registered routing
//! capacity before the gate is ever reached — `DrilledShellRefusedBySupport`
//! fires and the existing OQ-1/FU-34 reserve substitution converts it to an
//! emitted solid). **`feature/coverage-closure` (FU-56/FU-57) landed the same
//! day and does not move this fixture a third time:** measured directly
//! against the merged tree, `Sphere.prepped.stl`'s attempt 0 still fails
//! during `Routing`, which precedes `CoverageMeasurement` in the stage order
//! and keeps the pre-FU-56 `Failed(phase)` semantics by the controller's own
//! design (attempt 0's plan is the model's own, not the controller's
//! extension of it) — so the coverage-closure controller never gets a look
//! at this fixture and the FU-53-only route stands. `make_raking_blade.prepped.stl`
//! DID move on 2026-08-12 (FU-56 measures routed coverage inside
//! `run_support`, and that fixture's two residual islands have no
//! repair-pool candidate the CCO transcript ever admits, so it is now a
//! genuine coverage-closure refusal). See
//! `a_model_with_no_pinned_baseline_is_refused_rather_than_given_a_neighbours`
//! for the full attribution. No cap, gate or bound moved to produce any of
//! this — each move is the geometry, or the measurement, changing under an
//! unmoved rule.
//!
//! Deliberately absent: any new frozen digest (design record OQ-3). Everything
//! asserted is a completion, disposition or path property that a changed
//! member count cannot break.

use std::path::{Path, PathBuf};

use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    OrientationPolicy, OutputProfile, PipelineExitCode, PrepareReport, PrepareRequest, StageStop,
    SupportedGeometry,
};

#[path = "../examples/shared/first_artifact_chain.rs"]
mod chain;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn committed_fixture(name: &str) -> PathBuf {
    let path = workspace_root().join("corpus/baseline").join(name);
    assert!(
        path.is_file(),
        "committed fixture {} is missing; corpus/baseline is checked-in evidence",
        path.display()
    );
    path
}

/// A per-test scratch folder, removed on the way in and on the way out.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("plinth-prepare-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("in")).expect("scratch input folder");
        Self(path)
    }

    fn input(&self) -> PathBuf {
        self.0.join("in")
    }

    /// The output folder is deliberately NOT created: `prepare` owns creating
    /// it, and that is part of what this test gates.
    fn output(&self) -> PathBuf {
        self.0.join("out")
    }

    /// Place a committed fixture into the input folder under a chosen stem, so
    /// the stem -> artifact-name mapping is visible rather than incidental.
    fn place(&self, fixture: &str, stem: &str) {
        std::fs::copy(
            committed_fixture(fixture),
            self.input().join(format!("{stem}.stl")),
        )
        .expect("place fixture");
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.input().join(name), bytes).expect("write scratch input");
    }

}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A request whose orientation is pinned per model, by source stem — or, when
/// no baselines are named, the default dense sweep.
///
/// `OrientationPolicy::Pinned` is a **map**: a pinned baseline records one
/// mesh's rotation and `plinth_orient::run` cannot tell that it was handed a
/// different mesh, so each model in a folder names its own or is refused. The
/// dense sweep has no such hazard — it measures the mesh in front of it — which
/// is why it, and not a pin, is the default.
fn request(scratch: &Scratch, orientation: &[(&str, &str)]) -> PrepareRequest {
    let printer = OutputProfile::Saturn4Ultra
        .resolve()
        .expect("M0 Saturn profile")
        .clone();
    let orientation = if orientation.is_empty() {
        OrientationPolicy::DenseSweep
    } else {
        OrientationPolicy::Pinned(
            orientation
                .iter()
                .map(|(stem, baseline)| {
                    (
                        (*stem).to_owned(),
                        plinth_orient::PinnedBaseline {
                            pipeline_report: committed_fixture(baseline),
                        },
                    )
                })
                .collect(),
        )
    };
    PrepareRequest {
        input_models_dir: scratch.input(),
        output_dir: scratch.output(),
        orientation,
        output_profile: OutputProfile::Saturn4Ultra,
        decimation_budget: registered_decimation_budget().expect("registered budget"),
        hollow: plinth_hollow::HollowRequest::with_registered_wall_default(printer.clone())
            .expect("registered wall"),
        support: registered_support_request(&printer).expect("registered lift"),
        no_cache: false,
        stop_after: None,
    }
}

/// The single-model pinned request every fixture test here uses: one stem, the
/// baseline that belongs to it.
fn pinned(scratch: &Scratch, stem: &str, baseline: &str) -> PrepareRequest {
    request(scratch, &[(stem, baseline)])
}

fn outcome<'a>(report: &'a PrepareReport, model: &str) -> &'a ModelPreparationOutcome {
    &report
        .models
        .iter()
        .find(|entry| entry.model.as_str() == model)
        .unwrap_or_else(|| panic!("model {model} is missing from the report"))
        .outcome
}

/// **The load-bearing test: a folder in, a real `.chitubox` out.**
///
/// It also proves the executor's other half in the same run — the deliberately
/// corrupt `zz-not-a-model.stl` is refused *individually*, and the good model
/// beside it still reaches disk. One bad model must never cost the folder its
/// artifacts, and sorting the sources puts the bad one last so a bug that
/// aborted the loop would still have emitted; the second fixture is named to
/// sort *after* the good one for exactly that reason... and a third, sorting
/// *before* it, closes the other direction.
#[test]
fn a_folder_of_models_emits_one_artifact_each_and_one_bad_model_costs_only_itself() {
    let scratch = Scratch::new("folder");
    scratch.place("make_toed_slab.prepped.stl", "mini");
    scratch.write("aa-not-a-model.stl", b"neither binary nor ASCII STL");
    scratch.write("zz-not-a-model.stl", b"nor is this one");

    let report = prepare(pinned(&scratch, "mini", "make_toed_slab.pipeline.json"))
        .expect("the run itself must succeed even though two models in it cannot");

    // The output folder did not exist; `prepare` owns creating it.
    assert!(
        scratch.output().is_dir(),
        "prepare must create its own output folder"
    );

    // Deterministic order: sorted by source path, and the report follows it.
    assert_eq!(
        report
            .models
            .iter()
            .map(|entry| entry.model.as_str())
            .collect::<Vec<_>>(),
        ["aa-not-a-model", "mini", "zz-not-a-model"],
        "models must be reported in the executor's own sorted source order"
    );

    // The good model reached a real artifact at the stem-derived path.
    let artifact = scratch.output().join("mini.chitubox");
    let ModelPreparationOutcome::Artifact {
        emit,
        geometry,
        hollow_disposition,
        cavity_repair,
        wall_mm,
        realized_wall,
    } = outcome(&report, "mini")
    else {
        panic!(
            "mini must reach an artifact; it reported {:?}",
            outcome(&report, "mini")
        );
    };
    assert_eq!(emit.artifact_path, artifact);
    // **What is in the file, not merely that there is one.** Two of the four
    // geometries are substitutions the chain fell back to, and a report that
    // said only "emitted" could not tell them from the intended shell.
    assert_eq!(
        geometry,
        &SupportedGeometry::HollowStageDecidedSolid,
        "the emitted geometry changed; the artifact is not what it was"
    );
    assert_eq!(
        hollow_disposition,
        &plinth_hollow::HollowDisposition::Solid {
            reason: plinth_hollow::SolidDecisionReason::BelowThreshold
        }
    );
    // The repair reaches the report. This model decided solid before a cavity
    // was ever selected, so there is nothing to record -- and `None` is the
    // claim "no repair result exists here", which is what distinguishes it from
    // a recorded repair of size zero.
    assert_eq!(cavity_repair, &None);
    // **FU-37 reaches the run's own output.** The requested wall is on the
    // artifact record, so this run is distinguishable from the same run at a
    // different `--wall-mm`. It is the request's wall, not a constant: the
    // fixture asks for the registered default.
    assert_eq!(
        wall_mm.map(plinth_core::Finite::get),
        Some(2.5),
        "the requested wall must reach the artifact record"
    );
    // FU-55: this model decided solid before a shell was ever built, so there
    // is no realized wall to state -- `None` is the claim "nothing was
    // measured", not "the measurement was dropped".
    assert_eq!(realized_wall, &None);
    assert!(
        emit.support_record_count > 0,
        "an artifact with no supports"
    );
    assert!(artifact.is_file(), "{} was not written", artifact.display());
    assert_eq!(
        report.artifacts().collect::<Vec<_>>(),
        vec![artifact.as_path()]
    );

    // And it is a real `.chitubox`, judged by re-reading the bytes rather than
    // by trusting the producer: same independent audit `first_artifact.rs` uses.
    let audit = chain::audit_disk_artifact(&artifact, "mini").expect("reopen and audit");
    assert_eq!(audit.model_name, chain::EMITTED_MODEL_NAME);
    assert_eq!(audit.support_record_count, emit.support_record_count);
    assert_eq!(audit.file_bytes, emit.artifact_bytes);
    assert_eq!(
        audit.outward_wound_records,
        audit.support_record_count,
        "{} support meshes on disk are inward-wound",
        audit.support_record_count - audit.outward_wound_records
    );
    assert!(audit.model_signed_volume_mm3 > 0.0);
    assert!(
        audit.preview_is_native_fallback,
        "ChituBox refuses a from-scratch project without the native preview"
    );
    assert!(
        audit.format_gate_passed,
        "the plinth-verify format gate refused the emitted artifact"
    );

    // Both bad models failed individually, named, with no artifact and no
    // assumed size, because neither was ever measured.
    for bad in ["aa-not-a-model", "zz-not-a-model"] {
        let ModelPreparationOutcome::Failed {
            exit_code, detail, ..
        } = outcome(&report, bad)
        else {
            panic!("{bad} must fail, not emit");
        };
        assert_eq!(*exit_code, PipelineExitCode::Input);
        assert!(
            detail.contains("binary STL"),
            "{bad}'s failure must name what was wrong: {detail}"
        );
        assert!(
            !scratch.output().join(format!("{bad}.chitubox")).exists(),
            "{bad} wrote an artifact"
        );
    }
    assert!(
        report
            .models
            .iter()
            .find(|entry| entry.model.as_str() == "aa-not-a-model")
            .expect("reported")
            .assumed_print_size
            .is_none(),
        "a model that never loaded cannot have an assumed print size"
    );

    // The assumed-print-size line is present on the success path too, which is
    // the whole point of it: a mini processed at a tenth of its intended size
    // produces a plausible artifact and an unusable print.
    let size = report
        .models
        .iter()
        .find(|entry| entry.model.as_str() == "mini")
        .expect("reported")
        .assumed_print_size
        .expect("a model that decimated was measured");
    assert!(
        size.longest_axis_mm > 0.0 && size.longest_axis_mm.is_finite(),
        "assumed print size must be a real measurement: {size:?}"
    );
    assert_eq!(
        size.longest_axis_mm,
        size.dimensions_mm
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max),
        "the longest axis must be the longest bounding-box dimension"
    );

    // The run's exit code is the first non-Ok disposition in report order, and
    // `aa-not-a-model` sorts first.
    assert_eq!(report.exit_code(), PipelineExitCode::Input);
}

/// A coverage-refused model writes nothing, keeps its evidence, and projects
/// to exit code 5.
///
/// **Re-pinned onto `make_raking_blade` (2026-08-12).** This test used to run
/// `make_overhang_table`, which was the coverage-refusing fixture; with
/// nomination scored against the measurement's uncovered samples that model
/// now CLOSES and emits
/// (`plinth-support/tests/support_runner_integration.rs`,
/// `public_runner_closes_overhang_table_only_after_appending_repair_contacts`),
/// so it can no longer stand for a refusal. `make_raking_blade` can: two of
/// its islands have `NoRoutedContact` on every one of the 8 attempts — the
/// CCO transcript never admits any repair-pool candidate for either of them —
/// so it exhausts the registered bound and refuses. That refusal is measured,
/// not forced: nothing here touches the radius, the floor or the tolerance.
///
/// `run_support` is the coverage-closure controller: it measures ROUTED
/// coverage under the SAME registered radius the pre-write gate measures, so
/// a model that cannot close is refused by [`PipelineError::SealedRun`] —
/// `PipelineExitCode::Supports` (5) — before the pre-write gate is ever
/// reached. It reports through [`ModelPreparationOutcome::Failed`] rather
/// than [`ModelPreparationOutcome::Refused`] because `Refused` is reserved
/// for a verdict the gate itself spoke.
#[test]
fn a_sealed_run_refused_model_writes_no_artifact_and_exits_five() {
    let scratch = Scratch::new("refusal");
    scratch.place("make_raking_blade.prepped.stl", "blade");

    let report = prepare(pinned(
        &scratch,
        "blade",
        "make_raking_blade.pipeline.json",
    ))
    .expect("a refused model is a disposition, not a run failure");

    let ModelPreparationOutcome::Failed {
        exit_code,
        detail,
        evidence,
    } = outcome(&report, "blade")
    else {
        panic!(
            "the raking blade plan must be refused; it reported {:?}",
            outcome(&report, "blade")
        );
    };
    assert_eq!(*exit_code, PipelineExitCode::Supports);
    assert!(
        detail.contains("coverage closure refused"),
        "the detail must name the refusal: {detail}"
    );

    assert!(
        !scratch.output().join("blade.chitubox").exists(),
        "a coverage-closure-refused model wrote an artifact"
    );
    assert_eq!(report.artifacts().count(), 0);
    assert_eq!(report.exit_code(), PipelineExitCode::Supports);
    assert_eq!(PipelineExitCode::Supports as i32, 5);
    assert!(!report.has_gate_refusal());

    // **FU-57, real fixture.** The coverage-closure controller's refusal on
    // this exact geometry carries its own island evidence -- this proves the
    // sidecar/render writer against a genuinely unclosable island, not only
    // against a hand-built fixture.
    let evidence = evidence
        .as_ref()
        .expect("a coverage-closure refusal must carry FU-57 evidence");
    assert_eq!(
        evidence.source,
        plinth_pipeline::EvidenceSource::CoverageClosureController
    );
    assert_eq!(evidence.attempts, Some(5));
    assert!(!evidence.islands.is_empty(), "the unclosable island must be named");
    // FU-56 (2026-08-12 fix): per-island nomination accounting, so the card
    // says whether an island was ever attempted at all.
    assert!(
        evidence.islands.iter().any(|island| island.face_count() > 0),
        "the refused islands must name their faces"
    );
    assert!(
        evidence.write_error.is_none(),
        "the sidecar must write cleanly: {:?}",
        evidence.write_error
    );
    let sidecar_path = scratch.output().join("blade.coverage.json");
    assert_eq!(evidence.sidecar_path, sidecar_path);
    assert!(sidecar_path.exists(), "the sidecar must actually be on disk");
    let sidecar_bytes = std::fs::read(&sidecar_path).expect("read sidecar");
    let sidecar: serde_json::Value = serde_json::from_slice(&sidecar_bytes).expect("valid json");
    assert_eq!(sidecar["schema"], "plinth.coverage-refusal.v1");
    assert_eq!(sidecar["source"], "coverage_closure_controller");
    assert_eq!(sidecar["attempts"], 5);
    assert!(
        sidecar["closure"].is_object(),
        "the sidecar must embed the typed closure refusal verbatim"
    );
    assert!(
        sidecar["report"].is_null(),
        "a controller refusal must not carry a gate report"
    );

    assert!(
        !evidence.render_paths.is_empty(),
        "the worst island(s) must be rendered"
    );
    for render_path in &evidence.render_paths {
        assert!(render_path.exists(), "{} must exist", render_path.display());
        let bytes = std::fs::read(render_path).expect("read render");
        let svg = String::from_utf8(bytes).expect("utf8 svg");
        assert!(svg.starts_with("<svg"), "must be a well-formed svg: {svg}");
    }

    // No file was ever written at the `.chitubox` target -- evidence is a
    // different file class beside it, never at it.
    assert!(!scratch.output().join("blade.chitubox").exists());
}

/// FU-57: a model that starts passing does not leave yesterday's
/// coverage-refusal evidence sitting beside today's fresh artifact.
///
/// A `.coverage.json` (or a stray render) from an earlier refused run, left
/// beside a model that now emits cleanly, is read by the next thing down the
/// line as *this* run's answer -- the exact misleading hand-off
/// `clear_stale_artifact` already guards the `.chitubox` target against. The
/// extension in `prepare_one` owes the same guarantee to the evidence class.
#[test]
fn a_model_that_starts_passing_clears_its_own_stale_coverage_evidence() {
    let scratch = Scratch::new("stale-coverage-evidence");
    scratch.place("make_toed_slab.prepped.stl", "mini");

    std::fs::create_dir_all(scratch.output()).expect("plant an output folder");
    let stale_sidecar = scratch.output().join("mini.coverage.json");
    std::fs::write(&stale_sidecar, b"{\"schema\":\"stale\"}").expect("plant a stale sidecar");
    let stale_render = scratch.output().join("mini.coverage.island-3.svg");
    std::fs::write(&stale_render, b"<svg></svg>").expect("plant a stale render");
    // A render for a DIFFERENT model must never be touched by mini's own
    // clearing pass -- proves the match is by this model's own prefix, not by
    // extension alone.
    let unrelated_render = scratch.output().join("other.coverage.island-1.svg");
    std::fs::write(&unrelated_render, b"<svg></svg>").expect("plant an unrelated render");

    let report = prepare(pinned(&scratch, "mini", "make_toed_slab.pipeline.json"))
        .expect("a model that now passes is a disposition, not a run failure");

    assert!(
        matches!(outcome(&report, "mini"), ModelPreparationOutcome::Artifact { .. }),
        "mini must still emit; it reported {:?}",
        outcome(&report, "mini")
    );
    assert!(
        !stale_sidecar.exists(),
        "a stale coverage sidecar survived a model that now emits cleanly"
    );
    assert!(
        !stale_render.exists(),
        "a stale coverage render survived a model that now emits cleanly"
    );
    assert!(
        unrelated_render.exists(),
        "clearing mini's stale evidence must not touch another model's render"
    );
}

/// A refusal removes a *stale* artifact at the target too.
///
/// "A refused model leaves no path on disk" is a claim about the path, not
/// about one call. A model that emitted on an earlier run and is refused on
/// this one would otherwise leave the earlier run's file sitting there, where
/// the next thing to read it would take it for this run's answer.
#[test]
fn a_refusal_removes_an_earlier_runs_artifact_at_the_same_target() {
    let scratch = Scratch::new("stale");
    scratch.place("make_raking_blade.prepped.stl", "blade");
    std::fs::create_dir_all(scratch.output()).expect("plant an output folder");
    let target = scratch.output().join("blade.chitubox");
    std::fs::write(&target, b"an artifact from an earlier run").expect("plant a stale artifact");

    let report = prepare(pinned(
        &scratch,
        "blade",
        "make_raking_blade.pipeline.json",
    ))
    .expect("run");
    // FU-56, 2026-08-12: `overhang` now refuses at the sealed run (coverage
    // closure), not at the gate — see
    // `a_sealed_run_refused_model_writes_no_artifact_and_exits_five` for the
    // full attribution. The claim under test here — a stale artifact does not
    // survive ANY refusal at this target — is unaffected by which stage spoke
    // it.
    assert!(matches!(
        outcome(&report, "blade"),
        ModelPreparationOutcome::Failed {
            exit_code: PipelineExitCode::Supports,
            ..
        }
    ));
    assert!(
        !target.exists(),
        "a refusal left the previous run's artifact at {}",
        target.display()
    );
}

/// **The dense sweep orients a corpus model end to end, deterministically.**
///
/// This is the test the previous one could not be. Before `--orient` and the
/// `DenseSweep` default landed, the only policy that could orient anything was
/// a pin, so `plinth prepare` on arbitrary input reached a per-model refusal
/// and wrote nothing at all. Here a committed corpus model goes in with **no
/// baseline named anywhere**, the sweep chooses its own orientation, and a real
/// `.chitubox` comes out of a temp directory.
///
/// Three things are asserted, and the third is the one that matters:
///
/// 1. the run emits, under the default policy, with no pin in sight;
/// 2. the orientation is a *searched* one — the report carries the sweep's own
///    provenance, not a replayed rotation;
/// 3. two runs agree **byte for byte**, artifact and report both. The sweep
///    reads no clock and seeds nothing, its folds are serial in index order and
///    its rayon use is index-parallel map-collect, so this is a determinism
///    claim rather than a coincidence of one process.
#[test]
fn the_dense_sweep_orients_a_corpus_model_end_to_end_and_is_bit_stable() {
    let scratch = Scratch::new("dense-sweep");
    scratch.place("make_toed_slab.prepped.stl", "mini");
    let target = scratch.output().join("mini.chitubox");

    // `request(&scratch, &[])` is the DEFAULT policy: no baseline is named for
    // this model or any other.
    let first = prepare(request(&scratch, &[])).expect("the dense sweep must orient this model");
    let ModelPreparationOutcome::Artifact { emit, .. } = outcome(&first, "mini") else {
        panic!(
            "the searched model must reach an artifact; it reported {:?}",
            outcome(&first, "mini")
        );
    };
    assert_eq!(emit.artifact_path, target);
    assert!(target.is_file(), "{} was not written", target.display());
    let artifact = std::fs::read(&target).expect("read the emitted artifact");
    assert!(!artifact.is_empty());

    // Nothing was pinned, and nothing borrowed a rotation: the request this
    // test built names no baseline for any stem.
    assert_eq!(
        request(&scratch, &[]).orientation,
        OrientationPolicy::DenseSweep
    );

    let second = prepare(request(&scratch, &[])).expect("the second run must also orient");
    assert_eq!(
        first, second,
        "the run report is not bit-stable under search"
    );
    assert_eq!(
        serde_json::to_string(&first).expect("report serializes"),
        serde_json::to_string(&second).expect("report serializes"),
        "the serialized report is not byte-identical across searched runs"
    );
    assert_eq!(
        std::fs::read(&target).expect("read the re-emitted artifact"),
        artifact,
        "two dense-sweep runs produced different bytes for the same model"
    );
}

/// Two runs over the same folder produce the same report, byte for byte.
///
/// The executor reads no clock and seeds nothing from the environment, so this
/// is a real determinism claim and not a coincidence of one process.
#[test]
fn two_runs_over_one_folder_produce_byte_identical_reports() {
    let scratch = Scratch::new("determinism");
    scratch.place("make_toed_slab.prepped.stl", "mini");
    scratch.write("broken.stl", b"not an STL");

    let first =
        prepare(pinned(&scratch, "mini", "make_toed_slab.pipeline.json")).expect("first run");
    let second =
        prepare(pinned(&scratch, "mini", "make_toed_slab.pipeline.json")).expect("second run");
    assert_eq!(first, second, "the report is not run-to-run stable");
    assert_eq!(
        serde_json::to_string(&first).expect("report serializes"),
        serde_json::to_string(&second).expect("report serializes"),
        "the serialized report is not byte-identical across runs"
    );
    assert_eq!(first.artifacts().count(), 1);
}

/// An empty folder is a complete, successful run of zero models — not an error
/// and not a crash.
#[test]
fn an_empty_folder_is_a_successful_run_of_no_models() {
    let scratch = Scratch::new("empty");
    // A non-`.stl` file must not be picked up, and neither must a directory
    // that happens to end in `.stl`.
    scratch.write("notes.txt", b"not a model");
    std::fs::create_dir_all(scratch.input().join("a-directory.stl")).expect("decoy directory");

    let report = prepare(pinned(&scratch, "mini", "make_toed_slab.pipeline.json")).expect("run");
    assert!(report.models.is_empty());
    assert_eq!(report.exit_code(), PipelineExitCode::Ok);
    assert_eq!(report.artifacts().count(), 0);
}

/// A missing input folder is a failure of the **run**, not of a model, and
/// nothing is written.
#[test]
fn a_missing_input_folder_fails_the_run_before_anything_is_created() {
    let scratch = Scratch::new("missing-input");
    let mut request = pinned(&scratch, "mini", "make_toed_slab.pipeline.json");
    request.input_models_dir = scratch.input().join("does-not-exist");

    let error = prepare(request).expect_err("a missing input folder must fail the run");
    assert_eq!(error.exit_code(), PipelineExitCode::Input);
    assert!(
        !scratch.output().exists(),
        "the output folder was created despite the run failing"
    );
}

/// **A failure *before* the gates clears the stale artifact too.**
///
/// `judge_then_emit` owns the no-file guarantee only for models that reach it.
/// Every earlier stop — an unreadable file, a scale refusal, an unorientable
/// model, a refused sealed run — used to leave the previous run's artifact
/// standing beside an ERROR line, where the next reader takes it for this run's
/// answer. Three dispositions are checked here, each with a planted artifact:
/// a load failure, an orientation refusal, and the gate refusal that was
/// already covered, so the guarantee is about the *path* on every arm.
#[test]
fn every_failure_disposition_clears_an_earlier_runs_artifact_at_its_target() {
    let scratch = Scratch::new("stale-pre-gate");
    scratch.place("make_toed_slab.prepped.stl", "unpinned");
    scratch.write("broken.stl", b"not an STL at all");
    scratch.place("make_raking_blade.prepped.stl", "blade");

    std::fs::create_dir_all(scratch.output()).expect("plant an output folder");
    let planted = ["blade", "broken", "unpinned"].map(|stem| {
        let target = scratch.output().join(format!("{stem}.chitubox"));
        std::fs::write(&target, b"an artifact from an earlier, luckier run")
            .expect("plant a stale artifact");
        (stem, target)
    });

    // Only `blade` is pinned, so `unpinned` reaches the missing-baseline
    // refusal and `broken` never loads: three different pre-gate stops.
    let report = prepare(pinned(
        &scratch,
        "blade",
        "make_raking_blade.pipeline.json",
    ))
    .expect("a folder of failures is still a successful run");

    for (stem, target) in &planted {
        assert!(
            !matches!(
                outcome(&report, stem),
                ModelPreparationOutcome::Artifact { .. }
            ),
            "{stem} was supposed to fail; this test proves nothing if it emitted"
        );
        assert!(
            !target.exists(),
            "{stem} failed and left the earlier run's artifact at {}",
            target.display()
        );
    }
    assert_eq!(report.artifacts().count(), 0);
}

/// A pinned baseline is **one model's** rotation and is never lent to another.
///
/// `plinth_orient::run` applies whatever rotation the report it is handed
/// states, to whatever mesh it is handed, and cannot tell that the two describe
/// different objects — a misoriented model seals, passes the gates and emits
/// exactly like a correct one. So a heterogeneous folder gets per-model
/// baselines or per-model refusals, and never a neighbour's answer.
#[test]
fn a_model_with_no_pinned_baseline_is_refused_rather_than_given_a_neighbours() {
    let scratch = Scratch::new("per-model-baseline");
    scratch.place("make_toed_slab.prepped.stl", "mini");
    scratch.place("Sphere.prepped.stl", "sphere");

    let report = prepare(pinned(&scratch, "mini", "make_toed_slab.pipeline.json"))
        .expect("an unpinned model is a disposition, not a run failure");

    // The named model oriented and emitted.
    assert!(matches!(
        outcome(&report, "mini"),
        ModelPreparationOutcome::Artifact { .. }
    ));
    // The unnamed one did not, and said why.
    let ModelPreparationOutcome::Failed {
        exit_code, detail, ..
    } = outcome(&report, "sphere")
    else {
        panic!("an unpinned model must be refused, not silently given mini's rotation");
    };
    assert_eq!(*exit_code, PipelineExitCode::Input);
    assert!(
        detail.contains("no pinned orientation baseline"),
        "the refusal must name what is missing: {detail}"
    );
    assert!(!scratch.output().join("sphere.chitubox").exists());

    // Named per model, both models orient and both reach artifacts — the map
    // is a real per-model resolution and not a one-entry special case.
    let scratch = Scratch::new("per-model-baseline-both");
    scratch.place("make_toed_slab.prepped.stl", "mini");
    scratch.place("Sphere.prepped.stl", "sphere");
    let report = prepare(request(
        &scratch,
        &[
            ("mini", "make_toed_slab.pipeline.json"),
            ("sphere", "Sphere.pipeline.json"),
        ],
    ))
    .expect("run");
    // **The two fixtures take different routes, and the sphere's route has
    // moved twice.** `mini` is decided solid by the hollow stage on its own
    // terms and emits, throughout every revision below.
    //
    // Until the 2026-08-10 cavity-field exterior fix, `sphere`'s cavity
    // reached to within 0.5 mm of the outer surface at the six points where
    // it is tangent to its own bounding box; the sealed support run refused
    // *that* shell, and the chain substituted the oriented solid, so this
    // block used to assert two artifacts and a `DrilledShellRefusedBySupport`
    // substitution.
    //
    // From 2026-08-10 to 2026-08-12 the wall was no longer breached, the
    // drilled shell **sealed**, reached the pre-write gate on its own merits,
    // and `UncoveredIslands` refused it — terminally, because a gate refusal
    // is not answered by substituting different geometry. In that window this
    // block asserted one artifact, exit 3, and the same verdict
    // `first_artifact.rs`'s `corpus_sphere_drilled_plan_is_refused_before_any_file_exists`
    // pinned running the sphere alone.
    //
    // **2026-08-12, FU-53 landed: the sphere's route moved a second time, and
    // it now diverges from `first_artifact.rs`'s single-model run.** FU-53
    // corrected cavity selection to the exact nearest-surface distance plus
    // `HOLLOW_CAVITY_MARCH_MARGIN_MM`, which is honestly smaller. Support
    // demand on the resulting thinner-walled shell now exceeds the sealed
    // run's registered routing capacity before the gate is ever reached, so
    // the sphere takes `DrilledShellRefusedBySupport` again, the same named
    // outcome the pre-2026-08-10 revision above took, converted by the
    // existing OQ-1/FU-34 reserve substitution into an emitted solid artifact
    // rather than a terminal gate refusal. Both models now reach an artifact.
    //
    // **The same day, `feature/coverage-closure` (FU-56/FU-57) landed on
    // top and, on THIS fixture, changes nothing measured here.** `run_support`
    // is now the coverage-closure controller, but the controller only gets a
    // look at attempt 1 onward — attempt 0 keeps the pre-FU-56
    // `Failed(phase)` semantics exactly, because attempt 0's plan is the
    // model's own, not the controller's extension of it. The sphere's attempt
    // 0 fails during `Routing`, before coverage is ever measured (Routing
    // precedes `CoverageMeasurement` in the stage order), so this remains a
    // plain support failure — `is_support_refusal` still claims it, and the
    // OQ-1/FU-34 reserve still substitutes the oriented solid. Measured
    // directly against this merged tree: `outcome(&report, "sphere")` is
    // `Artifact { geometry: DrilledShellRefusedBySupport { .. }, .. }`,
    // exit `Ok`, unchanged from the FU-53-only shape.
    //
    // `first_artifact.rs`'s single-model run does NOT diverge the same way,
    // and it is not a folder-sharing effect (`PrepareResourcePolicy`'s own
    // doc: caps are absolute, fixed once from the largest model this
    // workspace runs, never scaled to what else is in the batch — confirmed
    // directly: running `sphere.prepped.stl` alone through this same
    // executor hits the identical routing refusal). The real difference is
    // the **input mesh**: `first_artifact.rs` decimates and orients the RAW
    // `corpus/models/Sphere.stl` fresh, while this fixture is
    // `Sphere.prepped.stl` — the legacy prep's *output*, already oriented and
    // cleaned once — run through decimation and orientation a SECOND time, a
    // known source of divergence this same module's doc comment already
    // named for the pre-write gates ("no checked-in `.prepped.stl` is refused
    // ... because a `.prepped.stl` is the legacy prep's output, already
    // oriented and cleaned, so it seals well when run again"). FU-53 is what
    // makes that divergence land on opposite sides of the routing cap this
    // time, not a new mechanism. No cap, gate or bound moved to produce any
    // of this: the geometry moved, and a resource ceiling this smaller,
    // honestly-walled shell newly approaches on this doubly-processed fixture
    // is measured and recorded here, not raised.
    // **The per-model baseline map is still proved either way:** both models
    // resolved their own baseline and both reached a stage that could only be
    // reached with it.
    assert_eq!(report.artifacts().count(), 2);
    assert_eq!(report.exit_code(), PipelineExitCode::Ok);
    let ModelPreparationOutcome::Artifact { geometry, .. } = outcome(&report, "mini") else {
        panic!("mini must reach an artifact");
    };
    assert_eq!(geometry, &SupportedGeometry::HollowStageDecidedSolid);
    let ModelPreparationOutcome::Artifact { geometry, .. } = outcome(&report, "sphere") else {
        panic!(
            "sphere's drilled shell now falls back to the solid reserve, not a gate refusal: {:?}",
            outcome(&report, "sphere")
        );
    };
    assert!(
        matches!(
            geometry,
            SupportedGeometry::DrilledShellRefusedBySupport { .. }
        ),
        "sphere must reach its artifact through the support-refusal reserve substitution: {geometry:?}"
    );
}

/// **A coverage refusal is not masked by an earlier unrelated failure.**
///
/// `aa-not-a-model.stl` sorts before `blade.stl` and fails to load, exactly
/// as a macOS `._sidecar` would. First-in-order alone would report exit 1 for
/// this folder — "one file was unreadable" — and the physics refusal, the one
/// verdict a calling script acts on, would be invisible.
///
/// **2026-08-12: this is now a REGRESSION guard, not a parked finding.** FU-56
/// gave the same safety verdict a second disposition — the closure controller
/// refuses as `Failed`/`Supports`, not as `Refused`/`Gates` — and the sticky
/// rule protecting the run-level signal only knew about `Gates`, so the
/// refusal class FU-57 exists to make visible became maskable by an unrelated
/// `Input` failure. `PrepareReport::exit_code` now makes a coverage refusal
/// sticky at second priority, behind `Gates`: a bound ADDED, no threshold
/// moved, and the remaining codes still first-in-order.
#[test]
fn a_coverage_closure_refusal_is_not_masked_by_an_earlier_load_failure() {
    let scratch = Scratch::new("sticky-gates");
    scratch.write("aa-not-a-model.stl", b"a sidecar, or a truncated download");
    scratch.place("make_raking_blade.prepped.stl", "blade");

    let report = prepare(pinned(
        &scratch,
        "blade",
        "make_raking_blade.pipeline.json",
    ))
    .expect("run");

    assert_eq!(
        report
            .models
            .iter()
            .map(|entry| entry.model.as_str())
            .collect::<Vec<_>>(),
        ["aa-not-a-model", "blade"],
        "the load failure must sort first for this test to mean anything"
    );
    assert!(matches!(
        outcome(&report, "aa-not-a-model"),
        ModelPreparationOutcome::Failed {
            exit_code: PipelineExitCode::Input,
            ..
        }
    ));
    assert!(matches!(
        outcome(&report, "blade"),
        ModelPreparationOutcome::Failed {
            exit_code: PipelineExitCode::Supports,
            ..
        }
    ));
    assert!(!report.has_gate_refusal());
    assert!(
        report.has_coverage_refusal(),
        "the closure controller's refusal is a coverage refusal and the run must say so"
    );
    assert_eq!(
        report.exit_code(),
        PipelineExitCode::Supports,
        "a coverage refusal is sticky behind `Gates`: an unrelated earlier load failure must \
         not hide a safety verdict at the run level"
    );
}

/// Every `stop_after` stage really stops, at that stage, with nothing written.
///
/// `stop_after` is a production field on `PrepareRequest` and had no test at
/// all: the four branch points, the `Ok(None)` protocol between `prepare_one`,
/// `run_from_oriented` and `seal_and_emit`, and the `expect()` that turns a
/// broken protocol into a whole-run panic were all unexecuted. A future edit
/// that breaks the protocol would turn one model's disposition into a crash of
/// the folder — precisely the failure this executor exists to prevent — so the
/// contract is pinned here rather than assumed.
/// Both fixtures are run at every stop, because they take different routes
/// through the hollow stage and each route has its own branch points: `mini` is
/// decided solid, `sphere` reaches `AwaitingInternalSupport` and is drilled, and
/// only running both executes both `Hollow` checks and both paths into
/// `seal_and_emit`'s `Support` check.
#[test]
fn each_stage_stop_halts_that_model_at_that_stage_and_writes_nothing() {
    let fixtures = [
        ("mini", "make_toed_slab.prepped.stl", "make_toed_slab"),
        ("sphere", "Sphere.prepped.stl", "Sphere"),
    ];
    for after in [
        StageStop::Decimate,
        StageStop::Orient,
        StageStop::Hollow,
        StageStop::Support,
    ] {
        for (stem, fixture, baseline) in fixtures {
            let scratch = Scratch::new(&format!("stop-{after:?}-{stem}").to_lowercase());
            scratch.place(fixture, stem);
            let mut request = pinned(&scratch, stem, &format!("{baseline}.pipeline.json"));
            request.stop_after = Some(after);

            let report =
                prepare(request).expect("a stage stop is a disposition, not a run failure");
            // **A refusal that happens BEFORE the stop still wins, and must —
            // but this fixture, run alone, never hits one.** The `Support`
            // stop is checked after `run_support` returns, so a model whose
            // support run refuses never reaches it. The coverage-closure
            // controller (FU-56) only gets a look on `sphere` from attempt 1
            // onward and in this per-model-baseline batch (see the
            // per-model-baseline test above) it never does, because attempt 0
            // fails during `Routing` — a two-model-batch resource-demand
            // effect, not present when `sphere` is prepared alone as it is
            // here. Measured directly: every fixture stops cleanly at every
            // stage, `sphere` included, so no special case is needed.
            assert_eq!(
                outcome(&report, stem),
                &ModelPreparationOutcome::Stopped { after },
                "stop_after {after:?} did not stop {stem} where it was told to"
            );
            assert!(
                !scratch.output().join(format!("{stem}.chitubox")).exists(),
                "stop_after {after:?} still wrote an artifact for {stem}"
            );
            assert_eq!(report.artifacts().count(), 0);
            // A stop is not a failure: the run's exit status stays clean.
            assert_eq!(report.exit_code(), PipelineExitCode::Ok);

            // The model was measured before it stopped, at every stop this
            // executor has, so the assumed print size is reported either way.
            assert!(report.models[0].assumed_print_size.is_some());
        }
    }

    // Without a stop, the same models run to their own end — so the assertions
    // above are about `stop_after` and not about models that never get there.
    // This is the same two-model batch as the per-model-baseline test above
    // (both fixtures together): `mini` emits, and `sphere`'s attempt 0 fails
    // during `Routing` before the coverage-closure controller ever gets a
    // look, so it reaches its artifact through the existing
    // `DrilledShellRefusedBySupport` reserve substitution — unmoved by
    // FU-56/FU-57. Both endings are ones a stage `stop_after` never reached,
    // which is what this block discriminates.
    let scratch = Scratch::new("stop-none");
    for (stem, fixture, _) in fixtures {
        scratch.place(fixture, stem);
    }
    let report = prepare(request(
        &scratch,
        &[
            ("mini", "make_toed_slab.pipeline.json"),
            ("sphere", "Sphere.pipeline.json"),
        ],
    ))
    .expect("run");
    assert_eq!(report.artifacts().count(), 2);
    assert!(!report.has_gate_refusal());
    assert!(!report.has_coverage_refusal());
}

/// **FU-37, end to end: two runs at different walls are distinguishable in
/// what the pipeline actually writes.**
///
/// The row's own motivating claim was that "two artifacts prepared at 2.5 mm
/// and 3.0 mm are byte-indistinguishable in metadata in every file the
/// pipeline writes". `HollowReport` gained the `wall` field, but the report
/// died inside `seal_and_emit` — `ChainOutcome` had no wall field and
/// `ModelPreparationOutcome::Artifact` serialized none — so the claim stayed
/// true of real runs while the in-crate tests, which serialize a
/// `HollowReport` in isolation, passed. This runs the **whole executor**
/// twice on the same fixture, changing nothing but `--wall-mm`, and asserts
/// the serialized reports differ in the wall and only there.
#[test]
fn two_prepare_runs_at_different_walls_are_distinguishable_in_the_serialized_report() {
    let printer = OutputProfile::Saturn4Ultra
        .resolve()
        .expect("M0 Saturn profile")
        .clone();

    let run_at = |label: &str, wall: plinth_core::PositiveFinite| {
        let scratch = Scratch::new(label);
        scratch.place("make_toed_slab.prepped.stl", "mini");
        let mut request = pinned(&scratch, "mini", "make_toed_slab.pipeline.json");
        request.hollow = plinth_hollow::HollowRequest {
            printer: printer.clone(),
            wall_mm: wall,
        };
        let report = prepare(request).expect("run");
        let json = serde_json::to_string(&report).expect("the report serializes");
        // Drop the scratch path, which differs per run by construction and
        // says nothing about the wall.
        let outcome = outcome(&report, "mini").clone();
        (outcome, json.contains("\"wall_mm\""))
    };

    let default_wall = plinth_hollow::registered_wall(plinth_hollow::WallSelection::Default)
        .expect("registered default wall");
    let heavy_wall = plinth_hollow::registered_wall(plinth_hollow::WallSelection::HandlingHeavy)
        .expect("registered handling-heavy wall");
    assert_ne!(
        default_wall.get(),
        heavy_wall.get(),
        "the two registered walls must differ for this test to mean anything"
    );

    let (thin, thin_has_key) = run_at("wall-default", default_wall);
    let (thick, thick_has_key) = run_at("wall-heavy", heavy_wall);
    assert!(
        thin_has_key && thick_has_key,
        "the serialized report must carry the wall at all"
    );

    let wall_of = |outcome: &ModelPreparationOutcome| match outcome {
        ModelPreparationOutcome::Artifact { wall_mm, .. } => {
            wall_mm.map(plinth_core::Finite::get)
        }
        other => panic!("the fixture must reach an artifact; got {other:?}"),
    };
    assert_eq!(wall_of(&thin), Some(default_wall.get()));
    assert_eq!(wall_of(&thick), Some(heavy_wall.get()));
    assert_ne!(
        thin, thick,
        "two runs differing only in --wall-mm must not produce identical artifact records"
    );
}
