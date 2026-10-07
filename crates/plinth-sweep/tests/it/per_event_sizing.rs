//! **Each member is sized for its worst SINGLE peel event** (rule a′,
//! owner-approved 2026-09-29;
//! `plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/cylinder-load-sharing.md`).
//!
//! The max-field ledger took each column's maximum over every event FIRST and
//! summed the maxima where the routing brought them together, so an arm at a
//! lowest line was sized for peaks from different layers added as if they
//! peeled at once. These oracles hand-place the arms on sliced meshes and call
//! the down-sweep directly, so the up-sweep's placement cannot move the
//! measurement:
//!
//! * **the staircase**: N steps, each its own event, all funnelled to ONE arm.
//!   The arm must be sized for one event, and the max-field order (sabotage)
//!   must size it for about N of them;
//! * **the leaned arm**: one arm catches ONE event at two heights (directly
//!   under the upper part, and again under the block the rest came down
//!   through). Below the lower catch it must carry the SUM; the max-field order
//!   kept only the larger part.
//!
//! Both fixtures funnel every event into the one arm with nothing lost, so the
//! arm's design load must EQUAL the largest single event. The event areas are
//! measured here from the slice stack through the production rasteriser's
//! carrier mask, which is the definition of an event's area, not the routing
//! under test.

use plinth_mesh::{make_box, make_laid_tube, Mesh};
use plinth_support::perception::{slice_stack, SliceSolid, SliceStackLimits};
use plinth_sweep::down_sweep::{self, InsertionPolicy, SizingPolicy};
use plinth_sweep::grid::{plan_grid, Grid};
use plinth_sweep::raster::{rasterize_into, LayerMasks, RasterScratch};
use plinth_sweep::{
    DrainKeepOut, Footprint, Landing, SeatFace, StrutKind, SweepPolicy, SweepStrut,
};

use crate::harness;

const PITCH_MM: f64 = 0.05;
/// Every fixture is shifted a quarter cell off the grid lines, so no box edge
/// lies exactly on a cell boundary and the carrier mask is not decided by
/// floating-point rounding.
const OFFSET_MM: f64 = 0.0125;

fn height() -> f64 {
    harness::layer_height_mm()
}

/// One mesh from several closed boxes that touch but never overlap: `(size,
/// centre x, centre y, bottom z)`.
fn boxes(parts: &[([f64; 3], f64, f64, f64)]) -> Mesh {
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for &(size, x, y, z) in parts {
        let part = make_box(size, [x + OFFSET_MM, y + OFFSET_MM, z]).expect("a positive box");
        let offset = u32::try_from(verts.len()).expect("a small fixture");
        verts.extend_from_slice(part.verts());
        tris.extend(
            part.tris()
                .iter()
                .map(|t| [t[0] + offset, t[1] + offset, t[2] + offset]),
        );
    }
    Mesh::new(verts, tris).expect("disjoint closed boxes make a valid mesh")
}

struct Setup {
    grid: Grid,
    slices: Vec<Vec<SliceSolid>>,
    sizing: SizingPolicy,
}

/// Slice `mesh` on layers of the registered height from `z = 0` (the plate)
/// and lay a grid over `[-extent, extent]²`.
fn setup(mesh: &Mesh, top_z: f64, extent: f64, per_event: bool) -> Setup {
    let h = height();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small positive layer count"
    )]
    let layers = (top_z / h).ceil() as usize;
    #[expect(clippy::cast_precision_loss, reason = "a small layer index")]
    let planes: Vec<f64> = (0..layers).map(|layer| (layer as f64 + 0.5) * h).collect();
    let slices =
        slice_stack(mesh, &planes, SliceStackLimits::production()).expect("the fixture slices");
    let (grid, _) = plan_grid(
        -extent,
        -extent,
        extent,
        extent,
        layers,
        PITCH_MM,
        1.0,
        u64::MAX,
    );
    let policy = SweepPolicy::from_registry().expect("the registry");
    let sizing = SizingPolicy {
        x_allowed: policy.x_allowed,
        tip_radius_min_mm: policy.tip_radius_min_mm,
        tip_radius_max_mm: policy.tip_radius_max_mm,
        owner_growth_ceiling_mm: policy.owner_growth_ceiling_mm,
        alpha: plinth_sweep::peel::alpha(policy.peel_field_mode),
        lamina_reach_mm: policy.kappa * h,
        straight_reach: policy.straight_reach,
        tip_column_roots: true,
        per_event,
        handed_column_ledger: true,
    };
    Setup {
        grid,
        slices,
        sizing,
    }
}

fn cell_at(grid: &Grid, x: f64, y: f64) -> usize {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a coordinate inside the grid"
    )]
    let (col, row) = (
        grid.col_coord(x + OFFSET_MM).floor() as usize,
        grid.row_coord(y + OFFSET_MM).floor() as usize,
    );
    grid.index(row, col)
}

/// A plate-rooted arm seated at `k_top`, standing in `path[slot]` at layer
/// `slot` (straight when `path` is empty).
fn arm(cell: usize, k_top: usize, path: Vec<u32>) -> SweepStrut {
    SweepStrut {
        id: 0,
        cell,
        center_mm: [0.0, 0.0],
        kind: StrutKind::Arm,
        min_radius_mm: 0.0,
        thickened_mm: Vec::new(),
        seat_on_strut: None,
        k_top,
        k_bottom: 0,
        landing: Landing::Plate,
        interior: false,
        radius_mm: Vec::new(),
        carried_mm2: Vec::new(),
        covered_demand: Vec::new(),
        inserted_by_load: false,
        path,
        straight_fallback: None,
        fallback_path: Vec::new(),
        head: None,
        seat_face: SeatFace {
            point_mm: None,
            shift_mm: [0.0; 3],
            footprint: Footprint::NotGraded,
        },
        seat_normal: [0.0; 3],
    }
}

/// The largest single peel event: the most carrier cells any one layer has,
/// times the cell's reference peel.
fn largest_event_mm2(setup: &Setup) -> f64 {
    let mut scratch = RasterScratch::new(&setup.grid);
    let mut masks = LayerMasks::default();
    let mut most = 0_usize;
    for solids in &setup.slices {
        rasterize_into(&setup.grid, solids, &mut scratch, &mut masks);
        most = most.max(masks.carrier_cells());
    }
    #[expect(clippy::cast_precision_loss, reason = "a cell count")]
    let cells = most as f64;
    cells * setup.grid.cell_area_mm2() * setup.sizing.alpha
}

fn size_one_arm(setup: &Setup, strut: SweepStrut) -> (SweepStrut, down_sweep::DownSweepOutcome) {
    let mut struts = vec![strut];
    let outcome = down_sweep::run(
        &setup.grid,
        &setup.slices,
        &mut struts,
        setup.sizing,
        InsertionPolicy::disabled(),
        &DrainKeepOut::none(),
        &down_sweep::UnrootedColumns::none(),
    );
    (struts.remove(0), outcome)
}

/// Three posts on a strip that rests on one arm, each capped by its own step
/// one layer thick at its own height. The strip's cells all lie within one
/// lamina reach of the arm, and each cap within reach of its own post, so
/// every event ends in the arm; each cap's load comes down its own post and
/// meets the others only at the strip.
fn staircase() -> Mesh {
    let h = height();
    let strip = 6.0 * h;
    let post = 0.2;
    let cap = 1.0;
    let mut parts = vec![([2.8, 0.3, h], 0.0, 0.0, strip)];
    for (index, x) in [-1.2_f64, 0.0, 1.2].into_iter().enumerate() {
        #[expect(clippy::cast_precision_loss, reason = "a step index")]
        let rise = 4.0 * (index as f64 + 1.0) * h;
        parts.push(([post, post, rise - h], x, 0.0, strip + h));
        parts.push(([cap, cap, h], x, 0.0, strip + rise));
    }
    boxes(&parts)
}

#[test]
fn a_staircase_of_events_funnelled_to_one_arm_sizes_it_for_one_event() {
    let mesh = staircase();
    let top = 6.0 * height() + 12.0 * height() + height();
    let per_event = setup(&mesh, top + height(), 3.0, true);
    let max_field = setup(&mesh, top + height(), 3.0, false);
    let seat = cell_at(&per_event.grid, 0.0, 0.0);
    let (sized, outcome) = size_one_arm(&per_event, arm(seat, 6, Vec::new()));
    let (sabotaged, _) = size_one_arm(&max_field, arm(seat, 6, Vec::new()));
    let event = largest_event_mm2(&per_event);
    let root = sized.carried_mm2[0];
    let root_max_field = sabotaged.carried_mm2[0];
    println!(
        "staircase: largest single event {event:.6} mm2; arm root per event {root:.6} mm2, \
         max-field {root_max_field:.6} mm2; {} events, largest one-event plate reaction {:.6}, \
         uncarried {:.6}, per-event residual {:.3e}",
        outcome.per_event.events,
        outcome.per_event.max_event_plate_mm2,
        outcome.uncarried_load_mm2,
        outcome.per_event.conservation_residual,
    );
    // The premise: every event reached the arm, so its root carries each event
    // whole and the largest single event is the design load.
    assert!(outcome.uncarried_load_mm2 <= 0.0, "the fixture lost load");
    assert!(outcome.per_event.events > 3, "the per-event pass never ran");
    assert!(
        outcome.per_event.conservation_residual <= 1.0e-9,
        "per-event conservation residual {:.3e}",
        outcome.per_event.conservation_residual
    );
    assert!(
        (root - event).abs() <= 1.0e-9 * event,
        "the arm carries {root:.9} mm2 at its root; the largest single event is {event:.9} mm2"
    );
    // And the defect is real on this fixture: the old order sums the caps.
    assert!(
        root_max_field > 2.0 * event,
        "the max-field order gave {root_max_field:.6} mm2 against one event of {event:.6} mm2; \
         the fixture no longer shows events summed at the lowest line"
    );
}

/// A slab whose left part rests on one arm directly and whose right part
/// rests on a block, and the SAME arm leans under that block lower down. Each
/// slab layer's event enters the arm twice: at the slab, and at the block's
/// underside.
#[test]
fn a_leaned_arm_that_catches_one_event_at_two_heights_carries_the_sum() {
    let h = height();
    let block_bottom = 6.0 * h;
    let slab_bottom = block_bottom + 10.0 * h;
    let mesh = boxes(&[
        ([2.0, 0.6, 3.0 * h], 1.0, 0.0, slab_bottom),
        (
            [0.5, 0.6, slab_bottom - block_bottom],
            1.75,
            0.0,
            block_bottom,
        ),
    ]);
    let top = slab_bottom + 3.0 * h;
    let per_event = setup(&mesh, top + h, 3.0, true);
    let max_field = setup(&mesh, top + h, 3.0, false);
    let grid = &per_event.grid;
    let under_block = cell_at(grid, 1.8, 0.0);
    let under_slab = cell_at(grid, 1.0, 0.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "small positive layer indices"
    )]
    let (k_block, k_slab) = (
        (block_bottom / h).round() as usize,
        (slab_bottom / h).round() as usize,
    );
    let path: Vec<u32> = (0..k_slab)
        .map(|layer| {
            let cell = if layer < k_block {
                under_block
            } else {
                under_slab
            };
            u32::try_from(cell).expect("a small grid")
        })
        .collect();
    let (sized, outcome) = size_one_arm(&per_event, arm(under_slab, k_slab, path.clone()));
    let (sabotaged, _) = size_one_arm(&max_field, arm(under_slab, k_slab, path));
    let event = largest_event_mm2(&per_event);
    let root = sized.carried_mm2[0];
    let tip = sized.tip_load_mm2();
    let root_max_field = sabotaged.carried_mm2[0];
    println!(
        "leaned arm: largest single event {event:.6} mm2; per event tip {tip:.6} root \
         {root:.6} mm2; max-field root {root_max_field:.6} mm2; same-event catches {} (largest \
         gain {:.6} mm2); uncarried {:.6}",
        outcome.per_event.same_event_catches,
        outcome.per_event.same_event_catch_gain_max_mm2,
        outcome.uncarried_load_mm2,
    );
    assert!(outcome.uncarried_load_mm2 <= 0.0, "the fixture lost load");
    // The path fired: one event entered this arm at two layers.
    assert!(
        outcome.per_event.same_event_catches > 0,
        "no event entered the arm at two heights, so this test proves nothing"
    );
    assert!(tip < root, "the lower catch added nothing below the tip");
    assert!(
        (root - event).abs() <= 1.0e-9 * event,
        "below both catches the arm carries {root:.9} mm2; the whole event is {event:.9} mm2"
    );
    assert!(
        root_max_field < root - 1.0e-6,
        "the max-field order already carried the sum ({root_max_field:.6} mm2), so the fixture \
         no longer shows the defect"
    );
}

/// On a real fixture through the whole sweep: every event is conserved, the
/// pass fired, and no member is sized above what the max field would have
/// charged it except by a same-event sum the max field under-counted.
#[test]
fn every_peel_event_is_conserved_on_the_laid_tube() {
    let mesh = make_laid_tube().expect("the laid tube fixture");
    let policy = SweepPolicy::from_registry().expect("the registry");
    let plan = harness::sweep_with(&mesh, 0.0, &policy);
    let per_event = &plan.report.per_event;
    println!("laid tube per-event report: {per_event:?}");
    assert!(per_event.enabled, "per-event sizing is off in production");
    assert!(
        per_event.events > 0 && per_event.routed_entries > 0,
        "the pass never ran"
    );
    assert!(
        per_event.conservation_residual <= 1.0e-9,
        "per-event conservation residual {:.3e}",
        per_event.conservation_residual
    );
    // One event can never put more on a member than the whole event.
    assert!(
        per_event.worst_member_mm2 <= per_event.max_event_mm2 * (1.0 + 1.0e-9),
        "a member carries {:.4} mm2, more than the largest event of {:.4} mm2",
        per_event.worst_member_mm2,
        per_event.max_event_mm2
    );
}
