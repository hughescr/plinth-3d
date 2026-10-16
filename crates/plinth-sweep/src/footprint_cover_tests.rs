//! **THE TIP COVER** ([`super::TipCover`], lane tc 2026-10-16) on a hand-made
//! field: a flat slab 3 mm wide and 6 mm long, its underside at z 1.0 (layer
//! 20), a raster with a 1 mm margin on its -x side, and two tips:
//!
//! * the TEST tip, either seated 0.3 mm out past the slab's edge (its whole
//!   disc over air: the footprint rule slides it in onto the face, further
//!   than its ball reaches, so its ball lets go of the seat it was placed for
//!   and the tip-cover check walks it), or seated over the slab's first
//!   column (half its disc off: the slide is short and the ball still holds
//!   its seat, so the check does not walk it);
//! * a control tip well inside the face, never moved.
//!
//! The repair tests read a SCRIPTED walk (a closure that strands one cell
//! whenever the slid root stands further than a set distance from it), so
//! each repair rung is reached on purpose; the planner's own walk
//! ([`crate::demand_replay::stranded_by_moves`]) is held to hand-derived
//! geometry in `demand_replay`'s tests, and to the Cylinder lid in the lane's
//! model runs.

use std::cell::RefCell;

use plinth_support::perception::{slice_stack, SliceSolid, SliceStackLimits};
use plinth_support::SweepArm;

use super::{apply, FootprintOutcome, MoveLimits, TipCover};
use crate::demand_replay::{MovedSeat, StrandedCell};
use crate::grid::{lay_out_at, Grid};
use crate::head_carrier_tests::{straight, H};
use crate::{Footprint, SweepStrut};

const COLS: usize = 80;
const ROWS: usize = 120;
const LAYERS: usize = 40;
/// The raster's left edge: a 1 mm margin past the slab's edge at x 0.
const ORIGIN_X: f64 = -1.0;
/// The slab's underside, z 1.0, is the bottom of layer 20.
const K_TOP: usize = 20;
/// The test tip's row: the slab's middle, y 3.025.
const ROW: usize = 60;
/// The test tip over air, 0.3 mm past the edge (x -0.275), and over the
/// slab's first column (x 0.025).
const OVER_AIR: usize = 17;
const OVER_EDGE: usize = 20;
const TIP_RADIUS_MM: f64 = 0.1;

fn grid() -> Grid {
    lay_out_at(ORIGIN_X, 0.0, H, COLS, ROWS)
}

/// The slab: x 0..3, y 0..6, z 1.0..1.5.
fn slab() -> plinth_mesh::Mesh {
    // `make_box` centres x and y on the translate and stands the box on its z.
    plinth_mesh::make_box([3.0, 6.0, 0.5], [1.5, 3.0, 1.0]).expect("a box")
}

fn slices() -> Vec<Vec<SliceSolid>> {
    let planes: Vec<f64> =
        (0..LAYERS).map(|layer| (f64::from(u32::try_from(layer).expect("small")) + 0.5) * H).collect();
    slice_stack(&slab(), &planes, SliceStackLimits::production()).expect("the slab slices")
}

/// The test tip (member 0) over column `col`, and the control (member 1).
fn field(grid: &Grid, col: usize) -> Vec<SweepStrut> {
    vec![
        straight(grid, 0, ROW, col, K_TOP, TIP_RADIUS_MM),
        straight(grid, 1, ROW, 60, K_TOP, TIP_RADIUS_MM),
    ]
}

fn arms(grid: &Grid, struts: &mut [SweepStrut]) -> Vec<SweepArm> {
    struts
        .iter_mut()
        .map(|strut| {
            strut.seat_normal = [0.0, 0.0, -1.0];
            let (seat, base) = crate::adapter::drawn_ends(grid, 0.0, H, strut);
            crate::adapter::sweep_arm(strut, seat, base, Some(seat), strut.radius_mm.clone())
        })
        .collect()
}

/// The footprint rule over the field with the test tip over `col`, with or
/// without a tip-cover check.
fn run_at(col: usize, cover: Option<TipCover<'_>>) -> (FootprintOutcome, Vec<SweepStrut>) {
    let grid = grid();
    let slices = slices();
    let mut struts = field(&grid, col);
    let arms = arms(&grid, &mut struts);
    let plan = plinth_core::LayerPlan::new(0.0, vec![H; LAYERS]).expect("a uniform plan");
    let keep_out = crate::drain::DrainKeepOut::none();
    let limits = MoveLimits {
        max_lean: 1.6,
        chord_slack: vec![f64::INFINITY; struts.len()],
        keep_out: &keep_out,
    };
    let outcome = apply(
        &grid,
        &slices,
        None,
        &mut struts,
        &arms,
        &plan,
        0.0,
        H,
        &limits,
        false,
        cover,
        |members: &[usize], _: &[SweepArm], _: &[[f64; 3]]| vec![[0.0, 0.0, -1.0]; members.len()],
    );
    (outcome, struts)
}

fn run(cover: Option<TipCover<'_>>) -> (FootprintOutcome, Vec<SweepStrut>) {
    run_at(OVER_AIR, cover)
}

/// Where member 0's root stands: its column plus its slide.
fn root(strut: &SweepStrut) -> [f64; 2] {
    let shift = if strut.seat_face.footprint == Footprint::MovedOntoFace {
        strut.seat_face.shift_mm
    } else {
        [0.0; 3]
    };
    [strut.center_mm[0] + shift[0], strut.center_mm[1] + shift[1]]
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The slide the rule takes with no check, with the test tip over `col`:
/// the field the scripted walks are set against. Anti-vacuity for every test
/// below.
fn unchecked_at(col: usize) -> (FootprintOutcome, Vec<SweepStrut>) {
    let (outcome, struts) = run_at(col, None);
    assert_eq!(
        struts[0].seat_face.footprint,
        Footprint::MovedOntoFace,
        "the fixture slides its test tip: {outcome:?}"
    );
    assert_eq!(struts[1].seat_face.footprint, Footprint::OnFace, "the control is on its face");
    (outcome, struts)
}

fn unchecked() -> (FootprintOutcome, Vec<SweepStrut>) {
    unchecked_at(OVER_AIR)
}

/// A scripted walk: the cell at `target` is stranded, blamed on member 0,
/// whenever member 0's slid root stands further than `limit` from it. Every
/// set of slid seats it is asked about is recorded.
fn scripted(
    target: [f64; 2],
    limit: f64,
    asked: &RefCell<Vec<Vec<MovedSeat>>>,
) -> impl FnMut(&[SweepStrut], &[MovedSeat]) -> Vec<StrandedCell> + '_ {
    move |_: &[SweepStrut], moved: &[MovedSeat]| {
        asked.borrow_mut().push(moved.to_vec());
        let grid = grid();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a fixture coordinate inside the grid"
        )]
        let cell = grid.index(
            grid.row_coord(target[1]).floor() as usize,
            grid.col_coord(target[0]).floor() as usize,
        );
        moved
            .iter()
            .filter(|seat| seat.index == 0 && distance(seat.xy_mm, target) > limit)
            .map(|_| StrandedCell { layer: K_TOP, cell, u: 1.5, blamed: 0, surface: true })
            .collect()
    }
}

/// A slide whose ball still holds the seat it was placed for leaves the tip
/// touching the cell it was rooted at: it is never walked, and the field is
/// the field the rule leaves with no check. SABOTAGE: walking it anyway (the
/// filter removed) asks the walk once.
#[test]
fn a_slide_that_still_holds_its_seat_is_not_walked() {
    let (_, plain) = unchecked_at(OVER_EDGE);
    let asked = RefCell::new(Vec::new());
    let mut walk = scripted([2.9, 0.1], 0.0, &asked);
    let (outcome, struts) =
        run_at(OVER_EDGE, Some(TipCover { reach_mm: 2.13, passes: 2, stranded: &mut walk }));
    assert!(asked.borrow().is_empty(), "walked: {:?}", asked.borrow());
    assert_eq!(outcome.cover.slides_holding_seat, 1, "{:?}", outcome.cover);
    assert!(!outcome.cover.enabled && outcome.cover.checks == 0);
    assert_eq!(struts, plain, "the field is exactly the unchecked one");
}

/// A slide that lets go of its seat but keeps its cover is left alone: the
/// check runs once, finds nothing, and the field is the field the rule leaves
/// with no check.
#[test]
fn a_slide_that_keeps_its_cover_is_left_alone() {
    let (plain, plain_struts) = unchecked();
    let asked = RefCell::new(Vec::new());
    let mut walk = |_: &[SweepStrut], moved: &[MovedSeat]| {
        asked.borrow_mut().push(moved.to_vec());
        Vec::new()
    };
    let (outcome, struts) = run(Some(TipCover { reach_mm: 2.13, passes: 2, stranded: &mut walk }));
    assert_eq!(asked.borrow().len(), 1, "one walk over the slid field: {:?}", outcome.cover);
    assert_eq!(asked.borrow()[0].len(), 1, "the one slid tip was walked");
    assert_eq!(struts, plain_struts, "the field is exactly the unchecked one");
    let cover = &outcome.cover;
    assert!(cover.enabled && cover.checks == 1, "{cover:?}");
    assert_eq!(cover.slides_holding_seat, 0, "the slide let go of its seat");
    assert_eq!(cover.found_cells, 0);
    assert!(cover.reseated.is_empty() && cover.undone.is_empty() && cover.left.is_empty());
    assert_eq!(
        FootprintOutcome { cover: super::TipCoverOutcome::default(), ..outcome },
        plain,
        "every other count is the unchecked one's"
    );
}

/// Repair (a): the slide strands a cell 0.4 mm across the face from where it
/// lands; the tip is searched again and takes a later on-face candidate whose
/// root reaches the cell, and the next walk finds nothing. SABOTAGE: with the
/// check off the tip keeps the stranding slide (the anti-vacuity run).
#[test]
fn a_slide_that_strands_demand_is_reseated_where_it_covers_it() {
    let (_, plain) = unchecked();
    let landed = root(&plain[0]);
    let target = [landed[0], landed[1] + 0.4];
    let limit = 0.3;
    assert!(distance(landed, target) > limit, "the unchecked slide strands the target");
    let asked = RefCell::new(Vec::new());
    let mut walk = scripted(target, limit, &asked);
    let (outcome, struts) = run(Some(TipCover { reach_mm: limit, passes: 2, stranded: &mut walk }));
    let cover = &outcome.cover;
    assert_eq!(struts[0].seat_face.footprint, Footprint::MovedOntoFace, "{cover:?}");
    assert!(
        distance(root(&struts[0]), target) <= limit,
        "the reseated root {:?} reaches the target {target:?}: {cover:?}",
        root(&struts[0])
    );
    assert_eq!(cover.reseated, vec![0], "{cover:?}");
    assert!(cover.undone.is_empty() && cover.left.is_empty(), "{cover:?}");
    assert_eq!(cover.found_cells, 1);
    assert_eq!(cover.slides_taken_back, 0, "an onto-face slide stands until it is replaced");
    assert!(cover.kept.is_empty(), "{cover:?}");
    assert_eq!(cover.checks, 2, "one walk found it, one confirmed the repair");
    assert_eq!(struts[1], plain[1], "the control tip is untouched");
}

/// No other seat on the face reaches the cell: the tip's disc hung off its
/// face before the slide, so undoing it would take contact area from the tip;
/// the slide the rule first chose stands, the field is the unchecked one, and
/// the stranded area is named in `left` (warned), never a refusal. The
/// re-search ran and the walks stop. SABOTAGE: undoing the slide instead
/// (the rule before the keep) leaves the tip `MoveRefused` with its disc
/// hanging off the face.
#[test]
fn an_onto_face_slide_no_reseat_can_save_is_kept_and_warned() {
    let (_, plain) = unchecked();
    let target = [2.9, 0.1];
    let asked = RefCell::new(Vec::new());
    let mut walk = scripted(target, 0.05, &asked);
    let (outcome, struts) = run(Some(TipCover { reach_mm: 0.05, passes: 2, stranded: &mut walk }));
    let cover = &outcome.cover;
    assert_eq!(struts, plain, "the slide the rule chose stands: {cover:?}");
    assert_eq!(cover.kept, vec![0], "{cover:?}");
    assert!(cover.undone.is_empty() && cover.reseated.is_empty(), "{cover:?}");
    assert_eq!(cover.left.len(), 1, "the stranded area is warned: {cover:?}");
    assert_eq!(cover.left[0].arm, 0);
    assert!(cover.left[0].surface_mm2 > 0.0 && cover.left[0].slide_mm > 0.0, "{cover:?}");
    assert!(cover.candidates_refused > 0, "the re-search ran: {cover:?}");
    // The first walk, and one after the passes to name what the kept slide
    // strands; bounded by the passes.
    assert!((2..=2 + 2).contains(&cover.checks), "bounded by the passes: {cover:?}");
    assert_eq!(asked.borrow().len(), cover.checks);
}

/// The repair does not depend on thread timing: the reseat run, on pools of
/// one and of eight threads, gives the same field, the same counts and the
/// same walks asked.
#[test]
fn the_repair_is_the_same_on_any_pool() {
    let (_, plain) = unchecked();
    let landed = root(&plain[0]);
    let target = [landed[0], landed[1] + 0.4];
    let once = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().expect("a pool");
        pool.install(|| {
            let asked = RefCell::new(Vec::new());
            let mut walk = scripted(target, 0.3, &asked);
            let (outcome, struts) =
                run(Some(TipCover { reach_mm: 0.3, passes: 2, stranded: &mut walk }));
            drop(walk);
            (format!("{outcome:?}"), struts, format!("{:?}", asked.into_inner()))
        })
    };
    let one = once(1);
    assert!(one.0.contains("reseated: [0]"), "anti-vacuity: the run repaired: {}", one.0);
    for _ in 0..3 {
        assert_eq!(once(8), one);
    }
    assert_eq!(once(1), one);
}
