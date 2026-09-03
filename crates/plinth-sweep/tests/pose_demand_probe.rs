//! **What `demand_area_mm2` measures, and what it does not** — the numbers
//! behind the 2026-09-03 finding, kept as a test so they cannot go stale.
//!
//! `demand_area_mm2` is the area that entered the up-sweep's demand set: model
//! cells that had no rooted carrier within reach **at their own birth layer**.
//! `docs/plans/slice-sweep-core-2026-09-02.md` §12 makes it the orientation
//! objective's support term, in place of the retired `w_peak · peak_xsec_cm2`.
//!
//! Measured, it is **anti-correlated with the support burden across the tilt
//! axis**, and the mechanism is structural rather than a defect in any one
//! number:
//!
//! * A FLAT down-face is born all at once with nothing beneath it, so every
//!   square millimetre of it enters the demand set.
//! * A SLOPED down-face is born one crescent at a time, and each crescent is
//!   within one lateral step of the crescent below it — which an arm on a lower
//!   layer already rooted. Almost none of it ever enters the demand set, even
//!   though the greedy then places an arm per tiny demand patch and ends up
//!   with TWICE the arms.
//!
//! Round 50 at the production pitch: the flat pose measures 1812.41 mm² of
//! demand and takes 243 arms; the same disc tilted 10° measures 21.29 mm² —
//! 85× less — and takes 495. The quantity that ranks these two poses correctly
//! is the arm count, which `MeasuredSupportDemand::struts` already publishes
//! and the score does not read.
//!
//! What this test asserts is the ARM COUNT ordering, because that is the
//! statement about the engine that must stay true. The demand areas are
//! printed beside it so the discrepancy is visible rather than remembered.

mod harness;

use harness::sweep_with;
use plinth_core::m0_constants;
use plinth_mesh::{make_recessed_disc, Mesh};
use plinth_sweep::{burden_cm2, SweepPlan, SweepPolicy};

fn rotate_about_y(mesh: &Mesh, degrees: f64) -> Mesh {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let verts = mesh
        .verts()
        .iter()
        .map(|vertex| {
            [
                vertex[0] * cos + vertex[2] * sin,
                vertex[1],
                -vertex[0] * sin + vertex[2] * cos,
            ]
        })
        .collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("a rigid rotation preserves a valid mesh")
}

/// The registered per-arm scar the objective's unit bridge uses
/// (`plinth_sweep::orient_demand`'s module doc).
fn tip_radius_default_mm() -> f64 {
    m0_constants()
        .expect("registry")
        .placeholders()
        .tip_radius_default_mm()
        .diagnostic()
        .get()
}

fn probe(name: &str, mesh: &Mesh, pitch_mm: f64) -> SweepPlan {
    let policy = SweepPolicy::from_registry()
        .expect("registry")
        .with_working_pitch_mm(pitch_mm);
    let plan = sweep_with(mesh, pitch_mm, &policy);
    println!(
        "{name:22} pitch {pitch_mm:.3} mm  demand {:9.2} mm2  arms {:5}  scar {:8.3} mm2  \
         BURDEN {:8.5} cm2  peak {:9.2} mm2  layers {:4}  overloaded {:5}  inserted {:4}",
        plan.report.demand_area_mm2,
        plan.report.struts_total,
        plan.report.total_scar_mm2,
        burden_cm2(plan.report.struts_total, tip_radius_default_mm()),
        plan.report.peak_layer_area_mm2,
        plan.report.layers,
        plan.report.overloaded_struts.len(),
        plan.report.arms_inserted_by_load,
    );
    plan
}

#[test]
fn a_tilt_costs_more_arms_than_the_flat_pose_while_measuring_less_demand() {
    let round_50 = make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128).expect("Round 50 profile");
    let round_25 = make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("Round 25 profile");

    let flat_50 = probe("Round 50 flat", &round_50, 0.05);
    let flipped_50 = probe("Round 50 flipped 180", &rotate_about_y(&round_50, 180.0), 0.05);
    let tilted_50 = probe("Round 50 tilted 10", &rotate_about_y(&round_50, 10.0), 0.05);
    let flat_25 = probe("Round 25 flat", &round_25, 0.05);
    let flipped_25 = probe("Round 25 flipped 180", &rotate_about_y(&round_25, 180.0), 0.05);
    let tilted_25 = probe("Round 25 tilted 3.6", &rotate_about_y(&round_25, 3.6), 0.05);

    // ---- the statement about the engine -----------------------------------
    //
    // A disc printed flat on its own base is the correct pose, and the field
    // the sweep places for it must be the cheaper one. This is what makes the
    // demand term's ranking a FINDING rather than an opinion: the burden is
    // measured here, in the same run, and it disagrees.
    for (name, flat, other) in [
        ("Round 50 tilted 10 deg", &flat_50, &tilted_50),
        ("Round 50 flipped 180 deg", &flat_50, &flipped_50),
        ("Round 25 tilted 3.6 deg", &flat_25, &tilted_25),
        ("Round 25 flipped 180 deg", &flat_25, &flipped_25),
    ] {
        assert!(
            flat.report.struts_total < other.report.struts_total,
            "{name} takes {} arms against the flat pose's {}; the flat pose must be the cheaper \
             field or the burden itself has moved",
            other.report.struts_total,
            flat.report.struts_total,
        );
    }

    // ---- the burden the objective actually reads ---------------------------
    //
    // `plinth_sweep::SweepDemand` prices a pose at
    // `burden_cm2(arms, TIP_RADIUS_DEFAULT_MM)`. The bridge is one fixed
    // positive factor, so this assertion is the arm-count assertion above
    // restated in the objective's own units -- which is the point: it is the
    // ranking the SCORE sees, measured here rather than reasoned about.
    let radius_mm = tip_radius_default_mm();
    for (name, flat, other) in [
        ("Round 50 tilted 10 deg", &flat_50, &tilted_50),
        ("Round 50 flipped 180 deg", &flat_50, &flipped_50),
        ("Round 25 tilted 3.6 deg", &flat_25, &tilted_25),
        ("Round 25 flipped 180 deg", &flat_25, &flipped_25),
    ] {
        let flat_burden = burden_cm2(flat.report.struts_total, radius_mm);
        let other_burden = burden_cm2(other.report.struts_total, radius_mm);
        assert!(
            flat_burden < other_burden,
            "{name} must cost the objective MORE than the flat pose: {other_burden:.5} cm2 \
             against {flat_burden:.5} cm2",
        );
    }

    // ---- and why the scar could not be the burden --------------------------
    //
    // `SweepPlan::total_scar_mm2`'s own invariant is
    // `sum(pi r^2) = peak_layer_area / X_ALLOWED`, independent of how many
    // arms share the load, so the total scar is very nearly a pose invariant.
    // Both discs' 180 deg flips demonstrate it ranking BACKWARDS. This is
    // asserted, not merely printed, so a future substitution back to the scar
    // fails here instead of on the plate.
    for (name, flat, flipped) in [
        ("Round 50", &flat_50, &flipped_50),
        ("Round 25", &flat_25, &flipped_25),
    ] {
        assert!(
            flipped.report.total_scar_mm2 < flat.report.total_scar_mm2,
            "{name}: the scar is expected to rank the 180 deg flip BACKWARDS (that is why it is \
             not the burden); it now reads {:.4} mm2 flipped against {:.4} mm2 flat",
            flipped.report.total_scar_mm2,
            flat.report.total_scar_mm2,
        );
        assert!(
            flipped.report.struts_total > flat.report.struts_total,
            "{name}: while the arm count ranks it correctly",
        );
    }

    // ---- and the demand term's ranking, printed as the finding -------------
    println!(
        "FINDING: Round 50 demand flat {:.2} mm2 vs tilted {:.2} mm2 (ratio {:.1}x in the TILT's \
         favour) while the arm counts are {} vs {} (ratio {:.2}x AGAINST it)",
        flat_50.report.demand_area_mm2,
        tilted_50.report.demand_area_mm2,
        flat_50.report.demand_area_mm2 / tilted_50.report.demand_area_mm2.max(1.0e-9),
        flat_50.report.struts_total,
        tilted_50.report.struts_total,
        tilted_50.report.struts_total as f64 / flat_50.report.struts_total as f64,
    );
}
