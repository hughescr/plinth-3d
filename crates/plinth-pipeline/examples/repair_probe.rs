//! The corpus repair census, kept re-checkable.
//!
//! This is the harness that produced the numbers behind the D-22 revision
//! (owner ruling 2026-08-10,
//! `docs/engineering-journal/decisions/2026-08-10-d22-revision-automatic-repair.md`),
//! landed as a permanent diagnostic rather than left as a throwaway. Its whole
//! job is to answer one question per model, four ways:
//!
//! ```text
//! raw               does the loaded input pass the closed-solid gate?
//! raw+repair        does it pass after the automatic pre-pass?
//! decimated         does the production decimated mesh pass?
//! decimated+repair  does the decimated mesh pass after a pre-pass?
//! ```
//!
//! The four-way split is what separates two very different findings that a
//! single before/after would blur together. Thirteen of the twenty-one refused
//! models are damaged **on disk**; seven are clean on disk and are damaged by
//! the **decimator**, which manufactures duplicate faces (Cat wild shape:
//! 82,564 → 53,150 faces gains 2 duplicate faces and 31 point-only-cut faces;
//! zombie-9-b: 373,748 → 51,974 gains 8). The production pre-pass runs on the
//! loaded input, so the `decimated+repair` column is the measurement of what
//! that placement does **not** reach — it is evidence for a work item, not a
//! description of the pipeline.
//!
//! **Each repair gets its own labelled line, because there are two of them.**
//! The `raw+repair` and `decimated+repair` columns are two *different* repair
//! runs over two different meshes, and one shared "repair actions" column
//! would be read as describing whichever verdict the eye landed on. It
//! measurably would have been: for the ~11 models that are clean on disk and
//! cut by the decimator, the raw repair takes no action at all, so a single
//! column printed an all-zeros action summary on the same row as the
//! `dec+repair` PASS — inviting a reader of FU-44 or the D-22 journal entry to
//! conclude that zero actions flipped a CUT into a PASS. The two summaries are
//! therefore printed on their own lines, always, including when they are all
//! zeros: a record that omits a row when it is empty cannot be diffed against
//! the next run.
//!
//! ```text
//! <name>          raw   raw+repair   decimated   dec+repair   faces raw->dec
//!     raw repair  what repair_mesh did to the loaded input
//!     dec repair  what repair_mesh did to the decimated mesh
//! ```
//!
//! **The verdicts come from the production gate.** Every column calls
//! `plinth_hollow::validate_closed_mesh`, the same function `create_shell` and
//! `validate_oriented_solid` call. A diagnostic with its own copy of the gate
//! is a diagnostic that will eventually disagree with the pipeline and be
//! believed anyway.
//!
//! ```sh
//! cargo run --release --offline -p plinth-pipeline --example repair_probe -- <folder-of-stl>
//! ```
//!
//! With no argument it reads `corpus/models`. Nothing is written: this prints
//! and exits, and its exit code is 0 unless a file could not be read at all —
//! a refused model is the finding, never an error of the harness.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use plinth_hollow::validate_closed_mesh;
use plinth_mesh::{load_binary_stl, repair_mesh, Mesh, WeldPolicy};
use plinth_pipeline::prepare::registered_decimation_budget;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let folder = args
        .first()
        .map_or_else(|| workspace_root().join("corpus/models"), PathBuf::from);
    let sources = match enumerate(&folder) {
        Ok(sources) if sources.is_empty() => {
            eprintln!("no .stl files in {}", folder.display());
            return ExitCode::FAILURE;
        }
        Ok(sources) => sources,
        Err(error) => {
            eprintln!("cannot read {}: {error}", folder.display());
            return ExitCode::FAILURE;
        }
    };
    let budget = match registered_decimation_budget() {
        Ok(budget) => budget,
        Err(error) => {
            eprintln!("decimation budget unavailable: {error}");
            return ExitCode::FAILURE;
        }
    };

    println!(
        "{:<34}{:>10}{:>12}{:>14}{:>12}{:>18}",
        "model", "raw", "raw+repair", "decimated", "dec+repair", "faces raw->dec"
    );
    let mut unreadable = 0_usize;
    for source in &sources {
        let name = source.file_stem().map_or_else(
            || source.display().to_string(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let Ok(raw) = load_binary_stl(source, WeldPolicy::None) else {
            println!("{name:<34}{:>10}", "UNREADABLE");
            unreadable += 1;
            continue;
        };

        let (raw_repaired, raw_actions) = repair(Some(&raw));

        let decimated = plinth_decimate::run(&raw, budget)
            .ok()
            .and_then(|outcome| outcome.into_decimated_model().ok())
            .map(|model| model.mesh().clone());
        let (decimated_repaired, decimated_actions) = repair(decimated.as_ref());

        println!(
            "{name:<34}{:>10}{:>12}{:>14}{:>12}{:>18}",
            verdict(Some(&raw)),
            verdict(raw_repaired.as_ref()),
            verdict(decimated.as_ref()),
            verdict(decimated_repaired.as_ref()),
            format!(
                "{}->{}",
                raw.tris().len(),
                decimated
                    .as_ref()
                    .map_or_else(|| "?".to_owned(), |mesh| mesh.tris().len().to_string())
            ),
        );
        // Each repair's own actions, under its own name. The two runs are
        // separate and their action counts are not interchangeable.
        println!("    raw repair  {raw_actions}");
        println!("    dec repair  {decimated_actions}");
    }

    if unreadable > 0 {
        eprintln!(
            "{unreadable} of {} sources could not be read",
            sources.len()
        );
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Run the production pre-pass over one mesh, returning the repaired mesh and
/// the record of what that particular run did.
///
/// The pair is returned together on purpose: every action summary this
/// diagnostic prints has to come back attached to the mesh it was measured on,
/// so a caller cannot pair one run's verdict with another run's counts.
fn repair(mesh: Option<&Mesh>) -> (Option<Mesh>, String) {
    let Some(mesh) = mesh else {
        return (None, "n/a (no mesh at this stage)".to_owned());
    };
    // No error arm: the pre-pass returns a mesh or a mesh. An input it could
    // not work on comes back untouched with the reason on its summary line,
    // which is what this column is here to show.
    let repaired = repair_mesh(mesh);
    let line = repaired.evidence().summary_line();
    (Some(repaired.into_mesh()), line)
}

/// PASS or the production gate's own refusal, abbreviated to fit a column.
fn verdict(mesh: Option<&Mesh>) -> String {
    match mesh {
        None => "n/a".to_owned(),
        Some(mesh) => match validate_closed_mesh(mesh) {
            Ok(()) => "PASS".to_owned(),
            Err(error) => match error {
                plinth_hollow::HollowError::NotClosedOrOriented => "CUT".to_owned(),
                plinth_hollow::HollowError::StrictAudit(_) => "AUDIT".to_owned(),
                other => format!("{other}"),
            },
        },
    }
}

fn enumerate(folder: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut sources = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("stl"))
            && path.is_file()
        {
            sources.push(path);
        }
    }
    sources.sort();
    Ok(sources)
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}
