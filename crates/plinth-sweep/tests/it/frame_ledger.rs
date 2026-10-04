//! **The brace frame's ledger is bookkeeping: the ladder decides the same
//! with it off** (band-speed lane, 2026-10-04).
//!
//! Round 13's frame bands (`brace_frame_bands`) classify every partner a run
//! could be braced to, by solving its panel, before asking whether any band
//! set can be planned on it. On the orientation search's demand runs nearly
//! every partner fails the planning, at every count, so nearly every solve
//! was spent on a refusal that no class could change (the toed slab's search
//! went from 38 s to over 37 minutes). With the ledger off
//! (`SweepPolicy::without_frame_ledger`, which the demand runs set) a pair no
//! band set can be planned on is set aside and classified only while it could
//! still be the pair a run is thickened into the window of.
//!
//! This is the equivalence test of that rule: the same sweep with the ledger
//! on and off builds the same struts, to the last field, on the four brace
//! fixtures (the Cylinder, the stilt plate, the brace pair, the laid tube),
//! and the off run really did leave pairs unclassified (a path hit, or the
//! comparison would be of two runs of the same code).

use plinth_mesh::{make_brace_pair, make_laid_tube, make_tall_stilt_plate, Mesh};
use plinth_sweep::{SweepPlan, SweepPolicy};

use crate::harness::sweep_with;
use crate::tapered_load_steps::cylinder;

/// The pairs the frame classified, whatever the class.
fn classified(plan: &SweepPlan) -> usize {
    let lane = &plan.report.slenderness_lane;
    lane.frame_pairs_past_ceiling + lane.frame_pairs_without_room + lane.frame_sets_refused_incomplete
}

/// Run `mesh` with the ledger on and off; return `(on, off)`.
fn both(mesh: &Mesh) -> (SweepPlan, SweepPlan) {
    let policy = SweepPolicy::from_registry().expect("the registry supplies every sweep number");
    let on = sweep_with(mesh, 0.0, &policy);
    let off = sweep_with(mesh, 0.0, &policy.without_frame_ledger());
    (on, off)
}

fn same_field(name: &str, mesh: &Mesh) -> (usize, usize) {
    let (on, off) = both(mesh);
    assert_eq!(on.struts.len(), off.struts.len(), "{name}: strut count");
    for (a, b) in on.struts.iter().zip(&off.struts) {
        assert_eq!(a, b, "{name}: strut {} differs with the ledger off", a.id);
    }
    let (lane_on, lane_off) = (&on.report.slenderness_lane, &off.report.slenderness_lane);
    for (what, a, b) in [
        ("braces added", lane_on.braces_added, lane_off.braces_added),
        ("band sets", lane_on.frame_band_sets, lane_off.frame_band_sets),
        ("members thickened", lane_on.members_thickened, lane_off.members_thickened),
        (
            "thickened into the window",
            lane_on.thickened_where_cheaper_than_bracing,
            lane_off.thickened_where_cheaper_than_bracing,
        ),
    ] {
        assert_eq!(a, b, "{name}: {what}");
    }
    println!(
        "{name}: {} struts, {} braces, {} band sets, {} thickened; pairs classified {} on, {} off",
        on.struts.len(),
        lane_on.braces_added,
        lane_on.frame_band_sets,
        lane_on.members_thickened,
        classified(&on),
        classified(&off)
    );
    (classified(&on), classified(&off))
}

/// The grade of one fixture: the same field, no pair classified that the
/// ledger-on run did not classify, and (the path hit) some pair left
/// unclassified, or the two runs would be one run of the same code twice.
fn graded(name: &str, mesh: &Mesh) {
    let (on, off) = same_field(name, mesh);
    assert!(off <= on, "{name}: the off run classifies no pair the on run does not");
    assert!(on > off, "{name}: path hit, with the ledger off some pair was set aside unclassified");
}

#[test]
fn the_ledger_off_builds_the_same_field_on_the_tall_stilt_plate() {
    graded("tall stilt plate", &make_tall_stilt_plate().expect("fixture"));
}

#[test]
fn the_ledger_off_builds_the_same_field_on_the_brace_pair() {
    graded("brace pair", &make_brace_pair().expect("fixture"));
}

#[test]
fn the_ledger_off_builds_the_same_field_on_the_laid_tube() {
    graded("laid tube", &make_laid_tube().expect("fixture"));
}

/// The Cylinder is the heaviest brace fixture (two full sweeps, about nine
/// minutes at four threads); run it by name for a milestone. Measured
/// 2026-10-04: 247 struts, 44 braces, 31 band sets, 51 thickened either way,
/// 6,697 pairs classified with the ledger on and 5 with it off.
#[test]
#[ignore = "two full Cylinder sweeps, about nine minutes; run by name at a milestone"]
fn the_ledger_off_builds_the_same_field_on_the_cylinder() {
    graded("cylinder", &cylinder());
}
