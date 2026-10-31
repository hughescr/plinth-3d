//! The JSON run report, written by hand (no serde). Keys keep a fixed order.
//!
//! Two parts, so a run can be compared without its timings: `models` and `job` hold the
//! results and are the same at any thread count; `run` holds the settings, time per step and
//! peak memory, which differ from run to run (design section 8: timings never mix with the
//! measured part).

use crate::check::Checked;
use crate::json::Json;
use crate::mem::Peak;
use crate::run::{Config, ModelRun, Outcome, Run};
use plinth_emit::TRANSPORT_LABEL;

/// Report format version.
pub const REPORT_VERSION: u64 = 1;

fn path(p: &std::path::Path) -> Json {
    Json::str(p.display().to_string())
}

fn count(n: usize) -> Json {
    Json::Uint(n as u64)
}

fn checker_json(c: Option<&Checked>) -> Json {
    c.map_or(Json::Null, Checked::to_json)
}

/// What a file is called in the report: a transport file, or (candidate mode, published only
/// after the checker passed it with no applicable rule unverified or violated) a print
/// candidate.
fn kind_of(candidate: bool) -> &'static str {
    if candidate { "candidate" } else { "transport" }
}

fn label_of(candidate: bool) -> &'static str {
    if candidate {
        "print candidate: the checker passed every applicable rule"
    } else {
        TRANSPORT_LABEL
    }
}

fn model_json(candidate: bool, m: &ModelRun) -> Json {
    let (status, output, bytes, stage, error, checked) = match &m.outcome {
        Outcome::Published {
            path: p,
            bytes,
            checked,
        } => (
            "published",
            path(p),
            Json::Uint(*bytes),
            Json::Null,
            Json::Null,
            Some(checked),
        ),
        Outcome::Failed {
            stage,
            message,
            checked,
        } => (
            "failed",
            Json::Null,
            Json::Null,
            Json::str(*stage),
            Json::str(message),
            checked.as_ref(),
        ),
    };
    let blockers = checked.map_or_else(Vec::new, Checked::candidate_blockers);
    let is_candidate = candidate && status == "published";
    Json::obj([
        ("input", path(&m.input)),
        ("output_name", Json::str(format!("{}.chitubox", m.stem))),
        ("status", Json::str(status)),
        ("kind", Json::str(kind_of(candidate))),
        ("print_candidate", Json::Bool(is_candidate)),
        ("label", Json::str(label_of(is_candidate))),
        (
            "candidate_blockers",
            Json::Arr(blockers.iter().map(Json::str).collect()),
        ),
        ("triangles", count(m.triangles)),
        ("output", output),
        ("bytes", bytes),
        ("failed_stage", stage),
        ("error", error),
        ("checker", checker_json(checked)),
    ])
}

fn peak_json(peak: &Result<Peak, String>) -> Json {
    match peak {
        Ok(p) => Json::obj([
            ("job_bytes", Json::Uint(p.job())),
            ("plinth_process_bytes", Json::Uint(p.own)),
            ("largest_child_bytes", Json::Uint(p.largest_child)),
            (
                "basis",
                Json::str(
                    "lifetime high-water marks (getrusage) of the plinth process and of the \
                     largest finished child (the checker), added as if they coexisted; a job \
                     figure, not one model's peak; it reports, it does not prevent",
                ),
            ),
        ]),
        Err(e) => Json::obj([("error", Json::str(e))]),
    }
}

fn steps_json(m: &ModelRun) -> Json {
    let parallel = m.parallel.map_or(Json::Null, |p| {
        Json::obj([
            ("tasks", count(p.tasks)),
            ("threads_ran", count(p.threads_ran)),
        ])
    });
    let steps = m
        .steps
        .iter()
        .map(|s| Json::obj([("step", Json::str(s.name)), ("seconds", Json::Num(s.secs))]))
        .collect();
    Json::obj([
        ("input", path(&m.input)),
        ("parallel_conversion", parallel),
        ("steps", Json::Arr(steps)),
    ])
}

/// The whole report for a finished run.
#[must_use]
pub fn to_json(cfg: &Config, run: &Run) -> Json {
    let failed = run.models.len() - run.published();
    Json::obj([
        ("report_version", Json::Uint(REPORT_VERSION)),
        (
            "tool",
            Json::obj([
                ("name", Json::str("plinth")),
                ("version", Json::str(env!("CARGO_PKG_VERSION"))),
            ]),
        ),
        ("kind", Json::str(kind_of(cfg.candidate))),
        (
            "print_candidate",
            Json::Bool(cfg.candidate && failed == 0 && !run.models.is_empty()),
        ),
        ("label", Json::str(label_of(cfg.candidate && failed == 0))),
        (
            "models",
            Json::Arr(
                run.models
                    .iter()
                    .map(|m| model_json(cfg.candidate, m))
                    .collect(),
            ),
        ),
        (
            "job",
            Json::obj([
                ("models", count(run.models.len())),
                ("published", count(run.published())),
                ("failed", count(failed)),
            ]),
        ),
        (
            "run",
            Json::obj([
                ("max_threads", count(cfg.max_threads)),
                ("checker", path(&cfg.checker)),
                ("limits", path(&cfg.limits)),
                ("total_seconds", Json::Num(run.total_secs)),
                ("peak_memory", peak_json(&run.peak)),
                (
                    "models",
                    Json::Arr(run.models.iter().map(steps_json).collect()),
                ),
            ]),
        ),
    ])
}

/// The FR-21 block of the report for one supported model (lane B; design
/// `step11-construction.md` section 9). Step 0 stub: it states that nothing was computed, and
/// `to_json` does not call it yet.
#[must_use]
pub fn fr21_json(data: &plinth_support::report_data::ReportData) -> Json {
    Json::obj([
        ("computed", Json::Bool(data.is_computed())),
        (
            "unverified",
            data.unverified.as_ref().map_or(Json::Null, Json::str),
        ),
    ])
}

/// One progress line for a finished model, for standard error.
#[must_use]
pub fn progress_line(candidate: bool, m: &ModelRun) -> String {
    let secs: f64 = m.steps.iter().map(|s| s.secs).sum();
    match &m.outcome {
        Outcome::Published { path, checked, .. } => format!(
            "plinth: {} -> {} ({}; checker: {}) {secs:.2} s",
            m.input.display(),
            path.display(),
            label_of(candidate),
            checked.verdict.name()
        ),
        Outcome::Failed { stage, message, .. } => {
            format!("plinth: FAILED at {stage}: {message}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::{Rule, Verdict};
    use std::path::PathBuf;

    fn run_with(outcome: Outcome) -> (Config, Run) {
        let cfg = Config {
            inputs: vec![PathBuf::from("a.stl")],
            out_dir: PathBuf::from("o"),
            max_threads: 2,
            checker: PathBuf::from("c"),
            limits: PathBuf::from("l"),
            candidate: false,
        };
        let m = ModelRun {
            input: PathBuf::from("a.stl"),
            stem: "a".into(),
            triangles: 12,
            steps: vec![],
            parallel: None,
            outcome,
        };
        let run = Run {
            models: vec![m],
            total_secs: 0.5,
            peak: Err("no rusage".into()),
        };
        (cfg, run)
    }

    #[test]
    fn a_report_is_labelled_and_splits_results_from_run_data() {
        let checked = Checked {
            verdict: Verdict::Unverified,
            sha256: "ab".into(),
            rules: vec![Rule {
                id: "H1".into(),
                status: "unverified".into(),
                reason: "later".into(),
            }],
            structure_notes: vec![],
        };
        let (cfg, run) = run_with(Outcome::Published {
            path: PathBuf::from("o/a.chitubox"),
            bytes: 5,
            checked,
        });
        let text = to_json(&cfg, &run).render();
        let j = crate::jsonin::parse(text.as_bytes()).unwrap();
        assert_eq!(j.get("print_candidate"), Some(&Json::Bool(false)));
        assert_eq!(j.get("label").and_then(Json::as_str), Some(TRANSPORT_LABEL));
        let m = &j.get("models").and_then(Json::as_arr).unwrap()[0];
        assert_eq!(m.get("status").and_then(Json::as_str), Some("published"));
        assert_eq!(m.get("print_candidate"), Some(&Json::Bool(false)));
        assert!(m.get("checker").and_then(|c| c.get("rules")).is_some());
        // Timings and memory are only under "run".
        assert!(!text[..text.find("\"run\"").unwrap()].contains("seconds"));
        assert!(j.get("run").and_then(|r| r.get("peak_memory")).is_some());
    }

    #[test]
    fn a_failure_names_its_stage_and_has_no_output() {
        let (cfg, run) = run_with(Outcome::Failed {
            stage: "read",
            message: "bad".into(),
            checked: None,
        });
        let j = crate::jsonin::parse(to_json(&cfg, &run).render().as_bytes()).unwrap();
        let m = &j.get("models").and_then(Json::as_arr).unwrap()[0];
        assert_eq!(m.get("failed_stage").and_then(Json::as_str), Some("read"));
        assert_eq!(m.get("output"), Some(&Json::Null));
        assert_eq!(
            j.get("job")
                .and_then(|x| x.get("failed"))
                .and_then(Json::as_u64),
            Some(1)
        );
    }
}
