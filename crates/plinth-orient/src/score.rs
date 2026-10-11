//! The orientation objective, evaluated for many candidate build directions at
//! once.
//!
//! This module is the **scoring substrate** of the dense orientation sweep: the
//! four objective terms, batched over candidates, and nothing else.  It has no
//! candidate grid, no search loop, no policy and no CLI surface — those consume
//! what is here.  Everything it computes is a pure function of `(mesh, LOD,
//! registered constants, candidate list)`.
//!
//! # The objective
//!
//! ```text
//! score(u) = ((w_sup·support_area_cm²(u) + p·F(peak_mm2(u)))
//!             + w_h·height_mm(u))
//!            + w_scar·Σ_down(area_mm²·delicacy)/100
//! ```
//!
//! recorded in `03-physics-and-constants.md` §3.8, with every weight a
//! registered `Placeholder`.  The association order above is **pinned** (the
//! einsum lesson, `docs/engineering-journal/04-migration-plan.md`
//! §"Five things that must be right"): a re-association is a
//! different set of rounding decisions and therefore a different argmin.
//!
//! ## The peak-cross-section term is now a peel-force term (LANE 3, 2026-09-02)
//!
//! `docs/plans/slice-sweep-core-2026-09-02.md` §12: on Round 50 the retired
//! `w_peak·peak_xsec_cm2` term alone was 80-84% of the flat pose's score, so
//! every disc tilted to minimize peak cross-section rather than being placed
//! by measured support demand. `ORIENT_WEIGHT_PEAK_XSEC_CM2` is retired from
//! this objective (still registered, unread — `placeholders.rs`'s own note).
//! In its place, `F(A) = c·A^n` with `c = P_ref·(A_ref·1e-6) / A_ref^n`, a
//! literature-anchored peel force in Newtons evaluated at the candidate's own
//! exact peak cross-section area. Neither `P_ref` nor `n` is a single
//! preferred value — the literature gives a PRESSURE bracket
//! (`ORIENT_PEEL_REFERENCE_PRESSURE_PA_LOW/HIGH`, Manizani ~9 kPa vs Pan et
//! al.'s PDMS ~19.6 kPa) and an EXPONENT bracket
//! (`ORIENT_PEEL_AREA_EXPONENT_LOW/HIGH`, 0.24 porosity-at-constant-envelope
//! vs 0.46 envelope-shrinks-with-area) — so [`PeelForceCorners`] carries all
//! four corner evaluations and [`ObjectiveWeights::registered`]'s score uses
//! the LOW-pressure/LOW-exponent corner as `p`, the review's own first-named
//! anchor point (`LITERATURE-NOTES.md:813,826`), while the other three ride
//! along for [`crate::search`]'s robustness check (D14). `p` itself,
//! `ORIENT_PEEL_POINTS_PER_NEWTON`, is a registered policy exchange rate
//! (there is no physical conversion from Newtons to the height/scar terms'
//! unmeasured points scale) and the run report publishes the weight value at
//! which the winning pose flips (D15).
//!
//! ## The scar term has a fourth half, and this module cannot measure either
//!
//! `Σ_down(...)` above prices the model's own down-facing area. It says
//! nothing about where the support field SEATS or STANDS: on `make_raking_blade`
//! the two flip poses tie on every term here (65.42 mm² of fragility in both,
//! because the 85°-raked blade is within 5° of vertical and never enters the
//! down-facing fold), and the pose that put a strut ON the blade won by three
//! arms of measured burden. Two missing quantities follow, both measurable
//! only from a real support field and both folded in at the same commit-tier
//! stage: the MODEL-LANDING scar — `Σ` over the arms that land on the model
//! of `π r_seat² × (fragility + D)` of the landed face — and the
//! TIP-CONTACT scar — the same product over the faces the arms' TIPS carry,
//! excluding a face the fold above already counts (`plinth_sweep::tip_landing`'s
//! own doc has the double-counting choice and the measurement behind it). Both
//! are the same product in the same units under the same `ORIENT_SCAR_WEIGHT`.
//! They are measured in `plinth_sweep::model_landing` and
//! `plinth_sweep::tip_landing`, arrive through
//! [`crate::demand::MeasuredSupportDemand`], and are folded into `scar_raw_mm2`
//! by [`crate::search`] at the commit tier's production stage. Here they are
//! [`CandidateTerms::landing_scar_raw_mm2`] and
//! [`CandidateTerms::tip_scar_raw_mm2`] `= None`: a stated absence.
//!
//! ## Fragility and display ADD; neither scales the other (2026-09-29)
//!
//! Every scar half prices an area by `(fragility + D)`: `Σ_down(area ×
//! (fragility + D))` here, and `π r² × (fragility + D)` for the two contact
//! scars. Both fields are dimensionless in `[0, 1]`, so each half is an area
//! in mm², and `ORIENT_SCAR_WEIGHT` prices both. `plinth-mesh/src/display.rs`
//! defines the two apart: delicacy asks *would a support break this*,
//! display asks *would a support spoil the look of this*, "neither is
//! expressed in terms of the other". Two independent harms of one contact
//! add.
//!
//! Until 2026-09-29 the display field entered as a multiplier, `area ×
//! fragility × (1 + D)`, so a display face was protected only in proportion
//! to its fragility, and a large, sturdy, textured base top was nearly
//! unpriced. On the Morel base that charge was 6.685 mm² against the 21.3 mm²
//! of fragility its flip saves on the underside, so the pose was decided by
//! support-placement details and turned over on develop `0f70cf2`
//! (`plinth-evidence/.../field-2026-09-28/lanes/morel-pose.md`). Where
//! `D = 0` the two forms are the same number, bit for bit.
//!
//! ## The search space is S², not SO(3)
//!
//! A candidate is a unit **build-up direction in model frame**, `u = Rᵀẑ`, and
//! that is the public type: azimuth is inexpressible rather than merely unused.
//! For any face normal `n`, `(Rn)_z = n·u`; height is `max(v·u) − min(v·u)`; and
//! a planar area is invariant under rotation *within* its own plane.  So every
//! term here is a function of `u` alone and a k-fold z-spin sweep would multiply
//! cost by k for exactly zero objective change.  The physics record refuses an
//! azimuthal term independently (§4.2 "Orientation term: None"; §2.4's Y-position
//! effect is ≤ 1 rung and is to be randomized, not tuned), and z-spin already
//! belongs to the future `plinth-plate` (`02:979`).  Test
//! `z_spin_is_a_null_direction_of_every_term` machine-checks the invariance so a
//! future term that breaks it fails loudly instead of silently costing k×.
//!
//! ## Delicacy is a property of the table being scored
//!
//! `OrientFaceTable` binds areas, normals, vertices **and** the delicacy field
//! to one `(mesh, LOD)` pair, and refuses a field measured on any other — the
//! recorded D-4 ruling (`02:224-253`).  The Python defect it names (delicacy
//! from the full mesh, areas from a 90 %-decimated proxy, so scar magnitudes
//! moved with the decimation ratio) cannot be expressed here: there is no
//! constructor that accepts a foreign field.
//!
//! ## The overhang cutoff is the engine's own expression
//!
//! `down_limit = -(OVERHANG_ANGLE_DEG.to_radians().sin())`, applied with strict
//! `<` — character for character what `plinth-support` evaluates at
//! `crates/plinth-support/src/lib.rs:814,820`, reading the same registered
//! constant.  Copying the expression rather than registering a second cutoff is
//! deliberate: two constants would be two things to keep aligned, and the sweep's
//! notion of "down-facing" must not be able to drift from the engine's.  At 45°
//! it equals the legacy −0.7071…; the three-decimal literal `-0.707` survives
//! only inside the legacy-compat driver.
//!
//! The sweep deliberately omits the engine's exterior, near-plate and
//! grounded-column filters (`plinth-support/src/lib.rs:819-887`), because §3.8
//! requires a proxy derivable **without running the support engine**.  At orient
//! time the mesh is pre-hollow solid, so the exterior mask is trivially true
//! anyway; the residual over-count is a bias to be measured per model, not
//! asserted away.
//!
//! # Batch-first and determinism
//!
//! Repo rule D-37.  One `NZ = N·U` product per candidate tile, faces outer and
//! candidates inner so the normal array streams once; the vertex-side
//! projections are three whole-array passes inside each candidate's own task.
//! No per-face spatial query and no per-candidate mesh rotation exists anywhere
//! in this file.  Rayon is used **across candidates only**, as index-parallel
//! map-collect under the workspace `PLINTH_THREADS` pool; every reduction inside
//! a candidate is a serial fold in index order, and the candidate tiling cannot
//! move a result because no fold crosses a tile boundary.

use geom_kernel::vec3;
use plinth_core::m0_constants;
use plinth_mesh::{
    build_face_topology, evaluate_delicacy, evaluate_display, mesh_fingerprint,
    DelicacyBuildConfig, DelicacyError, DelicacyField, DelicacySettings, DisplayBuildConfig,
    DisplayError, DisplayField, DisplaySettings, Mesh, TopologyError, WeldPolicy,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::section::{build_profile, SectionRefusal};

/// Registered `ConstantId`s of the objective, carried so a report can stamp the
/// weights it scored with rather than restating their values.
pub const ORIENT_WEIGHT_SUPPORT_AREA_CM2_ID: &str = "ORIENT_WEIGHT_SUPPORT_AREA_CM2";
/// Retired from the objective (LANE 3, 2026-09-02); still registered, unread.
/// See [`PeelForceCorners`] and `ORIENT_PEEL_POINTS_PER_NEWTON_ID`.
pub const ORIENT_WEIGHT_PEAK_XSEC_CM2_ID: &str = "ORIENT_WEIGHT_PEAK_XSEC_CM2";
pub const ORIENT_WEIGHT_HEIGHT_MM_ID: &str = "ORIENT_WEIGHT_HEIGHT_MM";
pub const ORIENT_SCAR_WEIGHT_ID: &str = "ORIENT_SCAR_WEIGHT";
/// The display-face field's own registered numbers, carried so a report can
/// stamp what the scar term's display factor was measured with. **No new
/// WEIGHT is registered for the display factor and none is read**: the scar
/// term keeps [`ORIENT_SCAR_WEIGHT_ID`] unchanged and the field enters as a
/// dimensionless per-area harm `D` beside fragility: `area × (fragility + D)`.
pub const DISPLAY_VIEW_DIRECTIONS_ID: &str = "DISPLAY_VIEW_DIRECTIONS";
pub const DISPLAY_DETAIL_TIP_MULTIPLE_ID: &str = "DISPLAY_DETAIL_TIP_MULTIPLE";
pub const OVERHANG_ANGLE_DEG_ID: &str = "OVERHANG_ANGLE_DEG";
pub const ORIENT_PEEL_POINTS_PER_NEWTON_ID: &str = "ORIENT_PEEL_POINTS_PER_NEWTON";
pub const ORIENT_PEEL_REFERENCE_AREA_MM2_ID: &str = "ORIENT_PEEL_REFERENCE_AREA_MM2";
pub const ORIENT_PEEL_REFERENCE_PRESSURE_PA_LOW_ID: &str = "ORIENT_PEEL_REFERENCE_PRESSURE_PA_LOW";
pub const ORIENT_PEEL_REFERENCE_PRESSURE_PA_HIGH_ID: &str =
    "ORIENT_PEEL_REFERENCE_PRESSURE_PA_HIGH";
pub const ORIENT_PEEL_AREA_EXPONENT_LOW_ID: &str = "ORIENT_PEEL_AREA_EXPONENT_LOW";
pub const ORIENT_PEEL_AREA_EXPONENT_HIGH_ID: &str = "ORIENT_PEEL_AREA_EXPONENT_HIGH";

/// A candidate build-up direction in model frame: the whole search space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UpDirection([f64; 3]);

impl UpDirection {
    /// The authored up direction, which is also the identity candidate.
    pub const MODEL_UP: Self = Self([0.0, 0.0, 1.0]);

    /// TEST ONLY: a direction holding exactly these components, unnormalized,
    /// so a boundary test can place a dot product on a chosen bit pattern.
    #[cfg(test)]
    pub(crate) const fn raw(direction: [f64; 3]) -> Self {
        Self(direction)
    }

    /// Normalizes a finite, non-degenerate direction.
    ///
    /// # Errors
    ///
    /// [`ScoreError::InvalidDirection`] for a non-finite or zero-length vector.
    ///
    /// # Behaviour change (geometry-consolidation OP-6 / canon §1)
    ///
    /// The prior body computed `norm` by the expanded sum-of-squares (bit-identical
    /// to the kernel's `dot(a, a).sqrt()`, per canon measurement) and divided each
    /// component by it. `vec3::normalize` instead multiplies by `1.0 / norm(a)`,
    /// which is not bit-identical to componentwise division: expect movement of
    /// **at most 1 ULP per component** on every direction this constructs. The
    /// refusal condition is unchanged — `vec3::normalize` returns `None` exactly
    /// when this function's old finite/positive-norm guard would have refused.
    pub fn new(direction: [f64; 3]) -> Result<Self, ScoreError> {
        let Some(normalized) = vec3::normalize(direction) else {
            return Err(ScoreError::InvalidDirection { direction });
        };
        Ok(Self([
            canonical_zero(normalized[0]),
            canonical_zero(normalized[1]),
            canonical_zero(normalized[2]),
        ]))
    }

    #[must_use]
    pub const fn get(self) -> [f64; 3] {
        self.0
    }

    /// The opposite build direction, `−u`: the same part upside down.
    /// Negation is exact, so the result is unit length whenever `self` is and
    /// needs no renormalization; `−0.0` is canonicalized like every other
    /// direction this type holds.
    #[must_use]
    pub fn antipode(self) -> Self {
        let [x, y, z] = self.0;
        Self([canonical_zero(-x), canonical_zero(-y), canonical_zero(-z)])
    }

    /// The rotation taking this direction to `+ẑ`: the shortest arc, with the
    /// single antipode `u = −ẑ` resolved by one pinned convention, 180° about
    /// `+X`.
    ///
    /// Row 2 is `u` by construction, so `(R·v)_z = v·u`; rows 0 and 1 are the
    /// in-plane basis the section solver measures areas in.  Written in the
    /// closed form `R = c·I + [v]ₓ + v·vᵀ/(1+c)` with `v = u × ẑ` and `c = u_z`.
    ///
    /// # `1 + c` is read off `x² + y²` on the negative hemisphere
    ///
    /// The real quantity `1 + c` stays bounded against `|v|² = 1 − c²`, so the
    /// closed form is singularity-free in exact arithmetic.  In `f64` it is not,
    /// and the failure is not at the antipode alone.  `c` carries an absolute
    /// rounding error of ~1e-16 from the normalization, so once `1 + c` is
    /// itself of that order the naive difference has **no** correct significant
    /// digits, and `k = 1/(1+c)` multiplies the error back up: the normalized
    /// direction `[2e-8, 0, −1]` has a true `1 + c` of 2e-16 and a computed one
    /// of 2.22e-16, which puts `R[1][1] = c + k·x²` at ≈ 0.8 where the true
    /// value is unity.  That is not a rotation, and rest-pose candidates
    /// (area-weighted `−n` aggregates) can land arbitrarily close to `−ẑ` on
    /// real meshes, so the matrix reaches production geometry at emit.
    ///
    /// `x² + y²` is the same quantity's well-conditioned twin — `1 − c² =
    /// (1 − c)(1 + c)` with `1 − c ≈ 2` there — so for `c < 0` the divisor is
    /// read off it: `1 + c = (x² + y²)/(1 − c)`.  Substituting it back gives
    /// `k·(x² + y²) = 1 − c` identically, which is exactly the identity row
    /// orthonormality needs, so the rows stay orthonormal to a ulp for every
    /// direction rather than only away from the pole.  For `c ≥ 0` the direct
    /// `1 + c ≥ 1` is already exact to a ulp and is kept, which also keeps
    /// `x² + y² → 0` at `+ẑ` out of the divisor.  Only the **exact** antipode
    /// (`x = y = 0`, `c = −1`) still needs a convention, and it takes the pinned
    /// one: 180° about `+X`.
    #[must_use]
    pub fn rotation_to_build_frame(self) -> [[f64; 3]; 3] {
        let [x, y, c] = self.0;
        let sin_squared = x * x + y * y;
        let denominator = if c >= 0.0 {
            1.0 + c
        } else {
            sin_squared / (1.0 - c)
        };
        if !denominator.is_finite() || denominator <= 0.0 {
            return [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]];
        }
        let k = 1.0 / denominator;
        [
            [c + k * y * y, -k * x * y, -x],
            [-k * x * y, c + k * x * x, -y],
            [x, y, c],
        ]
    }
}

/// The registered objective: four `Placeholder` weights and the overhang angle
/// they share with the support engine.
///
/// Every number here is non-`AcceptSafe` by construction (D-25).  A search
/// weight selects a candidate; it never accepts one, and acceptance remains the
/// `DefectReport`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectiveWeights {
    support_area_cm2: f64,
    height_mm: f64,
    scar: f64,
    overhang_angle_deg: f64,
    /// `ORIENT_PEEL_POINTS_PER_NEWTON`: policy exchange rate pricing the
    /// peel-force term. Replaces the retired `peak_xsec_cm2` weight.
    peel_points_per_newton: f64,
    /// `ORIENT_PEEL_REFERENCE_AREA_MM2`: 314 mm², the cross-section the
    /// literature's peel-force bracket is anchored at.
    peel_reference_area_mm2: f64,
    peel_pressure_pa_low: f64,
    peel_pressure_pa_high: f64,
    peel_area_exponent_low: f64,
    peel_area_exponent_high: f64,
}

impl ObjectiveWeights {
    /// Reads every number from the one M0 registry.
    ///
    /// # Errors
    ///
    /// [`ScoreError::Constants`] when the registry cannot be constructed.
    pub fn registered() -> Result<Self, ScoreError> {
        let constants = m0_constants().map_err(|error| ScoreError::Constants(error.to_string()))?;
        let values = constants.placeholders();
        let evidence = constants.evidence();
        Ok(Self {
            support_area_cm2: values.orient_weight_support_area_cm2().diagnostic().get(),
            height_mm: values.orient_weight_height_mm().diagnostic().get(),
            scar: values.orient_scar_weight().diagnostic().get(),
            overhang_angle_deg: values.overhang_angle_deg().diagnostic().get(),
            peel_points_per_newton: values.orient_peel_points_per_newton().diagnostic().get(),
            peel_reference_area_mm2: *evidence.orient_peel_reference_area_mm2().value(),
            peel_pressure_pa_low: *evidence.orient_peel_reference_pressure_pa_low().value(),
            peel_pressure_pa_high: *evidence.orient_peel_reference_pressure_pa_high().value(),
            peel_area_exponent_low: *evidence.orient_peel_area_exponent_low().value(),
            peel_area_exponent_high: *evidence.orient_peel_area_exponent_high().value(),
        })
    }

    /// The same objective with the scar term switched off.
    ///
    /// This is not an operator input and there is no CLI flag behind it: the
    /// weight is a registered constant.  It exists for the legacy-compat driver,
    /// which must reproduce a pre-scar argmin, and for the tests that prove a
    /// zero weight makes the answer *structurally* independent of the delicacy
    /// field rather than arithmetically so.
    #[must_use]
    pub const fn scar_free(mut self) -> Self {
        self.scar = 0.0;
        self
    }

    #[must_use]
    pub const fn support_area_cm2(self) -> f64 {
        self.support_area_cm2
    }
    #[must_use]
    pub const fn height_mm(self) -> f64 {
        self.height_mm
    }
    #[must_use]
    pub const fn scar(self) -> f64 {
        self.scar
    }
    #[must_use]
    pub const fn overhang_angle_deg(self) -> f64 {
        self.overhang_angle_deg
    }
    #[must_use]
    pub const fn peel_points_per_newton(self) -> f64 {
        self.peel_points_per_newton
    }
    #[must_use]
    pub const fn peel_reference_area_mm2(self) -> f64 {
        self.peel_reference_area_mm2
    }
    #[must_use]
    pub const fn peel_pressure_pa_low(self) -> f64 {
        self.peel_pressure_pa_low
    }
    #[must_use]
    pub const fn peel_pressure_pa_high(self) -> f64 {
        self.peel_pressure_pa_high
    }
    #[must_use]
    pub const fn peel_area_exponent_low(self) -> f64 {
        self.peel_area_exponent_low
    }
    #[must_use]
    pub const fn peel_area_exponent_high(self) -> f64 {
        self.peel_area_exponent_high
    }

    /// `F(A) = c·A^n` at one bracket corner, `c = P_ref·(A_ref·1e-6) / A_ref^n`
    /// — a peel force in Newtons. `A_ref` and `P_ref` come from the same
    /// literature specimen (a solid 20 mm disc measured on FEP and,
    /// independently, on PDMS); `A` is the candidate's own exact peak
    /// cross-section, `peak_mm2`.
    #[must_use]
    fn peel_force_n_at(self, peak_mm2: f64, pressure_pa: f64, exponent: f64) -> f64 {
        let coefficient =
            (pressure_pa * (self.peel_reference_area_mm2 * 1.0e-6)) / self.peel_reference_area_mm2.powf(exponent);
        coefficient * peak_mm2.powf(exponent)
    }

    /// All four bracket corners: {low, high} pressure x {low, high} exponent.
    #[must_use]
    pub fn peel_force_corners(self, peak_mm2: f64) -> PeelForceCorners {
        PeelForceCorners {
            low_pressure_low_exponent_n: self.peel_force_n_at(
                peak_mm2,
                self.peel_pressure_pa_low,
                self.peel_area_exponent_low,
            ),
            low_pressure_high_exponent_n: self.peel_force_n_at(
                peak_mm2,
                self.peel_pressure_pa_low,
                self.peel_area_exponent_high,
            ),
            high_pressure_low_exponent_n: self.peel_force_n_at(
                peak_mm2,
                self.peel_pressure_pa_high,
                self.peel_area_exponent_low,
            ),
            high_pressure_high_exponent_n: self.peel_force_n_at(
                peak_mm2,
                self.peel_pressure_pa_high,
                self.peel_area_exponent_high,
            ),
        }
    }

    /// The down-facing cutoff, byte for byte the expression
    /// `plinth_support::perceive_contacts_on_surface` evaluates from the same
    /// constant (`crates/plinth-support/src/lib.rs`, `let down_limit = ...`).
    /// A source-line citation drifted stale here once already (recorded 814,
    /// actually 871 by the time this was caught, a 57-line drift); naming the
    /// function instead of a line number is the fix, not just the number.
    #[must_use]
    pub fn down_limit(self) -> f64 {
        -self.overhang_angle_deg.to_radians().sin()
    }
}

/// Deterministic work-unit budgets for one batched scoring call.
///
/// These are resource guards, not registry members: nothing here can change a
/// score.  A budget reached is a **finding** — the call refuses naming both the
/// measured demand and the cap, before any working buffer is allocated, and it
/// never trims the candidate set to fit.  There is no wall clock anywhere
/// (O-4).
///
/// # Every cap is absolute, and that is the whole point
///
/// The first version of this type derived each cap from the very counts the
/// call then charged against it — `faces × candidates` against
/// `max(2^28, faces × 2048)`, with `candidates ≤ 653` and the multiplier 2048.
/// Every comparison was then structurally `n·a ≤ n·b` with `a < b`, so no
/// budget could fire for **any** mesh, and orientation was the one stage in the
/// pipeline with no effective work bound at all.  The caps below are therefore
/// fixed numbers derived once, from the largest model this workspace can run
/// end to end, and are supplied by the caller's resource policy
/// (`plinth_pipeline::PrepareResourcePolicy::orient`) exactly as the design's
/// §6 asks — not recomputed per rung from that rung's own size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrientSearchLimits {
    pub max_candidates: usize,
    pub max_candidate_face_products: usize,
    pub max_candidate_vertex_products: usize,
    pub max_section_breakpoints: usize,
    pub max_working_bytes: usize,
}

impl OrientSearchLimits {
    /// The candidate-count ceiling: `next_power_of_two(2 × 653)`, where 653 is
    /// the recorded coarse tier's own maximum (13 legacy + ≤128 rest poses + 512
    /// Fibonacci).  It bounds an absurd caller, not the recorded grid.
    pub const MAX_CANDIDATES: usize = 2048;

    /// The largest production mesh this workspace can hand the sweep: the
    /// Weeping Treant's prepared mesh, `corpus/baseline/Weeping Treant
    /// v1.2.prepped.stl`, measured 2026-08-10.
    pub const LARGEST_PRODUCTION_FACES: usize = 429_120;

    /// Its vertex count, and the structural maximum for any mesh of that many
    /// faces: a binary STL is a triangle soup and the loader's `WeldPolicy::None`
    /// keeps it one, so `verts = 3 × faces` exactly — measured, and an upper
    /// bound at the same time, since no mesh can carry more than three distinct
    /// vertices per triangle.
    pub const LARGEST_PRODUCTION_VERTS: usize = 3 * Self::LARGEST_PRODUCTION_FACES;

    /// The one production budget: five fixed work-unit ceilings.
    ///
    /// Each is `next_power_of_two(2 × worst case)`, the repo's own cap rule
    /// (`SUPPORT_ROUTE_CCO_MAX_SEAT_EVALUATIONS` and its sibling, FU-32 J5),
    /// where the worst case is the largest reachable model scored against the
    /// largest admissible candidate list — `LARGEST_PRODUCTION_FACES` (or
    /// `_VERTS`) × `MAX_CANDIDATES`.  A mesh about four times the treant, or a
    /// caller handing more than 2048 candidates, refuses; the recorded grid on
    /// every model in the corpus passes with the stated headroom and nothing is
    /// trimmed to fit.
    #[must_use]
    pub const fn production() -> Self {
        // 2 × 429_120 × 2048 = 1_757_675_520 → 2^31.
        const FACE_PRODUCTS: usize = 1 << 31;
        // 2 × 1_287_360 × 3 × 2048 = 15_818_096_640 → 2^34.
        const VERTEX_PRODUCTS: usize = 1 << 34;
        // 2 × 1_287_360 × 2048 = 5_272_698_880 → 2^33.
        const SECTION_BREAKPOINTS: usize = 1 << 33;
        // The treant's own per-candidate working set is 173_364_480 bytes
        // (`per_candidate_bytes` below). Four candidates in flight is the
        // smallest tile worth calling batched, so the byte ceiling is
        // next_power_of_two(4 × that) = 2^30 — the one cap whose multiplier is
        // the tile width rather than the usual 2×, because this cap also *sizes*
        // the tile and a 2× ceiling would force a two-candidate tile on the
        // largest model.
        const WORKING_BYTES: usize = 1 << 30;
        Self {
            max_candidates: Self::MAX_CANDIDATES,
            max_candidate_face_products: FACE_PRODUCTS,
            max_candidate_vertex_products: VERTEX_PRODUCTS,
            max_section_breakpoints: SECTION_BREAKPOINTS,
            max_working_bytes: WORKING_BYTES,
        }
    }
}

/// Bytes one candidate occupies while in flight, counting **every** buffer its
/// evaluation allocates rather than only the two obvious ones.
///
/// Per candidate: its column of `NZ` (`faces` f64), the in-plane projection
/// (`verts × 2` f64) and the heights (`verts` f64) that
/// [`project_vertices`] builds, and then the section solver's own three —
/// [`crate::section::build_profile`]'s sort permutation (`verts` u32), its
/// distinct-height breakpoint list (up to `verts` f64) and its segment tree of
/// quadratic coefficients (`2 × next_power_of_two(intervals)` nodes, so at most
/// `4 × verts` × `[f64; 3]`).  The last three were omitted when this guard was
/// first written, which undercharged the treant by 36 bytes per vertex — 46 MB
/// on a mesh whose whole charge was 34 MB.
fn per_candidate_bytes(faces: usize, verts: usize) -> usize {
    const F64: usize = std::mem::size_of::<f64>();
    const U32: usize = std::mem::size_of::<u32>();
    // NZ column + plane + heights + sort permutation + breakpoints + tree.
    let vertex_side = (2 * F64 + F64 + U32 + F64 + 4 * 3 * F64).saturating_mul(verts);
    faces.saturating_mul(F64).saturating_add(vertex_side)
}

/// What one scoring call charged before it allocated anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScorePlan {
    pub candidates: usize,
    pub face_direction_products: usize,
    pub vertex_direction_products: usize,
    pub section_breakpoints: usize,
    pub working_bytes: usize,
    pub candidate_tile: usize,
}

/// What it then executed.
///
/// Every field is **counted where the work happens** — one increment per
/// `n·u` written in the tile product, one per dot product taken in
/// [`project_vertices`], one per distinct breakpoint the section solver
/// resolved — and never copied from [`ScorePlan`].  A meter assigned from the
/// plan it is meant to check is an identity, not a receipt: it can only ever
/// agree, and `ChargeDrift` could then never fire on it.  The repo's own
/// precedent (FU-32 J5's routing meter) is a genuine independent count and this
/// is one too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScoreMeter {
    pub face_direction_products: usize,
    pub vertex_direction_products: usize,
    pub section_breakpoints: usize,
}

/// What one candidate's evaluation executed, counted as it ran.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CandidateWork {
    vertex_direction_products: usize,
    section_breakpoints: usize,
}

/// The peel-force bracket, evaluated at all four corners of
/// {low, high} pressure x {low, high} area exponent (D14). No corner is
/// preferred over another; [`ObjectiveWeights::registered`]'s score uses
/// [`Self::primary`] and [`crate::search`] checks the other three for
/// robustness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeelForceCorners {
    pub low_pressure_low_exponent_n: f64,
    pub low_pressure_high_exponent_n: f64,
    pub high_pressure_low_exponent_n: f64,
    pub high_pressure_high_exponent_n: f64,
}

impl PeelForceCorners {
    /// The corner the assembled `score` prices: LOW pressure (Manizani,
    /// FEP, ~2 mm/s), LOW exponent (porosity at constant envelope) — the
    /// literature review's own first-named anchor point
    /// (`LITERATURE-NOTES.md:813,826`). Not preferred as physics, just as the
    /// one corner a single scalar score must pick.
    #[must_use]
    pub const fn primary(self) -> f64 {
        self.low_pressure_low_exponent_n
    }

    /// All four corners as `(label, value)`, in a stable order, for a report
    /// or a flip-crossover search to walk without repeating the field list.
    #[must_use]
    pub const fn named(self) -> [(&'static str, f64); 4] {
        [
            (
                "low_pressure_low_exponent",
                self.low_pressure_low_exponent_n,
            ),
            (
                "low_pressure_high_exponent",
                self.low_pressure_high_exponent_n,
            ),
            (
                "high_pressure_low_exponent",
                self.high_pressure_low_exponent_n,
            ),
            (
                "high_pressure_high_exponent",
                self.high_pressure_high_exponent_n,
            ),
        ]
    }
}

/// The four terms and the assembled score for one candidate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CandidateTerms {
    pub support_area_cm2: f64,
    /// The exact peak cross-section, still computed and still reported —
    /// the peel-force term is a function of it — but no longer weighted
    /// directly into the score (LANE 3, 2026-09-02).
    pub peak_xsec_cm2: f64,
    /// `peak_xsec_cm2 * 100`, the raw mm² the peel-force bracket is
    /// evaluated at.
    pub peak_mm2: f64,
    pub peel_force_n: PeelForceCorners,
    /// `ObjectiveWeights::peel_points_per_newton * peel_force_n.primary()` —
    /// the term that actually enters `score`.
    pub peel_term: f64,
    pub height_mm: f64,
    pub scar_term: f64,
    /// `Σ_down(area_mm² × (fragility + display))` before the weight and the
    /// cm² conversion, carried so a report can show the measurement separately
    /// from its price. Equal to `fragility_raw_mm2 + display_raw_mm2` by
    /// construction.
    pub scar_raw_mm2: f64,
    /// `Σ_down(area_mm² × fragility)`: the quantity the scar term measured
    /// before the display-face field existed, published unchanged.
    /// **Fragility and display are two named quantities, not one blended
    /// number** (owner ruling, 2026-09-03), and this is the fragility half.
    pub fragility_raw_mm2: f64,
    /// `Σ_down(area_mm² × display)`: the display half, the INCREMENT the
    /// display-face field adds. Not scaled by fragility (2026-09-29; see the
    /// module doc). Zero exactly when no face that this pose turns downward
    /// carries display value, which is what makes `D = 0` reproduce the
    /// pre-display objective rather than approximate it.
    pub display_raw_mm2: f64,
    /// `Σ_model-landings(π r_seat² × (fragility + display))` of the landed
    /// face: the scar the SUPPORT FIELD leaves on the model, as opposed to the
    /// two halves above, which price the model's own down-facing area.
    ///
    /// **[`None`] is a stated absence, not a zero** — the same rule
    /// [`OrientFaceTable::display`] follows. It is `None` here for every
    /// candidate, because nothing this module measures can see a support
    /// field: only a PRODUCTION-fidelity sweep
    /// (`plinth_orient::demand::DemandFidelity::Production`) knows where the
    /// arms land, and the commit tier folds that measurement in afterwards
    /// (`crate::search::TermsRecord::repriced`). A candidate no production
    /// sweep reached therefore carries `None` all the way into the record,
    /// where it reads as "not measured" rather than "measured, and nothing
    /// landed".
    pub landing_scar_raw_mm2: Option<f64>,
    /// `Σ_tips(π r_tip² × (fragility + display))` of the face each arm's
    /// TIP carries — the mirror of [`Self::landing_scar_raw_mm2`], priced at
    /// the other end of the arm, and the reason a support field standing on
    /// the model's own delicate down-facing surface (not merely seated on
    /// it) enters the objective at all.
    ///
    /// **[`None`] is a stated absence, not a zero**, for the identical
    /// reason [`Self::landing_scar_raw_mm2`] is: this module cannot see a
    /// support field. It is `None` here for every candidate; only a
    /// PRODUCTION-fidelity sweep
    /// (`plinth_orient::demand::DemandFidelity::Production`) knows where an
    /// arm's tip carries, and the commit tier folds that measurement in
    /// afterwards (`crate::search::TermsRecord::repriced`).
    pub tip_scar_raw_mm2: Option<f64>,
    pub down_facing_faces: usize,
    pub score: f64,
}

/// **The per-model display-face map, summarised so it can be INSPECTED before
/// it steers anything** (owner ruling, 2026-09-03).
///
/// Everything here is a measurement of one `(mesh, LOD)` pair's display field,
/// read off the scoring table. It is a DISCLOSURE: nothing consumes it, no
/// gate compares it, and the field of record stays the digest-bound
/// [`plinth_mesh::DisplayField`] the table was assembled from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayFieldSummary {
    /// The digest of the field this summarises, so a summary can never be
    /// read against a different measurement.
    pub digest: String,
    pub faces: usize,
    pub total_area_mm2: f64,
    /// The plain per-face mean of D.
    pub mean: f64,
    /// The AREA-WEIGHTED mean of D, which is the figure comparable to the
    /// scar term: the scar fold weights every face by its area.
    pub area_weighted_mean: f64,
    /// Ten equal bins over `[0, 1]`, each holding the FRACTION OF TOTAL AREA
    /// whose D falls in that bin. Bin 0 is `[0, 0.1)`; the last bin is closed
    /// at 1.
    pub area_weighted_histogram: [f64; 10],
    /// The area-weighted mean of D over the faces whose AUTHORED normal points
    /// up (`n_z > 1/sqrt(2)`) and down (`n_z < -1/sqrt(2)`), the two numbers
    /// that say whether the authored up-signal actually separated this model.
    pub authored_up_area_weighted_mean: f64,
    pub authored_down_area_weighted_mean: f64,
    /// The eight faces with the highest D, in descending D then ascending
    /// face index, with each factor beside the product.
    pub top_faces: Vec<DisplayFaceSummary>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayFaceSummary {
    pub face: usize,
    pub display: f64,
    pub area_mm2: f64,
    pub authored_normal_z: f64,
}

/// How many faces [`DisplayFieldSummary::top_faces`] lists. A report width,
/// not a measurement: it cannot change a value, only how many of them a reader
/// sees, so it is a literal here rather than a registry entry.
const DISPLAY_SUMMARY_TOP_FACES: usize = 8;

impl OrientFaceTable {
    /// Summarise this table's display field, or `None` when no field was
    /// built — a stated absence, never a summary of zeros.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "one histogram bin index from a value the DisplayFaceValue contract has already \
                  clamped into [0, 1]; the floor is non-negative and below 10 by construction and \
                  the min(9) pins the closed end"
    )]
    pub fn display_summary(&self) -> Option<DisplayFieldSummary> {
        let digest = self.display_digest.clone()?;
        if self.display.len() != self.areas_mm2.len() {
            return None;
        }
        let axis = std::f64::consts::FRAC_1_SQRT_2;
        let mut total_area = 0.0_f64;
        let mut weighted = 0.0_f64;
        let mut plain = 0.0_f64;
        let mut histogram = [0.0_f64; 10];
        let (mut up_area, mut up_weighted) = (0.0_f64, 0.0_f64);
        let (mut down_area, mut down_weighted) = (0.0_f64, 0.0_f64);
        let mut ranked: Vec<(f64, usize)> = Vec::with_capacity(self.display.len());
        for (face, &display) in self.display.iter().enumerate() {
            let area = self.areas_mm2[face];
            total_area += area;
            weighted += area * display;
            plain += display;
            let bin = ((display * 10.0).floor().max(0.0) as usize).min(9);
            histogram[bin] += area;
            let normal_z = self.normals[face][2];
            if normal_z > axis {
                up_area += area;
                up_weighted += area * display;
            } else if normal_z < -axis {
                down_area += area;
                down_weighted += area * display;
            }
            ranked.push((display, face));
        }
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let scale = |value: f64, by: f64| if by > 0.0 { value / by } else { 0.0 };
        for bucket in &mut histogram {
            *bucket = scale(*bucket, total_area);
        }
        Some(DisplayFieldSummary {
            digest,
            faces: self.display.len(),
            total_area_mm2: total_area,
            mean: scale(plain, self.display.len() as f64),
            area_weighted_mean: scale(weighted, total_area),
            area_weighted_histogram: histogram,
            authored_up_area_weighted_mean: scale(up_weighted, up_area),
            authored_down_area_weighted_mean: scale(down_weighted, down_area),
            top_faces: ranked
                .into_iter()
                .take(DISPLAY_SUMMARY_TOP_FACES)
                .map(|(display, face)| DisplayFaceSummary {
                    face,
                    display,
                    area_mm2: self.areas_mm2[face],
                    authored_normal_z: self.normals[face][2],
                })
                .collect(),
        })
    }
}

/// A candidate excluded from ranking, with the reason it was excluded. Never a
/// sentinel score and never a silent skip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CandidateRefusal {
    pub candidate: usize,
    pub reason: CandidateRefusalReason,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CandidateRefusalReason {
    NonFiniteTerm { term: &'static str, value: f64 },
    NegativeSectionArea { peak_mm2: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CandidateOutcome {
    Scored(CandidateTerms),
    Refused(CandidateRefusal),
}

/// One batched scoring call's full result, in candidate order.
#[derive(Clone, Debug, PartialEq)]
pub struct ScoredCandidates {
    outcomes: Vec<CandidateOutcome>,
    plan: ScorePlan,
    meter: ScoreMeter,
}

impl ScoredCandidates {
    #[must_use]
    pub fn outcomes(&self) -> &[CandidateOutcome] {
        &self.outcomes
    }
    #[must_use]
    pub const fn plan(&self) -> ScorePlan {
        self.plan
    }
    #[must_use]
    pub const fn meter(&self) -> ScoreMeter {
        self.meter
    }
    /// Every candidate that was excluded from ranking, in candidate order.
    #[must_use]
    pub fn refusals(&self) -> Vec<CandidateRefusal> {
        self.outcomes
            .iter()
            .filter_map(|outcome| match outcome {
                CandidateOutcome::Refused(refusal) => Some(*refusal),
                CandidateOutcome::Scored(_) => None,
            })
            .collect()
    }
}

/// The orientation-invariant scoring table for one `(mesh, LOD)` pair.
///
/// Areas, unit normals, vertex positions and the delicacy field are built once
/// here and read by every candidate; the mesh is never rotated (D-4).
#[derive(Clone, Debug, PartialEq)]
pub struct OrientFaceTable {
    lod: String,
    mesh_fingerprint: String,
    verts: Vec<[f64; 3]>,
    tris: Vec<[u32; 3]>,
    areas_mm2: Vec<f64>,
    normals: Vec<[f64; 3]>,
    degenerate: Vec<bool>,
    delicacy: Vec<f64>,
    /// `None` when the field was never built because the scar weight is
    /// structurally zero — a stated absence, not a digest of zeros.
    delicacy_digest: Option<String>,
    /// The display-face field D, in source face order, or EMPTY when no field
    /// was built. An empty display vector is a stated absence and is read as
    /// `D = 0`, which is the pre-display objective exactly.
    display: Vec<f64>,
    /// `None` when no display field was built — the same stated-absence rule
    /// [`Self::delicacy_digest`] follows.
    display_digest: Option<String>,
    boundary_edges: usize,
    nonmanifold_edges: usize,
}

impl OrientFaceTable {
    /// Builds the table and measures its own delicacy field at this LOD.
    ///
    /// # Errors
    ///
    /// [`ScoreError::Delicacy`] when the field cannot be measured — deliberately
    /// a refusal rather than a zero field, which would be the positive assertion
    /// that nothing on the model is delicate; [`ScoreError::OpenBoundary`] when
    /// the welded mesh is not closed, which the exact section solver requires;
    /// [`ScoreError::Topology`] when the mesh cannot be welded at all.
    pub fn build(mesh: &Mesh, lod: impl Into<String>) -> Result<Self, ScoreError> {
        let lod = lod.into();
        let settings = DelicacySettings::production().map_err(ScoreError::Delicacy)?;
        let config = DelicacyBuildConfig::production(mesh.tris().len());
        let fragility =
            evaluate_delicacy(mesh, lod.clone(), settings, config).map_err(ScoreError::Delicacy)?;
        // The display-face field is measured HERE, on the same `(mesh, LOD)`
        // pair, in the mesh's AUTHORED frame — this table is orientation
        // invariant and the sweep never rotates a mesh (D-4), so "the frame
        // the table was built in" and "the frame the file was authored in"
        // are the same frame, structurally. There is no constructor anywhere
        // that could hand this table a field measured in a posed frame.
        let display_settings = DisplaySettings::production().map_err(ScoreError::Display)?;
        let display_config =
            DisplayBuildConfig::production(mesh.tris().len()).map_err(ScoreError::Display)?;
        let display = evaluate_display(mesh, lod.clone(), display_settings, display_config)
            .map_err(ScoreError::Display)?;
        Self::assemble(mesh, lod, Some(&fragility), Some(&display))
    }

    /// Builds the table with **no** delicacy field at all, for a sweep whose
    /// scar weight is structurally zero.
    ///
    /// `03-physics-and-constants.md` §3.8 requires that a zero scar weight
    /// reproduce the scar-free search *structurally rather than arithmetically*,
    /// and the 2026-08-10 producer ruling keeps that requirement in force.
    /// Building the field and then multiplying it by zero does not satisfy it:
    /// the field's own digest would still enter the search record, so a
    /// scar-free run's `search_digest` would move whenever the delicacy
    /// evaluator changed, and a delicacy failure would refuse a search that
    /// never wanted the number.  Here the field is simply not constructed, the
    /// digest is [`None`] rather than a hash of zeros, and `delicacy` is empty —
    /// which the scoring loop never indexes, because it hoists the scar branch
    /// out of the face pass.
    ///
    /// # Errors
    ///
    /// The closure and topology refusals of [`OrientFaceTable::build`].
    pub fn without_delicacy(mesh: &Mesh, lod: impl Into<String>) -> Result<Self, ScoreError> {
        Self::assemble(mesh, lod.into(), None, None)
    }

    /// Builds the table around a delicacy field that was already measured.
    ///
    /// The field must have been measured on **this** mesh at **this** LOD; both
    /// are checked, and a mismatch is a refusal.  That check is the whole reason
    /// this constructor is safe to expose: it is what makes the recorded Python
    /// defect inexpressible rather than merely discouraged.
    ///
    /// # Errors
    ///
    /// [`ScoreError::DelicacyLodMismatch`] when the field names another LOD;
    /// [`ScoreError::Delicacy`] when it was measured on another mesh; plus the
    /// closure and topology refusals of [`OrientFaceTable::build`].
    pub fn with_delicacy(
        mesh: &Mesh,
        lod: impl Into<String>,
        field: &DelicacyField,
    ) -> Result<Self, ScoreError> {
        Self::assemble(mesh, lod.into(), Some(field), None)
    }

    /// Builds the table around a fragility field AND a display-face field that
    /// were both already measured.
    ///
    /// Both are checked against this mesh and this LOD, for the same reason
    /// [`Self::with_delicacy`] checks one: a field measured on another object
    /// must be inexpressible here, not merely discouraged.
    ///
    /// # Errors
    ///
    /// The refusals of [`Self::with_delicacy`], plus
    /// [`ScoreError::DisplayLodMismatch`] when the display field names another
    /// LOD and [`ScoreError::Display`] when it was measured on another mesh.
    pub fn with_fields(
        mesh: &Mesh,
        lod: impl Into<String>,
        fragility: &DelicacyField,
        display: &DisplayField,
    ) -> Result<Self, ScoreError> {
        Self::assemble(mesh, lod.into(), Some(fragility), Some(display))
    }

    fn assemble(
        mesh: &Mesh,
        lod: String,
        field: Option<&DelicacyField>,
        display: Option<&DisplayField>,
    ) -> Result<Self, ScoreError> {
        if let Some(field) = field {
            if field.face_table().lod() != lod {
                return Err(ScoreError::DelicacyLodMismatch {
                    expected: lod,
                    actual: field.face_table().lod().to_string(),
                });
            }
            field.validate_mesh(mesh).map_err(ScoreError::Delicacy)?;
        }
        if let Some(display) = display {
            if display.face_table().lod() != lod {
                return Err(ScoreError::DisplayLodMismatch {
                    expected: lod,
                    actual: display.face_table().lod().to_string(),
                });
            }
            display.validate_mesh(mesh).map_err(ScoreError::Display)?;
        }

        // Welded topology, once. `WeldPolicy::ExactBits` is stated here rather
        // than taken from a caller for the same reason the delicacy evaluator
        // states it: under `WeldPolicy::None` an STL soup shares no vertices, so
        // every edge reads as a boundary and the closure check below would
        // refuse every model on disk.
        let _assemble_mark = geom_kernel::perf::stage("orient.table.assemble");
        let topology =
            build_face_topology(mesh, WeldPolicy::ExactBits).map_err(ScoreError::Topology)?;
        let counts = topology.counts();
        if counts.boundary_edges > 0 {
            // The section solver's shoelace identity assumes the section is a
            // union of closed loops. A hole breaks that, so an open LOD is
            // refused rather than answered with a plausible wrong area.
            //
            // **This precondition is wider than the design asked for, and the
            // width is recorded rather than implied.** Design §2.1 put the
            // closed-boundary floor on the *proxies*, where the LOD ladder can
            // catch it and answer with a finer rung. It is enforced here on
            // every LOD, including Exact — where there is no finer rung, so it
            // becomes `SearchError::ExactUnscorable` and the model is
            // unpreparable under the default `DenseSweep` policy while
            // `--orient pinned` still prepares it. That is a refusal class the
            // record did not have before; it is journalled with a corpus
            // reachability measurement (all eight corpus models weld closed) and
            // carried on the follow-ups ledger as FU-42.
            return Err(ScoreError::OpenBoundary {
                boundary_edges: counts.boundary_edges,
            });
        }

        let mut areas_mm2 = Vec::with_capacity(mesh.tris().len());
        let mut normals = Vec::with_capacity(mesh.tris().len());
        let mut degenerate = Vec::with_capacity(mesh.tris().len());
        for triangle in mesh.tris() {
            let a = mesh.verts()[triangle[0] as usize];
            let b = mesh.verts()[triangle[1] as usize];
            let c = mesh.verts()[triangle[2] as usize];
            let cross = vec3::cross(vec3::sub(b, a), vec3::sub(c, a));
            let length = vec3::norm(cross);
            if length.is_finite() && length > 0.0 {
                areas_mm2.push(0.5 * length);
                normals.push([cross[0] / length, cross[1] / length, cross[2] / length]);
                degenerate.push(false);
            } else {
                // A zero-area face has no outward direction and bounds nothing.
                // It carries no area into either masked fold and is skipped by
                // the section solver; the placeholder normal only keeps the
                // batched product well defined for every row.
                areas_mm2.push(0.0);
                normals.push([0.0, 0.0, 1.0]);
                degenerate.push(true);
            }
        }

        Ok(Self {
            lod,
            mesh_fingerprint: mesh_fingerprint(mesh).map_err(ScoreError::Topology)?,
            verts: mesh.verts().to_vec(),
            tris: mesh.tris().to_vec(),
            areas_mm2,
            normals,
            degenerate,
            delicacy: field.map_or_else(Vec::new, |field| {
                field
                    .face_values()
                    .iter()
                    .map(|value| value.weight())
                    .collect()
            }),
            delicacy_digest: field.map(|field| field.digest().to_string()),
            display: display.map_or_else(Vec::new, |display| {
                display
                    .face_values()
                    .iter()
                    .map(|value| value.weight())
                    .collect()
            }),
            display_digest: display.map(|display| display.digest().to_string()),
            boundary_edges: counts.boundary_edges,
            nonmanifold_edges: counts.nonmanifold_edges,
        })
    }

    #[must_use]
    pub fn lod(&self) -> &str {
        &self.lod
    }
    #[must_use]
    pub fn mesh_fingerprint(&self) -> &str {
        &self.mesh_fingerprint
    }
    /// The digest of the delicacy field this table was built around, or `None`
    /// when no field was built because the scar weight is structurally zero.
    #[must_use]
    pub fn delicacy_digest(&self) -> Option<&str> {
        self.delicacy_digest.as_deref()
    }
    /// The digest of the display-face field this table was built around, or
    /// `None` when no display field was built.
    #[must_use]
    pub fn display_digest(&self) -> Option<&str> {
        self.display_digest.as_deref()
    }
    /// The display-face field D in source face order, or an empty slice when
    /// no field was built.
    #[must_use]
    pub fn display(&self) -> &[f64] {
        &self.display
    }
    /// The fragility (delicacy) field in source face order, or an empty slice
    /// when no field was built — the same stated-absence rule
    /// [`Self::display`] follows.
    ///
    /// Published for the same reason `display` is: the model-landing scar
    /// (`plinth_sweep::model_landing`) weights each seat by the delicacy of
    /// the face it lands on, and it must read the field THIS table was scored
    /// against rather than re-evaluate one of its own.
    #[must_use]
    pub fn delicacy(&self) -> &[f64] {
        &self.delicacy
    }
    #[must_use]
    pub fn face_count(&self) -> usize {
        self.tris.len()
    }
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.verts.len()
    }
    #[must_use]
    pub const fn nonmanifold_edges(&self) -> usize {
        self.nonmanifold_edges
    }
    #[must_use]
    pub const fn boundary_edges(&self) -> usize {
        self.boundary_edges
    }
    /// Face areas in mm², in source face order.
    ///
    /// Exposed so the grid's rest-pose accumulator can weight face normals by
    /// the area they actually present to the plate, reading the very table that
    /// will score the poses it proposes rather than re-deriving a second face
    /// geometry pass over the same mesh.
    #[must_use]
    pub fn areas_mm2(&self) -> &[f64] {
        &self.areas_mm2
    }

    /// Unit outward normals, in source face order. A degenerate face carries a
    /// placeholder normal and a zero area, so an area-weighted read of this
    /// slice never sees it.
    #[must_use]
    pub fn normals(&self) -> &[[f64; 3]] {
        &self.normals
    }

    /// Total surface area, in mm² — the scale every area term is measured
    /// against.
    #[must_use]
    pub fn surface_area_mm2(&self) -> f64 {
        self.areas_mm2.iter().fold(0.0, |sum, area| sum + area)
    }
}

/// Scores every candidate against one table in one batched call.
///
/// # Errors
///
/// [`ScoreError::WorkBudgetExceeded`] when a deterministic work budget is
/// reached — computed exactly from counts before any allocation, and never
/// answered by shrinking the candidate set; [`ScoreError::ChargeDrift`] when the
/// executed work does not match what was charged.
pub fn score_candidates(
    table: &OrientFaceTable,
    candidates: &[UpDirection],
    weights: ObjectiveWeights,
    limits: OrientSearchLimits,
) -> Result<ScoredCandidates, ScoreError> {
    let plan = charge(table, candidates.len(), limits)?;
    if candidates.is_empty() {
        return Ok(ScoredCandidates {
            outcomes: Vec::new(),
            plan,
            meter: ScoreMeter {
                face_direction_products: 0,
                vertex_direction_products: 0,
                section_breakpoints: 0,
            },
        });
    }

    let faces = table.tris.len();
    let down_limit = weights.down_limit();
    let mut outcomes = Vec::with_capacity(candidates.len());
    let mut executed = ScoreMeter {
        face_direction_products: 0,
        vertex_direction_products: 0,
        section_breakpoints: 0,
    };

    for tile in candidates.chunks(plan.candidate_tile) {
        // The one batched product: faces outer, candidates inner, so the normal
        // array streams once per tile instead of once per candidate.
        let mut nz = vec![0.0_f64; faces * tile.len()];
        // Counted where it executes, one per product written, so the receipt is
        // a measurement of this loop rather than a restatement of the charge.
        let mut tile_face_products = 0_usize;
        for (face, normal) in table.normals.iter().enumerate() {
            let row = &mut nz[face * tile.len()..(face + 1) * tile.len()];
            for (slot, candidate) in row.iter_mut().zip(tile) {
                let u = candidate.get();
                *slot = vec3::dot(*normal, u);
                tile_face_products += 1;
            }
        }
        executed.face_direction_products = executed
            .face_direction_products
            .checked_add(tile_face_products)
            .ok_or(ScoreError::ArithmeticOverflow {
                section: "face-direction products",
            })?;

        // Index-parallel map-collect across candidates: every output lands in
        // the slot of its own index, so the tile's results are bit-identical at
        // any thread count.
        // The kernel's cached explicit pool at the workspace thread count,
        // never rayon's global pool: a one-shot global pool makes the thread
        // sweep inert (06-conventions §3). Caching it there rather than
        // building one per call keeps that property — the pool is still
        // explicit and still re-reads `PLINTH_THREADS` — while removing a
        // `pthread_create` storm from a function called once per tile batch.
        let tile_outcomes: Vec<(CandidateOutcome, CandidateWork)> =
            geom_kernel::run_in_pool(|| {
                tile.par_iter()
                    .enumerate()
                    .map(|(column, candidate)| {
                        score_one(
                            table,
                            *candidate,
                            column,
                            tile.len(),
                            &nz,
                            weights,
                            down_limit,
                        )
                    })
                    .collect()
            })
            .map_err(|error| ScoreError::Pool(error.to_string()))?;
        for (outcome, work) in tile_outcomes {
            executed.vertex_direction_products = executed
                .vertex_direction_products
                .checked_add(work.vertex_direction_products)
                .ok_or(ScoreError::ArithmeticOverflow {
                    section: "vertex-direction products",
                })?;
            executed.section_breakpoints = executed
                .section_breakpoints
                .checked_add(work.section_breakpoints)
                .ok_or(ScoreError::ArithmeticOverflow {
                    section: "section breakpoints",
                })?;
            outcomes.push(outcome);
        }
    }

    // Charged and metered, both stated, and now all three are checkable: the
    // two product counters are exhaustive passes and must land exactly on their
    // charge, while breakpoints are charged at their vertex-count bound and
    // metered at the distinct heights actually resolved, so under-running is
    // expected there and over-running is drift.
    for (charged, executed, exact, what) in [
        (
            plan.face_direction_products,
            executed.face_direction_products,
            true,
            "face-direction products",
        ),
        (
            plan.vertex_direction_products,
            executed.vertex_direction_products,
            true,
            "vertex-direction products",
        ),
        (
            plan.section_breakpoints,
            executed.section_breakpoints,
            false,
            "section breakpoints",
        ),
    ] {
        let drifted = if exact {
            executed != charged
        } else {
            executed > charged
        };
        if drifted {
            return Err(ScoreError::ChargeDrift {
                charged,
                executed,
                what,
            });
        }
    }

    Ok(ScoredCandidates {
        outcomes,
        plan,
        meter: executed,
    })
}

/// Charges the whole call from counts alone, before one working byte exists.
fn charge(
    table: &OrientFaceTable,
    candidates: usize,
    limits: OrientSearchLimits,
) -> Result<ScorePlan, ScoreError> {
    if candidates > limits.max_candidates {
        return Err(ScoreError::WorkBudgetExceeded {
            charge: candidates,
            cap: limits.max_candidates,
            what: "candidates",
        });
    }
    let faces = table.tris.len();
    let verts = table.verts.len();
    let product = |left: usize, right: usize, section: &'static str| {
        left.checked_mul(right)
            .ok_or(ScoreError::ArithmeticOverflow { section })
    };
    let face_direction_products = product(faces, candidates, "face-direction products")?;
    let vertex_direction_products = product(
        product(verts, 3, "vertex-direction products")?,
        candidates,
        "vertex-direction products",
    )?;
    let section_breakpoints = product(verts, candidates, "section breakpoints")?;
    for (charge, cap, what) in [
        (
            face_direction_products,
            limits.max_candidate_face_products,
            "face-direction products",
        ),
        (
            vertex_direction_products,
            limits.max_candidate_vertex_products,
            "vertex-direction products",
        ),
        (
            section_breakpoints,
            limits.max_section_breakpoints,
            "section breakpoints",
        ),
    ] {
        if charge > cap {
            return Err(ScoreError::WorkBudgetExceeded { charge, cap, what });
        }
    }

    // The byte guard sizes the candidate tile; it refuses only when a single
    // candidate does not fit, because a tile boundary can never change a result
    // (no fold crosses one).
    let per_candidate = per_candidate_bytes(faces, verts);
    if per_candidate > limits.max_working_bytes {
        return Err(ScoreError::WorkBudgetExceeded {
            charge: per_candidate,
            cap: limits.max_working_bytes,
            what: "candidate working bytes",
        });
    }
    let candidate_tile = (limits.max_working_bytes / per_candidate.max(1))
        .max(1)
        .min(candidates.max(1));
    Ok(ScorePlan {
        candidates,
        face_direction_products,
        vertex_direction_products,
        section_breakpoints,
        working_bytes: per_candidate.saturating_mul(candidate_tile),
        candidate_tile,
    })
}

/// The four terms for one candidate. Every fold here is serial in index order.
fn score_one(
    table: &OrientFaceTable,
    candidate: UpDirection,
    column: usize,
    stride: usize,
    nz: &[f64],
    weights: ObjectiveWeights,
    down_limit: f64,
) -> (CandidateOutcome, CandidateWork) {
    let index = column;

    // Support area and scar share one masked pass over faces. The scar branch is
    // hoisted out of the loop rather than tested per face: at a zero weight the
    // delicacy field is not read *at all*, so a scar-free score is independent of
    // it structurally, not by multiplying by zero.
    let mut support_area_mm2 = 0.0_f64;
    let mut fragility_raw_mm2 = 0.0_f64;
    let mut display_raw_mm2 = 0.0_f64;
    let mut down_facing_faces = 0_usize;
    if weights.scar == 0.0 {
        for face in 0..table.tris.len() {
            if nz[face * stride + column] < down_limit {
                support_area_mm2 += table.areas_mm2[face];
                down_facing_faces += 1;
            }
        }
    } else if table.display.is_empty() {
        // No display field was built for this table. The absence is read as
        // `D = 0` and is spelled as a SEPARATE loop rather than
        // as an index into an empty vector, for the same structural reason the
        // scar-free branch is hoisted: the pre-display objective must be
        // reproduced by not reading a field, not by multiplying one by zero.
        for face in 0..table.tris.len() {
            if nz[face * stride + column] < down_limit {
                support_area_mm2 += table.areas_mm2[face];
                fragility_raw_mm2 += table.areas_mm2[face] * table.delicacy[face];
                down_facing_faces += 1;
            }
        }
    } else {
        // `Σ_down(area × (fragility + D))`, accumulated as its two named
        // halves. The two harms ADD (2026-09-29): a support that would break
        // a face and a support that would spoil its look are two independent
        // harms, and a sturdy display face is spoiled exactly as much as a
        // fragile one. **The association is pinned to reproduce the
        // pre-display fold bit for bit when D is zero**: the fragility half
        // is the identical `area * delicacy` product in the identical order,
        // and the display half is a separate sum that adds `0.0` per face
        // when the field is zero. Distributing `(fragility + D)` inside the
        // multiply instead would round differently and could move an argmin
        // at 650 candidates for a field that says nothing.
        for face in 0..table.tris.len() {
            if nz[face * stride + column] < down_limit {
                support_area_mm2 += table.areas_mm2[face];
                fragility_raw_mm2 += table.areas_mm2[face] * table.delicacy[face];
                display_raw_mm2 += table.areas_mm2[face] * table.display[face];
                down_facing_faces += 1;
            }
        }
    }
    let scar_raw_mm2 = fragility_raw_mm2 + display_raw_mm2;

    let Projection {
        plane,
        heights,
        height_mm,
        lowest: _,
        vertex_direction_products,
    } = project_vertices(table, candidate);

    let profile = build_profile(&table.tris, &table.degenerate, &plane, &heights);
    let work = CandidateWork {
        vertex_direction_products,
        section_breakpoints: profile.breakpoint_count(),
    };
    let peak_mm2 = match profile.peak_mm2() {
        Ok(peak) => peak,
        Err(refusal) => match refusal {
            SectionRefusal::NonFinite => {
                return (
                    CandidateOutcome::Refused(CandidateRefusal {
                        candidate: index,
                        reason: CandidateRefusalReason::NonFiniteTerm {
                            term: "peak_xsec_cm2",
                            value: f64::NAN,
                        },
                    }),
                    work,
                )
            }
            SectionRefusal::NegativeArea { peak_mm2 } => {
                return (
                    CandidateOutcome::Refused(CandidateRefusal {
                        candidate: index,
                        reason: CandidateRefusalReason::NegativeSectionArea { peak_mm2 },
                    }),
                    work,
                )
            }
        },
    };

    let support_area_cm2 = canonical_zero(support_area_mm2 / 100.0);
    let peak_xsec_cm2 = canonical_zero(peak_mm2 / 100.0);
    let height_mm = canonical_zero(height_mm);
    let scar_term = canonical_zero(weights.scar * scar_raw_mm2 / 100.0);
    let peel_force_n = weights.peel_force_corners(peak_mm2);
    let peel_term = canonical_zero(weights.peel_points_per_newton * peel_force_n.primary());
    for (term, value) in [
        ("support_area_cm2", support_area_cm2),
        ("peak_xsec_cm2", peak_xsec_cm2),
        ("height_mm", height_mm),
        ("scar_term", scar_term),
        ("peel_term", peel_term),
        (
            "peel_force_n.low_pressure_low_exponent_n",
            peel_force_n.low_pressure_low_exponent_n,
        ),
        (
            "peel_force_n.low_pressure_high_exponent_n",
            peel_force_n.low_pressure_high_exponent_n,
        ),
        (
            "peel_force_n.high_pressure_low_exponent_n",
            peel_force_n.high_pressure_low_exponent_n,
        ),
        (
            "peel_force_n.high_pressure_high_exponent_n",
            peel_force_n.high_pressure_high_exponent_n,
        ),
    ] {
        if !value.is_finite() {
            return (
                CandidateOutcome::Refused(CandidateRefusal {
                    candidate: index,
                    reason: CandidateRefusalReason::NonFiniteTerm { term, value },
                }),
                work,
            );
        }
    }

    // The pinned association order. Re-associating is a different set of
    // rounding decisions and therefore, at 650 candidates, potentially a
    // different argmin. `peel_term` occupies exactly the slot the retired
    // `w_peak * peak_xsec_cm2` term held (LANE 3, 2026-09-02).
    let score = ((weights.support_area_cm2 * support_area_cm2 + peel_term) + weights.height_mm * height_mm)
        + scar_term;
    if !score.is_finite() {
        return (
            CandidateOutcome::Refused(CandidateRefusal {
                candidate: index,
                reason: CandidateRefusalReason::NonFiniteTerm {
                    term: "score",
                    value: score,
                },
            }),
            work,
        );
    }

    (
        CandidateOutcome::Scored(CandidateTerms {
            support_area_cm2,
            peak_xsec_cm2,
            peak_mm2: canonical_zero(peak_mm2),
            peel_force_n,
            peel_term,
            height_mm,
            scar_term,
            scar_raw_mm2: canonical_zero(scar_raw_mm2),
            fragility_raw_mm2: canonical_zero(fragility_raw_mm2),
            display_raw_mm2: canonical_zero(display_raw_mm2),
            // Structurally absent: see the field's own doc. No sweep has run.
            landing_scar_raw_mm2: None,
            // Structurally absent for the same reason: no sweep has run, so
            // nothing has a tip to carry a face.
            tip_scar_raw_mm2: None,
            down_facing_faces,
            score: canonical_zero(score),
        }),
        work,
    )
}

/// One candidate's vertex-side projections.
struct Projection {
    /// Coordinates in the candidate's in-plane basis, the frame the section
    /// areas are measured in.
    plane: Vec<[f64; 2]>,
    /// Heights along the candidate direction, shifted so the lowest is zero.
    heights: Vec<f64>,
    height_mm: f64,
    /// The unshifted height of the lowest vertex, so an absolute query height
    /// can be converted into the shifted frame.
    lowest: f64,
    /// Dot products actually taken here, counted one per product rather than
    /// restated from the caller's charge.
    vertex_direction_products: usize,
}

/// Three whole-array passes over the vertices, one per row of the candidate's
/// own rotation. No mesh is rotated and nothing is materialized per face.
fn project_vertices(table: &OrientFaceTable, candidate: UpDirection) -> Projection {
    let u = candidate.get();
    let rotation = candidate.rotation_to_build_frame();
    let mut products = 0_usize;
    let mut heights = Vec::with_capacity(table.verts.len());
    let mut lowest = f64::INFINITY;
    let mut highest = f64::NEG_INFINITY;
    for vertex in &table.verts {
        let height = vec3::dot(*vertex, u);
        products += 1;
        if height < lowest {
            lowest = height;
        }
        if height > highest {
            highest = height;
        }
        heights.push(height);
    }
    let height_mm = highest - lowest;
    for height in &mut heights {
        *height -= lowest;
    }
    let mut plane = Vec::with_capacity(table.verts.len());
    for vertex in &table.verts {
        plane.push([
            vec3::dot(*vertex, rotation[0]),
            vec3::dot(*vertex, rotation[1]),
        ]);
        products += 2;
    }
    Projection {
        plane,
        heights,
        height_mm,
        lowest,
        vertex_direction_products: products,
    }
}

/// The exact cross-section area, in mm², at one absolute height along a
/// candidate direction.
///
/// This is the same piecewise-quadratic function the peak term takes its
/// supremum of, evaluated rather than maximized.  It is a **diagnostic and
/// calibration** entry point — it is what lets the recorded
/// `peak_exact − peak_legacy_stepped` bias be measured per model rather than
/// asserted, and what the section oracle test checks against the geometry
/// kernel's own independent plane-slice batch.  Nothing on the scoring path
/// calls it.
///
/// `height_mm` is measured in the candidate's own build frame, i.e. `v·u`, the
/// same quantity `(R·v)_z` yields once the winner is applied.
///
/// # Errors
///
/// [`ScoreError::NonFiniteSection`] when the query height or the evaluated area
/// is not finite.
pub fn section_area_mm2_at(
    table: &OrientFaceTable,
    candidate: UpDirection,
    height_mm: f64,
) -> Result<f64, ScoreError> {
    if !height_mm.is_finite() {
        return Err(ScoreError::NonFiniteSection { height_mm });
    }
    let projection = project_vertices(table, candidate);
    let profile = build_profile(
        &table.tris,
        &table.degenerate,
        &projection.plane,
        &projection.heights,
    );
    profile
        .area_at(height_mm - projection.lowest)
        .map_err(|_| ScoreError::NonFiniteSection { height_mm })
}

const fn canonical_zero(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value
    }
}

#[derive(Clone, Debug, PartialEq, Error)]
pub enum ScoreError {
    #[error("registered orientation constants are unavailable: {0}")]
    Constants(String),
    #[error("candidate direction {direction:?} is not a finite nonzero vector")]
    InvalidDirection { direction: [f64; 3] },
    #[error("delicacy field names LOD {actual}, but the table is scoring LOD {expected}")]
    DelicacyLodMismatch { expected: String, actual: String },
    #[error("orientation delicacy field is unavailable: {0}")]
    Delicacy(DelicacyError),
    #[error("display field names LOD {actual}, but the table is scoring LOD {expected}")]
    DisplayLodMismatch { expected: String, actual: String },
    #[error("orientation display-face field is unavailable: {0}")]
    Display(DisplayError),
    #[error("orientation mesh topology error: {0}")]
    Topology(TopologyError),
    #[error(
        "the exact section solver requires a closed surface; this LOD has {boundary_edges} boundary edges"
    )]
    OpenBoundary { boundary_edges: usize },
    #[error("orientation scoring {what} charges {charge} against a {cap} cap")]
    WorkBudgetExceeded {
        charge: usize,
        cap: usize,
        what: &'static str,
    },
    #[error("orientation scoring executed {executed} {what} against a charge of {charged}")]
    ChargeDrift {
        charged: usize,
        executed: usize,
        what: &'static str,
    },
    #[error("orientation scoring overflowed while charging {section}")]
    ArithmeticOverflow { section: &'static str },
    #[error("the section area at height {height_mm} mm is not finite")]
    NonFiniteSection { height_mm: f64 },
    #[error("orientation scoring could not build its thread pool: {0}")]
    Pool(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_mesh::{make_box_default, make_thin_fin};

    fn cube_table() -> (Mesh, OrientFaceTable) {
        let mesh = make_box_default().expect("cube fixture");
        let table = OrientFaceTable::build(&mesh, "unit-lod").expect("scoring table");
        (mesh, table)
    }

    fn scored(table: &OrientFaceTable, weights: ObjectiveWeights) -> Vec<CandidateTerms> {
        let candidates = [
            UpDirection::MODEL_UP,
            UpDirection::new([0.4, -0.3, 0.6]).expect("unit direction"),
            UpDirection::new([0.0, 0.0, -1.0]).expect("antipode"),
        ];
        let limits = OrientSearchLimits::production();
        let outcomes =
            score_candidates(table, &candidates, weights, limits).expect("scoring must not refuse");
        outcomes
            .outcomes()
            .iter()
            .map(|outcome| match outcome {
                CandidateOutcome::Scored(terms) => *terms,
                CandidateOutcome::Refused(refusal) => panic!("refused: {refusal:?}"),
            })
            .collect()
    }

    #[test]
    fn the_build_frame_rotation_is_a_rotation_and_pins_the_antipode() {
        for direction in [
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
            [1.0, 0.0, 0.0],
            [0.3, -0.7, 0.5],
            [1e-18, 0.0, -1.0],
        ] {
            let up = UpDirection::new(direction).expect("unit direction");
            let rotation = up.rotation_to_build_frame();
            // Row 2 is the direction itself, so `(R·v)_z` is exactly `v·u` and
            // the height term and the section frame cannot disagree.
            for (component, expected) in rotation[2].iter().zip(up.get()) {
                assert!((component - expected).abs() <= 1e-15, "row 2 must be u");
            }
            for left in 0..3 {
                for right in 0..3 {
                    let dot: f64 = (0..3).map(|k| rotation[left][k] * rotation[right][k]).sum();
                    let expected = if left == right { 1.0 } else { 0.0 };
                    assert!(
                        (dot - expected).abs() <= 1e-12,
                        "rows must be orthonormal, got {dot} for ({left},{right})"
                    );
                }
            }
            let determinant = rotation[0][0]
                * (rotation[1][1] * rotation[2][2] - rotation[1][2] * rotation[2][1])
                - rotation[0][1]
                    * (rotation[1][0] * rotation[2][2] - rotation[1][2] * rotation[2][0])
                + rotation[0][2]
                    * (rotation[1][0] * rotation[2][1] - rotation[1][1] * rotation[2][0]);
            assert!(
                (determinant - 1.0).abs() <= 1e-12,
                "a reflection is not a rotation, got {determinant}"
            );
        }
        // The one pinned convention: the exact antipode is 180° about +X.
        assert_eq!(
            UpDirection::new([0.0, 0.0, -1.0])
                .expect("antipode")
                .rotation_to_build_frame(),
            [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]]
        );
    }

    /// **T4.** A zero scar weight does not multiply the delicacy field by zero —
    /// it never reads it.  Poisoning every face to maximal delicacy leaves a
    /// scar-free score bit-identical, and the registered weight moves it, so the
    /// independence is structural rather than arithmetic.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn a_zero_scar_weight_never_reads_the_delicacy_field() {
        let (_mesh, table) = cube_table();
        let mut poisoned = table.clone();
        poisoned.delicacy = vec![1.0; table.face_count()];

        let scar_free = ObjectiveWeights::registered()
            .expect("registered objective")
            .scar_free();
        for (clean, poison) in scored(&table, scar_free)
            .iter()
            .zip(scored(&poisoned, scar_free))
        {
            assert_eq!(clean.score.to_bits(), poison.score.to_bits());
            assert_eq!(clean.scar_raw_mm2, 0.0);
            assert_eq!(poison.scar_raw_mm2, 0.0);
        }

        // The registered weight does read it, so the test above is not vacuous.
        let registered = ObjectiveWeights::registered().expect("registered objective");
        let clean = scored(&table, registered);
        let poison = scored(&poisoned, registered);
        assert!(poison[0].score > clean[0].score);
        assert!(poison[0].scar_raw_mm2 > clean[0].scar_raw_mm2);
    }

    /// D-4: a field measured elsewhere cannot be scored here. The Python defect
    /// — delicacy from one mesh, areas from another — is inexpressible, not
    /// merely discouraged.
    #[test]
    fn a_delicacy_field_from_another_lod_or_another_mesh_is_refused() {
        let mesh = make_box_default().expect("cube fixture");
        let settings = DelicacySettings::production().expect("production settings");
        let config = DelicacyBuildConfig::production(mesh.tris().len());
        let field = evaluate_delicacy(&mesh, "l1", settings, config).expect("delicacy field");

        assert!(matches!(
            OrientFaceTable::with_delicacy(&mesh, "l2", &field),
            Err(ScoreError::DelicacyLodMismatch { .. })
        ));
        let other = make_thin_fin().expect("thin fin fixture");
        assert!(matches!(
            OrientFaceTable::with_delicacy(&other, "l1", &field),
            Err(ScoreError::Delicacy(_))
        ));
        assert!(OrientFaceTable::with_delicacy(&mesh, "l1", &field).is_ok());
    }

    /// The production budget is a real bound, not an identity.
    ///
    /// As first landed every cap was derived from the counts it was then
    /// compared against — `faces × candidates` against
    /// `max(2^28, faces × 2048)` with `candidates ≤ 653` — so every comparison
    /// reduced to `n·a ≤ n·b` with `a < b` and **no budget could fire for any
    /// mesh**. That defect is invisible to a test that only drives an
    /// overridden cap, because an overridden cap is by definition independent.
    /// So this asserts the two things that make the shipped numbers a bound:
    /// each is the stated `next_power_of_two(2 × worst case)`, and each is
    /// exceeded by a mesh large enough to deserve refusing.
    #[test]
    fn the_production_budget_is_independent_of_the_mesh_it_charges() {
        let limits = OrientSearchLimits::production();
        let faces = OrientSearchLimits::LARGEST_PRODUCTION_FACES;
        let verts = OrientSearchLimits::LARGEST_PRODUCTION_VERTS;
        let candidates = OrientSearchLimits::MAX_CANDIDATES;

        // The stated rule, evaluated rather than trusted.
        assert_eq!(
            limits.max_candidate_face_products,
            (2 * faces * candidates).next_power_of_two()
        );
        assert_eq!(
            limits.max_candidate_vertex_products,
            (2 * verts * 3 * candidates).next_power_of_two()
        );
        assert_eq!(
            limits.max_section_breakpoints,
            (2 * verts * candidates).next_power_of_two()
        );
        assert_eq!(
            limits.max_working_bytes,
            (4 * per_candidate_bytes(faces, verts)).next_power_of_two()
        );

        // The largest model this workspace can run fits, with headroom.
        assert!(faces * candidates <= limits.max_candidate_face_products);
        assert!(verts * 3 * candidates <= limits.max_candidate_vertex_products);
        assert!(verts * candidates <= limits.max_section_breakpoints);
        assert!(per_candidate_bytes(faces, verts) <= limits.max_working_bytes);

        // And a mesh eight times its size does not, which is what "cap" means.
        // Eight rather than four because the byte ceiling carries the tile
        // width's 4× rather than the usual 2×, so its headroom is ~6.6× where
        // the three product ceilings are 2.2–3.3×; a single multiplier that
        // clears all four states the loosest of them honestly.
        let huge_faces = faces * 8;
        let huge_verts = verts * 8;
        assert!(huge_faces * candidates > limits.max_candidate_face_products);
        assert!(huge_verts * 3 * candidates > limits.max_candidate_vertex_products);
        assert!(huge_verts * candidates > limits.max_section_breakpoints);
        assert!(per_candidate_bytes(huge_faces, huge_verts) > limits.max_working_bytes);
    }

    /// **T11.** Every budget refuses naming both numbers, before any working
    /// buffer exists, and never by trimming the candidate set to fit.
    #[test]
    fn every_work_budget_refuses_naming_both_numbers() {
        let (_mesh, table) = cube_table();
        let weights = ObjectiveWeights::registered().expect("registered objective");
        let candidates = vec![UpDirection::MODEL_UP; 8];
        let base = OrientSearchLimits::production();

        let cases = [
            (
                OrientSearchLimits {
                    max_candidates: 4,
                    ..base
                },
                "candidates",
            ),
            (
                OrientSearchLimits {
                    max_candidate_face_products: 4,
                    ..base
                },
                "face-direction products",
            ),
            (
                OrientSearchLimits {
                    max_candidate_vertex_products: 4,
                    ..base
                },
                "vertex-direction products",
            ),
            (
                OrientSearchLimits {
                    max_section_breakpoints: 4,
                    ..base
                },
                "section breakpoints",
            ),
            (
                OrientSearchLimits {
                    max_working_bytes: 8,
                    ..base
                },
                "candidate working bytes",
            ),
        ];
        for (limits, expected) in cases {
            match score_candidates(&table, &candidates, weights, limits) {
                Err(ScoreError::WorkBudgetExceeded { charge, cap, what }) => {
                    assert_eq!(what, expected);
                    assert!(charge > cap, "a refusal must state a demand over its cap");
                }
                other => panic!("expected a {expected} refusal, got {other:?}"),
            }
        }
    }

    /// A tile boundary is a memory decision, never a numeric one: no fold
    /// crosses one, so a one-candidate tile and a whole-batch tile agree bit for
    /// bit.
    #[test]
    fn candidate_tiling_cannot_move_a_score() {
        let (_mesh, table) = cube_table();
        let weights = ObjectiveWeights::registered().expect("registered objective");
        let candidates: Vec<UpDirection> = (0..16)
            .map(|step| {
                let angle = std::f64::consts::TAU * f64::from(step) / 16.0;
                UpDirection::new([angle.cos(), angle.sin(), 0.35]).expect("unit direction")
            })
            .collect();
        let wide = OrientSearchLimits::production();
        let narrow = OrientSearchLimits {
            max_working_bytes: per_candidate_bytes(table.face_count(), table.vertex_count()),
            ..wide
        };

        let wide_scores = score_candidates(&table, &candidates, weights, wide).expect("wide tile");
        let narrow_scores =
            score_candidates(&table, &candidates, weights, narrow).expect("narrow tile");
        assert_eq!(narrow_scores.plan().candidate_tile, 1);
        assert!(wide_scores.plan().candidate_tile > 1);
        assert_eq!(wide_scores.outcomes(), narrow_scores.outcomes());

        // Charged and executed are both stated, and the meter is checked
        // against counts derived here from the mesh rather than against the
        // plan it is supposed to audit — a meter compared only with its own
        // charge proves nothing about what ran.
        let expected_face_products = table.face_count() * candidates.len();
        let expected_vertex_products = table.vertex_count() * 3 * candidates.len();
        for scores in [&wide_scores, &narrow_scores] {
            let meter = scores.meter();
            assert_eq!(meter.face_direction_products, expected_face_products);
            assert_eq!(meter.vertex_direction_products, expected_vertex_products);
            assert!(meter.section_breakpoints <= scores.plan().section_breakpoints);
            assert!(meter.section_breakpoints > 0);
            assert_eq!(scores.plan().candidates, candidates.len());
        }
        // The tile width is a memory decision and moves no counter.
        assert_eq!(wide_scores.meter(), narrow_scores.meter());
    }

    /// A candidate a hair off the antipode is still a rotation.
    ///
    /// The closed form divides by `1 + c`, and near `−ẑ` that quantity is at
    /// the scale of `c`'s own rounding error, so reading it off `c` alone
    /// produced a matrix whose rows were not unit and not orthogonal — the
    /// normalized `[2e-8, 0, −1]` came out with a row-1 diagonal near 0.8. Rest
    /// poses are area-weighted `−n` aggregates and can land arbitrarily close
    /// to `−ẑ` on a real mesh, and the winner's matrix is applied to production
    /// geometry at emit, so this is a correctness fence and not a curiosity.
    #[test]
    fn a_near_antipode_direction_still_yields_an_orthonormal_rotation() {
        for offset in [2e-8, 1e-9, 1e-12, 1e-16, 1e-30, 0.0] {
            for direction in [
                [offset, 0.0, -1.0],
                [0.0, offset, -1.0],
                [offset, offset, -1.0],
            ] {
                let up = UpDirection::new(direction).expect("unit direction");
                let rotation = up.rotation_to_build_frame();
                for left in 0..3 {
                    for right in 0..3 {
                        let dot: f64 = (0..3).map(|k| rotation[left][k] * rotation[right][k]).sum();
                        let expected = if left == right { 1.0 } else { 0.0 };
                        assert!(
                            (dot - expected).abs() <= 1e-12,
                            "offset {offset} broke orthonormality: ({left},{right}) = {dot}"
                        );
                    }
                }
                // Row 2 is still the direction itself, so the height term and
                // the section frame cannot disagree.
                for (component, expected) in rotation[2].iter().zip(up.get()) {
                    assert!((component - expected).abs() <= 1e-15);
                }
            }
        }
    }

    /// The sweep's "down-facing" is the support engine's, by construction: the
    /// same registered constant through the same expression
    /// (`crates/plinth-support/src/lib.rs:814`), applied with the same strict
    /// `<`.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn the_down_facing_cutoff_is_the_support_engines_own_expression() {
        let weights = ObjectiveWeights::registered().expect("registered objective");
        assert_eq!(weights.overhang_angle_deg(), 45.0);
        assert_eq!(
            weights.down_limit().to_bits(),
            (-weights.overhang_angle_deg().to_radians().sin()).to_bits()
        );
        // At 45° this is the legacy −0.707…, to more digits than the legacy
        // literal carried.
        assert!((weights.down_limit() + std::f64::consts::FRAC_1_SQRT_2).abs() <= 1e-15);
    }

    /// The registered objective is exactly §3.8's, read from the one registry.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn the_objective_weights_are_the_registered_placeholders() {
        let weights = ObjectiveWeights::registered().expect("registered objective");
        assert_eq!(weights.support_area_cm2(), 0.5);
        assert_eq!(weights.height_mm(), 0.3);
        assert_eq!(weights.scar(), 0.4);
        assert_eq!(weights.scar_free().scar(), 0.0);
        assert_eq!(weights.peel_points_per_newton(), 1.0);
        assert_eq!(weights.peel_reference_area_mm2(), 314.0);
        assert_eq!(weights.peel_pressure_pa_low(), 9000.0);
        assert_eq!(weights.peel_pressure_pa_high(), 19600.0);
        assert_eq!(weights.peel_area_exponent_low(), 0.24);
        assert_eq!(weights.peel_area_exponent_high(), 0.46);
    }

    /// LANE 3, 2026-09-02: `F(A) = c*A^n` at the registered reference point
    /// reproduces the literature review's own worked arithmetic
    /// (`docs/plans/slice-sweep-core-2026-09-02.md` §12): evaluated AT
    /// `A = A_ref` the bracket collapses to `P_ref * A_ref` in Newtons,
    /// independent of which exponent corner is used.
    #[test]
    fn the_peel_force_bracket_collapses_to_pressure_times_area_at_the_reference_point() {
        let weights = ObjectiveWeights::registered().expect("registered objective");
        let corners = weights.peel_force_corners(weights.peel_reference_area_mm2());
        let low_pressure_n = weights.peel_pressure_pa_low() * weights.peel_reference_area_mm2() * 1.0e-6;
        let high_pressure_n = weights.peel_pressure_pa_high() * weights.peel_reference_area_mm2() * 1.0e-6;
        assert!((corners.low_pressure_low_exponent_n - low_pressure_n).abs() <= 1e-9);
        assert!((corners.low_pressure_high_exponent_n - low_pressure_n).abs() <= 1e-9);
        assert!((corners.high_pressure_low_exponent_n - high_pressure_n).abs() <= 1e-9);
        assert!((corners.high_pressure_high_exponent_n - high_pressure_n).abs() <= 1e-9);
        // 9000 Pa * 314 mm^2 (in m^2) = 2.826 N -- the design's own worked number.
        assert!((low_pressure_n - 2.826).abs() <= 1e-9);
    }

    #[test]
    fn a_degenerate_direction_is_refused_rather_than_normalized_to_nothing() {
        assert!(matches!(
            UpDirection::new([0.0, 0.0, 0.0]),
            Err(ScoreError::InvalidDirection { .. })
        ));
        assert!(matches!(
            UpDirection::new([f64::NAN, 0.0, 1.0]),
            Err(ScoreError::InvalidDirection { .. })
        ));
    }
}
