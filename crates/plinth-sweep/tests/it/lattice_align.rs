//! **Seat placement: the reach is a straight length, and each island is
//! covered by the fewest seats that cover it** (lane lattice-align,
//! 2026-09-30).
//!
//! Two engineering defects measured in
//! `plinth-evidence/.../field-2026-09-28/oversupport-attribution.md` (items 3
//! and 7), neither a physics change: `κ` as measured stays, and so does the
//! one-lamina reach `κ·h = 2.130 mm`.
//!
//! 1. **The metric.** The reach potential travelled on the 8-neighbour chamfer
//!    (steps `p`, `p√2`), which prices a straight run at 22.5° off an axis
//!    8.24 % long. A root therefore reached only `κ·h / 1.0824 = 1.968 mm` in
//!    its worst direction, and the lattice was pitched at that. The coupons
//!    measured `κ·h` as a straight span. The tests below grade the reach ball
//!    of one root against the true disc, in every direction, through both the
//!    layer potential and the per-seat relaxation; and they grade the other
//!    side of the same rule: the straight line is taken only where it lies in
//!    the cover, so a root still does not reach across a slot.
//! 2. **The seats.** The hex lattice was laid on the grid's own origin and knew
//!    nothing of an island's boundary, so a ring, a strip or a small island
//!    got lattice points wherever the grid fell and one greedy arm per gap
//!    between them: 2.8-3.5x the hex covering lower bound on the rings and
//!    strips, and 85 fill arms on Morel's 44 small islands. RULE: each island
//!    is covered completely by the fewest seats among coverings built to it
//!    and measured on it. The tests below seat one flat layer of four shapes
//!    and grade the seat count against the shape's own covering count, and
//!    grade that EVERY cell of every shape is rooted afterwards.
//!
//! The reference counts are geometry of the shape, written out per test; none
//! is read from production.
//!
//! # Sabotage (measured in the lane report, `lanes/lattice-align.md`)
//!
//! | change | what reddens |
//! |---|---|
//! | the straight sightline is never taken (`any_angle_relax`'s limit at −∞; `SweepPolicy::without_straight_reach`) | the two reach-disc tests |
//! | the sightline is taken without walking it (`line_of_sight` returns `true`) | `a_root_does_not_reach_across_a_slot` |
//! | the island trials are skipped (`GreedyScratch::island_seating = false`) | the ring, strip and small-island seat counts |

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test fixtures: a non-negative in-grid mm coordinate floored to a cell index, and \
          seat counts (<1e4) compared as f64"
)]

use crate::harness::{lamina_reach_mm, layer_height_mm};
use plinth_sweep::drain::DrainKeepOut;
use plinth_sweep::grid::{plan_grid, Grid};
use plinth_sweep::lean::LeanLaw;
use plinth_sweep::reach::{recompute_potential, relax_from_root, ReachState, SurfaceCreep};
use plinth_sweep::up_sweep::{place_layer, ColumnHistory, GreedyScratch};

const PITCH_MM: f64 = 0.05;

fn kappa() -> f64 {
    lamina_reach_mm() / layer_height_mm()
}

fn square_grid(side_mm: f64) -> Grid {
    plan_grid(0.0, 0.0, side_mm, side_mm, 1, PITCH_MM, 0.0, u64::MAX).0
}

fn cell_at(grid: &Grid, x: f64, y: f64) -> usize {
    grid.index(grid.row_coord(y).floor() as usize, grid.col_coord(x).floor() as usize)
}

fn centre(grid: &Grid, cell: usize) -> (f64, f64) {
    (
        grid.center_x_mm(grid.col_of(cell)),
        grid.center_y_mm(grid.row_of(cell)),
    )
}

/// How far the rooted set runs from `root` along a ray, in mm, stopping at
/// the first unrooted cell.
fn rooted_run_mm(grid: &Grid, state: &ReachState, root: usize, degrees: f64) -> f64 {
    let (x0, y0) = centre(grid, root);
    let (sin, cos) = degrees.to_radians().sin_cos();
    let step = PITCH_MM / 8.0;
    let mut last = 0.0;
    let mut t = 0.0;
    loop {
        t += step;
        let cell = cell_at(grid, x0 + t * cos, y0 + t * sin);
        if !state.is_rooted(cell) {
            return last;
        }
        let (x, y) = centre(grid, cell);
        last = ((x - x0).powi(2) + (y - y0).powi(2)).sqrt();
    }
}

/// The two checks every reach ball below must pass: in every direction the
/// rooted run is at least the registered reach less one raster pitch, and no
/// cell farther than the reach plus half a cell diagonal is rooted.
fn grade_reach_ball(grid: &Grid, state: &ReachState, root: usize, path: &str) {
    let reach = lamina_reach_mm();
    let mut shortest = (f64::INFINITY, 0.0);
    let mut degrees = 0.0;
    while degrees <= 90.0 {
        let run = rooted_run_mm(grid, state, root, degrees);
        if run < shortest.0 {
            shortest = (run, degrees);
        }
        degrees += 2.5;
    }
    println!(
        "{path}: shortest rooted run {:.4} mm at {:.1} deg against the reach {reach:.4} mm",
        shortest.0, shortest.1
    );
    assert!(
        shortest.0 >= reach - PITCH_MM,
        "{path}: the rooted run is {:.4} mm at {:.1} deg, short of the measured reach {reach:.4} \
         mm by more than one pitch; the metric is not measuring a straight length",
        shortest.0,
        shortest.1
    );
    let (x0, y0) = centre(grid, root);
    let beyond = (0..grid.cells())
        .filter(|cell| state.is_rooted(*cell))
        .map(|cell| {
            let (x, y) = centre(grid, cell);
            ((x - x0).powi(2) + (y - y0).powi(2)).sqrt()
        })
        .fold(0.0_f64, f64::max);
    assert!(
        beyond <= reach + PITCH_MM * core::f64::consts::FRAC_1_SQRT_2,
        "{path}: a cell {beyond:.4} mm from the root is rooted, beyond the reach {reach:.4} mm"
    );
}

#[test]
fn a_root_reaches_the_measured_reach_in_every_direction_through_the_layer_potential() {
    let grid = square_grid(12.0);
    let cover = vec![true; grid.cells()];
    let root = cell_at(&grid, 6.0, 6.0);
    let mut state = ReachState::new(&grid);
    recompute_potential(
        &grid,
        &cover,
        kappa(),
        layer_height_mm(),
        &[root],
        &SurfaceCreep::disabled(&grid),
        &mut state,
    );
    grade_reach_ball(&grid, &state, root, "layer potential");
}

#[test]
fn a_root_reaches_the_measured_reach_in_every_direction_through_one_insertion() {
    let grid = square_grid(12.0);
    let cover = vec![true; grid.cells()];
    let root = cell_at(&grid, 6.0, 6.0);
    let mut state = ReachState::new(&grid);
    // One potential pass with no root sets the lamina the step is priced in.
    recompute_potential(
        &grid,
        &cover,
        kappa(),
        layer_height_mm(),
        &[],
        &SurfaceCreep::disabled(&grid),
        &mut state,
    );
    let (mut queue, mut touched) = (Vec::new(), Vec::new());
    relax_from_root(&grid, &cover, kappa(), root, &mut state, &mut queue, &mut touched);
    grade_reach_ball(&grid, &state, root, "one insertion");
}

/// THE SABOTAGE, live: with the sightline never taken (the chamfer form,
/// `SweepPolicy::without_straight_reach`) the same root must fall short of the
/// reach in its worst direction by the chamfer's own 8.24 %, so the two
/// tests above can only be green because the straight line is measured.
#[test]
fn without_the_sightline_the_reach_falls_back_to_the_chamfer_octagon() {
    let grid = square_grid(12.0);
    let cover = vec![true; grid.cells()];
    let root = cell_at(&grid, 6.0, 6.0);
    let mut state = ReachState::new(&grid);
    state.straight_limit = f32::NEG_INFINITY;
    recompute_potential(
        &grid,
        &cover,
        kappa(),
        layer_height_mm(),
        &[root],
        &SurfaceCreep::disabled(&grid),
        &mut state,
    );
    let shortest = (0..=36)
        .map(|step| rooted_run_mm(&grid, &state, root, 2.5 * f64::from(step)))
        .fold(f64::INFINITY, f64::min);
    let octagon = lamina_reach_mm() / 1.0824;
    println!("chamfer form: shortest rooted run {shortest:.4} mm, octagon inradius {octagon:.4} mm");
    assert!(
        shortest < lamina_reach_mm() - PITCH_MM && shortest <= octagon + PITCH_MM,
        "the chamfer form reached {shortest:.4} mm in its worst direction; the sabotage does not \
         bite"
    );
    assert_eq!(state.sightlines_clear, 0, "the sabotage form took a sightline");
}

/// The straight line is taken only where it lies in the cover. A root beside
/// a slot 0.1 mm wide reaches the cell straight across it (1.0 mm away) only
/// by going round the slot's end, 1.2 mm up and 1.2 mm back: `2·√(0.5² +
/// 1.2²) = 2.6 mm`, beyond the 2.13 mm reach, so that cell must stay unrooted
/// on both paths, while the root's own side is reached in full. The slot's end
/// lies within reach, so sightlines round it must actually be tried and
/// refused.
#[test]
fn a_root_does_not_reach_across_a_slot() {
    let grid = square_grid(12.0);
    let mut cover = vec![true; grid.cells()];
    for (cell, covered) in cover.iter_mut().enumerate() {
        let (x, y) = centre(&grid, cell);
        if (x - 6.0).abs() < 0.05 && y < 3.2 {
            *covered = false;
        }
    }
    let root = cell_at(&grid, 5.5, 2.0);
    let across = cell_at(&grid, 6.5, 2.0);
    let beside = cell_at(&grid, 5.5, 2.0 + lamina_reach_mm() - PITCH_MM);
    for path in ["layer potential", "one insertion"] {
        let mut state = ReachState::new(&grid);
        let roots: &[usize] = if path == "layer potential" { &[root] } else { &[] };
        recompute_potential(
            &grid,
            &cover,
            kappa(),
            layer_height_mm(),
            roots,
            &SurfaceCreep::disabled(&grid),
            &mut state,
        );
        if path == "one insertion" {
            let (mut queue, mut touched) = (Vec::new(), Vec::new());
            relax_from_root(&grid, &cover, kappa(), root, &mut state, &mut queue, &mut touched);
        }
        assert!(
            !state.is_rooted(across),
            "{path}: the cell across the slot is rooted (U = {}); the reach went through empty \
             space",
            state.u[across]
        );
        assert!(
            state.is_rooted(beside),
            "{path}: the root's own side is not reached at reach less one pitch"
        );
        // PATH HIT: the slot must actually have blocked sightlines, or this
        // test would be green on a field that never tried to cross it.
        assert!(
            state.sightlines_blocked > 0 && state.sightlines_clear > 0,
            "{path}: sightlines {} clear / {} blocked; the slot was never tested",
            state.sightlines_clear,
            state.sightlines_blocked
        );
    }
}

/// Seat one fresh flat layer of `cover` the way the sweep does, and return
/// the seat count after checking that nothing is left unrooted.
fn seat_one_layer(grid: &Grid, cover: &[bool], what: &str) -> usize {
    seat_one_layer_with(grid, cover, what, false)
}

/// The same, with `legacy` selecting the pre-2026-09-30 placement: the
/// chamfer reach and the global lattice plus one greedy pass
/// (`GreedyScratch::legacy`, `ReachState::straight_limit = −∞`), the pair of
/// knobs `harness::at_path_fixture_seating` sets on a whole sweep.
fn seat_one_layer_with(grid: &Grid, cover: &[bool], what: &str, legacy: bool) -> usize {
    let height = layer_height_mm();
    let mut reach = ReachState::new(grid);
    if legacy {
        reach.straight_limit = f32::NEG_INFINITY;
    }
    recompute_potential(
        grid,
        cover,
        kappa(),
        height,
        &[],
        &SurfaceCreep::disabled(grid),
        &mut reach,
    );
    let mut history = ColumnHistory::new(grid);
    let mut scratch = if legacy {
        GreedyScratch::legacy(grid, lamina_reach_mm())
    } else {
        GreedyScratch::new(grid, lamina_reach_mm())
    };
    let mut struts = Vec::new();
    let outcome = place_layer(
        grid,
        0,
        cover,
        &DrainKeepOut::none(),
        &LeanLaw::disabled(height),
        true,
        kappa(),
        &mut reach,
        &mut history,
        &mut scratch,
        &mut struts,
        1_000_000,
    );
    let unrooted = (0..grid.cells())
        .filter(|cell| cover[*cell] && !reach.is_rooted(*cell))
        .count();
    assert_eq!(unrooted, 0, "{what}: {unrooted} cells left unrooted");
    assert_eq!(outcome.residual_demand, 0, "{what}: residual demand reported");
    struts.len()
}

/// How many seats a single row needs along a strip `width` wide and `length`
/// long: each disc of radius `R` centred on the strip's midline covers a
/// full-width run of `2·√(R² − (w/2)²)`.
fn single_row_count(length_mm: f64, width_mm: f64) -> f64 {
    let reach = lamina_reach_mm();
    let run = 2.0 * (reach * reach - 0.25 * width_mm * width_mm).sqrt();
    (length_mm / run).ceil()
}

fn grade_count(seats: usize, reference: f64, factor: f64, what: &str) {
    println!(
        "{what}: {seats} seats against a covering count of {reference:.0} (ratio {:.3}, allowed \
         {factor})",
        seats as f64 / reference
    );
    assert!(
        seats as f64 <= factor * reference,
        "{what}: {seats} seats, more than {factor} x the shape's own covering count {reference:.0}"
    );
}

/// The rim annulus of `Round_50` as it meets the supports glue face up: 2 mm
/// wide, 23 to 25 mm radius. A ring narrower than two reaches is covered by
/// ONE row of seats on its mid-circle.
fn shape(side_mm: f64, inside: impl Fn(f64, f64) -> bool) -> (Grid, Vec<bool>) {
    let grid = square_grid(side_mm);
    let cover = (0..grid.cells())
        .map(|cell| {
            let (x, y) = centre(&grid, cell);
            inside(x, y)
        })
        .collect();
    (grid, cover)
}

fn ring() -> (Grid, Vec<bool>) {
    shape(52.0, |x, y| {
        (23.0..25.0).contains(&((x - 26.0).powi(2) + (y - 26.0).powi(2)).sqrt())
    })
}

fn strip() -> (Grid, Vec<bool>) {
    shape(44.0, |x, y| (2.0..42.0).contains(&x) && (21.25..22.75).contains(&y))
}

fn small_islands() -> (Grid, Vec<bool>) {
    let centres: Vec<(f64, f64)> = (0..16)
        .map(|index| {
            let (i, j) = (f64::from(index % 4), f64::from(index / 4));
            // Offsets that are not a multiple of any lattice pitch.
            (3.3 + 6.1 * i + 0.37 * j, 3.1 + 6.2 * j + 0.29 * i)
        })
        .collect();
    shape(26.0, move |x, y| {
        centres
            .iter()
            .any(|(cx, cy)| (x - cx).powi(2) + (y - cy).powi(2) < 1.8 * 1.8)
    })
}

fn square() -> (Grid, Vec<bool>) {
    shape(34.0, |x, y| (2.0..32.0).contains(&x) && (2.0..32.0).contains(&y))
}

#[test]
fn a_ring_is_seated_as_one_row() {
    let (grid, cover) = ring();
    let seats = seat_one_layer(&grid, &cover, "ring");
    let reference = single_row_count(core::f64::consts::TAU * 24.0, 2.0);
    grade_count(seats, reference, 1.15, "ring 23-25 mm");
}

/// A strip 1.5 mm wide and 40 mm long, the shape of a laid tube's first
/// contact line.
#[test]
fn a_strip_is_seated_as_one_row() {
    let (grid, cover) = strip();
    let seats = seat_one_layer(&grid, &cover, "strip");
    let reference = single_row_count(40.0, 1.5);
    grade_count(seats, reference, 1.15, "strip 1.5 x 40 mm");
}

/// Sixteen small islands, each a disc 1.8 mm in radius, inside one reach of
/// its own centre: each is covered by ONE seat, whatever the global lattice's
/// phase is against it.
#[test]
fn a_small_island_takes_one_seat() {
    let (grid, cover) = small_islands();
    let seats = seat_one_layer(&grid, &cover, "small islands");
    grade_count(seats, 16.0, 1.0, "16 islands of radius 1.8 mm");
}

/// A 30 mm square slab: the hex covering lower bound at the measured reach is
/// `area / ((3√3/2)·R²)`. A boundary costs something over that bound; 1.35x
/// is the allowance.
#[test]
fn a_square_slab_is_seated_near_the_hex_bound() {
    let (grid, cover) = square();
    let seats = seat_one_layer(&grid, &cover, "square");
    let reach = lamina_reach_mm();
    let bound = (900.0 / (1.5 * 3.0_f64.sqrt() * reach * reach)).ceil();
    grade_count(seats, bound, 1.35, "square 30 x 30 mm");
}

/// THE SABOTAGE FORM IS THE PRE-LANE PLACEMENT, exactly. These four counts
/// are what develop `8007750`'s own code placed on the same four shapes (the
/// lane's red run, `lanes/lattice-align/red-run.log`: strip 21, islands 49,
/// ring 89, square 115). The legacy scratch and the chamfer reach must give
/// them to the seat, so `harness::at_path_fixture_seating` really is the old
/// field and not an approximation of it. (Measured separately on a whole
/// sweep: it gives the round-base ratchet's old 88 / 304 / 1067 exactly.)
#[test]
fn the_sabotage_form_is_the_pre_lane_placement() {
    for (name, (grid, cover), develop) in [
        ("strip", strip(), 21),
        ("small islands", small_islands(), 49),
        ("ring", ring(), 89),
        ("square", square(), 115),
    ] {
        let seats = seat_one_layer_with(&grid, &cover, name, true);
        assert_eq!(
            seats, develop,
            "{name}: the legacy form placed {seats} seats where develop placed {develop}"
        );
    }
}

/// PATH HITS AND THE SABOTAGE, in one place. On the ring the island trials
/// must have run and a covering other than the global lattice must have won
/// (the counters say so); and with the trials switched off the same ring must
/// take visibly more seats, still with every cell rooted. A green ring count
/// can therefore not mean "the trials never ran".
#[test]
fn the_ring_is_seated_by_the_trials_and_not_without_them() {
    let (grid, cover) = ring();
    let height = layer_height_mm();
    let run = |island_seating: bool| {
        let mut reach = ReachState::new(&grid);
        recompute_potential(
            &grid,
            &cover,
            kappa(),
            height,
            &[],
            &SurfaceCreep::disabled(&grid),
            &mut reach,
        );
        let mut history = ColumnHistory::new(&grid);
        let mut scratch = GreedyScratch::new(&grid, lamina_reach_mm());
        scratch.island_seating = island_seating;
        let mut struts = Vec::new();
        place_layer(
            &grid,
            0,
            &cover,
            &DrainKeepOut::none(),
            &LeanLaw::disabled(height),
            true,
            kappa(),
            &mut reach,
            &mut history,
            &mut scratch,
            &mut struts,
            1_000_000,
        );
        let unrooted = (0..grid.cells())
            .filter(|cell| cover[*cell] && !reach.is_rooted(*cell))
            .count();
        (
            struts.len(),
            unrooted,
            scratch.islands_compared,
            scratch.islands_realigned,
            scratch.seats_saved,
        )
    };
    let (seats, unrooted, compared, realigned, saved) = run(true);
    let (blind, blind_unrooted, blind_compared, _, _) = run(false);
    println!(
        "ring: {seats} seats with the trials (islands compared {compared}, realigned \
         {realigned}, seats saved {saved}); {blind} seats without them"
    );
    assert_eq!((unrooted, blind_unrooted), (0, 0), "both coverings must root the whole ring");
    assert!(compared >= 1 && realigned >= 1 && saved >= 1, "the trials never ran or never won");
    assert_eq!(blind_compared, 0, "the sabotage form must not run the trials");
    assert!(
        blind as f64 >= 1.3 * seats as f64,
        "without the trials the ring took {blind} seats against {seats}: the sabotage does not \
         bite, so the ring count does not grade the rule"
    );
}

/// END TO END: one whole sweep of a flat 20 mm slab on the production policy.
/// The up-sweep seats on the straight-line reach and the down-sweep routes on
/// the same metric, so every cell the up-sweep called rooted is routable:
/// residual demand 0 AND uncarried load 0, and the run report does not flag
/// the two sweeps as disagreeing. Both sightline counters must be non-zero, so
/// the metric was exercised in both sweeps, not merely compiled.
#[test]
fn a_flat_slab_is_rooted_and_carried_on_one_metric() {
    let mesh = plinth_mesh::make_box([20.0, 20.0, 2.0], [0.0, 0.0, 0.0]).expect("the slab");
    let policy = plinth_sweep::SweepPolicy::from_registry().expect("the registry");
    let plan = crate::harness::sweep_with(&mesh, 0.0, &policy);
    let report = &plan.report;
    let hits = report.counters;
    println!(
        "slab 20 x 20 mm: {} members; residual {} cells, uncarried {:.4} mm2, disagree {}; \
         sightlines {} clear / {} bent (reach), {} / {} (routing); up-sweep seats {} against a \
         hex bound {}",
        report.struts_total,
        report.residual_demand_cells,
        report.uncarried_load_mm2,
        report.sweeps_disagree,
        hits.sightlines_clear,
        hits.sightlines_blocked,
        hits.route_sightlines_clear,
        hits.route_sightlines_blocked,
        hits.island_seats,
        hits.island_hex_bound,
    );
    assert_eq!(report.residual_demand_cells, 0, "demand left unrooted");
    assert!(
        report.uncarried_load_mm2 <= 1.0e-9,
        "{:.4} mm2 of load the up-sweep called rooted could not be routed: the two sweeps are \
         not measuring on one metric",
        report.uncarried_load_mm2
    );
    assert!(!report.sweeps_disagree);
    assert!(
        hits.sightlines_clear > 0 && hits.route_sightlines_clear > 0,
        "a sweep never priced a straight sightline"
    );
    assert!(hits.island_seats > 0 && hits.island_hex_bound > 0);
}
