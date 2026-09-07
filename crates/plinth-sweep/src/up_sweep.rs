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

/// One arm's SHAFT, where it occupies one column: the closed layer interval
/// `[k_bottom, k_top]` and the arm that owns it.
///
/// A straight arm contributes exactly one of these; a leaned arm contributes
/// one per run of layers it spends in the same column, which is a run-length
/// encoding of its own path and never one entry per layer.
///
/// `next` chains the spans that share a column into an intrusive singly-linked
/// list whose head is [`ColumnHistory::shaft_head`]; `u32::MAX` ends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShaftSpan {
    pub k_bottom: u32,
    pub k_top: u32,
    pub id: u32,
    pub next: u32,
}

/// Column history the descent reads. Every `top_*` entry names the HIGHEST
/// layer strictly below the current one at which that column held something,
/// which is what makes "the column is clear down to the landing" true by
/// construction rather than by a second scan.
///
/// # Why a column also knows the SHAFTS that cross it, and not only their tops
///
/// Until 2026-09-05 a column knew one thing about arms: the highest layer any
/// arm reached in it. A descent could therefore only ever land on an arm's
/// TOP, and the owner's ruling of that day is that "the base needs to land
/// ANYWHERE on the support arm". The two are not the same rule, and the
/// difference is not academic: an arm's top is USELESS to a neighbour seated
/// ABOVE it but too far away to reach it, because closing a lateral gap `d`
/// needs at least `d / max_step` layers of descent and the top is only a few
/// layers down. That neighbour used to give up and run 200 layers to the
/// plate, alongside a trunk it was touching the whole way. With the interval
/// known, it lands on that same trunk's SHAFT at whatever layer the angle law
/// can actually reach, and the two become one tree.
///
/// # The shape, and why this one
///
/// A per-layer occupancy raster was the alternative. It is `cells x layers`,
/// and on Round 100 that is 4.3 M cells over 170 layers — 2.9 GB for the ids
/// alone, before anything is placed. The spans are one entry per (arm, column
/// run): 878 entries on Round 100, because 878 of its arms are straight and a
/// straight arm crosses one column. The head vector is the only per-cell cost
/// and it is one `u32`, the same as `top_strut_id` beside it. Reading is one
/// `u32` load for the overwhelmingly common empty column, which is what keeps
/// the descent's cone scan the same cost it was.
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
    /// Head of each column's MODEL-span chain; `u32::MAX` means this column
    /// never held cured material at any layer.
    ///
    /// The same run-length shape [`Self::shaft_head`] uses, and for the same
    /// reason: a per-(cell, layer) occupancy bitmap is 91 MB on Round 100
    /// before anything is placed, while a contiguous cured run costs one entry
    /// however many layers it spans. It exists because `top_model` is a RUNNING
    /// state — by the end of the up-sweep it holds each column's topmost model
    /// layer OVERALL — so a pass that runs after the sweep (the slenderness
    /// lane) cannot use it to ask "is there model HERE, at this layer" and
    /// would refuse every path on the strength of material far above it.
    pub model_head: Vec<u32>,
    /// Every `(column, contiguous cured run)` interval, in one flat arena.
    /// `ShaftSpan::id` is unused here and is always zero.
    pub model_spans: Vec<ShaftSpan>,
    /// Head of each column's shaft-span chain; `u32::MAX` means no arm crosses
    /// this column at any layer.
    pub shaft_head: Vec<u32>,
    /// Every `(arm, column run)` occupancy interval, in one flat arena.
    pub shaft_spans: Vec<ShaftSpan>,
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
            shaft_head: vec![u32::MAX; cells],
            shaft_spans: Vec::new(),
            model_head: vec![u32::MAX; cells],
            model_spans: Vec::new(),
        }
    }

    /// Record where one finished arm's shaft stands, column by column.
    ///
    /// `path` is the arm's own [`crate::SweepStrut::path`] — empty for a
    /// straight arm, and otherwise the cell at each layer from `k_bottom`
    /// upward. The arm's own seat layer `k_top` stands in `seat_cell`, which is
    /// where `cell_at` puts it too, so the two agree by construction.
    ///
    /// Runs of equal cells are folded into ONE span, so a leaned arm costs one
    /// entry per column it crosses rather than one per layer.
    pub fn stamp_shaft(&mut self, id: u32, k_bottom: usize, k_top: usize, seat_cell: usize, path: &[u32]) {
        if k_top < k_bottom {
            return;
        }
        let mut run_cell = usize::MAX;
        let mut run_from = k_bottom;
        for layer in k_bottom..=k_top {
            let here = path
                .get(layer - k_bottom)
                .map_or(seat_cell, |cell| *cell as usize);
            if here == run_cell {
                continue;
            }
            if run_cell != usize::MAX {
                self.push_span(run_cell, id, run_from, layer - 1);
            }
            run_cell = here;
            run_from = layer;
        }
        if run_cell != usize::MAX {
            self.push_span(run_cell, id, run_from, k_top);
        }
    }

    fn push_span(&mut self, cell: usize, id: u32, k_bottom: usize, k_top: usize) {
        let Some(head) = self.shaft_head.get_mut(cell) else {
            return;
        };
        let next = *head;
        let slot = u32::try_from(self.shaft_spans.len()).unwrap_or(u32::MAX);
        if slot == u32::MAX {
            // The arena is full. EMIT AND WARN: an unrecorded span costs a
            // later descent a landing it could have had, and never falsifies
            // one it takes.
            return;
        }
        *head = slot;
        self.shaft_spans.push(ShaftSpan {
            k_bottom: u32::try_from(k_bottom).unwrap_or(u32::MAX),
            k_top: u32::try_from(k_top).unwrap_or(u32::MAX),
            id,
            next,
        });
    }

    /// The HIGHEST layer at or below `ceiling` at which some arm's shaft stands
    /// in this column, with the arm that owns it.
    ///
    /// This is the whole of the shaft-landing rule. `ceiling` is the highest
    /// layer the descending arm may land at — its own seat layer minus the
    /// layers the angle law needs to close the lateral gap — so the answer is
    /// the cheapest landing on this column, because a descent that crosses more
    /// layers for the same lateral move always costs more
    /// ([`crate::lean::LeanLaw::descent_cost_mm`] is increasing in the layer
    /// count).
    /// Returns the arm's id, the landing layer, and that span's own top, so the
    /// caller can say whether the junction is at the parent's top or inside its
    /// shaft without a second lookup.
    #[must_use]
    pub fn shaft_landing(&self, cell: usize, ceiling: usize) -> Option<(u32, usize, usize)> {
        let mut slot = *self.shaft_head.get(cell)?;
        let mut best: Option<(u32, usize, usize)> = None;
        while slot != u32::MAX {
            let span = self.shaft_spans[slot as usize];
            slot = span.next;
            let bottom = span.k_bottom as usize;
            if bottom > ceiling {
                continue;
            }
            let top = span.k_top as usize;
            let here = top.min(ceiling);
            if best.is_none_or(|(_, layer, _)| here > layer) {
                best = Some((span.id, here, top));
            }
        }
        best
    }

    /// Whether CURED MODEL stands in this column at exactly this layer.
    ///
    /// The sharp question [`Self::shaft_blocks`] asks about arms, asked about
    /// the model. `top_model` answers a different one — "is there model at or
    /// above here" — which is the right question DURING the up-sweep, where
    /// the history has only been told about the layers already passed, and the
    /// wrong one afterwards, where every column under the model answers yes.
    #[must_use]
    pub fn model_blocks(&self, cell: usize, layer: usize) -> bool {
        let Some(&head) = self.model_head.get(cell) else {
            return false;
        };
        let mut slot = head;
        while slot != u32::MAX {
            let span = self.model_spans[slot as usize];
            slot = span.next;
            if (span.k_bottom as usize) <= layer && layer <= span.k_top as usize {
                return true;
            }
        }
        false
    }

    /// Record one layer's cured cells as model spans, extending the run each
    /// column is already in rather than opening a new one.
    ///
    /// Called from [`commit_layer_history`] BEFORE `top_model` is restamped,
    /// because "was this column cured one layer up" is exactly what the old
    /// stamp still says.
    fn stamp_model(&mut self, layer: usize, cover: &[bool]) {
        let k = u32::try_from(layer).unwrap_or(u32::MAX);
        for cell in 0..cover.len() {
            if !cover[cell] {
                continue;
            }
            let head = self.model_head[cell];
            // `top_model` holds layer + 1 of the topmost cured cell below, so
            // it reads exactly `layer` when the layer immediately under this
            // one was cured and this run continues.
            if head != u32::MAX && self.top_model[cell] == k {
                self.model_spans[head as usize].k_top = k;
                continue;
            }
            let slot = u32::try_from(self.model_spans.len()).unwrap_or(u32::MAX);
            if slot == u32::MAX {
                // The arena is full. EMIT AND WARN: an unrecorded run makes a
                // later path check MORE permissive, never less, and the run
                // report's own residual counters are what would show it.
                continue;
            }
            self.model_spans.push(ShaftSpan {
                k_bottom: k,
                k_top: k,
                id: 0,
                next: head,
            });
            self.model_head[cell] = slot;
        }
    }

    /// Whether an arm OTHER than `except` stands in this column at this layer.
    ///
    /// The path check asks this rather than "does any arm top out above here",
    /// which is what `top_strut` answers. The two agree exactly for a straight
    /// arm rooted at the plate; they differ for an arm that starts mid-air,
    /// where the top-based test blocks the empty layers BELOW it as well.
    ///
    /// `except` is the arm being landed on. A member may pass through the
    /// member it merges into — that overlap is the junction, not a collision —
    /// and refusing it would reject every shallow approach to a trunk, since
    /// the last layers of a shallow lean necessarily lie in the trunk's own
    /// column.
    #[must_use]
    pub fn shaft_blocks(&self, cell: usize, layer: usize, except: Option<u32>) -> bool {
        let Some(&head) = self.shaft_head.get(cell) else {
            return false;
        };
        let mut slot = head;
        while slot != u32::MAX {
            let span = self.shaft_spans[slot as usize];
            slot = span.next;
            if Some(span.id) == except {
                continue;
            }
            if (span.k_bottom as usize) <= layer && layer <= span.k_top as usize {
                return true;
            }
        }
        false
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
    /// Merges whose junction is STRICTLY INSIDE the parent's shaft, above its
    /// base and below its seat. The parent is split in effect: its radius
    /// profile grows from the junction downward and is untouched above it.
    /// The path-hit witness that shaft landings fire at all.
    pub junctions_mid_shaft: usize,
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
    shaft_landings: bool,
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
        junctions_mid_shaft: 0,
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
            shaft_landings,
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
            kind: crate::StrutKind::Arm,
            min_radius_mm: 0.0,
            seat_on_strut: None,
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
            fallback_path: Vec::new(),
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
        // And the SHAFT, layer interval by layer interval, so a later descent
        // can land anywhere on it and not only at its top. Nothing is copied:
        // the arm's own path is read in place.
        let placed = struts.last().expect("the arm just pushed");
        history.stamp_shaft(id, k_bottom, layer, cell, &placed.path);

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
    /// For a merge, whether the junction is at the parent shaft's own top.
    /// `false` means the junction is inside the shaft.
    at_parent_top: bool,
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
    shaft_landings: bool,
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
            let lateral_mm = crate::cell_distance_mm(grid, cell, candidate);
            // THE ANGLE LAW DECIDES HOW FAR DOWN THIS COLUMN THE LANDING MUST
            // BE, AND THEN THE COLUMN IS ASKED WHAT STANDS THERE. That order is
            // the whole of the shaft-landing rule. Closing `lateral_mm` needs at
            // least `lateral_mm / max_step` layers of descent, so no landing
            // above `layer - drop` is reachable however inviting it looks; and
            // among the layers that ARE reachable the HIGHEST is the cheapest,
            // because the descent cost is increasing in the layer count at a
            // fixed lateral move.
            let drop = law.layers_to_reach(lateral_mm);
            let ceiling = match layer.checked_sub(drop) {
                Some(ceiling) if drop > 0 => ceiling,
                _ => continue,
            };
            let model_top = history.top_model[candidate];
            let reachable_model =
                (model_top > 0 && model_top as usize - 1 <= ceiling).then(|| model_top as usize - 1);
            let (shaft, model_layer) = if shaft_landings {
                (history.shaft_landing(candidate, ceiling), reachable_model)
            } else {
                // The knob restores the top-only rule this engine had between
                // 2026-09-04 and the owner's 2026-09-05 ruling, EXACTLY: a
                // column offered its single topmost occupant and nothing else,
                // so an arm's top was invisible whenever model material stood
                // ANYWHERE higher in the same column — even model far out of
                // the angle law's reach, and even with the arm's own shaft
                // squarely within it. That blindness is part of what this lane
                // removes; reproducing it here is what makes the before/after
                // comparison a comparison.
                let strut_top = history.top_strut[candidate];
                if strut_top > 0 && strut_top >= model_top {
                    let top = strut_top as usize - 1;
                    (
                        (top <= ceiling).then(|| (history.top_strut_id[candidate], top, top)),
                        None,
                    )
                } else {
                    (None, reachable_model)
                }
            };
            // Two array reads still reject the overwhelming majority of the
            // box: an empty column has no shaft head and no model top.
            // An arm wins over model material at the same or a higher layer,
            // which is the tie the top-only rule broke the same way
            // (`top_strut >= top_model`).
            let take_shaft = match (shaft, model_layer) {
                (Some((_, at, _)), Some(model_at)) => at >= model_at,
                (Some(_), None) => true,
                (None, _) => false,
            };
            let (landing, k_bottom, rooted, class, at_parent_top) =
                if let (true, Some((id, at, span_top))) = (take_shaft, shaft) {
                    (
                        Landing::Strut { id, layer: at },
                        at,
                        true,
                        0_u8,
                        at == span_top,
                    )
                } else if let Some(at) = model_layer {
                    (
                        Landing::Model { layer: at },
                        at,
                        history.top_model_rooted[candidate],
                        1_u8,
                        false,
                    )
                } else {
                    if history.shaft_head[candidate] != u32::MAX || model_top > 0 {
                        // Occupied, but not at any layer this descent can
                        // reach. The plate under it is NOT a candidate: the
                        // path would have to pass through what stands there.
                        continue;
                    }
                    (Landing::Plate, 0, true, 2_u8, false)
                };
            // A sound lower bound on any descent to this landing — every layer
            // costs at least a vertical one — so a candidate that cannot beat
            // the straight descent even at that bound is dropped before it is
            // priced. This replaces the old "must land HIGHER than the straight
            // descent" test, which stopped being sound the moment a landing
            // could be chosen part-way down a shaft rather than at its top.
            let descent_layers = layer.saturating_sub(k_bottom);
            #[expect(
                clippy::cast_precision_loss,
                reason = "a layer count bounded by the plan's own layer count is exact in f64"
            )]
            let bare_mm = descent_layers as f64 * law.layer_height_mm();
            if bare_mm >= straight_cost {
                continue;
            }
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
                at_parent_top,
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
                if !candidate.at_parent_top {
                    outcome.junctions_mid_shaft += 1;
                }
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
    // The member this descent merges into, if any. Its own material is not an
    // obstacle to the member joining it: the last layers of a shallow approach
    // necessarily lie inside the trunk's own column, and that overlap IS the
    // junction. Every OTHER arm still blocks, which is what keeps this a check
    // rather than a formality.
    let parent = match candidate.landing {
        Landing::Strut { id, .. } => Some(id),
        Landing::Plate | Landing::Model { .. } => None,
    };
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
            // No model may already stand at this layer or above it in this
            // column: `top_model` holds layer + 1 of the topmost model cell, so
            // `> here` is exactly "cured at or above here".
            if history.top_model[here_cell] as usize > here {
                return None;
            }
            // Arms are asked the sharper question — "is an arm ACTUALLY here,
            // at this layer" — because a landing part-way down a shaft leaves
            // the trunk standing far above the path, and the top-based test
            // would refuse every such approach on the strength of material that
            // is nowhere near it.
            if history.shaft_blocks(here_cell, here, parent) {
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
    history.stamp_model(layer, cover);
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
    use super::{descend_leaning, lean_path, ColumnHistory, LayerOutcome, LeanCandidate};
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
            junctions_mid_shaft: 0,
            lean_candidates_priced: 0,
        }
    }

    /// A straight arm standing in one column from `k_bottom` to `k_top`,
    /// recorded the way `place_layer` records one: its TOP for the old
    /// top-only rule, and its whole SHAFT for the new one. Writing both is
    /// what lets one fixture be read by both rules.
    fn straight_arm(
        history: &mut ColumnHistory,
        id: u32,
        cell: usize,
        k_bottom: usize,
        k_top: usize,
    ) {
        history.top_strut[cell] = u32::try_from(k_top + 1).expect("a small layer index");
        history.top_strut_id[cell] = id;
        history.stamp_shaft(id, k_bottom, k_top, cell, &[]);
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
        straight_arm(&mut history, 7, neighbour, 0, 190);

        let mut scratch = Vec::new();
        let mut counts = outcome();
        let law = LeanLaw::new(H, 45.0, 25.0);
        let (landing, k_bottom, _, path) = descend_leaning(
            &grid,
            &history,
            &DrainKeepOut::none(),
            &law,
            true,
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
            true,
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

    /// The same geometry with the neighbour's whole SHAFT too far to one side
    /// for the layers available. `0.85 mm` needs 11 layers at the `0.0785 mm`
    /// the registered law admits, and this trunk stands only at layer 190 — it
    /// has no shaft below its own top to catch a descent lower down. So the
    /// angle refuses it and the arm goes straight down. This is what makes the
    /// merge tests statements about the ANGLE LAW rather than about the search
    /// radius: a trunk 0.85 mm away IS merged onto when its shaft runs down
    /// (`a_seat_lands_mid_shaft_...` below), and is refused when it does not.
    #[test]
    fn a_trunk_the_angle_law_cannot_reach_is_not_merged_onto() {
        let grid = grid();
        let mut history = ColumnHistory::new(&grid);
        let seat = grid.index(20, 20);
        let neighbour = grid.index(20, 37);
        straight_arm(&mut history, 7, neighbour, 190, 190);

        let mut scratch = Vec::new();
        let mut counts = outcome();
        let (landing, _, _, path) = descend_leaning(
            &grid,
            &history,
            &DrainKeepOut::none(),
            &LeanLaw::new(H, 45.0, 25.0),
            true,
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
        straight_arm(&mut history, 7, neighbour, 0, 190);
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
            true,
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
            true,
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

    /// **THE 2026-09-05 MECHANISM, AND ITS SABOTAGE.** A seat 200 layers up
    /// with a neighbouring trunk 0.4 mm to one side whose TOP is one single
    /// layer below the seat. Under the top-only rule that trunk is useless:
    /// closing 0.4 mm in one layer needs 0.4 mm of sideways move against the
    /// 0.0785 mm the law admits, so the arm gave up and ran all 200 layers to
    /// the plate — beside a trunk it was touching the whole way. The trunk's
    /// SHAFT runs from the plate to that top, so the same law reaches it six
    /// layers down, and the junction is STRICTLY INSIDE the parent.
    ///
    /// The sabotage is the knob, not the geometry: `shaft_landings = false`
    /// restores the top-only rule with the same grid, the same history, the
    /// same law and the same keep-out, and the same arm must go to the plate.
    #[test]
    fn a_seat_lands_mid_shaft_on_a_trunk_whose_top_it_cannot_reach() {
        let grid = grid();
        let mut history = ColumnHistory::new(&grid);
        let seat = grid.index(20, 20);
        let neighbour = grid.index(20, 28);
        // 8 cells is 0.4 mm at the 0.05 mm pitch; the top is at layer 199, one
        // layer below the seat, and the shaft runs all the way to the plate.
        straight_arm(&mut history, 7, neighbour, 0, 199);

        let mut scratch = Vec::new();
        let mut counts = outcome();
        let law = LeanLaw::new(H, 45.0, 25.0);
        let (landing, k_bottom, _, path) = descend_leaning(
            &grid,
            &history,
            &DrainKeepOut::none(),
            &law,
            true,
            4.0,
            seat,
            200,
            &mut scratch,
            &mut counts,
        );
        // `ceil(0.4 / 0.0785) = 6`, so the highest layer the law can reach is
        // 200 - 6 = 194, and the shaft is there.
        let Landing::Strut { id, layer: junction } = landing else {
            panic!("the descent must have merged, and it landed on {landing:?}")
        };
        assert_eq!(id, 7);
        assert_eq!(junction, 194);
        assert_eq!(k_bottom, junction);
        // The parent's own extent, read back rather than restated: the junction
        // must be STRICTLY between its base and its top.
        let (parent_bottom, parent_top) = (0_usize, 199_usize);
        assert!(
            parent_bottom < junction && junction < parent_top,
            "the junction {junction} must be strictly inside [{parent_bottom}, {parent_top}]"
        );
        assert_eq!(counts.junctions_mid_shaft, 1, "and be counted as mid-shaft");
        assert_eq!(counts.arms_merged, 1);
        assert_eq!(path.len(), 6, "six layers of descent");
        assert_eq!(
            path.first().copied().map(|cell| cell as usize),
            Some(neighbour),
            "the path must end in the parent's own column"
        );

        let mut sabotaged = outcome();
        let (landing, k_bottom, _, path) = descend_leaning(
            &grid,
            &history,
            &DrainKeepOut::none(),
            &law,
            false,
            4.0,
            seat,
            200,
            &mut scratch,
            &mut sabotaged,
        );
        assert_eq!(
            landing,
            Landing::Plate,
            "SABOTAGE: with shaft landings off the same arm must run to the plate"
        );
        assert_eq!(k_bottom, 0);
        assert!(path.is_empty());
        assert_eq!(sabotaged.arms_merged, 0);
        assert_eq!(sabotaged.junctions_mid_shaft, 0);
    }

    /// A LANDING ON A TRUNK'S TOP IS STILL A TOP LANDING. The first test's
    /// geometry — the trunk's top 10 layers below the seat and well within
    /// reach — must still land THERE and be counted as a top junction, not
    /// pushed down the shaft. The rule is "the highest reachable layer", and
    /// this is what says so.
    #[test]
    fn a_reachable_top_is_still_preferred_to_anywhere_lower_on_the_same_shaft() {
        let grid = grid();
        let mut history = ColumnHistory::new(&grid);
        let seat = grid.index(20, 20);
        let neighbour = grid.index(20, 28);
        straight_arm(&mut history, 7, neighbour, 0, 190);

        let mut scratch = Vec::new();
        let mut counts = outcome();
        let (landing, _, _, _) = descend_leaning(
            &grid,
            &history,
            &DrainKeepOut::none(),
            &LeanLaw::new(H, 45.0, 25.0),
            true,
            4.0,
            seat,
            200,
            &mut scratch,
            &mut counts,
        );
        assert_eq!(landing, Landing::Strut { id: 7, layer: 190 });
        assert_eq!(
            counts.junctions_mid_shaft, 0,
            "a landing at the parent's own top is not a mid-shaft junction"
        );
    }

    /// **THE PATH CHECK BITES ON A THIRD ARM.** The six-layer approach of the
    /// test above, handed to [`lean_path`] directly, with and without a third
    /// arm's shaft standing in the corridor it has to cross.
    ///
    /// It is asked at this level ON PURPOSE. In a whole descent a third arm
    /// sitting in the corridor is almost never a blocker, because it is a
    /// BETTER merge target: a minimum-drop path traces the reach ceiling
    /// exactly, so any shaft it touches is landable at the very layer it
    /// touches it, and the search takes the nearer member instead. Asserting
    /// the refusal through the search would therefore be asserting the
    /// ordering, not the check; asserting it here asserts the check.
    #[test]
    fn a_lean_whose_path_crosses_a_third_shaft_is_refused() {
        let grid = grid();
        let mut clear = ColumnHistory::new(&grid);
        let seat = grid.index(20, 20);
        let neighbour = grid.index(20, 28);
        straight_arm(&mut clear, 7, neighbour, 0, 199);
        let candidate = LeanCandidate {
            cost_mm: 0.0,
            class: 0,
            cell: neighbour,
            k_bottom: 194,
            landing: Landing::Strut { id: 7, layer: 194 },
            rooted: true,
            at_parent_top: false,
        };
        let path = lean_path(&grid, &clear, &DrainKeepOut::none(), 200, seat, &candidate)
            .expect("a clear corridor admits the approach");
        assert_eq!(path.len(), 6);
        assert_eq!(
            path.first().copied().map(|cell| cell as usize),
            Some(neighbour),
            "and the parent's own column is not an obstacle to the member joining it"
        );

        let mut blocked = ColumnHistory::new(&grid);
        straight_arm(&mut blocked, 7, neighbour, 0, 199);
        for column in 21..=27 {
            straight_arm(&mut blocked, 9, grid.index(20, column), 0, 210);
        }
        assert!(
            lean_path(&grid, &blocked, &DrainKeepOut::none(), 200, seat, &candidate).is_none(),
            "a path that would tunnel through a THIRD arm must be refused"
        );
    }

    /// **A COLUMN THAT HOLDS TWO ARMS OFFERS THE ONE THE LAW CAN REACH.**
    /// `top_strut` is one `u32` per column, so a leaned arm crossing high above
    /// a straight trunk hid that trunk completely: the column answered "an arm
    /// tops out at 199", the law could not reach 199, and the trunk running
    /// from the plate to layer 100 underneath it was never offered. The span
    /// index answers per interval, so it is.
    #[test]
    fn a_column_with_two_arms_in_it_offers_the_one_the_law_can_reach() {
        let grid = grid();
        let mut history = ColumnHistory::new(&grid);
        let seat = grid.index(20, 20);
        let neighbour = grid.index(20, 28);
        straight_arm(&mut history, 7, neighbour, 0, 100);
        straight_arm(&mut history, 8, neighbour, 199, 199);

        let mut scratch = Vec::new();
        let mut counts = outcome();
        let law = LeanLaw::new(H, 45.0, 25.0);
        let (landing, _, _, _) = descend_leaning(
            &grid,
            &history,
            &DrainKeepOut::none(),
            &law,
            true,
            4.0,
            seat,
            200,
            &mut scratch,
            &mut counts,
        );
        assert_eq!(
            landing,
            Landing::Strut { id: 7, layer: 100 },
            "the reachable arm is the lower one, and the column must offer it"
        );
        assert_eq!(counts.arms_merged, 1);

        let mut sabotaged = outcome();
        let (landing, _, _, _) = descend_leaning(
            &grid,
            &history,
            &DrainKeepOut::none(),
            &law,
            false,
            4.0,
            seat,
            200,
            &mut scratch,
            &mut sabotaged,
        );
        assert_eq!(
            landing,
            Landing::Plate,
            "SABOTAGE: the top-only rule sees only the arm at 199 and cannot reach it"
        );
        assert_eq!(sabotaged.arms_merged, 0);
    }

    /// The shaft index is a RUN-LENGTH encoding, and its cost is the claim the
    /// data-structure choice rests on: one entry per column a member crosses,
    /// never one per layer.
    #[test]
    fn the_shaft_index_costs_one_span_per_column_not_one_per_layer() {
        let grid = grid();
        let mut history = ColumnHistory::new(&grid);
        history.stamp_shaft(0, 0, 200, grid.index(20, 20), &[]);
        assert_eq!(
            history.shaft_spans.len(),
            1,
            "a 201-layer straight arm is ONE span"
        );
        let path: Vec<u32> = (0..4)
            .flat_map(|step| {
                core::iter::repeat_n(
                    u32::try_from(grid.index(20, 30 + step)).expect("a small cell index"),
                    3,
                )
            })
            .collect();
        history.stamp_shaft(1, 0, 12, grid.index(20, 34), &path);
        assert_eq!(
            history.shaft_spans.len(),
            1 + 5,
            "a 13-layer arm crossing four columns and seating in a fifth is FIVE spans"
        );
        assert!(history.shaft_blocks(grid.index(20, 31), 4, None));
        assert!(!history.shaft_blocks(grid.index(20, 31), 9, None));
        assert!(
            !history.shaft_blocks(grid.index(20, 31), 4, Some(1)),
            "the member being landed on is exempt"
        );
    }
}
