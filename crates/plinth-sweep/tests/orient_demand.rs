//! LANE 3 (`docs/plans/slice-sweep-core-2026-09-02.md` §12): the real
//! `plinth_orient::demand::SupportDemandMeasurer` implementation, exercised
//! end to end on a real fixture. `plinth-orient/tests/peel_demand_override.rs`
//! exercises the trait's own contract (the commit-tier override, its
//! fallback, and the robustness/flip-crossover records) through a test-local
//! mock; this is the one test of the real geometry this crate contributes.

use plinth_mesh::make_overhang_table;
use plinth_orient::demand::SupportDemandMeasurer;
use plinth_orient::UpDirection;
use plinth_sweep::CoarseSweepDemand;

/// The identity pose of a T-shaped overhang fixture needs real support under
/// the down-facing slab, so a correctly wired measurer must report a
/// positive material cost, not merely succeed.
#[test]
fn the_coarse_sweep_demand_measures_a_real_overhang() {
    let mesh = make_overhang_table().expect("overhang table fixture");
    let measurer = CoarseSweepDemand;
    let measured = measurer
        .measure_support_area_cm2(&mesh, UpDirection::MODEL_UP)
        .expect("a well-formed fixture must not fail the coarse-raster sweep");

    assert!(
        measured.support_area_cm2 > 0.0,
        "an overhang at identity needs real support material, got {}",
        measured.support_area_cm2
    );
    assert!(measured.struts > 0, "an overhang at identity needs at least one arm");
    assert!(measured.layers > 0);
    assert!(measured.achieved_pitch_mm > 0.0);
}

/// Two different candidate poses of the same mesh measure different demand —
/// the whole point of replacing a single proxy number with a real
/// orientation-dependent measurement.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn different_poses_measure_different_demand() {
    let mesh = make_overhang_table().expect("overhang table fixture");
    let measurer = CoarseSweepDemand;
    let identity = measurer
        .measure_support_area_cm2(&mesh, UpDirection::MODEL_UP)
        .expect("identity pose must measure");
    let sideways = measurer
        .measure_support_area_cm2(
            &mesh,
            UpDirection::new([1.0, 0.0, 0.0]).expect("unit x direction"),
        )
        .expect("a sideways pose must also measure");

    assert_ne!(
        identity.support_area_cm2, sideways.support_area_cm2,
        "a real measurement must depend on the candidate, unlike a fixed proxy"
    );
}
