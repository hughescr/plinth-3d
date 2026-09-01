//! Process-global, result-inert counters for near-parallel Möller
//! classifications.
//!
//! # What this is, and what it deliberately is not
//!
//! `docs/specs/geometry-adjudication.md` §EXT-4 describes a per-model
//! `NearParallelLedger` threaded through triage, repair, the hollow mesh path,
//! orientation search, the pipeline report and the CLI. **That is not what this
//! module is**, by owner ruling: the plumbing had grown larger than the defect
//! it watches, and three review rounds were spent rediscovering that it keeps
//! being dropped at a stage boundary. What replaced it is here — one atomic per
//! Möller CALL SITE inside this kernel, incremented when the uncertain
//! classification fires, read once at the end of a run.
//!
//! That answers the only question actually being asked: *does refusing on
//! uncertainty make models stop emitting, and at what rate?* Per-model
//! attribution comes from the other direction — the corpus disposition
//! comparison names any model that emitted before and does not now.
//!
//! # Contract
//!
//! * **Result-inert.** Nothing in this module is read by any query, and no
//!   result of this kernel depends on a counter's value. This is the same
//!   status the determinism checklist in `lib.rs` already grants
//!   `bvh::BVH_BUILD_ATTEMPTS` and `NativeSurface::band_counter`: mutable
//!   state confined to telemetry atomics.
//! * **Process-global and monotone.** Counters accumulate across every
//!   [`crate::NativeSurface`] in the process and never decrease except through
//!   [`reset_near_parallel_counts`], which exists for tests.
//! * **`Relaxed` ordering.** The counters are a tally, not a synchronisation
//!   device; no other memory is published through them.
//! * **Saturating.** `fetch_add` on a `u64` cannot realistically wrap here
//!   (2^64 Möller evaluations), and the reader states the raw value.
//!
//! # Per-row uncertainty is NOT this
//!
//! Every classified entry point in [`crate::NativeSurface`] returns its own
//! per-row `near_parallel` tally, and that is the propagation contract the
//! callers must honour — a caller decides its disposition from ITS row's
//! tally, never from a global counter. These counters exist to answer the
//! run-rate question after the fact; collapsing a caller decision onto them
//! would be the silent-miss defect wearing a third name.

use std::sync::atomic::{AtomicU64, Ordering};

/// The collecting BVH walk (`queries.rs`, `collect_hits_with_stack`) — the
/// walk behind `cast_rays_classified`, the penetration interval batches and
/// `sphere_breach_classified`.
static COLLECTING_WALK: AtomicU64 = AtomicU64::new(0);

/// The counting BVH walk (`queries.rs`, `count_hits_with_stack`) — the walk
/// behind `cast_rays_census_classified` and
/// `penetration_census_batch_classified`.
static COUNTING_WALK: AtomicU64 = AtomicU64::new(0);

/// The `seg_tri_dist` pierce candidate (`queries.rs`) — behind
/// `segment_clearance_classified`, `segment_clearance_excluding_classified`
/// and `faces_near_segments_batch_classified`.
static SEG_TRI_PIERCE: AtomicU64 = AtomicU64::new(0);

/// The three Möller call sites inside this kernel, one atomic each.
///
/// These are CALL SITES, not callers: `CollectingWalk` is charged for every
/// near-parallel face the collecting walk evaluated, whichever public entry
/// point drove it. Attributing an increment to a stage is the run report's
/// job, from the per-row tallies the entry points return.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NearParallelSite {
    /// `collect_hits_with_stack`.
    CollectingWalk,
    /// `count_hits_with_stack`.
    CountingWalk,
    /// `seg_tri_dist`'s interior-pierce candidate.
    SegTriPierce,
}

impl NearParallelSite {
    /// Every site, in report order. A reader that iterates this cannot miss a
    /// site the way a hand-written match arm can.
    pub const ALL: [Self; 3] = [Self::CollectingWalk, Self::CountingWalk, Self::SegTriPierce];

    /// The stable label used in run output. These strings are the report keys;
    /// changing one changes what a corpus diff is comparing.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CollectingWalk => "kernel.walk.collecting",
            Self::CountingWalk => "kernel.walk.counting",
            Self::SegTriPierce => "kernel.seg_tri.pierce",
        }
    }

    const fn counter(self) -> &'static AtomicU64 {
        match self {
            Self::CollectingWalk => &COLLECTING_WALK,
            Self::CountingWalk => &COUNTING_WALK,
            Self::SegTriPierce => &SEG_TRI_PIERCE,
        }
    }
}

/// Charge `n` near-parallel classifications to `site`.
///
/// Called from the walks with the per-ray tally, once per ray, rather than
/// once per face — the atomic is off the innermost loop by construction.
/// `n == 0` still performs the add; the branch would cost more than the
/// uncontended no-op it avoids.
#[inline]
pub(crate) fn charge_near_parallel(site: NearParallelSite, n: u64) {
    site.counter().fetch_add(n, Ordering::Relaxed);
}

/// A snapshot of the three counters.
///
/// A snapshot is not atomic ACROSS sites: the three loads happen in sequence,
/// so a run still in flight can produce a snapshot no single instant ever
/// held. Read it at end of run, which is what it is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NearParallelCounts {
    /// `collect_hits_with_stack` evaluations classified `NearParallel`.
    pub collecting_walk: u64,
    /// `count_hits_with_stack` evaluations classified `NearParallel`.
    pub counting_walk: u64,
    /// `seg_tri_dist` pierce candidates classified `NearParallel`.
    pub seg_tri_pierce: u64,
}

impl NearParallelCounts {
    /// The count for one site.
    #[must_use]
    pub const fn get(self, site: NearParallelSite) -> u64 {
        match site {
            NearParallelSite::CollectingWalk => self.collecting_walk,
            NearParallelSite::CountingWalk => self.counting_walk,
            NearParallelSite::SegTriPierce => self.seg_tri_pierce,
        }
    }

    /// Every site summed.
    #[must_use]
    pub const fn total(self) -> u64 {
        self.collecting_walk + self.counting_walk + self.seg_tri_pierce
    }

    /// One line per site, in [`NearParallelSite::ALL`] order, for the end of a
    /// run.
    ///
    /// **A table of zeros is a result, not an omission.** Every site is
    /// printed whether or not it fired, so a run whose output is missing a row
    /// is an incomplete run rather than a quiet one.
    #[must_use]
    pub fn report(self) -> String {
        let mut out = String::new();
        for site in NearParallelSite::ALL {
            out.push_str(site.label());
            out.push(' ');
            out.push_str(&self.get(site).to_string());
            out.push('\n');
        }
        out
    }
}

/// Read the process-global near-parallel counters.
///
/// This is the counter-reading function: call it once at the end of a run and
/// print [`NearParallelCounts::report`].
#[must_use]
pub fn near_parallel_counts() -> NearParallelCounts {
    NearParallelCounts {
        collecting_walk: COLLECTING_WALK.load(Ordering::Relaxed),
        counting_walk: COUNTING_WALK.load(Ordering::Relaxed),
        seg_tri_pierce: SEG_TRI_PIERCE.load(Ordering::Relaxed),
    }
}

/// Zero every counter and return what they held.
///
/// For tests that assert a planted configuration fires a specific site. In a
/// process running real work this races every other consumer by construction —
/// the counters are global — so production code must not call it.
pub fn reset_near_parallel_counts() -> NearParallelCounts {
    NearParallelCounts {
        collecting_walk: COLLECTING_WALK.swap(0, Ordering::Relaxed),
        counting_walk: COUNTING_WALK.swap(0, Ordering::Relaxed),
        seg_tri_pierce: SEG_TRI_PIERCE.swap(0, Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::{charge_near_parallel, near_parallel_counts, NearParallelCounts, NearParallelSite};

    #[test]
    fn labels_are_distinct_and_cover_every_site() {
        let mut labels: Vec<&str> = NearParallelSite::ALL.iter().map(|s| s.label()).collect();
        assert_eq!(labels.len(), 3, "one label per site");
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), 3, "labels must be distinct report keys");
    }

    #[test]
    fn a_charge_is_visible_to_the_reader() {
        // Global counters: assert a DELTA, and only that it is at least the
        // planted amount. Another test in this binary may charge concurrently;
        // it can only ever add.
        let before = near_parallel_counts();
        charge_near_parallel(NearParallelSite::SegTriPierce, 7);
        let after = near_parallel_counts();
        assert!(
            after.seg_tri_pierce - before.seg_tri_pierce >= 7,
            "charge did not reach the reader: {before:?} -> {after:?}"
        );
    }

    #[test]
    fn report_prints_every_site_including_zeros() {
        let counts = NearParallelCounts::default();
        let text = counts.report();
        assert_eq!(text.lines().count(), 3, "a zero row is still a row: {text}");
        for site in NearParallelSite::ALL {
            assert!(
                text.contains(site.label()),
                "site {} missing from report: {text}",
                site.label()
            );
        }
        assert_eq!(counts.total(), 0);
    }
}
