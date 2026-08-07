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
    RadiusQueryLimits,
};
use meshopt::{simplify_with_locks, typed_to_bytes, SimplifyOptions, VertexDataAdapter};
use plinth_core::{m0_constants, PositiveFinite};
use plinth_mesh::{
    build_face_topology, mesh_fingerprint, EdgeTopologyCounts, Mesh, StrictMeshAuditError,
    WeldPolicy, WeldedView,
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
    /// Output vertex number -> deterministic representative original source
    /// vertex number, in ascending representative-ID order.  Exact-coordinate
    /// aliases share one output vertex; a locked alias is preferred as that
    /// welded vertex's representative. This is the authoritative remapping for
    /// later stages.
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
    #[error("could not build exact-coordinate simplification topology: {message}")]
    WeldView { message: String },
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

impl ProbeConfig {
    /// Constructs explicit probe limits without exposing downstream crates to
    /// `geom_kernel` merely to pass this stage request through.
    #[must_use]
    pub const fn new(
        grid_pitch_mm: f64,
        max_grid_bytes: usize,
        nearest_upper_bound_mm: f64,
    ) -> Self {
        Self {
            surface: NativeSurfaceConfig {
                grid_pitch_mm,
                max_grid_bytes,
            },
            nearest_upper_bound_mm,
        }
    }
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

    /// Refuses incomplete or impossible execution inputs before any probe
    /// surface allocation or native call. There is intentionally no default
    /// probe configuration: the load-to-decimate integration owns supplying a
    /// finite profile-derived request.
    pub fn validate_for_execution(self) -> Result<(), BudgetConfigError> {
        if !self.probe.surface.grid_pitch_mm.is_finite() || self.probe.surface.grid_pitch_mm <= 0.0
        {
            return Err(BudgetConfigError::InvalidGridPitch {
                value: self.probe.surface.grid_pitch_mm,
            });
        }
        if self.probe.surface.max_grid_bytes == 0 || self.probe.surface.max_grid_bytes == usize::MAX
        {
            return Err(BudgetConfigError::InvalidGridBytes {
                value: self.probe.surface.max_grid_bytes,
            });
        }
        if !self.probe.nearest_upper_bound_mm.is_finite() || self.probe.nearest_upper_bound_mm < 0.0
        {
            return Err(BudgetConfigError::InvalidNearestUpperBound {
                value: self.probe.nearest_upper_bound_mm,
            });
        }
        Ok(())
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
    /// Canonical digest of the sampled candidate. A final re-build must match
    /// this before its evidence can be accepted.
    pub candidate_fingerprint: String,
    pub probe: TwoSidedProbe,
    /// A requested ratio below one is not success merely because meshoptimizer
    /// accepted it.  Production decimation must actually remove faces.
    pub reduced: bool,
    pub passed: bool,
}

/// Deterministic evidence from the precondition and every bisection probe.
#[derive(Clone, Debug, PartialEq)]
pub struct BudgetSearchTrace {
    pub scale: ScaleSanity,
    pub budget: ErrorBudget,
    pub protected_vertex_count: usize,
    pub steps: Vec<BudgetSearchStep>,
    /// Every full-population attempt, in deterministic most-aggressive-first
    /// order. A failed attempt never suppresses denser sampled-safe candidates.
    pub final_attempts: Vec<FinalVerificationAttempt>,
    pub adaptive_exploration: AdaptiveExplorationTrace,
    /// Why no safely reduced mesh was authorized when this trace backs an
    /// [`BudgetSearchOutcome::AlreadyPrintScale`] result.
    pub reduction_refusals: Vec<ReductionRefusalReason>,
}

/// Full verification evidence for one sampled-safe, genuinely reduced QEM
/// candidate.  The candidate is deterministically rebuilt to avoid retaining
/// several production-scale meshes during the sampled search.
#[derive(Clone, Debug, PartialEq)]
pub struct FinalVerificationAttempt {
    pub requested_ratio: f64,
    pub achieved_ratio: f64,
    pub output_face_count: usize,
    pub candidate_fingerprint: String,
    /// Absent when topology refused the candidate before the expensive full
    /// population probe.
    pub probe: Option<TwoSidedProbe>,
    pub topology_after: EdgeTopologyCounts,
    pub result: FinalVerificationResult,
    pub downstream_utility: DownstreamUtilityEvidence,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdaptiveExplorationTrace {
    pub probe_limit: usize,
    pub probes: Vec<AdaptiveProbeStep>,
    pub remaining_intervals: Vec<IntegerTargetInterval>,
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntegerTargetInterval {
    pub first_target_faces: usize,
    pub last_target_faces: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdaptiveProbeStep {
    pub target_face_count: usize,
    pub output_face_count: usize,
    pub candidate_fingerprint: String,
    pub plateau_of_fingerprint: Option<String>,
    pub sampled_probe: TwoSidedProbe,
    pub topology_after: Option<EdgeTopologyCounts>,
    pub full_probe: Option<TwoSidedProbe>,
    pub result: AdaptiveProbeResult,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdaptiveProbeResult {
    NoActualReduction,
    Plateau,
    SampledBudgetExceeded,
    TopologyIncreased,
    FullBudgetExceeded,
    GeometricallyEligible,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FinalVerificationResult {
    GeometricallyEligible,
    BudgetExceeded,
    TopologyIncreased,
}

/// The one registered AcceptSafe downstream-utility witness.
///
/// Reducing the support-mesh face count `F` strictly reduces the demand on two
/// *registered* execution-authority resources whose demand is an exact function
/// of `F` alone: the minima-kNN batch's `source_count` (which is `F` itself,
/// capped by `SUPPORT_KNN_MAX_WORK_PER_FACE`) and the `81·F` per-triangle term
/// of the parity-grid projection `81·F + 36·R + 4`, capped by
/// `SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE`.
///
/// The claim deliberately stops there. It does **not** assert that the `36·R`
/// reference term falls: `R/F` is measured to *rise* as a mesh coarsens (526
/// references per face on the 244-face Cylinder against 2.83 on the 429,120-face
/// Weeping Treant v1.2), so total `R` falls sublinearly and could in principle
/// be flat. Saying only what is exactly provable is what keeps this witness from
/// inheriting the n = 1 treant coefficients.
pub const SUPPORT_FACE_SCALED_RESOURCE_DEMAND: &str = "support-face-scaled-resource-demand";

/// Opaque proof that a reduced mesh saves registered downstream work or a
/// registered bounded resource. The field stays private and the type has no
/// public constructor: the only way to obtain one is for
/// `assess_downstream_utility` to satisfy every clause of a registered witness.
///
/// ```compile_fail,E0451
/// use plinth_decimate::DownstreamUseful;
///
/// let _ = DownstreamUseful { witness_id: "unregistered" };
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownstreamUseful {
    witness_id: &'static str,
}

impl DownstreamUseful {
    /// Mints the witness. Crate-private on purpose, and the thing it withholds
    /// is *minting*, not *naming*: the registered id is exported as
    /// [`SUPPORT_FACE_SCALED_RESOURCE_DEMAND`], so any caller may name it, read
    /// it, and compare it against [`DownstreamUseful::witness_id`]. What no
    /// caller outside this crate can do is construct a `DownstreamUseful` —
    /// with that id or any other — because this constructor is crate-private
    /// and the field is private. A witness is therefore always something
    /// [`assess_downstream_utility`] produced by satisfying every clause, never
    /// something a caller asserted by writing the right string.
    const fn witness(witness_id: &'static str) -> Self {
        Self { witness_id }
    }

    #[must_use]
    pub const fn witness_id(&self) -> &'static str {
        self.witness_id
    }
}

/// Independent production-acceptance authority. Geometric eligibility alone
/// never creates `DownstreamUseful`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownstreamUtilityEvidence {
    DownstreamUseful(DownstreamUseful),
    NoDownstreamUtilityEvidence,
}

/// Evidence that a requested simplification was correctly refused while the
/// canonical original remained safe to print.  These are successful policy
/// outcomes, not errors hidden behind an identity mesh.
#[derive(Clone, Debug, PartialEq)]
pub enum ReductionRefusalReason {
    ProtectionCap {
        trace: Box<FeatureProtectionTrace>,
    },
    NoActualReduction {
        requested_ratio: f64,
        input_face_count: usize,
        output_face_count: usize,
    },
    SampledBudgetExceeded {
        requested_ratio: f64,
        probe: TwoSidedProbe,
        budget: ErrorBudget,
    },
    FinalBudgetExceeded {
        requested_ratio: f64,
        probe: TwoSidedProbe,
        budget: ErrorBudget,
        slack: f64,
    },
    TopologyIncreased {
        requested_ratio: f64,
        before: EdgeTopologyCounts,
        after: EdgeTopologyCounts,
    },
    NoDownstreamUtilityEvidence {
        geometrically_eligible_candidate_count: usize,
    },
}

/// An opaque verified production candidate. Only this crate can construct one;
/// callers may inspect immutable verification evidence or consume it to take
/// ownership of the mesh.
#[derive(Debug, PartialEq)]
pub struct BudgetSearchSuccess {
    mesh: Mesh,
    chosen_ratio: f64,
    trace: BudgetSearchTrace,
    final_probe: TwoSidedProbe,
    topology_before: EdgeTopologyCounts,
    topology_after: EdgeTopologyCounts,
}

impl BudgetSearchSuccess {
    /// Borrows the verified production mesh for a disk-audit or rendering
    /// boundary without transferring its one-shot downstream authorization.
    /// The caller cannot mutate this mesh or construct another verified result.
    #[must_use]
    pub const fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    /// Consumes the verified result and transfers its production mesh to the
    /// next pipeline stage. No cloneable verified authorization is retained.
    #[must_use]
    pub fn into_mesh(self) -> Mesh {
        self.mesh
    }

    #[must_use]
    pub const fn chosen_ratio(&self) -> f64 {
        self.chosen_ratio
    }

    #[must_use]
    pub const fn trace(&self) -> &BudgetSearchTrace {
        &self.trace
    }

    #[must_use]
    pub const fn final_probe(&self) -> &TwoSidedProbe {
        &self.final_probe
    }

    #[must_use]
    pub const fn topology_before(&self) -> EdgeTopologyCounts {
        self.topology_before
    }

    #[must_use]
    pub const fn topology_after(&self) -> EdgeTopologyCounts {
        self.topology_after
    }
}

/// Successful scale validation either produces one verified mesh or a named,
/// candidate-free scale skip.
#[derive(Debug, PartialEq)]
pub enum BudgetSearchOutcome {
    SkippedScaleSuspicious {
        scale: ScaleSanity,
    },
    Verified(Box<BudgetSearchSuccess>),
    /// The original passed a strict canonical identity audit, but no genuinely
    /// reduced candidate met the printer-resolution, topology, and protection
    /// contract.  It owns the same one-shot verified capability as `Verified`.
    AlreadyPrintScale(Box<BudgetSearchSuccess>),
}

/// The production disposition carried across the orientation boundary.
///
/// `Reduced` requires every clause of a registered [`DownstreamUseful`] witness
/// (see `assess_downstream_utility`); geometric eligibility alone still never
/// authorizes a reduced production mesh. As of 2026-08-07 the one registered
/// witness is [`SUPPORT_FACE_SCALED_RESOURCE_DEMAND`], and no corpus model
/// reaches it: five refuse at `ProtectionCap`, `Sphere` and `make_toed_slab`
/// sit below `DECIMATION_UTILITY_MIN_INPUT_FACES`, and the Weeping Treant v1.2
/// is stopped by the topology gate before the witness is ever consulted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecimationDisposition {
    Reduced,
    AlreadyPrintScale,
}

/// Complete immutable M3 evidence retained through every downstream history.
/// Fields are private so callers cannot forge accepted evidence; read-only
/// accessors expose the original trace without recomputation or lossy copying.
///
/// ```compile_fail,E0451
/// use plinth_decimate::{DecimationAudit, DecimationDisposition};
///
/// let _ = DecimationAudit {
///     disposition: DecimationDisposition::AlreadyPrintScale,
///     input_faces: 1,
///     output_faces: 1,
///     chosen_ratio: 1.0,
///     trace: todo!(),
///     final_probe: todo!(),
///     topology_before: todo!(),
///     topology_after: todo!(),
/// };
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct DecimationAudit {
    disposition: DecimationDisposition,
    input_faces: usize,
    output_faces: usize,
    chosen_ratio: f64,
    trace: BudgetSearchTrace,
    final_probe: TwoSidedProbe,
    topology_before: EdgeTopologyCounts,
    topology_after: EdgeTopologyCounts,
}

impl DecimationAudit {
    #[must_use]
    pub const fn disposition(&self) -> DecimationDisposition {
        self.disposition
    }

    #[must_use]
    pub const fn trace(&self) -> &BudgetSearchTrace {
        &self.trace
    }

    #[must_use]
    pub const fn final_probe(&self) -> &TwoSidedProbe {
        &self.final_probe
    }

    #[must_use]
    pub const fn topology_before(&self) -> EdgeTopologyCounts {
        self.topology_before
    }

    #[must_use]
    pub const fn topology_after(&self) -> EdgeTopologyCounts {
        self.topology_after
    }

    /// Returns the stable summary projection without exposing writable fields.
    #[must_use]
    pub const fn report(&self) -> DecimationReport<'_> {
        DecimationReport { audit: self }
    }
}

/// Read-only summary projection over the complete retained audit.
#[derive(Clone, Copy, Debug)]
pub struct DecimationReport<'a> {
    audit: &'a DecimationAudit,
}

impl DecimationReport<'_> {
    #[must_use]
    pub const fn disposition(self) -> DecimationDisposition {
        self.audit.disposition
    }

    #[must_use]
    pub const fn input_faces(self) -> usize {
        self.audit.input_faces
    }

    #[must_use]
    pub const fn output_faces(self) -> usize {
        self.audit.output_faces
    }

    #[must_use]
    pub const fn chosen_ratio(self) -> f64 {
        self.audit.chosen_ratio
    }

    #[must_use]
    pub const fn p99_mm(self) -> f64 {
        self.audit.final_probe.p99_mm
    }

    #[must_use]
    pub const fn p99_9_mm(self) -> f64 {
        self.audit.final_probe.p99_9_mm
    }
}

/// The sole move-only decimation value accepted by orientation.
#[derive(Debug)]
pub struct DecimatedModel {
    mesh: Mesh,
    audit: DecimationAudit,
}

impl DecimatedModel {
    #[must_use]
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub const fn audit(&self) -> &DecimationAudit {
        &self.audit
    }

    #[must_use]
    pub const fn report(&self) -> DecimationReport<'_> {
        self.audit.report()
    }

    /// Moves the only production mesh and its passive evidence into orientation.
    #[must_use]
    pub fn into_parts(self) -> (Mesh, DecimationAudit) {
        (self.mesh, self.audit)
    }
}

impl BudgetSearchOutcome {
    /// Converts a successful M3 outcome into the existing orientation boundary.
    /// A scale skip remains explicit and cannot manufacture a production mesh.
    pub fn into_decimated_model(self) -> Result<DecimatedModel, ScaleSanity> {
        let (success, disposition) = match self {
            Self::SkippedScaleSuspicious { scale } => return Err(scale),
            Self::Verified(success) => (success, DecimationDisposition::Reduced),
            Self::AlreadyPrintScale(success) => (success, DecimationDisposition::AlreadyPrintScale),
        };
        let BudgetSearchSuccess {
            mesh,
            chosen_ratio,
            trace,
            final_probe,
            topology_before,
            topology_after,
        } = *success;
        let output_faces = mesh.tris().len();
        let input_faces = trace
            .steps
            .first()
            .map_or(output_faces, |step| step.output_face_count);
        Ok(DecimatedModel {
            mesh,
            audit: DecimationAudit {
                disposition,
                input_faces,
                output_faces,
                chosen_ratio,
                trace,
                final_probe,
                topology_before,
                topology_after,
            },
        })
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum BudgetConfigError {
    #[error("registered M0 constants are unavailable: {message}")]
    Constants { message: String },
    #[error("registered pixel pitch cannot form a positive finite budget: {message}")]
    PixelPitch { message: String },
    #[error("probe grid pitch must be positive and finite, got {value}")]
    InvalidGridPitch { value: f64 },
    #[error("probe grid byte cap must be finite and nonzero, got {value}")]
    InvalidGridBytes { value: usize },
    #[error("probe nearest upper bound must be finite and nonnegative, got {value}")]
    InvalidNearestUpperBound { value: f64 },
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

/// Typed M3 budget-search failures.  An exhausted safe-reduction search is
/// deliberately not an error: it returns `AlreadyPrintScale` instead.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum BudgetSearchError {
    #[error("registered M0 constants are unavailable: {message}")]
    Constants { message: String },
    #[error("{phase} failed before a verified candidate existed: {message}")]
    SearchExecution {
        phase: &'static str,
        message: String,
    },
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
    /// Source-order vertices selected by the geometric halo before the
    /// structural survival closure.
    pub halo_vertex_count: usize,
    /// Additional source-order aliases locked so every protected welded
    /// position owns at least one fully locked incident triangle.
    pub lock_closure_added_vertex_count: usize,
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
    #[error("feature-protection operational lock topology failed: {message}")]
    OperationalLockTopology { message: String },
    #[error(
        "protected exact-coordinate vertex represented by source vertex {source_vertex} has no incident triangle"
    )]
    ProtectedVertexWithoutIncidentFace { source_vertex: usize },
    #[error(
        "FeatureProtectionCapExceeded: protected vertices exceed the 12% cap after bounded backoff"
    )]
    CapExceeded { trace: Box<FeatureProtectionTrace> },
}

/// Typed failures from the sole public production decimation path.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum DecimationError {
    #[error(transparent)]
    Config(#[from] BudgetConfigError),
    #[error(transparent)]
    FeatureProtection(#[from] FeatureProtectionError),
    #[error(transparent)]
    Budget(#[from] BudgetSearchError),
}

/// The only public M3 production entry point. It selects protected original
/// vertices from one ratio-0.20 backward probe, then runs the D2 budget search
/// with exactly those locks. The lock-accepting search helper is private, so an
/// external caller cannot obtain a verified production mesh by supplying an
/// all-false lock vector.
///
/// ```compile_fail,E0603
/// use plinth_decimate::run_budget_search;
/// ```
///
/// ```compile_fail,E0451
/// use plinth_decimate::{BudgetSearchOutcome, BudgetSearchSuccess};
///
/// let forged = BudgetSearchSuccess {
///     mesh: todo!(),
///     chosen_ratio: 0.2,
///     trace: todo!(),
///     final_probe: todo!(),
///     topology_before: todo!(),
///     topology_after: todo!(),
/// };
/// let _ = BudgetSearchOutcome::Verified(Box::new(forged));
/// ```
///
/// ```compile_fail,E0616
/// use plinth_decimate::BudgetSearchSuccess;
///
/// fn rewrite_evidence(success: &mut BudgetSearchSuccess) {
///     success.chosen_ratio = 1.0;
/// }
/// ```
///
/// ```compile_fail,E0596
/// use plinth_decimate::BudgetSearchSuccess;
///
/// fn mutate_read_only_trace(success: &BudgetSearchSuccess) {
///     success.trace().steps.clear();
/// }
/// ```
///
/// ```compile_fail,E0599
/// use plinth_decimate::BudgetSearchSuccess;
///
/// fn duplicate_authorization(success: BudgetSearchSuccess) {
///     let _copy = success.clone();
/// }
/// ```
pub fn run(
    original: &Mesh,
    budget: DecimationBudget,
) -> Result<BudgetSearchOutcome, DecimationError> {
    budget.validate_for_execution()?;
    // Scale is a named pre-search SKIP.  Do this before the feature probe so a
    // deliberately out-of-scale mesh cannot turn into an unrelated cap error.
    let policy = search_policy()?;
    let scale = scale_sanity(original);
    if !scale.longest_axis_mm.is_finite()
        || scale.longest_axis_mm < policy.scale_min_mm
        || scale.longest_axis_mm > policy.scale_max_mm
    {
        return Ok(BudgetSearchOutcome::SkippedScaleSuspicious { scale });
    }

    match select_feature_protection(original, budget.probe) {
        Ok((protected, _)) => Ok(run_budget_search(original, &protected, budget)?),
        Err(FeatureProtectionError::CapExceeded {
            trace: protection_trace,
        }) => {
            let trace = new_budget_trace(scale, budget, &policy, 0);
            Ok(already_print_scale(
                original,
                budget.probe,
                trace,
                ReductionRefusalReason::ProtectionCap {
                    trace: protection_trace,
                },
            )?)
        }
        Err(error) => Err(error.into()),
    }
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
    utility: UtilityPolicy,
}

/// The registered authorities behind the downstream-utility witness, read once
/// per run rather than once per candidate.
///
/// The three deviation bounds are `Derived` from the vendor pitch and the
/// owner-ratified print floor; the face threshold is a `Placeholder`. That
/// split is the point: the threshold decides only *whether to attempt*, never
/// *whether the result is safe*.
#[derive(Clone, Copy, Debug, PartialEq)]
struct UtilityPolicy {
    min_input_faces: usize,
    max_p99_deviation_mm: f64,
    max_p99_9_deviation_mm: f64,
    max_absolute_deviation_mm: f64,
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
    let lock_topology = OperationalLockTopology::build(original)?;
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
            let max_output_pairs = seed_points.len().saturating_mul(original.verts().len());
            let max_output_bytes = seed_points
                .len()
                .saturating_add(1)
                .saturating_mul(std::mem::size_of::<usize>())
                .saturating_add(
                    max_output_pairs
                        .saturating_mul(std::mem::size_of::<u64>() + std::mem::size_of::<f64>()),
                );
            let neighbors = index
                .radius_neighbors_batch(
                    &seed_points,
                    radius,
                    RadiusQueryLimits {
                        max_output_pairs,
                        max_output_bytes,
                    },
                )
                .map_err(|error| FeatureProtectionError::HaloQuery {
                    message: point_query_message(error),
                })?;
            for point_id in neighbors.point_ids {
                let vertex =
                    usize::try_from(point_id).expect("point index IDs originate from usize");
                locks[vertex] = true;
            }
        }
        let halo_vertex_count = locks.iter().filter(|&&locked| locked).count();
        let closure = lock_topology.close(&locks)?;
        let protected_vertex_count = closure
            .source_locks
            .iter()
            .filter(|&&locked| locked)
            .count();
        trace.attempts.push(FeatureProtectionAttempt {
            attempt,
            seed_threshold_mm: threshold,
            halo_radius_mm: radius,
            seed_count: seed_ids.len(),
            halo_vertex_count,
            lock_closure_added_vertex_count: protected_vertex_count - halo_vertex_count,
            protected_vertex_count,
        });
        if protected_vertex_count <= cap_count {
            return Ok((closure.source_locks, trace));
        }
        threshold *= policy.seed_multiplier;
        radius *= policy.halo_multiplier;
    }
    Err(FeatureProtectionError::CapExceeded {
        trace: Box::new(trace),
    })
}

struct OperationalLockTopology {
    source_to_welded: Vec<u32>,
    representative_source: Vec<u32>,
    faces: Vec<[u32; 3]>,
    incident_faces: Vec<Vec<usize>>,
}

struct OperationalLockClosure {
    source_locks: Vec<bool>,
}

impl OperationalLockTopology {
    fn build(mesh: &Mesh) -> Result<Self, FeatureProtectionError> {
        let view = WeldedView::build(mesh, WeldPolicy::ExactBits).map_err(|error| {
            FeatureProtectionError::OperationalLockTopology {
                message: error.to_string(),
            }
        })?;
        let source_to_welded = view.map().to_vec();
        let mut representative_source = vec![u32::MAX; view.vertex_count() as usize];
        for (source, &welded) in source_to_welded.iter().enumerate() {
            let representative = &mut representative_source[welded as usize];
            if *representative == u32::MAX {
                *representative = source as u32;
            }
        }
        let faces = mesh
            .tris()
            .iter()
            .map(|triangle| triangle.map(|source| source_to_welded[source as usize]))
            .collect::<Vec<_>>();
        let mut incident_faces = vec![Vec::new(); view.vertex_count() as usize];
        for (face, triangle) in faces.iter().enumerate() {
            for &vertex in triangle {
                let incidents = &mut incident_faces[vertex as usize];
                if incidents.last().copied() != Some(face) {
                    incidents.push(face);
                }
            }
        }
        Ok(Self {
            source_to_welded,
            representative_source,
            faces,
            incident_faces,
        })
    }

    /// Meshoptimizer's lock bit prevents a locked vertex from moving, but it
    /// does not prevent all of that vertex's incident triangles from becoming
    /// degenerate when its unlocked neighbors collapse.  For each geometric
    /// halo vertex, reserve one incident triangle.  The locally minimal face
    /// adds the fewest currently-unlocked welded vertices; source face order is
    /// the deterministic tie break.  Every newly locked vertex lies in that
    /// same fully locked triangle, so the one pass closes the complete final
    /// operational lock set.
    fn close(
        &self,
        source_locks: &[bool],
    ) -> Result<OperationalLockClosure, FeatureProtectionError> {
        debug_assert_eq!(source_locks.len(), self.source_to_welded.len());
        let mut welded_locks = vec![false; self.incident_faces.len()];
        for (&welded, &locked) in self.source_to_welded.iter().zip(source_locks) {
            welded_locks[welded as usize] |= locked;
        }
        let halo_welded_vertices = welded_locks
            .iter()
            .enumerate()
            .filter_map(|(vertex, &locked)| locked.then_some(vertex))
            .collect::<Vec<_>>();

        for vertex in halo_welded_vertices {
            let incidents = &self.incident_faces[vertex];
            if incidents.is_empty() {
                return Err(FeatureProtectionError::ProtectedVertexWithoutIncidentFace {
                    source_vertex: self.representative_source[vertex] as usize,
                });
            }
            if incidents
                .iter()
                .any(|&face| self.faces[face].iter().all(|&v| welded_locks[v as usize]))
            {
                continue;
            }
            let face = *incidents
                .iter()
                .min_by_key(|&&face| {
                    (
                        self.faces[face]
                            .iter()
                            .filter(|&&v| !welded_locks[v as usize])
                            .count(),
                        face,
                    )
                })
                .expect("nonempty incidents checked above");
            for &face_vertex in &self.faces[face] {
                welded_locks[face_vertex as usize] = true;
            }
        }

        let source_locks = self
            .source_to_welded
            .iter()
            .map(|&welded| welded_locks[welded as usize])
            .collect();
        Ok(OperationalLockClosure { source_locks })
    }
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

/// Runs M3's quality-first reduction search over the accepted D0/D1
/// primitives. Ratio 1.0 is an identity audit/evidence endpoint, never a
/// purported reduction.
///
/// The returned [`BudgetSearchOutcome`] makes a scale skip explicit and keeps
/// it candidate-free.  Every non-skip success owns exactly one final mesh;
/// all failed paths retain only deterministic evidence, never an exportable
/// candidate.
fn run_budget_search(
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
    let mut trace = new_budget_trace(
        scale,
        budget,
        &policy,
        protected.iter().filter(|&&locked| locked).count(),
    );

    // This canonical identity probe establishes that the original itself is
    // auditable and exactly within budget. It is intentionally not eligible to
    // win the reduction search.
    let initial = canonical_identity_step(original, protected, budget.probe).map_err(|error| {
        BudgetSearchError::SearchExecution {
            phase: "ratio-high precondition",
            message: error,
        }
    })?;
    let initial_passed = within_budget(&initial.probe, error_budget)
        && !initial.reduced
        && initial.output_face_count == original.tris().len();
    trace.steps.push(initial);
    if !initial_passed {
        return Err(BudgetSearchError::SearchExecution {
            phase: "ratio-1 canonical identity audit",
            message: "identity candidate exceeded a zero-error safety expectation".to_owned(),
        });
    }

    let mut low = policy.ratio_low;
    let mut high = policy.ratio_high;
    for _ in 0..policy.bisections {
        let midpoint = (low + high) / 2.0;
        let step = sampled_step(original, protected, midpoint, budget.probe, error_budget)
            .map_err(|error| BudgetSearchError::SearchExecution {
                phase: "ratio bisection",
                message: error,
            })?;
        match (step.reduced, step.passed) {
            // A real, budget-safe candidate lets us try for still fewer faces.
            (true, true) => high = midpoint,
            // A real candidate that exceeds a measured limit needs less
            // simplification (a higher requested ratio).
            (true, false) => low = midpoint,
            // A meshoptimizer no-op must never win.  A lower requested ratio
            // is the only direction that could produce a real reduction.
            (false, _) => high = midpoint,
        }
        record_sampled_refusal(&mut trace, &step, original.tris().len(), error_budget);
        trace.steps.push(step);
    }

    let mut candidates = trace
        .steps
        .iter()
        .filter(|step| step.reduced && step.passed)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        // Compare actual ratios without rounding, then requested target as a
        // deterministic tie-breaker.
        (left.output_face_count as u128 * original.tris().len() as u128)
            .cmp(&(right.output_face_count as u128 * original.tris().len() as u128))
            .then_with(|| left.ratio.total_cmp(&right.ratio))
    });
    if candidates.is_empty() {
        record_unexplored_dense_interval(original.tris().len(), &mut trace);
        return already_print_scale(
            original,
            budget.probe,
            trace,
            ReductionRefusalReason::NoActualReduction {
                requested_ratio: policy.ratio_high,
                input_face_count: original.tris().len(),
                output_face_count: original.tris().len(),
            },
        );
    }

    for candidate in candidates {
        let final_outcome = simplify_locked(
            original,
            &SimplifyRequest {
                target_ratio: candidate.ratio,
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
        let rebuilt_fingerprint = mesh_fingerprint(&final_outcome.mesh).map_err(|error| {
            BudgetSearchError::SearchExecution {
                phase: "final candidate fingerprint",
                message: error.to_string(),
            }
        })?;
        if rebuilt_fingerprint != candidate.candidate_fingerprint {
            return Err(BudgetSearchError::SearchExecution {
                phase: "final candidate deterministic rebuild",
                message: format!(
                    "requested ratio {} sampled fingerprint {} rebuilt as {}",
                    candidate.ratio, candidate.candidate_fingerprint, rebuilt_fingerprint
                ),
            });
        }
        let topology_after = build_face_topology(&final_outcome.mesh, WeldPolicy::ExactBits)
            .map(|topology| topology.edge_counts())
            .map_err(|error| {
                verification_failed(
                    trace.clone(),
                    topology_before,
                    None,
                    None,
                    VerificationFailureReason::TopologyAudit {
                        message: error.to_string(),
                    },
                )
            })?;
        if !topology_not_increased(topology_before, topology_after) {
            trace.final_attempts.push(FinalVerificationAttempt {
                requested_ratio: candidate.ratio,
                achieved_ratio: final_outcome.achieved_ratio,
                output_face_count: final_outcome.output_face_count,
                candidate_fingerprint: rebuilt_fingerprint,
                probe: None,
                topology_after,
                result: FinalVerificationResult::TopologyIncreased,
                downstream_utility: DownstreamUtilityEvidence::NoDownstreamUtilityEvidence,
            });
            trace
                .reduction_refusals
                .push(ReductionRefusalReason::TopologyIncreased {
                    requested_ratio: candidate.ratio,
                    before: topology_before,
                    after: topology_after,
                });
            continue;
        }
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
                Some(topology_after),
                VerificationFailureReason::FinalProbe {
                    message: error.to_string(),
                },
            )
        })?;
        let result = classify_final_attempt(
            &final_probe,
            error_budget,
            policy.final_slack,
            topology_before,
            topology_after,
        );
        let downstream_utility = assess_downstream_utility(
            policy.utility,
            &final_outcome,
            &final_probe,
            topology_before,
            topology_after,
        );
        trace.final_attempts.push(FinalVerificationAttempt {
            requested_ratio: candidate.ratio,
            achieved_ratio: final_outcome.achieved_ratio,
            output_face_count: final_outcome.output_face_count,
            candidate_fingerprint: rebuilt_fingerprint,
            probe: Some(final_probe.clone()),
            topology_after,
            result: result.clone(),
            downstream_utility: downstream_utility.clone(),
        });
        match result {
            FinalVerificationResult::GeometricallyEligible => {
                if let DownstreamUtilityEvidence::DownstreamUseful(_) = downstream_utility {
                    return Ok(BudgetSearchOutcome::Verified(Box::new(
                        BudgetSearchSuccess {
                            mesh: final_outcome.mesh,
                            chosen_ratio: final_outcome.achieved_ratio,
                            trace,
                            final_probe,
                            topology_before,
                            topology_after,
                        },
                    )));
                }
            }
            FinalVerificationResult::BudgetExceeded => {
                trace
                    .reduction_refusals
                    .push(ReductionRefusalReason::FinalBudgetExceeded {
                        requested_ratio: candidate.ratio,
                        probe: final_probe,
                        budget: error_budget,
                        slack: policy.final_slack,
                    });
            }
            FinalVerificationResult::TopologyIncreased => {
                unreachable!("topology is audited before the expensive full-population probe")
            }
        }
    }

    run_adaptive_diagnostics(
        original,
        protected,
        budget,
        topology_before,
        &policy,
        &mut trace,
    )?;

    let geometrically_eligible_candidate_count = trace
        .final_attempts
        .iter()
        .filter(|attempt| attempt.result == FinalVerificationResult::GeometricallyEligible)
        .count()
        + trace
            .adaptive_exploration
            .probes
            .iter()
            .filter(|probe| probe.result == AdaptiveProbeResult::GeometricallyEligible)
            .count();
    already_print_scale(
        original,
        budget.probe,
        trace,
        ReductionRefusalReason::NoDownstreamUtilityEvidence {
            geometrically_eligible_candidate_count,
        },
    )
}

fn record_unexplored_dense_interval(input_faces: usize, trace: &mut BudgetSearchTrace) {
    let Some(first_target_faces) = trace
        .steps
        .iter()
        .filter(|step| step.reduced)
        .map(|step| step.output_face_count)
        .max()
        .and_then(|faces| faces.checked_add(1))
    else {
        return;
    };
    if first_target_faces < input_faces {
        trace.adaptive_exploration.remaining_intervals = vec![IntegerTargetInterval {
            first_target_faces,
            last_target_faces: input_faces - 1,
        }];
        trace.adaptive_exploration.complete = false;
    }
}

/// The registered `support-face-scaled-resource-demand` witness predicate.
///
/// Every clause must hold. Three read evidence the search already computes; two
/// are new. In clause order:
///
/// - **Threshold.** `input_face_count >= DECIMATION_UTILITY_MIN_INPUT_FACES`.
///   This is checked *first and here*, rather than in front of the search, so
///   that every trace below the threshold stays byte-identical to the
///   pre-witness engine — same `final_attempts`, same
///   `NoDownstreamUtilityEvidence` refusal, same returned `original.clone()`.
///   The seven frozen OD-8 census digests are therefore frozen *structurally*,
///   not merely measured to be unmoved.
/// - **U1 — genuine reduction.** `output_face_count < input_face_count`.
/// - **U2 — bounded typical deviation.** `p99 <= DECIMATION_MAX_P99_DEVIATION_MM`
///   and `p99.9 <= DECIMATION_MAX_P999_DEVIATION_MM`. Numerically identical to
///   what the search enforces, and deliberately re-asserted against the
///   `Derived` constants rather than the `Placeholder` search factors: no slack
///   multiplier applies here, so a candidate admitted on the final ×1.1 slack
///   still fails this clause.
/// - **U3 — bounded worst-case deviation.** `max <= DECIMATION_MAX_ABSOLUTE_DEVIATION_MM`.
///   The genuinely new physics: p99.9 leaves 0.1 % of samples unbounded, and on
///   a 429 k-face mesh that is ~1,700 points, any one of which could be a
///   collapsed twig. An absent `max_mm` is a refusal, never a pass.
/// - **U4 — structural invariants downstream stages assume.**
///   `topology_not_increased`, the same predicate the search gate uses, because
///   `stitch_layer` genuinely requires degree-2 adjacency. Strict audit is
///   already enforced inside `compact_result` and inside every probe. OD-9
///   exterior-ness is relied on, not re-proved: the contact machinery screens it
///   later. **This clause is defence in depth and is unreachable in production
///   today** — see the note at the clause itself.
/// - **U5 — the registered saving.** The two execution-authority resources whose
///   demand is an exact function of face count alone must strictly fall: the
///   minima-kNN batch's `source_count` (`= F`) and the `81·F` per-triangle term
///   of the parity-grid projection. Those two demands are proportional by
///   construction, so this is one comparison and not two — see
///   [`registered_face_scaled_demand_falls`]. It is implied by U1, and is
///   computed anyway so the registered claim is machine-checked rather than
///   asserted in prose. It stops at those two terms on purpose — see
///   [`SUPPORT_FACE_SCALED_RESOURCE_DEMAND`].
fn assess_downstream_utility(
    policy: UtilityPolicy,
    candidate: &SimplifyOutcome,
    final_probe: &TwoSidedProbe,
    topology_before: EdgeTopologyCounts,
    topology_after: EdgeTopologyCounts,
) -> DownstreamUtilityEvidence {
    // A non-finite deviation is a refusal, not a comparison: `NaN > limit` is
    // false, so an unguarded `>` would read a missing measurement as a pass.
    let Some(max_mm) = final_probe.max_mm.filter(|value| value.is_finite()) else {
        return DownstreamUtilityEvidence::NoDownstreamUtilityEvidence;
    };
    if candidate.input_face_count < policy.min_input_faces
        || candidate.output_face_count >= candidate.input_face_count
        || !final_probe.p99_mm.is_finite()
        || !final_probe.p99_9_mm.is_finite()
        || final_probe.p99_mm > policy.max_p99_deviation_mm
        || final_probe.p99_9_mm > policy.max_p99_9_deviation_mm
        || max_mm > policy.max_absolute_deviation_mm
        // U4. Unreachable from `run_budget_search` as that function is ordered
        // today: it applies `topology_not_increased` to the same two counts and
        // `continue`s on failure BEFORE it runs the full-population probe or
        // calls this predicate, so every candidate that arrives here has
        // already satisfied this clause and it can never be the clause that
        // refuses. It is kept deliberately rather than deleted as dead code:
        // it is the only place the *witness* — as opposed to the search — takes
        // responsibility for the invariant `stitch_layer` needs, the unit tests
        // pin it in isolation, and a future reordering that probed before it
        // gated topology (or a second caller of this predicate) would otherwise
        // silently lose the guard rather than fail a test. Its strength is
        // bounded by FU-26: the counter it reads under-counts non-manifold
        // edges and cannot see duplicate faces at all, so it is a floor on the
        // damage, not a measurement of it.
        || !topology_not_increased(topology_before, topology_after)
        || !registered_face_scaled_demand_falls(
            candidate.input_face_count,
            candidate.output_face_count,
        )
    {
        return DownstreamUtilityEvidence::NoDownstreamUtilityEvidence;
    }
    DownstreamUtilityEvidence::DownstreamUseful(DownstreamUseful::witness(
        SUPPORT_FACE_SCALED_RESOURCE_DEMAND,
    ))
}

/// U5's arithmetic — one clause, because the two registered demands are
/// exactly proportional by construction.
///
/// The witness names two *resources*: the minima-kNN batch's `source_count`,
/// which is `F` itself and is capped by `SUPPORT_KNN_MAX_WORK_PER_FACE`, and
/// the `81·F` per-triangle term of the parity projection `81·F + 36·R + 4`
/// (`rust/geom_kernel/src/parity.rs`), capped by
/// `SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE`. They are not two independent
/// *quantities*: `81·F` is `F` multiplied by a positive integer constant fixed
/// in the kernel, so over non-negative face counts `81·out < 81·in` is `out <
/// in` restated. Asserting both would be one comparison written twice and
/// would read as two checks where there is one, so the clause is stated once
/// and the proportionality is what makes the single comparison cover both
/// named resources. `u5_is_one_clause_because_both_registered_demands_are_\
/// proportional_to_the_face_count` pins that equivalence, including at the
/// `81 · usize::MAX` width where the scaled form needs `u128` not to wrap.
///
/// The `36·R` reference term is deliberately absent from the claim, and is the
/// reason this predicate cannot simply be "the mesh got smaller": `R` is the
/// one term of that projection that is *not* an exact function of `F`, and it
/// is measured to fall sublinearly (526 references per face on the 244-face
/// Cylinder against 2.83 on the 429,120-face treant). See
/// [`SUPPORT_FACE_SCALED_RESOURCE_DEMAND`].
fn registered_face_scaled_demand_falls(input_faces: usize, output_faces: usize) -> bool {
    output_faces < input_faces
}

fn record_sampled_refusal(
    trace: &mut BudgetSearchTrace,
    step: &BudgetSearchStep,
    input_face_count: usize,
    budget: ErrorBudget,
) {
    if !step.reduced {
        trace
            .reduction_refusals
            .push(ReductionRefusalReason::NoActualReduction {
                requested_ratio: step.ratio,
                input_face_count,
                output_face_count: step.output_face_count,
            });
    } else if !step.passed {
        trace
            .reduction_refusals
            .push(ReductionRefusalReason::SampledBudgetExceeded {
                requested_ratio: step.ratio,
                probe: step.probe.clone(),
                budget,
            });
    }
}

fn classify_final_attempt(
    probe: &TwoSidedProbe,
    budget: ErrorBudget,
    slack: f64,
    topology_before: EdgeTopologyCounts,
    topology_after: EdgeTopologyCounts,
) -> FinalVerificationResult {
    if !within_final_slack(probe, budget, slack) {
        FinalVerificationResult::BudgetExceeded
    } else if !topology_not_increased(topology_before, topology_after) {
        FinalVerificationResult::TopologyIncreased
    } else {
        FinalVerificationResult::GeometricallyEligible
    }
}

fn new_budget_trace(
    scale: ScaleSanity,
    budget: DecimationBudget,
    policy: &SearchPolicy,
    protected_vertex_count: usize,
) -> BudgetSearchTrace {
    BudgetSearchTrace {
        scale,
        budget: ErrorBudget {
            p99_mm: budget.pixel_um.get() * policy.p99_factor,
            p99_9_mm: budget.pixel_um.get() * policy.p99_9_factor,
        },
        protected_vertex_count,
        steps: Vec::with_capacity(policy.bisections + 1),
        final_attempts: Vec::with_capacity(policy.bisections),
        adaptive_exploration: AdaptiveExplorationTrace {
            probe_limit: policy.bisections,
            probes: Vec::with_capacity(policy.bisections),
            remaining_intervals: Vec::new(),
            complete: true,
        },
        reduction_refusals: Vec::new(),
    }
}

/// Authorize the canonical input only after a strict audit and full, two-sided
/// zero-error identity probe.  This is the sole identity success path; callers
/// cannot use a lower-ratio meshoptimizer no-op as a substitute.
fn already_print_scale(
    original: &Mesh,
    probe_config: ProbeConfig,
    mut trace: BudgetSearchTrace,
    reason: ReductionRefusalReason,
) -> Result<BudgetSearchOutcome, BudgetSearchError> {
    original
        .audit_strict()
        .map_err(|error| BudgetSearchError::SearchExecution {
            phase: "canonical identity strict audit",
            message: error.to_string(),
        })?;
    let topology = build_face_topology(original, WeldPolicy::ExactBits)
        .map(|topology| topology.edge_counts())
        .map_err(|error| BudgetSearchError::SearchExecution {
            phase: "canonical identity topology audit",
            message: error.to_string(),
        })?;
    let final_probe =
        exact_identity_probe(original, ProbePopulation::Full, probe_config).map_err(|error| {
            BudgetSearchError::SearchExecution {
                phase: "canonical identity full probe",
                message: error.to_string(),
            }
        })?;
    if final_probe.p99_mm != 0.0 || final_probe.p99_9_mm != 0.0 || final_probe.max_mm != Some(0.0) {
        return Err(BudgetSearchError::SearchExecution {
            phase: "canonical identity full probe",
            message: "identity probe was not exactly zero".to_owned(),
        });
    }
    trace.reduction_refusals.push(reason);
    Ok(BudgetSearchOutcome::AlreadyPrintScale(Box::new(
        BudgetSearchSuccess {
            mesh: original.clone(),
            chosen_ratio: 1.0,
            trace,
            final_probe,
            topology_before: topology,
            topology_after: topology,
        },
    )))
}

fn canonical_identity_step(
    original: &Mesh,
    protected: &[bool],
    probe_config: ProbeConfig,
) -> Result<BudgetSearchStep, String> {
    let outcome = simplify_locked(
        original,
        &SimplifyRequest {
            target_ratio: 1.0,
            vertex_locks: protected.to_vec(),
        },
    )
    .map_err(|error| error.to_string())?;
    if !outcome.identity_bypass
        || outcome.output_face_count != original.tris().len()
        || outcome.mesh != *original
    {
        return Err("ratio-1 simplification did not preserve canonical source identity".to_owned());
    }
    let probe = exact_identity_probe(original, ProbePopulation::Sampled, probe_config)
        .map_err(|error| error.to_string())?;
    Ok(BudgetSearchStep {
        ratio: 1.0,
        output_face_count: outcome.output_face_count,
        output_vertex_count: outcome.output_vertex_count,
        candidate_fingerprint: mesh_fingerprint(&outcome.mesh)
            .map_err(|error| error.to_string())?,
        probe,
        reduced: false,
        passed: false,
    })
}

fn exact_identity_probe(
    mesh: &Mesh,
    population: ProbePopulation,
    config: ProbeConfig,
) -> Result<TwoSidedProbe, ProbeError> {
    // The candidate is the very same canonical mesh, established above by
    // exact bitwise identity.  Its two directed surface distances are therefore
    // exactly zero; routing the fact through a floating nearest-query kernel
    // would manufacture numerical noise into an identity proof.
    audit_probe_inputs(mesh, mesh, config)?;
    let (_, forward) = select_points(mesh.tris().len(), population, FORWARD_CAP, FORWARD_SEED);
    let (_, backward) = select_points(mesh.verts().len(), population, BACKWARD_CAP, BACKWARD_SEED);
    let zero_count = forward.selected_count + backward.selected_count;
    if zero_count == 0 {
        return Err(ProbeError::EmptyPopulation);
    }
    Ok(TwoSidedProbe {
        candidate_face_count: mesh.tris().len(),
        forward,
        backward,
        p99_mm: 0.0,
        p99_9_mm: 0.0,
        max_mm: Some(0.0),
        distance_digest: digest_f64_bits(&vec![0.0; zero_count]),
    })
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
    let reduced = outcome.output_face_count < original.tris().len();
    let passed = reduced && within_budget(&probe, budget);
    Ok(BudgetSearchStep {
        ratio,
        output_face_count: outcome.output_face_count,
        output_vertex_count: outcome.output_vertex_count,
        candidate_fingerprint: mesh_fingerprint(&outcome.mesh)
            .map_err(|error| error.to_string())?,
        probe,
        reduced,
        passed,
    })
}

fn ratio_for_integer_target(input_faces: usize, target_faces: usize) -> Result<f64, String> {
    if input_faces == 0 || target_faces == 0 || target_faces >= input_faces {
        return Err(format!(
            "integer target {target_faces} must be within 1..{input_faces}"
        ));
    }
    let ratio = (target_faces as f64 + 0.25) / input_faces as f64;
    if target_face_count(input_faces, ratio) != target_faces {
        return Err(format!(
            "ratio {ratio} did not round-trip to integer target {target_faces}"
        ));
    }
    Ok(ratio)
}

fn run_adaptive_diagnostics(
    original: &Mesh,
    protected: &[bool],
    budget: DecimationBudget,
    topology_before: EdgeTopologyCounts,
    policy: &SearchPolicy,
    trace: &mut BudgetSearchTrace,
) -> Result<(), BudgetSearchError> {
    let input_faces = original.tris().len();
    let Some(first_target_faces) = trace
        .final_attempts
        .iter()
        .map(|attempt| attempt.output_face_count)
        .max()
        .and_then(|faces| faces.checked_add(1))
    else {
        return Ok(());
    };
    if first_target_faces >= input_faces {
        return Ok(());
    }

    let mut intervals = vec![IntegerTargetInterval {
        first_target_faces,
        last_target_faces: input_faces - 1,
    }];
    let mut known_fingerprints = trace
        .steps
        .iter()
        .map(|step| step.candidate_fingerprint.clone())
        .chain(
            trace
                .final_attempts
                .iter()
                .map(|attempt| attempt.candidate_fingerprint.clone()),
        )
        .collect::<Vec<_>>();

    while trace.adaptive_exploration.probes.len() < policy.bisections && !intervals.is_empty() {
        let interval_index = intervals
            .iter()
            .enumerate()
            .max_by_key(|(_, interval)| interval.last_target_faces - interval.first_target_faces)
            .map(|(index, _)| index)
            .expect("non-empty interval list");
        let interval = intervals.remove(interval_index);
        let target_face_count = interval.first_target_faces
            + (interval.last_target_faces - interval.first_target_faces) / 2;
        if interval.first_target_faces < target_face_count {
            intervals.push(IntegerTargetInterval {
                first_target_faces: interval.first_target_faces,
                last_target_faces: target_face_count - 1,
            });
        }
        if target_face_count < interval.last_target_faces {
            intervals.push(IntegerTargetInterval {
                first_target_faces: target_face_count + 1,
                last_target_faces: interval.last_target_faces,
            });
        }
        intervals.sort_by_key(|item| (item.first_target_faces, item.last_target_faces));

        let ratio =
            ratio_for_integer_target(input_faces, target_face_count).map_err(|message| {
                BudgetSearchError::SearchExecution {
                    phase: "adaptive integer target",
                    message,
                }
            })?;
        let outcome = simplify_locked(
            original,
            &SimplifyRequest {
                target_ratio: ratio,
                vertex_locks: protected.to_vec(),
            },
        )
        .map_err(|error| BudgetSearchError::SearchExecution {
            phase: "adaptive simplification",
            message: error.to_string(),
        })?;
        let sampled_probe = probe_two_sided(
            original,
            &outcome.mesh,
            ProbePopulation::Sampled,
            budget.probe,
        )
        .map_err(|error| BudgetSearchError::SearchExecution {
            phase: "adaptive sampled probe",
            message: error.to_string(),
        })?;
        let candidate_fingerprint = mesh_fingerprint(&outcome.mesh).map_err(|error| {
            BudgetSearchError::SearchExecution {
                phase: "adaptive candidate fingerprint",
                message: error.to_string(),
            }
        })?;
        let reduced = outcome.output_face_count < input_faces;
        let passed = reduced && within_budget(&sampled_probe, trace.budget);
        let plateau_of_fingerprint = known_fingerprints
            .iter()
            .find(|fingerprint| **fingerprint == candidate_fingerprint)
            .cloned();
        let mut adaptive = AdaptiveProbeStep {
            target_face_count,
            output_face_count: outcome.output_face_count,
            candidate_fingerprint: candidate_fingerprint.clone(),
            plateau_of_fingerprint,
            sampled_probe,
            topology_after: None,
            full_probe: None,
            result: AdaptiveProbeResult::NoActualReduction,
        };
        adaptive.result = if !reduced {
            AdaptiveProbeResult::NoActualReduction
        } else if adaptive.plateau_of_fingerprint.is_some() {
            AdaptiveProbeResult::Plateau
        } else if !passed {
            AdaptiveProbeResult::SampledBudgetExceeded
        } else {
            let topology_after = build_face_topology(&outcome.mesh, WeldPolicy::ExactBits)
                .map(|topology| topology.edge_counts())
                .map_err(|error| BudgetSearchError::SearchExecution {
                    phase: "adaptive topology audit",
                    message: error.to_string(),
                })?;
            adaptive.topology_after = Some(topology_after);
            if !topology_not_increased(topology_before, topology_after) {
                AdaptiveProbeResult::TopologyIncreased
            } else {
                let full_probe =
                    probe_two_sided(original, &outcome.mesh, ProbePopulation::Full, budget.probe)
                        .map_err(|error| BudgetSearchError::SearchExecution {
                        phase: "adaptive full probe",
                        message: error.to_string(),
                    })?;
                let within = within_final_slack(&full_probe, trace.budget, policy.final_slack);
                adaptive.full_probe = Some(full_probe);
                if within {
                    AdaptiveProbeResult::GeometricallyEligible
                } else {
                    AdaptiveProbeResult::FullBudgetExceeded
                }
            }
        };
        known_fingerprints.push(candidate_fingerprint);
        trace.adaptive_exploration.probes.push(adaptive);
    }
    trace.adaptive_exploration.complete = intervals.is_empty();
    trace.adaptive_exploration.remaining_intervals = intervals;
    Ok(())
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

#[cfg(test)]
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
    let evidence = constants.evidence();
    let min_input_faces = usize::try_from(
        placeholders
            .decimation_utility_min_input_faces()
            .diagnostic()
            .get(),
    )
    .map_err(|error| BudgetSearchError::Constants {
        message: format!("downstream-utility face threshold cannot fit usize: {error}"),
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
        utility: UtilityPolicy {
            min_input_faces,
            max_p99_deviation_mm: *evidence.decimation_max_p99_deviation_mm().value(),
            max_p99_9_deviation_mm: *evidence.decimation_max_p999_deviation_mm().value(),
            max_absolute_deviation_mm: *evidence.decimation_max_absolute_deviation_mm().value(),
        },
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
    if request.target_ratio == 1.0 {
        let (_, max_input_quantization_delta) = quantize_positions(mesh.verts())?;
        return Ok(identity_outcome(
            mesh,
            request,
            target_faces,
            max_input_quantization_delta,
            locked_vertex_count,
        ));
    }

    let welded = prepare_welded_input(mesh, &request.vertex_locks)?;
    let position_source = if welded.is_identity {
        mesh.verts()
    } else {
        welded.view.verts()
    };
    let (positions, max_input_quantization_delta) = quantize_positions(position_source)?;
    let indices = flatten_mapped_indices(mesh.tris(), welded.view.map());
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
        &welded.vertex_locks,
        target_faces * 3,
        f32::MAX,
        SimplifyOptions::ErrorAbsolute,
        Some(&mut advisory_error),
    );
    compact_result(
        mesh,
        request,
        &welded,
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

struct WeldedSimplificationInput {
    view: WeldedView,
    vertex_locks: Vec<bool>,
    representative_original_ids: Vec<u32>,
    is_identity: bool,
}

/// Builds the one explicit exact-coordinate indexing view required by QEM.
/// Binary STL remains an untouched triangle soup in `mesh`; only this private
/// simplifier input is welded.  Any locked source alias locks its entire
/// equivalence class, and the first locked alias (otherwise first alias) is
/// retained as deterministic f64 provenance.
fn prepare_welded_input(
    mesh: &Mesh,
    source_locks: &[bool],
) -> Result<WeldedSimplificationInput, SimplifyError> {
    let view = WeldedView::build(mesh, WeldPolicy::ExactBits).map_err(|error| {
        SimplifyError::WeldView {
            message: error.to_string(),
        }
    })?;
    let welded_count = view.vertex_count() as usize;
    let mut vertex_locks = vec![false; welded_count];
    let mut representative_original_ids = vec![u32::MAX; welded_count];
    let mut representative_is_locked = vec![false; welded_count];
    let mut is_identity = welded_count == mesh.verts().len();

    for (source, (&welded, &locked)) in view.map().iter().zip(source_locks).enumerate() {
        let welded = welded as usize;
        is_identity &= welded == source;
        if representative_original_ids[welded] == u32::MAX
            || (locked && !representative_is_locked[welded])
        {
            representative_original_ids[welded] = source as u32;
            representative_is_locked[welded] = locked;
        }
        vertex_locks[welded] |= locked;
    }

    debug_assert!(representative_original_ids
        .iter()
        .all(|&source| source != u32::MAX));
    Ok(WeldedSimplificationInput {
        view,
        vertex_locks,
        representative_original_ids,
        is_identity,
    })
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

fn flatten_mapped_indices(triangles: &[[u32; 3]], source_to_welded: &[u32]) -> Vec<u32> {
    let mut indices = Vec::with_capacity(triangles.len() * 3);
    for triangle in triangles {
        indices.extend(triangle.map(|source| source_to_welded[source as usize]));
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
    welded: &WeldedSimplificationInput,
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
    let mut retained = vec![false; welded.view.vertex_count() as usize];
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
    for (source, &locked) in request.vertex_locks.iter().enumerate() {
        let welded_vertex = welded.view.map()[source] as usize;
        if locked && !retained[welded_vertex] {
            return Err(SimplifyError::LockedVertexRemoved { vertex: source });
        }
    }
    let mut retained_welded_and_original = retained
        .iter()
        .enumerate()
        .filter_map(|(welded_vertex, &used)| {
            used.then_some((
                welded_vertex as u32,
                welded.representative_original_ids[welded_vertex],
            ))
        })
        .collect::<Vec<_>>();
    retained_welded_and_original.sort_unstable_by_key(|&(_, original)| original);
    let retained_original_vertex_ids = retained_welded_and_original
        .iter()
        .map(|&(_, original)| original)
        .collect::<Vec<_>>();
    let mut welded_to_output = vec![u32::MAX; retained.len()];
    let compact_vertices = retained_welded_and_original
        .iter()
        .enumerate()
        .map(|(output, &(welded_vertex, original))| {
            welded_to_output[welded_vertex as usize] = output as u32;
            mesh.verts()[original as usize]
        })
        .collect::<Vec<_>>();
    let compact_triangles = result
        .chunks_exact(3)
        .map(|face| {
            [
                welded_to_output[face[0] as usize],
                welded_to_output[face[1] as usize],
                welded_to_output[face[2] as usize],
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

    fn triangle_soup(mesh: &Mesh) -> Mesh {
        let mut vertices = Vec::with_capacity(mesh.tris().len() * 3);
        let mut triangles = Vec::with_capacity(mesh.tris().len());
        for triangle in mesh.tris() {
            let base = vertices.len() as u32;
            vertices.extend(triangle.map(|vertex| mesh.verts()[vertex as usize]));
            triangles.push([base, base + 1, base + 2]);
        }
        Mesh::new(vertices, triangles).unwrap()
    }

    fn captured_treant_locked_fan() -> Mesh {
        Mesh::new(
            vec![
                [-11.470499038696289, -6.670965671539307, 91.73716735839844],
                [-11.429944038391113, -6.334640026092529, 92.0300521850586],
                [-11.352010726928711, -6.334236145019531, 91.598388671875],
                [-11.398161888122559, -6.333822250366211, 91.85111236572266],
            ],
            vec![[0, 1, 2], [0, 2, 3], [1, 0, 3]],
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
    fn exact_welded_triangle_soup_reduces_through_the_same_qem_path() {
        let soup = triangle_soup(&octahedron());
        assert_eq!(soup.verts().len(), soup.tris().len() * 3);
        let outcome = simplify_locked(&soup, &request(0.5, vec![false; soup.verts().len()]))
            .expect("triangle-soup simplification");
        assert!(outcome.output_face_count < outcome.input_face_count);
        outcome.mesh.audit_strict().unwrap();
        let backward = backward_error_map(&soup, &outcome.mesh, probe_config(10.0)).unwrap();
        assert_eq!(backward.len(), soup.verts().len());
    }

    #[test]
    fn geometrically_distinct_one_ulp_seam_is_not_welded() {
        let soup = triangle_soup(&octahedron());
        assert_eq!(soup.verts()[1], soup.verts()[3]);
        let mut vertices = soup.verts().to_vec();
        vertices[3][1] = f64::from_bits(vertices[3][1].to_bits() + 1);
        let distinct = Mesh::new(vertices, soup.tris().to_vec()).unwrap();
        let locks = vec![false; distinct.verts().len()];
        let welded = prepare_welded_input(&distinct, &locks).unwrap();
        assert_ne!(welded.view.map()[1], welded.view.map()[3]);
        assert_eq!(welded.view.vertex_count(), 7);
    }

    #[test]
    fn physically_identical_signed_zero_coordinates_are_welded() {
        let mesh = Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [-0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            vec![[0, 2, 3], [1, 2, 3]],
        )
        .unwrap();
        let welded = prepare_welded_input(&mesh, &[false; 4]).unwrap();
        assert_eq!(welded.view.map()[0], welded.view.map()[1]);
        assert_eq!(welded.view.vertex_count(), 3);
    }

    #[test]
    fn lock_on_a_nonrepresentative_triangle_soup_alias_survives() {
        let soup = triangle_soup(&octahedron());
        // Soup IDs 1 and 3 are aliases of shared octahedron vertex 2. Lock
        // the later alias so the test proves conservative lock aggregation,
        // not accidental survival of the first-occurrence representative.
        assert_eq!(soup.verts()[1], soup.verts()[3]);
        let mut locks = vec![false; soup.verts().len()];
        locks[3] = true;
        let outcome = simplify_locked(&soup, &request(0.5, locks)).unwrap();
        let output = outcome
            .retained_original_vertex_ids
            .iter()
            .position(|&source| source == 3)
            .expect("locked alias is retained as deterministic provenance");
        assert_eq!(outcome.mesh.verts()[output], soup.verts()[3]);
        assert!(outcome.output_face_count < outcome.input_face_count);
    }

    #[test]
    fn shared_index_input_keeps_identity_weld_mapping_and_behavior() {
        let mesh = octahedron();
        let welded = prepare_welded_input(&mesh, &[false; 6]).unwrap();
        assert!(welded.is_identity);
        assert_eq!(welded.view.verts(), mesh.verts());
        assert_eq!(welded.view.map(), &[0, 1, 2, 3, 4, 5]);
        let outcome = simplify_locked(&mesh, &request(0.5, vec![false; 6])).unwrap();
        assert_eq!(outcome.output_face_count, 4);
    }

    #[test]
    fn captured_treant_fan_closes_a_bare_lock_over_the_minimum_incident_face() {
        let mesh = captured_treant_locked_fan();
        let mut locks = vec![false; 4];
        locks[0] = true;
        assert!(matches!(
            simplify_locked(&mesh, &request(0.34, locks.clone())),
            Err(SimplifyError::EmptyResult)
        ));
        let topology = OperationalLockTopology::build(&mesh).unwrap();
        let first = topology.close(&locks).unwrap();
        let second = topology.close(&locks).unwrap();
        assert_eq!(first.source_locks, vec![true, true, true, false]);
        assert_eq!(first.source_locks, second.source_locks);
        let outcome = simplify_locked(&mesh, &request(0.34, first.source_locks)).unwrap();
        assert!(outcome.output_face_count < mesh.tris().len());
        assert!(outcome.mesh.tris().iter().any(|triangle| {
            let mut sources =
                triangle.map(|output| outcome.retained_original_vertex_ids[output as usize]);
            sources.sort_unstable();
            sources == [0, 1, 2]
        }));
    }

    #[test]
    fn operational_lock_closure_is_part_of_every_cap_preflight() {
        let mesh = captured_treant_locked_fan();
        let mut errors = vec![0.0; 4];
        errors[0] = 1.0;
        let failure = select_feature_protection_from_errors(
            &mesh,
            &errors,
            FeatureProtectionPolicy {
                halo_radius_mm: 0.0,
                cap_fraction: 0.5,
                ..feature_policy(0.5)
            },
        )
        .unwrap_err();
        let FeatureProtectionError::CapExceeded { trace } = failure else {
            panic!("closure exceeding the cap must be a typed no-candidate failure")
        };
        assert_eq!(trace.cap_count, 2);
        assert_eq!(trace.attempts.len(), 4);
        assert!(trace.attempts.iter().all(|attempt| {
            attempt.halo_vertex_count == 1
                && attempt.lock_closure_added_vertex_count == 2
                && attempt.protected_vertex_count == 3
        }));
    }

    #[test]
    fn protected_unreferenced_vertex_is_a_typed_no_candidate_failure() {
        let mesh = Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [2.0, 2.0, 2.0],
            ],
            vec![[0, 1, 2]],
        )
        .unwrap();
        let errors = vec![0.0, 0.0, 0.0, 1.0];
        assert!(matches!(
            select_feature_protection_from_errors(
                &mesh,
                &errors,
                FeatureProtectionPolicy {
                    halo_radius_mm: 0.0,
                    cap_fraction: 1.0,
                    ..feature_policy(1.0)
                },
            ),
            Err(FeatureProtectionError::ProtectedVertexWithoutIncidentFace { source_vertex: 3 })
        ));
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

    /// Repeated D0 calls within one process produce the same result. The
    /// workspace thread-setting sweep is an isolated-process integration test
    /// (`tests/thread_sweep.rs`), because process-global environment mutation
    /// inside this parallel unit-test binary would be unsound.
    #[test]
    fn repeated_d0_calls_do_not_change_the_deterministic_outcome() {
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
            final_attempts: Vec::new(),
            adaptive_exploration: AdaptiveExplorationTrace {
                probe_limit: 0,
                probes: Vec::new(),
                remaining_intervals: Vec::new(),
                complete: true,
            },
            reduction_refusals: Vec::new(),
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
                matches!(
                    outcome,
                    BudgetSearchOutcome::Verified(_) | BudgetSearchOutcome::AlreadyPrintScale(_)
                ),
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
    fn exhausted_reduction_search_returns_audited_already_print_scale() {
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
        let outcome = run_budget_search(&mesh, &[false; 6], d2_budget(1e-100)).unwrap();
        let BudgetSearchOutcome::AlreadyPrintScale(success) = outcome else {
            panic!("unaffordable reductions must retain audited print-scale input");
        };
        assert_eq!(success.chosen_ratio(), 1.0);
        assert_eq!(success.mesh().tris().len(), mesh.tris().len());
        assert_eq!(success.final_probe().p99_mm, 0.0);
        assert_eq!(success.final_probe().p99_9_mm, 0.0);
        assert_eq!(success.trace().steps.len(), 6);
        assert!(success.trace().steps[1..]
            .iter()
            .all(|step| step.reduced && !step.passed));
        assert!(!success.trace().adaptive_exploration.complete);
        assert!(!success
            .trace()
            .adaptive_exploration
            .remaining_intervals
            .is_empty());
        assert_eq!(
            success
                .trace()
                .adaptive_exploration
                .remaining_intervals
                .last()
                .unwrap()
                .last_target_faces,
            mesh.tris().len() - 1
        );
        assert!(success
            .trace()
            .reduction_refusals
            .iter()
            .any(|reason| matches!(reason, ReductionRefusalReason::SampledBudgetExceeded { .. })));
    }

    #[test]
    fn five_bisections_select_the_lowest_observed_passing_ratio_with_honest_locks() {
        let (mesh, locks) = locked_box(10.0);
        let outcome = run_budget_search(&mesh, &locks, d2_budget(1_000_000.0)).unwrap();
        let BudgetSearchOutcome::AlreadyPrintScale(success) = outcome else {
            panic!("a fully protected simple mesh must be explicit print-scale identity");
        };
        assert_eq!(
            success.trace.steps.len(),
            6,
            "identity evidence plus five bisections"
        );
        assert!(!success.trace.steps[0].reduced);
        assert_eq!(success.chosen_ratio, 1.0);
        assert!(success.mesh.audit_strict().is_ok());
        assert_eq!(success.mesh.tris().len(), mesh.tris().len());
        assert_eq!(success.trace.protected_vertex_count, mesh.verts().len());
    }

    #[test]
    fn orientation_boundary_preserves_identity_without_registered_utility() {
        // A 12-face box is far below DECIMATION_UTILITY_MIN_INPUT_FACES, so the
        // witness refuses on its first clause and this whole path is
        // byte-identical to the engine that had no witness at all.
        let (mesh, locks) = locked_box(10.0);
        let outcome = run_budget_search(&mesh, &locks, d2_budget(1_000_000.0)).unwrap();
        let BudgetSearchOutcome::AlreadyPrintScale(success) = &outcome else {
            panic!("a mesh below the registered face threshold may not be authorized as reduced");
        };
        assert!(success
            .trace()
            .final_attempts
            .iter()
            .all(|attempt| matches!(
                attempt.downstream_utility,
                DownstreamUtilityEvidence::NoDownstreamUtilityEvidence
            )));
        let allocation = success.mesh().verts().as_ptr();
        let model = outcome
            .into_decimated_model()
            .expect("a successful identity outcome crosses the stage boundary");
        assert_eq!(model.mesh().verts().as_ptr(), allocation);
        assert_eq!(
            model.report().disposition(),
            DecimationDisposition::AlreadyPrintScale
        );
        assert_eq!(model.report().chosen_ratio(), 1.0);
        assert_eq!(model.report().input_faces(), mesh.tris().len());
        assert_eq!(model.report().output_faces(), mesh.tris().len());
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
    fn final_topology_refusal_does_not_disqualify_a_denser_sampled_safe_candidate() {
        let budget = ErrorBudget {
            p99_mm: 1.0,
            p99_9_mm: 1.0,
        };
        let before = EdgeTopologyCounts::default();
        // Synthetic final evidence mirrors the Treant regression: the lower
        // ratio is sampled-safe but opens non-manifold edges, while the denser
        // sampled-safe candidate is topology-safe and must remain eligible.
        let attempts = [
            classify_final_attempt(
                &probe_with_percentiles(0.0, 0.0),
                budget,
                1.1,
                before,
                EdgeTopologyCounts {
                    boundary_edges: 0,
                    nonmanifold_edges: 24,
                },
            ),
            classify_final_attempt(
                &probe_with_percentiles(0.0, 0.0),
                budget,
                1.1,
                before,
                before,
            ),
        ];
        assert_eq!(attempts[0], FinalVerificationResult::TopologyIncreased);
        assert_eq!(attempts[1], FinalVerificationResult::GeometricallyEligible);
        assert_eq!(
            attempts
                .iter()
                .position(|result| *result == FinalVerificationResult::GeometricallyEligible),
            Some(1),
            "the second, denser candidate is selected after the first refusal"
        );
    }

    #[test]
    fn final_error_refusal_does_not_disqualify_a_denser_sampled_safe_candidate() {
        let budget = ErrorBudget {
            p99_mm: 1.0,
            p99_9_mm: 1.0,
        };
        let before = EdgeTopologyCounts::default();
        let attempts = [
            classify_final_attempt(
                &probe_with_percentiles(1.101, 0.0),
                budget,
                1.1,
                before,
                before,
            ),
            classify_final_attempt(
                &probe_with_percentiles(0.0, 0.0),
                budget,
                1.1,
                before,
                before,
            ),
        ];
        assert_eq!(attempts[0], FinalVerificationResult::BudgetExceeded);
        assert_eq!(attempts[1], FinalVerificationResult::GeometricallyEligible);
    }

    #[test]
    fn repeated_budget_searches_are_deterministic_within_one_process() {
        let (mesh, locks) = locked_box(10.0);
        let first = run_budget_search(&mesh, &locks, d2_budget(1_000_000.0)).unwrap();
        let second = run_budget_search(&mesh, &locks, d2_budget(1_000_000.0)).unwrap();
        assert_eq!(first, second);
        let trace = match first {
            BudgetSearchOutcome::Verified(success)
            | BudgetSearchOutcome::AlreadyPrintScale(success) => success.trace,
            BudgetSearchOutcome::SkippedScaleSuspicious { .. } => panic!("box scale is valid"),
        };
        assert!(trace.adaptive_exploration.probes.len() <= search_policy().unwrap().bisections);
        assert_eq!(
            trace.adaptive_exploration.complete,
            trace.adaptive_exploration.remaining_intervals.is_empty()
        );
    }

    #[test]
    fn adaptive_ratios_round_trip_to_every_integer_target() {
        for input_faces in 2..=512 {
            for target_faces in 1..input_faces {
                let ratio = ratio_for_integer_target(input_faces, target_faces).unwrap();
                assert_eq!(target_face_count(input_faces, ratio), target_faces);
            }
        }
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
        let points = (0..25).map(|id| [id as f64, 0.0, 0.0]).collect();
        let original = Mesh::new(points, vec![[0, 2, 4]]).unwrap();
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
        let points = (0..25).map(|id| [id as f64, 0.0, 0.0]).collect();
        let original = Mesh::new(points, vec![[0, 2, 4], [0, 4, 6]]).unwrap();
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

    // ---- The registered downstream-utility witness -------------------------

    fn utility_policy() -> UtilityPolicy {
        search_policy().unwrap().utility
    }

    /// A candidate that satisfies every clause with room to spare: well above
    /// the face threshold, a real reduction, and deviations an order of
    /// magnitude inside all three registered bounds.
    fn admissible_candidate() -> SimplifyOutcome {
        SimplifyOutcome {
            mesh: octahedron(),
            retained_original_vertex_ids: Vec::new(),
            input_face_count: 65_024,
            output_face_count: 40_000,
            input_vertex_count: 32_514,
            output_vertex_count: 20_002,
            requested_ratio: 0.615,
            target_face_count: 40_000,
            achieved_ratio: 40_000.0 / 65_024.0,
            meshopt_advisory_error: 0.0,
            max_input_quantization_delta: 0.0,
            locked_vertex_count: 0,
            identity_bypass: false,
        }
    }

    fn admissible_probe() -> TwoSidedProbe {
        let mut probe = probe_with_percentiles(0.001, 0.002);
        probe.max_mm = Some(0.01);
        probe
    }

    fn clean_topology() -> EdgeTopologyCounts {
        EdgeTopologyCounts {
            boundary_edges: 0,
            nonmanifold_edges: 0,
        }
    }

    fn assess(
        candidate: &SimplifyOutcome,
        probe: &TwoSidedProbe,
        after: EdgeTopologyCounts,
    ) -> DownstreamUtilityEvidence {
        assess_downstream_utility(utility_policy(), candidate, probe, clean_topology(), after)
    }

    #[test]
    fn the_registered_witness_is_minted_only_when_every_clause_holds() {
        let evidence = assess(
            &admissible_candidate(),
            &admissible_probe(),
            clean_topology(),
        );
        let DownstreamUtilityEvidence::DownstreamUseful(witness) = evidence else {
            panic!("an admissible candidate must mint the registered witness");
        };
        assert_eq!(witness.witness_id(), SUPPORT_FACE_SCALED_RESOURCE_DEMAND);
        assert_eq!(witness.witness_id(), "support-face-scaled-resource-demand");
    }

    #[test]
    fn the_face_threshold_refuses_below_the_registered_floor_and_admits_at_it() {
        let policy = utility_policy();
        assert_eq!(policy.min_input_faces, 32_768);

        // One face below the floor refuses without examining any geometry:
        // this is what keeps every small-model trace byte-identical.
        let mut small = admissible_candidate();
        small.input_face_count = policy.min_input_faces - 1;
        small.output_face_count = policy.min_input_faces - 2;
        assert_eq!(
            assess(&small, &admissible_probe(), clean_topology()),
            DownstreamUtilityEvidence::NoDownstreamUtilityEvidence
        );

        // Exactly at the floor it is admitted; the clause is `>=`.
        let mut at_floor = admissible_candidate();
        at_floor.input_face_count = policy.min_input_faces;
        at_floor.output_face_count = policy.min_input_faces - 1;
        assert!(matches!(
            assess(&at_floor, &admissible_probe(), clean_topology()),
            DownstreamUtilityEvidence::DownstreamUseful(_)
        ));
    }

    #[test]
    fn the_threshold_covers_every_reachable_corpus_model_that_would_otherwise_decimate() {
        // Sphere (4,760 faces) and make_toed_slab (140) are the two corpus
        // models with GeometricallyEligible candidates; both sit below the
        // floor, which is why their frozen census digests cannot move.
        let policy = utility_policy();
        for (name, faces) in [("Sphere", 4_760_usize), ("make_toed_slab", 140)] {
            let mut candidate = admissible_candidate();
            candidate.input_face_count = faces;
            candidate.output_face_count = faces - 1;
            assert_eq!(
                assess(&candidate, &admissible_probe(), clean_topology()),
                DownstreamUtilityEvidence::NoDownstreamUtilityEvidence,
                "{name} must stay below the {} face floor",
                policy.min_input_faces
            );
        }
    }

    #[test]
    fn u1_refuses_a_candidate_that_removed_no_faces() {
        let mut candidate = admissible_candidate();
        candidate.output_face_count = candidate.input_face_count;
        assert_eq!(
            assess(&candidate, &admissible_probe(), clean_topology()),
            DownstreamUtilityEvidence::NoDownstreamUtilityEvidence
        );

        let mut grown = admissible_candidate();
        grown.output_face_count = grown.input_face_count + 1;
        assert_eq!(
            assess(&grown, &admissible_probe(), clean_topology()),
            DownstreamUtilityEvidence::NoDownstreamUtilityEvidence
        );
    }

    #[test]
    fn u2_refuses_on_either_percentile_independently_and_grants_no_slack() {
        let policy = utility_policy();
        let candidate = admissible_candidate();

        let mut at_limit = admissible_probe();
        at_limit.p99_mm = policy.max_p99_deviation_mm;
        at_limit.p99_9_mm = policy.max_p99_9_deviation_mm;
        assert!(
            matches!(
                assess(&candidate, &at_limit, clean_topology()),
                DownstreamUtilityEvidence::DownstreamUseful(_)
            ),
            "the bounds are inclusive"
        );

        let mut p99_over = at_limit.clone();
        p99_over.p99_mm = policy.max_p99_deviation_mm * 1.0000001;
        assert_eq!(
            assess(&candidate, &p99_over, clean_topology()),
            DownstreamUtilityEvidence::NoDownstreamUtilityEvidence
        );

        let mut p99_9_over = at_limit.clone();
        p99_9_over.p99_9_mm = policy.max_p99_9_deviation_mm * 1.0000001;
        assert_eq!(
            assess(&candidate, &p99_9_over, clean_topology()),
            DownstreamUtilityEvidence::NoDownstreamUtilityEvidence
        );

        // The search's final classification admits a candidate on the ×1.1
        // recheck slack. The witness does not: no slack multiplier applies.
        let slack = search_policy().unwrap().final_slack;
        let mut on_slack = at_limit;
        on_slack.p99_mm = policy.max_p99_deviation_mm * slack;
        assert_eq!(
            assess(&candidate, &on_slack, clean_topology()),
            DownstreamUtilityEvidence::NoDownstreamUtilityEvidence
        );
    }

    #[test]
    fn u3_refuses_a_worst_point_the_percentile_budget_cannot_see() {
        let policy = utility_policy();
        let candidate = admissible_candidate();

        // The Phase-1 staircase, reproduced as a unit case: the Weeping Treant
        // v1.2 at 520 faces removed measured p99 exactly 0, p99.9 0.000073 mm
        // and max 0.0566357 mm — a factor of 776. U2 sees nothing wrong.
        let mut staircase = admissible_probe();
        staircase.p99_mm = 0.0;
        staircase.p99_9_mm = 0.000_073;
        staircase.max_mm = Some(0.056_635_7);
        assert!(
            staircase.p99_mm <= policy.max_p99_deviation_mm
                && staircase.p99_9_mm <= policy.max_p99_9_deviation_mm,
            "the percentile clause must be the one that passes here"
        );
        assert!(
            matches!(
                assess(&candidate, &staircase, clean_topology()),
                DownstreamUtilityEvidence::DownstreamUseful(_)
            ),
            "0.0566 mm is inside the 0.08 mm absolute bound"
        );

        let mut at_limit = staircase.clone();
        at_limit.max_mm = Some(policy.max_absolute_deviation_mm);
        assert!(matches!(
            assess(&candidate, &at_limit, clean_topology()),
            DownstreamUtilityEvidence::DownstreamUseful(_)
        ));

        let mut over = staircase.clone();
        over.max_mm = Some(policy.max_absolute_deviation_mm * 1.0000001);
        assert_eq!(
            assess(&candidate, &over, clean_topology()),
            DownstreamUtilityEvidence::NoDownstreamUtilityEvidence
        );

        // A missing or non-finite worst point is a refusal, never a pass.
        let mut absent = staircase.clone();
        absent.max_mm = None;
        assert_eq!(
            assess(&candidate, &absent, clean_topology()),
            DownstreamUtilityEvidence::NoDownstreamUtilityEvidence
        );

        let mut not_a_number = staircase;
        not_a_number.max_mm = Some(f64::NAN);
        assert_eq!(
            assess(&candidate, &not_a_number, clean_topology()),
            DownstreamUtilityEvidence::NoDownstreamUtilityEvidence
        );
    }

    #[test]
    fn u4_refuses_any_increase_in_boundary_or_nonmanifold_edges() {
        // This test is the reason the U4 clause exists at all: `run_budget_search`
        // gates on the same two counts and `continue`s before it ever calls
        // `assess_downstream_utility`, so in production no candidate can reach
        // the witness with increased topology. The clause is defence in depth,
        // and this is what would fail if a future reordering removed it.
        let candidate = admissible_candidate();
        let probe = admissible_probe();

        for after in [
            EdgeTopologyCounts {
                boundary_edges: 0,
                nonmanifold_edges: 1,
            },
            EdgeTopologyCounts {
                boundary_edges: 1,
                nonmanifold_edges: 0,
            },
        ] {
            assert_eq!(
                assess(&candidate, &probe, after),
                DownstreamUtilityEvidence::NoDownstreamUtilityEvidence,
                "{after:?}"
            );
        }

        // The Weeping Treant v1.2's own numbers — 3 non-manifold edges at
        // 0.742 % reduction, monotone in faces removed thereafter. On the
        // production path the search gate refuses them first; here the witness
        // is shown to refuse them on its own.
        assert_eq!(
            assess(
                &candidate,
                &probe,
                EdgeTopologyCounts {
                    boundary_edges: 0,
                    nonmanifold_edges: 3,
                }
            ),
            DownstreamUtilityEvidence::NoDownstreamUtilityEvidence
        );
    }

    #[test]
    fn u5_is_one_clause_because_both_registered_demands_are_proportional_to_the_face_count() {
        // The witness names two registered resources: the minima-kNN batch's
        // `source_count` (= F) and the parity projection's 81·F per-triangle
        // term. This test is what licenses stating the clause once: it
        // recomputes both demands separately and requires them to agree on
        // every case, including the width at which the scaled form would wrap
        // a `usize`. If the kernel ever makes the parity term something other
        // than a positive constant times F, this fails and the single clause
        // must become two.
        const PARITY_GRID_BYTES_PER_FACE: u128 = 81;
        for (input_faces, output_faces, expected) in [
            (65_024_usize, 40_000_usize, true),
            (32_768, 32_767, true),
            (65_024, 65_024, false),
            (65_024, 65_025, false),
            (usize::MAX, usize::MAX - 1, true),
            (0, 0, false),
        ] {
            let knn_falls = (output_faces as u128) < (input_faces as u128);
            let parity_falls = (output_faces as u128) * PARITY_GRID_BYTES_PER_FACE
                < (input_faces as u128) * PARITY_GRID_BYTES_PER_FACE;
            assert_eq!(
                knn_falls, parity_falls,
                "{input_faces} -> {output_faces}: the two registered demands are proportional by \
                 construction and must never disagree"
            );
            assert_eq!(
                registered_face_scaled_demand_falls(input_faces, output_faces),
                expected,
                "{input_faces} -> {output_faces}"
            );
            assert_eq!(
                registered_face_scaled_demand_falls(input_faces, output_faces),
                knn_falls && parity_falls
            );
        }
    }

    #[test]
    fn decimation_witness_bounds_agree_with_the_search_policy_bounds() {
        // The witness asserts Derived bounds; the search enforces the same two
        // numbers as products of Placeholder fractions. They must agree
        // bit-for-bit at the ratified pixel pitch, so that a move in either is
        // loud rather than silent.
        let policy = search_policy().unwrap();
        let budget = DecimationBudget::ratified_default(d2_probe_config()).unwrap();
        let search = ErrorBudget {
            p99_mm: budget.pixel_um.get() * policy.p99_factor,
            p99_9_mm: budget.pixel_um.get() * policy.p99_9_factor,
        };
        assert_eq!(
            search.p99_mm.to_bits(),
            policy.utility.max_p99_deviation_mm.to_bits(),
            "DECIMATION_MAX_P99_DEVIATION_MM {} disagrees with the search limit {}",
            policy.utility.max_p99_deviation_mm,
            search.p99_mm
        );
        assert_eq!(
            search.p99_9_mm.to_bits(),
            policy.utility.max_p99_9_deviation_mm.to_bits(),
            "DECIMATION_MAX_P999_DEVIATION_MM {} disagrees with the search limit {}",
            policy.utility.max_p99_9_deviation_mm,
            search.p99_9_mm
        );
        assert_eq!(policy.utility.max_p99_deviation_mm, 0.0095);
        assert_eq!(policy.utility.max_p99_9_deviation_mm, 0.019);

        // The absolute bound is a separate authority with no search twin: it
        // is the registered print floor, and nothing in the search sees it.
        let constants = m0_constants().unwrap();
        assert_eq!(
            policy.utility.max_absolute_deviation_mm,
            *constants.evidence().print_floor_chord_mm().value()
        );
        assert_eq!(policy.utility.max_absolute_deviation_mm, 0.08);
    }
}
