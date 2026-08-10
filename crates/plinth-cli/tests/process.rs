use std::process::{Command, Output};

fn plinth(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_plinth"))
        .args(arguments)
        .output()
        .expect("run plinth process")
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

#[test]
fn valid_paths_with_spaces_reach_only_the_unimplemented_runner_boundary() {
    let output = plinth(&[
        "prepare",
        "--out-dir",
        "prepared models",
        "--no-cache",
        "--input-models-dir",
        "source models",
        "--output",
        "saturn4-ultra",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        "plinth prepare: execution is not implemented in this parser milestone\n"
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
            "wall thickness \"-2.5\" is not a positive finite number of millimetres",
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
