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
//! # Pose records, rows and stage marks (X0, 2026-10-10)
//!
//! Three additive sections of the sidecar, all write-only and all off with the
//! counters:
//!
//! * **`poses`**: every [`Span::SweepRun`] opens a *frame* on its thread. The
//!   frame sums the time of every span that closes on that thread while it is
//!   the innermost frame, and on close becomes one record (phase, label, wall
//!   time, per-span calls and thread time). This is how the winning pose of the
//!   orientation search is priced alone: the global span table sums all poses.
//!   [`pose_label`] names the next frame. A frame that runs while another frame
//!   of the same thread is open (rayon work stealing) is added to the outer
//!   frame's `nested_ms`, so the outer wall can be corrected.
//! * **`rows`**: [`row`] stores one small fixed-width record per call, batch
//!   or search ([`Rows`]). The multiset of non-time columns does not depend on
//!   the thread count; the sidecar sorts the rows. Medians and shares are read
//!   from rows afterwards, never inside the product.
//! * **`stages`**: [`stage`] times a named region of a stage (Decimate,
//!   OrientTable, Hollow, Emit) by a static label.
//!
//! # Adding a counter
//!
//! Append one line to `works!` below, then call [`add`] from a file that owns
//! the site. Order is the sidecar order; append-only keeps diffs readable.

use std::any::Any;
use std::cell::{Cell, RefCell};
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
    /// Pose-search diagnostics (`plinth_orient::diag`): work done for the
    /// diagnostics alone, so a run that collects them still reports the
    /// pipeline's own phases without it.
    Diagnostic,
    Other,
}

const PHASE_N: usize = 12;

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
        Self::Diagnostic,
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
            Self::Diagnostic => "Diagnostic",
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
            10 => Self::Diagnostic,
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
    /// Bounded clearance rows put to the refusal search first
    /// (`segment_clearance_classified_at_least`).
    SegProveQueries, "kernel.seg_prove.queries", Work;
    /// Of those, rows the search refused (no full walk run).
    SegProveRefused, "kernel.seg_prove.refused", Work;
    /// Faces whose segment distance the refusal search computed.
    SegProveTrisTested, "kernel.seg_prove.tris_tested", Work;
    /// Tip landings put to the nearest-face scan (`tip_landing`).
    NearestTipRows, "nearest.tip.rows", Work;
    /// Landing-face pairs the scan measured: `rows * faces` for a batch small
    /// enough to scan, far fewer through the bounding-box tree.
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
    /// Mover reads the lip-seat round loop asked for (one per proposal).
    LipSeatMoverReads, "lip_seat.mover_reads", Work;
    /// Of those, reads a batch had already taken ahead on the same path.
    LipSeatMoverReadsAhead, "lip_seat.mover_reads_ahead", Work;
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
    /// Stage frames the shake check put to the solver.
    ShakeFramesSolved, "shake.frames_solved", Work;
    /// Batches those frames were solved in (a function of the skyline sizes
    /// and the solve budget only).
    ShakeBatches, "shake.batches", Work;
    /// Skyline `f64`s of those frames, summed.
    ShakeSkylineEntries, "shake.skyline_entries", Work;
    /// Vertex blocks of the curvature spread, overflow remainders included
    /// (a fixed plan: independent of the thread count).
    DelicacyBlocks, "delicacy.blocks", Work;
    /// Waves of those blocks.
    DelicacyWaves, "delicacy.waves", Work;
    /// Blocks that hit the pair cap and continued as a remainder.
    DelicacyOverflowSplits, "delicacy.overflow_splits", Work;
    /// Faces that received two or more curvature addends: the only faces
    /// where the order of the serial replay can matter.
    DelicacyFacesMultiplyCharged, "delicacy.faces_multiply_charged", Work;
    /// Faces found inside their own neighbourhood across all display walks
    /// (exceeds `display.faces` exactly when some walk expanded).
    DisplayMembersTotal, "display.members_total", Work;
    /// Down-sweep runs (`down_sweep::run`, per-event form) that the reuse
    /// probe measured. One per run, so it equals the sum of the
    /// `sweep.down.calls.*` counters.
    ReuseRuns, "reuse.runs", Work;
    /// Of those, runs with no earlier run in the same pose frame: nothing to
    /// reuse from.
    ReuseFirstRuns, "reuse.first_runs", Work;
    /// Of those, runs whose live members equal the previous run's exactly.
    ReuseUnchangedRuns, "reuse.unchanged_runs", Work;
    /// Layers of the measured runs, summed.
    ReuseLayers, "reuse.layers", Work;
    /// Layers above the highest changed member's top (the prefix a resumed
    /// sweep skips), summed. A first run counts none.
    ReuseLayersAboveMember, "reuse.layers_above.member", Work;
    /// Layers above the highest layer whose live-member fingerprint changed,
    /// summed. At least the member figure.
    ReuseLayersAboveLayer, "reuse.layers_above.layer", Work;
    /// Active (layer, band) pairs below the changed prefix, summed. Zero
    /// unless the band probe is on (`PLINTH_PERF_BANDS=1`).
    ReuseBandsTotal, "reuse.bands_total", Work;
    /// Of those, pairs whose hash equals the previous run's.
    ReuseBandsEqual, "reuse.bands_equal", Work;
    /// Batched clearance calls (`clearance_rows_excluding`).
    ClearBatches, "clear.batches", Work;
    /// Rows of those batches.
    ClearBatchRows, "clear.batch_rows", Work;
    /// Faces tested across those batches.
    ClearBatchTris, "clear.batch_tris", Work;
    /// The largest single row's faces tested, summed over batches.
    ClearBatchTrisMaxRow, "clear.batch_tris_max_row", Work;
    /// Main brace partner searches (`tie_by_crosses`, `tie_by_band_set`).
    PartnerSearches, "brace.partner_searches", Work;
    /// Candidates those searches planned (trials).
    PartnerTrials, "brace.partner_trials", Work;
    /// Searches that built something.
    PartnerWins, "brace.partner_wins", Work;
    /// Sum of the winning candidates' ranks (0 = nearest).
    PartnerWinRankSum, "brace.partner_win_rank_sum", Work;
}

const WORK_N: usize = Work::ALL.len();

impl Work {
    /// The sizing-site span a `sweep.down.calls.*` charge names, for the next
    /// `down_sweep::run` on this thread. Every call site charges its counter
    /// just before it calls `run`, so the charge doubles as the site label.
    #[must_use]
    pub const fn size_span(self) -> Option<Span> {
        match self {
            Self::SweepDownCallInitial => Some(Span::SizeInitial),
            Self::SweepDownCallRemoval => Some(Span::SizeRemoval),
            Self::SweepDownCallMergesUndone => Some(Span::SizeMergesUndone),
            Self::SweepDownCallCongestion => Some(Span::SizeCongestion),
            Self::SweepDownCallCongestionTrial => Some(Span::SizeCongestionTrial),
            Self::SweepDownCallStrandedRound => Some(Span::SizeStrandedRound),
            Self::SweepDownCallStrandedPutBack => Some(Span::SizeStrandedPutBack),
            Self::SweepDownCallForcedMerges => Some(Span::SizeForcedMerges),
            Self::SweepDownCallUnmergeAfterMerges => Some(Span::SizeUnmergeAfterMerges),
            Self::SweepDownCallGraze => Some(Span::SizeGraze),
            Self::SweepDownCallHeadJoin => Some(Span::SizeHeadJoin),
            Self::SweepDownCallLipSeat => Some(Span::SizeLipSeat),
            Self::SweepDownCallTies => Some(Span::SizeTies),
            Self::SweepDownCallModelTies => Some(Span::SizeModelTies),
            _ => None,
        }
    }
}

/// A timed region. The sidecar reports calls, summed thread time and the
/// longest call, per phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Span {
    /// One `run_sweep_with_holes`. Also opens a pose frame.
    SweepRun,
    /// One `slice_stack`.
    SliceStack,
    /// One stage-shake check.
    StageShake,
    /// One delicacy curvature spread.
    Curvature,
    /// One display neighbourhood pass.
    DisplayPass,
    /// One `down_sweep::sweep_pass`.
    SweepPass,
    /// A layer's rasterisation inside a pass (a pure function of the layer).
    PassPrefix,
    /// The routing field's clearance build (a pure function of the layer's cover); inside `down.pass.route`.
    PassClearance,
    /// `route_within_cover`, clearance build included.
    PassRoute,
    /// The dense per-cell loops of a layer: fills, spread, max, model-section check, end-of-layer copies.
    PassDense,
    /// The load-insertion step of a layer, trigger scan included.
    PassInsertion,
    /// The band probe's own hashing (measurement cost, inside `down.pass`).
    PassBandHash,
    /// One whole per-event `down_sweep::run`.
    DownRun,
    /// `per_event::size`, inside `down.run`.
    PerEventSize,
    /// A `down_sweep::run` from the first sizing site.
    SizeInitial,
    /// ... the under-loaded removal loop.
    SizeRemoval,
    /// ... after merges the trunk cannot carry are undone.
    SizeMergesUndone,
    /// ... after the congestion lane.
    SizeCongestion,
    /// ... the congestion lane's trial.
    SizeCongestionTrial,
    /// ... after a stranded-seat branch round.
    SizeStrandedRound,
    /// ... after stranded-seat branches are put back.
    SizeStrandedPutBack,
    /// ... after the slenderness lane's forced merges.
    SizeForcedMerges,
    /// ... after the un-merge of overgrown forced merges.
    SizeUnmergeAfterMerges,
    /// ... after graze heads.
    SizeGraze,
    /// ... after a head-join pass.
    SizeHeadJoin,
    /// ... inside the lip-seat rule's sizing.
    SizeLipSeat,
    /// ... after a tie pass.
    SizeTies,
    /// ... after a model-tie pass.
    SizeModelTies,
    /// A `run` whose call site charged no `sweep.down.calls.*` counter.
    SizeOther,
    /// `lip_seat::apply`.
    LipApply,
    /// `lip_seat::measure_sized`.
    LipMeasure,
    /// `lip_seat::blame`.
    LipBlame,
    /// `lip_seat::clearance_violators`.
    LipViolators,
    /// One batched clearance call (`clearance_rows_excluding`).
    ClearBatch,
    /// One tip-landing nearest-face scan.
    TipNearest,
    /// One main brace partner search.
    PartnerSearch,
    /// One candidate of a partner search.
    PartnerTrial,
    /// One tie pass of the slenderness lane (`apply_ties_with_model`).
    BraceLane,
}

const SPAN_N: usize = 38;

impl Span {
    /// Every span.
    pub const ALL: [Self; SPAN_N] = [
        Self::SweepRun,
        Self::SliceStack,
        Self::StageShake,
        Self::Curvature,
        Self::DisplayPass,
        Self::SweepPass,
        Self::PassPrefix,
        Self::PassClearance,
        Self::PassRoute,
        Self::PassDense,
        Self::PassInsertion,
        Self::PassBandHash,
        Self::DownRun,
        Self::PerEventSize,
        Self::SizeInitial,
        Self::SizeRemoval,
        Self::SizeMergesUndone,
        Self::SizeCongestion,
        Self::SizeCongestionTrial,
        Self::SizeStrandedRound,
        Self::SizeStrandedPutBack,
        Self::SizeForcedMerges,
        Self::SizeUnmergeAfterMerges,
        Self::SizeGraze,
        Self::SizeHeadJoin,
        Self::SizeLipSeat,
        Self::SizeTies,
        Self::SizeModelTies,
        Self::SizeOther,
        Self::LipApply,
        Self::LipMeasure,
        Self::LipBlame,
        Self::LipViolators,
        Self::ClearBatch,
        Self::TipNearest,
        Self::PartnerSearch,
        Self::PartnerTrial,
        Self::BraceLane,
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
            Self::SweepPass => "down.pass",
            Self::PassPrefix => "down.pass.prefix",
            Self::PassClearance => "down.pass.clearance",
            Self::PassRoute => "down.pass.route",
            Self::PassDense => "down.pass.dense",
            Self::PassInsertion => "down.pass.insertion",
            Self::PassBandHash => "probe.band_hash",
            Self::DownRun => "down.run",
            Self::PerEventSize => "down.per_event",
            Self::SizeInitial => "sizing.initial",
            Self::SizeRemoval => "sizing.removal",
            Self::SizeMergesUndone => "sizing.merges_undone",
            Self::SizeCongestion => "sizing.congestion",
            Self::SizeCongestionTrial => "sizing.congestion_trial",
            Self::SizeStrandedRound => "sizing.stranded_round",
            Self::SizeStrandedPutBack => "sizing.stranded_put_back",
            Self::SizeForcedMerges => "sizing.forced_merges",
            Self::SizeUnmergeAfterMerges => "sizing.unmerge_after_merges",
            Self::SizeGraze => "sizing.graze",
            Self::SizeHeadJoin => "sizing.head_join",
            Self::SizeLipSeat => "sizing.lip_seat",
            Self::SizeTies => "sizing.ties",
            Self::SizeModelTies => "sizing.model_ties",
            Self::SizeOther => "sizing.other",
            Self::LipApply => "lip_seat.apply",
            Self::LipMeasure => "lip_seat.measure",
            Self::LipBlame => "lip_seat.blame",
            Self::LipViolators => "lip_seat.violators",
            Self::ClearBatch => "clearance.batch",
            Self::TipNearest => "nearest.tip",
            Self::PartnerSearch => "brace.partner_search",
            Self::PartnerTrial => "brace.partner_trial",
            Self::BraceLane => "brace.lane",
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
    /// The pose frames open on this thread, innermost last.
    static FRAMES: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
    /// The label the next frame takes ([`pose_label`]).
    static PENDING_LABEL: RefCell<Option<String>> = const { RefCell::new(None) };
    /// The sizing site named by the last `sweep.down.calls.*` charge, as a
    /// `Span` index plus one; 0 = none ([`size_span`]).
    static SITE: Cell<u8> = const { Cell::new(0) };
    /// Faces the last segment walk on this thread tested ([`walk_tris`]).
    static WALK_TRIS: Cell<u64> = const { Cell::new(0) };
    /// The id of the innermost open frame on this thread, 0 when none.
    static POSE_ID: Cell<u64> = const { Cell::new(0) };
}

/// One open pose frame ([`Span::SweepRun`]).
struct Frame {
    phase: Phase,
    label: String,
    id: u64,
    span_ns: [u64; SPAN_N],
    span_calls: [u64; SPAN_N],
    /// Wall time of frames that ran nested inside this one on this thread.
    nested_ns: u64,
    /// Scratch for one probe ([`with_pose_slot`]).
    slot: Option<Box<dyn Any>>,
}

/// A closed frame: one pose's sweep.
struct PoseRecord {
    phase: Phase,
    label: String,
    id: u64,
    wall_ns: u64,
    nested_ns: u64,
    span_ns: [u64; SPAN_N],
    span_calls: [u64; SPAN_N],
}

/// One fixed-width record of a call, batch or search. The first two columns
/// of every row are the phase and the pose id; `cols` follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Rows {
    /// One per-event `down_sweep::run`: how much of it a resumed sweep could
    /// skip (see `reuse_probe`).
    DownReuse,
    /// One batched clearance call.
    ClearBatch,
    /// One tip-landing nearest-face scan.
    TipBatch,
    /// One main brace partner search.
    PartnerSearch,
}

/// The widest row payload.
pub const ROW_WIDTH: usize = 12;
const ROWS_N: usize = 4;

impl Rows {
    /// Every kind.
    pub const ALL: [Self; ROWS_N] = [
        Self::DownReuse,
        Self::ClearBatch,
        Self::TipBatch,
        Self::PartnerSearch,
    ];

    /// The sidecar label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::DownReuse => "down.reuse",
            Self::ClearBatch => "clear.batch",
            Self::TipBatch => "tip.batch",
            Self::PartnerSearch => "brace.partner_search",
        }
    }

    /// The payload column names, after `phase` and `pose`. A name ending in
    /// `_ns` is a time: reported, never compared.
    #[must_use]
    pub const fn cols(self) -> &'static [&'static str] {
        match self {
            // dirty_* is the number of layers from the bottom up to and
            // including the highest changed one (0 = nothing changed), so
            // `layers - dirty_*` is the prefix above it.
            Self::DownReuse => &[
                "site",
                "layers",
                "cells",
                "first",
                "dirty_member",
                "dirty_layer",
                "bands_total",
                "bands_equal",
            ],
            Self::ClearBatch => &[
                "rows",
                "tris",
                "tris_max_row",
                "frustum",
                "excluded",
                "wall_ns",
                "row_ns_sum",
                "row_ns_max",
            ],
            Self::TipBatch => &["rows", "faces", "wall_ns"],
            Self::PartnerSearch => &[
                "kind",
                "candidates",
                "trials",
                "win_rank_plus1",
                "wall_ns",
                "trial_ns_sum",
                "win_trial_ns",
            ],
        }
    }
}

struct RowRec {
    kind: Rows,
    phase: Phase,
    pose: u64,
    cols: [u64; ROW_WIDTH],
}

/// One named stage region ([`stage`]).
struct StageRec {
    phase: Phase,
    label: &'static str,
    calls: u64,
    ns: u64,
    max_ns: u64,
}

static POSES: Mutex<Vec<PoseRecord>> = Mutex::new(Vec::new());
static ROW_STORE: Mutex<Vec<RowRec>> = Mutex::new(Vec::new());
static ROWS_DROPPED: AtomicU64 = AtomicU64::new(0);
static STAGES: Mutex<Vec<StageRec>> = Mutex::new(Vec::new());
/// Row cap per model; rows past it are counted, not stored.
const ROW_CAP: usize = 1_000_000;

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
        lock_clear(&POSES);
        lock_clear(&ROW_STORE);
        lock_clear(&STAGES);
        ROWS_DROPPED.store(0, Relax);
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
    switch_phase(phase, true)
}

/// [`set_phase`] without the partial sidecar write. For a short bracket
/// (the diagnostics' own phase) whose two writes would cost more than the
/// work inside it: rewriting a large sidecar twice is hundreds of
/// milliseconds on a big model. The checkpoint thread still rewrites the
/// partial sidecar on its period, and [`end_model`] writes the final one.
pub fn set_phase_quiet(phase: Phase) -> Phase {
    switch_phase(phase, false)
}

fn switch_phase(phase: Phase, write: bool) -> Phase {
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
            if write {
                write_sidecar(slot, true, now);
            }
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

/// Write `bytes` next to the sidecar of the model in flight, as
/// `<dir>/<stem>.<ext>` (atomically: a temporary file, then a rename).
/// Write-only like everything here: nothing reads it back. `Ok(None)` when
/// counting is off or no model is in flight.
///
/// # Errors
///
/// Any I/O error from creating the folder, writing or renaming; the caller
/// decides whether that matters (the diagnostics warn and go on).
pub fn write_side_file(ext: &str, bytes: &[u8]) -> std::io::Result<Option<PathBuf>> {
    if !enabled() {
        return Ok(None);
    }
    let target = SLOT.lock().ok().and_then(|guard| {
        guard
            .as_ref()
            .map(|slot| (slot.dir.clone(), file_stem(&slot.model)))
    });
    let Some((dir, stem)) = target else {
        return Ok(None);
    };
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join(format!("{stem}.{ext}.tmp"));
    let done = dir.join(format!("{stem}.{ext}"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, &done)?;
    Ok(Some(done))
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
    if let Some(site) = work.size_span() {
        SITE.with(|cell| cell.set(site as u8 + 1));
    }
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
        let depth = if span == Span::SweepRun {
            push_frame(phase)
        } else {
            0
        };
        SpanGuard(Some((span, phase, started, start_ns)), depth)
    } else {
        SpanGuard(None, 0)
    }
}

/// Charges its region's time to a [`Span`] on drop. A [`Span::SweepRun`]
/// guard also owns the pose frame it opened (`.1` is its depth).
#[must_use = "a span measures until it is dropped"]
pub struct SpanGuard(Option<(Span, Phase, Instant, u64)>, usize);

impl Drop for SpanGuard {
    fn drop(&mut self) {
        if let Some((span, phase, started, start_ns)) = self.0.take() {
            let ns = ns_between(started, Instant::now());
            let cell = &my_shard().spans[phase.index()][span.index()];
            cell.ns.fetch_add(ns, Relax);
            cell.max_ns.fetch_max(ns, Relax);
            cell.start_out.fetch_add(start_ns, Relax);
            cell.calls.fetch_add(1, Relax);
            if self.1 > 0 {
                pop_frame(self.1, ns);
            } else {
                charge_frame(span, ns);
            }
        }
    }
}

// ----------------------------------------------------------- pose frames

/// FNV-1a over `text`, cut to 48 bits so the id survives a JSON number.
fn pose_id(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash & 0xFFFF_FFFF_FFFF
}

/// Name the next frame this thread opens (the next [`Span::SweepRun`]). The
/// closure runs only when counting is on.
pub fn pose_label(label: impl FnOnce() -> String) {
    if enabled() {
        let text = label();
        let _ = PENDING_LABEL.try_with(|cell| *cell.borrow_mut() = Some(text));
    }
}

/// Open a frame; returns its depth (at least 1), or 0 when it could not.
fn push_frame(phase: Phase) -> usize {
    let label = PENDING_LABEL
        .try_with(|cell| cell.borrow_mut().take())
        .ok()
        .flatten()
        .unwrap_or_else(|| "unlabelled".to_owned());
    let id = pose_id(&label);
    let _ = POSE_ID.try_with(|cell| cell.set(id));
    FRAMES
        .try_with(|frames| {
            let mut frames = frames.borrow_mut();
            frames.push(Frame {
                phase,
                label,
                id,
                span_ns: [0; SPAN_N],
                span_calls: [0; SPAN_N],
                nested_ns: 0,
                slot: None,
            });
            frames.len()
        })
        .unwrap_or(0)
}

/// Close the frame at `depth` (and any left open above it) into a record.
fn pop_frame(depth: usize, wall_ns: u64) {
    let closed = FRAMES
        .try_with(|frames| {
            let mut frames = frames.borrow_mut();
            if frames.len() < depth {
                return None;
            }
            frames.truncate(depth);
            let frame = frames.pop()?;
            if let Some(outer) = frames.last_mut() {
                outer.nested_ns = outer.nested_ns.saturating_add(wall_ns);
            }
            let _ = POSE_ID.try_with(|cell| cell.set(frames.last().map_or(0, |outer| outer.id)));
            Some(frame)
        })
        .ok()
        .flatten();
    if let Some(frame) = closed {
        if let Ok(mut poses) = POSES.lock() {
            poses.push(PoseRecord {
                phase: frame.phase,
                label: frame.label,
                id: frame.id,
                wall_ns,
                nested_ns: frame.nested_ns,
                span_ns: frame.span_ns,
                span_calls: frame.span_calls,
            });
        }
    }
}

/// Add a closed span to the innermost frame of this thread, if any.
fn charge_frame(span: Span, ns: u64) {
    let _ = FRAMES.try_with(|frames| {
        if let Ok(mut frames) = frames.try_borrow_mut() {
            if let Some(top) = frames.last_mut() {
                top.span_ns[span.index()] = top.span_ns[span.index()].saturating_add(ns);
                top.span_calls[span.index()] += 1;
            }
        }
    });
}

/// Run `f` on the innermost pose frame's scratch value of type `T`, creating
/// it with `T::default()` on first use. `None` when counting is off or this
/// thread has no frame. `f` must not open or drop a span.
pub fn with_pose_slot<T: Any + Default, R>(f: impl FnOnce(&mut T) -> R) -> Option<R> {
    if !enabled() {
        return None;
    }
    FRAMES
        .try_with(|frames| {
            let mut frames = frames.try_borrow_mut().ok()?;
            let frame = frames.last_mut()?;
            let slot = frame.slot.get_or_insert_with(|| Box::new(T::default()));
            Some(f(slot.downcast_mut::<T>()?))
        })
        .ok()
        .flatten()
}

/// The pose id of this thread's innermost frame, 0 when none. Kept in its own
/// cell so a row can be stored from inside [`with_pose_slot`], where the frame
/// list is borrowed.
fn current_pose() -> u64 {
    POSE_ID.try_with(Cell::get).unwrap_or(0)
}

/// The span of the sizing site the last `sweep.down.calls.*` charge on this
/// thread named, taking the name (a later `run` without a charge reads
/// [`Span::SizeOther`]).
pub fn size_span() -> SpanGuard {
    if !enabled() {
        return SpanGuard(None, 0);
    }
    let named = SITE.with(|cell| cell.replace(0));
    span(Span::ALL.get(usize::from(named).wrapping_sub(1)).copied().unwrap_or(Span::SizeOther))
}

/// The sizing-site index (`Span` index) the last charge named, without
/// taking it; `u64::MAX` when none. For rows.
#[must_use]
pub fn size_site_peek() -> u64 {
    let named = SITE.with(Cell::get);
    if named == 0 {
        u64::MAX
    } else {
        u64::from(named) - 1
    }
}

/// The faces the last segment walk on this thread tested; set by the walk,
/// read by the batch loop that called it. Zero when counting is off.
pub fn note_walk_tris(n: u64) {
    WALK_TRIS.with(|cell| cell.set(n));
}

/// Take the value [`note_walk_tris`] left.
#[must_use]
pub fn walk_tris() -> u64 {
    WALK_TRIS.with(|cell| cell.replace(0))
}

// ------------------------------------------------------- rows and stages

fn lock_clear<T>(store: &Mutex<Vec<T>>) {
    if let Ok(mut guard) = store.lock() {
        guard.clear();
    }
}

/// Store one row of `kind`. `payload` longer than [`ROW_WIDTH`] is cut.
pub fn row(kind: Rows, payload: &[u64]) {
    if !enabled() {
        return;
    }
    let mut cols = [0_u64; ROW_WIDTH];
    for (slot, value) in cols.iter_mut().zip(payload) {
        *slot = *value;
    }
    let rec = RowRec {
        kind,
        phase: Phase::from_index(PHASE.load(Relax)),
        pose: current_pose(),
        cols,
    };
    if let Ok(mut store) = ROW_STORE.lock() {
        if store.len() < ROW_CAP {
            store.push(rec);
            return;
        }
    }
    ROWS_DROPPED.fetch_add(1, Relax);
}

/// Time a named region of a stage; charged when the guard drops. Inert when
/// counting is off.
#[must_use = "a stage mark measures until it is dropped"]
pub fn stage(label: &'static str) -> StageGuard {
    if enabled() {
        StageGuard(Some((label, Phase::from_index(PHASE.load(Relax)), Instant::now())))
    } else {
        StageGuard(None)
    }
}

/// Charges a stage region on drop.
#[must_use = "a stage mark measures until it is dropped"]
pub struct StageGuard(Option<(&'static str, Phase, Instant)>);

impl Drop for StageGuard {
    fn drop(&mut self) {
        if let Some((label, phase, started)) = self.0.take() {
            let ns = ns_between(started, Instant::now());
            if let Ok(mut stages) = STAGES.lock() {
                match stages.iter_mut().find(|s| s.phase == phase && s.label == label) {
                    Some(rec) => {
                        rec.calls += 1;
                        rec.ns = rec.ns.saturating_add(ns);
                        rec.max_ns = rec.max_ns.max(ns);
                    }
                    None => stages.push(StageRec {
                        phase,
                        label,
                        calls: 1,
                        ns,
                        max_ns: ns,
                    }),
                }
            }
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
         \"totals\":{{\"work\":{},\"sched\":{}}},{}}}\n",
        json_string(&slot.model),
        crate::par::current_threads(),
        json_string(active.label()),
        ms(ns_between(slot.started, now)),
        phases.join(","),
        work_object(&totals, Class::Work, false),
        work_object(&totals, Class::Sched, false),
        extras_json(),
    )
}

/// The `span_labels`, `poses`, `stages` and `rows` keys, without braces. The
/// span index in a row's `site` column indexes `span_labels`. Everything is
/// sorted so equal content serialises equally.
fn extras_json() -> String {
    let labels: Vec<String> = Span::ALL.iter().map(|s| json_string(s.label())).collect();
    let mut poses = Vec::new();
    if let Ok(store) = POSES.lock() {
        let mut order: Vec<&PoseRecord> = store.iter().collect();
        order.sort_by(|a, b| {
            (a.phase.index(), &a.label, a.wall_ns).cmp(&(b.phase.index(), &b.label, b.wall_ns))
        });
        for rec in order {
            let mut spans = Vec::new();
            for s in Span::ALL {
                let calls = rec.span_calls[s.index()];
                if calls > 0 {
                    spans.push(format!(
                        "{}:{{\"calls\":{calls},\"thread_ms\":{}}}",
                        json_string(s.label()),
                        ms(rec.span_ns[s.index()])
                    ));
                }
            }
            poses.push(format!(
                "{{\"phase\":{},\"label\":{},\"id\":{},\"wall_ms\":{},\"nested_ms\":{},\"spans\":{{{}}}}}",
                json_string(rec.phase.label()),
                json_string(&rec.label),
                rec.id,
                ms(rec.wall_ns),
                ms(rec.nested_ns),
                spans.join(",")
            ));
        }
    }
    let mut stages = Vec::new();
    if let Ok(store) = STAGES.lock() {
        let mut order: Vec<&StageRec> = store.iter().collect();
        order.sort_by(|a, b| (a.phase.index(), a.label).cmp(&(b.phase.index(), b.label)));
        for rec in order {
            stages.push(format!(
                "{{\"phase\":{},\"label\":{},\"calls\":{},\"thread_ms\":{},\"max_ms\":{}}}",
                json_string(rec.phase.label()),
                json_string(rec.label),
                rec.calls,
                ms(rec.ns),
                ms(rec.max_ns)
            ));
        }
    }
    let mut kinds = Vec::new();
    if let Ok(store) = ROW_STORE.lock() {
        for kind in Rows::ALL {
            let mut data: Vec<(usize, u64, [u64; ROW_WIDTH])> = store
                .iter()
                .filter(|r| r.kind == kind)
                .map(|r| (r.phase.index(), r.pose, r.cols))
                .collect();
            if data.is_empty() {
                continue;
            }
            data.sort_unstable();
            let names: Vec<String> = ["phase", "pose"]
                .into_iter()
                .chain(kind.cols().iter().copied())
                .map(json_string)
                .collect();
            let lines: Vec<String> = data
                .iter()
                .map(|(phase, pose, cols)| {
                    let mut line = format!("[{phase},{pose}");
                    for value in &cols[..kind.cols().len().min(ROW_WIDTH)] {
                        line.push_str(&format!(",{value}"));
                    }
                    line.push(']');
                    line
                })
                .collect();
            kinds.push(format!(
                "{}:{{\"cols\":[{}],\"data\":[{}]}}",
                json_string(kind.label()),
                names.join(","),
                lines.join(",")
            ));
        }
    }
    format!(
        "\"span_labels\":[{}],\"poses\":[{}],\"stages\":[{}],\"rows\":{{{}}},\"rows_dropped\":{}",
        labels.join(","),
        poses.join(","),
        stages.join(","),
        kinds.join(","),
        ROWS_DROPPED.load(Relax)
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
    fn a_quiet_phase_switch_attributes_work_and_writes_no_partial_sidecar() {
        let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = scratch("quiet");
        enable(dir.clone(), HOUR);
        begin_model("quiet");
        let sidecar = dir.join("quiet.perf.json");
        let _ = std::fs::remove_file(&sidecar);
        assert_eq!(set_phase_quiet(Phase::Diagnostic), Phase::Load);
        add(Work::SweepRuns, 3);
        assert_eq!(set_phase_quiet(Phase::Load), Phase::Diagnostic);
        assert!(!sidecar.exists(), "a quiet switch writes no partial sidecar");
        // The loud one still does.
        assert_eq!(set_phase(Phase::Emit), Phase::Load);
        assert!(sidecar.exists());
        end_model();
        let text = std::fs::read_to_string(&sidecar).expect("final");
        assert!(text.contains("{\"phase\":\"Diagnostic\""), "{text}");
        let diagnostic = text.split("{\"phase\":\"Diagnostic\"").nth(1).expect("the phase");
        assert!(diagnostic.contains("\"sweep.runs\":3"), "{text}");
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
    fn a_side_file_lands_beside_the_sidecar_and_only_while_a_model_is_in_flight() {
        let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = scratch("sidefile");
        ENABLED.store(false, Relax);
        assert!(write_side_file("orient.json", b"x").expect("off").is_none());
        enable(dir.clone(), HOUR);
        end_model();
        assert!(write_side_file("orient.json", b"x").expect("no model").is_none());
        begin_model("A B");
        let written = write_side_file("orient.json", b"{\"k\":1}")
            .expect("written")
            .expect("a model is in flight");
        assert_eq!(written, dir.join("A B.orient.json"));
        assert_eq!(std::fs::read(&written).expect("read back"), b"{\"k\":1}");
        assert!(!dir.join("A B.orient.json.tmp").exists());
        end_model();
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
    fn pose_frames_rows_stages_and_sites_reach_the_sidecar() {
        let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = scratch("frames-rows");
        enable(dir.clone(), HOUR);
        begin_model("fr");
        set_phase(Phase::Production);
        assert!(with_pose_slot(|n: &mut u32| *n += 1).is_none(), "no frame yet");
        pose_label(|| "pose A".to_owned());
        {
            let _run = span(Span::SweepRun);
            add(Work::SweepDownCallLipSeat, 1);
            assert_eq!(size_site_peek(), Span::SizeLipSeat as u64);
            {
                let _size = size_span();
                std::thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(size_site_peek(), u64::MAX, "the site is taken once");
            assert_eq!(with_pose_slot(|n: &mut u32| { *n += 1; *n }), Some(1));
            assert_eq!(with_pose_slot(|n: &mut u32| { *n += 1; *n }), Some(2));
            // A frame opened inside another is a nested one for the outer.
            pose_label(|| "inner".to_owned());
            {
                let _inner = span(Span::SweepRun);
                let _pass = span(Span::SweepPass);
                std::thread::sleep(Duration::from_millis(2));
            }
            row(Rows::TipBatch, &[7, 9, 11]);
            // A row stored from inside the slot closure (where the frame list
            // is borrowed) still carries the pose id.
            with_pose_slot(|_: &mut u32| row(Rows::TipBatch, &[1, 2, 3]));
        }
        {
            let _mark = stage("test.stage");
        }
        end_model();
        let text = std::fs::read_to_string(dir.join("fr.perf.json")).expect("final");
        assert!(text.contains("\"label\":\"pose A\""), "{text}");
        assert!(text.contains("\"label\":\"inner\""), "{text}");
        // The sizing span reached the outer frame, the pass span the inner one.
        let outer = &text[text.find("\"label\":\"pose A\"").expect("outer")..];
        let outer = &outer[..outer.find("}}").expect("end of outer") + 2];
        assert!(outer.contains("\"sizing.lip_seat\":{\"calls\":1"), "{outer}");
        assert!(!outer.contains("\"down.pass\""), "{outer}");
        let inner = &text[text.find("\"label\":\"inner\"").expect("inner")..];
        let inner = &inner[..inner.find("}}").expect("end of inner") + 2];
        assert!(inner.contains("\"down.pass\":{\"calls\":1"), "{inner}");
        assert!(!outer.contains("\"nested_ms\":0.000"), "the outer frame saw the nested one: {outer}");
        let want = format!("[5,{},7,9,11]", pose_id("pose A"));
        assert!(text.contains(&want), "{want} not in {text}");
        let want = format!("[5,{},1,2,3]", pose_id("pose A"));
        assert!(text.contains(&want), "{want} not in {text}");
        assert!(text.contains("\"tip.batch\":{\"cols\":[\"phase\",\"pose\",\"rows\",\"faces\",\"wall_ns\"]"), "{text}");
        assert!(text.contains("\"label\":\"test.stage\",\"calls\":1"), "{text}");
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
