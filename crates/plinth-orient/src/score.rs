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
//! score(u) = ((w_sup·support_area_cm²(u) + w_peak·peak_xsec_cm²(u))
//!             + w_h·height_mm(u))
//!            + w_scar·Σ_down(area_mm²·delicacy)/100
//! ```
//!
//! recorded in `03-physics-and-constants.md` §3.8, with every weight a
//! registered `Placeholder`.  The association order above is **pinned** (the
//! einsum lesson, `02-target-architecture.md:781-783`): a re-association is a
//! different set of rounding decisions and therefore a different argmin.
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

use plinth_core::m0_constants;
use plinth_mesh::{
    build_face_topology, evaluate_delicacy, mesh_fingerprint, DelicacyBuildConfig, DelicacyError,
    DelicacyField, DelicacySettings, Mesh, TopologyError, WeldPolicy,
};
use rayon::prelude::*;
use thiserror::Error;

use crate::section::{build_profile, SectionRefusal};

/// Registered `ConstantId`s of the objective, carried so a report can stamp the
/// weights it scored with rather than restating their values.
pub const ORIENT_WEIGHT_SUPPORT_AREA_CM2_ID: &str = "ORIENT_WEIGHT_SUPPORT_AREA_CM2";
pub const ORIENT_WEIGHT_PEAK_XSEC_CM2_ID: &str = "ORIENT_WEIGHT_PEAK_XSEC_CM2";
pub const ORIENT_WEIGHT_HEIGHT_MM_ID: &str = "ORIENT_WEIGHT_HEIGHT_MM";
pub const ORIENT_SCAR_WEIGHT_ID: &str = "ORIENT_SCAR_WEIGHT";
pub const OVERHANG_ANGLE_DEG_ID: &str = "OVERHANG_ANGLE_DEG";

/// A candidate build-up direction in model frame: the whole search space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UpDirection([f64; 3]);

impl UpDirection {
    /// The authored up direction, which is also the identity candidate.
    pub const MODEL_UP: Self = Self([0.0, 0.0, 1.0]);

    /// Normalizes a finite, non-degenerate direction.
    ///
    /// # Errors
    ///
    /// [`ScoreError::InvalidDirection`] for a non-finite or zero-length vector.
    pub fn new(direction: [f64; 3]) -> Result<Self, ScoreError> {
        let norm = (direction[0] * direction[0]
            + direction[1] * direction[1]
            + direction[2] * direction[2])
            .sqrt();
        if !direction.iter().all(|value| value.is_finite()) || !norm.is_finite() || norm <= 0.0 {
            return Err(ScoreError::InvalidDirection { direction });
        }
        Ok(Self([
            canonical_zero(direction[0] / norm),
            canonical_zero(direction[1] / norm),
            canonical_zero(direction[2] / norm),
        ]))
    }

    #[must_use]
    pub const fn get(self) -> [f64; 3] {
        self.0
    }

    /// The rotation taking this direction to `+ẑ`: the shortest arc, with the
    /// single antipode `u = −ẑ` resolved by one pinned convention, 180° about
    /// `+X`.
    ///
    /// Row 2 is `u` by construction, so `(R·v)_z = v·u`; rows 0 and 1 are the
    /// in-plane basis the section solver measures areas in.  Written in the
    /// singularity-free closed form `R = c·I + [v]ₓ + v·vᵀ/(1+c)` with
    /// `v = u × ẑ` and `c = u_z`: the `v·vᵀ` term stays bounded as `c → −1`
    /// because `|v|² = 1 − c²`, so only the exact antipode needs the convention.
    #[must_use]
    pub fn rotation_to_build_frame(self) -> [[f64; 3]; 3] {
        let [x, y, c] = self.0;
        let denominator = 1.0 + c;
        if denominator <= 0.0 {
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
    peak_xsec_cm2: f64,
    height_mm: f64,
    scar: f64,
    overhang_angle_deg: f64,
}

impl ObjectiveWeights {
    /// Reads all five numbers from the one M0 registry.
    ///
    /// # Errors
    ///
    /// [`ScoreError::Constants`] when the registry cannot be constructed.
    pub fn registered() -> Result<Self, ScoreError> {
        let values = m0_constants()
            .map_err(|error| ScoreError::Constants(error.to_string()))?
            .placeholders();
        Ok(Self {
            support_area_cm2: values.orient_weight_support_area_cm2().diagnostic().get(),
            peak_xsec_cm2: values.orient_weight_peak_xsec_cm2().diagnostic().get(),
            height_mm: values.orient_weight_height_mm().diagnostic().get(),
            scar: values.orient_scar_weight().diagnostic().get(),
            overhang_angle_deg: values.overhang_angle_deg().diagnostic().get(),
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
    pub const fn peak_xsec_cm2(self) -> f64 {
        self.peak_xsec_cm2
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

    /// The down-facing cutoff, byte for byte the expression
    /// `plinth-support/src/lib.rs:814` evaluates from the same constant.
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

    /// Guards scaled to the model they will be asked to score, following the
    /// support stage's `max(floor, per_element × count)` idiom.
    #[must_use]
    pub fn production(faces: usize, verts: usize) -> Self {
        const PRODUCT_FLOOR: usize = 1 << 28;
        const BYTE_FLOOR: usize = 64 * 1024 * 1024;
        // Four candidates in flight is the smallest tile worth calling batched,
        // so the byte guard never forces a degenerate tile on a real model.
        let column_bytes = column_bytes(faces, verts).saturating_mul(4);
        Self {
            max_candidates: Self::MAX_CANDIDATES,
            max_candidate_face_products: PRODUCT_FLOOR
                .max(faces.saturating_mul(Self::MAX_CANDIDATES)),
            max_candidate_vertex_products: PRODUCT_FLOOR
                .max(verts.saturating_mul(3).saturating_mul(Self::MAX_CANDIDATES)),
            max_section_breakpoints: PRODUCT_FLOOR.max(verts.saturating_mul(Self::MAX_CANDIDATES)),
            max_working_bytes: BYTE_FLOOR.max(column_bytes),
        }
    }
}

/// Bytes one candidate occupies while in flight: its column of `NZ` plus the
/// three vertex-side projections its section solver consumes.
fn column_bytes(faces: usize, verts: usize) -> usize {
    faces
        .saturating_add(verts.saturating_mul(3))
        .saturating_mul(std::mem::size_of::<f64>())
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

/// What it then executed. Charged and metered are both stated; the exact pair
/// must agree, and the bounded pair may only come in under its charge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScoreMeter {
    pub face_direction_products: usize,
    pub vertex_direction_products: usize,
    pub section_breakpoints: usize,
}

/// The four terms and the assembled score for one candidate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CandidateTerms {
    pub support_area_cm2: f64,
    pub peak_xsec_cm2: f64,
    pub height_mm: f64,
    pub scar_term: f64,
    /// `Σ_down(area_mm² × delicacy)` before the weight and the cm² conversion,
    /// carried so a report can show the measurement separately from its price.
    pub scar_raw_mm2: f64,
    pub down_facing_faces: usize,
    pub score: f64,
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
    delicacy_digest: String,
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
        let field =
            evaluate_delicacy(mesh, lod.clone(), settings, config).map_err(ScoreError::Delicacy)?;
        Self::with_delicacy(mesh, lod, &field)
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
        let lod = lod.into();
        if field.face_table().lod() != lod {
            return Err(ScoreError::DelicacyLodMismatch {
                expected: lod,
                actual: field.face_table().lod().to_string(),
            });
        }
        field.validate_mesh(mesh).map_err(ScoreError::Delicacy)?;

        // Welded topology, once. `WeldPolicy::ExactBits` is stated here rather
        // than taken from a caller for the same reason the delicacy evaluator
        // states it: under `WeldPolicy::None` an STL soup shares no vertices, so
        // every edge reads as a boundary and the closure check below would
        // refuse every model on disk.
        let topology =
            build_face_topology(mesh, WeldPolicy::ExactBits).map_err(ScoreError::Topology)?;
        let counts = topology.counts();
        if counts.boundary_edges > 0 {
            // The section solver's shoelace identity assumes the section is a
            // union of closed loops. A hole breaks that, so this is a refusal
            // the LOD ladder can catch and answer with a finer proxy — never a
            // silently wrong area.
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
            let cross = cross(subtract(b, a), subtract(c, a));
            let length = norm(cross);
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
            delicacy: field
                .face_values()
                .iter()
                .map(|value| value.weight())
                .collect(),
            delicacy_digest: field.digest().to_string(),
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
    #[must_use]
    pub fn delicacy_digest(&self) -> &str {
        &self.delicacy_digest
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
    let mut breakpoints_executed = 0_usize;

    // One explicit pool per call at the workspace thread count, never rayon's
    // global pool: a one-shot global pool makes the thread sweep inert
    // (06-conventions §3).
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(geom_kernel::current_threads())
        .build()
        .map_err(|error| ScoreError::Pool(error.to_string()))?;

    for tile in candidates.chunks(plan.candidate_tile) {
        // The one batched product: faces outer, candidates inner, so the normal
        // array streams once per tile instead of once per candidate.
        let mut nz = vec![0.0_f64; faces * tile.len()];
        for (face, normal) in table.normals.iter().enumerate() {
            let row = &mut nz[face * tile.len()..(face + 1) * tile.len()];
            for (slot, candidate) in row.iter_mut().zip(tile) {
                let u = candidate.get();
                *slot = normal[0] * u[0] + normal[1] * u[1] + normal[2] * u[2];
            }
        }

        // Index-parallel map-collect across candidates: every output lands in
        // the slot of its own index, so the tile's results are bit-identical at
        // any thread count.
        let tile_outcomes: Vec<(CandidateOutcome, usize)> = pool.install(|| {
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
        });
        for (outcome, breakpoints) in tile_outcomes {
            breakpoints_executed = breakpoints_executed.checked_add(breakpoints).ok_or(
                ScoreError::ArithmeticOverflow {
                    section: "section breakpoints",
                },
            )?;
            outcomes.push(outcome);
        }
    }

    let meter = ScoreMeter {
        face_direction_products: plan.face_direction_products,
        vertex_direction_products: plan.vertex_direction_products,
        section_breakpoints: breakpoints_executed,
    };
    // The two exact terms are executed exactly as charged; breakpoints are
    // charged at their vertex-count bound and metered at the distinct heights
    // actually resolved, so under-running is expected and over-running is drift.
    if meter.section_breakpoints > plan.section_breakpoints {
        return Err(ScoreError::ChargeDrift {
            charged: plan.section_breakpoints,
            executed: meter.section_breakpoints,
            what: "section breakpoints",
        });
    }

    Ok(ScoredCandidates {
        outcomes,
        plan,
        meter,
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
    let per_candidate = column_bytes(faces, verts);
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
) -> (CandidateOutcome, usize) {
    let index = column;

    // Support area and scar share one masked pass over faces. The scar branch is
    // hoisted out of the loop rather than tested per face: at a zero weight the
    // delicacy field is not read *at all*, so a scar-free score is independent of
    // it structurally, not by multiplying by zero.
    let mut support_area_mm2 = 0.0_f64;
    let mut scar_raw_mm2 = 0.0_f64;
    let mut down_facing_faces = 0_usize;
    if weights.scar == 0.0 {
        for face in 0..table.tris.len() {
            if nz[face * stride + column] < down_limit {
                support_area_mm2 += table.areas_mm2[face];
                down_facing_faces += 1;
            }
        }
    } else {
        for face in 0..table.tris.len() {
            if nz[face * stride + column] < down_limit {
                support_area_mm2 += table.areas_mm2[face];
                scar_raw_mm2 += table.areas_mm2[face] * table.delicacy[face];
                down_facing_faces += 1;
            }
        }
    }

    let Projection {
        plane,
        heights,
        height_mm,
        lowest: _,
    } = project_vertices(table, candidate);

    let profile = build_profile(&table.tris, &table.degenerate, &plane, &heights);
    let breakpoints = profile.breakpoint_count();
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
                    breakpoints,
                )
            }
            SectionRefusal::NegativeArea { peak_mm2 } => {
                return (
                    CandidateOutcome::Refused(CandidateRefusal {
                        candidate: index,
                        reason: CandidateRefusalReason::NegativeSectionArea { peak_mm2 },
                    }),
                    breakpoints,
                )
            }
        },
    };

    let support_area_cm2 = canonical_zero(support_area_mm2 / 100.0);
    let peak_xsec_cm2 = canonical_zero(peak_mm2 / 100.0);
    let height_mm = canonical_zero(height_mm);
    let scar_term = canonical_zero(weights.scar * scar_raw_mm2 / 100.0);
    for (term, value) in [
        ("support_area_cm2", support_area_cm2),
        ("peak_xsec_cm2", peak_xsec_cm2),
        ("height_mm", height_mm),
        ("scar_term", scar_term),
    ] {
        if !value.is_finite() {
            return (
                CandidateOutcome::Refused(CandidateRefusal {
                    candidate: index,
                    reason: CandidateRefusalReason::NonFiniteTerm { term, value },
                }),
                breakpoints,
            );
        }
    }

    // The pinned association order. Re-associating is a different set of
    // rounding decisions and therefore, at 650 candidates, potentially a
    // different argmin.
    let score = ((weights.support_area_cm2 * support_area_cm2
        + weights.peak_xsec_cm2 * peak_xsec_cm2)
        + weights.height_mm * height_mm)
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
            breakpoints,
        );
    }

    (
        CandidateOutcome::Scored(CandidateTerms {
            support_area_cm2,
            peak_xsec_cm2,
            height_mm,
            scar_term,
            scar_raw_mm2: canonical_zero(scar_raw_mm2),
            down_facing_faces,
            score: canonical_zero(score),
        }),
        breakpoints,
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
}

/// Three whole-array passes over the vertices, one per row of the candidate's
/// own rotation. No mesh is rotated and nothing is materialized per face.
fn project_vertices(table: &OrientFaceTable, candidate: UpDirection) -> Projection {
    let u = candidate.get();
    let rotation = candidate.rotation_to_build_frame();
    let mut heights = Vec::with_capacity(table.verts.len());
    let mut lowest = f64::INFINITY;
    let mut highest = f64::NEG_INFINITY;
    for vertex in &table.verts {
        let height = vertex[0] * u[0] + vertex[1] * u[1] + vertex[2] * u[2];
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
            vertex[0] * rotation[0][0] + vertex[1] * rotation[0][1] + vertex[2] * rotation[0][2],
            vertex[0] * rotation[1][0] + vertex[1] * rotation[1][1] + vertex[2] * rotation[1][2],
        ]);
    }
    Projection {
        plane,
        heights,
        height_mm,
        lowest,
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

fn subtract(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn norm(value: [f64; 3]) -> f64 {
    (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt()
}

const fn canonical_zero(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value
    }
}

#[derive(Debug, PartialEq, Error)]
pub enum ScoreError {
    #[error("registered orientation constants are unavailable: {0}")]
    Constants(String),
    #[error("candidate direction {direction:?} is not a finite nonzero vector")]
    InvalidDirection { direction: [f64; 3] },
    #[error("delicacy field names LOD {actual}, but the table is scoring LOD {expected}")]
    DelicacyLodMismatch { expected: String, actual: String },
    #[error("orientation delicacy field is unavailable: {0}")]
    Delicacy(DelicacyError),
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
        let limits = OrientSearchLimits::production(table.face_count(), table.vertex_count());
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

    /// **T11.** Every budget refuses naming both numbers, before any working
    /// buffer exists, and never by trimming the candidate set to fit.
    #[test]
    fn every_work_budget_refuses_naming_both_numbers() {
        let (_mesh, table) = cube_table();
        let weights = ObjectiveWeights::registered().expect("registered objective");
        let candidates = vec![UpDirection::MODEL_UP; 8];
        let base = OrientSearchLimits::production(table.face_count(), table.vertex_count());

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
        let wide = OrientSearchLimits::production(table.face_count(), table.vertex_count());
        let narrow = OrientSearchLimits {
            max_working_bytes: column_bytes(table.face_count(), table.vertex_count()),
            ..wide
        };

        let wide_scores = score_candidates(&table, &candidates, weights, wide).expect("wide tile");
        let narrow_scores =
            score_candidates(&table, &candidates, weights, narrow).expect("narrow tile");
        assert_eq!(narrow_scores.plan().candidate_tile, 1);
        assert!(wide_scores.plan().candidate_tile > 1);
        assert_eq!(wide_scores.outcomes(), narrow_scores.outcomes());

        // Charged and executed are both stated; the exact terms agree and the
        // bounded one comes in under its charge.
        let plan = wide_scores.plan();
        let meter = wide_scores.meter();
        assert_eq!(plan.face_direction_products, meter.face_direction_products);
        assert_eq!(
            plan.vertex_direction_products,
            meter.vertex_direction_products
        );
        assert!(meter.section_breakpoints <= plan.section_breakpoints);
        assert_eq!(plan.candidates, candidates.len());
    }

    /// The sweep's "down-facing" is the support engine's, by construction: the
    /// same registered constant through the same expression
    /// (`crates/plinth-support/src/lib.rs:814`), applied with the same strict
    /// `<`.
    #[test]
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
    fn the_objective_weights_are_the_registered_placeholders() {
        let weights = ObjectiveWeights::registered().expect("registered objective");
        assert_eq!(weights.support_area_cm2(), 0.5);
        assert_eq!(weights.peak_xsec_cm2(), 3.0);
        assert_eq!(weights.height_mm(), 0.3);
        assert_eq!(weights.scar(), 0.4);
        assert_eq!(weights.scar_free().scar(), 0.0);
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
