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
        thickened_mm: Vec::new(),
        seat_on_strut: None,
        k_top,
        k_bottom: 0,
        landing: Landing::Plate,
        interior: false,
        radius_mm: vec![radius; k_top + 1],
        carried_mm2: vec![0.0; k_top + 1],
        covered_demand: Vec::new(),
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
        &mut struts,
        &arms,
        &plan,
        0.0,
        H,
        &limits,
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
    let surface = std::cell::OnceCell::new();
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
    let surface = std::cell::OnceCell::new();
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
                crate::lip_seat::Trigger::Clearance { violators: &violators }
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
        };
        let mut heads = crate::heads::HeadsOutcome::default();
        crate::size_heads_for_final_posts(&grid, &mut struts, sizing(), 0.1, |_| 0.5, &mut heads);
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
        crate::size_heads_for_final_posts(&grid, &mut struts, sizing(), 0.1, |_| 0.5, &mut heads);
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
        let history = ColumnHistory::new(grid);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(H, 45.0, 25.0);
        let law = crate::slenderness::SlendernessLaw::new(50.0, 0.05, 1.0, 32.5);
        let context = slender_context(grid, &history, &keep_out, &lean, &law);
        let mesh = slab_mesh();
        let plan = plinth_core::LayerPlan::new(0.0, vec![H; 300]).expect("a uniform plan");
        let surface = std::cell::OnceCell::new();
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
        };
        let env = crate::graze::GrazeEnv { joins: &joins, profiles, rule, flare_factor: 1.6 };
        let mut outcome = HeadsOutcome::default();
        let converted = crate::graze::convert(&env, struts, &mut outcome);
        (converted, outcome)
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
