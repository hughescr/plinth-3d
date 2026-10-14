//! **A joining load is booked from the joint down** (v19 fix, lane L4,
//! 2026-10-08; heads are planned only where needed, Craig 2026-10-08).
//!
//! The join test hand-places two arms under a slab and calls the down-sweep
//! directly (the way `per_event_sizing` does), so the up-sweep's placement
//! cannot move the measurement:
//!
//! * the HOST is a headed arm (head over layers 45..=60, joint at layer 44);
//! * the LANDER is an arm that lands on the host 0.25 mm below its contact, in
//!   the head span (layer 55). The control lands it on the plate instead.
//!
//! A load that enters a headed arm between its contact `D` and its joint `K`
//! does not pass through the contact (ruling 2c). So the host's tip, and every
//! slot between the contact and the joint, must carry exactly what they carry
//! with the lander on the plate, and the sum must appear from the joint's slot
//! down. Before the lane the lander's load entered at layer 55 and every slot
//! under it, joint included, was sized for the sum (the v19 Cylinder's 0.965 mm
//! joint).
//!
//! SABOTAGE (run by hand): drop `SweepStrut::joint_hold_layer` from the
//! `defer_to_shaft` test in `down_sweep.rs` (or from `junction_layer` in
//! `per_event.rs`) and the head slots read the sum: both `assert_eq!`s on the
//! slots above the joint go red.

use plinth_mesh::{make_box, Mesh};
use plinth_support::perception::{slice_stack, SliceSolid, SliceStackLimits};
use plinth_sweep::down_sweep::{self, InsertionPolicy, SizingPolicy};
use plinth_sweep::grid::{plan_grid, Grid};
use plinth_sweep::heads::{Head, HeadLaw};
use plinth_sweep::{DrainKeepOut, Footprint, Landing, SeatFace, StrutKind, SweepPolicy, SweepStrut};

use crate::harness;

const PITCH_MM: f64 = 0.05;
const K_TOP: usize = 60;
const K_JOINT: usize = 44;
const LANDED_AT: usize = 55;
/// A quarter cell off the grid lines, so no box edge decides a raster cell.
const OFFSET_MM: f64 = 0.0125;

struct Setup {
    grid: Grid,
    slices: Vec<Vec<SliceSolid>>,
    sizing: SizingPolicy,
}

fn height() -> f64 {
    harness::layer_height_mm()
}

fn z_of(layer: usize) -> f64 {
    f64::from(u32::try_from(layer).expect("small")) * height()
}

fn setup() -> Setup {
    let h = height();
    // A slab 1.6 x 1.0 mm and three layers thick, its underside at layer K_TOP.
    let mesh: Mesh = make_box([1.6, 1.0, 3.0 * h], [OFFSET_MM, OFFSET_MM, z_of(K_TOP)])
        .expect("a positive box");
    let layers = K_TOP + 4;
    #[expect(clippy::cast_precision_loss, reason = "a small layer index")]
    let planes: Vec<f64> = (0..layers).map(|layer| (layer as f64 + 0.5) * h).collect();
    let slices =
        slice_stack(&mesh, &planes, SliceStackLimits::production()).expect("the fixture slices");
    let (grid, _) = plan_grid(-2.0, -2.0, 2.0, 2.0, layers, PITCH_MM, 1.0, u64::MAX);
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
        per_event: true,
        handed_column_ledger: true,
        layer_height_mm: h,
        plate_z_mm: 0.0,
    };
    Setup { grid, slices, sizing }
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

fn arm(id: u32, cell: usize, center: [f64; 2]) -> SweepStrut {
    SweepStrut {
        id,
        cell,
        center_mm: center,
        kind: StrutKind::Arm,
        min_radius_mm: 0.0,
        thickened_mm: Vec::new(),
        seat_on_strut: None,
        k_top: K_TOP,
        k_bottom: 0,
        landing: Landing::Plate,
        interior: false,
        radius_mm: Vec::new(),
        carried_mm2: Vec::new(),
        covered_demand: Vec::new(), held_birth_cells: Vec::new(), twin_host: false,
        inserted_by_load: false,
        path: Vec::new(),
        straight_fallback: None,
        fallback_path: Vec::new(),
        head: None,
        seat_face: SeatFace {
            point_mm: None,
            shift_mm: [0.0; 3],
            footprint: Footprint::NotGraded,
            contact_ball_radius_mm: None,
            foot: None,
        },
        seat_normal: [0.0; 3],
    }
}

/// The two arms: a headed host under the slab's left half and a lander under
/// its right half, landed on the plate (`landed_at = None`) or on the host at
/// `landed_at`.
fn field(setup: &Setup, landed_at: Option<usize>) -> Vec<SweepStrut> {
    let grid = &setup.grid;
    let (host_x, lander_x) = (-0.4, 0.4);
    let host_cell = cell_at(grid, host_x, 0.0);
    let cell_u32 = |cell: usize| u32::try_from(cell).expect("a small grid");
    let mut host = arm(0, host_cell, [host_x, 0.0]);
    host.head = Some(Head {
        contact_mm: [host_x, 0.0, z_of(K_TOP)],
        unit: [0.0, 0.0, -1.0],
        joint_mm: [host_x, 0.0, z_of(K_JOINT)],
        k_joint: K_JOINT,
        joint_cell: cell_u32(host_cell),
        joint_radius_mm: 0.2,
        planned_tip_radius_mm: 0.1,
        gap_mm: 0.3,
        cos_lean_max: 0.5373,
        cells: vec![cell_u32(host_cell); K_TOP - K_JOINT],
        ball_allowance_mm: 0.0,
        link_mm: 0.0,
        drawn_straight: false,
        law_planned: HeadLaw::envelope(0.5373),
        law_drawn: None,
        ledger_radius_mm: Vec::new(),
        certified: None,
        old_law: None,
    });
    host.set_post(Landing::Plate, 0, vec![cell_u32(host_cell); K_JOINT + 1]);
    assert!(host.head_intact(), "the fixture's head must be intact");
    let mut lander = arm(1, cell_at(grid, lander_x, 0.0), [lander_x, 0.0]);
    if let Some(layer) = landed_at {
        lander.landing = Landing::Strut { id: 0, layer };
        lander.k_bottom = layer;
    }
    vec![host, lander]
}

fn size(setup: &Setup, landed_at: Option<usize>) -> (Vec<SweepStrut>, down_sweep::DownSweepOutcome) {
    let mut struts = field(setup, landed_at);
    let outcome = down_sweep::run(
        &setup.grid,
        &setup.slices,
        &mut struts,
        setup.sizing,
        InsertionPolicy::disabled(),
        &DrainKeepOut::none(),
        &down_sweep::UnrootedColumns::none(),
    );
    (struts, outcome)
}

#[test]
fn a_load_joining_a_head_span_is_booked_from_the_joint_down() {
    let setup = setup();
    let (control, _) = size(&setup, None);
    let (joined, outcome) = size(&setup, Some(LANDED_AT));
    let (host_control, host_joined) = (&control[0], &joined[0]);
    let handed = joined[1].carried_mm2[0];
    println!(
        "host tip {:.6} / {:.6} mm (control / joined), joint slot carried {:.6} / {:.6} mm2, \
         the lander hands {handed:.6} mm2 at layer {LANDED_AT}; held to the joint: {}",
        host_joined.tip_radius_mm(),
        host_control.tip_radius_mm(),
        host_joined.carried_mm2[K_JOINT],
        host_control.carried_mm2[K_JOINT],
        outcome.head_span_merges_held_to_joint,
    );
    assert!(handed > 0.0, "anti-vacuity: the lander carries a load to hand over");
    assert!(
        outcome.head_span_merges_held_to_joint >= 1,
        "the hold fired (path hit): {}",
        outcome.head_span_merges_held_to_joint
    );
    // The contact and every slot between it and the joint carry the host's own
    // load only: the area-law tip is the control's, bit for bit.
    assert_eq!(
        host_joined.tip_radius_mm().to_bits(),
        host_control.tip_radius_mm().to_bits(),
        "the arm's tip radius equals its own area-law tip"
    );
    assert_eq!(
        host_joined.carried_mm2[K_JOINT + 1..],
        host_control.carried_mm2[K_JOINT + 1..],
        "no joining load is booked into the head slots"
    );
    // From the joint's slot down the shaft carries the sum.
    for slot in 0..=K_JOINT {
        let sum = host_control.carried_mm2[slot] + handed;
        assert!(
            (host_joined.carried_mm2[slot] - sum).abs() <= 1.0e-9 * sum.max(1.0),
            "slot {slot}: {} against the control {} plus {handed}",
            host_joined.carried_mm2[slot],
            host_control.carried_mm2[slot]
        );
    }
}
