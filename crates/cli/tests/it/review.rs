//! Review mode (`--review-construction`): supports for any already posed STL, to look at.
//! It completes no stage and never produces a print candidate. These tests use a small box
//! (the planner finishes it in well under a second) and a fake checker, so they are fast;
//! the real checker runs on the owner-facing models by hand.

use crate::common::{Fake, config, names, root, scratch, write_box};
use plinth_cli::publish::Faults;
use plinth_cli::run::{self, Config, Outcome, REVIEW_FILE_TAG, REVIEW_LABEL, Review, Run};
use plinth_cli::{jsonin, report};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

fn review_cfg(w: &Path, fake: &Fake, tag: &str, budget: Duration) -> Config {
    let mut cfg = config(w, vec![write_box(w, "posed.stl", 3.0)]);
    cfg.checker = fake.install(w, tag);
    // Construction needs the real limits (the shared helper writes a stub).
    cfg.limits = root().join("limits.toml");
    cfg.review = Some(Review {
        plan_budget: budget,
        checker_timeout: Duration::from_secs(60),
        stop_plan_at_phase: None,
        hollow_fixture: false,
    });
    cfg
}

fn go(cfg: &Config) -> Run {
    run::run(cfg, Faults::default(), &mut |_| {}).expect("run starts")
}

fn published(r: &Run) -> (&Path, &run::ModelRun) {
    match &r.models[0].outcome {
        Outcome::Published { path, .. } => (path, &r.models[0]),
        Outcome::Failed { stage, message, .. } => panic!("failed at {stage}: {message}"),
    }
}

fn failure(r: &Run) -> (&'static str, &str) {
    match &r.models[0].outcome {
        Outcome::Failed { stage, message, .. } => (stage, message),
        Outcome::Published { path, .. } => panic!("published {}", path.display()),
    }
}

/// Models whose steps include the planner: the path-hit count of the construction path.
fn planner_hits(r: &Run) -> usize {
    r.models
        .iter()
        .filter(|m| m.steps.iter().any(|s| s.name == "plan"))
        .count()
}

fn violated_h2() -> Fake {
    Fake {
        verdict: "fail",
        code: 1,
        ..Fake::honest().with_rule("H2", "violation")
    }
}

#[test]
fn review_mode_and_candidate_mode_never_combine() {
    let w = scratch("review-conflict");
    let mut cfg = review_cfg(&w, &Fake::honest(), "fake", Duration::from_secs(60));
    cfg.candidate = true;
    let e = run::run(&cfg, Faults::default(), &mut |_| {}).unwrap_err();
    assert!(e.contains("cannot be combined"), "{e}");
    assert!(!cfg.out_dir.exists(), "refused before any output");
    // The real binary refuses the flags too, as a usage error.
    let out = Command::new(env!("CARGO_BIN_EXE_plinth"))
        .args(["--candidate", "--review-construction", "--out-dir"])
        .arg(w.join("bin-out"))
        .arg(w.join("posed.stl"))
        .output()
        .expect("start plinth");
    assert_eq!(out.status.code(), Some(4));
    assert!(!w.join("bin-out").exists());
}

#[test]
fn hollow_refusal_happens_before_planning_and_publication() {
    let w = scratch("review-hollow-refuse");
    let mut cfg = review_cfg(&w, &Fake::honest(), "fake", Duration::from_secs(60));
    cfg.review.as_mut().unwrap().hollow_fixture = true;
    // A 3 mm box cannot contain the required 2.501 mm inward offset.
    let r = go(&cfg);
    let (stage, message) = failure(&r);
    assert_eq!(stage, "hollow", "{message}");
    assert!(message.contains("offset"), "{message}");
    assert_eq!(planner_hits(&r), 0);
    assert!(r.models[0].steps.iter().any(|s| s.name == "hollow"));
    assert!(!r.models[0].steps.iter().any(|s| s.name == "check"));
    assert_eq!(names(&cfg.out_dir), Vec::<String>::new());
}

#[test]
fn an_arbitrary_mesh_reaches_the_planner_and_every_label_says_review_only() {
    let w = scratch("review-arbitrary");
    let cfg = review_cfg(&w, &Fake::honest(), "fake", Duration::from_secs(60));
    let r = go(&cfg);
    assert_eq!(planner_hits(&r), 1, "the posed box must be planned");
    let (path, m) = published(&r);
    assert!(m.supported);
    let steps: Vec<&str> = m.steps.iter().map(|s| s.name).collect();
    for needed in ["slice", "plan", "construct", "emit", "check", "report"] {
        assert!(steps.contains(&needed), "{needed} missing in {steps:?}");
    }
    assert!(!steps.contains(&"convert"), "never the transport path");
    // File name, embedded model name, text and JSON reports carry the label.
    let file = path.file_name().unwrap().to_string_lossy();
    assert!(file.contains(REVIEW_FILE_TAG), "{file}");
    // A plan the deadline never touched says nothing of being stopped.
    assert_eq!(m.review.expect("review record").stopped_at_budget, None);
    assert!(!file.contains("stopped-at-time-budget"), "{file}");
    let native = plinth_chitubox::parse(&fs::read(path).unwrap()).unwrap();
    assert!(native.models[0].name.contains(REVIEW_LABEL));
    assert!(!native.models[0].name.contains("stopped at time budget"));
    let stem = file.trim_end_matches(".chitubox");
    let dir = path.parent().unwrap();
    let text = fs::read_to_string(dir.join(format!("{stem}.fr21.txt"))).unwrap();
    assert!(text.contains(REVIEW_LABEL), "{text}");
    let fr = jsonin::parse(&fs::read(dir.join(format!("{stem}.fr21.json"))).unwrap()).unwrap();
    assert_eq!(
        fr.get("status").and_then(plinth_cli::json::Json::as_str),
        Some(REVIEW_LABEL)
    );
    let doc = report::to_json(&cfg, &r);
    let json = doc.render();
    assert!(json.contains("\"kind\": \"review\""), "{json}");
    assert!(json.contains("\"print_candidate\": false"));
    assert!(!json.contains("\"print_candidate\": true"));
    assert!(json.contains("plan_budget_seconds"));
    assert!(json.contains("\"step\": \"plan\""), "per-step timings");
    let line = report::progress_line(false, m);
    assert!(line.contains(REVIEW_LABEL), "{line}");
    // The same input outside review mode is the transport path and never plans.
    let w = scratch("review-control");
    let mut plain = config(&w, vec![write_box(&w, "posed.stl", 3.0)]);
    plain.limits = root().join("limits.toml");
    let r = go(&plain);
    assert_eq!(planner_hits(&r), 0);
    assert!(!r.models[0].supported);
}

#[test]
fn a_forged_filename_never_grants_approval() {
    let w = scratch("review-forged");
    let input = w.join("Round 25.keepup.stl");
    fs::write(&input, crate::common::box_stl([1.0, 2.0, 3.0], 3.0)).unwrap();
    let mut cfg = config(&w, vec![input]);
    cfg.limits = root().join("limits.toml");
    let r = go(&cfg);
    assert_eq!(
        planner_hits(&r),
        0,
        "approval is the content hash, not the name"
    );
    assert!(!r.models[0].supported);
    assert_eq!(names(&cfg.out_dir), ["Round 25.keepup.chitubox"]);
}

#[test]
fn an_expired_budget_refuses_and_names_the_phase() {
    let w = scratch("review-budget-refuse");
    let cfg = review_cfg(&w, &Fake::honest(), "fake", Duration::from_nanos(1));
    let r = go(&cfg);
    let (stage, message) = failure(&r);
    assert_eq!(stage, "plan");
    assert!(message.contains("time budget"), "{message}");
    assert!(message.contains("during"), "the phase is named: {message}");
    assert!(
        r.models[0].steps.iter().any(|s| s.name == "plan"),
        "the planning time is reported"
    );
    assert_eq!(names(&cfg.out_dir), Vec::<String>::new(), "nothing is left");
}

#[test]
fn a_short_budget_never_yields_an_unlabelled_file() {
    // The planner's deadline is forced at a named phase (a test hook, not the clock, so the
    // result does not depend on machine speed). Stopped before any contact is chosen, the
    // result is a refusal naming the budget; stopped with contacts chosen, it is a file
    // that says it was stopped (in its name, its embedded name and its status); a plan
    // that finished in time says nothing of the kind.
    let stop_at = |tag: &str, phase: &'static str| {
        let w = scratch(&format!("review-budget-{tag}"));
        let mut cfg = review_cfg(&w, &Fake::honest(), "fake", Duration::from_secs(600));
        cfg.review.as_mut().unwrap().stop_plan_at_phase = Some(phase);
        let r = go(&cfg);
        (cfg, r)
    };
    // Refuse at budget: nothing chosen yet.
    let (cfg, r) = stop_at("refuse", "round start");
    let (stage, message) = failure(&r);
    assert_eq!(stage, "plan", "{message}");
    assert!(message.contains("time budget"), "{message}");
    assert!(message.contains("round start"), "{message}");
    assert_eq!(names(&cfg.out_dir), Vec::<String>::new(), "nothing is left");
    // Emit at budget: contacts chosen, the file says so everywhere.
    let (_cfg, r) = stop_at("emit", "anchoring and coverage");
    let (path, m) = published(&r);
    let review = m.review.expect("review record");
    assert_eq!(review.stopped_at_budget, Some("anchoring and coverage"));
    let file = path.file_name().unwrap().to_string_lossy();
    assert!(file.contains("stopped-at-time-budget"), "{file}");
    assert!(file.contains(REVIEW_FILE_TAG), "{file}");
    let native = plinth_chitubox::parse(&fs::read(path).unwrap()).unwrap();
    let name = &native.models[0].name;
    assert!(name.contains("stopped at time budget"), "{name}");
    assert!(name.contains(REVIEW_LABEL));
    assert!(review_status_says_stopped(m));
}

fn review_status_says_stopped(m: &run::ModelRun) -> bool {
    m.review
        .as_ref()
        .is_some_and(|r| run::review_status(r).contains("stopped at time budget"))
}

#[test]
fn a_checker_failed_file_is_kept_in_review_mode_only() {
    let w = scratch("review-keep");
    let cfg = review_cfg(&w, &violated_h2(), "fake-fail", Duration::from_secs(60));
    let r = go(&cfg);
    let (path, m) = published(&r);
    assert!(m.review.expect("review record").checker_failed);
    let file = path.file_name().unwrap().to_string_lossy();
    assert!(
        file.contains("checker-failed") && file.contains(REVIEW_FILE_TAG),
        "{file}"
    );
    assert!(path.is_file());
    let dir = path.parent().unwrap();
    let report_name = file.replace(".chitubox", ".checker-report.json");
    let raw = jsonin::parse(&fs::read(dir.join(&report_name)).expect("checker report kept"))
        .expect("the checker's own report");
    assert_eq!(
        raw.get("verdict").and_then(plinth_cli::json::Json::as_str),
        Some("fail")
    );
    let line = report::progress_line(false, m);
    assert!(line.contains("checker-failed"), "{line}");
    let doc = report::to_json(&cfg, &r).render();
    assert!(doc.contains("\"checker_failed\": true"), "{doc}");
    // The same violation outside review mode is a refusal and leaves nothing.
    let w = scratch("review-keep-control");
    let mut plain = config(&w, vec![write_box(&w, "posed.stl", 3.0)]);
    plain.limits = root().join("limits.toml");
    plain.checker = violated_h2().install(&w, "fake-fail");
    let r = go(&plain);
    assert_eq!(failure(&r).0, "check");
    assert_eq!(names(&plain.out_dir), Vec::<String>::new());
    plain.candidate = true;
    let r = go(&plain);
    assert_eq!(failure(&r).0, "check");
    assert_eq!(names(&plain.out_dir), Vec::<String>::new());
}

#[test]
fn a_malformed_file_is_still_refused_in_review_mode() {
    let w = scratch("review-malformed");
    let fake = Fake {
        verdict: "fail",
        code: 1,
        file_fields: r#""readable":false,"parse_error":"bad magic""#,
        ..Fake::honest()
    };
    let cfg = review_cfg(&w, &fake, "fake-bad", Duration::from_secs(60));
    let r = go(&cfg);
    assert_eq!(failure(&r).0, "check");
    assert_eq!(names(&cfg.out_dir), Vec::<String>::new());
}

#[test]
fn a_checker_that_hangs_is_stopped_at_the_timeout() {
    let w = scratch("review-timeout");
    let mut cfg = review_cfg(&w, &Fake::honest(), "fake", Duration::from_secs(60));
    let hang: PathBuf = w.join("hang-check");
    fs::write(&hang, "#!/bin/sh\nexec sleep 30\n").unwrap();
    fs::set_permissions(&hang, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    cfg.checker = hang;
    cfg.review = Some(Review {
        plan_budget: Duration::from_secs(60),
        checker_timeout: Duration::from_millis(300),
        stop_plan_at_phase: None,
        hollow_fixture: false,
    });
    let started = std::time::Instant::now();
    let r = go(&cfg);
    let (stage, message) = failure(&r);
    assert_eq!(stage, "check");
    assert!(message.contains("did not finish"), "{message}");
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "the checker was not waited for"
    );
    assert_eq!(names(&cfg.out_dir), Vec::<String>::new());
}
