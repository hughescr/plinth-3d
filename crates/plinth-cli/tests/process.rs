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
        "Usage: plinth prepare --input-models-dir DIR --output saturn4-ultra --out-dir DIR [--wall-mm MM] [--no-cache]"
    ));
    for essential in [
        "--input-models-dir DIR",
        "--output PROFILE",
        "--out-dir DIR",
        "--wall-mm MM",
        "--no-cache",
    ] {
        assert!(stdout.contains(essential), "missing help item {essential}");
    }
}

/// `plinth prepare` now executes, and this is the whole of what it does today.
///
/// **The CLI cannot produce an artifact, and that is a wiring gap, not a bug in
/// this run.** `parse_args` hardcodes `OrientationPolicy::LegacyCoarse` — the
/// ratified command line has no `--orient` flag — and the native coarse
/// orientation search of `02-target-architecture.md` §6 is unported. So every
/// model reaches a per-model orientation refusal. What this test pins is that
/// the refusal is *per model, named, and non-fatal to the run*: the process
/// enumerates the folder, measures each model, prints the run report including
/// the assumed print size, creates the output folder, writes nothing, and exits
/// on the pipeline's own projection rather than a CLI-invented code.
///
/// When `--orient pinned:<baseline>` lands, this test is where the artifact
/// assertion goes. `plinth-pipeline`'s `prepare_folder.rs` already proves the
/// executor emits a real `.chitubox` under a pinned policy, so the only missing
/// piece is the flag.
#[test]
fn prepare_executes_the_folder_and_reports_every_model_by_name() {
    let scratch = Scratch::new("execute");
    scratch.place("make_toed_slab.prepped.stl", "mini");

    let output = plinth(&[
        "prepare",
        "--out-dir",
        scratch.output().to_str().expect("scratch path is Unicode"),
        "--no-cache",
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
    assert!(
        stdout.contains("orientation is never inferred"),
        "an unorientable model must say why, by name: {stdout}"
    );
    assert!(
        stdout.contains("SUMMARY  emitted=0  gate-refused=0  failed=1  stopped=0"),
        "the run report must total the dispositions: {stdout}"
    );
    // `--no-cache` is accepted and honestly reported: no cache exists yet, so
    // the flag is a no-op rather than a bypass of machinery that is not there.
    assert!(
        stdout.contains("--no-cache is a no-op"),
        "the cache state must be reported honestly: {stdout}"
    );

    // The pipeline's own exit-code projection, not a second contract.
    assert_eq!(output.status.code(), Some(1), "stdout was: {stdout}");
    assert!(
        scratch.output().is_dir(),
        "prepare owns creating its output folder"
    );
    assert!(
        !scratch.output().join("mini.chitubox").exists(),
        "a model that never oriented wrote an artifact"
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
