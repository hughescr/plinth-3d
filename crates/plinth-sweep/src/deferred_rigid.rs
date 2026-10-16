//! The rigid-body share reading, run later than the sweep that produced the
//! field (perf lane `ov`, 2026-10-15).
//!
//! [`crate::rigid_share::diagnose`] is a WARNING: it reads the finished, sized
//! field along the routing the last down-sweep recorded, and it moves and
//! sizes nothing. Nothing a later stage or the second-contact chooser reads
//! depends on it, yet it ran inside every emitted sweep, on the critical path:
//! about 0.4 s in the winner's production sweep and again in the second
//! contact's re-plan, which is discarded on most corpus models (kept on 6 of 23).
//!
//! A sweep with [`crate::SweepPolicy::rigid_share_capture`] stores a
//! [`DeferredRigidShare`] instead of running the reading. [`complete_rigid_share`]
//! (or [`crate::complete_for_emission`]) runs it, on the plan that is emitted
//! only; the caller may also take the snapshot out of the plan
//! ([`take_deferred_rigid_share`]) and run it beside other work.
//!
//! **Why a snapshot and not a re-run on `plan.struts`.** The reading belongs
//! to the field as it stands at the end of sizing, BEFORE the seat-face pass
//! and the footprint rule move seats. The snapshot is taken on the very line
//! the in-run call stood on, from the very arguments, so the numbers are
//! identical. Everything in it is shared (`Arc`), so cloning a plan that holds
//! one copies nothing.

use std::sync::Arc;

use plinth_support::perception::SliceSolid;

use crate::grid::Grid;
use crate::per_event::RouteLog;
use crate::rigid_share::{self, RigidShareReport};
use crate::{SweepPlan, SweepStrut};

struct Snapshot {
    grid: Grid,
    slices: Arc<Vec<Vec<SliceSolid>>>,
    struts: Vec<SweepStrut>,
    log: RouteLog,
    layer_height_mm: f64,
    l_max: f64,
    tip_radius_max_mm: f64,
}

/// Exactly the arguments of [`rigid_share::diagnose`], owned and shared.
#[derive(Clone)]
pub struct DeferredRigidShare {
    snapshot: Arc<Snapshot>,
}

impl DeferredRigidShare {
    /// Copy the struts, take the route log and share the slices, as the
    /// in-run call would have been handed them.
    #[must_use]
    pub(crate) fn capture(
        grid: &Grid,
        slices: Arc<Vec<Vec<SliceSolid>>>,
        struts: &[SweepStrut],
        log: RouteLog,
        layer_height_mm: f64,
        l_max: f64,
        tip_radius_max_mm: f64,
    ) -> Self {
        Self {
            snapshot: Arc::new(Snapshot {
                grid: *grid,
                slices,
                struts: struts.to_vec(),
                log,
                layer_height_mm,
                l_max,
                tip_radius_max_mm,
            }),
        }
    }

    /// Run the reading the sweep would have run.
    #[must_use]
    pub fn run(&self) -> RigidShareReport {
        let snapshot = &*self.snapshot;
        rigid_share::diagnose(
            &snapshot.grid,
            &snapshot.slices,
            &snapshot.struts,
            &snapshot.log,
            snapshot.layer_height_mm,
            snapshot.l_max,
            snapshot.tip_radius_max_mm,
        )
    }
}

impl core::fmt::Debug for DeferredRigidShare {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // A snapshot of a whole field is not something to print.
        formatter
            .debug_struct("DeferredRigidShare")
            .field("struts", &self.snapshot.struts.len())
            .finish_non_exhaustive()
    }
}

/// Run the reading a sweep deferred and store its result where the sweep would
/// have put it. Returns whether a reading was pending. A plan whose reading
/// already ran (or was never deferred) is left as it is.
pub fn complete_rigid_share(plan: &mut SweepPlan) -> bool {
    let Some(deferred) = plan.deferred_rigid_share.take() else {
        return false;
    };
    plan.report.rigid_share = deferred.run();
    true
}

/// Take the pending reading out of `plan`, to run it somewhere else. The plan
/// reads as "did not run" in its place until the result is stored with
/// [`store_rigid_share`].
pub fn take_deferred_rigid_share(plan: &mut SweepPlan) -> Option<DeferredRigidShare> {
    plan.deferred_rigid_share.take()
}

/// Store a result obtained from [`DeferredRigidShare::run`] on the plan the
/// snapshot was taken out of.
pub fn store_rigid_share(plan: &mut SweepPlan, report: RigidShareReport) {
    plan.report.rigid_share = report;
}
