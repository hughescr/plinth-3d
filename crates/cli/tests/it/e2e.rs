//! End to end (T2): the real `plinth` binary, the real `plinth-check` program and the three
//! original Round STLs, against the committed stage manifest
//! (`fixtures/round/stage-manifest.tsv`). The checker must be built first
//! (`scripts/t` does this once before nextest starts and passes the absolute path in
//! `PLINTH_CHECK_BIN`). These tests never run cargo; without the variable they fail.

use crate::common::{names, root, scratch};
use plinth_cli::json::Json;
use plinth_cli::jsonin;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const GIB: u64 = 1 << 30;
const LABEL: &str = "transport, not a print candidate";

/// The thread count this slot may use (set by `scripts/t`); 1 when absent.
#[allow(
    clippy::disallowed_methods,
    reason = "test-only read of the slot allocation set by scripts/t"
)]
fn slot_threads() -> usize {
    std::env::var("NEXTEST_TEST_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .clamp(1, 8)
}

/// True in the bench epoch (nextest profile `bench`).
#[allow(
    clippy::disallowed_methods,
    reason = "test-only read of the profile name nextest exports"
)]
fn in_bench_epoch() -> bool {
    std::env::var("NEXTEST_PROFILE").is_ok_and(|p| p == "bench")
}

struct Row {
    model: String,
    input: PathBuf,
    mode: String,
    verdict: String,
    pass: BTreeSet<String>,
    violation: BTreeSet<String>,
    unverified: BTreeSet<String>,
    not_applicable: BTreeSet<String>,
}

fn set(text: &str) -> BTreeSet<String> {
    text.split(',')
        .filter(|s| *s != "-" && !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn manifest() -> Vec<Row> {
    let dir = root().join("fixtures/round");
    let text = fs::read_to_string(dir.join("stage-manifest.tsv")).expect("stage manifest");
    let rows: Vec<Row> = text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            assert_eq!(c.len(), 8, "manifest row: {l}");
            Row {
                model: c[0].to_owned(),
                input: dir.join(c[1]),
                mode: c[2].to_owned(),
                verdict: c[3].to_owned(),
                pass: set(c[4]),
                violation: set(c[5]),
                unverified: set(c[6]),
                not_applicable: set(c[7]),
            }
        })
        .collect();
    let models: Vec<&str> = rows.iter().map(|r| r.model.as_str()).collect();
    assert_eq!(
        models,
        ["Round 25", "Round 50", "Round 100"],
        "the manifest covers all three"
    );
    rows
}

/// The built checker program, from `PLINTH_CHECK_BIN` (set by `scripts/t`, which builds it once
/// before nextest starts). These tests never build it and never skip.
#[allow(
    clippy::disallowed_methods,
    reason = "test-only read of the checker path set by scripts/t"
)]
fn checker_bin() -> PathBuf {
    let path = std::env::var_os("PLINTH_CHECK_BIN")
        .map(PathBuf::from)
        .expect("run via scripts/t; PLINTH_CHECK_BIN not set");
    assert!(
        path.is_file(),
        "PLINTH_CHECK_BIN names a file that is not there: {} (run via scripts/t)",
        path.display()
    );
    path
}

/// Run `plinth` with the checker from `PLINTH_CHECK_BIN`, unless `args` pick their own.
fn plinth(args: &[&str], inputs: &[&Path]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_plinth"));
    if !args.contains(&"--checker") {
        cmd.arg("--checker").arg(checker_bin());
    }
    cmd.args(args).args(inputs).output().expect("start plinth")
}

fn text(o: &[u8]) -> String {
    String::from_utf8_lossy(o).into_owned()
}

/// Run `plinth` on `inputs` at `threads`, into `out`, with the report in `report`.
fn run_ok(out: &Path, report: &Path, threads: usize, inputs: &[&Path]) -> Json {
    let t = threads.to_string();
    let args = [
        "--max-threads",
        &t,
        "--out-dir",
        &out.display().to_string(),
        "--report",
        &report.display().to_string(),
    ];
    let o = plinth(&args, inputs);
    assert!(
        o.status.success(),
        "plinth exit {:?}\nstderr: {}\nstdout: {}",
        o.status.code(),
        text(&o.stderr),
        text(&o.stdout)
    );
    jsonin::parse(&fs::read(report).expect("report")).expect("report is JSON")
}

fn model(report: &Json, i: usize) -> &Json {
    &report.get("models").and_then(Json::as_arr).expect("models")[i]
}

fn ids(checker: &Json, status: &str) -> BTreeSet<String> {
    checker
        .get("rules")
        .and_then(Json::as_arr)
        .expect("rules")
        .iter()
        .filter(|r| r.get("status").and_then(Json::as_str) == Some(status))
        .map(|r| r.get("id").and_then(Json::as_str).expect("id").to_owned())
        .collect()
}

/// The triangle count in a binary STL's header.
fn stl_triangles(path: &Path) -> u64 {
    let b = fs::read(path).expect("stl");
    u64::from(u32::from_le_bytes([b[80], b[81], b[82], b[83]]))
}

#[test]
fn e2e_rounds_match_the_stage_manifest() {
    let rows = manifest();
    let w = scratch("e2e-manifest");
    let alloc = slot_threads();
    let inputs: Vec<&Path> = rows.iter().map(|r| r.input.as_path()).collect();
    let out = w.join("out");
    let report = run_ok(&out, &w.join("report.json"), alloc, &inputs);
    // The CLI takes inputs in sorted path order: Round 100, Round 25, Round 50.
    for (i, name) in ["Round 100", "Round 25", "Round 50"].iter().enumerate() {
        let row = rows.iter().find(|r| r.model == *name).expect("row");
        let m = model(&report, i);
        assert_eq!(row.mode, "transport", "only the transport stage exists yet");
        assert_eq!(
            m.get("status").and_then(Json::as_str),
            Some("published"),
            "{name}"
        );
        assert_eq!(
            m.get("kind").and_then(Json::as_str),
            Some(row.mode.as_str())
        );
        assert_eq!(m.get("print_candidate"), Some(&Json::Bool(false)));
        assert_eq!(m.get("label").and_then(Json::as_str), Some(LABEL));
        assert_eq!(
            m.get("triangles").and_then(Json::as_u64),
            Some(stl_triangles(&row.input)),
            "{name}: triangles read equal the STL header's count"
        );
        let c = m.get("checker").expect("checker answer");
        assert_eq!(
            c.get("verdict").and_then(Json::as_str),
            Some(row.verdict.as_str())
        );
        assert_eq!(ids(c, "pass"), row.pass, "{name} pass");
        assert_eq!(ids(c, "violation"), row.violation, "{name} violation");
        assert_eq!(ids(c, "unverified"), row.unverified, "{name} unverified");
        assert_eq!(
            ids(c, "not_applicable"),
            row.not_applicable,
            "{name} not applicable"
        );
        // The file on disk is what the checker read.
        let file = out.join(format!("{name}.chitubox"));
        let bytes = fs::read(&file).expect("published file");
        assert_eq!(
            m.get("bytes").and_then(Json::as_u64),
            Some(bytes.len() as u64)
        );
        let project = plinth_chitubox::parse(&bytes).expect("our reader parses it");
        assert_eq!(project.models.len(), 1);
        let pm = &project.models[0];
        assert!(pm.name.contains(LABEL), "{}", pm.name);
        assert!(pm.supports.is_none(), "a transport file has no supports");
        assert_eq!(pm.mesh.len() as u64, stl_triangles(&row.input));
        assert!(
            (pm.translate[2] - 5.0).abs() < f32::EPSILON,
            "{:?}",
            pm.translate
        );
    }
    assert_eq!(
        names(&out),
        [
            "Round 100.chitubox",
            "Round 25.chitubox",
            "Round 50.chitubox"
        ],
        "no temp or extra file"
    );
    let job = report.get("job").expect("job");
    assert_eq!(job.get("published").and_then(Json::as_u64), Some(3));
    assert_eq!(job.get("failed").and_then(Json::as_u64), Some(0));
    // Memory (NFR-4): printed, and at most 1 GB for the Round bases.
    let run = report.get("run").expect("run");
    let peak = run
        .get("peak_memory")
        .and_then(|p| p.get("job_bytes"))
        .and_then(Json::as_u64);
    let peak = peak.expect("job peak memory is printed");
    assert!(peak > 0 && peak <= GIB, "job peak memory {peak} bytes");
    // Time per step is printed for every step of every model, and the thread cap held.
    for m in run
        .get("models")
        .and_then(Json::as_arr)
        .expect("run models")
    {
        let steps: Vec<&str> = m
            .get("steps")
            .and_then(Json::as_arr)
            .expect("steps")
            .iter()
            .filter_map(|s| s.get("step").and_then(Json::as_str))
            .collect();
        assert_eq!(
            steps,
            ["read", "convert", "serialize", "write", "check", "publish"]
        );
        let ran = m
            .get("parallel_conversion")
            .and_then(|p| p.get("threads_ran"));
        let ran = ran.and_then(Json::as_u64).expect("threads ran");
        assert!(
            ran >= 1 && ran as usize <= alloc,
            "{ran} threads on an allocation of {alloc}"
        );
    }
}

fn round25() -> PathBuf {
    root().join("fixtures/round/original/Round 25.stl")
}

fn bytes_at(w: &Path, tag: &str, threads: usize) -> Vec<u8> {
    let out = w.join(tag);
    let input = round25();
    run_ok(&out, &w.join(format!("{tag}.json")), threads, &[&input]);
    fs::read(out.join("Round 25.chitubox")).expect("published file")
}

#[test]
fn e2e_round25_is_identical_at_one_thread_and_the_slot_allocation() {
    let w = scratch("e2e-threads");
    let alloc = slot_threads();
    let one = bytes_at(&w, "one", 1);
    let many = bytes_at(&w, "many", alloc);
    assert!(!one.is_empty());
    assert!(one == many, "bytes differ between 1 and {alloc} threads");
}

/// Eight workers are more than a writing slot may use, so this runs only in the bench epoch
/// (the `bench` nextest profile); started anywhere else it fails, never passes unrun.
#[test]
fn threads_bench_e2e_round25_is_identical_at_one_and_eight_threads() {
    assert!(
        in_bench_epoch() && slot_threads() >= 8,
        "bench-epoch test started outside the bench epoch (profile bench, 8 threads)"
    );
    let w = scratch("e2e-eight");
    assert!(bytes_at(&w, "one", 1) == bytes_at(&w, "eight", 8));
}

/// A valid input whose stem is 220 characters (final name 229) and a valid report name of
/// 240 characters both work: temp names do not grow with the final name (macOS limit 255).
#[test]
fn e2e_long_but_valid_input_and_report_names_publish() {
    let w = scratch("e2e-long");
    let stem = "x".repeat(220);
    let input = w.join(format!("{stem}.stl"));
    fs::copy(round25(), &input).expect("copy Round 25 to a long name");
    let out = w.join("out");
    let report_path = w.join(format!("{}.json", "r".repeat(235)));
    let report = run_ok(&out, &report_path, 1, &[&input]);
    assert_eq!(
        model(&report, 0).get("status").and_then(Json::as_str),
        Some("published")
    );
    assert_eq!(names(&out), [format!("{stem}.chitubox")]);
    assert_eq!(
        names(&w).iter().filter(|n| n.starts_with('.')).count(),
        0,
        "no temp left beside the report"
    );
    assert!(report_path.is_file());
}

#[test]
fn e2e_exit_codes_and_no_overwrite_through_the_real_binary() {
    let w = scratch("e2e-exit");
    let input = round25();
    let out = w.join("out");
    let report = run_ok(&out, &w.join("first.json"), 1, &[&input]);
    assert_eq!(
        model(&report, 0).get("status").and_then(Json::as_str),
        Some("published")
    );
    let first = fs::read(out.join("Round 25.chitubox")).unwrap();
    // The same output name again: refused for that model, exit 1, the first file untouched.
    let o = plinth(
        &[
            "--out-dir",
            &out.display().to_string(),
            "--report",
            &w.join("second.json").display().to_string(),
        ],
        &[&input],
    );
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert!(
        text(&o.stderr).contains("already exists"),
        "{}",
        text(&o.stderr)
    );
    assert_eq!(fs::read(out.join("Round 25.chitubox")).unwrap(), first);
    assert_eq!(names(&out), ["Round 25.chitubox"]);
    // A missing input: that model fails, exit 1, nothing is written for it.
    let missing = w.join("missing.stl");
    let out2 = w.join("out2");
    let o = plinth(&["--out-dir", &out2.display().to_string()], &[&missing]);
    assert_eq!(o.status.code(), Some(1));
    assert!(names(&out2).is_empty());
    // Usage errors: exit 4. There is no overwrite flag.
    for bad in [
        &["--out-dir", "x", "--overwrite"][..],
        &["--out-dir", "x", "--max-threads", "0"],
        &["--max-threads", "2"],
    ] {
        let o = plinth(bad, &[&input]);
        assert_eq!(o.status.code(), Some(4), "{bad:?}: {}", text(&o.stderr));
    }
    // The checker program is required: with a wrong path, nothing is read or written.
    let out3 = w.join("out3");
    let o = plinth(
        &[
            "--out-dir",
            &out3.display().to_string(),
            "--checker",
            "/nonexistent/plinth-check",
        ],
        &[&input],
    );
    assert_eq!(o.status.code(), Some(4), "{}", text(&o.stderr));
    assert!(!out3.exists(), "not even the output folder is made");
}

fn load_ratios_are_met_or_located(doc: &Json) -> bool {
    let Some(fr) = doc.get("fr21") else {
        return false;
    };
    let Some(ratios) = fr
        .get("load")
        .and_then(|l| l.get("largest_ratios"))
        .and_then(Json::as_arr)
    else {
        return false;
    };
    let Some(warnings) = fr.get("warnings").and_then(Json::as_arr) else {
        return false;
    };
    !ratios.is_empty()
        && ratios.iter().all(|r| {
            let ratio = match r.get("tributary_ratio") {
                Some(Json::Num(r)) if r.is_finite() => *r,
                Some(Json::Uint(r)) => *r as f64,
                _ => return false,
            };
            if ratio <= plinth_support::load::DESIGN_RATIO {
                return true;
            }
            let layer = r.get("worst_layer").and_then(Json::as_u64);
            let contact = r.get("contact").and_then(Json::as_u64);
            warnings.iter().any(|w| {
                matches!(
                    w.get("class").and_then(Json::as_str),
                    Some("intervention" | "unprintable as is")
                ) && w
                    .get("kind")
                    .and_then(Json::as_str)
                    .is_some_and(|s| s.contains("load") || s.contains("ratio"))
                    && w.get("message")
                        .and_then(Json::as_str)
                        .is_some_and(|s| !s.is_empty())
                    && [("layers", layer), ("contacts", contact)]
                        .iter()
                        .any(|(key, id)| {
                            id.is_some()
                                && w.get(key)
                                    .and_then(Json::as_arr)
                                    .is_some_and(|a| a.iter().any(|x| x.as_u64() == *id))
                        })
            })
        })
}

#[test]
fn named_load_blocker_test_rejects_prose_and_wrong_locations() {
    let parse = |s: &str| jsonin::parse(s.as_bytes()).unwrap();
    let mut doc = parse(
        r#"{"fr21":{"load":{"largest_ratios":[{"tributary_ratio":21.0,"worst_layer":42,"contact":3}],"sharing_models":"load intervention unprintable"},"warnings":[]}}"#,
    );
    assert!(!load_ratios_are_met_or_located(&doc));
    doc = parse(
        r#"{"fr21":{"load":{"largest_ratios":[{"tributary_ratio":21.0,"worst_layer":42,"contact":3}]},"warnings":[{"class":"intervention","kind":"load ratio","layers":[41],"contacts":[2],"message":"add a support"}]}}"#,
    );
    assert!(!load_ratios_are_met_or_located(&doc));
    doc = parse(
        r#"{"fr21":{"load":{"largest_ratios":[{"tributary_ratio":21.0,"worst_layer":42,"contact":3}]},"warnings":[{"class":"intervention","kind":"load ratio","layers":[42],"contacts":[],"message":"add a support"}]}}"#,
    );
    assert!(load_ratios_are_met_or_located(&doc));
}

/// Deliverable witness: construction failure is a failure, never transport fallback.
#[test]
fn e2e_approved_round25_never_falls_back_to_transport() {
    let w = scratch("approved-construction");
    let input = root().join("fixtures/round/approved/Round 25.keepup.stl");
    let out = w.join("out");
    let report = w.join("run.json");
    let result = plinth(
        &[
            "--max-threads",
            &slot_threads().to_string(),
            "--out-dir",
            &out.display().to_string(),
            "--report",
            &report.display().to_string(),
        ],
        &[&input],
    );
    println!(
        "output directory: {}\nreport: {}\n{}",
        out.display(),
        report.display(),
        text(&result.stderr)
    );
    let doc = jsonin::parse(&fs::read(&report).unwrap()).unwrap();
    println!("{}", doc.render());
    let m = model(&doc, 0);
    assert_eq!(m.get("print_candidate"), Some(&Json::Bool(false)));
    assert!(result.status.success(), "{}", text(&result.stderr));
    assert_eq!(m.get("kind").and_then(Json::as_str), Some("supported"));
    let file = std::path::Path::new(m.get("output").and_then(Json::as_str).unwrap());
    let bundle = file.parent().unwrap();
    let bytes = fs::read(file).unwrap();
    let native = plinth_chitubox::parse(&bytes).unwrap();
    let records = &native.models[0].supports.as_ref().unwrap().records;
    let contacts = records
        .iter()
        .filter(|r| r.record_type == plinth_chitubox::RecordType::TIP_BALL)
        .count();
    assert!(contacts >= 12, "only {contacts} contacts");
    // A frozen output has passed rooted column/brace/raft completion; it cannot
    // be emitted from an unrouted proposal. Require real non-head members too.
    assert!(records.len() > contacts * 4, "no routed members");
    let fr = jsonin::parse(&fs::read(bundle.join("Round 25.keepup.fr21.json")).unwrap()).unwrap();
    assert!(
        load_ratios_are_met_or_located(&fr),
        "unreported overload: {}",
        fr.render()
    );
    assert_eq!(
        fs::read_dir(bundle.join("Round 25.keepup.pictures"))
            .unwrap()
            .count(),
        12
    );
}
