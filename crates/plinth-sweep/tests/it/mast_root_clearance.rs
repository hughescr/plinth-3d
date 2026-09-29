//! **A mast is plate-rooted, and its root flare has to clear the roots that
//! are already there** — the follow-up to Lane 3's own finding of 2026-09-06,
//! that after the congestion lane had spent four rounds spreading a footprint,
//! the SLENDERNESS lane put masts back on top of it.
//!
//! The defect was one line of reasoning, not one line of code: the mast search
//! asked whether a plate cell was EMPTY (`shaft_head == u32::MAX`), which is a
//! question about a single cell, and then landed a disc of
//! `FLARE_R_FACTOR × r_m` on it. A cell one grid pitch from a 0.6 mm root is
//! empty and is not clear.
//!
//! What is graded here:
//!
//! * every mast's flared disc clears every other plate root's flared disc, on
//!   the two fixtures that place masts and stay clear (asserted non-vacuous),
//!   and the one that does NOT stay clear publishes what it leaves;
//! * with the clearance test deleted, 127 overlapping mast pairs come back on
//!   the laid tube;
//! * on the laid tube the FINISHED field is no more congested than the
//!   congestion lane's own exit, which is the claim Lane 3 could not make;
//! * the flank arms' pre-sizing radius estimate leaves no overlapping root
//!   behind once the down-sweep has sized the field.

use crate::harness;

use harness::sweep_with;
use plinth_mesh::{make_brace_pair, make_laid_tube, make_lonely_peak, Mesh};
use plinth_sweep::{Landing, StrutKind, SweepPlan, SweepPolicy, SweepStrut};

fn policy() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every number the sweep reads")
}

/// The registered root flare, read from the registry here rather than from the
/// crate under test, so a wrong factor in the code cannot agree with itself.
fn flare_factor() -> f64 {
    policy().flare_r_factor
}

/// Every plate root's centre and FLARED radius, in millimetres — the disc the
/// emitter actually lays. A brace gets no root flare and never lands on the
/// plate, so it is not one.
fn flared_roots(plan: &SweepPlan) -> Vec<(u32, StrutKind, f64, f64, f64)> {
    let flare = flare_factor();
    plan.struts
        .iter()
        .filter(|strut| matches!(strut.landing, Landing::Plate) && strut.kind != StrutKind::Brace)
        .filter_map(|strut: &SweepStrut| {
            let root = *strut.radius_mm.first()?;
            (root.is_finite() && root > 0.0).then(|| {
                let cell = strut.base_cell();
                (
                    strut.id,
                    strut.kind,
                    plan.grid.center_x_mm(plan.grid.col_of(cell)),
                    plan.grid.center_y_mm(plan.grid.row_of(cell)),
                    root * flare,
                )
            })
        })
        .collect()
}

/// Unordered pairs of plate roots whose flared discs actually intersect, with
/// at least one of them a mast. `O(n²)` on purpose: a test that re-implemented
/// the production sweep line would grade its own copy of the algorithm rather
/// than the field.
fn overlapping_pairs_touching_a_mast(plan: &SweepPlan) -> Vec<(u32, u32, f64, f64)> {
    let roots = flared_roots(plan);
    let mut out = Vec::new();
    for (slot, first) in roots.iter().enumerate() {
        for second in &roots[slot + 1..] {
            if first.1 != StrutKind::Mast && second.1 != StrutKind::Mast {
                continue;
            }
            let dx = second.2 - first.2;
            let dy = second.3 - first.3;
            let centres = dx.hypot(dy);
            let reach = first.4 + second.4;
            if centres < reach {
                out.push((first.0, second.0, centres, reach));
            }
        }
    }
    out
}

fn masts(plan: &SweepPlan) -> usize {
    plan.struts
        .iter()
        .filter(|strut| strut.kind == StrutKind::Mast)
        .count()
}

/// The two slenderness fixtures at their AUTHORED pose — the harness runs a
/// mesh exactly as it is given, with no orientation search, which is what makes
/// these two place masts at all.
fn lonely_peak() -> Mesh {
    make_lonely_peak().expect("the lonely peak fixture")
}

fn brace_pair() -> Mesh {
    make_brace_pair().expect("the brace pair fixture")
}

fn laid_tube() -> Mesh {
    make_laid_tube().expect("the laid tube fixture")
}

/// **THE CLAIM.** No mast's flared disc laps over any plate root's flared disc,
/// on the finished, SIZED field.
///
/// Asserted pairwise over the real field, and asserted NON-VACUOUS: a fixture
/// that placed no mast would pass this trivially, so the mast count is graded
/// first and by name.
#[test]
fn every_mast_clears_every_plate_root_on_the_brace_pair() {
    let plan = sweep_with(&brace_pair(), 0.0, &policy());
    let placed = masts(&plan);
    assert!(
        placed > 0,
        "anti-vacuity: make_brace_pair must place at least one mast for the clearance claim to \
         mean anything; it placed {placed}"
    );
    let overlaps = overlapping_pairs_touching_a_mast(&plan);
    assert!(
        overlaps.is_empty(),
        "{placed} masts placed and {} of them lap over a plate root: {overlaps:?} \
         (id, id, centre distance mm, sum of flared radii mm)",
        overlaps.len()
    );
}

/// The same claim on the fixture that places FIFTY-THREE masts, which is where
/// the rule is actually load-bearing.
#[test]
fn every_mast_clears_every_plate_root_on_the_laid_tube() {
    let plan = sweep_with(&laid_tube(), 0.0, &policy());
    let placed = masts(&plan);
    assert!(
        placed > 10,
        "anti-vacuity: make_laid_tube must place many masts for this to be the load-bearing case; \
         it placed {placed}"
    );
    let overlaps = overlapping_pairs_touching_a_mast(&plan);
    assert!(
        overlaps.is_empty(),
        "{placed} masts placed and {} of them lap over a plate root: {overlaps:?}",
        overlaps.len()
    );
}

/// **WHERE THE CLAIM IS FALSE, SAID OUT LOUD.** `make_lonely_peak` finishes
/// with two lapping mast roots, and it finishes with exactly the same two
/// whether the clearance rule is in force or deleted.
///
/// That is not a placement failure: both cells were clear at the radius the
/// mast had when they were chosen (`r_m = max(r_min, H_first/S)`), and the
/// down-sweep afterwards sized both roots larger than the radius the search
/// was given. The residual is the SAME class of defect one level up — a disc
/// registered before its size is known — and it is published as
/// `mast_roots_overlapping_after_sizing` rather than asserted away.
///
/// This test grades HONESTY, not the count: the number the report publishes
/// must equal the number the field actually has. A future change that closes
/// the residual makes both zero and this test stays green.
#[test]
fn the_lonely_peak_publishes_the_mast_laps_its_sizing_pass_leaves() {
    let plan = sweep_with(&lonely_peak(), 0.0, &policy());
    assert!(
        masts(&plan) > 0,
        "anti-vacuity: make_lonely_peak must place a mast"
    );
    let measured = overlapping_pairs_touching_a_mast(&plan);
    let published = plan
        .report
        .slenderness_lane
        .mast_roots_overlapping_after_sizing;
    // The published figure counts MASTS that lap; the pairwise measurement
    // counts PAIRS. A lapping mast is in at least one pair, so a non-zero
    // measurement demands a non-zero publication and the other way round.
    assert_eq!(
        published == 0,
        measured.is_empty(),
        "the report publishes {published} lapping mast roots and the field has {} lapping pairs: \
         {measured:?}",
        measured.len()
    );
}

/// **THE SABOTAGE DRILL.** Delete the clearance rule and the defect comes back.
///
/// A green clearance test only means something if the check is what makes it
/// green. `without_mast_root_clearance` restores the pre-2026-09-07 per-cell
/// occupancy test — is this cell empty — and nothing else.
///
/// **The fixture is `make_laid_tube`, and it was CHOSEN BY MEASUREMENT.**
/// `make_tall_stilt_plate` was tried first and is not a witness: it places one
/// mast and leaves zero lapping mast pairs with the rule on AND off, so it
/// cannot tell the two engines apart. That is reported rather than hidden. The
/// laid tube can: 0 lapping mast pairs in force, 127 with the rule deleted.
#[test]
fn deleting_the_clearance_rule_brings_the_overlapping_masts_back() {
    let mesh = laid_tube();
    let clean = sweep_with(&mesh, 0.0, &policy());
    let sabotaged = sweep_with(&mesh, 0.0, &policy().without_mast_root_clearance());

    let clean_masts = masts(&clean);
    let sabotaged_masts = masts(&sabotaged);
    assert!(
        clean_masts > 0 && sabotaged_masts > 0,
        "anti-vacuity: both fields must place masts; production {clean_masts}, \
         sabotaged {sabotaged_masts}"
    );

    let clean_overlaps = overlapping_pairs_touching_a_mast(&clean);
    let sabotaged_overlaps = overlapping_pairs_touching_a_mast(&sabotaged);
    assert!(
        clean_overlaps.is_empty(),
        "production placed {clean_masts} masts and {} of them lap: {clean_overlaps:?}",
        clean_overlaps.len()
    );
    assert!(
        sabotaged_overlaps.len() > 10,
        "the sabotage drill deleted the clearance rule and only {} pairs overlapped \
         ({sabotaged_masts} masts placed). Either this fixture no longer exercises the rule, or \
         the rule is not what is keeping the production field clean — say which before believing \
         the green test above.",
        sabotaged_overlaps.len()
    );

    // And the ratio the owner reads in ChituBox moves with it.
    assert!(
        clean.report.congestion_lane.after.all.overlap_pairs
            < sabotaged.report.congestion_lane.after.all.overlap_pairs,
        "finished overlapping root pairs: {} in force against {} with the rule deleted",
        clean.report.congestion_lane.after.all.overlap_pairs,
        sabotaged.report.congestion_lane.after.all.overlap_pairs,
    );
}

/// **WHAT LANE 3 COULD NOT CLAIM.** The finished field is no more congested
/// than the congestion lane's own exit.
///
/// At 3ff52c7 the laid tube left the congestion lane at `1.0004` with 2
/// overlapping pairs and its FINISHED file read `1.0926` with 155, every bit of
/// the difference put there by masts. The claim graded here is the one that
/// change makes available: the finished ratio is within a stated tolerance of
/// the exit ratio, rather than a tenth above it.
///
/// **The tolerance is a TEST TOLERANCE and not a registered constant.** `0.01`
/// is one percent of the ratio, which is an order of magnitude below the
/// `0.0922` the defect used to add and an order of magnitude above the rounding
/// of a rasterised area.
#[test]
fn the_finished_laid_tube_is_no_worse_than_the_lane_left_it() {
    const RATIO_TOLERANCE: f64 = 0.01;

    let plan = sweep_with(&laid_tube(), 0.0, &policy());
    let lane = &plan.report.congestion_lane;

    // Anti-vacuity: the lane has to have run and the slenderness lane has to
    // have placed masts, or there is nothing between the two censuses.
    assert!(
        lane.rounds_used > 0,
        "anti-vacuity: the congestion lane ran zero rounds on the laid tube"
    );
    assert!(
        plan.report.slenderness_lane.masts_dropped > 0,
        "anti-vacuity: no mast was placed, so the finished census cannot differ from the exit one \
         for the reason this test exists"
    );

    let exit = lane.at_lane_exit.all.ratio;
    let finished = lane.after.all.ratio;
    assert!(
        finished <= exit + RATIO_TOLERANCE,
        "the finished field is congested at {finished:.4} against a lane exit of {exit:.4}, more \
         than the {RATIO_TOLERANCE} tolerance; the masts are back on top of the footprint"
    );

    // And the pair count, which is the number that matches what the eye sees.
    let exit_pairs = lane.at_lane_exit.all.overlap_pairs;
    let finished_pairs = lane.after.all.overlap_pairs;
    assert!(
        finished_pairs <= exit_pairs * 4,
        "overlapping root pairs went {exit_pairs} at the lane exit -> {finished_pairs} finished"
    );
}

/// **THE FLANK ARMS' PRE-SIZING ESTIMATE, GRADED ON THE SIZED FIELD.**
///
/// A flank arm's plate cell is chosen before the arm exists, so its root radius
/// is an ESTIMATE; until 2026-09-06 the estimate was the smallest printable
/// radius, which is optimistic by construction and let the sizing pass grow the
/// discs into one another. The count is published either way — this asserts it
/// is zero, and prints it when it is not so the number is never silent.
#[test]
fn the_flank_arms_leave_no_overlapping_root_once_the_field_is_sized() {
    let plan = sweep_with(&laid_tube(), 0.0, &policy());
    let lane = &plan.report.congestion_lane;
    assert!(
        lane.arms_inserted > 0,
        "anti-vacuity: no flank arm was inserted on the laid tube, so the estimate was never used"
    );
    assert!(
        lane.flank_radius_used_mm > 0.0,
        "the estimate in force must be published: it read {}",
        lane.flank_radius_used_mm
    );
    assert_eq!(
        lane.flank_roots_overlapping_after_sizing, 0,
        "{} of the {} inserted flank roots still overlap another plate root once the down-sweep \
         has sized them (estimate in force {:.4} mm; mean placed root {:.4} mm; hex-cell area law \
         {:.4} mm)",
        lane.flank_roots_overlapping_after_sizing,
        lane.arms_inserted,
        lane.flank_radius_used_mm,
        lane.flank_radius_mean_placed_mm,
        lane.flank_radius_area_law_mm,
    );
}

/// **`shaft_spans_recorded` DESCRIBES THE FINISHED FIELD.**
///
/// It used to be read off the UP-SWEEP's column index, which nothing writes to
/// after the descent, the congestion lane and the slenderness lane have each
/// built their own. On a field where those lanes add members it was reporting a
/// pre-descent number. A field with more members than the up-sweep placed must
/// record at least as many spans as it has members.
#[test]
fn the_shaft_span_count_is_read_from_the_finished_field() {
    let plan = sweep_with(&laid_tube(), 0.0, &policy());
    let members = plan
        .struts
        .iter()
        .filter(|strut| strut.k_bottom != usize::MAX && strut.k_top >= strut.k_bottom)
        .count();
    assert!(
        plan.report.shaft_spans_recorded >= members,
        "{} shaft spans recorded against {members} members that have a shaft; the counter is \
         being read off an index the finished field never reached",
        plan.report.shaft_spans_recorded
    );
}
