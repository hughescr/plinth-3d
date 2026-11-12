//! Per-rule normal-CLI tests (lane C; design `step11-construction.md` section 8.1).
//!
//! For each rule H1 to H7 and H9, a file made through the production writer that breaks it must
//! reach no print candidate, no geometry acceptance, name the rule and a location in the
//! report, and exit nonzero. Each rule is also tried with a measurement that cannot be made
//! (a part that is not the native shape; for H7 no input STL; for H5 an open shell) and with a
//! stale file hash. The checker runs as the real `plinth-check` program, through the
//! production hand-off (`plinth_cli::check::run`, the call the publish step makes) and once
//! directly for its report and exit code.
//!
//! What this shows: a failed or unverified checker answer comes back as blockers
//! (`Checked::candidate_blockers`), which is the data on which the publish step refuses a
//! print candidate (`run::write_check_publish`, `cfg.candidate`). The last test drives that
//! publish step for real on a model-only run. What it does not show: the supported-output
//! publish path (lane A's seam), which does not exist yet.

use crate::broken::{self, Case, Expect, all_cases, scratch};
use crate::readers_agree::{checker_binary, repo_root};
use plinth_cli::check::{self, Job, Verdict};
use plinth_cli::json::Json;
use plinth_cli::jsonin;
use std::fs;
use std::path::Path;
use std::process::Command;

/// The rules a file can break, in order.
const RULES: [&str; 8] = ["H1", "H2", "H3", "H4", "H5", "H6", "H7", "H9"];

/// The first case of `rule` that must be a violation.
fn violation_case(rule: &str) -> &'static Case {
    all_cases()
        .find(|c| c.rule == rule && c.expect == Expect::Violation)
        .unwrap_or_else(|| panic!("{rule} has no violation case"))
}

/// Run the real checker directly and return its exit code and parsed report.
fn run_binary(dir: &Path, bytes: &[u8], stl: Option<&[u8]>) -> (i32, Json) {
    let file = dir.join("f.chitubox");
    fs::write(&file, bytes).expect("write file");
    let report = dir.join("direct.json");
    let limits = repo_root().join("limits.toml");
    let mut cmd = Command::new(checker_binary());
    cmd.arg("check")
        .arg("--chitubox")
        .arg(&file)
        .arg("--limits")
        .arg(&limits)
        .arg("--out")
        .arg(&report);
    if let Some(s) = stl {
        let p = dir.join("f.stl");
        fs::write(&p, s).expect("write stl");
        cmd.arg("--stl").arg(&p);
    }
    let out = cmd.output().expect("runs the checker");
    let text = fs::read(&report).expect("the checker wrote a report");
    let json = jsonin::parse(&text).expect("report is JSON");
    (out.status.code().unwrap_or(-1), json)
}

fn rule<'a>(report: &'a Json, id: &str) -> &'a Json {
    report
        .get("rules")
        .and_then(Json::as_arr)
        .and_then(|a| {
            a.iter()
                .find(|r| r.get("id").and_then(Json::as_str) == Some(id))
        })
        .unwrap_or_else(|| panic!("no rule {id} in the report"))
}

fn rejection_for<'a>(report: &'a Json, id: &str, kind: &str) -> Option<&'a Json> {
    report
        .get("rejections")
        .and_then(Json::as_arr)?
        .iter()
        .find(|r| {
            r.get("rule").and_then(Json::as_str) == Some(id)
                && r.get("kind").and_then(Json::as_str) == Some(kind)
        })
}

fn is_false(report: &Json, key: &str) -> bool {
    report.get(key) == Some(&Json::Bool(false))
}

fn acceptance(report: &Json) -> Option<&str> {
    report.get("geometry")?.get("acceptance")?.as_str()
}

/// A file in which `rule`'s measurement cannot be made, and the STL to give with it.
fn unmeasurable(rule: &str) -> (Vec<u8>, Option<Vec<u8>>) {
    let mut built = broken::sound();
    match rule {
        "H4" => {
            // The marker pad on the plate is not the native shape.
            if let Some(last) = built.records.last_mut() {
                last.mesh.pop();
            }
        }
        "H5" => {
            built.mesh.pop();
        }
        _ => {
            // The column rod (record 3) is not the native shape for its fields.
            built.records[3].mesh.pop();
        }
    }
    let stl = (rule != "H7").then(|| built.stl_bytes());
    (built.chitubox(), stl)
}

#[test]
fn broken_each_violated_rule_blocks_the_candidate_and_is_named_with_a_location() {
    for id in RULES {
        let case = violation_case(id);
        let built = (case.build)();
        let (bytes, stl) = (built.chitubox(), built.stl_bytes());
        let dir = scratch(&format!("block-{id}"));
        // Through the production hand-off: the answer the publish step consults.
        let checked = broken::check_bytes(&dir, &bytes, &stl)
            .unwrap_or_else(|e| panic!("{id} / {}: {e}", case.name));
        assert_eq!(checked.verdict, Verdict::Fail, "{id} / {}", case.name);
        let blockers = checked.candidate_blockers();
        assert!(
            blockers
                .iter()
                .any(|b| b.starts_with(&format!("{id} is violation"))),
            "{id} / {}: {blockers:?}",
            case.name
        );
        // The checker's own report and exit code.
        let (code, report) = run_binary(&dir, &bytes, Some(&stl));
        assert_eq!(code, 1, "{id} / {}: a violation exits 1", case.name);
        assert_eq!(
            rule(&report, id).get("status").and_then(Json::as_str),
            Some("violation")
        );
        assert_eq!(
            acceptance(&report),
            Some("violation"),
            "{id} / {}",
            case.name
        );
        assert!(is_false(&report, "ready_for_owner_review"), "{id}");
        assert!(is_false(&report, "credible_to_print"), "{id}");
        let rej = rejection_for(&report, id, "violation")
            .unwrap_or_else(|| panic!("{id} / {}: no rejection entry", case.name));
        let loc = rej
            .get("location")
            .unwrap_or_else(|| panic!("{id}: no location key"));
        let placed = loc.get("record").is_some_and(|r| r != &Json::Null)
            || loc.get("xyz").is_some_and(|x| x != &Json::Null);
        assert!(
            placed,
            "{id} / {}: the rejection has no record or position: {rej:?}",
            case.name
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn broken_each_review_case_blocks_the_candidate_through_the_normal_cli() {
    // The files that used to pass a rule falsely (mesh wider or narrower than its fields' bound,
    // a lens join, an air gap, a patch interval too wide, a same-support pair of patches, a
    // region that closes on the model, a model nothing holds) are blocked by the production
    // hand-off and by the program's own exit code and report, each with the rule named.
    for (id, name) in broken::REVIEW_CASES {
        let case = all_cases()
            .find(|c| c.rule == *id && c.name == *name)
            .unwrap_or_else(|| panic!("{id} / {name}: no such case"));
        let word = case.expect.word();
        let built = (case.build)();
        let (bytes, stl) = (built.chitubox(), built.stl_bytes());
        let dir = scratch(&format!("review-{id}-{}", name.len()));
        let checked = broken::check_bytes(&dir, &bytes, &stl)
            .unwrap_or_else(|e| panic!("{id} / {name}: {e}"));
        let blockers = checked.candidate_blockers();
        assert!(
            blockers
                .iter()
                .any(|b| b.starts_with(&format!("{id} is {word}"))),
            "{id} / {name}: {blockers:?}"
        );
        let (code, report) = run_binary(&dir, &bytes, Some(&stl));
        // Nonzero: 1 for a violation anywhere in the file, 2 when nothing worse than an
        // unverified rule is found.
        assert!(
            code == 1 || (code == 2 && word == "unverified"),
            "{id} / {name}: exit {code}"
        );
        assert_eq!(
            rule(&report, id).get("status").and_then(Json::as_str),
            Some(word),
            "{id} / {name}"
        );
        assert_ne!(acceptance(&report), Some("accepted"), "{id} / {name}");
        assert!(is_false(&report, "ready_for_owner_review"), "{id} / {name}");
        assert!(is_false(&report, "credible_to_print"), "{id} / {name}");
        assert!(
            rejection_for(&report, id, word).is_some(),
            "{id} / {name}: not in the rejection list"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn broken_each_unmeasurable_rule_blocks_the_candidate_as_unverified() {
    for id in RULES {
        let (bytes, stl) = unmeasurable(id);
        let dir = scratch(&format!("unver-{id}"));
        let stl_ref = stl.as_deref();
        let (code, report) = run_binary(&dir, &bytes, stl_ref);
        let status = rule(&report, id).get("status").and_then(Json::as_str);
        assert_eq!(status, Some("unverified"), "{id}: {report:?}");
        assert_eq!(code, 2, "{id}: unverified exits 2");
        assert!(is_false(&report, "ready_for_owner_review"), "{id}");
        assert_eq!(acceptance(&report), Some("unverified"), "{id}");
        assert!(
            rejection_for(&report, id, "unverified").is_some(),
            "{id}: not in the rejection list"
        );
        // Through the hand-off (it always passes an STL path, so H7 is only exercised above).
        if id != "H7" {
            let checked =
                broken::check_bytes(&dir, &bytes, stl_ref.unwrap_or(&[])).expect("answer");
            assert!(
                checked
                    .candidate_blockers()
                    .iter()
                    .any(|b| b.starts_with(&format!("{id} is unverified"))),
                "{id}: {:?}",
                checked.candidate_blockers()
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn broken_a_stale_file_hash_is_refused_for_every_rule() {
    // The checker's answer is about one set of bytes; the publish step holds another: the same
    // size with one byte changed, so only the hash can tell.
    for id in RULES {
        let case = violation_case(id);
        let built = (case.build)();
        let (bytes, stl) = (built.chitubox(), built.stl_bytes());
        let mut other = bytes.clone();
        other[100] ^= 1;
        let dir = scratch(&format!("stale-{id}"));
        let file = dir.join("f.chitubox");
        fs::write(&file, &bytes).expect("write");
        fs::write(dir.join("f.stl"), &stl).expect("write");
        let limits = repo_root().join("limits.toml");
        let answer = check::run(&Job {
            checker: &checker_binary(),
            limits: &limits,
            chitubox: &file,
            stl: &dir.join("f.stl"),
            report: &dir.join("r.json"),
            written: &other,
            declared_hollow: false,
        });
        let err = answer.expect_err("an answer about other bytes must be refused");
        assert!(err.contains("another file"), "{id}: {err}");
        let _ = fs::remove_dir_all(&dir);
    }
}

/// A binary STL of a cube of half-width `h` centred on the origin.
fn cube_stl(h: f32) -> Vec<u8> {
    let tris = broken::cube([0.0; 3], h, false);
    let mut out = vec![0_u8; 80];
    out.extend_from_slice(&u32::try_from(tris.len()).expect("count").to_le_bytes());
    for t in &tris {
        out.extend_from_slice(&[0; 12]);
        for v in t.iter().flatten() {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&[0; 2]);
    }
    out
}

#[test]
fn broken_a_model_only_run_in_candidate_mode_publishes_nothing() {
    // The real publish step (`plinth_cli::run::run`) with the real checker: a transport
    // file has no supports, so H3 and H8 are unverified and nothing may be offered.
    let dir = scratch("publish");
    let input = dir.join("cube.stl");
    fs::write(&input, cube_stl(5.0)).expect("write stl");
    let out_dir = dir.join("out");
    let cfg = plinth_cli::run::Config {
        inputs: vec![input],
        out_dir: out_dir.clone(),
        max_threads: 1,
        checker: checker_binary(),
        limits: repo_root().join("limits.toml"),
        candidate: true,
        review: None,
    };
    let run = plinth_cli::run::run(&cfg, plinth_cli::publish::Faults::default(), &mut |_| {})
        .expect("setup is fine");
    assert_eq!(run.published(), 0);
    match &run.models[0].outcome {
        plinth_cli::run::Outcome::Failed {
            stage,
            message,
            checked,
        } => {
            assert_eq!(*stage, "candidate", "{message}");
            let checked = checked.as_ref().expect("the checker answered");
            // The transport file's lift, fidelity and closed solid are measured and fine.
            let st = |id: &str| {
                checked
                    .rules
                    .iter()
                    .find(|r| r.id == id)
                    .map(|r| r.status.clone())
            };
            assert_eq!(st("H4").as_deref(), Some("pass"), "{message}");
            assert_eq!(st("H7").as_deref(), Some("pass"), "{message}");
            assert_eq!(st("H5").as_deref(), Some("not_applicable"), "{message}");
            assert_eq!(st("H3").as_deref(), Some("unverified"), "{message}");
        }
        other => panic!("expected a refused candidate, got {other:?}"),
    }
    let left: Vec<_> = fs::read_dir(&out_dir)
        .map(|rd| rd.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "files left behind: {left:?}");
    let _ = fs::remove_dir_all(&dir);
}
