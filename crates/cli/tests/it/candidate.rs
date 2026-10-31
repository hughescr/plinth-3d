//! A print candidate is never published while any applicable rule is unverified or violated
//! (design dev-loop.md section 6, "Broken files"; section 10 step 8). These run the normal CLI
//! path (the library `run::run`, and the real `plinth` binary for the exit code and the
//! report) with a fake checker that answers as a checker would for a broken plan. Each blocked
//! case has a control that does publish, so a CLI that blocked everything would fail here.

use crate::common::{Fake, config, names, scratch, write_box};
use plinth_cli::json::Json;
use plinth_cli::jsonin;
use plinth_cli::publish::Faults;
use plinth_cli::run::{self, Config, Outcome, Run};
use std::fs;
use std::path::Path;
use std::process::Command;

const RULES: [&str; 9] = ["H1", "H2", "H3", "H4", "H5", "H6", "H7", "H8", "H9"];

/// A checker answer: every rule `status` except the listed ones.
fn answer(
    verdict: &'static str,
    code: i32,
    status: &'static str,
    except: &[(&str, &'static str)],
) -> Fake {
    Fake {
        verdict,
        code,
        rules: RULES
            .iter()
            .map(|id| {
                let s = except.iter().find(|e| e.0 == *id).map_or(status, |e| e.1);
                (*id, s)
            })
            .collect(),
        ..Fake::honest()
    }
}

fn candidate_cfg(w: &Path, fake: &Fake, tag: &str) -> Config {
    let mut cfg = config(w, vec![write_box(w, "a.stl", 10.0)]);
    cfg.checker = fake.install(w, tag);
    cfg.candidate = true;
    cfg
}

fn go(cfg: &Config) -> Run {
    run::run(cfg, Faults::default(), &mut |_| {}).expect("run starts")
}

fn failure(r: &Run) -> (&'static str, &str) {
    match &r.models[0].outcome {
        Outcome::Failed { stage, message, .. } => (stage, message),
        Outcome::Published { path, .. } => panic!("published {}", path.display()),
    }
}

#[test]
fn broken_candidate_all_rules_unverified_is_blocked_and_nothing_is_left() {
    let w = scratch("cand-unverified");
    let cfg = candidate_cfg(&w, &Fake::honest(), "fake-honest");
    let r = go(&cfg);
    let (stage, message) = failure(&r);
    assert_eq!(stage, "candidate", "{message}");
    for id in RULES {
        assert!(
            message.contains(&format!("{id} is unverified")),
            "{id}: {message}"
        );
    }
    assert_eq!(
        names(&cfg.out_dir),
        Vec::<String>::new(),
        "no file, no temp file"
    );
    assert_eq!(r.published(), 0);
}

#[test]
fn broken_candidate_one_unverified_rule_is_blocked() {
    let w = scratch("cand-one");
    let fake = answer(
        "unverified",
        2,
        "pass",
        &[("H3", "unverified"), ("H8", "not_applicable")],
    );
    let cfg = candidate_cfg(&w, &fake, "fake-h3");
    let r = go(&cfg);
    let (stage, message) = failure(&r);
    assert_eq!(stage, "candidate");
    assert!(message.contains("H3 is unverified"), "{message}");
    assert!(
        !message.contains("H1 is"),
        "only the undecided rule is named: {message}"
    );
    assert_eq!(names(&cfg.out_dir), Vec::<String>::new());
}

#[test]
fn broken_candidate_a_violated_rule_is_blocked_and_named() {
    for id in RULES {
        let w = scratch(&format!("cand-viol-{id}"));
        let fake = answer("fail", 1, "pass", &[(id, "violation")]);
        let cfg = candidate_cfg(&w, &fake, "fake-viol");
        let r = go(&cfg);
        let (stage, message) = failure(&r);
        assert_eq!(
            stage, "check",
            "a violation is a checker rejection: {message}"
        );
        assert!(message.contains(id), "{id}: {message}");
        assert_eq!(names(&cfg.out_dir), Vec::<String>::new(), "{id}");
    }
}

#[test]
fn broken_candidate_control_a_clean_pass_is_published() {
    let w = scratch("cand-pass");
    // Every rule decided; H8 is the owner's ChituBox record, not the checker's measurement.
    let fake = answer("pass", 0, "pass", &[("H8", "not_applicable")]);
    let cfg = candidate_cfg(&w, &fake, "fake-pass");
    let r = go(&cfg);
    assert!(matches!(r.models[0].outcome, Outcome::Published { .. }));
    assert_eq!(names(&cfg.out_dir), ["a.chitubox"]);
}

#[test]
fn broken_candidate_control_the_same_unverified_file_is_published_as_transport() {
    let w = scratch("cand-transport");
    let mut cfg = candidate_cfg(&w, &Fake::honest(), "fake-honest");
    cfg.candidate = false;
    let r = go(&cfg);
    assert!(matches!(r.models[0].outcome, Outcome::Published { .. }));
    assert_eq!(names(&cfg.out_dir), ["a.chitubox"]);
}

/// Run the real `plinth` binary with `--candidate` and the fake checker.
fn binary(w: &Path, fake: &Fake, tag: &str) -> (i32, Json, Vec<String>) {
    let checker = fake.install(w, tag);
    let limits = w.join("limits.toml");
    fs::write(&limits, "[limits]\n").expect("limits");
    let input = write_box(w, "a.stl", 10.0);
    let out = w.join("out");
    let report = w.join("report.json");
    let status = Command::new(env!("CARGO_BIN_EXE_plinth"))
        .args(["--candidate", "--max-threads", "1", "--out-dir"])
        .arg(&out)
        .arg("--report")
        .arg(&report)
        .arg("--checker")
        .arg(&checker)
        .arg("--limits")
        .arg(&limits)
        .arg(&input)
        .output()
        .expect("start plinth")
        .status;
    let json = jsonin::parse(&fs::read(&report).expect("report")).expect("report is JSON");
    (status.code().unwrap_or(-1), json, names(&out))
}

#[test]
fn broken_candidate_binary_exits_nonzero_and_reports_no_candidate() {
    let w = scratch("cand-bin-blocked");
    let (code, report, files) = binary(&w, &Fake::honest(), "fake-honest");
    assert_eq!(code, 1);
    assert!(files.is_empty(), "{files:?}");
    assert_eq!(report.get("print_candidate"), Some(&Json::Bool(false)));
    assert_eq!(report.get("kind").and_then(Json::as_str), Some("candidate"));
    let m = &report.get("models").and_then(Json::as_arr).expect("models")[0];
    assert_eq!(
        m.get("failed_stage").and_then(Json::as_str),
        Some("candidate")
    );
    assert_eq!(m.get("print_candidate"), Some(&Json::Bool(false)));
    let blockers = m
        .get("candidate_blockers")
        .and_then(Json::as_arr)
        .expect("blockers");
    assert_eq!(blockers.len(), 10, "the verdict and nine rules");
}

#[test]
fn broken_candidate_binary_control_a_pass_exits_zero_and_reports_a_candidate() {
    let w = scratch("cand-bin-pass");
    let fake = answer("pass", 0, "pass", &[("H8", "not_applicable")]);
    let (code, report, files) = binary(&w, &fake, "fake-pass");
    assert_eq!(code, 0);
    assert_eq!(files, ["a.chitubox"]);
    assert_eq!(report.get("print_candidate"), Some(&Json::Bool(true)));
    let m = &report.get("models").and_then(Json::as_arr).expect("models")[0];
    assert_eq!(m.get("print_candidate"), Some(&Json::Bool(true)));
    assert_eq!(
        m.get("candidate_blockers")
            .and_then(Json::as_arr)
            .map(<[Json]>::len),
        Some(0)
    );
}
