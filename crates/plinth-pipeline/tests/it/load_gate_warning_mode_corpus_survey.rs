//! **Verifies, first-hand, how many area-law violations WARNING MODE actually
//! finds on the full corpus** -- the number the brief asked this lane to
//! confirm rather than repeat from an earlier report.
//!
//! This file does not pin a single golden number for the whole corpus,
//! because the earlier report itself did not describe one measurement: every
//! corpus model has its own emitted-contact record, its own judged count and
//! its own violation count, and they are not the same fixture as the
//! procedural pins in `load_gate_area_law.rs` -- those run the IN-REPO
//! procedural generator (`plinth_mesh::make_toed_slab()`, ...), this file
//! runs the CHECKED-IN corpus STL through the CHECKED-IN pinned baseline
//! orientation (`corpus/baseline/*.pipeline.json`), which is a different
//! candidate set and therefore a different plan for the same model NAME.
//!
//! Each model is run behind [`PER_MODEL_BUDGET`], because
//! `crates/plinth-support/tests/support_runner_integration.rs` and the
//! ratchet harness both document the same trap independently: at least one
//! corpus model's coverage-repair search is, in practice, unbounded, and a
//! caller that waits for it loses its whole run. Ruling 2 (2026-08-26,
//! "measure the models that FINISH, named") governs this file's report: a
//! model that does not finish inside the budget is named as excluded, never
//! silently dropped from an average or a total.

use crate::chain;

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Generous relative to the ratchet harness's 120 s: this file's only job is
/// to run the SEALED chain once per model (no repeated ratchet comparison
/// arithmetic on top), so a slightly longer allowance still finishes this
/// test in well under the corpus's own worst case (Weeping Treant's
/// documented multi-hour repair search, which this budget is not trying to
/// outlast -- it is trying to name it as excluded quickly).
const PER_MODEL_BUDGET: Duration = Duration::from_secs(150);

/// The eight corpus STL names, in `corpus/models` order.
const CORPUS_MODELS: [&str; 8] = [
    "Sphere",
    "Cylinder",
    "make_box",
    "make_overhang_table",
    "make_toed_slab",
    "make_raking_blade",
    "make_thin_fin",
    "Weeping Treant v1.2",
];

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "plinth-load-gate-survey-{label}-{}.chitubox",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(self.0.with_extension("coverage.repair.json"));
    }
}

enum Measured {
    /// The gate ran and found nothing to report.
    Clean { judged: usize },
    /// WARNING MODE's evidence: the model still emitted.
    Warned { judged: usize, violations: usize },
    /// The chain stopped before or instead of a `Load` verdict (a different
    /// gate, a coverage-closure refusal, a hard-mode Load refusal such as
    /// `LoadRecordMismatch` or a record the run could not describe, or a setup
    /// failure). Named, not swallowed.
    Stopped(String),
}

/// Run one corpus model to a written artifact on its own thread, so a model
/// whose coverage-repair search is effectively unbounded cannot cost this
/// test its own result for every other model.
fn measure(name: &'static str) -> (String, Option<Measured>) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let scratch = Scratch::new(name);
        let outcome = chain::run_corpus_model(name, &scratch.0);
        let measured = match outcome {
            Ok(chain::ChainOutcome {
                load_warnings,
                routed_contacts,
                ..
            }) => match load_warnings {
                // `judged` is not carried on the outcome directly when the
                // gate cleared, because `LoadWarnings` is not constructed for
                // a clean verdict -- see `GateVerdict::Warned`'s own doc.
                // `routed_contacts` is the sealed snapshot's own independent
                // count of what the run routed, which `judge_emitted_load_law`
                // requires the emitted-contact record to identify exactly,
                // once each (`GateRefusal::LoadRecordMismatch` otherwise) --
                // so on a clean verdict it is exactly what was judged.
                None => Measured::Clean {
                    judged: routed_contacts,
                },
                Some(warnings) => Measured::Warned {
                    judged: warnings.judged,
                    violations: warnings.violations.len(),
                },
            },
            Err(stop) => Measured::Stopped(stop.to_string()),
        };
        let _ = tx.send(measured);
    });
    match rx.recv_timeout(PER_MODEL_BUDGET) {
        Ok(measured) => {
            let _ = handle.join();
            (name.to_owned(), Some(measured))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => (name.to_owned(), None),
        Err(mpsc::RecvTimeoutError::Disconnected) => (
            name.to_owned(),
            Some(Measured::Stopped(
                "measurement thread ended without a result".to_owned(),
            )),
        ),
    }
}

/// **Run with:**
/// `cargo test -p plinth-pipeline --profile gate --offline --test \
///  load_gate_warning_mode_corpus_survey -- --ignored --nocapture`
///
/// Prints one line per corpus model naming its judged/violation counts, or
/// naming why it was excluded -- never a total that quietly drops an
/// unmeasured model.
#[test]
#[ignore = "runs the full sealed chain on all 8 corpus models; slow, and at \
            least one is documented to never finish under a plain cargo test"]
fn survey_load_gate_violations_across_the_named_corpus() {
    for name in CORPUS_MODELS {
        let (name, measured) = measure(name);
        match measured {
            None => println!(
                "SURVEY\t{name}\tEXCLUDED\tdid not finish within {:?}",
                PER_MODEL_BUDGET
            ),
            Some(Measured::Clean { judged }) => {
                println!("SURVEY\t{name}\tCLEAN\tjudged={judged}\tviolations=0");
            }
            Some(Measured::Warned { judged, violations }) => {
                println!("SURVEY\t{name}\tWARNED\tjudged={judged}\tviolations={violations}");
            }
            Some(Measured::Stopped(reason)) => {
                println!("SURVEY\t{name}\tEXCLUDED\t{reason}");
            }
        }
    }
}
