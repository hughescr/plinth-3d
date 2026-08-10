//! Design phase E4: `plinth_pipeline::prepare` over a real folder.
//!
//! `tests/first_artifact.rs` proves the *chain* joins up on one model. This
//! proves the **executor** does: a folder of source `.stl` in, one
//! `.chitubox` per model that passes the pre-write gates out, and one typed
//! disposition per model either way.
//!
//! Every fixture here is either a **committed** file under `corpus/baseline/`
//! or an in-repo procedural mesh written out as a binary STL, never a
//! provisioned corpus model — so this test runs in a fresh clone with nothing
//! symlinked. `corpus/baseline/*.prepped.stl` are checked in; `corpus/models/`
//! is not.
//!
//! **A finding this test surfaced, recorded rather than papered over — and
//! half of it expired on 2026-08-10.** It used to read: no checked-in
//! `.prepped.stl` is refused by the pre-write gates, because both
//! `make_overhang_table.prepped.stl` and `Sphere.prepped.stl` seal and emit
//! when run through the whole chain, while the *procedural*
//! `make_overhang_table()` and the provisioned `corpus/models/Sphere.stl` are
//! refused for uncovered islands (`tests/first_artifact.rs`). That is still
//! true of `make_overhang_table.prepped.stl`, and the reasoning still holds —
//! a `.prepped.stl` is the legacy prep's *output*, already oriented and
//! cleaned, so it seals well when run again. It is **no longer true of
//! `Sphere.prepped.stl`**: the cavity-field exterior fix stopped its hollow
//! cavity from breaching the wall at its poles, so its drilled shell now seals
//! instead of being refused by the support run, reaches the gate on its own
//! merits, and is refused there for uncovered islands like every other
//! `Sphere`. The refusal case here is therefore driven by the procedural mesh
//! *and* by that one committed fixture.
//!
//! Deliberately absent: any new frozen digest (design record OQ-3). Everything
//! asserted is a completion, disposition or path property that a changed
//! member count cannot break.

use std::path::{Path, PathBuf};

use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    OrientationPolicy, OutputProfile, PipelineExitCode, PreWriteGate, PrepareReport,
    PrepareRequest, StageStop, SupportedGeometry,
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

    /// Put an in-repo procedural mesh into the input folder as a real binary
    /// STL, because `prepare` takes a folder of files, not meshes.
    ///
    /// Needed because no checked-in `corpus/baseline/*.prepped.stl` is refused
    /// by the pre-write gates — and that is not an accident. A `.prepped.stl`
    /// is the *output* of the legacy prep, already oriented and cleaned, so it
    /// seals well when run through the chain again. The procedural fixtures
    /// are the raw geometry the gates were characterised on.
    fn write_mesh(&self, stem: &str, mesh: &plinth_mesh::Mesh) {
        plinth_mesh::write_binary_stl(self.input().join(format!("{stem}.stl")), mesh)
            .expect("write procedural fixture as binary STL");
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
        let ModelPreparationOutcome::Failed { exit_code, detail } = outcome(&report, bad) else {
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

/// A gate-refused model writes nothing and projects to exit code 3.
///
/// `make_overhang_table` is the model `plinth-verify`'s own frozen coverage
/// golden is built from: its sealed plan leaves uncovered islands. That is a
/// support-planning finding, not this executor's to fix. What this executor
/// owes is that the refusal stops the file.
#[test]
fn a_gate_refused_model_writes_no_artifact_and_exits_three() {
    let scratch = Scratch::new("refusal");
    scratch.write_mesh(
        "overhang",
        &plinth_mesh::make_overhang_table().expect("procedural overhang table"),
    );

    let report = prepare(pinned(
        &scratch,
        "overhang",
        "make_overhang_table.pipeline.json",
    ))
    .expect("a refused model is a disposition, not a run failure");

    let ModelPreparationOutcome::Refused {
        gate,
        geometry,
        violation,
    } = outcome(&report, "overhang")
    else {
        panic!(
            "the overhang table's plan must be refused; it reported {:?}",
            outcome(&report, "overhang")
        );
    };
    // The gate as a value, so a renamed variant is a compile error rather than
    // an assertion that silently becomes permanently false.
    assert_eq!(*gate, Some(PreWriteGate::UncoveredIslands));
    // *What* was refused. Without this the same assertions pass just as
    // happily on "support refused the shell, the chain fell back to the
    // oriented solid, and the gate refused that instead" -- the exact
    // substitution the terminal-refusal design exists to prevent.
    assert_eq!(
        geometry.as_ref(),
        Some(&SupportedGeometry::HollowStageDecidedSolid),
        "the overhang table reached the gate on a different geometry"
    );
    assert!(
        violation.contains("pre-write physics gates"),
        "the violation must name the refusal: {violation}"
    );

    assert!(
        !scratch.output().join("overhang.chitubox").exists(),
        "a gate-refused model wrote an artifact"
    );
    assert_eq!(report.artifacts().count(), 0);
    assert_eq!(report.exit_code(), PipelineExitCode::Gates);
    assert_eq!(PipelineExitCode::Gates as i32, 3);
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
    scratch.write_mesh(
        "overhang",
        &plinth_mesh::make_overhang_table().expect("procedural overhang table"),
    );
    std::fs::create_dir_all(scratch.output()).expect("plant an output folder");
    let target = scratch.output().join("overhang.chitubox");
    std::fs::write(&target, b"an artifact from an earlier run").expect("plant a stale artifact");

    let report = prepare(pinned(
        &scratch,
        "overhang",
        "make_overhang_table.pipeline.json",
    ))
    .expect("run");
    assert!(matches!(
        outcome(&report, "overhang"),
        ModelPreparationOutcome::Refused { .. }
    ));
    assert!(
        !target.exists(),
        "a gate refusal left the previous run's artifact at {}",
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
    scratch.write_mesh(
        "overhang",
        &plinth_mesh::make_overhang_table().expect("procedural overhang table"),
    );

    std::fs::create_dir_all(scratch.output()).expect("plant an output folder");
    let planted = ["broken", "overhang", "unpinned"].map(|stem| {
        let target = scratch.output().join(format!("{stem}.chitubox"));
        std::fs::write(&target, b"an artifact from an earlier, luckier run")
            .expect("plant a stale artifact");
        (stem, target)
    });

    // Only `overhang` is pinned, so `unpinned` reaches the missing-baseline
    // refusal and `broken` never loads: three different pre-gate stops.
    let report = prepare(pinned(
        &scratch,
        "overhang",
        "make_overhang_table.pipeline.json",
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
    let ModelPreparationOutcome::Failed { exit_code, detail } = outcome(&report, "sphere") else {
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
    // **The two fixtures take different routes, and the sphere's route moved
    // on 2026-08-10.** `mini` is decided solid by the hollow stage on its own
    // terms and emits. `sphere` hollows, drains and drills — and until the
    // cavity-field exterior fix its cavity reached to within 0.5 mm of the
    // outer surface at the six points where it is tangent to its own bounding
    // box, the sealed support run refused *that* shell, and the chain
    // substituted the oriented solid, so this block used to assert two
    // artifacts and a `DrilledShellRefusedBySupport` substitution. With the
    // wall no longer breached the drilled shell **seals**, reaches the
    // pre-write gate on its own merits, and `UncoveredIslands` refuses it —
    // terminally, because a gate refusal is not answered by substituting
    // different geometry. So one artifact, exit 3, and the sphere's outcome is
    // a refusal carrying the geometry it was spoken over.
    //
    // This is the same verdict `first_artifact.rs`'s
    // `corpus_sphere_drilled_plan_is_refused_before_any_file_exists` already
    // pins; the sphere joins it rather than escaping it through a fallback. No
    // gate, cap or bound moved. **The per-model baseline map is still proved:**
    // both models resolved their own baseline and both reached a stage that
    // could only be reached with it.
    assert_eq!(report.artifacts().count(), 1);
    assert_eq!(report.exit_code(), PipelineExitCode::Gates);
    let ModelPreparationOutcome::Artifact { geometry, .. } = outcome(&report, "mini") else {
        panic!("mini must reach an artifact");
    };
    assert_eq!(geometry, &SupportedGeometry::HollowStageDecidedSolid);
    let ModelPreparationOutcome::Refused { gate, geometry, .. } = outcome(&report, "sphere") else {
        panic!(
            "sphere's drilled shell must now be refused by the gate: {:?}",
            outcome(&report, "sphere")
        );
    };
    assert_eq!(*gate, Some(plinth_pipeline::PreWriteGate::UncoveredIslands));
    assert_eq!(
        geometry.as_ref(),
        Some(&SupportedGeometry::DrilledShellSealed),
        "the refusal must be spoken over the shell the chain planned, not over a substitute"
    );
}

/// **A gate refusal is not masked by an earlier unrelated failure.**
///
/// `aa-not-a-model.stl` sorts before `overhang.stl` and fails to load, exactly
/// as a macOS `._sidecar` would. First-in-order alone would report exit 1 for
/// this folder — "one file was unreadable" — and the physics refusal, the one
/// verdict a calling script acts on, would be invisible.
#[test]
fn a_gate_refusal_behind_an_earlier_load_failure_still_exits_three() {
    let scratch = Scratch::new("sticky-gates");
    scratch.write("aa-not-a-model.stl", b"a sidecar, or a truncated download");
    scratch.write_mesh(
        "overhang",
        &plinth_mesh::make_overhang_table().expect("procedural overhang table"),
    );

    let report = prepare(pinned(
        &scratch,
        "overhang",
        "make_overhang_table.pipeline.json",
    ))
    .expect("run");

    assert_eq!(
        report
            .models
            .iter()
            .map(|entry| entry.model.as_str())
            .collect::<Vec<_>>(),
        ["aa-not-a-model", "overhang"],
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
        outcome(&report, "overhang"),
        ModelPreparationOutcome::Refused { .. }
    ));
    assert!(report.has_gate_refusal());
    assert_eq!(
        report.exit_code(),
        PipelineExitCode::Gates,
        "the run contains a gate refusal and must exit 3 whatever else it hit"
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
    // `mini` emits; `sphere`'s drilled shell now seals and is refused by the
    // `UncoveredIslands` gate (see the per-model-baseline test above for the
    // 2026-08-10 attribution), which is still a stage `stop_after` never
    // reached and therefore still discriminates.
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
    assert_eq!(report.artifacts().count(), 1);
    assert!(report.has_gate_refusal());
}
