//! The mandatory production QEM-decimation substrate for Plinth.
//!
//! This crate is deliberately a narrow adapter around `meshopt`'s locked QEM
//! simplifier.  It is not an orientation aid or an optional cleanup pass: M3
//! makes its print-scale output the only mesh every later production stage may
//! consume.  Probing, protection-halo selection, error-budget measurement,
//! and bisection belong to later M3 leaves; this leaf only performs one
//! deterministic locked simplification request.
#![forbid(unsafe_code)]

use std::mem::size_of;

use geom_kernel::{
    NativeSurface, NativeSurfaceConfig, PointCloudIndex, PointIndexBuildError, PointQueryError,
};
use meshopt::{simplify_with_locks, typed_to_bytes, SimplifyOptions, VertexDataAdapter};
use plinth_core::{m0_constants, PositiveFinite};
use plinth_mesh::{
    build_face_topology, EdgeTopologyCounts, Mesh, StrictMeshAuditError, WeldPolicy,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{Rng, SeedableRng};
use thiserror::Error;

/// A single locked QEM request.  `true` means protected: the vertex is passed
/// to meshoptimizer as `meshopt_SimplifyVertex_Lock`, never as a weight.
#[derive(Clone, Debug, PartialEq)]
pub struct SimplifyRequest {
    /// Desired output face fraction, rounded down to a whole face with a
    /// one-face minimum.  The derived target is then multiplied by three.
    pub target_ratio: f64,
    /// One hard lock per source vertex. `true` means protected.
    pub vertex_locks: Vec<bool>,
}

/// The completed mesh and evidence required by later protection/error stages.
#[derive(Clone, Debug, PartialEq)]
pub struct SimplifyOutcome {
    /// Compact, source-f64 output mesh.
    pub mesh: Mesh,
    /// Output vertex number -> original source vertex number, in ascending
    /// source-ID order. This is the authoritative remapping for later stages.
    pub retained_original_vertex_ids: Vec<u32>,
    pub input_face_count: usize,
    pub output_face_count: usize,
    pub input_vertex_count: usize,
    pub output_vertex_count: usize,
    pub requested_ratio: f64,
    pub target_face_count: usize,
    pub achieved_ratio: f64,
    /// meshoptimizer's native advisory result only; it is *not* Plinth's
    /// two-sided geometric error budget (D2 owns that measurement).
    pub meshopt_advisory_error: f32,
    /// Largest absolute coordinate delta introduced only in the f64->f32
    /// adapter input. Output coordinates remain original f64 values.
    pub max_input_quantization_delta: f64,
    pub locked_vertex_count: usize,
    /// `true` only when target ratio was exactly one and the adapter bypassed
    /// meshoptimizer to preserve source topology and f64 bits exactly.
    pub identity_bypass: bool,
}

/// The boundary failures that D0 can establish without inventing repair.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum SimplifyError {
    #[error("strict mesh audit failed: {0}")]
    StrictAudit(#[from] StrictMeshAuditError),
    #[error("target ratio must be finite and in (0, 1], got {ratio}")]
    InvalidRatio { ratio: f64 },
    #[error("vertex-lock length {actual} does not equal vertex count {expected}")]
    LockLength { expected: usize, actual: usize },
    #[error("source vertex count {vertex_count} exceeds meshopt's u32 index space")]
    VertexCountTooLarge { vertex_count: usize },
    #[error("source index count {index_count} exceeds meshopt's accepted u32 count")]
    IndexCountTooLarge { index_count: usize },
    #[error("vertex {vertex} axis {axis} cannot be represented as finite f32: {value}")]
    CoordinateNotFiniteF32 {
        vertex: usize,
        axis: usize,
        value: f64,
    },
    #[error("meshopt vertex adapter rejected the prepared f32 buffer: {message}")]
    VertexAdapter { message: String },
    #[error("meshopt returned malformed index count {index_count}")]
    MalformedResultIndexCount { index_count: usize },
    #[error("meshopt returned out-of-range index {index} at result position {position}; vertex count {vertex_count}")]
    ResultIndexOutOfRange {
        position: usize,
        index: u32,
        vertex_count: usize,
    },
    #[error("meshopt returned no triangles")]
    EmptyResult,
    #[error("mesh construction after deterministic compaction failed: {message}")]
    OutputMesh { message: String },
    #[error("hard-locked source vertex {vertex} disappeared from the result")]
    LockedVertexRemoved { vertex: usize },
}

/// Explicit construction and nearest-query limits for D1 probe surfaces.
/// There are deliberately no hidden grid-pitch or nearest-distance defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeConfig {
    pub surface: NativeSurfaceConfig,
    pub nearest_upper_bound_mm: f64,
}

/// Select all points for final verification or capped deterministic samples for
/// exploratory probes. Sampling uses ChaCha8 as Plinth's ratified replacement,
/// not a claim of byte-identical numpy RNG output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbePopulation {
    Full,
    Sampled,
}

/// Reproducible selection evidence without retaining a large point copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionEvidence {
    pub source_count: usize,
    pub selected_count: usize,
    pub seed: Option<u64>,
    pub selected_index_digest: u64,
}

/// Two-sided nearest-surface evidence for later M3 budget policy.
#[derive(Clone, Debug, PartialEq)]
pub struct TwoSidedProbe {
    pub candidate_face_count: usize,
    pub forward: SelectionEvidence,
    pub backward: SelectionEvidence,
    pub p99_mm: f64,
    pub p99_9_mm: f64,
    pub max_mm: Option<f64>,
    /// Digest of the finite, source-order gathered distances before percentile
    /// sorting; this permits deterministic report comparisons across threads.
    pub distance_digest: u64,
}

/// Typed D1 failures. No non-finite distance or bounded-query miss is ignored.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum ProbeError {
    #[error("strict {side} mesh audit failed: {message}")]
    StrictAudit { side: &'static str, message: String },
    #[error("nearest upper bound must be finite and non-negative, got {value}")]
    InvalidNearestUpperBound { value: f64 },
    #[error("failed to construct {side} nearest surface: {message}")]
    SurfaceBuild { side: &'static str, message: String },
    #[error("{side} point {point} has non-finite {axis} coordinate {value}")]
    NonFinitePoint {
        side: &'static str,
        point: usize,
        axis: usize,
        value: f64,
    },
    #[error("{side} nearest query returned no hit at point {point}")]
    NoHit { side: &'static str, point: usize },
    #[error("{side} nearest query returned non-finite distance {distance} at point {point}")]
    NonFiniteDistance {
        side: &'static str,
        point: usize,
        distance: f64,
    },
    #[error("cannot compute a percentile over an empty distance population")]
    EmptyPopulation,
}

/// Explicit, typed inputs for M3's production error-budget search.  The
/// default pixel pitch is deliberately constructed only from the registered
/// Saturn measurement; callers selecting another printer must provide a
/// positive finite pitch themselves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecimationBudget {
    pub pixel_um: PositiveFinite,
    pub probe: ProbeConfig,
}

impl DecimationBudget {
    pub const fn new(pixel_um: PositiveFinite, probe: ProbeConfig) -> Self {
        Self { pixel_um, probe }
    }

    /// Builds the default budget from the registered Saturn 4 Ultra Y-axis
    /// pixel pitch, the coarser measured 19 µm printer axis.
    pub fn ratified_default(probe: ProbeConfig) -> Result<Self, BudgetConfigError> {
        let constants = m0_constants().map_err(|error| BudgetConfigError::Constants {
            message: error.to_string(),
        })?;
        let pixel_um = PositiveFinite::new(*constants.saturn4_ultra().pixel_pitch_y_um().value())
            .map_err(|error| BudgetConfigError::PixelPitch {
            message: error.to_string(),
        })?;
        Ok(Self::new(pixel_um, probe))
    }
}

/// The two error limits derived from the explicitly supplied pixel pitch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ErrorBudget {
    pub p99_mm: f64,
    pub p99_9_mm: f64,
}

/// The exact input-scale evidence used to distinguish a safe no-output SKIP
/// from a failed candidate search.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleSanity {
    pub dimensions_mm: [f64; 3],
    pub longest_axis_mm: f64,
}

/// One sampled QEM/probe observation.  It deliberately keeps the D1 evidence
/// but not a second candidate mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct BudgetSearchStep {
    pub ratio: f64,
    pub output_face_count: usize,
    pub output_vertex_count: usize,
    pub probe: TwoSidedProbe,
    pub passed: bool,
}

/// Deterministic evidence from the precondition and every bisection probe.
#[derive(Clone, Debug, PartialEq)]
pub struct BudgetSearchTrace {
    pub scale: ScaleSanity,
    pub budget: ErrorBudget,
    pub protected_vertex_count: usize,
    pub steps: Vec<BudgetSearchStep>,
}

/// A verified production candidate.  This is the only variant that owns a
/// candidate mesh, so failed and skipped calls cannot be exported by mistake.
#[derive(Clone, Debug, PartialEq)]
pub struct BudgetSearchSuccess {
    pub mesh: Mesh,
    pub chosen_ratio: f64,
    pub trace: BudgetSearchTrace,
    pub final_probe: TwoSidedProbe,
    pub topology_before: EdgeTopologyCounts,
    pub topology_after: EdgeTopologyCounts,
}

/// Successful scale validation either produces one verified mesh or a named,
/// candidate-free scale skip.
#[derive(Clone, Debug, PartialEq)]
pub enum BudgetSearchOutcome {
    SkippedScaleSuspicious { scale: ScaleSanity },
    Verified(Box<BudgetSearchSuccess>),
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum BudgetConfigError {
    #[error("registered M0 constants are unavailable: {message}")]
    Constants { message: String },
    #[error("registered pixel pitch cannot form a positive finite budget: {message}")]
    PixelPitch { message: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum VerificationFailureReason {
    FinalSimplify {
        message: String,
    },
    FinalProbe {
        message: String,
    },
    BudgetSlackExceeded {
        budget: ErrorBudget,
        final_probe: TwoSidedProbe,
        slack: f64,
    },
    TopologyIncreased {
        before: EdgeTopologyCounts,
        after: EdgeTopologyCounts,
    },
    TopologyAudit {
        message: String,
    },
}

/// Full evidence retained when the final-only verification phase refuses to
/// return a candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct VerificationFailure {
    pub trace: BudgetSearchTrace,
    pub final_probe: Option<TwoSidedProbe>,
    pub topology_before: EdgeTopologyCounts,
    pub topology_after: Option<EdgeTopologyCounts>,
    pub reason: VerificationFailureReason,
}

/// Typed M3 budget-search failures.  In particular, an unreachable loose
/// ratio is distinct from a scale skip and from final verification refusal.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum BudgetSearchError {
    #[error("registered M0 constants are unavailable: {message}")]
    Constants { message: String },
    #[error("{phase} failed before a verified candidate existed: {message}")]
    SearchExecution {
        phase: &'static str,
        message: String,
    },
    #[error("DecimationBudgetUnreachable: ratio 0.50 exceeds the sampled error budget")]
    DecimationBudgetUnreachable { trace: Box<BudgetSearchTrace> },
    #[error("VerificationFailed: final full-population verification refused the candidate")]
    VerificationFailed { failure: Box<VerificationFailure> },
}

/// One deterministic feature-protection selection attempt.  Attempt zero uses
/// the ratified seed threshold and halo; later attempts are the bounded
/// cap-backoff sequence.
#[derive(Clone, Debug, PartialEq)]
pub struct FeatureProtectionAttempt {
    pub attempt: usize,
    pub seed_threshold_mm: f64,
    pub halo_radius_mm: f64,
    pub seed_count: usize,
    pub protected_vertex_count: usize,
}

/// Evidence from the feature probe and the complete cap-adaptation sequence.
#[derive(Clone, Debug, PartialEq)]
pub struct FeatureProtectionTrace {
    pub probe_ratio: f64,
    pub cap_count: usize,
    pub attempts: Vec<FeatureProtectionAttempt>,
}

/// Typed failures from M3's mandatory protected-feature step.  Neither error
/// contains a candidate mesh, so a cap failure cannot accidentally escape into
/// the production budget search or export path.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum FeatureProtectionError {
    #[error("registered M0 constants are unavailable: {message}")]
    Constants { message: String },
    #[error("feature-probe simplification failed: {message}")]
    ProbeSimplify { message: String },
    #[error("feature-probe backward error map failed: {message}")]
    ProbeBackward { message: String },
    #[error("feature-protection input has {errors} errors for {vertices} vertices")]
    ErrorMapLength { vertices: usize, errors: usize },
    #[error("feature-protection point index failed: {message}")]
    PointIndex { message: String },
    #[error("feature-protection halo query failed: {message}")]
    HaloQuery { message: String },
    #[error(
        "FeatureProtectionCapExceeded: protected vertices exceed the 12% cap after bounded backoff"
    )]
    CapExceeded { trace: Box<FeatureProtectionTrace> },
}

/// The only public M3 entry point.  It first derives hard locks from the
/// aggressive backward-only feature probe, then performs the accepted D2
/// budget search with exactly those locks.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum DecimationError {
    #[error(transparent)]
    FeatureProtection(#[from] FeatureProtectionError),
    #[error(transparent)]
    Budget(#[from] BudgetSearchError),
}

/// Selects M3's protected original vertices from one ratio-0.20 backward
/// probe, then runs the staged budget search.  This is deliberately the only
/// production path: callers cannot omit feature protection by passing a lock
/// vector themselves.
pub fn run(
    original: &Mesh,
    budget: DecimationBudget,
) -> Result<BudgetSearchOutcome, DecimationError> {
    let (protected, _) = select_feature_protection(original, budget.probe)?;
    Ok(run_budget_search(original, &protected, budget)?)
}

#[derive(Clone, Copy)]
struct SearchPolicy {
    scale_min_mm: f64,
    scale_max_mm: f64,
    ratio_low: f64,
    ratio_high: f64,
    bisections: usize,
    p99_factor: f64,
    p99_9_factor: f64,
    final_slack: f64,
}

#[derive(Clone, Copy)]
struct FeatureProtectionPolicy {
    probe_ratio: f64,
    seed_threshold_mm: f64,
    halo_radius_mm: f64,
    cap_fraction: f64,
    backoff_attempts: usize,
    seed_multiplier: f64,
    halo_multiplier: f64,
}

/// Runs the mandatory ratio-0.20 D1 backward map and turns it into hard locks.
/// The spatial portion is one gathered batch query over every seed; the CSR
/// result is scattered into original global vertex IDs without renumbering.
pub fn select_feature_protection(
    original: &Mesh,
    probe_config: ProbeConfig,
) -> Result<(Vec<bool>, FeatureProtectionTrace), FeatureProtectionError> {
    let policy = feature_protection_policy()?;
    let probe = simplify_locked(
        original,
        &SimplifyRequest {
            target_ratio: policy.probe_ratio,
            vertex_locks: vec![false; original.verts().len()],
        },
    )
    .map_err(|error| FeatureProtectionError::ProbeSimplify {
        message: error.to_string(),
    })?;
    let errors = backward_error_map(original, &probe.mesh, probe_config).map_err(|error| {
        FeatureProtectionError::ProbeBackward {
            message: error.to_string(),
        }
    })?;
    select_feature_protection_from_errors(original, &errors, policy)
}

fn select_feature_protection_from_errors(
    original: &Mesh,
    errors: &[f64],
    policy: FeatureProtectionPolicy,
) -> Result<(Vec<bool>, FeatureProtectionTrace), FeatureProtectionError> {
    if errors.len() != original.verts().len() {
        return Err(FeatureProtectionError::ErrorMapLength {
            vertices: original.verts().len(),
            errors: errors.len(),
        });
    }
    let index = PointCloudIndex::new(original.verts()).map_err(|error| {
        FeatureProtectionError::PointIndex {
            message: point_index_message(error),
        }
    })?;
    let cap_count = (original.verts().len() as f64 * policy.cap_fraction).floor() as usize;
    let mut trace = FeatureProtectionTrace {
        probe_ratio: policy.probe_ratio,
        cap_count,
        attempts: Vec::with_capacity(policy.backoff_attempts + 1),
    };
    let mut threshold = policy.seed_threshold_mm;
    let mut radius = policy.halo_radius_mm;

    // Attempt zero plus precisely the configured number of bounded backoffs.
    for attempt in 0..=policy.backoff_attempts {
        let seed_ids = errors
            .iter()
            .enumerate()
            .filter_map(|(vertex, &error)| (error > threshold).then_some(vertex))
            .collect::<Vec<_>>();
        let seed_points = seed_ids
            .iter()
            .map(|&vertex| original.verts()[vertex])
            .collect::<Vec<_>>();
        let mut locks = vec![false; original.verts().len()];
        if !seed_points.is_empty() {
            let neighbors =
                index
                    .radius_neighbors_batch(&seed_points, radius)
                    .map_err(|error| FeatureProtectionError::HaloQuery {
                        message: point_query_message(error),
                    })?;
            for point_id in neighbors.point_ids {
                let vertex =
                    usize::try_from(point_id).expect("point index IDs originate from usize");
                locks[vertex] = true;
            }
        }
        let protected_vertex_count = locks.iter().filter(|&&locked| locked).count();
        trace.attempts.push(FeatureProtectionAttempt {
            attempt,
            seed_threshold_mm: threshold,
            halo_radius_mm: radius,
            seed_count: seed_ids.len(),
            protected_vertex_count,
        });
        if protected_vertex_count <= cap_count {
            return Ok((locks, trace));
        }
        threshold *= policy.seed_multiplier;
        radius *= policy.halo_multiplier;
    }
    Err(FeatureProtectionError::CapExceeded {
        trace: Box::new(trace),
    })
}

fn point_index_message(error: PointIndexBuildError) -> String {
    error.to_string()
}

fn point_query_message(error: PointQueryError) -> String {
    error.to_string()
}

fn feature_protection_policy() -> Result<FeatureProtectionPolicy, FeatureProtectionError> {
    let constants = m0_constants().map_err(|error| FeatureProtectionError::Constants {
        message: error.to_string(),
    })?;
    let placeholders = constants.placeholders();
    let adaptation = placeholders.decimation_adaptation_policy().diagnostic();
    Ok(FeatureProtectionPolicy {
        probe_ratio: placeholders
            .decimation_feature_probe_ratio()
            .diagnostic()
            .get(),
        seed_threshold_mm: placeholders.decimation_seed_error_mm().diagnostic().get(),
        halo_radius_mm: placeholders.decimation_feature_halo_mm().diagnostic().get(),
        cap_fraction: placeholders
            .decimation_protected_cap_fraction()
            .diagnostic()
            .get(),
        backoff_attempts: adaptation.adaptations().get() as usize,
        seed_multiplier: adaptation.feature_multiplier(),
        halo_multiplier: adaptation.error_multiplier(),
    })
}

/// Runs M3 stages 1, 4b, 5, 6, and 7 over the accepted D0/D1 primitives.
///
/// The returned [`BudgetSearchOutcome`] makes a scale skip explicit and keeps
/// it candidate-free.  Every non-skip success owns exactly one final mesh;
/// all failed paths retain only deterministic evidence, never an exportable
/// candidate.
pub fn run_budget_search(
    original: &Mesh,
    protected: &[bool],
    budget: DecimationBudget,
) -> Result<BudgetSearchOutcome, BudgetSearchError> {
    let policy = search_policy()?;
    let scale = scale_sanity(original);
    if !scale.longest_axis_mm.is_finite()
        || scale.longest_axis_mm < policy.scale_min_mm
        || scale.longest_axis_mm > policy.scale_max_mm
    {
        return Ok(BudgetSearchOutcome::SkippedScaleSuspicious { scale });
    }

    let topology_before = build_face_topology(original, WeldPolicy::ExactBits)
        .map(|topology| topology.edge_counts())
        .map_err(|error| BudgetSearchError::SearchExecution {
            phase: "original topology audit",
            message: error.to_string(),
        })?;
    let error_budget = ErrorBudget {
        p99_mm: budget.pixel_um.get() * policy.p99_factor,
        p99_9_mm: budget.pixel_um.get() * policy.p99_9_factor,
    };
    let protected_vertex_count = protected.iter().filter(|&&locked| locked).count();
    let mut trace = BudgetSearchTrace {
        scale,
        budget: error_budget,
        protected_vertex_count,
        steps: Vec::with_capacity(policy.bisections + 1),
    };

    let initial = sampled_step(
        original,
        protected,
        policy.ratio_high,
        budget.probe,
        error_budget,
    )
    .map_err(|error| BudgetSearchError::SearchExecution {
        phase: "ratio-high precondition",
        message: error,
    })?;
    let initial_passed = initial.passed;
    trace.steps.push(initial);
    if !initial_passed {
        return Err(BudgetSearchError::DecimationBudgetUnreachable {
            trace: Box::new(trace),
        });
    }

    let mut lowest_passing_ratio = policy.ratio_high;
    let mut low = policy.ratio_low;
    let mut high = policy.ratio_high;
    for _ in 0..policy.bisections {
        let midpoint = (low + high) / 2.0;
        let step = sampled_step(original, protected, midpoint, budget.probe, error_budget)
            .map_err(|error| BudgetSearchError::SearchExecution {
                phase: "ratio bisection",
                message: error,
            })?;
        if step.passed {
            high = midpoint;
            if midpoint < lowest_passing_ratio {
                lowest_passing_ratio = midpoint;
            }
        } else {
            low = midpoint;
        }
        trace.steps.push(step);
    }

    let final_outcome = simplify_locked(
        original,
        &SimplifyRequest {
            target_ratio: lowest_passing_ratio,
            vertex_locks: protected.to_vec(),
        },
    )
    .map_err(|error| {
        verification_failed(
            trace.clone(),
            topology_before,
            None,
            None,
            VerificationFailureReason::FinalSimplify {
                message: error.to_string(),
            },
        )
    })?;
    let final_probe = probe_two_sided(
        original,
        &final_outcome.mesh,
        ProbePopulation::Full,
        budget.probe,
    )
    .map_err(|error| {
        verification_failed(
            trace.clone(),
            topology_before,
            None,
            None,
            VerificationFailureReason::FinalProbe {
                message: error.to_string(),
            },
        )
    })?;
    let topology_after = build_face_topology(&final_outcome.mesh, WeldPolicy::ExactBits)
        .map(|topology| topology.edge_counts())
        .map_err(|error| {
            verification_failed(
                trace.clone(),
                topology_before,
                Some(final_probe.clone()),
                None,
                VerificationFailureReason::TopologyAudit {
                    message: error.to_string(),
                },
            )
        })?;
    verify_final_evidence(
        &trace,
        topology_before,
        final_probe.clone(),
        topology_after,
        error_budget,
        policy.final_slack,
    )?;

    Ok(BudgetSearchOutcome::Verified(Box::new(
        BudgetSearchSuccess {
            mesh: final_outcome.mesh,
            chosen_ratio: lowest_passing_ratio,
            trace,
            final_probe,
            topology_before,
            topology_after,
        },
    )))
}

fn sampled_step(
    original: &Mesh,
    protected: &[bool],
    ratio: f64,
    probe_config: ProbeConfig,
    budget: ErrorBudget,
) -> Result<BudgetSearchStep, String> {
    let outcome = simplify_locked(
        original,
        &SimplifyRequest {
            target_ratio: ratio,
            vertex_locks: protected.to_vec(),
        },
    )
    .map_err(|error| error.to_string())?;
    let probe = probe_two_sided(
        original,
        &outcome.mesh,
        ProbePopulation::Sampled,
        probe_config,
    )
    .map_err(|error| error.to_string())?;
    let passed = within_budget(&probe, budget);
    Ok(BudgetSearchStep {
        ratio,
        output_face_count: outcome.output_face_count,
        output_vertex_count: outcome.output_vertex_count,
        probe,
        passed,
    })
}

fn within_budget(probe: &TwoSidedProbe, budget: ErrorBudget) -> bool {
    probe.p99_mm <= budget.p99_mm && probe.p99_9_mm <= budget.p99_9_mm
}

fn within_final_slack(probe: &TwoSidedProbe, budget: ErrorBudget, slack: f64) -> bool {
    probe.p99_mm <= budget.p99_mm * slack && probe.p99_9_mm <= budget.p99_9_mm * slack
}

fn topology_not_increased(before: EdgeTopologyCounts, after: EdgeTopologyCounts) -> bool {
    after.boundary_edges <= before.boundary_edges
        && after.nonmanifold_edges <= before.nonmanifold_edges
}

fn verify_final_evidence(
    trace: &BudgetSearchTrace,
    topology_before: EdgeTopologyCounts,
    final_probe: TwoSidedProbe,
    topology_after: EdgeTopologyCounts,
    budget: ErrorBudget,
    slack: f64,
) -> Result<(), BudgetSearchError> {
    if !within_final_slack(&final_probe, budget, slack) {
        return Err(verification_failed(
            trace.clone(),
            topology_before,
            Some(final_probe.clone()),
            Some(topology_after),
            VerificationFailureReason::BudgetSlackExceeded {
                budget,
                final_probe,
                slack,
            },
        ));
    }
    if !topology_not_increased(topology_before, topology_after) {
        return Err(verification_failed(
            trace.clone(),
            topology_before,
            Some(final_probe),
            Some(topology_after),
            VerificationFailureReason::TopologyIncreased {
                before: topology_before,
                after: topology_after,
            },
        ));
    }
    Ok(())
}

fn verification_failed(
    trace: BudgetSearchTrace,
    topology_before: EdgeTopologyCounts,
    final_probe: Option<TwoSidedProbe>,
    topology_after: Option<EdgeTopologyCounts>,
    reason: VerificationFailureReason,
) -> BudgetSearchError {
    BudgetSearchError::VerificationFailed {
        failure: Box::new(VerificationFailure {
            trace,
            final_probe,
            topology_before,
            topology_after,
            reason,
        }),
    }
}

fn scale_sanity(mesh: &Mesh) -> ScaleSanity {
    let mut minimum = mesh.verts()[0];
    let mut maximum = mesh.verts()[0];
    for point in &mesh.verts()[1..] {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(point[axis]);
            maximum[axis] = maximum[axis].max(point[axis]);
        }
    }
    let dimensions_mm = [
        maximum[0] - minimum[0],
        maximum[1] - minimum[1],
        maximum[2] - minimum[2],
    ];
    let longest_axis_mm = dimensions_mm
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    ScaleSanity {
        dimensions_mm,
        longest_axis_mm,
    }
}

fn search_policy() -> Result<SearchPolicy, BudgetSearchError> {
    let constants = m0_constants().map_err(|error| BudgetSearchError::Constants {
        message: error.to_string(),
    })?;
    let placeholders = constants.placeholders();
    let scale = placeholders.decimation_scale_acceptance_mm().diagnostic();
    let ratios = *placeholders.decimation_ratio_search().diagnostic();
    let bisections = usize::try_from(ratios.bisections().get()).map_err(|error| {
        BudgetSearchError::Constants {
            message: format!("ratio bisection count cannot fit usize: {error}"),
        }
    })?;
    Ok(SearchPolicy {
        scale_min_mm: scale.lower(),
        scale_max_mm: scale.upper(),
        ratio_low: ratios.lower(),
        ratio_high: ratios.upper(),
        bisections,
        p99_factor: placeholders.decimation_p99_factor().diagnostic().get(),
        p99_9_factor: placeholders.decimation_p999_factor().diagnostic().get(),
        final_slack: placeholders.decimation_recheck_slack().diagnostic().get(),
    })
}

const FORWARD_CAP: usize = 150_000;
const BACKWARD_CAP: usize = 120_000;
const FORWARD_SEED: u64 = 1;
const BACKWARD_SEED: u64 = 42;

/// Builds one candidate surface and queries every original vertex in one batch.
/// Returned distances preserve original vertex order for feature-map generation.
pub fn backward_error_map(
    original: &Mesh,
    candidate: &Mesh,
    config: ProbeConfig,
) -> Result<Vec<f64>, ProbeError> {
    audit_probe_inputs(original, candidate, config)?;
    let candidate_surface = surface(candidate, "candidate", config.surface)?;
    query_batch(
        &candidate_surface,
        original.verts(),
        config.nearest_upper_bound_mm,
        "backward",
    )
}

/// Performs exactly two whole-batch nearest queries: candidate face centres to
/// original, and original vertices to candidate. Both result populations are
/// concatenated before NumPy-compatible linear percentiles are calculated.
pub fn probe_two_sided(
    original: &Mesh,
    candidate: &Mesh,
    population: ProbePopulation,
    config: ProbeConfig,
) -> Result<TwoSidedProbe, ProbeError> {
    audit_probe_inputs(original, candidate, config)?;
    let original_surface = surface(original, "original", config.surface)?;
    let candidate_surface = surface(candidate, "candidate", config.surface)?;
    let all_forward = face_centres(candidate)?;
    let (forward_indices, forward) =
        select_points(all_forward.len(), population, FORWARD_CAP, FORWARD_SEED);
    let (backward_indices, backward) = select_points(
        original.verts().len(),
        population,
        BACKWARD_CAP,
        BACKWARD_SEED,
    );
    let forward_points = forward_indices
        .iter()
        .map(|&index| all_forward[index])
        .collect::<Vec<_>>();
    let backward_points = backward_indices
        .iter()
        .map(|&index| original.verts()[index])
        .collect::<Vec<_>>();
    let mut distances = query_batch(
        &original_surface,
        &forward_points,
        config.nearest_upper_bound_mm,
        "forward",
    )?;
    distances.extend(query_batch(
        &candidate_surface,
        &backward_points,
        config.nearest_upper_bound_mm,
        "backward",
    )?);
    if distances.is_empty() {
        return Err(ProbeError::EmptyPopulation);
    }
    let mut sorted = distances.clone();
    sorted.sort_by(f64::total_cmp);
    let p99_mm = linear_percentile(&sorted, 0.99)?;
    let p99_9_mm = linear_percentile(&sorted, 0.999)?;
    let max_mm = sorted.last().copied();
    Ok(TwoSidedProbe {
        candidate_face_count: candidate.tris().len(),
        forward,
        backward,
        p99_mm,
        p99_9_mm,
        max_mm,
        distance_digest: digest_f64_bits(&distances),
    })
}

fn audit_probe_inputs(
    original: &Mesh,
    candidate: &Mesh,
    config: ProbeConfig,
) -> Result<(), ProbeError> {
    original
        .audit_strict()
        .map_err(|error| ProbeError::StrictAudit {
            side: "original",
            message: error.to_string(),
        })?;
    candidate
        .audit_strict()
        .map_err(|error| ProbeError::StrictAudit {
            side: "candidate",
            message: error.to_string(),
        })?;
    if !config.nearest_upper_bound_mm.is_finite() || config.nearest_upper_bound_mm < 0.0 {
        return Err(ProbeError::InvalidNearestUpperBound {
            value: config.nearest_upper_bound_mm,
        });
    }
    Ok(())
}

fn surface(
    mesh: &Mesh,
    side: &'static str,
    config: NativeSurfaceConfig,
) -> Result<NativeSurface, ProbeError> {
    NativeSurface::from_indexed(mesh.verts(), mesh.tris(), config).map_err(|error| {
        ProbeError::SurfaceBuild {
            side,
            message: error.to_string(),
        }
    })
}

fn query_batch(
    surface: &NativeSurface,
    points: &[[f64; 3]],
    upper_bound: f64,
    side: &'static str,
) -> Result<Vec<f64>, ProbeError> {
    for (point, value) in points.iter().enumerate() {
        for (axis, coordinate) in value.iter().copied().enumerate() {
            if !coordinate.is_finite() {
                return Err(ProbeError::NonFinitePoint {
                    side,
                    point,
                    axis,
                    value: coordinate,
                });
            }
        }
    }
    let (distances, indices) = surface.query(points, upper_bound);
    debug_assert_eq!(distances.len(), points.len());
    debug_assert_eq!(indices.len(), points.len());
    for (point, &distance) in distances.iter().enumerate() {
        if indices[point] < 0 || indices[point] as usize >= surface.nfaces() {
            return Err(ProbeError::NoHit { side, point });
        }
        if !distance.is_finite() {
            return Err(ProbeError::NonFiniteDistance {
                side,
                point,
                distance,
            });
        }
    }
    Ok(distances)
}

fn face_centres(mesh: &Mesh) -> Result<Vec<[f64; 3]>, ProbeError> {
    mesh.tris()
        .iter()
        .enumerate()
        .map(|(face, indices)| {
            let a = mesh.verts()[indices[0] as usize];
            let b = mesh.verts()[indices[1] as usize];
            let c = mesh.verts()[indices[2] as usize];
            let centre = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            for (axis, value) in centre.iter().copied().enumerate() {
                if !value.is_finite() {
                    return Err(ProbeError::NonFinitePoint {
                        side: "forward",
                        point: face,
                        axis,
                        value,
                    });
                }
            }
            Ok(centre)
        })
        .collect()
}

fn select_points(
    source_count: usize,
    population: ProbePopulation,
    cap: usize,
    seed: u64,
) -> (Vec<usize>, SelectionEvidence) {
    let use_sample = matches!(population, ProbePopulation::Sampled) && source_count > cap;
    let mut indices = (0..source_count).collect::<Vec<_>>();
    if use_sample {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        for index in 0..cap {
            let chosen = index + uniform_index(&mut rng, source_count - index);
            indices.swap(index, chosen);
        }
        indices.truncate(cap);
        indices.sort_unstable();
    }
    let evidence = SelectionEvidence {
        source_count,
        selected_count: indices.len(),
        seed: use_sample.then_some(seed),
        selected_index_digest: digest_indices(&indices),
    };
    (indices, evidence)
}

fn uniform_index(rng: &mut ChaCha8Rng, upper: usize) -> usize {
    let upper = upper as u64;
    let zone = u64::MAX - u64::MAX % upper;
    loop {
        let value = rng.next_u64();
        if value < zone {
            return (value % upper) as usize;
        }
    }
}

fn linear_percentile(sorted: &[f64], quantile: f64) -> Result<f64, ProbeError> {
    if sorted.is_empty() {
        return Err(ProbeError::EmptyPopulation);
    }
    let rank = (sorted.len() - 1) as f64 * quantile;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    Ok(sorted[lower] + (sorted[upper] - sorted[lower]) * (rank - lower as f64))
}

fn digest_indices(indices: &[usize]) -> u64 {
    digest_words(indices.iter().map(|&index| index as u64))
}
fn digest_f64_bits(values: &[f64]) -> u64 {
    digest_words(values.iter().map(|value| value.to_bits()))
}
fn digest_words(words: impl IntoIterator<Item = u64>) -> u64 {
    let mut state = 0xcbf2_9ce4_8422_2325_u64;
    for word in words {
        for byte in word.to_le_bytes() {
            state ^= u64::from(byte);
            state = state.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    state
}

/// Simplifies `mesh` once using upstream locked QEM.
///
/// The target-face rounding rule is `max(1, floor(input_faces * ratio))`.
/// It is converted to a target index count by multiplication by three.  When
/// topology or locks prevent reaching that target, the successful constrained
/// result is returned unchanged for later M3 policy to decide.
pub fn simplify_locked(
    mesh: &Mesh,
    request: &SimplifyRequest,
) -> Result<SimplifyOutcome, SimplifyError> {
    mesh.audit_strict()?;
    if !request.target_ratio.is_finite()
        || !(0.0 < request.target_ratio && request.target_ratio <= 1.0)
    {
        return Err(SimplifyError::InvalidRatio {
            ratio: request.target_ratio,
        });
    }
    if request.vertex_locks.len() != mesh.verts().len() {
        return Err(SimplifyError::LockLength {
            expected: mesh.verts().len(),
            actual: request.vertex_locks.len(),
        });
    }
    if mesh.verts().len() > u32::MAX as usize {
        return Err(SimplifyError::VertexCountTooLarge {
            vertex_count: mesh.verts().len(),
        });
    }
    let index_count =
        mesh.tris()
            .len()
            .checked_mul(3)
            .ok_or(SimplifyError::IndexCountTooLarge {
                index_count: usize::MAX,
            })?;
    if index_count > u32::MAX as usize {
        return Err(SimplifyError::IndexCountTooLarge { index_count });
    }

    let input_faces = mesh.tris().len();
    let target_faces = target_face_count(input_faces, request.target_ratio);
    let locked_vertex_count = request
        .vertex_locks
        .iter()
        .filter(|&&locked| locked)
        .count();
    let (positions, max_input_quantization_delta) = quantize_positions(mesh.verts())?;

    if request.target_ratio == 1.0 {
        return Ok(identity_outcome(
            mesh,
            request,
            target_faces,
            max_input_quantization_delta,
            locked_vertex_count,
        ));
    }

    let indices = flatten_indices(mesh.tris());
    let adapter = VertexDataAdapter::new(typed_to_bytes(&positions), size_of::<[f32; 3]>(), 0)
        .map_err(|error| SimplifyError::VertexAdapter {
            message: error.to_string(),
        })?;
    let mut advisory_error = 0.0_f32;
    // `ErrorAbsolute` makes f32::MAX a deliberately permissive absolute ceiling:
    // face ratio controls D0. D2 supplies Plinth's real two-sided measurement.
    let result = simplify_with_locks(
        &indices,
        &adapter,
        &request.vertex_locks,
        target_faces * 3,
        f32::MAX,
        SimplifyOptions::ErrorAbsolute,
        Some(&mut advisory_error),
    );
    compact_result(
        mesh,
        request,
        result,
        ResultMetadata {
            input_faces,
            target_faces,
            meshopt_advisory_error: advisory_error,
            max_input_quantization_delta,
            locked_vertex_count,
        },
    )
}

fn target_face_count(input_faces: usize, ratio: f64) -> usize {
    ((input_faces as f64 * ratio).floor() as usize).max(1)
}

fn quantize_positions(vertices: &[[f64; 3]]) -> Result<(Vec<[f32; 3]>, f64), SimplifyError> {
    let mut positions = Vec::with_capacity(vertices.len());
    let mut max_delta = 0.0_f64;
    for (vertex, point) in vertices.iter().enumerate() {
        let mut converted = [0.0_f32; 3];
        for (axis, value) in point.iter().copied().enumerate() {
            let quantized = value as f32;
            if !quantized.is_finite() {
                return Err(SimplifyError::CoordinateNotFiniteF32 {
                    vertex,
                    axis,
                    value,
                });
            }
            max_delta = max_delta.max((value - f64::from(quantized)).abs());
            converted[axis] = quantized;
        }
        positions.push(converted);
    }
    Ok((positions, max_delta))
}

fn flatten_indices(triangles: &[[u32; 3]]) -> Vec<u32> {
    let mut indices = Vec::with_capacity(triangles.len() * 3);
    for triangle in triangles {
        indices.extend_from_slice(triangle);
    }
    indices
}

fn identity_outcome(
    mesh: &Mesh,
    request: &SimplifyRequest,
    target_face_count: usize,
    max_input_quantization_delta: f64,
    locked_vertex_count: usize,
) -> SimplifyOutcome {
    SimplifyOutcome {
        mesh: mesh.clone(),
        retained_original_vertex_ids: (0..mesh.verts().len() as u32).collect(),
        input_face_count: mesh.tris().len(),
        output_face_count: mesh.tris().len(),
        input_vertex_count: mesh.verts().len(),
        output_vertex_count: mesh.verts().len(),
        requested_ratio: request.target_ratio,
        target_face_count,
        achieved_ratio: 1.0,
        meshopt_advisory_error: 0.0,
        max_input_quantization_delta,
        locked_vertex_count,
        identity_bypass: true,
    }
}

struct ResultMetadata {
    input_faces: usize,
    target_faces: usize,
    meshopt_advisory_error: f32,
    max_input_quantization_delta: f64,
    locked_vertex_count: usize,
}

fn compact_result(
    mesh: &Mesh,
    request: &SimplifyRequest,
    result: Vec<u32>,
    metadata: ResultMetadata,
) -> Result<SimplifyOutcome, SimplifyError> {
    if result.is_empty() {
        return Err(SimplifyError::EmptyResult);
    }
    if !result.len().is_multiple_of(3) {
        return Err(SimplifyError::MalformedResultIndexCount {
            index_count: result.len(),
        });
    }
    let mut retained = vec![false; mesh.verts().len()];
    for (position, &index) in result.iter().enumerate() {
        let original = index as usize;
        if original >= retained.len() {
            return Err(SimplifyError::ResultIndexOutOfRange {
                position,
                index,
                vertex_count: retained.len(),
            });
        }
        retained[original] = true;
    }
    for (vertex, &locked) in request.vertex_locks.iter().enumerate() {
        if locked && !retained[vertex] {
            return Err(SimplifyError::LockedVertexRemoved { vertex });
        }
    }
    let retained_original_vertex_ids = retained
        .iter()
        .enumerate()
        .filter_map(|(index, &used)| used.then_some(index as u32))
        .collect::<Vec<_>>();
    let mut original_to_output = vec![u32::MAX; retained.len()];
    let compact_vertices = retained_original_vertex_ids
        .iter()
        .enumerate()
        .map(|(output, &original)| {
            original_to_output[original as usize] = output as u32;
            mesh.verts()[original as usize]
        })
        .collect::<Vec<_>>();
    let compact_triangles = result
        .chunks_exact(3)
        .map(|face| {
            [
                original_to_output[face[0] as usize],
                original_to_output[face[1] as usize],
                original_to_output[face[2] as usize],
            ]
        })
        .collect::<Vec<_>>();
    let compact_mesh = Mesh::new(compact_vertices, compact_triangles).map_err(|error| {
        SimplifyError::OutputMesh {
            message: error.to_string(),
        }
    })?;
    compact_mesh
        .audit_strict()
        .map_err(|error| SimplifyError::OutputMesh {
            message: error.to_string(),
        })?;
    Ok(SimplifyOutcome {
        output_face_count: compact_mesh.tris().len(),
        output_vertex_count: compact_mesh.verts().len(),
        mesh: compact_mesh,
        retained_original_vertex_ids,
        input_face_count: metadata.input_faces,
        input_vertex_count: mesh.verts().len(),
        requested_ratio: request.target_ratio,
        target_face_count: metadata.target_faces,
        achieved_ratio: result.len() as f64 / 3.0 / metadata.input_faces as f64,
        meshopt_advisory_error: metadata.meshopt_advisory_error,
        max_input_quantization_delta: metadata.max_input_quantization_delta,
        locked_vertex_count: metadata.locked_vertex_count,
        identity_bypass: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn octahedron() -> Mesh {
        Mesh::new(
            vec![
                [1.1, 0.0, 0.0],
                [-1.1, 0.0, 0.0],
                [0.0, 1.1, 0.0],
                [0.0, -1.1, 0.0],
                [0.0, 0.0, 1.1],
                [0.0, 0.0, -1.1],
            ],
            vec![
                [0, 2, 4],
                [2, 1, 4],
                [1, 3, 4],
                [3, 0, 4],
                [2, 0, 5],
                [1, 2, 5],
                [3, 1, 5],
                [0, 3, 5],
            ],
        )
        .unwrap()
    }
    fn request(ratio: f64, locks: Vec<bool>) -> SimplifyRequest {
        SimplifyRequest {
            target_ratio: ratio,
            vertex_locks: locks,
        }
    }

    fn probe_config(bound: f64) -> ProbeConfig {
        ProbeConfig {
            surface: NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: usize::MAX,
            },
            nearest_upper_bound_mm: bound,
        }
    }

    fn triangle(z: f64) -> Mesh {
        Mesh::new(
            vec![[0.0, 0.0, z], [3.0, 0.0, z], [0.0, 3.0, z]],
            vec![[0, 1, 2]],
        )
        .unwrap()
    }

    #[test]
    fn ratio_one_is_exact_identity() {
        let mesh = octahedron();
        let outcome = simplify_locked(&mesh, &request(1.0, vec![false; 6])).unwrap();
        assert!(outcome.identity_bypass);
        assert_eq!(outcome.mesh, mesh);
        assert_eq!(outcome.retained_original_vertex_ids, vec![0, 1, 2, 3, 4, 5]);
    }
    #[test]
    fn unlocked_mesh_reduces_and_stays_strict() {
        let outcome = simplify_locked(&octahedron(), &request(0.5, vec![false; 6])).unwrap();
        assert!(outcome.output_face_count < outcome.input_face_count);
        outcome.mesh.audit_strict().unwrap();
    }
    #[test]
    fn all_locks_report_constrained_success() {
        let outcome = simplify_locked(&octahedron(), &request(0.25, vec![true; 6])).unwrap();
        assert_eq!(outcome.output_face_count, 8);
        assert_eq!(outcome.achieved_ratio, 1.0);
        assert_eq!(outcome.locked_vertex_count, 6);
    }
    #[test]
    fn protected_bits_survive() {
        let mesh = octahedron();
        let mut locks = vec![false; 6];
        locks[2] = true;
        let outcome = simplify_locked(&mesh, &request(0.5, locks)).unwrap();
        let unlocked = simplify_locked(&mesh, &request(0.5, vec![false; 6])).unwrap();
        let output = outcome
            .retained_original_vertex_ids
            .iter()
            .position(|&id| id == 2)
            .unwrap();
        assert_eq!(
            outcome.mesh.verts()[output][0].to_bits(),
            mesh.verts()[2][0].to_bits()
        );
        assert_eq!(
            outcome.mesh.verts()[output][1].to_bits(),
            mesh.verts()[2][1].to_bits()
        );
        assert!(unlocked.output_face_count < unlocked.input_face_count);
        assert!(!unlocked.retained_original_vertex_ids.contains(&2));
    }
    #[test]
    fn rejects_bad_boundaries() {
        let mesh = octahedron();
        for ratio in [f64::NAN, 0.0, -1.0, 1.01] {
            assert!(matches!(
                simplify_locked(&mesh, &request(ratio, vec![false; 6])),
                Err(SimplifyError::InvalidRatio { .. })
            ));
        }
        assert!(matches!(
            simplify_locked(&mesh, &request(0.5, vec![])),
            Err(SimplifyError::LockLength { .. })
        ));
        let overflow = Mesh::new(
            vec![[f64::MAX, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            vec![[0, 1, 2]],
        )
        .unwrap();
        assert!(matches!(
            simplify_locked(&overflow, &request(0.5, vec![false; 3])),
            Err(SimplifyError::CoordinateNotFiniteF32 { .. })
        ));
        let degenerate = Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .unwrap();
        assert!(matches!(
            simplify_locked(&degenerate, &request(0.5, vec![false; 3])),
            Err(SimplifyError::StrictAudit(_))
        ));
    }
    #[test]
    fn reports_nonbinary_quantization_without_leaking_it_to_output() {
        let mesh = octahedron();
        let outcome = simplify_locked(&mesh, &request(1.0, vec![false; 6])).unwrap();
        assert!(outcome.max_input_quantization_delta > 0.0);
        assert_eq!(
            outcome.mesh.verts()[0][0].to_bits(),
            mesh.verts()[0][0].to_bits()
        );
    }
    #[test]
    fn repeated_calls_are_identical() {
        let mesh = octahedron();
        let input = request(0.5, vec![false; 6]);
        assert_eq!(
            simplify_locked(&mesh, &input).unwrap(),
            simplify_locked(&mesh, &input).unwrap()
        );
    }

    /// The CI gate runs this under `PLINTH_THREADS=1`, `4`, and `16`; D0 is
    /// one whole-mesh meshopt call and must not vary with the workspace-wide
    /// scheduler setting.
    #[test]
    fn plinth_threads_does_not_change_the_deterministic_outcome() {
        let mesh = octahedron();
        let request = request(0.5, vec![false; 6]);
        let first = simplify_locked(&mesh, &request).unwrap();
        let second = simplify_locked(&mesh, &request).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn numpy_linear_percentiles_cover_tiny_odd_even_and_boundaries() {
        assert_eq!(linear_percentile(&[4.0], 0.99).unwrap(), 4.0);
        assert_eq!(linear_percentile(&[0.0, 10.0], 0.99).unwrap(), 9.9);
        assert_eq!(linear_percentile(&[0.0, 10.0, 20.0], 0.5).unwrap(), 10.0);
        assert_eq!(
            linear_percentile(&[0.0, 10.0, 20.0, 30.0], 0.0).unwrap(),
            0.0
        );
        assert_eq!(
            linear_percentile(&[0.0, 10.0, 20.0, 30.0], 1.0).unwrap(),
            30.0
        );
        assert!(
            (linear_percentile(&[0.0, 10.0, 20.0, 30.0], 0.999).unwrap() - 29.97).abs() < 1e-12
        );
    }

    #[test]
    fn batched_two_sided_probe_has_exact_parallel_plane_distances() {
        let original = triangle(1.0);
        let candidate = triangle(0.0);
        let backward = backward_error_map(&original, &candidate, probe_config(2.0)).unwrap();
        assert_eq!(backward, vec![1.0; 3]);
        let report = probe_two_sided(
            &original,
            &candidate,
            ProbePopulation::Full,
            probe_config(2.0),
        )
        .unwrap();
        assert_eq!(report.forward.selected_count, 1);
        assert_eq!(report.backward.selected_count, 3);
        assert_eq!(report.p99_mm, 1.0);
        assert_eq!(report.p99_9_mm, 1.0);
        assert_eq!(report.max_mm, Some(1.0));
    }

    #[test]
    fn backward_probe_exposes_a_lost_thin_feature_that_forward_misses() {
        let original = Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [3.0, 0.0, 0.0],
                [0.0, 3.0, 0.0],
                [1.0, 1.0, 10.0],
                [1.5, 1.0, 10.0],
                [1.0, 1.5, 10.0],
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        )
        .unwrap();
        let candidate = Mesh::new(
            vec![[0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [0.0, 3.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .unwrap();
        let original_surface = NativeSurface::from_indexed(
            original.verts(),
            original.tris(),
            probe_config(20.0).surface,
        )
        .unwrap();
        let candidate_centres = face_centres(&candidate).unwrap();
        let (forward_distances, _) = original_surface.query(&candidate_centres, 20.0);
        assert!(forward_distances
            .iter()
            .all(|distance| distance.to_bits() == 0.0_f64.to_bits()));
        let report = probe_two_sided(
            &original,
            &candidate,
            ProbePopulation::Full,
            probe_config(20.0),
        )
        .unwrap();
        assert_eq!(report.forward.selected_count, 1);
        assert!(report.p99_mm > 9.9);
        let map = backward_error_map(&original, &candidate, probe_config(20.0)).unwrap();
        assert_eq!(&map[..3], &[0.0, 0.0, 0.0]);
        assert!(map[3..].iter().all(|&distance| distance > 9.9));
    }

    #[test]
    fn capped_selection_is_seeded_sorted_and_reproducible() {
        let (first_indices, first) = select_points(
            FORWARD_CAP + 11,
            ProbePopulation::Sampled,
            FORWARD_CAP,
            FORWARD_SEED,
        );
        let (again_indices, again) = select_points(
            FORWARD_CAP + 11,
            ProbePopulation::Sampled,
            FORWARD_CAP,
            FORWARD_SEED,
        );
        let (_, different_seed) = select_points(
            BACKWARD_CAP + 11,
            ProbePopulation::Sampled,
            BACKWARD_CAP,
            BACKWARD_SEED,
        );
        assert_eq!(first.selected_count, FORWARD_CAP);
        assert_eq!(first.seed, Some(1));
        assert_eq!(first, again);
        assert_eq!(first_indices, again_indices);
        assert!(first_indices.windows(2).all(|pair| pair[0] < pair[1]));
        assert_ne!(
            first.selected_index_digest,
            different_seed.selected_index_digest
        );
        let (full_indices, full) =
            select_points(3, ProbePopulation::Full, FORWARD_CAP, FORWARD_SEED);
        assert_eq!(full_indices, vec![0, 1, 2]);
        assert_eq!(full.seed, None);
    }

    #[test]
    fn bounded_no_hit_and_invalid_config_are_typed() {
        let original = triangle(1.0);
        let candidate = triangle(0.0);
        assert!(matches!(
            backward_error_map(&original, &candidate, probe_config(0.5)),
            Err(ProbeError::NoHit {
                side: "backward",
                ..
            })
        ));
        assert!(matches!(
            probe_two_sided(
                &original,
                &candidate,
                ProbePopulation::Full,
                probe_config(f64::INFINITY)
            ),
            Err(ProbeError::InvalidNearestUpperBound { .. })
        ));
    }

    #[test]
    fn probe_report_is_thread_count_invariant() {
        let original = triangle(1.0);
        let candidate = triangle(0.0);
        let first = probe_two_sided(
            &original,
            &candidate,
            ProbePopulation::Full,
            probe_config(2.0),
        )
        .unwrap();
        let second = probe_two_sided(
            &original,
            &candidate,
            ProbePopulation::Full,
            probe_config(2.0),
        )
        .unwrap();
        assert_eq!(first, second);
    }

    fn d2_probe_config() -> ProbeConfig {
        ProbeConfig {
            surface: NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: usize::MAX,
            },
            nearest_upper_bound_mm: 10_000.0,
        }
    }

    fn d2_budget(pixel_um: f64) -> DecimationBudget {
        DecimationBudget::new(PositiveFinite::new(pixel_um).unwrap(), d2_probe_config())
    }

    fn locked_box(size: f64) -> (Mesh, Vec<bool>) {
        let mesh = plinth_mesh::make_box([size, size, size], [0.0, 0.0, 0.0]).unwrap();
        let locks = vec![true; mesh.verts().len()];
        (mesh, locks)
    }

    fn point_cloud_mesh(points: Vec<[f64; 3]>) -> Mesh {
        // Feature selection only owns the original global vertex ID space; it
        // does not inspect triangles after D1 has provided the error map.
        Mesh::new(points, vec![[0, 1, 2]]).unwrap()
    }

    fn feature_policy(cap_fraction: f64) -> FeatureProtectionPolicy {
        FeatureProtectionPolicy {
            probe_ratio: 0.2,
            seed_threshold_mm: 0.05,
            halo_radius_mm: 0.4,
            cap_fraction,
            backoff_attempts: 3,
            seed_multiplier: 2.0,
            halo_multiplier: 0.6,
        }
    }

    fn probe_with_percentiles(p99_mm: f64, p99_9_mm: f64) -> TwoSidedProbe {
        TwoSidedProbe {
            candidate_face_count: 1,
            forward: SelectionEvidence {
                source_count: 1,
                selected_count: 1,
                seed: None,
                selected_index_digest: 0,
            },
            backward: SelectionEvidence {
                source_count: 1,
                selected_count: 1,
                seed: None,
                selected_index_digest: 0,
            },
            p99_mm,
            p99_9_mm,
            max_mm: Some(p99_9_mm),
            distance_digest: 0,
        }
    }

    fn trace_with_budget(budget: ErrorBudget) -> BudgetSearchTrace {
        BudgetSearchTrace {
            scale: ScaleSanity {
                dimensions_mm: [10.0, 10.0, 10.0],
                longest_axis_mm: 10.0,
            },
            budget,
            protected_vertex_count: 0,
            steps: Vec::new(),
        }
    }

    #[test]
    fn ratified_default_uses_the_registered_measured_coarser_pixel_axis() {
        let budget = DecimationBudget::ratified_default(d2_probe_config()).unwrap();
        let constants = m0_constants().unwrap();
        assert_eq!(
            budget.pixel_um.get(),
            *constants.saturn4_ultra().pixel_pitch_y_um().value()
        );
    }

    #[test]
    fn scale_sanity_accepts_exact_bounds_and_skips_only_outside_them() {
        for size in [10.0, 300.0] {
            let (mesh, locks) = locked_box(size);
            let outcome = run_budget_search(&mesh, &locks, d2_budget(1_000_000.0)).unwrap();
            assert!(
                matches!(outcome, BudgetSearchOutcome::Verified(_)),
                "{size}"
            );
        }
        for size in [9.999, 300.001] {
            let (mesh, locks) = locked_box(size);
            let outcome = run_budget_search(&mesh, &locks, d2_budget(1_000_000.0)).unwrap();
            assert!(matches!(
                outcome,
                BudgetSearchOutcome::SkippedScaleSuspicious { .. }
            ));
        }
    }

    #[test]
    fn ratio_high_unreachable_stops_before_bisection_and_returns_no_candidate() {
        let mesh = Mesh::new(
            vec![
                [5.0, 0.0, 0.0],
                [-5.0, 0.0, 0.0],
                [0.0, 5.0, 0.0],
                [0.0, -5.0, 0.0],
                [0.0, 0.0, 5.0],
                [0.0, 0.0, -5.0],
            ],
            vec![
                [0, 2, 4],
                [2, 1, 4],
                [1, 3, 4],
                [3, 0, 4],
                [2, 0, 5],
                [1, 2, 5],
                [3, 1, 5],
                [0, 3, 5],
            ],
        )
        .unwrap();
        let error = run_budget_search(&mesh, &[false; 6], d2_budget(1e-100)).unwrap_err();
        match error {
            BudgetSearchError::DecimationBudgetUnreachable { trace } => {
                assert_eq!(trace.steps.len(), 1);
                assert_eq!(trace.steps[0].ratio, 0.5);
                assert!(!trace.steps[0].passed);
            }
            other => panic!("expected named unreachable error, got {other:?}"),
        }
    }

    #[test]
    fn five_bisections_select_the_lowest_observed_passing_ratio_with_honest_locks() {
        let (mesh, locks) = locked_box(10.0);
        let outcome = run_budget_search(&mesh, &locks, d2_budget(1_000_000.0)).unwrap();
        let BudgetSearchOutcome::Verified(success) = outcome else {
            panic!("locked printable mesh must verify");
        };
        assert_eq!(
            success.trace.steps.len(),
            6,
            "one high probe plus five bisections"
        );
        assert!(success.trace.steps.iter().all(|step| step.passed));
        let expected = success
            .trace
            .steps
            .iter()
            .map(|step| step.ratio)
            .min_by(f64::total_cmp)
            .unwrap();
        assert_eq!(success.chosen_ratio, expected);
        assert!(success.mesh.audit_strict().is_ok());
        assert_eq!(success.mesh.tris().len(), mesh.tris().len());
        assert_eq!(success.trace.protected_vertex_count, mesh.verts().len());
    }

    #[test]
    fn p99_and_p999_are_independent_search_limits_and_slack_is_final_only() {
        let budget = ErrorBudget {
            p99_mm: 1.0,
            p99_9_mm: 2.0,
        };
        assert!(!within_budget(&probe_with_percentiles(1.01, 2.0), budget));
        assert!(!within_budget(&probe_with_percentiles(1.0, 2.01), budget));
        assert!(within_budget(&probe_with_percentiles(1.0, 2.0), budget));
        let slack = search_policy().unwrap().final_slack;
        assert!(within_final_slack(
            &probe_with_percentiles(budget.p99_mm * slack, budget.p99_9_mm * slack),
            budget,
            slack,
        ));
        assert!(!within_final_slack(
            &probe_with_percentiles(budget.p99_mm * (slack + 0.0001), budget.p99_9_mm * slack,),
            budget,
            slack,
        ));
        assert!(
            !within_budget(&probe_with_percentiles(1.01, 2.0), budget),
            "the 1.1x final slack must never relax search or precondition probes"
        );
        let error = verify_final_evidence(
            &trace_with_budget(budget),
            EdgeTopologyCounts::default(),
            probe_with_percentiles(budget.p99_mm * (slack + 0.0001), budget.p99_9_mm),
            EdgeTopologyCounts::default(),
            budget,
            slack,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            BudgetSearchError::VerificationFailed {
                failure
            } if matches!(failure.reason, VerificationFailureReason::BudgetSlackExceeded { .. })
        ));
    }

    #[test]
    fn topology_guard_refuses_either_canonical_edge_count_increase() {
        let before = EdgeTopologyCounts {
            boundary_edges: 2,
            nonmanifold_edges: 1,
        };
        assert!(topology_not_increased(before, before));
        assert!(!topology_not_increased(
            before,
            EdgeTopologyCounts {
                boundary_edges: 3,
                nonmanifold_edges: 1,
            }
        ));
        let error = verify_final_evidence(
            &trace_with_budget(ErrorBudget {
                p99_mm: 1.0,
                p99_9_mm: 1.0,
            }),
            before,
            probe_with_percentiles(0.0, 0.0),
            EdgeTopologyCounts {
                boundary_edges: 2,
                nonmanifold_edges: 2,
            },
            ErrorBudget {
                p99_mm: 1.0,
                p99_9_mm: 1.0,
            },
            1.1,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            BudgetSearchError::VerificationFailed {
                failure
            } if matches!(failure.reason, VerificationFailureReason::TopologyIncreased { .. })
        ));
        assert!(!topology_not_increased(
            before,
            EdgeTopologyCounts {
                boundary_edges: 2,
                nonmanifold_edges: 2,
            }
        ));
    }

    #[test]
    fn budget_search_is_deterministic_under_the_thread_count_gate() {
        let (mesh, locks) = locked_box(10.0);
        let mut outcomes = Vec::new();
        for _threads in [1, 4, 16] {
            outcomes.push(run_budget_search(&mesh, &locks, d2_budget(1_000_000.0)).unwrap());
        }
        assert_eq!(outcomes[0], outcomes[1]);
        assert_eq!(outcomes[1], outcomes[2]);
    }

    #[test]
    fn feature_seeds_use_only_the_backward_error_map() {
        let original = Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [3.0, 0.0, 0.0],
                [0.0, 3.0, 0.0],
                [1.0, 1.0, 10.0],
                [1.5, 1.0, 10.0],
                [1.0, 1.5, 10.0],
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        )
        .unwrap();
        let candidate = Mesh::new(
            vec![[0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [0.0, 3.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .unwrap();
        let backward = backward_error_map(&original, &candidate, probe_config(20.0)).unwrap();
        assert_eq!(&backward[..3], &[0.0, 0.0, 0.0]);
        let (locks, trace) = select_feature_protection_from_errors(
            &original,
            &backward,
            FeatureProtectionPolicy {
                halo_radius_mm: 0.0,
                cap_fraction: 1.0,
                ..feature_policy(1.0)
            },
        )
        .unwrap();
        assert_eq!(trace.attempts[0].seed_count, 3);
        assert_eq!(locks, vec![false, false, false, true, true, true]);
    }

    #[test]
    fn feature_halo_is_inclusive_and_scattered_to_original_ids() {
        let mut points = vec![
            [0.0, 0.0, 0.0],
            [0.2, 0.0, 0.0],
            [0.4, 0.0, 0.0],
            [0.400_001, 0.0, 0.0],
        ];
        points.extend((4..25).map(|id| [10.0 + id as f64, 0.0, 0.0]));
        let original = point_cloud_mesh(points);
        let mut errors = vec![0.0; 25];
        errors[0] = 0.051;
        let (locks, trace) =
            select_feature_protection_from_errors(&original, &errors, feature_policy(0.12))
                .unwrap();
        assert_eq!(trace.cap_count, 3);
        assert_eq!(trace.attempts[0].protected_vertex_count, 3);
        assert_eq!(&locks[..4], &[true, true, true, false]);
        assert_eq!(locks.iter().filter(|&&locked| locked).count(), 3);
    }

    #[test]
    fn feature_cap_uses_exact_floor_twelve_percent() {
        let original = point_cloud_mesh((0..25).map(|id| [id as f64, 0.0, 0.0]).collect());
        let mut errors = vec![0.0; 25];
        errors[0] = 0.1;
        errors[2] = 0.1;
        errors[4] = 0.1;
        let (locks, trace) =
            select_feature_protection_from_errors(&original, &errors, feature_policy(0.12))
                .unwrap();
        assert_eq!(trace.cap_count, 3);
        assert_eq!(locks.iter().filter(|&&locked| locked).count(), 3);
    }

    #[test]
    fn feature_cap_backoff_is_bounded_and_deterministic() {
        let original = point_cloud_mesh((0..25).map(|id| [id as f64, 0.0, 0.0]).collect());
        let mut errors = vec![0.0; 25];
        for id in [0, 2, 4, 6] {
            errors[id] = 1.0;
        }
        let failure =
            select_feature_protection_from_errors(&original, &errors, feature_policy(0.12))
                .unwrap_err();
        let FeatureProtectionError::CapExceeded { trace } = failure else {
            panic!("cap exhaustion must be typed")
        };
        assert_eq!(trace.cap_count, 3);
        assert_eq!(
            trace.attempts.len(),
            4,
            "initial attempt plus exactly three backoffs"
        );
        assert_eq!(
            trace
                .attempts
                .iter()
                .map(|attempt| attempt.seed_threshold_mm)
                .collect::<Vec<_>>(),
            vec![0.05, 0.1, 0.2, 0.4]
        );
        assert_eq!(
            trace
                .attempts
                .iter()
                .map(|attempt| attempt.halo_radius_mm)
                .collect::<Vec<_>>(),
            vec![0.4, 0.4 * 0.6, 0.4 * 0.6 * 0.6, 0.4 * 0.6 * 0.6 * 0.6]
        );
        assert!(trace
            .attempts
            .iter()
            .all(|attempt| attempt.protected_vertex_count > trace.cap_count));
    }

    #[test]
    fn feature_locks_are_hard_constraints_in_simplification() {
        let mesh = octahedron();
        let mut errors = vec![0.0; mesh.verts().len()];
        errors[2] = 1.0;
        let (locks, _) = select_feature_protection_from_errors(
            &mesh,
            &errors,
            FeatureProtectionPolicy {
                halo_radius_mm: 0.0,
                cap_fraction: 1.0,
                ..feature_policy(1.0)
            },
        )
        .unwrap();
        let outcome = simplify_locked(&mesh, &request(0.5, locks)).unwrap();
        assert!(outcome.retained_original_vertex_ids.contains(&2));
    }

    #[test]
    fn production_feature_selection_pins_the_ratio_twenty_probe() {
        let (mesh, _) = locked_box(10.0);
        let error = select_feature_protection(&mesh, d2_probe_config()).unwrap_err();
        let FeatureProtectionError::CapExceeded { trace } = error else {
            panic!("the production path must refuse a candidate when the cap cannot be met")
        };
        assert_eq!(trace.probe_ratio, 0.2);
    }
}
