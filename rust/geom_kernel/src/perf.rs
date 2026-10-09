//! Write-only work counters, phase wall time and span timers for performance
//! rounds (`plinth prepare --perf DIR`).
//!
//! # What this is, and what it is not
//!
//! A measurement harness for the speed rounds. It answers "how much LOGICAL
//! work did this run do, in which phase" with exact integers, so a change that
//! removes work can be proved by a count that falls while the emitted bytes
//! stay equal, even when the wall-clock gain is below the noise.
//!
//! # Contract
//!
//! * **Write-only.** No getter, snapshot type or reader is public. Only this
//!   module serialises, so product code cannot branch on a count or a clock.
//!   (The one exception is the [`FrameTally`] hand-back, which carries a
//!   count from the thread that made it to the place that classifies it and
//!   is never inspected.)
//! * **Near-zero cost when off.** [`add`] is one `Relaxed` load of a static
//!   flag. Nothing is written, no thread starts and no clock is read until
//!   [`enable`] is called, which only the CLI does, from `--perf DIR` or
//!   `PLINTH_PERF_DIR`.
//! * **Batch-first charging.** A kernel counts into local integers and calls
//!   [`add`] once per query, call or batch, never inside a per-triangle or
//!   per-cell loop. A brute-force scan is charged as `rows * faces` before its
//!   `par_iter`.
//! * **Result-inert and deterministic.** Integer sums commute, so shard totals
//!   do not depend on which worker charged what. Each charge depends only on
//!   logical work. Spans and phase wall time are time: reported, never
//!   compared. A counter whose value depends on the thread count (speculative
//!   work) is [`Class::Sched`] and is never compared across widths.
//! * **Attribution.** The phase is one global byte, set by the pipeline's main
//!   thread after a join only; a worker reads the phase of its batch.
//!
//! # Sidecar
//!
//! `<dir>/<model>.perf.json`, schema `plinth-perf/1`, written (atomically,
//! temp file then rename) at every phase change as `"partial": true` and at
//! [`end_model`] as `"partial": false`. A failed write is a WARNING on stderr,
//! never a failure of the model. The sidecar is outside the model's output
//! folder, so no digest sees it.
//!
//! # Adding a counter
//!
//! Append one line to `works!` below, then call [`add`] from a file that owns
//! the site. Order is the sidecar order; append-only keeps diffs readable.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering::Relaxed as Relax;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize};
use std::sync::Mutex;
use std::time::Instant;

/// Whether a counter is exact logical work (compared across runs and thread
/// counts) or depends on scheduling (recorded, never compared).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Exact. Equal at every thread count; compared against a baseline.
    Work,
    /// Width- or schedule-dependent (speculative batches). Never compared.
    Sched,
}

/// Where in a model's run a charge happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    Load,
    Repair,
    Decimate,
    OrientTable,
    Prefilter,
    Production,
    Hollow,
    SealFields,
    FinalSweep,
    Emit,
    Other,
}

const PHASE_N: usize = 11;

impl Phase {
    /// Every phase, in run order.
    pub const ALL: [Self; PHASE_N] = [
        Self::Load,
        Self::Repair,
        Self::Decimate,
        Self::OrientTable,
        Self::Prefilter,
        Self::Production,
        Self::Hollow,
        Self::SealFields,
        Self::FinalSweep,
        Self::Emit,
        Self::Other,
    ];

    /// The sidecar label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Load => "Load",
            Self::Repair => "Repair",
            Self::Decimate => "Decimate",
            Self::OrientTable => "OrientTable",
            Self::Prefilter => "Prefilter",
            Self::Production => "Production",
            Self::Hollow => "Hollow",
            Self::SealFields => "SealFields",
            Self::FinalSweep => "FinalSweep",
            Self::Emit => "Emit",
            Self::Other => "Other",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }

    const fn from_index(index: u8) -> Self {
        match index {
            0 => Self::Load,
            1 => Self::Repair,
            2 => Self::Decimate,
            3 => Self::OrientTable,
            4 => Self::Prefilter,
            5 => Self::Production,
            6 => Self::Hollow,
            7 => Self::SealFields,
            8 => Self::FinalSweep,
            9 => Self::Emit,
            _ => Self::Other,
        }
    }
}

/// Declares [`Work`]: variant, sidecar label, class. Append only.
macro_rules! works {
    ($( $(#[$doc:meta])* $variant:ident, $label:literal, $class:ident; )*) => {
        /// One named counter.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(u16)]
        pub enum Work {
            $( $(#[$doc])* $variant, )*
        }

        impl Work {
            /// Every counter, in declaration (sidecar) order.
            pub const ALL: &'static [Self] = &[ $( Self::$variant, )* ];

            /// The sidecar label.
            #[must_use]
            pub const fn label(self) -> &'static str {
                match self { $( Self::$variant => $label, )* }
            }

            /// Exact work or schedule-dependent.
            #[must_use]
            pub const fn class(self) -> Class {
                match self { $( Self::$variant => Class::$class, )* }
            }
        }
    };
}

works! {
    /// Welded vertices with a nonzero curvature, each spread once
    /// (`delicacy_eval::curvature_by_face`).
    DelicacyVerticesSpread, "delicacy.vertices_spread", Work;
    /// `(vertex, face)` pairs whose ball-face intersection was measured.
    DelicacyPairsMeasured, "delicacy.pairs_measured", Work;
    /// Pairs that received a nonzero share of a vertex's curvature.
    DelicacyPairsCharged, "delicacy.pairs_charged", Work;
    /// Non-degenerate faces whose detail neighbourhood the display pass walked.
    DisplayFaces, "display.faces", Work;
    /// Faces examined (frontier entries) across all display neighbourhood walks.
    DisplayPairsMeasured, "display.pairs_measured", Work;
    /// Segment-to-surface branch-and-bound walks (clearance rows).
    SegWalkQueries, "kernel.seg_walk.queries", Work;
    /// BVH nodes popped by those walks.
    SegWalkNodesVisited, "kernel.seg_walk.nodes_visited", Work;
    /// BVH nodes popped and pruned by the box bound.
    SegWalkNodesPruned, "kernel.seg_walk.nodes_pruned", Work;
    /// Faces whose segment distance those walks computed.
    SegWalkTrisTested, "kernel.seg_walk.tris_tested", Work;
    /// Tip landings put to the nearest-face scan (`tip_landing`).
    NearestTipRows, "nearest.tip.rows", Work;
    /// Landing-times-face pairs of that scan (`rows * faces`).
    NearestTipTrisTested, "nearest.tip.tris_tested", Work;
    /// Rows of the model-landing scar's nearest-face scan.
    NearestModelLandingRows, "nearest.model_landing.rows", Work;
    /// `rows * faces` of that scan.
    NearestModelLandingTrisTested, "nearest.model_landing.tris_tested", Work;
    /// Rows of the footprint-normal scans (ball scan and nearest admitted face).
    NearestFootprintNormalRows, "nearest.footprint_normal.rows", Work;
    /// `rows * faces` of those scans.
    NearestFootprintNormalTrisTested, "nearest.footprint_normal.tris_tested", Work;
    /// `slice_stack` calls.
    SliceStackCalls, "slice.stack_calls", Work;
    /// Planes sliced by those calls.
    SliceLayers, "slice.layers", Work;
    /// Kernel surfaces built by `slice_stack`.
    SliceSurfaceBuilds, "slice.surface_builds", Work;
    /// `run_sweep_with_holes` calls.
    SweepRuns, "sweep.runs", Work;
    /// Down-sweep calls, per call site in `run_sweep_with_holes`: the first sizing.
    SweepDownCallInitial, "sweep.down.calls.initial", Work;
    /// ... the under-loaded removal loop.
    SweepDownCallRemoval, "sweep.down.calls.removal", Work;
    /// ... after merges the trunk cannot carry are undone.
    SweepDownCallMergesUndone, "sweep.down.calls.merges_undone", Work;
    /// ... after the congestion lane.
    SweepDownCallCongestion, "sweep.down.calls.congestion", Work;
    /// ... the congestion lane's trial without the inserted arms.
    SweepDownCallCongestionTrial, "sweep.down.calls.congestion_trial", Work;
    /// ... after a stranded-seat branch round.
    SweepDownCallStrandedRound, "sweep.down.calls.stranded_round", Work;
    /// ... after stranded-seat branches are put back.
    SweepDownCallStrandedPutBack, "sweep.down.calls.stranded_put_back", Work;
    /// ... after the slenderness lane's forced merges.
    SweepDownCallForcedMerges, "sweep.down.calls.forced_merges", Work;
    /// ... after the un-merge of overgrown forced merges.
    SweepDownCallUnmergeAfterMerges, "sweep.down.calls.unmerge_after_merges", Work;
    /// ... after graze heads.
    SweepDownCallGraze, "sweep.down.calls.graze", Work;
    /// ... after a head-join pass.
    SweepDownCallHeadJoin, "sweep.down.calls.head_join", Work;
    /// ... inside the lip-seat rule's sizing (`LipSeatRun::size`).
    SweepDownCallLipSeat, "sweep.down.calls.lip_seat", Work;
    /// ... after a tie pass.
    SweepDownCallTies, "sweep.down.calls.ties", Work;
    /// ... after a model-tie pass.
    SweepDownCallModelTies, "sweep.down.calls.model_ties", Work;
    /// Top-down passes (`down_sweep::sweep_pass`).
    SweepDownPasses, "sweep.down.passes", Work;
    /// `cells * layers` of those passes.
    SweepDownCellLayers, "sweep.down.cell_layers", Work;
    /// Cells of the reach chamfer distance transform.
    ReachChamferCells, "reach.chamfer_cells", Work;
    /// Demand-replay passes.
    ReplayCalls, "replay.calls", Work;
    /// `cells * layers` of those passes.
    ReplayCellLayers, "replay.cell_layers", Work;
    /// Head-join passes that moved at least one lander (loop iterations).
    SweepHeadJoinPasses, "sweep.head_join_passes", Work;
    /// Tie-ladder passes that applied something (fixed-point iterations).
    SweepTiePasses, "sweep.tie_passes", Work;
    /// Lip-seat passes.
    LipSeatPasses, "lip_seat.passes", Work;
    /// Lip-seat whole-run restarts.
    LipSeatReruns, "lip_seat.reruns", Work;
    /// Arms put to the lip-seat solid-clearance batch (`rows`).
    LipSeatClearanceRows, "lip_seat.clearance_rows", Work;
    /// Brace partner searches (`partner_candidates` calls).
    BracePartnerCalls, "brace.partner_calls", Work;
    /// Members scanned by those searches.
    BracePartnerScanned, "brace.partner_scanned", Work;
    /// Candidates they returned.
    BracePartnerReturned, "brace.partner_returned", Work;
    /// Rounds of the tie ladder's per-pass fixed point.
    BraceRounds, "brace.rounds", Work;
    /// Frame solves the serial algorithm performs (committed).
    FrameSolvesCommitted, "frame.solves.committed", Work;
    /// Nodes of those solves.
    FramePointsCommitted, "frame.points.committed", Work;
    /// Frame solves of a deferred-pair batch past the pair the scan takes;
    /// the batch is sized by the thread count, so this depends on width.
    FrameSolvesSpeculative, "frame.solves.speculative", Sched;
    /// Nodes of those solves.
    FramePointsSpeculative, "frame.points.speculative", Sched;
    /// Stage-shake checks run on a finished field.
    ShakeChecks, "shake.checks", Work;
    /// Growing stages those checks examined.
    ShakeStages, "shake.stages", Work;
    /// Winner-plan cache hits (zero until round 1 wires them).
    CacheWinnerPlanHit, "cache.winner_plan.hit", Work;
    /// Winner-plan cache misses (zero until round 1 wires them).
    CacheWinnerPlanMiss, "cache.winner_plan.miss", Work;
}

const WORK_N: usize = Work::ALL.len();

/// A timed region. The sidecar reports calls, summed thread time and the
/// longest call, per phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Span {
    /// One `run_sweep_with_holes`.
    SweepRun,
    /// One `slice_stack`.
    SliceStack,
    /// One stage-shake check.
    StageShake,
    /// One delicacy curvature spread.
    Curvature,
    /// One display neighbourhood pass.
    DisplayPass,
}

const SPAN_N: usize = 5;

impl Span {
    /// Every span.
    pub const ALL: [Self; SPAN_N] = [
        Self::SweepRun,
        Self::SliceStack,
        Self::StageShake,
        Self::Curvature,
        Self::DisplayPass,
    ];

    /// The sidecar label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::SweepRun => "sweep.run",
            Self::SliceStack => "slice.stack",
            Self::StageShake => "shake.check",
            Self::Curvature => "delicacy.curvature",
            Self::DisplayPass => "display.neighbourhood",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

// ---------------------------------------------------------------- storage

const SHARDS: usize = 64;

struct SpanCell {
    calls: AtomicU64,
    ns: AtomicU64,
    max_ns: AtomicU64,
}

impl SpanCell {
    const fn new() -> Self {
        Self {
            calls: AtomicU64::new(0),
            ns: AtomicU64::new(0),
            max_ns: AtomicU64::new(0),
        }
    }
}

/// One thread's cells, aligned so two shards never share a cache line.
#[repr(align(128))]
struct Shard {
    work: [[AtomicU64; WORK_N]; PHASE_N],
    spans: [[SpanCell; SPAN_N]; PHASE_N],
}

impl Shard {
    const fn new() -> Self {
        Self {
            work: [const { [const { AtomicU64::new(0) }; WORK_N] }; PHASE_N],
            spans: [const { [const { SpanCell::new() }; SPAN_N] }; PHASE_N],
        }
    }

    fn zero(&self) {
        for row in &self.work {
            for cell in row {
                cell.store(0, Relax);
            }
        }
        for row in &self.spans {
            for cell in row {
                cell.calls.store(0, Relax);
                cell.ns.store(0, Relax);
                cell.max_ns.store(0, Relax);
            }
        }
    }
}

static TABLE: [Shard; SHARDS] = [const { Shard::new() }; SHARDS];
static ENABLED: AtomicBool = AtomicBool::new(false);
static PHASE: AtomicU8 = AtomicU8::new(Phase::Other as u8);
static NEXT_SHARD: AtomicUsize = AtomicUsize::new(0);
static WARNED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static MY_SHARD: Cell<usize> = const { Cell::new(usize::MAX) };
    /// `(solves, points)` while a [`capture_frames`] scope is open on this thread.
    static CAPTURE: Cell<Option<(u64, u64)>> = const { Cell::new(None) };
}

fn my_shard() -> &'static Shard {
    let slot = MY_SHARD.with(|cell| {
        let mut slot = cell.get();
        if slot == usize::MAX {
            slot = NEXT_SHARD.fetch_add(1, Relax) % SHARDS;
            cell.set(slot);
        }
        slot
    });
    &TABLE[slot]
}

struct ModelSlot {
    model: String,
    dir: PathBuf,
    started: Instant,
    phase_since: Instant,
    phase_wall_ns: [u64; PHASE_N],
}

static DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
static SLOT: Mutex<Option<ModelSlot>> = Mutex::new(None);

// -------------------------------------------------------------------- API

/// Switch counting on and name the sidecar folder. Called by the CLI only.
pub fn enable(dir: PathBuf) {
    if let Ok(mut slot) = DIR.lock() {
        *slot = Some(dir);
    }
    ENABLED.store(true, Relax);
}

/// Whether counting is on. One `Relaxed` load.
#[inline]
#[must_use]
pub fn enabled() -> bool {
    ENABLED.load(Relax)
}

/// Start a model: zero every counter, enter [`Phase::Load`], name the sidecar.
pub fn begin_model(model: &str) {
    if !enabled() {
        return;
    }
    let Some(dir) = DIR.lock().ok().and_then(|slot| slot.clone()) else {
        return;
    };
    for shard in &TABLE {
        shard.zero();
    }
    PHASE.store(Phase::Load as u8, Relax);
    let now = Instant::now();
    if let Ok(mut slot) = SLOT.lock() {
        *slot = Some(ModelSlot {
            model: model.to_owned(),
            dir,
            started: now,
            phase_since: now,
            phase_wall_ns: [0; PHASE_N],
        });
    }
}

/// Enter `phase` and return the one it replaces. Call from the pipeline's
/// main thread after a join only. Writes a partial sidecar.
pub fn set_phase(phase: Phase) -> Phase {
    let previous = Phase::from_index(PHASE.load(Relax));
    if !enabled() {
        return previous;
    }
    let now = Instant::now();
    if let Ok(mut guard) = SLOT.lock() {
        if let Some(slot) = guard.as_mut() {
            let spent = ns_between(slot.phase_since, now);
            slot.phase_wall_ns[previous.index()] =
                slot.phase_wall_ns[previous.index()].saturating_add(spent);
            slot.phase_since = now;
            PHASE.store(phase as u8, Relax);
            write_sidecar(slot, true, now);
        }
    }
    previous
}

/// Finish the model: write the final sidecar (`"partial": false`).
pub fn end_model() {
    if !enabled() {
        return;
    }
    let now = Instant::now();
    let taken = SLOT.lock().ok().and_then(|mut guard| guard.take());
    if let Some(mut slot) = taken {
        let active = Phase::from_index(PHASE.load(Relax));
        let spent = ns_between(slot.phase_since, now);
        slot.phase_wall_ns[active.index()] =
            slot.phase_wall_ns[active.index()].saturating_add(spent);
        slot.phase_since = now;
        write_sidecar(&slot, false, now);
    }
}

/// Add `n` to counter `work` in the current phase. Returns at once when off.
#[inline]
pub fn add(work: Work, n: u64) {
    if !ENABLED.load(Relax) {
        return;
    }
    add_enabled(work, n);
}

#[inline(never)]
fn add_enabled(work: Work, n: u64) {
    let phase = PHASE.load(Relax) as usize;
    my_shard().work[phase.min(PHASE_N - 1)][work as usize].fetch_add(n, Relax);
}

/// Widen a count for [`add`].
#[inline]
#[must_use]
pub fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// Time a region; the time is charged when the guard drops. `None` inside (no
/// clock read) when off.
#[inline]
pub fn span(span: Span) -> SpanGuard {
    if ENABLED.load(Relax) {
        SpanGuard(Some((span, Phase::from_index(PHASE.load(Relax)), Instant::now())))
    } else {
        SpanGuard(None)
    }
}

/// Charges its region's time to a [`Span`] on drop.
#[must_use = "a span measures until it is dropped"]
pub struct SpanGuard(Option<(Span, Phase, Instant)>);

impl Drop for SpanGuard {
    fn drop(&mut self) {
        if let Some((span, phase, started)) = self.0.take() {
            let ns = ns_between(started, Instant::now());
            let cell = &my_shard().spans[phase.index()][span.index()];
            cell.calls.fetch_add(1, Relax);
            cell.ns.fetch_add(ns, Relax);
            cell.max_ns.fetch_max(ns, Relax);
        }
    }
}

/// Frame-solve counts carried from the thread that made them to the place
/// that knows whether the work was committed or speculative.
#[derive(Clone, Copy, Debug, Default)]
#[must_use = "commit or speculate the tally, or the solves are never counted"]
pub struct FrameTally {
    solves: u64,
    points: u64,
}

impl FrameTally {
    /// The solves were part of the work the serial algorithm does.
    pub fn commit(self) {
        add(Work::FrameSolvesCommitted, self.solves);
        add(Work::FramePointsCommitted, self.points);
    }

    /// The solves ran only because a batch was sized by the thread count.
    pub fn speculate(self) {
        add(Work::FrameSolvesSpeculative, self.solves);
        add(Work::FramePointsSpeculative, self.points);
    }
}

/// One frame solve of `points` nodes. Charged as committed, unless a
/// [`capture_frames`] scope is open on this thread, which holds it back.
#[inline]
pub fn frame_solve(points: usize) {
    if !ENABLED.load(Relax) {
        return;
    }
    let points = count(points);
    let held = CAPTURE.with(|cell| match cell.get() {
        Some((solves, held)) => {
            cell.set(Some((solves + 1, held + points)));
            true
        }
        None => false,
    });
    if !held {
        add_enabled(Work::FrameSolvesCommitted, 1);
        add_enabled(Work::FramePointsCommitted, points);
    }
}

/// Run `f`; the frame solves it makes on this thread are held back and
/// returned, for the caller to [`FrameTally::commit`] or
/// [`FrameTally::speculate`]. Off, it just runs `f`.
pub fn capture_frames<R>(f: impl FnOnce() -> R) -> (R, FrameTally) {
    if !ENABLED.load(Relax) {
        return (f(), FrameTally::default());
    }
    let outer = CAPTURE.with(|cell| cell.replace(Some((0, 0))));
    let result = f();
    let (solves, points) = CAPTURE
        .with(|cell| cell.replace(outer))
        .unwrap_or((0, 0));
    (result, FrameTally { solves, points })
}

// ------------------------------------------------------------ serialising

fn ns_between(from: Instant, to: Instant) -> u64 {
    u64::try_from(to.saturating_duration_since(from).as_nanos()).unwrap_or(u64::MAX)
}

/// Sums over all shards: `work[phase][counter]` and `spans[phase][span]`.
type WorkSums = Vec<Vec<u64>>;
type SpanSums = Vec<Vec<(u64, u64, u64)>>;

fn sum_shards() -> (WorkSums, SpanSums) {
    let mut work = vec![vec![0_u64; WORK_N]; PHASE_N];
    let mut spans = vec![vec![(0_u64, 0_u64, 0_u64); SPAN_N]; PHASE_N];
    for shard in &TABLE {
        for (phase, row) in shard.work.iter().enumerate() {
            for (index, cell) in row.iter().enumerate() {
                work[phase][index] += cell.load(Relax);
            }
        }
        for (phase, row) in shard.spans.iter().enumerate() {
            for (index, cell) in row.iter().enumerate() {
                let sum = &mut spans[phase][index];
                sum.0 += cell.calls.load(Relax);
                sum.1 += cell.ns.load(Relax);
                sum.2 = sum.2.max(cell.max_ns.load(Relax));
            }
        }
    }
    (work, spans)
}

fn ms(ns: u64) -> String {
    // Whole nanoseconds to milliseconds with three decimals, integer math.
    format!("{}.{:03}", ns / 1_000_000, (ns / 1_000) % 1_000)
}

fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn work_object(row: &[u64], class: Class, only_nonzero: bool) -> String {
    let mut parts = Vec::new();
    for &w in Work::ALL {
        let n = row[w as usize];
        if w.class() == class && (!only_nonzero || n > 0) {
            parts.push(format!("{}:{n}", json_string(w.label())));
        }
    }
    format!("{{{}}}", parts.join(","))
}

fn serialise(slot: &ModelSlot, partial: bool, now: Instant) -> String {
    let (work, spans) = sum_shards();
    let active = Phase::from_index(PHASE.load(Relax));
    let mut phases = Vec::new();
    let mut totals = vec![0_u64; WORK_N];
    for phase in Phase::ALL {
        let row = &work[phase.index()];
        for (total, n) in totals.iter_mut().zip(row) {
            *total += n;
        }
        let mut wall_ns = slot.phase_wall_ns[phase.index()];
        if partial && phase == active {
            wall_ns = wall_ns.saturating_add(ns_between(slot.phase_since, now));
        }
        let mut span_parts = Vec::new();
        for s in Span::ALL {
            let (calls, ns, max_ns) = spans[phase.index()][s.index()];
            if calls > 0 {
                span_parts.push(format!(
                    "{}:{{\"calls\":{calls},\"thread_ms\":{},\"max_ms\":{}}}",
                    json_string(s.label()),
                    ms(ns),
                    ms(max_ns)
                ));
            }
        }
        if wall_ns == 0 && row.iter().all(|&n| n == 0) && span_parts.is_empty() {
            continue;
        }
        phases.push(format!(
            "{{\"phase\":{},\"wall_ms\":{},\"work\":{},\"sched\":{},\"spans\":{{{}}}}}",
            json_string(phase.label()),
            ms(wall_ns),
            work_object(row, Class::Work, true),
            work_object(row, Class::Sched, true),
            span_parts.join(",")
        ));
    }
    let unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    format!(
        "{{\"schema\":\"plinth-perf/1\",\"model\":{},\"partial\":{partial},\"threads\":{},\
         \"checkpoint_unix_ms\":{unix_ms},\"active_phase\":{},\"wall_ms\":{},\"phases\":[{}],\
         \"totals\":{{\"work\":{},\"sched\":{}}}}}\n",
        json_string(&slot.model),
        crate::par::current_threads(),
        json_string(active.label()),
        ms(ns_between(slot.started, now)),
        phases.join(","),
        work_object(&totals, Class::Work, false),
        work_object(&totals, Class::Sched, false),
    )
}

fn write_sidecar(slot: &ModelSlot, partial: bool, now: Instant) {
    let text = serialise(slot, partial, now);
    if let Err(error) = write_atomic(&slot.dir, &slot.model, &text) {
        if !WARNED.swap(true, Relax) {
            eprintln!(
                "WARNING: perf sidecar for {} not written to {}: {error}",
                slot.model,
                slot.dir.display()
            );
        }
    }
}

fn write_atomic(dir: &Path, model: &str, text: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let stem: String = model
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' })
        .collect();
    let tmp = dir.join(format!("{stem}.perf.json.tmp"));
    let done = dir.join(format!("{stem}.perf.json"));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayon::prelude::*;
    use std::sync::Mutex as TestMutex;

    /// The counters are process-global, so these tests run one at a time.
    static SERIAL: TestMutex<()> = TestMutex::new(());

    fn totals_of(w: Work) -> u64 {
        let (work, _) = sum_shards();
        work.iter().map(|row| row[w as usize]).sum()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("plinth-perf-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn off_writes_nothing() {
        let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        ENABLED.store(false, Relax);
        for shard in &TABLE {
            shard.zero();
        }
        add(Work::SweepRuns, 5);
        assert_eq!(totals_of(Work::SweepRuns), 0);
        let (value, tally) = capture_frames(|| {
            frame_solve(7);
            3
        });
        assert_eq!(value, 3);
        tally.commit();
        assert_eq!(totals_of(Work::FrameSolvesCommitted), 0);
        assert!(span(Span::SweepRun).0.is_none());
    }

    #[test]
    fn on_sums_exactly_and_equal_at_every_width() {
        let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = scratch("widths");
        enable(dir.clone());
        let mut seen = Vec::new();
        for width in [1_usize, 4, 16] {
            begin_model("widths");
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(width)
                .build()
                .expect("pool");
            pool.install(|| {
                (0..10_000_u64).into_par_iter().for_each(|i| {
                    add(Work::SegWalkQueries, 1);
                    add(Work::SegWalkTrisTested, i % 7);
                });
            });
            seen.push((
                totals_of(Work::SegWalkQueries),
                totals_of(Work::SegWalkTrisTested),
            ));
            end_model();
        }
        let expected: u64 = (0..10_000_u64).map(|i| i % 7).sum();
        assert!(seen.iter().all(|&s| s == (10_000, expected)), "{seen:?}");
        ENABLED.store(false, Relax);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn phase_attribution_and_final_sidecar() {
        let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = scratch("phases");
        enable(dir.clone());
        begin_model("phased");
        add(Work::SweepRuns, 1);
        assert_eq!(set_phase(Phase::Production), Phase::Load);
        (0..64_u64).into_par_iter().for_each(|_| add(Work::SweepRuns, 1));
        {
            let _span = span(Span::SweepRun);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let partial = std::fs::read_to_string(dir.join("phased.perf.json")).expect("partial");
        assert!(partial.contains("\"partial\":true"), "{partial}");
        end_model();
        // A later phase change after the end writes nothing over the final file.
        assert_eq!(set_phase(Phase::Emit), Phase::Production);
        let text = std::fs::read_to_string(dir.join("phased.perf.json")).expect("final");
        assert!(text.contains("\"partial\":false"), "{text}");
        assert!(text.contains("{\"phase\":\"Load\""), "{text}");
        assert!(text.contains("\"sweep.runs\":1"), "{text}");
        assert!(text.contains("\"sweep.runs\":64"), "{text}");
        assert!(text.contains("\"sweep.run\":{\"calls\":1"), "{text}");
        ENABLED.store(false, Relax);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn captured_frames_are_classified_by_the_caller() {
        let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = scratch("frames");
        enable(dir.clone());
        begin_model("frames");
        frame_solve(10);
        let (_, held) = capture_frames(|| {
            frame_solve(4);
            frame_solve(6);
        });
        assert_eq!(totals_of(Work::FrameSolvesCommitted), 1);
        held.speculate();
        assert_eq!(totals_of(Work::FrameSolvesCommitted), 1);
        assert_eq!(totals_of(Work::FrameSolvesSpeculative), 2);
        assert_eq!(totals_of(Work::FramePointsSpeculative), 10);
        end_model();
        ENABLED.store(false, Relax);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn labels_are_unique() {
        let mut labels: Vec<&str> = Work::ALL.iter().map(|w| w.label()).collect();
        labels.sort_unstable();
        let before = labels.len();
        labels.dedup();
        assert_eq!(before, labels.len());
    }
}
