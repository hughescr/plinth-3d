//! The write-only perf counters (`geom_kernel::perf`) on a real sweep.
//!
//! The counters are process-global, and a test binary runs its tests
//! concurrently and shares one rayon pool setting, so the parent below runs
//! this same binary in fresh child processes at `PLINTH_THREADS` 1 and 8 (the
//! pattern `orient_thread_sweep` uses) and compares the sidecars the children
//! write. A third child runs with counting off and must write nothing.
//!
//! What it pins:
//! * the exact (`work`) counters are equal at 1 and 8 threads, byte for byte
//!   as serialised, so a counter that depends on scheduling fails here;
//! * the instrumented paths fired (anti-vacuity): sweep runs, down-sweep
//!   passes and cell-layers, frame solves, brace partner searches;
//! * counting off writes no sidecar.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::harness;

use harness::sweep_with_uncached;
use plinth_mesh::make_tall_stilt_plate;
use plinth_sweep::{perf, SweepPolicy};

const CHILD_MODE: &str = "PLINTH_PERF_COUNTERS_CHILD";
const CHILD_TEST: &str = "perf_counters::child_runs_one_sweep";
const SIDECAR: &str = "stilt.perf.json";

/// Not a test of its own: the body the parent launches in a fresh process.
/// Without the marker variable it returns at once.
#[test]
fn child_runs_one_sweep() {
    if env::var_os(CHILD_MODE).is_none() {
        return;
    }
    let dir = env::var_os("PLINTH_PERF_DIR");
    if let Some(dir) = &dir {
        perf::enable(PathBuf::from(dir), perf::DEFAULT_CHECKPOINT);
    }
    assert_eq!(perf::enabled(), dir.is_some(), "counting is on only when asked");
    perf::begin_model("stilt");
    let policy = SweepPolicy::from_registry().expect("the registry supplies every number");
    let mesh = make_tall_stilt_plate().expect("the tall stilt plate fixture");
    // Uncached: the counters count the run, so a remembered plan would read zero.
    let plan = sweep_with_uncached(&mesh, &policy);
    assert!(plan.report.struts_total > 0, "the sweep placed nothing");
    perf::end_model();
}

fn unique_directory(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    env::temp_dir().join(format!("plinth-perf-counters-{tag}-{}-{nanos}", std::process::id()))
}

fn run_child(threads: usize, perf_dir: Option<&Path>) {
    let mut command = Command::new(env::current_exe().expect("locate this test binary"));
    command
        .arg("--exact")
        .arg(CHILD_TEST)
        .arg("--nocapture")
        .env("PLINTH_THREADS", threads.to_string())
        .env("RAYON_NUM_THREADS", threads.to_string())
        .env(CHILD_MODE, "1")
        .env_remove("PLINTH_PERF_DIR");
    if let Some(dir) = perf_dir {
        command.env("PLINTH_PERF_DIR", dir);
    }
    let status = command.status().expect("launch the child");
    assert!(status.success(), "child failed at {threads} threads: {status}");
}

/// The text of `"<key>":{...}` inside `text`, braces included.
fn object_of<'a>(text: &'a str, key: &str) -> &'a str {
    let needle = format!("\"{key}\":{{");
    let start = text.find(&needle).unwrap_or_else(|| panic!("no {key} in {text}"));
    let from = start + needle.len() - 1;
    let end = text[from..].find('}').expect("closing brace") + from + 1;
    &text[from..end]
}

fn value_of(object: &str, label: &str) -> u64 {
    let needle = format!("\"{label}\":");
    let at = object.find(&needle).unwrap_or_else(|| panic!("no {label} in {object}"));
    object[at + needle.len()..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .expect("a count")
}

#[test]
fn work_counters_are_equal_at_1_and_8_threads_and_fire() {
    let root = unique_directory("on");
    let mut works = Vec::new();
    // The reuse rows hold no time column, so they are equal at every width too.
    let mut reuse_rows = Vec::new();
    for threads in [1_usize, 8] {
        let dir = root.join(format!("t{threads}"));
        run_child(threads, Some(&dir));
        let text = fs::read_to_string(dir.join(SIDECAR)).expect("final sidecar written");
        assert!(text.contains("\"partial\":false"), "{text}");
        assert!(text.contains("\"schema\":\"plinth-perf/1\""), "{text}");
        let totals = &text[text.find("\"totals\"").expect("totals")..];
        works.push(object_of(totals, "work").to_owned());
        reuse_rows.push(object_of(&text, "down.reuse").to_owned());
        // The pose frame of the sweep names its sizing sites and pass spans.
        for label in ["sizing.initial", "down.run", "down.pass", "down.per_event"] {
            assert!(text.contains(&format!("\"{label}\":{{\"calls\":")), "no {label} span: {text}");
        }
        assert!(text.contains("\"label\":\"unlabelled\""), "{text}");
        assert!(text.contains("\"brace.partner_search\":{\"cols\":"), "no partner rows: {text}");
    }
    assert_eq!(works[0], works[1], "work counters moved with the thread count");
    assert_eq!(reuse_rows[0], reuse_rows[1], "reuse rows moved with the thread count");

    // Anti-vacuity: the instrumented paths ran on this fixture.
    let work = &works[0];
    for label in [
        "sweep.runs",
        "sweep.down.calls.initial",
        "sweep.down.passes",
        "sweep.down.cell_layers",
        "frame.solves.committed",
        "frame.points.committed",
        "brace.partner_calls",
        "brace.partner_scanned",
        "brace.partner_searches",
        "brace.partner_trials",
        "reuse.runs",
        "reuse.layers",
        "clear.batches",
        "clear.batch_rows",
        "shake.checks",
    ] {
        assert!(value_of(work, label) > 0, "{label} stayed zero: {work}");
    }
    // Every down-sweep call is measured by the probe: one run per site charge.
    let calls: u64 = [
        "initial", "removal", "merges_undone", "congestion", "congestion_trial", "stranded_round",
        "stranded_put_back", "forced_merges", "unmerge_after_merges", "graze", "head_join",
        "lip_seat", "ties", "model_ties",
    ]
    .iter()
    .map(|site| {
        let needle = format!("\"sweep.down.calls.{site}\":");
        work.find(&needle).map_or(0, |_| value_of(work, &format!("sweep.down.calls.{site}")))
    })
    .sum();
    assert_eq!(value_of(work, "reuse.runs"), calls, "{work}");
    assert!(
        value_of(work, "clear.batch_tris_max_row") <= value_of(work, "clear.batch_tris"),
        "a row cannot test more faces than its batch: {work}"
    );
    assert!(
        value_of(work, "sweep.down.cell_layers") >= value_of(work, "sweep.down.passes"),
        "cell-layers are at least one per pass"
    );
    let _ = fs::remove_dir_all(&root);
}

/// With no `PLINTH_PERF_DIR` the child runs the same sweep with counting off
/// (it asserts that itself) and the run still succeeds.
#[test]
fn counting_off_runs_the_same_sweep() {
    run_child(1, None);
}
