//! **A brace ties a member on its FREE SHAFT, never inside its root flare**
//! (fix/brace-not-in-flare, 2026-09-30).
//!
//! The finding (`plinth-evidence/.../field-2026-09-28/oversupport-attribution.md`,
//! "The make_laid_tube low records"): the laid tube's `|X|` pair 177/178 ran
//! from layer 43 down to layer 5, so each diagonal's lower end sat on its
//! partner's axis 0.25 mm above the plate, inside that partner's root flare
//! (the boss top is 1.7 mm up) and over its raft pad. The flare is already a
//! rigid base: a tie inside it adds resin and a record below the raft top, and
//! no stiffness.
//!
//! The rule: a brace tie point lies above the top of the root flare of every
//! member it ties, and above the raft top; a band that cannot meet this moves
//! up or is dropped.
//!
//! **What is graded is the FILE.** The flares are read from the `RootFlare`
//! records the lowering emits (their boss top, foot and radii), never from the
//! crate's own idea of where a flare ends, so a wrong flare height in the rule
//! cannot agree with itself. The brace ends are the axis points the adapter
//! draws each diagonal between: its seat at `k_top` and its landing at
//! `k_bottom`, over the cells it joins.

use crate::harness;

use harness::sweep_with;
use plinth_core::{m0_constants, Finite, LayerPlan, PositiveFinite};
use plinth_mesh::make_laid_tube;
use plinth_support::{BuildFrame, SupportSegmentKind};
use plinth_sweep::{to_primitives, StrutKind, SweepPlan, SweepPolicy};

fn policy() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every number the sweep reads")
}

/// One emitted root flare: its axis point at the boss top and at the foot, and
/// its radius at each.
struct Flare {
    top: [f64; 3],
    foot: [f64; 3],
    top_radius_mm: f64,
    foot_radius_mm: f64,
}

impl Flare {
    /// Whether `point` lies inside this flare's solid: between its foot and
    /// its boss top in height, and inside its radius at that height (the
    /// frustum's radius runs linearly from the foot to the top).
    fn contains(&self, point: [f64; 3]) -> bool {
        let (low, high) = (self.foot[2].min(self.top[2]), self.foot[2].max(self.top[2]));
        if point[2] < low || point[2] > high {
            return false;
        }
        let span = self.top[2] - self.foot[2];
        let t = if span.abs() > 0.0 {
            (point[2] - self.foot[2]) / span
        } else {
            0.0
        };
        let axis = [
            self.foot[0] + t * (self.top[0] - self.foot[0]),
            self.foot[1] + t * (self.top[1] - self.foot[1]),
        ];
        let radius = self.foot_radius_mm + t * (self.top_radius_mm - self.foot_radius_mm);
        (point[0] - axis[0]).hypot(point[1] - axis[1]) < radius
    }
}

fn emitted_flares(plan: &SweepPlan) -> Vec<Flare> {
    let layers = LayerPlan::new(plan.plate_z_mm, vec![plan.layer_height_mm; plan.layers])
        .expect("the plan's own uniform layer plan");
    let frame = BuildFrame::new(
        Finite::new(plan.plate_z_mm).expect("a finite plate"),
        PositiveFinite::new(harness::LIFT_MM).expect("a positive lift"),
    );
    let emission = to_primitives(plan, frame, &layers, 4_000_000, 1 << 30)
        .expect("the lowering refuses only about the request");
    emission
        .primitives
        .iter()
        .filter(|primitive| primitive.segment_kind == SupportSegmentKind::RootFlare)
        .map(|flare| Flare {
            top: flare.start_mm.map(Finite::get),
            foot: flare.end_mm.map(Finite::get),
            top_radius_mm: flare.start_radius_mm.get(),
            foot_radius_mm: flare.end_radius_mm.get(),
        })
        .collect()
}

/// Both ends of every live brace, as the adapter draws them: `(brace id,
/// "seat" | "landing", point)`.
#[expect(
    clippy::cast_precision_loss,
    reason = "a layer index bounded by the plan's own layer count is exact in f64"
)]
fn brace_ends(plan: &SweepPlan) -> Vec<(u32, &'static str, [f64; 3])> {
    let grid = &plan.grid;
    let at = |cell: usize, layer: usize| {
        [
            grid.center_x_mm(grid.col_of(cell)),
            grid.center_y_mm(grid.row_of(cell)),
            plan.plate_z_mm + layer as f64 * plan.layer_height_mm,
        ]
    };
    plan.struts
        .iter()
        .filter(|strut| {
            strut.kind == StrutKind::Brace
                && strut.k_bottom != usize::MAX
                && strut.k_top > strut.k_bottom
        })
        .flat_map(|brace| {
            [
                (brace.id, "seat", at(brace.cell, brace.k_top)),
                (brace.id, "landing", at(brace.base_cell(), brace.k_bottom)),
            ]
        })
        .collect()
}

fn raft_height_mm() -> f64 {
    // Moved from the placeholders to the evidence on 2026-09-30 (the owner's
    // ChituBox profile export, `raft_support.raftthickness`), same value.
    *m0_constants()
        .expect("registry")
        .evidence()
        .chitubox_raft_height_mm()
        .value()
}

/// Every brace end inside a flare the file draws, or at or below the raft top,
/// described. Empty when the rule holds.
fn brace_ends_in_a_flare(plan: &SweepPlan, flares: &[Flare]) -> Vec<String> {
    let raft_top = plan.plate_z_mm + raft_height_mm();
    let mut inside: Vec<String> = Vec::new();
    for (id, end, point) in brace_ends(plan) {
        if let Some(flare) = flares.iter().find(|flare| flare.contains(point)) {
            inside.push(format!(
                "brace {id} {end} at {:.3} mm over the plate, inside the flare at ({:.3}, {:.3}) \
                 whose boss top is {:.3} mm up",
                point[2] - plan.plate_z_mm,
                flare.foot[0],
                flare.foot[1],
                flare.top[2] - plan.plate_z_mm,
            ));
        }
        if point[2] <= raft_top {
            inside.push(format!(
                "brace {id} {end} at {:.3} mm over the plate, at or below the raft top",
                point[2] - plan.plate_z_mm
            ));
        }
    }
    inside
}

/// **THE MAST PATH, on the laid tube with the thicken rung switched off** (the
/// configuration `mast_root_clearance.rs` grades masts in, because production
/// thickens instead of dropping masts on this fixture). A new mast is only as
/// tall as its tie, so a mast band that dips into a flare cannot move up
/// inside its posts; the rung plans the mast again with the tie moved up. The
/// claim is the same: no brace end, a mast's cross included, lies in a flare;
/// and masts are still placed, some of them by that retry (path hit).
#[test]
fn no_mast_brace_end_on_the_laid_tube_lies_inside_a_root_flare() {
    let mesh = make_laid_tube().expect("the laid tube fixture");
    let policy = SweepPolicy {
        slenderness_thicken: false,
        ..policy()
    };
    let plan = sweep_with(&mesh, 0.0, &policy);
    let flares = emitted_flares(&plan);
    let lane = &plan.report.slenderness_lane;
    let inside = brace_ends_in_a_flare(&plan, &flares);
    println!(
        "laid tube, thicken off: {} masts, {} placed by a raised retry; bands moved {}, dropped \
         {}; {} brace ends in a flare or the raft; {} warnings",
        lane.masts_dropped,
        lane.masts_raised_above_flare,
        lane.brace_bands_moved_above_flare,
        lane.brace_bands_dropped_in_flare,
        inside.len(),
        lane.warnings.len(),
    );
    assert!(
        lane.masts_dropped > 0,
        "anti-vacuity: the laid tube with thickening off placed no mast"
    );
    assert!(
        lane.masts_raised_above_flare > 0,
        "path hit: no mast was planned again with its tie moved up out of a flare"
    );
    assert!(
        inside.is_empty(),
        "{} brace ends lie inside a root flare or the raft:\n{}",
        inside.len(),
        inside.join("\n")
    );
}

/// **THE CLAIM, on the laid tube**: no brace end lies inside any root flare
/// the file draws, and none lies at or below the raft top. Non-vacuous: the
/// field has braces and flares, and the flares are tall enough to hold a tie
/// (the recorded boss top is 1.7 mm over the plate).
#[test]
fn no_brace_end_on_the_laid_tube_lies_inside_a_root_flare() {
    let mesh = make_laid_tube().expect("the laid tube fixture");
    let plan = sweep_with(&mesh, 0.0, &policy());
    let flares = emitted_flares(&plan);
    let ends = brace_ends(&plan);
    assert!(
        !flares.is_empty(),
        "anti-vacuity: the laid tube drew no root flare"
    );
    assert!(
        !ends.is_empty(),
        "anti-vacuity: the laid tube placed no brace"
    );
    let tallest = flares
        .iter()
        .map(|flare| flare.top[2] - plan.plate_z_mm)
        .fold(0.0_f64, f64::max);
    assert!(
        tallest > 1.0,
        "anti-vacuity: the tallest flare rises only {tallest:.3} mm"
    );

    let inside = brace_ends_in_a_flare(&plan, &flares);
    let lane = &plan.report.slenderness_lane;
    println!(
        "laid tube: {} brace ends, {} root flares (tallest {tallest:.3} mm); {} in a flare or \
         the raft; bands moved up {}, dropped {}; braces added {}, masts {}, warnings {}",
        ends.len(),
        flares.len(),
        inside.len(),
        lane.brace_bands_moved_above_flare,
        lane.brace_bands_dropped_in_flare,
        lane.braces_added,
        lane.masts_dropped,
        lane.warnings.len(),
    );
    assert!(
        inside.is_empty(),
        "{} brace ends lie inside a root flare or the raft:\n{}",
        inside.len(),
        inside.join("\n")
    );
    // PATH HIT: the claim holds because the rule fired on this field, not
    // because no band happened to reach low. Before the rule, 12 brace ends
    // (6 bands) lay inside flares on this fixture (red log, 2026-09-30).
    assert!(
        lane.brace_bands_moved_above_flare + lane.brace_bands_dropped_in_flare > 0,
        "path hit: the free-shaft rule neither moved nor dropped a band on the laid tube"
    );
}
