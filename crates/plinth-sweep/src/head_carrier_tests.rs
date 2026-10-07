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
    struts: &mut Vec<SweepStrut>,
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
