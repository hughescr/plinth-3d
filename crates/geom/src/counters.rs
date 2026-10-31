//! Batch counters: how many calls each batch operation received and how many items those
//! calls processed (design section 8, "Counters before tuning"). A hot path that makes a
//! per-item call shows up as a mean of about 1 item per call.
//!
//! The counters are thread-local and cost one add per batch call, never one per item. A
//! batch operation records on the calling thread only; helper threads it starts do not
//! record, so one batch call is always one record.
//!
//! A parallel batch also leaves a [`ParReport`] on the calling thread: how many tasks it
//! made and how many distinct threads ran them, measured from the threads that really ran.
//! [`take_par`] reads it through the public entrypoint, so a test can prove the allocation
//! was respected without a second code path.

use std::cell::Cell;

/// A batch operation that is counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `closest_points` and `closest_points_par`: items are query points.
    ClosestPoints,
    /// Morton key computation: items are positions.
    MortonKeys,
    /// BVH construction: items are triangles.
    BvhBuild,
    /// `slice::slice_layers`: items are layers.
    SliceLayers,
    /// `raster::rasterise`: items are layers.
    Rasterise,
    /// `label::label_components`: items are layers.
    LabelComponents,
    /// `patch::ball_patches`: items are balls.
    BallPatches,
    /// `clearance::solid_gaps`: items are solids.
    SolidGaps,
    /// `ray::first_hits`: items are rays.
    FirstHits,
}

const OPS: usize = 9;

impl Op {
    const fn slot(self) -> usize {
        match self {
            Self::ClosestPoints => 0,
            Self::MortonKeys => 1,
            Self::BvhBuild => 2,
            Self::SliceLayers => 3,
            Self::Rasterise => 4,
            Self::LabelComponents => 5,
            Self::BallPatches => 6,
            Self::SolidGaps => 7,
            Self::FirstHits => 8,
        }
    }
}

/// Calls and items seen by one operation on this thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Count {
    /// Number of batch calls.
    pub calls: u64,
    /// Total items over all those calls.
    pub items: u64,
}

impl Count {
    const ZERO: Self = Self { calls: 0, items: 0 };

    /// Mean items per call, or `None` when there were no calls.
    #[must_use]
    pub fn mean_items_per_call(&self) -> Option<f64> {
        if self.calls == 0 {
            None
        } else {
            #[allow(clippy::cast_precision_loss, reason = "counters stay far below 2^53")]
            Some(self.items as f64 / self.calls as f64)
        }
    }
}

/// What the last parallel batch on this thread did: the tasks it made and the distinct
/// threads that ran them (the calling thread included). Both are 0 for an empty batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParReport {
    /// Number of tasks the batch was split into.
    pub tasks: usize,
    /// Number of distinct threads that ran a task, measured by thread identity.
    pub threads: usize,
}

thread_local! {
    static COUNTS: Cell<[Count; OPS]> = const { Cell::new([Count::ZERO; OPS]) };
    static LAST_PAR: Cell<Option<ParReport>> = const { Cell::new(None) };
}

/// Keep the report of a parallel batch that just finished (one store per batch).
pub(crate) fn record_par(r: ParReport) {
    LAST_PAR.with(|c| c.set(Some(r)));
}

/// The report of the last parallel batch finished on the calling thread, cleared by the
/// read. `None` when no batch finished since the last read.
#[must_use]
pub fn take_par() -> Option<ParReport> {
    LAST_PAR.with(Cell::take)
}

/// Record one batch call of `items` items (called once at the top of a batch function).
pub(crate) fn record(op: Op, items: usize) {
    COUNTS.with(|c| {
        let mut all = c.get();
        let e = &mut all[op.slot()];
        e.calls += 1;
        e.items += items as u64;
        c.set(all);
    });
}

/// The counts of `op` on the calling thread since the last [`reset`].
#[must_use]
pub fn snapshot(op: Op) -> Count {
    COUNTS.with(|c| c.get()[op.slot()])
}

/// Zero every counter of the calling thread.
pub fn reset() {
    COUNTS.with(|c| c.set([Count::ZERO; OPS]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threads_records_calls_and_items_per_thread() {
        // The test starts one helper thread: allowed only when the allocation is 2 or more.
        let spawn_ok = crate::testutil::slot_threads() >= 2;
        reset();
        record(Op::ClosestPoints, 10);
        record(Op::ClosestPoints, 30);
        let c = snapshot(Op::ClosestPoints);
        assert_eq!((c.calls, c.items), (2, 40));
        assert_eq!(c.mean_items_per_call(), Some(20.0));
        assert_eq!(snapshot(Op::MortonKeys), Count::default());
        assert_eq!(snapshot(Op::MortonKeys).mean_items_per_call(), None);
        if spawn_ok {
            std::thread::spawn(|| assert_eq!(snapshot(Op::ClosestPoints), Count::default()))
                .join()
                .expect("thread");
        }
        reset();
        assert_eq!(snapshot(Op::ClosestPoints), Count::default());
    }

    #[test]
    fn par_report_is_kept_per_thread_and_cleared_by_the_read() {
        let _ = take_par();
        let r = ParReport {
            tasks: 2,
            threads: 2,
        };
        record_par(r);
        assert_eq!(take_par(), Some(r));
        assert_eq!(take_par(), None);
    }
}
