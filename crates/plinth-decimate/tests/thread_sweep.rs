//! Process-isolated M3 determinism sweep for the sole workspace thread control.
//!
//! A Rust test binary executes tests concurrently, so mutating `PLINTH_THREADS`
//! in-process would make this assertion vacuous and racy. The parent test below
//! launches this same test binary in three fresh processes instead.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use geom_kernel::{current_threads, NativeSurfaceConfig};
use plinth_decimate::{run, BudgetSearchOutcome, DecimationBudget, ProbeConfig};
use plinth_mesh::Mesh;
use sha2::{Digest, Sha256};

const CHILD_MODE: &str = "PLINTH_DECIMATE_THREAD_SWEEP_CHILD";
const CHILD_OUTPUT: &str = "PLINTH_DECIMATE_THREAD_SWEEP_OUTPUT";
const CHILD_TEST: &str = "thread_sweep_child_writes_canonical_production_report";

#[test]
fn protected_budget_search_is_bit_identical_at_1_4_and_16_threads() {
    let directory = unique_output_directory();
    fs::create_dir_all(&directory).expect("create isolated thread-sweep output directory");

    let mut reports = Vec::new();
    for threads in [1_usize, 4, 16] {
        let output = directory.join(format!("threads-{threads}.report"));
        let status = Command::new(env::current_exe().expect("locate this integration test binary"))
            .arg("--exact")
            .arg(CHILD_TEST)
            .arg("--nocapture")
            .env("PLINTH_THREADS", threads.to_string())
            .env(CHILD_MODE, "1")
            .env(CHILD_OUTPUT, &output)
            .status()
            .unwrap_or_else(|error| {
                panic!("ThreadSweepChildLaunchFailed: threads={threads}: {error}")
            });
        assert!(
            status.success(),
            "ThreadSweepChildFailed: threads={threads}, status={status}"
        );
        reports.push(fs::read(&output).unwrap_or_else(|error| {
            panic!(
                "ThreadSweepChildMissingOutput: threads={threads}, path={}: {error}",
                output.display()
            )
        }));
    }

    for (threads, report) in [4_usize, 16].into_iter().zip(&reports[1..]) {
        assert_eq!(
            &reports[0], report,
            "ThreadSweepReportMismatch: threads=1 versus threads={threads}"
        );
    }
}

#[test]
fn thread_sweep_child_writes_canonical_production_report() {
    if env::var_os(CHILD_MODE).as_deref() != Some("1".as_ref()) {
        return;
    }
    let output = env::var_os(CHILD_OUTPUT)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            panic!("ThreadSweepChildOutputMissing: {CHILD_OUTPUT} was not supplied")
        });
    let expected_threads = env::var("PLINTH_THREADS")
        .expect("ThreadSweepChildThreadsMissing: PLINTH_THREADS was not supplied")
        .parse::<usize>()
        .expect("ThreadSweepChildThreadsInvalid: PLINTH_THREADS must be a positive integer");
    assert_eq!(
        current_threads(),
        expected_threads,
        "ThreadSweepChildEffectiveCountMismatch"
    );

    let report = canonical_production_report();
    fs::write(&output, report).unwrap_or_else(|error| {
        panic!(
            "ThreadSweepChildOutputWriteFailed: path={}: {error}",
            output.display()
        )
    });
}

fn canonical_production_report() -> String {
    // This printable-scale smooth mesh forces the public production entry
    // point through feature protection, QEM/budget search, and final
    // verification without immediately exhausting the 12% protection cap.
    let mesh = smooth_print_scale_sphere();
    // An explicit finite construction cap, not a default: `usize::MAX` is now
    // refused by `DecimationBudget::validate_for_execution` as no budget at
    // all.  Only `grid_pitch_mm` is fingerprinted into the native surface, so
    // this cap cannot move the report this sweep compares across thread
    // counts; it can only refuse construction outright.
    let budget = DecimationBudget::ratified_default(ProbeConfig {
        surface: NativeSurfaceConfig {
            grid_pitch_mm: 1.0,
            max_grid_bytes: 64 * 1024 * 1024,
        },
        nearest_upper_bound_mm: 100.0,
    })
    .expect("construct ratified M3 budget");
    let outcome = run(&mesh, budget).expect("protected M3 production run must succeed");
    let (status, success) = match outcome {
        BudgetSearchOutcome::Verified(success) => ("verified", success),
        BudgetSearchOutcome::AlreadyPrintScale(success) => ("already_print_scale", success),
        BudgetSearchOutcome::SkippedScaleSuspicious { .. } => {
            panic!("ThreadSweepFixtureUnexpectedScaleSkip")
        }
    };

    let mut report = String::new();
    report.push_str("plinth/m3-thread-sweep-report/v1\n");
    report.push_str("status=");
    report.push_str(status);
    report.push('\n');
    push_usize(&mut report, "input_faces", mesh.tris().len());
    push_usize(&mut report, "output_faces", success.mesh().tris().len());
    push_f64(&mut report, "chosen_ratio", success.chosen_ratio());
    let trace = success.trace();
    push_usize(
        &mut report,
        "protected_vertex_count",
        trace.protected_vertex_count,
    );
    push_f64(
        &mut report,
        "scale_longest_axis",
        trace.scale.longest_axis_mm,
    );
    push_f64(&mut report, "budget_p99", trace.budget.p99_mm);
    push_f64(&mut report, "budget_p999", trace.budget.p99_9_mm);
    for (index, step) in trace.steps.iter().enumerate() {
        push_f64(&mut report, &format!("step.{index}.ratio"), step.ratio);
        push_usize(
            &mut report,
            &format!("step.{index}.faces"),
            step.output_face_count,
        );
        push_usize(
            &mut report,
            &format!("step.{index}.vertices"),
            step.output_vertex_count,
        );
        push_probe(&mut report, &format!("step.{index}.probe"), &step.probe);
        report.push_str(&format!("step.{index}.passed={}\n", step.passed));
    }
    push_probe(&mut report, "final_probe", success.final_probe());
    let before = success.topology_before();
    let after = success.topology_after();
    push_usize(
        &mut report,
        "topology_before.boundary",
        before.boundary_edges,
    );
    push_usize(
        &mut report,
        "topology_before.nonmanifold",
        before.nonmanifold_edges,
    );
    push_usize(&mut report, "topology_after.boundary", after.boundary_edges);
    push_usize(
        &mut report,
        "topology_after.nonmanifold",
        after.nonmanifold_edges,
    );
    push_mesh(&mut report, success.mesh());
    let digest = Sha256::digest(report.as_bytes());
    report.push_str(&format!("sha256={digest:x}\n"));
    report
}

fn push_probe(report: &mut String, prefix: &str, probe: &plinth_decimate::TwoSidedProbe) {
    push_usize(
        report,
        &format!("{prefix}.candidate_faces"),
        probe.candidate_face_count,
    );
    push_f64(report, &format!("{prefix}.p99"), probe.p99_mm);
    push_f64(report, &format!("{prefix}.p999"), probe.p99_9_mm);
    push_f64(
        report,
        &format!("{prefix}.max"),
        probe.max_mm.unwrap_or(f64::NAN),
    );
    push_u64(
        report,
        &format!("{prefix}.distance_digest"),
        probe.distance_digest,
    );
    push_selection(report, &format!("{prefix}.forward"), &probe.forward);
    push_selection(report, &format!("{prefix}.backward"), &probe.backward);
}

fn push_selection(
    report: &mut String,
    prefix: &str,
    selection: &plinth_decimate::SelectionEvidence,
) {
    push_usize(
        report,
        &format!("{prefix}.source_count"),
        selection.source_count,
    );
    push_usize(
        report,
        &format!("{prefix}.selected_count"),
        selection.selected_count,
    );
    report.push_str(&format!("{prefix}.seed={:?}\n", selection.seed));
    push_u64(
        report,
        &format!("{prefix}.selected_index_digest"),
        selection.selected_index_digest,
    );
}

fn push_mesh(report: &mut String, mesh: &Mesh) {
    for (index, vertex) in mesh.verts().iter().enumerate() {
        report.push_str(&format!(
            "mesh.vertex.{index}={:016x},{:016x},{:016x}\n",
            vertex[0].to_bits(),
            vertex[1].to_bits(),
            vertex[2].to_bits()
        ));
    }
    for (index, triangle) in mesh.tris().iter().enumerate() {
        report.push_str(&format!(
            "mesh.triangle.{index}={},{},{}\n",
            triangle[0], triangle[1], triangle[2]
        ));
    }
}

fn push_usize(report: &mut String, key: &str, value: usize) {
    report.push_str(&format!("{key}={value}\n"));
}

fn push_u64(report: &mut String, key: &str, value: u64) {
    report.push_str(&format!("{key}={value:016x}\n"));
}

fn push_f64(report: &mut String, key: &str, value: f64) {
    report.push_str(&format!("{key}={:016x}\n", value.to_bits()));
}

fn unique_output_directory() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after Unix epoch")
        .as_nanos();
    env::temp_dir().join(format!(
        "plinth-decimate-thread-sweep-{}-{nanos}",
        std::process::id()
    ))
}

fn smooth_print_scale_sphere() -> Mesh {
    const LONGITUDE: usize = 256;
    const LATITUDE: usize = 128;
    const RADIUS_MM: f64 = 10.0;
    let mut vertices = Vec::with_capacity(2 + (LATITUDE - 1) * LONGITUDE);
    vertices.push([0.0, 0.0, RADIUS_MM]);
    for latitude in 1..LATITUDE {
        let polar = std::f64::consts::PI * latitude as f64 / LATITUDE as f64;
        for longitude in 0..LONGITUDE {
            let azimuth = 2.0 * std::f64::consts::PI * longitude as f64 / LONGITUDE as f64;
            vertices.push([
                RADIUS_MM * polar.sin() * azimuth.cos(),
                RADIUS_MM * polar.sin() * azimuth.sin(),
                RADIUS_MM * polar.cos(),
            ]);
        }
    }
    let south = vertices.len() as u32;
    vertices.push([0.0, 0.0, -RADIUS_MM]);

    let ring = |latitude: usize, longitude: usize| -> u32 {
        1 + ((latitude - 1) * LONGITUDE + longitude % LONGITUDE) as u32
    };
    let mut triangles = Vec::with_capacity(2 * LONGITUDE * (LATITUDE - 1));
    for longitude in 0..LONGITUDE {
        triangles.push([0, ring(1, longitude + 1), ring(1, longitude)]);
    }
    for latitude in 1..(LATITUDE - 1) {
        for longitude in 0..LONGITUDE {
            let next = longitude + 1;
            triangles.push([
                ring(latitude, longitude),
                ring(latitude, next),
                ring(latitude + 1, next),
            ]);
            triangles.push([
                ring(latitude, longitude),
                ring(latitude + 1, next),
                ring(latitude + 1, longitude),
            ]);
        }
    }
    for longitude in 0..LONGITUDE {
        triangles.push([
            ring(LATITUDE - 1, longitude),
            ring(LATITUDE - 1, longitude + 1),
            south,
        ]);
    }
    Mesh::new(vertices, triangles).expect("construct nontrivial protected decimation fixture")
}
