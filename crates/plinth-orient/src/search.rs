//! The dense sweep itself: the recorded three-tier funnel, the bit-stable
//! argmin, and the provenance record that makes a searched winner pinnable.
//!
//! [`grid`](crate::grid) says which directions exist and in what order;
//! [`score`](crate::score) evaluates the objective for a whole candidate list at
//! once.  This module is what runs between them — the LOD ladder, the
//! promotion, the comparator, and the record.
//!
//! # The funnel is the recorded one (D-12, `docs/engineering-journal/03-physics-and-constants.md` §3.8)
//!
//! | Tier | Candidates | LOD |
//! |---|---|---|
//! | coarse | the whole grid (≤ 653) | L2, ~2 % of Exact |
//! | refine | top `ORIENT_REFINE_TOP_K` | L1, ~10 % of Exact |
//! | commit | top `ORIENT_COMMIT_TOP_K`, **plus the legacy 13 and the flat-face poses** | **Exact** |
//!
//! The escort in the commit row is the one place this deviates from the
//! recorded funnel, and it is there because the recorded funnel measurably did
//! not deliver what D-12 claims for it — see [`escort`], which carries the
//! measurements (the legacy 13 on `Sphere`, 2026-08; the flat-face poses on
//! `Round 50/100`, 2026-09-28). Neither recorded width moved.
//!
//! "Exact" is the protected, decimated **production mesh** this stage is handed,
//! not the original high-detail input (`docs/engineering-journal/04-migration-plan.md`
//! §"Rust deliverables", the Coarse/Refine/Exact tier table) — orientation runs
//! after decimation by construction, so there is no higher-detail mesh in scope
//! at all.  Commitment happens only at the exact tier of that same table: a
//! proxy can mis-rank, never mis-report, because the winner's published score
//! is always recomputed on the production mesh.
//!
//! # Search proxies can never become production geometry
//!
//! [`SearchProxy`] wraps a mesh with no conversion into `DecimatedModel`,
//! `OrientedModel` or anything the emit path can reach: L1/L2 proxies never
//! become alternate downstream production meshes; here that is a property of
//! what compiles, not a rule someone has to remember.  The proxies
//! deliberately bypass the production decimation gates — error budget, topology
//! gate, downstream-utility witness — because those gates protect *shipped*
//! geometry and these meshes are never shipped.
//!
//! # Fallback is loud
//!
//! The retired heuristic fell back to the full mesh silently when its proxy
//! failed.  Here a tier whose proxy has fewer than `ORIENT_PROXY_MIN_FACES`
//! faces, has any boundary edge (which breaks the exact section solver's closed
//! shoelace identity), or that the simplifier refuses outright, falls back to
//! the next finer LOD and is recorded as a typed [`ProxyFallback`] naming the
//! tier, the reason and the LOD actually used.
//!
//! # The argmin is bit-stable and has exactly one key
//!
//! Candidates are scanned by id ascending with strict `<`, so the **first** id
//! attaining the minimum wins.  That is not aesthetics: identity is
//! `CandidateId` 0, and the legacy writer's `np.eye(4)` seed made identity win
//! every tie it took part in — two committed baselines (`Cylinder`, `make_box`)
//! record exactly that.  There is **no epsilon anywhere**: an `|a−b| < ε`
//! comparator is not transitive and would re-introduce the order dependence this
//! is built to remove.  A non-finite term is a typed refusal carried in the
//! record and excluded from ranking — never a sentinel score and never a silent
//! skip.

use std::collections::BTreeSet;

use geom_kernel::vec3;
use plinth_core::{m0_constants, Fidelity, Finite};
use plinth_decimate::{simplify_locked, SimplifyError, SimplifyRequest};
use plinth_mesh::Mesh;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::grid::{Candidate, CandidateBlock, GridSettings, RestOrder, SweepGrid};
use crate::score::{
    score_candidates, CandidateOutcome, CandidateRefusalReason, CandidateTerms, ObjectiveWeights,
    OrientFaceTable, OrientSearchLimits, PeelForceCorners, ScoreError,
    ORIENT_PEEL_AREA_EXPONENT_HIGH_ID, ORIENT_PEEL_AREA_EXPONENT_LOW_ID,
    ORIENT_PEEL_POINTS_PER_NEWTON_ID, ORIENT_PEEL_REFERENCE_AREA_MM2_ID,
    ORIENT_PEEL_REFERENCE_PRESSURE_PA_HIGH_ID, ORIENT_PEEL_REFERENCE_PRESSURE_PA_LOW_ID,
    ORIENT_SCAR_WEIGHT_ID, ORIENT_WEIGHT_HEIGHT_MM_ID, ORIENT_WEIGHT_PEAK_XSEC_CM2_ID,
    ORIENT_WEIGHT_SUPPORT_AREA_CM2_ID, OVERHANG_ANGLE_DEG_ID, UpDirection,
};
use crate::BaselineSelection;

/// Registered `ConstantId`s of the ladder.
pub const ORIENT_PROXY_L1_RATIO_ID: &str = "ORIENT_PROXY_L1_RATIO";
pub const ORIENT_PROXY_L2_OF_L1_RATIO_ID: &str = "ORIENT_PROXY_L2_OF_L1_RATIO";
pub const ORIENT_PROXY_MIN_FACES_ID: &str = "ORIENT_PROXY_MIN_FACES";
pub const ORIENT_REFINE_TOP_K_ID: &str = "ORIENT_REFINE_TOP_K";
pub const ORIENT_COMMIT_TOP_K_ID: &str = "ORIENT_COMMIT_TOP_K";

/// The LOD names that appear in every record. The exact tier's name is distinct
/// from the pipeline's own `"prepared"` because a delicacy field is bound to a
/// `(mesh, LOD)` pair and two fields over the same mesh at two names are two
/// distinguishable fields — which is the point of naming them at all.
pub const EXACT_LOD: &str = "orient-exact";
pub const L1_LOD: &str = "orient-l1";
pub const L2_LOD: &str = "orient-l2";

/// A mesh built only to rank candidates faster.
///
/// There is deliberately no `From<SearchProxy>` for any production model type
/// and no accessor that yields an owned `Mesh`: a search proxy cannot reach the
/// emit path in code that compiles (`docs/engineering-journal/04-migration-plan.md`
/// §"Rust deliverables", the Coarse/Refine/Exact tier table).
#[derive(Clone, Debug)]
pub struct SearchProxy {
    mesh: Mesh,
    requested_ratio: f64,
    achieved_ratio: f64,
}

impl SearchProxy {
    fn simplify(mesh: &Mesh, ratio: f64) -> Result<Self, SimplifyError> {
        let outcome = simplify_locked(
            mesh,
            &SimplifyRequest {
                target_ratio: ratio,
                vertex_locks: vec![false; mesh.verts().len()],
            },
        )?;
        Ok(Self {
            mesh: outcome.mesh,
            requested_ratio: outcome.requested_ratio,
            achieved_ratio: outcome.achieved_ratio,
        })
    }

    fn mesh(&self) -> &Mesh {
        &self.mesh
    }
}

/// The registered ladder: two proxy ratios, the integrity floor, and the two
/// funnel widths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LadderSettings {
    l1_ratio_bits: u64,
    l2_of_l1_ratio_bits: u64,
    min_faces: usize,
    refine_top_k: usize,
    commit_top_k: usize,
}

impl LadderSettings {
    /// Reads all five numbers from the one M0 registry.
    ///
    /// # Errors
    ///
    /// [`ScoreError::Constants`] when the registry cannot be constructed or a
    /// count constant is not a whole positive number.
    pub fn registered() -> Result<Self, ScoreError> {
        let values = m0_constants()
            .map_err(|error| ScoreError::Constants(error.to_string()))?
            .placeholders();
        Ok(Self {
            l1_ratio_bits: values.orient_proxy_l1_ratio().diagnostic().get().to_bits(),
            l2_of_l1_ratio_bits: values
                .orient_proxy_l2_of_l1_ratio()
                .diagnostic()
                .get()
                .to_bits(),
            min_faces: whole(
                values.orient_proxy_min_faces().diagnostic().get(),
                ORIENT_PROXY_MIN_FACES_ID,
            )?,
            refine_top_k: whole(
                values.orient_refine_top_k().diagnostic().get(),
                ORIENT_REFINE_TOP_K_ID,
            )?,
            commit_top_k: whole(
                values.orient_commit_top_k().diagnostic().get(),
                ORIENT_COMMIT_TOP_K_ID,
            )?,
        })
    }

    #[must_use]
    pub fn l1_ratio(self) -> f64 {
        f64::from_bits(self.l1_ratio_bits)
    }
    #[must_use]
    pub fn l2_of_l1_ratio(self) -> f64 {
        f64::from_bits(self.l2_of_l1_ratio_bits)
    }
    #[must_use]
    pub const fn min_faces(self) -> usize {
        self.min_faces
    }
    #[must_use]
    pub const fn refine_top_k(self) -> usize {
        self.refine_top_k
    }
    #[must_use]
    pub const fn commit_top_k(self) -> usize {
        self.commit_top_k
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "float-to-integer casts on lattice or step arithmetic: Rust's `as` saturates at the integer \
          bounds and maps NaN to 0, and every site in this function is range-checked before the cast \
          or clamped into range after it (read site by site, 2026-08-26). See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn whole(value: f64, id: &'static str) -> Result<usize, ScoreError> {
    if !value.is_finite() || value < 1.0 || value.fract() != 0.0 || value > 1_048_576.0 {
        return Err(ScoreError::Constants(format!(
            "{id} = {value} is not a whole count"
        )));
    }
    Ok(value as usize)
}

/// Everything the sweep reads that is not the mesh.
///
/// The three settings fields are registered constants; `limits` is the
/// caller's resource policy, which is not a registry member because nothing in
/// it can change a score. There is no operator input here and no CLI flag
/// behind any of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepRequest {
    pub weights: ObjectiveWeights,
    pub grid: GridSettings,
    pub ladder: LadderSettings,
    /// The work budget every tier is charged against, supplied by the caller —
    /// `plinth_pipeline::PrepareResourcePolicy::orient` in production. It is
    /// deliberately **not** derived from the mesh being scored: a cap computed
    /// from the counts it is then compared against can never fire.
    pub limits: OrientSearchLimits,
}

impl SweepRequest {
    /// The one production configuration: registered constants, and the
    /// production work budget.
    ///
    /// # Errors
    ///
    /// [`ScoreError::Constants`] when any registered number is unavailable or
    /// malformed.
    pub fn registered() -> Result<Self, ScoreError> {
        Ok(Self {
            weights: ObjectiveWeights::registered()?,
            grid: GridSettings::registered()?,
            ladder: LadderSettings::registered()?,
            limits: OrientSearchLimits::production(),
        })
    }

    /// `true` when the scar term is switched off at the weight, so the delicacy
    /// field is not merely multiplied by zero but never built.
    #[must_use]
    pub fn scar_free(&self) -> bool {
        self.weights.scar() == 0.0
    }
}

/// Which tier a number was measured at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchTier {
    Coarse,
    Refine,
    Commit,
}

impl SearchTier {
    /// The cost vocabulary's own fidelity word for this tier: a cost always
    /// states how much it lied.
    #[must_use]
    pub const fn fidelity(self) -> Fidelity {
        match self {
            Self::Coarse => Fidelity::Surrogate,
            Self::Refine => Fidelity::ProxyRefined,
            Self::Commit => Fidelity::Exact,
        }
    }
}

/// Why a tier could not use the proxy it planned to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProxyFallbackReason {
    /// The simplifier refused the request outright.
    SimplifyRefused,
    /// Below `ORIENT_PROXY_MIN_FACES`; too coarse to carry the objective.
    BelowFaceFloor { faces: usize, floor: usize },
    /// A hole. The exact section solver's shoelace identity assumes the section
    /// is a union of closed loops, so an open proxy would give a wrong area
    /// rather than a slower one.
    OpenBoundary { boundary_edges: usize },
    /// The proxy's own delicacy field could not be measured, so the scar term
    /// would have had to be faked as zeros.
    FieldUnavailable,
    /// The rung this one cascades from is absent, so there was nothing to
    /// coarsen.
    ///
    /// No proxy was built and none was measured. This variant exists precisely
    /// so the record does not have to describe a mesh that never existed in
    /// order to say why a tier is missing: it previously borrowed
    /// `BelowFaceFloor { faces: 0 }`, which published "0 faces is below the
    /// 256-face search-proxy floor" about a proxy that was never simplified and
    /// does not have zero faces. The same landing's own decision record forbids
    /// exactly that — there is no substitute value for a measurement that did
    /// not happen.
    NoCascadeSource,
}

/// A tier that did not get the LOD it planned, and what it used instead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyFallback {
    pub planned_lod: String,
    pub reason: ProxyFallbackReason,
    pub used_lod: String,
    pub detail: String,
}

/// One LOD's identity and shape, so every rank is auditable against the exact
/// geometry it was computed on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LodRecord {
    pub lod: String,
    pub mesh_fingerprint: String,
    pub faces: usize,
    pub verts: usize,
    /// Absent for the exact LOD, which is not a simplification of anything in
    /// scope.
    pub requested_ratio: Option<Finite>,
    pub achieved_ratio: Option<Finite>,
    pub boundary_edges: usize,
    pub nonmanifold_edges: usize,
    /// The digest of this LOD's delicacy field, or `null` when the scar weight
    /// is structurally zero and no field was built.
    ///
    /// `null` is what makes the §3.8 requirement structural rather than
    /// arithmetic: a scar-free run's `search_digest` cannot depend on delicacy
    /// bytes, because there are none in the record to depend on.
    pub delicacy_digest: Option<String>,
    /// **The display-face map for this LOD, summarised** (owner ruling,
    /// 2026-09-03: publish D so it can be inspected BEFORE it steers
    /// anything). `null` on a scar-free run, where no field is built at all,
    /// for the same structural reason `delicacy_digest` is `null` there.
    ///
    /// `#[serde(default)]` so a record written before this field still
    /// deserializes, and it deserializes to "the record does not hold this".
    #[serde(default)]
    pub display: Option<crate::DisplayFieldSummary>,
}

/// The peel-force bracket's four corners, as a record (D14).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeelForceCornersRecord {
    pub low_pressure_low_exponent_n: Finite,
    pub low_pressure_high_exponent_n: Finite,
    pub high_pressure_low_exponent_n: Finite,
    pub high_pressure_high_exponent_n: Finite,
}

impl PeelForceCornersRecord {
    fn new(corners: PeelForceCorners) -> Result<Self, SearchError> {
        Ok(Self {
            low_pressure_low_exponent_n: finite(corners.low_pressure_low_exponent_n)?,
            low_pressure_high_exponent_n: finite(corners.low_pressure_high_exponent_n)?,
            high_pressure_low_exponent_n: finite(corners.high_pressure_low_exponent_n)?,
            high_pressure_high_exponent_n: finite(corners.high_pressure_high_exponent_n)?,
        })
    }
}

/// The four terms and the score, as a record.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermsRecord {
    pub support_area_cm2: Finite,
    /// The exact peak cross-section, still computed and reported; no longer
    /// weighted directly into `score` (LANE 3, 2026-09-02). See `peel_term`.
    pub peak_xsec_cm2: Finite,
    pub peak_mm2: Finite,
    pub peel_force_n: PeelForceCornersRecord,
    pub peel_term: Finite,
    pub height_mm: Finite,
    pub scar_term: Finite,
    /// The WHOLE scar measurement: `fragility + display + landing + tip`,
    /// where the landing and tip halves are present only for a candidate a
    /// production sweep measured.
    pub scar_raw_mm2: Finite,
    /// The two halves the scorer measured off the model's own down-facing
    /// area, published beside the landing and tip halves so the identity
    /// `scar_raw = fragility + display + landing + tip` can be read off one
    /// record instead of re-derived from the mesh.
    pub fragility_raw_mm2: Finite,
    pub display_raw_mm2: Finite,
    /// `Σ_model-landings(π r_seat² × (fragility + D))`, or [`None`] when
    /// no production sweep measured this candidate — a stated absence, never
    /// a zero (`crate::demand::MeasuredSupportDemand`'s own rule).
    pub landing_scar_raw_mm2: Option<Finite>,
    /// `Σ_tips(π r_tip² × (fragility + D))` over tip charges the analytic
    /// down-facing fold does not already count
    /// (`plinth_sweep::tip_landing`'s own doc), or [`None`] under the same
    /// stated-absence rule as [`Self::landing_scar_raw_mm2`].
    pub tip_scar_raw_mm2: Option<Finite>,
    pub down_facing_faces: usize,
    pub score: Finite,
}

/// The scar term for a raw scar of `raw_mm2`: the one expression
/// [`TermsRecord::repriced`] and [`TermsRecord::production_lower_bound`] both
/// read, in the association `score::score_one` uses.
fn scar_term_of(weights: ObjectiveWeights, raw_mm2: f64) -> f64 {
    weights.scar() * raw_mm2 / 100.0
}

/// The commit-tier price of a candidate: the one expression
/// [`TermsRecord::repriced`] (the full score) and
/// [`TermsRecord::production_lower_bound`] (the bound) both evaluate, so the
/// bound is the full score's own arithmetic with the measured terms set to
/// zero and cannot drift from it.
fn price(
    weights: ObjectiveWeights,
    support_area_cm2: f64,
    peel_term: f64,
    height_mm: f64,
    scar_term: f64,
) -> f64 {
    ((weights.support_area_cm2() * support_area_cm2 + peel_term) + weights.height_mm() * height_mm)
        + scar_term
}

impl TermsRecord {
    /// A lower bound on this candidate's score after its production sweep
    /// measures its burden `A` and its two scars `l`, `t` — or [`None`] when
    /// the bound cannot be certified.
    ///
    /// # Why it is a lower bound, in the arithmetic of the score itself
    ///
    /// [`Self::repriced`] prices a finalist as
    /// `price(w, A, peel_term, h, scar_term)` with
    /// `scar_term = scar_term_of(w, scar_raw + (l + t))`, or keeps the
    /// scorer's `scar_term` when neither scar was measured. The fixed parts
    /// `peel_term` and `h` are known before the sweep; so is `scar_raw`.
    /// Only `A`, `l` and `t` are measured, and each is a sum of products of
    /// areas (`π r²`, a foot's contact area) and of field values, so `>= 0`;
    /// `plinth_orient::demand::ProductionRace::finished` voids the certificate
    /// if a finished pose ever reports otherwise. With the weights `w_a` and
    /// `w_scar` non-negative and finite (checked here):
    ///
    /// * `w_a · A >= w_a · 0 = 0` and addition of a non-negative number never
    ///   lowers an IEEE-754 sum, so `price(.., A, ..) >= price(.., 0, ..)`;
    /// * `scar_raw + (l + t) >= scar_raw`, and `x ↦ w_scar · x / 100` is
    ///   monotone for `w_scar >= 0`, so the measured scar term is at least
    ///   `scar_term_of(w, scar_raw)`; when nothing was measured the term is
    ///   `self.scar_term` itself. The bound takes the smaller of the two, so
    ///   it holds on both branches even if the scorer's `scar_term` and
    ///   `scar_term_of(scar_raw)` ever differ in the last bit;
    /// * every fallback in [`Self::repriced`] (a non-finite measured value
    ///   keeps the earlier field) returns a field of `self`, and `self`'s
    ///   burden is checked `>= 0` here, so the fallback is at least the bound
    ///   as well.
    ///
    /// `peel_force_n` is the corner the caller prices at (the primary corner
    /// for the ranking, any one of the four for the robustness check).
    #[must_use]
    pub(crate) fn production_lower_bound(
        &self,
        weights: ObjectiveWeights,
        peel_force_n: f64,
    ) -> Option<f64> {
        let w_a = weights.support_area_cm2();
        let w_scar = weights.scar();
        if !(w_a.is_finite() && w_a >= 0.0 && w_scar.is_finite() && w_scar >= 0.0) {
            return None;
        }
        // `partial_cmp`, not `>= 0.0`: a NaN burden is not `>= 0`.
        if self.support_area_cm2.get().partial_cmp(&0.0).is_none_or(std::cmp::Ordering::is_lt) {
            return None;
        }
        let peel_term = weights.peel_points_per_newton() * peel_force_n;
        let scar_floor = self
            .scar_term
            .get()
            .min(scar_term_of(weights, self.scar_raw_mm2.get()));
        let bound = price(weights, 0.0, peel_term, self.height_mm.get(), scar_floor);
        bound.is_finite().then_some(bound)
    }

    pub(crate) fn new(terms: CandidateTerms) -> Result<Self, SearchError> {
        Ok(Self {
            support_area_cm2: finite(terms.support_area_cm2)?,
            peak_xsec_cm2: finite(terms.peak_xsec_cm2)?,
            peak_mm2: finite(terms.peak_mm2)?,
            peel_force_n: PeelForceCornersRecord::new(terms.peel_force_n)?,
            peel_term: finite(terms.peel_term)?,
            height_mm: finite(terms.height_mm)?,
            scar_term: finite(terms.scar_term)?,
            scar_raw_mm2: finite(terms.scar_raw_mm2)?,
            fragility_raw_mm2: finite(terms.fragility_raw_mm2)?,
            display_raw_mm2: finite(terms.display_raw_mm2)?,
            landing_scar_raw_mm2: terms.landing_scar_raw_mm2.map(finite).transpose()?,
            tip_scar_raw_mm2: terms.tip_scar_raw_mm2.map(finite).transpose()?,
            down_facing_faces: terms.down_facing_faces,
            score: finite(terms.score)?,
        })
    }

    /// Re-scores this candidate's already-measured terms at one bracket
    /// corner (or at a different `support_area_cm2` demand), holding every
    /// other weight at its registered value. Used by the commit-tier
    /// real-sweep-demand override and by the four-corner robustness check —
    /// both re-price an already-measured candidate rather than re-running the
    /// batched scorer, because neither the peel corner nor the demand
    /// substitution changes `peak_mm2` or `height_mm`.
    ///
    /// `landing_scar_raw_mm2` and `tip_scar_raw_mm2` are the two things here
    /// that can move the scar term. Either or both `None` leaves the
    /// corresponding half of `scar_term`/`scar_raw_mm2` exactly as it was,
    /// which is what the prefilter stage and the robustness check both pass
    /// (both `None`). `Some` folds a production sweep's measured scar half
    /// into the scar term at the SAME registered `ORIENT_SCAR_WEIGHT` and
    /// the same association `score::score_one` uses (`weights.scar * raw /
    /// 100`), so `l = 0.0` (or `t = 0.0`) reproduces the pre-fold score bit
    /// for bit rather than approximately — `raw + 0.0 == raw` in IEEE, and
    /// the quotient that follows is the identical expression on the
    /// identical operand.
    ///
    /// Each half may be folded in **once**: `self.landing_scar_raw_mm2` is
    /// `None` on every path that reaches a `Some` for it here (and likewise
    /// for `self.tip_scar_raw_mm2`), because only the commit tier's
    /// production stage passes either and it visits each finalist once.
    pub(crate) fn repriced(
        self,
        weights: ObjectiveWeights,
        support_area_cm2: f64,
        peel_force_n: f64,
        landing_scar_raw_mm2: Option<f64>,
        tip_scar_raw_mm2: Option<f64>,
    ) -> Self {
        debug_assert!(
            landing_scar_raw_mm2.is_none() || self.landing_scar_raw_mm2.is_none(),
            "a model-landing scar would be folded into this candidate twice"
        );
        debug_assert!(
            tip_scar_raw_mm2.is_none() || self.tip_scar_raw_mm2.is_none(),
            "a tip-contact scar would be folded into this candidate twice"
        );
        let peel_term = weights.peel_points_per_newton() * peel_force_n;
        let landing = landing_scar_raw_mm2.and_then(|landing| Finite::new(landing).ok());
        let tip = tip_scar_raw_mm2.and_then(|tip| Finite::new(tip).ok());
        let (scar_raw_mm2, scar_term, landing, tip) = if landing.is_some() || tip.is_some() {
            let added = landing.map_or(0.0, Finite::get) + tip.map_or(0.0, Finite::get);
            let raw = self.scar_raw_mm2.get() + added;
            let term = scar_term_of(weights, raw);
            (
                Finite::new(raw).unwrap_or(self.scar_raw_mm2),
                Finite::new(term).unwrap_or(self.scar_term),
                landing.or(self.landing_scar_raw_mm2),
                tip.or(self.tip_scar_raw_mm2),
            )
        } else {
            // Neither half was measured, or what was measured is not a
            // finite number: keep the scar term the scorer produced, which
            // is emit-and-warn rather than a refusal.
            (self.scar_raw_mm2, self.scar_term, self.landing_scar_raw_mm2, self.tip_scar_raw_mm2)
        };
        let score = price(
            weights,
            support_area_cm2,
            peel_term,
            self.height_mm.get(),
            scar_term.get(),
        );
        Self {
            support_area_cm2: Finite::new(support_area_cm2).unwrap_or(self.support_area_cm2),
            peel_term: Finite::new(peel_term).unwrap_or(self.peel_term),
            scar_term,
            scar_raw_mm2,
            landing_scar_raw_mm2: landing,
            tip_scar_raw_mm2: tip,
            score: Finite::new(score).unwrap_or(self.score),
            ..self
        }
    }
}

/// One ranked candidate at one tier.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RankedCandidate {
    pub rank: usize,
    pub candidate: usize,
    pub block: CandidateBlock,
    /// The build-up direction `u` this candidate IS, in model frame. Carried
    /// so the ranking reads on its own: before 2026-09-28 a reader (the
    /// Round 50 diagnosis probe among them) had to rebuild the grid off a
    /// re-simplified proxy just to learn which pose a row was, which is a
    /// second implementation of the funnel standing between the record and
    /// its reader.
    pub up_model_frame: [Finite; 3],
    /// For a [`CertifiedLoser`], `terms` is the price the candidate had BEFORE
    /// its production sweep (prefilter fidelity, `score` included): its
    /// production score is unknown and is not recorded. Every reader of
    /// `terms.score` on a commit-tier row checks `certified_loser` first.
    pub terms: TermsRecord,
    /// `Some` when the production batch certified this candidate a loser and
    /// its production score is therefore UNKNOWN. Absent from the record of
    /// every search that stopped nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certified_loser: Option<CertifiedLoser>,
}

/// A finalist the production batch certified a loser (perf lane B1).
///
/// Its production sweep was stopped, or its result was set aside, because its
/// score could not be as low as the winner's: the lower bound
/// ([`TermsRecord::production_lower_bound`]) is strictly above the winner's
/// score. The score itself is UNKNOWN; this record states that and nothing
/// more. The set of certified losers is a function of the inputs alone (every
/// finalist whose bound is strictly above the winner's score), never of which
/// sweep a thread had already abandoned.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CertifiedLoser {
    /// Strictly above the winner's score; the candidate's score is at least
    /// this.
    pub lower_bound_score: Finite,
}

///A candidate excluded from ranking, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefusalRecord {
    pub tier: SearchTier,
    pub candidate: usize,
    pub reason: String,
}

/// One tier of the funnel, as executed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierRecord {
    pub tier: SearchTier,
    pub fidelity: Fidelity,
    pub lod: String,
    pub scored: usize,
    pub ranked: usize,
    /// How many of this tier's ranking were promoted to the next tier by score.
    pub promoted: usize,
    /// How many candidates reached this tier by right rather than by promotion
    /// — the legacy block and the flat-face poses (see `escort`). Zero at
    /// every tier but the commit tier.
    pub escorted: usize,
    /// The full ranking at this tier — the recorded eyeball artifact.
    pub ranking: Vec<RankedCandidate>,
}

/// The grid as searched.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GridRecord {
    pub legacy: usize,
    pub rest_poses: usize,
    pub fibonacci: usize,
    /// Rest poses that qualified as flat faces (`grid` module doc).
    pub flat_faces: usize,
    /// Candidates that rest a flat face on the plate — the set escorted to
    /// the commit tier by right beside the legacy 13 (a flat face resting at
    /// identity is covered by legacy candidate 0 and counted here too).
    pub flat_face_poses: usize,
    pub duplicates_dropped: usize,
    pub candidates: usize,
    pub fibonacci_count: usize,
    pub rest_pose_max: usize,
    pub rest_pose_reference_bins: usize,
    pub dedupe_tolerance_deg: Finite,
    pub direction_digest: String,
    /// Always zero. The search space is S², so azimuth is not searched; the
    /// field exists so `plinth-plate` can fill it later with no schema change.
    pub z_spin_deg: Finite,
}

/// One weight, with the registered id it came from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeightRecord {
    pub constant_id: String,
    pub value: Finite,
}

/// The objective as priced, stamped with the ids rather than restating values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectiveRecord {
    pub support_area_cm2: WeightRecord,
    pub height_mm: WeightRecord,
    pub scar: WeightRecord,
    pub overhang_angle_deg: WeightRecord,
    /// Prices the peel-force term (LANE 3, 2026-09-02); retires
    /// `ORIENT_WEIGHT_PEAK_XSEC_CM2`'s role, which stays registered below,
    /// unread by the objective.
    pub peel_points_per_newton: WeightRecord,
    /// Registered but no longer read by `score`; kept here so a reader can
    /// see the retirement rather than infer it from an absent field.
    pub peak_xsec_cm2_retired: WeightRecord,
    pub peel_reference_area_mm2: WeightRecord,
    pub peel_pressure_pa_low: WeightRecord,
    pub peel_pressure_pa_high: WeightRecord,
    pub peel_area_exponent_low: WeightRecord,
    pub peel_area_exponent_high: WeightRecord,
    pub down_limit: Finite,
    /// The exact supremum, by name — the retired 4 mm/0.1 mm sampler is not
    /// reachable from this path.
    pub peak_section_method: String,
}

/// One commit-tier candidate's real sweep measurement, beside the proxy it
/// replaced (LANE 3, §12). `measured_support_area_cm2` is `None`, and
/// `measurement_error` names why, when the sweep itself could not be run for
/// this candidate — the proxy value is then what actually scored it, never a
/// refusal (emit and warn, never refuse).
///
/// Two rasters can produce this row, and `fidelity` says which one did.
/// Every admitted candidate is measured at
/// [`crate::demand::DemandFidelity::Prefilter`]; only the top
/// `ORIENT_COMMIT_TOP_K` of those are re-measured at
/// [`crate::demand::DemandFidelity::Production`], and for those the
/// prefilter's own numbers are kept in `prefilter_*` so the two rasters can
/// be compared in the record rather than in someone's memory.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitDemandRecord {
    pub candidate: usize,
    pub proxy_support_area_cm2: Finite,
    /// The burden that actually priced this candidate's support term.
    pub measured_support_area_cm2: Option<Finite>,
    pub struts: Option<usize>,
    pub total_scar_mm2: Option<Finite>,
    pub achieved_pitch_mm: Option<Finite>,
    pub residual_demand_cells: Option<usize>,
    pub uncarried_load_mm2: Option<Finite>,
    /// How many of this pose's arms seated on the model, and the scar they
    /// leave there. The count is published beside the scar so a `0.0` scar is
    /// read as "nothing landed" rather than inferred. The scar is `None`
    /// whenever the row's `fidelity` is `"prefilter"`: only a production
    /// sweep's landings are the landings the emitted file will have.
    pub model_landings: Option<usize>,
    pub landing_scar_raw_mm2: Option<Finite>,
    /// `Σ` over this pose's arms of the tip-contact scar their tips carry,
    /// mirroring `model_landings`/`landing_scar_raw_mm2` at the other end of
    /// the arm. `None` under the same prefilter rule.
    pub tip_scar_raw_mm2: Option<Finite>,
    /// `"prefilter"` or `"production"` — which raster the numbers above were
    /// measured on. `None` only when no measurement succeeded at all.
    pub fidelity: Option<String>,
    /// What the prefilter raster said for this candidate, kept even after a
    /// production measurement replaced it.
    pub prefilter_support_area_cm2: Option<Finite>,
    pub prefilter_struts: Option<usize>,
    pub measurement_error: Option<String>,
    /// `Some` when this finalist was certified a loser: the row then keeps
    /// its prefilter numbers (its production sweep contributes nothing) and
    /// `fidelity` stays `"prefilter"`. Absent from every record that stopped
    /// nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certified_loser: Option<CertifiedLoser>,
}

impl CommitDemandRecord {
    /// A row for a candidate nothing has measured yet.
    fn unmeasured(candidate: usize, proxy_support_area_cm2: Finite) -> Self {
        Self {
            candidate,
            proxy_support_area_cm2,
            measured_support_area_cm2: None,
            struts: None,
            total_scar_mm2: None,
            achieved_pitch_mm: None,
            residual_demand_cells: None,
            uncarried_load_mm2: None,
            model_landings: None,
            landing_scar_raw_mm2: None,
            tip_scar_raw_mm2: None,
            fidelity: None,
            prefilter_support_area_cm2: None,
            prefilter_struts: None,
            measurement_error: None,
            certified_loser: None,
        }
    }

    /// `true` when this candidate carries a production-raster burden, which
    /// is the only fidelity allowed to crown a pose.
    pub(crate) fn measured_at_production(&self) -> bool {
        self.fidelity.as_deref() == Some(crate::demand::DemandFidelity::Production.label())
    }
}

/// What a row records when the measurer returned fewer results than it was
/// handed candidates. The batch contract says the vector is index-parallel,
/// so a short return is treated as a per-candidate failure rather than
/// silently attributing one pose's sweep to another.
const SHORT_DEMAND_BATCH: &str =
    "the demand measurer returned fewer measurements than candidates; this pose kept its proxy";

/// What a row records when a pose came back stopped without a certificate.
const UNCERTIFIED_STOP: &str =
    "the production sweep of this pose was stopped without a certified lower bound; this pose kept its proxy";

/// `true` when every delicacy and display value the production scars are
/// priced by is finite and non-negative: the premise, with the registered
/// weights, of [`TermsRecord::production_lower_bound`]. An empty field is a
/// stated absence and reads as zero.
fn fields_non_negative(table: &OrientFaceTable) -> bool {
    let ok = |values: &[f64]| values.iter().all(|value| value.is_finite() && *value >= 0.0);
    ok(table.delicacy()) && ok(table.display())
}

/// Which finalists the production batch certifies as losers.
#[derive(Debug, PartialEq)]
pub(crate) struct LoserVerdict {
    /// Index (into the finalists) of the lowest finished score, ties to the
    /// lower candidate id; `None` when no finalist finished.
    pub(crate) winner: Option<usize>,
    /// `Some(lower bound)` for each finalist certified a loser.
    pub(crate) certified: Vec<Option<f64>>,
    /// Finalists that finished with a score BELOW their own bound: the bound's
    /// premise failed for them, so they are not certified.
    pub(crate) violations: Vec<usize>,
}

/// The certified set, from the finished scores alone.
///
/// The winner is the lowest finished score (ties to the lower candidate id,
/// the full ranking's own tie rule). A finalist is a certified loser exactly
/// when it is not the winner and its lower bound is STRICTLY above the
/// winner's score: its own score is then at least the bound, so it is
/// strictly worse than the winner, with no tie to break. A bound that merely
/// equals the winner's score certifies nothing. The set depends on the
/// winner's score and the bounds only, so it is the same whichever sweeps a
/// thread had already abandoned, and whether a candidate finished, failed or
/// stopped does not change whether it is in it.
pub(crate) fn certify_losers(
    candidates: &[usize],
    bounds: &[Option<f64>],
    finished: &[Option<f64>],
) -> LoserVerdict {
    let winner = finished
        .iter()
        .enumerate()
        .filter_map(|(index, score)| score.map(|score| (index, score)))
        .min_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then(candidates[left.0].cmp(&candidates[right.0]))
        });
    let mut certified = vec![None; candidates.len()];
    let mut violations = Vec::new();
    if let Some((winner_index, winner_score)) = winner {
        for (index, bound) in bounds.iter().enumerate().take(candidates.len()) {
            let Some(bound) = *bound else { continue };
            if index == winner_index || bound.partial_cmp(&winner_score) != Some(std::cmp::Ordering::Greater) {
                continue;
            }
            if finished.get(index).copied().flatten().is_some_and(|score| score < bound) {
                violations.push(index);
            } else {
                certified[index] = Some(bound);
            }
        }
    }
    LoserVerdict {
        winner: winner.map(|(index, _)| index),
        certified,
        violations,
    }
}

/// Disclosure of the stops, on stderr like the other `[orient]` lines. It
/// reads the clock and is in no record or digest.
fn report_certified_stops(
    candidates: &[usize],
    verdict: &LoserVerdict,
    race: &crate::demand::ProductionRace,
    produced: &[crate::demand::RaceOutcome],
) {
    let Some(winner) = verdict.winner else { return };
    for (index, bound) in verdict.certified.iter().enumerate() {
        let Some(bound) = bound else { continue };
        let how = match produced.get(index) {
            Some(crate::demand::RaceOutcome::Stopped) => "stopped before it finished",
            Some(crate::demand::RaceOutcome::Failed(_)) => "failed before it could be stopped",
            _ => "finished before the stop reached it; its score is set aside",
        };
        let at = race
            .stop_log()
            .into_iter()
            .find(|(stopped, _, _)| *stopped == index)
            .map_or_else(
                || "no stop was requested".to_owned(),
                |(_, requested, returned)| {
                    format!(
                        "stop requested at {requested:.1} s, returned at {}",
                        returned.map_or_else(|| "never".to_owned(), |s| format!("{s:.1} s"))
                    )
                },
            );
        eprintln!(
            "[orient] certified loser: candidate {} lower bound {bound:.5} is above candidate {}'s \
             finished score; {how}; {at}",
            candidates[index], candidates[winner]
        );
    }
}

/// Copies one measurement into its row. The prefilter fields are the
/// caller's to set, because only the caller knows whether this measurement
/// IS the prefilter or the production pass that replaced it.
fn fill_measured(
    row: &mut CommitDemandRecord,
    measured: &crate::demand::MeasuredSupportDemand,
) -> Result<(), SearchError> {
    row.measured_support_area_cm2 = Some(finite(measured.support_area_cm2)?);
    row.struts = Some(measured.struts);
    row.total_scar_mm2 = Some(finite(measured.total_scar_mm2)?);
    row.achieved_pitch_mm = Some(finite(measured.achieved_pitch_mm)?);
    row.residual_demand_cells = Some(measured.residual_demand_cells);
    row.uncarried_load_mm2 = Some(finite(measured.uncarried_load_mm2)?);
    row.model_landings = Some(measured.model_landings);
    row.landing_scar_raw_mm2 = measured.landing_scar_raw_mm2.map(finite).transpose()?;
    row.tip_scar_raw_mm2 = measured.tip_scar_raw_mm2.map(finite).transpose()?;
    row.fidelity = Some(measured.fidelity.label().to_owned());
    row.measurement_error = None;
    Ok(())
}

/// Re-sorts and re-ranks the commit tier after a demand pass.
///
/// The comparator is `score_tier`'s — score ascending, ties to the lower
/// `CandidateId` — with ONE precedence in front of it: a candidate carrying a
/// production-raster burden sorts ahead of one carrying only a prefilter
/// burden, whatever the two scores say. That is `FidelityLedger`'s own
/// doctrine (an exact number outranks a surrogate one) applied to the
/// quantity the surrogate is measured to get wrong: the coarse raster ranks
/// a disc's 180° flip backwards, so letting a coarse score beat a production
/// score would reintroduce the very error the second stage exists to
/// remove. The full ranking is still published, so the prefilter-only
/// candidates and their scores stay visible below the finalists.
fn rerank_commit(ranking: &mut [RankedCandidate], rows: &[CommitDemandRecord]) {
    let production = |candidate: usize| -> bool {
        rows.iter()
            .find(|row| row.candidate == candidate)
            .is_some_and(CommitDemandRecord::measured_at_production)
    };
    // A certified loser has no production score. It ranks with the
    // production-measured candidates, at the BEST score it could have (its
    // lower bound), and never ahead of the winner, whose score is strictly
    // below every certified bound. Where a known score and a bound meet, the
    // order is the candidate id's, as for equal scores: the runner-up it
    // yields is the best candidate that is not provably worse, which is the
    // honest statement when one score is unknown.
    let key = |entry: &RankedCandidate| -> f64 {
        entry
            .certified_loser
            .map_or_else(|| entry.terms.score.get(), |loser| loser.lower_bound_score.get())
    };
    ranking.sort_by(|left, right| {
        let known = |entry: &RankedCandidate| production(entry.candidate) || entry.certified_loser.is_some();
        known(right)
            .cmp(&known(left))
            .then(key(left).total_cmp(&key(right)))
            .then(left.candidate.cmp(&right.candidate))
    });
    for (rank, entry) in ranking.iter_mut().enumerate() {
        entry.rank = rank;
    }
}

/// The peel-force bracket's robustness check (D14): the SAME commit-tier
/// candidate set, re-scored at each of the four corners in turn (holding
/// every other term fixed at what was actually measured), with the argmin
/// candidate at each corner named. Agreement is the decision; a
/// `pose_choice_not_robust` disagreement names which corners moved the
/// winner rather than averaging over them.
///
/// **Deviation from a literal four-independent-funnels reading of §12**,
/// disclosed rather than hidden: promotion through the coarse and refine
/// tiers still runs ONCE, at the primary corner. Only the commit tier's own
/// candidate set — already the tier the decision is made at, per
/// `FidelityLedger`'s own doctrine — is re-scored at all four corners. A
/// literal re-run of the whole three-tier funnel four times would have
/// quadrupled the commit tier's real-sweep-demand cost (LANE 2 measured
/// single production sweeps at up to 76.85 s), for a question this
/// narrower, cheaper check already answers: does the corner choice move the
/// winner AMONG the candidates that reached the tier where the decision is
/// made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeelRobustnessRecord {
    /// `(corner label, winning candidate id)`, in the four-corner order
    /// `PeelForceCorners::named` defines. `None` when the commit tier
    /// itself has no ranking (which `search` already refuses on), or when the
    /// corner is listed in `undetermined_corners`.
    pub winner_at_each_corner: Vec<(String, Option<usize>)>,
    /// `true` only when every corner NAMES the primary corner's winner; a
    /// corner that could not be decided is not an agreement.
    pub agrees_at_every_corner: bool,
    /// `true` only on a PROVEN disagreement, in `disagreeing_corners`. An
    /// undetermined corner is not evidence either way.
    pub pose_choice_not_robust: bool,
    pub disagreeing_corners: Vec<String>,
    /// Corners at which a certified loser (whose production score is unknown)
    /// might still have won: its lower bound at that corner is not strictly
    /// above the best known score there. Absent from every record that
    /// stopped nothing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub undetermined_corners: Vec<String>,
}

/// One weight's flip crossover between the commit-tier winner and runner-up
/// (D15): the value that weight would need to take, holding every other
/// weight at its registered value, for the runner-up to tie the winner.
/// `crossover` is `None` when the two candidates' own term values are equal
/// (no finite weight distinguishes them on this axis) or there is no
/// runner-up at all.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeightFlipRecord {
    pub weight_id: String,
    pub registered_value: Finite,
    pub crossover: Option<Finite>,
    /// `true` when `crossover` is positive and within a factor of two of
    /// `registered_value` — close enough that this decision rests more on
    /// the registered weight's own value than on the measured geometry.
    pub pose_choice_not_robust: bool,
}

/// What the funnel's cheapness actually cost, measured rather than assumed.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FidelityLedger {
    /// The committed winner's rank at each proxy tier, or `None` when it never
    /// appeared in that tier's ranking at all.
    ///
    /// `None` is not a decoration: it is exactly the case the legacy-block
    /// escort exists for. On the committed `Sphere` the winner is the legacy
    /// X −20 pose, which the L1 proxy ranked **478th** of 643 and the refine
    /// tier therefore never saw — it reached the exact tier by right. A
    /// sentinel rank would have hidden that; the absence states it.
    pub coarse_rank_of_final_winner: Option<usize>,
    pub refine_rank_of_final_winner: Option<usize>,
    /// `true` when the coarse tier's own best candidate is the one the exact
    /// tier committed to.
    pub proxy_agreed_with_exact: bool,
    pub winner_runner_up_angle_deg: Option<Finite>,
    pub grid_spacing_deg: Finite,
    /// The committed margin: winner vs runner-up, both at the **commit** tier
    /// on the production mesh. This is the number the decision was actually
    /// made by.
    pub winner_runner_up_margin: Option<Finite>,
    /// Set only when the runner-up is a [`CertifiedLoser`]: its score is
    /// unknown, `winner_runner_up_margin` is then `None`, and this is a lower
    /// bound on the margin (its lower bound minus the winner's score).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winner_runner_up_margin_lower_bound: Option<Finite>,
    /// The tier `winner_runner_up_margin` was measured at. Always
    /// [`SearchTier::Commit`]; stated so a reader never has to assume it.
    pub margin_tier: SearchTier,
    /// `false` when the winner's margin over the runner-up is smaller than the
    /// score spread across the winner's own grid neighbours — the honest "this
    /// instrument did not resolve this choice".
    ///
    /// **Both sides are measured at [`Self::spread_tier`].** They have to be:
    /// only the coarse tier scores the winner's grid neighbours at all, so a
    /// spread can only exist there, and comparing an Exact-tier margin against a
    /// ~2 %-proxy spread would mix fidelities — the proxy's own noise inflates
    /// the spread and biases the verdict toward `false` for reasons that have
    /// nothing to do with the grid. So the margin compared here is the coarse
    /// margin between the same two candidates, and
    /// [`Self::winner_runner_up_margin`] beside it is the committed one.
    ///
    /// Read it for what it is: a statement about whether the **coarse grid**
    /// separated these two directions, not about whether the pipeline resolved
    /// the choice. `None` when either candidate has no coarse score, which
    /// happens when it reached the commit tier by escort rather than promotion
    /// and was refused at the coarse tier.
    pub decision_resolved: Option<bool>,
    /// The same-tier margin `decision_resolved` was computed from.
    pub winner_runner_up_margin_at_spread_tier: Option<Finite>,
    pub neighbour_spread: Finite,
    /// The tier `neighbour_spread` and the verdict were measured at. Always
    /// [`SearchTier::Coarse`]; stated so the fidelity is never inferred.
    pub spread_tier: SearchTier,
}

/// Charged and metered, both stated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkRecord {
    pub charged_face_direction_products: usize,
    pub charged_vertex_direction_products: usize,
    pub charged_section_breakpoints: usize,
    pub executed_face_direction_products: usize,
    pub executed_vertex_direction_products: usize,
    pub executed_section_breakpoints: usize,
}

/// The winner, as a record that can later be replayed as a pin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WinnerRecord {
    pub candidate: usize,
    pub block: CandidateBlock,
    pub selection: BaselineSelection,
    pub up_model_frame: [Finite; 3],
    pub terms: TermsRecord,
}

/// Everything one dense sweep decided, and on what evidence.
///
/// The schema is versioned and `deny_unknown_fields`: a record written by a
/// different search is refused rather than partially believed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DenseSweepRecord {
    pub schema_version: u32,
    pub model_fingerprint: String,
    pub grid: GridRecord,
    pub objective: ObjectiveRecord,
    pub lods: Vec<LodRecord>,
    pub fallbacks: Vec<ProxyFallback>,
    pub tiers: Vec<TierRecord>,
    pub winner: WinnerRecord,
    pub runner_up: Option<RankedCandidate>,
    pub fidelity: FidelityLedger,
    /// LANE 3 §12: the commit tier's real sweep measurement per candidate,
    /// beside the projected-area proxy it replaced there.
    pub commit_tier_demand: Vec<CommitDemandRecord>,
    /// LANE 3 §12/D14: whether the peel-force bracket's choice of corner
    /// moves the commit-tier winner.
    pub peel_robustness: PeelRobustnessRecord,
    /// LANE 3 §12/D15: per-weight flip crossovers between the winner and
    /// runner-up.
    pub weight_flips: Vec<WeightFlipRecord>,
    pub work: WorkRecord,
    pub refusals: Vec<RefusalRecord>,
    /// SHA-256 over the canonical serialization of everything above. One string
    /// answers "did the search change".
    pub search_digest: String,
}

/// The current record schema.
///
/// 2 (2026-09-28): every ranking row carries its `up_model_frame`, and the
/// grid record names its flat faces and the flat-face poses escorted to the
/// commit tier.
pub const DENSE_SWEEP_SCHEMA_VERSION: u32 = 2;

/// What one sweep produced: the winner and the evidence.
#[derive(Clone, Debug, PartialEq)]
pub struct SweepOutcome {
    winner: Candidate,
    record: DenseSweepRecord,
}

impl SweepOutcome {
    #[must_use]
    pub const fn winner(&self) -> &Candidate {
        &self.winner
    }
    #[must_use]
    pub const fn record(&self) -> &DenseSweepRecord {
        &self.record
    }
    #[must_use]
    pub fn into_record(self) -> DenseSweepRecord {
        self.record
    }
}

#[derive(Clone, Debug, PartialEq, Error)]
pub enum SearchError {
    #[error("orientation scoring failed: {0}")]
    Score(#[from] ScoreError),
    #[error(
        "every candidate was refused at the {tier:?} tier, so there is nothing to commit to; the first reason was: {first}"
    )]
    NoCandidateSurvived { tier: SearchTier, first: String },
    #[error("the exact production mesh cannot be scored, so no orientation can be committed: {0}")]
    ExactUnscorable(ScoreError),
    #[error("orientation search produced a non-finite {field}")]
    NonFiniteRecord { field: &'static str },
    #[error(
        "the orientation search record could not be serialized, so it has no determinism digest: {detail}"
    )]
    RecordUnserializable { detail: String },
}

fn finite(value: f64) -> Result<Finite, SearchError> {
    Finite::new(value).map_err(|_| SearchError::NonFiniteRecord { field: "term" })
}

/// One resolved rung of the ladder: a scoring table, its record, and — for a
/// proxy rung — the [`SearchProxy`] mesh the next rung down is cascaded from.
///
/// The exact rung deliberately owns no mesh: it borrows the caller's production
/// mesh and is never a `SearchProxy`, so there is no path by which a proxy and a
/// production mesh become interchangeable here either.
struct Rung {
    lod: &'static str,
    table: OrientFaceTable,
    record: LodRecord,
    proxy: Option<SearchProxy>,
}

/// Runs the recorded three-tier funnel over one production mesh.
///
/// `mesh` is the **Exact** LOD: the protected, decimated production mesh this
/// stage is handed. Nothing higher-detail is in scope, by construction.
///
/// # Errors
///
/// [`SearchError::ExactUnscorable`] when the production mesh itself cannot be
/// scored — a refusal, deliberately, rather than a fallback to identity, which
/// is a rotation like any other; [`SearchError::NoCandidateSurvived`] when every
/// candidate at some tier produced a non-finite term.
///
/// `demand` is LANE 3's real-sweep-demand override (§12): when `Some`, every
/// candidate that reaches the commit tier is re-measured through it and its
/// `support_area_cm2` term is replaced (never refused — a measurement
/// failure keeps that one candidate's proxy value and is named in the
/// record's `commit_tier_demand`). `None` reproduces the pre-LANE-3 proxy-
/// only objective exactly, which is what every test in this crate that does
/// not itself exercise the override passes.
///
/// # Diagnostics (`PLINTH_ORIENT_DIAG`, [`crate::diag`])
///
/// With `--perf` on, the pose-stability diagnostics are collected and written
/// to `<perf-dir>/<model>.orient.json`. They are a separate value
/// ([`crate::diag::SweepDiagnostics`]): the record, its digest and the winner
/// are the same with them on or off.
pub fn search(
    mesh: &Mesh,
    request: &SweepRequest,
    demand: Option<&dyn crate::demand::SupportDemandMeasurer>,
) -> Result<SweepOutcome, SearchError> {
    let switches = crate::diag::Switches::from_env();
    for warning in &switches.warnings {
        eprintln!("WARNING: PLINTH_ORIENT_DIAG: {warning}");
    }
    let perf_on = geom_kernel::perf::enabled();
    if switches.wants_production_sweeps() && !switches.off && !perf_on {
        eprintln!(
            "WARNING: PLINTH_ORIENT_DIAG: prod-all and extra= need --perf (the diagnostics have \
             nowhere to go without it); ignored"
        );
    }
    let config = switches.config(perf_on);
    let ablation = crate::ablate::Ablation::from_env();
    for warning in &ablation.warnings {
        eprintln!("WARNING: PLINTH_ORIENT_ABLATE: {warning}");
    }
    if ablation.exact_shadow && config.is_none() {
        eprintln!(
            "WARNING: PLINTH_ORIENT_ABLATE: exact-shadow needs --perf with the diagnostics on \
             (it has nowhere to go otherwise); ignored"
        );
    }
    if ablation.exact_grid {
        eprintln!(
            "[orient] ABLATION exact-grid: the candidate grid is built from the exact table \
             (an experiment, not the recorded search; use --no-cache)"
        );
    }
    if ablation.exact_score {
        eprintln!(
            "[orient] ABLATION exact-score: every funnel tier scores on the exact table, the \
             proxy tiers are retired (an experiment, not the recorded search; use --no-cache)"
        );
    }
    // The `orient.json` write happens inside `search_core`, in the same
    // diagnostic perf phase as the collection.
    let (outcome, _) = search_core(mesh, request, demand, config.as_ref(), true, &ablation)?;
    if switches.digest {
        eprintln!("[orient] search_digest {}", outcome.record.search_digest);
    }
    Ok(outcome)
}

/// [`search`] with the ablation given instead of read from the environment,
/// and the diagnostics (when `config` is `Some`) returned instead of written.
///
/// # Errors
///
/// As [`search`].
pub fn search_ablated(
    mesh: &Mesh,
    request: &SweepRequest,
    demand: Option<&dyn crate::demand::SupportDemandMeasurer>,
    config: Option<&crate::diag::DiagnosticsConfig>,
    ablation: &crate::ablate::Ablation,
) -> Result<(SweepOutcome, Option<crate::diag::SweepDiagnostics>), SearchError> {
    search_core(mesh, request, demand, config, false, ablation)
}

/// [`search`] with the diagnostics returned instead of written, whatever the
/// environment says. The outcome is the one [`search`] returns.
///
/// # Errors
///
/// As [`search`].
pub fn search_with_diagnostics(
    mesh: &Mesh,
    request: &SweepRequest,
    demand: Option<&dyn crate::demand::SupportDemandMeasurer>,
    config: &crate::diag::DiagnosticsConfig,
) -> Result<(SweepOutcome, crate::diag::SweepDiagnostics), SearchError> {
    let (outcome, diagnostics) = search_core(
        mesh,
        request,
        demand,
        Some(config),
        false,
        &crate::ablate::Ablation::default(),
    )?;
    Ok((
        outcome,
        diagnostics.expect("search_core returns diagnostics whenever it is given a config"),
    ))
}

/// The search. With `diag`, also collects the diagnostics; with `write_json`
/// too, writes them to `<perf-dir>/<model>.orient.json` (both inside the
/// diagnostic perf phase).
pub(crate) fn search_core(
    mesh: &Mesh,
    request: &SweepRequest,
    demand: Option<&dyn crate::demand::SupportDemandMeasurer>,
    diag: Option<&crate::diag::DiagnosticsConfig>,
    write_json: bool,
    ablation: &crate::ablate::Ablation,
) -> Result<(SweepOutcome, Option<crate::diag::SweepDiagnostics>), SearchError> {
    let mut fallbacks = Vec::new();
    // The diagnostics' captures: copies taken in place, read by nothing here.
    let mut raw = crate::diag::RawSnapshots::default();

    // The exact rung first: if the production mesh cannot be scored there is no
    // orientation to commit to and the whole sweep refuses by name.
    let scar_free = request.scar_free();
    // Write-only stage marks (X0, 2026-10-10): where the OrientTable phase
    // spends its time. Inert unless `--perf` is on.
    let exact_mark = geom_kernel::perf::stage("orient.rung_exact");
    let exact = rung(mesh, EXACT_LOD, None, scar_free).map_err(|error| match error {
        RungError::Score(error) => SearchError::ExactUnscorable(error),
        RungError::Simplify(_) | RungError::BelowFloor { .. } | RungError::NoCascadeSource => {
            SearchError::ExactUnscorable(ScoreError::Constants(
                "the exact LOD is never simplified".to_owned(),
            ))
        }
    })?;
    drop(exact_mark);

    // ABLATION `exact-score` with `exact-grid` (pose-stability Lane C): every tier
    // scores on the exact table and the grid reads it too, so no proxy is built.
    // `exact-score` alone still builds them: the grid reads the coarse proxy.
    let build_proxies = !(ablation.exact_score && ablation.exact_grid);
    let (l1, l2) = if build_proxies {
        // L1, then L2 cascaded FROM L1 — a refinement-consistent coarsening, so a
        // rank cannot invert merely because two independent QEM runs chose
        // different edges.
        let l1_mark = geom_kernel::perf::stage("orient.rung_l1");
        let l1 = build_proxy_rung(
            mesh,
            L1_LOD,
            request.ladder.l1_ratio(),
            request.ladder,
            scar_free,
        );
        drop(l1_mark);
        let l1 = record_fallback(l1, L1_LOD, EXACT_LOD, &mut fallbacks);
        let l2_mark = geom_kernel::perf::stage("orient.rung_l2");
        let l2 = match l1.as_ref().and_then(Rung::proxy_mesh) {
            Some(source) => build_proxy_rung(
                source,
                L2_LOD,
                request.ladder.l2_of_l1_ratio(),
                request.ladder,
                scar_free,
            ),
            // No L1 means no cascade source. Coarsening the exact mesh directly
            // would produce an L2 that is not a coarsening of the L1 that is not
            // there, which is precisely the independent-QEM-run inversion the
            // cascade exists to prevent. It is recorded as its own reason: there is
            // no proxy here to measure, so the record must not describe one.
            None => Err(RungError::NoCascadeSource),
        };
        drop(l2_mark);
        let l2 = record_fallback(
            l2,
            L2_LOD,
            if l1.is_some() { L1_LOD } else { EXACT_LOD },
            &mut fallbacks,
        );
        (l1, l2)
    } else {
        (None, None)
    };

    // The grid's table: the coarse proxy's, unless `exact-grid` reads the exact one.
    let proxy_coarse = l2.as_ref().or(l1.as_ref()).unwrap_or(&exact);
    // ABLATION `exact-score` retires the funnel's proxy tiers (design D-12): the
    // coarse, refine and commit tiers all score on the exact table. The tier
    // widths (the cuts) are unchanged; with one scorer each cut keeps the best
    // of the same ranking, so the proxies cannot reorder the survivors.
    let (coarse, refine) = if ablation.exact_score {
        (&exact, &exact)
    } else {
        (proxy_coarse, l1.as_ref().unwrap_or(&exact))
    };

    // The grid's rest-pose block is read off the tier that scores it: rest poses
    // are a property of the geometry being searched. ABLATION `exact-grid`
    // (pose-stability Lane A) reads them off the exact table instead, in
    // ascending bin order; the coarse tier still scores on its proxy.
    let (grid_rung, rest_order) = if ablation.exact_grid {
        (&exact, RestOrder::ByBinIndex)
    } else {
        (proxy_coarse, RestOrder::ByArea)
    };
    let grid_mark = geom_kernel::perf::stage("orient.grid");
    // With the diagnostics on, the SAME binning pass is watched by an
    // observer (G1/G2); with them off the pass carries no observer at all.
    let (grid, grid_capture) = if diag.is_some() {
        let mut observer = crate::diag::GridObserver::default();
        let (grid, bins) =
            SweepGrid::build_observed(&grid_rung.table, request.grid, rest_order, &mut observer)?;
        (grid, Some(observer.finish(bins)))
    } else {
        (
            SweepGrid::build_ordered(&grid_rung.table, request.grid, rest_order)?,
            None,
        )
    };
    let directions = grid.directions();
    drop(grid_mark);

    let mut tiers = Vec::with_capacity(3);
    let mut refusals = Vec::new();
    let mut work = WorkRecord {
        charged_face_direction_products: 0,
        charged_vertex_direction_products: 0,
        charged_section_breakpoints: 0,
        executed_face_direction_products: 0,
        executed_vertex_direction_products: 0,
        executed_section_breakpoints: 0,
    };

    // The legacy block and the flat-face poses, always. See `escort` below.
    let by_right: Vec<usize> = grid
        .candidates()
        .iter()
        .filter(|candidate| candidate.block == CandidateBlock::Legacy)
        .map(|candidate| candidate.id)
        .chain(grid.flat_face_poses().iter().copied())
        .collect();

    let mut admitted: Vec<usize> = (0..grid.candidates().len()).collect();
    let mut escorted = 0_usize;
    for (tier, rung, width) in [
        (
            SearchTier::Coarse,
            coarse,
            Some(request.ladder.refine_top_k()),
        ),
        (
            SearchTier::Refine,
            refine,
            Some(request.ladder.commit_top_k()),
        ),
        (SearchTier::Commit, &exact, None),
    ] {
        let tier_mark = geom_kernel::perf::stage(match tier {
            SearchTier::Coarse => "orient.score_coarse",
            SearchTier::Refine => "orient.score_refine",
            SearchTier::Commit => "orient.score_commit",
        });
        let mut record = score_tier(
            tier,
            &rung.table,
            rung.lod,
            &grid,
            &directions,
            &admitted,
            request.weights,
            request.limits,
            &mut refusals,
            &mut work,
        )?;
        drop(tier_mark);
        record.promoted = width.map_or(0, |width| record.ranking.len().min(width));
        record.escorted = if tier == SearchTier::Commit {
            escorted
        } else {
            0
        };
        if diag.is_some() && tier == SearchTier::Commit {
            raw.exact_table.clone_from(&record.ranking);
        }
        let promoted: Vec<usize> = record
            .ranking
            .iter()
            .take(width.unwrap_or(0))
            .map(|ranked| ranked.candidate)
            .collect();
        if tier == SearchTier::Refine {
            let (set, added) = escort(&promoted, &by_right);
            escorted = added;
            admitted = set;
            if diag.is_some() {
                raw.promoted_to_commit.clone_from(&promoted);
            }
        } else {
            admitted = promoted;
        }
        tiers.push(record);
    }

    // LANE 3 §12: replace the commit tier's projected-area proxy with the
    // sweep's real measured SUPPORT BURDEN, in two stages.
    //
    // Stage 1 measures every candidate the commit tier admitted
    // (`ORIENT_COMMIT_TOP_K` promoted plus the escorted legacy 13 and
    // flat-face poses — up to ~16-18 on the corpus) on the cheap coarsened
    // raster. Stage 2 re-measures the
    // top `ORIENT_COMMIT_TOP_K` of those at the PRODUCTION pitch, the raster
    // the emitted support field is actually planned on.
    //
    // Both bounds come from the search's own funnel widths and neither
    // decides anything: widening either costs sweeps and cannot make a pose
    // better. What the split buys is the one thing the coarse raster cannot
    // give — measured, at the coarse pitch a 25 mm disc's 180° flip takes 88
    // arms against the correct pose's 102, and at the production pitch it
    // takes 88 against 70. So a prefilter number keeps a pose in the running
    // and a production number crowns it, which is why the final comparator
    // sorts production-measured candidates ahead of prefilter-only ones.
    //
    // A candidate whose sweep fails keeps its proxy value and the failure is
    // named, never refused.
    let mut commit_tier_demand: Vec<CommitDemandRecord> = Vec::new();
    if let (Some(measurer), Some(commit_record)) = (demand, tiers.last_mut()) {
        // ---- stage 1: the prefilter, over the whole admitted set ----------
        let admitted_directions: Vec<UpDirection> = commit_record
            .ranking
            .iter()
            .map(|entry| grid.candidates()[entry.candidate].direction)
            .collect();
        let phase_before = geom_kernel::perf::set_phase(geom_kernel::perf::Phase::Prefilter);
        let prefiltered = measurer.measure_support_demand(
            mesh,
            &exact.table,
            &admitted_directions,
            crate::demand::DemandFidelity::Prefilter,
        );
        geom_kernel::perf::set_phase(phase_before);
        for (index, entry) in commit_record.ranking.iter_mut().enumerate() {
            let mut row = CommitDemandRecord::unmeasured(
                entry.candidate,
                finite(entry.terms.support_area_cm2.get())?,
            );
            match prefiltered.get(index) {
                Some(Ok(measured)) => {
                    entry.terms = entry.terms.repriced(
                        request.weights,
                        measured.support_area_cm2,
                        entry.terms.peel_force_n.low_pressure_low_exponent_n.get(),
                        // The coarse raster prunes and never prices the scar:
                        // where a coarse arm lands (or carries) is not where
                        // the emitted field's arm lands (or carries).
                        None,
                        None,
                    );
                    fill_measured(&mut row, measured)?;
                    row.prefilter_support_area_cm2 = row.measured_support_area_cm2;
                    row.prefilter_struts = row.struts;
                }
                Some(Err(error)) => row.measurement_error = Some(error.clone()),
                None => row.measurement_error = Some(SHORT_DEMAND_BATCH.to_owned()),
            }
            commit_tier_demand.push(row);
        }
        rerank_commit(&mut commit_record.ranking, &commit_tier_demand);
        if diag.is_some() {
            raw.after_prefilter = Some(commit_record.ranking.clone());
        }

        // ---- stage 2: the finalists, at the production pitch --------------
        let finalists = request.ladder.commit_top_k().min(commit_record.ranking.len());
        let finalist_directions: Vec<UpDirection> = commit_record.ranking[..finalists]
            .iter()
            .map(|entry| grid.candidates()[entry.candidate].direction)
            .collect();
        // THE CERTIFIED EARLY STOP (perf lane B1). Each finalist's lower bound
        // is known before any production sweep runs; the batch stops a sweep
        // once a finished one scores strictly below that bound
        // (`demand::ProductionRace`). The race owns WHEN a sweep stops; the
        // certified set below owns what the record says, from the winner's
        // score alone.
        let weights = request.weights;
        let bounds_allowed = fields_non_negative(&exact.table);
        let finalist_terms: Vec<TermsRecord> =
            commit_record.ranking[..finalists].iter().map(|entry| entry.terms).collect();
        let bounds: Vec<Option<f64>> = finalist_terms
            .iter()
            .map(|terms| {
                bounds_allowed
                    .then(|| {
                        terms.production_lower_bound(
                            weights,
                            terms.peel_force_n.low_pressure_low_exponent_n.get(),
                        )
                    })
                    .flatten()
            })
            .collect();
        let price_terms = finalist_terms.clone();
        let race = crate::demand::ProductionRace::new(
            bounds.clone(),
            Box::new(move |index, measured| {
                price_terms.get(index).map(|terms| {
                    terms
                        .repriced(
                            weights,
                            measured.support_area_cm2,
                            terms.peel_force_n.low_pressure_low_exponent_n.get(),
                            measured.landing_scar_raw_mm2,
                            measured.tip_scar_raw_mm2,
                        )
                        .score
                        .get()
                })
            }),
        );
        let phase_before = geom_kernel::perf::set_phase(geom_kernel::perf::Phase::Production);
        let produced = measurer.measure_production_race(
            mesh,
            &exact.table,
            &finalist_directions,
            &race,
        );
        geom_kernel::perf::set_phase(phase_before);
        let finished_scores: Vec<Option<f64>> = finalist_terms
            .iter()
            .enumerate()
            .map(|(index, terms)| match produced.get(index) {
                Some(crate::demand::RaceOutcome::Measured(measured)) => Some(
                    terms
                        .repriced(
                            weights,
                            measured.support_area_cm2,
                            terms.peel_force_n.low_pressure_low_exponent_n.get(),
                            measured.landing_scar_raw_mm2,
                            measured.tip_scar_raw_mm2,
                        )
                        .score
                        .get(),
                ),
                _ => None,
            })
            .collect();
        let candidate_ids: Vec<usize> = commit_record.ranking[..finalists]
            .iter()
            .map(|entry| entry.candidate)
            .collect();
        let verdict = certify_losers(&candidate_ids, &bounds, &finished_scores);
        for &index in &verdict.violations {
            eprintln!(
                "WARNING: [orient] finalist {} scored below its own lower bound; its production \
                 measurement is kept and the bound is not trusted for it",
                candidate_ids[index]
            );
        }
        report_certified_stops(&candidate_ids, &verdict, &race, &produced);
        for (index, entry) in commit_record.ranking[..finalists].iter_mut().enumerate() {
            let Some(row) = commit_tier_demand
                .iter_mut()
                .find(|row| row.candidate == entry.candidate)
            else {
                continue;
            };
            if let Some(bound) = verdict.certified[index] {
                // A certified loser: its production score is unknown and is
                // not read, whether the sweep stopped, finished or failed.
                // The row keeps its prefilter numbers and says why.
                let loser = CertifiedLoser {
                    lower_bound_score: finite(bound)?,
                };
                entry.certified_loser = Some(loser);
                row.certified_loser = Some(loser);
                continue;
            }
            match produced.get(index) {
                Some(crate::demand::RaceOutcome::Measured(measured)) => {
                    entry.terms = entry.terms.repriced(
                        request.weights,
                        measured.support_area_cm2,
                        entry.terms.peel_force_n.low_pressure_low_exponent_n.get(),
                        // The one place the measured model-landing and
                        // tip-contact scars are folded in, and the one
                        // fidelity allowed to price either.
                        measured.landing_scar_raw_mm2,
                        measured.tip_scar_raw_mm2,
                    );
                    let prefilter_area = row.prefilter_support_area_cm2;
                    let prefilter_struts = row.prefilter_struts;
                    fill_measured(row, measured)?;
                    row.prefilter_support_area_cm2 = prefilter_area;
                    row.prefilter_struts = prefilter_struts;
                }
                Some(crate::demand::RaceOutcome::Failed(error)) => {
                    row.measurement_error = Some(error.clone());
                }
                // A stop the certified set does not contain cannot happen
                // (every stop is requested against a finished score, and the
                // winner's is the lowest); if it ever does, the pose is a
                // failed measurement, named, and keeps its prefilter price.
                Some(crate::demand::RaceOutcome::Stopped) => {
                    debug_assert!(false, "a stopped finalist was not certified a loser");
                    row.measurement_error = Some(UNCERTIFIED_STOP.to_owned());
                }
                None => row.measurement_error = Some(SHORT_DEMAND_BATCH.to_owned()),
            }
        }
        rerank_commit(&mut commit_record.ranking, &commit_tier_demand);
        #[cfg(test)]
        {
            if diag.is_some_and(|config| config.sabotage_feed_rerank) {
                sabotage_feed_rerank(
                    &mut commit_record.ranking,
                    &mut commit_tier_demand,
                    &raw,
                    &grid,
                    mesh,
                    &exact.table,
                    request,
                    measurer,
                );
            }
        }
        if diag.is_some() {
            raw.after_production = Some(commit_record.ranking.clone());
        }
    }

    let record_mark = geom_kernel::perf::stage("orient.record");
    let commit = tiers.last().expect("three tiers were scored");
    let winner_rank = commit
        .ranking
        .first()
        .ok_or_else(|| SearchError::NoCandidateSurvived {
            tier: SearchTier::Commit,
            first: refusals.last().map_or_else(
                || "no reason recorded".to_owned(),
                |refusal| refusal.reason.clone(),
            ),
        })?
        .clone();
    let winner = grid.candidates()[winner_rank.candidate];
    let runner_up = commit.ranking.get(1).cloned();

    let peel_robustness = peel_robustness(&commit.ranking, request.weights);
    // A certified runner-up has no production score, so no crossover can be
    // solved against it: its flips are absent, as for a search with no
    // runner-up, and the runner-up row says why (`certified_loser`).
    let weight_flips = weight_flips(
        &winner_rank.terms,
        runner_up
            .as_ref()
            .filter(|r| r.certified_loser.is_none())
            .map(|r| &r.terms),
        request.weights,
    );

    let fidelity = ledger(&grid, &tiers, &winner_rank, runner_up.as_ref(), request)?;

    let mut lods = vec![exact.record.clone()];
    if let Some(rung) = &l1 {
        lods.push(rung.record.clone());
    }
    if let Some(rung) = &l2 {
        lods.push(rung.record.clone());
    }

    let mut record = DenseSweepRecord {
        schema_version: DENSE_SWEEP_SCHEMA_VERSION,
        model_fingerprint: exact.table.mesh_fingerprint().to_owned(),
        grid: grid_record(&grid)?,
        objective: objective_record(request.weights)?,
        lods,
        fallbacks,
        tiers,
        winner: WinnerRecord {
            candidate: winner.id,
            block: winner.block,
            selection: winner.selection,
            up_model_frame: [
                finite(winner.direction.get()[0])?,
                finite(winner.direction.get()[1])?,
                finite(winner.direction.get()[2])?,
            ],
            terms: winner_rank.terms,
        },
        runner_up,
        fidelity,
        commit_tier_demand,
        peel_robustness,
        weight_flips,
        work,
        refusals,
        search_digest: String::new(),
    };
    record.search_digest = digest_record(&record)?;
    drop(record_mark);

    // The diagnostics are built from the FINISHED record and nothing they
    // compute is read back (module doc, `crate::diag`).
    let diagnostics = diag.zip(grid_capture.as_ref()).map(|(config, grid_capture)| {
        crate::diag::in_diagnostic_phase(|| {
            let build_mark = geom_kernel::perf::stage("orient.diag");
            let diagnostics = crate::diag::build(&crate::diag::DiagInputs {
                config,
                mesh,
                request,
                grid: &grid,
                grid_capture,
                grid_table: &grid_rung.table,
                grid_table_lod: grid_rung.lod,
                ablation,
                exact_table: &exact.table,
                demand,
                tiers: &record.tiers,
                snapshots: &raw,
                record: &record,
            });
            drop(build_mark);
            if write_json {
                crate::diag::write_orient_json(&record, &diagnostics);
            }
            diagnostics
        })
    });
    Ok((SweepOutcome { winner, record }, diagnostics))
}

/// TEST ONLY: the defect the diagnostics must never have. Feeds the
/// separate measurer's production numbers for EVERY admitted pose into the
/// commit ranking and re-ranks, so a test can show that comparing the record
/// and its digest with the diagnostics on and off would catch it.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn sabotage_feed_rerank(
    ranking: &mut [RankedCandidate],
    rows: &mut [CommitDemandRecord],
    raw: &crate::diag::RawSnapshots,
    grid: &SweepGrid,
    mesh: &Mesh,
    exact_table: &OrientFaceTable,
    request: &SweepRequest,
    measurer: &dyn crate::demand::SupportDemandMeasurer,
) {
    let Some(diagnostic) = measurer.diagnostic_measurer() else {
        return;
    };
    let items: Vec<crate::diag::DiagItem> = raw
        .exact_table
        .iter()
        .map(|entry| crate::diag::DiagItem {
            label: String::new(),
            key: None,
            candidate: Some(entry.candidate),
            direction: grid.candidates()[entry.candidate].direction,
            exact_terms: entry.terms,
        })
        .collect();
    let leaked = crate::diag::measure_items(
        diagnostic.as_ref(),
        mesh,
        exact_table,
        request.weights,
        items,
    );
    for entry in ranking.iter_mut() {
        let row = leaked.iter().find(|row| row.candidate == Some(entry.candidate));
        if let Some(terms) = row.and_then(|row| row.production_terms) {
            entry.terms = terms;
        }
    }
    for row in rows.iter_mut() {
        row.fidelity = Some(crate::demand::DemandFidelity::Production.label().to_owned());
    }
    rerank_commit(ranking, rows);
}

impl Rung {
    /// The proxy mesh this rung was built from — the source the next, coarser
    /// rung cascades off. Only a proxy rung has one.
    fn proxy_mesh(&self) -> Option<&Mesh> {
        self.proxy.as_ref().map(SearchProxy::mesh)
    }
}

/// The exact-shadow's scoring (pose-stability Lane A, out of band): the
/// candidates `admitted`, scored on `table` and ranked by the search's own
/// comparator, with nothing recorded in the search's work counters or
/// refusals. Returns the ranking and the number of refused candidates.
///
/// # Errors
///
/// As the tier scoring: a table that cannot score, or no candidate surviving.
pub(crate) fn shadow_rank(
    table: &OrientFaceTable,
    lod: &str,
    grid: &SweepGrid,
    admitted: &[usize],
    weights: ObjectiveWeights,
    limits: OrientSearchLimits,
) -> Result<(Vec<RankedCandidate>, usize), SearchError> {
    let mut refusals = Vec::new();
    let mut work = WorkRecord {
        charged_face_direction_products: 0,
        charged_vertex_direction_products: 0,
        charged_section_breakpoints: 0,
        executed_face_direction_products: 0,
        executed_vertex_direction_products: 0,
        executed_section_breakpoints: 0,
    };
    let directions = grid.directions();
    let record = score_tier(
        SearchTier::Coarse,
        table,
        lod,
        grid,
        &directions,
        admitted,
        weights,
        limits,
        &mut refusals,
        &mut work,
    )?;
    Ok((record.ranking, refusals.len()))
}

/// Scores one tier and ranks it.
#[allow(clippy::too_many_arguments)]
fn score_tier(
    tier: SearchTier,
    table: &OrientFaceTable,
    lod: &str,
    grid: &SweepGrid,
    directions: &[crate::score::UpDirection],
    admitted: &[usize],
    weights: ObjectiveWeights,
    limits: OrientSearchLimits,
    refusals: &mut Vec<RefusalRecord>,
    work: &mut WorkRecord,
) -> Result<TierRecord, SearchError> {
    let subset: Vec<crate::score::UpDirection> =
        admitted.iter().map(|index| directions[*index]).collect();
    let scored = score_candidates(table, &subset, weights, limits)?;
    let plan = scored.plan();
    let meter = scored.meter();
    work.charged_face_direction_products += plan.face_direction_products;
    work.charged_vertex_direction_products += plan.vertex_direction_products;
    work.charged_section_breakpoints += plan.section_breakpoints;
    work.executed_face_direction_products += meter.face_direction_products;
    work.executed_vertex_direction_products += meter.vertex_direction_products;
    work.executed_section_breakpoints += meter.section_breakpoints;

    let mut ranking = Vec::with_capacity(subset.len());
    for (position, outcome) in scored.outcomes().iter().enumerate() {
        let candidate = admitted[position];
        match outcome {
            CandidateOutcome::Scored(terms) => {
                let direction = grid.candidates()[candidate].direction.get();
                ranking.push(RankedCandidate {
                    rank: 0,
                    candidate,
                    block: grid.candidates()[candidate].block,
                    up_model_frame: [
                        finite(direction[0])?,
                        finite(direction[1])?,
                        finite(direction[2])?,
                    ],
                    terms: TermsRecord::new(*terms)?,
                    certified_loser: None,
                });
            }
            CandidateOutcome::Refused(refusal) => refusals.push(RefusalRecord {
                tier,
                candidate,
                reason: describe(refusal.reason),
            }),
        }
    }
    if ranking.is_empty() {
        return Err(SearchError::NoCandidateSurvived {
            tier,
            first: refusals.last().map_or_else(
                || "no reason recorded".to_owned(),
                |refusal| refusal.reason.clone(),
            ),
        });
    }

    // The one comparator: score ascending, ties to the lower CandidateId. No
    // epsilon, no second semantic. Every score here is finite (a non-finite term
    // is a refusal above), so `total_cmp` is a total order over exactly the
    // values present, and `-0.0` was canonicalized to `0.0` before it arrived.
    ranking.sort_by(|left, right| {
        left.terms
            .score
            .get()
            .total_cmp(&right.terms.score.get())
            .then(left.candidate.cmp(&right.candidate))
    });
    for (rank, entry) in ranking.iter_mut().enumerate() {
        entry.rank = rank;
    }

    Ok(TierRecord {
        tier,
        fidelity: tier.fidelity(),
        lod: lod.to_owned(),
        scored: subset.len(),
        ranked: ranking.len(),
        promoted: 0,
        escorted: 0,
        ranking,
    })
}

/// Adds the legacy block to the commit tier's candidate set, whether or not the
/// proxies promoted it, and returns the set in ascending id order.
///
/// **This is a deliberate strengthening of the recorded funnel, and it is here
/// because the recorded funnel measurably did not deliver what D-12 claims for
/// it.** D-12 keeps the legacy 13 permanently "so the sweep is never worse by
/// construction". Enumerating them at the coarse tier is not sufficient for
/// that: the committed score is computed at the exact LOD, so a proxy that
/// mis-ranks a legacy candidate out of the top `ORIENT_REFINE_TOP_K` discards
/// the very candidate the guarantee rests on, and the guarantee quietly becomes
/// a hope about proxy fidelity.
///
/// Measured, not hypothesized. On the committed `Sphere.prepped.stl` the
/// unescorted funnel committed to a candidate scoring **16.3173** where the
/// best of the legacy 13, scored on the same exact table, scores **16.3009** —
/// a 0.1 % regression against the search this one replaces, produced entirely
/// by proxy mis-ranking. Escorting makes "never worse" a property of the
/// candidate set at the tier that decides, which is the only tier where the
/// claim means anything.
///
/// The cost is bounded and small: at most 13 additional exact-tier scorings,
/// against a `ORIENT_COMMIT_TOP_K` of 3. No cap, gate or width was raised to
/// make room for it — the two recorded funnel widths are untouched, and the
/// escort is counted separately in the tier record so a reader can see exactly
/// how many candidates reached the exact tier by promotion and how many by
/// right.
///
/// # The flat-face poses ride the same escort (2026-09-28)
///
/// The second set carried by right is [`SweepGrid::flat_face_poses`]: for
/// every flat face of the part, the pose that rests it on the plate (`grid`
/// module doc). A disc has two flat faces, one per side, so both its
/// horizontal poses ride — glue face down and glue face up. Measured, not
/// hypothesized, on the same grounds as the legacy block: on `Round_50.stl`
/// and `Round_100.stl` the glue-face-up flip — the owner-ruled correct pose —
/// was ranked **76th of 646** and **482nd of 644** by the coarse proxy and
/// never measured, because the proxy's 45° support step made ~45° tilts look
/// an order of magnitude cheaper than they measure (7517–12381 arms against
/// 1058 on Round 100; `plinth-evidence/slice-sweep-2026-09-02/
/// field-2026-09-28/round50.md` Q2). A disc adds one candidate here (its
/// other flat pose is identity, already legacy); a part with no flat face
/// adds none.
pub(crate) fn escort(promoted: &[usize], by_right: &[usize]) -> (Vec<usize>, usize) {
    let mut set: BTreeSet<usize> = promoted.iter().copied().collect();
    let mut added = 0_usize;
    for candidate in by_right {
        if set.insert(*candidate) {
            added += 1;
        }
    }
    (set.into_iter().collect(), added)
}

pub(crate) fn describe(reason: CandidateRefusalReason) -> String {
    match reason {
        CandidateRefusalReason::NonFiniteTerm { term, value } => {
            format!("{term} evaluated to {value}")
        }
        CandidateRefusalReason::NegativeSectionArea { peak_mm2 } => {
            format!("the peak section area is negative: {peak_mm2} mm²")
        }
    }
}

enum RungError {
    Score(ScoreError),
    Simplify(SimplifyError),
    BelowFloor { faces: usize, floor: usize },
    NoCascadeSource,
}

/// Builds one rung's table and record, or says why it could not.
///
/// `scar_free` is not a performance switch. §3.8 requires a zero scar weight to
/// reproduce the scar-free search **structurally**, so at a zero weight the
/// delicacy field is not built here at all: it cannot fail the sweep, and its
/// digest cannot enter `search_digest`.
fn rung(
    mesh: &Mesh,
    lod: &'static str,
    proxy: Option<SearchProxy>,
    scar_free: bool,
) -> Result<Rung, RungError> {
    let table_mark = geom_kernel::perf::stage("orient.face_table");
    let table = if scar_free {
        OrientFaceTable::without_delicacy(mesh, lod)
    } else {
        OrientFaceTable::build(mesh, lod)
    }
    .map_err(RungError::Score)?;
    drop(table_mark);
    let record = LodRecord {
        lod: lod.to_owned(),
        mesh_fingerprint: table.mesh_fingerprint().to_owned(),
        faces: table.face_count(),
        verts: table.vertex_count(),
        requested_ratio: proxy
            .as_ref()
            .and_then(|proxy| Finite::new(proxy.requested_ratio).ok()),
        achieved_ratio: proxy
            .as_ref()
            .and_then(|proxy| Finite::new(proxy.achieved_ratio).ok()),
        boundary_edges: table.boundary_edges(),
        nonmanifold_edges: table.nonmanifold_edges(),
        delicacy_digest: table.delicacy_digest().map(str::to_owned),
        display: table.display_summary(),
    };
    Ok(Rung {
        lod,
        table,
        record,
        proxy,
    })
}

fn build_proxy_rung(
    source: &Mesh,
    lod: &'static str,
    ratio: f64,
    ladder: LadderSettings,
    scar_free: bool,
) -> Result<Rung, RungError> {
    let simplify_mark = geom_kernel::perf::stage("orient.simplify");
    let proxy = SearchProxy::simplify(source, ratio).map_err(RungError::Simplify)?;
    drop(simplify_mark);
    let faces = proxy.mesh().tris().len();
    if faces < ladder.min_faces() {
        return Err(RungError::BelowFloor {
            faces,
            floor: ladder.min_faces(),
        });
    }
    let mesh = proxy.mesh().clone();
    rung(&mesh, lod, Some(proxy), scar_free)
}

/// Turns a rung failure into a recorded fallback rather than a silent one.
fn record_fallback(
    outcome: Result<Rung, RungError>,
    planned: &str,
    used: &str,
    fallbacks: &mut Vec<ProxyFallback>,
) -> Option<Rung> {
    match outcome {
        Ok(rung) => Some(rung),
        Err(error) => {
            let (reason, detail) = match error {
                RungError::Simplify(error) => {
                    (ProxyFallbackReason::SimplifyRefused, error.to_string())
                }
                RungError::BelowFloor { faces, floor } => (
                    ProxyFallbackReason::BelowFaceFloor { faces, floor },
                    format!("{faces} faces is below the {floor}-face search-proxy floor"),
                ),
                RungError::NoCascadeSource => (
                    ProxyFallbackReason::NoCascadeSource,
                    format!(
                        "no {planned} proxy was built: the rung it cascades from is absent, so \
                         there was nothing to coarsen and no proxy was measured"
                    ),
                ),
                RungError::Score(ScoreError::OpenBoundary { boundary_edges }) => (
                    ProxyFallbackReason::OpenBoundary { boundary_edges },
                    format!("the proxy has {boundary_edges} boundary edges"),
                ),
                RungError::Score(error) => {
                    (ProxyFallbackReason::FieldUnavailable, error.to_string())
                }
            };
            fallbacks.push(ProxyFallback {
                planned_lod: planned.to_owned(),
                reason,
                used_lod: used.to_owned(),
                detail,
            });
            None
        }
    }
}

fn grid_record(grid: &SweepGrid) -> Result<GridRecord, SearchError> {
    Ok(GridRecord {
        legacy: grid.legacy_count(),
        rest_poses: grid.rest_pose_count(),
        fibonacci: grid.fibonacci_count(),
        flat_faces: grid.flat_face_count(),
        flat_face_poses: grid.flat_face_poses().len(),
        duplicates_dropped: grid.duplicates_dropped(),
        candidates: grid.candidates().len(),
        fibonacci_count: grid.settings().fibonacci_count(),
        rest_pose_max: grid.settings().rest_pose_max(),
        rest_pose_reference_bins: grid.settings().reference_bins(),
        dedupe_tolerance_deg: finite(grid.settings().dedupe_tolerance_deg())?,
        direction_digest: grid.digest().to_owned(),
        z_spin_deg: finite(0.0)?,
    })
}

/// D14: re-scores every commit-tier candidate at each of the four bracket
/// corners in turn, holding `support_area_cm2`, `height_mm` and `scar_term`
/// at what was actually measured for that candidate, and names the argmin
/// at each corner.
/// One bracket corner's reader: `(label, accessor)`.
type CornerReader = (&'static str, fn(PeelForceCornersRecord) -> f64);

fn peel_robustness(ranking: &[RankedCandidate], weights: ObjectiveWeights) -> PeelRobustnessRecord {
    let corners: [CornerReader; 4] = [
        ("low_pressure_low_exponent", |c: PeelForceCornersRecord| {
            c.low_pressure_low_exponent_n.get()
        }),
        ("low_pressure_high_exponent", |c: PeelForceCornersRecord| {
            c.low_pressure_high_exponent_n.get()
        }),
        ("high_pressure_low_exponent", |c: PeelForceCornersRecord| {
            c.high_pressure_low_exponent_n.get()
        }),
        (
            "high_pressure_high_exponent",
            |c: PeelForceCornersRecord| c.high_pressure_high_exponent_n.get(),
        ),
    ];
    let decided: Vec<(String, CornerWinner)> = corners
        .iter()
        .map(|(label, read)| ((*label).to_owned(), corner_winner(ranking, weights, *read)))
        .collect();
    let winners: Vec<(String, Option<usize>)> = decided
        .iter()
        .map(|(label, winner)| {
            (
                label.clone(),
                match winner {
                    CornerWinner::Named(candidate) => *candidate,
                    CornerWinner::Undetermined => None,
                },
            )
        })
        .collect();
    let undetermined_corners: Vec<String> = decided
        .iter()
        .filter(|(_, winner)| *winner == CornerWinner::Undetermined)
        .map(|(label, _)| label.clone())
        .collect();
    let primary = winners.first().and_then(|(_, winner)| *winner);
    // A disagreement is PROVEN: a corner whose winner is named and differs.
    let disagreeing_corners: Vec<String> = decided
        .iter()
        .filter(|(_, winner)| match winner {
            CornerWinner::Named(candidate) => *candidate != primary,
            CornerWinner::Undetermined => false,
        })
        .map(|(label, _)| label.clone())
        .collect();
    PeelRobustnessRecord {
        agrees_at_every_corner: disagreeing_corners.is_empty() && undetermined_corners.is_empty(),
        pose_choice_not_robust: !disagreeing_corners.is_empty(),
        disagreeing_corners,
        winner_at_each_corner: winners,
        undetermined_corners,
    }
}

/// A corner's winner, or the statement that a certified loser might have won
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CornerWinner {
    Named(Option<usize>),
    Undetermined,
}

/// The argmin candidate id under one peel-force corner, re-pricing every
/// candidate's already-measured terms rather than re-scoring from the mesh —
/// the corner choice changes nothing but the peel term.
///
/// A certified loser has no production score to re-price. It is held to its
/// lower bound at THIS corner: when that bound is strictly above the best
/// known score at the corner it cannot have won it and is left out; when it is
/// not, the corner is [`CornerWinner::Undetermined`], because the unknown
/// score might have been the lowest.
fn corner_winner(
    ranking: &[RankedCandidate],
    weights: ObjectiveWeights,
    read_corner: fn(PeelForceCornersRecord) -> f64,
) -> CornerWinner {
    let best = corner_best(ranking, weights, read_corner, |entry| entry.certified_loser.is_none());
    let Some((_, best_score)) = best else {
        return CornerWinner::Named(None);
    };
    let might_have_won = ranking
        .iter()
        .filter(|entry| entry.certified_loser.is_some())
        .any(|entry| {
            let peel_force_n = read_corner(entry.terms.peel_force_n);
            entry
                .terms
                .production_lower_bound(weights, peel_force_n)
                .is_none_or(|bound| bound.partial_cmp(&best_score) != Some(std::cmp::Ordering::Greater))
        });
    if might_have_won {
        CornerWinner::Undetermined
    } else {
        CornerWinner::Named(best.map(|(candidate, _)| candidate))
    }
}

/// The lowest `(candidate, corner score)` among the entries `keep` admits.
fn corner_best(
    ranking: &[RankedCandidate],
    weights: ObjectiveWeights,
    read_corner: fn(PeelForceCornersRecord) -> f64,
    keep: impl Fn(&RankedCandidate) -> bool,
) -> Option<(usize, f64)> {
    ranking
        .iter()
        .filter(|entry| keep(entry))
        .map(|entry| {
            let peel_force_n = read_corner(entry.terms.peel_force_n);
            let repriced = entry
                .terms
                // The scar term, landing and tip halves included, is already
                // in these terms and a corner cannot move either.
                .repriced(
                    weights,
                    entry.terms.support_area_cm2.get(),
                    peel_force_n,
                    None,
                    None,
                );
            (entry.candidate, repriced.score.get())
        })
        .min_by(|left, right| left.1.total_cmp(&right.1).then(left.0.cmp(&right.0)))
}

/// D15: the value one weight would need, holding every other weight at its
/// registered value, for the runner-up to tie the winner — solving
/// `fixed_winner + w*term_winner = fixed_runner + w*term_runner` for `w`.
/// `None` when the two candidates' terms agree on this axis (no weight
/// distinguishes them).
fn flip_crossover(
    score_winner: f64,
    score_runner: f64,
    term_winner: f64,
    term_runner: f64,
    registered_weight: f64,
) -> Option<f64> {
    let denominator = term_winner - term_runner;
    if denominator.abs() < 1e-12 {
        return None;
    }
    let fixed_winner = score_winner - registered_weight * term_winner;
    let fixed_runner = score_runner - registered_weight * term_runner;
    Some((fixed_runner - fixed_winner) / denominator)
}

/// D15: the flip crossover for each of the four price terms, between the
/// commit-tier winner and runner-up.
fn weight_flips(
    winner: &TermsRecord,
    runner_up: Option<&TermsRecord>,
    weights: ObjectiveWeights,
) -> Vec<WeightFlipRecord> {
    let Some(runner) = runner_up else {
        return Vec::new();
    };
    let mut flips = Vec::with_capacity(4);
    let mut push = |weight_id: &str, registered: f64, term_winner: f64, term_runner: f64| {
        let crossover = flip_crossover(
            winner.score.get(),
            runner.score.get(),
            term_winner,
            term_runner,
            registered,
        );
        let pose_choice_not_robust = crossover.is_some_and(|value| {
            registered > 0.0 && value > 0.0 && (value / registered).max(registered / value) <= 2.0
        });
        flips.push(WeightFlipRecord {
            weight_id: weight_id.to_owned(),
            registered_value: Finite::new(registered)
                .unwrap_or_else(|_| Finite::new(0.0).expect("zero is finite")),
            crossover: crossover.and_then(|value| Finite::new(value).ok()),
            pose_choice_not_robust,
        });
    };
    push(
        ORIENT_WEIGHT_SUPPORT_AREA_CM2_ID,
        weights.support_area_cm2(),
        winner.support_area_cm2.get(),
        runner.support_area_cm2.get(),
    );
    push(
        ORIENT_PEEL_POINTS_PER_NEWTON_ID,
        weights.peel_points_per_newton(),
        winner.peel_force_n.low_pressure_low_exponent_n.get(),
        runner.peel_force_n.low_pressure_low_exponent_n.get(),
    );
    push(
        ORIENT_WEIGHT_HEIGHT_MM_ID,
        weights.height_mm(),
        winner.height_mm.get(),
        runner.height_mm.get(),
    );
    push(
        ORIENT_SCAR_WEIGHT_ID,
        weights.scar(),
        winner.scar_raw_mm2.get() / 100.0,
        runner.scar_raw_mm2.get() / 100.0,
    );
    flips
}

fn objective_record(weights: ObjectiveWeights) -> Result<ObjectiveRecord, SearchError> {
    let weight = |id: &str, value: f64| -> Result<WeightRecord, SearchError> {
        Ok(WeightRecord {
            constant_id: id.to_owned(),
            value: finite(value)?,
        })
    };
    // Registered but no longer read by `score`; read fresh from the registry
    // rather than restated, so the "unchanged, unread" claim is checkable.
    let peak_xsec_retired_value = m0_constants()
        .map_err(|error| SearchError::Score(ScoreError::Constants(error.to_string())))?
        .placeholders()
        .orient_weight_peak_xsec_cm2()
        .diagnostic()
        .get();
    Ok(ObjectiveRecord {
        support_area_cm2: weight(
            ORIENT_WEIGHT_SUPPORT_AREA_CM2_ID,
            weights.support_area_cm2(),
        )?,
        height_mm: weight(ORIENT_WEIGHT_HEIGHT_MM_ID, weights.height_mm())?,
        scar: weight(ORIENT_SCAR_WEIGHT_ID, weights.scar())?,
        overhang_angle_deg: weight(OVERHANG_ANGLE_DEG_ID, weights.overhang_angle_deg())?,
        peel_points_per_newton: weight(
            ORIENT_PEEL_POINTS_PER_NEWTON_ID,
            weights.peel_points_per_newton(),
        )?,
        peak_xsec_cm2_retired: weight(ORIENT_WEIGHT_PEAK_XSEC_CM2_ID, peak_xsec_retired_value)?,
        peel_reference_area_mm2: weight(
            ORIENT_PEEL_REFERENCE_AREA_MM2_ID,
            weights.peel_reference_area_mm2(),
        )?,
        peel_pressure_pa_low: weight(
            ORIENT_PEEL_REFERENCE_PRESSURE_PA_LOW_ID,
            weights.peel_pressure_pa_low(),
        )?,
        peel_pressure_pa_high: weight(
            ORIENT_PEEL_REFERENCE_PRESSURE_PA_HIGH_ID,
            weights.peel_pressure_pa_high(),
        )?,
        peel_area_exponent_low: weight(
            ORIENT_PEEL_AREA_EXPONENT_LOW_ID,
            weights.peel_area_exponent_low(),
        )?,
        peel_area_exponent_high: weight(
            ORIENT_PEEL_AREA_EXPONENT_HIGH_ID,
            weights.peel_area_exponent_high(),
        )?,
        down_limit: finite(weights.down_limit())?,
        peak_section_method: "exact_piecewise_quadratic_supremum".to_owned(),
    })
}

/// The fidelity ledger: what the funnel's cheapness cost, measured.
fn ledger(
    grid: &SweepGrid,
    tiers: &[TierRecord],
    winner: &RankedCandidate,
    runner_up: Option<&RankedCandidate>,
    request: &SweepRequest,
) -> Result<FidelityLedger, SearchError> {
    let rank_of = |tier: SearchTier| -> Option<usize> {
        tiers
            .iter()
            .find(|record| record.tier == tier)
            .and_then(|record| {
                record
                    .ranking
                    .iter()
                    .find(|entry| entry.candidate == winner.candidate)
            })
            .map(|entry| entry.rank)
    };
    let coarse = tiers
        .iter()
        .find(|record| record.tier == SearchTier::Coarse)
        .expect("the coarse tier ran");
    let proxy_agreed = coarse
        .ranking
        .first()
        .is_some_and(|entry| entry.candidate == winner.candidate);

    // `sqrt(4π/N)` is the mean angular spacing of N points spread evenly over
    // the sphere: the resolution of the instrument, stated beside the margin it
    // is being asked to resolve.
    let spacing_rad = (4.0 * std::f64::consts::PI / request.grid.fibonacci_count() as f64).sqrt();
    let spacing_deg = spacing_rad.to_degrees();

    let winner_direction = grid.candidates()[winner.candidate].direction.get();
    let angle = runner_up.map(|entry| {
        let other = grid.candidates()[entry.candidate].direction.get();
        let dot = vec3::dot(winner_direction, other).clamp(-1.0, 1.0);
        dot.acos().to_degrees()
    });
    // A certified runner-up has no production score: its margin is UNKNOWN,
    // and only a lower bound on it is stated, beside it.
    let margin = runner_up
        .filter(|entry| entry.certified_loser.is_none())
        .map(|entry| entry.terms.score.get() - winner.terms.score.get());
    let margin_lower_bound = runner_up.and_then(|entry| {
        entry
            .certified_loser
            .map(|loser| loser.lower_bound_score.get() - winner.terms.score.get())
    });

    // The spread of coarse scores across the winner's own grid neighbours: if
    // the objective moves more between neighbours than the winner beat the
    // runner-up by, this instrument did not resolve this choice.
    let neighbour_cosine = spacing_rad.cos();
    let mut lowest = f64::INFINITY;
    let mut highest = f64::NEG_INFINITY;
    for entry in &coarse.ranking {
        let other = grid.candidates()[entry.candidate].direction.get();
        let dot = vec3::dot(winner_direction, other);
        if dot >= neighbour_cosine {
            lowest = lowest.min(entry.terms.score.get());
            highest = highest.max(entry.terms.score.get());
        }
    }
    let spread = if lowest.is_finite() && highest.is_finite() {
        highest - lowest
    } else {
        0.0
    };

    // The verdict is computed entirely on the coarse tier, because that is the
    // only tier where the winner's grid neighbours were scored. Taking the
    // margin from the commit tier and the spread from the coarse one compared
    // two different instruments and reported the difference as a property of the
    // grid.
    let coarse_score = |candidate: usize| -> Option<f64> {
        coarse
            .ranking
            .iter()
            .find(|entry| entry.candidate == candidate)
            .map(|entry| entry.terms.score.get())
    };
    let coarse_margin = match (coarse_score(winner.candidate), runner_up) {
        (Some(winner_score), Some(entry)) => {
            coarse_score(entry.candidate).map(|other| other - winner_score)
        }
        _ => None,
    };

    Ok(FidelityLedger {
        coarse_rank_of_final_winner: rank_of(SearchTier::Coarse),
        refine_rank_of_final_winner: rank_of(SearchTier::Refine),
        proxy_agreed_with_exact: proxy_agreed,
        winner_runner_up_angle_deg: angle.map(finite).transpose()?,
        grid_spacing_deg: finite(spacing_deg)?,
        winner_runner_up_margin: margin.map(finite).transpose()?,
        winner_runner_up_margin_lower_bound: margin_lower_bound.map(finite).transpose()?,
        margin_tier: SearchTier::Commit,
        decision_resolved: coarse_margin.map(|margin| margin >= spread),
        winner_runner_up_margin_at_spread_tier: coarse_margin.map(finite).transpose()?,
        neighbour_spread: finite(spread)?,
        spread_tier: SearchTier::Coarse,
    })
}

/// SHA-256 over the record's own canonical serialization.
///
/// The digest is computed over the record with its digest field empty, so it is
/// a function of the search and not of itself.
///
/// A serialization failure is a **typed refusal**, never a default. This
/// function used to end `serde_json::to_vec(record).unwrap_or_default()`, so any
/// failure yielded the SHA-256 of the empty string — the same constant for every
/// record. `search_digest` is the one string T4, T7, the thread sweep and the
/// cache-equivalence check compare on, so a path under which two different
/// searches agree is precisely the defect the digest exists to detect.
fn digest_record(record: &DenseSweepRecord) -> Result<String, SearchError> {
    let canonical =
        serde_json::to_vec(record).map_err(|error| SearchError::RecordUnserializable {
            detail: error.to_string(),
        })?;
    let mut hasher = Sha256::new();
    hasher.update(canonical);
    let digest = hasher.finalize();
    let mut text = String::with_capacity(digest.len() * 2);
    for byte in digest {
        text.push_str(&format!("{byte:02x}"));
    }
    Ok(text)
}

/// Every distinct LOD name a record names, for a caller that wants to assert the
/// ladder without walking the tiers.
#[must_use]
pub fn lods_used(record: &DenseSweepRecord) -> BTreeSet<String> {
    record
        .tiers
        .iter()
        .map(|tier| tier.lod.clone())
        .collect::<BTreeSet<_>>()
}
