//! Measures the wall a hollowed shell ACTUALLY delivers, over its whole
//! cavity surface, and prints the distribution beside the cavity volume.
//!
//! ```sh
//! cargo run --release --offline -p plinth-hollow --example wall_distribution -- \
//!   "/path/to/model.stl" [more.stl ...]
//! ```
//!
//! **Why this exists.** The engine's own FU-55 check
//! (`checked_realized_wall_mm`) records one number per shell: the MINIMUM
//! realized wall. That is the right number to gate on and the wrong number to
//! judge a hollowing by — a shell whose minimum is 2.1 mm could be a correct
//! 2.5 mm hollowing with one thin spot, or an eggshell throughout, and the
//! minimum alone cannot tell them apart. The owner's ruling of 2026-08-13 is
//! stated as a target AND a minimum (aim 2.5, never under 2.0), so verifying
//! it needs the shape of the distribution, not its left endpoint.
//!
//! **Method.** Runs `create_shell` under genuinely unmodified
//! `H0AdmissionLimits::production()` and the registered `HollowSettings` — the
//! shipped cavity path, no decimation and no orientation sweep, so what is
//! measured is what those settings produce. Then, on the DELIVERED shell mesh,
//! it queries the exact distance from every vertex to the source surface
//! through `geom_kernel::NativeSurface` (batched, one call per chunk — never a
//! per-vertex call). Outer-shell vertices sit ON the source surface and come
//! back at ~0; cavity vertices come back at the wall thickness. Splitting at
//! `OUTER_SURFACE_TOLERANCE_MM` separates the two, and every statistic below
//! is over the cavity side only.
//!
//! **Area-weighted, not vertex-counted.** Each cavity vertex carries one third
//! of the area of its incident cavity triangles, so a percentile here means
//! "this share of the cavity SURFACE" and is directly comparable to the same
//! statistic measured on a ChituBox export. Vertex-counting would over-weight
//! the densely tessellated regions, which are exactly the steep ones.
//!
//! **What it cannot see.** The sampling is at mesh vertices, so a thin spot
//! strictly between two vertices is not measured. That is the same
//! methodology, and the same caveat, as the engine's own check.
//!
//! Writes nothing anywhere: this binary prints and exits. Run outputs belong
//! in a scratch directory outside the repository, and redirecting this one's
//! stdout is how to put them there.

use std::env;
use std::path::Path;
use std::process::ExitCode;

use geom_kernel::{NativeSurface, NativeSurfaceConfig};
use plinth_hollow::{
    create_shell, registered_minimum_wall_mm, HollowSettings,
};
use plinth_mesh::{load_binary_stl, Mesh, WeldPolicy};

/// A vertex closer to the source surface than this is on the OUTER shell, not
/// the cavity. The Boolean leaves the outer surface coincident with the source
/// to within kernel noise, and the nearest cavity vertex is a whole wall away
/// (2 mm at the very least, by the minimum-wall check the shell already
/// passed), so any split in between separates the two sets identically. This
/// is a classifier, never a tolerance on a measurement.
const OUTER_SURFACE_TOLERANCE_MM: f64 = 0.10;

/// Points per `NativeSurface::query` call. Matches the order of magnitude the
/// hollow stage itself uses for its realized-wall query.
const QUERY_BATCH_POINTS: usize = 262_144;

fn main() -> ExitCode {
    let paths: Vec<String> = env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!(
            "usage: wall_distribution <model.stl> [more.stl ...]\n\
             prints the delivered wall distribution and cavity volume for each"
        );
        return ExitCode::FAILURE;
    }
    let minimum_wall_mm = match registered_minimum_wall_mm() {
        Ok(value) => value.get(),
        Err(error) => {
            eprintln!("the registered minimum wall must publish: {error}");
            return ExitCode::FAILURE;
        }
    };
    let settings = match HollowSettings::from_registered_constants() {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("registered hollow settings must construct: {error:?}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "registered settings: wall target {} mm, minimum wall {} mm, \
         march margin {} mm, pitch {} mm",
        settings.wall_thickness_mm().get(),
        minimum_wall_mm,
        settings.cavity_march_margin_mm().get(),
        settings.voxel_pitch_mm().get(),
    );

    let mut failures = 0_usize;
    for path in &paths {
        println!("\n=== {path} ===");
        if let Err(reason) = measure_one(Path::new(path), settings, minimum_wall_mm) {
            println!("REFUSED: {reason}");
            failures += 1;
        }
    }
    if failures == paths.len() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn measure_one(
    path: &Path,
    settings: HollowSettings,
    minimum_wall_mm: f64,
) -> Result<(), String> {
    let source = load_binary_stl(path, WeldPolicy::None)
        .map_err(|error| format!("cannot load {}: {error:?}", path.display()))?;
    println!(
        "source: {} vertices, {} triangles",
        source.verts().len(),
        source.tris().len()
    );
    // The source surface is needed AFTER `create_shell` consumes the mesh, so
    // build the query index from a copy of the buffers first.
    let source_verts = source.verts().to_vec();
    let source_tris = source.tris().to_vec();

    let candidate = create_shell(source, settings)
        .map_err(|error| format!("{error:?}"))?;
    let cavity = *candidate.cavity();
    let shell = candidate.mesh();

    let solid_mm3 = cavity.source_volume_mm3().get();
    let cavity_mm3 = cavity.cavity_volume_mm3().get();
    println!(
        "cavity: {cavity_mm3:.1} mm^3 of {solid_mm3:.1} mm^3 solid = {:.1}% saved",
        100.0 * cavity_mm3 / solid_mm3
    );
    println!(
        "engine's own recorded realized wall: {}",
        cavity.realized_wall()
    );

    let surface = NativeSurface::from_indexed(
        &source_verts,
        &source_tris,
        NativeSurfaceConfig {
            grid_pitch_mm: settings.voxel_pitch_mm().get(),
            max_grid_bytes: 512 * 1024 * 1024,
        },
    )
    .map_err(|error| format!("cannot index the source surface: {error:?}"))?;

    // One batched query per chunk over every shell vertex — the whole point of
    // `NativeSurface::query` is that it is index-parallel over the batch, so a
    // per-vertex call here would be a defect, not merely slow.
    let query_bound_mm = 4.0 * settings.wall_thickness_mm().get();
    let mut distances = Vec::with_capacity(shell.verts().len());
    for chunk in shell.verts().chunks(QUERY_BATCH_POINTS) {
        let (chunk_distances, _faces) = surface.query(chunk, query_bound_mm);
        distances.extend(chunk_distances);
    }

    let areas = vertex_areas(shell, &distances);
    let mut samples: Vec<(f64, f64)> = distances
        .iter()
        .zip(&areas)
        .filter(|(distance, area)| **distance > OUTER_SURFACE_TOLERANCE_MM && **area > 0.0)
        .map(|(distance, area)| (*distance, *area))
        .collect();
    if samples.is_empty() {
        return Err("the delivered shell has no cavity surface to measure".to_owned());
    }
    samples.sort_by(|left, right| left.0.total_cmp(&right.0));

    // `fold` from a positive zero, not `sum`: f64::sum folds from -0.0, so an
    // empty selection below prints as "-0.00", which reads like a defect.
    let total_area: f64 = samples.iter().fold(0.0, |total, (_, area)| total + area);
    let percentile = |fraction: f64| -> f64 {
        let target = fraction * total_area;
        let mut running = 0.0;
        for (wall, area) in &samples {
            running += area;
            if running >= target {
                return *wall;
            }
        }
        samples[samples.len() - 1].0
    };
    let mean = samples
        .iter()
        .map(|(wall, area)| wall * area)
        .sum::<f64>()
        / total_area;

    println!(
        "delivered wall over {:.1} mm^2 of cavity surface ({} vertices), area-weighted:",
        total_area,
        samples.len()
    );
    println!(
        "  min {:.3}  p5 {:.3}  median {:.3}  p95 {:.3}  max {:.3}  mean {mean:.3}",
        samples[0].0,
        percentile(0.05),
        percentile(0.50),
        percentile(0.95),
        samples[samples.len() - 1].0,
    );
    for bound in [minimum_wall_mm, 2.2, 2.4, settings.wall_thickness_mm().get()] {
        let under: f64 = samples
            .iter()
            .filter(|(wall, _)| *wall < bound)
            .fold(0.0, |total, (_, area)| total + area);
        println!(
            "  area under {bound:.2} mm: {under:.2} mm^2 = {:.3}%",
            100.0 * under / total_area
        );
    }
    let below_minimum = samples[0].0 < minimum_wall_mm;
    println!(
        "  VERDICT: minimum wall {minimum_wall_mm} mm is {}",
        if below_minimum {
            "VIOLATED — this shell should never have shipped"
        } else {
            "respected"
        }
    );
    Ok(())
}

/// One third of the area of each incident triangle, accumulated per vertex,
/// counting only triangles that lie wholly on the cavity side. A triangle
/// straddling the split (which the Boolean produces where a drain bore would
/// later break through, and around the cavity's own rim) belongs to neither
/// surface and is left out of both.
fn vertex_areas(mesh: &Mesh, distances: &[f64]) -> Vec<f64> {
    let verts = mesh.verts();
    let mut areas = vec![0.0; verts.len()];
    for triangle in mesh.tris() {
        let indices = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        if indices
            .iter()
            .any(|index| distances[*index] <= OUTER_SURFACE_TOLERANCE_MM)
        {
            continue;
        }
        let [a, b, c] = [verts[indices[0]], verts[indices[1]], verts[indices[2]]];
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cross = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        let area =
            0.5 * (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let third = area / 3.0;
        for index in indices {
            areas[index] += third;
        }
    }
    areas
}
