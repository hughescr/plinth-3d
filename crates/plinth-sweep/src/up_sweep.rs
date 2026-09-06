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
use crate::lean::LeanLaw;
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
    /// Reused across every descent so the lean search allocates once per run
    /// rather than once per arm.
    lean_candidates: Vec<super::up_sweep::LeanCandidate>,
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
            lean_candidates: Vec::new(),
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
    /// Arms whose descent leaned rather than dropping straight down, because
    /// leaning was cheaper under the SAME objective — arm material, with the
    /// leaned part priced by the registered overhang weight.
    pub arms_leaned: usize,
    /// Arms that leaned onto ANOTHER ARM. The merged trunk carries the summed
    /// load through the existing `Landing::Strut` injection, so its section
    /// grows under the area law exactly as if the load had been its own.
    pub arms_merged: usize,
    /// Descents where a cheaper lean was found and then REFUSED, because the
    /// path it needed ran through model material, through another arm, or
    /// through a drain's escape cylinder. The witness that the path check
    /// actually bites; a WARNING quantity, never a refusal.
    pub lean_paths_blocked: usize,
    /// Candidate landings the lean search priced. The path-hit witness that
    /// the search ran at all.
    pub lean_candidates_priced: u64,
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
    law: &LeanLaw,
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
        arms_leaned: 0,
        arms_merged: 0,
        lean_paths_blocked: 0,
        lean_candidates_priced: 0,
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
        let (landing, k_bottom, rooted_landing, path) = descend_leaning(
            grid,
            history,
            keep_out,
            law,
            separation_mm,
            cell,
            layer,
            &mut scratch.lean_candidates,
            &mut outcome,
        );
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
            // Recorded ONLY for a leaned arm, and recorded NOW, because the
            // column history this reads is a running state that will not say
            // the same thing again once the sweep has passed this layer.
            seat_solid_layers: 0,
            seat_normal: [0.0; 3],
            straight_fallback: (!path.is_empty()).then(|| {
                let (landing, bottom, _) = descend(history, cell, layer);
                (landing, bottom)
            }),
            path,
        });
        // Every column the arm passes through now holds an arm, not just its
        // seat column: a leaned arm that only stamped its seat would let the
        // next descent route straight through its shaft.
        let stamp = u32::try_from(layer + 1).unwrap_or(u32::MAX);
        let placed = struts.last().expect("the arm just pushed");
        if placed.path.is_empty() {
            history.top_strut[cell] = stamp;
            history.top_strut_id[cell] = id;
        } else {
            for (slot, &occupied) in placed.path.iter().enumerate() {
                let occupied = occupied as usize;
                let here = u32::try_from(k_bottom + slot + 1).unwrap_or(u32::MAX);
                if history.top_strut[occupied] < here {
                    history.top_strut[occupied] = here;
                    history.top_strut_id[occupied] = id;
                }
            }
            if history.top_strut[cell] < stamp {
                history.top_strut[cell] = stamp;
                history.top_strut_id[cell] = id;
            }
        }

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
/// What a landing still owes, in mm of load path, after the arm stops.
///
/// **A LANDING IS NOT ALWAYS A SINK, AND PRICING IT AS ONE IS A MEASURED
/// DEFECT.** The plate reacts a load and ends its path; another arm has
/// already paid for its own way down and the merge adds no new path; but model
/// material only HANDS THE LOAD ON — it must still travel from the strike face
/// to the plate, through whatever is under it.
///
/// Ignoring that was measured on 2026-09-04 and it is degenerate. Priced on arm
/// length alone, the cheapest descent from anywhere on a sphere's underside is
/// a one-lamina stub onto the sphere's own surface a fraction of a millimetre
/// inboard: the Sphere went from 116 arms with 114.20 mm² reaching the plate to
/// 19 plate arms and 107.50 mm², its median slenderness collapsed to 0.5:1
/// (arms half as long as they are wide are discs, not supports), its emitted
/// primitives fell 314 → 251 as the stubs lost their root flares, its uncarried
/// load ROSE 371.54 → 378.21 mm², and its total scar ROSE 10.75 → 12.51 mm² —
/// the engine buying a shorter arm with a new scar, which is the other half of
/// the objective the length alone cannot see.
///
/// The charge is the honest LOWER bound on what a model landing defers: the
/// strike face's own height above the plate, `k_bottom · h`, because no path
/// from there to the plate can be shorter than straight down. It is a
/// derivation from the frame, not a weight, and it has no tunable part.
///
/// Its consequence is stated rather than discovered: a straight descent costs
/// exactly `layer · h` whether it ends on the plate or on the model, so an
/// up-sweep lean can only ever win by ending on the PLATE or by MERGING onto
/// another arm. Landing higher up the model surface is the down-sweep's job,
/// where the arm exists to relieve a specific overload and its landing is
/// chosen by the load, not by its length.
fn deferred_path_mm(landing: Landing, k_bottom: usize, layer_height_mm: f64) -> f64 {
    match landing {
        Landing::Plate | Landing::Strut { .. } => 0.0,
        #[expect(
            clippy::cast_precision_loss,
            reason = "a layer index bounded by the plan's own layer count is exact in f64"
        )]
        Landing::Model { .. } => k_bottom as f64 * layer_height_mm,
    }
}

/// One candidate landing for a descent, priced but not yet path-checked.
#[derive(Clone, Copy, Debug)]
struct LeanCandidate {
    cost_mm: f64,
    /// `0` merge onto an arm, `1` land on the model, `2` land on the plate.
    /// Only ever a tie-break, and it is stated here rather than left to the
    /// order the scan happens to visit cells in.
    class: u8,
    cell: usize,
    k_bottom: usize,
    landing: Landing,
    rooted: bool,
}

/// The descent, with the lean the owner asked for on 2026-09-03.
///
/// Straight down is computed first and is the answer unless some leaning
/// descent is STRICTLY cheaper under the arm's own material cost. Ties go to
/// straight down, which is what keeps a flat disc — where every column's top
/// is at the same layer, so no lean can shorten anything — byte-identical.
///
/// # Why the search is a cone and not a disc
///
/// Any descent's cost is at least the straight-line distance it covers, so a
/// candidate further from the seat than the straight descent's own length can
/// never be cheaper; and the law admits at most `max_step` of sideways travel
/// per layer, so a candidate that lands `m` layers down must be within
/// `m · max_step`. Those two bounds are what stop this being a whole-grid
/// search: the visited box is capped by the arm's own length, and the accepted
/// set is a cone.
#[expect(
    clippy::too_many_arguments,
    reason = "the descent reads the grid, the history, the keep-out, the law, its own seat and \
          layer, its search bound and the outcome it reports into; a struct would move the \
          argument list rather than shorten it"
)]
fn descend_leaning(
    grid: &Grid,
    history: &ColumnHistory,
    keep_out: &DrainKeepOut,
    law: &LeanLaw,
    search_radius_mm: f64,
    cell: usize,
    layer: usize,
    scratch: &mut Vec<LeanCandidate>,
    outcome: &mut LayerOutcome,
) -> (Landing, usize, bool, Vec<u32>) {
    let (straight_landing, straight_bottom, straight_rooted) = descend(history, cell, layer);
    let straight_layers = layer.saturating_sub(straight_bottom);
    let Some(straight_length) = law.descent_cost_mm(straight_layers, 0.0) else {
        return (straight_landing, straight_bottom, straight_rooted, Vec::new());
    };
    let straight_cost =
        straight_length + deferred_path_mm(straight_landing, straight_bottom, law.layer_height_mm());
    if !law.leans() {
        return (straight_landing, straight_bottom, straight_rooted, Vec::new());
    }

    // The box the two bounds above allow. `straight_cost` is the straight
    // descent's length, and no cheaper descent can travel further sideways
    // than that.
    let radius_mm = search_radius_mm.min(straight_cost);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a non-negative finite length divided by the grid pitch, floored to a loop bound"
    )]
    let radius_cells = (radius_mm / grid.pitch_mm()).floor().max(0.0) as usize;
    if radius_cells == 0 {
        return (straight_landing, straight_bottom, straight_rooted, Vec::new());
    }
    let cols = grid.cols();
    let seat_row = grid.row_of(cell);
    let seat_col = grid.col_of(cell);
    let row_lo = seat_row.saturating_sub(radius_cells);
    let row_hi = (seat_row + radius_cells).min(grid.rows() - 1);
    let col_lo = seat_col.saturating_sub(radius_cells);
    let col_hi = (seat_col + radius_cells).min(cols - 1);

    scratch.clear();
    for row in row_lo..=row_hi {
        let base = row * cols;
        for col in col_lo..=col_hi {
            let candidate = base + col;
            if candidate == cell {
                continue;
            }
            let strut_top = history.top_strut[candidate];
            let model_top = history.top_model[candidate];
            // Only a landing STRICTLY HIGHER than the straight descent's can be
            // cheaper: every leaned layer costs at least a vertical one, so
            // crossing the same number of layers can never win. Two array reads
            // reject the overwhelming majority of the box here, which is what
            // makes the scan affordable.
            let (landing, k_bottom, rooted, class) = if strut_top > 0 && strut_top >= model_top {
                let landing_layer = strut_top as usize - 1;
                (
                    Landing::Strut {
                        id: history.top_strut_id[candidate],
                        layer: landing_layer,
                    },
                    landing_layer,
                    true,
                    0_u8,
                )
            } else if model_top > 0 {
                let landing_layer = model_top as usize - 1;
                (
                    Landing::Model {
                        layer: landing_layer,
                    },
                    landing_layer,
                    history.top_model_rooted[candidate],
                    1_u8,
                )
            } else {
                (Landing::Plate, 0, true, 2_u8)
            };
            if k_bottom <= straight_bottom {
                continue;
            }
            let descent_layers = layer.saturating_sub(k_bottom);
            let lateral_mm = crate::cell_distance_mm(grid, cell, candidate);
            let Some(length_mm) = law.descent_cost_mm(descent_layers, lateral_mm) else {
                continue;
            };
            let cost_mm = length_mm + deferred_path_mm(landing, k_bottom, law.layer_height_mm());
            outcome.lean_candidates_priced += 1;
            if cost_mm >= straight_cost {
                continue;
            }
            scratch.push(LeanCandidate {
                cost_mm,
                class,
                cell: candidate,
                k_bottom,
                landing,
                rooted,
            });
        }
    }
    if scratch.is_empty() {
        return (straight_landing, straight_bottom, straight_rooted, Vec::new());
    }
    // Cheapest first; a tie goes to the merge, then to the model, then to the
    // plate, then to the lower cell index. Every step of that order is total,
    // so the winner does not depend on the scan order.
    scratch.sort_unstable_by(|a, b| {
        a.cost_mm
            .total_cmp(&b.cost_mm)
            .then_with(|| a.class.cmp(&b.class))
            .then_with(|| a.cell.cmp(&b.cell))
    });

    let mut blocked = false;
    for candidate in scratch.iter() {
        if let Some(path) = lean_path(grid, history, keep_out, layer, cell, candidate) {
            outcome.arms_leaned += 1;
            if candidate.class == 0 {
                outcome.arms_merged += 1;
            }
            return (candidate.landing, candidate.k_bottom, candidate.rooted, path);
        }
        blocked = true;
    }
    if blocked {
        outcome.lean_paths_blocked += 1;
    }
    (straight_landing, straight_bottom, straight_rooted, Vec::new())
}

/// The cells a leaning descent would occupy, or `None` if it cannot have them.
///
/// The path is the straight segment from the seat to the landing — the cheapest
/// shape for a given pair of endpoints, because the per-layer cost is convex in
/// the step (see [`LeanLaw::descent_cost_mm`]) — sampled once per layer. It is
/// rejected when any layer of it would stand inside model material, inside
/// another arm, or inside a drain's escape cylinder. The landing layer itself
/// is exempt from the occupancy test: striking one lamina into the surface is
/// what a landing IS (`docs/physics.md`:284-324).
fn lean_path(
    grid: &Grid,
    history: &ColumnHistory,
    keep_out: &DrainKeepOut,
    layer: usize,
    seat_cell: usize,
    candidate: &LeanCandidate,
) -> Option<Vec<u32>> {
    let cols = grid.cols();
    let span = layer - candidate.k_bottom;
    let seat_row = grid.row_of(seat_cell);
    let seat_col = grid.col_of(seat_cell);
    let base_row = grid.row_of(candidate.cell);
    let base_col = grid.col_of(candidate.cell);
    let mut path = Vec::with_capacity(span);
    #[expect(
        clippy::cast_precision_loss,
        reason = "layer and cell indices bounded by the plan's own extents are exact in f64"
    )]
    let span_f = span as f64;
    for slot in 0..span {
        let here = candidate.k_bottom + slot;
        #[expect(
            clippy::cast_precision_loss,
            reason = "a slot index bounded by the layer count is exact in f64"
        )]
        let fraction = slot as f64 / span_f;
        #[expect(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "an interpolation between two in-range grid indices, rounded back onto the \
                  grid and then clamped by construction"
        )]
        let row = ((base_row as f64) + ((seat_row as f64) - (base_row as f64)) * fraction)
            .round()
            .max(0.0) as usize;
        #[expect(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "an interpolation between two in-range grid indices, rounded back onto the \
                  grid and then clamped by construction"
        )]
        let col = ((base_col as f64) + ((seat_col as f64) - (base_col as f64)) * fraction)
            .round()
            .max(0.0) as usize;
        let row = row.min(grid.rows() - 1);
        let col = col.min(cols - 1);
        let here_cell = row * cols + col;
        if !keep_out.admits(here_cell, here) {
            return None;
        }
        if slot > 0 {
            // Nothing may already stand at this layer or above it in this
            // column: `top_model` and `top_strut` hold layer + 1 of the topmost
            // occupant, so `> here` is exactly "occupied at or above here".
            if history.top_model[here_cell] as usize > here {
                return None;
            }
            if history.top_strut[here_cell] as usize > here {
                return None;
            }
        }
        path.push(u32::try_from(here_cell).unwrap_or(u32::MAX));
    }
    Some(path)
}

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

#[cfg(test)]
mod descent_tests {
    use super::{descend_leaning, ColumnHistory, LayerOutcome};
    use crate::drain::DrainKeepOut;
    use crate::grid::plan_grid;
    use crate::lean::LeanLaw;
    use crate::Landing;

    const H: f64 = 0.05;

    fn outcome() -> LayerOutcome {
        LayerOutcome {
            demand_cells: 0,
            residual_demand: 0,
            unrooted_landings: 0,
            keep_out_refusals: 0,
            arms_leaned: 0,
            arms_merged: 0,
            lean_paths_blocked: 0,
            lean_candidates_priced: 0,
        }
    }

    /// A grid one working pitch across, the same 0.05 mm the corpus runs at.
    fn grid() -> crate::grid::Grid {
        plan_grid(0.0, 0.0, 2.0, 2.0, 400, H, 0.0, 1_000_000_000).0
    }

    /// THE MECHANISM, AND ITS SABOTAGE. A seat 200 layers above the plate with
    /// a neighbouring arm's top 10 layers below it and 0.4 mm to one side: the
    /// merge is 10 leaned layers instead of 200 vertical ones, so it wins by an
    /// order of magnitude. With `LeanLaw::disabled` — the same geometry, the
    /// same history, the same keep-out — the descent must fall back to the
    /// plate and the merge count must go to zero. A test that only asserted
    /// the merge would pass on a descent that merged unconditionally.
    #[test]
    fn a_seat_merges_onto_a_neighbouring_trunk_and_the_disabled_law_sends_it_to_the_plate() {
        let grid = grid();
        let mut history = ColumnHistory::new(&grid);
        let seat = grid.index(20, 20);
        // The neighbour: 8 cells away is 0.4 mm at the 0.05 mm pitch, and its
        // top is 10 layers down, so 0.04 mm of sideways move per layer against
        // the 0.0785 mm the law allows.
        let neighbour = grid.index(20, 28);
        history.top_strut[neighbour] = 191;
        history.top_strut_id[neighbour] = 7;

        let mut scratch = Vec::new();
        let mut counts = outcome();
        let law = LeanLaw::new(H, 45.0, 25.0);
        let (landing, k_bottom, _, path) = descend_leaning(
            &grid,
            &history,
            &DrainKeepOut::none(),
            &law,
            4.0,
            seat,
            200,
            &mut scratch,
            &mut counts,
        );
        assert_eq!(
            landing,
            Landing::Strut { id: 7, layer: 190 },
            "the descent should have merged onto the neighbouring trunk"
        );
        assert_eq!(k_bottom, 190);
        assert_eq!(counts.arms_merged, 1, "the merge must be counted");
        assert_eq!(counts.arms_leaned, 1);
        assert_eq!(path.len(), 10, "ten layers of descent, one cell each");
        assert_eq!(
            path.first().copied().map(|cell| cell as usize),
            Some(neighbour),
            "the path must end in the neighbour's own column"
        );
        assert_eq!(
            path.last().copied().map(|cell| cell as usize),
            Some(grid.index(20, 21)),
            "and start one step off the seat"
        );

        let mut sabotaged = outcome();
        let (landing, k_bottom, _, path) = descend_leaning(
            &grid,
            &history,
            &DrainKeepOut::none(),
            &LeanLaw::disabled(H),
            4.0,
            seat,
            200,
            &mut scratch,
            &mut sabotaged,
        );
        assert_eq!(
            landing,
            Landing::Plate,
            "with the lean disabled the descent must drop straight down to the plate"
        );
        assert_eq!(k_bottom, 0);
        assert!(path.is_empty(), "a straight arm carries no path");
        assert_eq!(
            sabotaged.arms_merged, 0,
            "SABOTAGE: disabling the lean must take the merge count to zero"
        );
        assert_eq!(sabotaged.arms_leaned, 0);
    }

    /// The same geometry with the neighbour's top too far to one side for the
    /// available layers. `0.8 mm` over 10 layers is `0.08 mm` per layer against
    /// the `0.0785 mm` the registered law admits, so the angle refuses it and
    /// the descent goes straight down. This is what makes the first test a
    /// statement about the ANGLE LAW rather than about the search radius.
    #[test]
    fn a_trunk_the_angle_law_cannot_reach_is_not_merged_onto() {
        let grid = grid();
        let mut history = ColumnHistory::new(&grid);
        let seat = grid.index(20, 20);
        let neighbour = grid.index(20, 37);
        history.top_strut[neighbour] = 191;
        history.top_strut_id[neighbour] = 7;

        let mut scratch = Vec::new();
        let mut counts = outcome();
        let (landing, _, _, path) = descend_leaning(
            &grid,
            &history,
            &DrainKeepOut::none(),
            &LeanLaw::new(H, 45.0, 25.0),
            4.0,
            seat,
            200,
            &mut scratch,
            &mut counts,
        );
        assert_eq!(landing, Landing::Plate);
        assert!(path.is_empty());
        assert_eq!(counts.arms_merged, 0);
    }

    /// A KEEP-OUT IN THE WAY. The merge of the first test, with a drain
    /// cylinder standing across the path: the lean is refused, the arm goes
    /// straight down, and `lean_paths_blocked` is the witness that the path
    /// check is what refused it rather than the price.
    #[test]
    fn a_lean_whose_path_crosses_a_keep_out_is_refused_and_counted() {
        let grid = grid();
        let mut history = ColumnHistory::new(&grid);
        let seat = grid.index(20, 20);
        let neighbour = grid.index(20, 28);
        history.top_strut[neighbour] = 191;
        history.top_strut_id[neighbour] = 7;
        let keep_out = DrainKeepOut::build(
            &grid,
            &[crate::drain::DrainSpec {
                outlet_mm: [grid.center_x_mm(24), grid.center_y_mm(20), 0.0],
                radius_mm: 0.15,
                outward_axis: [0.0, 0.0, -1.0],
            }],
            0.0,
            0.0,
            400,
            0.0,
            H,
        );

        let mut scratch = Vec::new();
        let mut counts = outcome();
        let (landing, _, _, path) = descend_leaning(
            &grid,
            &history,
            &keep_out,
            &LeanLaw::new(H, 45.0, 25.0),
            4.0,
            seat,
            200,
            &mut scratch,
            &mut counts,
        );
        assert_eq!(
            landing,
            Landing::Plate,
            "a lean whose path crosses the resin's way out must be refused"
        );
        assert!(path.is_empty());
        assert_eq!(counts.arms_merged, 0);
        assert_eq!(
            counts.lean_paths_blocked, 1,
            "the refusal must be counted as a blocked path, not silently priced away"
        );
    }

    /// A MODEL LANDING IS NEVER WORTH LEANING FOR. The neighbour column holds
    /// model rather than an arm, 10 layers down: priced on arm length alone
    /// this is the cheapest descent in sight and it is exactly the degenerate
    /// stub measured on 2026-09-04. With the deferred load path charged, it
    /// costs the same `layer * h` the straight descent does and the tie goes
    /// straight down.
    #[test]
    fn a_model_landing_a_lean_away_never_beats_the_straight_descent() {
        let grid = grid();
        let mut history = ColumnHistory::new(&grid);
        let seat = grid.index(20, 20);
        let neighbour = grid.index(20, 28);
        history.top_model[neighbour] = 191;
        history.top_model_rooted[neighbour] = true;

        let mut scratch = Vec::new();
        let mut counts = outcome();
        let (landing, k_bottom, _, path) = descend_leaning(
            &grid,
            &history,
            &DrainKeepOut::none(),
            &LeanLaw::new(H, 45.0, 25.0),
            4.0,
            seat,
            200,
            &mut scratch,
            &mut counts,
        );
        assert_eq!(landing, Landing::Plate);
        assert_eq!(k_bottom, 0);
        assert!(path.is_empty());
        assert_eq!(counts.arms_leaned, 0);
        assert!(
            counts.lean_candidates_priced > 0,
            "the candidate must have been PRICED and rejected, not skipped before pricing"
        );
    }
}
