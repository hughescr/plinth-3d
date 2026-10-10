//! Deterministic parallel scaffolding for the two surface walks of this crate:
//! the curvature spread ([`crate::delicacy_eval`]) and the display
//! neighbourhood pass ([`crate::display_eval`]).
//!
//! # The rule that keeps the bytes still
//!
//! Both walks are, per work item (a welded vertex, a face), a pure function of
//! shared read-only arrays. The curvature spread is a SCATTER: item `v` adds
//! `amount` to `charge[face]` for each face its ball reaches, and the serial
//! code adds in ascending-vertex, then walk, order. Floating-point addition is
//! not associative, so that order is part of the answer.
//!
//! The scatter is therefore split in two:
//!
//! 1. **Walk** (parallel): fixed vertex blocks each return their
//!    `(face, amount)` pairs, in walk order, as a flat stream. Which worker ran
//!    a block, and when, changes nothing in the stream.
//! 2. **Replay** (serial): the streams are applied to `charge` in ascending
//!    block order. Every face then receives the same addends, in the same
//!    order, as in the serial walk, so the sums are the same bits at any
//!    thread count, block size or wave size.
//!
//! The amount is computed in the walk (`value * inside`) and added in the
//! replay. Rust never contracts a multiply and an add into an FMA and has no
//! fast-math, so splitting them gives the same bits.
//!
//! # Memory is bounded by waves and a soft cap
//!
//! A wave holds at most [`SpreadPlan::wave_blocks`] block streams. A block
//! stops early once its stream reaches [`SpreadPlan::pairs_per_block_cap`]
//! pairs and reports the first unprocessed vertex; the replay applies the
//! prefix, then splits the remainder into smaller blocks and replays them in
//! place, before the next block of the wave. The cap is soft (a block always
//! finishes the vertex it is on) and, like the block and wave sizes, decides
//! only WHEN an addend is computed, never what it is or where it sits in the
//! order.

use std::ops::Range;
use std::sync::Mutex;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

/// Execution limits for a spread: memory and scheduling only, never a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SpreadPlan {
    /// Vertices per block.
    pub(crate) block_vertices: u32,
    /// Blocks run (and held) at once.
    pub(crate) wave_blocks: usize,
    /// Soft cap on the `(face, amount)` pairs one block holds.
    pub(crate) pairs_per_block_cap: usize,
}

impl SpreadPlan {
    /// The production plan: 512-vertex blocks, 32-block waves, a 1 Mi-pair cap
    /// (12 MiB of stream per block, under 0.5 GiB per wave).
    pub(crate) const PRODUCTION: Self = Self {
        block_vertices: 512,
        wave_blocks: 32,
        pairs_per_block_cap: 1 << 20,
    };
}

/// What the wave driver did. Exact counts of the (deterministic) block plan:
/// they depend on the plan and the data, never on the thread count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WaveStats {
    /// Blocks run, remainder blocks included.
    pub(crate) blocks: usize,
    /// Waves run, remainder waves included.
    pub(crate) waves: usize,
    /// Blocks that hit the pair cap and were continued as a remainder.
    pub(crate) overflow_splits: usize,
}

/// Test-only mutation points of the wave driver. The production type
/// ([`Exact`]) leaves both off; the generic parameter is monomorphised, so the
/// branches are compiled out of the production driver.
pub(crate) trait SpreadHooks {
    /// Replay the blocks of a wave in DESCENDING order (a sabotage: it must
    /// change the sums on a fixture where order matters).
    const REVERSE_BLOCKS: bool = false;
    /// Skip the replay of an overflow remainder (a sabotage).
    const DROP_REMAINDER: bool = false;
}

/// The production driver: nothing mutated.
pub(crate) struct Exact;
impl SpreadHooks for Exact {}

/// A free list of per-worker scratch values. One take and one return per
/// block, so the lock is touched twice per block and never inside a walk.
pub(crate) struct ScratchPool<T>(Mutex<Vec<T>>);

impl<T> ScratchPool<T> {
    pub(crate) const fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }

    /// Takes a scratch value, or builds a fresh one when none is free.
    pub(crate) fn take(&self, make: impl FnOnce() -> T) -> T {
        let free = self.0.lock().ok().and_then(|mut list| list.pop());
        free.unwrap_or_else(make)
    }

    /// Returns a scratch value for the next block.
    pub(crate) fn put(&self, value: T) {
        if let Ok(mut list) = self.0.lock() {
            list.push(value);
        }
    }
}

/// Fixed, contiguous sub-ranges of `range`, `block_len` long (the last may be
/// shorter). `block_len` must be at least 1.
pub(crate) fn chunk(range: &Range<u32>, block_len: u32) -> Vec<Range<u32>> {
    assert!(block_len >= 1, "a block holds at least one item");
    let mut blocks = Vec::new();
    let mut at = range.start;
    while at < range.end {
        let end = range.end.min(at.saturating_add(block_len));
        blocks.push(at..end);
        at = end;
    }
    blocks
}

/// Runs `range` as waves of parallel blocks and replays the results serially,
/// in ascending block order, through `replay`.
///
/// * `spread(block)` is the pure block walk; it may stop early.
/// * `stopped_at(&out)` is `Some(s)` exactly when the block stopped early:
///   items `[block.start, s)` are in the output, `s` is the FIRST UNPROCESSED
///   item, and `block.start < s < block.end`. A block always completes its
///   first item, so a remainder is never empty and every block makes progress.
/// * `replay(&out)` applies one block's output (serial, in order).
///
/// An overflow remainder `[s, end)` is run as its own waves with blocks of
/// `max(1, ceil(len / wave_blocks))` items and replayed IMMEDIATELY after the
/// block's prefix and before the next block of the wave. Every call's range is
/// strictly shorter than its parent block and a one-item block cannot overflow,
/// so the recursion is at most `1 + log_wave_blocks(block_vertices)` deep.
///
/// Must run inside the pool that should do the work (the caller installs it).
pub(crate) fn spread_range<H, B, S, O, R>(
    range: Range<u32>,
    block_len: u32,
    plan: &SpreadPlan,
    stats: &mut WaveStats,
    spread: &S,
    stopped_at: &O,
    replay: &mut R,
) where
    H: SpreadHooks,
    B: Send,
    S: Fn(Range<u32>) -> B + Sync,
    O: Fn(&B) -> Option<u32>,
    R: FnMut(&B),
{
    assert!(block_len >= 1, "a block holds at least one item");
    assert!(plan.wave_blocks >= 1, "a wave holds at least one block");
    for wave in chunk(&range, block_len).chunks(plan.wave_blocks) {
        stats.waves += 1;
        stats.blocks += wave.len();
        let outputs: Vec<B> = wave
            .par_iter()
            .map(|block| spread(block.clone()))
            .collect();
        let order: Vec<usize> = if H::REVERSE_BLOCKS {
            (0..wave.len()).rev().collect()
        } else {
            (0..wave.len()).collect()
        };
        for index in order {
            let block = &wave[index];
            let output = &outputs[index];
            replay(output);
            if let Some(split) = stopped_at(output) {
                debug_assert!(
                    block.start < split && split < block.end,
                    "stopped_at {split} outside ({}, {})",
                    block.start,
                    block.end
                );
                stats.overflow_splits += 1;
                if H::DROP_REMAINDER {
                    continue;
                }
                let remainder = split..block.end;
                let next_len = (remainder.end - remainder.start)
                    .div_ceil(u32::try_from(plan.wave_blocks).unwrap_or(u32::MAX))
                    .max(1);
                spread_range::<H, B, S, O, R>(
                    remainder, next_len, plan, stats, spread, stopped_at, replay,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_tile_the_range_exactly() {
        for (len, block) in [(0_u32, 1_u32), (1, 1), (7, 7), (8, 7), (512, 512), (513, 512), (10, 3)] {
            let blocks = chunk(&(5..5 + len), block);
            let mut at = 5;
            for piece in &blocks {
                assert_eq!(piece.start, at);
                assert!(piece.end > piece.start && piece.end - piece.start <= block);
                at = piece.end;
            }
            assert_eq!(at, 5 + len);
        }
    }

    #[test]
    #[should_panic(expected = "at least one item")]
    fn a_zero_length_block_is_refused() {
        let _ = chunk(&(0..4), 0);
    }

    #[test]
    fn the_scratch_pool_reuses_what_it_is_given() {
        let pool = ScratchPool::<Vec<u8>>::new();
        let mut made = 0;
        let mut first = pool.take(|| {
            made += 1;
            Vec::new()
        });
        first.push(7);
        pool.put(first);
        let again = pool.take(|| {
            made += 1;
            Vec::new()
        });
        assert_eq!(again, vec![7]);
        assert_eq!(made, 1);
    }
}
