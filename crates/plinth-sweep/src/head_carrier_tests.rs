//! **Every pass except the producer honours heads** (lane `head-carrier`,
//! 2026-10-06). One hand-made headed field per pass family. No head is
//! produced yet, so each fixture sets `head` and the matching path by hand.
//!
//! For each family the test asserts that (a) the pass FIRED on the headed arm
//! (`headed_kept >= 1`, or for the un-merge the restore happened), (b) the
//! landing ledger reads `heads_not_intact` 0 and `joins_above_joint` 0, and (c)
//! every head field is bit-identical before and after.
//!
//! SABOTAGE (each family): remove that pass's guard, or stamp with plain
//! `stamp_shaft`; the named assertion goes red.

use std::collections::HashSet;

use crate::grid::{lay_out_at, Grid};
use crate::heads::Head;
use crate::landing_check::check_landings;
use crate::up_sweep::ColumnHistory;
use crate::{Landing, SeatFace, StrutKind, SweepStrut};

pub(crate) const H: f64 = 0.05;
const SIDE: usize = 300;
/// `ceil(0.2204 / 0.05)`: the joint's clearance in layers.
const JOINT_CLEAR: usize = 5;

pub(crate) fn grid() -> Grid {
    lay_out_at(0.0, 0.0, H, SIDE, SIDE)
}

fn cell_u32(cell: usize) -> u32 {
    u32::try_from(cell).expect("a small grid")
}

/// A straight plate arm at `(row, col)` standing over layers `0..=k_top`.
pub(crate) fn straight(
    grid: &Grid,
    id: u32,
    row: usize,
    col: usize,
    k_top: usize,
    radius: f64,
) -> SweepStrut {
    SweepStrut {
        id,
        cell: grid.index(row, col),
        center_mm: [grid.center_x_mm(col), grid.center_y_mm(row)],
        kind: StrutKind::Arm,
        min_radius_mm: 0.0,
        thickened_mm: Vec::new(), foot_radius_mm: None,
        seat_on_strut: None,
        k_top,
        k_bottom: 0,
        landing: Landing::Plate,
        interior: false,
        radius_mm: vec![radius; k_top + 1],
        carried_mm2: vec![0.0; k_top + 1],
        covered_demand: Vec::new(), held_births: Vec::new(), twin_host: false,
        inserted_by_load: false,
        path: Vec::new(),
        seat_face: SeatFace::default(),
        seat_normal: [0.0; 3],
        straight_fallback: None,
        fallback_path: Vec::new(),
        head: None,
    }
}

/// A head over layers `k_joint + 1 ..= k_top`: the joint in `joint_cell`, the
/// head's cells in the seat cell `strut.cell`.
fn head_of(strut: &SweepStrut, k_joint: usize, joint_cell: usize) -> Head {
    let z = |layer: usize| f64::from(u32::try_from(layer).expect("small")) * H;
    Head {
        contact_mm: [strut.center_mm[0], strut.center_mm[1], z(strut.k_top)],
        unit: [0.66, 0.0, -0.75],
        joint_mm: [strut.center_mm[0], strut.center_mm[1], z(k_joint)],
        k_joint,
        joint_cell: cell_u32(joint_cell),
        joint_radius_mm: 0.2204,
        planned_tip_radius_mm: 0.05,
        gap_mm: 0.3,
        cos_lean_max: 0.5373,
        cells: vec![cell_u32(strut.cell); strut.k_top - k_joint],
        ball_allowance_mm: 0.0,
        link_mm: 0.0,
        drawn_straight: false,
        law_planned: crate::heads::HeadLaw::envelope(0.5373),
        law_drawn: None,
        ledger_radius_mm: Vec::new(),
        certified: None,
        old_law: None,
    }
}

/// `strut` given a head over its top `k_top - k_joint` layers, with the post
/// the strut already had (a straight column from its landing in the joint cell,
/// or `post` where given). The path is set through `set_post`.
fn give_head(strut: &mut SweepStrut, k_joint: usize, joint_cell: usize, post: Option<Vec<u32>>) {
    strut.head = Some(head_of(strut, k_joint, joint_cell));
    let post = post.unwrap_or_else(|| {
        vec![cell_u32(joint_cell); (k_joint + 1).saturating_sub(strut.k_bottom)]
    });
    let (landing, k_bottom) = (strut.landing, strut.k_bottom);
    strut.set_post(landing, k_bottom, post);
    assert!(strut.head_intact(), "the fixture's own head must be intact");
}

/// A headed plate arm: a vertical post in its own column to the joint.
pub(crate) fn headed(
    grid: &Grid,
    id: u32,
    row: usize,
    col: usize,
    k_top: usize,
    k_joint: usize,
    radius: f64,
) -> SweepStrut {
    let mut strut = straight(grid, id, row, col, k_top, radius);
    let cell = strut.cell;
    give_head(&mut strut, k_joint, cell, None);
    strut
}

/// The heads of a field, to compare bit for bit after a pass.
fn heads_of(struts: &[SweepStrut]) -> Vec<Option<Head>> {
    struts.iter().map(|strut| strut.head.clone()).collect()
}

/// (b) and (c): the ledger reads 0 and 0, and every head field is what it was.
fn assert_heads_honoured(before: &[Option<Head>], k_tops: &[usize], struts: &[SweepStrut], what: &str) {
    let check = check_landings(struts, H);
    assert!(
        check.heads_not_intact.is_empty(),
        "{what}: heads whose path no longer carries them: {:?}",
        check.heads_not_intact
    );
    assert!(
        check.joins_above_joint.is_empty(),
        "{what}: members joined above a head's joint: {:?}",
        check.joins_above_joint
    );
    for (index, head) in before.iter().enumerate() {
        assert_eq!(&struts[index].head, head, "{what}: head of member {index} changed");
        assert_eq!(struts[index].k_top, k_tops[index], "{what}: k_top of member {index} changed");
    }
}

fn k_tops(struts: &[SweepStrut]) -> Vec<usize> {
    struts.iter().map(|strut| strut.k_top).collect()
}

fn slender_context<'a>(
    grid: &'a Grid,
    history: &'a ColumnHistory,
    keep_out: &'a crate::drain::DrainKeepOut,
    lean: &'a crate::lean::LeanLaw,
    law: &'a crate::slenderness::SlendernessLaw,
) -> crate::slenderness::SlendernessContext<'a> {
    crate::slenderness::SlendernessContext {
        grid,
        history,
        keep_out,
        lean,
        law,
        layer_height_mm: H,
        layers: 500,
        flare_factor: 1.6,
        mast_root_clearance: true,
        final_clearance_mm: 0.3,
        member_clearance: true,
        model_surface: None,
        plate_z_mm: 0.0,
        shaft_landings: true,
        merge: true,
        brace: true,
        thicken: true,
        mast: false,
        max_struts: 1_000,
        x_allowed: 12.0,
        seat_faces: crate::tip_landing::SeatFaceRule::from_overhang_band(45.0, 25.0),
        load_step_taper: crate::adapter::LoadStepTaper::of(lean),
        brace_cap: true,
        brace_window: true,
        frame_bands: true,
        frame_ledger: true,
        model_tie: true,
    }
}

// ----------------------------------------------------------------- the index

/// RULE 1. A headed arm stamps its post as LANDABLE only up to the joinable
/// top (`k_joint - ceil(r_K / h)`); the joint and the head BLOCK but are never
/// landed on, and the top is recorded per id. A straight arm stamps exactly as
/// `stamp_shaft` does. SABOTAGE: stamping the headed arm with plain
/// `stamp_shaft` lets `shaft_landing` return a layer on the head.
#[test]
fn a_headed_arm_is_landable_only_below_its_joint_and_blocks_above_it() {
    let grid = grid();
    let (k_top, k_joint) = (400, 380);
    let strut = headed(&grid, 0, 150, 150, k_top, k_joint, 0.05);
    let mut index = ColumnHistory::new(&grid);
    index.stamp_strut(&strut, H);
    let joinable = k_joint - JOINT_CLEAR;
    assert_eq!(index.joinable_tops.get(&0), Some(&Some(joinable)));
    let (id, layer, span_top) = index
        .shaft_landing(strut.cell, k_top)
        .expect("the post is landable");
    assert_eq!(id, 0);
    assert_eq!(layer, joinable, "a landing never passes the joinable top");
    assert_eq!(span_top, k_top, "the span itself still runs to the seat");
    // The excluding form obeys the cap too, and a ceiling under it is honoured.
    assert_eq!(
        index.shaft_landing_excluding(strut.cell, k_top, &[7]).map(|found| found.1),
        Some(joinable)
    );
    assert_eq!(index.shaft_landing(strut.cell, 100).map(|found| found.1), Some(100));
    assert_eq!(index.shaft_landing_excluding(strut.cell, k_top, &[0]), None);
    // The head and the joint block, every layer to the seat.
    for layer in [joinable, k_joint, k_joint + 1, k_top] {
        assert!(index.shaft_blocks(strut.cell, layer, None), "layer {layer} must block");
    }
    // A straight arm is stamped exactly as `stamp_shaft` stamps it.
    let plain = straight(&grid, 1, 150, 160, k_top, 0.05);
    let (mut by_strut, mut by_shaft) = (ColumnHistory::new(&grid), ColumnHistory::new(&grid));
    by_strut.stamp_strut(&plain, H);
    by_shaft.stamp_shaft(1, 0, k_top, plain.cell, &plain.path);
    assert_eq!(by_strut.shaft_spans, by_shaft.shaft_spans);
    assert!(by_strut.joinable_tops.is_empty());
    // A joint too low for any join leaves nothing landable at all.
    let low = headed(&grid, 2, 150, 170, 40, 3, 0.05);
    let mut index = ColumnHistory::new(&grid);
    index.stamp_strut(&low, H);
    assert_eq!(index.joinable_tops.get(&2), Some(&None));
    assert_eq!(index.shaft_landing(low.cell, 40), None);
    assert!(index.shaft_blocks(low.cell, 0, None));
}

// -------------------------------------------------------------- the ledger

/// RULE 9. The ledger names a path that lost its head, a landing above the
/// joint, and a brace top seated above it: and reads 0 and 0 on a field that
/// honours them.
#[test]
fn the_ledger_names_a_broken_head_and_a_join_above_the_joint() {
    let grid = grid();
    let (k_top, k_joint) = (400, 380);
    let host = headed(&grid, 0, 150, 150, k_top, k_joint, 0.3);
    let joinable = k_joint - JOINT_CLEAR;
    let child = |id: u32, layer: usize| {
        let mut child = straight(&grid, id, 150, 120, k_top, 0.1);
        let host_cell = u32::try_from(grid.index(150, 150)).expect("small");
        child.landing = Landing::Strut { id: 0, layer };
        child.k_bottom = layer;
        child.path = vec![host_cell; 1];
        child
    };
    let ok = check_landings(&[host.clone(), child(1, joinable)], H);
    assert!(ok.heads_not_intact.is_empty() && ok.joins_above_joint.is_empty(), "{ok:?}");
    assert!(ok.clean(), "{ok:?}");
    let above = check_landings(&[host.clone(), child(1, joinable + 1)], H);
    assert_eq!(above.joins_above_joint, vec![1], "a landing on the joint sphere");
    assert!(!above.clean());
    // A brace top seated on the host above the joinable top.
    let mut brace = straight(&grid, 1, 150, 120, joinable + 3, 0.05);
    brace.kind = StrutKind::Brace;
    brace.seat_on_strut = Some(0);
    brace.cell = host.cell;
    let seated = check_landings(&[host.clone(), brace.clone()], H);
    assert_eq!(seated.joins_above_joint, vec![1], "{seated:?}");
    brace.k_top = joinable;
    let fine = check_landings(&[host.clone(), brace], H);
    assert!(fine.joins_above_joint.is_empty(), "{fine:?}");
    // A rewritten path breaks the head.
    let mut broken = host;
    broken.path = vec![cell_u32(broken.cell); k_joint + 1];
    let check = check_landings(&[broken], H);
    assert_eq!(check.heads_not_intact, vec![0]);
}

// -------------------------------------------------------------- congestion

/// RULE 3, congestion lean. Two stacked plate roots, one of them headed: the
/// lean re-lands the straight one and leaves the headed one where it is,
/// counted in `headed_kept`. SABOTAGE: with the guard removed the headed arm is
/// re-routed and the ledger reads `heads_not_intact`.
#[test]
fn the_congestion_lean_never_re_routes_a_headed_arm() {
    let grid = grid();
    let history = ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let context = crate::congestion::CongestionContext {
        grid: &grid,
        history: &history,
        keep_out: &keep_out,
        lean: &lean,
        layer_height_mm: H,
        layers: 500,
        flare_factor: 1.6,
        tip_radius_min_mm: 0.05,
        hex_separation_mm: 1.0,
        x_allowed: 18.0,
        outward_lean: true,
        flank_arms: false,
        max_struts: 1_000,
    };
    let mut struts = vec![
        straight(&grid, 0, 150, 150, 200, 0.3),
        headed(&grid, 1, 150, 156, 200, 190, 0.3),
    ];
    let before = heads_of(&struts);
    let tops = k_tops(&struts);
    let path_before = struts[1].path.clone();
    let mut outcome = crate::congestion::CongestionOutcome::default();
    let moved = crate::congestion::apply_remedies(&context, &mut struts, &mut outcome);
    assert!(outcome.arms_leaned >= 1 && moved >= 1, "anti-vacuity: the straight root was leaned");
    assert!(outcome.headed_kept >= 1, "the pass fired on the headed arm: {outcome:?}");
    assert_eq!(struts[1].path, path_before, "the headed arm's route is untouched");
    assert_heads_honoured(&before, &tops, &struts, "congestion lean");
}

/// QUALITY LANE Q2 (2026-10-10), congestion lean. Two stacked plate roots, one
/// of them an arm a twin seat was merged into (`twin_host`): the twin rested on
/// it before the merge, so the lean never moved it, and the merge must keep it
/// so. Without the flag the same pair leans BOTH roots (the control run).
/// SABOTAGE: drop the `twin_host` loop in `congestion::lean_congested_roots`
/// and the host is re-routed, to the nearest clear plate cell outside the
/// silhouette (Deinonychus arm 183: 5.55 mm for a 0.018 mm sliver of overlap).
#[test]
fn the_congestion_lean_never_re_routes_an_arm_a_twin_was_merged_into() {
    let grid = grid();
    let history = ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let context = crate::congestion::CongestionContext {
        grid: &grid,
        history: &history,
        keep_out: &keep_out,
        lean: &lean,
        layer_height_mm: H,
        layers: 500,
        flare_factor: 1.6,
        tip_radius_min_mm: 0.05,
        hex_separation_mm: 1.0,
        x_allowed: 18.0,
        outward_lean: true,
        flank_arms: false,
        max_struts: 1_000,
    };
    let pair = || {
        vec![
            straight(&grid, 0, 150, 150, 200, 0.3),
            straight(&grid, 1, 150, 156, 200, 0.3),
        ]
    };
    // Control: neither is a host, so the pass re-routes at least one of them,
    // and the one it would spare in the run below is among those it moves.
    let mut control = pair();
    let mut control_outcome = crate::congestion::CongestionOutcome::default();
    crate::congestion::apply_remedies(&context, &mut control, &mut control_outcome);
    assert!(
        !control[1].path.is_empty(),
        "anti-vacuity: without the flag the second root is leaned: {control_outcome:?}"
    );
    let mut struts = pair();
    struts[1].twin_host = true;
    let mut outcome = crate::congestion::CongestionOutcome::default();
    let moved = crate::congestion::apply_remedies(&context, &mut struts, &mut outcome);
    assert!(outcome.arms_leaned >= 1 && moved >= 1, "the other root was leaned: {outcome:?}");
    assert!(struts[1].path.is_empty(), "the merged host's route is untouched");
}

// ---------------------------------------------------------- forced merges

/// RULES 1 and 3, forced merges. A thin, tall headed arm over its limit is not
/// merged (counted), and a straight arm beside it that IS merged lands on the
/// headed arm only BELOW its joint. SABOTAGE: the guard removed (and the run
/// top left at `k_top`) re-lands the headed arm; stamping with plain
/// `stamp_shaft` lets the straight arm land on the head.
#[test]
fn a_forced_merge_never_moves_a_headed_arm_and_lands_below_its_joint() {
    let grid = grid();
    let history = ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = crate::slenderness::SlendernessLaw::new(50.0, 0.05, 1.0, 32.5);
    let context = slender_context(&grid, &history, &keep_out, &lean, &law);
    let (k_top, k_joint) = (400, 380);
    let mut struts = vec![
        headed(&grid, 0, 150, 150, k_top, k_joint, 0.05),
        straight(&grid, 1, 150, 165, k_top, 0.05),
    ];
    let before = heads_of(&struts);
    let tops = k_tops(&struts);
    let path_before = struts[0].path.clone();
    let (outcome, forced) = crate::slenderness::apply_merges(&context, &mut struts);
    assert!(outcome.headed_kept >= 1, "the pass fired on the headed arm: {outcome:?}");
    assert_eq!(struts[0].path, path_before);
    assert_eq!(struts[0].landing, Landing::Plate);
    // Anti-vacuity: the straight arm WAS merged, onto the headed arm's post.
    assert_eq!(forced.len(), 1, "{forced:?}");
    let Landing::Strut { id: 0, layer } = struts[1].landing else {
        panic!("the straight arm should land on the headed arm: {:?}", struts[1].landing);
    };
    assert!(
        struts[0].joinable_at(layer, H),
        "landed at layer {layer}, above the joinable top {}",
        k_joint - JOINT_CLEAR
    );
    assert_heads_honoured(&before, &tops, &struts, "forced merge");
}

// -------------------------------------------------------- stranded branches

fn stranded_run(
    struts: &mut [SweepStrut],
    stranded: &[u32],
) -> crate::stranded_branch::StrandedBranchOutcome {
    let grid = grid();
    let history = ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = crate::slenderness::SlendernessLaw::new(50.0, 0.05, 1.0, 32.5);
    let context = slender_context(&grid, &history, &keep_out, &lean, &law);
    let mut outcome = crate::stranded_branch::StrandedBranchOutcome::default();
    let refused: HashSet<(u32, u32)> = HashSet::new();
    let made = crate::stranded_branch::branch_round(&context, 0.0, struts, stranded, &refused, &mut outcome);
    assert!(made.len() <= 1, "at most the straight control branches");
    outcome
}

/// RULE 3, stranded branches, case one: the stranded CHILD is headed. It is
/// kept; the straight control (same field, no head) branches. SABOTAGE: the
/// guard removed re-lands the headed child and breaks its head.
#[test]
fn a_stranded_headed_child_is_kept_and_the_straight_control_branches() {
    let grid = grid();
    let build = |headed_child: bool| {
        let child = if headed_child {
            headed(&grid, 1, 150, 156, 100, 90, 0.3)
        } else {
            straight(&grid, 1, 150, 156, 100, 0.3)
        };
        vec![straight(&grid, 0, 150, 150, 100, 0.3), child]
    };
    let mut control = build(false);
    let made = {
        let outcome = stranded_run(&mut control, &[1]);
        assert_eq!(outcome.headed_kept, 0);
        outcome.branches.len()
    };
    assert_eq!(made, 1, "anti-vacuity: the straight child branches onto its lapped neighbour");
    let mut struts = build(true);
    let before = heads_of(&struts);
    let tops = k_tops(&struts);
    let outcome = stranded_run(&mut struts, &[1]);
    assert!(outcome.headed_kept >= 1, "the pass fired on the headed child: {outcome:?}");
    assert_eq!(struts[1].landing, Landing::Plate);
    assert_heads_honoured(&before, &tops, &struts, "stranded branch (headed child)");
}

/// RULE 3, stranded branches, case two: a HEADED member rests on the stranded
/// child, so the branch would have to re-land it. The branch is refused.
/// SABOTAGE: the guard removed re-lands the headed dependant.
#[test]
fn a_branch_whose_dependant_is_headed_is_refused() {
    let grid = grid();
    let child_cell = grid.index(150, 156);
    let mut dependant = straight(&grid, 2, 150, 190, 100, 0.1);
    dependant.landing = Landing::Strut { id: 1, layer: 50 };
    dependant.k_bottom = 50;
    dependant.radius_mm = vec![0.1; 51];
    dependant.carried_mm2 = vec![0.0; 51];
    let post = vec![cell_u32(child_cell); 90 - 50 + 1];
    give_head(&mut dependant, 90, child_cell, Some(post));
    let mut struts = vec![
        straight(&grid, 0, 150, 150, 100, 0.3),
        straight(&grid, 1, 150, 156, 100, 0.3),
        dependant,
    ];
    let before = heads_of(&struts);
    let tops = k_tops(&struts);
    let outcome = stranded_run(&mut struts, &[1]);
    assert!(outcome.headed_kept >= 1, "the pass fired on the headed dependant: {outcome:?}");
    assert_eq!(struts[1].landing, Landing::Plate, "the branch was refused");
    assert_heads_honoured(&before, &tops, &struts, "stranded branch (headed dependant)");
}

// ----------------------------------------------------------------- un-merge

/// RULE 4. The un-merge restores a headed arm through `set_post`: its fallback
/// holds the POST only and the head slots are appended after it. SABOTAGE: a
/// plain `path = fallback_path` leaves a path without its head and the ledger
/// reads `heads_not_intact`.
#[test]
fn the_un_merge_restores_the_post_and_keeps_the_head() {
    let grid = grid();
    let (k_top, k_joint) = (400, 380);
    let trunk = {
        let mut trunk = straight(&grid, 0, 150, 150, k_top, 1.0);
        trunk.radius_mm = vec![1.0; k_top + 1];
        trunk
    };
    let own_cell = grid.index(150, 160);
    let mut arm = straight(&grid, 1, 150, 160, k_top, 0.1);
    arm.landing = Landing::Strut { id: 0, layer: 100 };
    arm.k_bottom = 100;
    arm.radius_mm = vec![0.1; k_top - 100 + 1];
    arm.carried_mm2 = vec![0.0; k_top - 100 + 1];
    // Merged: the post runs down the trunk's column, then into the joint cell.
    let mut post = vec![cell_u32(trunk.cell); k_joint - 100];
    post.push(cell_u32(own_cell));
    give_head(&mut arm, k_joint, own_cell, Some(post));
    // Where it was before the merge: the plate, a vertical post in its own cell.
    arm.straight_fallback = Some((Landing::Plate, 0));
    arm.fallback_path = vec![cell_u32(own_cell); k_joint + 1];
    let mut struts = vec![trunk, arm];
    let before = heads_of(&struts);
    let tops = k_tops(&struts);
    let unmerged = super::unmerge_overgrown(&mut struts, 1.0);
    assert_eq!(unmerged.undone, 1, "the restore happened: {unmerged:?}");
    assert_eq!(struts[1].landing, Landing::Plate);
    assert_eq!(struts[1].k_bottom, 0);
    assert_eq!(struts[1].path.len(), k_top + 1, "post then head");
    assert!(struts[1].head_intact());
    assert_heads_honoured(&before, &tops, &struts, "un-merge");
}

// ------------------------------------------------- unbraced runs and bands

/// RULE 5. A headed member's run ends at its JOINABLE top, and no band, tie or
/// `|X|` is built above it. Two thin tall headed posts beside each other are
/// braced (anti-vacuity: braces are built), every brace top and every landing
/// stays at or under the joinable top, and the ledger reads 0 and 0. SABOTAGE:
/// the run top left at `k_top` (and the band top limit at `k_top`) builds bands
/// on the head and the ledger reads `joins_above_joint`.
#[test]
fn bands_and_ties_never_attach_above_a_headed_posts_joint() {
    let grid = grid();
    let history = ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = crate::slenderness::SlendernessLaw::new(50.0, 0.05, 1.0, 32.5);
    let context = slender_context(&grid, &history, &keep_out, &lean, &law);
    let (k_top, k_joint) = (400, 396);
    let mut struts = vec![
        headed(&grid, 0, 150, 100, k_top, k_joint, 0.05),
        headed(&grid, 1, 150, 140, k_top, k_joint, 0.05),
    ];
    let joinable = k_joint - JOINT_CLEAR;
    // The runs: the headed arm's run tops out at the joinable top.
    let runs = crate::slenderness::unbraced_runs_drawn(
        &grid,
        &struts,
        H,
        &law,
        crate::adapter::LoadStepTaper::of(&lean),
    );
    let top_run = runs.iter().filter(|run| run.strut_id == 0).map(|run| run.k_top).max();
    assert_eq!(top_run, Some(joinable), "a headed arm's top run ends at its joinable top");
    let straight_runs = crate::slenderness::unbraced_runs_drawn(
        &grid,
        &[straight(&grid, 0, 150, 100, k_top, 0.05)],
        H,
        &law,
        crate::adapter::LoadStepTaper::of(&lean),
    );
    assert_eq!(straight_runs.iter().map(|run| run.k_top).max(), Some(k_top));
    let before = heads_of(&struts);
    let tops = k_tops(&struts);
    let mut outcome = crate::slenderness::SlendernessOutcome::default();
    crate::slenderness::apply_ties(&context, &mut struts, &mut outcome);
    assert!(struts.len() > 2, "anti-vacuity: the pair was braced ({} members)", struts.len());
    for strut in &struts[2..] {
        if strut.kind == StrutKind::Brace {
            assert!(strut.k_top <= joinable, "brace {} tops out at {}", strut.id, strut.k_top);
        }
        if let Landing::Strut { id, layer } = strut.landing {
            if id < 2 {
                assert!(layer <= joinable, "member {} lands at {layer}", strut.id);
            }
        }
    }
    assert_heads_honoured(&before, &tops, &struts, "bands and ties");
}

/// REVIEW FINDING 2. A member joined to a headed child of a sliding straight
/// host follows the child's POST line (base to the joint K), not a line to its
/// `k_top`. The child's base moves 0.075 mm; at layer 180 of a post from 150 to
/// 190 that line has moved 0.25 * 0.075. SABOTAGE: interpolating to `k_top`
/// gives 0.4 * 0.075.
#[test]
fn a_member_on_a_headed_child_follows_the_post_line_to_the_joint() {
    let grid = grid();
    let (k_top, k_joint, landing) = (200, 190, 150);
    let mut host = straight(&grid, 0, 150, 150, k_top, 0.3);
    let slide = [0.1, 0.0, 0.0];
    host.seat_face = SeatFace {
        point_mm: None,
        shift_mm: slide,
        footprint: crate::Footprint::MovedOntoFace,
        contact_ball_radius_mm: None,
        foot: None,
    };
    let child_cell = grid.index(150, 156);
    let mut child = straight(&grid, 1, 150, 156, k_top, 0.1);
    child.landing = Landing::Strut { id: 0, layer: landing };
    child.k_bottom = landing;
    child.radius_mm = vec![0.1; k_top - landing + 1];
    child.carried_mm2 = vec![0.0; k_top - landing + 1];
    let mut post = vec![cell_u32(host.cell); k_joint - landing];
    post.push(cell_u32(child_cell));
    give_head(&mut child, k_joint, child_cell, Some(post));
    // A brace seated on the headed child at layer 180 (below its joinable top).
    let mut brace = straight(&grid, 2, 150, 156, 180, 0.05);
    brace.kind = StrutKind::Brace;
    brace.seat_on_strut = Some(1);
    let struts = vec![host, child, brace];
    let followed = crate::footprint::followed_shifts(&struts);
    let base = followed[1].base_mm.expect("the post base follows the host");
    let seat = followed[2].seat_mm.expect("the brace follows the child's post line");
    assert!(
        (seat[0] - 0.25 * base[0]).abs() < 1e-12,
        "follows by {seat:?}, expected {} (post line to K)",
        0.25 * base[0]
    );
}

// -------------------------------------------------------------- the finish

/// The lowering's arms for a hand-made field: every member seated square under
/// a flat face at its own seat plane, tilted 30 degrees out toward -X.
fn tilted_arms(grid: &Grid, struts: &mut [SweepStrut]) -> Vec<plinth_support::SweepArm> {
    let (sin, cos) = 30.0_f64.to_radians().sin_cos();
    struts
        .iter_mut()
        .map(|strut| {
            strut.seat_normal = [-sin, 0.0, -cos];
            let (seat, base) = crate::adapter::drawn_ends(grid, 0.0, H, strut);
            crate::adapter::sweep_arm(strut, seat, base, Some(seat), strut.radius_mm.clone())
        })
        .collect()
}

/// RULE 7, the footprint slide. A seat whose disc hangs off its face is slid
/// along the face, unless the arm is HEADED: a head is never slid. With no
/// carrier raster at all every graded disc is off its face, so the straight
/// control is searched (and found a knife edge) while the headed arm keeps its
/// seat as a refused move. SABOTAGE: with the guard removed the headed arm is
/// searched like the control and reads a knife edge, not a refused move.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "a head is never slid: its shift is pinned to the exact zero the rule leaves"
)]
fn the_footprint_rule_never_slides_a_headed_arm() {
    let grid = grid();
    let mut struts = vec![
        straight(&grid, 0, 150, 150, 200, 0.1),
        headed(&grid, 1, 150, 170, 200, 190, 0.1),
    ];
    let arms = tilted_arms(&grid, &mut struts);
    let before = heads_of(&struts);
    let tops = k_tops(&struts);
    let plan = plinth_core::LayerPlan::new(0.0, vec![H; 300]).expect("a uniform plan");
    let keep_out = crate::drain::DrainKeepOut::none();
    let limits = crate::footprint::MoveLimits {
        max_lean: 1.6,
        chord_slack: vec![f64::INFINITY; 2],
        keep_out: &keep_out,
    };
    let outcome = crate::footprint::apply(
        &grid,
        &[],
        None,
        &mut struts,
        &arms,
        &plan,
        0.0,
        H,
        &limits,
        false,
        |members: &[usize], _: &[plinth_support::SweepArm], _: &[[f64; 3]]| {
            vec![[0.0, 0.0, -1.0]; members.len()]
        },
    );
    assert_eq!(outcome.tips_graded, 2, "anti-vacuity: both discs were graded: {outcome:?}");
    assert!(outcome.headed_kept >= 1, "the pass fired on the headed arm: {outcome:?}");
    assert!(
        matches!(
            struts[1].seat_face.footprint,
            crate::Footprint::MoveRefused { disc_on_face } if disc_on_face == 0.0
        ),
        "a headed disc off its face is a refused move: {:?}",
        struts[1].seat_face.footprint
    );
    assert_eq!(struts[1].seat_face.shift_mm, [0.0; 3], "heads are never slid");
    assert!(
        matches!(struts[0].seat_face.footprint, crate::Footprint::KnifeEdge { .. }),
        "control: the straight arm was searched: {:?}",
        struts[0].seat_face.footprint
    );
    assert_heads_honoured(&before, &tops, &struts, "footprint slide");
}

/// CHALLENGE POINT 2.3: a headed child lands on a STRAIGHT host and the
/// footprint rule slides that host. The head is untouched, the child's SEAT
/// (the joint `K`) does not follow, but its post BASE follows the host's axis,
/// and the chord the follower shift re-tilts is the POST's (from `K`), not the
/// seat's. SABOTAGE: `chord_scales` on `drawn_ends` reads a different chord.
#[test]
fn a_headed_child_on_a_sliding_host_keeps_its_head_and_its_post_follows() {
    let grid = grid();
    let (k_top, k_joint, landing) = (200, 190, 150);
    let mut host = straight(&grid, 0, 150, 150, k_top, 0.3);
    let slide = [0.1, 0.0, 0.0];
    host.seat_face = SeatFace {
        point_mm: None,
        shift_mm: slide,
        footprint: crate::Footprint::MovedOntoFace,
        contact_ball_radius_mm: None,
        foot: None,
    };
    let child_cell = grid.index(150, 156);
    let mut child = straight(&grid, 1, 150, 156, k_top, 0.1);
    child.landing = Landing::Strut { id: 0, layer: landing };
    child.k_bottom = landing;
    child.radius_mm = vec![0.1; k_top - landing + 1];
    child.carried_mm2 = vec![0.0; k_top - landing + 1];
    let mut post = vec![cell_u32(host.cell); k_joint - landing];
    post.push(cell_u32(child_cell));
    give_head(&mut child, k_joint, child_cell, Some(post));
    let struts = vec![host, child];
    let before = heads_of(&struts);
    let tops = k_tops(&struts);
    let followed = crate::footprint::followed_shifts(&struts);
    assert_eq!(followed[1].seat_mm, None, "the joint K never follows");
    let base = followed[1].base_mm.expect("the post base follows the host's axis");
    // The host's axis moves linearly from nothing at its base to the slide at
    // its seat: 150 of its 200 layers up.
    assert!((base[0] - slide[0] * 0.75).abs() < 1e-12, "post base follows by {base:?}");
    // The chord that shift re-tilts is the POST's, K to the base.
    let (k, foot) = crate::adapter::drawn_post_ends(&grid, 0.0, H, &struts[1]);
    let chord = |(a, b): ([f64; 3], [f64; 3])| {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    };
    let moved_foot = [foot[0] + base[0], foot[1] + base[1], foot[2] + base[2]];
    let expected = chord((k, moved_foot)) / chord((k, foot));
    let scales = crate::footprint::chord_scales(&grid, 0.0, H, &struts);
    assert!((scales[1] - expected).abs() < 1e-12, "{} vs {expected}", scales[1]);
    assert_heads_honoured(&before, &tops, &struts, "headed child on a sliding host");
}

// --------------------------------------------------------------- head joins

/// A closed box over `[0, 15]^2` whose underside is at `z = 10.0`: the face a
/// plate arm of 200 layers (10 mm) seats under.
fn slab_mesh() -> plinth_mesh::Mesh {
    let verts: Vec<[f64; 3]> = (0..8)
        .map(|corner| {
            [
                f64::from(corner & 1) * 15.0,
                f64::from((corner >> 1) & 1) * 15.0,
                10.0 + f64::from((corner >> 2) & 1) * 2.0,
            ]
        })
        .collect();
    let tris: Vec<[u32; 3]> = vec![
        [0, 2, 1],
        [1, 2, 3],
        [4, 5, 6],
        [5, 7, 6],
        [0, 1, 4],
        [1, 5, 4],
        [2, 6, 3],
        [3, 6, 7],
        [0, 4, 2],
        [2, 4, 6],
        [1, 3, 5],
        [3, 7, 5],
    ];
    plinth_mesh::Mesh::new(verts, tris).expect("a closed box")
}

/// RULE 3, head joins. A lander joined in its host's seat zone is normally
/// re-landed down the host's chain; a HEADED lander is kept where it is
/// (counted). (The join layers a headed HOST offers are pinned by
/// `head_join::head_carrier_join_tests`.) SABOTAGE: with the guard removed the
/// headed lander is re-landed like the control and `headed_kept` reads 0.
#[test]
fn a_head_join_keeps_a_headed_lander_where_it_is() {
    let grid = grid();
    let history = ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = crate::slenderness::SlendernessLaw::new(50.0, 0.05, 1.0, 32.5);
    let context = slender_context(&grid, &history, &keep_out, &lean, &law);
    let mesh = slab_mesh();
    let plan = plinth_core::LayerPlan::new(0.0, vec![H; 300]).expect("a uniform plan");
    let surface = std::sync::OnceLock::new();
    let joins = crate::head_join::HeadJoinContext {
        slender: &context,
        mesh: &mesh,
        layer_plan: &plan,
        plate_z_mm: 0.0,
        clearance_mm: 0.3,
        standoff_mm: 0.6,
        surface_pitch_mm: 0.5,
        surface_max_bytes: 1 << 28,
        surface: &surface,
        contacts: &crate::contact_growth::MeasureMemo::default(),
    };
    let build = |headed_lander: bool| {
        let mut host = straight(&grid, 0, 150, 150, 200, 0.4);
        host.radius_mm[200] = 0.05;
        let lander_cell = grid.index(150, 152);
        let mut lander = straight(&grid, 1, 150, 152, 200, 0.05);
        lander.landing = Landing::Strut { id: 0, layer: 195 };
        lander.k_bottom = 195;
        lander.radius_mm = vec![0.05; 6];
        lander.carried_mm2 = vec![0.0; 6];
        if headed_lander {
            let post = vec![cell_u32(host.cell), cell_u32(lander_cell), cell_u32(lander_cell)];
            give_head(&mut lander, 197, lander_cell, Some(post));
        } else {
            lander.path = vec![cell_u32(host.cell); 5];
        }
        vec![host, lander]
    };
    let run = |struts: &mut Vec<SweepStrut>| {
        let mut outcome = crate::head_join::HeadJoinOutcome::default();
        let moved = crate::head_join::apply(&joins, struts, &mut outcome, true);
        (moved, outcome)
    };
    let mut control = build(false);
    let (_, outcome) = run(&mut control);
    assert!(
        outcome.joins_in_seat_zone_before >= 1,
        "anti-vacuity: the join is in its host's seat zone: {outcome:?}"
    );
    assert_eq!(outcome.headed_kept, 0);
    let mut struts = build(true);
    let before = heads_of(&struts);
    let tops = k_tops(&struts);
    let path_before = struts[1].path.clone();
    let (moved, outcome) = run(&mut struts);
    assert!(outcome.joins_in_seat_zone_before >= 1, "{outcome:?}");
    assert!(outcome.headed_kept >= 1, "the pass fired on the headed lander: {outcome:?}");
    assert_eq!(moved, 0);
    assert_eq!(struts[1].path, path_before);
    assert_eq!(struts[1].landing, Landing::Strut { id: 0, layer: 195 });
    assert_heads_honoured(&before, &tops, &struts, "head join");
}

// ---------------------------------------------------------------- lip seats

/// A tiny closed box as the model: the lip-seat pass reads the mesh only
/// through its surface, which these fixtures leave unbuilt.
fn box_mesh() -> plinth_mesh::Mesh {
    let verts: Vec<[f64; 3]> = (0..8)
        .map(|corner| {
            [
                f64::from(corner & 1) * 2.0,
                f64::from((corner >> 1) & 1) * 2.0,
                100.0 + f64::from((corner >> 2) & 1) * 2.0,
            ]
        })
        .collect();
    let tris: Vec<[u32; 3]> = vec![
        [0, 2, 1],
        [1, 2, 3],
        [4, 5, 6],
        [5, 7, 6],
        [0, 1, 4],
        [1, 5, 4],
        [2, 6, 3],
        [3, 6, 7],
        [0, 4, 2],
        [2, 4, 6],
        [1, 3, 5],
        [3, 7, 5],
    ];
    plinth_mesh::Mesh::new(verts, tris).expect("a closed box")
}

/// RULE 3, lip seats, both triggers. A headed arm the seat-bite trigger (or the
/// clearance re-route) sends is neither re-landed nor moved as a child; the
/// straight control sent the same way IS re-landed. SABOTAGE: with the guards
/// removed the headed arm is re-landed and the ledger reads `heads_not_intact`.
#[test]
fn the_lip_seat_pass_never_re_lands_a_headed_arm_on_either_trigger() {
    let grid = grid();
    let history = ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = crate::slenderness::SlendernessLaw::new(30.0, 0.05, 1.0, 32.5);
    let context = slender_context(&grid, &history, &keep_out, &lean, &law);
    let mesh = box_mesh();
    let plan = plinth_core::LayerPlan::new(0.0, vec![H; 300]).expect("a uniform plan");
    let surface = std::sync::OnceLock::new();
    assert!(surface.set(None).is_ok(), "a fresh cell");
    let joins = crate::head_join::HeadJoinContext {
        slender: &context,
        mesh: &mesh,
        layer_plan: &plan,
        plate_z_mm: 0.0,
        clearance_mm: 0.3,
        standoff_mm: 0.6,
        surface_pitch_mm: 0.5,
        surface_max_bytes: 1 << 20,
        surface: &surface,
        contacts: &crate::contact_growth::MeasureMemo::default(),
    };
    let bite = plinth_support::ShaftSeatBite {
        bite_mm: 0.2,
        approach_deg: 60.0,
        limit_deg: 20.0,
    };
    for clearance in [false, true] {
        let run = |headed_arm: bool| {
            let mut struts = vec![if headed_arm {
                headed(&grid, 0, 150, 150, 200, 190, 0.1)
            } else {
                straight(&grid, 0, 150, 150, 200, 0.1)
            }];
            let arms = tilted_arms(&grid, &mut struts);
            let bites = vec![Some(bite)];
            let violators = [true];
            let trigger = if clearance {
                crate::lip_seat::Trigger::Clearance {
                    violators: &violators,
                    foot_sizing: None,
                }
            } else {
                crate::lip_seat::Trigger::SeatBite
            };
            let before = heads_of(&struts);
            let tops = k_tops(&struts);
            let mut outcome = crate::lip_seat::LipSeatOutcome::default();
            let moves = crate::lip_seat::apply(
                &joins, &plan, 0.0, &mut struts, &arms, &bites, trigger, &[], &mut outcome,
            );
            (struts, before, tops, outcome, moves.len())
        };
        let (_, _, _, control, moved) = run(false);
        assert!(moved >= 1, "anti-vacuity (clearance {clearance}): the straight arm is re-landed: {control:?}");
        assert_eq!(control.headed_kept, 0);
        let (struts, before, tops, outcome, moved) = run(true);
        assert!(outcome.headed_kept >= 1, "(clearance {clearance}) the pass fired: {outcome:?}");
        assert_eq!(moved, 0, "(clearance {clearance}) a headed arm is never re-landed");
        assert_eq!(struts[0].landing, Landing::Plate);
        assert_heads_honoured(&before, &tops, &struts, "lip seat");
    }
}

/// **THE STANDING ARM'S SEAT READ IS A MEMO OF THE UNCACHED READ** (perf lane
/// A1, 2026-10-14). `lip_seat::tips_seat` reads each pair's standing arm (its
/// seat site and its contact there) once per pass, from `StandingSeats`, and
/// only the lean every round. Over three rounds that mix new arms, arms seen
/// before, an arm twice in one round and a different lean for an arm seen
/// before, every row equals what the whole-pair read without a memo (the code
/// before the lane, restated here) gives, bit for bit; each standing arm is
/// read once. A NEW pass is a new `StandingSeats`: a changed arm at the same
/// index is read afresh and again equals the uncached read (nothing is kept between
/// passes).
/// ANTI-VACUITY: at least one standing arm makes contact, so the equality is
/// not two lists of `None`. SABOTAGE (by hand: the memo keyed by a constant
/// instead of the arm index): the second round reads another arm's answer and
/// the equality goes red.
#[test]
fn the_standing_seat_read_is_a_memo_of_the_uncached_read() {
    use crate::lip_seat::{tips_seat, StandingSeats, TipContact};
    let grid = grid();
    let history = ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = crate::slenderness::SlendernessLaw::new(50.0, 0.05, 1.0, 32.5);
    let context = slender_context(&grid, &history, &keep_out, &lean, &law);
    let mesh = slab_mesh();
    let plan = plinth_core::LayerPlan::new(0.0, vec![H; 300]).expect("a uniform plan");
    let surface = std::sync::OnceLock::new();
    let contacts = crate::contact_growth::MeasureMemo::default();
    let joins = crate::head_join::HeadJoinContext {
        slender: &context,
        mesh: &mesh,
        layer_plan: &plan,
        plate_z_mm: 0.0,
        clearance_mm: 0.3,
        standoff_mm: 0.6,
        surface_pitch_mm: 0.5,
        surface_max_bytes: 1 << 28,
        surface: &surface,
        contacts: &contacts,
    };
    // The code before the lane: one whole-pair read, no memo.
    let uncached = |pairs: &[(usize, &plinth_support::SweepArm, &plinth_support::SweepArm)]| {
        let points: Vec<Option<[f64; 3]>> = pairs.iter().map(|(_, stands, _)| stands.seat_face_point_mm).collect();
        let table = crate::footprint_normal::FaceTable::of(&mesh);
        let sites = crate::footprint_normal::sites_at(&table, &points, context.seat_faces);
        let mut rows = Vec::new();
        let mut row_sites = Vec::new();
        for ((_, stands, lean), site) in pairs.iter().zip(&sites) {
            for arm in [stands, lean] {
                rows.push(plinth_support::SweepArm {
                    contact_ball_radius_mm: None,
                    ..(*arm).clone()
                });
                row_sites.push(*site);
            }
        }
        let env = crate::footprint_normal::MeasureEnv {
            mesh: &mesh,
            surface: joins.surface(),
            layer_plan: &plan,
        };
        let pixel = grid.pitch_mm() * grid.pitch_mm();
        let read = crate::contact_growth::measure(&env, &rows, &row_sites);
        let contact = |row: Option<&Option<crate::contact_growth::Measured>>| TipContact {
            seats: row
                .and_then(Option::as_ref)
                .is_some_and(|m| m.real_mm2 > 0.0 && m.real_mm2 + pixel >= m.planned_mm2),
            real_mm2: row.and_then(Option::as_ref).map(|m| m.real_mm2),
            pixel_mm2: pixel,
        };
        read.chunks(2)
            .map(|both| (contact(both.first()), contact(both.get(1))))
            .collect::<Vec<_>>()
    };
    // Four standing arms over the slab at different cells and radii, and two
    // different sets of leans (other arms, tilted the same way).
    let arms_of = |radius: f64, col_shift: usize| {
        let mut struts: Vec<SweepStrut> = (0..4)
            .map(|k| straight(&grid, k, 150 + 7 * k as usize, 150 + col_shift + 11 * k as usize, 200, radius))
            .collect();
        tilted_arms(&grid, &mut struts)
    };
    let stands = arms_of(0.1, 0);
    let leans_a = arms_of(0.15, 3);
    let leans_b = arms_of(0.2, 5);
    let rounds: [Vec<(usize, &plinth_support::SweepArm, &plinth_support::SweepArm)>; 3] = [
        vec![(0, &stands[0], &leans_a[0]), (1, &stands[1], &leans_a[1])],
        vec![
            (2, &stands[2], &leans_a[2]),
            (1, &stands[1], &leans_b[1]),
            (0, &stands[0], &leans_b[0]),
            (2, &stands[2], &leans_b[2]),
        ],
        vec![(3, &stands[3], &leans_a[3]), (0, &stands[0], &leans_a[1])],
    ];
    let mut memo = StandingSeats::default();
    let mut any_contact = false;
    for (round, pairs) in rounds.iter().enumerate() {
        let got = tips_seat(&joins, &plan, &mut memo, pairs);
        let want = uncached(pairs);
        assert_eq!(got, want, "round {round}: the memoised read is the uncached read");
        any_contact |= got.iter().any(|(stands, _)| stands.real_mm2.is_some_and(|real| real > 0.0));
    }
    assert!(any_contact, "anti-vacuity: some standing arm makes contact");
    assert_eq!(memo.arms_read(), 4, "each standing arm is read once");
    // The balls of the context's memo (the contact memo across passes): a
    // second pass, with its own standing-seat memo, asks the same balls again.
    // It equals the unmemoised read and adds no entry.
    let kept = contacts.len();
    assert!(kept > 0, "the context's memo holds the balls asked");
    let mut second_pass = StandingSeats::default();
    for (round, pairs) in rounds.iter().enumerate() {
        assert_eq!(
            tips_seat(&joins, &plan, &mut second_pass, pairs),
            uncached(pairs),
            "round {round}, second pass: the context's memoised read is the uncached read"
        );
    }
    assert_eq!(contacts.len(), kept, "a second pass over the same balls adds no entry");
    // A new pass, a changed arm at the same index: read afresh.
    let moved = arms_of(0.1, 20);
    let again = [(0, &moved[0], &leans_a[0]), (1, &moved[1], &leans_a[1])];
    let mut fresh = StandingSeats::default();
    assert_eq!(
        tips_seat(&joins, &plan, &mut fresh, &again),
        uncached(&again),
        "a new pass reads its own arms"
    );
}

/// Closed boxes `(min, max)` as one mesh: the slab the arm seats under, and a
/// wall beside its shaft.
fn boxes_mesh(boxes: &[([f64; 3], [f64; 3])]) -> plinth_mesh::Mesh {
    const SIDES: [[u32; 3]; 12] = [
        [0, 2, 1],
        [1, 2, 3],
        [4, 5, 6],
        [5, 7, 6],
        [0, 1, 4],
        [1, 5, 4],
        [2, 6, 3],
        [3, 6, 7],
        [0, 4, 2],
        [2, 4, 6],
        [1, 3, 5],
        [3, 7, 5],
    ];
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for (lo, hi) in boxes {
        let first = u32::try_from(verts.len()).expect("a few vertices");
        for corner in 0..8_u32 {
            verts.push([
                if corner & 1 == 0 { lo[0] } else { hi[0] },
                if (corner >> 1) & 1 == 0 { lo[1] } else { hi[1] },
                if (corner >> 2) & 1 == 0 { lo[2] } else { hi[2] },
            ]);
        }
        tris.extend(SIDES.iter().map(|tri| [first + tri[0], first + tri[1], first + tri[2]]));
    }
    plinth_mesh::Mesh::new(verts, tris).expect("closed boxes")
}

/// **THE SLENDERNESS REFUSAL IS THE LAST TEST, NOT THE FIRST** (Q1 review,
/// 2026-10-11). The lip-seat rule refuses a lean the slenderness law cannot
/// hold, and stops there for the arm. It once stopped at the cheapest
/// descent that cleared the model, BEFORE the plate-root test: a descent
/// whose flared root lapped an existing root, and which the root test would
/// have discarded, ended the whole search, although a later descent clear of
/// that root, with room to thicken, satisfied both rules.
///
/// An arm seated under a slab leans -X to a plate root. A low wall 0.305 mm
/// from the axis of the cheapest descent leaves the solid no room to grow
/// (its clearance reads 0.305 mm against the 0.3 mm gap, so the radius that
/// fits is the drawn 0.1 mm plus 0.005 mm). The next descents lie farther
/// from the wall and read 0.31 mm clear (0.1107 mm fits). The ratio `S` is set
/// to 95 so that the run of every descent in play, about 10.2 mm, lies between
/// the two limits: over `S * 0.105 = 9.98 mm` for the cheapest, under
/// `S * 0.1107 = 10.5 mm` for the later ones.
///
/// CONTROL (anti-vacuity): with no other root the cheapest descent is
/// admissible, trips the law, and the arm is kept for it. TEST: an existing
/// root, drawn 0.6 mm wide at the end of a short one-cell post (so no cell
/// near it reads as a landing on the post), laps that cheapest descent; the
/// root test discards it, and the arm re-lands on a later one that holds.
/// SABOTAGE (run on the committed `5260311` ordering): the arm is kept for
/// slenderness at the cheapest descent and the root test is never reached
/// (`candidates_refused_roots` 0, `moved` 0).
#[test]
fn an_inadmissible_cheapest_descent_does_not_end_the_slenderness_search() {
    let grid = grid();
    let history = ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = crate::slenderness::SlendernessLaw::new(95.0, 0.05, 1.0, 32.5);
    let off = crate::slenderness::SlendernessLaw::disabled();
    let mesh = boxes_mesh(&[
        ([0.0, 0.0, 10.0], [15.0, 15.0, 12.0]),
        ([2.0, 7.93, 0.5], [13.0, 8.93, 3.0]),
    ]);
    let plan = plinth_core::LayerPlan::new(0.0, vec![H; 300]).expect("a uniform plan");
    let bite = plinth_support::ShaftSeatBite {
        bite_mm: 0.2,
        approach_deg: 60.0,
        limit_deg: 20.0,
    };
    // Re-land arm 0 under `law`, with a second plate member (a root drawn
    // 0.6 mm wide) at `root_col` of the arm's row when given. Returns the
    // field, the outcome and the number of moves.
    let run = |law: &crate::slenderness::SlendernessLaw, root_col: Option<usize>| {
        let context = slender_context(&grid, &history, &keep_out, &lean, law);
        let surface = std::sync::OnceLock::new();
        let joins = crate::head_join::HeadJoinContext {
            slender: &context,
            mesh: &mesh,
            layer_plan: &plan,
            plate_z_mm: 0.0,
            clearance_mm: 0.3,
            standoff_mm: 0.6,
            surface_pitch_mm: 0.1,
            surface_max_bytes: 1 << 28,
            surface: &surface,
            contacts: &crate::contact_growth::MeasureMemo::default(),
        };
        let mut struts = vec![straight(&grid, 0, 150, 150, 200, 0.1)];
        if let Some(col) = root_col {
            struts.push(straight(&grid, 1, 150, col, 2, 0.05));
        }
        let mut arms = tilted_arms(&grid, &mut struts);
        // The root is DRAWN wide (a heavy load below a short post) while its
        // post stays one cell wide, so no cell near it reads as a landing on
        // the post: the search offers the plate there and the root test is
        // what discards it.
        if root_col.is_some() {
            arms[1].radius_profile_mm = vec![0.6; 3];
        }
        let mut bites = vec![None; struts.len()];
        bites[0] = Some(bite);
        let mut outcome = crate::lip_seat::LipSeatOutcome::default();
        let moves = crate::lip_seat::apply(
            &joins,
            &plan,
            0.0,
            &mut struts,
            &arms,
            &bites,
            crate::lip_seat::Trigger::SeatBite,
            &[],
            &mut outcome,
        );
        (struts, outcome, moves.len())
    };
    // The cheapest descent with no law at all: the one the root test must
    // discard in the TEST below.
    let (free, _, moved) = run(&off, None);
    assert_eq!(moved, 1, "anti-vacuity: the arm has a descent when nothing limits the lean");
    let cheapest = free[0].base_cell();
    // CONTROL: the law on, no other root. The cheapest descent is
    // admissible, its run is over S times the radius that fits beside the
    // wall, and the arm keeps its seat, kept for slenderness.
    let (kept, outcome, moved) = run(&law, None);
    assert_eq!(moved, 0, "the cheapest descent trips the law: {outcome:?}");
    assert_eq!(outcome.candidates_refused_slender, 1, "{outcome:?}");
    assert_eq!(outcome.kept_for_slenderness, 1, "{outcome:?}");
    assert_eq!(outcome.candidates_refused_roots, 0, "{outcome:?}");
    assert_eq!(kept[0].landing, Landing::Plate);
    assert_eq!(kept[0].base_cell(), kept[0].cell, "the arm keeps its seat");
    // TEST: a root whose flared disc laps that cheapest descent. The root
    // test discards it, and the search goes on to a descent clear of the
    // root, where the wall is farther and the run fits.
    let (moved_to, outcome, moved) = run(&law, Some(106));
    assert!(
        outcome.candidates_refused_roots >= 1,
        "the root test reached the cheapest descent: {outcome:?}"
    );
    assert_eq!(moved, 1, "the arm re-lands on a later descent: {outcome:?}");
    assert_eq!(outcome.candidates_refused_slender, 0, "{outcome:?}");
    assert_eq!(outcome.kept_for_slenderness, 0, "{outcome:?}");
    assert_eq!(outcome.candidates_verified, 1, "{outcome:?}");
    assert_ne!(moved_to[0].base_cell(), cheapest, "not the descent the root discarded");
    assert!(moved_to[0].is_leaned(), "the arm leans to its new root");
}

/// **THE DEEP SCAN GIVES THE ROUNDS' ANSWER** (lane S2, 2026-10-15;
/// `lip_seat::scan_open_jobs`). Two arms seat under a slab over a block that
/// stands under both seats, so the cheapest descents of each run through or
/// beside the block and their clearance reads refuse them, one candidate per
/// round. The scan settles the rounds in which both jobs, and then the one
/// left, are refused, ahead in chunks. Two blocks: a LOW one, which a lean wide of
/// it clears (the walk stops and the arm moves), and a TALL one, which no
/// descent clears (every candidate of both lists is refused, as the
/// Deinonychus walker's 90,000 are).
///
/// TEST: with the scan on (and the shadow check, which re-asks every
/// candidate it settled one at a time) the field, the outcome and the moves
/// are those of the rounds with the scan off. ANTI-VACUITY: the rounds refuse
/// more candidates for clearance than two first chunks hold, the scan settles
/// candidates, and under the low block an arm moves. SABOTAGE: stopping the
/// scan one candidate late (the stop counted as a refusal) changes the
/// outcome's refusal count and the move under the low block.
#[test]
fn the_deep_scan_gives_the_rounds_answer() {
    let grid = grid();
    let history = ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let off = crate::slenderness::SlendernessLaw::disabled();
    let low = boxes_mesh(&[
        ([0.0, 0.0, 10.0], [15.0, 15.0, 12.0]),
        ([6.0, 6.0, 0.5], [9.0, 9.0, 2.0]),
    ]);
    let tall = boxes_mesh(&[
        ([0.0, 0.0, 10.0], [15.0, 15.0, 12.0]),
        ([5.5, 5.0, 0.5], [9.5, 10.5, 7.0]),
    ]);
    let plan = plinth_core::LayerPlan::new(0.0, vec![H; 300]).expect("a uniform plan");
    let bite = plinth_support::ShaftSeatBite {
        bite_mm: 0.2,
        approach_deg: 60.0,
        limit_deg: 20.0,
    };
    // The low block, and a small block beside the seats that a child
    // following arm 0 meets on some of its new paths.
    let child_block = boxes_mesh(&[
        ([0.0, 0.0, 10.0], [15.0, 15.0, 12.0]),
        ([6.0, 6.0, 0.5], [9.0, 9.0, 2.0]),
        ([6.5, 8.2, 6.0], [8.0, 8.8, 7.0]),
    ]);
    let run = |mesh: &plinth_mesh::Mesh, child: bool, scan: bool, verify: bool| {
        let context = slender_context(&grid, &history, &keep_out, &lean, &off);
        let surface = std::sync::OnceLock::new();
        let joins = crate::head_join::HeadJoinContext {
            slender: &context,
            mesh,
            layer_plan: &plan,
            plate_z_mm: 0.0,
            clearance_mm: 0.3,
            standoff_mm: 0.6,
            surface_pitch_mm: 0.1,
            surface_max_bytes: 1 << 28,
            surface: &surface,
            contacts: &crate::contact_growth::MeasureMemo::default(),
        };
        let mut struts = vec![
            straight(&grid, 0, 150, 150, 200, 0.1),
            straight(&grid, 1, 160, 145, 200, 0.1),
        ];
        if child {
            // A support arm resting on arm 0 at layer 60, its path leaning
            // from arm 0's column up to its own seat: it moves with arm 0.
            let (landing, k_top) = (60_usize, 200_usize);
            let mut rests = straight(&grid, 2, 165, 175, k_top, 0.1);
            rests.landing = Landing::Strut { id: 0, layer: landing };
            rests.k_bottom = landing;
            rests.radius_mm = vec![0.1; k_top - landing + 1];
            rests.carried_mm2 = vec![0.0; k_top - landing + 1];
            rests.path = (landing..k_top)
                .map(|layer| {
                    let at = |from: usize, to: usize| from + (to - from) * (layer - landing) / (k_top - landing);
                    cell_u32(grid.index(at(150, 165), at(150, 175)))
                })
                .collect();
            struts.push(rests);
        }
        let arms = tilted_arms(&grid, &mut struts);
        let mut bites = vec![Some(bite); 2];
        bites.resize(struts.len(), None);
        let ((outcome, moves), scanned, child_refused) = crate::lip_seat::with_scan(scan, verify, || {
            let mut outcome = crate::lip_seat::LipSeatOutcome::default();
            let moves = crate::lip_seat::apply(
                &joins,
                &plan,
                0.0,
                &mut struts,
                &arms,
                &bites,
                crate::lip_seat::Trigger::SeatBite,
                &[],
                &mut outcome,
            );
            (outcome, moves.len())
        });
        (format!("{struts:?}"), format!("{outcome:?}"), outcome, moves, scanned, child_refused)
    };
    // The shadow check re-reads every verdict one row at a time; the tall
    // block's 60,000 refusals are left to the comparison alone.
    for (what, mesh, child, moves, verify) in [
        ("low block", &low, false, true, true),
        ("tall block", &tall, false, false, false),
        ("a child that follows", &child_block, true, true, true),
    ] {
        let (rounds_field, rounds_outcome, rounds, rounds_moves, rounds_scanned, _) = run(mesh, child, false, false);
        assert_eq!(rounds_scanned, 0, "{what}: the scan is off");
        assert!(
            rounds.candidates_refused_clearance > 2 * crate::lip_seat::SCAN_FIRST,
            "{what}: anti-vacuity: the rounds refuse more candidates for clearance than two first chunks hold: {rounds:?}"
        );
        assert_eq!(rounds_moves >= 1, moves, "{what}: anti-vacuity: {rounds:?}");
        let (scan_field, scan_outcome, _, scan_moves, scanned, child_refused) = run(mesh, child, true, verify);
        assert!(scanned > 0, "{what}: the scan settled candidates");
        assert_eq!(scan_outcome, rounds_outcome, "{what}: the outcome is the rounds' outcome");
        assert_eq!(scan_moves, rounds_moves, "{what}: the moves are the rounds' moves");
        assert_eq!(scan_field, rounds_field, "{what}: the field is the rounds' field");
        assert_eq!(child_refused > 0, child, "{what}: anti-vacuity: a child's read refused candidates: {child_refused}");
    }
}

/// **THE DEEP SCAN HOLDS ITS BYTE BUDGET AND STILL GIVES THE ROUNDS' ANSWER**
/// (lane S2 review, 2026-10-15; `lip_seat::scan_open_jobs`). With a tiny
/// read-ahead byte limit, two jobs whose every candidate is refused fill the
/// scan's stores past what the ledger holds, so the scan must take only the
/// rounds its stores prove and settle on (its fallback), never holding more
/// than the limit.
///
/// Three fields: the TALL block under two seat-bite arms (every descent runs
/// into it); a MID block whose refused prefixes outgrow the stores before a
/// walk stops and an arm moves (the fallback takes proven rounds ahead of a
/// commit); and a sealed CAVITY under the clearance re-route: two hosts
/// seated under its ceiling, each carrying a child seated outside the
/// cavity's wall, so a host whose proposed axis stands in the enclosed air is
/// counted hidden and its child (whose new path crosses the wall) refuses the
/// candidate: a refusal after a hidden member, with a transitive follower.
///
/// TEST: scan on (MID and the cavity also under the shadow check) equals scan
/// off at the same limit in field, outcome and moves; the ledger never held
/// more than the limit; no chunk's working set passed its share.
/// ANTI-VACUITY: the scan settled candidates and fell back for want of room
/// at least once in each field, an arm moves in MID, and the cavity's
/// refusals counted hidden members. SABOTAGE: with the stores' room check
/// and the chunk's room bound removed, TALL's ledger reaches 999,392 bytes
/// against its 262,144 limit. (Taking one round past the proven ones in the
/// fallback is NOT a defect this can see, and is in fact exact: round R + 1's
/// refusals are decided on reads made at its start, before any commit.)
#[test]
fn the_deep_scan_holds_its_byte_budget_and_gives_the_rounds_answer() {
    use crate::cavity::fixtures::{cavity_box_sized, cell, map_of};
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let off = crate::slenderness::SlendernessLaw::disabled();
    let plan = plinth_core::LayerPlan::new(0.0, vec![H; 300]).expect("a uniform plan");
    let bite = plinth_support::ShaftSeatBite {
        bite_mm: 0.2,
        approach_deg: 60.0,
        limit_deg: 20.0,
    };
    // One `apply` on `mesh` over `grid` (with `history`), scan on or off, at
    // the byte limit `limit`.
    #[expect(clippy::too_many_arguments, reason = "the field, its trigger and the switches")]
    fn run(
        grid: &Grid,
        history: &ColumnHistory,
        keep_out: &crate::drain::DrainKeepOut,
        lean: &crate::lean::LeanLaw,
        law: &crate::slenderness::SlendernessLaw,
        plan: &plinth_core::LayerPlan,
        mesh: &plinth_mesh::Mesh,
        mut struts: Vec<SweepStrut>,
        bites: &[Option<plinth_support::ShaftSeatBite>],
        violators: Option<&[bool]>,
        scan: bool,
        verify: bool,
        limit: usize,
    ) -> (String, crate::lip_seat::LipSeatOutcome, usize, usize, crate::lip_seat::ScanMemory) {
        let context = slender_context(grid, history, keep_out, lean, law);
        let surface = std::sync::OnceLock::new();
        let joins = crate::head_join::HeadJoinContext {
            slender: &context,
            mesh,
            layer_plan: plan,
            plate_z_mm: 0.0,
            clearance_mm: 0.3,
            standoff_mm: 0.6,
            surface_pitch_mm: 0.1,
            surface_max_bytes: 1 << 28,
            surface: &surface,
            contacts: &crate::contact_growth::MeasureMemo::default(),
        };
        let arms = tilted_arms(grid, &mut struts);
        let trigger = violators.map_or(crate::lip_seat::Trigger::SeatBite, |violators| {
            crate::lip_seat::Trigger::Clearance {
                violators,
                foot_sizing: None,
            }
        });
        let (((outcome, moves), scanned, _), memory) = crate::lip_seat::with_budget(Some(limit), || {
            crate::lip_seat::with_scan(scan, verify, || {
                let mut outcome = crate::lip_seat::LipSeatOutcome::default();
                let moves = crate::lip_seat::apply(
                    &joins,
                    plan,
                    0.0,
                    &mut struts,
                    &arms,
                    bites,
                    trigger,
                    &[],
                    &mut outcome,
                );
                (outcome, moves.len())
            })
        });
        (format!("{struts:?}"), outcome, moves, scanned, memory)
    }

    // A member resting on `host` at `landing`, seated at `(row, col)`, its
    // path leaning from the host's column up to its own.
    let resting = |grid: &Grid, id: u32, host: &SweepStrut, row: usize, col: usize, k_top: usize, landing: usize| {
        let mut member = straight(grid, id, row, col, k_top, 0.1);
        member.landing = Landing::Strut { id: host.id, layer: landing };
        member.k_bottom = landing;
        member.radius_mm = vec![0.1; k_top - landing + 1];
        member.carried_mm2 = vec![0.0; k_top - landing + 1];
        let (host_row, host_col) = (grid.row_of(host.cell), grid.col_of(host.cell));
        member.path = (landing..k_top)
            .map(|layer| {
                let at = |from: usize, to: usize| from + (to - from) * (layer - landing) / (k_top - landing);
                cell_u32(grid.index(at(host_row, row), at(host_col, col)))
            })
            .collect();
        member
    };

    // TALL: two seat-bite arms over a block no descent clears.
    {
        let grid = grid();
        let history = ColumnHistory::new(&grid);
        let tall = boxes_mesh(&[
            ([0.0, 0.0, 10.0], [15.0, 15.0, 12.0]),
            ([5.5, 5.0, 0.5], [9.5, 10.5, 7.0]),
        ]);
        let struts = vec![
            straight(&grid, 0, 150, 150, 200, 0.1),
            straight(&grid, 1, 160, 145, 200, 0.1),
        ];
        let bites = vec![Some(bite); 2];
        let limit = 256 << 10;
        let go = |scan: bool| {
            run(&grid, &history, &keep_out, &lean, &off, &plan, &tall, struts.clone(), &bites, None, scan, false, limit)
        };
        let (rounds_field, rounds, rounds_moves, _, _) = go(false);
        let (scan_field, scanned_outcome, scan_moves, scanned, memory) = go(true);
        assert_eq!(rounds_moves, 0, "tall: anti-vacuity: no descent clears");
        assert!(rounds.candidates_refused_clearance > 10_000, "tall: anti-vacuity: {rounds:?}");
        assert!(memory.peak <= limit, "tall: the ledger held {} > {limit}", memory.peak);
        assert!(scanned > 0 && memory.fallbacks > 0, "tall: the scan ran and fell back: {memory:?}");
        assert!(!memory.chunk_over, "tall: a chunk's working set passed its share");
        assert_eq!(format!("{scanned_outcome:?}"), format!("{rounds:?}"), "tall: the outcome is the rounds'");
        assert_eq!((scan_field, scan_moves), (rounds_field, rounds_moves), "tall: the field is the rounds'");
    }

    // MID: two seat-bite arms over a block 3.5 mm tall; each walk refuses
    // more candidates than the limit's stores hold before one passes and an
    // arm moves, so the fallback takes proven rounds ahead of a commit.
    {
        let grid = grid();
        let history = ColumnHistory::new(&grid);
        let low = boxes_mesh(&[
            ([0.0, 0.0, 10.0], [15.0, 15.0, 12.0]),
            ([6.0, 6.0, 0.5], [9.0, 9.0, 3.5]),
        ]);
        let struts = vec![
            straight(&grid, 0, 150, 150, 200, 0.1),
            straight(&grid, 1, 160, 145, 200, 0.1),
        ];
        let bites = vec![Some(bite); 2];
        let limit = 128 << 10;
        let go = |scan: bool| {
            run(&grid, &history, &keep_out, &lean, &off, &plan, &low, struts.clone(), &bites, None, scan, scan, limit)
        };
        let (rounds_field, rounds, rounds_moves, _, _) = go(false);
        let (scan_field, scanned_outcome, scan_moves, scanned, memory) = go(true);
        assert!(rounds_moves >= 1, "mid: anti-vacuity: an arm moves: {rounds:?}");
        assert!(memory.peak <= limit, "mid: the ledger held {} > {limit}", memory.peak);
        assert!(scanned > 0 && memory.fallbacks > 0, "mid: the scan ran and fell back: {memory:?}");
        assert!(!memory.chunk_over, "mid: a chunk's working set passed its share");
        assert_eq!(format!("{scanned_outcome:?}"), format!("{rounds:?}"), "mid: the outcome is the rounds'");
        assert_eq!((scan_field, scan_moves), (rounds_field, rounds_moves), "mid: the field is the rounds'");
    }

    // CAVITY: two hosts in enclosed air under the clearance re-route, each
    // carrying a child seated outside the wall.
    {
        let cavity = cavity_box_sized(false, false, 0.0, 4.0, 0.5);
        let (map, grid) = map_of(&cavity, &[]);
        let mut history = ColumnHistory::new(&grid);
        history.hidden = map;
        let at = |x: f64, y: f64| {
            let cell = cell(&grid, x, y);
            (grid.row_of(cell), grid.col_of(cell))
        };
        // The ceiling's underside at 3.5 mm: layer 70.
        let k_top = 70;
        let (r0, c0) = at(5.55, 4.25);
        let (r1, c1) = at(5.55, 5.75);
        let host0 = straight(&grid, 0, r0, c0, k_top, 0.1);
        let host1 = straight(&grid, 1, r1, c1, k_top, 0.1);
        let (cr0, cc0) = at(7.55, 4.25);
        let (cr1, cc1) = at(7.55, 5.75);
        let child0 = resting(&grid, 2, &host0, cr0, cc0, k_top, 5);
        let child1 = resting(&grid, 3, &host1, cr1, cc1, k_top, 5);
        let struts = vec![host0, host1, child0, child1];
        let bites = vec![None; 4];
        let violators = [true, true, false, false];
        let limit = 128 << 10;
        let go = |scan: bool, verify: bool| {
            run(
                &grid,
                &history,
                &keep_out,
                &lean,
                &off,
                &plan,
                &cavity,
                struts.clone(),
                &bites,
                Some(&violators),
                scan,
                verify,
                limit,
            )
        };
        let (rounds_field, rounds, rounds_moves, _, _) = go(false, false);
        let (scan_field, scanned_outcome, scan_moves, scanned, memory) = go(true, true);
        assert!(
            rounds.candidates_refused_clearance > 0 && rounds.candidates_hidden > 0,
            "cavity: anti-vacuity: refusals after a hidden member: {rounds:?}"
        );
        assert!(memory.peak <= limit, "cavity: the ledger held {} > {limit}", memory.peak);
        assert!(scanned > 0 && memory.fallbacks > 0, "cavity: the scan ran and fell back: {memory:?}");
        assert!(!memory.chunk_over, "cavity: a chunk's working set passed its share");
        assert_eq!(format!("{scanned_outcome:?}"), format!("{rounds:?}"), "cavity: the outcome is the rounds'");
        assert_eq!((scan_field, scan_moves), (rounds_field, rounds_moves), "cavity: the field is the rounds'");
    }
}

/// The slab over the arm of the kept-arm tests (its underside at 10 mm, the
/// arm's seat at (7.525, 7.525)) cut short at `x_max_mm`: the tip ball of a
/// stub seated there overhangs the edge by a part of its disc, and a lean that
/// tilts the ball back over the slab seats where the stub does not.
fn slab_ending_at(x_max_mm: f64) -> plinth_mesh::Mesh {
    boxes_mesh(&[([0.0, 0.0, 10.0], [x_max_mm, 15.0, 12.0])])
}

/// The edge at which the lean's tip seats (real contact within a pixel of its
/// plan) and the stub's does not: 0.12 mm past the seat, on the side the lean
/// tilts from. Read by the fixture's own contact reads (stub 0.0640 of 0.0679
/// mm2, lean 0.0590 of 0.0604, a pixel 0.0025).
const LEAN_SEATS_EDGE_MM: f64 = 7.645;

/// **THE SLENDERNESS REFUSAL KEEPS THE STANDING ARM UNLESS THE LEAN SEATS WHERE
/// THE ARM DOES NOT** (q12 regressions, 2026-10-12; joint round 5, 2026-10-14,
/// `joint/CLAW-POST.md`). The refusal trades the lean for the arm as it
/// stands. A SHAPE test of that arm (a one-layer stub on the model is "no
/// support") took Bat_Cat_Owl arm 20's 40 degree lean, a 0.10 mm rod 3.4 times
/// over its limit, in place of a stub whose tip touched the claw's 0.018 mm
/// edge: the stub's contact was 0.0000 mm2 and so was the lean's.
///
/// The configuration of the test above (law S = 95, the wall 0.305 mm from
/// the cheapest descent, whose run trips the law). CONTROLS: the arm stands as
/// a straight post to the plate, and is kept, its lean refused; and the same
/// seat stands as a one-layer stub onto the model, the tip seated on a wide
/// slab: kept, its lean refused (the stub is a support by its contact, not
/// by its length). TEST 1: the slab ends 0.12 mm past the seat, the stub's
/// ball overhangs and the lean's seats: the lean is taken, counted as taken.
/// TEST 2: the slab begins past the seat, neither tip reaches the face (arm
/// 20): the stub is kept (a graze of the slab's edge is some contact). TEST 3
/// (round 6): the slab begins out of reach of both balls, the stub touches
/// nothing and so does the lean, so it is reported as holding nothing and is
/// dropped by the caller (not kept as a nub). SABOTAGE (by hand, 2026-10-14: the shape test back, a one-layer stub on the
/// model "is no support"): the stub CONTROL and TEST 2 lose the stub (`moved`
/// 1); (2026-10-12: refuse whenever `lean_too_slender`) TEST 1 is kept (`moved`
/// 0); (round 6: the nub rule off) TEST 3 reports nothing unheld.
#[test]
fn a_stub_is_kept_in_place_of_a_lean_unless_the_lean_seats_where_it_does_not() {
    let grid = grid();
    let history = ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = crate::slenderness::SlendernessLaw::new(95.0, 0.05, 1.0, 32.5);
    let wall = ([2.0, 7.93, 0.5], [13.0, 8.93, 3.0]);
    let wide = boxes_mesh(&[([0.0, 0.0, 10.0], [15.0, 15.0, 12.0]), wall]);
    let short = boxes_mesh(&[([0.0, 0.0, 10.0], [LEAN_SEATS_EDGE_MM, 15.0, 12.0]), wall]);
    let past = boxes_mesh(&[([7.63, 0.0, 10.0], [15.0, 15.0, 12.0]), wall]);
    let gone = boxes_mesh(&[([8.2, 0.0, 10.0], [15.0, 15.0, 12.0]), wall]);
    let plan = plinth_core::LayerPlan::new(0.0, vec![H; 300]).expect("a uniform plan");
    let bite = plinth_support::ShaftSeatBite {
        bite_mm: 0.2,
        approach_deg: 60.0,
        limit_deg: 20.0,
    };
    let run = |mesh: &plinth_mesh::Mesh, stub: bool| {
        let context = slender_context(&grid, &history, &keep_out, &lean, &law);
        let surface = std::sync::OnceLock::new();
        let joins = crate::head_join::HeadJoinContext {
            slender: &context,
            mesh,
            layer_plan: &plan,
            plate_z_mm: 0.0,
            clearance_mm: 0.3,
            standoff_mm: 0.6,
            surface_pitch_mm: 0.1,
            surface_max_bytes: 1 << 28,
            surface: &surface,
            contacts: &crate::contact_growth::MeasureMemo::default(),
        };
        let mut arm = straight(&grid, 0, 150, 150, 200, 0.1);
        if stub {
            arm.k_bottom = 199;
            arm.landing = Landing::Model { layer: 199 };
            arm.radius_mm = vec![0.1; 2];
            arm.carried_mm2 = vec![0.0; 2];
        }
        let mut struts = vec![arm];
        let arms = tilted_arms(&grid, &mut struts);
        let mut outcome = crate::lip_seat::LipSeatOutcome::default();
        let moves = crate::lip_seat::apply(
            &joins,
            &plan,
            0.0,
            &mut struts,
            &arms,
            &[Some(bite)],
            crate::lip_seat::Trigger::SeatBite,
            &[],
            &mut outcome,
        );
        (struts, outcome, moves.len())
    };
    let (kept, outcome, moved) = run(&wide, false);
    assert_eq!(moved, 0, "CONTROL: the straight post is kept: {outcome:?}");
    assert_eq!(outcome.kept_for_slenderness_ids, vec![0], "{outcome:?}");
    assert_eq!(outcome.slender_taken_kept_not_clear, 0, "{outcome:?}");
    assert_eq!(kept[0].k_bottom, 0);
    let (kept, outcome, moved) = run(&wide, true);
    assert_eq!(moved, 0, "CONTROL: a stub whose tip seats is kept: {outcome:?}");
    assert_eq!(outcome.candidates_refused_slender, 1, "PATH HIT: {outcome:?}");
    assert_eq!(outcome.kept_for_slenderness_ids, vec![0], "{outcome:?}");
    assert_eq!(outcome.slender_taken_kept_not_clear, 0, "{outcome:?}");
    assert_eq!(kept[0].k_bottom, 199, "it keeps its one-layer stub");
    assert!(outcome.unheld_pending.is_empty(), "CONTROL: a stub that seats holds something");
    let (kept, outcome, moved) = run(&past, true);
    assert_eq!(moved, 0, "TEST 2: neither tip seats, the stub is kept: {outcome:?}");
    assert_eq!(outcome.candidates_refused_slender, 1, "PATH HIT: {outcome:?}");
    assert_eq!(outcome.slender_taken_kept_not_clear, 0, "{outcome:?}");
    assert_eq!(kept[0].k_bottom, 199, "it keeps its one-layer stub");
    // A tip that grazes the slab's edge (0.0057 mm2 of real contact, its lean
    // 0.0022) holds a little: "better than no support", so it is KEPT, not
    // dropped as a nub.
    assert!(outcome.unheld_pending.is_empty(), "a tip with some contact is a support: {outcome:?}");
    // **THE NUB IS NOT KEPT** (round 6, `NUB-CLAW.md` part 1). Bat_Cat_Owl arm
    // 20 was a one-layer stub whose tip touched the model nowhere (0.0000 mm2),
    // and the lean touched nothing either. TEST 3: the slab begins 0.7 mm past
    // the seat, out of reach of the stub's ball and of the lean's. Neither
    // holds anything: the stub is reported for dropping, with the load it was
    // seated for, and the arm is not counted as kept for its slenderness.
    // SABOTAGE (the old rule, `!(lean_seats && !stands_seats)` alone):
    // `unheld_pending` stays empty and `kept_for_slenderness_ids` is `[0]`.
    let (kept, outcome, moved) = run(&gone, true);
    assert_eq!(moved, 0, "TEST 3: no lean is taken: {outcome:?}");
    assert_eq!(outcome.unheld_pending.len(), 1, "the nub is reported: {outcome:?}");
    assert_eq!(outcome.unheld_pending[0].arm, 0);
    assert!(outcome.kept_for_slenderness_ids.is_empty(), "{outcome:?}");
    assert_eq!(kept[0].k_bottom, 199, "the stub is left for the caller to drop");
    let (taken, outcome, moved) = run(&short, true);
    assert_eq!(moved, 1, "TEST 1: the lean seats where the stub does not: {outcome:?}");
    assert_eq!(outcome.slender_taken_kept_not_clear, 1, "PATH HIT: {outcome:?}");
    assert_eq!(outcome.candidates_refused_slender, 0, "{outcome:?}");
    assert!(outcome.kept_for_slenderness_ids.is_empty(), "{outcome:?}");
    assert!(taken[0].is_leaned(), "the arm leans to the plate");
    assert_eq!(taken[0].landing, Landing::Plate);
}

/// **A LEAN ONLY THE THICKEN RUNG'S TAPER WOULD HOLD IS REFUSED WHILE THE ARM
/// STANDS AS A SUPPORT** (q12 regressions, 2026-10-12, review). A lean leaving
/// its seat face reads its clearance at the gap itself, so the uniform reading
/// (`S` times the thickest radius that keeps the gap) leaves it no room to
/// grow, although the rung's taper (tip kept, root raised away from the face)
/// would hold it clear of the model. Admitting such leans (`ff2e64c`) took ten
/// more leans on Deinonychus than Q12 did, two of them the long slanted runs
/// Q12 removed; the arms the taper reading was for (`make_brace_pair` arm 3,
/// the Morel base's arm 44) emit as before without it.
///
/// A slab over a straight 0.1 mm arm, at S = 50 (a run of about 10.2 mm, which
/// needs a uniform 0.2 mm: more than the measured clearance leaves room for;
/// with no wall the taper stands clear of the slab, the fixture on which
/// `ff2e64c` took the lean). TEST: the arm stands as a straight post to the
/// plate, a support, and is kept, its lean refused. CONTROL: the same seat
/// stands as a one-layer stub under a slab cut short, where the lean seats and
/// the stub does not; the lean is taken (the kept-arm
/// test), so the refusal above is the slenderness test's and not a fixture
/// with no admissible lean. SABOTAGE: the taper admission (`ff2e64c`
/// `lean_too_slender` stage 2) takes the TEST lean, which that commit's own
/// test asserted on this fixture (`moved` 1).
#[test]
fn a_lean_only_the_taper_would_hold_is_refused_while_the_arm_stands() {
    let grid = grid();
    let history = ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = crate::slenderness::SlendernessLaw::new(50.0, 0.05, 1.0, 32.5);
    // The post stands under a wide slab; the CONTROL stub under a slab cut at
    // `LEAN_SEATS_EDGE_MM`, where its lean seats and it does not.
    let wide = boxes_mesh(&[([0.0, 0.0, 10.0], [15.0, 15.0, 12.0])]);
    let short = slab_ending_at(LEAN_SEATS_EDGE_MM);
    let plan = plinth_core::LayerPlan::new(0.0, vec![H; 300]).expect("a uniform plan");
    let bite = plinth_support::ShaftSeatBite {
        bite_mm: 0.2,
        approach_deg: 60.0,
        limit_deg: 20.0,
    };
    let run = |stub: bool| {
        let context = slender_context(&grid, &history, &keep_out, &lean, &law);
        let surface = std::sync::OnceLock::new();
        let joins = crate::head_join::HeadJoinContext {
            slender: &context,
            mesh: if stub { &short } else { &wide },
            layer_plan: &plan,
            plate_z_mm: 0.0,
            clearance_mm: 0.3,
            standoff_mm: 0.6,
            surface_pitch_mm: 0.1,
            surface_max_bytes: 1 << 28,
            surface: &surface,
            contacts: &crate::contact_growth::MeasureMemo::default(),
        };
        let mut arm = straight(&grid, 0, 150, 150, 200, 0.1);
        if stub {
            arm.k_bottom = 199;
            arm.landing = Landing::Model { layer: 199 };
            arm.radius_mm = vec![0.1; 2];
            arm.carried_mm2 = vec![0.0; 2];
        }
        let mut struts = vec![arm];
        let arms = tilted_arms(&grid, &mut struts);
        let mut outcome = crate::lip_seat::LipSeatOutcome::default();
        let moves = crate::lip_seat::apply(
            &joins,
            &plan,
            0.0,
            &mut struts,
            &arms,
            &[Some(bite)],
            crate::lip_seat::Trigger::SeatBite,
            &[],
            &mut outcome,
        );
        (struts, outcome, moves.len())
    };
    let (kept, outcome, moved) = run(false);
    assert_eq!(moved, 0, "the standing post is kept, the lean refused: {outcome:?}");
    assert_eq!(outcome.candidates_refused_slender, 1, "PATH HIT: {outcome:?}");
    assert_eq!(outcome.kept_for_slenderness_ids, vec![0], "{outcome:?}");
    assert_eq!(outcome.slender_taken_kept_not_clear, 0, "{outcome:?}");
    assert_eq!(kept[0].base_cell(), kept[0].cell, "the arm keeps its seat");
    assert_eq!(kept[0].k_bottom, 0, "and its post to the plate");
    let (taken, outcome, moved) = run(true);
    assert_eq!(moved, 1, "CONTROL: a lean exists and a stub whose tip misses its face does not keep it out: {outcome:?}");
    assert_eq!(outcome.slender_taken_kept_not_clear, 1, "{outcome:?}");
    assert!(taken[0].is_leaned(), "the arm leans to the plate");
    assert_eq!(taken[0].landing, Landing::Plate);
}

/// The slab-and-plate fixture of the kept-arm tests (a slab over arm 0, law
/// `S = law_s`), with `mesh` as the model: re-land arm 0 `passes` times on
/// PRODUCTION [`crate::lip_seat::apply`], each pass on a fresh copy of
/// `arm` and all into ONE outcome (as the sweep passes its one outcome through
/// every pass). Returns the last pass's field, the outcome and the moves.
fn lip_seat_passes(
    mesh: &plinth_mesh::Mesh,
    law_s: f64,
    arm: &SweepStrut,
    passes: usize,
) -> (Vec<SweepStrut>, crate::lip_seat::LipSeatOutcome, usize) {
    lip_seat_field_passes(mesh, law_s, std::slice::from_ref(arm), passes)
}

/// [`lip_seat_passes`] on a field of several members: the seat bite is read on
/// the LAST one (the arm under test); the others stand as they are.
fn lip_seat_field_passes(
    mesh: &plinth_mesh::Mesh,
    law_s: f64,
    field: &[SweepStrut],
    passes: usize,
) -> (Vec<SweepStrut>, crate::lip_seat::LipSeatOutcome, usize) {
    let grid = grid();
    let history = ColumnHistory::new(&grid);
    let keep_out = crate::drain::DrainKeepOut::none();
    let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
    let law = crate::slenderness::SlendernessLaw::new(law_s, 0.05, 1.0, 32.5);
    let plan = plinth_core::LayerPlan::new(0.0, vec![H; 300]).expect("a uniform plan");
    let bite = plinth_support::ShaftSeatBite {
        bite_mm: 0.2,
        approach_deg: 60.0,
        limit_deg: 20.0,
    };
    let context = slender_context(&grid, &history, &keep_out, &lean, &law);
    let surface = std::sync::OnceLock::new();
    let joins = crate::head_join::HeadJoinContext {
        slender: &context,
        mesh,
        layer_plan: &plan,
        plate_z_mm: 0.0,
        clearance_mm: 0.3,
        standoff_mm: 0.6,
        surface_pitch_mm: 0.1,
        surface_max_bytes: 1 << 28,
        surface: &surface,
        contacts: &crate::contact_growth::MeasureMemo::default(),
    };
    let mut outcome = crate::lip_seat::LipSeatOutcome::default();
    let mut last = Vec::new();
    let mut moved = 0;
    for _ in 0..passes {
        let mut struts = field.to_vec();
        let arms = tilted_arms(&grid, &mut struts);
        let mut bites = vec![None; field.len()];
        if let Some(last_bite) = bites.last_mut() {
            *last_bite = Some(bite);
        }
        moved = crate::lip_seat::apply(
            &joins,
            &plan,
            0.0,
            &mut struts,
            &arms,
            &bites,
            crate::lip_seat::Trigger::SeatBite,
            &[],
            &mut outcome,
        )
        .len();
        last = struts;
    }
    (last, outcome, moved)
}

/// A straight 0.1 mm arm seated under the slab (layer 200) whose shaft is
/// three layers long, landing as given.
fn short_arm(landing: Landing, k_bottom: usize) -> SweepStrut {
    let mut arm = straight(&grid(), 0, 150, 150, 200, 0.1);
    arm.k_bottom = k_bottom;
    arm.landing = landing;
    arm.radius_mm = vec![0.1; 201 - k_bottom];
    arm.carried_mm2 = vec![0.0; 201 - k_bottom];
    arm
}

/// **AN ARM THAT FLOATS ABOVE ITS GROUND IS NO SUPPORT TO KEEP** (lip-seat
/// guard review, 2026-10-12). The slenderness refusal keeps the arm as it
/// stands, so that arm must stand on something. A clear three-layer arm
/// (`k_top` 200, `k_bottom` 198) that claims `Landing::Plate` ends two layers
/// above the plate in the ledger's own terms (`landing_check::ground_short`):
/// it passed the clearance and length tests and was kept.
///
/// The fixture of `a_lean_only_the_taper_would_hold_is_refused_while_the_arm_stands`
/// (S = 50, a slab, no wall). CONTROL: the same arm landing on the model at
/// its own lower layer (`Model { layer: 198 }`) is rooted, a support: kept,
/// its lean refused (path hit `candidates_refused_slender` 1; the lean is
/// slender, so the next assertion is the guard's and not a fixture with no
/// lean). TEST: the arm claiming the plate at layer 198 is not rooted: the
/// lean is taken. SABOTAGE (by hand, the `rooted[index_of]` term of the
/// kept-arm test removed): the floating arm is kept (`moved` 0).
#[test]
fn an_arm_floating_above_its_ground_is_not_kept_in_place_of_a_lean() {
    let mesh = boxes_mesh(&[([0.0, 0.0, 10.0], [15.0, 15.0, 12.0])]);
    let (kept, outcome, moved) =
        lip_seat_passes(&mesh, 50.0, &short_arm(Landing::Model { layer: 198 }, 198), 1);
    assert_eq!(moved, 0, "CONTROL: the rooted arm is kept: {outcome:?}");
    assert_eq!(outcome.candidates_refused_slender, 1, "{outcome:?}");
    assert_eq!(outcome.slender_taken_kept_not_clear, 0, "{outcome:?}");
    assert_eq!(kept[0].k_bottom, 198);
    let (taken, outcome, moved) = lip_seat_passes(&mesh, 50.0, &short_arm(Landing::Plate, 198), 1);
    assert_eq!(moved, 1, "the floating arm is not kept: {outcome:?}");
    assert_eq!(outcome.slender_taken_kept_not_clear, 1, "PATH HIT: {outcome:?}");
    assert_eq!(outcome.candidates_refused_slender, 0, "{outcome:?}");
    assert!(taken[0].is_leaned(), "the arm leans to the plate");
}

/// **A ONE-LAYER STUB ON A ROOTED MEMBER IS KEPT WHERE ITS LEAN SEATS NO
/// BETTER: ITS OVER-SLENDER LEAN IS REFUSED** (joint round 4, 2026-10-13,
/// `joint/LONG-ROD.md`; joint round 5, 2026-10-14). Bat_Cat_Owl arm 33 stood
/// one layer long on its sibling 32, whose chain reached the plate through the
/// fat slanted post; a shape test took its 12 mm lean to the plate (a 0.10 mm
/// rod, 5.4 times over `S * r`) as if it were a stub on the model wall. A
/// member's chain carries it. The test is now the tips' real contact, not the
/// stub's shape or its landing.
///
/// The fixture of `an_arm_floating_above_its_ground_is_not_kept_in_place_of_a_lean`
/// (S = 50, a slab). The stub rests on post 0, a straight plate post one cell
/// beside its seat, so it leans 45 degrees onto the face. TEST: the slab
/// begins past the seat, neither the stub's tip nor the lean's reaches the face:
/// the lean is refused (nothing taken) and, since round 6, the stub is not
/// kept either: it holds nothing and is reported for dropping. CONTROLS: under a wide slab the 45 degree stub does not
/// seat (0.0 of 0.034 mm2) and the plate lean does, so the lean is taken (a
/// stub on a member has no shield but its contact); and the same stub on a
/// post that floats above the plate is not kept (`rooted`), whatever seats.
#[test]
fn a_one_layer_stub_on_a_rooted_member_is_kept_in_place_of_a_lean() {
    let mesh = boxes_mesh(&[([0.0, 0.0, 10.0], [15.0, 15.0, 12.0])]);
    let past = boxes_mesh(&[([7.63, 0.0, 10.0], [15.0, 15.0, 12.0])]);
    let grid = grid();
    // The post stands one cell beside the stub's seat, as a sibling does; the
    // stub reaches it by a one-layer lean (a landing on a member needs the
    // member at the stub's base cell: `landing_check::stands_at`). Its own
    // column is clear below, as the owl's was.
    let post = straight(&grid, 0, 150, 151, 199, 0.1);
    let post_cell = u32::try_from(post.cell).expect("a small cell");
    let stub = |landing: Landing| {
        let mut arm = straight(&grid, 1, 150, 150, 200, 0.1);
        arm.k_bottom = 199;
        arm.landing = landing;
        if matches!(landing, Landing::Strut { .. }) {
            arm.path = vec![post_cell, u32::try_from(arm.cell).expect("a small cell")];
        }
        arm.radius_mm = vec![0.1; 2];
        arm.carried_mm2 = vec![0.0; 2];
        arm
    };
    let on_post = Landing::Strut { id: 0, layer: 199 };
    let (kept, outcome, moved) =
        lip_seat_field_passes(&past, 50.0, &[post.clone(), stub(on_post)], 1);
    // Round 6 (`NUB-CLAW.md` part 1): neither tip reaches the face (0.0 mm2
    // for the stub, a lean under one pixel), so the stub holds nothing. Its
    // over-slender lean is still not taken (`moved` 0, as in round 5), and
    // the stub is no longer kept as a nub: it is reported for dropping, whatever
    // member it rests on. SABOTAGE (the nub rule off): `unheld_pending` is
    // empty and the stub is kept for its slenderness (`candidates_refused_slender`
    // 1, `kept_for_slenderness_ids` `[1]`), as before.
    assert_eq!(moved, 0, "the over-slender lean is not taken: {outcome:?}");
    assert_eq!(outcome.unheld_pending.len(), 1, "PATH HIT: the stub holds nothing: {outcome:?}");
    assert_eq!(outcome.unheld_pending[0].arm, 1, "{outcome:?}");
    assert!(outcome.kept_for_slenderness_ids.is_empty(), "{outcome:?}");
    assert_eq!(outcome.slender_taken_kept_not_clear, 0, "{outcome:?}");
    assert_eq!(kept[1].landing, on_post, "the caller drops it; until then it keeps its member");
    let (taken, outcome, moved) =
        lip_seat_field_passes(&mesh, 50.0, &[post.clone(), stub(on_post)], 1);
    assert_eq!(moved, 1, "CONTROL: a lean that seats replaces a stub that does not: {outcome:?}");
    assert_eq!(outcome.slender_taken_kept_not_clear, 1, "{outcome:?}");
    assert!(taken[1].is_leaned(), "the stub leans to the plate");
    let mut floating = post;
    floating.k_bottom = 150;
    floating.radius_mm = vec![0.1; 50];
    floating.carried_mm2 = vec![0.0; 50];
    let (_, outcome, moved) = lip_seat_field_passes(&past, 50.0, &[floating, stub(on_post)], 1);
    assert_eq!(moved, 1, "CONTROL: a stub on a floating post is not kept: {outcome:?}");
    assert_eq!(outcome.slender_taken_kept_not_clear, 1, "{outcome:?}");
}

/// **THE COUNT OF LEANS TAKEN FOR WANT OF A SUPPORT IS SUMMED OVER EVERY
/// PASS** (lip-seat guard review, 2026-10-12). The sweep calls the rule
/// several times (the pass, its fixed point, the pass after the head joins)
/// with one outcome, and the count was reset at the start of each call, so
/// the final disclosure read only the last call's. Two passes on the one-layer
/// stub of `a_stub_is_kept_in_place_of_a_lean_unless_the_lean_seats_where_it_does_not`
/// (TEST 1, the slab cut short), each taking its lean, count 2. SABOTAGE (by
/// hand, the reset restored): the count reads 1.
#[test]
fn the_count_of_leans_taken_for_want_of_a_support_sums_over_passes() {
    let mesh = slab_ending_at(LEAN_SEATS_EDGE_MM);
    let stub = short_arm(Landing::Model { layer: 199 }, 199);
    let (_, outcome, moved) = lip_seat_passes(&mesh, 50.0, &stub, 1);
    assert_eq!((moved, outcome.slender_taken_kept_not_clear), (1, 1), "{outcome:?}");
    let (_, outcome, moved) = lip_seat_passes(&mesh, 50.0, &stub, 2);
    assert_eq!(moved, 1, "{outcome:?}");
    assert_eq!(outcome.slender_taken_kept_not_clear, 2, "{outcome:?}");
}

/// **A TAPERED ARM IS READ ON THE SOLID THE FILE DRAWS, NOT ON ITS WIDEST
/// RADIUS ALL ALONG** (lip-seat guard review, 2026-10-12). The kept-arm test
/// asks whether the arm as it stands enters the model. The batched clearance
/// read draws the axis at the arm's widest radius over its whole length, so
/// an arm fat at its root and thin under the seat read inside a wall that its
/// thin shaft stands clear of, was called no support, and its lean was taken
/// in its place (a too-slender lean, around Q1).
///
/// The slab fixture (S = 30: a run of about 10.2 mm is over `S * 0.19`) with a low wall 0.15 mm beside the axis at 5.5 to
/// 7 mm height, where the arm is drawn 0.1 mm (0.19 mm below layer 100, 5 mm
/// up). The widest radius, 0.19 mm, would enter the wall by 0.04 mm; the drawn
/// 0.1 mm shaft stands 0.05 mm clear. CONTROL: the same arm drawn 0.19 mm all
/// along really enters the wall: it is no support and the lean is taken (so
/// the wall does bite at that radius). TEST: the tapered arm is kept, its lean
/// refused (path hit `candidates_refused_slender` 1). SABOTAGE (by hand, the
/// guard reads `before` in place of `standing_drawn`): the tapered arm is
/// called no support and the lean is taken (`moved` 1).
#[test]
fn a_tapered_arm_clear_of_a_wall_on_its_drawn_solid_is_kept() {
    let grid = grid();
    let (ax, ay) = (grid.center_x_mm(150), grid.center_y_mm(150));
    let mesh = boxes_mesh(&[
        ([0.0, 0.0, 10.0], [15.0, 15.0, 12.0]),
        ([ax + 0.15, ay - 2.0, 5.5], [ax + 4.0, ay + 2.0, 7.0]),
    ]);
    let arm_with = |radius_at: &dyn Fn(usize) -> f64| {
        let mut arm = straight(&grid, 0, 150, 150, 200, 0.1);
        arm.radius_mm = (0..=200).map(radius_at).collect();
        arm
    };
    let uniform = arm_with(&|_| 0.19);
    let (_, outcome, moved) = lip_seat_passes(&mesh, 30.0, &uniform, 1);
    assert_eq!(moved, 1, "CONTROL: a 0.19 mm shaft enters the wall, no support: {outcome:?}");
    assert_eq!(outcome.slender_taken_kept_not_clear, 1, "{outcome:?}");
    let taper = arm_with(&|layer| if layer < 100 { 0.19 } else { 0.1 });
    let (kept, outcome, moved) = lip_seat_passes(&mesh, 30.0, &taper, 1);
    assert_eq!(moved, 0, "the tapered arm clears the wall on its drawn solid: {outcome:?}");
    assert_eq!(outcome.candidates_refused_slender, 1, "PATH HIT: {outcome:?}");
    assert_eq!(outcome.slender_taken_kept_not_clear, 0, "{outcome:?}");
    assert_eq!(kept[0].k_bottom, 0, "and keeps its post to the plate");
}

// ---------------------------------------------------------------------------
// The head-first lane's two rulings of 2026-10-06: a head is planned at an
// ESTIMATE of the final tip radius, and a head the final sizing grew across the
// clearance is downgraded to the straight tip.
// ---------------------------------------------------------------------------

mod head_first_rulings {
    use super::*;
    use crate::heads::{
        downgrade_grown_heads, planned_tip_radius_mm, HeadRule, TYPICAL_TIP_RADIUS_MM,
    };

    fn rule(max_radius_mm: f64) -> HeadRule {
        HeadRule {
            gap_mm: 0.3,
            allowance_mm: 0.1,
            cos_lean_max: 0.5373,
            max_radius_mm,
            link_mm: 0.0,
            typical_tip_radius_mm: TYPICAL_TIP_RADIUS_MM,
            max_normal_z: -0.34,
            layer_height_mm: H,
            plate_z_mm: 0.0,
        }
    }

    /// (a) The plan reads the estimate where the seat's own demand asks for
    /// less, the demand where it asks for more, and falls back to the demand
    /// where the estimate's joint would exceed the registered maximum radius.
    #[test]
    fn a_head_is_planned_at_the_larger_of_its_demand_and_the_corpus_estimate() {
        let wide = rule(1.0);
        let no_ball = |_: f64| 0.0;
        assert!((planned_tip_radius_mm(0.05, &wide, no_ball) - TYPICAL_TIP_RADIUS_MM).abs() < 1e-12);
        assert!((planned_tip_radius_mm(0.21, &wide, no_ball) - 0.21).abs() < 1e-12);
        // A joint for r_t = 0.15 is 0.5486 mm: over a 0.5 mm maximum, so the
        // demand's own radius is kept.
        let narrow = rule(0.5);
        assert!((planned_tip_radius_mm(0.05, &narrow, no_ball) - 0.05).abs() < 1e-12);
        // The ball's height moves the joint out and the lever with it: a joint
        // that fits at 0.15 with no ball is refused with a 1.5-factor ball.
        let roomy = rule(0.6);
        assert!((planned_tip_radius_mm(0.05, &roomy, no_ball) - TYPICAL_TIP_RADIUS_MM).abs() < 1e-12);
        assert!((planned_tip_radius_mm(0.05, &roomy, |_| 1.5) - 0.05).abs() < 1e-12);
    }

    /// (neck) The joint is planned `g` beyond the ball's far side, not beyond
    /// the plane: the bare neck `(K - D)·u - r_K - 2R` is the gap. Every term is
    /// read off the planned head, none off the helper under test.
    #[test]
    fn the_planned_joint_leaves_the_gap_as_a_bare_neck_beyond_the_ball() {
        let grid = grid();
        let rule = rule(1.0);
        let (tip, ball_radius) = (0.15, 0.0975);
        let request = |ball_radius_mm: f64| crate::heads::HeadRequest {
            axis_point_mm: [7.0, 7.0, 10.0],
            face_point_mm: [7.0, 7.0, 10.0],
            unit: [0.66, 0.0, -0.75],
            tip_radius_lb_mm: tip,
            ball_radius_mm,
            seat_cell: 0,
            k_top: 200,
        };
        let neck = |head: &Head| {
            let to_k = [
                head.joint_mm[0] - head.contact_mm[0],
                head.joint_mm[1] - head.contact_mm[1],
                head.joint_mm[2] - head.contact_mm[2],
            ];
            let along: f64 = (0..3).map(|axis| to_k[axis] * head.unit[axis]).sum();
            along - head.joint_radius_mm - 2.0 * ball_radius
        };
        let head = crate::heads::plan_head(&request(ball_radius), &rule, &grid).expect("a head");
        assert!(
            (neck(&head) - rule.gap_mm).abs() < 1e-9,
            "the bare neck {} is the gap {}",
            neck(&head),
            rule.gap_mm
        );
        // Anti-vacuity: a head planned with no ball allowance has its neck
        // taken back by the ball's whole height.
        let bare = crate::heads::plan_head(&request(0.0), &rule, &grid).expect("a head");
        assert!(neck(&bare) < rule.gap_mm - 0.15, "the old plan's neck {}", neck(&bare));
    }

    /// (neck) A head whose stand-off clears the gap but whose neck, at the
    /// FINAL tip radius's ball, is under the rasterisation allowance is KEPT
    /// (v19 fix, 2026-10-07: emit and warn, never refuse), with exactly one
    /// warning naming the member, the stand-off, the ball height and the neck;
    /// the same head with a smaller ball needs no warning.
    #[test]
    fn a_grown_head_with_a_joint_slot_keeps_its_head_and_warns_once() {
        let grid = grid();
        // Along u: 0.5 mm * 0.75 / |u| = 0.3754 mm; r_K 0.05: stand-off 0.3254.
        let fresh = || {
            let mut strut = headed(&grid, 0, 150, 150, 200, 190, 0.05);
            *strut.radius_mm.last_mut().expect("a ledger") = 0.1;
            vec![strut]
        };
        // Ball 0.1 mm high at tip 0.1 with factor 0.5: neck 0.2254 over 0.1.
        let mut kept = fresh();
        let quiet = downgrade_grown_heads(&mut kept, 0.1, |_| 0.5);
        assert!(quiet.downgraded.is_empty() && quiet.kept_with_warning.is_empty(), "a neck of 0.2254 stands");
        assert!(!kept[0].head.as_ref().expect("a head").drawn_straight);
        // Ball 0.3 mm high at factor 1.5: neck 0.0254, under the allowance
        // though the stand-off (0.3254) is above the gap (0.3). The head is
        // kept, drawn as a head, and warned about exactly once.
        let mut grown = fresh();
        let before = grown[0].clone();
        let loud = downgrade_grown_heads(&mut grown, 0.1, |_| 1.5);
        assert!(loud.downgraded.is_empty(), "the head is kept: {loud:?}");
        assert_eq!(loud.kept_with_warning.len(), 1, "exactly one warning: {loud:?}");
        let warning = loud.kept_with_warning[0];
        assert_eq!(warning.member, 0);
        assert!((warning.ball_height_mm - 0.3).abs() < 1e-12, "{warning:?}");
        assert!((warning.neck_mm - (warning.standoff_mm - 0.3)).abs() < 1e-12, "{warning:?}");
        assert!((warning.neck_mm - 0.0254).abs() < 1e-3, "{warning:?}");
        assert!(!grown[0].head.as_ref().expect("a head").drawn_straight);
        assert_eq!(grown[0], before, "a kept head changes nothing in the strut");
    }

    /// (v19 fix) A head the ledger cannot draw (no joint slot) is the ONE head
    /// given up, and EVERY slot of the strut ends equal to the area-law ledger
    /// saved before the head minimum: no joint-sized post is left under the
    /// straight tip.
    #[test]
    fn a_downgraded_strut_with_no_joint_slot_ends_with_every_slot_on_its_ledger() {
        let grid = grid();
        let mut strut = headed(&grid, 0, 150, 150, 200, 190, 0.05);
        let ledger: Vec<f64> = (0..strut.radius_mm.len())
            .map(|slot| 0.05 + 1e-4 * f64::from(u32::try_from(slot).expect("small")))
            .collect();
        // The sized (head-minimum) field: joint-sized all down the column.
        strut.radius_mm = vec![0.965; ledger.len()];
        {
            let head = strut.head.as_mut().expect("a head");
            head.ledger_radius_mm.clone_from(&ledger);
            // The joint stands under the strut's lowest layer: no joint slot.
            head.k_joint = 0;
        }
        strut.k_bottom = 5;
        let mut struts = vec![strut];
        let outcome = downgrade_grown_heads(&mut struts, 0.0, |_| 0.0);
        assert_eq!(outcome.downgraded, vec![0], "{outcome:?}");
        assert!(outcome.kept_with_warning.is_empty());
        assert!(struts[0].head.as_ref().is_some_and(|head| head.drawn_straight));
        assert_eq!(struts[0].radius_mm, ledger, "every slot is the ledger's");
    }

    /// (follow) The members landed on a downgraded post stand on its OLD axis
    /// (K to the base); the line now drawn is the straight arm from the seat. The
    /// offset moves a junction from the old line onto the new one at every layer
    /// the post is joinable at. Both lines are derived here from the grid.
    #[test]
    fn a_member_on_a_downgraded_post_follows_the_straight_line_now_drawn() {
        let grid = grid();
        let mut strut = straight(&grid, 0, 150, 150, 200, 0.05);
        let post: Vec<u32> = (0..=190)
            .map(|slot| cell_u32(grid.index(150, 130 + (slot * 10) / 190)))
            .collect();
        let joint_cell = post[190] as usize;
        give_head(&mut strut, 190, joint_cell, Some(post));
        let z =|layer: usize| f64::from(u32::try_from(layer).expect("small")) * H;
        // K over its own cell, 0.5 mm off the contact (the lateral run).
        let k = [
            grid.center_x_mm(grid.col_of(joint_cell)),
            grid.center_y_mm(grid.row_of(joint_cell)),
            z(190),
        ];
        let d = [strut.center_mm[0], strut.center_mm[1], z(200)];
        let base_cell = strut.base_cell();
        let base = [
            grid.center_x_mm(grid.col_of(base_cell)),
            grid.center_y_mm(grid.row_of(base_cell)),
            0.0,
        ];
        {
            let head = strut.head.as_mut().expect("a head");
            head.joint_mm = k;
            head.contact_mm = d;
            head.drawn_straight = true;
        }
        let offset = crate::adapter::downgraded_line_offset(None, &grid, 0.0, H, &strut)
            .expect("a downgraded head has an offset");
        // A member joined at layer 100: on the old line before, on the new line
        // after.
        let at = |from: [f64; 3], to: [f64; 3], layer: usize| {
            let t = (z(layer) - from[2]) / (to[2] - from[2]);
            [from[0] + t * (to[0] - from[0]), from[1] + t * (to[1] - from[1])]
        };
        let (old_xy, new_xy) = (at(base, k, 100), at(base, d, 100));
        let shift = crate::adapter::offset_at(&offset, &strut, 100);
        let moved = [old_xy[0] + shift[0], old_xy[1] + shift[1]];
        assert!(
            (moved[0] - new_xy[0]).hypot(moved[1] - new_xy[1]) < 1e-9,
            "the member lands on the line drawn: {moved:?} against {new_xy:?}"
        );
        // Anti-vacuity: without the follow the member is further off the post
        // than any branch and post radius here (0.05 + 0.1 mm).
        let air = (old_xy[0] - new_xy[0]).hypot(old_xy[1] - new_xy[1]);
        assert!(air > 0.15, "the displacement the follow removes is {air} mm");
        // And a head still drawn is no business of this offset.
        strut.head.as_mut().expect("a head").drawn_straight = false;
        assert!(crate::adapter::downgraded_line_offset(None, &grid, 0.0, H, &strut).is_none());
    }

    /// (b) A joint whose final stand-off is under `g` is KEPT and warned about
    /// (emit and warn); one that stands `g` off, and a straight arm, are
    /// untouched and silent.
    #[test]
    fn a_head_the_final_sizing_grew_across_the_clearance_is_kept_and_warned() {
        let grid = grid();
        // |DK| along u = 0.5 mm * 0.75 = 0.375 mm: a joint of 0.05 stands 0.325
        // off (kept); a joint of 0.2204 stands 0.1546 off (grown).
        let mut struts = vec![
            headed(&grid, 0, 150, 150, 200, 190, 0.05),
            headed(&grid, 1, 150, 160, 200, 190, 0.05),
            straight(&grid, 2, 150, 170, 200, 0.05),
        ];
        // The tip's own radius stays 0.1 (the contact slot), the head slots
        // are raised, and arm 1's joint slot grew.
        for strut in &mut struts {
            *strut.radius_mm.last_mut().expect("a ledger") = 0.1;
        }
        for slot in 191..200 {
            struts[1].radius_mm[slot] = 0.18;
        }
        struts[1].radius_mm[190] = 0.2204;
        let before = struts[0].clone();
        let straight_before = struts[2].clone();
        let after_arm = struts[1].clone();
        let outcome = downgrade_grown_heads(&mut struts, 0.0, |_| 0.0);
        assert!(outcome.downgraded.is_empty(), "no head is given up: {outcome:?}");
        assert_eq!(outcome.kept_with_warning.len(), 1, "one warning, for the grown joint");
        assert_eq!(outcome.kept_with_warning[0].member, 1);
        assert!(outcome.kept_with_warning[0].standoff_mm < 0.3, "{outcome:?}");
        assert_eq!(struts[0], before, "a joint that stands the gap off is untouched");
        assert_eq!(struts[2], straight_before, "a straight arm is untouched");
        assert_eq!(struts[1], after_arm, "the warned head and its sizes are untouched");
        assert!(struts[1].head.as_ref().is_some_and(|head| !head.drawn_straight));
    }

    /// (b) A leaned post keeps its base, its landing and its path (the members
    /// that landed on it still stand on the cells it occupied), so the landing
    /// ledger reads the downgraded field as rooted as it read it headed.
    #[test]
    fn a_downgraded_leaning_arm_keeps_its_base_and_its_route() {
        let grid = grid();
        let mut strut = straight(&grid, 0, 150, 150, 200, 0.05);
        let post: Vec<u32> = (0..=190)
            .map(|slot| cell_u32(grid.index(150, 130 + (slot * 18) / 190)))
            .collect();
        let base = post[0] as usize;
        let joint_cell = post[190] as usize;
        give_head(&mut strut, 190, joint_cell, Some(post));
        // The ledger stops under the joint: the head cannot be drawn.
        strut.radius_mm.truncate(190);
        strut.head.as_mut().expect("a head").ledger_radius_mm = vec![0.05; 190];
        let mut struts = vec![strut];
        let path_before = struts[0].path.clone();
        assert_eq!(downgrade_grown_heads(&mut struts, 0.0, |_| 0.0).downgraded, vec![0]);
        let arm = &struts[0];
        assert!(arm.head.as_ref().is_some_and(|head| head.drawn_straight));
        assert_eq!(arm.base_cell(), base, "the base stays where the post landed");
        assert_eq!(arm.path, path_before);
        assert!(arm.is_leaned());
        let check = check_landings(&struts, H);
        assert!(check.heads_not_intact.is_empty(), "the route is untouched: {check:?}");
    }
}

// ---------------------------------------------------------------------------
// v19 fix, lane L1 (2026-10-07): heads, joints and posts are sized for the
// post they are DRAWN with, not for the 57.5 degree worst case.
// ---------------------------------------------------------------------------

mod actual_lean_sizing {
    use super::*;
    use crate::down_sweep::{resize_heads_to_drawn_posts, SizingPolicy};
    use crate::heads::final_joint_radius_mm;

    const K_TOP: usize = 108;
    const K_JOINT: usize = 100;
    const TIP: f64 = 0.1;
    const COS_MAX: f64 = 0.5373;

    fn sizing() -> SizingPolicy {
        SizingPolicy {
            x_allowed: 12.0,
            tip_radius_min_mm: 0.05,
            tip_radius_max_mm: 1.0,
            owner_growth_ceiling_mm: 0.4,
            alpha: 1.0,
            lamina_reach_mm: 2.13,
            straight_reach: true,
            tip_column_roots: true,
            per_event: true,
            handed_column_ledger: true,
            layer_height_mm: H,
            plate_z_mm: 0.0,
        }
    }

    /// A head whose axis `u = (0, 10, -1)/sqrt 101` is nearly horizontal, with
    /// its joint `K` exactly over a cell centre, standing over a post that
    /// runs to `run_cols` columns away along x (0 = vertical). The ledger has
    /// one slot per layer, 0.06 mm at the tip's neighbours and `TIP` at the
    /// contact, and was saved by hand as the down-sweep would.
    fn field(run_cols: usize) -> (Grid, Vec<SweepStrut>) {
        let grid = grid();
        let (row, col) = (150, 270);
        let joint_cell = grid.index(row, col);
        let z = |layer: usize| f64::from(u32::try_from(layer).expect("small")) * H;
        let k = [grid.center_x_mm(col), grid.center_y_mm(row), z(K_JOINT)];
        // `L u = (0, 0.8, -0.08)`: `|DK|` = 0.8 * sqrt(101) / 10.
        let d = [k[0], k[1] - 0.8, k[2] + 0.08];
        let mut strut = straight(&grid, 0, row, col, K_TOP, 0.06);
        // The seat cell is the contact's own, 16 rows from the joint's.
        strut.cell = grid.index(row - 16, col);
        strut.center_mm = [d[0], d[1]];
        strut.radius_mm.truncate(K_TOP);
        *strut.radius_mm.last_mut().expect("a ledger") = TIP;
        let post: Vec<u32> = (0..=K_JOINT)
            .map(|slot| cell_u32(grid.index(row, col - run_cols + (run_cols * slot) / K_JOINT)))
            .collect();
        give_head(&mut strut, K_JOINT, joint_cell, Some(post));
        {
            let head = strut.head.as_mut().expect("a head");
            let unit = [0.0, 10.0 / 101.0_f64.sqrt(), -1.0 / 101.0_f64.sqrt()];
            head.unit = unit;
            head.contact_mm = d;
            head.joint_mm = k;
            head.ledger_radius_mm = strut.radius_mm.clone();
        }
        (grid, vec![strut])
    }

    fn joint_of(strut: &SweepStrut) -> f64 {
        strut.radius_mm[K_JOINT]
    }

    fn length_of(strut: &SweepStrut) -> f64 {
        let head = strut.head.as_ref().expect("a head");
        (0..3).map(|axis| (head.joint_mm[axis] - head.contact_mm[axis]).powi(2)).sum::<f64>().sqrt()
    }

    /// A vertical post below a near-horizontal head: the exact joint is far
    /// under the 57.5 degree envelope's. A post drawn at 68 degrees (past the
    /// lean law's maximum, no clamp) side-loads the same head and is ABOVE the
    /// envelope. The post under `K` starts from its own section, not the
    /// joint's.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "the contact slot is pinned to the exact area-law tip value the re-size leaves"
    )]
    fn the_resized_joint_follows_the_drawn_post_below_and_above_the_envelope() {
        let (grid, mut vertical) = field(0);
        let envelope = final_joint_radius_mm(TIP, length_of(&vertical[0]), COS_MAX);
        assert_eq!(resize_heads_to_drawn_posts(&grid, &mut vertical, sizing()), 1);
        let law = vertical[0].head.as_ref().expect("a head").law_drawn.expect("a drawn law");
        assert!((law.cos_lean - 1.0).abs() < 1e-9, "a vertical post: {law:?}");
        let exact = joint_of(&vertical[0]);
        assert!(exact < 0.9 * envelope, "vertical post: {exact} against the envelope {envelope}");
        // The post below K starts from its OWN section at the full lever
        // |DK| over its ledger (Craig's ruling A), not from the joint's radius.
        let own = crate::heads::post_section_law_mm(0.06, TIP, length_of(&vertical[0]), law);
        let post_top = vertical[0].radius_mm[K_JOINT - 1];
        assert!((post_top - own).abs() < 1e-12, "post under K {post_top} against its own {own}");
        assert!((post_top - exact).abs() > 1e-6, "not the joint's radius {exact}");
        // 12.5 mm of run over 5.0 mm of height = 250 columns: 68.2 degrees
        // from K to the plate, but the file's plate post ends at its BOSS TOP
        // (about 1.2 mm up), a steeper line of about 73 degrees. The law reads
        // the drawn shaft, not the chord (review finding, 2026-10-08).
        let (grid, mut leaning) = field(250);
        assert_eq!(resize_heads_to_drawn_posts(&grid, &mut leaning, sizing()), 1);
        let law = leaning[0].head.as_ref().expect("a head").law_drawn.expect("a drawn law");
        let lean_deg = law.cos_lean.acos().to_degrees();
        assert!(lean_deg > 72.0 && lean_deg < 74.5, "the drawn lean {lean_deg}");
        assert!(
            joint_of(&leaning[0]) > envelope,
            "a 68 degree post grows the joint past the envelope: {} against {envelope}",
            joint_of(&leaning[0])
        );
        // The head slots follow the same law, and the contact stays the area-law tip.
        assert_eq!(leaning[0].radius_mm[K_TOP - 1], TIP);
        // A second pass with the same input is idempotent (rebuilt from the ledger).
        let before = leaning[0].radius_mm.clone();
        resize_heads_to_drawn_posts(&grid, &mut leaning, sizing());
        assert_eq!(leaning[0].radius_mm, before);
    }
    /// **A host whose seat moves tilts the post of a headed child that lands
    /// on it** (review finding, 2026-10-07). The first sizing runs before the
    /// footprint rule creates that move, so it sized the joint and post for a
    /// vertical post; the adapter then follows the host at the post's base
    /// while `K` stays frozen. `size_heads_for_final_posts`, run after the
    /// moves, must read the post the file draws: the stored law is that of the
    /// line from `K` to the host-shifted base, and the sections grow with it.
    /// SABOTAGE: leave the body of `size_heads_for_final_posts` empty (or drop
    /// its call after `footprint::apply`) and the child keeps the vertical
    /// post's sections, so every assertion below goes red.
    #[test]
    fn a_host_move_after_the_first_sizing_resizes_the_headed_child_for_its_final_post() {
        const LANDING: usize = 40;
        let (grid, mut one) = field(0);
        let mut child = one.remove(0);
        let joint_cell = grid.index(150, 270);
        child.id = 1;
        child.landing = Landing::Strut { id: 0, layer: LANDING };
        child.k_bottom = LANDING;
        child.radius_mm = child.radius_mm.split_off(LANDING);
        child.carried_mm2 = vec![0.0; child.radius_mm.len()];
        child.set_post(child.landing, LANDING, vec![cell_u32(joint_cell); K_JOINT - LANDING + 1]);
        child.head.as_mut().expect("a head").ledger_radius_mm = child.radius_mm.clone();
        assert!(child.head_intact(), "the fixture's head must be intact");
        let host = straight(&grid, 0, 150, 270, K_TOP, 0.3);
        let mut struts = vec![host, child];
        // The first sizing: nothing has moved, the post is vertical.
        assert_eq!(resize_heads_to_drawn_posts(&grid, &mut struts, sizing()), 1);
        let first = struts[1].radius_mm.clone();
        let first_lean = struts[1].head.as_ref().expect("a head").law_drawn.expect("a law").cos_lean;
        assert!((first_lean - 1.0).abs() < 1e-9, "a vertical post: {first_lean}");
        // The footprint rule moves the host's seat by 4.05 mm along x; its
        // axis moves linearly from nothing at its base (layer 0) to the slide
        // at its seat (layer 108), so the child's base, on the host at layer
        // 40, follows by 1.5 mm. The post then runs 3.0 mm down, 1.5 mm across.
        let slide = 4.05;
        struts[0].seat_face = SeatFace {
            point_mm: None,
            shift_mm: [slide, 0.0, 0.0],
            footprint: crate::Footprint::MovedOntoFace,
            contact_ball_radius_mm: None,
            foot: None,
        };
        let mut heads = crate::heads::HeadsOutcome::default();
        crate::size_heads_for_final_posts(&grid, &mut struts, sizing(), 0.1, |_| 0.5, &mut heads, None);
        assert_eq!(heads.resized_after_moves, 1, "the child was sized again");
        let law = struts[1].head.as_ref().expect("a head").law_drawn.expect("a law");
        let across = slide * LANDING as f64 / K_TOP as f64;
        let down = (K_JOINT - LANDING) as f64 * H;
        let expected = down / across.hypot(down);
        assert!(
            (law.cos_lean - expected).abs() < 1e-6,
            "the law reads the final post: {} against {expected}",
            law.cos_lean
        );
        assert!(
            struts[1].radius_mm[K_JOINT - LANDING] > first[K_JOINT - LANDING] + 1e-6,
            "the joint grew for the tilted post: {} against {}",
            struts[1].radius_mm[K_JOINT - LANDING],
            first[K_JOINT - LANDING]
        );
        // A second pass with the moves settled changes nothing.
        let settled = struts[1].radius_mm.clone();
        crate::size_heads_for_final_posts(&grid, &mut struts, sizing(), 0.1, |_| 0.5, &mut heads, None);
        assert_eq!(struts[1].radius_mm, settled);
    }
}

// ---------------------------------------------------------------------------
// Craig 2026-10-08: a head ONLY where a straight shaft would graze the model.
// The conversion re-lands the post from the new joint within the lean law; an
// arm that carries members stays straight and is named.
// ---------------------------------------------------------------------------

mod graze_conversion {
    use super::*;
    use crate::heads::{GrazeReason, HeadsOutcome};

    const K_TOP: usize = 200;
    /// The grazer's old landing on the host, 5 mm under the slab.
    const OLD_LANDING: usize = 100;
    /// Columns between the grazer and its host: 6 mm, 50 degrees off the
    /// normal over the 5 mm drop to the old landing.
    const RUN_COLS: usize = 120;

    /// A host (id 0) standing straight under the slab, the grazer (id 1) 6 mm
    /// away and landed on the host's shaft at layer 100, and, when `carried`,
    /// a third arm (id 2) landed on the grazer.
    fn field(grid: &Grid, carried: bool) -> Vec<SweepStrut> {
        field_on(grid, carried, 1.0)
    }

    /// [`field`] on a host of radius `host_radius`. A trunk as thick as the
    /// registered maximum takes the post: a lander does not make its host fat,
    /// so a host thinner than the joint is not offered.
    fn field_on(grid: &Grid, carried: bool, host_radius: f64) -> Vec<SweepStrut> {
        let host_col = 30;
        let host = straight(grid, 0, 150, host_col, K_TOP, host_radius);
        let mut grazer = straight(grid, 1, 150, host_col + RUN_COLS, K_TOP, 0.05);
        grazer.landing = Landing::Strut { id: 0, layer: OLD_LANDING };
        grazer.k_bottom = OLD_LANDING;
        // A shaft of 0.3 mm over a 0.05 mm tip: a post of its own may stand
        // 15 mm unbraced, so the plate under K (about 8 mm down) is allowed.
        grazer.radius_mm = vec![0.3; K_TOP - OLD_LANDING + 1];
        if let Some(tip) = grazer.radius_mm.last_mut() {
            *tip = 0.05;
        }
        grazer.carried_mm2 = vec![0.0; K_TOP - OLD_LANDING + 1];
        let span = K_TOP - OLD_LANDING;
        grazer.path = (0..=span)
            .map(|slot| cell_u32(grid.index(150, host_col + (RUN_COLS * slot) / span)))
            .collect();
        let mut out = vec![host, grazer];
        if carried {
            let mut rider = straight(grid, 2, 150, host_col + RUN_COLS, 151, 0.05);
            rider.landing = Landing::Strut { id: 1, layer: 150 };
            rider.k_bottom = 150;
            rider.radius_mm = vec![0.05; 2];
            rider.carried_mm2 = vec![0.0; 2];
            out.push(rider);
        }
        out
    }

    /// Run the conversion on `struts` over the slab fixture.
    fn convert(grid: &Grid, struts: &mut [SweepStrut]) -> (usize, HeadsOutcome) {
        convert_on(grid, struts, &slab_mesh())
    }

    /// [`convert`] over any model `mesh`.
    fn convert_on(grid: &Grid, struts: &mut [SweepStrut], mesh: &plinth_mesh::Mesh) -> (usize, HeadsOutcome) {
        let history = ColumnHistory::new(grid);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
        let law = crate::slenderness::SlendernessLaw::new(50.0, 0.05, 1.0, 32.5);
        let context = slender_context(grid, &history, &keep_out, &lean, &law);
        let plan = plinth_core::LayerPlan::new(0.0, vec![H; 300]).expect("a uniform plan");
        let surface = std::sync::OnceLock::new();
        let joins = crate::head_join::HeadJoinContext {
            slender: &context,
            mesh,
            layer_plan: &plan,
            plate_z_mm: 0.0,
            clearance_mm: 0.3,
            standoff_mm: 0.6,
            surface_pitch_mm: 0.5,
            surface_max_bytes: 1 << 28,
            surface: &surface,
            contacts: &crate::contact_growth::MeasureMemo::default(),
        };
        let constants = plinth_core::m0_constants().expect("the registry");
        let profiles = constants.evidence().chitubox_tip_contact_profiles().value();
        let rule = crate::heads::HeadRule {
            gap_mm: 0.3,
            allowance_mm: 0.0692,
            cos_lean_max: lean.cos_max_lean(),
            max_radius_mm: 1.0,
            typical_tip_radius_mm: crate::heads::TYPICAL_TIP_RADIUS_MM,
            max_normal_z: context.seat_faces.max_normal_z(),
            layer_height_mm: H,
            plate_z_mm: 0.0,
            link_mm: 0.0,
        };
        let planes: Vec<f64> = (0..300).map(|layer| (f64::from(layer) + 0.5) * H).collect();
        let slices = plinth_support::perception::slice_stack(
            mesh,
            &planes,
            plinth_support::perception::SliceStackLimits::production(),
        )
        .expect("the slab slices");
        let env = crate::graze::GrazeEnv {
            joins: &joins,
            profiles,
            rule,
            flare_factor: 1.6,
            slices: &slices,
        };
        let mut outcome = HeadsOutcome::default();
        let converted = crate::graze::convert(&env, struts, &mut outcome);
        (converted, outcome)
    }

    /// [`field`] with the grazer standing over `grazer_col` and its host
    /// [`RUN_COLS`] columns away, on whichever side leaves room.
    fn field_at(grid: &Grid, grazer_col: usize) -> Vec<SweepStrut> {
        let host_col = if grazer_col >= RUN_COLS { grazer_col - RUN_COLS } else { grazer_col + RUN_COLS };
        let host = straight(grid, 0, 150, host_col, K_TOP, 1.0);
        let mut grazer = straight(grid, 1, 150, grazer_col, K_TOP, 0.05);
        grazer.landing = Landing::Strut { id: 0, layer: OLD_LANDING };
        grazer.k_bottom = OLD_LANDING;
        grazer.radius_mm = vec![0.3; K_TOP - OLD_LANDING + 1];
        if let Some(tip) = grazer.radius_mm.last_mut() {
            *tip = 0.05;
        }
        grazer.carried_mm2 = vec![0.0; K_TOP - OLD_LANDING + 1];
        let span = K_TOP - OLD_LANDING;
        grazer.path = (0..=span)
            .map(|slot| {
                let col = if grazer_col >= host_col {
                    host_col + ((grazer_col - host_col) * slot) / span
                } else {
                    host_col - ((host_col - grazer_col) * slot) / span
                };
                cell_u32(grid.index(150, col))
            })
            .collect();
        vec![host, grazer]
    }

    /// **F3 (`MODEL-FEET.md`, case B): a straight tip is judged again at the
    /// radius the final sizing gave it.** A tip at column 150 leans 25 degrees
    /// off the slab's underside onto a host 47 columns (2.35 mm) away over the
    /// 5 mm drop. At the planning radius placement reads (0.15 mm,
    /// [`crate::heads::TYPICAL_TIP_RADIUS_MM`]) and up to 0.3 mm its shaft does
    /// not skim, so the conversion asks nothing and the tip stays straight. The
    /// final sizing grows it to 0.4 mm: at that radius and that angle its drawn
    /// shaft grazes the face, and the same conversion, asked again on the sized
    /// field, gives it a head. That is the second conversion the pipeline now
    /// runs after the final sizing; SABOTAGE: without it the tip is judged only
    /// at the radius it had first, which is the first assertion here.
    #[test]
    fn a_tip_the_final_sizing_grew_into_a_graze_is_converted_when_asked_again() {
        let grid = grid();
        let sized_at = |radius: f64| {
            let mut field = field_at_run(&grid, 150, 47);
            for slot in &mut field[1].radius_mm {
                *slot = radius;
            }
            field
        };
        for radius in [crate::heads::TYPICAL_TIP_RADIUS_MM, 0.3] {
            let mut planned = sized_at(radius);
            let (converted, outcome) = convert(&grid, &mut planned);
            assert_eq!(
                (outcome.graze_asked, converted),
                (0, 0),
                "at {radius} mm it does not graze, and stays straight: {outcome:?}"
            );
        }
        let mut sized = sized_at(0.4);
        let (converted, outcome) = convert(&grid, &mut sized);
        assert_eq!(outcome.graze_asked, 1, "at its final radius it grazes: {outcome:?}");
        assert_eq!(converted, 1, "asked again, it is given a head: {outcome:?}");
        assert_eq!(outcome.graze_converted, vec![1]);
        assert!(sized[1].head.as_ref().is_some_and(|head| !head.drawn_straight && head.planned_tip_radius_mm >= 0.4 - 1e-9));
    }

    /// [`field_at`] with the host `run` columns from the grazer.
    fn field_at_run(grid: &Grid, grazer_col: usize, run: usize) -> Vec<SweepStrut> {
        let host_col = grazer_col - run;
        let host = straight(grid, 0, 150, host_col, K_TOP, 1.0);
        let mut grazer = straight(grid, 1, 150, grazer_col, K_TOP, 0.05);
        grazer.landing = Landing::Strut { id: 0, layer: OLD_LANDING };
        grazer.k_bottom = OLD_LANDING;
        grazer.radius_mm = vec![0.3; K_TOP - OLD_LANDING + 1];
        grazer.carried_mm2 = vec![0.0; K_TOP - OLD_LANDING + 1];
        let span = K_TOP - OLD_LANDING;
        grazer.path = (0..=span)
            .map(|slot| cell_u32(grid.index(150, host_col + ((grazer_col - host_col) * slot) / span)))
            .collect();
        vec![host, grazer]
    }

    /// **A head never slides, so a tip the footprint rule would slide is not
    /// given one** (quality lane Q2, Deinonychus arm 133). Two identical
    /// grazers differ in one thing, where they stand on the slab's underside:
    /// in the middle the whole contact disc lies on the face and the tip gets
    /// its head; 0.075 mm from the slab's edge the disc hangs off the face, the
    /// footprint rule would slide the straight tip onto the face, and a head
    /// converted at the seat cell would keep the disc off the face and lose
    /// the slide's reach. That tip stays straight, named `SlidesToItsFace`.
    /// SABOTAGE: drop the `slides_to_face` test in `graze::convert` and the
    /// edge tip converts (the second block goes red); the middle tip is the
    /// control that the grade does not refuse everything.
    #[test]
    fn a_grazing_tip_whose_disc_hangs_off_its_face_is_left_straight_for_the_slide() {
        let grid = grid();
        // Control: in the middle of the face, the disc is on it.
        let mut middle = field_at(&grid, 150);
        let (converted, outcome) = convert(&grid, &mut middle);
        assert_eq!(outcome.graze_asked, 1, "anti-vacuity: it grazes: {outcome:?}");
        assert_eq!(converted, 1, "a tip with its disc on the face is converted: {outcome:?}");
        // The edge: column 1 stands 0.075 mm inside the slab's x = 0 wall.
        let mut edge = field_at(&grid, 1);
        let before = edge[1].clone();
        let (converted, outcome) = convert(&grid, &mut edge);
        assert_eq!(outcome.graze_asked, 1, "anti-vacuity: it grazes: {outcome:?}");
        assert_eq!(converted, 0, "{outcome:?}");
        assert_eq!(edge[1], before, "the straight tip is untouched, for the slide");
        assert_eq!(outcome.graze_reasons, vec![(1, GrazeReason::SlidesToItsFace)]);
    }

    /// RED before the lane (nothing converted a grazing straight tip). A tip
    /// that leans 50 degrees off the slab's underside, landed on a host 6 mm
    /// away, grazes the slab: it gets a head, and its post is RE-LANDED lower
    /// on the same host, inside the lean law, because the joint stands about
    /// 1.8 mm under the seat and the old landing is now too steep for it.
    /// SABOTAGE: drop the lean filter in `candidates_for` and the post keeps
    /// the highest layer, which breaks the law (the last assertion goes red);
    /// drop the census and nothing is converted (the first one does).
    #[test]
    fn a_grazing_tip_gets_a_head_and_its_post_is_re_landed_lower_within_the_lean_law() {
        let grid = grid();
        let mut struts = field(&grid, false);
        let (converted, outcome) = convert(&grid, &mut struts);
        assert_eq!(outcome.graze_asked, 1, "anti-vacuity: the census names the grazer: {outcome:?}");
        assert_eq!(converted, 1, "{outcome:?}");
        assert_eq!(outcome.graze_converted, vec![1]);
        assert_eq!(outcome.built, 1);
        let grazer = &struts[1];
        let head = grazer.head.as_ref().expect("the grazer carries a head now");
        assert!(grazer.head_intact(), "the head is carried by the path");
        assert!(struts[0].head.is_none(), "the square host keeps its straight tip");
        let Landing::Strut { id: 0, layer } = grazer.landing else {
            panic!("the post stays on its host: {:?}", grazer.landing);
        };
        assert!(layer < OLD_LANDING, "re-landed LOWER on the host: {layer}");
        assert_eq!(grazer.k_bottom, layer);
        let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
        let lateral = crate::cell_distance_mm(&grid, head.joint_cell as usize, grazer.base_cell());
        assert!(
            lean.descent_cost_mm(head.k_joint - layer, lateral).is_some(),
            "the post from K ({} layers, {lateral:.2} mm sideways) is inside the lean law",
            head.k_joint - layer
        );
        let check = check_landings(&struts, H);
        assert!(check.clean(), "{check:?}");
    }

    /// An arm another member rests on is not converted: moving its post would
    /// move theirs. It stays straight and the run names it with the reason.
    /// SABOTAGE: drop the `carries` guard and the carrier is converted.
    #[test]
    fn a_grazing_tip_that_carries_a_member_stays_straight_and_is_named() {
        let grid = grid();
        let mut struts = field(&grid, true);
        let before = struts[1].clone();
        let (converted, outcome) = convert(&grid, &mut struts);
        assert_eq!(converted, 0, "{outcome:?}");
        assert!(outcome.graze_asked >= 1, "anti-vacuity: it grazes: {outcome:?}");
        assert_eq!(struts[1], before, "the carrier is untouched");
        assert!(
            outcome.graze_reasons.contains(&(1, GrazeReason::CarriesMembers)),
            "named with its reason: {outcome:?}"
        );
    }

    /// A host thinner than the joint would be RAISED by the post, so no host
    /// post is offered; the plate under the joint is (brief 3e.3), and a thin
    /// host no longer ends the search. The post goes down to the plate under K,
    /// a vertical post. SABOTAGE: drop the plate candidate in `candidates_for`
    /// and the arm stays straight with `HostTooThin` (the earlier behaviour).
    #[test]
    fn a_grazing_tip_whose_host_is_thinner_than_the_joint_lands_on_the_plate_under_k() {
        let grid = grid();
        let mut struts = field_on(&grid, false, 0.2);
        let (converted, outcome) = convert(&grid, &mut struts);
        assert_eq!(outcome.graze_asked, 1, "anti-vacuity: it grazes: {outcome:?}");
        assert_eq!(converted, 1, "{outcome:?}");
        assert_eq!(outcome.graze_converted, vec![1]);
        let grazer = &struts[1];
        let head = grazer.head.as_ref().expect("the grazer carries a head now");
        assert_eq!(grazer.landing, Landing::Plate, "the post stands on the plate");
        assert_eq!(grazer.k_bottom, 0);
        assert_eq!(grazer.base_cell(), head.joint_cell as usize, "under K");
        assert!(grazer.head_intact(), "the head is carried by the path");
        let check = check_landings(&struts, H);
        assert!(check.clean(), "{check:?}");
    }

    /// The plate candidate is refused alone when its new root would lap a root
    /// already on the plate: the thin-host arm then stays straight with the
    /// reason. A plate arm (id 2, root radius 1.0 mm, 2.75 mm from the joint's
    /// column and clear of the post's shaft) is the other root.
    /// SABOTAGE: drop the root test in `choose_post` and the arm converts.
    #[test]
    fn a_plate_root_that_would_lap_another_is_refused_and_the_arm_stays_straight() {
        let grid = grid();
        // Where the unblocked field puts the root: the blocker stands 2.75 mm
        // (55 columns) from it, on the far side from the host.
        let mut probe = field_on(&grid, false, 0.2);
        let (probed, _) = convert(&grid, &mut probe);
        assert_eq!(probed, 1, "anti-vacuity: unblocked, the arm lands on the plate");
        let root_col = grid.col_of(probe[1].base_cell());
        let mut struts = field_on(&grid, false, 0.2);
        struts.push(straight(&grid, 2, 150, root_col + 40, 100, 1.0));
        let (converted, outcome) = convert(&grid, &mut struts);
        assert_eq!(outcome.graze_asked, 1, "anti-vacuity: it grazes: {outcome:?}");
        assert_eq!(converted, 0, "{outcome:?}");
        assert!(struts[1].head.is_none());
        assert_eq!(outcome.graze_reasons, vec![(1, GrazeReason::HostTooThin)]);
    }

    /// A post of its own that would stand longer than the slenderness law lets
    /// its section stand unbraced is not offered: the plate under K is about
    /// 8 mm down and a 0.05 mm shaft may stand 2.5 mm. (Measured on the tall
    /// stilt plate: 40 posts of 32 mm at 0.29 mm radius left 30 runs over the
    /// limit.) SABOTAGE: drop the run test in `choose_post` and it converts.
    #[test]
    fn a_plate_post_longer_than_its_unbraced_limit_is_not_offered() {
        let grid = grid();
        let mut struts = field_on(&grid, false, 0.2);
        struts[1].radius_mm = vec![0.05; struts[1].radius_mm.len()];
        let (converted, outcome) = convert(&grid, &mut struts);
        assert_eq!(outcome.graze_asked, 1, "anti-vacuity: it grazes: {outcome:?}");
        assert_eq!(converted, 0, "{outcome:?}");
        assert!(struts[1].head.is_none());
        assert_eq!(outcome.graze_reasons, vec![(1, GrazeReason::HostTooThin)]);
    }

    /// A grazing PLATE arm is converted too: its post moves to the plate under
    /// K (brief 3e.3: the candidate list is not empty for a plate arm), and its
    /// old root leaves the plate. The arm leans 50 degrees off the slab's
    /// normal from the plate to the slab. SABOTAGE: return early for a
    /// non-host arm in `candidates_for` and it stays straight.
    #[test]
    fn a_grazing_plate_arm_gets_a_head_and_a_post_on_the_plate_under_k() {
        let grid = grid();
        let run_cols = 238;
        let col = 270;
        let mut arm = straight(&grid, 0, 150, col, K_TOP, 0.3);
        if let Some(tip) = arm.radius_mm.last_mut() {
            *tip = 0.05;
        }
        arm.path = (0..=K_TOP)
            .map(|slot| cell_u32(grid.index(150, col - run_cols + (run_cols * slot) / K_TOP)))
            .collect();
        let mut struts = vec![arm];
        let (converted, outcome) = convert(&grid, &mut struts);
        assert_eq!(outcome.graze_asked, 1, "anti-vacuity: it grazes: {outcome:?}");
        assert_eq!(converted, 1, "{outcome:?}");
        let arm = &struts[0];
        let head = arm.head.as_ref().expect("the arm carries a head now");
        assert_eq!(arm.landing, Landing::Plate);
        assert_eq!(arm.base_cell(), head.joint_cell as usize, "the post stands under K");
        assert!(arm.head_intact());
    }
}
