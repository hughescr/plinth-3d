//! Down-sweep reuse probe (X0, 2026-10-10): how much of each per-event
//! `down_sweep::run` a resumed sweep could have skipped.
//!
//! A WRITE-ONLY MEASUREMENT. Nothing here changes a member, a field or a
//! decision; every function returns at once when `perf` counting is off, and
//! the product never reads what it stores. It exists to answer one question
//! before any lane builds an incremental down sweep: of the cell-layers a run
//! walks, what fraction lies ABOVE the highest layer the previous run's edit
//! touched (the prefix a resume skips), and, separately, how many of the
//! (layer, band) regions below that line come out equal anyway (what a
//! regional reuse could skip).
//!
//! # What is compared
//!
//! The previous run in the SAME pose frame (one `run_sweep_with_holes`; the
//! slices are fixed inside it, so two runs differ only by the members). Three
//! figures per run, written as one `down.reuse` row:
//!
//! * `dirty_member`: the number of layers from the bottom up to and including
//!   the highest layer any CHANGED, added or removed member reaches (members
//!   matched by id; a removal renumbers ids, so this over-states the change
//!   after one and is the conservative figure). A resume point at the top of
//!   the changed members, the retained r3 S3 design's rule.
//! * `dirty_layer`: the same count for the highest layer whose live-member
//!   fingerprint changed (id-free, so it is not fooled by a renumbering). At
//!   most `dirty_member`: the permissive figure.
//! * `bands_total`, `bands_equal`: of the (layer, 32-row band) pairs below the
//!   `dirty_layer` line that carry load or an arm in either run, how many have
//!   the same hash of load, carriers and per-arm ledger slots after the
//!   member phase of pass 1. Collected only with `PLINTH_PERF_BANDS=1`,
//!   because the hashing costs real time and would distort the spans.
//!
//! `layers - dirty_*` is the prefix above the line. A first run in a frame has
//! nothing to compare with and is marked `first` (it counts no prefix).

use crate::SweepStrut;
use geom_kernel::perf;

/// Rows per band in the band comparison.
const BAND_ROWS: usize = 32;

#[inline]
fn mix(h: u64, v: u64) -> u64 {
    // The rotation keeps `mix(a, b)` and `mix(b, a)` apart; `h ^ v` alone is
    // symmetric, so two inputs with the same XOR would collide.
    let x = (h.rotate_left(23) ^ v).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^ (x >> 29)
}

/// One run's fingerprints.
#[derive(Default)]
struct Snapshot {
    /// Per member (index = id): fingerprint and the layers up to and including
    /// its top alive layer (`min(k_top, layers)`).
    members: Vec<(u64, usize)>,
    /// Per layer: the commutative sum of the fingerprints of the members
    /// alive there, id left out.
    layer: Vec<u64>,
}

/// The probe's state for one pose frame.
#[derive(Default)]
pub(crate) struct Probe {
    bands_on: Option<bool>,
    previous: Option<Snapshot>,
    /// The previous run's pass-1 band hashes, `[layer * bands + band]`, 0 for
    /// an empty band.
    prev_bands: Vec<u64>,
    /// This run's, being filled by pass 1.
    cur_bands: Vec<u64>,
    bands: usize,
    pending: Option<Pending>,
    /// M0 (2026-10-14): pass- and run-level state, kept across the runs of
    /// the pose frame.
    trace: TraceState,
}

/// What the pass trace remembers between passes and runs of one pose frame.
#[derive(Default)]
struct TraceState {
    run_seq: u64,
    pass_seq: u64,
    pass_in_run: u64,
    run_started: Option<std::time::Instant>,
    /// Counting time of this run's passes (taken out of the run's wall).
    run_count_ns: u64,
    run_layers: u64,
    run_site: u64,
    /// Per-layer fingerprints of the last pass that ended: the base the next
    /// pass compares with (this run's previous pass, or the previous run's
    /// last).
    last_pass_fp: Vec<u64>,
    /// Per-member ledger fingerprints after the last per-event step.
    prev_ledger: Vec<u64>,
    /// The pass row, waiting for the member comparison that follows the pass.
    pending_pass: Option<PassRow>,
    /// Member geometry prints of the previous run, and of this run.
    geom_prev: Option<Vec<u64>>,
    geom_now: Vec<u64>,
    /// Route-log hash of the previous run's last pass.
    route_prev: Option<u64>,
}

/// One pass's row, minus the member comparison.
struct PassRow {
    site: u64,
    run: u64,
    pass_in_run: u64,
    pass_id: u64,
    layers: u64,
    cells: u64,
    wall_ns: u64,
    cmp: u64,
    dirty: u64,
    hi_inserted: u64,
    inserted: u64,
    skip_ns: u64,
}

impl PassRow {
    fn emit(&self, members_changed: u64, members_total: u64) {
        perf::row(
            perf::Rows::DownPass,
            &[
                self.site,
                self.run,
                self.pass_in_run,
                self.pass_id,
                self.layers,
                self.cells,
                self.wall_ns,
                self.cmp,
                self.dirty,
                self.hi_inserted,
                self.inserted,
                self.skip_ns,
                members_changed,
                members_total,
            ],
        );
    }
}

/// What `begin` worked out, until pass 1 ends and the row can be written.
struct Pending {
    site: u64,
    layers: usize,
    cells: usize,
    first: bool,
    dirty_member: usize,
    dirty_layer: usize,
}

fn member_print(strut: &SweepStrut) -> u64 {
    let mut h = mix(0x51ed_270b, strut.kind as u64);
    for value in [
        strut.cell as u64,
        strut.k_top as u64,
        strut.k_bottom as u64,
        strut.min_radius_mm.to_bits(),
        u64::from(strut.seat_on_strut.unwrap_or(u32::MAX)),
        u64::from(strut.inserted_by_load),
        strut.joint_hold_layer().map_or(u64::MAX, |layer| layer as u64),
    ] {
        h = mix(h, value);
    }
    h = match strut.landing {
        crate::Landing::Plate => mix(h, 1),
        crate::Landing::Model { layer } => mix(mix(h, 2), layer as u64),
        crate::Landing::Strut { id, layer } => mix(mix(mix(h, 3), u64::from(id)), layer as u64),
    };
    for cell in &strut.path {
        h = mix(h, u64::from(*cell));
    }
    for radius in &strut.thickened_mm {
        h = mix(h, radius.to_bits());
    }
    h
}

/// A member's GEOMETRY print: what routing reads (kind, position, extent,
/// landing, path, insertion flag), without the sizing inputs
/// (`min_radius_mm`, `thickened_mm`). Two runs whose members have equal
/// geometry prints can differ only in sizing.
fn geom_print(strut: &SweepStrut) -> u64 {
    let mut h = mix(0x6e0_6e0, strut.kind as u64);
    for value in [
        strut.cell as u64,
        strut.k_top as u64,
        strut.k_bottom as u64,
        u64::from(strut.seat_on_strut.unwrap_or(u32::MAX)),
        u64::from(strut.inserted_by_load),
        strut.joint_hold_layer().map_or(u64::MAX, |layer| layer as u64),
    ] {
        h = mix(h, value);
    }
    h = match strut.landing {
        crate::Landing::Plate => mix(h, 1),
        crate::Landing::Model { layer } => mix(mix(h, 2), layer as u64),
        crate::Landing::Strut { id, layer } => mix(mix(mix(h, 3), u64::from(id)), layer as u64),
    };
    for cell in &strut.path {
        h = mix(h, u64::from(*cell));
    }
    h
}

/// A member's identity for the layer fingerprints: everything but its id and
/// its cell at the layer.
fn layer_seed(strut: &SweepStrut) -> u64 {
    mix(
        mix(0x7a11_7a11, strut.kind as u64),
        strut.min_radius_mm.to_bits(),
    )
}

/// Whether the band probe is collecting for this frame.
pub(crate) fn collecting() -> bool {
    perf::with_pose_slot(|probe: &mut Probe| probe.bands_on == Some(true)).unwrap_or(false)
}

/// Fingerprint the members of a per-event run about to start, and compare
/// them with the previous run in this pose frame. `site` is the sizing site
/// (`perf::size_site_peek`).
pub(crate) fn begin(cells: usize, layers: usize, struts: &[SweepStrut], site: u64) {
    if !perf::enabled() {
        return;
    }
    let mut now = Snapshot {
        members: Vec::with_capacity(struts.len()),
        layer: vec![0; layers],
    };
    for strut in struts {
        let print = member_print(strut);
        let top = strut.k_top.min(layers);
        now.members.push((print, top));
        if strut.k_bottom == usize::MAX {
            continue;
        }
        let seed = layer_seed(strut);
        for layer in strut.k_bottom..top {
            let mut h = mix(seed, strut.cell_at(layer) as u64);
            h = mix(h, strut.thickened_mm.get(layer).map_or(0, |r| r.to_bits()));
            now.layer[layer] = now.layer[layer].wrapping_add(h);
        }
    }
    let _count_span = perf::span(perf::Span::CountOverhead);
    let geoms: Vec<u64> = struts.iter().map(geom_print).collect();
    perf::with_pose_slot(|probe: &mut Probe| {
        let trace = &mut probe.trace;
        trace.run_seq += 1;
        trace.pass_in_run = 0;
        trace.run_started = Some(std::time::Instant::now());
        trace.run_count_ns = 0;
        trace.run_layers = layers as u64;
        trace.run_site = site;
        trace.geom_now = geoms;
        probe.bands_on.get_or_insert_with(|| {
            std::env::var_os("PLINTH_PERF_BANDS").is_some_and(|value| value == "1")
        });
        let first = probe.previous.is_none();
        let (mut dirty_member, mut dirty_layer) = (layers, layers);
        if let Some(before) = probe.previous.as_ref() {
            dirty_member = 0;
            for index in 0..before.members.len().max(now.members.len()) {
                let (old, new) = (before.members.get(index), now.members.get(index));
                if old.map(|m| m.0) != new.map(|m| m.0) {
                    let top = old.map_or(0, |m| m.1).max(new.map_or(0, |m| m.1));
                    dirty_member = dirty_member.max(top);
                }
            }
            dirty_layer = (0..layers)
                .rev()
                .find(|&layer| before.layer.get(layer) != now.layer.get(layer))
                .map_or(0, |layer| layer + 1);
        }
        probe.previous = Some(now);
        probe.bands = 0;
        probe.cur_bands.clear();
        probe.pending = Some(Pending {
            site,
            layers,
            cells,
            first,
            dirty_member,
            dirty_layer,
        });
    });
}

/// Size the band store for a pass-1 sweep over `cells` cells of `cols`
/// columns.
pub(crate) fn begin_bands(layers: usize, cells: usize, cols: usize) {
    let rows = cells.checked_div(cols).unwrap_or(0);
    let bands = rows.div_ceil(BAND_ROWS);
    perf::with_pose_slot(|probe: &mut Probe| {
        probe.bands = bands;
        probe.cur_bands.clear();
        probe.cur_bands.resize(layers * bands, 0);
    });
}

/// One layer of pass 1 is done: hash its bands. `arms` are the non-growing
/// arms of the layer as (cell, value) with the value folded from the arm's
/// ledger slot by [`slot_value`].
pub(crate) fn layer_done(
    layer: usize,
    cols: usize,
    load_out: &[f64],
    carriers: &[bool],
    arms: &[(usize, u64)],
) {
    perf::with_pose_slot(|probe: &mut Probe| {
        let bands = probe.bands;
        if bands == 0 || probe.cur_bands.len() < (layer + 1) * bands || cols == 0 {
            return;
        }
        let cells = load_out.len();
        let mut arm_acc = vec![0_u64; bands];
        let mut arm_any = vec![false; bands];
        for &(cell, value) in arms {
            let band = (cell / cols) / BAND_ROWS;
            if band < bands {
                arm_acc[band] = arm_acc[band].wrapping_add(mix(cell as u64, value));
                arm_any[band] = true;
            }
        }
        for band in 0..bands {
            let from = band * BAND_ROWS * cols;
            let to = ((band + 1) * BAND_ROWS * cols).min(cells);
            let mut h = 0_u64;
            let mut loaded = false;
            for cell in from..to {
                let load = load_out[cell];
                if load != 0.0 {
                    h = mix(mix(h, cell as u64), load.to_bits());
                    loaded = true;
                }
                if carriers[cell] {
                    h = mix(h, cell as u64 ^ 0xC0DE);
                }
            }
            probe.cur_bands[layer * bands + band] = if loaded || arm_any[band] {
                mix(h, arm_acc[band]) | 1
            } else {
                0
            };
        }
    });
}

/// A value for an arm's ledger slot, for [`layer_done`].
pub(crate) fn slot_value(carried_mm2: f64, radius_mm: f64) -> u64 {
    mix(carried_mm2.to_bits(), radius_mm.to_bits())
}

/// Pass 1 of the run has finished: compare the bands, charge the counters and
/// write the run's row.
pub(crate) fn end_first_pass() {
    perf::with_pose_slot(|probe: &mut Probe| {
        let Some(p) = probe.pending.take() else {
            return;
        };
        let (mut total, mut equal) = (0_u64, 0_u64);
        let bands = probe.bands;
        if !p.first && bands > 0 && probe.prev_bands.len() == probe.cur_bands.len() {
            // Layers strictly under the highest changed one.
            let below = p.dirty_layer.saturating_sub(1);
            for layer in 0..below.min(p.layers) {
                for band in 0..bands {
                    let (was, is) = (
                        probe.prev_bands[layer * bands + band],
                        probe.cur_bands[layer * bands + band],
                    );
                    if was != 0 || is != 0 {
                        total += 1;
                        equal += u64::from(was == is);
                    }
                }
            }
        }
        if bands > 0 {
            probe.prev_bands = std::mem::take(&mut probe.cur_bands);
        }
        let unchanged = !p.first && p.dirty_member == 0 && p.dirty_layer == 0;
        perf::add(perf::Work::ReuseRuns, 1);
        perf::add(perf::Work::ReuseFirstRuns, u64::from(p.first));
        perf::add(perf::Work::ReuseUnchangedRuns, u64::from(unchanged));
        perf::add(perf::Work::ReuseLayers, perf::count(p.layers));
        if !p.first {
            perf::add(
                perf::Work::ReuseLayersAboveMember,
                perf::count(p.layers.saturating_sub(p.dirty_member)),
            );
            perf::add(
                perf::Work::ReuseLayersAboveLayer,
                perf::count(p.layers.saturating_sub(p.dirty_layer)),
            );
        }
        perf::add(perf::Work::ReuseBandsTotal, total);
        perf::add(perf::Work::ReuseBandsEqual, equal);
        perf::row(
            perf::Rows::DownReuse,
            &[
                p.site,
                perf::count(p.layers),
                perf::count(p.cells),
                u64::from(p.first),
                perf::count(p.dirty_member),
                perf::count(p.dirty_layer),
                total,
                equal,
            ],
        );
    });
}

// ------------------------------------------------------------ pass trace
//
// M0 (2026-10-14): per pass and per layer, what a resumed or windowed
// down-sweep could skip. Write-only like the rest of this file: nothing here
// changes a member, a field or a decision, every entry point returns at once
// when `perf` counting is off, and the product never reads what it stores.

fn ns_since(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// One layer of one pass: time and the per-loop eligible and written cells
/// (see the `down.layer` column notes in `geom_kernel::perf`).
#[derive(Clone, Copy, Default)]
pub(crate) struct LayerRec {
    pub(crate) ns: u64,
    pub(crate) route_ns: u64,
    pub(crate) loop_ns: u64,
    pub(crate) tail_ns: u64,
    pub(crate) route_elig: u64,
    pub(crate) route_written: u64,
    pub(crate) spread_elig: u64,
    pub(crate) max_elig: u64,
    pub(crate) max_written: u64,
    pub(crate) fill_elig: u64,
    pub(crate) copy_elig: u64,
    pub(crate) copy_written: u64,
    /// Carriers this layer inserted, the trigger hosts that asked, and the
    /// insertion round (0-based) of the last insertion.
    pub(crate) inserted: u64,
    pub(crate) hosts: u64,
    pub(crate) round: u64,
    /// Fingerprint of what the layer hands down (load and carriers).
    pub(crate) fp: u64,
    /// Time the counting itself took inside this layer; taken out of `ns`.
    pub(crate) count_ns: u64,
}

/// A pass's trace, built by `sweep_pass` only while counting is on.
pub(crate) struct PassTrace {
    /// The layer being walked.
    pub(crate) cur: LayerRec,
    recs: Vec<LayerRec>,
    cells: usize,
    started: std::time::Instant,
    layer_started: std::time::Instant,
}

impl PassTrace {
    /// `None` when counting is off.
    pub(crate) fn begin(layers: usize, cells: usize) -> Option<Self> {
        if !perf::enabled() {
            return None;
        }
        let now = std::time::Instant::now();
        Some(Self {
            cur: LayerRec::default(),
            recs: vec![LayerRec::default(); layers],
            cells,
            started: now,
            layer_started: now,
        })
    }

    pub(crate) fn layer_begin(&mut self) {
        self.cur = LayerRec::default();
        self.layer_started = std::time::Instant::now();
    }

    /// Run measurement-only work `f` on the current layer's record, charging
    /// its time to the layer's `count_ns` and to [`perf::Span::CountOverhead`]
    /// so that no time column includes it.
    pub(crate) fn counting<R>(&mut self, f: impl FnOnce(&mut LayerRec) -> R) -> R {
        let _span = perf::span(perf::Span::CountOverhead);
        let started = std::time::Instant::now();
        let out = f(&mut self.cur);
        self.cur.count_ns += ns_since(started);
        out
    }

    pub(crate) fn layer_end(&mut self, layer: usize) {
        self.cur.ns = ns_since(self.layer_started).saturating_sub(self.cur.count_ns);
        if let Some(slot) = self.recs.get_mut(layer) {
            *slot = self.cur;
        }
    }

    /// The pass is over: write the layer rows, compare with the base pass,
    /// and park the pass row until the member comparison
    /// ([`pass_done`]) completes it.
    pub(crate) fn finish(self) {
        let layers = self.recs.len();
        // The pass's wall time without the counting inside it.
        let counted: u64 = self.recs.iter().map(|rec| rec.count_ns).sum();
        let wall_ns = ns_since(self.started).saturating_sub(counted);
        let cells = self.cells as u64;
        let recs = self.recs;
        let _count_span = perf::span(perf::Span::CountOverhead);
        perf::with_pose_slot(|probe: &mut Probe| {
            let trace = &mut probe.trace;
            if let Some(stale) = trace.pending_pass.take() {
                stale.emit(0, 0);
            }
            trace.pass_seq += 1;
            trace.pass_in_run += 1;
            trace.run_count_ns += counted;
            let pass_id = trace.pass_seq;
            let fps: Vec<u64> = recs.iter().map(|rec| rec.fp).collect();
            let (cmp, dirty) = if trace.last_pass_fp.len() == layers && layers > 0 {
                let dirty = (0..layers)
                    .rev()
                    .find(|&layer| trace.last_pass_fp[layer] != fps[layer])
                    .map_or(0, |layer| layer + 1);
                (if trace.pass_in_run > 1 { 1 } else { 2 }, dirty)
            } else {
                (0, layers)
            };
            // The prefix above the highest differing layer, in measured time.
            let skip_ns: u64 = recs.iter().skip(dirty).map(|rec| rec.ns).sum();
            let mut hi_inserted = 0_u64;
            let mut inserted = 0_u64;
            for (layer, rec) in recs.iter().enumerate() {
                perf::row(
                    perf::Rows::DownLayer,
                    &[
                        pass_id,
                        layer as u64,
                        rec.ns,
                        rec.route_ns,
                        rec.loop_ns,
                        rec.tail_ns,
                        rec.route_elig,
                        rec.route_written,
                        rec.spread_elig,
                        rec.max_elig,
                        rec.max_written,
                        rec.fill_elig,
                        rec.copy_elig,
                        rec.copy_written,
                    ],
                );
                if rec.inserted > 0 {
                    hi_inserted = hi_inserted.max(layer as u64 + 1);
                    inserted += rec.inserted;
                    perf::row(
                        perf::Rows::DownInsert,
                        &[pass_id, layer as u64, rec.inserted, rec.hosts, rec.round],
                    );
                }
            }
            perf::add(perf::Work::PassLayers, layers as u64);
            if cmp > 0 {
                perf::add(perf::Work::PassLayersReusable, (layers - dirty) as u64);
            }
            trace.last_pass_fp = fps;
            trace.pending_pass = Some(PassRow {
                site: trace.run_site,
                run: trace.run_seq,
                pass_in_run: trace.pass_in_run,
                pass_id,
                layers: layers as u64,
                cells,
                wall_ns,
                cmp,
                dirty: dirty as u64,
                hi_inserted,
                inserted,
                skip_ns,
            });
        });
    }
}

/// Fingerprint of what a layer hands to the one below it: the cells that
/// carry load (with the load's bits) and the carrier cells.
pub(crate) fn layer_fingerprint(load_out: &[f64], carriers: &[bool]) -> u64 {
    let mut h = 0x1a7e_4f1d_u64;
    for (cell, (&load, &carrier)) in load_out.iter().zip(carriers).enumerate() {
        if load != 0.0 {
            h = mix(mix(h, cell as u64), load.to_bits());
        }
        if carrier {
            h = mix(h, cell as u64 ^ 0xC0DE);
        }
    }
    h
}

/// The per-event step has rewritten the members' ledgers: count the members
/// whose ledger differs from the previous pass's (or the previous run's last
/// pass, for pass 1) and write the pass row.
pub(crate) fn pass_done(struts: &[SweepStrut]) {
    if !perf::enabled() {
        return;
    }
    let _count_span = perf::span(perf::Span::CountOverhead);
    perf::with_pose_slot(|probe: &mut Probe| {
        let trace = &mut probe.trace;
        let Some(row) = trace.pending_pass.take() else {
            return;
        };
        let now: Vec<u64> = struts
            .iter()
            .map(|strut| {
                let mut h = mix(0x1ed6e4, strut.radius_mm.len() as u64);
                for value in strut.radius_mm.iter().chain(&strut.carried_mm2) {
                    h = mix(h, value.to_bits());
                }
                h
            })
            .collect();
        let changed = now
            .iter()
            .enumerate()
            .filter(|(index, print)| trace.prev_ledger.get(*index) != Some(*print))
            .count() as u64;
        let total = now.len() as u64;
        trace.prev_ledger = now;
        row.emit(changed, total);
    });
}

/// A hash of a run's final routing record, for the route-equality figure.
pub(crate) fn route_hash(log: &crate::per_event::RouteLog) -> u64 {
    let mut h = 0x20e7_0001_u64;
    for (layer, (hops, reads)) in log.hops.iter().zip(&log.reads).enumerate() {
        h = mix(h, layer as u64);
        for &(a, b) in hops {
            h = mix(mix(h, u64::from(a)), u64::from(b));
        }
        h = mix(h, 0xBEEF);
        for &(a, b) in reads {
            h = mix(mix(h, u64::from(a)), u64::from(b));
        }
    }
    h
}

/// The run is over: write its row (passes, wall time, whether its routing
/// and its member geometry equal the previous run's in this pose).
pub(crate) fn run_end(passes: usize, inserted: u64, log: &crate::per_event::RouteLog) {
    if !perf::enabled() {
        return;
    }
    let _count_span = perf::span(perf::Span::CountOverhead);
    let route = route_hash(log);
    perf::with_pose_slot(|probe: &mut Probe| {
        let trace = &mut probe.trace;
        if let Some(stale) = trace.pending_pass.take() {
            stale.emit(0, 0);
        }
        let verdict = |same: Option<bool>| same.map_or(2, u64::from);
        let route_same = verdict(trace.route_prev.map(|before| before == route));
        let geom_same = verdict(
            trace
                .geom_prev
                .as_ref()
                .map(|before| *before == trace.geom_now),
        );
        trace.route_prev = Some(route);
        trace.geom_prev = Some(std::mem::take(&mut trace.geom_now));
        let wall = trace
            .run_started
            .take()
            .map_or(0, ns_since)
            .saturating_sub(trace.run_count_ns);
        perf::row(
            perf::Rows::DownRun,
            &[
                trace.run_site,
                trace.run_seq,
                passes as u64,
                wall,
                route_same,
                geom_same,
                inserted,
                trace.run_layers,
            ],
        );
    });
}

#[cfg(test)]
mod tests {
    use super::{layer_fingerprint, member_print, mix};
    use crate::{Landing, SeatFace, StrutKind, SweepStrut};

    fn strut(id: u32, k_top: usize) -> SweepStrut {
        SweepStrut {
            id,
            cell: 10 + id as usize,
            center_mm: [0.0, 0.0],
            kind: StrutKind::Arm,
            min_radius_mm: 0.0,
            thickened_mm: Vec::new(), foot_radius_mm: None,
            seat_on_strut: None,
            k_top,
            k_bottom: 0,
            landing: Landing::Plate,
            interior: false,
            radius_mm: Vec::new(),
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(), held_births: Vec::new(), twin_host: false,
            inserted_by_load: false,
            path: Vec::new(),
            seat_face: SeatFace::default(),
            seat_normal: [0.0; 3],
            straight_fallback: None,
            fallback_path: Vec::new(),
            head: None,
        }
    }

    /// The fingerprint sees every input the sweep reads and none of its
    /// outputs (a sized ledger must not look like an edit).
    #[test]
    fn a_member_print_moves_with_inputs_and_not_with_outputs() {
        let base = strut(3, 20);
        let mut sized = base.clone();
        sized.radius_mm = vec![0.5; 20];
        sized.carried_mm2 = vec![1.0; 20];
        assert_eq!(member_print(&base), member_print(&sized), "outputs are not inputs");
        let mut moved = base.clone();
        moved.cell += 1;
        assert_ne!(member_print(&base), member_print(&moved));
        let mut taller = base.clone();
        taller.k_top += 1;
        assert_ne!(member_print(&base), member_print(&taller));
        let mut thick = base.clone();
        thick.thickened_mm = vec![0.2; 4];
        assert_ne!(member_print(&base), member_print(&thick));
        assert_ne!(mix(1, 2), mix(2, 1));
    }

    /// The layer fingerprint sees a load moved one cell, a load changed by
    /// one bit and a carrier added, and nothing else.
    #[test]
    fn a_layer_fingerprint_moves_with_load_and_carriers() {
        let load = [0.0, 1.5, 0.0, 2.0];
        let carriers = [false, true, false, false];
        let base = layer_fingerprint(&load, &carriers);
        assert_eq!(base, layer_fingerprint(&load, &carriers));
        assert_ne!(base, layer_fingerprint(&[0.0, 0.0, 1.5, 2.0], &carriers));
        assert_ne!(base, layer_fingerprint(&[0.0, 1.5 + f64::EPSILON, 0.0, 2.0], &carriers));
        assert_ne!(base, layer_fingerprint(&load, &[false, true, true, false]));
        assert_ne!(
            layer_fingerprint(&[0.0; 4], &[false; 4]),
            layer_fingerprint(&[0.0; 4], &[true, false, false, false])
        );
    }
}
