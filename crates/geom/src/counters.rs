//! Batch counters: how many calls each batch operation received and how many items those
//! calls processed (design section 8, "Counters before tuning"). A hot path that makes a
//! per-item call shows up as a mean of about 1 item per call.
//!
//! The counters are thread-local and cost one add per batch call, never one per item. A
//! batch operation records on the calling thread only; helper threads it starts do not
//! record, so one batch call is always one record.

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
}

const OPS: usize = 3;

impl Op {
    const fn slot(self) -> usize {
        match self {
            Self::ClosestPoints => 0,
            Self::MortonKeys => 1,
            Self::BvhBuild => 2,
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

thread_local! {
    static COUNTS: Cell<[Count; OPS]> = const { Cell::new([Count::ZERO; OPS]) };
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
    fn records_calls_and_items_per_thread() {
        reset();
        record(Op::ClosestPoints, 10);
        record(Op::ClosestPoints, 30);
        let c = snapshot(Op::ClosestPoints);
        assert_eq!((c.calls, c.items), (2, 40));
        assert_eq!(c.mean_items_per_call(), Some(20.0));
        assert_eq!(snapshot(Op::MortonKeys), Count::default());
        assert_eq!(snapshot(Op::MortonKeys).mean_items_per_call(), None);
        std::thread::spawn(|| assert_eq!(snapshot(Op::ClosestPoints), Count::default()))
            .join()
            .expect("thread");
        reset();
        assert_eq!(snapshot(Op::ClosestPoints), Count::default());
    }
}
