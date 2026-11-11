//! The `plinth` binary: exit 0 when every model was published, 1 when some model failed (the
//! others are kept), 4 for a usage or setup error.

use plinth_cli::publish::{self, Fault, Faults};
use plinth_cli::{args, config_from, json::Json, report, run};
use std::path::Path;
use std::process::ExitCode;

/// Write the report to `path` under the no-overwrite rule (temp file, then a new name).
fn write_report(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty());
    let dir = dir.unwrap_or_else(|| Path::new("."));
    let temp = publish::temp_path(dir, "report");
    publish::write_temp(&temp, text.as_bytes(), Fault::None)?;
    publish::publish(&temp, path)
}

fn emit_report(path: Option<&Path>, doc: &Json) -> bool {
    let text = doc.render();
    match path {
        Some(p) => match write_report(p, &text) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("plinth: cannot write the report {}: {e}", p.display());
                print!("{text}");
                false
            }
        },
        None => {
            print!("{text}");
            true
        }
    }
}

fn main() -> ExitCode {
    // Keeps the canary chain in the binary so a canary edit relinks it.
    std::hint::black_box(plinth_cli::canary_chain());
    let usage = |msg: &str| {
        eprintln!("plinth: {msg}\n{}", args::USAGE);
        ExitCode::from(4)
    };
    let opts = match args::parse(std::env::args_os().skip(1)) {
        Ok(o) => o,
        Err(e) => return usage(&e),
    };
    let exe = std::env::current_exe().unwrap_or_default();
    let cfg = match config_from(&opts, &exe) {
        Ok(c) => c,
        Err(e) => return usage(&e),
    };
    let mut progress = |m: &run::ModelRun| eprintln!("{}", report::progress_line(cfg.candidate, m));
    let finished = match run::run(&cfg, Faults::default(), &mut progress) {
        Ok(r) => r,
        Err(e) => return usage(&e),
    };
    let wrote = emit_report(opts.report.as_deref(), &report::to_json(&cfg, &finished));
    // A review file kept although the checker found a rule violated still counts as a
    // failure for the exit status: the file exists to be looked at, nothing more.
    let all_published = finished.published() == finished.models.len()
        && !finished
            .models
            .iter()
            .any(|m| m.review.is_some_and(|r| r.checker_failed));
    ExitCode::from(u8::from(!(all_published && wrote)))
}
