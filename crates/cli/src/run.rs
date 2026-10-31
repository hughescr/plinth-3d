//! The transport run: each input STL is read, lifted, written to a temp file, checked by the
//! separate `plinth-check` program on those exact bytes, and published only if the checker
//! read the file cleanly (design sections 7.3 and 10 step 7). Models run one at a time, in
//! sorted input order; a failed model never stops the others, and finished files are kept
//! (PRD FR-42).

use crate::check::{self, Checked};
use crate::mem;
use crate::names::{self, Named};
use crate::publish::{self, Fault, Faults};
use plinth_emit::{Parallel, transport_project};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Everything a run needs, with every default already resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Input STL files, as given.
    pub inputs: Vec<PathBuf>,
    /// Directory for the finished files.
    pub out_dir: PathBuf,
    /// Cap on worker threads for every parallel step.
    pub max_threads: usize,
    /// The `plinth-check` program.
    pub checker: PathBuf,
    /// The limits file handed to the checker.
    pub limits: PathBuf,
}

/// Wall time of one step of one model.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// `read`, `convert`, `serialize`, `write`, `check` or `publish`.
    pub name: &'static str,
    /// Seconds.
    pub secs: f64,
}

/// What became of one model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The file is in the output folder under its final name.
    Published {
        /// The final path.
        path: PathBuf,
        /// Its size.
        bytes: u64,
        /// What the checker said.
        checked: Checked,
    },
    /// Nothing was published for this model.
    Failed {
        /// The step that failed.
        stage: &'static str,
        /// A located message.
        message: String,
        /// The checker's answer, when it got that far.
        checked: Option<Checked>,
    },
}

/// One model's record.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelRun {
    /// The input path.
    pub input: PathBuf,
    /// The output name without extension.
    pub stem: String,
    /// Triangles read (0 when the read failed).
    pub triangles: usize,
    /// Time per step, in the order run.
    pub steps: Vec<Step>,
    /// Tasks and threads of the parallel step, when it ran.
    pub parallel: Option<Parallel>,
    /// The result.
    pub outcome: Outcome,
}

/// A finished run.
#[derive(Debug, Clone)]
pub struct Run {
    /// One record per input, in sorted input order.
    pub models: Vec<ModelRun>,
    /// Wall seconds for the whole run.
    pub total_secs: f64,
    /// Job peak memory, or why it could not be read.
    pub peak: Result<mem::Peak, String>,
}

impl Run {
    /// Number of models that were published.
    #[must_use]
    pub fn published(&self) -> usize {
        self.models
            .iter()
            .filter(|m| matches!(m.outcome, Outcome::Published { .. }))
            .count()
    }
}

type Failure = (&'static str, String);

fn timed<T>(steps: &mut Vec<Step>, name: &'static str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let v = f();
    steps.push(Step {
        name,
        secs: t.elapsed().as_secs_f64(),
    });
    v
}

/// Read, lift and serialise one model. Everything large is dropped before this returns, so
/// the checker starts with only the file bytes' size in hand (design section 8).
fn build_bytes(cfg: &Config, named: &Named, m: &mut ModelRun) -> Result<Vec<u8>, Failure> {
    let mesh = timed(&mut m.steps, "read", || {
        plinth_geom::read_stl_file(&named.input)
    })
    .map_err(|e| ("read", e.to_string()))?;
    m.triangles = mesh.triangle_count();
    let (project, parallel) = timed(&mut m.steps, "convert", || {
        transport_project(&named.stem, &mesh, cfg.max_threads)
    })
    .map_err(|e| ("convert", e.to_string()))?;
    m.parallel = Some(parallel);
    drop(mesh);
    timed(&mut m.steps, "serialize", || project.to_bytes())
        .map_err(|e| ("serialize", e.to_string()))
}

fn io_message(what: &str, path: &Path, e: &std::io::Error) -> String {
    if e.kind() == ErrorKind::AlreadyExists {
        format!(
            "{what} {} already exists; it is never overwritten",
            path.display()
        )
    } else {
        format!("{what} {}: {e}", path.display())
    }
}

/// Write the temp file, run the checker on it, publish on success. On any failure the temp
/// file is removed (except for a simulated crash, which leaves it as a dead process would).
fn write_check_publish(
    cfg: &Config,
    named: &Named,
    bytes: &[u8],
    fault: Fault,
    m: &mut ModelRun,
) -> Result<Outcome, Failure> {
    let final_name = format!("{}.chitubox", named.stem);
    let dest = cfg.out_dir.join(&final_name);
    let temp = publish::temp_path(&cfg.out_dir, "model");
    timed(&mut m.steps, "write", || {
        publish::write_temp(&temp, bytes, fault)
    })
    .map_err(|e| ("write", io_message("cannot write", &temp, &e)))?;
    let report = publish::temp_path(&cfg.out_dir, "check");
    let job = check::Job {
        checker: &cfg.checker,
        limits: &cfg.limits,
        chitubox: &temp,
        stl: &named.input,
        report: &report,
        written: bytes,
    };
    let checked = timed(&mut m.steps, "check", || check::run(&job)).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        ("check", e)
    })?;
    if !checked.verdict.publishable() {
        let _ = std::fs::remove_file(&temp);
        let violated = checked.ids_with("violation");
        let message = format!(
            "{}: the checker rejected the file (violated rules: {}; structure: {}); nothing was published",
            named.input.display(),
            if violated.is_empty() {
                "none".to_owned()
            } else {
                violated.join(", ")
            },
            if checked.structure_notes.is_empty() {
                "no notes".to_owned()
            } else {
                checked.structure_notes.join("; ")
            }
        );
        return Ok(Outcome::Failed {
            stage: "check",
            message,
            checked: Some(checked),
        });
    }
    if fault == Fault::CrashBeforePublish {
        return Err((
            "publish",
            format!(
                "simulated crash before publish; {} was left behind",
                temp.display()
            ),
        ));
    }
    timed(&mut m.steps, "publish", || publish::publish(&temp, &dest))
        .map_err(|e| ("publish", io_message("cannot publish", &dest, &e)))?;
    Ok(Outcome::Published {
        path: dest,
        bytes: bytes.len() as u64,
        checked,
    })
}

fn run_model(cfg: &Config, named: &Named, fault: Fault) -> ModelRun {
    let mut m = ModelRun {
        input: named.input.clone(),
        stem: named.stem.clone(),
        triangles: 0,
        steps: Vec::new(),
        parallel: None,
        outcome: Outcome::Failed {
            stage: "read",
            message: String::new(),
            checked: None,
        },
    };
    let result = build_bytes(cfg, named, &mut m)
        .and_then(|bytes| write_check_publish(cfg, named, &bytes, fault, &mut m));
    m.outcome = result.unwrap_or_else(|(stage, message)| Outcome::Failed {
        stage,
        message: format!("{}: {message}", named.input.display()),
        checked: None,
    });
    m
}

/// Check what a run needs before touching any input.
fn preflight(cfg: &Config) -> Result<(), String> {
    for (what, path) in [
        ("the checker", &cfg.checker),
        ("the limits file", &cfg.limits),
    ] {
        if !path.is_file() {
            let hint = if what == "the checker" {
                " (build it: cd checker && cargo build --locked)"
            } else {
                ""
            };
            return Err(format!("{what} not found: {}{hint}", path.display()));
        }
    }
    std::fs::create_dir_all(&cfg.out_dir)
        .map_err(|e| format!("cannot create {}: {e}", cfg.out_dir.display()))
}

/// Run every input. `progress` is called once per finished model.
///
/// # Errors
/// A setup message (missing checker, limits file or output folder) before any input is read.
pub fn run(
    cfg: &Config,
    faults: Faults,
    progress: &mut dyn FnMut(&ModelRun),
) -> Result<Run, String> {
    preflight(cfg)?;
    let started = Instant::now();
    let mut models = Vec::new();
    for (i, named) in names::assign(&cfg.inputs).iter().enumerate() {
        let m = run_model(cfg, named, faults.for_model(i));
        progress(&m);
        models.push(m);
    }
    Ok(Run {
        models,
        total_secs: started.elapsed().as_secs_f64(),
        peak: mem::peak(),
    })
}
