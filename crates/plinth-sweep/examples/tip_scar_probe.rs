//! Print the measured tip-contact scar of a fixture's two flip poses, arm by
//! arm, BOTH double-counting choices side by side, and the pose the whole
//! search commits to with the term on and withheld.
//!
//! `cargo run -p plinth-sweep --release --example tip_scar_probe -- [fixture]`

use plinth_core::{m0_constants, LayerPlan};
use plinth_orient::search::{search, SweepRequest};
use plinth_orient::{ObjectiveWeights, OrientFaceTable, UpDirection};
use plinth_sweep::{run_sweep, tip_landings, SweepDemand, SweepPolicy};

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "raking_blade".to_string());
    let mesh = match which.as_str() {
        "toed_slab" => plinth_mesh::make_toed_slab(),
        "overhang_table" => plinth_mesh::make_overhang_table(),
        "round_50" => plinth_mesh::make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128),
        "round_25" => plinth_mesh::make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96),
        _ => plinth_mesh::make_raking_blade(),
    }
    .expect("fixture");
    let table = OrientFaceTable::build(&mesh, "exact").expect("table");
    let weights = ObjectiveWeights::registered().expect("weights");
    let down_limit = weights.down_limit();
    let poses = [("identity  (authored up)", [0.0, 0.0, 1.0]), ("inverted  (flipped)   ", [0.0, 0.0, -1.0])];

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
        println!("\n{label}: {} arms", plan.struts.len());

        let scar = plinth_sweep::tip_contact_scar(
            &transformed,
            &plan,
            table.delicacy(),
            table.display(),
            down_limit,
        )
        .expect("a table with a fragility field");
        let choice_ii_total: f64 = scar.raw_mm2;
        let choice_i_total: f64 = scar.charges.iter().map(|charge| charge.scar_mm2).sum();
        let excluded_count = scar.charges.iter().filter(|charge| charge.already_counted).count();
        let excluded_total: f64 = scar
            .charges
            .iter()
            .filter(|charge| charge.already_counted)
            .map(|charge| charge.scar_mm2)
            .sum();
        println!(
            "  tips {} mapped {} | choice(i) charge-everything {choice_i_total:.5} mm2 | \
             choice(ii) exclude-already-counted {choice_ii_total:.5} mm2 | excluded {excluded_count} \
             charges worth {excluded_total:.5} mm2",
            scar.tips, scar.mapped,
        );
        for landing in tip_landings(&plan) {
            let Some(charge) = scar.charges.iter().find(|charge| charge.strut == landing.strut)
            else {
                continue;
            };
            println!(
                "  strut {:4} tip ({:7.3},{:7.3},{:7.3}) r_tip {:.4} -> face {:2} (dist {:.4}) \
                 fragility {:.5} scar {:.5} mm2 already_counted {}",
                landing.strut,
                landing.point_mm[0],
                landing.point_mm[1],
                landing.point_mm[2],
                landing.radius_mm,
                charge.face,
                charge.distance_mm,
                table.delicacy().get(charge.face).copied().unwrap_or(0.0),
                charge.scar_mm2,
                charge.already_counted,
            );
        }
    }

    let request = SweepRequest::registered().expect("registered");
    for (label, demand) in [
        ("tip scar ON ", SweepDemand::production()),
        ("tip scar OFF", SweepDemand::without_tip_contact_scar()),
    ] {
        let outcome = search(&mesh, &request, Some(&demand)).expect("searchable");
        let winner = outcome.winner();
        let record = outcome.record();
        let commit = record.tiers.last().expect("commit tier");
        println!("\n{label}: winner {:?}", winner.selection);
        for entry in commit.ranking.iter().take(4) {
            println!(
                "  rank {} candidate {} score {:.6} scar_raw {:.6} landing {:?} tip {:?}",
                entry.rank,
                entry.candidate,
                entry.terms.score.get(),
                entry.terms.scar_raw_mm2.get(),
                entry.terms.landing_scar_raw_mm2.map(plinth_core::Finite::get),
                entry.terms.tip_scar_raw_mm2.map(plinth_core::Finite::get),
            );
        }
    }
}
