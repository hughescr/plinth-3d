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
//! temp file then rename) at every phase change and at every checkpoint tick
//! as `"partial": true`, and at [`end_model`] as `"partial": false`. A failed
//! write is a WARNING on stderr, never a failure of the model. The sidecar is
//! outside the model's output folder, so no digest sees it.
//!
//! The file name keeps the model's name as it is. Only characters that a file
//! name cannot hold (path separators, `:` and the other Windows-reserved
//! characters, control characters) and `%` itself are written as `%XX` per
//! UTF-8 byte, so two distinct model names never share a sidecar.
//!
//! # Checkpoints
//!
//! [`enable`] starts one plain thread (not a rayon worker, so no pool changes
//! size) that rewrites the partial sidecar every `checkpoint` period. A run
//! that is killed deep inside one phase therefore still leaves the counters
//! of the work finished so far. Spans count their entry too, so a span still
//! open at a checkpoint (a three-hour production batch inside one
//! `sweep.run`) shows as `open` and `open_ms`. Open time is approximate (the
//! cells are read without a lock and the result is clamped at zero); the
//! final sidecar has no open spans. A checkpoint is diagnostic evidence and is
//! never compared against another run.
//!
//! # Adding a counter
//!
//! Append one line to `works!` below, then call [`add`] from a file that owns
//! the site. Order is the sidecar order; append-only keeps diffs readable.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering::Relaxed as Relax;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize};
use std::sync::{Mutex, Once, OnceLock};
use std::time::{Duration, Instant};

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

/// `entered` and `start_in` move on entry; `calls` (completed), `ns`,
/// `max_ns` and `start_out` on drop. The starts are nanoseconds since
/// [`EPOCH`], summed with wrapping adds, so at any instant the open count is
/// `entered - calls` and the open time is `open * now - (start_in - start_out)`.
struct SpanCell {
    entered: AtomicU64,
    calls: AtomicU64,
    ns: AtomicU64,
    max_ns: AtomicU64,
    start_in: AtomicU64,
    start_out: AtomicU64,
}

impl SpanCell {
    const fn new() -> Self {
        Self {
            entered: AtomicU64::new(0),
            calls: AtomicU64::new(0),
            ns: AtomicU64::new(0),
            max_ns: AtomicU64::new(0),
            start_in: AtomicU64::new(0),
            start_out: AtomicU64::new(0),
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
                cell.entered.store(0, Relax);
                cell.calls.store(0, Relax);
                cell.ns.store(0, Relax);
                cell.max_ns.store(0, Relax);
                cell.start_in.store(0, Relax);
                cell.start_out.store(0, Relax);
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
/// The zero of every span start time.
static EPOCH: OnceLock<Instant> = OnceLock::new();
/// Checkpoint period in milliseconds; the ticker re-reads it every slice.
static CHECKPOINT_MS: AtomicU64 = AtomicU64::new(DEFAULT_CHECKPOINT_MS);
static TICKER: Once = Once::new();

/// The period used when the caller names none.
pub const DEFAULT_CHECKPOINT: Duration = Duration::from_secs(60);
const DEFAULT_CHECKPOINT_MS: u64 = 60_000;
/// The ticker wakes at least this often to compare the clock with the period.
const TICK_SLICE: Duration = Duration::from_millis(50);

fn epoch() -> Instant {
    *EPOCH.get_or_init(Instant::now)
}

fn since_epoch(at: Instant) -> u64 {
    ns_between(epoch(), at)
}

// -------------------------------------------------------------------- API

/// Switch counting on, name the sidecar folder and start the checkpoint
/// thread, which rewrites the partial sidecar every `checkpoint` (at least one
/// millisecond). Called by the CLI only. A second call changes the folder and
/// the period; the one thread keeps running.
pub fn enable(dir: PathBuf, checkpoint: Duration) {
    if let Ok(mut slot) = DIR.lock() {
        *slot = Some(dir);
    }
    epoch();
    let period = u64::try_from(checkpoint.as_millis()).unwrap_or(u64::MAX).max(1);
    CHECKPOINT_MS.store(period, Relax);
    ENABLED.store(true, Relax);
    TICKER.call_once(|| {
        let started = std::thread::Builder::new()
            .name("plinth-perf-checkpoint".to_owned())
            .spawn(ticker);
        if let Err(error) = started {
            eprintln!("WARNING: perf checkpoint thread not started: {error}");
        }
    });
}

/// The checkpoint thread: forever, wake in short slices and write a partial
/// sidecar each time a full period has passed.
fn ticker() {
    let mut last = Instant::now();
    loop {
        let period = Duration::from_millis(CHECKPOINT_MS.load(Relax));
        let wait = period.saturating_sub(last.elapsed()).min(TICK_SLICE);
        std::thread::sleep(wait);
        if last.elapsed() >= Duration::from_millis(CHECKPOINT_MS.load(Relax)) {
            last = Instant::now();
            checkpoint_now();
        }
    }
}

/// Write a partial sidecar for the model in flight, if any. Under the slot
/// lock, so it can neither interleave with `set_phase` nor land after
/// `end_model` has taken the slot (a later tick finds none and writes nothing).
fn checkpoint_now() {
    if !enabled() {
        return;
    }
    if let Ok(guard) = SLOT.lock() {
        if let Some(slot) = guard.as_ref() {
            write_sidecar(slot, true, Instant::now());
        }
    }
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
    // Zero and publish under the slot lock, so a checkpoint tick never reads a
    // half-zeroed table.
    if let Ok(mut slot) = SLOT.lock() {
        for shard in &TABLE {
            shard.zero();
        }
        PHASE.store(Phase::Load as u8, Relax);
        let now = Instant::now();
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
        let phase = Phase::from_index(PHASE.load(Relax));
        let started = Instant::now();
        let start_ns = since_epoch(started);
        // The entry is visible at once, so a checkpoint taken while the span
        // is still open can show it. Start first, count second: a reader that
        // sees the count sees the start.
        let cell = &my_shard().spans[phase.index()][span.index()];
        cell.start_in.fetch_add(start_ns, Relax);
        cell.entered.fetch_add(1, Relax);
        SpanGuard(Some((span, phase, started, start_ns)))
    } else {
        SpanGuard(None)
    }
}

/// Charges its region's time to a [`Span`] on drop.
#[must_use = "a span measures until it is dropped"]
pub struct SpanGuard(Option<(Span, Phase, Instant, u64)>);

impl Drop for SpanGuard {
    fn drop(&mut self) {
        if let Some((span, phase, started, start_ns)) = self.0.take() {
            let ns = ns_between(started, Instant::now());
            let cell = &my_shard().spans[phase.index()][span.index()];
            cell.ns.fetch_add(ns, Relax);
            cell.max_ns.fetch_max(ns, Relax);
            cell.start_out.fetch_add(start_ns, Relax);
            cell.calls.fetch_add(1, Relax);
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

/// One span's sums over all shards, in one phase.
#[derive(Clone, Copy, Default)]
struct SpanSum {
    /// Completed calls.
    calls: u64,
    /// Summed duration of completed calls.
    ns: u64,
    /// Longest completed call.
    max_ns: u64,
    /// Spans entered and not yet dropped, as far as the racy read can tell.
    open: u64,
    /// Time those open spans have been running, clamped at zero.
    open_ns: u64,
}

/// Sums over all shards: `work[phase][counter]` and `spans[phase][span]`.
type WorkSums = Vec<Vec<u64>>;
type SpanSums = Vec<Vec<SpanSum>>;

fn sum_shards() -> (WorkSums, SpanSums) {
    let mut work = vec![vec![0_u64; WORK_N]; PHASE_N];
    let mut spans = vec![vec![SpanSum::default(); SPAN_N]; PHASE_N];
    // (entered, start_in, start_out) summed with wrapping adds, per cell.
    let mut open_raw = vec![vec![(0_u64, 0_u64, 0_u64); SPAN_N]; PHASE_N];
    for shard in &TABLE {
        for (phase, row) in shard.work.iter().enumerate() {
            for (index, cell) in row.iter().enumerate() {
                work[phase][index] += cell.load(Relax);
            }
        }
        for (phase, row) in shard.spans.iter().enumerate() {
            for (index, cell) in row.iter().enumerate() {
                // Writers add the start, then the count. Read the count, then
                // the start, so a counted span has its start in the sum.
                let calls = cell.calls.load(Relax);
                let start_out = cell.start_out.load(Relax);
                let entered = cell.entered.load(Relax);
                let start_in = cell.start_in.load(Relax);
                let sum = &mut spans[phase][index];
                sum.calls += calls;
                sum.ns += cell.ns.load(Relax);
                sum.max_ns = sum.max_ns.max(cell.max_ns.load(Relax));
                let raw = &mut open_raw[phase][index];
                raw.0 = raw.0.wrapping_add(entered);
                raw.1 = raw.1.wrapping_add(start_in);
                raw.2 = raw.2.wrapping_add(start_out);
            }
        }
    }
    // The clock is read after every cell, so no observed entry starts later.
    let now_ns = since_epoch(Instant::now());
    for (phase, row) in spans.iter_mut().enumerate() {
        for (index, sum) in row.iter_mut().enumerate() {
            let (entered, start_in, start_out) = open_raw[phase][index];
            sum.open = entered.saturating_sub(sum.calls);
            // open * now - (sum of the open starts); the start sums wrap, so
            // their difference is exact modulo 2^64. Anything that does not
            // fit (a torn read) clamps to zero.
            let starts = i128::from(start_in.wrapping_sub(start_out));
            let open_ns = i128::from(sum.open) * i128::from(now_ns) - starts;
            sum.open_ns = u64::try_from(open_ns.max(0)).unwrap_or(0);
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
            let sum = spans[phase.index()][s.index()];
            // The final sidecar is exact: every span has closed, and a leaked
            // guard is not reported as running.
            let (open, open_ns) = if partial { (sum.open, sum.open_ns) } else { (0, 0) };
            if sum.calls > 0 || open > 0 {
                span_parts.push(format!(
                    "{}:{{\"calls\":{},\"open\":{open},\"thread_ms\":{},\"open_ms\":{},\"max_ms\":{}}}",
                    json_string(s.label()),
                    sum.calls,
                    ms(sum.ns),
                    ms(open_ns),
                    ms(sum.max_ns)
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

/// The model's name as a file stem. Valid stems pass through unchanged
/// (`A B` stays `A B`, and differs from `A_B`). A character no file name can
/// hold, and `%` (the escape), becomes `%XX` per UTF-8 byte; the map is
/// injective, so distinct names never share a sidecar.
fn file_stem(model: &str) -> String {
    let mut out = String::with_capacity(model.len());
    for c in model.chars() {
        let reserved = c.is_control()
            || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '%');
        if reserved {
            let mut buffer = [0_u8; 4];
            for byte in c.encode_utf8(&mut buffer).bytes() {
                out.push_str(&format!("%{byte:02X}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn write_atomic(dir: &Path, model: &str, text: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let stem = file_stem(model);
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

    /// A period long enough that the ticker never fires inside a test.
    const HOUR: Duration = Duration::from_secs(3600);

    /// The text of the `"<key>":{...}` object inside `text`, braces included.
    fn object_of<'a>(text: &'a str, key: &str) -> &'a str {
        let needle = format!("\"{key}\":{{");
        let start = text.find(&needle).unwrap_or_else(|| panic!("no {key} in {text}"));
        let from = start + needle.len() - 1;
        let end = text[from..].find('}').expect("closing brace") + from + 1;
        &text[from..end]
    }

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
        enable(dir.clone(), HOUR);
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
        enable(dir.clone(), HOUR);
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
        enable(dir.clone(), HOUR);
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
    fn checkpoint_shows_an_open_span_and_the_closed_one_after() {
        let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = scratch("open");
        enable(dir.clone(), HOUR);
        begin_model("open");
        set_phase(Phase::Production);
        let outer = span(Span::SweepRun);
        let inner = span(Span::SweepRun);
        std::thread::sleep(Duration::from_millis(5));
        checkpoint_now();
        let text = std::fs::read_to_string(dir.join("open.perf.json")).expect("checkpoint");
        assert!(text.contains("\"partial\":true"), "{text}");
        let run = object_of(&text, "sweep.run");
        assert!(run.contains("\"calls\":0,\"open\":2,"), "{run}");
        assert!(!run.contains("\"open_ms\":0.000"), "{run}");
        // One of the two overlapping spans closes: one open, one call.
        drop(inner);
        checkpoint_now();
        let text = std::fs::read_to_string(dir.join("open.perf.json")).expect("checkpoint");
        let run = object_of(&text, "sweep.run");
        assert!(run.contains("\"calls\":1,\"open\":1,"), "{run}");
        drop(outer);
        checkpoint_now();
        let text = std::fs::read_to_string(dir.join("open.perf.json")).expect("checkpoint");
        let run = object_of(&text, "sweep.run");
        assert!(run.contains("\"calls\":2,\"open\":0,"), "{run}");
        assert!(run.contains("\"open_ms\":0.000"), "{run}");
        end_model();
        ENABLED.store(false, Relax);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The killed-run case: no phase change and no `end_model`, yet the
    /// counters of the finished work and the open span reach the file.
    #[test]
    fn checkpoint_thread_writes_mid_phase_work_without_a_phase_change() {
        let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = scratch("ticker");
        enable(dir.clone(), Duration::from_millis(20));
        begin_model("ticker");
        set_phase(Phase::Production);
        let path = dir.join("ticker.perf.json");
        let entry = std::fs::read_to_string(&path).expect("phase-change snapshot");
        assert!(entry.contains("\"sweep.runs\":0"), "{entry}");
        let _open = span(Span::SweepRun);
        add(Work::SweepRuns, 3);
        let mut seen = String::new();
        for _ in 0..250 {
            std::thread::sleep(Duration::from_millis(20));
            seen = std::fs::read_to_string(&path).unwrap_or_default();
            if seen.contains("\"sweep.runs\":3") && seen.contains("\"open\":1") {
                break;
            }
        }
        assert!(seen.contains("\"partial\":true"), "{seen}");
        assert!(seen.contains("\"sweep.runs\":3"), "{seen}");
        assert!(seen.contains("\"open\":1"), "{seen}");
        assert!(seen.contains("\"active_phase\":\"Production\""), "{seen}");
        // Slow the ticker down again before the final write.
        enable(dir.clone(), HOUR);
        end_model();
        let last = std::fs::read_to_string(&path).expect("final");
        assert!(last.contains("\"partial\":false"), "{last}");
        // A tick after the end finds no model and leaves the final file alone.
        checkpoint_now();
        assert_eq!(std::fs::read_to_string(&path).expect("final"), last);
        ENABLED.store(false, Relax);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn distinct_model_names_get_distinct_sidecars() {
        assert_eq!(file_stem("A B"), "A B");
        assert_eq!(file_stem("A_B"), "A_B");
        assert_eq!(file_stem("Café-1.0"), "Café-1.0");
        assert_eq!(file_stem("a:b"), "a%3Ab");
        assert_eq!(file_stem("a%3Ab"), "a%253Ab");
        let names = ["A B", "A_B", "A:B", "A%3AB", "A/B", "A\\B", "A\tB", "a b"];
        let mut stems: Vec<String> = names.iter().map(|n| file_stem(n)).collect();
        stems.sort_unstable();
        stems.dedup();
        assert_eq!(stems.len(), names.len(), "{stems:?}");

        let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = scratch("names");
        enable(dir.clone(), HOUR);
        for (name, runs) in [("A B", 1_u64), ("A_B", 2)] {
            begin_model(name);
            add(Work::SweepRuns, runs);
            end_model();
        }
        let spaced = std::fs::read_to_string(dir.join("A B.perf.json")).expect("A B");
        let under = std::fs::read_to_string(dir.join("A_B.perf.json")).expect("A_B");
        assert!(spaced.contains("\"sweep.runs\":1"), "{spaced}");
        assert!(under.contains("\"sweep.runs\":2"), "{under}");
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
