//! **The lip-seat clearance memo** (perf lane memo, 2026-10-15): the answer of
//! every bounded solid-clearance row the lip-seat rule sends to the kernel,
//! kept by the row's EXACT kernel inputs, for one model surface.
//!
//! # Why
//!
//! The lip-seat rule asks the same rows again and again: its deep scan walks
//! the same candidate lists in lip seats A and B (two `apply` calls each), and
//! the second-contact re-plan (P2) starts from the winner's prefix and walks
//! what the winner walked. S2's exact-key probe counted the repeats
//! (`plinth-evidence/perf-2026-10-14/memo/RESULTS.md`): on Deinonychus 68 % of
//! the winner's mover rows and 86 % of P2's were asked earlier in the same
//! chain of plans.
//!
//! # Why a hit is exact
//!
//! [`crate::lip_seat`]'s bounded read (`ReadFrame::solid_at_least`) builds,
//! per row, the five kernel inputs (seat, base, radius, seat inset, landing
//! zone) and hands them with the row's bound to
//! [`geom_kernel::NativeSurface::segment_clearance_classified_at_least`] with
//! constant everything else (`r0 = r1 = radius`, standoff `0`, no excluded
//! faces, the capsule reading). The kernel's batch is an index-parallel map:
//! row `i` reads only the inputs at `i` and the surface, its walk is a pure,
//! deterministic function of them, and the only state it touches is
//! result-inert telemetry (scratch stacks, counters). So a row's answer is a
//! function of `(surface, the 5 inputs, the bound)` alone, whatever batch
//! carries it. [`RowKey`] holds those ten numbers by their bits, and a memo
//! holds rows of ONE surface (below), so a key that matches is the same
//! question, and the kept answer is the kernel's answer, bit for bit.
//!
//! **One surface per memo, by ownership, not by address.** The memo lives
//! in the plan's [`crate::sweep_state::Prefix`] beside the model surface cell
//! it reads (`model_surface`), shared by every clone of that prefix as the
//! cell is, and it is reached only through
//! [`crate::head_join::HeadJoinContext::clearances`], whose `surface` is that
//! same cell. A prefix is consumed only with the mesh it was built from
//! ([`crate::sweep_state::EditSeed::miss`]), so the P2 re-plan that starts
//! from the winner's prefix reads the winner's surface, the very same object.
//!
//! # Determinism and contention
//!
//! A memo belongs to one chain of plans (a sweep, then the re-plans started
//! from its prefix), and that chain runs one plan at a time and asks its
//! reads from one driver at a time, so which rows hit is a function of the
//! sequence of reads alone, never of the thread count: the `work` counters
//! stay scheduling-independent. The lock is taken twice per batch (look up,
//! then keep), never held across a kernel call or any parallel work, and no
//! other plan shares it, so it is never contended.
//!
//! # Memory
//!
//! Every byte the table has ALLOCATED (its buckets' capacity, not its
//! length) is charged to the memo's ledger, whose limit is
//! `PLINTH_SWEEP_LIPSEAT_MEMO_BYTES` (default [`MEMO_BYTES`]; `0` turns the
//! memo off). A growth that would take the table past the limit is not made:
//! the rows that still fit the allocated table are kept, the rest are not,
//! and from then on the memo only answers. Whether the limit trips changes no
//! answer: an unkept row is read again from the kernel when it is asked.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::sync::Mutex;

/// The bytes one memo's table may hold allocated, by default: room for
/// 2^20 buckets (917,504 rows) at 97 bytes each. A Deinonychus chain asks
/// about 0.5 M distinct rows (measured, lane memo).
pub(crate) const MEMO_BYTES: usize = 128 << 20;

/// [`MEMO_BYTES`], or `PLINTH_SWEEP_LIPSEAT_MEMO_BYTES` where set (a
/// diagnostic: the output does not depend on it; `0` is off). Read once.
fn memo_limit_bytes() -> usize {
    static LIMIT: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *LIMIT.get_or_init(|| {
        std::env::var("PLINTH_SWEEP_LIPSEAT_MEMO_BYTES")
            .ok()
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(MEMO_BYTES)
    })
}

/// The exact key of one bounded clearance row: the seat (3), the base (3),
/// the radius, the seat inset, the landing zone, and the bound, by their
/// bits. Built only by [`RowKey::of`], which takes each by name.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct RowKey([u64; 10]);

impl RowKey {
    /// The key of the row with these kernel inputs and this bound.
    pub(crate) fn of(seat: [f64; 3], base: [f64; 3], radius: f64, inset: f64, landing_zone: f64, bound: f64) -> Self {
        Self([
            seat[0].to_bits(),
            seat[1].to_bits(),
            seat[2].to_bits(),
            base[0].to_bits(),
            base[1].to_bits(),
            base[2].to_bits(),
            radius.to_bits(),
            inset.to_bits(),
            landing_zone.to_bits(),
            bound.to_bits(),
        ])
    }
}

impl Hash for RowKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for &word in &self.0 {
            state.write_u64(word);
        }
    }
}

/// A fast word hasher for [`RowKey`] (no untrusted keys reach it): a
/// multiply-rotate fold, finished with `fmix64`'s avalanche so the table's
/// top-bit tags are well spread.
#[derive(Default)]
pub(crate) struct RowHasher(u64);

impl Hasher for RowHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_u64(u64::from(byte));
        }
    }

    fn write_u64(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95);
    }

    fn finish(&self) -> u64 {
        let mut h = self.0;
        h ^= h >> 33;
        h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
        h ^= h >> 33;
        h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        h ^ (h >> 33)
    }
}

type Table = HashMap<RowKey, Option<f64>, BuildHasherDefault<RowHasher>>;

/// The bytes one bucket of the table holds: a key, an answer, a control byte.
const BUCKET_BYTES: usize = std::mem::size_of::<(RowKey, Option<f64>)>() + 1;

/// The buckets a table of `capacity` rows has allocated (the table's own
/// rule: up to 7/8 full past 8 buckets, a power of two).
const fn buckets_of(capacity: usize) -> usize {
    if capacity == 0 {
        0
    } else if capacity < 8 {
        (capacity + 1).next_power_of_two()
    } else {
        (capacity.saturating_mul(8) / 7).next_power_of_two()
    }
}

/// The bytes a table allocated for `capacity` rows: its buckets, and the
/// control group past their end.
const fn bytes_of(capacity: usize) -> usize {
    let buckets = buckets_of(capacity);
    if buckets == 0 {
        0
    } else {
        buckets * BUCKET_BYTES + 16
    }
}

/// What a memo did (for the log and the tests).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MemoStats {
    /// Rows answered from the memo.
    pub(crate) hits: u64,
    /// Rows sent to the kernel.
    pub(crate) misses: u64,
    /// Of those, rows not kept for want of room.
    pub(crate) unkept: u64,
    /// The bytes the table has allocated now (what the ledger holds).
    pub(crate) held: usize,
    /// The most the ledger held, a growth's old and new tables both counted.
    pub(crate) peak: usize,
}

struct Inner {
    table: Table,
    /// A growth was refused: keep nothing more.
    full: bool,
    stats: MemoStats,
    /// Tests: every hit reads `None` (the memo lies).
    #[cfg(test)]
    sabotage: bool,
}

/// **The bounded clearance rows already asked of ONE model surface**, kept by
/// their exact inputs (module doc). Held by a plan's prefix, shared by its
/// clones, read only through `lip_seat::ReadFrame::solid_at_least`.
pub(crate) struct ClearanceMemo {
    /// The ledger's limit: [`memo_limit_bytes`] (`0`: off).
    limit: usize,
    inner: Mutex<Inner>,
}

impl Default for ClearanceMemo {
    fn default() -> Self {
        Self::with_limit(memo_limit_bytes())
    }
}

impl ClearanceMemo {
    /// A memo whose table may hold `limit` bytes allocated (`0`: off).
    pub(crate) fn with_limit(limit: usize) -> Self {
        Self {
            limit,
            inner: Mutex::new(Inner {
                table: Table::default(),
                full: false,
                stats: MemoStats::default(),
                #[cfg(test)]
                sabotage: false,
            }),
        }
    }

    /// Whether the memo is in use (a limit above zero).
    pub(crate) const fn on(&self) -> bool {
        self.limit > 0
    }

    /// The kept answer of each of `keys`, `None` where the memo holds none.
    /// A poisoned lock answers nothing (every row goes to the kernel).
    pub(crate) fn look_up(&self, keys: &[RowKey]) -> Vec<Option<Option<f64>>> {
        let Ok(mut inner) = self.inner.lock() else {
            return vec![None; keys.len()];
        };
        let found: Vec<Option<Option<f64>>> = keys.iter().map(|key| inner.table.get(key).copied()).collect();
        let hits = found.iter().filter(|row| row.is_some()).count() as u64;
        inner.stats.hits += hits;
        inner.stats.misses += keys.len() as u64 - hits;
        #[cfg(test)]
        if inner.sabotage {
            return found.into_iter().map(|row| row.map(|_| None)).collect();
        }
        found
    }

    /// Keep `answers[j]` under `keys[rows[j]]`, in order, while the ledger
    /// has room (module doc, "Memory"). Returns how many were not kept.
    pub(crate) fn keep(&self, keys: &[RowKey], rows: &[usize], answers: &[Option<f64>]) -> u64 {
        debug_assert_eq!(rows.len(), answers.len(), "one answer per row sent");
        let Ok(mut inner) = self.inner.lock() else {
            return rows.len() as u64;
        };
        let inner = &mut *inner;
        let wanted = rows.len();
        let len = inner.table.len();
        let capacity = inner.table.capacity();
        // Grow only to a table the ledger can hold: the size the table would
        // grow to (at least one more row than it holds room for).
        if !inner.full && len + wanted > capacity {
            let grown = bytes_of((len + wanted).max(capacity + 1));
            if grown <= self.limit {
                let before = bytes_of(capacity);
                inner.table.reserve(wanted);
                let after = bytes_of(inner.table.capacity());
                inner.stats.peak = inner.stats.peak.max(before + after);
                inner.stats.held = after;
                debug_assert!(after <= self.limit, "the table grew past its ledger");
            } else {
                inner.full = true;
            }
        }
        // The rows that fit the allocated table, in order; never a growth.
        let room = inner.table.capacity().saturating_sub(inner.table.len());
        let (mut kept, mut taken) = (0_usize, 0_usize);
        for (&row, &answer) in rows.iter().zip(answers) {
            if kept == room {
                break;
            }
            // A key asked twice in one batch is kept once.
            if inner.table.insert(keys[row], answer).is_none() {
                kept += 1;
            }
            taken += 1;
        }
        let unkept = (wanted - taken) as u64;
        inner.stats.unkept += unkept;
        inner.stats.held = bytes_of(inner.table.capacity());
        inner.stats.peak = inner.stats.peak.max(inner.stats.held);
        unkept
    }

    /// What the memo did so far.
    pub(crate) fn stats(&self) -> MemoStats {
        self.inner.lock().map_or_else(|_| MemoStats::default(), |inner| inner.stats)
    }

    /// The heap bytes the table has allocated.
    pub(crate) fn approx_bytes(&self) -> u64 {
        self.stats().held as u64
    }

    /// Tests: make every hit read `None` (a lying memo).
    #[cfg(test)]
    pub(crate) fn sabotage(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.sabotage = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{bytes_of, ClearanceMemo, RowKey};

    fn key(i: u64) -> RowKey {
        #[expect(clippy::cast_precision_loss, reason = "small test indices")]
        let x = i as f64;
        RowKey::of([x, 0.0, 1.0], [x, 0.0, 0.0], 0.1, 0.2, 0.0, 0.3)
    }

    /// Kept rows read back by their exact key; a key differing in one bit
    /// (here the bound, then the sign of zero) is a miss.
    #[test]
    fn a_kept_row_is_read_back_only_by_its_exact_key() {
        let memo = ClearanceMemo::with_limit(1 << 20);
        let keys: Vec<RowKey> = (0..100).map(key).collect();
        let answers: Vec<Option<f64>> = (0..100_u32).map(|i| (i % 3 != 0).then(|| f64::from(i) * 0.01)).collect();
        let rows: Vec<usize> = (0..100).collect();
        assert_eq!(memo.keep(&keys, &rows, &answers), 0);
        let found = memo.look_up(&keys);
        for (got, want) in found.iter().zip(&answers) {
            assert_eq!(got.map(|v| v.map(f64::to_bits)), Some(want.map(f64::to_bits)));
        }
        let near = RowKey::of([0.0, 0.0, 1.0], [0.0, 0.0, 0.0], 0.1, 0.2, 0.0, 0.3 + f64::EPSILON);
        let signed = RowKey::of([-0.0, 0.0, 1.0], [0.0, 0.0, 0.0], 0.1, 0.2, 0.0, 0.3);
        assert_eq!(memo.look_up(&[near, signed]), vec![None, None]);
    }

    /// The ledger: the table's allocated bytes never pass the limit, rows past
    /// it are counted unkept, and the kept ones still read back.
    #[test]
    fn the_table_never_holds_more_than_its_limit() {
        let limit = bytes_of(1000);
        let memo = ClearanceMemo::with_limit(limit);
        let keys: Vec<RowKey> = (0..5000).map(key).collect();
        let answers = vec![Some(1.0); 5000];
        let mut unkept = 0;
        for chunk in (0..5000).collect::<Vec<usize>>().chunks(300) {
            memo.look_up(&chunk.iter().map(|&row| keys[row]).collect::<Vec<_>>());
            unkept += memo.keep(&keys, chunk, &answers[..chunk.len()]);
            assert!(memo.stats().held <= limit, "{:?}", memo.stats());
        }
        let stats = memo.stats();
        assert!(unkept > 0 && stats.unkept == unkept, "{stats:?}");
        assert!(stats.peak <= limit + limit / 2 + 64, "a growth holds the old and the new table: {stats:?}");
        let kept = memo.look_up(&keys).iter().filter(|row| row.is_some()).count();
        assert_eq!(kept as u64, 5000 - unkept);
        assert!(!ClearanceMemo::with_limit(0).on(), "a zero limit is off");
    }
}
