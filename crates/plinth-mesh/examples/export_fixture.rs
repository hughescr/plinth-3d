//! Write a procedural `make_*` fixture out as a binary STL, so it can be run
//! through the production CLI exactly like a corpus model.
//!
//! ```text
//! cargo run -p plinth-mesh --example export_fixture -- make_brace_pair <out-dir>
//! cargo run -p plinth-mesh --example export_fixture -- --all <out-dir>
//! ```
//!
//! It is an EXAMPLE and not a test for the same reason `corpus_gate` is: it
//! writes files, and `cargo test --workspace --all-features` must stay a
//! clean-checkout gate that writes nothing outside its own scratch.
//!
//! **This is deliberately not the corpus gate.** `corpus_gate` audits the eight
//! provisioned corpus members against pinned digests and its membership is
//! frozen; a fixture exported here is a MODEL TO MEASURE, not a corpus member,
//! and adding one to that gate would change what the gate is.
//!
//! `write_binary_stl` refuses any coordinate that is not exactly representable
//! in f32, so every fixture reachable here is built from exact values and a
//! rounding mistake in one of them is a failure at this boundary rather than a
//! silent change of geometry.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use plinth_mesh::{write_binary_stl, Mesh, MeshError};

type Builder = fn() -> Result<Mesh, MeshError>;

/// Every procedural fixture this example can export, by the name the CLI and
/// the evidence tables use for it.
/// `make_toed_slab`, `make_raking_blade` and `make_thin_fin` are deliberately
/// NOT here: their cone and offset geometry carries f64 coordinates that are not
/// exactly representable in f32, so `write_binary_stl` refuses them by design,
/// and their provisioned `corpus/models/*.stl` are what the measurements use.
const FIXTURES: [(&str, Builder); 7] = [
    ("make_box", plinth_mesh::make_box_default),
    ("make_overhang_table", plinth_mesh::make_overhang_table),
    ("make_uncovered_rim_table", plinth_mesh::make_uncovered_rim_table),
    // The slenderness lane's three (2026-09-06).
    ("make_tall_stilt_plate", plinth_mesh::make_tall_stilt_plate),
    ("make_lonely_peak", plinth_mesh::make_lonely_peak),
    ("make_brace_pair", plinth_mesh::make_brace_pair),
    // The congestion lane's own (2026-09-06).
    ("make_laid_tube", plinth_mesh::make_laid_tube),
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (selector, out_dir) = match args.as_slice() {
        [name, directory] => (name.clone(), PathBuf::from(directory)),
        _ => {
            eprintln!(
                "usage: export_fixture <fixture-name|--all> <out-dir>\nknown fixtures: {}",
                FIXTURES
                    .iter()
                    .map(|(name, _)| *name)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = std::fs::create_dir_all(&out_dir) {
        eprintln!("cannot create {}: {error}", out_dir.display());
        return ExitCode::FAILURE;
    }
    let wanted: Vec<&(&str, Builder)> = if selector == "--all" {
        FIXTURES.iter().collect()
    } else {
        match FIXTURES.iter().find(|(name, _)| *name == selector) {
            Some(entry) => vec![entry],
            None => {
                eprintln!(
                    "unknown fixture {selector}; known fixtures: {}",
                    FIXTURES
                        .iter()
                        .map(|(name, _)| *name)
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                return ExitCode::FAILURE;
            }
        }
    };
    let mut failures = 0_usize;
    for (name, build) in wanted {
        match export(name, *build, &out_dir) {
            Ok(path) => println!("wrote {}", path.display()),
            Err(reason) => {
                eprintln!("FINDING\t{name}\t{reason}");
                failures += 1;
            }
        }
    }
    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn export(name: &str, build: Builder, out_dir: &Path) -> Result<PathBuf, String> {
    let mesh = build().map_err(|error| format!("fixture did not build: {error}"))?;
    let path = out_dir.join(format!("{name}.stl"));
    write_binary_stl(&path, &mesh).map_err(|error| format!("cannot write STL: {error}"))?;
    Ok(path)
}
