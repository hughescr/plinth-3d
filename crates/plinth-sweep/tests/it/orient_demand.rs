//! LANE 3 (`docs/plans/slice-sweep-core-2026-09-02.md` §12): the real
//! `plinth_orient::demand::SupportDemandMeasurer` implementation, exercised
//! end to end on a real fixture. `plinth-orient/tests/peel_demand_override.rs`
//! exercises the trait's own contract (the commit-tier override, its two
//! fidelities, its fallback, and the robustness/flip-crossover records)
//! through a test-local mock; this is the one test of the real geometry this
//! crate contributes.

use crate::harness::memo_run_sweep;
use plinth_core::{m0_constants, LayerPlan};
use plinth_mesh::{make_overhang_table, make_tall_stilt_plate};
use plinth_orient::demand::{DemandFidelity, SupportDemandMeasurer};
use plinth_orient::{OrientFaceTable, UpDirection};
use plinth_sweep::{burden_cm2, StrutKind, SweepDemand, SweepPlan, SweepPolicy};

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
#[cfg_attr(not(feature = "slow-tests"), ignore = "slow: scripts/gate-full.sh runs it")]
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

/// The same pose, swept directly, so a test can compare `MeasuredSupportDemand`
/// against the plan it was measured from. Reproduces `measure_one`'s own
/// layer-count arithmetic (`orient_demand.rs`), the same pattern
/// `tests/tip_contact_scar.rs`'s and `tests/model_landing_scar.rs`'s `swept`
/// helpers use, so this sweeps the exact pose the measurer swept.
fn swept_at(mesh: &plinth_mesh::Mesh, candidate: UpDirection) -> SweepPlan {
    let transformed =
        plinth_orient::oriented_recentered_mesh(mesh, candidate).expect("an orientable fixture");
    let constants = m0_constants().expect("the registry");
    let policy = SweepPolicy::from_registry().expect("the registered sweep policy");
    let lift_mm = constants.placeholders().lift_mm().diagnostic().get();
    let layer_height_mm = *constants.evidence().layer_height_default_mm().value();
    let max_z = transformed
        .verts()
        .iter()
        .fold(f64::NEG_INFINITY, |acc, vertex| acc.max(vertex[2]));
    let plate_z = -lift_mm;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the same layer-count arithmetic orient_demand.rs's measure_one performs, so this \
                  helper sweeps the pose the measurer swept"
    )]
    let count = (((max_z - plate_z) / layer_height_mm).ceil().max(1.0) as usize) + 1;
    let layers = LayerPlan::new(plate_z, vec![layer_height_mm; count]).expect("a layer plan");
    memo_run_sweep(&transformed, &layers, &policy).expect("a sweepable fixture")
}

/// **Anti-vacuity, and the sabotage this fix must survive.** The slenderness
/// lane (2026-09-05/06) added `StrutKind::Brace` and `StrutKind::Mast`
/// members to the strut set; neither touches a model face or leaves a scar,
/// so `MeasuredSupportDemand::struts` (the burden's own free variable) must
/// count only the members that seat a tip into the model, with the brace and
/// mast counts published beside it rather than folded in. Those members are
/// `StrutKind::Arm` and, since the D8 ruling (2026-10-03),
/// `StrutKind::Tie`: a model tie seats a tip into the model and leaves a
/// scar, so it is a clip like an arm (`StrutKind::seats_on_model`).
///
/// `make_tall_stilt_plate` at its own authored pose is the fixture
/// `tests/slenderness_lane.rs` uses to prove the ladder fires on a real
/// field, so it is used here at `UpDirection::MODEL_UP` to prove the SAME
/// thing about the counts the demand measurer reports. The anti-vacuity
/// assertion (braces or masts > 0) is checked FIRST: without it, a
/// regression back to counting every strut (`plan.struts.len()`) could pass
/// this test vacuously on a fixture that never grows a brace or a mast.
#[test]
#[cfg_attr(not(feature = "slow-tests"), ignore = "slow: scripts/gate-full.sh runs it")]
fn the_burden_counts_arms_and_model_ties_only_on_a_fixture_that_also_places_braces_and_masts() {
    let mesh = make_tall_stilt_plate().expect("the tall stilt plate fixture");
    let table = scoring_table(&mesh);
    let candidate = UpDirection::MODEL_UP;

    let plan = swept_at(&mesh, candidate);
    let arms_in_plan = plan
        .struts
        .iter()
        .filter(|strut| strut.kind == StrutKind::Arm)
        .count();
    let ties_in_plan = plan
        .struts
        .iter()
        .filter(|strut| strut.kind == StrutKind::Tie)
        .count();
    let braces_in_plan = plan
        .struts
        .iter()
        .filter(|strut| strut.kind == StrutKind::Brace)
        .count();
    let masts_in_plan = plan
        .struts
        .iter()
        .filter(|strut| strut.kind == StrutKind::Mast)
        .count();

    // Anti-vacuity: the fixture must actually exercise the slenderness lane,
    // or the equality checks below cannot tell "counts arms only" apart from
    // "counts everything" (they agree whenever braces + masts == 0).
    assert!(
        braces_in_plan > 0 || masts_in_plan > 0,
        "the stilt-plate fixture placed no braces ({braces_in_plan}) and no masts \
         ({masts_in_plan}) at its authored pose; this test cannot distinguish an arm-only \
         count from a total-strut count without at least one of either"
    );

    let results = SweepDemand::production().measure_support_demand(
        &mesh,
        &table,
        &[candidate],
        DemandFidelity::Production,
    );
    let measured = results[0].as_ref().expect("a well-formed fixture measures");

    // The sabotage this guards against: reverting to `plan.struts.len()`
    // (every kind) would make `measured.struts` equal
    // `arms_in_plan + ties_in_plan + braces_in_plan + masts_in_plan`, which
    // the anti-vacuity assertion above guarantees is strictly greater than
    // the model-seated count alone -- so this assertion goes red under that
    // regression. Where the fixture grows a model tie, dropping ties from the
    // count (`== StrutKind::Arm`) goes red here too.
    assert_eq!(
        measured.struts,
        arms_in_plan + ties_in_plan,
        "the measured burden's strut count must be the model-seated count alone (arms \
         {arms_in_plan} + model ties {ties_in_plan}), not the total strut count (braces \
         {braces_in_plan}, masts {masts_in_plan})"
    );
    assert_eq!(
        measured.braces, braces_in_plan,
        "the brace count published beside the burden must match the plan's own braces"
    );
    assert_eq!(
        measured.masts, masts_in_plan,
        "the mast count published beside the burden must match the plan's own masts"
    );
}
