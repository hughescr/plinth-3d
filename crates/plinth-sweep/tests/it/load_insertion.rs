//! **Arms placed by LOAD, not only by reach** — the owner's ruling of
//! 2026-09-03, measured against the field the same fixture gets without it.
//!
//! Reach answers "can cured material span from here to a carrier". The area law
//! answers "can this arm carry what is routed through it". Until this ruling
//! the engine asked only the first question and, where the answer to the second
//! was no, clamped the tip at `r_max` and warned: on `oracle_ramp` that was
//! 8 arms, 514 warned arm-layers and 15.73 mm² of scar against the 39.37 mm²
//! the area law asked for — a 60 % shortfall on a file the oracle still passed,
//! because emit-and-warn turns a shortfall into a warning.
//!
//! This is not an assertion that the insertion "does something". It runs the
//! SAME fixture with the insertion off (`SweepPolicy::without_load_insertion`)
//! and on, and grades the difference, so a change that quietly stops the
//! insertion firing turns it red rather than leaving a green test that means
//! "the path never ran".

use crate::harness;

use harness::{sweep_with, x_allowed};
use plinth_mesh::{make_recessed_disc, Mesh};
use plinth_sweep::SweepPolicy;

/// Tilt a mesh about the X axis — the same rigid rotation `oracle_ramp` uses,
/// and for the same reason: the fixture under test is a SLOPE, and pinning the
/// pose here keeps the orientation search out of the measurement entirely.
fn tilt_about_x(mesh: &Mesh, degrees: f64) -> Mesh {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let verts = mesh
        .verts()
        .iter()
        .map(|vertex| {
            [
                vertex[0],
                vertex[1] * cos - vertex[2] * sin,
                vertex[1] * sin + vertex[2] * cos,
            ]
        })
        .collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("a rigid rotation preserves a valid mesh")
}

fn tilted_disc() -> Mesh {
    let flat = make_recessed_disc(11.5, 12.5, 3.5, 2.5, 10.0, 96).expect("the Round 25 profile");
    tilt_about_x(&flat, 20.0)
}

/// `Σ over arms of tip_load / X_ALLOWED` — the scar area the registered area
/// law asks this field for, arm by arm. Independent of how the engine chose to
/// size anything: it reads the loads, not the radii.
fn required_scar_mm2(plan: &plinth_sweep::SweepPlan) -> f64 {
    plan.struts
        .iter()
        .map(|strut| strut.tip_load_mm2() / x_allowed())
        .sum()
}

#[test]
fn the_area_law_places_arms_reach_never_asked_for_and_the_shortfall_falls() {
    let mesh = tilted_disc();
    let policy = SweepPolicy::from_registry().expect("the registry");
    let without = sweep_with(&mesh, 0.1, &policy.with_working_pitch_mm(0.1).without_load_insertion());
    let with = sweep_with(&mesh, 0.1, &policy.with_working_pitch_mm(0.1));

    let report = |name: &str, plan: &plinth_sweep::SweepPlan| {
        let required = required_scar_mm2(plan);
        println!(
            "{name}: arms {} (inserted by load {}), scar {:.4} mm2 of {required:.4} mm2 required \
             (ratio {:.4}), overloaded arm-layers {}, worst realized X {:.3}, insertion rounds \
             {}, layers round-bound-exhausted {}, layers geometry-exhausted {}",
            plan.report.struts_total,
            plan.report.arms_inserted_by_load,
            plan.report.total_scar_mm2,
            plan.report.total_scar_mm2 / required,
            plan.report.overloaded_struts.len(),
            plan.report
                .overloaded_struts
                .iter()
                .map(|arm| arm.realized_x)
                .fold(0.0_f64, f64::max),
            plan.report.insertion_rounds,
            plan.report.insertion_rounds_exhausted,
            plan.report.insertion_geometry_exhausted,
        );
        (required, plan.report.total_scar_mm2 / required)
    };
    let (_, ratio_without) = report("reach only    ", &without);
    let (_, ratio_with) = report("reach and load", &with);

    // ---- the path fired ---------------------------------------------------
    assert_eq!(
        without.report.arms_inserted_by_load, 0,
        "the insertion was switched OFF and still added arms"
    );
    assert_eq!(
        without.report.insertion_rounds, 0,
        "the insertion was switched OFF and the area law was still consulted"
    );
    assert!(
        with.report.insertion_rounds > 0,
        "the area law was never consulted; this comparison would be about nothing"
    );
    assert!(
        with.report.arms_inserted_by_load > 0,
        "the area law asked for no arms on a 20 deg underside where {} arm-layers are over the \
         limit; either the trigger never fired or the footprint search finds nothing",
        without.report.overloaded_struts.len(),
    );

    // ---- and it moved the quantity it exists to move ----------------------
    //
    // The shortfall is what the ruling is about: an arm clamped at `r_max`
    // provides LESS scar than the law asks for, and the ratio of provided to
    // required is exactly how much less. Adding carriers can only raise it.
    assert!(
        ratio_with > ratio_without,
        "the scar the field provides is {ratio_with:.4} of what the area law asks for, against \
         {ratio_without:.4} without the insertion; adding carriers must reduce the shortfall"
    );
    assert!(
        with.report.overloaded_struts.len() < without.report.overloaded_struts.len(),
        "{} overloaded arm-layers with the insertion against {} without; the arms it added must \
         take load off the arms that were over the limit",
        with.report.overloaded_struts.len(),
        without.report.overloaded_struts.len(),
    );

    // ---- and it did not break what it must not break ----------------------
    assert!(
        !with.report.sweeps_disagree,
        "the inserted arms left the two sweeps contradicting each other"
    );
    assert!(
        with.report.uncarried_load_mm2 <= 0.0,
        "{:.4} mm2 of load reached no carrier after insertion",
        with.report.uncarried_load_mm2,
    );
    assert!(
        with.report.conservation_residual <= 1.0e-9 * with.report.injected_load_mm2.max(1.0),
        "conservation residual {:.3e} after insertion",
        with.report.conservation_residual,
    );
    // Every inserted arm is a real column with a real ledger, not a record with
    // nothing in it: it seats against cured material above it and stops at the
    // first thing its column meets going down.
    for strut in with.struts.iter().filter(|strut| strut.inserted_by_load) {
        assert!(
            strut.k_top > strut.k_bottom,
            "inserted arm {} has no length: k_top {} k_bottom {}",
            strut.id,
            strut.k_top,
            strut.k_bottom,
        );
        assert_eq!(
            strut.carried_mm2.len(),
            strut.k_top - strut.k_bottom,
            "inserted arm {}'s ledger does not span its own extent",
            strut.id,
        );
        assert!(
            strut.covered_demand.is_empty(),
            "inserted arm {} claims to root demand cells; the removal pass would then believe it \
             was placed by reach",
            strut.id,
        );
        // The flare property the area law depends on: carried is non-decreasing
        // downward, so the narrowest section is at the tip.
        for pair in strut.carried_mm2.windows(2) {
            assert!(
                pair[0] >= pair[1] - 1.0e-9,
                "inserted arm {}'s carried profile decreases downward",
                strut.id,
            );
        }
    }
}

#[test]
fn a_flat_disc_needs_no_load_driven_arms_at_all() {
    // The counterpart, so "it always inserts" cannot pass for "it inserts where
    // the law asks". A flat disc's arms sit on a hex lattice at 3.41 mm, so one
    // arm drains at most a hex cell of 10.06 mm2 of peel against the 37.70 mm2
    // one tip may carry — a factor of 3.7 of headroom. The insertion must find
    // nothing to do.
    let policy = SweepPolicy::from_registry().expect("the registry");
    let plan = sweep_with(
        &make_recessed_disc(24.0, 25.0, 3.5, 2.5, 22.5, 128).expect("the Round 50 profile"),
        0.05,
        &policy.with_working_pitch_mm(0.05),
    );
    println!(
        "flat disc: arms {} (inserted by load {}), overloaded arm-layers {}, insertion rounds {}",
        plan.report.struts_total,
        plan.report.arms_inserted_by_load,
        plan.report.overloaded_struts.len(),
        plan.report.insertion_rounds,
    );
    assert!(
        plan.report.insertion_rounds > 0,
        "the insertion step never ran, so finding nothing to do proves nothing"
    );
    assert_eq!(
        plan.report.arms_inserted_by_load, 0,
        "the area law added arms to a flat disc where every arm has 3.7x of headroom"
    );
    assert!(
        plan.report.overloaded_struts.is_empty(),
        "a flat disc overloaded {} arm-layers",
        plan.report.overloaded_struts.len()
    );
}
