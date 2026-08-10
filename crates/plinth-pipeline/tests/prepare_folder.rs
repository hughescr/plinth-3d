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
//! **A finding this test surfaced, recorded rather than papered over.** No
//! checked-in `.prepped.stl` is refused by the pre-write gates: both
//! `make_overhang_table.prepped.stl` and `Sphere.prepped.stl` seal and emit
//! when run through the whole chain, while the *procedural*
//! `make_overhang_table()` and the provisioned `corpus/models/Sphere.stl` are
//! refused for uncovered islands (`tests/first_artifact.rs`). That is
//! consistent — a `.prepped.stl` is the legacy prep's *output*, already
//! oriented and cleaned — but it means the refusal case here is driven by the
//! procedural mesh, which is the geometry the coverage gate was characterised
//! on.
//!
//! Deliberately absent: any new frozen digest (design record OQ-3). Everything
//! asserted is a completion, disposition or path property that a changed
//! member count cannot break.

use std::path::{Path, PathBuf};

use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    OrientationPolicy, OutputProfile, PipelineExitCode, PreWriteGate, PrepareReport,
    PrepareRequest, SupportedGeometry,
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

/// A request whose orientation is pinned to one committed baseline report.
///
/// `OrientationPolicy::Pinned` is the only policy that can orient anything
/// today, and it names one baseline for the whole run — so each test here pins
/// the baseline belonging to the fixture it is about.
fn request(scratch: &Scratch, baseline: &str, orientation: Option<&str>) -> PrepareRequest {
    let printer = OutputProfile::Saturn4Ultra
        .resolve()
        .expect("M0 Saturn profile")
        .clone();
    PrepareRequest {
        input_models_dir: scratch.input(),
        output_dir: scratch.output(),
        orientation: orientation.map_or(OrientationPolicy::LegacyCoarse, |_| {
            OrientationPolicy::Pinned(plinth_orient::PinnedBaseline {
                pipeline_report: committed_fixture(baseline),
            })
        }),
        output_profile: OutputProfile::Saturn4Ultra,
        decimation_budget: registered_decimation_budget().expect("registered budget"),
        hollow: plinth_hollow::HollowRequest::with_registered_wall_default(printer.clone())
            .expect("registered wall"),
        support: registered_support_request(&printer).expect("registered lift"),
        no_cache: false,
        stop_after: None,
    }
}

fn pinned(scratch: &Scratch, baseline: &str) -> PrepareRequest {
    request(scratch, baseline, Some(baseline))
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

    let report = prepare(pinned(&scratch, "make_toed_slab.pipeline.json"))
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
    let ModelPreparationOutcome::Artifact { emit } = outcome(&report, "mini") else {
        panic!(
            "mini must reach an artifact; it reported {:?}",
            outcome(&report, "mini")
        );
    };
    assert_eq!(emit.artifact_path, artifact);
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

    let report = prepare(pinned(&scratch, "make_overhang_table.pipeline.json"))
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

    let report = prepare(pinned(&scratch, "make_overhang_table.pipeline.json")).expect("run");
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

/// The legacy-coarse orientation policy refuses **per model**, and does not
/// crash, guess, or quietly choose identity.
///
/// This is the current honest state of the ratified CLI surface: `parse_args`
/// hardcodes `OrientationPolicy::LegacyCoarse` and there is no `--orient` flag,
/// so `plinth prepare` reaches this refusal for every model. The native coarse
/// search of `02-target-architecture.md` §6 is unported; choosing identity in
/// its absence would be inferring an orientation, which `plinth-orient` refuses
/// to do even for identity.
#[test]
fn the_unported_orientation_policy_refuses_per_model_rather_than_crashing() {
    let scratch = Scratch::new("legacy-coarse");
    scratch.place("make_toed_slab.prepped.stl", "mini");

    let report = prepare(request(&scratch, "make_toed_slab.pipeline.json", None))
        .expect("an unorientable model is a disposition, not a run failure");

    let ModelPreparationOutcome::Failed { exit_code, detail } = outcome(&report, "mini") else {
        panic!("an unorientable model must fail by name");
    };
    assert_eq!(*exit_code, PipelineExitCode::Input);
    assert!(
        detail.contains("orientation is never inferred"),
        "the refusal must say why it is a refusal: {detail}"
    );
    assert!(!scratch.output().join("mini.chitubox").exists());

    // The model was still measured before the policy refused it, so the
    // operator learns the assumed print size even on this path.
    assert!(report.models[0].assumed_print_size.is_some());
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

    let first = prepare(pinned(&scratch, "make_toed_slab.pipeline.json")).expect("first run");
    let second = prepare(pinned(&scratch, "make_toed_slab.pipeline.json")).expect("second run");
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

    let report = prepare(pinned(&scratch, "make_toed_slab.pipeline.json")).expect("run");
    assert!(report.models.is_empty());
    assert_eq!(report.exit_code(), PipelineExitCode::Ok);
    assert_eq!(report.artifacts().count(), 0);
}

/// A missing input folder is a failure of the **run**, not of a model, and
/// nothing is written.
#[test]
fn a_missing_input_folder_fails_the_run_before_anything_is_created() {
    let scratch = Scratch::new("missing-input");
    let mut request = pinned(&scratch, "make_toed_slab.pipeline.json");
    request.input_models_dir = scratch.input().join("does-not-exist");

    let error = prepare(request).expect_err("a missing input folder must fail the run");
    assert_eq!(error.exit_code(), PipelineExitCode::Input);
    assert!(
        !scratch.output().exists(),
        "the output folder was created despite the run failing"
    );
}
