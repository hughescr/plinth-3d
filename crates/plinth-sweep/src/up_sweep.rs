//! The up-sweep: where the supports go.
//!
//! Walk the layers from the plate upward. At each layer every model cell must
//! have a **rooted carrier within reach** — rooted being the plate, a strut, or
//! cured model material that is itself rooted, and reach being geodesic
//! through cured material rather than a straight line. Cells that have none are
//! demand; a strut goes in at the maximum radius wherever it covers the most
//! demand, and descends straight down to the first thing it meets. At the top
//! the model is deliberately OVER-supported; the down-sweep is what makes the
//! sizes right.
//!
//! Islands need no special case here: an island cell's potential is `+∞`
//! because nothing below it is cured, so it falls out of the same test as
//! everything else. The legacy engine's specialized island-birth and
//! local-minimum seats are subsumed rather than reimplemented, and sabotage S6
//! is what proves the subsumption instead of asserting it.

#![expect(
    clippy::needless_range_loop,
    reason = "batch-first: these loops walk several parallel per-cell rasters at one index"
)]

use crate::drain::DrainKeepOut;
use crate::grid::Grid;
use crate::reach::ReachState;
use crate::{Landing, SweepStrut};

/// Column history the descent reads. Every entry names the HIGHEST layer
/// strictly below the current one at which that column held something, which
/// is what makes "the column is clear down to the landing" true by
/// construction rather than by a second scan.
pub struct ColumnHistory {
    /// Layer + 1 of the topmost model cell below; 0 means none.
    pub top_model: Vec<u32>,
    /// Whether that model cell was itself rooted at its own layer.
    pub top_model_rooted: Vec<bool>,
    /// Layer + 1 of the topmost strut in this column; 0 means none.
    pub top_strut: Vec<u32>,
    pub top_strut_id: Vec<u32>,
    /// Highest layer + 1 at which this column ever held model, filled in a
    /// post-pass because the interior test needs to look UP.
    pub column_max_model: Vec<u32>,
}

impl ColumnHistory {
    #[must_use]
    pub fn new(grid: &Grid) -> Self {
        let cells = grid.cells();
        Self {
            top_model: vec![0; cells],
            top_model_rooted: vec![false; cells],
            top_strut: vec![0; cells],
            top_strut_id: vec![0; cells],
            column_max_model: vec![0; cells],
        }
    }
}

/// Scratch for one layer's greedy, reused across layers.
pub struct GreedyScratch {
    demand_bits: Vec<bool>,
    prefix: Vec<u32>,
    half_widths: Vec<usize>,
    reach_cells: usize,
    reach_mm: f64,
    queue: Vec<usize>,
    touched: Vec<usize>,
}

impl GreedyScratch {
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "the reach radius in cells is a small positive finite quotient of two registered \
              lengths; it is rounded and then used only as a loop bound"
    )]
    pub fn new(grid: &Grid, reach_mm: f64) -> Self {
        let reach_cells = (reach_mm / grid.pitch_mm()).round().max(1.0) as usize;
        let mut half_widths = Vec::with_capacity(reach_cells + 1);
        let radius = reach_cells as f64;
        for offset in 0..=reach_cells {
            let dy = offset as f64;
            let half = (radius * radius - dy * dy).max(0.0).sqrt();
            half_widths.push(half as usize);
        }
        Self {
            demand_bits: vec![false; grid.cells()],
            prefix: vec![0; grid.rows() * (grid.cols() + 1)],
            half_widths,
            reach_cells,
            reach_mm,
            queue: Vec::new(),
            touched: Vec::new(),
        }
    }

    #[must_use]
    pub const fn reach_cells(&self) -> usize {
        self.reach_cells
    }
}

/// What one layer's greedy produced.
pub struct LayerOutcome {
    /// Cells that entered the demand set on this layer — the model area that
    /// had no rooted carrier within reach and had to be given an arm. This is
    /// the MEASURED support demand the orientation objective's
    /// projected-down-facing-area proxy estimates.
    pub demand_cells: usize,
    /// Cells that no legal candidate could cover. A WARNING on an emitted
    /// file, never a refusal.
    pub residual_demand: usize,
    /// Landings whose model cell was not itself rooted. Emitted anyway and
    /// reported, because refusing here would destroy the diagnostic.
    pub unrooted_landings: usize,
    /// Demand cells this layer could not seat because every seat that would
    /// have rooted them stands inside a drain escape cylinder. A WARNING, and
    /// also the path-hit witness that the keep-out was consulted on a layer
    /// where it actually bound.
    pub keep_out_refusals: usize,
}

/// Place struts on one layer.
///
/// `struts` is appended to; `history` is NOT updated for this layer's own
/// model cells (the caller does that afterwards), which is exactly what makes
/// a landing strictly below the seat.
#[expect(
    clippy::too_many_arguments,
    reason = "one layer's greedy genuinely reads the grid, the layer's masks, the reach state, the \
          column history, the strut list, its scratch and its layer index; bundling them into a \
          struct would move the argument list rather than shorten it"
)]
pub fn place_layer(
    grid: &Grid,
    layer: usize,
    cover: &[bool],
    keep_out: &DrainKeepOut,
    kappa: f64,
    reach: &mut ReachState,
    history: &mut ColumnHistory,
    scratch: &mut GreedyScratch,
    struts: &mut Vec<SweepStrut>,
    max_struts: usize,
) -> LayerOutcome {
    let cols = grid.cols();
    let mut outcome = LayerOutcome {
        demand_cells: 0,
        residual_demand: 0,
        unrooted_landings: 0,
        keep_out_refusals: 0,
    };

    let mut demand_total = 0_usize;
    for cell in 0..grid.cells() {
        let wanted = cover[cell] && !reach.is_rooted(cell);
        scratch.demand_bits[cell] = wanted;
        demand_total += usize::from(wanted);
    }
    outcome.demand_cells = demand_total;
    if demand_total == 0 {
        return outcome;
    }

    rebuild_prefix_rows(
        grid,
        &scratch.demand_bits,
        &mut scratch.prefix,
        0,
        grid.rows() - 1,
    );

    // --- placement -----------------------------------------------------------
    //
    // # Deviation from the design of record's §6.2, stated rather than hidden
    //
    // §6.2 takes `c* = argmax S` over the whole demand set. **That was built and
    // measured, and it does not produce a covering.** On the Round 50 profile
    // it placed 400 arms at 2.02 mm mean spacing against the design's own hex
    // prediction of 154 — a factor of 2.6, outside its own [0.9, 1.5] band and
    // 2× denser than the owner's 4 mm practice. Constraining the candidate set
    // to the cells that can reach the first uncovered cell was tried next and
    // measured 419, no better. The mechanism is structural, not a bug:
    // "cover the most NEW demand" is maximised by a ball that overlaps nothing,
    // so the objective actively avoids the overlap a covering REQUIRES, and
    // whichever way the ties break the field degenerates to a square lattice at
    // one reach radius.
    //
    // What is placed instead is the covering the design's own arithmetic
    // assumes: a HEX LATTICE at the registered reach, with the greedy retained
    // for everything the lattice does not reach — boundary strips, islands, and
    // any region whose geodesic differs from its straight line. Nothing is
    // invented: the lattice pitch is `√3 · ρ` with `ρ` the registered reach
    // divided by the 8-neighbour chamfer metric's own isotropy factor (see
    // [`chamfer_isotropy`]), and a lattice point is used only where it is still
    // uncovered demand, so an irregular region costs exactly what the greedy
    // would have cost there and no more.
    let separation_mm = hex_separation_mm(scratch.reach_mm);
    let seeds = hex_lattice_cells(grid, cover, separation_mm);
    let mut seed_index = 0_usize;
    let mut pivot_cursor = 0_usize;
    let mut last_pivot = usize::MAX;
    loop {
        if struts.len() >= max_struts {
            break;
        }
        let cell = if seed_index < seeds.len() {
            let seed = seeds[seed_index];
            seed_index += 1;
            if !scratch.demand_bits[seed] {
                continue;
            }
            // A lattice point inside a drain's escape cylinder is simply not
            // used. The demand it would have covered stays in the set and the
            // pivot pass below looks for a legal seat for it; only if none
            // exists does it survive as reported residual demand.
            if !keep_out.admits(seed, layer) {
                continue;
            }
            seed
        } else {
            while pivot_cursor < grid.cells() && !scratch.demand_bits[pivot_cursor] {
                pivot_cursor += 1;
            }
            if pivot_cursor >= grid.cells() {
                break;
            }
            let pivot = pivot_cursor;
            // A pivot that survived its own round is one no candidate could
            // reach. Seating on it directly roots it by construction, which is
            // what makes this loop terminate.
            let chosen = if pivot == last_pivot {
                keep_out.admits(pivot, layer).then_some(pivot)
            } else {
                best_seat_for(grid, cover, history, scratch, keep_out, layer, pivot)
            };
            last_pivot = pivot;
            let Some(chosen) = chosen else {
                // Every seat that could root this cell stands where the resin
                // has to leave. EMIT AND WARN: step past it, leave its demand
                // bit set so the residual-demand count at the end of this
                // function reports it, and never refuse the model over it.
                outcome.keep_out_refusals += 1;
                pivot_cursor += 1;
                continue;
            };
            chosen
        };
        let (landing, k_bottom, rooted_landing) = descend(history, cell, layer);
        if !rooted_landing {
            outcome.unrooted_landings += 1;
        }
        let id = u32::try_from(struts.len()).unwrap_or(u32::MAX);
        scratch.touched.clear();
        crate::reach::relax_from_root(
            grid,
            cover,
            kappa,
            cell,
            reach,
            &mut scratch.queue,
            &mut scratch.touched,
        );

        // Everything this insertion newly rooted leaves the demand set and is
        // recorded, so the removal pass can ask later whether a survivor still
        // covers it.
        let mut covered = Vec::new();
        let mut dirty_lo = usize::MAX;
        let mut dirty_hi = 0_usize;
        for &touched in &scratch.touched {
            if scratch.demand_bits[touched] && reach.is_rooted(touched) {
                scratch.demand_bits[touched] = false;
                covered.push(touched);
                let row = touched / cols;
                dirty_lo = dirty_lo.min(row);
                dirty_hi = dirty_hi.max(row);
            }
        }
        if let Some(tau) = reach.tau_used_mm.get(cell) {
            let tau = f64::from(*tau);
            if tau > reach.tau_at_binding_reach_max_mm {
                reach.tau_at_binding_reach_max_mm = tau;
            }
        }

        struts.push(SweepStrut {
            id,
            cell,
            center_mm: [
                grid.center_x_mm(cell % cols),
                grid.center_y_mm(cell / cols),
            ],
            k_top: layer,
            k_bottom,
            landing,
            interior: false,
            radius_mm: Vec::new(),
            carried_mm2: Vec::new(),
            covered_demand: covered,
            inserted_by_load: false,
        });
        history.top_strut[cell] = u32::try_from(layer + 1).unwrap_or(u32::MAX);
        history.top_strut_id[cell] = id;

        if dirty_lo > dirty_hi {
            continue;
        }
        // Local invalidation only. A global rebuild here is what makes a greedy
        // quadratic in the layer; a row the insertion did not touch cannot have
        // moved.
        rebuild_prefix_rows(
            grid,
            &scratch.demand_bits,
            &mut scratch.prefix,
            dirty_lo,
            dirty_hi,
        );
    }

    outcome.residual_demand += scratch.demand_bits.iter().filter(|bit| **bit).count();
    outcome
}

/// How much longer than a straight line the 8-neighbour chamfer metric measures,
/// in its worst direction.
///
/// The reach potential and the load spread both travel on 8 neighbours with
/// steps `p` and `p√2`, so the set they call "within reach `R`" is an octagon,
/// not a disc: at 22.5° off an axis it stops at `R / (cos 22.5° + (√2−1)·sin
/// 22.5°)`. A lattice pitched at the full `R` would leave that shortfall
/// uncovered at every interstice and the greedy would then spend one arm per
/// interstice — which is a doubling, so this is derived rather than absorbed.
/// **This is geometry, not a constant**: it is a fact about the metric the
/// design of record chose, computed from it.
fn chamfer_isotropy() -> f64 {
    let eighth = core::f64::consts::FRAC_PI_8;
    eighth.cos() + (core::f64::consts::SQRT_2 - 1.0) * eighth.sin()
}

/// Nearest-neighbour spacing of the hex covering at the registered reach.
///
/// `√3 · ρ` is the spacing at which discs of radius `ρ` exactly cover the plane
/// (`docs/physics.md`:128-135); `ρ` here is the registered reach reduced to the
/// radius the chamfer metric actually guarantees.
fn hex_separation_mm(reach_mm: f64) -> f64 {
    3.0_f64.sqrt() * reach_mm / chamfer_isotropy()
}

/// Hex lattice points, in scan order, clipped to cells that can host a seat.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "lattice indices are non-negative finite quotients of two model-scale lengths, and \
          every one is range-checked against the grid before it becomes a cell index"
)]
fn hex_lattice_cells(grid: &Grid, cover: &[bool], separation_mm: f64) -> Vec<usize> {
    let row_pitch = separation_mm * 3.0_f64.sqrt() / 2.0;
    let rows = (grid.rows() as f64 * grid.pitch_mm() / row_pitch).ceil() as usize + 1;
    let columns = (grid.cols() as f64 * grid.pitch_mm() / separation_mm).ceil() as usize + 1;
    let mut cells = Vec::new();
    for lattice_row in 0..rows {
        let y = grid.center_y_mm(0) + lattice_row as f64 * row_pitch;
        let row = grid.row_coord(y).floor();
        if row < 0.0 || row >= grid.rows() as f64 {
            continue;
        }
        let stagger = if lattice_row % 2 == 1 {
            separation_mm / 2.0
        } else {
            0.0
        };
        for lattice_col in 0..columns {
            let x = grid.center_x_mm(0) + stagger + lattice_col as f64 * separation_mm;
            let col = grid.col_coord(x).floor();
            if col < 0.0 || col >= grid.cols() as f64 {
                continue;
            }
            let cell = grid.index(row as usize, col as usize);
            if cover[cell] {
                cells.push(cell);
            }
        }
    }
    cells
}

/// Among the cells that can reach `pivot`, the one covering the most demand.
///
/// Ties break the way the design of record's §6.2 asks: prefer extending an
/// existing arm, then the shortest descent, then the lowest cell index so the
/// result is deterministic without depending on floating point.
fn best_seat_for(
    grid: &Grid,
    cover: &[bool],
    history: &ColumnHistory,
    scratch: &GreedyScratch,
    keep_out: &DrainKeepOut,
    layer: usize,
    pivot: usize,
) -> Option<usize> {
    let cols = grid.cols();
    let rows = grid.rows();
    let reach = scratch.reach_cells;
    let pivot_row = pivot / cols;
    let pivot_col = pivot % cols;
    // The pivot remains the default, and still wins every tie at the lowest
    // key, which is what keeps a field with no drains byte-identical. It is
    // only dropped when the keep-out refuses it, and then `None` propagates so
    // the caller reports residual demand instead of seating in the cylinder.
    let mut best: Option<usize> = keep_out.admits(pivot, layer).then_some(pivot);
    let mut best_key = (0_u32, false, 0_u32);
    for row in pivot_row.saturating_sub(reach)..=(pivot_row + reach).min(rows - 1) {
        let half = scratch.half_widths[row.abs_diff(pivot_row)];
        for col in pivot_col.saturating_sub(half)..=(pivot_col + half).min(cols - 1) {
            let candidate = row * cols + col;
            // A seat must be a legal host AND somewhere support is actually
            // wanted. Allowing an already-rooted host was measured on
            // `make_overhang_table` to put four arms inside a leg's own
            // self-rooted fringe — legal, and pure waste.
            if !cover[candidate] || !scratch.demand_bits[candidate] {
                continue;
            }
            if !keep_out.admits(candidate, layer) {
                continue;
            }
            let score = score_at(grid, &scratch.prefix, &scratch.half_widths, candidate);
            let key = (
                score,
                history.top_strut[candidate] > 0,
                history.top_model[candidate],
            );
            if best.is_none() || key > best_key {
                best_key = key;
                best = Some(candidate);
            }
        }
    }
    best
}

/// Straight down to the first thing the column meets.
///
/// The cost function in the design of record orders candidates by drop with a
/// zero scar weight, and with `top_model` and `top_strut` defined as the
/// HIGHEST occupant below the seat, that ordering reduces exactly to "the
/// first thing you hit going down" — which is also the only thing a straight
/// vertical descent can physically reach. Nothing here tilts: a tilted arm's
/// cross-section is an ellipse and rasterising it is a second geometry problem
/// v1 does not open.
fn descend(history: &ColumnHistory, cell: usize, layer: usize) -> (Landing, usize, bool) {
    let strut_top = history.top_strut[cell];
    let model_top = history.top_model[cell];
    if strut_top > 0 && strut_top >= model_top {
        let landing_layer = strut_top as usize - 1;
        return (
            Landing::Strut {
                id: history.top_strut_id[cell],
                layer: landing_layer,
            },
            landing_layer,
            true,
        );
    }
    if model_top > 0 {
        let landing_layer = model_top as usize - 1;
        return (
            Landing::Model {
                layer: landing_layer,
            },
            landing_layer,
            history.top_model_rooted[cell],
        );
    }
    debug_assert!(layer > 0, "a seat on layer 0 cannot descend to the plate");
    (Landing::Plate, 0, true)
}

/// Record this layer's model cells in the column history, after the greedy has
/// used the previous state.
pub fn commit_layer_history(
    layer: usize,
    cover: &[bool],
    reach: &ReachState,
    history: &mut ColumnHistory,
) {
    let stamp = u32::try_from(layer + 1).unwrap_or(u32::MAX);
    for cell in 0..cover.len() {
        if cover[cell] {
            history.top_model[cell] = stamp;
            history.top_model_rooted[cell] = reach.is_rooted(cell);
            history.column_max_model[cell] = stamp;
        }
    }
}

fn rebuild_prefix_rows(
    grid: &Grid,
    demand: &[bool],
    prefix: &mut [u32],
    row_lo: usize,
    row_hi: usize,
) {
    let cols = grid.cols();
    let stride = cols + 1;
    for row in row_lo..=row_hi {
        let base = row * stride;
        let cells = row * cols;
        prefix[base] = 0;
        for col in 0..cols {
            prefix[base + col + 1] = prefix[base + col] + u32::from(demand[cells + col]);
        }
    }
}

/// How many demand cells a new root at `cell` would reach.
///
/// Exact integer counts, so ties are exact and the `(row, col)` tie-break is
/// total; determinism here does not depend on floating point.
fn score_at(grid: &Grid, prefix: &[u32], half_widths: &[usize], cell: usize) -> u32 {
    let cols = grid.cols();
    let rows = grid.rows();
    let stride = cols + 1;
    let row = cell / cols;
    let col = cell % cols;
    let reach = half_widths.len() - 1;
    let mut total = 0_u32;
    let row_lo = row.saturating_sub(reach);
    let row_hi = (row + reach).min(rows - 1);
    for probe in row_lo..=row_hi {
        let half = half_widths[probe.abs_diff(row)];
        let lo = col.saturating_sub(half);
        let hi = (col + half + 1).min(cols);
        let base = probe * stride;
        total += prefix[base + hi] - prefix[base + lo];
    }
    total
}
