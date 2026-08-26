use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn plinth(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_plinth"))
        .args(arguments)
        .output()
        .expect("run plinth process")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// A per-test scratch tree, removed on the way in and on the way out.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("plinth-cli-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("source models")).expect("scratch input folder");
        Self(path)
    }

    /// A folder path containing a space, so the process test keeps covering
    /// the quoting the parser tests already cover in-process.
    fn input(&self) -> PathBuf {
        self.0.join("source models")
    }

    fn output(&self) -> PathBuf {
        self.0.join("prepared models")
    }

    fn place(&self, fixture: &str, stem: &str) {
        std::fs::copy(
            workspace_root().join("corpus/baseline").join(fixture),
            self.input().join(format!("{stem}.stl")),
        )
        .expect("place committed fixture");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("CLI output is UTF-8")
}

#[test]
fn root_and_prepare_help_pin_the_stable_usage_essentials() {
    let root = plinth(&["--help"]);
    assert!(root.status.success());
    let root_stdout = text(&root.stdout);
    assert!(root_stdout.contains("Usage: plinth <subcommand>"));
    assert!(root_stdout.contains("prepare  Prepare every source model in a directory"));

    let prepare = plinth(&["prepare", "--help"]);
    assert!(prepare.status.success());
    let stdout = text(&prepare.stdout);
    assert!(stdout.contains(
        "Usage: plinth prepare --input-models-dir DIR --output saturn4-ultra --out-dir DIR [--orient dense|pinned:DIR] [--wall-mm MM]"
    ));
    for essential in [
        "--input-models-dir DIR",
        "--output PROFILE",
        "--out-dir DIR",
        "--orient MODE",
        "--wall-mm MM",
    ] {
        assert!(stdout.contains(essential), "missing help item {essential}");
    }
}

/// **`plinth prepare` prepares.** A folder in, a real `.chitubox` out, with no
/// baseline named anywhere.
///
/// This test used to assert the opposite, and the comment it carried explained
/// why: `parse_args` hardcoded an orientation policy with no implementation, so
/// every model reached a per-model refusal and the CLI could not produce an
/// artifact at all. That was the standing product defect. `--orient` landed
/// with `dense` as the default, and this is where the artifact assertion and
/// the observed exit 0 said they would go.
///
/// What is pinned here is the whole process boundary: the folder is enumerated,
/// each model measured and reported by name with its assumed print size, the
/// orientation authority is stated, the output folder is created, a real file
/// is written at the stem-derived path, and the exit code is the pipeline's own
/// projection rather than a CLI-invented one.
#[test]
fn prepare_searches_each_model_orientation_and_writes_one_artifact_each() {
    let scratch = Scratch::new("execute");
    scratch.place("make_toed_slab.prepped.stl", "mini");

    let output = plinth(&[
        "prepare",
        "--out-dir",
        scratch.output().to_str().expect("scratch path is Unicode"),
        "--input-models-dir",
        scratch.input().to_str().expect("scratch path is Unicode"),
        "--output",
        "saturn4-ultra",
    ]);
    let stdout = text(&output.stdout);

    assert!(
        stdout.contains("MODEL  mini"),
        "the run report must name every model: {stdout}"
    );
    // Audit fix #3: the size the run assumed, on every model it measured.
    assert!(
        stdout.contains("assumed print size  longest axis 30.000 mm"),
        "the run report must carry the assumed print size: {stdout}"
    );
    // Which authority chose the rotation. A searched artifact and a replayed
    // one are indistinguishable from the file alone.
    assert!(
        stdout.contains("orientation  dense sweep (searched per model)"),
        "the run report must state the orientation authority: {stdout}"
    );
    assert!(
        stdout.contains("result  EMITTED"),
        "the searched model must reach an artifact: {stdout}"
    );
    assert!(
        stdout.contains("SUMMARY  emitted=1  gate-refused=0  failed=0  stopped=0"),
        "the run report must total the dispositions: {stdout}"
    );
    // The pipeline's own exit-code projection, not a second contract.
    assert_eq!(output.status.code(), Some(0), "stdout was: {stdout}");
    assert!(
        scratch.output().is_dir(),
        "prepare owns creating its output folder"
    );
    let artifact = scratch.output().join("mini.chitubox");
    assert!(
        artifact.is_file(),
        "the searched model wrote no artifact at {}",
        artifact.display()
    );
}

/// `--orient pinned:DIR` still replays a baseline, per model and by stem.
///
/// Deleting the unimplemented policy did not delete the pinned one: replaying a
/// frozen rotation is how the corpus baselines stay reproducible. The
/// resolution is per stem — a directory that does not name this model refuses
/// it individually rather than lending it a neighbour's rotation — and that
/// refusal is asserted here beside the successful replay.
#[test]
fn the_pinned_orientation_mode_replays_a_baseline_by_stem() {
    let scratch = Scratch::new("pinned");
    scratch.place("make_toed_slab.prepped.stl", "make_toed_slab");
    scratch.place("Sphere.prepped.stl", "unpinned-model");

    let baselines = workspace_root().join("corpus/baseline");
    let output = plinth(&[
        "prepare",
        "--input-models-dir",
        scratch.input().to_str().expect("scratch path is Unicode"),
        "--output",
        "saturn4-ultra",
        "--out-dir",
        scratch.output().to_str().expect("scratch path is Unicode"),
        "--orient",
        &format!("pinned:{}", baselines.display()),
    ]);
    let stdout = text(&output.stdout);

    assert!(
        stdout.contains("orientation  pinned baselines from"),
        "the run report must state the pinned authority: {stdout}"
    );
    assert!(
        scratch.output().join("make_toed_slab.chitubox").is_file(),
        "the pinned model wrote no artifact: {stdout}"
    );
    assert!(
        stdout.contains("no pinned orientation baseline"),
        "a model the directory does not name must be refused by name: {stdout}"
    );
    assert!(
        !scratch.output().join("unpinned-model.chitubox").exists(),
        "an unpinned model must not borrow a neighbour's rotation"
    );
}

/// One spelling per concept: a near-miss orientation mode is refused, not
/// guessed at.
#[test]
fn an_unknown_orientation_mode_is_refused_by_name() {
    let scratch = Scratch::new("orient-mode");
    let output = plinth(&[
        "prepare",
        "--input-models-dir",
        scratch.input().to_str().expect("scratch path is Unicode"),
        "--output",
        "saturn4-ultra",
        "--out-dir",
        scratch.output().to_str().expect("scratch path is Unicode"),
        "--orient",
        "densest",
    ]);
    assert!(!output.status.success());
    let stderr = text(&output.stderr);
    assert!(
        stderr.contains("unknown orientation mode") && stderr.contains("dense or pinned:DIR"),
        "the refusal must name the sanctioned spellings: {stderr}"
    );
    assert!(
        !scratch.output().exists(),
        "a parse refusal must create nothing"
    );
}

/// Two invocations over the same folder print byte-identical reports.
///
/// Nothing in the report reads a clock, and model order is the executor's own
/// sorted order, so this is a determinism claim about the process boundary and
/// not only about the library.
#[test]
fn two_invocations_over_one_folder_print_byte_identical_reports() {
    let scratch = Scratch::new("determinism");
    scratch.place("make_toed_slab.prepped.stl", "mini");
    scratch.place("Sphere.prepped.stl", "sphere");

    let run = || {
        let output = plinth(&[
            "prepare",
            "--input-models-dir",
            scratch.input().to_str().expect("scratch path is Unicode"),
            "--output",
            "saturn4-ultra",
            "--out-dir",
            scratch.output().to_str().expect("scratch path is Unicode"),
        ]);
        (output.status.code(), text(&output.stdout))
    };
    let first = run();
    assert_eq!(first, run(), "the run report is not invocation-stable");
    assert!(
        first.1.contains("MODEL  mini") && first.1.contains("MODEL  sphere"),
        "both models must be reported: {}",
        first.1
    );
}

/// A missing input folder is a run failure, named, before anything is created.
#[test]
fn a_missing_input_folder_is_named_and_creates_nothing() {
    let scratch = Scratch::new("missing");
    let missing = scratch.input().join("does-not-exist");

    let output = plinth(&[
        "prepare",
        "--input-models-dir",
        missing.to_str().expect("scratch path is Unicode"),
        "--output",
        "saturn4-ultra",
        "--out-dir",
        scratch.output().to_str().expect("scratch path is Unicode"),
    ]);
    let stderr = text(&output.stderr);
    assert!(
        stderr.contains("cannot enumerate input model folder"),
        "the run failure must name the folder: {stderr}"
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(
        !scratch.output().exists(),
        "the output folder was created despite the run failing"
    );
}

#[test]
fn invalid_forms_exit_nonzero_with_named_deterministic_diagnostics() {
    let cases: &[(&[&str], &str)] = &[
        (&[], "missing subcommand"),
        (&["score"], "unknown subcommand \"score\""),
        (
            &["prepare", "--input-models-dir", "models"],
            "missing required flag --out-dir",
        ),
        (
            &[
                "prepare",
                "--input-models-dir",
                "one",
                "--input-models-dir",
                "two",
            ],
            "duplicate flag --input-models-dir",
        ),
        (
            &["prepare", "--output", "bad-profile"],
            "unknown output profile \"bad-profile\"; expected \"saturn4-ultra\"",
        ),
        (
            &["prepare", "models"],
            "unexpected positional argument \"models\"",
        ),
        (
            &["prepare", "--cache=false"],
            "unknown flag \"--cache=false\"",
        ),
        (
            &["prepare", "--wall-mm", "-2.5"],
            "wall thickness \"-2.5\" is not a wall this stage can produce: expected a positive finite number of millimetres, no finer than the frozen hollow voxel pitch",
        ),
        (
            &["prepare", "--wall-mm", "0.001"],
            "wall thickness \"0.001\" is not a wall this stage can produce: expected a positive finite number of millimetres, no finer than the frozen hollow voxel pitch",
        ),
    ];
    for (arguments, expected) in cases {
        let output = plinth(arguments);
        assert_eq!(output.status.code(), Some(2), "arguments {arguments:?}");
        assert!(
            text(&output.stderr).starts_with(&format!("plinth: {expected}\n")),
            "arguments {arguments:?}: {}",
            text(&output.stderr)
        );
    }
}
