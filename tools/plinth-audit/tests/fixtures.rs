//! The planted-defect fixture harness.
//!
//! A fixture is a `.chitubox` file carrying ONE deliberate defect, plus a
//! statement of which check must fire on it. The harness runs the real audit
//! over the file and asserts the stated check actually fails with the stated
//! code. **A fixture whose defect does not trip its check is a HARNESS
//! FAILURE** -- that is the whole point of the instrument: it proves a check
//! can see the thing it claims to see.
//!
//! # Fixture layout: one pair of files per fixture, never a shared manifest
//!
//! ```text
//! fixtures/<name>.chitubox        the planted-defect file
//! fixtures/<name>.expect.json     what must happen to it
//! ```
//!
//! One pair per fixture, so eight agents adding fixtures in parallel never
//! touch the same file. There is deliberately no `fixtures/manifest.json`.
//!
//! # `<name>.expect.json`
//!
//! ```jsonc
//! {
//!   "description": "tip ball resting on the surface, not embedded",
//!   "check":  "i1_seat_depth",       // schema key from src/checks/mod.rs
//!   "expect": "check_fails",         // or "check_passes", or "file_fails"
//!   "codes":  ["SEAT_DEPTH_ZERO"],   // at least ONE must appear (check_fails)
//!   "model":  0,                     // optional, default 0
//!   "min_failures": 1                // optional, default 1
//! }
//! ```
//!
//! * `"expect": "check_fails"` -- the named check must report `"fail"` AND at
//!   least one failure whose `code` is in `codes`. This is the normal form.
//! * `"expect": "check_passes"` -- a NEGATIVE CONTROL. The named check must
//!   report `"pass"` on this file. Every agent should ship at least one, or a
//!   check that fails everything looks perfect.
//! * `"expect": "file_fails"` -- a file-level abort. `codes` are matched
//!   against `verdict.file_level_failures[].code`, e.g. `FILE_PARSE_FAILED`.
//!
//! # Pending fixtures, and why they do not go green
//!
//! While a check is still the stub from `src/checks/mod.rs` it fails
//! everything with `AUDIT_CHECK_NOT_IMPLEMENTED`. That is **not** the fixture's
//! defect firing, and the harness will not accept it as one: such a fixture is
//! reported PENDING and counted, never passed. `AUDIT_CHECK_NOT_IMPLEMENTED`
//! is refused as an expected `code` outright, so nobody can satisfy a fixture
//! by leaving a check unwritten.
//!
//! # Building a fixture file
//!
//! Use [`plinth_audit::synth`]. It writes a valid container and validates
//! nothing, so the defect you plant stays planted. See its module docs.

use plinth_audit::check::NOT_IMPLEMENTED_CODE;
use plinth_audit::checks::GATING_IDS;
use plinth_audit::context::AuditOptions;
use plinth_audit::json::{self, Json};
use plinth_audit::run;
use std::path::{Path, PathBuf};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

#[derive(Debug)]
struct Expectation {
    name: String,
    artifact: PathBuf,
    description: String,
    check: String,
    expect: String,
    codes: Vec<String>,
    model: usize,
    min_failures: usize,
}

fn collect_expect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.filter_map(Result::ok) {
        let p = e.path();
        if p.is_dir() {
            // Fixtures live one directory per fixture -- `<name>/<name>.expect.json`
            // -- so discovery MUST descend. A flat scan finds nothing and the
            // harness then goes green having proved nothing at all, which is
            // exactly the failure this file exists to prevent.
            collect_expect_files(&p, out);
        } else if p.to_str().is_some_and(|s| s.ends_with(".expect.json")) {
            out.push(p);
        }
    }
}

fn load_expectations() -> Vec<Result<Expectation, String>> {
    let mut paths: Vec<PathBuf> = Vec::new();
    collect_expect_files(&fixtures_dir(), &mut paths);
    paths.sort();
    paths.iter().map(|p| parse_expectation(p)).collect()
}

fn parse_expectation(path: &Path) -> Result<Expectation, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let v = json::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .trim_end_matches(".expect.json")
        .to_string();
    let artifact = path.with_file_name(format!("{name}.chitubox"));
    if !artifact.exists() {
        return Err(format!(
            "{}: no sibling artifact at {}",
            path.display(),
            artifact.display()
        ));
    }
    let get_str = |k: &str| -> Result<String, String> {
        v.get(k)
            .and_then(Json::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("{}: missing string field {k:?}", path.display()))
    };
    let expect = get_str("expect")?;
    if !["check_fails", "check_passes", "file_fails"].contains(&expect.as_str()) {
        return Err(format!(
            "{}: expect must be check_fails, check_passes or file_fails, got {expect:?}",
            path.display()
        ));
    }
    let check = if expect == "file_fails" {
        v.get("check")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string()
    } else {
        let c = get_str("check")?;
        if !GATING_IDS.contains(&c.as_str()) {
            return Err(format!(
                "{}: check {c:?} is not one of {GATING_IDS:?}",
                path.display()
            ));
        }
        c
    };
    let codes: Vec<String> = v
        .get("codes")
        .and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if codes.iter().any(|c| c == NOT_IMPLEMENTED_CODE) {
        return Err(format!(
            "{}: {NOT_IMPLEMENTED_CODE} is not an acceptable expected code -- a fixture \
             may not be satisfied by leaving a check unwritten",
            path.display()
        ));
    }
    if expect != "check_passes" && codes.is_empty() {
        return Err(format!(
            "{}: expect {expect:?} needs a non-empty `codes` array",
            path.display()
        ));
    }
    Ok(Expectation {
        name,
        artifact,
        description: v
            .get("description")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string(),
        check,
        expect,
        codes,
        model: v.get("model").and_then(Json::as_f64).unwrap_or(0.0) as usize,
        min_failures: v
            .get("min_failures")
            .and_then(Json::as_f64)
            .map_or(1, |x| x as usize),
    })
}

/// Outcome of running one fixture.
enum Verdict {
    Satisfied,
    /// The named check is still a stub, so nothing was proved. Never a pass.
    Pending,
    Failed(String),
}

/// Every OTHER gating check that also failed on this fixture. A fixture is
/// meant to plant ONE defect; anything else that fires is collateral, and the
/// harness prints it so "trips its check AND ONLY its check" is a measured
/// statement rather than an assumption. It is reported, not asserted: a
/// synthetic file can legitimately fail a second invariant (a bare island has
/// no supports at all, so several load invariants are vacuous or open), and
/// the honest move is to show the list.
fn collateral_failures(doc: &Json, model_index: usize, target: &str) -> Vec<String> {
    let Some(model) = doc
        .get("models")
        .and_then(Json::as_array)
        .and_then(|a| a.get(model_index))
    else {
        return Vec::new();
    };
    let Some(inv) = model.get("invariants") else {
        return Vec::new();
    };
    GATING_IDS
        .iter()
        .filter(|id| **id != target)
        .filter(|id| {
            inv.get(id)
                .and_then(|i| i.get("status"))
                .and_then(Json::as_str)
                == Some("fail")
        })
        .map(|id| (*id).to_string())
        .collect()
}

fn run_fixture(e: &Expectation) -> Verdict {
    let options = AuditOptions::default();
    let result = run::audit_file(&e.artifact, &options, None);
    let doc = &result.document;

    if e.expect == "file_fails" {
        let found: Vec<String> = doc
            .get("verdict")
            .and_then(|v| v.get("file_level_failures"))
            .and_then(Json::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|f| f.get("code").and_then(Json::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        return if e.codes.iter().any(|c| found.contains(c)) {
            Verdict::Satisfied
        } else {
            Verdict::Failed(format!(
                "expected one of {:?} among the file-level failures, found {found:?}",
                e.codes
            ))
        };
    }

    let Some(model) = doc
        .get("models")
        .and_then(Json::as_array)
        .and_then(|a| a.get(e.model))
    else {
        return Verdict::Failed(format!("no model {} in the result document", e.model));
    };
    let Some(inv) = model.get("invariants").and_then(|i| i.get(&e.check)) else {
        return Verdict::Failed(format!("no invariant {:?} in the result document", e.check));
    };
    let status = inv.get("status").and_then(Json::as_str).unwrap_or("");
    let failures: Vec<(String, String)> = inv
        .get("failures")
        .and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .map(|f| {
                    (
                        f.get("code")
                            .and_then(Json::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        f.get("message")
                            .and_then(Json::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    // A stub proves nothing, whatever the fixture asks for.
    if failures.iter().any(|(c, _)| c == NOT_IMPLEMENTED_CODE) {
        return Verdict::Pending;
    }

    match e.expect.as_str() {
        "check_passes" => {
            if status == "pass" {
                Verdict::Satisfied
            } else {
                Verdict::Failed(format!(
                    "negative control: {} must PASS this file, but it reported {status} with {:?}",
                    e.check,
                    failures.iter().map(|(c, _)| c).collect::<Vec<_>>()
                ))
            }
        }
        _ => {
            if status != "fail" {
                return Verdict::Failed(format!(
                    "{} reported {status}; the planted defect did not trip it",
                    e.check
                ));
            }
            let matched: Vec<&(String, String)> = failures
                .iter()
                .filter(|(c, _)| e.codes.contains(c))
                .collect();
            if matched.len() < e.min_failures {
                return Verdict::Failed(format!(
                    "{} failed, but with {:?}; expected at least {} of {:?}. \
                     A check that fails for the WRONG reason has not seen the defect.",
                    e.check,
                    failures.iter().map(|(c, _)| c).collect::<Vec<_>>(),
                    e.min_failures,
                    e.codes
                ));
            }
            // The message must be non-empty and must name the code, so the log
            // and the JSON agree (schema rule 4).
            for (code, msg) in matched {
                if !msg.starts_with(code.as_str()) {
                    return Verdict::Failed(format!(
                        "failure message {msg:?} does not begin with its code {code:?}"
                    ));
                }
            }
            Verdict::Satisfied
        }
    }
}

#[test]
fn every_fixture_trips_the_check_it_names() {
    let expectations = load_expectations();
    assert!(
        !expectations.is_empty(),
        "no fixtures discovered under {}. An empty discovery is a HARNESS FAILURE, \
         never a pass: a harness that finds nothing proves nothing. Each fixture is \
         <dir>/<name>.chitubox + <dir>/<name>.expect.json.",
        fixtures_dir().display()
    );
    let mut satisfied = 0usize;
    let mut pending: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    for e in expectations {
        match e {
            Err(msg) => failed.push(format!("MALFORMED  {msg}")),
            Ok(e) => match run_fixture(&e) {
                Verdict::Satisfied => {
                    satisfied += 1;
                    let doc = run::audit_file(&e.artifact, &AuditOptions::default(), None).document;
                    let collateral = collateral_failures(&doc, e.model, &e.check);
                    eprintln!(
                        "  ok       {} [{}] {}\n             also failing: {}",
                        e.name,
                        e.check,
                        e.description,
                        if collateral.is_empty() {
                            "none -- this fixture trips its check and ONLY its check".to_string()
                        } else {
                            collateral.join(", ")
                        }
                    );
                }
                Verdict::Pending => {
                    pending.push(format!("{} [{}] {}", e.name, e.check, e.description));
                }
                Verdict::Failed(why) => {
                    failed.push(format!("FIXTURE    {} [{}]: {why}", e.name, e.check));
                }
            },
        }
    }
    eprintln!(
        "fixtures: {satisfied} satisfied, {} pending, {} failed",
        pending.len(),
        failed.len()
    );
    for p in &pending {
        eprintln!("  PENDING  {p}  (its check is still a stub; nothing was proved)");
    }
    assert!(
        failed.is_empty(),
        "fixture harness failures:\n  {}",
        failed.join("\n  ")
    );
}

/// The guard that keeps the harness honest while the checks are unwritten.
///
/// Every stub must report NOT IMPLEMENTED as a **failure**, with
/// `checked == indeterminate` so the counts stay truthful, and must never
/// report `"pass"`. If this test ever goes quiet because a stub started
/// passing, the never-pass rule has been broken.
#[test]
fn every_unimplemented_check_reports_failure_never_pass() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("exports");
    let sample = dir.join("sphere-first-artifact.chitubox");
    if !sample.exists() {
        eprintln!("no artifact at {}; skipping", sample.display());
        return;
    }
    let result = run::audit_file(&sample, &AuditOptions::default(), None);
    let model = result
        .document
        .get("models")
        .and_then(Json::as_array)
        .and_then(|a| a.first())
        .expect("one model");
    let invariants = model.get("invariants").expect("invariants");
    let mut stubs = 0usize;
    for id in GATING_IDS {
        let inv = invariants.get(id).unwrap_or_else(|| panic!("no {id}"));
        let status = inv.get("status").and_then(Json::as_str).unwrap_or("");
        let codes: Vec<&str> = inv
            .get("failures")
            .and_then(Json::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|f| f.get("code").and_then(Json::as_str))
                    .collect()
            })
            .unwrap_or_default();
        if codes.contains(&NOT_IMPLEMENTED_CODE) {
            stubs += 1;
            assert_eq!(status, "fail", "{id}: a stub must never report pass");
            let checked = inv.get("checked").and_then(Json::as_f64).unwrap_or(0.0);
            let indet = inv
                .get("indeterminate")
                .and_then(Json::as_f64)
                .unwrap_or(0.0);
            assert!(checked > 0.0, "{id}: a stub must report an honest scope");
            assert!(
                (checked - indet).abs() < 0.5,
                "{id}: a stub must count every item it did not establish"
            );
        }
        assert!(
            status == "pass" || status == "fail",
            "{id}: status must be exactly pass or fail, got {status:?}"
        );
    }
    eprintln!("{stubs} of {} invariants are still stubs", GATING_IDS.len());
}

/// `print_candidate` is true only when all EIGHT gating invariants pass. With
/// any stub present it must be false, on every file.
#[test]
fn a_file_is_never_a_print_candidate_while_a_stub_remains() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("exports");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    // A full eight-invariant audit is minutes of CPU per artifact -- the
    // treant is 28 MB / 429k model triangles / 2585 records, and even the
    // small `cylinder` needs 133 million ownership samples. Walking all of
    // `exports/` turned this binary into a 38-minute test nobody would run,
    // and a test nobody runs proves nothing. The claim under test here is a
    // property of the DRIVER, not of any particular model, so the two
    // SMALLEST artifacts establish it just as well as eight do. The corpus
    // sweep is a separate, deliberate run with its own per-model time bound.
    const N_SMALLEST: usize = 2;
    let mut candidates: Vec<(u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("chitubox"))
        .map(|p| (std::fs::metadata(&p).map(|m| m.len()).unwrap_or(u64::MAX), p))
        .collect();
    candidates.sort();
    candidates.truncate(N_SMALLEST);
    for (bytes, p) in candidates {
        eprintln!("never-pass check on {} ({bytes} bytes)", p.display());
        let r = run::audit_file(&p, &AuditOptions::default(), None);
        let stubbed = r
            .document
            .get("models")
            .and_then(Json::as_array)
            .map(|models| {
                models.iter().any(|m| {
                    m.get("invariants").is_some_and(|inv| {
                        GATING_IDS.iter().any(|id| {
                            inv.get(id)
                                .and_then(|i| i.get("failures"))
                                .and_then(Json::as_array)
                                .is_some_and(|f| {
                                    f.iter().any(|x| {
                                        x.get("code").and_then(Json::as_str)
                                            == Some(NOT_IMPLEMENTED_CODE)
                                    })
                                })
                        })
                    })
                })
            })
            .unwrap_or(false);
        if stubbed {
            assert!(
                !r.print_candidate,
                "{} was called a print candidate with stubs present",
                p.display()
            );
            assert_ne!(
                r.exit_code(),
                0,
                "{}: exit code must be non-zero",
                p.display()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The harness auditing itself
// ---------------------------------------------------------------------------
//
// A harness that cannot fail proves nothing about the fixtures it runs. These
// three tests sabotage it deliberately: one fixture whose defect genuinely
// fires must be Satisfied, one whose defect does NOT fire must be Failed, and
// one aimed at a stub must be Pending rather than either.

fn scratch_dir() -> PathBuf {
    let d = std::env::temp_dir().join("plinth-audit-harness-selftest");
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

fn expectation(artifact: PathBuf, check: &str, expect: &str, codes: &[&str]) -> Expectation {
    Expectation {
        name: "selftest".into(),
        artifact,
        description: "harness self-test".into(),
        check: check.into(),
        expect: expect.into(),
        codes: codes.iter().map(|s| (*s).to_string()).collect(),
        model: 0,
        min_failures: 1,
    }
}

/// A small, valid, well-formed file: closed model, one seated contact chain.
fn clean_artifact(path: &Path) {
    use plinth_audit::reader::RecordKind;
    use plinth_audit::synth::{box_soup, SynthFile, SynthModel, SynthRecord};
    let f = SynthFile::new().with_model(SynthModel {
        name: "selftest".into(),
        translate_mm: [0.0, 0.0, 5.0],
        mesh: box_soup([-5.0, -5.0, -2.0], [5.0, 5.0, 2.0]),
        records: vec![
            SynthRecord::new(
                RecordKind::ContactSphere,
                [0.0, 0.0, -1.9],
                [0.0, 0.0, -2.8],
                0.45,
                0.45,
            )
            .with_matching_soup(),
            SynthRecord::new(
                RecordKind::ContactTaper,
                [0.0, 0.0, -2.35],
                [0.0, 0.0, -4.0],
                0.3,
                0.35,
            )
            .with_declared_depth(0.1)
            .with_matching_soup(),
            SynthRecord::new(
                RecordKind::Member,
                [0.0, 0.0, -4.0],
                [0.0, 0.0, -6.8],
                0.35,
                0.35,
            )
            .with_matching_soup(),
            SynthRecord::new(
                RecordKind::RaftPadOrBridge,
                [0.0, 0.0, -7.0],
                [0.0, 0.0, -7.0],
                2.2,
                0.2,
            )
            .as_pad(),
        ],
    });
    f.write(path).expect("write fixture");
}

#[test]
fn the_harness_confirms_a_defect_that_really_fires() {
    let p = scratch_dir().join("truncated.chitubox");
    // Six bytes: not even a header. FILE_PARSE_FAILED must fire.
    std::fs::write(&p, [0x43u8, 0x12, 0x23, 0xAB, 0x01, 0x00]).expect("write");
    let e = expectation(p, "", "file_fails", &["FILE_PARSE_FAILED"]);
    assert!(
        matches!(run_fixture(&e), Verdict::Satisfied),
        "the harness must confirm a defect that really fires"
    );
}

#[test]
fn the_harness_refuses_a_fixture_whose_defect_does_not_fire() {
    let p = scratch_dir().join("clean.chitubox");
    clean_artifact(&p);
    // This file parses cleanly and its mesh is closed, so the claimed defect
    // is absent. The harness must FAIL rather than shrug.
    let e = expectation(p, "", "file_fails", &["FILE_MESH_NOT_CLOSED"]);
    match run_fixture(&e) {
        Verdict::Failed(why) => {
            assert!(why.contains("FILE_MESH_NOT_CLOSED"), "{why}");
        }
        Verdict::Satisfied => panic!("the harness accepted a defect that never fired"),
        Verdict::Pending => panic!("a file-level expectation cannot be pending"),
    }
}

#[test]
fn a_stub_can_never_satisfy_a_fixture() {
    let p = scratch_dir().join("clean.chitubox");
    clean_artifact(&p);
    // I1 is asked to fire with a real code. While it is a stub it fails with
    // AUDIT_CHECK_NOT_IMPLEMENTED, which must count as PENDING, never as the
    // fixture being satisfied -- otherwise every fixture goes green by nobody
    // writing any check at all.
    let e = expectation(
        p.clone(),
        "i1_seat_depth",
        "check_fails",
        &["SEAT_DEPTH_ZERO"],
    );
    match run_fixture(&e) {
        Verdict::Pending => {}
        Verdict::Satisfied => panic!("a stub satisfied a fixture; the harness is worthless"),
        Verdict::Failed(_) => { /* acceptable once I1 is implemented and correct */ }
    }
    // The same protection covers a negative control.
    let e = expectation(p, "i1_seat_depth", "check_passes", &[]);
    assert!(
        !matches!(run_fixture(&e), Verdict::Satisfied),
        "a stub must never satisfy a negative control either"
    );
}

/// `AUDIT_CHECK_NOT_IMPLEMENTED` must be refused as an expected code, so the
/// pending path cannot be bypassed by naming the stub's own code.
#[test]
fn the_stub_code_is_refused_as_an_expected_code() {
    let dir = scratch_dir();
    let art = dir.join("named.chitubox");
    clean_artifact(&art);
    let spec = dir.join("named.expect.json");
    std::fs::write(
        &spec,
        format!(
            r#"{{"description":"x","check":"i1_seat_depth","expect":"check_fails",
                 "codes":["{NOT_IMPLEMENTED_CODE}"]}}"#
        ),
    )
    .expect("write");
    let err = parse_expectation(&spec).expect_err("must be refused");
    assert!(err.contains(NOT_IMPLEMENTED_CODE), "{err}");
}
