//! Witness: the parallel curvature spread and display neighbourhood pass equal
//! their frozen serial oracles bit for bit, on real STL files, at every thread
//! width asked for.
//!
//! ```text
//! PLINTH_WITNESS_WIDTHS="1 4 16" cargo run -p plinth-mesh --offline --release \
//!     --features serial-oracle --example surface_field_witness -- <file.stl> ...
//! ```
//!
//! Per file and width it prints the three field digests (parallel and serial)
//! and every path-hit counter, and exits non-zero if any field differs by a
//! bit, any work count differs from the oracle's, or a path-hit counter that
//! must be positive is zero. Reads the STL as authored (no orientation,
//! decimation or hollowing). Run under the shared CPU lock.

use plinth_mesh::surface_oracle::compare_surface_fields;
use plinth_mesh::{load_binary_stl, WeldPolicy};

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: surface_field_witness <file.stl> [file.stl ...]");
        std::process::exit(2);
    }
    let widths: Vec<String> = std::env::var("PLINTH_WITNESS_WIDTHS")
        .unwrap_or_else(|_| "1 4 16".to_owned())
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let mut failed = false;
    for path in &paths {
        let mesh = match load_binary_stl(std::path::Path::new(path), WeldPolicy::None) {
            Ok(mesh) => mesh,
            Err(error) => {
                println!("{path}: NOT READ: {error}");
                failed = true;
                continue;
            }
        };
        for width in &widths {
            // `current_threads` re-reads PLINTH_THREADS on every kernel call.
            std::env::set_var("PLINTH_THREADS", width);
            let started = std::time::Instant::now();
            let report = match compare_surface_fields(&mesh) {
                Ok(report) => report,
                Err(error) => {
                    println!("{path} t{width}: REFUSED: {error}");
                    failed = true;
                    continue;
                }
            };
            let elapsed = started.elapsed().as_secs_f64();
            println!(
                "{path} t{width} faces={} ({elapsed:.1}s parallel+serial)",
                report.faces
            );
            for field in [&report.curvature, &report.display_detail, &report.display_curvature] {
                println!(
                    "  {:<18} parallel={} serial={} {}",
                    field.name,
                    field.parallel_digest,
                    field.serial_digest,
                    if field.identical() { "IDENTICAL" } else { "DIFFERS" }
                );
            }
            println!(
                "  counters: vertices_spread={} (serial {}) pairs_measured={} (serial {}) \
                 pairs_charged={} (serial {}) faces_multiply_charged={} blocks={} waves={} \
                 overflow_splits={} faces_walked={} non_degenerate_faces={} members_total={}",
                report.vertices_spread,
                report.serial_vertices_spread,
                report.pairs_measured,
                report.serial_pairs_measured,
                report.pairs_charged,
                report.serial_pairs_charged,
                report.faces_multiply_charged,
                report.blocks,
                report.waves,
                report.overflow_splits,
                report.faces_walked,
                report.non_degenerate_faces,
                report.members_total,
            );
            let paths_hit = report.vertices_spread > 0
                && report.pairs_charged > 0
                && report.faces_multiply_charged > 0
                && report.faces_walked == report.non_degenerate_faces
                && report.members_total > report.faces_walked;
            if !report.all_identical() || !paths_hit {
                println!(
                    "  FAIL: identical={} path_hits={paths_hit}",
                    report.all_identical()
                );
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}
