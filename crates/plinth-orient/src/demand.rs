//! The interface LANE 3 (`docs/plans/slice-sweep-core-2026-09-02.md` §12)
//! uses to replace the objective's projected-area support proxy with the
//! sweep's real measured demand at the commit tier.
//!
//! `score::score_one`'s `support_area_cm2` is a PROXY — the raw down-facing
//! projected area, deliberately without the support engine's exterior,
//! near-plate and grounded-column filters (`score.rs`'s own module doc). §12
//! asks for the sweep's real measured demand instead, at least where the
//! decision is actually made (the commit tier — `FidelityLedger`'s own
//! doctrine).
//!
//! # Why a trait, not a direct call
//!
//! `plinth-orient` cannot depend on `plinth-sweep` directly: `plinth-sweep`
//! already depends on `plinth-support`, which depends on `plinth-hollow`,
//! which depends on `plinth-orient` (to orient a mesh before choosing a
//! drain direction) — `orient -> sweep -> support -> hollow -> orient` is a
//! genuine package cycle, not a stylistic preference (measured: `cargo check`
//! refuses it outright). This trait is the dependency-inversion seam: the
//! real implementation lives in `plinth-sweep` (which is free to depend on
//! `plinth-orient` for [`UpDirection`] and [`crate::oriented_recentered_mesh`],
//! since nothing `plinth-orient` depends on reaches back to `plinth-sweep`),
//! and the caller that wires the two together is `plinth-pipeline`, which
//! already depends on both.

use plinth_mesh::Mesh;

use crate::score::UpDirection;

/// What one candidate's real sweep measured, beyond the one number
/// (`support_area_cm2`) that feeds the objective — kept for the run report so
/// a reader can see the measurement was real rather than assumed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasuredSupportDemand {
    /// The quantity that replaces the projected-area proxy in the
    /// objective's support term, at the SAME registered weight
    /// (`ORIENT_WEIGHT_SUPPORT_AREA_CM2`, unchanged in value).
    pub support_area_cm2: f64,
    pub struts: usize,
    pub achieved_pitch_mm: f64,
    pub layers: usize,
    pub residual_demand_cells: usize,
    pub uncarried_load_mm2: f64,
}

/// A pluggable measurer of one candidate's real support demand.
///
/// # Errors
///
/// Any `Err(String)` is a reason the caller (the commit-tier override inside
/// `search::search`) FALLS BACK to the projected-area proxy for this one
/// candidate and records the failure by name — never a reason to refuse the
/// whole search (emit and warn, never refuse). The error is a plain `String`
/// rather than a typed enum specifically so this trait never has to name a
/// `plinth-sweep` error type, which would reintroduce the dependency this
/// trait exists to invert.
pub trait SupportDemandMeasurer {
    fn measure_support_area_cm2(
        &self,
        mesh: &Mesh,
        candidate: UpDirection,
    ) -> Result<MeasuredSupportDemand, String>;
}
