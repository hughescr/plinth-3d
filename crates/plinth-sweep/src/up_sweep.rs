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

use std::collections::BTreeMap;

use crate::drain::DrainKeepOut;
use crate::grid::Grid;
use crate::heads::{
    check_head, plan_head, placement_normals, planned_tip_radius_mm, Head, HeadCapsule, HeadCheckCtx, HeadRefusal,
    HeadRequest, HeadRule, HeadSkip, HeadsOutcome, PlacementBall, PlacementEnv, SkippedSeat,
};
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
    /// EXCLUSIVE end of the layers a later member may LAND on in this span:
    /// layers `>= landable_end` block but never take a landing. `u32::MAX`
    /// means no cap (every span except the post-top of a headed arm, whose
    /// head slots and joint clearance are blocking-only; see
    /// [`ColumnHistory::stamp_strut`]).
    pub landable_end: u32,
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
    /// Which members stand inside a hidden cavity, where the 0.3 mm final
    /// clearance gap does not apply (Craig, 2026-10-05; [`crate::cavity`]).
    /// Empty (exempts nothing) until the run builds it from the slices; kept
    /// here because every reader of the finished field's clearance already
    /// holds the column history.
    pub hidden: crate::cavity::HiddenCavity,
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
    /// Per headed arm id, the highest layer another member may join it at
    /// (`k_joint - ceil(r_K / h)`; `None` where the joint stands too low for
    /// any join). Filled by [`Self::stamp_strut`]; the producer lane reads it.
    pub joinable_tops: BTreeMap<u32, Option<usize>>,
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
            hidden: crate::cavity::HiddenCavity::none(),
            shaft_head: vec![u32::MAX; cells],
            shaft_spans: Vec::new(),
            model_head: vec![u32::MAX; cells],
            model_spans: Vec::new(),
            joinable_tops: BTreeMap::new(),
        }
    }

    /// Record one finished member's shaft, honouring its head.
    ///
    /// A straight arm stamps exactly as [`Self::stamp_shaft`] does. A HEADED
    /// arm stamps its whole path (post and head slots both BLOCK), but its
    /// post is LANDABLE only up to the joinable top, `k_joint - ceil(r_K / h)`:
    /// a later member may never land on the joint sphere or the head, and its
    /// own section must stay clear of the sphere
    /// ([`crate::SweepStrut::joinable_at`]). The joinable top is recorded per
    /// id in [`Self::joinable_tops`].
    pub fn stamp_strut(&mut self, strut: &SweepStrut, layer_height_mm: f64) {
        if strut.head.is_none() {
            self.stamp_shaft(strut.id, strut.k_bottom, strut.k_top, strut.cell, &strut.path);
            return;
        }
        let joinable_top = strut.joinable_top_layer(layer_height_mm);
        self.joinable_tops.insert(strut.id, joinable_top);
        // Exclusive end: one past the joinable top, or nothing landable.
        let landable_end = joinable_top.map_or(0, |top| u32::try_from(top + 1).unwrap_or(u32::MAX));
        self.stamp_runs(
            strut.id,
            strut.k_bottom,
            strut.k_top,
            strut.cell,
            &strut.path,
            landable_end,
        );
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
        self.stamp_runs(id, k_bottom, k_top, seat_cell, path, u32::MAX);
    }

    /// [`Self::stamp_shaft`] with the exclusive end of the landable layers.
    fn stamp_runs(
        &mut self,
        id: u32,
        k_bottom: usize,
        k_top: usize,
        seat_cell: usize,
        path: &[u32],
        landable_end: u32,
    ) {
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
                self.push_span(run_cell, id, run_from, layer - 1, landable_end);
            }
            run_cell = here;
            run_from = layer;
        }
        if run_cell != usize::MAX {
            self.push_span(run_cell, id, run_from, k_top, landable_end);
        }
    }

    fn push_span(&mut self, cell: usize, id: u32, k_bottom: usize, k_top: usize, landable_end: u32) {
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
            landable_end,
        });
    }

    /// Raise the top of arm `id`'s span in `cell` from `from_k_top` to
    /// `to_k_top`, and the column's top-strut stamp with it: the arm's seat
    /// moved up a few layers in the column it already stands in
    /// ([`raise_host_seat`]). A span that is not there is left alone.
    pub(crate) fn raise_shaft_top(&mut self, id: u32, cell: usize, from_k_top: usize, to_k_top: usize) {
        let mut slot = self.shaft_head.get(cell).copied().unwrap_or(u32::MAX);
        while slot != u32::MAX {
            let span = &mut self.shaft_spans[slot as usize];
            if span.id == id && span.k_top as usize == from_k_top {
                span.k_top = u32::try_from(to_k_top).unwrap_or(u32::MAX);
                break;
            }
            slot = span.next;
        }
        let stamp = u32::try_from(to_k_top + 1).unwrap_or(u32::MAX);
        if self.top_strut[cell] < stamp {
            self.top_strut[cell] = stamp;
            self.top_strut_id[cell] = id;
        }
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
            // A span's landable layers end below `landable_end`: a headed
            // arm's joint and head block but are never landed on.
            let end = span.landable_end as usize;
            if bottom > ceiling || bottom >= end {
                continue;
            }
            let top = span.k_top as usize;
            let here = top.min(ceiling).min(end - 1);
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
                landable_end: u32::MAX,
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
    /// Whether a pivot no demand seat can reach (all of them in a drain's
    /// escape cylinder) may seat on a rooted underside cell that reaches it
    /// ([`stranded_seat_for`]). `false` is the sabotage form
    /// ([`crate::SweepPolicy::without_stranded_seating`]): such demand is left
    /// BARE, as before 2026-10-01.
    pub stranded_seating: bool,
    /// Seats placed that way. Path-hit witness.
    pub stranded_seats: u64,
    /// Each committed stranded seat as `(seat cell, seat layer)`, in the
    /// order it was placed. A member's id is renumbered by the removal pass,
    /// its seat cell and seat layer are not, so this is how the stranded-seat
    /// branch rule ([`crate::stranded_branch`]) finds the members this rule
    /// seated once the field is sized.
    pub stranded_seat_cells: Vec<(usize, usize)>,
    /// Whether a seat whose tip ball overlaps an earlier seat's is merged into
    /// it ([`twin_host`]). `false` is the sabotage form
    /// ([`crate::SweepPolicy::without_twin_merge`]).
    pub twin_merge: bool,
    /// Seats merged that way. Path-hit witness.
    pub twin_merges: u64,
    /// Where each arm that has been raised first seated (arm id to layer).
    twin_first_seat: BTreeMap<u32, usize>,
    /// The articulated heads built so far, and what was refused
    /// ([`HeadsState`]); empty unless the sweep runs with heads.
    pub heads: HeadsState,
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
            stranded_seating: true,
            stranded_seats: 0,
            stranded_seat_cells: Vec::new(),
            twin_merge: true,
            twin_merges: 0,
            twin_first_seat: BTreeMap::new(),
            heads: HeadsState::default(),
        }
    }

    #[must_use]
    pub const fn reach_cells(&self) -> usize {
        self.reach_cells
    }
}

/// What one layer's greedy produced.
#[derive(Debug)]
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
    /// What the drawn-solid test did on this layer ([`DescentTally`]).
    pub solid: DescentTally,
}

impl LayerOutcome {
    /// An outcome with every count at zero.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            demand_cells: 0,
            residual_demand: 0,
            unrooted_landings: 0,
            keep_out_refusals: 0,
            arms_leaned: 0,
            arms_merged: 0,
            lean_paths_blocked: 0,
            junctions_mid_shaft: 0,
            lean_candidates_priced: 0,
            solid: DescentTally::default(),
        }
    }

    /// Fold in what ONE accepted descent counted (a trial descent that was
    /// not taken is dropped, never folded).
    fn absorb_descent(&mut self, other: &LayerOutcome) {
        self.arms_leaned += other.arms_leaned;
        self.arms_merged += other.arms_merged;
        self.lean_paths_blocked += other.lean_paths_blocked;
        self.junctions_mid_shaft += other.junctions_mid_shaft;
        self.lean_candidates_priced += other.lean_candidates_priced;
        self.solid.absorb(&other.solid);
    }
}

/// **What the drawn-solid test did** (fix/drawn-solid-clearance, 2026-10-04):
/// the path-hit witnesses that the test ran and that it bit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DescentTally {
    /// Straight descents whose drawn solid was tested against the model.
    pub tested: u64,
    /// Of those, straight descents standing inside the preferred 0.6 mm
    /// placement margin (kept when they clear the 0.3 mm final gap).
    pub inside_preferred_margin: u64,
    /// Of those, straight descents that stand wholly in a hidden cavity (the
    /// solid at its lower-bound radius in hidden voxels), where the final gap
    /// is not asked (Craig, 2026-10-05).
    pub in_hidden_cavity: u64,
    /// Straight descents that do NOT clear the final gap: the ones the
    /// search replaces (a lean) or whose seat is moved.
    pub refused_at_final_gap: u64,
    /// Refused straight descents replaced by a leaned one that clears the
    /// preferred margin.
    pub cleared_by_lean: u64,
    /// Refused straight descents replaced by a descent that clears the
    /// final gap only (no candidate cleared the preferred margin).
    pub at_final_gap_only: u64,
    /// Descents no candidate cleared even at the final gap: kept, and
    /// WARNED by the census (emit and warn).
    pub uncleared: u64,
    /// Lean candidates refused because their drawn solid enters the gap.
    pub lean_candidates_refused: u64,
    /// Seats moved to another cell that covers the SAME demand because no
    /// descent from the first cell cleared.
    pub seats_moved: u64,
    /// **Model landings whose foot does not fit** (lane 2, 2026-10-12,
    /// `MODEL-FEET.md` F2): lean candidates onto the model refused because
    /// the foot's contact disc does not lie wholly on cured model
    /// ([`foot_on_model`]).
    pub foot_candidates_refused: u64,
    /// Straight descents onto the model whose foot does not fit, replaced by
    /// another candidate the search priced (a lean, a merge, the plate, or a
    /// model landing whose foot fits).
    pub foot_straight_replaced: u64,
    /// Straight descents onto the model whose foot does not fit and for which
    /// no candidate was found: kept, and WARNED with the percent on the model
    /// on the finished field (emit and warn).
    pub foot_straight_kept: u64,
}

impl DescentTally {
    pub(crate) fn absorb(&mut self, other: &DescentTally) {
        self.foot_candidates_refused += other.foot_candidates_refused;
        self.foot_straight_replaced += other.foot_straight_replaced;
        self.foot_straight_kept += other.foot_straight_kept;
        self.tested += other.tested;
        self.inside_preferred_margin += other.inside_preferred_margin;
        self.in_hidden_cavity += other.in_hidden_cavity;
        self.refused_at_final_gap += other.refused_at_final_gap;
        self.cleared_by_lean += other.cleared_by_lean;
        self.at_final_gap_only += other.at_final_gap_only;
        self.uncleared += other.uncleared;
        self.lean_candidates_refused += other.lean_candidates_refused;
        self.seats_moved += other.seats_moved;
    }
}

/// **What a descent needs to test its drawn solid against the model**
/// (fix/drawn-solid-clearance, 2026-10-04): Craig's two-tier clearance
/// (2026-08-22) and the lower bound on the radius the arm will be drawn at.
#[derive(Clone, Copy, Debug)]
pub struct DescentClearance<'a> {
    pub layer_height_mm: f64,
    /// The plate's z in the mesh's frame (the raster's frame has it at 0).
    pub plate_z_mm: f64,
    /// `STANDOFF_MM`, the preferred placement margin.
    pub preferred_gap_mm: f64,
    /// `FINAL_CLEARANCE_GAP_MM`, the hard limit a placement may still use.
    pub final_gap_mm: f64,
    /// The registered minimum tip radius: no arm is drawn thinner.
    pub tip_radius_min_mm: f64,
    /// `X_ALLOWED`: the area law's ratio of carried area to arm section.
    pub x_allowed: f64,
    /// [`crate::lean::LeanLaw::face_zone_factor`]: the longest zone a shaft
    /// is exempt for (used whole only where `surface` is absent).
    pub zone_factor: f64,
    /// The model's surface index: names each contact zone's face and gives
    /// the census's own verdict on what the raster flags
    /// ([`crate::member_clearance::ShaftRead::intrusion`]). `None` where it
    /// could not be built.
    pub surface: Option<SurfaceRef<'a>>,
    /// What the up-sweep reads to build an articulated head at each steep seat
    /// ([`HeadEnv`]); `None` is the straight tip everywhere
    /// ([`crate::SweepPolicy::heads`] off).
    pub heads: Option<HeadEnv<'a>>,
}

/// What [`commit_seat`] reads to build an articulated head: the model's
/// triangles, the registered contact-ball table, the registered numbers of the
/// head law, and the separation a head keeps from every accepted member.
#[derive(Clone, Copy)]
pub struct HeadEnv<'a> {
    pub mesh: &'a plinth_mesh::Mesh,
    pub profiles: &'a plinth_core::TipContactProfiles,
    pub rule: HeadRule,
    /// The ids of the model's faces inside the seat-face band
    /// ([`crate::heads::band_face_ids`]): what a seat on a wall's edge reads.
    pub band_faces: &'a [u32],
    /// The clearance a head keeps from stamped shafts and earlier heads.
    pub separation_mm: f64,
}

impl core::fmt::Debug for HeadEnv<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HeadEnv").field("rule", &self.rule).finish_non_exhaustive()
    }
}

/// The margin, in degrees, below the skim angle under which a seat's facet is
/// too flat to ask the footprint for a head. A SEARCH PRUNE and not a rule:
/// the footprint normal differs from the seat facet's by at most 9.88 degrees
/// on the v16 corpus, and [`plan_head`] still decides on the footprint's own
/// normal. It only saves the footprint query on faces no head could want.
const HEAD_PRUNE_MARGIN_DEG: f64 = 15.0;

/// The running state of heads across one sweep: the accepted heads' capsules
/// (the placement tests' "earlier heads"), what was built or refused, and the
/// placement residuals.
#[derive(Debug, Default)]
pub struct HeadsState {
    pub accepted: Vec<HeadCapsule>,
    pub outcome: HeadsOutcome,
    /// Per built head, the angle between the normal the seat was placed with
    /// (pass 1) and the refined one (pass 2), in degrees.
    pub residuals_deg: Vec<f64>,
}

impl HeadsState {
    /// Publish the counts into the run report: heads built, refusals by
    /// reason, and the placement residual (max, median).
    pub fn publish(&self, into: &mut HeadsOutcome) {
        into.built = self.outcome.built;
        into.refusals.clone_from(&self.outcome.refusals);
        into.skips.clone_from(&self.outcome.skips);
        into.skipped_seats.clone_from(&self.outcome.skipped_seats);
        let mut sorted = self.residuals_deg.clone();
        sorted.sort_by(f64::total_cmp);
        if let Some(&max) = sorted.last() {
            let middle = sorted.len() / 2;
            into.placement_residual_deg_max = max;
            into.placement_residual_deg_median = if sorted.len() % 2 == 1 {
                sorted[middle]
            } else {
                0.5 * (sorted[middle - 1] + sorted[middle])
            };
        }
    }
}

/// A borrowed model surface, `Debug` without printing the index.
#[derive(Clone, Copy)]
pub struct SurfaceRef<'a>(pub &'a geom_kernel::NativeSurface);

impl core::fmt::Debug for SurfaceRef<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SurfaceRef(..)")
    }
}

impl DescentClearance<'_> {
    /// The shaft reader of this clearance on `history`.
    pub(crate) fn read<'h>(
        &'h self,
        grid: &'h Grid,
        history: &'h ColumnHistory,
    ) -> crate::member_clearance::ShaftRead<'h> {
        crate::member_clearance::ShaftRead {
            grid,
            history,
            layer_height_mm: self.layer_height_mm,
            plate_z_mm: self.plate_z_mm,
            surface: self.surface.map(|surface| surface.0),
            zone_factor: self.zone_factor,
        }
    }

    /// A LOWER bound on the radius an arm that covers `area_mm2` of demand
    /// is drawn at: the larger of the minimum tip radius and the area-law
    /// radius `√(area / (X · π))` of what it covers. Later landers only add
    /// load, so the drawn radius is never smaller.
    #[must_use]
    pub fn radius_lower_bound_mm(&self, area_mm2: f64) -> f64 {
        let law = if self.x_allowed > 0.0 && area_mm2.is_finite() && area_mm2 > 0.0 {
            (area_mm2 / (self.x_allowed * core::f64::consts::PI)).sqrt()
        } else {
            0.0
        };
        self.tip_radius_min_mm.max(law)
    }
}

/// How well a descent clears the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Clears {
    /// Clears the preferred placement margin.
    Preferred,
    /// Clears the final gap only.
    FinalOnly,
    /// Clears neither: kept and warned.
    Neither,
    /// Not tested (no clearance given, or no solid to read).
    Untested,
}

/// One descent: where it lands, whether the landing is rooted, the leaned
/// path (empty when straight) and how well its drawn solid clears the model.
pub(crate) struct Descent {
    pub landing: Landing,
    pub k_bottom: usize,
    pub rooted: bool,
    pub path: Vec<u32>,
    pub clears: Clears,
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
    place_layer_cleared(
        grid,
        layer,
        cover,
        keep_out,
        law,
        shaft_landings,
        kappa,
        reach,
        history,
        scratch,
        struts,
        max_struts,
        None,
    )
}

/// [`place_layer`] with the drawn-solid test: every descent's solid is read
/// against the model at the two-tier clearance (`clearance`; `None` is the
/// retired centreline-only placement, kept for the tests that pin it).
#[expect(
    clippy::too_many_arguments,
    reason = "one layer's greedy genuinely reads the grid, the layer's masks, the reach state, the \
          column history, the strut list, its scratch, its layer index and the clearance; \
          bundling them into a struct would move the argument list rather than shorten it"
)]
pub fn place_layer_cleared(
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
    clearance: Option<&DescentClearance<'_>>,
) -> LayerOutcome {
    let mut outcome = LayerOutcome::empty();

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
        clearance,
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
    clearance: Option<&'a DescentClearance<'a>>,
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
    let mut stranded_seats = 0_u64;
    let mut stranded_cells: Vec<(usize, usize)> = Vec::new();
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
        let stranded_at_pivot = stranded_seats;
        // A pivot that survived its own round is one no candidate could
        // reach. Seating on it directly roots it by construction, which is
        // what makes this loop terminate.
        let chosen = if pivot == last_pivot {
            keep_out.admits(pivot, layer).then_some(pivot)
        } else {
            best_seat_for(grid, cover, history, scratch, keep_out, layer, pivot, ridge).or_else(
                || {
                    let host = scratch
                        .stranded_seating
                        .then(|| {
                            stranded_seat_for(
                                grid,
                                cover,
                                history,
                                scratch,
                                keep_out,
                                layer,
                                pivot,
                                &reach.u_prev,
                            )
                        })
                        .flatten();
                    if host.is_some() && matches!(placer, Placer::Commit { .. }) {
                        stranded_seats += 1;
                    }
                    host
                },
            )
        };
        last_pivot = pivot;
        // Which seats the stranded rule chose, read off the counter it just
        // bumped rather than threaded out of the closure.
        if stranded_seats > stranded_at_pivot {
            if let Some(host) = chosen {
                stranded_cells.push((host, layer));
            }
        }
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
    scratch.stranded_seats += stranded_seats;
    scratch.stranded_seat_cells.extend(stranded_cells);
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

/// Why [`plan_seat_head`] returned no head.
#[derive(Debug)]
enum HeadExit {
    /// A head was asked for and refused: counted by reason and member id.
    Refused(HeadRefusal),
    /// No head was asked for: counted by reason, and logged with the normals
    /// the decision read (Craig 2026-10-08). The member id is filled in by
    /// the caller.
    Skipped(SkippedSeat),
}

/// The search prune on the nearest face's unit normal `facet`: `Some(reason)`
/// when the seat wants no head and the footprint query is not worth asking. A
/// facet that faces up, or is steeper than the seat-face band admits, wants no
/// head; a facet more than [`HEAD_PRUNE_MARGIN_DEG`] below the skim angle is
/// too flat for the footprint to bring it over.
fn facet_skip(facet: [f64; 3], rule: &HeadRule, tip_radius_lb_mm: f64) -> Option<HeadSkip> {
    let facet_cos = -facet[2];
    if facet_cos <= 0.0 {
        return Some(HeadSkip::FacetNotDownFacing);
    }
    if facet[2] >= rule.max_normal_z {
        return Some(HeadSkip::FacetSteeperThanBand);
    }
    let secant = 1.0 + rule.allowance_mm / (rule.gap_mm + tip_radius_lb_mm);
    let skim_deg = (1.0 / secant).clamp(-1.0, 1.0).acos().to_degrees();
    (facet_cos > (skim_deg - HEAD_PRUNE_MARGIN_DEG).max(0.0).to_radians().cos())
        .then_some(HeadSkip::FacetTooFlat)
}

/// **Plan and check one articulated head for the seat at `cell`** (layer
/// `seating.layer`), before anything rests on the arm. `tip_radius_lb_mm` is
/// the lower bound on the tip's drawn radius (what the seat's demand needs).
///
/// `Ok((head, residual_deg))` when a head is built. `Err(Skipped)` when no
/// head is asked for (heads off, a facet that is not steep, a face too flat to
/// be worth the footprint query: [`HEAD_PRUNE_MARGIN_DEG`], a footprint normal
/// that is not steep): the caller counts it by reason and logs the normals.
/// `Err(Refused(reason))` for every other refusal, which the caller counts by
/// reason and member id.
///
/// The nearest model face to the seat's axis point is named by ONE surface
/// query, which gives the facet's normal for the prune, the seat triangle's
/// identity for the footprint kernel and the face point for the head's plane.
fn plan_seat_head(
    seating: &Seating<'_>,
    history: &ColumnHistory,
    accepted: &[HeadCapsule],
    cell: usize,
    tip_radius_lb_mm: f64,
) -> Result<(Head, f64), HeadExit> {
    let skipped = |reason, seat_mm, plate_z_mm, facet, footprint| {
        HeadExit::Skipped(SkippedSeat { id: 0, reason, seat_mm, plate_z_mm, facet, footprint })
    };
    let Some(env) = seating.clearance.and_then(|clear| clear.heads.map(|env| (clear, env))) else {
        let grid = seating.grid;
        let seat_mm = [grid.center_x_mm(grid.col_of(cell)), grid.center_y_mm(grid.row_of(cell)), 0.0];
        return Err(skipped(HeadSkip::HeadsOff, seat_mm, 0.0, None, None));
    };
    let (clear, env) = env;
    let rule = env.rule;
    // The head is planned at an ESTIMATE of the tip's final radius, not at the
    // minimum its own demand allows: later landers and peel events grow the tip
    // after placement, and a joint planned small stands across the clearance
    // once it grows ([`planned_tip_radius_mm`]; the check after the final
    // sizing decides what is kept).
    let tip_radius_lb_mm =
        planned_tip_radius_mm(tip_radius_lb_mm, &rule, |radius| env.profiles.ball_factor(radius));
    let Some(surface) = clear.surface.map(|surface| surface.0) else {
        return Err(HeadExit::Refused(HeadRefusal::NoSurface));
    };
    let grid = seating.grid;
    #[expect(clippy::cast_precision_loss, reason = "a layer index is exact in f64")]
    let axis_point = [
        grid.center_x_mm(grid.col_of(cell)),
        grid.center_y_mm(grid.row_of(cell)),
        rule.plate_z_mm + (seating.layer as f64 + 0.5) * rule.layer_height_mm,
    ];
    let points = [axis_point];
    let (_, closest, faces, normals) = surface.project(&points);
    let (Some(&face_point), Some(&face), Some(&facet_raw)) =
        (closest.first(), faces.first(), normals.first())
    else {
        return Err(HeadExit::Refused(HeadRefusal::NoSurface));
    };
    let (Ok(face), Some(facet)) = (u32::try_from(face), geom_kernel::vec3::normalize(facet_raw))
    else {
        return Err(HeadExit::Refused(HeadRefusal::NoSurface));
    };
    // The search prune (see [`facet_skip`]). Where the nearest face of ANY
    // slope is not a down-facing face (a seat on the edge between the curved
    // face it supports and a vertical wall: Cylinder members 144 and 151), the
    // seat reads the nearest face INSIDE the seat-face band instead.
    let mut skip = facet_skip(facet, &rule, tip_radius_lb_mm);
    let (face_point, face, facet) = if matches!(
        skip,
        Some(HeadSkip::FacetNotDownFacing | HeadSkip::FacetSteeperThanBand)
    ) {
        match crate::heads::nearest_band_face(surface, env.mesh, env.band_faces, axis_point) {
            Some((point, band_face, normal)) => {
                skip = facet_skip(normal, &rule, tip_radius_lb_mm);
                (point, band_face, normal)
            }
            None => (face_point, face, facet),
        }
    } else {
        (face_point, face, facet)
    };
    if let Some(reason) = skip {
        return Err(skipped(reason, axis_point, rule.plate_z_mm, Some(facet), None));
    }
    let ball = PlacementBall::from_profiles(axis_point, face, facet, tip_radius_lb_mm, env.profiles);
    let placement = PlacementEnv {
        mesh: env.mesh,
        surface: Some(surface),
        max_normal_z: rule.max_normal_z,
    };
    let Some(found) = placement_normals(&placement, &[ball]).into_iter().next() else {
        return Err(HeadExit::Refused(HeadRefusal::NoFootprint));
    };
    let (unit, residual_deg) = found.map_err(HeadExit::Refused)?;
    // The head's axis runs through the DRAWN seat (the cell centre at the seat
    // layer's own z, `adapter::drawn_ends`), because the lowering seats the
    // ball along `u` from there: a head axis through the footprint's ball
    // centre (half a layer higher) would leave the neck a degree off the ball's
    // axis, and the seat trim would cut the neck's wide first ring.
    #[expect(clippy::cast_precision_loss, reason = "a layer index is exact in f64")]
    let drawn_seat = [
        axis_point[0],
        axis_point[1],
        rule.plate_z_mm + seating.layer as f64 * rule.layer_height_mm,
    ];
    let request = HeadRequest {
        axis_point_mm: drawn_seat,
        face_point_mm: face_point,
        unit,
        tip_radius_lb_mm,
        ball_radius_mm: ball.ball_factor * tip_radius_lb_mm,
        seat_cell: u32::try_from(cell).unwrap_or(u32::MAX),
        k_top: seating.layer,
    };
    let head = match plan_head(&request, &rule, grid) {
        Ok(head) => head,
        Err(HeadRefusal::NotSteep) => {
            return Err(skipped(
                HeadSkip::FootprintNotSteep,
                axis_point,
                rule.plate_z_mm,
                Some(facet),
                Some(unit),
            ));
        }
        Err(reason) => return Err(HeadExit::Refused(reason)),
    };
    let ctx = HeadCheckCtx {
        grid,
        history,
        hidden: &history.hidden,
        except: &[],
        keep_out: seating.keep_out,
        surface: Some(surface),
        mesh: env.mesh,
        plate_z_mm: rule.plate_z_mm,
        layer_height_mm: rule.layer_height_mm,
        separation_mm: env.separation_mm,
    };
    check_head(&head, tip_radius_lb_mm, &ctx, accepted).map_err(HeadExit::Refused)?;
    Ok((head, residual_deg))
}

/// The contact-ball radius of a seat that covers `cells` demand cells, at the
/// smallest tip the area law allows for that demand
/// ([`DescentClearance::radius_lower_bound_mm`]) times the registered ball
/// factor of that tip (one without the contact-ball table). The smallest tip is
/// the one that is certain, so two balls that overlap at this size overlap at
/// any size the sizing later gives them.
fn twin_ball_mm(clear: &DescentClearance<'_>, cells: usize, cell_area_mm2: f64) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "a covered-cell count is exact in f64")]
    let radius = clear.radius_lower_bound_mm(cells as f64 * cell_area_mm2);
    clear.heads.map_or(1.0, |env| env.profiles.ball_factor(radius)) * radius
}

/// The earlier arm a seat at `cell` on this layer is a twin of, if any: the
/// arm whose SEAT stands in this very column, below this layer, and whose tip
/// ball overlaps the ball this seat would draw (`|c1 - c2| < r1 + r2`, a
/// geometric identity and no tolerance; both balls on the column's axis, so
/// the distance is the height between the two seats).
///
/// The height is read from where the earlier arm FIRST seated
/// (`first_seat_layer`), not from where earlier merges have since raised it,
/// so a column of seats one layer apart cannot chain a tip upward past the
/// ball it started with. Candidates are the spans of the column's own shaft
/// chain, so the lookup reads the few arms that cross this column, not the
/// arm list; ties go to the lowest id.
fn twin_host(
    seating: &Seating<'_>,
    clear: &DescentClearance<'_>,
    struts: &[SweepStrut],
    history: &ColumnHistory,
    first_seat_layer: &BTreeMap<u32, usize>,
    cell: usize,
    covered_cells: usize,
) -> Option<u32> {
    let layer = seating.layer;
    let cell_area = seating.grid.cell_area_mm2();
    let new_ball = twin_ball_mm(clear, covered_cells, cell_area);
    // (layers between the seats, id): nearest first, then the lowest id.
    let mut best: Option<(usize, u32)> = None;
    let mut slot = history.shaft_head.get(cell).copied().unwrap_or(u32::MAX);
    while slot != u32::MAX {
        let span = history.shaft_spans[slot as usize];
        slot = span.next;
        let Some(host) = struts.get(span.id as usize).filter(|host| host.id == span.id) else {
            continue;
        };
        // The span that holds the host's SEAT: its top layer, in its seat cell.
        if host.cell != cell
            || host.kind != crate::StrutKind::Arm
            || host.inserted_by_load
            || span.k_top as usize != host.k_top
            || host.k_top >= layer
        {
            continue;
        }
        let first = first_seat_layer.get(&host.id).copied().unwrap_or(host.k_top);
        let layers_apart = layer.saturating_sub(first);
        #[expect(clippy::cast_precision_loss, reason = "a layer count is exact in f64")]
        let distance = layers_apart as f64 * clear.layer_height_mm;
        let reach = new_ball + twin_ball_mm(clear, host.covered_demand.len(), cell_area);
        if distance < reach && best.is_none_or(|nearest| (layers_apart, host.id) < nearest) {
            best = Some((layers_apart, host.id));
        }
    }
    best.map(|(_, id)| id)
}

/// Raise `host`'s seat to `layer`, in the column it already stands in, and
/// give it the demand a twin seat there would have covered.
///
/// The arm keeps everything below: its landing, its post, its head (the
/// head's millimetre geometry is frozen at placement and is not touched). Its
/// bookkeeping gains the cells of the layers it now spans: its `k_top`, the
/// seat cell on its path (leaned or headed arms) and on the head's cell list,
/// and its shaft span and column top in the history, so the down-sweep sees an
/// arm alive on the new layer and routes the twin's load into its contact.
fn raise_host_seat(
    host: &mut SweepStrut,
    history: &mut ColumnHistory,
    layer: usize,
    covered: &[usize],
) {
    let steps = layer - host.k_top;
    let seat = u32::try_from(host.cell).unwrap_or(u32::MAX);
    history.raise_shaft_top(host.id, host.cell, host.k_top, layer);
    if !host.path.is_empty() {
        host.path.extend(core::iter::repeat_n(seat, steps));
    }
    if let Some(head) = host.head.as_mut() {
        head.cells.extend(core::iter::repeat_n(seat, steps));
    }
    host.k_top = layer;
    host.twin_host = true;
    host.covered_demand.extend_from_slice(covered);
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
        keep_out,
        law,
        shaft_landings,
        separation_mm,
        clearance,
        ..
    } = *seating;
    let cols = grid.cols();
    let id = u32::try_from(struts.len()).unwrap_or(u32::MAX);

    // Root the seat in the reach field and clear the demand it covers, then
    // descend FROM what it covers: the drawn solid's radius is bounded below
    // by the area law on that demand. Under the drawn-solid test the rooting
    // runs under the journal so a seat whose every descent enters the model
    // can be undone bit for bit and moved (below); without the test it is
    // the placement this engine always made.
    let mut cell = cell;
    if clearance.is_some() {
        reach.begin_journal();
    }
    let mut covered = root_and_clear(seating, cell, reach, scratch);

    // **ONE SUPPORT PER SPIKE** (owner ruling 2026-10-10, `DEINO-DIAGNOSIS.md`).
    // A sharp tip first appears as a hairline the raster covers but does not
    // carry, so the next layer, the first that carries, finds its column
    // unrooted and seats a second arm one layer above the first, 0.03-0.09 mm
    // from it. The second arm's shaft then stands in the first arm's way (the
    // head check refuses it `HitsMember`) and leans away on its own. When this
    // seat's tip ball overlaps the ball of an arm already seated in this very
    // column, there is one contact, not two: that arm RISES to this layer and
    // takes this seat's demand, so the down-sweep finds an arm alive on this
    // layer and sizes its contact for both loads. (Dropping the seat without
    // raising the arm was built and measured first: it leaves this layer with
    // no arm alive on it, and 75 mm2 of load on the Deinonychus reached no
    // carrier.)
    if let Some(clear) = clearance.filter(|_| scratch.twin_merge) {
        if let Some(host) = twin_host(
            seating,
            clear,
            &struts[..],
            history,
            &scratch.twin_first_seat,
            cell,
            covered.len(),
        ) {
            reach.end_journal();
            let (mut dirty_lo, mut dirty_hi) = (usize::MAX, 0_usize);
            for &touched in &covered {
                let row = touched / cols;
                dirty_lo = dirty_lo.min(row);
                dirty_hi = dirty_hi.max(row);
            }
            let host_strut = &mut struts[host as usize];
            scratch.twin_first_seat.entry(host).or_insert(host_strut.k_top);
            raise_host_seat(host_strut, history, layer, &covered);
            scratch.twin_merges += 1;
            return (dirty_lo, dirty_hi);
        }
    }
    let mut trial = LayerOutcome::empty();
    let mut descent = descend_cleared(
        grid,
        history,
        keep_out,
        law,
        shaft_landings,
        separation_mm,
        cell,
        layer,
        &mut scratch.lean_candidates,
        &mut trial,
        clearance,
        clearance.map_or(0.0, |c| {
            c.radius_lower_bound_mm(covered.len() as f64 * grid.cell_area_mm2())
        }),
    );
    if let (Some(clear), Clears::Neither) = (clearance, descent.clears) {
        // **THE SEAT MAY MOVE, BUT ONLY TO A CELL THAT COVERS THE SAME
        // DEMAND** (Craig, R1 of `clearance-census-diagnosis.md`): nothing
        // that this seat roots is left unrooted by the move.
        let first_covered = covered.clone();
        undo_rooting(reach, scratch, &covered);
        let mut moved = false;
        let lower_bound = clear.radius_lower_bound_mm(first_covered.len() as f64 * grid.cell_area_mm2());
        for candidate in reseat_cells(seating, cell) {
            // The cheap necessary test first, on the straight descent alone
            // (a lean search per candidate cell is thousands of tests per
            // seat): a descent that enters the model at the radius the FIRST
            // seat's demand needs enters it at any larger one, and the
            // covering is only measured for a cell whose straight descent
            // clears the final gap.
            let (landing_there, bottom_there, _) = descend(history, candidate, layer);
            let read = clear.read(grid, history);
            if read
                .intrusion(
                    (candidate, layer),
                    read.seat_face((candidate, layer)),
                    (candidate, bottom_there),
                    matches!(landing_there, Landing::Model { .. }),
                    lower_bound,
                    clear.final_gap_mm,
                )
                .is_some()
            {
                continue;
            }
            reach.begin_journal();
            let covered_there = root_and_clear(seating, candidate, reach, scratch);
            let same_demand = first_covered.iter().all(|&c| reach.is_rooted(c));
            if same_demand {
                let mut there = LayerOutcome::empty();
                let moved_descent = descend_cleared(
                    grid,
                    history,
                    keep_out,
                    law,
                    shaft_landings,
                    separation_mm,
                    candidate,
                    layer,
                    &mut scratch.lean_candidates,
                    &mut there,
                    Some(clear),
                    clear.radius_lower_bound_mm(covered_there.len() as f64 * grid.cell_area_mm2()),
                );
                if moved_descent.clears != Clears::Neither {
                    cell = candidate;
                    covered = covered_there;
                    descent = moved_descent;
                    trial = there;
                    trial.solid.seats_moved += 1;
                    moved = true;
                    break;
                }
            }
            undo_rooting(reach, scratch, &covered_there);
        }
        if !moved {
            // No such cell clears either: keep the seat, WARN (the census
            // names it). Re-root where it stood.
            reach.begin_journal();
            covered = root_and_clear(seating, cell, reach, scratch);
        }
    }
    if clearance.is_some() {
        reach.end_journal();
    }

    // **THE ARTICULATED HEAD** (Craig 2026-10-06), now that the seat cell and
    // what it covers are final and before anything rests on the arm: on a steep
    // face the arm's top is a short straight head along the footprint normal to
    // a joint sphere, and the post descends from the joint. Where the head
    // cannot be built the arm keeps today's straight tip, counted by reason;
    // `NotSteep` and a face too flat to ask are not counted at all. A shaft
    // that would graze the model on a gentler face is converted later, where
    // layout can still move the post ([`crate::graze`]).
    let mut head_built: Option<Head> = None;
    if clearance.is_some_and(|clear| clear.heads.is_some()) {
        let lower_bound = clearance.map_or(0.0, |clear| {
            clear.radius_lower_bound_mm(covered.len() as f64 * grid.cell_area_mm2())
        });
        match plan_seat_head(seating, history, &scratch.heads.accepted, cell, lower_bound) {
            Err(HeadExit::Skipped(seat)) => {
                scratch.heads.outcome.skips.entry(seat.reason).or_default().push(id);
                scratch.heads.outcome.skipped_seats.push(SkippedSeat { id, ..seat });
            }
            Err(HeadExit::Refused(reason)) => {
                scratch.heads.outcome.refusals.entry(reason).or_default().push(id);
            }
            Ok((head, residual_deg)) => {
                // The post, from the joint, against the history BELOW the joint
                // (the running state holds the model and the other seats above
                // it), read as a solid of the joint's own radius.
                let mut post_trial = LayerOutcome::empty();
                let head_margin_mm = law.layer_height_mm()
                    + crate::member_clearance::raster_half_diagonal_mm(grid);
                let post = descend_cleared_from(
                    grid,
                    history,
                    keep_out,
                    law,
                    shaft_landings,
                    separation_mm,
                    head.joint_cell as usize,
                    head.k_joint,
                    layer,
                    &mut scratch.lean_candidates,
                    &mut post_trial,
                    clearance,
                    head.joint_radius_mm,
                    Some(&PostPlane {
                        joint_mm: head.joint_mm,
                        contact_mm: head.contact_mm,
                        unit: head.unit,
                        // The post is never drawn thinner than the joint, and the
                        // joint itself grows with the final tip (frozen at K,
                        // wider only): the sizing may draw the post anywhere
                        // up to the registered maximum radius, so a leaning
                        // post is admitted only if even that stays off the
                        // plane (measured: a post leaning back at 80 degrees to
                        // the normal, its joint grown 0.22 to 0.53 mm, was cut
                        // by the trim on the ball fixture).
                        radius_mm: clearance
                            .and_then(|clear| clear.heads)
                            .map_or(head.joint_radius_mm, |env| {
                                env.rule.max_radius_mm.max(head.joint_radius_mm)
                            }),
                        plate_z_mm: clearance.map_or(0.0, |clear| clear.plate_z_mm),
                        margin_mm: head_margin_mm,
                    }),
                    // The post's foot passes at least the seat's own load:
                    // the area law on what the seat covers, not the joint.
                    lower_bound,
                );
                if post.clears == Clears::Neither && descent.clears != Clears::Neither {
                    scratch
                        .heads
                        .outcome
                        .refusals
                        .entry(HeadRefusal::PostUncleared)
                        .or_default()
                        .push(id);
                } else {
                    scratch.heads.accepted.push(HeadCapsule::of(&head));
                    scratch.heads.outcome.built += 1;
                    scratch.heads.residuals_deg.push(residual_deg);
                    descent = post;
                    trial = post_trial;
                    head_built = Some(head);
                }
            }
        }
    }
    outcome.absorb_descent(&trial);
    let Descent {
        landing,
        k_bottom,
        rooted: rooted_landing,
        path,
        ..
    } = descent;
    if !rooted_landing {
        outcome.unrooted_landings += 1;
    }

    // Everything this insertion newly rooted left the demand set and is
    // recorded (`covered`), so the removal pass can ask later whether a
    // survivor still covers it.
    let mut dirty_lo = usize::MAX;
    let mut dirty_hi = 0_usize;
    for &touched in &covered {
        let row = touched / cols;
        dirty_lo = dirty_lo.min(row);
        dirty_hi = dirty_hi.max(row);
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
        twin_host: false,
        inserted_by_load: false,
        // Recorded ONLY for a leaned arm, and recorded NOW, because the column
        // history this reads is a running state that will not say the same
        // thing again once the sweep has passed this layer.
        seat_face: crate::SeatFace::default(),
        seat_normal: [0.0; 3],
        fallback_path: Vec::new(),
        head: head_built.clone(),
        // A headed arm's fallback is the straight post from its JOINT, post
        // only (`set_post` puts the head slots back on a restore).
        straight_fallback: (!path.is_empty()).then(|| {
            let (landing, bottom, _, _) = match &head_built {
                Some(head) => descend_below(history, head.joint_cell as usize, head.k_joint, layer),
                None => descend_below(history, cell, layer, layer),
            };
            (landing, bottom)
        }),
        path,
    });
    if let Some(head) = &head_built {
        // The post runs from the joint: the descent's cells stop one layer
        // under it, so the joint's own cell closes them, and `set_post` follows
        // them with the head's cells up to the seat (a straight post is the
        // joint cell all the way down).
        let placed = struts.last_mut().expect("the arm just pushed");
        let mut post = core::mem::take(&mut placed.path);
        if !post.is_empty() {
            post.push(head.joint_cell);
        }
        placed.set_post(landing, k_bottom, post);
    }
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
    // arm's own path is read in place. A headed arm stamps its post landable
    // only up to its joinable top, and its head as blocking only.
    let placed = struts.last().expect("the arm just pushed");
    history.stamp_strut(placed, clearance.map_or(0.0, |clear| clear.layer_height_mm));
    (dirty_lo, dirty_hi)
}

/// Root a seat at `cell` in the reach field and take everything it newly
/// roots out of the demand set; returns those cells (what the seat covers).
fn root_and_clear(
    seating: &Seating<'_>,
    cell: usize,
    reach: &mut ReachState,
    scratch: &mut GreedyScratch,
) -> Vec<usize> {
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
    let mut covered = Vec::new();
    for &touched in &scratch.touched {
        if scratch.demand_bits[touched] && reach.is_rooted(touched) {
            scratch.demand_bits[touched] = false;
            covered.push(touched);
        }
    }
    covered
}

/// Undo [`root_and_clear`] bit for bit: the journal puts the reach field
/// back and the covered cells return to the demand set.
fn undo_rooting(reach: &mut ReachState, scratch: &mut GreedyScratch, covered: &[usize]) {
    reach.undo_journal();
    for &cell in covered {
        scratch.demand_bits[cell] = true;
    }
}

/// The cells a seat that has no clear descent may move to, nearest first
/// (ties by cell index): model cells of this layer, admitted by the drain
/// keep-out, within HALF the covering's seat spacing of where it stood
/// (`separation_mm / 2`, the covering's own geometry: one step further and
/// the seat is nearer its neighbour's than its own). The caller keeps only
/// a cell that covers the same demand.
fn reseat_cells(seating: &Seating<'_>, cell: usize) -> Vec<usize> {
    let Seating {
        grid,
        layer,
        cover,
        keep_out,
        separation_mm,
        ..
    } = *seating;
    let radius_mm = 0.5 * separation_mm;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a non-negative finite length divided by the grid pitch, floored to a loop bound"
    )]
    let radius_cells = (radius_mm / grid.pitch_mm()).floor().max(0.0) as usize;
    let (row, col) = (grid.row_of(cell), grid.col_of(cell));
    let mut found: Vec<(f64, usize)> = Vec::new();
    for r in row.saturating_sub(radius_cells)..=(row + radius_cells).min(grid.rows() - 1) {
        for c in col.saturating_sub(radius_cells)..=(col + radius_cells).min(grid.cols() - 1) {
            let candidate = r * grid.cols() + c;
            if candidate == cell || !cover[candidate] || !keep_out.admits(candidate, layer) {
                continue;
            }
            let lateral = crate::cell_distance_mm(grid, cell, candidate);
            if lateral <= radius_mm {
                found.push((lateral, candidate));
            }
        }
    }
    found.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    found.into_iter().map(|(_, candidate)| candidate).collect()
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

/// A seat for a pivot [`best_seat_for`] found NO seat for: every demand cell
/// in reach of it, the pivot included, stands in a drain's escape cylinder,
/// or outside the ridge band. `None` when there is still none.
///
/// # Why a rooted host is allowed here, and only here (2026-10-01)
///
/// [`best_seat_for`] seats only on DEMAND, because a host that is already
/// rooted is legal but wasted ("four arms inside a leg's own self-rooted
/// fringe", `make_overhang_table`). That rule assumes the demand it leaves
/// behind can still be reached from demand. Beside a drain it cannot: the
/// keep-out is `bore + widest arm + standoff` in radius (2.85 mm on the
/// Cylinder) against a reach of 2.13 mm, the ring of cells just outside it is
/// rooted by the first seats on the layer, and the cells inside it that a seat
/// on that ring WOULD reach are then left BARE although a legal arm reaches
/// them. Measured on the Cylinder at develop `3372093`: 1,555 ceiling cells
/// (3.89 mm2) bare on the layer the cavity closes over its drain, carried up 55
/// layers, with the potential the only thing that covered more of the disc —
/// through arms the creep weight's tail happened to seat lower down, which the
/// overhang cut removes (`lanes/creep-45.md`). Here the pivot itself is the
/// demand the arm is for, so the host need not be.
///
/// The host must still be a legal seat (`cover`, admitted by the keep-out),
/// and either demand or material that did NOT carry one layer down: a real
/// underside, never a cell inside solid material whose arm would land on the
/// model one layer below it. Choice and tie-breaks are [`best_seat_for`]'s.
/// No keep-out, no change: without drains the pivot is always admitted, so
/// [`best_seat_for`] never returns `None` and this never runs.
#[expect(
    clippy::too_many_arguments,
    reason = "the same inputs as `best_seat_for`, plus the carried-column field; a struct would \
          move the list rather than shorten it"
)]
fn stranded_seat_for(
    grid: &Grid,
    cover: &[bool],
    history: &ColumnHistory,
    scratch: &GreedyScratch,
    keep_out: &DrainKeepOut,
    layer: usize,
    pivot: usize,
    carried: &[f32],
) -> Option<usize> {
    let cols = grid.cols();
    let rows = grid.rows();
    let reach = scratch.reach_cells;
    let pivot_row = pivot / cols;
    let pivot_col = pivot % cols;
    let mut best: Option<usize> = None;
    let mut best_key = (0_u32, false, 0_u32);
    for row in pivot_row.saturating_sub(reach)..=(pivot_row + reach).min(rows - 1) {
        let half = scratch.half_widths[row.abs_diff(pivot_row)];
        for col in pivot_col.saturating_sub(half)..=(pivot_col + half).min(cols - 1) {
            let candidate = row * cols + col;
            // `carried` is finite exactly where the column carried one layer
            // down ([`crate::reach::commit_layer`]).
            if !(cover[candidate]
                && keep_out.admits(candidate, layer)
                && (scratch.demand_bits[candidate] || !carried[candidate].is_finite()))
            {
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
#[cfg(test)]
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
    let descent = descend_cleared(
        grid,
        history,
        keep_out,
        law,
        shaft_landings,
        search_radius_mm,
        cell,
        layer,
        scratch,
        outcome,
        None,
        0.0,
    );
    (descent.landing, descent.k_bottom, descent.rooted, descent.path)
}

/// [`descend_leaning`] with the drawn-solid test (fix/drawn-solid-clearance,
/// 2026-10-04, `clearance-census-diagnosis.md` R1).
///
/// With `clearance` given, EVERY candidate descent, the straight one
/// included, is read as the solid the file will draw: its straight axis,
/// outside its own seat zone (and landing zone on the model), at a LOWER
/// BOUND `radius_lb_mm` on its drawn radius, against the cured model, at
/// Craig's two tiers (2026-08-22). Candidates are taken in the order they
/// always were (cheapest first, a tie to the straight descent). A straight
/// descent that clears the FINAL gap is exactly the descent this function
/// always returned (a leaned one replaces it only as before, and only if it
/// clears the final gap too): the 0.6 mm margin is a placement-family search
/// margin, and only the final gap refuses geometry. Where the straight
/// descent does not clear the final gap, a costlier lean is admitted (it
/// would not be priced before) and the PREFERRED margin chooses: a candidate
/// that clears 0.6 mm beats any that clears only the final gap, which beats
/// none. `Clears::Neither` means no descent clears even the final gap: the
/// straight one is returned and the caller may move the seat.
#[expect(
    clippy::too_many_arguments,
    reason = "the descent reads the grid, the history, the keep-out, the law, its own seat and \
          layer, its search bound, the outcome it reports into, and the clearance with its \
          radius; a struct would move the argument list rather than shorten it"
)]
fn descend_cleared(
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
    clearance: Option<&DescentClearance<'_>>,
    radius_lb_mm: f64,
) -> Descent {
    descend_cleared_from(
        grid,
        history,
        keep_out,
        law,
        shaft_landings,
        search_radius_mm,
        cell,
        layer,
        layer,
        scratch,
        outcome,
        clearance,
        radius_lb_mm,
        None,
        // A seat's own descent: its drawn lower bound IS the area law on what
        // it covers, the least load its foot can pass.
        radius_lb_mm,
    )
}

/// The seat plane an articulated head's post must stay on the FREE side of.
///
/// The seat trim cuts away everything of the post that lies beyond the seat
/// plane (through the contact `D`, normal `u`), so a post that leans back under
/// the face must not be chosen: it would be cut off its own joint. A lean
/// candidate is admitted only if its drawn base, with the post's radius about
/// the axis, stands on the free side with a margin of one layer and half a cell
/// (the drawn post is that far off its grid route). The straight post under the
/// joint always stands on the free side (it moves along `-z`, and `u_z < 0`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct PostPlane {
    /// The joint `K`, exact, in the lowering's frame.
    pub(crate) joint_mm: [f64; 3],
    /// The contact centre `D`.
    pub(crate) contact_mm: [f64; 3],
    /// The outward normal `u` of the seat plane.
    pub(crate) unit: [f64; 3],
    /// An UPPER bound on the post's radius (the registered maximum): the
    /// joint grows with the final tip and the sizing may thicken the post
    /// toward its base, and a ring of that radius must still stand off the
    /// plane.
    pub(crate) radius_mm: f64,
    /// The plate's z in the lowering's frame (the grid's axis points have it at 0).
    pub(crate) plate_z_mm: f64,
    /// The margin the base keeps from the plane.
    pub(crate) margin_mm: f64,
}

impl PostPlane {
    /// Whether a post from the joint to the axis point `base` (raster frame:
    /// the plate at z = 0) stays on the free side of the plane.
    pub(crate) fn admits(&self, base: [f64; 3]) -> bool {
        use geom_kernel::vec3;
        let base = [base[0], base[1], base[2] + self.plate_z_mm];
        let to_base = vec3::sub(base, self.joint_mm);
        let Some(axis) = vec3::normalize(to_base) else {
            return true;
        };
        let along = vec3::dot(axis, self.unit);
        let across = (1.0 - along * along).max(0.0).sqrt();
        let height = vec3::dot(vec3::sub(base, self.contact_mm), self.unit);
        height - self.radius_mm * across >= self.margin_mm
    }
}

/// [`descend_cleared`] from a START that need not be the layer being placed.
///
/// `(cell, layer)` is where the descent starts: the seat for today's arms, and
/// the joint `(joint_cell, k_joint)` for the post of an articulated head.
/// `running_layer` is the layer the running column history stands at (the
/// layer being placed). When the two are equal every read is the running state,
/// bit for bit as before. When the start is LOWER, the running state holds
/// material ABOVE the start (other seats of this layer, the model between the
/// joint and the seat), which a post that begins at the joint must not land
/// on, so every history read goes through [`model_top_below`],
/// [`strut_top_below`] and [`ColumnHistory::model_blocks`], which answer for
/// the layers below the start only. The seat face is not read for such a
/// start: a post's contact zone at its joint is zero.
///
/// **A MODEL LANDING MUST TAKE ITS WHOLE FOOT** (lane 2, 2026-10-12,
/// `MODEL-FEET.md` F2): `foot_radius_mm` is a lower bound on the contact the
/// arm's foot must deliver (the area law on what the seat covers), and a
/// landing on the model is accepted only when that disc lies wholly on cured
/// model ([`foot_on_model`]). A lean candidate whose foot does not fit is not
/// a candidate. A straight descent whose foot does not fit is treated as one
/// that does not clear: the search takes the cheapest candidate it priced
/// that does fit (a costlier one is admitted, exactly as for a straight
/// descent the drawn-solid test refuses), and where there is none the
/// straight landing is kept with its own clearance verdict, counted, and
/// warned on the finished field with the percent on the model. `0` tests the
/// axis cell alone, which every model landing stands on by construction.
#[expect(
    clippy::too_many_arguments,
    reason = "the descent reads the grid, the history, the keep-out, the law, its own start, \
          the running layer, its search bound, the outcome it reports into, the clearance \
          with its radius, and its foot; a struct would move the argument list rather than \
          shorten it"
)]
fn descend_cleared_from(
    grid: &Grid,
    history: &ColumnHistory,
    keep_out: &DrainKeepOut,
    law: &LeanLaw,
    shaft_landings: bool,
    search_radius_mm: f64,
    cell: usize,
    layer: usize,
    running_layer: usize,
    scratch: &mut Vec<LeanCandidate>,
    outcome: &mut LayerOutcome,
    clearance: Option<&DescentClearance<'_>>,
    radius_lb_mm: f64,
    post_plane: Option<&PostPlane>,
    foot_radius_mm: f64,
) -> Descent {
    let from_joint = running_layer != layer;
    let (straight_landing, straight_bottom, straight_rooted, head_blocked) =
        descend_below(history, cell, layer, running_layer);
    let foot_fits = |base_cell: usize, k_bottom: usize| {
        let (on, total) = foot_on_model(grid, history, base_cell, k_bottom, foot_radius_mm);
        on == total
    };
    let foot_misfit =
        matches!(straight_landing, Landing::Model { .. }) && !foot_fits(cell, straight_bottom);
    let straight = |clears: Clears| Descent {
        landing: straight_landing,
        k_bottom: straight_bottom,
        rooted: straight_rooted,
        path: Vec::new(),
        clears,
    };
    // How well a candidate's axis (seat at `layer`, base at `at` in
    // `base_cell`) clears the model, at one tier.
    //
    // **A TIER IMPLIES THE FINAL GAP** (challenger round 2, 2026-10-04): the
    // seat and landing zones a shaft is excused grow with the gap asked of it
    // (`zone_factor · (r + gap)`), so a short descent can lie wholly inside its
    // zones at the 0.6 mm preferred tier and still have a testable stretch at
    // the 0.3 mm final one. A stricter tier therefore passes only a descent
    // that also clears the final gap at the final gap's own zones.
    //
    // **THE ZONES ARE THE SEAT'S AND THE LANDING'S OWN** (challenger round 3):
    // the face this seat stands against is named once (one nearest-point
    // query), and each candidate's landing face once per candidate that the
    // raster reads ([`crate::member_clearance::ShaftRead::intrusion`]).
    let seat_face = if from_joint {
        None
    } else {
        clearance.and_then(|clear| clear.read(grid, history).seat_face((cell, layer)))
    };
    // Whether the PROPOSED descent, the straight axis from this seat down to
    // `base_cell` at `k_bottom`, with a solid of the lower-bound radius about
    // it, stands wholly in hidden voxels of the cavity map.
    let in_cavity = |clear: &DescentClearance<'_>, base_cell: usize, k_bottom: usize| {
        history.hidden.segment_hidden(
            crate::member_clearance::axis_point(grid, clear.layer_height_mm, cell, layer),
            crate::member_clearance::axis_point(grid, clear.layer_height_mm, base_cell, k_bottom),
            radius_lb_mm,
        )
    };
    let blocked = |clear: &DescentClearance<'_>,
                   gap_mm: f64,
                   base_cell: usize,
                   k_bottom: usize,
                   landing: Landing| {
        let read = clear.read(grid, history);
        let enters = |tier_mm: f64| {
            read.intrusion(
                (cell, layer),
                seat_face,
                (base_cell, k_bottom),
                matches!(landing, Landing::Model { .. }),
                radius_lb_mm,
                tier_mm,
            )
            .is_some()
        };
        // **THE FINAL GAP DOES NOT APPLY INSIDE A HIDDEN CAVITY** (Craig
        // ruling 2026-10-05, on his 2026-08-31 ruling that supports hidden in
        // a cavity are never removed or cleaned; challenger, same day). The
        // cavity map is built from all the slices before the up-sweep starts,
        // so it answers for the PROPOSED descent now: a straight axis from
        // this seat to the landing, with the solid at the lower bound of its
        // radius about it, wholly in hidden voxels is not refused for the
        // gap. The model-crossing and drain checks are not this test's:
        // `descend_cleared` keeps its path test (`shaft_blocks`) and the
        // drain keep-out for every candidate, and a descent that enters the
        // model is not in enclosed air by construction. Outside a cavity
        // nothing changes. The PREFERRED margin is a placement search margin
        // and is still read.
        let at_final = || !in_cavity(clear, base_cell, k_bottom) && enters(clear.final_gap_mm);
        if gap_mm > clear.final_gap_mm {
            at_final() || enters(gap_mm)
        } else if gap_mm < clear.final_gap_mm {
            enters(gap_mm)
        } else {
            at_final()
        }
    };
    let straight_layers = layer.saturating_sub(straight_bottom);
    let Some(straight_length) = law.descent_cost_mm(straight_layers, 0.0) else {
        return straight(Clears::Untested);
    };
    let straight_cost =
        straight_length + deferred_path_mm(straight_landing, straight_bottom, law.layer_height_mm());
    // The straight descent's own verdict, at both tiers (one test when it
    // clears the preferred margin).
    let straight_clears = clearance.map(|clear| {
        outcome.solid.tested += 1;
        if in_cavity(clear, cell, straight_bottom) {
            outcome.solid.in_hidden_cavity += 1;
        }
        if !blocked(clear, clear.preferred_gap_mm, cell, straight_bottom, straight_landing) {
            Clears::Preferred
        } else {
            outcome.solid.inside_preferred_margin += 1;
            if !blocked(clear, clear.final_gap_mm, cell, straight_bottom, straight_landing) {
                Clears::FinalOnly
            } else {
                outcome.solid.refused_at_final_gap += 1;
                Clears::Neither
            }
        }
    });
    // **A STRAIGHT DESCENT THAT CLEARS THE FINAL GAP STAYS** (Craig's ruling,
    // 2026-08-22: the 0.6 mm margin is a placement-family SEARCH margin, only
    // the 0.3 mm final gap refuses geometry; and the Round discs, whose posts
    // stand 0.3 to 0.6 mm from the ring's walls, are the byte-identity
    // control of this rule). The preferred margin chooses among the
    // alternatives only where the straight descent has to be replaced.
    //
    // **A HEAD BLOCKS THE STRAIGHT DESCENT** (articulated heads, 2026-10-06): a
    // straight descent that would land on an earlier headed arm ABOVE its
    // joinable top is refused, so it is not "clear" and a lean candidate is
    // taken instead. With none, the straight landing stands (the ledger reports
    // it as a join above the joint, never a refusal).
    let straight_clear_but_foot =
        !head_blocked && straight_clears.is_none_or(|clears| clears != Clears::Neither);
    let straight_is_clear = straight_clear_but_foot && !foot_misfit;
    if !law.leans() {
        outcome.solid.foot_straight_kept += u64::from(foot_misfit);
        return straight(straight_clears.unwrap_or(Clears::Untested));
    }

    // The box the two bounds above allow. `straight_cost` is the straight
    // descent's length, and no cheaper descent can travel further sideways
    // than that. Where the straight descent itself does not clear, nothing
    // bounds the cost of the alternative by it any more.
    let radius_mm = if straight_is_clear {
        search_radius_mm.min(straight_cost)
    } else {
        search_radius_mm
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a non-negative finite length divided by the grid pitch, floored to a loop bound"
    )]
    let radius_cells = (radius_mm / grid.pitch_mm()).floor().max(0.0) as usize;
    if radius_cells == 0 {
        outcome.solid.foot_straight_kept += u64::from(foot_misfit);
        return straight(straight_clears.unwrap_or(Clears::Untested));
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
            let model_top = model_top_below(history, candidate, layer, running_layer);
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
                let (strut_top, strut_top_id) =
                    strut_top_below(history, candidate, layer, running_layer);
                if strut_top > 0 && strut_top >= model_top {
                    let top = strut_top as usize - 1;
                    ((top <= ceiling).then_some((strut_top_id, top, top)), None)
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
                        model_rooted_below(history, candidate, layer, running_layer, model_top),
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
            // A head's post never leans back across its own seat plane: the
            // seat trim would cut it off the joint.
            if let Some(plane) = post_plane {
                #[expect(clippy::cast_precision_loss, reason = "a layer index is exact in f64")]
                let base = [
                    grid.center_x_mm(grid.col_of(candidate)),
                    grid.center_y_mm(grid.row_of(candidate)),
                    k_bottom as f64 * law.layer_height_mm(),
                ];
                if !plane.admits(base) {
                    continue;
                }
            }
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
            if straight_is_clear && bare_mm >= straight_cost {
                continue;
            }
            let Some(length_mm) = law.descent_cost_mm(descent_layers, lateral_mm) else {
                continue;
            };
            let cost_mm = length_mm + deferred_path_mm(landing, k_bottom, law.layer_height_mm());
            outcome.lean_candidates_priced += 1;
            if straight_is_clear && cost_mm >= straight_cost {
                continue;
            }
            // A model landing takes its whole foot or is not a candidate.
            if matches!(landing, Landing::Model { .. }) && !foot_fits(candidate, k_bottom) {
                outcome.solid.foot_candidates_refused += 1;
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
        outcome.solid.foot_straight_kept += u64::from(foot_misfit);
        return straight(straight_clears.unwrap_or(Clears::Untested));
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

    let mut path_blocked = false;
    // Tiers, best first. Without the clearance there is the one pass this
    // search always made. With it, a pass takes only candidates that clear
    // its gap; the straight descent stands in the order at its own cost, a
    // tie going to it, and wins the pass it clears.
    let tiers: &[(f64, Clears)] = match clearance {
        None => &[(0.0, Clears::Untested)],
        // The straight descent clears the final gap and stays; a cheaper lean
        // replaces it only if it clears the final gap too.
        Some(clear) if straight_is_clear => &[(clear.final_gap_mm, Clears::FinalOnly)],
        Some(clear) => &[
            (clear.preferred_gap_mm, Clears::Preferred),
            (clear.final_gap_mm, Clears::FinalOnly),
        ],
    };
    let mut paths: Vec<Option<Option<Vec<u32>>>> = vec![None; scratch.len()];
    for (tier_index, &(gap_mm, tier)) in tiers.iter().enumerate() {
        // The straight descent has been read already: it passes every tier
        // of this list or none (see `tiers`).
        let straight_passes = straight_is_clear;
        for (index, candidate) in scratch.iter().enumerate() {
            // The straight descent stands in the order at its own cost: a
            // candidate no cheaper than it comes after it, and it wins the
            // pass it clears. (Without the clearance every candidate is
            // cheaper, so this never fires.)
            if clearance.is_some() && straight_passes && candidate.cost_mm >= straight_cost {
                break;
            }
            let path = paths[index]
                .get_or_insert_with(|| {
                    lean_path_from(grid, history, keep_out, layer, running_layer, cell, candidate)
                })
                .as_ref();
            let Some(path) = path else {
                path_blocked = true;
                continue;
            };
            if let Some(clear) = clearance {
                if blocked(clear, gap_mm, candidate.cell, candidate.k_bottom, candidate.landing) {
                    if tier_index == 0 {
                        outcome.solid.lean_candidates_refused += 1;
                    }
                    continue;
                }
            }
            outcome.arms_leaned += 1;
            if candidate.class == 0 {
                outcome.arms_merged += 1;
                if !candidate.at_parent_top {
                    outcome.junctions_mid_shaft += 1;
                }
            }
            if !straight_clear_but_foot && clearance.is_some() {
                match tier {
                    Clears::FinalOnly => outcome.solid.at_final_gap_only += 1,
                    _ => outcome.solid.cleared_by_lean += 1,
                }
            }
            outcome.solid.foot_straight_replaced += u64::from(foot_misfit);
            return Descent {
                landing: candidate.landing,
                k_bottom: candidate.k_bottom,
                rooted: candidate.rooted,
                path: path.clone(),
                clears: tier,
            };
        }
        if clearance.is_some() && straight_passes {
            if path_blocked {
                outcome.lean_paths_blocked += 1;
            }
            return straight(straight_clears.unwrap_or(Clears::Untested));
        }
    }
    if path_blocked {
        outcome.lean_paths_blocked += 1;
    }
    if (head_blocked || foot_misfit) && straight_clears != Some(Clears::Neither) {
        // No lean candidate either: the straight landing stands (its own
        // clearance verdict), and the landing ledger names the join, or the
        // finished field's foot census the share of the foot on the model.
        outcome.solid.foot_straight_kept += u64::from(foot_misfit);
        return straight(straight_clears.unwrap_or(Clears::Untested));
    }
    if clearance.is_some() {
        outcome.solid.uncleared += 1;
        return straight(Clears::Neither);
    }
    straight(Clears::Untested)
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
#[cfg(test)]
fn lean_path(
    grid: &Grid,
    history: &ColumnHistory,
    keep_out: &DrainKeepOut,
    layer: usize,
    seat_cell: usize,
    candidate: &LeanCandidate,
) -> Option<Vec<u32>> {
    lean_path_from(grid, history, keep_out, layer, layer, seat_cell, candidate)
}

/// [`lean_path`] from a start `(seat_cell, layer)` under a running history at
/// `running_layer` (see [`descend_cleared_from`]). At the running layer the
/// model test is the running `top_model` ("cured at or above here"); under a
/// lower start, where `top_model` also holds material ABOVE the start, it is
/// the sharp question [`ColumnHistory::model_blocks`] ("cured exactly here").
fn lean_path_from(
    grid: &Grid,
    history: &ColumnHistory,
    keep_out: &DrainKeepOut,
    layer: usize,
    running_layer: usize,
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
            let in_model = if layer == running_layer {
                history.top_model[here_cell] as usize > here
            } else {
                history.model_blocks(here_cell, here)
            };
            if in_model {
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
    let (landing, bottom, rooted, _) = descend_below(history, cell, layer, layer);
    (landing, bottom, rooted)
}

/// The topmost model layer + 1 in `cell` that stands BELOW `start`, with the
/// history running at `running_layer`; 0 means none.
///
/// At the running layer this is the running state `top_model[cell]`, exactly
/// as before. Under a lower start (the joint of an articulated head) the
/// running state also holds the model between the joint and the seat, which a
/// post that begins at the joint must not land on, so the answer is read from
/// the column's cured runs: the top of the highest run that tops out under
/// `start`.
fn model_top_below(history: &ColumnHistory, cell: usize, start: usize, running: usize) -> u32 {
    if start == running {
        return history.top_model[cell];
    }
    start
        .checked_sub(1)
        .and_then(|below| history.model_landing_at_or_below(cell, below))
        .map_or(0, |top| u32::try_from(top + 1).unwrap_or(u32::MAX))
}

/// The topmost strut layer + 1 in `cell` below `start` and its owner (0 and 0
/// for none), with the history running at `running_layer`.
///
/// At the running layer: the running `top_strut`, as before. Under a lower
/// start: the highest LANDABLE layer of any shaft below `start`
/// ([`ColumnHistory::shaft_landing`]), which keeps a headed host's joint and
/// head out of reach.
fn strut_top_below(history: &ColumnHistory, cell: usize, start: usize, running: usize) -> (u32, u32) {
    if start == running {
        return (history.top_strut[cell], history.top_strut_id[cell]);
    }
    start
        .checked_sub(1)
        .and_then(|below| history.shaft_landing(cell, below))
        .map_or((0, 0), |(id, layer, _)| {
            (u32::try_from(layer + 1).unwrap_or(u32::MAX), id)
        })
}

/// Whether the topmost model cell below `start` was itself rooted. The running
/// flag belongs to the running top; under a lower start it is read only where
/// that top IS the model landing, and is `false` (warn, never hide) where the
/// landing is a lower run whose own flag was not kept.
fn model_rooted_below(
    history: &ColumnHistory,
    cell: usize,
    start: usize,
    running: usize,
    landing_top: u32,
) -> bool {
    if start == running {
        history.top_model_rooted[cell]
    } else {
        history.top_model[cell] == landing_top && history.top_model_rooted[cell]
    }
}

/// Whether a running-state landing on `owner` at `layer` meets a HEAD: the
/// owner is headed and `layer` is above its joinable top
/// ([`ColumnHistory::joinable_tops`]; no joinable layer at all blocks all).
fn lands_on_head(history: &ColumnHistory, owner: u32, layer: usize) -> bool {
    !history.joinable_tops.is_empty()
        && history
            .joinable_tops
            .get(&owner)
            .is_some_and(|top| top.is_none_or(|joinable| layer > joinable))
}

/// [`descend`] from `start` under a history running at `running`, and whether
/// the straight landing is BLOCKED by a headed arm (a running-state strut hit
/// above its owner's joinable top; never for a start below the running layer,
/// whose shaft reads are landable-only already).
fn descend_below(
    history: &ColumnHistory,
    cell: usize,
    start: usize,
    running: usize,
) -> (Landing, usize, bool, bool) {
    let (strut_top, strut_id) = strut_top_below(history, cell, start, running);
    let model_top = model_top_below(history, cell, start, running);
    if strut_top > 0 && strut_top >= model_top {
        let landing_layer = strut_top as usize - 1;
        return (
            Landing::Strut {
                id: strut_id,
                layer: landing_layer,
            },
            landing_layer,
            true,
            start == running && lands_on_head(history, strut_id, landing_layer),
        );
    }
    if model_top > 0 {
        let landing_layer = model_top as usize - 1;
        return (
            Landing::Model {
                layer: landing_layer,
            },
            landing_layer,
            model_rooted_below(history, cell, start, running, model_top),
            false,
        );
    }
    debug_assert!(start > 0, "a seat on layer 0 cannot descend to the plate");
    (Landing::Plate, 0, true, false)
}

/// **How much of a model foot's contact stands on cured model** (lane 2,
/// 2026-10-12, `MODEL-FEET.md` F2): `(cells on the model, cells in the disc)`
/// for the disc of `radius_mm` about `cell`'s centre at the landing `layer`,
/// measured on the column history's cured runs, never assumed from the axis
/// cell alone.
///
/// A cell is in the disc when its centre is (the axis cell always is; cells
/// past the raster's edge count, and are never on the model). It is ON THE
/// MODEL when it is cured at the landing layer or at the layer below: the
/// foot ball is seated by the seat law at least one layer plus one pixel into
/// the face (`contact::rasterisation_allowance_mm`), so a surface one layer
/// lower than the axis's is still inside the seat. No band is added beyond
/// the law's own layer term.
///
/// The ONE predicate the descent's candidate test, the lip-seat and
/// clearance re-landings and the finished field's census all read, so "the
/// search accepted it" and "the census says it fits" cannot disagree.
#[must_use]
pub(crate) fn foot_on_model(
    grid: &Grid,
    history: &ColumnHistory,
    cell: usize,
    layer: usize,
    radius_mm: f64,
) -> (usize, usize) {
    let pitch = grid.pitch_mm();
    let usable = radius_mm.is_finite() && radius_mm > 0.0 && pitch > 0.0;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a finite non-negative radius over the grid pitch, floored to a loop bound"
    )]
    let reach = if usable {
        (radius_mm / pitch).floor() as usize
    } else {
        0
    };
    let on_model = |here: usize| {
        history.model_blocks(here, layer)
            || layer
                .checked_sub(1)
                .is_some_and(|below| history.model_blocks(here, below))
    };
    if reach == 0 {
        return (usize::from(on_model(cell)), 1);
    }
    let (row, col) = (grid.row_of(cell), grid.col_of(cell));
    // A relative part in a billion: a centre ON the circle is in the disc.
    let limit = radius_mm * radius_mm * (1.0 + 1.0e-9);
    let (mut on, mut total) = (0_usize, 0_usize);
    for dr in 0..=2 * reach {
        for dc in 0..=2 * reach {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a raster offset bounded by the disc's own cell reach is exact in f64"
            )]
            let (dy, dx) = (
                (dr as f64 - reach as f64) * pitch,
                (dc as f64 - reach as f64) * pitch,
            );
            if dx * dx + dy * dy > limit {
                continue;
            }
            total += 1;
            let (Some(r), Some(c)) = ((row + dr).checked_sub(reach), (col + dc).checked_sub(reach))
            else {
                continue;
            };
            if r < grid.rows() && c < grid.cols() && on_model(grid.index(r, c)) {
                on += 1;
            }
        }
    }
    (on, total)
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
    use super::{
        descend_leaning, lean_path, ColumnHistory, DescentClearance, LayerOutcome, LeanCandidate,
    };
    use crate::drain::DrainKeepOut;
    use crate::grid::plan_grid;
    use crate::lean::LeanLaw;
    use crate::Landing;

    const H: f64 = 0.05;

    fn outcome() -> LayerOutcome {
        LayerOutcome::empty()
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

    // -------------------------------------------------------------------
    // fix/drawn-solid-clearance (2026-10-04): the descent reads its drawn solid.
    // -------------------------------------------------------------------

    /// Cure a wall: every cell in `cells` is model over layers `k0..=k1`, as
    /// `commit_layer_history` would have recorded it layer by layer.
    fn cure_wall(history: &mut ColumnHistory, grid: &crate::grid::Grid, cells: &[usize], k0: usize, k1: usize) {
        for layer in k0..=k1 {
            let mut cover = vec![false; grid.cells()];
            for &cell in cells {
                cover[cell] = true;
            }
            history.stamp_model(layer, &cover);
            for &cell in cells {
                history.top_model[cell] = u32::try_from(layer + 1).expect("a small layer index");
            }
        }
    }

    /// The two tiers with no surface: the raster's own flag stands, with the
    /// longest zones (the degraded form; the surface tests are below).
    fn two_tier(law: &LeanLaw) -> DescentClearance<'static> {
        DescentClearance {
            layer_height_mm: H,
            plate_z_mm: 0.0,
            preferred_gap_mm: 0.6,
            final_gap_mm: 0.3,
            tip_radius_min_mm: 0.05,
            x_allowed: 18.0,
            zone_factor: law.face_zone_factor(),
            surface: None,
            heads: None,
        }
    }

    /// THE MECHANISM (R1, `clearance-census-diagnosis.md` section 4), AND ITS
    /// SABOTAGE. A seat 200 layers up, a cured wall 0.35 mm beside its column
    /// from layer 10 to 199. Without the clearance (the retired centreline
    /// descent) the straight descent runs down the wall's whole 9.5 mm at a
    /// distance no solid of radius 0.05 plus the 0.3 mm final gap can hold.
    /// With it the straight descent is refused at the preferred margin AND the
    /// final gap, and the cheapest lean that clears wins even though it costs
    /// more than the straight one; its path then never comes nearer the wall
    /// than the final gap below the seat zone.
    #[test]
    fn a_descent_beside_a_wall_is_tested_at_its_smallest_drawn_radius() {
        // 8 mm square: room to lean 3 mm away from the wall.
        let grid = plan_grid(0.0, 0.0, 8.0, 8.0, 400, H, 0.0, 1_000_000_000).0;
        let mut history = ColumnHistory::new(&grid);
        let seat = grid.index(80, 80);
        // Seven cells = 0.35 mm from the seat's column to the wall's cell centres.
        let wall: Vec<usize> = (60..101).map(|row| grid.index(row, 87)).collect();
        cure_wall(&mut history, &grid, &wall, 10, 199);
        let law = LeanLaw::new(H, 45.0, 25.0);
        let clear = two_tier(&law);
        let mut scratch = Vec::new();

        let mut retired = outcome();
        let (landing, _, _, path) = descend_leaning(
            &grid, &history, &DrainKeepOut::none(), &law, true, 4.0, seat, 200, &mut scratch, &mut retired,
        );
        assert_eq!(landing, Landing::Plate);
        assert!(path.is_empty(), "SABOTAGE: the centreline descent drops straight beside the wall");
        assert_eq!(retired.solid.tested, 0, "and tests no solid");

        let mut counts = outcome();
        let descent = super::descend_cleared(
            &grid, &history, &DrainKeepOut::none(), &law, true, 4.0, seat, 200, &mut scratch, &mut counts,
            Some(&clear), 0.05,
        );
        assert_eq!(counts.solid.tested, 1, "the straight descent's solid was read");
        assert_eq!(counts.solid.refused_at_final_gap, 1, "and refused at the 0.3 mm final gap");
        assert!(!descent.path.is_empty(), "a leaned descent was taken: {counts:?}");
        assert_eq!(descent.landing, Landing::Plate);
        assert_eq!(descent.clears, super::Clears::Preferred, "and it clears the preferred margin");
        assert_eq!(counts.solid.cleared_by_lean, 1);
        // The test's own reading of the path: every slot more than 2 mm under
        // the seat (past any zone the law allows) is at least the final gap
        // plus the smallest radius from the nearest wall cell centre.
        let sx = grid.center_x_mm(87);
        for (slot, &cell) in descent.path.iter().enumerate() {
            let layer = descent.k_bottom + slot;
            if layer + 40 > 200 {
                continue;
            }
            let (x, y) = (
                grid.center_x_mm(grid.col_of(cell as usize)),
                grid.center_y_mm(grid.row_of(cell as usize)),
            );
            let nearest = wall
                .iter()
                .map(|&w| (x - grid.center_x_mm(grid.col_of(w))).hypot(y - grid.center_y_mm(grid.row_of(w))))
                .fold(f64::INFINITY, f64::min);
            assert!(
                nearest >= 0.05 + 0.3,
                "layer {layer}: the path is {nearest} mm from the wall (wall x {sx})"
            );
        }
    }

    /// **PREFERRED CANNOT BYPASS THE FINAL GAP** (challenger round 2,
    /// 2026-10-04). The exempt stretch at a seat grows with the gap asked of it
    /// (`zone_factor · (r + gap)`), so a 1 mm descent at radius 0.05 mm is wholly
    /// inside its seat zone at the 0.6 mm preferred tier (1.21 mm) although its
    /// lowest 0.35 mm is testable at the 0.3 mm final tier (0.65 mm). A wall
    /// 0.2 mm beside that lower part then read `Preferred` and skipped the final
    /// test. Red before the fix: the straight descent was `Preferred`, counted
    /// refused by nothing. Green: the straight descent is refused at the final
    /// gap (and so is any lean that ends beside the wall), and nothing the
    /// descent returns as `Preferred` stands inside the final gap.
    #[test]
    fn a_short_descent_cannot_pass_the_preferred_tier_by_zone_alone() {
        let grid = plan_grid(0.0, 0.0, 8.0, 8.0, 400, H, 0.0, 1_000_000_000).0;
        let mut history = ColumnHistory::new(&grid);
        let seat = grid.index(80, 80);
        // 0.2 mm beside the column, over the lowest 0.4 mm only.
        let wall: Vec<usize> = (60..101).map(|row| grid.index(row, 84)).collect();
        cure_wall(&mut history, &grid, &wall, 0, 7);
        let law = LeanLaw::new(H, 45.0, 25.0);
        let clear = two_tier(&law);
        let seat_layer = 20; // 1 mm over the plate
        let mut scratch = Vec::new();
        let mut counts = outcome();
        let descent = super::descend_cleared(
            &grid, &history, &DrainKeepOut::none(), &law, true, 4.0, seat, seat_layer, &mut scratch,
            &mut counts, Some(&clear), 0.05,
        );
        assert_eq!(
            counts.solid.refused_at_final_gap, 1,
            "the straight descent stands 0.2 mm from the wall's lowest 0.4 mm: {counts:?}"
        );
        assert!(
            !(descent.path.is_empty() && descent.clears != super::Clears::Neither),
            "the straight descent must not be kept as clear: {:?}",
            descent.clears
        );
        // The same fixture with a wall far enough away reads Preferred, so the
        // refusal above is the wall's, not a blanket one.
        let mut far_history = ColumnHistory::new(&grid);
        let far: Vec<usize> = (60..101).map(|row| grid.index(row, 100)).collect();
        cure_wall(&mut far_history, &grid, &far, 0, 7);
        let mut far_counts = outcome();
        let far_descent = super::descend_cleared(
            &grid, &far_history, &DrainKeepOut::none(), &law, true, 4.0, seat, seat_layer,
            &mut scratch, &mut far_counts, Some(&clear), 0.05,
        );
        assert_eq!(far_descent.clears, super::Clears::Preferred, "{far_counts:?}");
    }

    /// A seat in a slot between two walls, open only along the walls: every
    /// lean stays 0.35 mm from one of them, so no descent clears, and the
    /// answer is `Clears::Neither` with the straight descent kept (emit and
    /// warn), counted as uncleared.
    #[test]
    fn a_descent_no_lean_can_clear_is_kept_and_counted_uncleared() {
        let grid = grid();
        let mut history = ColumnHistory::new(&grid);
        let seat = grid.index(20, 20);
        let walls: Vec<usize> = (0..grid.rows())
            .flat_map(|row| [grid.index(row, 13), grid.index(row, 27)])
            .collect();
        cure_wall(&mut history, &grid, &walls, 10, 199);
        let law = LeanLaw::new(H, 45.0, 25.0);
        let clear = two_tier(&law);
        let mut scratch = Vec::new();
        let mut counts = outcome();
        let descent = super::descend_cleared(
            &grid, &history, &DrainKeepOut::none(), &law, true, 4.0, seat, 200, &mut scratch, &mut counts,
            Some(&clear), 0.05,
        );
        assert_eq!(descent.clears, super::Clears::Neither, "{counts:?}");
        assert!(descent.path.is_empty(), "the straight descent is kept");
        assert_eq!(counts.solid.uncleared, 1);
        assert!(counts.solid.lean_candidates_refused > 0, "leans were tried and refused: {counts:?}");
    }

    /// Where nothing stands beside the descent the drawn-solid test changes
    /// nothing: the same landing and path as the retired descent, and the
    /// straight descent clears the preferred margin (the byte-identity the
    /// Round discs rest on).
    #[test]
    fn a_descent_with_no_wall_beside_it_is_unchanged() {
        let grid = grid();
        let mut history = ColumnHistory::new(&grid);
        let neighbour = grid.index(20, 28);
        straight_arm(&mut history, 7, neighbour, 0, 190);
        let seat = grid.index(20, 20);
        let law = LeanLaw::new(H, 45.0, 25.0);
        let clear = two_tier(&law);
        let mut scratch = Vec::new();
        let mut retired = outcome();
        let before = descend_leaning(
            &grid, &history, &DrainKeepOut::none(), &law, true, 4.0, seat, 200, &mut scratch, &mut retired,
        );
        let mut counts = outcome();
        let after = super::descend_cleared(
            &grid, &history, &DrainKeepOut::none(), &law, true, 4.0, seat, 200, &mut scratch, &mut counts,
            Some(&clear), 0.05,
        );
        assert_eq!((after.landing, after.k_bottom, after.rooted, after.path.clone()), before);
        // The merge onto the neighbouring trunk is the same descent as the
        // retired search took; with the straight descent clear, only the
        // final gap is asked of it.
        assert_ne!(after.clears, super::Clears::Neither);
        assert_eq!(counts.solid.tested, 1, "the solid was read");
        assert_eq!(counts.solid.inside_preferred_margin, 0);
        assert_eq!(counts.solid.refused_at_final_gap, 0);
    }

    /// **THE PREFERRED MARGIN IS A SEARCH MARGIN, NOT A GATE** (Craig's
    /// 2026-08-22 ruling, and the Round discs' byte identity, which stand
    /// 0.3 to 0.6 mm from their ring's walls): a wall 0.55 mm from the seat's
    /// column is inside the 0.6 mm margin and outside the 0.3 mm final gap, so
    /// the straight descent STAYS, exactly the descent the retired search
    /// returned, and is counted as inside the margin and refused by nothing.
    #[test]
    fn a_straight_descent_inside_the_preferred_margin_but_clear_of_the_final_gap_stays() {
        let grid = plan_grid(0.0, 0.0, 8.0, 8.0, 400, H, 0.0, 1_000_000_000).0;
        let mut history = ColumnHistory::new(&grid);
        let seat = grid.index(80, 80);
        // Eleven cells = 0.55 mm: past the final reach (0.05 + 0.3 + 0.035)
        // and inside the preferred one (0.05 + 0.6 + 0.035).
        let wall: Vec<usize> = (60..101).map(|row| grid.index(row, 91)).collect();
        cure_wall(&mut history, &grid, &wall, 10, 199);
        let law = LeanLaw::new(H, 45.0, 25.0);
        let clear = two_tier(&law);
        let mut scratch = Vec::new();
        let mut retired = outcome();
        let before = descend_leaning(
            &grid, &history, &DrainKeepOut::none(), &law, true, 4.0, seat, 200, &mut scratch, &mut retired,
        );
        let mut counts = outcome();
        let after = super::descend_cleared(
            &grid, &history, &DrainKeepOut::none(), &law, true, 4.0, seat, 200, &mut scratch, &mut counts,
            Some(&clear), 0.05,
        );
        assert_eq!((after.landing, after.k_bottom, after.rooted, after.path.clone()), before);
        assert_eq!(counts.solid.inside_preferred_margin, 1, "path hit: the wall is inside the margin");
        assert_eq!(counts.solid.refused_at_final_gap, 0);
        assert_eq!(after.clears, super::Clears::FinalOnly);
    }

    /// **THE FINAL GAP DOES NOT APPLY TO A DESCENT INSIDE A HIDDEN CAVITY**
    /// (Craig ruling 2026-10-05; challenger 2026-10-05: the up-sweep's
    /// refusing landing search read the 0.3 mm gap unconditionally). The
    /// fixture of `a_descent_beside_a_wall_is_tested_at_its_smallest_drawn_radius`
    /// (a cured wall 0.35 mm beside the seat's column, the straight descent
    /// inside the final gap) with the cavity map of a real sealed 6 mm box
    /// with 1.5 mm walls around the seat:
    ///
    /// - SEALED cavity: the straight descent is NOT refused at the final gap
    ///   (`refused_at_final_gap` 0), it stays (`Clears::FinalOnly`, no path),
    ///   and the path hit `in_hidden_cavity` is 1;
    /// - the NEGATIVE CONTROLS, same wall, same seat: the same box with a side
    ///   wall left out (an open pocket) and no cavity at all both refuse it at
    ///   the final gap and replace it by a lean, as before.
    ///
    /// SABOTAGE RUN (by hand): `in_cavity` answering `false` refuses the sealed
    /// case too and the first assertion fails.
    #[test]
    fn a_straight_descent_inside_a_hidden_cavity_is_not_refused_at_the_final_gap() {
        use crate::cavity::fixtures::{cavity_box_sized, map_on};
        let grid = plan_grid(0.0, 0.0, 8.0, 8.0, 400, H, 0.0, 1_000_000_000).0;
        let seat = grid.index(80, 80);
        let wall: Vec<usize> = (60..101).map(|row| grid.index(row, 87)).collect();
        let law = LeanLaw::new(H, 45.0, 25.0);
        let clear = two_tier(&law);
        let run = |map: Option<bool>| {
            let mut history = ColumnHistory::new(&grid);
            cure_wall(&mut history, &grid, &wall, 10, 79);
            if let Some(open_side) = map {
                history.hidden = map_on(&cavity_box_sized(open_side, false, 0.0, 6.0, 1.5), &[], 0.0, &grid);
            }
            let mut scratch = Vec::new();
            let mut counts = outcome();
            let descent = super::descend_cleared(
                &grid, &history, &DrainKeepOut::none(), &law, true, 4.0, seat, 80, &mut scratch, &mut counts,
                Some(&clear), 0.05,
            );
            (descent, counts, history.hidden.census().enclosed_voxels)
        };
        let (descent, counts, enclosed) = run(Some(false));
        assert!(enclosed > 0, "anti-vacuity: the sealed box encloses air");
        assert_eq!(counts.solid.refused_at_final_gap, 0, "{counts:?}");
        assert_eq!(counts.solid.in_hidden_cavity, 1, "path hit: the descent stood in the cavity: {counts:?}");
        assert_eq!(counts.solid.inside_preferred_margin, 1, "anti-vacuity: the wall is inside the margin");
        assert!(descent.path.is_empty() && descent.clears == super::Clears::FinalOnly, "{:?}", descent.clears);
        for (map, what) in [(Some(true), "an open pocket"), (None, "no cavity")] {
            let (descent, counts, _) = run(map);
            assert_eq!(counts.solid.refused_at_final_gap, 1, "{what}: {counts:?}");
            assert_eq!(counts.solid.in_hidden_cavity, 0, "{what}: {counts:?}");
            assert!(!descent.path.is_empty(), "{what}: a leaned descent replaces it: {counts:?}");
        }
    }

    /// **A SHAFT IS EXEMPT FOR ITS OWN CONTACT ZONE, NOT FOR THE LONGEST ONE**
    /// (challenger round 3, 2026-10-04). A 0.6 mm branch of radius 0.05 mm
    /// under a flat seat (the underside of a slab at z = 2): the flat seat's
    /// zone is `(0.3 + 0.05) / 1 = 0.35 mm`, but the lean law's longest zone
    /// is `1.86 * 0.35 = 0.651 mm`, which exempted the branch whole, so a
    /// foreign wall 0.25 mm from its axis beside its lowest 0.2 mm (clearance
    /// 0.2 mm) was never tested.
    ///
    /// SABOTAGE INSIDE: the retired read (`shaft_model_intrusion`, the longest
    /// zone for every shaft) accepts it. CONTROLS: a wall 0.8 mm away clears;
    /// a thin lip that stands only inside the seat's own zone (0.05 mm under
    /// the slab) is still excused.
    #[test]
    fn a_short_branch_under_a_flat_seat_is_tested_below_the_flat_seats_own_zone() {
        let grid = plan_grid(-2.0, -2.0, 2.0, 2.0, 400, H, 0.0, 1_000_000_000).0;
        let law = LeanLaw::new(H, 45.0, 25.0);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the cell under the origin of a small test grid"
        )]
        let seat = grid.index(grid.row_coord(0.0) as usize, grid.col_coord(0.0) as usize);
        // The raster of a wall `wall.0..wall.1` in x, 1 mm across in y, from
        // z `z.0` to `z.1`.
        let at = |wall: (f64, f64), z: (f64, f64)| -> ColumnHistory {
            let mut history = ColumnHistory::new(&grid);
            let cells: Vec<usize> = (0..grid.cells())
                .filter(|&cell| {
                    let (x, y) = (
                        grid.center_x_mm(grid.col_of(cell)),
                        grid.center_y_mm(grid.row_of(cell)),
                    );
                    x >= wall.0 && x <= wall.1 && y.abs() <= 0.5
                })
                .collect();
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a height over the layer height, in whole layers of a small test frame"
            )]
            let (k0, k1) = ((z.0 / H).round() as usize, (z.1 / H).round() as usize - 1);
            cure_wall(&mut history, &grid, &cells, k0, k1);
            history
        };
        let (k_seat, k_base) = (40_usize, 28_usize); // z = 2.0 and 1.4: a 0.6 mm branch
        let radius = 0.05;
        // The slab (whose underside is the seat) and the wall are two bodies
        // in one surface; the raster knows the wall.
        fn read_of<'a>(
            grid: &'a crate::grid::Grid,
            history: &'a ColumnHistory,
            surface: &'a geom_kernel::NativeSurface,
            zone_factor: f64,
        ) -> crate::member_clearance::ShaftRead<'a> {
            crate::member_clearance::ShaftRead {
                grid,
                history,
                layer_height_mm: H,
                plate_z_mm: 0.0,
                surface: Some(surface),
                zone_factor,
            }
        }
        let both = |wall: (f64, f64), z: (f64, f64)| -> geom_kernel::NativeSurface {
            two_boxes(
                ([-1.0, -1.0, 2.0], [1.0, 1.0, 3.0]),
                ([wall.0, -0.5, z.0], [wall.1, 0.5, z.1]),
            )
        };

        // The challenger's shape: wall top 1.6 mm, 0.25 mm beside the axis.
        let history = at((0.25, 0.75), (0.0, 1.6));
        let surface = both((0.25, 0.75), (0.0, 1.6));
        let read = read_of(&grid, &history, &surface, law.face_zone_factor());
        let face = read.seat_face((seat, k_seat));
        let face_normal = face.expect("the seat's face is the slab's underside").normal;
        assert!(face_normal[2] < -0.99, "a flat seat: {face_normal:?}");
        assert!(
            crate::member_clearance::shaft_model_intrusion(
                &grid, &history, H, (seat, k_seat), (seat, k_base), false, radius, 0.3, law.face_zone_factor(),
            )
            .is_none(),
            "SABOTAGE: the retired read excuses the whole branch"
        );
        let hit = read.intrusion((seat, k_seat), face, (seat, k_base), false, radius, 0.3);
        assert!(hit.is_some(), "the lowest 0.2 mm stands 0.2 mm from the wall: refused");

        // Control: a wall 0.8 mm away clears.
        let history = at((0.85, 1.35), (0.0, 1.6));
        let surface = both((0.85, 1.35), (0.0, 1.6));
        let read = read_of(&grid, &history, &surface, law.face_zone_factor());
        let face = read.seat_face((seat, k_seat));
        assert!(read.intrusion((seat, k_seat), face, (seat, k_base), false, radius, 0.3).is_none());

        // Control: a thin wall that stands only inside the seat's own zone (a
        // 0.04 mm lip 0.2 mm beside the axis, 0.05 mm under the slab) is still
        // excused: the raster flags it (its reach carries half a cell's
        // diagonal) and the census's own predicate, which leaves the zone out,
        // reads the rest of the branch clear.
        let history = at((0.2, 0.7), (1.95, 1.99));
        let surface = both((0.2, 0.7), (1.95, 1.99));
        let read = read_of(&grid, &history, &surface, law.face_zone_factor());
        let face = read.seat_face((seat, k_seat));
        assert!(
            read.intrusion((seat, k_seat), face, (seat, k_base), false, radius, 0.3).is_none(),
            "inside the seat's own zone: not the branch's to answer for"
        );
    }

    /// Two boxes as one surface.
    fn two_boxes(
        a: ([f64; 3], [f64; 3]),
        b: ([f64; 3], [f64; 3]),
    ) -> geom_kernel::NativeSurface {
        crate::member_clearance::boxes_surface(&[a, b])
    }
}

/// **The producer's descent mechanisms** (lane `head-first`, 2026-10-06): the
/// post of a head descends from its JOINT against the history below it, and a
/// straight descent that meets a head is blocked.
///
/// SABOTAGE (each run by hand): [`model_top_below`] reading `top_model` for a
/// lower start turns the first test red; [`lands_on_head`] returning `false`
/// turns the second red.
#[cfg(test)]
mod head_first_tests {
    use super::{
        descend_below, descend_cleared, descend_cleared_from, ColumnHistory, LayerOutcome,
    };
    use crate::drain::DrainKeepOut;
    use crate::head_carrier_tests::{grid, headed, H};
    use crate::lean::LeanLaw;
    use crate::Landing;

    /// Cure `cell` over layers `from..=to` the way `commit_layer_history` does.
    fn cure(history: &mut ColumnHistory, cells: usize, cell: usize, from: usize, to: usize) {
        let mut cover = vec![false; cells];
        cover[cell] = true;
        for layer in from..=to {
            history.stamp_model(layer, &cover);
            history.top_model[cell] = u32::try_from(layer + 1).expect("small");
            history.top_model_rooted[cell] = true;
            history.column_max_model[cell] = u32::try_from(layer + 1).expect("small");
        }
    }

    /// The concave case. A head's joint stands at layer 100 and its seat at
    /// 150, and cured model stands in the JOINT's column from 110 to 149 (a
    /// concave wall above the joint). The post, descending from the joint, must
    /// reach the plate; the running state, read from the seat, lands on that
    /// model (the control, which also proves the fixture holds the model).
    #[test]
    fn the_post_descends_from_the_joint_and_never_lands_on_the_model_above_it() {
        let grid = grid();
        let mut history = ColumnHistory::new(&grid);
        let joint_cell = grid.index(150, 150);
        cure(&mut history, grid.cells(), joint_cell, 110, 149);
        let law = LeanLaw::new(H, 45.0, 25.0);
        let mut scratch = Vec::new();
        let keep_out = DrainKeepOut::none();

        // Control: from the seat layer the same column lands on that model.
        let mut counts = LayerOutcome::empty();
        let from_seat = descend_cleared(
            &grid, &history, &keep_out, &law, true, 4.0, joint_cell, 150, &mut scratch,
            &mut counts, None, 0.0,
        );
        assert_eq!(
            from_seat.landing,
            Landing::Model { layer: 149 },
            "control: the running state holds the model above the joint"
        );

        // The post: start at the joint, history running at the seat's layer.
        let mut counts = LayerOutcome::empty();
        let post = descend_cleared_from(
            &grid, &history, &keep_out, &law, true, 4.0, joint_cell, 100, 150, &mut scratch,
            &mut counts, None, 0.0, None, 0.0,
        );
        assert_eq!(
            post.landing,
            Landing::Plate,
            "the post starts under the model and lands on the plate, not on the model above"
        );
        assert_eq!(post.k_bottom, 0);
        assert!(post.path.is_empty(), "a straight post carries no path");
        // The same from the straight descent alone.
        let (landing, bottom, _, blocked) = descend_below(&history, joint_cell, 100, 150);
        assert_eq!((landing, bottom, blocked), (Landing::Plate, 0, false));
    }

    /// A head's post never leans back across its own seat plane (the seat trim
    /// would cut it off the joint): a base under the model side of the plane is
    /// refused, a base straight under the joint and one leaning outward are
    /// admitted. SABOTAGE: `admits` returning `true` turns this red.
    #[test]
    fn a_post_may_not_lean_back_across_its_seat_plane() {
        use super::PostPlane;
        let phi = 41.3_f64.to_radians();
        let unit = [phi.sin(), 0.0, -phi.cos()];
        let contact = [0.0, 0.0, 10.0];
        let joint = [contact[0] + 0.5 * unit[0], 0.0, contact[2] + 0.5 * unit[2]];
        let plane = PostPlane {
            joint_mm: joint,
            contact_mm: contact,
            unit,
            radius_mm: 0.2,
            plate_z_mm: 0.0,
            margin_mm: 0.07,
        };
        // Straight under the joint, and leaning out away from the model.
        assert!(plane.admits([joint[0], joint[1], 2.0]));
        assert!(plane.admits([joint[0] + 3.0, joint[1], 6.0]));
        // Leaning back under the face: beyond the plane, on the model's side.
        assert!(!plane.admits([joint[0] - 5.0, joint[1], 8.0]));
    }

    /// A later seat's straight descent meets an earlier head: blocked, so it
    /// leans onto the trunk beside it (priced as dearer than the straight drop,
    /// which an unblocked descent keeps); with the lean off it keeps the
    /// straight landing, and the block is reported.
    #[test]
    fn a_straight_descent_onto_a_head_is_blocked_and_leans_or_keeps_the_landing() {
        let grid = grid();
        let mut history = ColumnHistory::new(&grid);
        // A headed arm: joint at layer 100, seat (and head top) at layer 190,
        // standing in (150, 150). Its post is landable only to layer 95.
        let host = headed(&grid, 7, 150, 150, 190, 100, 0.2);
        let host_cell = host.cell;
        history.stamp_strut(&host, H);
        for (slot, &occupied) in host.path.iter().enumerate() {
            let here = u32::try_from(host.k_bottom + slot + 1).expect("small");
            if history.top_strut[occupied as usize] < here {
                history.top_strut[occupied as usize] = here;
                history.top_strut_id[occupied as usize] = 7;
            }
        }
        // A trunk eight cells (0.4 mm) off, its top at layer 150.
        let trunk_cell = grid.index(150, 158);
        history.top_strut[trunk_cell] = 151;
        history.top_strut_id[trunk_cell] = 3;
        history.stamp_shaft(3, 0, 150, trunk_cell, &[]);
        let law = LeanLaw::new(H, 45.0, 25.0);
        let mut scratch = Vec::new();
        let keep_out = DrainKeepOut::none();

        // The straight descent from layer 200 lands on the head: blocked.
        let (landing, bottom, _, blocked) = descend_below(&history, host_cell, 200, 200);
        assert_eq!(landing, Landing::Strut { id: 7, layer: 190 });
        assert_eq!(bottom, 190);
        assert!(blocked, "a landing above the joinable top (layer 95) is blocked");

        // It leans onto the trunk instead (the straight drop is ten layers, the
        // lean fifty: only the block sends it there).
        let mut counts = LayerOutcome::empty();
        let leaned = descend_cleared(
            &grid, &history, &keep_out, &law, true, 4.0, host_cell, 200, &mut scratch,
            &mut counts, None, 0.0,
        );
        assert_eq!(
            leaned.landing,
            Landing::Strut { id: 3, layer: 150 },
            "blocked by the head, the seat leans onto the trunk"
        );
        assert!(!leaned.path.is_empty());

        // With the lean off there is no alternative: the straight landing
        // stands (the landing ledger reports it as a join above the joint).
        let mut counts = LayerOutcome::empty();
        let kept = descend_cleared(
            &grid, &history, &keep_out, &LeanLaw::disabled(H), true, 4.0, host_cell, 200,
            &mut scratch, &mut counts, None, 0.0,
        );
        assert_eq!(kept.landing, Landing::Strut { id: 7, layer: 190 });
        assert!(kept.path.is_empty());
    }
}

#[cfg(test)]
mod head_skip_tests {
    use super::{facet_skip, HeadsState};
    use crate::heads::{HeadRule, HeadSkip, HeadsOutcome, SkippedSeat, TYPICAL_TIP_RADIUS_MM};

    fn rule() -> HeadRule {
        HeadRule {
            gap_mm: 0.3,
            allowance_mm: 0.1,
            cos_lean_max: 0.5373,
            max_radius_mm: 1.0,
            link_mm: 0.0,
            typical_tip_radius_mm: TYPICAL_TIP_RADIUS_MM,
            max_normal_z: -0.34,
            layer_height_mm: 0.05,
            plate_z_mm: 0.0,
        }
    }

    /// The facet exits that used to return silently each name their reason,
    /// and a skipped seat survives the publish into the run report.
    #[test]
    fn every_silent_facet_exit_names_a_reason_and_is_published() {
        let rule = rule();
        let tilt = |deg: f64| [deg.to_radians().sin(), 0.0, -deg.to_radians().cos()];
        assert_eq!(facet_skip([0.0, 0.0, 1.0], &rule, 0.15), Some(HeadSkip::FacetNotDownFacing));
        assert_eq!(facet_skip([1.0, 0.0, 0.0], &rule, 0.15), Some(HeadSkip::FacetNotDownFacing));
        assert_eq!(facet_skip(tilt(80.0), &rule, 0.15), Some(HeadSkip::FacetSteeperThanBand));
        assert_eq!(facet_skip(tilt(5.0), &rule, 0.15), Some(HeadSkip::FacetTooFlat));
        assert_eq!(facet_skip(tilt(45.0), &rule, 0.15), None);

        let seat = SkippedSeat {
            id: 7,
            reason: HeadSkip::FacetTooFlat,
            seat_mm: [1.0, 2.0, 3.0],
            plate_z_mm: 0.0,
            facet: Some(tilt(5.0)),
            footprint: None,
        };
        let mut state = HeadsState::default();
        state.outcome.skips.entry(seat.reason).or_default().push(seat.id);
        state.outcome.skipped_seats.push(seat);
        let mut report = HeadsOutcome::default();
        state.publish(&mut report);
        assert_eq!(report.skips[&HeadSkip::FacetTooFlat], vec![7]);
        assert_eq!(report.skipped_seats, vec![seat]);
    }

    /// A seat whose axis point lies on the edge between a 45 degree
    /// down-facing face and a vertical wall: the nearest face of any slope is
    /// the wall (skipped as not down-facing), the nearest face in the band is
    /// the slope, and the slope passes the prune, so the footprint query is
    /// reached (Cylinder members 144 and 151).
    #[test]
    fn a_seat_on_a_walls_edge_reads_the_band_face_and_reaches_the_footprint_query() {
        use crate::heads::{band_face_ids, nearest_band_face};
        use geom_kernel::{NativeSurface, NativeSurfaceConfig};
        let rule = rule();
        let s = 45.0_f64.to_radians();
        let (sin, cos) = (s.sin(), s.cos());
        // Face 0: the slope, outward normal (sin, 0, -cos). Face 1: a wall in
        // the plane x = 0, outward normal (-1, 0, 0).
        let verts = vec![
            [0.0, 0.0, 0.0],
            [0.0, 2.0, 0.0],
            [2.0 * cos, 0.0, 2.0 * sin],
            [0.0, 0.0, -2.0],
        ];
        let tris = vec![[0, 1, 2], [0, 1, 3]];
        let mesh = plinth_mesh::Mesh::new(verts, tris).expect("two triangles");
        let surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            NativeSurfaceConfig { grid_pitch_mm: 0.25, max_grid_bytes: 64 << 20 },
        )
        .expect("a surface");
        let seat = [-0.01, 0.5, -0.5];

        let (_, _, faces, normals) = surface.project(&[seat]);
        assert_eq!(faces[0], 1, "the wall is the nearest face of any slope");
        assert_eq!(
            facet_skip(normals[0], &rule, 0.15),
            Some(HeadSkip::FacetNotDownFacing),
            "read as it is today, the seat is skipped"
        );

        let band = band_face_ids(&mesh, rule.max_normal_z);
        assert_eq!(band, vec![0], "only the slope is in the band");
        let (_, face, normal) =
            nearest_band_face(&surface, &mesh, &band, seat).expect("a band face");
        assert_eq!(face, 0);
        assert!((normal[0] - sin).abs() < 1e-9 && (normal[2] + cos).abs() < 1e-9, "{normal:?}");
        assert_eq!(facet_skip(normal, &rule, 0.15), None, "the footprint query is asked");
    }
}

/// **A model landing must take its whole foot** (lane 2, 2026-10-12,
/// `MODEL-FEET.md` F2).
#[cfg(test)]
mod foot_fit_tests {
    use super::{descend_cleared_from, foot_on_model, ColumnHistory, LayerOutcome};
    use crate::drain::DrainKeepOut;
    use crate::grid::plan_grid;
    use crate::lean::LeanLaw;
    use crate::Landing;

    const H: f64 = 0.05;

    /// Cure `cells` over layers `0..=top`, as `commit_layer_history` records
    /// them layer by layer.
    fn cure(history: &mut ColumnHistory, grid: &crate::grid::Grid, cells: &[usize], top: usize) {
        for layer in 0..=top {
            let mut cover = vec![false; grid.cells()];
            for &cell in cells {
                cover[cell] = true;
            }
            history.stamp_model(layer, &cover);
            for &cell in cells {
                history.top_model[cell] = u32::try_from(layer + 1).expect("a small layer");
                history.top_model_rooted[cell] = true;
            }
        }
    }

    /// A one-cell-wide FIN (0.05 mm, a claw's edge) under the seat, and a wide
    /// BLOCK 0.5 mm to one side, both topping out at layer 99; the seat at
    /// layer 200.
    fn fin_and_block() -> (crate::grid::Grid, ColumnHistory, usize, usize) {
        let grid = plan_grid(0.0, 0.0, 8.0, 8.0, 400, H, 0.0, 1_000_000_000).0;
        let mut history = ColumnHistory::new(&grid);
        let fin: Vec<usize> = (60..101).map(|row| grid.index(row, 80)).collect();
        let block: Vec<usize> = (60..101)
            .flat_map(|row| (90..111).map(move |col| (row, col)))
            .map(|(row, col)| grid.index(row, col))
            .collect();
        cure(&mut history, &grid, &fin, 99);
        cure(&mut history, &grid, &block, 99);
        let seat = grid.index(80, 80);
        let block_cell = grid.index(80, 100);
        (grid, history, seat, block_cell)
    }

    /// The census reads cured cells under the WHOLE disc, never the axis cell
    /// alone: a 0.1 mm foot on the fin has its axis cell and the two along the
    /// fin on the model, and the rest of its disc in the air; on the block it
    /// is wholly on the model. A zero radius reads the axis cell only.
    #[test]
    fn the_foot_census_reads_the_whole_disc_on_the_cured_raster() {
        let (grid, history, seat, block_cell) = fin_and_block();
        let (on, total) = foot_on_model(&grid, &history, seat, 99, 0.1);
        assert!(total > 3, "a 0.1 mm disc at a 0.05 mm pitch spans {total} cells");
        assert_eq!(on, 5, "the axis cell and two cells each way along the fin");
        assert_eq!(foot_on_model(&grid, &history, block_cell, 99, 0.1), (total, total));
        assert_eq!(foot_on_model(&grid, &history, seat, 99, 0.0), (1, 1));
        // A layer of the seat law's own depth: one layer lower is still in
        // the seat, two are not.
        assert_eq!(foot_on_model(&grid, &history, block_cell, 100, 0.1).0, total);
        assert_eq!(foot_on_model(&grid, &history, block_cell, 101, 0.1).0, 0);
    }

    /// THE RULE, AND ITS SABOTAGE. The straight descent lands on the fin. With
    /// a 0.1 mm foot it does not fit, so the search takes a candidate whose
    /// foot does (the block, 0.5 mm aside), refusing every other fin cell it
    /// prices on the way. SABOTAGE: a zero foot (the axis cell alone, the test
    /// before 2026-10-12) lands straight on the fin and refuses nothing.
    #[test]
    fn a_foot_that_does_not_fit_on_a_fin_moves_to_a_landing_it_fits() {
        let (grid, history, seat, block_cell) = fin_and_block();
        let law = LeanLaw::new(H, 45.0, 25.0);
        let descend = |foot_radius_mm: f64| {
            let mut scratch = Vec::new();
            let mut outcome = LayerOutcome::empty();
            let descent = descend_cleared_from(
                &grid,
                &history,
                &DrainKeepOut::none(),
                &law,
                true,
                4.0,
                seat,
                200,
                200,
                &mut scratch,
                &mut outcome,
                None,
                0.0,
                None,
                foot_radius_mm,
            );
            (descent, outcome)
        };

        let (sabotaged, counts) = descend(0.0);
        assert_eq!(sabotaged.landing, Landing::Model { layer: 99 });
        assert!(sabotaged.path.is_empty(), "straight down onto the fin");
        assert_eq!(counts.solid.foot_candidates_refused, 0);
        assert_eq!(counts.solid.foot_straight_replaced, 0);

        let (fitted, counts) = descend(0.1);
        let base = fitted.path.first().map_or(seat, |cell| *cell as usize);
        assert_ne!(base, seat, "the foot left the fin");
        assert_ne!(grid.col_of(base), 80, "no landing on the fin");
        // The cheapest candidate that fits: the plate beside the fin (its
        // straight 10 mm against the block's 5 mm lean plus the 5 mm the
        // block still owes the plate), or the block itself; never the fin.
        match fitted.landing {
            Landing::Plate => {}
            Landing::Model { layer } => {
                let (on, total) = foot_on_model(&grid, &history, base, layer, 0.1);
                assert_eq!(on, total, "the landing taken holds the whole foot");
                assert!(
                    (90..=110).contains(&grid.col_of(base)),
                    "on the block (block cell {block_cell}, base {base})"
                );
            }
            Landing::Strut { .. } => panic!("there is no arm to land on"),
        }
        assert_eq!(counts.solid.foot_straight_replaced, 1);
        assert!(counts.solid.foot_candidates_refused > 0, "the other fin cells were priced");
        assert_eq!(counts.solid.foot_straight_kept, 0);
    }

    /// EMIT AND WARN. With no lean at all (the law disabled) nothing can
    /// replace the straight descent: it is kept, on the fin, and counted.
    #[test]
    fn a_foot_with_nowhere_else_to_go_keeps_its_landing_and_is_counted() {
        let (grid, history, seat, _) = fin_and_block();
        let law = LeanLaw::disabled(H);
        let mut scratch = Vec::new();
        let mut outcome = LayerOutcome::empty();
        let descent = descend_cleared_from(
            &grid,
            &history,
            &DrainKeepOut::none(),
            &law,
            true,
            4.0,
            seat,
            200,
            200,
            &mut scratch,
            &mut outcome,
            None,
            0.0,
            None,
            0.1,
        );
        assert_eq!(descent.landing, Landing::Model { layer: 99 });
        assert!(descent.path.is_empty());
        assert_eq!(outcome.solid.foot_straight_kept, 1);
    }
}
