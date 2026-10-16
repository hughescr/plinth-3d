//! **THE TIP COVER** ([`super::TipCover`], lane tc 2026-10-16) on a hand-made
//! field: a flat slab 3 mm wide and 6 mm long, its underside at z 1.0 (layer
//! 20), and one tip seated at the slab's edge so its contact disc hangs off
//! the face. The footprint rule slides that tip inward along the face; the
//! tests ask what the tip-cover check does with the slide.
//!
//! The repair tests read a SCRIPTED walk (a closure that strands one cell
//! whenever the slid root stands further than a set distance from it), so
//! each repair rung is reached on purpose. The last test reads the REAL walk
//! ([`crate::demand_replay::stranded_by_moves`]) on the same slab: the slide
//! strands demand, the repair covers it, and the sabotage (the check off)
//! leaves it stranded.

use std::cell::RefCell;

use plinth_support::perception::{slice_stack, SliceSolid, SliceStackLimits};
use plinth_support::SweepArm;

use super::{apply, FootprintOutcome, MoveLimits, TipCover};
use crate::demand_replay::{MovedSeat, ReplayContext, StrandedCell};
use crate::grid::{lay_out_at, Grid};
use crate::head_carrier_tests::{straight, H};
use crate::{Footprint, SweepStrut};

const COLS: usize = 60;
const ROWS: usize = 120;
const LAYERS: usize = 40;
/// The slab's underside, z 1.0, is the bottom of layer 20.
const K_TOP: usize = 20;
/// The edge tip's row: the slab's middle, y 3.025.
const ROW: usize = 60;

fn grid() -> Grid {
    lay_out_at(0.0, 0.0, H, COLS, ROWS)
}

/// The slab: x 0..3, y 0..6, z 1.0..1.5.
fn slab() -> plinth_mesh::Mesh {
    // `make_box` centres x and y on the translate and stands the box on its z.
    plinth_mesh::make_box([3.0, 6.0, 0.5], [1.5, 3.0, 1.0]).expect("a box")
}

fn planes() -> Vec<f64> {
    (0..LAYERS).map(|layer| (f64::from(u32::try_from(layer).expect("small")) + 0.5) * H).collect()
}

fn slices(mesh: &plinth_mesh::Mesh) -> Vec<Vec<SliceSolid>> {
    slice_stack(mesh, &planes(), SliceStackLimits::production()).expect("the slab slices")
}

/// The edge tip (member 0, over the slab's first column, its disc hanging
/// off the face at x 0) and a control tip well inside the face (member 1).
/// Fat tips ([`TIP_RADIUS_MM`]), so the edge tip's disc is wide and its slide
/// moves its contact far enough to take its reach off cells it held (a 0.1 mm
/// tip slides 0.15 mm, which no cell on the 0.05 mm lattice notices).
fn field(grid: &Grid) -> Vec<SweepStrut> {
    vec![
        straight(grid, 0, ROW, 0, K_TOP, TIP_RADIUS_MM),
        straight(grid, 1, ROW, 40, K_TOP, TIP_RADIUS_MM),
    ]
}

const TIP_RADIUS_MM: f64 = 0.8;

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

/// The footprint rule over the field, with or without a tip-cover check.
fn run(cover: Option<TipCover<'_>>) -> (FootprintOutcome, Vec<SweepStrut>) {
    let grid = grid();
    let mesh = slab();
    let slices = slices(&mesh);
    let mut struts = field(&grid);
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

/// The slide the rule takes with no check: the field the scripted walks are
/// set against. Anti-vacuity for every test below.
fn unchecked() -> (FootprintOutcome, Vec<SweepStrut>) {
    let (outcome, struts) = run(None);
    assert_eq!(
        struts[0].seat_face.footprint,
        Footprint::MovedOntoFace,
        "the fixture slides its edge tip: {outcome:?}"
    );
    assert_eq!(struts[1].seat_face.footprint, Footprint::OnFace, "the control is on its face");
    (outcome, struts)
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

/// A slide that keeps its cover is left alone: the check runs once, finds
/// nothing, and the field is the field the rule leaves with no check.
#[test]
fn a_slide_that_keeps_its_cover_is_left_alone() {
    let (plain, plain_struts) = unchecked();
    let asked = RefCell::new(Vec::new());
    let mut walk = |_: &[SweepStrut], moved: &[MovedSeat]| {
        asked.borrow_mut().push(moved.to_vec());
        Vec::new()
    };
    let (outcome, struts) = run(Some(TipCover { reach_mm: 2.13, passes: 2, stranded: &mut walk }));
    assert_eq!(asked.borrow().len(), 1, "one walk over the slid field");
    assert_eq!(asked.borrow()[0].len(), 1, "the one slid tip was walked");
    assert_eq!(struts, plain_struts, "the field is exactly the unchecked one");
    let cover = &outcome.cover;
    assert!(cover.enabled && cover.checks == 1, "{cover:?}");
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
        "the reseated root {:?} reaches the target {target:?}",
        root(&struts[0])
    );
    assert_eq!(cover.reseated, vec![0], "{cover:?}");
    assert!(cover.undone.is_empty() && cover.left.is_empty(), "{cover:?}");
    assert_eq!(cover.found_cells, 1);
    assert_eq!(cover.slides_taken_back, 1);
    assert_eq!(cover.checks, 2, "one walk found it, one confirmed the repair");
    assert_eq!(struts[1], plain[1], "the control tip is untouched");
}

/// Repair (c): no slide anywhere reaches the cell, so after the passes the
/// slide is undone; the tip, whose disc hung off its face, keeps its placed
/// seat as a refused move (warned), never a refusal of the model.
#[test]
fn a_slide_no_reseat_can_save_is_undone() {
    let (_, plain) = unchecked();
    let target = [2.9, 0.1];
    let asked = RefCell::new(Vec::new());
    let mut walk = scripted(target, 0.05, &asked);
    let (outcome, struts) = run(Some(TipCover { reach_mm: 0.05, passes: 2, stranded: &mut walk }));
    let cover = &outcome.cover;
    assert!(
        matches!(struts[0].seat_face.footprint, Footprint::MoveRefused { .. }),
        "{:?} {cover:?}",
        struts[0].seat_face.footprint
    );
    assert!(struts[0].seat_face.shift_mm.iter().all(|v| v.abs() < 1e-12), "the placed seat");
    assert_eq!(cover.undone, vec![0], "{cover:?}");
    assert!(cover.left.is_empty(), "nothing is left uncovered: {cover:?}");
    assert!(outcome.moves_refused.iter().any(|(arm, _)| *arm == 0), "warned as a refused move");
    assert!(cover.checks <= 2 + 2, "bounded by the passes: {cover:?}");
    assert_eq!(struts[1], plain[1], "the control tip is untouched");
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
    for _ in 0..3 {
        assert_eq!(once(8), one);
    }
    assert_eq!(once(1), one);
}

/// THE REAL WALK. The edge tip roots the slab's first layer out to 2.13 mm
/// from its column; its inward slide moves that reach off the cells along the
/// slab's edge about 2.1 mm either side of it. With the check, the walk finds
/// them, the repair covers them, and a walk over the field as published
/// strands nothing. SABOTAGE: with the check off the published field still
/// strands them.
#[test]
fn the_real_walk_finds_a_stranding_slide_and_the_repair_covers_it() {
    let grid = grid();
    let mesh = slab();
    let slices = slices(&mesh);
    let planes = planes();
    let births = crate::births::Births::none();
    let context = ReplayContext {
        grid: &grid,
        slices: &slices,
        layer_cache: None,
        planes: &planes,
        mesh: &mesh,
        kappa: 2.13 / H,
        layer_height_mm: H,
        layers: LAYERS,
        straight_reach: true,
        creep_weight: false,
        creep_overhang_cut: true,
        overhang_centre_deg: 45.0,
        overhang_transition_deg: 25.0,
        max_restarts: 2,
        first_loss_ratio: 2.592 / 2.130,
        births: &births,
    };
    // What a published field leaves: every MovedOntoFace tip rooted where
    // its contact moved to, its balls derived by the lowering as production
    // derives them (the placed arm, and the arm with its seat slid).
    let plan = plinth_core::LayerPlan::new(0.0, vec![H; LAYERS]).expect("a uniform plan");
    let published = |struts: &[SweepStrut]| {
        let mut placed_struts = field(&grid);
        let placed = arms(&grid, &mut placed_struts);
        let moved: Vec<MovedSeat> = struts
            .iter()
            .enumerate()
            .filter(|(_, strut)| strut.seat_face.footprint == Footprint::MovedOntoFace)
            .map(|(index, strut)| {
                let shift = strut.seat_face.shift_mm;
                let slid = SweepArm {
                    seat_mm: geom_kernel::vec3::add(placed[index].seat_mm, shift),
                    seat_face_point_mm: placed[index]
                        .seat_face_point_mm
                        .map(|point| geom_kernel::vec3::add(point, shift)),
                    radius_profile_mm: Vec::new(),
                    ..placed[index].clone()
                };
                let discs = plinth_support::tip_contact_discs(&[placed[index].clone(), slid], &plan)
                    .expect("discs");
                let ball = |disc: Option<&plinth_support::TipContactDisc>| {
                    disc.map(|disc| super::Slid::of(disc).ungrown())
                };
                MovedSeat {
                    index,
                    xy_mm: root(strut),
                    contact_radius_mm: discs[1].as_ref().map_or(0.0, |disc| disc.radius_mm),
                    placed_ball: ball(discs[0].as_ref()),
                    ball: ball(discs[1].as_ref()),
                }
            })
            .collect();
        crate::demand_replay::stranded_by_moves(&context, struts, &moved)
    };
    let (_, plain) = unchecked();
    let sabotaged = published(&plain);
    assert!(
        !sabotaged.is_empty(),
        "SABOTAGE: with the check off the slide ({:?}) strands demand",
        plain[0].seat_face.shift_mm
    );
    let mut walk = |struts: &[SweepStrut], moved: &[MovedSeat]| {
        crate::demand_replay::stranded_by_moves(&context, struts, moved)
    };
    let (outcome, struts) = run(Some(TipCover { reach_mm: 2.13, passes: 2, stranded: &mut walk }));
    let cover = &outcome.cover;
    println!(
        "real walk: unchecked slide {:?} stranded {} cells; checked: {cover:?}, tip now {:?}",
        plain[0].seat_face.shift_mm,
        sabotaged.len(),
        struts[0].seat_face
    );
    assert!(cover.found_cells > 0, "the check found what the slide stranded: {cover:?}");
    assert_eq!(cover.found_slides, 1);
    assert!(cover.left.is_empty(), "{cover:?}");
    assert_eq!(cover.reseated.len() + cover.undone.len(), 1, "{cover:?}");
    assert!(published(&struts).is_empty(), "the published field strands nothing: {cover:?}");
}
