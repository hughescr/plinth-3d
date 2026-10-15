//! **Per-event sizing (rule a′, owner-approved 2026-09-29).** Each peel event is
//! routed down on its own, and each member is sized for its single worst event.
//!
//! # The defect this replaces
//!
//! The down-sweep ([`crate::down_sweep`]) carries ONE load raster down the
//! stack, and each carrier cell keeps `max(routed, own)` over every event above
//! it. The lateral step then SUMS those per-column maxima where they meet. On a
//! body whose load walks down its own flanks to one line — a laid-over
//! cylinder — the load that meets at that line is the sum of column maxima from
//! events that never peel at the same instant: the Cylinder's arm 273 was
//! charged 41.71 mm² where no single event ever puts more than 14.70 mm² on it
//! (`plinth-evidence/.../field-2026-09-28/cylinder-load-sharing.md`, §2).
//! `spread(max) ≥ max(spread)` is conservative; it is also, on that shape, 2.8×
//! wrong.
//!
//! # The rule
//!
//! Keep the engine's own routing — straight down a column, and at an edge ALL of
//! a cell's load to its single nearest carrier within one lamina reach — and
//! change only the ORDER of max and sum. Every layer's peel is its own event.
//! For each event, route its cells down through the recorded routing, push what
//! reaches each member down that member's chain (a child hands its load to its
//! parent, a model landing hands it back to the model at its footprint), and
//! only THEN take the maximum over events, per member and per layer.
//!
//! Two consequences follow without a rule of their own:
//!
//! * a member that catches two parts of the SAME event at two heights (a leaned
//!   arm crossing columns) carries their SUM below the lower catch, because
//!   both are one event's load in one member at one instant;
//! * a child's load joins its parent's for the same event, not the child's
//!   worst event added to the parent's worst event.
//!
//! # Why it is exact and still batch-first
//!
//! The routing sends ALL of a cell's load to ONE place, so every cell of every
//! layer has exactly one path. The down-sweep records only what that path
//! needs — the cells whose load did not go straight down, and who read each
//! carrier cell — and this pass composes the paths UPWARD, one raster per
//! layer: `dest(layer, cell)` is the member and layer the cell's load first
//! enters, or the plate, or lost. Each event is then one histogram over its own
//! cells, and one push over the members it touched. Cost
//! `O(layers · cells + events · touched members)`, one pass.
//!
//! # A design load is a suffix maximum
//!
//! Within one event the force in a member is non-decreasing downward (load only
//! joins). So the design load at a layer — the largest force any event puts
//! there — is the largest cumulative force any event reaches at that layer OR
//! ABOVE it. Recording, per member and layer, the largest cumulative force any
//! event reaches there, and taking the running maximum from the tip down, is
//! the exact design load, and it is the same `max(previous, ·)` walk the ledger
//! has always used.
//!
//! # Conservation is checked per event
//!
//! For every event, plate reaction + lost = the event's own area, to 1e-9
//! relative. A break is a defect in this code, and like the down-sweep's own
//! conservation assertion it is a hard error rather than a warning.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "cell, layer and member indices are bounded by the cell-layer budget and the strut \
          cap and fit u32; cell counts into f64 areas are exact"
)]

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use plinth_support::perception::SliceSolid;
use rayon::prelude::*;

use crate::down_sweep::SizingPolicy;
use crate::grid::Grid;
use crate::raster::{rasterize_into, LayerMasks, RasterScratch};
use crate::{Landing, StrutKind, SweepStrut};

/// A path that reached no carrier.
pub(crate) const LOST: u64 = u64::MAX;
/// A path that reached the plate through model material.
pub(crate) const PLATE: u64 = u64::MAX - 1;
/// A recorded hop to nowhere (out of reach, or no root at all).
const NO_TARGET: u32 = u32::MAX;
/// Set on a recorded read when the member reads peel (an ARM). A brace or a
/// mast is lateral stiffening and reads none (`down_sweep::run`).
const READS_PEEL: u32 = 1 << 31;

/// A member and the layer at which load enters it.
#[must_use]
pub(crate) fn entry(member: usize, layer: usize) -> u64 {
    ((member as u64) << 32) | layer as u64
}

/// The inverse of [`entry`].
#[must_use]
pub(crate) fn unpack(key: u64) -> (usize, usize) {
    ((key >> 32) as usize, (key & 0xffff_ffff) as usize)
}

/// What the down-sweep's top-down pass recorded about its own routing, so the
/// per-event pass can follow exactly the same paths without routing again.
///
/// * `hops[L]` — while the pass was at layer `L`, every loaded cell of layer
///   `L+1` whose column was NOT a carrier at `L`, and where the routing field
///   sent it (`NO_TARGET` when nowhere). Every other loaded cell went straight
///   down.
/// * `reads[L]` — every member that read a cell at layer `L`, in the order the
///   pass read them (`member | READS_PEEL` for an arm). The first arm in that
///   order is the one that took the cell's load; a member that lands on the
///   model there puts its own load back into the cell for whoever reads next.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct RouteLog {
    pub(crate) hops: Vec<Vec<(u32, u32)>>,
    pub(crate) reads: Vec<Vec<(u32, u32)>>,
}

impl core::fmt::Debug for RouteLog {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RouteLog")
            .field("layers", &self.hops.len())
            .field("hops", &self.hops.iter().map(Vec::len).sum::<usize>())
            .field("reads", &self.reads.iter().map(Vec::len).sum::<usize>())
            .finish()
    }
}

impl RouteLog {
    pub(crate) fn new(layers: usize) -> Self {
        Self {
            hops: vec![Vec::new(); layers],
            reads: vec![Vec::new(); layers],
        }
    }

    pub(crate) fn record_hop(&mut self, layer: usize, cell: usize, target: Option<usize>) {
        let target = target.map_or(NO_TARGET, |t| t as u32);
        self.hops[layer].push((cell as u32, target));
    }

    pub(crate) fn record_read(
        &mut self,
        layer: usize,
        cell: usize,
        member: usize,
        reads_peel: bool,
    ) {
        let tag = member as u32 | if reads_peel { READS_PEEL } else { 0 };
        self.reads[layer].push((cell as u32, tag));
    }

    /// Whether the log holds anything at all (the sabotage path records none).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.hops.is_empty()
    }
}

/// What the per-event pass measured. Published in the run report beside the
/// max-field accounting, which it does not replace.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PerEventReport {
    /// Whether the sizing ran per event (false only under the sabotage knob
    /// `SweepPolicy::with_max_field_sizing`).
    pub enabled: bool,
    /// Peel events routed: layers with any cured carrier cell.
    pub events: usize,
    /// The largest single event, mm².
    pub max_event_mm2: f64,
    /// The largest plate reaction any ONE event produced, mm².
    pub max_event_plate_mm2: f64,
    /// The largest load any one event lost (reached no carrier), mm².
    pub max_event_uncarried_mm2: f64,
    /// The worst per-event conservation residual, relative to the event.
    pub conservation_residual: f64,
    /// `(event, member, layer)` entries routed into members. Path-hit witness.
    pub routed_entries: u64,
    /// Child-to-parent hand-offs, summed over events.
    pub child_feeds: u64,
    /// Model-landing hand-backs that entered another member, summed over events.
    pub model_continuations: u64,
    /// `(event, member)` pairs where one member caught routed load of ONE event
    /// at two or more layers. Those parts are summed below the lower catch; the
    /// max-field ledger kept only the larger. The witness for that rule.
    pub same_event_catches: u64,
    /// The most any such member gained by summing instead of maxing, mm².
    pub same_event_catch_gain_max_mm2: f64,
    /// Load a child handed to a parent that never reads at or below the
    /// landing layer (a stale landing), summed over events, mm². Counted as
    /// lost; the landing check names the member.
    pub stale_landing_mm2: f64,
    /// The heaviest member under the max-field ledger, mm², and under this
    /// rule. The difference is what the order change bought.
    pub worst_member_max_field_mm2: f64,
    pub worst_member_mm2: f64,
    pub worst_member_id: u32,
    /// `(member, layer)` pairs where one event routes more than `ℓ_max` into a
    /// member at one layer — the per-event insertion trigger — on the field as
    /// it was finally sized.
    pub triggers_over_limit: usize,
    /// Per-event passes run by the last down-sweep call (1 when nothing
    /// triggered an insertion).
    pub passes: usize,
}

/// One layer as the walker sees it, handed to an observer after the layer's
/// destinations are known.
pub(crate) struct LayerView<'a> {
    pub(crate) layer: usize,
    pub(crate) masks: &'a LayerMasks,
    /// The cells whose destination is set: every carrier cell of this layer
    /// and every cell a member read here.
    pub(crate) touched: &'a [usize],
    /// Final destination of the load at each cell of THIS layer (valid for
    /// carry cells and read cells; `LOST` elsewhere).
    pub(crate) dest: &'a [u64],
    /// Final destination of each cell of the layer BELOW (valid where that
    /// layer was carry or read).
    pub(crate) dest_below: &'a [u64],
    /// Where a cell of this layer steps to on the layer below: the target cell,
    /// or `None` when it is lost. Straight down unless recorded.
    pub(crate) hop: &'a dyn Fn(usize) -> Option<usize>,
}

/// Layers rasterised together, in parallel, ahead of the sequential
/// composition. Not a physical quantity: it trades `2 · cells` bytes of mask
/// per layer held against the parallelism available, and no result depends on
/// it.
const RASTER_CHUNK_LAYERS: usize = 16;

/// The rows and columns a layer's solids can occupy: their outer loops'
/// bounding box, one cell wider on every side. `None` for an empty layer.
#[expect(
    clippy::cast_sign_loss,
    reason = "grid coordinates are clamped to [0, rows or cols] before the cast"
)]
fn layer_box(
    grid: &Grid,
    solids: &[SliceSolid],
) -> Option<(core::ops::Range<usize>, core::ops::Range<usize>)> {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for solid in solids {
        for point in solid.outer().points_xy_mm() {
            for axis in 0..2 {
                lo[axis] = lo[axis].min(point[axis]);
                hi[axis] = hi[axis].max(point[axis]);
            }
        }
    }
    if !(lo[0].is_finite() && lo[1].is_finite() && hi[0].is_finite() && hi[1].is_finite()) {
        return None;
    }
    let span = |from: f64, to: f64, count: usize| {
        let count_f = count as f64;
        let start = (from.floor() - 1.0).clamp(0.0, count_f) as usize;
        let end = (to.ceil() + 1.0).clamp(0.0, count_f) as usize;
        start..end
    };
    Some((
        span(grid.row_coord(lo[1]), grid.row_coord(hi[1]), grid.rows()),
        span(grid.col_coord(lo[0]), grid.col_coord(hi[0]), grid.cols()),
    ))
}

/// Compose the recorded routing upward and hand each layer to `observe`.
///
/// Also resolves every model landing's hand-back (`cont[member]`) at the
/// layer it lands on, before `observe` sees that layer.
pub(crate) fn walk(
    grid: &Grid,
    slices: &[Vec<SliceSolid>],
    struts: &[SweepStrut],
    log: &RouteLog,
    cont: &mut [u64],
    mut observe: impl FnMut(&LayerView<'_>, &[u64]),
) {
    let cells = grid.cells();
    let layers = slices.len().min(log.hops.len());
    // The masks are rasterised a chunk of layers at a time, in parallel: each
    // layer's masks depend on that layer's slices alone, so the result is the
    // same whatever the thread count, and the composition below — which does
    // depend on order — stays sequential.
    let mut chunk: Vec<(RasterScratch, LayerMasks)> = (0..RASTER_CHUNK_LAYERS.min(layers.max(1)))
        .map(|_| (RasterScratch::new(grid), LayerMasks::default()))
        .collect();
    let mut dest_prev = vec![LOST; cells];
    let mut dest_cur = vec![LOST; cells];
    let mut touched_prev: Vec<usize> = Vec::new();
    let mut touched_cur: Vec<usize> = Vec::new();
    let mut first_reader = vec![0_u32; cells];
    let mut reader_stamp = vec![0_u32; cells];
    let mut hop_target = vec![0_u32; cells];
    let mut hop_stamp = vec![0_u32; cells];
    // Model landings, bucketed by the layer they land on.
    let mut lands_at: Vec<Vec<usize>> = vec![Vec::new(); layers];
    for (member, strut) in struts.iter().enumerate() {
        if matches!(strut.landing, Landing::Model { .. }) && strut.k_bottom < layers {
            lands_at[strut.k_bottom].push(member);
        }
    }
    for layer in 0..layers {
        let within = layer % RASTER_CHUNK_LAYERS;
        if within == 0 {
            let end = (layer + RASTER_CHUNK_LAYERS).min(layers);
            chunk[..end - layer]
                .par_iter_mut()
                .zip(&slices[layer..end])
                .for_each(|((scratch, masks), solids)| {
                    rasterize_into(grid, solids, scratch, masks);
                });
        }
        let masks = &chunk[within].1;
        let stamp = layer as u32 + 1;
        for &(cell, tag) in &log.reads[layer] {
            let cell = cell as usize;
            if tag & READS_PEEL != 0 && reader_stamp[cell] != stamp {
                reader_stamp[cell] = stamp;
                first_reader[cell] = tag & !READS_PEEL;
            }
        }
        if layer > 0 {
            for &(cell, target) in &log.hops[layer - 1] {
                hop_stamp[cell as usize] = stamp;
                hop_target[cell as usize] = target;
            }
        }
        let below = |cell: usize| -> u64 {
            if layer == 0 {
                return PLATE;
            }
            if hop_stamp[cell] == stamp {
                let target = hop_target[cell];
                if target == NO_TARGET {
                    LOST
                } else {
                    dest_prev[target as usize]
                }
            } else {
                dest_prev[cell]
            }
        };
        touched_cur.clear();
        // Read cells first (few: one per member alive here), then the carrier
        // cells, looked for only inside this layer's own bounding box.
        for &(cell, _) in &log.reads[layer] {
            let cell = cell as usize;
            if reader_stamp[cell] == stamp && dest_cur[cell] == LOST {
                dest_cur[cell] = entry(first_reader[cell] as usize, layer);
                touched_cur.push(cell);
            }
        }
        if let Some((rows, cols)) = layer_box(grid, &slices[layer]) {
            for row in rows {
                let base = row * grid.cols();
                for cell in base + cols.start..base + cols.end {
                    if masks.carry[cell] && reader_stamp[cell] != stamp {
                        dest_cur[cell] = below(cell);
                        touched_cur.push(cell);
                    }
                }
            }
        }
        // A model landing puts its load back at its own footprint AFTER it has
        // read there: the next arm to read that cell at this layer takes it,
        // and otherwise it steps down exactly as the cell's own load would.
        for &member in &lands_at[layer] {
            let cell = struts[member].cell_at(layer);
            let reads = &log.reads[layer];
            let after = reads
                .iter()
                .position(|&(c, tag)| c as usize == cell && (tag & !READS_PEEL) as usize == member)
                .and_then(|at| {
                    reads[at + 1..]
                        .iter()
                        .find(|&&(c, tag)| c as usize == cell && tag & READS_PEEL != 0)
                })
                .map(|&(_, tag)| entry((tag & !READS_PEEL) as usize, layer));
            cont[member] = after.unwrap_or_else(|| below(cell));
        }
        let hop = |cell: usize| -> Option<usize> {
            if layer == 0 {
                return None;
            }
            if hop_stamp[cell] == stamp {
                let target = hop_target[cell];
                (target != NO_TARGET).then_some(target as usize)
            } else {
                Some(cell)
            }
        };
        let view = LayerView {
            layer,
            masks,
            touched: &touched_cur,
            dest: &dest_cur,
            dest_below: &dest_prev,
            hop: &hop,
        };
        observe(&view, cont);
        for &cell in &touched_prev {
            dest_prev[cell] = LOST;
        }
        core::mem::swap(&mut dest_prev, &mut dest_cur);
        core::mem::swap(&mut touched_prev, &mut touched_cur);
    }
}

/// The layer at which a parent first takes up a child's load that landed on it
/// at `landed`, exactly as the down-sweep's `pending` hand-off does: the
/// parent's first read at or below `landed`, one slot lower when that read is
/// an arm's CONTACT slot and there is a slot below it. `None` when the parent
/// never reads there (a stale landing).
fn junction_layer(parent: &SweepStrut, landed: usize, layers: usize) -> Option<usize> {
    if parent.k_bottom == usize::MAX || parent.k_top == 0 {
        return None;
    }
    let top = parent.k_top.min(layers).checked_sub(1)?;
    if top < parent.k_bottom {
        return None;
    }
    // A load that enters a headed arm between its contact and its joint is
    // read at the joint, never above it (v19 fix, 2026-10-08;
    // [`SweepStrut::joint_hold_layer`]).
    let first = parent.joint_hold_layer().map_or(landed.min(top), |joint| landed.min(top).min(joint));
    if first < parent.k_bottom {
        return None;
    }
    let at_contact = parent.kind == StrutKind::Arm && first + 1 == parent.k_top;
    if at_contact && parent.k_bottom < first {
        Some(first - 1)
    } else {
        Some(first)
    }
}

/// One event's work list: each touched member's inputs, popped feeders first.
struct Queue<'a> {
    inputs: &'a mut Vec<Vec<(u32, f64, bool)>>,
    queued: &'a mut Vec<bool>,
    heap: &'a mut BinaryHeap<Reverse<(u32, u32)>>,
    rank: &'a [u32],
}

impl Queue<'_> {
    /// Load `load` enters `member` at `layer`; `routed` when it arrived by the
    /// routing field (a relievable load) rather than from a child.
    fn push(&mut self, member: usize, layer: usize, load: f64, routed: bool) {
        self.inputs[member].push((layer as u32, load, routed));
        if !self.queued[member] {
            self.queued[member] = true;
            self.heap.push(Reverse((self.rank[member], member as u32)));
        }
    }

    fn pop(&mut self) -> Option<usize> {
        let Reverse((_, member)) = self.heap.pop()?;
        let member = member as usize;
        self.queued[member] = false;
        Some(member)
    }
}

/// The per-event design loads of one field.
#[derive(Clone)]
pub(crate) struct PerEventSizing {
    /// Per member, per ledger slot: the largest cumulative force any single
    /// event puts there. Not yet the suffix maximum.
    pub(crate) best: Vec<Vec<f64>>,
    /// `entry(member, layer)` → the largest load ONE event routes into that
    /// member at that layer, for every pair above `ℓ_max`.
    pub(crate) triggers: HashMap<u64, f64>,
    pub(crate) report: PerEventReport,
    /// Per member: whether any event's load reached it.
    pub(crate) reached: Vec<bool>,
}

/// The number of layers a member's ledger spans (the down-sweep's own rule).
pub(crate) fn ledger_len(strut: &SweepStrut, layers: usize) -> usize {
    let top = strut.k_top.min(layers).saturating_sub(1);
    top.saturating_sub(strut.k_bottom) + 1
}

/// Route every event on its own and size every member for its worst one.
///
/// # Panics
///
/// On a per-event conservation residual above 1e-9 relative: a defect in this
/// code, not a statement about a model.
pub(crate) fn size(
    grid: &Grid,
    slices: &[Vec<SliceSolid>],
    struts: &[SweepStrut],
    log: &RouteLog,
    sizing: SizingPolicy,
    limit: &dyn Fn(usize, usize) -> f64,
) -> PerEventSizing {
    let layers = slices.len();
    let members = struts.len();
    let unit = grid.cell_area_mm2() * sizing.alpha;
    let mut best: Vec<Vec<f64>> = struts
        .iter()
        .map(|strut| {
            if strut.k_bottom == usize::MAX {
                Vec::new()
            } else {
                vec![0.0; ledger_len(strut, layers)]
            }
        })
        .collect();
    // Feeders before receivers. A hand-back to the model enters its receiver
    // strictly below the lander's own bottom, and a child's hand-off enters its
    // parent at or below the child's bottom, so the bottom layer, high first,
    // orders both; where a child and its parent end on the same layer, the
    // deeper one in the landing chain goes first.
    let depth: Vec<u32> = (0..members)
        .map(|member| {
            let mut depth = 0_u32;
            let mut at = member;
            while let Landing::Strut { id, .. } = struts[at].landing {
                let parent = id as usize;
                if parent >= members || parent == at || depth as usize > members {
                    break;
                }
                depth += 1;
                at = parent;
            }
            depth
        })
        .collect();
    let mut order: Vec<usize> = (0..members).collect();
    order.sort_by(|&a, &b| {
        struts[b]
            .k_bottom
            .min(layers)
            .cmp(&struts[a].k_bottom.min(layers))
            .then(depth[b].cmp(&depth[a]))
            .then(a.cmp(&b))
    });
    let mut rank = vec![0_u32; members];
    for (position, &member) in order.iter().enumerate() {
        rank[member] = position as u32;
    }

    let mut report = PerEventReport {
        enabled: true,
        ..PerEventReport::default()
    };
    let mut triggers: HashMap<u64, f64> = HashMap::new();
    let mut cont = vec![LOST; members];
    let mut keys: Vec<u64> = Vec::new();
    // Per member, this event's inputs: (layer, load, arrived by routing).
    let mut inputs: Vec<Vec<(u32, f64, bool)>> = vec![Vec::new(); members];
    let mut queued = vec![false; members];
    let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
    // Per member, what it has already handed on in THIS event (`None`: not
    // reached yet), and the members this event reached.
    let mut emitted: Vec<Option<f64>> = vec![None; members];
    let mut touched: Vec<usize> = Vec::new();
    let mut reached = vec![false; members];

    walk(grid, slices, struts, log, &mut cont, |view, cont| {
        keys.clear();
        for &cell in view.touched {
            if view.masks.carry[cell] {
                keys.push(view.dest[cell]);
            }
        }
        if keys.is_empty() {
            return;
        }
        keys.sort_unstable();
        let event_area = keys.len() as f64 * unit;
        report.events += 1;
        report.max_event_mm2 = report.max_event_mm2.max(event_area);
        let mut plate = 0.0;
        let mut lost = 0.0;
        let mut queue = Queue {
            inputs: &mut inputs,
            queued: &mut queued,
            heap: &mut heap,
            rank: &rank,
        };
        let mut start = 0;
        while start < keys.len() {
            let key = keys[start];
            let mut end = start + 1;
            while end < keys.len() && keys[end] == key {
                end += 1;
            }
            let load = (end - start) as f64 * unit;
            match key {
                PLATE => plate += load,
                LOST => lost += load,
                _ => {
                    let (member, layer) = unpack(key);
                    report.routed_entries += 1;
                    queue.push(member, layer, load, true);
                }
            }
            start = end;
        }
        // The order above puts feeders first, so each member is normally
        // popped once. It is not relied on: a member reached again after it
        // was processed re-reads its WHOLE input list (every force can only
        // grow, so the maxima below stay exact) and hands on only what it had
        // not handed on yet, so nothing is counted twice whatever the order.
        while let Some(member) = queue.pop() {
            let strut = &struts[member];
            if emitted[member].is_none() {
                touched.push(member);
                reached[member] = true;
            }
            let mut list = core::mem::take(&mut queue.inputs[member]);
            list.sort_by_key(|input| Reverse(input.0));
            let mut cumulative = 0.0;
            let mut routed_layers = 0_u32;
            let mut routed_total = 0.0;
            let mut routed_largest: f64 = 0.0;
            let mut at = 0;
            while at < list.len() {
                let layer = list[at].0 as usize;
                let mut routed_here = 0.0;
                while at < list.len() && list[at].0 as usize == layer {
                    cumulative += list[at].1;
                    if list[at].2 {
                        routed_here += list[at].1;
                    }
                    at += 1;
                }
                if routed_here > 0.0 {
                    routed_layers += 1;
                    routed_total += routed_here;
                    routed_largest = routed_largest.max(routed_here);
                }
                if routed_here > limit(member, layer) {
                    let slot = triggers.entry(entry(member, layer)).or_insert(0.0);
                    *slot = slot.max(routed_here);
                }
                if let Some(slot) = layer
                    .checked_sub(strut.k_bottom)
                    .and_then(|slot| best[member].get_mut(slot))
                {
                    *slot = slot.max(cumulative);
                }
            }
            let already = emitted[member].unwrap_or(0.0);
            if routed_layers > 1 && emitted[member].is_none() {
                report.same_event_catches += 1;
                report.same_event_catch_gain_max_mm2 = report
                    .same_event_catch_gain_max_mm2
                    .max(routed_total - routed_largest);
            }
            // Keep the inputs until the event ends (a later arrival re-reads
            // them); the buffers are cleared, allocation kept, at event end.
            queue.inputs[member] = list;
            emitted[member] = Some(cumulative);
            let total = cumulative - already;
            // Forces only grow, so the delta is never negative; nothing new to
            // hand on means nothing to do.
            if total <= 0.0 {
                continue;
            }
            match strut.landing {
                Landing::Plate => plate += total,
                Landing::Model { .. } => match cont[member] {
                    PLATE => plate += total,
                    LOST => lost += total,
                    key => {
                        let (receiver, layer) = unpack(key);
                        if receiver == member || receiver >= members {
                            lost += total;
                        } else {
                            report.model_continuations += 1;
                            queue.push(receiver, layer, total, true);
                        }
                    }
                },
                // The child hands its load on at the layer it terminates on,
                // which is its own bottom (the down-sweep's `pending`).
                Landing::Strut { id, .. } => {
                    let parent = id as usize;
                    let at = (parent < members && parent != member)
                        .then(|| junction_layer(&struts[parent], strut.k_bottom, layers))
                        .flatten();
                    match at {
                        Some(layer) => {
                            report.child_feeds += 1;
                            queue.push(parent, layer, total, false);
                        }
                        None => {
                            lost += total;
                            report.stale_landing_mm2 += total;
                        }
                    }
                }
            }
        }
        for member in touched.drain(..) {
            queue.inputs[member].clear();
            emitted[member] = None;
        }
        let residual = ((plate + lost) - event_area).abs();
        assert!(
            residual <= 1.0e-9 * event_area.max(1.0),
            "layer {}: one peel event of {event_area} mm2 reached the plate as {plate} mm2 and \
             lost {lost} mm2; force must be conserved within one peel event",
            view.layer
        );
        report.conservation_residual = report.conservation_residual.max(residual / event_area);
        report.max_event_plate_mm2 = report.max_event_plate_mm2.max(plate);
        report.max_event_uncarried_mm2 = report.max_event_uncarried_mm2.max(lost);
    });
    report.triggers_over_limit = triggers.len();
    PerEventSizing {
        best,
        triggers,
        report,
        reached,
    }
}
