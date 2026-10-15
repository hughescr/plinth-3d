//! Compare the exact curvature spread with the protected proxy on STL files.
//!
//! ```text
//! cargo run -p plinth-mesh --release --example delicacy_proxy_probe -- <file.stl> ...
//! ```
//!
//! Prints the time of each evaluation, the per-face curvature difference, and the
//! quantity the orientation score reads: the area-weighted delicacy summed over
//! the faces that would face down, for a spread of up directions. Nothing here
//! decides anything.

use plinth_mesh::{
    evaluate_delicacy_with, load_binary_stl, DelicacyBuildConfig, DelicacyEvaluation,
    DelicacySettings, WeldPolicy,
};

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a percentile index of a non-negative fraction of a slice length"
)]
fn main() {
    let settings = DelicacySettings::production().expect("production delicacy settings");
    let mut directions: Vec<[f64; 3]> = Vec::new();
    for i in 0..12 {
        for j in 0..6 {
            let azimuth = f64::from(i) * std::f64::consts::TAU / 12.0;
            let polar = f64::from(j) * std::f64::consts::PI / 5.0;
            directions.push([
                polar.sin() * azimuth.cos(),
                polar.sin() * azimuth.sin(),
                polar.cos(),
            ]);
        }
    }
    for path in std::env::args().skip(1) {
        let mesh = load_binary_stl(std::path::Path::new(&path), WeldPolicy::None).expect("stl");
        let config = DelicacyBuildConfig::production(mesh.tris().len());
        let run = |evaluation| {
            let started = std::time::Instant::now();
            let field = evaluate_delicacy_with(&mesh, "probe", settings, config, evaluation)
                .expect("field");
            (field, started.elapsed().as_secs_f64())
        };
        let (exact, t_exact) = run(DelicacyEvaluation::Exact);
        let (proxy, t_proxy) = run(DelicacyEvaluation::ProtectedProxy);
        let verts = mesh.verts();
        let mut areas = Vec::new();
        let mut normals = Vec::new();
        for t in mesh.tris() {
            let (a, b, c) = (verts[t[0] as usize], verts[t[1] as usize], verts[t[2] as usize]);
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let x = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let l = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
            areas.push(0.5 * l);
            normals.push(if l > 0.0 { [x[0] / l, x[1] / l, x[2] / l] } else { [0.0; 3] });
        }
        let ev = exact.face_values();
        let pv = proxy.face_values();
        let mut diffs: Vec<f64> = ev
            .iter()
            .zip(pv)
            .map(|(a, b)| (a.curvature() - b.curvature()).abs())
            .collect();
        let total_area: f64 = areas.iter().sum();
        let mass_exact: f64 = ev.iter().zip(&areas).map(|(v, a)| v.curvature() * a).sum();
        let mass_proxy: f64 = pv.iter().zip(&areas).map(|(v, a)| v.curvature() * a).sum();
        let abs_mass: f64 = diffs.iter().zip(&areas).map(|(d, a)| d * a).sum();
        diffs.sort_by(f64::total_cmp);
        let q = |p: f64| diffs[((diffs.len() - 1) as f64 * p) as usize];
        println!("{path}: faces {} area {total_area:.1} mm2", mesh.tris().len());
        println!("  exact {t_exact:.3} s   proxy {t_proxy:.3} s");
        println!(
            "  curvature mass exact {mass_exact:.4} proxy {mass_proxy:.4}  area-weighted |diff| {:.5} of {:.4}",
            abs_mass, mass_exact
        );
        println!(
            "  per-face |diff| p50 {:.5} p90 {:.5} p99 {:.5} p99.9 {:.5} max {:.5}",
            q(0.5),
            q(0.9),
            q(0.99),
            q(0.999),
            q(1.0)
        );
        // Faces the clamp has not saturated: the only faces where the curvature
        // term can still distinguish anything.
        let mut open = 0_usize;
        let mut open_sum = 0.0_f64;
        let mut open_max = 0.0_f64;
        let mut saturated = 0_usize;
        for (a, b) in ev.iter().zip(pv) {
            if a.curvature() >= 0.999 {
                saturated += 1;
            } else {
                let d = (a.curvature() - b.curvature()).abs();
                open += 1;
                open_sum += d;
                open_max = open_max.max(d);
            }
        }
        println!(
            "  faces saturated at 1.0 in the exact field: {saturated} of {}; unsaturated {open}: mean |diff| {:.5} max {:.5}",
            ev.len(),
            open_sum / open.max(1) as f64,
            open_max
        );
        let mut worst = 0.0_f64;
        let mut sum_rel = 0.0_f64;
        let mut worst_abs = 0.0_f64;
        for d in &directions {
            let (mut e, mut p) = (0.0_f64, 0.0_f64);
            for f in 0..areas.len() {
                let down = -(normals[f][0] * d[0] + normals[f][1] * d[1] + normals[f][2] * d[2]);
                if down > std::f64::consts::FRAC_1_SQRT_2 {
                    e += areas[f] * ev[f].weight();
                    p += areas[f] * pv[f].weight();
                }
            }
            if e > 1e-9 {
                worst = worst.max(((p - e) / e).abs());
                sum_rel += ((p - e) / e).abs();
            }
            worst_abs = worst_abs.max((p - e).abs());
        }
        println!(
            "  down-facing area-weighted delicacy over {} up directions: worst rel diff {:.4}, mean rel diff {:.4}, worst abs {:.4} mm2",
            directions.len(),
            worst,
            sum_rel / directions.len() as f64,
            worst_abs
        );
    }
}
