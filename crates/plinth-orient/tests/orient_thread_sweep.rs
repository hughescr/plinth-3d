//! Process-isolated determinism sweep for the batched orientation scorer.
//!
//! Scoring is index-parallel map-collect across candidates under the one
//! workspace thread control, so a score must be bit-identical at any thread
//! count.  A Rust test binary runs its tests concurrently, so mutating
//! `PLINTH_THREADS` in-process would make the assertion both racy and vacuous;
//! the parent below launches this same binary in three fresh processes instead,
//! exactly as `plinth-decimate`'s sweep does.
//!
//! The child writes two things: every term of every candidate as raw f64 bits
//! (the scoring substrate), and the **`search_digest` of a whole dense sweep**
//! (the search tier). The second is the stronger claim, because the digest
//! covers the grid, the LOD ladder, every tier's full ranking, the fidelity
//! ledger and the work counters — so a thread count that moved *any* of them,
//! including which candidate was committed to, changes one string.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use plinth_mesh::{make_overhang_table, make_thin_fin, make_toed_slab};
use plinth_orient::search::{search, SweepRequest};
use plinth_orient::{
    score_candidates, CandidateOutcome, ObjectiveWeights, OrientFaceTable, OrientSearchLimits,
    UpDirection,
};

const CHILD_MODE: &str = "PLINTH_ORIENT_THREAD_SWEEP_CHILD";
const CHILD_OUTPUT: &str = "PLINTH_ORIENT_THREAD_SWEEP_OUTPUT";
const CHILD_TEST: &str = "thread_sweep_child_writes_canonical_scores";

#[test]
fn batched_candidate_scoring_is_bit_identical_at_1_4_and_16_threads() {
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

    assert!(
        !reports[0].is_empty(),
        "the sweep must compare a nonempty report"
    );
    for (threads, report) in [4_usize, 16].into_iter().zip(&reports[1..]) {
        assert_eq!(
            &reports[0], report,
            "ThreadSweepScoreMismatch: threads=1 versus threads={threads}"
        );
    }
}

#[test]
fn thread_sweep_child_writes_canonical_scores() {
    let Ok(output) = env::var(CHILD_OUTPUT) else {
        // Ordinary run: this test exists only for the parent to launch.
        assert!(env::var(CHILD_MODE).is_err());
        return;
    };
    let weights = ObjectiveWeights::registered().expect("registered objective");
    let candidates = fibonacci_directions(97);
    let mut report = String::new();
    for (name, mesh) in [
        ("thin_fin", make_thin_fin().expect("thin fin fixture")),
        (
            "overhang_table",
            make_overhang_table().expect("overhang table fixture"),
        ),
        ("toed_slab", make_toed_slab().expect("toed slab fixture")),
    ] {
        let table = OrientFaceTable::build(&mesh, "sweep-lod").expect("scoring table");
        let limits = OrientSearchLimits::production();
        let scored = score_candidates(&table, &candidates, weights, limits)
            .expect("scoring must not refuse");
        report.push_str(&format!(
            "{name} {} {}\n",
            table
                .delicacy_digest()
                .expect("a scar-weighted table has a field"),
            scored.meter().section_breakpoints
        ));
        for (index, outcome) in scored.outcomes().iter().enumerate() {
            match outcome {
                CandidateOutcome::Scored(terms) => report.push_str(&format!(
                    "{index} {:016x} {:016x} {:016x} {:016x} {:016x} {}\n",
                    terms.support_area_cm2.to_bits(),
                    terms.peak_xsec_cm2.to_bits(),
                    terms.height_mm.to_bits(),
                    terms.scar_term.to_bits(),
                    terms.score.to_bits(),
                    terms.down_facing_faces,
                )),
                CandidateOutcome::Refused(refusal) => {
                    report.push_str(&format!("{index} refused {refusal:?}\n"));
                }
            }
        }
    }
    // The search tier, end to end. One string per fixture that is a function of
    // everything the sweep decided.
    let request = SweepRequest::registered().expect("the registered sweep configuration");
    for (name, mesh) in [
        ("thin_fin", make_thin_fin().expect("thin fin fixture")),
        (
            "overhang_table",
            make_overhang_table().expect("overhang table fixture"),
        ),
        ("toed_slab", make_toed_slab().expect("toed slab fixture")),
    ] {
        let outcome = search(&mesh, &request, None).expect("the dense sweep must not refuse a fixture");
        let record = outcome.record();
        report.push_str(&format!(
            "sweep {name} {} winner={} score={:016x}\n",
            record.search_digest,
            record.winner.candidate,
            record.winner.terms.score.get().to_bits(),
        ));
    }

    fs::write(PathBuf::from(output), report).expect("write the child's canonical score report");
}

/// The recorded Fibonacci-sphere generator, at a count small enough to keep the
/// sweep cheap. The grid itself is the search tier's business; here it is only a
/// deterministic spread of directions to score.
fn fibonacci_directions(count: usize) -> Vec<UpDirection> {
    let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    (0..count)
        .map(|index| {
            let z = 1.0 - (2.0 * index as f64 + 1.0) / count as f64;
            let radius = (1.0 - z * z).max(0.0).sqrt();
            let angle = index as f64 * golden;
            UpDirection::new([radius * angle.cos(), radius * angle.sin(), z])
                .expect("a Fibonacci direction is a unit vector")
        })
        .collect()
}

fn unique_output_directory() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the unix epoch")
        .as_nanos();
    env::temp_dir().join(format!(
        "plinth-orient-thread-sweep-{nanos}-{}",
        std::process::id()
    ))
}
