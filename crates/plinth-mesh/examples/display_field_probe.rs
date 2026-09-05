//! Print the display-face field of one or more STL files, so the map can be
//! INSPECTED before it steers anything.
//!
//! Owner ruling, 2026-09-03: the per-model D summary is published in the run
//! report, and this example is the standalone form of the same summary — it
//! reads a mesh straight off disk, in its AUTHORED frame, with no orientation,
//! decimation or hollowing in the way, so what it prints is the field as
//! `plinth-orient` would measure it at the Exact LOD.
//!
//! ```text
//! cargo run -p plinth-mesh --offline --release --example display_field_probe -- <file.stl> ...
//! ```
//!
//! Every number printed here is a measurement of the file named beside it.
//! Nothing here decides anything; there is no threshold and no verdict.

use plinth_mesh::{
    evaluate_display, load_binary_stl, DisplayBuildConfig, DisplaySettings, WeldPolicy,
};

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "one bin index from a value the DisplayFaceValue contract has already clamped into \
              [0, 1]; the floor is non-negative and below 10 by construction and the min(9) pins \
              the closed end"
)]
fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: display_field_probe <file.stl> [file.stl ...]");
        std::process::exit(2);
    }
    let settings = DisplaySettings::production().expect("production display settings");
    println!(
        "settings: {} view directions, R_detail {:.4} mm",
        settings.view_directions(),
        settings.detail_radius_mm(),
    );
    for path in paths {
        let mesh = match load_binary_stl(std::path::Path::new(&path), WeldPolicy::None) {
            Ok(mesh) => mesh,
            Err(error) => {
                println!("{path}: NOT READ: {error}");
                continue;
            }
        };
        let config = DisplayBuildConfig::production(mesh.tris().len()).expect("display config");
        let started = std::time::Instant::now();
        let field = match evaluate_display(&mesh, "probe", settings, config) {
            Ok(field) => field,
            Err(error) => {
                println!("{path}: REFUSED: {error}");
                continue;
            }
        };
        let elapsed = started.elapsed().as_secs_f64();

        // Per-face area and authored normal z, recomputed here rather than
        // exported from the evaluator: this is a probe, and it should not be
        // able to agree with the evaluator by sharing its arithmetic.
        let verts = mesh.verts();
        let mut areas = Vec::with_capacity(mesh.tris().len());
        let mut normal_z = Vec::with_capacity(mesh.tris().len());
        for triangle in mesh.tris() {
            let a = verts[triangle[0] as usize];
            let b = verts[triangle[1] as usize];
            let c = verts[triangle[2] as usize];
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cross = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let length = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
            areas.push(0.5 * length);
            normal_z.push(if length > 0.0 { cross[2] / length } else { 0.0 });
        }

        let values = field.face_values();
        let total_area: f64 = areas.iter().sum();
        let mut weighted = 0.0_f64;
        let mut plain = 0.0_f64;
        let mut histogram = [0.0_f64; 10];
        let mut up_area = 0.0_f64;
        let mut up_weighted = 0.0_f64;
        let mut down_area = 0.0_f64;
        let mut down_weighted = 0.0_f64;
        let mut top: Vec<(f64, usize)> = Vec::new();
        for (face, value) in values.iter().enumerate() {
            let d = value.weight();
            weighted += areas[face] * d;
            plain += d;
            // Ten equal bins over [0, 1]; D is already clamped into that
            // range by the contract, so the `min` only catches D == 1 exactly.
            let bin = ((d * 10.0).floor().max(0.0) as usize).min(9);
            histogram[bin] += areas[face];
            if normal_z[face] > std::f64::consts::FRAC_1_SQRT_2 {
                up_area += areas[face];
                up_weighted += areas[face] * d;
            } else if normal_z[face] < -std::f64::consts::FRAC_1_SQRT_2 {
                down_area += areas[face];
                down_weighted += areas[face] * d;
            }
            top.push((d, face));
        }
        top.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));

        println!("\n{path}");
        println!(
            "  faces {}  total area {:.2} mm2  digest {}  {:.2} s",
            values.len(),
            total_area,
            &field.digest()[..12],
            elapsed
        );
        println!(
            "  D mean (per face) {:.6}   D mean (area-weighted) {:.6}",
            plain / values.len() as f64,
            weighted / total_area,
        );
        println!(
            "  authored UP faces (n_z > 1/sqrt(2)): area {:.2} mm2, area-weighted D {:.6}",
            up_area,
            if up_area > 0.0 {
                up_weighted / up_area
            } else {
                0.0
            }
        );
        println!(
            "  authored DOWN faces (n_z < -1/sqrt(2)): area {:.2} mm2, area-weighted D {:.6}",
            down_area,
            if down_area > 0.0 {
                down_weighted / down_area
            } else {
                0.0
            }
        );
        print!("  area-weighted histogram of D (10 bins, % of area):");
        for bucket in histogram {
            print!(" {:.2}", bucket / total_area * 100.0);
        }
        println!();
        println!("  the 8 faces with the highest D:");
        for &(d, face) in top.iter().take(8) {
            let value = values[face];
            println!(
                "    face {face:>7}  D {d:.6}  visibility {:.6}  detail {:.6}  tip_factor {:.6}  \
                 n_z {:+.4}  area {:.5} mm2",
                value.visibility(),
                value.detail(),
                value.tip_factor(),
                normal_z[face],
                areas[face],
            );
        }
    }
}
