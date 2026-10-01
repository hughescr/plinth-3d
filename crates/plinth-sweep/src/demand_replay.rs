//! The multi-layer reach check, re-run over a proposed arm set before the
//! removal pass deletes anything (lane neighbour-cover, 2026-09-30).
//!
//! # Why the removal pass needs it
//!
//! Until this lane the removal pass (`remove_under_loaded`) accepted an arm's
//! demand as covered only by a survivor SEATED ON THE SAME LAYER within one
//! lamina's reach. That test can be wrong in both directions:
//!
//! * it is blind to every layer above the seat. An arm roots its demand at
//!   its seat, and the material built on that demand inherits the potential
//!   upward ([`crate::reach::recompute_potential`]), so removing the arm can
//!   unroot cells several layers higher that no seat-layer test looks at;
//! * it measures a straight chamfer distance, while the potential travels only
//!   THROUGH cured cover. Across a gap in the cover the straight line is
//!   shorter than any path the lamina can take.
//!
//! Both are answered the same way: re-run the up-sweep's own reach potential,
//! layer by layer, over the arm set that would remain, and compare it with the
//! same potential over the arm set as it stands. An arm set that leaves a
//! demand cell unrooted which the current set roots is refused. Nothing here
//! is a new criterion: it is the placement's own question, asked again of the
//! field the removal would leave.
//!
//! # What an arm on another layer can and cannot do
//!
//! An arm seated at layer `k` is a root at `k` and nowhere else; material
//! above it inherits that root through vertical inheritance. So an arm seated
//! BELOW a demand cell can cover it (through the cured material between), and
//! an arm seated ABOVE it cannot: when the lower layer is cured and peeled the
//! upper arm's column has not yet touched the model. That is the printing
//! order, not a modelling choice, and the replay carries it by construction
//! because it walks the layers in build order.
//!
//! # Batch-first
//!
//! One replay evaluates the JOINT counterfactual — every tentatively doomed
//! arm gone at once — so the interaction "A and B together uncover what
//! neither does alone" is the case evaluated. Where it finds a cell the full
//! set roots and the proposed set does not, it restores the NEAREST doomed arm
//! that could have rooted it (seated at or below the cell's layer) and checks
//! again. A restored arm on the same layer is relaxed in place; one on a lower
//! layer changes every layer from its seat upward, so the replay restarts. The
//! set the last replay passes is therefore the set that is removed, and it was
//! verified whole. Restarts are bounded by the registered removal pass count;
//! running out restores EVERY doomed arm, which is the over-support direction.

#![expect(
    clippy::cast_possible_truncation,
    reason = "the potential is kept in f32 by `reach`; the one f64 -> f32 cast here is the \
          first-loss ratio (about 1.2), compared against that f32 potential and never stored"
)]
#![expect(
    clippy::needless_range_loop,
    reason = "batch-first: the layer loop walks the slice stack, the plane list and the per-layer \
          root lists at one index, and writes two reach states as it goes"
)]

use plinth_mesh::Mesh;
use plinth_support::perception::SliceSolid;

use crate::grid::Grid;
use crate::raster::{rasterize_into, LayerMasks, RasterScratch};
use crate::reach::{self, ReachState, SurfaceCreep};
use crate::SweepStrut;

/// Everything the replay reads. The same inputs the up-sweep read, so the
/// replay of an unchanged arm set reproduces the placement's own potential.
pub(crate) struct ReplayContext<'a> {
    pub grid: &'a Grid,
    pub slices: &'a [Vec<SliceSolid>],
    /// The slicing plane of each layer; a layer's material is born in
    /// `[plane - h, plane]`, exactly as the up-sweep loads the creep band.
    pub planes: &'a [f64],
    pub mesh: &'a Mesh,
    pub kappa: f64,
    pub layer_height_mm: f64,
    pub layers: usize,
    /// The up-sweep's metric ([`crate::SweepPolicy::straight_reach`]): the
    /// straight length, or the 8-neighbour chamfer when it is off. The replay
    /// must price a run as the placement did, or it reproduces a different
    /// potential from the one the seats were placed on.
    pub straight_reach: bool,
    pub creep_weight: bool,
    pub overhang_centre_deg: f64,
    pub overhang_transition_deg: f64,
    /// Upper bound on restarts; the registered removal pass count.
    pub max_restarts: u32,
    /// `LAMINA_REACH_RATIO_FIRST_LOSS / LAMINA_REACH_RATIO`: the potential at
    /// which the plates 1 + 4 coupon first lost a lamina (2.592 mm against the
    /// placement's 2.130 mm). **Reporting only**: it says how far past the
    /// placement's reach each refused cell lies, and decides nothing.
    pub first_loss_ratio: f64,
}

/// What one call measured. Path-hit counters and the refusals.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReplayOutcome {
    /// Full replays run (1 + restarts). Zero means nothing was doomed.
    pub replays: u64,
    /// Layers on which the proposed set's potential was computed. The witness
    /// that the check ran over the model, not merely that it was called.
    pub trial_layers: u64,
    /// Cover cells unrooted by the CURRENT arm set on the first replay: the
    /// placement's residual demand, re-measured. Equal to the up-sweep's own
    /// residual when no arm has been added or moved since placement.
    pub baseline_unrooted_cells: u64,
    /// Cells the proposed set would have left unrooted that the current set
    /// roots, summed over every check. Each one restored an arm.
    pub newly_unrooted_cells: u64,
    /// Of `newly_unrooted_cells`, those whose potential was still at or below
    /// the first-loss ratio: past the placement's reach, inside the coupon's
    /// first-loss bracket. **Reporting only.**
    pub newly_unrooted_within_first_loss: u64,
    /// The largest finite potential among `newly_unrooted_cells`: how far past
    /// the placement's reach (`U = 1`) the worst refused cell lies. Zero when
    /// none was found. Cells no root reaches at all are not in this number.
    pub newly_unrooted_max_u: f64,
    /// Doomed arms the replay restored.
    pub refused: u64,
    /// Of `refused`, the arms every cell that restored them lay inside the
    /// first-loss bracket (finite and at most `first_loss_ratio`), on the
    /// layer where they were restored. **Reporting only, and partial**: the
    /// layers above a restored seat are never examined without the arm, so
    /// this is not the count a reach at the bracket's far end would free
    /// (see `report::RemovalCoverageReport::refused_by_replay_within_first_loss`).
    pub refused_within_first_loss: u64,
    /// Whether the restart bound ran out and every doomed arm was restored.
    pub exhausted: bool,
}

/// Restore every tentatively doomed arm whose removal would leave a demand
/// cell unrooted that the current field roots. `doomed` is edited in place;
/// what is still doomed on return passed a replay of the whole model.
pub(crate) fn refuse_unrooted_demand(
    context: &ReplayContext<'_>,
    struts: &[SweepStrut],
    doomed: &mut [bool],
) -> ReplayOutcome {
    let mut outcome = ReplayOutcome::default();
    if context.layers == 0 || !doomed.iter().any(|flag| *flag) {
        return outcome;
    }
    // Every arm is a root at its seat layer and seat cell, which is exactly
    // where the up-sweep relaxed it (`up_sweep::place_layer`). Load-placed arms
    // are real tips too and count the same, in both sets.
    let mut roots_by_layer: Vec<Vec<usize>> = vec![Vec::new(); context.layers];
    for (index, strut) in struts.iter().enumerate() {
        if strut.kind == crate::StrutKind::Arm && strut.k_top < context.layers {
            roots_by_layer[strut.k_top].push(index);
        }
    }
    // The worst potential among the cells that restored each arm; `NaN` for an
    // arm restored wholesale, with no cell of its own to name. Reporting only.
    let mut blame_u = vec![f32::NEG_INFINITY; struts.len()];
    let mut verified = false;
    for restart in 0..=context.max_restarts {
        outcome.replays += 1;
        let pass = replay_once(
            context,
            struts,
            &roots_by_layer,
            doomed,
            &mut blame_u,
            &mut outcome,
            restart == 0,
        );
        if matches!(pass, Pass::Verified) || !doomed.iter().any(|flag| *flag) {
            verified = true;
            break;
        }
    }
    if !verified {
        // Out of restarts: keep everything. Over-support, never a bare cell.
        restore_all(doomed, &mut blame_u, &mut outcome);
        outcome.exhausted = true;
    }
    let ratio = context.first_loss_ratio as f32;
    outcome.refused_within_first_loss = blame_u
        .iter()
        .filter(|u| u.is_finite() && **u <= ratio)
        .count() as u64;
    outcome
}

fn restore_all(doomed: &mut [bool], blame_u: &mut [f32], outcome: &mut ReplayOutcome) {
    for (flag, blame) in doomed.iter_mut().zip(blame_u.iter_mut()) {
        if *flag {
            *flag = false;
            *blame = f32::NAN;
            outcome.refused += 1;
        }
    }
}

enum Pass {
    Verified,
    Restart,
}

fn replay_once(
    context: &ReplayContext<'_>,
    struts: &[SweepStrut],
    roots_by_layer: &[Vec<usize>],
    doomed: &mut [bool],
    blame_u: &mut [f32],
    outcome: &mut ReplayOutcome,
    first: bool,
) -> Pass {
    let ratio = context.first_loss_ratio as f32;
    let grid = context.grid;
    let doomed_layers = || {
        doomed
            .iter()
            .enumerate()
            .filter(|(_, flag)| **flag)
            .map(|(index, _)| struts[index].k_top)
    };
    let Some(lowest) = doomed_layers().min() else {
        return Pass::Verified;
    };
    let highest = doomed_layers().max().unwrap_or(lowest);
    let mut creep = if context.creep_weight {
        SurfaceCreep::new(
            context.mesh,
            grid,
            context.overhang_centre_deg,
            context.overhang_transition_deg,
        )
    } else {
        SurfaceCreep::disabled(grid)
    };
    let mut scratch = RasterScratch::new(grid);
    let mut masks = LayerMasks::default();
    // The up-sweep's own metric switch (`run_sweep`), applied to both sets.
    let new_state = || {
        let mut state = ReachState::new(grid);
        if !context.straight_reach {
            state.straight_limit = f32::NEG_INFINITY;
        }
        state
    };
    // "Nearest" doomed arm, for the restore below, in the same metric.
    let metric = crate::ReachMetric {
        reach_mm: context.kappa * context.layer_height_mm,
        straight: context.straight_reach,
    };
    let mut current = new_state();
    let mut proposed: Option<ReachState> = None;
    let mut trial_done = false;
    let mut queue = Vec::new();
    let mut touched = Vec::new();
    let mut fresh = Vec::new();
    let relax = |state: &mut ReachState,
                 cover: &[bool],
                 cell: usize,
                 queue: &mut Vec<usize>,
                 touched: &mut Vec<usize>| {
        touched.clear();
        reach::relax_from_root(grid, cover, context.kappa, cell, state, queue, touched);
    };

    for layer in 0..context.layers {
        rasterize_into(grid, &context.slices[layer], &mut scratch, &mut masks);
        if masks.cover.iter().all(|filled| !*filled) {
            // The up-sweep's own empty-layer reset, for both sets.
            current.u.fill(f32::INFINITY);
            reach::commit_layer(&mut current, &masks.carry);
            if let Some(state) = proposed.as_mut() {
                state.u.fill(f32::INFINITY);
                reach::commit_layer(state, &masks.carry);
            }
            continue;
        }
        creep.load_layer(grid, context.planes[layer] - context.layer_height_mm, context.planes[layer]);
        // Below the lowest doomed seat the two sets are the same set, so the
        // proposed one is only started there, from the current one's carry.
        if proposed.is_none() && !trial_done && layer >= lowest {
            let mut state = new_state();
            state.u_prev.copy_from_slice(&current.u_prev);
            proposed = Some(state);
        }
        reach::recompute_potential(
            grid,
            &masks.cover,
            context.kappa,
            context.layer_height_mm,
            &[],
            &creep,
            &mut current,
        );
        for &index in &roots_by_layer[layer] {
            relax(&mut current, &masks.cover, struts[index].cell, &mut queue, &mut touched);
        }
        if first {
            outcome.baseline_unrooted_cells += masks
                .cover
                .iter()
                .zip(&current.u)
                .filter(|(filled, u)| **filled && **u > 1.0)
                .count() as u64;
        }
        let Some(trial) = proposed.as_mut() else {
            reach::commit_layer(&mut current, &masks.carry);
            continue;
        };
        outcome.trial_layers += 1;
        reach::recompute_potential(
            grid,
            &masks.cover,
            context.kappa,
            context.layer_height_mm,
            &[],
            &creep,
            trial,
        );
        for &index in &roots_by_layer[layer] {
            if !doomed[index] {
                relax(trial, &masks.cover, struts[index].cell, &mut queue, &mut touched);
            }
        }
        // Every cell the current set roots and the proposed one does not
        // restores the nearest doomed arm that could have rooted it.
        loop {
            fresh.clear();
            for cell in 0..grid.cells() {
                if masks.cover[cell] && trial.u[cell] > 1.0 && current.u[cell] <= 1.0 {
                    fresh.push(cell);
                }
            }
            if fresh.is_empty() {
                break;
            }
            outcome.newly_unrooted_cells += fresh.len() as u64;
            for &cell in &fresh {
                let u = trial.u[cell];
                if u.is_finite() {
                    outcome.newly_unrooted_max_u = outcome.newly_unrooted_max_u.max(f64::from(u));
                    if u <= ratio {
                        outcome.newly_unrooted_within_first_loss += 1;
                    }
                }
            }
            let mut restart = false;
            let mut restored_any = false;
            for &cell in &fresh {
                let u = trial.u[cell];
                if u <= 1.0 {
                    // Rooted by an arm restored earlier in this sweep of `fresh`.
                    continue;
                }
                let blamed = doomed
                    .iter()
                    .enumerate()
                    .filter(|(index, flag)| **flag && struts[*index].k_top <= layer)
                    .map(|(index, _)| (metric.distance_mm(grid, cell, struts[index].cell), index))
                    .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                let Some((_, index)) = blamed else {
                    continue;
                };
                doomed[index] = false;
                // A non-finite `u` (no root reaches the cell at all) is kept
                // as infinity, which no bracket contains.
                blame_u[index] = blame_u[index].max(u);
                outcome.refused += 1;
                restored_any = true;
                if struts[index].k_top == layer {
                    relax(trial, &masks.cover, struts[index].cell, &mut queue, &mut touched);
                } else {
                    restart = true;
                }
            }
            if restart {
                return Pass::Restart;
            }
            if !restored_any {
                // No doomed arm at or below this layer is left to blame, yet
                // the proposed set roots less than the current one. Nothing
                // proposed can be trusted: keep every arm.
                restore_all(doomed, blame_u, outcome);
                return Pass::Verified;
            }
        }
        reach::commit_layer(&mut current, &masks.carry);
        reach::commit_layer(trial, &masks.carry);
        // Above the highest doomed seat the two sets place the same roots, so
        // once the two carries agree every layer above agrees too. The first
        // replay still walks the current set to the top, so the residual it
        // reports is the whole model's and can be held against the placement's.
        if layer >= highest && trial.u_prev == current.u_prev {
            if !first {
                return Pass::Verified;
            }
            proposed = None;
            trial_done = true;
        }
    }
    Pass::Verified
}

#[cfg(test)]
mod tests {
    //! Hand-derived geometry. A slab `S1` 4 mm long (x 0..4), 1 mm wide
    //! (y −0.5..0.5), its underside at z 1.0, so its first layer is layer 20
    //! at 0.05 mm (plane 1.025). Reach is one lamina, κ·h = 42.6006450 × 0.05
    //! = 2.1300 mm, in the 8-neighbour chamfer metric: a cell `dx` along and
    //! `dy` across is `(dx − dy) + √2·dy` away when `dx ≥ dy`.
    //!
    //! The farthest cell of `S1` from the nearest of two arms at x 0.9 and
    //! 3.1 is at x 2.0, y ±0.5: `(1.1 − 0.5) + √2·0.5 = 1.307 mm`, inside
    //! reach. The far end, x 4.0, from x 0.9 alone: `3.1 mm`, outside it.
    //!
    //! No production function decides anything in these tests: the expected
    //! verdicts are the two numbers above.

    use super::{refuse_unrooted_demand, ReplayContext};
    use crate::grid::{plan_grid, Grid};
    use crate::{Landing, StrutKind, SweepStrut};
    use plinth_mesh::{make_box, Mesh};
    use plinth_support::perception::{slice_stack, SliceStackLimits};

    const H: f64 = 0.05;
    const LAYERS: usize = 50;
    const KAPPA: f64 = 42.600_645;

    /// `S1` alone, or `S1` and `S2`: the same slab again, directly above it
    /// from z 2.0 (layer 40), and not touching it.
    fn fixture(with_upper: bool) -> Mesh {
        let lower = make_box([4.0, 1.0, 0.5], [2.0, 0.0, 1.0]).expect("S1");
        if !with_upper {
            return lower;
        }
        let upper = make_box([4.0, 1.0, 0.5], [2.0, 0.0, 2.0]).expect("S2");
        let offset = u32::try_from(lower.verts().len()).expect("small");
        let mut verts = lower.verts().to_vec();
        verts.extend_from_slice(upper.verts());
        let mut tris = lower.tris().to_vec();
        tris.extend(upper.tris().iter().map(|t| t.map(|v| v + offset)));
        Mesh::new(verts, tris).expect("two disjoint boxes")
    }

    fn cell_at(grid: &Grid, x: f64, y: f64) -> usize {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "fixture coordinates inside the grid's own bounds"
        )]
        let (col, row) = (grid.col_coord(x).floor() as usize, grid.row_coord(y).floor() as usize);
        row * grid.cols() + col
    }

    fn arm(grid: &Grid, id: u32, x: f64, k_top: usize) -> SweepStrut {
        SweepStrut {
            id,
            cell: cell_at(grid, x, 0.0),
            center_mm: [x, 0.0],
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
            path: Vec::new(),
            seat_face: crate::SeatFace::default(),
            seat_normal: [0.0; 3],
            straight_fallback: None,
            fallback_path: Vec::new(),
        }
    }

    fn run(
        mesh: &Mesh,
        place: &dyn Fn(&Grid) -> Vec<SweepStrut>,
        doomed: &mut Vec<bool>,
    ) -> super::ReplayOutcome {
        let (grid, _) = plan_grid(0.0, -0.5, 4.0, 0.5, LAYERS, H, 1.0, u64::MAX);
        let planes: Vec<f64> = (0..LAYERS).map(|l| (l as f64 + 0.5) * H).collect();
        let slices = slice_stack(mesh, &planes, SliceStackLimits::production()).expect("slices");
        let struts = place(&grid);
        if doomed.is_empty() {
            doomed.resize(struts.len(), false);
        }
        let context = ReplayContext {
            grid: &grid,
            slices: &slices,
            planes: &planes,
            mesh,
            kappa: KAPPA,
            layer_height_mm: H,
            layers: LAYERS,
            straight_reach: true,
            creep_weight: false,
            overhang_centre_deg: 45.0,
            overhang_transition_deg: 25.0,
            max_restarts: 3,
            first_loss_ratio: 2.592 / 2.130,
        };
        refuse_unrooted_demand(&context, &struts, doomed)
    }

    /// Three arms on `S1` at x 0.9, 2.0 and 3.1. The outer two reach every
    /// cell (worst 1.307 mm), so the middle one is not needed and stays
    /// doomed; nothing is refused and nothing is found unrooted.
    #[test]
    fn an_arm_whose_cells_two_neighbours_reach_is_let_go() {
        let mesh = fixture(false);
        let place =
            |g: &Grid| vec![arm(g, 0, 0.9, 20), arm(g, 1, 2.0, 20), arm(g, 2, 3.1, 20)];
        let mut doomed = vec![false, true, false];
        let outcome = run(&mesh, &place, &mut doomed);
        assert_eq!(doomed, vec![false, true, false], "{outcome:?}");
        assert_eq!(outcome.refused, 0);
        assert_eq!(outcome.newly_unrooted_cells, 0);
        assert_eq!(outcome.baseline_unrooted_cells, 0, "all three root the whole slab");
        assert!(outcome.replays >= 1 && outcome.trial_layers > 0, "the replay ran: {outcome:?}");
    }

    /// The same slab with two arms, x 0.9 and 3.1, and the one at 3.1 doomed:
    /// the far end is 3.1 mm from x 0.9, outside reach, so the arm is kept,
    /// and the cells that made it so are counted.
    #[test]
    fn an_arm_whose_far_cells_no_neighbour_reaches_is_kept() {
        let mesh = fixture(false);
        let place = |g: &Grid| vec![arm(g, 0, 0.9, 20), arm(g, 1, 3.1, 20)];
        let mut doomed = vec![false, true];
        let outcome = run(&mesh, &place, &mut doomed);
        assert_eq!(doomed, vec![false, false], "{outcome:?}");
        assert_eq!(outcome.refused, 1);
        assert!(outcome.newly_unrooted_cells > 0, "{outcome:?}");
    }

    /// BUILD ORDER. The same two arms on `S1`, plus an arm seated on `S2`
    /// directly above the doomed one (x 3.1, layer 40). In straight-line
    /// distance it stands on top of every cell the doomed arm roots, so a
    /// test that took coverage from an arm on ANY layer by distance alone
    /// would let the doomed arm go. It must not: when layer 20 cures and
    /// peels, the arm at layer 40 has not reached the model yet.
    #[test]
    fn an_arm_seated_above_does_not_cover_the_layer_below() {
        let mesh = fixture(true);
        let place =
            |g: &Grid| vec![arm(g, 0, 0.9, 20), arm(g, 1, 3.1, 20), arm(g, 2, 3.1, 40)];
        let mut doomed = vec![false, true, false];
        let outcome = run(&mesh, &place, &mut doomed);
        assert_eq!(doomed, vec![false, false, false], "{outcome:?}");
        assert_eq!(outcome.refused, 1);
        // And S2's own arm, doomed, is kept too: nothing else roots S2.
        let mut doomed = vec![false, false, true];
        let outcome = run(&mesh, &place, &mut doomed);
        assert_eq!(doomed, vec![false, false, false], "{outcome:?}");
    }

    /// The joint counterfactual. Doom the arm at 0.9 AND the middle one: the
    /// replay finds the near end unrooted, restores the NEAREST doomed arm
    /// (0.9, not 2.0), and the middle one still goes.
    #[test]
    fn a_joint_removal_restores_the_nearest_arm_and_lets_the_other_go() {
        let mesh = fixture(false);
        let place =
            |g: &Grid| vec![arm(g, 0, 0.9, 20), arm(g, 1, 2.0, 20), arm(g, 2, 3.1, 20)];
        let mut doomed = vec![true, true, false];
        let outcome = run(&mesh, &place, &mut doomed);
        assert_eq!(doomed, vec![false, true, false], "{outcome:?}");
        assert_eq!(outcome.refused, 1);
    }
}
