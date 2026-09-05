//! The interface LANE 3 (`docs/plans/slice-sweep-core-2026-09-02.md` §12)
//! uses to replace the objective's projected-area support proxy with the
//! sweep's real measured SUPPORT BURDEN at the commit tier.
//!
//! `score::score_one`'s `support_area_cm2` is a PROXY — the raw down-facing
//! projected area, deliberately without the support engine's exterior,
//! near-plate and grounded-column filters (`score.rs`'s own module doc). §12
//! asks for the sweep's real measurement instead, at least where the
//! decision is actually made (the commit tier — `FidelityLedger`'s own
//! doctrine).
//!
//! # Two fidelities, because one of them cannot be afforded on every pose
//!
//! §12's prefilter is the projected down-facing area, and it already runs:
//! it is what ranks the coarse and refine tiers and decides which poses
//! reach the commit tier at all. What this trait adds is a SECOND prefilter
//! inside the commit tier, because the commit tier's admitted set is
//! `ORIENT_COMMIT_TOP_K` promoted plus the escorted legacy block (up to ~16
//! poses), and a production-pitch sweep of all of them is not affordable on
//! a tall model (LANE 2 measured one production sweep of the Weeping Treant
//! at 76.85 s).
//!
//! So: [`DemandFidelity::Prefilter`] measures every admitted pose on a
//! coarsened raster, and only the top `ORIENT_COMMIT_TOP_K` of those are
//! re-measured at [`DemandFidelity::Production`] — the pitch the emitted
//! file is actually built at. The prefilter PRUNES; only a production
//! measurement is allowed to decide, which is why `search` sorts
//! production-measured candidates ahead of prefilter-only ones regardless of
//! score. This matters and is measured: at the coarse pitch the arm counts
//! rank Round 25's 180° flip WRONG (88 arms flipped against 102 flat), and
//! at the production pitch they rank it right (70 flat against 88 flipped).
//! A coarse number is good enough to keep a pose in the running and not good
//! enough to crown it.
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

use serde::{Deserialize, Serialize};

use plinth_mesh::Mesh;

use crate::score::{OrientFaceTable, UpDirection};

/// Which raster the burden behind one number was measured on.
///
/// This is a FIDELITY label in the same sense as
/// [`crate::search::SearchTier::fidelity`]: a cost always states how much it
/// lied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DemandFidelity {
    /// A coarsened raster, run over every commit-tier candidate. Cheap
    /// enough for the whole admitted set and NOT trusted to pick a winner:
    /// its only job is to choose which poses are worth the real thing.
    Prefilter,
    /// The registered `SWEEP_WORKING_PITCH_MM` — the same raster the emitted
    /// support field is planned on. The only fidelity a winner may be
    /// chosen at.
    Production,
}

impl DemandFidelity {
    /// The word that goes in a record, so a reader never has to infer it from
    /// an achieved pitch.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Prefilter => "prefilter",
            Self::Production => "production",
        }
    }
}

/// What one candidate's real sweep measured, beyond the one number
/// (`support_area_cm2`) that feeds the objective — kept for the run report so
/// a reader can see the measurement was real rather than assumed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasuredSupportDemand {
    /// The BURDEN that replaces the projected-area proxy in the objective's
    /// support term, at the SAME registered weight
    /// (`ORIENT_WEIGHT_SUPPORT_AREA_CM2`, unchanged in value).
    ///
    /// The implementation names the quantity and its unit bridge; see
    /// `plinth_sweep::SweepDemand`. It is an area in cm² so the weight it
    /// multiplies keeps the units the registry gave it.
    pub support_area_cm2: f64,
    /// The arm count this pose costs — the free variable the burden is built
    /// from, published raw beside it so the conversion can be checked.
    pub struts: usize,
    /// `Σ π r²` over the tips. Published because it is the quantity the
    /// burden was chosen OVER, and because the area law makes it very nearly
    /// a pose invariant (`SweepPlan::total_scar_mm2`'s own doc:
    /// `Σ π r² = injected_load / X_ALLOWED`, equal to `peak_layer_area /
    /// X_ALLOWED` only on flat-bottomed models, independent of how many arms
    /// share the load) — which is exactly why it cannot rank poses.
    pub total_scar_mm2: f64,
    pub achieved_pitch_mm: f64,
    pub layers: usize,
    pub residual_demand_cells: usize,
    pub uncarried_load_mm2: f64,
    /// How many of this pose's arms seat on the MODEL rather than on the
    /// plate or on another arm. Published raw beside the scar below so a
    /// zero scar can be read as "nothing landed on the model" rather than
    /// guessed at.
    pub model_landings: usize,
    /// `Σ` over those landings of `π r_seat² × fragility × (1 + D)` of the
    /// face each one seats on, in mm² — the scar this pose's SUPPORT FIELD
    /// leaves on the model, in the same units and under the same
    /// `ORIENT_SCAR_WEIGHT` as the objective's down-facing scar.
    ///
    /// **[`None`] is a stated absence, never a zero.** It is `None` for a
    /// [`DemandFidelity::Prefilter`] row by construction — the coarse raster
    /// exists to prune, and a landing measured on it is not the landing the
    /// emitted file will have — and `None` when the table this pose was
    /// scored against carries no fragility field to weight a seat by. Only a
    /// `Some(0.0)` asserts that a production sweep ran and nothing it placed
    /// touched the model.
    pub landing_scar_raw_mm2: Option<f64>,
    /// `Σ` over this pose's arms of `π r_tip² × fragility × (1 + D)` of the
    /// face each one's TIP carries — the mirror of
    /// [`Self::landing_scar_raw_mm2`], priced at the other end of the arm,
    /// and excluding a tip charged against a face the analytic down-facing
    /// fold already counts (see `plinth_sweep::tip_landing`'s module doc for
    /// the double-counting choice and the measurement behind it).
    ///
    /// **[`None`] is a stated absence, never a zero**, following the same
    /// rule as [`Self::landing_scar_raw_mm2`].
    pub tip_scar_raw_mm2: Option<f64>,
    /// Which raster this row was measured on. A row is only allowed to crown
    /// a pose when this is [`DemandFidelity::Production`].
    pub fidelity: DemandFidelity,
}

/// A pluggable measurer of one batch of candidates' real support burden.
///
/// Batch-first by construction: the caller hands over every pose it wants
/// measured at one fidelity in a single call, so an implementation is free to
/// share whatever it can across poses. A per-pose call in this position would
/// be a defect.
///
/// # Contract
///
/// The returned vector is index-parallel with `candidates` and the same
/// length. A caller that receives a shorter vector treats the missing entries
/// as failures and keeps the proxy for them, so a short return degrades the
/// same way an `Err` does rather than mis-attributing one pose's measurement
/// to another.
///
/// Any `Err(String)` is a reason the caller (the commit-tier override inside
/// `search::search`) FALLS BACK to the projected-area proxy for that one
/// candidate and records the failure by name — never a reason to refuse the
/// whole search (emit and warn, never refuse). The error is a plain `String`
/// rather than a typed enum specifically so this trait never has to name a
/// `plinth-sweep` error type, which would reintroduce the dependency this
/// trait exists to invert.
/// `table` is the scoring table for `mesh` at the tier that is measuring —
/// the commit tier's Exact table, whose face order IS `mesh.tris()`'s order.
/// It is handed over so an implementation that prices where the arms LAND can
/// read the fragility and display fields the objective was scored against
/// instead of measuring second copies of them. An implementation that does
/// not price landings ignores it.
pub trait SupportDemandMeasurer {
    fn measure_support_demand(
        &self,
        mesh: &Mesh,
        table: &OrientFaceTable,
        candidates: &[UpDirection],
        fidelity: DemandFidelity,
    ) -> Vec<Result<MeasuredSupportDemand, String>>;
}
