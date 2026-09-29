//! **A footprint stacked by less than one flank arm does not buy dozens.**
//!
//! Challenge of 2026-09-29 on the wall-creep lane
//! (`plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/lanes/wall-creep.md`):
//! Round 25 flipped came out of the congestion lane with ONE overlapping root
//! pair (congestion 1.0006, about 0.06 mm² of stacked flared plate area), and
//! remedy 2 answered it with **49** flank arms at shelf layer 144. Those 49
//! were exactly the 49 over-limit runs that then drew 83 |X| braces and 58
//! masts. The laid tube at its production pose shows the same defect: the lean
//! takes the footprint to 1.0004, and remedy 2 still seats **84** flank arms.
//!
//! The rule graded (`crates/plinth-sweep/src/congestion.rs`,
//! `insert_flank_arms`): a flank arm can take at most its own flared disc of
//! stacked plate area off the stacked roots, so the arms a round inserts are
//! sized by the stack the lean left, not by how many seats the shelf has.
//!
//! # What is measured
//!
//! The fixture is `make_laid_tube` turned to the pose the production CLI's
//! dense orientation search commits for it, "searched up-direction (0.3827,
//! 0.0000, 0.9239)": a turn of −22.5° about Y. Measured, this harness then
//! reproduces the CLI's own congestion line for the tube (1.0458 before, 23
//! roots leaned, 84 flank arms at shelf layer 308, flank radius 0.2264 mm).
//!
//! The premise is read off the published census, and the test fails loudly if
//! it does not hold: in every round remedy 2 ran, the stacked area it was
//! called for is SMALLER than one flank arm's flared disc. The claim is the
//! physical consequence: such a round needs one arm, so the lane inserts at
//! most one flank arm per round it ran. Nothing below recomputes the arm count
//! the way `insert_flank_arms` does.

use crate::harness;

use harness::sweep_with;
use plinth_mesh::{make_laid_tube, Mesh};
use plinth_sweep::{StrutKind, SweepPolicy};

/// The production pose: a rigid turn about Y that takes the CLI's searched
/// up-direction `(sin 22.5°, 0, cos 22.5°)` to `+Z`.
fn about_y(mesh: &Mesh, deg: f64) -> Mesh {
    let (s, c) = deg.to_radians().sin_cos();
    let verts = mesh
        .verts()
        .iter()
        .map(|v| [v[0] * c + v[2] * s, v[1], -v[0] * s + v[2] * c])
        .collect();
    Mesh::new(verts, mesh.tris().to_vec()).expect("a rigid rotation preserves a valid mesh")
}

fn flare() -> f64 {
    plinth_core::m0_constants()
        .expect("the registry constructs")
        .placeholders()
        .flare_r_factor()
        .diagnostic()
        .get()
}

#[test]
fn a_sub_disc_stack_buys_at_most_one_flank_arm_per_round() {
    let mesh = about_y(&make_laid_tube().expect("the laid tube fixture"), -22.5);
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
        "laid tube, production pose: congestion {:.5} -> {:.5} at lane exit, pairs {} -> {}; \
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
    // The premise, from the published census.
    for (round, stacked) in lane.flank_stacked_area_mm2.iter().enumerate() {
        assert!(
            *stacked > 0.0 && *stacked < disc_mm2,
            "round {round}: remedy 2 ran on {stacked} mm2 of stacked area, not a sub-disc stack \
             against {disc_mm2} mm2; the fixture no longer presents the case this file grades"
        );
    }
    // The claim.
    assert!(
        lane.arms_inserted <= lane.flank_stacked_area_mm2.len(),
        "{} flank arms for {} round(s) in which the stack was smaller than one flank arm's own \
         disc: remedy 2 is sized by the seats, not by the stack",
        lane.arms_inserted,
        lane.flank_stacked_area_mm2.len()
    );
}

/// **Flank arms that relieved nothing are not followed by more.**
///
/// On the same fixture the lean leaves one overlapping pair that no flank arm
/// clears: the stacked area after each round's sizing is the same 0.06 mm²
/// (measured, 2026-09-29, in all four rounds). Remedy 2 runs again only on a
/// stack its previous arms made SMALLER, so every stack it ran on must be
/// strictly smaller than the one before, and on this fixture the lane must
/// have declined at least one round for that reason (the path-hit witness).
#[test]
fn flank_arms_that_relieved_nothing_are_not_repeated() {
    let mesh = about_y(&make_laid_tube().expect("the laid tube fixture"), -22.5);
    let policy = SweepPolicy::from_registry().expect("the registry supplies every sweep number");
    let plan = sweep_with(&mesh, 0.0, &policy);
    let lane = &plan.report.congestion_lane;
    println!(
        "laid tube, production pose: remedy 2 ran on stacks {:?} mm2, declined {} round(s) as \
         not relieving; {} flank arms, {} rounds, exit congestion {:.5}, {} members",
        lane.flank_stacked_area_mm2,
        lane.flank_rounds_not_relieving,
        lane.arms_inserted,
        lane.rounds_used,
        lane.at_lane_exit.all.ratio,
        plan.struts.len(),
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
