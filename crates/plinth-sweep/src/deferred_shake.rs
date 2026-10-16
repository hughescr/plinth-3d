//! The construction-stage shake check, run later than the sweep that produced
//! the field (`r1-winner-reuse`, 2026-10-09).
//!
//! The orientation search prices its three finalists with the full production
//! sweep and, when one of them wins, the pipeline needs that same plan again
//! for the file. Everything the sweep computes is a pure function of the mesh,
//! the layers and the policy, so the plan can be kept and reused. The shake
//! check ([`crate::stage_shake`]) is the one expensive step that only the
//! EMITTED plan needs (it feeds one WARNING line; no pose score reads it). So a
//! measurement sweep with [`crate::SweepPolicy::stage_shake_capture`] stores a
//! [`DeferredStageShake`] instead of running the check, and
//! [`complete_for_emission`] runs it, on the winner only.
//!
//! **Why a snapshot and not a re-run on `plan.struts`.** The check reads the
//! field as it stands at the end of the sweep's sizing, BEFORE the seat-face
//! pass and the footprint rule move seats. A re-run on the finished plan would
//! read other struts and print other numbers. The snapshot is taken on the
//! same line, from the same arguments, so the numbers are identical.

use crate::grid::Grid;
use crate::slenderness::SlendernessLaw;
use crate::stage_shake::{self, StageShake};
use crate::{SweepPlan, SweepStrut};

/// Exactly the arguments of [`stage_shake::check`], owned.
#[derive(Clone)]
pub struct DeferredStageShake {
    grid: Grid,
    struts: Vec<SweepStrut>,
    profiles: Vec<Vec<f64>>,
    layer_height_mm: f64,
    law: SlendernessLaw,
    pressure_kpa: f64,
}

impl DeferredStageShake {
    /// Copy the struts and take the profiles and law the in-run check would
    /// have been called with.
    #[must_use]
    pub fn capture(
        grid: &Grid,
        struts: &[SweepStrut],
        profiles: Vec<Vec<f64>>,
        layer_height_mm: f64,
        law: SlendernessLaw,
        pressure_kpa: f64,
    ) -> Self {
        Self {
            grid: *grid,
            struts: struts.to_vec(),
            profiles,
            layer_height_mm,
            law,
            pressure_kpa,
        }
    }

    /// Run the check the sweep would have run.
    #[must_use]
    pub fn run(&self) -> StageShake {
        stage_shake::check(
            &self.grid,
            &self.struts,
            &self.profiles,
            self.layer_height_mm,
            &self.law,
            self.pressure_kpa,
        )
    }
}

impl core::fmt::Debug for DeferredStageShake {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // A snapshot of a whole field is not something to print.
        formatter
            .debug_struct("DeferredStageShake")
            .field("struts", &self.struts.len())
            .field("layer_height_mm", &self.layer_height_mm)
            .field("pressure_kpa", &self.pressure_kpa)
            .finish_non_exhaustive()
    }
}

/// Run the shake check a measurement sweep deferred, and store its result where
/// the sweep would have put it. Returns whether a check was pending. A plan
/// whose check already ran (or was never deferred) is left as it is.
pub fn complete_for_emission(plan: &mut SweepPlan) -> bool {
    // The rigid-body share reading is deferred the same way
    // ([`crate::deferred_rigid`]) and is completed here too.
    let rigid = crate::deferred_rigid::complete_rigid_share(plan);
    let Some(deferred) = plan.deferred_stage_shake.take() else {
        return rigid;
    };
    plan.report.slenderness_lane.stage_shake = deferred.run();
    true
}
