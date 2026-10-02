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

    /// Take back the spans a [`Self::stamp_shaft`] of a STRAIGHT shaft at
    /// `cell` added, given the span count from just before that stamp.
    ///
    /// A straight shaft occupies one column, so its stamp pushed at most one
    /// span, onto `cell`'s list, and that span is the list's head. Popping it
    /// and restoring the head leaves the index exactly as it was before the
    /// stamp: the same spans, the same slots, the same list order. Used by the
    /// slenderness lane's mast search, which stamps a trial mast into the live
    /// index instead of rebuilding a whole index per attempt.
    pub(crate) fn unstamp_straight_shaft(&mut self, cell: usize, spans_before: usize) {
        while self.shaft_spans.len() > spans_before {
            let slot = self.shaft_spans.len() - 1;
            let Some(span) = self.shaft_spans.pop() else {
                return;
            };
            debug_assert_eq!(
                self.shaft_head.get(cell).copied(),
                u32::try_from(slot).ok(),
                "the span being taken back must head the straight shaft's own column"
            );
            if let Some(head) = self.shaft_head.get_mut(cell) {
                *head = span.next;
            }
        }
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
        self.shaft_landing_excluding(cell, ceiling, &[])
    }

    /// [`Self::shaft_landing`] ignoring the shafts of `except` (a member being
    /// RE-landed, whose own old shaft the index still holds, and the members
    /// that move with it: `lip_seat.rs`, 2026-09-29).
    #[must_use]
    pub fn shaft_landing_excluding(
        &self,
        cell: usize,
        ceiling: usize,
        except: &[u32],
    ) -> Option<(u32, usize, usize)> {
        let mut slot = *self.shaft_head.get(cell)?;
        let mut best: Option<(u32, usize, usize)> = None;
        while slot != u32::MAX {
            let span = self.shaft_spans[slot as usize];
            slot = span.next;
            if except.contains(&span.id) {
                continue;
            }
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

    /// The HIGHEST layer at or below `ceiling` that is the TOP of a cured run
    /// in this column: the strike face a descent landing on the model meets
    /// there, as [`descend`] lands on `top_model - 1`. `None` when no cured
    /// run in this column tops out at or below `ceiling`.
    ///
    /// The model counterpart of [`Self::shaft_landing_excluding`], read from
    /// the per-column model spans because `top_model` is a running state
    /// that, after the up-sweep, names the column's topmost cured layer
    /// overall (fix/morel-long-posts, 2026-10-02: the lip-seat rule's
    /// re-landing search asked only the shaft index, so a column whose
    /// landing was model material was offered as a PLATE landing).
    #[must_use]
    pub fn model_landing_at_or_below(&self, cell: usize, ceiling: usize) -> Option<usize> {
        let mut slot = *self.model_head.get(cell)?;
        let mut best: Option<usize> = None;
        while slot != u32::MAX {
            let span = self.model_spans[slot as usize];
            slot = span.next;
            let top = span.k_top as usize;
            if top <= ceiling && best.is_none_or(|layer| top > layer) {
                best = Some(top);
            }
        }
        best
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
        self.shaft_blocks_excluding(cell, layer, except.as_slice())
    }

    /// [`Self::shaft_blocks`] with several members excepted (a re-landed
    /// member's own old shaft, and the members that move with it).
    #[must_use]
    pub fn shaft_blocks_excluding(&self, cell: usize, layer: usize, except: &[u32]) -> bool {
        let Some(&head) = self.shaft_head.get(cell) else {
            return false;
        };
        let mut slot = head;
        while slot != u32::MAX {
            let span = self.shaft_spans[slot as usize];
            slot = span.next;
            if except.contains(&span.id) {
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
    /// The candidate disc's radius in cells, unrounded.
    radius_cells: f64,
    reach_mm: f64,
    queue: Vec<usize>,
    touched: Vec<usize>,
    /// Reused across every descent so the lean search allocates once per run
    /// rather than once per arm.
    lean_candidates: Vec<super::up_sweep::LeanCandidate>,
    /// Whether each island is seated by the cheapest measured covering
    /// ([`choose_plan`]). `false` is the sabotage form: the global lattice and
    /// the greedy only, over the whole layer at once, as before 2026-09-30
    /// ([`crate::SweepPolicy::without_island_seating`]).
    pub island_seating: bool,
    /// The hex lattice's nearest-neighbour separation, mm
    /// ([`hex_separation_mm`]; the chamfer's in [`GreedyScratch::legacy`]).
    pub separation_mm: f64,
    /// Per cell, the island of demand it belongs to on the current layer;
    /// `u32::MAX` elsewhere. Reset over the island cells after every layer.
    island_of: Vec<u32>,
    /// Demand cells a TRIAL covering cleared, so the trial can be undone.
    cleared: Vec<usize>,
    /// Per cell of the island being seated, its depth inside the island's
    /// edge in mm ([`island_depth`]); zero elsewhere. Read by the ridge band.
    depth: Vec<f32>,
    /// Path-hit witnesses of the island seating: islands that were measured
    /// with more than one covering, and islands where a covering other than
    /// the global lattice won and was seated.
    pub islands_compared: u64,
    pub islands_realigned: u64,
    /// Seats the chosen coverings saved against the global lattice's own
    /// count on the same islands, measured in the same trial.
    pub seats_saved: u64,
    /// Seats placed on islands, and the islands' hex covering lower bound
    /// (see [`crate::report::PathHits::island_hex_bound`]).
    pub island_seats: u64,
    pub island_hex_bound: u64,
}

impl GreedyScratch {
    #[must_use]
    pub fn new(grid: &Grid, reach_mm: f64) -> Self {
        // The candidate disc is the reach disc less one raster pitch, in the
        // same straight-line metric the potential uses. In open cover a root
        // reaches the full `κ·h`; but where the island's edge is a raster
        // staircase the sightline to an edge cell can cross the stair, and the
        // path then bends at the last cell and pays up to one step more
        // (`reach::any_angle_relax`). A seat chosen at the very edge of the
        // disc then fails to root the pivot it was chosen for, and the pivot
        // takes a second seat: measured on a 2 mm ring, 61 seats against the
        // 42 that keeping one pitch inside the reach takes. Geometry of the
        // raster, not a tuned constant.
        let radius = reach_mm / grid.pitch_mm() - 1.0;
        Self::with_radius(grid, reach_mm, radius, true)
    }

    /// The scratch this engine's greedy used before 2026-09-30: the global
    /// lattice and one raster-order pivot pass over the whole layer, the
    /// candidate disc rounded to whole cells, and the chamfer's lattice
    /// separation ([`chamfer_hex_separation_mm`]). **Sabotage and comparison
    /// only** ([`crate::SweepPolicy::without_island_seating`] together with
    /// [`crate::SweepPolicy::without_straight_reach`] is the pre-lane seat
    /// placement).
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a rounded, positive reach in cells (~40) is exact as a usize"
    )]
    pub fn legacy(grid: &Grid, reach_mm: f64) -> Self {
        let rounded = (reach_mm / grid.pitch_mm()).round().max(1.0);
        let mut scratch = Self::with_radius(grid, reach_mm, rounded, false);
        scratch.reach_cells = rounded as usize;
        scratch.separation_mm = chamfer_hex_separation_mm(reach_mm);
        scratch
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "the reach radius in cells is a small positive finite quotient of two registered \
              lengths; it is truncated and then used only as a loop bound"
    )]
    fn with_radius(grid: &Grid, reach_mm: f64, radius: f64, island_seating: bool) -> Self {
        let reach_cells = radius.floor().max(1.0) as usize;
        let mut half_widths = Vec::with_capacity(reach_cells + 1);
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
            radius_cells: radius,
            reach_mm,
            queue: Vec::new(),
            touched: Vec::new(),
            lean_candidates: Vec::new(),
            island_seating,
            separation_mm: hex_separation_mm(reach_mm, grid.pitch_mm()),
            island_of: vec![u32::MAX; grid.cells()],
            cleared: Vec::new(),
            depth: vec![0.0; grid.cells()],
            islands_compared: 0,
            islands_realigned: 0,
            seats_saved: 0,
            island_seats: 0,
            island_hex_bound: 0,
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
    // less the half-diagonal a lattice point moves when it is snapped to a
    // cell ([`hex_separation_mm`]), and a lattice point is used only where it
    // is still uncovered demand.
    //
    // # Each island is seated by the cheapest covering MEASURED on it (2026-09-30)
    //
    // One global lattice knows nothing of where an island's boundary is. On a
    // ring or a strip it leaves lattice points wherever the global grid
    // happens to fall — some a cell inside the edge, covering a sliver — and
    // the greedy then spends one arm per gap between them: 2.8-3.5x the hex
    // covering lower bound on the round-base rings, the laid tube's strips and
    // the Cylinder's islands, and 44 small Morel islands took 85 fill arms
    // rooting 0.3-1.8 mm2 each against 6-10 for a lattice arm
    // (`oversupport-attribution.md` items 3 and 7).
    //
    // THE RULE: each island's demand is covered, completely, by the covering
    // with the FEWEST seats among several that are each built to the island
    // and each MEASURED on it with the real reach field — never estimated.
    // The candidates are the global lattice (so no island can be seated worse
    // than before on its own layer), and then by the island's shape: on an
    // island deeper than one reach radius, lattices ALIGNED to it (rows
    // parallel to its extent, the first row half a reach inside its edge,
    // [`aligned_lattice_cells`]); on a thinner one (a strip, a ring, a small
    // island) the greedy alone and the RIDGE covering, which seats one row
    // down the island's midline, each seat as far along as still covers the
    // whole cross-section at the first uncovered cell ([`best_seat_for`]).
    // Every candidate ends
    // with the same pivot pass, so every candidate covers every coverable
    // cell; the choice changes how many arms, never whether the demand is
    // met. The trials run on the live field under a journal and are undone
    // bit for bit ([`ReachState::begin_journal`]); the winner is then seated
    // for real, seat for seat as measured.
    let separation_mm = scratch.separation_mm;
    let lattice = hex_lattice_cells(grid, cover, separation_mm);
    let seating = Seating {
        grid,
        layer,
        cover,
        keep_out,
        law,
        shaft_landings,
        kappa,
        separation_mm,
        max_struts,
    };
    let islands = label_islands(grid, scratch);
    // One seat's share of the optimal covering at the MEASURED reach, for the
    // efficiency the run reports (not for any decision).
    let hex_cell_mm2 = 1.5 * 3.0_f64.sqrt() * scratch.reach_mm * scratch.reach_mm;
    let mut lattice_by_island: Vec<Vec<usize>> = vec![Vec::new(); islands.len()];
    for &cell in &lattice {
        let id = scratch.island_of[cell];
        if id != u32::MAX {
            lattice_by_island[id as usize].push(cell);
        }
    }
    if !scratch.island_seating {
        // THE SABOTAGE FORM, and the placement before 2026-09-30: every
        // lattice point of the layer first, then one raster-order pivot pass
        // over all of its demand, as if the layer were one island.
        for island in &islands {
            scratch.island_hex_bound += hex_bound(island.len(), grid.cell_area_mm2(), hex_cell_mm2);
        }
        let mut everything: Vec<usize> = islands.iter().flatten().copied().collect();
        everything.sort_unstable();
        let mut placer = Placer::Commit {
            struts: &mut *struts,
            outcome: &mut outcome,
        };
        let (seats, _) = cover_island(
            &seating,
            &lattice,
            None,
            &everything,
            reach,
            history,
            scratch,
            &mut placer,
        );
        scratch.island_seats += seats.len() as u64;
    }
    for (id, island) in islands.iter().enumerate() {
        if !scratch.island_seating || struts.len() >= max_struts {
            break;
        }
        let open = island.iter().filter(|cell| scratch.demand_bits[**cell]).count();
        if open == 0 {
            continue;
        }
        scratch.island_hex_bound += hex_bound(open, grid.cell_area_mm2(), hex_cell_mm2);
        let plan = choose_plan(
            &seating,
            u32::try_from(id).unwrap_or(u32::MAX),
            island,
            &lattice_by_island[id],
            reach,
            history,
            scratch,
        );
        let mut placer = Placer::Commit {
            struts: &mut *struts,
            outcome: &mut outcome,
        };
        let (seats, _) =
            cover_island(&seating, &plan, None, island, reach, history, scratch, &mut placer);
        scratch.island_seats += seats.len() as u64;
    }
    for island in &islands {
        for &cell in island {
            scratch.island_of[cell] = u32::MAX;
        }
    }

    outcome.residual_demand += scratch.demand_bits.iter().filter(|bit| **bit).count();
    outcome
}

/// `max(1, ⌈area / hex cell⌉)` for an island of `cells` cells.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a cell count below ~1e7 is exact in f64, and the quotient is a small positive count"
)]
fn hex_bound(cells: usize, cell_area_mm2: f64, hex_cell_mm2: f64) -> u64 {
    (cells as f64 * cell_area_mm2 / hex_cell_mm2).ceil().max(1.0) as u64
}

/// What every seat on one layer reads and nothing on it changes.
#[derive(Clone, Copy)]
struct Seating<'a> {
    grid: &'a Grid,
    layer: usize,
    cover: &'a [bool],
    keep_out: &'a DrainKeepOut,
    law: &'a LeanLaw,
    shaft_landings: bool,
    kappa: f64,
    separation_mm: f64,
    max_struts: usize,
}

/// Whether a covering is being measured or seated.
enum Placer<'s> {
    /// Relax the reach field and clear demand only, under the journal.
    Trial,
    /// Seat the arm for real: descend, record it, stamp the column history.
    Commit {
        struts: &'s mut Vec<SweepStrut>,
        outcome: &'s mut LayerOutcome,
    },
}

/// The connected islands of this layer's demand (8-connected), each in scan
/// order, in the order of their first cell. Labels `scratch.island_of`.
fn label_islands(grid: &Grid, scratch: &mut GreedyScratch) -> Vec<Vec<usize>> {
    let cols = grid.cols();
    let rows = grid.rows();
    let mut islands: Vec<Vec<usize>> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..grid.cells() {
        if !scratch.demand_bits[start] || scratch.island_of[start] != u32::MAX {
            continue;
        }
        let id = u32::try_from(islands.len()).unwrap_or(u32::MAX);
        let mut cells = Vec::new();
        scratch.island_of[start] = id;
        stack.push(start);
        while let Some(cell) = stack.pop() {
            cells.push(cell);
            let row = cell / cols;
            let col = cell % cols;
            for near_row in row.saturating_sub(1)..=(row + 1).min(rows - 1) {
                for near_col in col.saturating_sub(1)..=(col + 1).min(cols - 1) {
                    let near = near_row * cols + near_col;
                    if scratch.demand_bits[near] && scratch.island_of[near] == u32::MAX {
                        scratch.island_of[near] = id;
                        stack.push(near);
                    }
                }
            }
        }
        cells.sort_unstable();
        islands.push(cells);
    }
    islands
}

/// The seats of the cheapest measured covering of one island, in the order
/// they were placed. See the placement comment in [`place_layer`].
#[expect(
    clippy::cast_possible_truncation,
    reason = "a reach radius of ~2 mm is held in f32 beside the f32 depth field it is compared \
          with; f32 resolves it to 1e-7 mm"
)]
fn choose_plan(
    seating: &Seating<'_>,
    id: u32,
    island: &[usize],
    lattice_seeds: &[usize],
    reach: &mut ReachState,
    history: &mut ColumnHistory,
    scratch: &mut GreedyScratch,
) -> Vec<usize> {
    let global = trial(seating, lattice_seeds, None, island, reach, history, scratch);
    if global.len() <= 1 {
        return global;
    }
    scratch.islands_compared += 1;
    let baseline = global.len();
    let mut best = global;
    let mut other_won = false;
    island_depth(seating.grid, island, id, &scratch.island_of, &mut scratch.depth);
    let rho = (seating.separation_mm / 3.0_f64.sqrt()) as f32;
    let deepest = island
        .iter()
        .map(|cell| scratch.depth[*cell])
        .fold(0.0_f32, f32::max);
    // Which coverings are worth measuring is decided by the island's shape,
    // and only to bound the cost: every one of them is a complete covering,
    // so leaving one out can cost seats and never coverage. An island no
    // deeper than one reach radius anywhere is a strip or a ring: no lattice
    // row fits across it, and the row the greedy walks down its midline is the
    // covering. A deeper island is where a lattice pays, and where the
    // greedy's cost (a full candidate search per seat) is largest.
    let mut candidates: Vec<(Vec<usize>, Option<f32>)> = Vec::new();
    if deepest <= rho {
        candidates.push((Vec::new(), None));
        candidates.push((Vec::new(), Some(0.5 * rho)));
    } else {
        for theta_deg in island_orientations(seating.grid, island) {
            for phase in [0.0, 0.25, 0.5] {
                candidates.push((
                    aligned_lattice_cells(
                        seating.grid,
                        island,
                        id,
                        &scratch.island_of,
                        seating.separation_mm,
                        theta_deg,
                        phase,
                    ),
                    None,
                ));
            }
        }
    }
    for (seeds, ridge) in &candidates {
        let plan = trial(seating, seeds, *ridge, island, reach, history, scratch);
        if plan.len() < best.len() {
            best = plan;
            other_won = true;
        }
    }
    for &cell in island {
        scratch.depth[cell] = 0.0;
    }
    if other_won {
        scratch.islands_realigned += 1;
        scratch.seats_saved += (baseline - best.len()) as u64;
    }
    best
}

/// Each cell's depth inside one island's edge, in mm: the 8-neighbour chamfer
/// distance to the nearest cell outside the island, less the half cell by
/// which the edge lies beyond the outermost cell centres. A placement
/// heuristic only: it chooses which candidate cells a ridge covering may seat
/// on, and the reach field then measures what those seats actually cover.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the pitch (0.05 mm) is held in f32 like the depth field; a signed row or column is \
          cast back to usize only after it is checked non-negative"
)]
fn island_depth(grid: &Grid, island: &[usize], id: u32, island_of: &[u32], depth: &mut [f32]) {
    let cols = grid.cols();
    let pitch = grid.pitch_mm() as f32;
    let diagonal = pitch * core::f32::consts::SQRT_2;
    let (mut row_lo, mut row_hi, mut col_lo, mut col_hi) = (usize::MAX, 0, usize::MAX, 0);
    for &cell in island {
        depth[cell] = f32::INFINITY;
        row_lo = row_lo.min(cell / cols);
        row_hi = row_hi.max(cell / cols);
        col_lo = col_lo.min(cell % cols);
        col_hi = col_hi.max(cell % cols);
    }
    // Outside the island (or off the grid) reads as depth zero at the centre
    // of the neighbouring cell.
    let at = |depth: &[f32], row: isize, col: isize| -> f32 {
        if row < 0 || col < 0 || row as usize >= grid.rows() || col as usize >= cols {
            return 0.0;
        }
        let cell = row as usize * cols + col as usize;
        if island_of[cell] == id {
            depth[cell]
        } else {
            0.0
        }
    };
    #[expect(
        clippy::cast_possible_wrap,
        reason = "row and column indices are bounded by the grid side (~1e4), far inside isize"
    )]
    let signed = |value: usize| value as isize;
    for row in row_lo..=row_hi {
        for col in col_lo..=col_hi {
            let cell = row * cols + col;
            if island_of[cell] != id {
                continue;
            }
            let (r, c) = (signed(row), signed(col));
            let best = depth[cell]
                .min(at(depth, r, c - 1) + pitch)
                .min(at(depth, r - 1, c) + pitch)
                .min(at(depth, r - 1, c - 1) + diagonal)
                .min(at(depth, r - 1, c + 1) + diagonal);
            depth[cell] = best;
        }
    }
    for row in (row_lo..=row_hi).rev() {
        for col in (col_lo..=col_hi).rev() {
            let cell = row * cols + col;
            if island_of[cell] != id {
                continue;
            }
            let (r, c) = (signed(row), signed(col));
            let best = depth[cell]
                .min(at(depth, r, c + 1) + pitch)
                .min(at(depth, r + 1, c) + pitch)
                .min(at(depth, r + 1, c + 1) + diagonal)
                .min(at(depth, r + 1, c - 1) + diagonal);
            depth[cell] = best;
        }
    }
    for &cell in island {
        depth[cell] -= 0.5 * pitch;
    }
}

/// Measure one covering of `island` from `seeds` on the live field, then put
/// the field back exactly as it was. Returns the seats it placed, in order.
fn trial(
    seating: &Seating<'_>,
    seeds: &[usize],
    ridge: Option<f32>,
    island: &[usize],
    reach: &mut ReachState,
    history: &mut ColumnHistory,
    scratch: &mut GreedyScratch,
) -> Vec<usize> {
    reach.begin_journal();
    scratch.cleared.clear();
    let (seats, dirty) = cover_island(
        seating,
        seeds,
        ridge,
        island,
        reach,
        history,
        scratch,
        &mut Placer::Trial,
    );
    reach.undo_journal();
    for index in 0..scratch.cleared.len() {
        let cell = scratch.cleared[index];
        scratch.demand_bits[cell] = true;
    }
    scratch.cleared.clear();
    if dirty.0 <= dirty.1 {
        rebuild_prefix_rows(
            seating.grid,
            &scratch.demand_bits,
            &mut scratch.prefix,
            dirty.0,
            dirty.1,
        );
    }
    seats
}

/// Cover what is left of one island's demand: the given seeds first, in
/// order, wherever each is still uncovered demand and admitted by the drain
/// keep-out, then the pivot pass until nothing coverable is left.
///
/// `ridge` is `Some(ρ/2)` for the ridge covering (see [`best_seat_for`]),
/// `None` for the plain greedy.
///
/// Returns the seats placed and the demand rows they changed.
#[expect(
    clippy::too_many_arguments,
    reason = "one covering reads the layer's fixed inputs, its seeds, its fill rule and its \
          island, and writes the reach field, the history, the scratch and the placer"
)]
fn cover_island(
    seating: &Seating<'_>,
    seeds: &[usize],
    ridge: Option<f32>,
    island: &[usize],
    reach: &mut ReachState,
    history: &mut ColumnHistory,
    scratch: &mut GreedyScratch,
    placer: &mut Placer<'_>,
) -> (Vec<usize>, (usize, usize)) {
    let Seating {
        grid,
        layer,
        cover,
        keep_out,
        max_struts,
        ..
    } = *seating;
    let mut seats = Vec::new();
    let mut dirty = (usize::MAX, 0_usize);
    let at_limit = |placer: &Placer<'_>| match placer {
        Placer::Trial => false,
        Placer::Commit { struts, .. } => struts.len() >= max_struts,
    };
    let place = |cell: usize,
                 reach: &mut ReachState,
                 history: &mut ColumnHistory,
                 scratch: &mut GreedyScratch,
                 placer: &mut Placer<'_>| match placer {
        Placer::Trial => trial_seat(seating, cell, reach, scratch),
        Placer::Commit { struts, outcome } => {
            commit_seat(seating, cell, reach, history, scratch, struts, outcome)
        }
    };
    // A lattice point inside a drain's escape cylinder is simply not used. The
    // demand it would have covered stays in the set and the pivot pass below
    // looks for a legal seat for it; only if none exists does it survive as
    // reported residual demand.
    for &seed in seeds {
        if at_limit(placer) {
            return (seats, dirty);
        }
        if !scratch.demand_bits[seed] || !keep_out.admits(seed, layer) {
            continue;
        }
        let (lo, hi) = place(seed, reach, history, scratch, placer);
        seats.push(seed);
        dirty = (dirty.0.min(lo), dirty.1.max(hi));
    }
    // The seeds do not read the prefix sums; the pivot pass does.
    if dirty.0 <= dirty.1 {
        rebuild_prefix_rows(grid, &scratch.demand_bits, &mut scratch.prefix, dirty.0, dirty.1);
    }
    let mut cursor = 0_usize;
    let mut last_pivot = usize::MAX;
    loop {
        if at_limit(placer) {
            break;
        }
        while cursor < island.len() && !scratch.demand_bits[island[cursor]] {
            cursor += 1;
        }
        if cursor >= island.len() {
            break;
        }
        let pivot = island[cursor];
        // A pivot that survived its own round is one no candidate could
        // reach. Seating on it directly roots it by construction, which is
        // what makes this loop terminate.
        let chosen = if pivot == last_pivot {
            keep_out.admits(pivot, layer).then_some(pivot)
        } else {
            best_seat_for(grid, cover, history, scratch, keep_out, layer, pivot, ridge)
        };
        last_pivot = pivot;
        let Some(chosen) = chosen else {
            // Every seat that could root this cell stands where the resin has
            // to leave. EMIT AND WARN: step past it, leave its demand bit set
            // so the residual-demand count at the end of the layer reports it,
            // and never refuse the model over it.
            if let Placer::Commit { outcome, .. } = placer {
                outcome.keep_out_refusals += 1;
            }
            cursor += 1;
            continue;
        };
        let (lo, hi) = place(chosen, reach, history, scratch, placer);
        seats.push(chosen);
        if lo > hi {
            continue;
        }
        dirty = (dirty.0.min(lo), dirty.1.max(hi));
        // Local invalidation only. A global rebuild here is what makes a
        // greedy quadratic in the layer; a row the insertion did not touch
        // cannot have moved.
        rebuild_prefix_rows(grid, &scratch.demand_bits, &mut scratch.prefix, lo, hi);
    }
    (seats, dirty)
}

/// A trial seat: root it in the reach field and clear the demand it covers,
/// recording every cleared cell so [`trial`] can put it back.
fn trial_seat(
    seating: &Seating<'_>,
    cell: usize,
    reach: &mut ReachState,
    scratch: &mut GreedyScratch,
) -> (usize, usize) {
    let cols = seating.grid.cols();
    scratch.touched.clear();
    crate::reach::relax_from_root(
        seating.grid,
        seating.cover,
        seating.kappa,
        cell,
        reach,
        &mut scratch.queue,
        &mut scratch.touched,
    );
    let mut dirty = (usize::MAX, 0_usize);
    for index in 0..scratch.touched.len() {
        let touched = scratch.touched[index];
        if scratch.demand_bits[touched] && reach.is_rooted(touched) {
            scratch.demand_bits[touched] = false;
            scratch.cleared.push(touched);
            let row = touched / cols;
            dirty = (dirty.0.min(row), dirty.1.max(row));
        }
    }
    dirty
}

/// The orientations an island's aligned lattices are tried at, in degrees:
/// rows along the grid's X and along its Y (the hex lattice repeats every
/// 60°, so 30° is rows along Y), and, for an elongated island, rows along
/// and across its principal axis.
#[expect(
    clippy::cast_precision_loss,
    reason = "cell indices and counts are bounded by the grid size (~1e7) and are exact enough in \
          f64 for a second moment"
)]
fn island_orientations(grid: &Grid, island: &[usize]) -> Vec<f64> {
    let mut angles = vec![0.0, 30.0];
    let cols = grid.cols();
    let count = island.len() as f64;
    if island.len() < 3 {
        return angles;
    }
    let (mut sx, mut sy) = (0.0, 0.0);
    for &cell in island {
        sx += (cell % cols) as f64;
        sy += (cell / cols) as f64;
    }
    let (mx, my) = (sx / count, sy / count);
    let (mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0);
    for &cell in island {
        let dx = (cell % cols) as f64 - mx;
        let dy = (cell / cols) as f64 - my;
        xx += dx * dx;
        yy += dy * dy;
        xy += dx * dy;
    }
    // The eigenvalues of the second moment; an island is elongated when the
    // major one is at least twice the minor one. A disc, a ring or a square
    // is not, and has no principal axis worth aligning to.
    let mean = 0.5 * (xx + yy);
    let spread = (0.25 * (xx - yy).powi(2) + xy * xy).sqrt();
    if mean - spread > 0.0 && (mean + spread) / (mean - spread) < 2.0 {
        return angles;
    }
    let axis = 0.5 * (2.0 * xy).atan2(xx - yy).to_degrees();
    for theta in [axis, axis + 30.0] {
        let folded = theta.rem_euclid(60.0);
        if angles.iter().all(|seen| (seen - folded).abs() > 1.0) {
            angles.push(folded);
        }
    }
    angles
}

/// Hex lattice points laid out on one island, with its rows at `theta_deg`
/// to the grid's X axis and aligned to the island's own extent: the first row
/// half a reach radius inside the island's edge across the rows (so that
/// row's discs cover the edge exactly), and each row starting `phase` of a
/// lattice pitch from the island's edge along the rows. Only points that fall
/// on the island's own cells are returned, in row order.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "lattice indices are non-negative finite quotients of model-scale lengths, and every \
          point is range-checked against the grid before it becomes a cell index"
)]
fn aligned_lattice_cells(
    grid: &Grid,
    island: &[usize],
    id: u32,
    island_of: &[u32],
    separation_mm: f64,
    theta_deg: f64,
    phase: f64,
) -> Vec<usize> {
    let cols = grid.cols();
    let (sin, cos) = theta_deg.to_radians().sin_cos();
    let (mut u_lo, mut u_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut v_lo, mut v_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for &cell in island {
        let x = grid.center_x_mm(cell % cols);
        let y = grid.center_y_mm(cell / cols);
        let u = x * cos + y * sin;
        let v = -x * sin + y * cos;
        u_lo = u_lo.min(u);
        u_hi = u_hi.max(u);
        v_lo = v_lo.min(v);
        v_hi = v_hi.max(v);
    }
    // The island's edge is half a cell beyond its outermost cell centres.
    let half = 0.5 * grid.pitch_mm();
    let (u_lo, u_hi, v_lo, v_hi) = (u_lo - half, u_hi + half, v_lo - half, v_hi + half);
    let rho = separation_mm / 3.0_f64.sqrt();
    let row_pitch = 1.5 * rho;
    let rows = ((v_hi - v_lo) / row_pitch).ceil() as usize + 1;
    let per_row = ((u_hi - u_lo) / separation_mm).ceil() as usize + 2;
    let mut cells = Vec::new();
    for row in 0..rows {
        let v = v_lo + 0.5 * rho + row as f64 * row_pitch;
        let stagger = if row % 2 == 1 { 0.5 * separation_mm } else { 0.0 };
        for column in 0..per_row {
            let u = u_lo + (phase * separation_mm + stagger).rem_euclid(separation_mm)
                + column as f64 * separation_mm;
            let x = u * cos - v * sin;
            let y = u * sin + v * cos;
            let (row_f, col_f) = (grid.row_coord(y).floor(), grid.col_coord(x).floor());
            if row_f < 0.0
                || col_f < 0.0
                || row_f >= grid.rows() as f64
                || col_f >= cols as f64
            {
                continue;
            }
            let cell = grid.index(row_f as usize, col_f as usize);
            if island_of[cell] == id {
                cells.push(cell);
            }
        }
    }
    cells
}

/// Seat one arm for real at `cell`: descend, root it, record it and stamp the
/// column history. Returns the demand rows it changed (`lo > hi` when none).
fn commit_seat(
    seating: &Seating<'_>,
    cell: usize,
    reach: &mut ReachState,
    history: &mut ColumnHistory,
    scratch: &mut GreedyScratch,
    struts: &mut Vec<SweepStrut>,
    outcome: &mut LayerOutcome,
) -> (usize, usize) {
    let Seating {
        grid,
        layer,
        cover,
        keep_out,
        law,
        shaft_landings,
        kappa,
        separation_mm,
        ..
    } = *seating;
    let cols = grid.cols();
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
        outcome,
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
        thickened_mm: Vec::new(),
        seat_on_strut: None,
        k_top: layer,
        k_bottom,
        landing,
        interior: false,
        radius_mm: Vec::new(),
        carried_mm2: Vec::new(),
        covered_demand: covered,
        inserted_by_load: false,
        // Recorded ONLY for a leaned arm, and recorded NOW, because the column
        // history this reads is a running state that will not say the same
        // thing again once the sweep has passed this layer.
        seat_face: crate::SeatFace::default(),
        seat_normal: [0.0; 3],
        fallback_path: Vec::new(),
        straight_fallback: (!path.is_empty()).then(|| {
            let (landing, bottom, _) = descend(history, cell, layer);
            (landing, bottom)
        }),
        path,
    });
    // Every column the arm passes through now holds an arm, not just its seat
    // column: a leaned arm that only stamped its seat would let the next
    // descent route straight through its shaft.
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
    // And the SHAFT, layer interval by layer interval, so a later descent can
    // land anywhere on it and not only at its top. Nothing is copied: the
    // arm's own path is read in place.
    let placed = struts.last().expect("the arm just pushed");
    history.stamp_shaft(id, k_bottom, layer, cell, &placed.path);
    (dirty_lo, dirty_hi)
}

/// Nearest-neighbour spacing of the hex covering at the registered reach.
///
/// `√3 · ρ` is the spacing at which discs of radius `ρ` exactly cover the plane
/// (`docs/physics.md`:128-135). The reach field is now measured on the
/// straight-line metric ([`crate::reach::any_angle_relax`]), so a root reaches
/// the full registered `κ·h` in every direction and `ρ` is that reach less
/// ONE thing: a lattice point is seated at the centre of the raster cell it
/// falls in, which moves it by up to half a cell diagonal, `p/√2`. A covering
/// of radius `κ·h − p/√2` about the ideal points is therefore a covering of
/// radius `κ·h` about the seated ones. **Geometry, not a constant.**
///
/// Until 2026-09-30 `ρ` was `κ·h / 1.0824`, the inradius of the octagon the
/// 8-neighbour chamfer called "within reach" (`oversupport-attribution.md`
/// item 7): 3.408 mm separation at `κ·h = 2.130 mm`, now 3.628 mm.
pub(crate) fn hex_separation_mm(reach_mm: f64, pitch_mm: f64) -> f64 {
    3.0_f64.sqrt() * (reach_mm - pitch_mm * core::f64::consts::FRAC_1_SQRT_2)
}

/// The separation this engine used before 2026-09-30, when reach travelled
/// on the 8-neighbour chamfer: `√3·κh / 1.0824`, the chamfer's inradius.
/// **Sabotage and comparison only** ([`crate::SweepPolicy::without_straight_reach`]).
pub(crate) fn chamfer_hex_separation_mm(reach_mm: f64) -> f64 {
    let eighth = core::f64::consts::FRAC_PI_8;
    3.0_f64.sqrt() * reach_mm / (eighth.cos() + (core::f64::consts::SQRT_2 - 1.0) * eighth.sin())
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
#[expect(
    clippy::cast_possible_truncation,
    reason = "the pitch (0.05 mm) is held in f32 beside the f32 depth field it bounds"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "the search reads the grid, the cover, the history's tie-break, the greedy's scratch, \
          the keep-out, its layer and pivot, and the optional ridge band; a struct would move the \
          list rather than shorten it"
)]
fn best_seat_for(
    grid: &Grid,
    cover: &[bool],
    history: &ColumnHistory,
    scratch: &GreedyScratch,
    keep_out: &DrainKeepOut,
    layer: usize,
    pivot: usize,
    ridge_depth_mm: Option<f32>,
) -> Option<usize> {
    let cols = grid.cols();
    let rows = grid.rows();
    let reach = scratch.reach_cells;
    let pivot_row = pivot / cols;
    let pivot_col = pivot % cols;
    let admissible = |candidate: usize| -> bool {
        // A seat must be a legal host AND somewhere support is actually
        // wanted. Allowing an already-rooted host was measured on
        // `make_overhang_table` to put four arms inside a leg's own
        // self-rooted fringe — legal, and pure waste.
        cover[candidate] && scratch.demand_bits[candidate] && keep_out.admits(candidate, layer)
    };
    // THE RIDGE BAND (the ridge covering of [`choose_plan`]). A seat at
    // depth `d` inside an island's edge covers the edge beside it over a run
    // of `2·√(R² − d²)`, and a hex row at `d = ρ/2` covers its edge exactly.
    // So the band is `ρ/2` inside the edge where the island is that deep
    // within reach of the pivot, and the deepest reachable cells (the
    // island's midline) where it is thinner: one row down a strip or round a
    // ring, an edge row round a wide island. `None` is the plain greedy.
    let band = ridge_depth_mm.map(|half_rho| {
        let mut deepest = 0.0_f32;
        for row in pivot_row.saturating_sub(reach)..=(pivot_row + reach).min(rows - 1) {
            let half = scratch.half_widths[row.abs_diff(pivot_row)];
            for col in pivot_col.saturating_sub(half)..=(pivot_col + half).min(cols - 1) {
                let candidate = row * cols + col;
                if admissible(candidate) {
                    deepest = deepest.max(scratch.depth[candidate]);
                }
            }
        }
        let target = half_rho.min(deepest);
        let slack = grid.pitch_mm() as f32;
        (target - slack, target + slack)
    });
    // THE PIVOT'S CROSS-SECTION. A ridge seat chosen only to reach the pivot
    // goes as far forward as it can, and where the pivot sits on one edge of a
    // strip it then overshoots the OTHER edge and leaves a sliver of it behind,
    // which costs a whole seat later (measured on a 2 mm ring: 60 seats against
    // 42). So the seat must also cover every uncovered cell of the pivot's own
    // island within one band width (`2·target + p`) of it, no deeper than the
    // band covers. A disc covers a set exactly when it covers the set's convex
    // hull, so only the hull's vertices are tested. When no candidate can
    // cover them all, the band choice alone stands.
    let hull = band.map_or_else(Vec::new, |(lo, _)| {
        let target = lo + grid.pitch_mm() as f32;
        cross_section_hull(grid, scratch, pivot, 2.0 * target + grid.pitch_mm() as f32)
    });
    let radius_sq = scratch.radius_cells * scratch.radius_cells;
    let covers_hull = |candidate: usize| -> bool {
        let (row, col) = ((candidate / cols) as f64, (candidate % cols) as f64);
        hull.iter()
            .all(|&(r, c)| (r - row).powi(2) + (c - col).powi(2) <= radius_sq)
    };
    let mut best_hull: Option<(usize, (u32, bool, u32))> = None;
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
            if !admissible(candidate) {
                continue;
            }
            if let Some((lo, hi)) = band {
                let depth = scratch.depth[candidate];
                if depth < lo || depth > hi {
                    continue;
                }
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
            if !hull.is_empty()
                && best_hull.is_none_or(|(_, held)| key > held)
                && covers_hull(candidate)
            {
                best_hull = Some((candidate, key));
            }
        }
    }
    best_hull.map_or(best, |(candidate, _)| Some(candidate))
}

/// The convex hull, in (row, col) cell units, of the uncovered cells of the
/// pivot's island within `within_mm` of the pivot and no deeper than
/// `within_mm` inside the island's edge. See [`best_seat_for`].
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "cell indices and radii are bounded by the grid side (~1e4) and exact in f64"
)]
fn cross_section_hull(
    grid: &Grid,
    scratch: &GreedyScratch,
    pivot: usize,
    within_mm: f32,
) -> Vec<(f64, f64)> {
    let cols = grid.cols();
    let rows = grid.rows();
    let island = scratch.island_of[pivot];
    let radius = f64::from(within_mm) / grid.pitch_mm();
    let span = radius.floor() as usize;
    let (pivot_row, pivot_col) = (pivot / cols, pivot % cols);
    let mut points: Vec<(f64, f64)> = Vec::new();
    for row in pivot_row.saturating_sub(span)..=(pivot_row + span).min(rows - 1) {
        for col in pivot_col.saturating_sub(span)..=(pivot_col + span).min(cols - 1) {
            let cell = row * cols + col;
            let (dr, dc) = (row as f64 - pivot_row as f64, col as f64 - pivot_col as f64);
            if dr * dr + dc * dc > radius * radius
                || !scratch.demand_bits[cell]
                || scratch.island_of[cell] != island
                || scratch.depth[cell] > within_mm
            {
                continue;
            }
            points.push((row as f64, col as f64));
        }
    }
    convex_hull(points)
}

/// Andrew's monotone chain; collinear points are dropped.
fn convex_hull(mut points: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    points.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    };
    let mut hull: Vec<(f64, f64)> = Vec::with_capacity(2 * points.len());
    for pass in 0..2 {
        let start = hull.len();
        let ordered: Box<dyn Iterator<Item = &(f64, f64)>> = if pass == 0 {
            Box::new(points.iter())
        } else {
            Box::new(points.iter().rev())
        };
        for &point in ordered {
            while hull.len() >= start + 2
                && cross(hull[hull.len() - 2], hull[hull.len() - 1], point) <= 0.0
            {
                hull.pop();
            }
            hull.push(point);
        }
        hull.pop();
    }
    hull
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
pub(crate) fn deferred_path_mm(landing: Landing, k_bottom: usize, layer_height_mm: f64) -> f64 {
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
