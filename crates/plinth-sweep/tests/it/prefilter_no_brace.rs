//! **The orientation search's quick pass skips bracing** (Craig ruling
//! 2026-10-04, "Skip bracing in the quick pass").
//!
//! The search ranks hundreds of candidate poses with a cheap prefilter and
//! fully measures the top few. Since round 13 the prefilter ran the
//! slenderness ties (`apply_ties`: the `|X|` crosses' frame solves, the D9
//! re-checks, thickening, masts, model ties) for every candidate, which took
//! the toed slab's process tests from 38 s to 765 s. The rule now:
//!
//! * a PREFILTER measurement runs no tie pass (`SweepPolicy::
//!   without_slenderness_ties`), so it reports 0 braces, 0 masts and no model
//!   tie in its arm count;
//! * a PRODUCTION measurement (the finalists) and the emitted field keep every
//!   rung, exactly as before.
//!
//! What each test pins, and how it fails:
//!
//! | test | red when |
//! |---|---|
//! | `a_sweep_without_ties_places_no_brace_mast_or_tie` | the knob is not read (the loop still runs), or a rung ignores it |
//! | `the_same_sweep_with_ties_places_some` | the fixtures stop exercising the ladder, which would make the test above vacuous |
//! | `the_prefilter_measures_no_brace_mast_or_tie` | `DemandSettings` stops switching the ties off for `Prefilter` |
//! | `the_production_measurement_keeps_every_rung` | the ties are switched off for `Production` too (the sabotage that would leave the final stage unbraced) |

use plinth_core::{m0_constants, LayerPlan};
use plinth_mesh::{make_brace_pair, make_tall_stilt_plate, Mesh};
use plinth_orient::demand::{DemandFidelity, SupportDemandMeasurer};
use plinth_orient::{OrientFaceTable, UpDirection};
use plinth_sweep::{StrutKind, SweepDemand, SweepPlan, SweepPolicy};

use crate::harness::{memo_run_sweep, sweep_with};

fn count(plan: &SweepPlan, kind: StrutKind) -> usize {
    plan.struts.iter().filter(|strut| strut.kind == kind).count()
}

fn fixtures() -> [(&'static str, Mesh); 2] {
    [
        ("tall stilt plate", make_tall_stilt_plate().expect("fixture")),
        ("brace pair", make_brace_pair().expect("fixture")),
    ]
}

#[test]
fn a_sweep_without_ties_places_no_brace_mast_or_tie() {
    let policy = SweepPolicy::from_registry().expect("registry").without_slenderness_ties();
    for (name, mesh) in fixtures() {
        let plan = sweep_with(&mesh, 0.0, &policy);
        for kind in [StrutKind::Brace, StrutKind::Mast, StrutKind::Tie] {
            assert_eq!(count(&plan, kind), 0, "{name}: {kind:?} members with the ties off");
        }
        let lane = &plan.report.slenderness_lane;
        assert_eq!(lane.tie_passes, 0, "{name}: tie passes with the ties off");
        assert_eq!(lane.braces_added, 0, "{name}: braces added");
        assert_eq!(lane.members_thickened, 0, "{name}: members thickened");
        assert!(plan.struts.iter().any(|s| s.kind == StrutKind::Arm), "{name}: arms still placed");
    }
}

#[test]
fn the_same_sweep_with_ties_places_some() {
    let policy = SweepPolicy::from_registry().expect("registry");
    for (name, mesh) in fixtures() {
        let plan = sweep_with(&mesh, 0.0, &policy);
        let extra = count(&plan, StrutKind::Brace) + count(&plan, StrutKind::Mast) + count(&plan, StrutKind::Tie);
        assert!(extra > 0, "{name}: path hit; the ladder placed nothing, so the ties-off test proves nothing");
        assert!(plan.report.slenderness_lane.tie_passes > 0, "{name}: tie passes ran");
    }
}

/// The layer plan `orient_demand.rs`'s `measure_one` builds for a pose.
fn demand_sweep_up(mesh: &Mesh, policy: &SweepPolicy, up: UpDirection) -> SweepPlan {
    let constants = m0_constants().expect("the registry");
    let lift_mm = constants.placeholders().lift_mm().diagnostic().get();
    let height = *constants.evidence().layer_height_default_mm().value();
    let transformed = plinth_orient::oriented_recentered_mesh(mesh, up).expect("orientable");
    let max_z = transformed.verts().iter().fold(f64::NEG_INFINITY, |acc, v| acc.max(v[2]));
    let plate_z = -lift_mm;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the same layer-count arithmetic orient_demand.rs's measure_one performs"
    )]
    let count = (((max_z - plate_z) / height).ceil().max(1.0) as usize) + 1;
    let layers = LayerPlan::new(plate_z, vec![height; count]).expect("a layer plan");
    memo_run_sweep(&transformed, &layers, policy).expect("a sweepable fixture")
}

/// A pose at which the tall stilt plate's COARSE sweep (the prefilter's own
/// raster) grows braces and model ties with the ladder on: measured
/// 2026-10-04, 4 braces and 11 ties, 349 arms (`[0, 1, 1]` is the model's
/// +Y/+Z diagonal turned up). The authored pose grows none at that raster, so
/// a test there would pass whatever the knob did.
const TILTED: [f64; 3] = [0.0, 1.0, 1.0];

#[test]
fn the_prefilter_measures_no_brace_mast_or_tie() {
    let mesh = make_tall_stilt_plate().expect("fixture");
    let up = UpDirection::new(TILTED).expect("a direction");
    // The prefilter reads no drawn solid of a descent (its raster's half
    // diagonal, 0.35 mm, exceeds the 0.3 mm gap: `orient_demand.rs`); the
    // by-hand sweeps here are taken under the same policy so they stay the
    // prefilter's own field (fix/drawn-solid-clearance, 2026-10-04).
    let base = SweepPolicy::from_registry().expect("registry").without_drawn_solid_descent();
    let coarse = base.with_working_pitch_mm(base.working_pitch_mm * 10.0).without_frame_ledger();

    // PATH HIT: at the prefilter's own coarse raster the full ladder does
    // place braces and ties at this pose, so the zeros below are the knob's
    // doing and not the raster's.
    //
    // RE-PINNED 2026-10-05 (challenger round 2 of fix/drawn-solid-clearance):
    // the witness ladder also runs without the member-against-model clearance.
    // The thickening refusal now compares the plan with the profile drawn at
    // each slot (it no longer excuses a slot because a wider one stands
    // elsewhere in the piece), and at this raster the half-diagonal term
    // (0.35 mm) exceeds the 0.3 mm gap, so it refused 151 thickenings and the
    // ladder placed 0 braces and 0 ties here (was 4 and 11). The production
    // prefilter runs no tie pass and reads neither rule; the witness only has
    // to show that a ladder places braces and ties at this pose.
    let full = demand_sweep_up(&mesh, &coarse.without_member_clearance(), up);
    let (full_braces, full_ties) = (count(&full, StrutKind::Brace), count(&full, StrutKind::Tie));
    println!("coarse sweep, ties on: {full_braces} braces, {full_ties} ties, {} arms", count(&full, StrutKind::Arm));
    let lane = &full.report.slenderness_lane;
    println!(
        "slenderness lane: tie passes {}, thickened {}, thickenings refused (radius {}, disc {}, model {}), braces added {}",
        lane.tie_passes,
        lane.members_thickened,
        lane.thicken_refused_radius,
        lane.thicken_refused_disc,
        lane.thicken_refused_model,
        lane.braces_added
    );
    assert!(full_braces > 0 && full_ties > 0, "path hit: the full coarse ladder placed {full_braces} braces, {full_ties} ties");

    let table = OrientFaceTable::build(&mesh, "exact").expect("a closed fixture scores");
    let results = SweepDemand::production().measure_support_demand(&mesh, &table, &[up], DemandFidelity::Prefilter);
    let measured = results[0].as_ref().expect("a well-formed fixture measures");
    assert_eq!(measured.braces, 0, "prefilter braces");
    assert_eq!(measured.masts, 0, "prefilter masts");

    // The arm count is the arms of the same coarse sweep with no ties: no
    // model tie inside it (the full run's burden would be arms + ties).
    let by_hand = demand_sweep_up(&mesh, &coarse.without_slenderness_ties(), up);
    assert_eq!(count(&by_hand, StrutKind::Tie), 0, "no model tie in the prefilter's count");
    assert_eq!(measured.struts, count(&by_hand, StrutKind::Arm), "prefilter arm count is the arms of the tie-less coarse sweep");
}

#[test]
fn the_production_measurement_keeps_every_rung() {
    // The stilt plate at its authored pose grows braces or masts (the burden
    // test in `orient_demand.rs` relies on it); a production measurement must
    // report exactly what the full ladder placed.
    let mesh = make_tall_stilt_plate().expect("fixture");
    let table = OrientFaceTable::build(&mesh, "exact").expect("scores");
    let results = SweepDemand::production().measure_support_demand(
        &mesh,
        &table,
        &[UpDirection::MODEL_UP],
        DemandFidelity::Production,
    );
    let measured = results[0].as_ref().expect("measures");
    assert!(
        measured.braces + measured.masts > 0,
        "production measured {} braces {} masts: the final stage is unbraced",
        measured.braces,
        measured.masts
    );
}
