//! Print the measured model-landing scar of a fixture's two flip poses, arm by
//! arm, and the pose the whole search commits to with the term on and with it
//! withheld.
//!
//! `cargo run -p plinth-sweep --release --example landing_scar_probe -- [fixture]`

use plinth_core::{m0_constants, LayerPlan};
use plinth_orient::search::{search, SweepRequest};
use plinth_orient::{OrientFaceTable, UpDirection};
use plinth_sweep::{model_landings, run_sweep, SweepDemand, SweepPolicy};

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "raking_blade".to_string());
    let mesh = match which.as_str() {
        "toed_slab" => plinth_mesh::make_toed_slab(),
        "overhang_table" => plinth_mesh::make_overhang_table(),
        _ => plinth_mesh::make_raking_blade(),
    }
    .expect("fixture");
    let table = OrientFaceTable::build(&mesh, "exact").expect("table");
    let poses = [
        ("identity  (blade down)", [0.0, 0.0, 1.0]),
        ("inverted  (blade up)  ", [0.0, 0.0, -1.0]),
    ];

    let constants = m0_constants().expect("registry");
    let policy = SweepPolicy::from_registry().expect("policy");
    let lift_mm = constants.placeholders().lift_mm().diagnostic().get();
    let layer_height_mm = *constants.evidence().layer_height_default_mm().value();

    for (label, direction) in poses {
        let candidate = UpDirection::new(direction).expect("unit");
        let transformed =
            plinth_orient::oriented_recentered_mesh(&mesh, candidate).expect("oriented");
        let max_z = transformed
            .verts()
            .iter()
            .fold(f64::NEG_INFINITY, |acc, v| acc.max(v[2]));
        let plate_z = -lift_mm;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the same layer-count arithmetic orient_demand.rs performs, so this probe sweeps \
                      the pose the measurer sweeps"
        )]
        let count = (((max_z - plate_z) / layer_height_mm).ceil().max(1.0) as usize) + 1;
        let layers = LayerPlan::new(plate_z, vec![layer_height_mm; count]).expect("layers");
        let plan = run_sweep(&transformed, &layers, &policy).expect("swept");
        println!(
            "\n{label}: {} arms, landings plate {} model {} strut {} (interior {})",
            plan.struts.len(),
            plan.report.landings.plate,
            plan.report.landings.model,
            plan.report.landings.strut,
            plan.report.landings.interior,
        );
        for landing in model_landings(&plan) {
            let strut = &plan.struts[landing.strut];
            // The nearest face, by hand here so the probe can print WHICH face
            // was charged rather than only the total.
            let mut best = (f64::INFINITY, usize::MAX);
            for (face, triangle) in transformed.tris().iter().enumerate() {
                let a = transformed.verts()[triangle[0] as usize];
                let b = transformed.verts()[triangle[1] as usize];
                let c = transformed.verts()[triangle[2] as usize];
                let closest = geom_kernel::closest_on_tri(landing.point_mm, a, b, c);
                let d = (0..3)
                    .map(|k| (landing.point_mm[k] - closest[k]).powi(2))
                    .sum::<f64>();
                if d < best.0 {
                    best = (d, face);
                }
            }
            let face = best.1;
            let fragility = table.delicacy().get(face).copied().unwrap_or(0.0);
            let display = table.display().get(face).copied().unwrap_or(0.0);
            let contact = core::f64::consts::PI * landing.radius_mm * landing.radius_mm;
            println!(
                "  strut {:4} seat ({:7.3},{:7.3},{:7.3}) r_seat {:.4} r_tip {:.4} -> face {:2} \
                 (dist {:.4}) fragility {:.5} D {:.5} scar {:.5} mm2 interior {}",
                landing.strut,
                landing.point_mm[0],
                landing.point_mm[1],
                landing.point_mm[2],
                landing.radius_mm,
                strut.tip_radius_mm(),
                face,
                best.0.sqrt(),
                fragility,
                display,
                contact * fragility * (1.0 + display),
                landing.interior,
            );
        }
        // What the blade's own faces carry, so a reader can tell a slab
        // landing from a blade landing without counting vertices.
        let blade_faces: Vec<usize> = (12..table.face_count().min(24)).collect();
        let blade_fragility: Vec<String> = blade_faces
            .iter()
            .map(|face| format!("{}:{:.4}", face, table.delicacy()[*face]))
            .collect();
        println!("  blade faces (12..24) fragility {}", blade_fragility.join(" "));
    }

    let mut request = SweepRequest::registered().expect("registered");
    // A reference probe: the exact field (the registered request defaults to the proxy).
    request.delicacy = plinth_orient::DelicacyEvaluation::Exact;
    for (label, demand) in [
        ("landing scar ON ", SweepDemand::production()),
        ("landing scar OFF", SweepDemand::without_model_landing_scar()),
    ] {
        let outcome = search(&mesh, &request, Some(&demand)).expect("searchable");
        let winner = outcome.winner();
        let record = outcome.record();
        let commit = record.tiers.last().expect("commit tier");
        println!("\n{label}: winner {:?}", winner.selection);
        for entry in commit.ranking.iter().take(4) {
            let row = record
                .commit_tier_demand
                .iter()
                .find(|row| row.candidate == entry.candidate);
            println!(
                "  rank {} candidate {} score {:.6} scar_raw {:.6} frag {:.6} disp {:.6} \
                 landing {:?} arms {:?} model_landings {:?} fidelity {:?}",
                entry.rank,
                entry.candidate,
                entry.terms.score.get(),
                entry.terms.scar_raw_mm2.get(),
                entry.terms.fragility_raw_mm2.get(),
                entry.terms.display_raw_mm2.get(),
                entry.terms.landing_scar_raw_mm2.map(plinth_core::Finite::get),
                row.and_then(|row| row.struts),
                row.and_then(|row| row.model_landings),
                row.and_then(|row| row.fidelity.clone()),
            );
        }
    }
}
