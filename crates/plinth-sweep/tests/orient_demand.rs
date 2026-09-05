//! LANE 3 (`docs/plans/slice-sweep-core-2026-09-02.md` §12): the real
//! `plinth_orient::demand::SupportDemandMeasurer` implementation, exercised
//! end to end on a real fixture. `plinth-orient/tests/peel_demand_override.rs`
//! exercises the trait's own contract (the commit-tier override, its two
//! fidelities, its fallback, and the robustness/flip-crossover records)
//! through a test-local mock; this is the one test of the real geometry this
//! crate contributes.

use plinth_core::m0_constants;
use plinth_mesh::make_overhang_table;
use plinth_orient::demand::{DemandFidelity, SupportDemandMeasurer};
use plinth_orient::{OrientFaceTable, UpDirection};
use plinth_sweep::{burden_cm2, SweepDemand};

/// The scoring table the commit tier hands the measurer. Built here exactly as
/// `plinth_orient::search` builds its Exact rung, so the fragility and display
/// fields the landing scar reads are the fields the objective is scored with.
fn scoring_table(mesh: &plinth_mesh::Mesh) -> OrientFaceTable {
    OrientFaceTable::build(mesh, "exact").expect("a closed fixture always scores")
}

/// The identity pose of a T-shaped overhang fixture needs real support under
/// the down-facing slab, so a correctly wired measurer must report a
/// positive material cost, not merely succeed.
#[test]
fn the_sweep_demand_measures_a_real_overhang() {
    let mesh = make_overhang_table().expect("overhang table fixture");
    let results =
        SweepDemand::production().measure_support_demand(&mesh, &scoring_table(&mesh), &[UpDirection::MODEL_UP], DemandFidelity::Prefilter);

    assert_eq!(results.len(), 1, "the batch contract is index-parallel");
    let measured = results[0]
        .as_ref()
        .expect("a well-formed fixture must not fail the prefilter sweep");

    assert!(
        measured.support_area_cm2 > 0.0,
        "an overhang at identity needs real support material, got {}",
        measured.support_area_cm2
    );
    assert!(
        measured.struts > 0,
        "an overhang at identity needs at least one arm"
    );
    assert!(measured.layers > 0);
    assert!(measured.achieved_pitch_mm > 0.0);
    assert!(measured.total_scar_mm2 > 0.0);
    assert_eq!(measured.fidelity, DemandFidelity::Prefilter);
}

/// The burden is the arm count in cm², through the ONE fixed factor the
/// module doc states. Re-derived here from the registry rather than pinned to
/// a literal, so a change to `TIP_RADIUS_DEFAULT_MM` moves the term and the
/// test together and a change to the FORM of the bridge fails.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "an exact identity between two evaluations of the same closed-form expression, not a \
          geometric comparison. See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn the_burden_is_the_arm_count_through_one_fixed_unit_bridge() {
    let mesh = make_overhang_table().expect("overhang table fixture");
    let tip_radius_mm = m0_constants()
        .expect("registry")
        .placeholders()
        .tip_radius_default_mm()
        .diagnostic()
        .get();
    let results =
        SweepDemand::production().measure_support_demand(&mesh, &scoring_table(&mesh), &[UpDirection::MODEL_UP], DemandFidelity::Prefilter);
    let measured = results[0].as_ref().expect("a measurable fixture");

    assert_eq!(
        measured.support_area_cm2,
        burden_cm2(measured.struts, tip_radius_mm),
        "the burden must be the arm count times one fixed per-arm scar, in cm2"
    );
    // The bridge is a UNIT CONVERSION: it must be strictly monotone in the
    // count and free of any other input, or it would be ranking poses.
    assert!(burden_cm2(0, tip_radius_mm) == 0.0);
    assert!(burden_cm2(2, tip_radius_mm) > burden_cm2(1, tip_radius_mm));
    assert_eq!(
        burden_cm2(4, tip_radius_mm),
        2.0 * burden_cm2(2, tip_radius_mm),
        "a unit conversion is linear in the count it converts"
    );
}

/// The two fidelities are two different rasters, and the production one is
/// the registered working pitch. If they ever returned the same raster the
/// caller's second stage would be paying for nothing.
#[test]
fn the_production_fidelity_sweeps_a_finer_raster_than_the_prefilter() {
    let mesh = make_overhang_table().expect("overhang table fixture");
    let poses = [UpDirection::MODEL_UP];
    let prefilter = SweepDemand::production().measure_support_demand(&mesh, &scoring_table(&mesh), &poses, DemandFidelity::Prefilter);
    let production = SweepDemand::production().measure_support_demand(&mesh, &scoring_table(&mesh), &poses, DemandFidelity::Production);

    let coarse = prefilter[0].as_ref().expect("prefilter measures");
    let fine = production[0].as_ref().expect("production measures");
    assert!(
        fine.achieved_pitch_mm < coarse.achieved_pitch_mm,
        "the production raster ({} mm) must be finer than the prefilter's ({} mm)",
        fine.achieved_pitch_mm,
        coarse.achieved_pitch_mm,
    );
    assert_eq!(fine.fidelity, DemandFidelity::Production);
}

/// Two different candidate poses of the same mesh measure different burdens —
/// the whole point of replacing a single proxy number with a real
/// orientation-dependent measurement.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn different_poses_measure_different_burdens() {
    let mesh = make_overhang_table().expect("overhang table fixture");
    let poses = [
        UpDirection::MODEL_UP,
        UpDirection::new([1.0, 0.0, 0.0]).expect("unit x direction"),
    ];
    let measured = SweepDemand::production().measure_support_demand(&mesh, &scoring_table(&mesh), &poses, DemandFidelity::Prefilter);

    assert_eq!(measured.len(), poses.len());
    let identity = measured[0].as_ref().expect("identity pose must measure");
    let sideways = measured[1].as_ref().expect("a sideways pose must also measure");
    assert_ne!(
        identity.support_area_cm2, sideways.support_area_cm2,
        "a real measurement must depend on the candidate, unlike a fixed proxy"
    );
}
