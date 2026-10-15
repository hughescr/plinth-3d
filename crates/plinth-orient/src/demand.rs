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
//! `ORIENT_COMMIT_TOP_K` promoted plus the escorted legacy block and flat-face
//! poses (up to ~16-18 poses), and a production-pitch sweep of all of them is not affordable on
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

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

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
    ///
    /// **`StrutKind::Arm` only.** A brace and a mast (the slenderness lane,
    /// 2026-09-05/06) touch no model face and leave no scar, so they are not
    /// part of what the owner pays in clips-to-remove; counting them here
    /// would over-state the burden by exactly the stiffening the lane added
    /// to make the arms themselves legal.
    pub struts: usize,
    /// Cross-braces this pose's sweep placed (`StrutKind::Brace`), published
    /// beside [`Self::struts`] so what the pose costs in STIFFENING is still
    /// visible without being allowed to decide the pose. Never folded into
    /// [`Self::struts`] or [`Self::support_area_cm2`].
    pub braces: usize,
    /// Plate-rooted masts this pose's sweep placed (`StrutKind::Mast`), the
    /// mirror of [`Self::braces`] for the other slenderness remedy. Same
    /// rule: published, never counted into the burden.
    pub masts: usize,
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
    /// `Σ` over those landings of `π r_seat² × (fragility + D)` of the
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
    /// `Σ` over this pose's arms of `π r_tip² × (fragility + D)` of the
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

    /// A SEPARATE measurer for the diagnostics (`PLINTH_ORIENT_DIAG=prod-all`
    /// or `extra=`): it measures exactly as `self` does but keeps nothing.
    ///
    /// A retaining measurer replaces its retained production batch on every
    /// production call, so a diagnostic batch run through it would evict the
    /// winner's plan. The diagnostics therefore never use `self`; they use the
    /// measurer this returns, and what it measures never enters the ranking.
    /// `None` (the default) means there is none, and the diagnostics then
    /// skip the extra production measurements and say so.
    fn diagnostic_measurer(&self) -> Option<Box<dyn SupportDemandMeasurer>> {
        None
    }

    /// The finalists' production batch, run as a [`ProductionRace`]: the same
    /// measurements as `measure_support_demand(.., Production)`, except that
    /// an implementation MAY stop a pose whose [`ProductionRace::is_stopped`]
    /// flag is set, and MUST call [`ProductionRace::finished`] with each
    /// pose's measurement the moment it has one.
    ///
    /// The default measures every pose and never stops one, which is what a
    /// measurer that knows nothing of the race should do. The vector is
    /// index-parallel with `candidates`. A pose reported
    /// [`RaceOutcome::Stopped`] must have had its flag set first; the search
    /// refuses to read a stop it did not certify.
    fn measure_production_race(
        &self,
        mesh: &Mesh,
        table: &OrientFaceTable,
        candidates: &[UpDirection],
        _race: &ProductionRace,
    ) -> Vec<RaceOutcome> {
        self.measure_support_demand(mesh, table, candidates, DemandFidelity::Production)
            .into_iter()
            .map(|result| match result {
                Ok(measured) => RaceOutcome::Measured(measured),
                Err(error) => RaceOutcome::Failed(error),
            })
            .collect()
    }
}

/// What one finalist's production measurement came to in a [`ProductionRace`].
#[derive(Clone, Debug, PartialEq)]
pub enum RaceOutcome {
    /// Measured to the end.
    Measured(MeasuredSupportDemand),
    /// The measurement failed; the candidate keeps its prefilter price, as
    /// in a batch without a race.
    Failed(String),
    /// Withdrawn at a safe point because the race certified it a loser. It
    /// has NO production score; the search records it as stopped, with the
    /// lower bound that certified it.
    Stopped,
}

/// How a [`ProductionRace`] prices a finished pose: the search's own scoring
/// call on the pose's measurement (pose index, measurement), or `None` when it
/// cannot be priced.
pub type RacePrice = Box<dyn Fn(usize, &MeasuredSupportDemand) -> Option<f64> + Send + Sync>;

/// The certified early stop of the production batch (perf lane B1,
/// 2026-10-14).
///
/// # Why a pose can be stopped, and only then
///
/// A finalist's final score is (`search::TermsRecord::repriced`)
///
/// ```text
/// ((w_a·A + peel) + w_h·h) + scar_term(raw + l + t)
/// ```
///
/// `peel`, `h` and `raw` are fixed before any production sweep runs (the
/// exact table and the peel corner). `A`, `l` and `t` are the production
/// sweep's measurements: the burden `A = arms·π·r²/100`, the model-landing
/// scar `l = Σ A_contact·(fragility + display)` and the tip-contact scar
/// `t = Σ π r_tip²·(fragility + display)`. Each is a sum of products of
/// areas and of field values that the search checks are `>= 0`
/// ([`crate::search::TermsRecord::production_lower_bound`] returns `None`
/// otherwise), and each weight multiplying them is checked `>= 0` the same
/// way. IEEE-754 addition of a non-negative term never lowers a sum, and
/// multiplying or dividing by a non-negative or positive number is monotone,
/// so the SAME arithmetic with `A = l = t = 0` is `<=` the full score, bit
/// for bit: that is the candidate's **lower bound**, and the search computes
/// it by calling the one scoring function with the measured terms zeroed, not
/// by a second formula.
///
/// So when some finished pose's score is STRICTLY below a still-running
/// pose's lower bound, the running pose scores strictly more than a finished
/// one. It cannot win, and it cannot tie for the win, whatever the sweep
/// would have measured. A pose whose bound merely EQUALS the best finished
/// score is not stopped: it could tie, and ties go to the lower candidate id.
///
/// # Determinism
///
/// The race decides only WHEN a certified loser is stopped. The winner is the
/// minimum over the poses that finish, and the minimum is never stopped
/// (its bound is `<=` its score `<=` every other finished score, so its bound
/// is never strictly above one). Which losers had stopped by the time the
/// batch returned depends on timing, so the search does not record it: after
/// the batch it recomputes the certified set from the final winner's score
/// alone and records every candidate above it as a certified loser, whether
/// that candidate stopped, finished or failed. The record and the winner are
/// a function of the inputs, not of the thread count.
pub struct ProductionRace {
    // See [`RacePrice`] for `price`.
    /// Index-parallel with the batch. `None`: this pose can never be stopped.
    bounds: Vec<Option<f64>>,
    /// The finished pose's score, by the search's own scoring call; `None`
    /// when the measurement cannot be priced (it is then no leader).
    price: RacePrice,
    tokens: Vec<Arc<AtomicBool>>,
    /// Lowest finished score so far.
    leader: Mutex<Option<f64>>,
    /// Set once a finished pose broke the non-negativity the bound rests on:
    /// no stop is requested after that.
    poisoned: AtomicBool,
    started: Instant,
    /// Milliseconds after `started` at which each pose's stop was requested;
    /// [`u64::MAX`] for never. Disclosure only.
    requested_ms: Vec<AtomicU64>,
    stopped_ms: Vec<AtomicU64>,
}

impl ProductionRace {
    /// A race over `bounds.len()` poses.
    #[must_use]
    pub fn new(bounds: Vec<Option<f64>>, price: RacePrice) -> Self {
        let count = bounds.len();
        Self {
            bounds,
            price,
            tokens: (0..count).map(|_| Arc::new(AtomicBool::new(false))).collect(),
            leader: Mutex::new(None),
            poisoned: AtomicBool::new(false),
            started: Instant::now(),
            requested_ms: (0..count).map(|_| AtomicU64::new(u64::MAX)).collect(),
            stopped_ms: (0..count).map(|_| AtomicU64::new(u64::MAX)).collect(),
        }
    }

    /// A race in which nothing can be stopped.
    #[must_use]
    pub fn never_stops(count: usize) -> Self {
        Self::new(vec![None; count], Box::new(|_, _| None))
    }

    /// The cancel flag of pose `index`; the measurer installs it for the
    /// duration of that pose's sweep.
    #[must_use]
    pub fn token(&self, index: usize) -> Option<&Arc<AtomicBool>> {
        self.tokens.get(index)
    }

    /// `true` once pose `index` has been certified a loser and told to stop.
    #[must_use]
    pub fn is_stopped(&self, index: usize) -> bool {
        self.tokens
            .get(index)
            .is_some_and(|flag| flag.load(Ordering::Acquire))
    }

    /// Pose `index` returned [`RaceOutcome::Stopped`] now (disclosure).
    pub fn note_stopped(&self, index: usize) {
        if let Some(slot) = self.stopped_ms.get(index) {
            slot.store(self.elapsed_ms(), Ordering::Relaxed);
        }
    }

    fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX - 1)
    }

    /// Pose `index` finished with `measured`. Updates the leader and stops
    /// every pose whose lower bound is strictly above it.
    pub fn finished(&self, index: usize, measured: &MeasuredSupportDemand) {
        let usable = [
            Some(measured.support_area_cm2),
            measured.landing_scar_raw_mm2,
            measured.tip_scar_raw_mm2,
        ]
        .into_iter()
        .flatten()
        .all(|value| value.is_finite() && value >= 0.0);
        if !usable {
            // The bound assumes these are non-negative and finite. One that
            // is not voids the certificate for every pose: stop no more.
            self.poisoned.store(true, Ordering::Release);
            return;
        }
        let Some(score) = (self.price)(index, measured).filter(|score| score.is_finite()) else {
            return;
        };
        let leader = {
            let mut leader = self.leader.lock().unwrap_or_else(PoisonError::into_inner);
            let best = leader.map_or(score, |best| best.min(score));
            *leader = Some(best);
            best
        };
        if self.poisoned.load(Ordering::Acquire) {
            return;
        }
        for (other, bound) in self.bounds.iter().enumerate() {
            if other != index && bound.is_some_and(|bound| bound > leader) {
                let flag = &self.tokens[other];
                if !flag.swap(true, Ordering::AcqRel) {
                    self.requested_ms[other].store(self.elapsed_ms(), Ordering::Relaxed);
                }
            }
        }
    }

    /// Whether a finished pose broke the non-negativity the bound rests on.
    #[must_use]
    pub fn poisoned(&self) -> bool {
        self.poisoned.load(Ordering::Acquire)
    }

    /// Disclosure: `(pose index, seconds after the batch began when its stop
    /// was requested, seconds when it actually returned)`, for every pose a
    /// stop was requested for. The second is `None` for a pose that finished
    /// or failed before it saw the flag.
    #[must_use]
    pub fn stop_log(&self) -> Vec<(usize, f64, Option<f64>)> {
        let seconds = |slot: &AtomicU64| -> Option<f64> {
            let ms = slot.load(Ordering::Relaxed);
            (ms != u64::MAX).then(|| ms as f64 / 1000.0)
        };
        self.requested_ms
            .iter()
            .enumerate()
            .filter_map(|(index, requested)| {
                seconds(requested).map(|at| (index, at, seconds(&self.stopped_ms[index])))
            })
            .collect()
    }
}
