//! Perf lane DD: decimation's adaptive diagnostics run beside orientation, and
//! two things must hold whatever the scheduling.
//!
//! 1. **Nothing orientation produces appears before the diagnostics have
//!    joined.** A run whose diagnostics fail stops where a synchronous run
//!    stopped: no `<model>.orient.json`, no "entering Orientation" stage line.
//!    A run whose diagnostics succeed publishes both, in the synchronous order.
//! 2. **The diagnostics' perf stages are charged to the Decimate phase**, never
//!    to the phase the main thread has moved on to, and the orientation timer
//!    does not include the wait for the join.
//!
//! This is its own test program (not a module of `tests/it`) because it
//! switches the process-wide perf counters on, which would otherwise slow and
//! perturb every other pipeline test, and because the perf phase is one global.
//! The tests share it, so they run one at a time.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use plinth_pipeline::{
    prepare, registered_decimation_budget, registered_support_request, ModelPreparationOutcome,
    OrientationPolicy, OutputProfile, PrepareReport, PrepareRequest, StageStop,
};
use plinth_support::coverage_repair::capture::RunLogCapture;

static PERF: Mutex<()> = Mutex::new(());

const STEM: &str = "pawn";

struct Scratch {
    root: PathBuf,
    _serial: MutexGuard<'static, ()>,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let serial = PERF.lock().unwrap_or_else(PoisonError::into_inner);
        let root = std::env::temp_dir().join(format!(
            "plinth-deferred-diag-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("in")).expect("scratch input folder");
        Self {
            root,
            _serial: serial,
        }
    }

    fn perf_dir(&self) -> PathBuf {
        self.root.join("perf")
    }

    /// The dual-cavity pawn reaches the adaptive diagnostics at the registered
    /// 19 um budget (see `plinth-decimate`'s deferred tests), so its decimation
    /// is deferred.
    fn place_pawn(&self) {
        let pawn = plinth_mesh::make_dual_cavity_pawn().expect("procedural pawn");
        self.write_snapped(&pawn);
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "the fixture's coordinates are snapped to f32 because a binary STL stores f32; \
              see docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn write_snapped(&self, pawn: &plinth_mesh::Mesh) {
        let verts: Vec<[f64; 3]> = pawn
            .verts()
            .iter()
            .map(|v| {
                [
                    f64::from(v[0] as f32),
                    f64::from(v[1] as f32),
                    f64::from(v[2] as f32),
                ]
            })
            .collect();
        let snapped = plinth_mesh::Mesh::new(verts, pawn.tris().to_vec()).expect("snapped pawn");
        plinth_mesh::write_binary_stl(self.root.join("in").join(format!("{STEM}.stl")), &snapped)
            .expect("write pawn");
    }

    /// `dense`: the real orientation search (about two minutes on the pawn);
    /// otherwise a pinned baseline, which orients in milliseconds and so leaves
    /// the diagnostics' tail to the join.
    fn request(&self, dense: bool) -> PrepareRequest {
        let printer = OutputProfile::Saturn4Ultra
            .resolve()
            .expect("M0 Saturn profile")
            .clone();
        let orientation = if dense {
            OrientationPolicy::DenseSweep
        } else {
            let report = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../corpus/baseline/make_toed_slab.pipeline.json");
            OrientationPolicy::Pinned(
                [(
                    STEM.to_owned(),
                    plinth_orient::PinnedBaseline {
                        pipeline_report: report,
                    },
                )]
                .into_iter()
                .collect(),
            )
        };
        PrepareRequest {
            input_models_dir: self.root.join("in"),
            output_dir: self.root.join("out"),
            orientation,
            no_orient: Default::default(),
            placement: plinth_pipeline::Placement::Legacy,
            output_profile: OutputProfile::Saturn4Ultra,
            decimation_budget: registered_decimation_budget().expect("registered budget"),
            hollow: plinth_hollow::HollowRequest::with_registered_wall_default(printer.clone())
                .expect("registered wall"),
            support: registered_support_request(&printer).expect("registered lift"),
            no_cache: false,
            stop_after: Some(StageStop::Orient),
        }
    }

    /// One pawn through decimation and orientation with `--perf` on; the run
    /// lines this thread printed come back with the report.
    fn run(&self, dense: bool) -> (PrepareReport, Vec<String>) {
        self.place_pawn();
        plinth_sweep::perf::enable(self.perf_dir(), Duration::from_secs(3600));
        let capture = RunLogCapture::install();
        let report = prepare(self.request(dense)).expect("the folder is enumerable");
        (report, capture.lines())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn sidecar(dir: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(dir.join(format!("{STEM}.perf.json")))
        .expect("the perf sidecar is written for every model");
    serde_json::from_str(&text).expect("sidecar json")
}

fn stages(sidecar: &serde_json::Value) -> Vec<(String, String, f64)> {
    sidecar["stages"]
        .as_array()
        .expect("stages array")
        .iter()
        .map(|stage| {
            (
                stage["phase"].as_str().expect("phase").to_owned(),
                stage["label"].as_str().expect("label").to_owned(),
                stage["thread_ms"].as_f64().expect("thread_ms"),
            )
        })
        .collect()
}

fn decimation_budget() -> plinth_decimate::DecimationBudget {
    registered_decimation_budget().expect("registered budget")
}

/// A failed diagnostic leaves no orientation output: the orientation ran (it
/// overlapped the diagnostics) but nothing it would have published exists.
fn failed_diagnostic_leaves_no_orientation_output(dense: bool) {
    let scratch = Scratch::new(if dense { "failed-dense" } else { "failed" });
    let _fault = plinth_decimate::injection::inject(decimation_budget(), true, Duration::ZERO);
    let (report, lines) = scratch.run(dense);
    let outcome = &report.models[0].outcome;
    let ModelPreparationOutcome::Failed { detail, .. } = outcome else {
        panic!("the injected diagnostics failure must fail the model, got {outcome:?}");
    };
    assert!(
        detail.contains("injected adaptive fault"),
        "the synchronous run's error must be the one reported: {detail}"
    );
    assert!(
        !scratch
            .perf_dir()
            .join(format!("{STEM}.orient.json"))
            .exists(),
        "orient.json was published before the diagnostics joined"
    );
    assert!(
        lines.iter().any(|line| line.contains("entering Decimation")),
        "{lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("entering Orientation")),
        "the Orientation stage line was printed before the diagnostics joined: {lines:?}"
    );
}

/// The fast form: a pinned orientation, so the stage line is what is checked.
#[test]
fn a_failed_diagnostic_leaves_no_orientation_stage_line() {
    failed_diagnostic_leaves_no_orientation_output(false);
}

/// The real form: the dense orientation search runs to the end beside the
/// failing diagnostic and must still publish no `orient.json`.
#[test]
#[ignore = "about two minutes: a full dense orientation search of the pawn"]
fn a_failed_diagnostic_leaves_no_orient_json_after_a_dense_search() {
    failed_diagnostic_leaves_no_orientation_output(true);
}

/// A successful run still prints the Orientation line, after the Decimation
/// line (the line is held until the join, not dropped).
#[test]
fn a_successful_run_prints_the_orientation_line_after_the_decimation_line() {
    let scratch = Scratch::new("ok");
    let (report, lines) = scratch.run(false);
    assert!(
        matches!(
            report.models[0].outcome,
            ModelPreparationOutcome::Stopped {
                after: StageStop::Orient
            }
        ),
        "{:?}",
        report.models[0].outcome
    );
    let position = |needle: &str| {
        lines
            .iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("no `{needle}` line in {lines:?}"))
    };
    assert!(position("entering Decimation") < position("entering Orientation"));
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.contains("entering Orientation"))
            .count(),
        1
    );
}

/// The job starts after the main thread has left the Decimate phase (it sleeps
/// first), yet every `decimate.*` stage is in Decimate; and the orientation
/// timer holds orientation's compute, not the wait for the join.
#[test]
fn the_diagnostics_stages_are_decimate_whatever_the_scheduling_and_the_join_is_not_orientation_time() {
    // The pinned orientation takes milliseconds, so the wait is nearly all of
    // the delay; the bound sits far from both.
    const START_DELAY: Duration = Duration::from_millis(2000);
    let scratch = Scratch::new("phase");
    let _fault = plinth_decimate::injection::inject(decimation_budget(), false, START_DELAY);
    let (report, _lines) = scratch.run(false);
    assert!(
        matches!(
            report.models[0].outcome,
            ModelPreparationOutcome::Stopped {
                after: StageStop::Orient
            }
        ),
        "{:?}",
        report.models[0].outcome
    );
    let stages = stages(&sidecar(&scratch.perf_dir()));
    let decimate: Vec<_> = stages
        .iter()
        .filter(|(_, label, _)| label.starts_with("decimate."))
        .collect();
    assert!(
        decimate.iter().any(|(_, label, _)| label == "decimate.adaptive"),
        "the pawn must reach the adaptive stage: {stages:?}"
    );
    for (phase, label, _) in &decimate {
        assert_eq!(phase, "Decimate", "{label} was charged to {phase}: {stages:?}");
    }
    let orient_ms = stages
        .iter()
        .find(|(phase, label, _)| phase == "OrientTable" && label == "orient.orient_model")
        .map(|(_, _, ms)| *ms)
        .unwrap_or_else(|| panic!("orient.orient_model missing from OrientTable: {stages:?}"));
    assert!(
        orient_ms < 1000.0,
        "orient.orient_model ({orient_ms} ms) includes the wait for a job that slept {} ms",
        START_DELAY.as_millis()
    );
}

impl Scratch {
    /// A small recessed disc: the dense orientation search over it takes
    /// seconds, and it is too simple to reach the adaptive diagnostics, so its
    /// decimation is not deferred.
    fn place_disc(&self) {
        let disc = plinth_mesh::make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128)
            .expect("procedural disc");
        self.write_snapped(&disc);
    }

    /// One dense run into `perf_dir`; the run lines come back with the report.
    fn run_dense_into(&self, perf_dir: PathBuf) -> (PrepareReport, Vec<String>) {
        plinth_sweep::perf::enable(perf_dir, Duration::from_secs(3600));
        let capture = RunLogCapture::install();
        let report = prepare(self.request(true)).expect("the folder is enumerable");
        (report, capture.lines())
    }
}

fn orient_json(dir: &Path) -> serde_json::Value {
    let bytes = std::fs::read(dir.join(format!("{STEM}.orient.json")))
        .expect("orient.json was written");
    serde_json::from_slice(&bytes).expect("orient.json is json")
}

/// An orientation that fails AFTER its search (the transformed mesh is not
/// finite) must still leave the `orient.json` the search wrote, byte-for-byte
/// what the synchronous run wrote: a failure never destroys diagnostic
/// evidence. The model still fails with the orientation error, and the
/// Orientation stage line is still printed.
fn orientation_failure_after_search_keeps_orient_json(label: &str, place: fn(&Scratch)) {
    let scratch = Scratch::new(label);
    place(&scratch);

    let reference_dir = scratch.root.join("perf-reference");
    let (reference, _) = scratch.run_dense_into(reference_dir.clone());
    assert!(
        matches!(
            reference.models[0].outcome,
            ModelPreparationOutcome::Stopped {
                after: StageStop::Orient
            }
        ),
        "the reference run must orient cleanly: {:?}",
        reference.models[0].outcome
    );

    let failed_dir = scratch.root.join("perf-failed");
    let _fault = plinth_orient::injection::fail_after_search();
    let (failed, lines) = scratch.run_dense_into(failed_dir.clone());
    let ModelPreparationOutcome::Failed { detail, .. } = &failed.models[0].outcome else {
        panic!(
            "the injected orientation failure must fail the model, got {:?}",
            failed.models[0].outcome
        );
    };
    assert!(
        detail.contains("invalid production mesh"),
        "the orientation error must be the one reported: {detail}"
    );
    assert_eq!(
        orient_json(&failed_dir),
        orient_json(&reference_dir),
        "orient.json after a post-search failure must equal the synchronous run's"
    );
    assert!(
        lines.iter().any(|line| line.contains("entering Orientation")),
        "the synchronous run printed the Orientation line before the failure: {lines:?}"
    );
}

/// The fast form: no deferred diagnostics, so the pending output is published
/// at the same point a synchronous run published it.
#[test]
fn an_orientation_failure_after_its_search_keeps_orient_json() {
    orientation_failure_after_search_keeps_orient_json("orient-fail", Scratch::place_disc);
}

/// The deferred form: the pawn's diagnostics run beside the search, succeed,
/// and only then is the failed orientation's `orient.json` published.
#[test]
#[ignore = "about four minutes: two full dense orientation searches of the pawn"]
fn an_orientation_failure_after_a_dense_search_keeps_orient_json_past_the_join() {
    orientation_failure_after_search_keeps_orient_json("orient-fail-pawn", Scratch::place_pawn);
}
