//! **Remedy 2 buys no more flank arms than the stack asks for, and stops when
//! they relieve nothing.**
//!
//! Challenge of 2026-09-29 on the wall-creep lane
//! (`plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/lanes/wall-creep.md`):
//! Round 25 flipped came out of the congestion lane with ONE overlapping root
//! pair (congestion 1.0006, about 0.06 mm² of stacked flared plate area), and
//! remedy 2 answered it with **49** flank arms at shelf layer 144. Those 49
//! were exactly the 49 over-limit runs that then drew 83 |X| braces and 58
//! masts. The laid tube at its production pose showed the same defect: the
//! lean took the footprint to 1.0004, and remedy 2 still seated **84** flank
//! arms.
//!
//! The rule graded (`crates/plinth-sweep/src/congestion.rs`,
//! `insert_flank_arms`): a flank arm can take at most its own flared disc of
//! stacked plate area off the stacked roots, so the arms a round inserts are
//! sized by the stack the lean left, not by how many seats the shelf has.
//!
//! # The fixture: the laid tube at its AUTHORED pose (re-scoped 2026-09-29)
//!
//! This file first graded the tube at its production pose (a turn of −22.5°
//! about Y), where the lean left a stack smaller than one flank disc. Develop
//! has since changed the field under that pose: between `d103c48` (79 flank
//! arms at shelf 308 in the production run) and `75c52c8` (0 flank arms) the
//! lean alone clears it: 27 overlapping pairs before the lane, and the census
//! after the lean no longer reads congested (this harness: 1.0388 -> 1.0000,
//! 0 pairs), so remedy 2 never runs there and the file tested nothing
//! ("remedy 2 never ran"). No fixture in the suite now presents a SUB-disc
//! stack to remedy 2.
//!
//! The tube at its authored pose (the fixture `congestion_lane` grades) still
//! reaches remedy 2, measured 2026-09-29 on the develop-merged tree: the lean
//! leaves 2.58 mm² stacked, which is 3.25 flank discs of 0.792 mm². So the
//! claim is stated in its general form, of which "a sub-disc stack buys at
//! most one arm" is the special case: every flank arm beyond the one that
//! covers the last partial disc buys relief the plate has no stack for.
//!
//! The premise is read off the published census and the test fails loudly if
//! it does not hold. Nothing below recomputes the arm count the way
//! `insert_flank_arms` does; the bound is an inequality over the published
//! stacked areas and the published flank radius.

use crate::harness;

use harness::sweep_with;
use plinth_mesh::make_laid_tube;
use plinth_sweep::{StrutKind, SweepPolicy};

fn flare() -> f64 {
    plinth_core::m0_constants()
        .expect("the registry constructs")
        .placeholders()
        .flare_r_factor()
        .diagnostic()
        .get()
}

/// **A round inserts at most the flank arms whose flared discs the stack can
/// fill, plus the one that covers the last partial disc.** Summed over the
/// rounds remedy 2 ran: `inserted < Σ stacked / disc + rounds`.
#[test]
fn flank_arms_are_no_more_than_the_stack_asks_for() {
    let mesh = make_laid_tube().expect("the laid tube fixture");
    let policy = SweepPolicy::from_registry().expect("the registry supplies every sweep number");
    let plan = sweep_with(&mesh, 0.0, &policy);
    let lane = &plan.report.congestion_lane;

    let disc_mm2 = core::f64::consts::PI * (lane.flank_radius_used_mm * flare()).powi(2);
    let flank_members = plan
        .struts
        .iter()
        .filter(|s| lane.inserted_ids.contains(&s.id) && s.kind == StrutKind::Arm)
        .count();
    println!(
        "laid tube, authored pose: congestion {:.5} -> {:.5} at lane exit, pairs {} -> {}; \
         {} rounds, {} roots leaned; remedy 2 ran in {} round(s) on stacked areas {:?} mm2 \
         against one flank disc of {disc_mm2:.4} mm2; {} flank arms wanted, {} inserted \
         ({flank_members} on the finished field); {} members in all",
        lane.before.all.ratio,
        lane.at_lane_exit.all.ratio,
        lane.before.all.overlap_pairs,
        lane.at_lane_exit.all.overlap_pairs,
        lane.rounds_used,
        lane.arms_leaned,
        lane.flank_stacked_area_mm2.len(),
        lane.flank_stacked_area_mm2,
        lane.flank_arms_wanted,
        lane.arms_inserted,
        plan.struts.len(),
    );

    // Anti-vacuity: the fixture presents the defect's precondition, and
    // remedy 2 actually ran on it.
    assert!(
        lane.before.all.congested() && lane.arms_leaned > 0,
        "the fixture is not congested before the lane, or the lean never ran: {} / {}",
        lane.before.all.ratio,
        lane.arms_leaned
    );
    assert!(
        !lane.flank_stacked_area_mm2.is_empty() && lane.arms_inserted > 0,
        "remedy 2 never ran, so this file tests nothing"
    );
    assert!(disc_mm2 > 0.0, "no flank disc was registered");
    // The premise, from the published census: a real, finite stack each round.
    for (round, stacked) in lane.flank_stacked_area_mm2.iter().enumerate() {
        assert!(
            stacked.is_finite() && *stacked > 0.0,
            "round {round}: remedy 2 ran on {stacked} mm2 of stacked area"
        );
    }
    // The claim.
    let discs_of_stack: f64 = lane
        .flank_stacked_area_mm2
        .iter()
        .map(|stacked| stacked / disc_mm2)
        .sum();
    #[expect(
        clippy::cast_precision_loss,
        reason = "arm and round counts are small integers, exact in f64"
    )]
    let (inserted, rounds) = (
        lane.arms_inserted as f64,
        lane.flank_stacked_area_mm2.len() as f64,
    );
    assert!(
        inserted < discs_of_stack + rounds,
        "{} flank arms for {discs_of_stack:.3} flank discs of stacked area over {} round(s): \
         remedy 2 is sized by the seats, not by the stack",
        lane.arms_inserted,
        lane.flank_stacked_area_mm2.len()
    );
}

/// **Flank arms that relieved nothing are not followed by more.**
///
/// On the same fixture the flank arms of remedy 2's first round leave the
/// stack no smaller once the down-sweep has sized them (measured 2026-09-29:
/// one round on 2.58 mm², then two rounds declined). Remedy 2 runs again only
/// on a stack its previous arms made SMALLER, so every stack it ran on must be
/// strictly smaller than the one before, and on this fixture the lane must
/// have declined at least one round for that reason (the path-hit witness).
#[test]
fn flank_arms_that_relieved_nothing_are_not_repeated() {
    let mesh = make_laid_tube().expect("the laid tube fixture");
    let policy = SweepPolicy::from_registry().expect("the registry supplies every sweep number");
    let plan = sweep_with(&mesh, 0.0, &policy);
    let lane = &plan.report.congestion_lane;
    println!(
        "laid tube, authored pose: remedy 2 ran on stacks {:?} mm2, declined {} round(s) as \
         not relieving; {} flank arms, {} rounds, exit congestion {:.5}, {} members",
        lane.flank_stacked_area_mm2,
        lane.flank_rounds_not_relieving,
        lane.arms_inserted,
        lane.rounds_used,
        lane.at_lane_exit.all.ratio,
        plan.struts.len(),
    );
    assert!(
        !lane.flank_stacked_area_mm2.is_empty() && lane.arms_inserted > 0,
        "remedy 2 never ran, so this file tests nothing"
    );
    assert!(
        lane.at_lane_exit.all.congested(),
        "the fixture came out clean, so no flank arm was ever unable to relieve it"
    );
    for pair in lane.flank_stacked_area_mm2.windows(2) {
        assert!(
            pair[1] < pair[0],
            "remedy 2 ran again on {} mm2 after its arms left {} mm2: arms that relieved nothing \
             were followed by more ({:?})",
            pair[1],
            pair[0],
            lane.flank_stacked_area_mm2
        );
    }
    assert!(
        lane.flank_rounds_not_relieving > 0,
        "the stack never failed to shrink on this fixture, so the rule was never exercised"
    );
}
