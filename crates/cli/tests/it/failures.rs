//! Failure handling of the transport path (PRD FR-37, FR-42; design section 7.3): bad input,
//! output-name collisions, an interrupted write, a full disk, a crash before publishing and
//! a checker that fails or misbehaves. A fake checker keeps these independent of the real
//! one; the real one is exercised by the `e2e_*` tests.

use crate::common::{Fake, config, fake_checker, names, scratch, write_box};
use plinth_cli::publish::{ENOSPC, Fault, Faults};
use plinth_cli::run::{self, Config, Outcome, Run};
use std::fs;

fn go(cfg: &Config, faults: Faults) -> Run {
    run::run(cfg, faults, &mut |_| {}).expect("run starts")
}

fn failed_stage(r: &Run, i: usize) -> (&'static str, &str) {
    match &r.models[i].outcome {
        Outcome::Failed { stage, message, .. } => (stage, message),
        Outcome::Published { path, .. } => panic!("model {i} was published: {}", path.display()),
    }
}

fn published_name(r: &Run, i: usize) -> String {
    match &r.models[i].outcome {
        Outcome::Published { path, .. } => path.file_name().unwrap().to_string_lossy().into_owned(),
        Outcome::Failed { stage, message, .. } => panic!("model {i} failed at {stage}: {message}"),
    }
}

#[test]
fn a_good_run_publishes_and_leaves_no_temp_file() {
    let w = scratch("good");
    let cfg = config(&w, vec![write_box(&w, "a.stl", 10.0)]);
    let r = go(&cfg, Faults::default());
    assert_eq!(published_name(&r, 0), "a.chitubox");
    assert_eq!(names(&cfg.out_dir), ["a.chitubox"]);
    let steps: Vec<&str> = r.models[0].steps.iter().map(|s| s.name).collect();
    assert_eq!(
        steps,
        ["read", "convert", "serialize", "write", "check", "publish"]
    );
}

#[test]
fn bad_input_fails_that_model_only_and_leaves_no_file_for_it() {
    let w = scratch("bad-input");
    let good = write_box(&w, "b-good.stl", 10.0);
    let short = w.join("a-short.stl");
    fs::write(&short, [0_u8; 20]).unwrap();
    let truncated = w.join("c-truncated.stl");
    let mut bytes = crate::common::box_stl([0.0; 3], 5.0);
    bytes.truncate(bytes.len() - 7);
    fs::write(&truncated, bytes).unwrap();
    let garbage = w.join("d-garbage.stl");
    fs::write(&garbage, vec![0x5a_u8; 4000]).unwrap();
    let empty = w.join("e-empty.stl");
    fs::write(&empty, b"").unwrap();
    let missing = w.join("f-missing.stl");
    let nan = w.join("g-nan.stl");
    let mut bytes = crate::common::box_stl([0.0; 3], 5.0);
    bytes[84 + 12..84 + 16].copy_from_slice(&f32::NAN.to_le_bytes());
    fs::write(&nan, bytes).unwrap();
    let cfg = config(
        &w,
        vec![short, good, truncated, garbage, empty, missing, nan],
    );
    let r = go(&cfg, Faults::default());
    // Models are in sorted path order: a-short, b-good, c-truncated, ...
    assert_eq!(r.models.len(), 7);
    assert_eq!(published_name(&r, 1), "b-good.chitubox");
    for i in [0, 2, 3, 4, 5, 6] {
        let (stage, message) = failed_stage(&r, i);
        assert_eq!(stage, "read", "model {i}: {message}");
        assert!(
            message.contains(".stl"),
            "the message names the input: {message}"
        );
        assert!(r.models[i].steps.iter().all(|s| s.name == "read"));
    }
    assert_eq!(
        names(&cfg.out_dir),
        ["b-good.chitubox"],
        "no file, no temp, for bad input"
    );
    assert_eq!(r.published(), 1);
}

#[test]
fn same_stem_in_different_folders_gets_two_names_and_neither_overwrites() {
    let w = scratch("collide");
    let one = write_box(&w.join("one"), "x.stl", 10.0);
    let two = write_box(&w.join("two"), "x.stl", 20.0);
    let upper = write_box(&w.join("three"), "X.stl", 30.0);
    let cfg = config(&w, vec![two, upper, one]);
    let r = go(&cfg, Faults::default());
    let got: Vec<String> = (0..3).map(|i| published_name(&r, i)).collect();
    // Sorted path order is one/x.stl, three/X.stl, two/x.stl, whatever the order given.
    // Names compare without regard to case, so the second and third clash with the first.
    assert_eq!(got, ["x.chitubox", "X-2.chitubox", "x-3.chitubox"]);
    let folders: Vec<_> = r
        .models
        .iter()
        .map(|m| m.input.parent().unwrap().file_name().unwrap().to_owned())
        .collect();
    assert_eq!(folders, ["one", "three", "two"]);
    // Each file holds its own box: the three boxes differ, so the three files differ.
    let files: Vec<Vec<u8>> = got
        .iter()
        .map(|n| fs::read(cfg.out_dir.join(n)).unwrap())
        .collect();
    assert!(files[0] != files[1] && files[1] != files[2] && files[0] != files[2]);
    assert_eq!(names(&cfg.out_dir).len(), 3);
}

#[test]
fn an_existing_output_is_never_replaced() {
    let w = scratch("exists");
    let a = write_box(&w, "a.stl", 10.0);
    let b = write_box(&w, "b.stl", 10.0);
    let cfg = config(&w, vec![a, b]);
    fs::create_dir_all(&cfg.out_dir).unwrap();
    fs::write(cfg.out_dir.join("a.chitubox"), b"another input's output").unwrap();
    let r = go(&cfg, Faults::default());
    let (stage, message) = failed_stage(&r, 0);
    assert_eq!(stage, "publish");
    assert!(message.contains("already exists"), "{message}");
    assert_eq!(published_name(&r, 1), "b.chitubox");
    assert_eq!(
        fs::read(cfg.out_dir.join("a.chitubox")).unwrap(),
        b"another input's output"
    );
    assert_eq!(
        names(&cfg.out_dir),
        ["a.chitubox", "b.chitubox"],
        "no temp left"
    );
}

#[test]
fn an_interrupted_write_leaves_no_file_and_the_other_model_is_kept() {
    let w = scratch("interrupt");
    let a = write_box(&w, "a.stl", 10.0);
    let b = write_box(&w, "b.stl", 10.0);
    let cfg = config(&w, vec![a, b]);
    for model in [0, 1] {
        let _ = fs::remove_dir_all(&cfg.out_dir);
        let faults = Faults {
            model,
            fault: Fault::Interrupt { after: 1000 },
        };
        let r = go(&cfg, faults);
        let (stage, message) = failed_stage(&r, model);
        assert_eq!(stage, "write");
        assert!(message.contains("interrupted"), "{message}");
        let other = 1 - model;
        let want = if other == 0 {
            "a.chitubox"
        } else {
            "b.chitubox"
        };
        assert_eq!(published_name(&r, other), want);
        assert_eq!(
            names(&cfg.out_dir),
            [want],
            "only the finished model remains"
        );
        // The checker never ran on the partial file.
        assert!(r.models[model].steps.iter().all(|s| s.name != "check"));
    }
}

#[test]
fn a_full_disk_leaves_no_file_and_says_so() {
    let w = scratch("full");
    let cfg = config(&w, vec![write_box(&w, "a.stl", 10.0)]);
    for after in [0, 4096] {
        let faults = Faults {
            model: 0,
            fault: Fault::DiskFull { after },
        };
        let r = go(&cfg, faults);
        let (stage, message) = failed_stage(&r, 0);
        assert_eq!(stage, "write");
        let os = std::io::Error::from_raw_os_error(ENOSPC).to_string();
        assert!(message.contains(&os), "{message} should mention {os}");
        assert!(names(&cfg.out_dir).is_empty(), "{:?}", names(&cfg.out_dir));
    }
}

#[test]
fn a_crash_before_publish_leaves_only_a_hidden_temp_and_the_next_run_works() {
    let w = scratch("crash");
    let cfg = config(&w, vec![write_box(&w, "a.stl", 10.0)]);
    let r = go(
        &cfg,
        Faults {
            model: 0,
            fault: Fault::CrashBeforePublish,
        },
    );
    let (stage, _) = failed_stage(&r, 0);
    assert_eq!(stage, "publish");
    let left = names(&cfg.out_dir);
    assert_eq!(left.len(), 1, "{left:?}");
    assert!(
        left[0].starts_with('.') && left[0].contains(".tmp-"),
        "{left:?}"
    );
    assert!(
        !left.iter().any(|n| n == "a.chitubox"),
        "no final name looks complete"
    );
    let r = go(&cfg, Faults::default());
    assert_eq!(published_name(&r, 0), "a.chitubox");
}

#[test]
fn a_failing_checker_blocks_publishing_and_removes_the_temp_file() {
    let w = scratch("check-fail");
    let mut cfg = config(&w, vec![write_box(&w, "a.stl", 10.0)]);
    let rejecting = Fake {
        verdict: "fail",
        code: 1,
        ..Fake::honest()
    }
    .with_rule("H2", "violation");
    cfg.checker = rejecting.install(&w, "fake-rejecting");
    let r = go(&cfg, Faults::default());
    let (stage, message) = failed_stage(&r, 0);
    assert_eq!(stage, "check");
    assert!(message.contains("rejected"), "{message}");
    assert!(names(&cfg.out_dir).is_empty(), "{:?}", names(&cfg.out_dir));
}

#[test]
fn a_misbehaving_checker_never_lets_a_file_through() {
    let w = scratch("check-odd");
    let input = write_box(&w, "a.stl", 10.0);
    let odd = |name: &str, fake: Fake| fake.install(&w, name);
    let cases = [
        fake_checker(&w, "unverified", 4, 0), // I/O or usage error, not a verdict
        fake_checker(&w, "unverified", 0, 0), // exit code says pass, report says unverified
        fake_checker(&w, "pass", 2, 0),       // exit code says unverified, report says pass
        fake_checker(&w, "unverified", 2, 5), // read a file of another size
        w.join("no-such-checker"),            // cannot start
        // The verdict says unverified (exit 2), but a rule is violated.
        odd("violation", Fake::honest().with_rule("H1", "violation")),
        // Unverified (exit 2), but the file is marked unreadable.
        odd(
            "unreadable",
            Fake {
                file_fields: r#""readable":false"#,
                ..Fake::honest()
            },
        ),
        // The regions do not tile the file, or an anomaly is listed.
        odd(
            "untiled",
            Fake {
                file_fields: r#""readable":true,"regions_tile_the_file":false,"anomalies":[],"layout_walk_differences":[]"#,
                ..Fake::honest()
            },
        ),
        odd(
            "anomaly",
            Fake {
                file_fields: r#""readable":true,"regions_tile_the_file":true,"anomalies":["odd"],"layout_walk_differences":[]"#,
                ..Fake::honest()
            },
        ),
        // Pass (exit 0) with only H1 listed, and passing.
        odd(
            "only-h1",
            Fake {
                verdict: "pass",
                code: 0,
                rules: vec![("H1", "pass")],
                ..Fake::honest()
            },
        ),
        // Pass (exit 0) with every rule listed, one of them still unverified.
        odd(
            "pass-unverified",
            Fake {
                verdict: "pass",
                code: 0,
                rules: Fake::honest()
                    .rules
                    .into_iter()
                    .map(|(id, _)| (id, if id == "H4" { "unverified" } else { "pass" }))
                    .collect(),
                ..Fake::honest()
            },
        ),
        // A rule listed twice, and a status nobody defined.
        odd(
            "duplicate",
            Fake {
                rules: vec![("H1", "unverified"); 9],
                ..Fake::honest()
            },
        ),
        odd("unknown-status", Fake::honest().with_rule("H3", "maybe")),
        // The real size, but the hash of another file of the same size.
        odd(
            "wrong-file",
            Fake {
                hash_other_same_size: true,
                ..Fake::honest()
            },
        ),
    ];
    // What each refusal must say, in the order above: a case must fail for its own reason,
    // not because a stand-in script broke.
    let reasons = [
        "exit code 4",
        "exit code says pass",
        "exit code says unverified",
        "another size",
        "not found",
        "call for fail",
        "call for fail",
        "call for fail",
        "call for fail",
        "H1 to H9",
        "call for unverified",
        "H1 to H9",
        "unknown status",
        "another file than the one written",
    ];
    assert_eq!(cases.len(), reasons.len());
    for (checker, reason) in cases.into_iter().zip(reasons) {
        let mut cfg = config(&w, vec![input.clone()]);
        cfg.checker = checker.clone();
        let started = run::run(&cfg, Faults::default(), &mut |_| {});
        match started {
            Err(setup) => assert!(setup.contains(reason), "{setup}"),
            Ok(r) => {
                let (stage, message) = failed_stage(&r, 0);
                assert_eq!(stage, "check", "{}: {message}", checker.display());
                assert!(message.contains(reason), "{}: {message}", checker.display());
            }
        }
        assert!(
            !cfg.out_dir.exists() || names(&cfg.out_dir).is_empty(),
            "{}: {:?}",
            checker.display(),
            names(&cfg.out_dir)
        );
    }
}

#[test]
fn a_missing_limits_file_stops_the_run_before_any_input_is_read() {
    let w = scratch("no-limits");
    let mut cfg = config(&w, vec![w.join("never-read.stl")]);
    cfg.limits = w.join("no-such-limits.toml");
    let e = run::run(&cfg, Faults::default(), &mut |_| {}).unwrap_err();
    assert!(e.contains("limits file not found"), "{e}");
}
