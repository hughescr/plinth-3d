//! NativeSurface: the query engine. All methods are pure functions of immutable
//! kernel state + their own inputs, parallelized index-wise (spec sec 4). No
//! cross-query reductions anywhere.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use rayon::prelude::*;

use crate::bvh::Bvh;
use crate::closest::{closest_on_tri, segseg};
use crate::consts::{DEDUP_LOC_MM, EPS, NEAR_WALL_SKIP, THRESHOLD_BAND_MM};
use crate::mesh::Mesh;
use crate::moller::{dedup_loc_into, moller};
use crate::par;
use crate::parity::{ParityBuildError, ParityVote};
use crate::vec3::{add, cross, dot, norm, scale, sub, V3};
use crate::{SurfaceIndexKind, SurfaceSubsetError, SurfaceTriangleSoupDigest};

thread_local! {
    static STACK: RefCell<Vec<i32>> = RefCell::new(Vec::with_capacity(128));
    static ZSCRATCH: RefCell<Vec<f64>> = RefCell::new(Vec::with_capacity(64));
}

pub struct Interval {
    pub s_a: f64,
    pub s_b: f64,
    pub entered: bool,
    pub exited: bool,
    pub max_depth: f64,
    /// -1 = axis ray; k >= 0 = offset ray k
    pub ray: i32,
}

/// One centerline or capsule-offset ray used by penetration interval queries.
///
/// `label == -1` identifies the centerline; non-negative labels identify
/// offset rays in their canonical angular order.  The segment length is the
/// exact norm of this ray's own direction, which differs from the centerline
/// length for a tapered member.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapsuleRay {
    label: i32,
    origin: V3,
    direction: V3,
    length_mm: f64,
}

impl CapsuleRay {
    #[must_use]
    pub const fn label(self) -> i32 {
        self.label
    }

    #[must_use]
    pub const fn origin(self) -> V3 {
        self.origin
    }

    #[must_use]
    pub const fn direction(self) -> V3 {
        self.direction
    }

    #[must_use]
    pub const fn length_mm(self) -> f64 {
        self.length_mm
    }
}

/// Construct the canonical centerline and, when requested, capsule-offset
/// rays for one tapered member.
///
/// This is shared with consumers that validate interval arc-length bounds, so
/// the query and validation paths cannot drift in basis, offset, label, or
/// tapered-ray length.
#[must_use]
pub fn capsule_rays(
    p0: V3,
    p1: V3,
    r0: f64,
    r1: f64,
    n_offset_rays: usize,
    rays_all: bool,
) -> CapsuleRays {
    let axis = sub(p1, p0);
    let axis_length = norm(axis);
    let offset_count = usize::from(rays_all && axis_length >= 1e-9) * n_offset_rays;
    let (e1, e2) = if offset_count == 0 {
        ([0.0; 3], [0.0; 3])
    } else {
        perp_basis(scale(axis, 1.0 / axis_length))
    };
    CapsuleRays {
        p0,
        p1,
        r0,
        r1,
        axis,
        axis_length,
        e1,
        e2,
        offset_count,
        next: 0,
    }
}

/// Allocation-free iterator over the canonical rays of one tapered member.
#[derive(Clone, Copy, Debug)]
pub struct CapsuleRays {
    p0: V3,
    p1: V3,
    r0: f64,
    r1: f64,
    axis: V3,
    axis_length: f64,
    e1: V3,
    e2: V3,
    offset_count: usize,
    next: usize,
}

impl Iterator for CapsuleRays {
    type Item = CapsuleRay;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next == 0 {
            self.next = 1;
            return Some(CapsuleRay {
                label: -1,
                origin: self.p0,
                direction: self.axis,
                length_mm: self.axis_length,
            });
        }
        let offset = self.next - 1;
        if offset >= self.offset_count {
            return None;
        }
        self.next += 1;
        let th = 2.0 * std::f64::consts::PI * (offset as f64) / (self.offset_count as f64);
        let unit = add(scale(self.e1, th.cos()), scale(self.e2, th.sin()));
        let origin = add(self.p0, scale(unit, self.r0));
        let direction = sub(add(self.p1, scale(unit, self.r1)), origin);
        Some(CapsuleRay {
            label: offset as i32,
            origin,
            direction,
            length_mm: norm(direction),
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self
            .offset_count
            .saturating_add(1)
            .saturating_sub(self.next);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for CapsuleRays {}

/// A companion input slice did not match its batch's authoritative length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchInputError {
    pub operation: &'static str,
    pub field: &'static str,
    pub expected: usize,
    pub actual: usize,
}

impl BatchInputError {
    fn require_len(
        operation: &'static str,
        field: &'static str,
        expected: usize,
        actual: usize,
    ) -> Result<(), Self> {
        if actual == expected {
            Ok(())
        } else {
            Err(Self {
                operation,
                field,
                expected,
                actual,
            })
        }
    }
}

impl std::fmt::Display for BatchInputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}: {} has {} items; expected {}",
            self.operation, self.field, self.actual, self.expected
        )
    }
}

impl std::error::Error for BatchInputError {}

/// Caller-owned hard limits for [`NativeSurface::penetration_intervals_batch_bounded`].
///
/// The limits describe the whole batch, not one Rayon worker. Before any
/// interval extraction begins the kernel COUNTS the batch's demand with an
/// allocation-free census pass — one extra BVH descent per ray, `O(depth)`
/// memory — and the counted totals (sums over rows, never `rows x max`) are
/// what these limits refuse. Count-before-charge-before-allocate: every row's
/// scratch buffers are pre-reserved, fallibly, at exactly its counted budget,
/// and the execution pass refuses with
/// [`PenetrationBatchError::CensusDrift`] rather than grow one. The
/// conservative every-ray-hits-every-face shape survives only in
/// [`NativeSurface::penetration_intervals_batch_worst_case_limits`], which is
/// a sizing helper for callers that need a sufficient ceiling, not a charge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PenetrationBatchLimits {
    pub max_rows: usize,
    pub max_total_work: usize,
    pub max_total_hits: usize,
    pub max_total_samples: usize,
    pub max_total_intervals: usize,
    pub max_scratch_bytes: usize,
}

/// Caller-owned policy for projection onto a restricted set of global faces.
///
/// `None` means the exact nearest allowed face at any distance. A finite cap
/// returns a per-point [`AllowedFaceProjectionOutcome::NoWinner`] when every
/// allowed face is farther away. Equality is admitted: a projection exactly
/// at the cap is a winner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AllowedFaceProjectionPolicy {
    pub max_distance_mm: Option<f64>,
}

/// Whole-batch limits for [`NativeSurface::project_allowed_faces_batch`].
///
/// The node-visit limit includes one visit per existing BVH node to build the
/// allowed-subtree mask plus a deterministic conservative total over all query
/// rows. Since each leaf has at most `LEAF_MAX` faces, this setup-node charge
/// also bounds one-time face-filter checks. The triangle-test limit is the
/// conservative total over all rows. `max_memory_bytes` covers every retained
/// buffer used by the call: gathered allowed IDs, face/subtree masks, leaf
/// ranges, fixed traversal stacks, parallel row results, and serially
/// scattered outcomes. Allocator metadata and Rayon's executor-internal
/// storage are not included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllowedFaceProjectionLimits {
    pub max_points: usize,
    pub max_allowed_faces: usize,
    pub max_total_node_visits: usize,
    pub max_total_triangle_tests: usize,
    pub max_memory_bytes: usize,
}

/// Exact winner for one allowed-face projection row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AllowedFaceProjection {
    pub distance_mm: f64,
    pub point: V3,
    /// Stable global face ID from the source surface.
    pub face_id: u32,
}

/// Why a valid point has no winner under an explicit projection policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AllowedFaceProjectionNoWinner {
    BeyondMaxDistance { max_distance_mm: f64 },
}

/// One output row. Input and resource failures are reported by the outer
/// `Result`; `NoWinner` is therefore never overloaded with an error state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AllowedFaceProjectionOutcome {
    Projected(AllowedFaceProjection),
    NoWinner(AllowedFaceProjectionNoWinner),
}

/// Named preflight or allocation refusal from an allowed-face batch.
#[derive(Clone, Debug, PartialEq)]
pub enum AllowedFaceProjectionError {
    EmptyAllowedFaces,
    PointLimitExceeded {
        required: usize,
        limit: usize,
    },
    AllowedFaceLimitExceeded {
        required: usize,
        limit: usize,
    },
    DuplicateFaceId {
        face_id: u32,
    },
    FaceIdOutOfBounds {
        position: usize,
        face_id: u32,
        global_face_count: usize,
    },
    FaceIdNotSelected {
        position: usize,
        face_id: u32,
    },
    NonFinitePoint {
        point: usize,
        axis: usize,
    },
    InvalidMaxDistance {
        max_distance_mm: f64,
    },
    ResourceArithmeticOverflow {
        resource: &'static str,
    },
    ResourceLimitExceeded {
        resource: &'static str,
        required: usize,
        limit: usize,
    },
    AllocationFailure {
        buffer: &'static str,
        requested_bytes: usize,
    },
}

impl std::fmt::Display for AllowedFaceProjectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyAllowedFaces => {
                write!(formatter, "allowed-face projection requires at least one face")
            }
            Self::PointLimitExceeded { required, limit } => write!(
                formatter,
                "allowed-face projection needs {required} points; limit is {limit}"
            ),
            Self::AllowedFaceLimitExceeded { required, limit } => write!(
                formatter,
                "allowed-face projection needs {required} allowed faces; limit is {limit}"
            ),
            Self::DuplicateFaceId { face_id } => write!(
                formatter,
                "allowed-face projection repeats global face ID {face_id}"
            ),
            Self::FaceIdOutOfBounds {
                position,
                face_id,
                global_face_count,
            } => write!(
                formatter,
                "allowed_face_ids[{position}] is {face_id}, outside {global_face_count} global faces"
            ),
            Self::FaceIdNotSelected {
                position,
                face_id,
            } => write!(
                formatter,
                "allowed_face_ids[{position}] global face ID {face_id} is not selected by this surface"
            ),
            Self::NonFinitePoint { point, axis } => write!(
                formatter,
                "allowed-face projection point {point} axis {axis} is non-finite"
            ),
            Self::InvalidMaxDistance { max_distance_mm } => write!(
                formatter,
                "allowed-face projection max_distance_mm must be finite and non-negative, got {max_distance_mm}"
            ),
            Self::ResourceArithmeticOverflow { resource } => write!(
                formatter,
                "allowed-face projection resource arithmetic overflow in {resource}"
            ),
            Self::ResourceLimitExceeded {
                resource,
                required,
                limit,
            } => write!(
                formatter,
                "allowed-face projection {resource} requires {required}; limit is {limit}"
            ),
            Self::AllocationFailure {
                buffer,
                requested_bytes,
            } => write!(
                formatter,
                "allowed-face projection could not reserve {requested_bytes} bytes for {buffer}"
            ),
        }
    }
}

impl std::error::Error for AllowedFaceProjectionError {}

#[derive(Clone, Copy)]
struct AllowedFaceProjectionRequirements {
    stack_capacity: usize,
    memory_bytes: usize,
}

#[derive(Clone, Copy, Default)]
struct AllowedLeafRange {
    start: usize,
    count: usize,
}

fn checked_allowed_projection_mul(
    left: usize,
    right: usize,
    resource: &'static str,
) -> Result<usize, AllowedFaceProjectionError> {
    left.checked_mul(right)
        .ok_or(AllowedFaceProjectionError::ResourceArithmeticOverflow { resource })
}

fn checked_allowed_projection_add(
    left: usize,
    right: usize,
    resource: &'static str,
) -> Result<usize, AllowedFaceProjectionError> {
    left.checked_add(right)
        .ok_or(AllowedFaceProjectionError::ResourceArithmeticOverflow { resource })
}

fn allowed_projection_bytes<T>(
    count: usize,
    resource: &'static str,
) -> Result<usize, AllowedFaceProjectionError> {
    checked_allowed_projection_mul(count, std::mem::size_of::<T>(), resource)
}

fn reserve_allowed_projection_vec<T>(
    capacity: usize,
    buffer: &'static str,
) -> Result<Vec<T>, AllowedFaceProjectionError> {
    let requested_bytes = allowed_projection_bytes::<T>(capacity, buffer)?;
    let mut values = Vec::new();
    values.try_reserve_exact(capacity).map_err(|_| {
        AllowedFaceProjectionError::AllocationFailure {
            buffer,
            requested_bytes,
        }
    })?;
    Ok(values)
}

/// Refusal from the bounded penetration batch. Resource values are always the
/// preflight-required amount and the caller-supplied hard limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PenetrationBatchError {
    Input(BatchInputError),
    ResourceLimitExceeded {
        resource: &'static str,
        required: usize,
        limit: usize,
    },
    ArithmeticOverflow {
        resource: &'static str,
    },
    AllocationFailure {
        row: Option<usize>,
        buffer: &'static str,
        requested_bytes: usize,
    },
    /// The execution pass exceeded the budget the census pass counted for it.
    ///
    /// The census walk and the fill walk are the same traversal over the same
    /// immutable BVH with the same predicate, so this is impossible by
    /// construction; the check is a proof obligation that preserves
    /// count-before-charge-before-allocate. A future edit that lets the two
    /// walks drift fails loudly here instead of silently regrowing a
    /// `try_reserve_exact` buffer past its afforded capacity.
    CensusDrift {
        row: usize,
        resource: &'static str,
        counted: usize,
        executed: usize,
    },
    /// The batch's EXECUTED totals exceeded the totals that were CHARGED for
    /// it (FU-8g H2). [`Self::CensusDrift`] proves the same relation one row
    /// and one buffer at a time; this proves it for the aggregate the caller
    /// actually spent against its cap, which is the number a resource policy
    /// is written in. Like `CensusDrift` it is impossible by construction and
    /// exists so that a future edit fails loudly instead of overspending a
    /// charge nobody re-checks.
    ChargeDrift {
        resource: &'static str,
        charged: usize,
        executed: usize,
    },
}

impl From<BatchInputError> for PenetrationBatchError {
    fn from(error: BatchInputError) -> Self {
        Self::Input(error)
    }
}

impl std::fmt::Display for PenetrationBatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input(error) => error.fmt(formatter),
            Self::ResourceLimitExceeded {
                resource,
                required,
                limit,
            } => write!(
                formatter,
                "penetration batch {resource} requires {required}; limit is {limit}"
            ),
            Self::ArithmeticOverflow { resource } => {
                write!(formatter, "penetration batch resource arithmetic overflow in {resource}")
            }
            Self::AllocationFailure {
                row,
                buffer,
                requested_bytes,
            } => match row {
                Some(row) => write!(
                    formatter,
                    "penetration batch row {row} could not reserve {requested_bytes} bytes for {buffer}"
                ),
                None => write!(
                    formatter,
                    "penetration batch could not reserve {requested_bytes} bytes for {buffer}"
                ),
            },
            Self::CensusDrift {
                row,
                resource,
                counted,
                executed,
            } => write!(
                formatter,
                "penetration batch row {row} executed {executed} {resource}; census counted {counted}"
            ),
            Self::ChargeDrift {
                resource,
                charged,
                executed,
            } => write!(
                formatter,
                "penetration batch executed {executed} {resource}; the charge was {charged}"
            ),
        }
    }
}

impl std::error::Error for PenetrationBatchError {}

#[derive(Clone, Copy)]
struct PenetrationRowBudget {
    rays: usize,
    hits_per_ray: usize,
    samples_per_ray: usize,
    subsegments_per_ray: usize,
    intervals: usize,
    traversal_stack: usize,
    parity_crossings: usize,
}

/// Worst-case (`rows x max`) batch totals. Since the census pass landed these
/// govern no execution: they exist solely for the sizing helper
/// [`NativeSurface::penetration_intervals_batch_worst_case_limits`].
#[derive(Clone, Copy)]
struct PenetrationBatchRequirements {
    rows: usize,
    work: usize,
    hits: usize,
    samples: usize,
    intervals: usize,
    scratch_bytes: usize,
}

/// One row's counted penetration demand: crossings and traversal tests summed
/// over the row's rays (1, or `n_offset_rays + 1` when `rays_all`), plus the
/// summed ray arc length. Produced by
/// [`NativeSurface::penetration_census_batch`]; no per-hit storage exists
/// anywhere in the counting pass.
///
/// The two `max_ray_*` fields exist because the fill pass re-creates its
/// scratch buffers PER RAY: a sound per-ray reservation needs the worst
/// single ray, while the batch totals need the row sums, and collapsing the
/// two into one number would either under-reserve a ray or over-charge the
/// row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenetrationRowCensus {
    /// Möller crossings the fill walk will push for this row — exactly
    /// `hits.len()` summed over the row's rays, before deduplication.
    pub crossings: usize,
    /// Executed traversal effort: BVH nodes popped plus Möller evaluations,
    /// summed over the row's rays. This is the honest `work` quantity; it is
    /// deliberately NOT the same number as `crossings` (storage).
    pub tests: usize,
    /// Summed length of the row's rays in millimetres, for the closed-form
    /// sample bound.
    pub seg_mm: f64,
    /// The largest single-ray crossing count in the row — the per-ray scratch
    /// reservation for the hit-shaped buffers.
    pub max_ray_crossings: usize,
    /// The longest single ray in millimetres — the per-ray form of the
    /// closed-form sample bound.
    pub max_ray_seg_mm: f64,
}

/// What the fill pass ACTUALLY executed, summed over the batch's rows: the
/// EXECUTED half of the charged-vs-executed pair (FU-8g H2, the G3/G6c
/// idiom — both numbers stated, the relation asserted rather than assumed).
///
/// `work` and `hits` are metered in exactly the units
/// [`PenetrationRowCensus::tests`] and [`PenetrationRowCensus::crossings`]
/// count, so on an undrifted pair they are EQUAL to the charge, not merely
/// under it; `samples` and `intervals` are bounded quantities and land at or
/// under their counted budgets. Nothing here is a policy: it is the receipt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PenetrationBatchMeter {
    /// Rows the fill pass executed — every row of the batch, degenerate rows
    /// included (a degenerate row executes and meters zero of everything
    /// else).
    pub rows: usize,
    /// Executed traversal effort: BVH nodes popped plus Möller evaluations,
    /// summed over every ray of every row.
    pub work: usize,
    /// Möller crossings the fill walk actually pushed, before deduplication.
    pub hits: usize,
    /// Sample points the fill pass actually placed.
    pub samples: usize,
    /// Intervals the fill pass actually emitted.
    pub intervals: usize,
}

/// One row's executed totals. Aggregated into [`PenetrationBatchMeter`] by
/// the metered batch entry point.
#[derive(Clone, Copy, Debug, Default)]
struct PenetrationRowMeter {
    work: usize,
    hits: usize,
    samples: usize,
    intervals: usize,
}

/// Counted (census-derived) batch requirements: totals are SUMS over per-row
/// budgets, never `rows x max`.
struct PenetrationCensusRequirements {
    rows: usize,
    work: usize,
    hits: usize,
    samples: usize,
    intervals: usize,
    scratch_bytes: usize,
    row_budgets: Vec<PenetrationRowBudget>,
}

fn checked_penetration_mul(
    a: usize,
    b: usize,
    resource: &'static str,
) -> Result<usize, PenetrationBatchError> {
    a.checked_mul(b)
        .ok_or(PenetrationBatchError::ArithmeticOverflow { resource })
}

fn checked_penetration_add(
    a: usize,
    b: usize,
    resource: &'static str,
) -> Result<usize, PenetrationBatchError> {
    a.checked_add(b)
        .ok_or(PenetrationBatchError::ArithmeticOverflow { resource })
}

fn checked_penetration_bytes<T>(
    count: usize,
    resource: &'static str,
) -> Result<usize, PenetrationBatchError> {
    checked_penetration_mul(count, std::mem::size_of::<T>(), resource)
}

/// The fourteen per-row scratch buffers of `pen_intervals_one` which coexist
/// for a ray, summed into one row's byte budget. Interval output is bounded
/// separately by `max_total_intervals`; it is not scratch.
///
/// Shared by the worst-case and census requirement derivations so the two
/// charges cannot drift in shape — only their inputs differ.
fn penetration_row_scratch_bytes(
    rays: usize,
    hits_per_ray: usize,
    subsegments_per_ray: usize,
    samples_per_ray: usize,
    traversal_stack: usize,
    parity_crossings: usize,
) -> Result<usize, PenetrationBatchError> {
    let raylist_bytes = checked_penetration_bytes::<(i32, V3, V3, f64)>(rays, "ray list bytes")?;
    let hits_bytes = checked_penetration_bytes::<(f64, u32)>(hits_per_ray, "hit bytes")?;
    let tvals_bytes = checked_penetration_bytes::<f64>(hits_per_ray, "crossing parameter bytes")?;
    let dedup_bytes = checked_penetration_bytes::<f64>(hits_per_ray, "dedup bytes")?;
    let bounds_bytes = checked_penetration_bytes::<f64>(
        checked_penetration_add(hits_per_ray, 2, "boundary capacity")?,
        "boundary bytes",
    )?;
    let samples_bytes = checked_penetration_bytes::<V3>(samples_per_ray, "sample point bytes")?;
    let slices_bytes =
        checked_penetration_bytes::<(usize, usize)>(subsegments_per_ray, "slice bytes")?;
    let sub_bounds_bytes = checked_penetration_bytes::<(f64, f64, bool, bool)>(
        subsegments_per_ray,
        "subsegment bound bytes",
    )?;
    let inside_bytes = checked_penetration_bytes::<bool>(samples_per_ray, "inside bytes")?;
    let depths_bytes = checked_penetration_bytes::<f64>(samples_per_ray, "sample depth bytes")?;
    let votes_bytes = checked_penetration_bytes::<bool>(subsegments_per_ray, "vote bytes")?;
    let subdepth_bytes =
        checked_penetration_bytes::<f64>(subsegments_per_ray, "subsegment depth bytes")?;
    let traversal_bytes =
        checked_penetration_bytes::<i32>(traversal_stack, "BVH traversal stack bytes")?;
    let parity_bytes = checked_penetration_bytes::<f64>(parity_crossings, "parity crossing bytes")?;
    [
        raylist_bytes,
        hits_bytes,
        tvals_bytes,
        dedup_bytes,
        bounds_bytes,
        samples_bytes,
        slices_bytes,
        sub_bounds_bytes,
        inside_bytes,
        depths_bytes,
        votes_bytes,
        subdepth_bytes,
        traversal_bytes,
        parity_bytes,
    ]
    .into_iter()
    .try_fold(0usize, |total, bytes| {
        checked_penetration_add(total, bytes, "row scratch bytes")
    })
}

/// The closed-form honest sample bound (FU-8g design F10), per row.
///
/// `pen_intervals_one` samples each subsegment at
/// `ns = cap.min(3.max(trunc(lsub / majority_step_mm)))`, and per ray the
/// subsegment lengths sum to that ray's arc length, so a row's total sample
/// count is bounded by
/// `min(cap x subsegments, 3 x subsegments + seg / majority_step_mm)`.
/// The real-valued `seg / step` term is integerized with `ceil` — the safe
/// direction — so per-subsegment truncation can never exceed it. When the
/// ratio is non-finite or beyond `usize`, the `cap` arm alone governs, which
/// mirrors exactly what the sampling loop's saturating cast executes.
fn census_row_sample_budget(
    subsegments: usize,
    seg_mm: f64,
    majority_step_mm: f64,
    cap: usize,
) -> Result<usize, PenetrationBatchError> {
    let cap_arm = checked_penetration_mul(subsegments, cap, "samples per row")?;
    let step_samples = (seg_mm / majority_step_mm).ceil();
    if !step_samples.is_finite() || step_samples < 0.0 || step_samples >= usize::MAX as f64 {
        return Ok(cap_arm);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let step_samples = step_samples as usize;
    let honest_arm = checked_penetration_mul(subsegments, 3, "samples per row")?;
    let honest_arm = checked_penetration_add(honest_arm, step_samples, "samples per row")?;
    Ok(cap_arm.min(honest_arm))
}

fn reserve_penetration_vec<T>(
    capacity: usize,
    row: Option<usize>,
    buffer: &'static str,
) -> Result<Vec<T>, PenetrationBatchError> {
    let requested_bytes = checked_penetration_bytes::<T>(capacity, buffer)?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(capacity)
        .map_err(|_| PenetrationBatchError::AllocationFailure {
            row,
            buffer,
            requested_bytes,
        })?;
    Ok(result)
}

pub type CastRaysBatch = (Vec<i64>, Vec<f64>, Vec<i64>);
pub type SphereBreachBatch = (Vec<bool>, Vec<f64>, Vec<f64>);

pub struct NativeSurface {
    pub(crate) mesh: Mesh,
    pub bvh: Bvh,
    pub pv: ParityVote,
    pub grid_pitch_mm: f64,
    pub(crate) max_grid_bytes: usize,
    pub(crate) source_triangle_soup_digest: SurfaceTriangleSoupDigest,
    exterior_mask: OnceLock<Vec<bool>>,
    // result-inert telemetry (spec sec 4.2): decisions within 1e-6 of a
    // threshold, and measured-tie breach disagreements.
    pub band_counter: AtomicU64,
    pub tie_counter: AtomicU64,
}

#[inline]
fn box_min(a: V3, b: V3) -> V3 {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])]
}
#[inline]
fn box_max(a: V3, b: V3) -> V3 {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]
}

/// Distance from point to node AABB = sqrt(sum of clamped axis gaps squared)
/// (spec sec 3.3). sqrt is correctly rounded and monotone => pruning sound.
#[inline]
fn point_box_dist(p: V3, bmin: V3, bmax: V3) -> f64 {
    let mut s = 0.0f64;
    for k in 0..3 {
        let d = if p[k] < bmin[k] {
            bmin[k] - p[k]
        } else if p[k] > bmax[k] {
            p[k] - bmax[k]
        } else {
            0.0
        };
        s += d * d;
    }
    s.sqrt()
}

/// Distance lower bound between a segment's AABB and a node AABB
/// (Amendment A17): sqrt(sum max(0, axis-gap)^2). Sound: each axis gap <= any
/// point-pair coordinate difference.
#[inline]
fn box_box_dist(amin: V3, amax: V3, bmin: V3, bmax: V3) -> f64 {
    let mut s = 0.0f64;
    for k in 0..3 {
        let g = (bmin[k] - amax[k]).max(amin[k] - bmax[k]).max(0.0);
        s += g * g;
    }
    s.sqrt()
}

/// Ray (bounded param interval [t0,t1]) vs padded node AABB (slab test).
#[inline]
fn ray_box(o: V3, d: V3, bmin: V3, bmax: V3, pad: f64, t0: f64, t1: f64) -> bool {
    let mut tmin = t0;
    let mut tmax = t1;
    for k in 0..3 {
        let lo = bmin[k] - pad;
        let hi = bmax[k] + pad;
        if d[k] == 0.0 {
            if o[k] < lo || o[k] > hi {
                return false;
            }
        } else {
            let inv = 1.0 / d[k];
            let mut ta = (lo - o[k]) * inv;
            let mut tb = (hi - o[k]) * inv;
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            if ta > tmin {
                tmin = ta;
            }
            if tb < tmax {
                tmax = tb;
            }
            if tmin > tmax {
                return false;
            }
        }
    }
    true
}

fn build_indexes(
    mesh: &Mesh,
    grid_pitch_mm: f64,
    max_grid_bytes: usize,
) -> Result<(Bvh, ParityVote), (SurfaceIndexKind, String)> {
    let bvh = Bvh::build(mesh).map_err(|reason| (SurfaceIndexKind::Bvh, reason))?;
    let pv = ParityVote::build(mesh, grid_pitch_mm, max_grid_bytes)
        .map_err(|reason| (SurfaceIndexKind::Parity, reason.to_string()))?;
    Ok((bvh, pv))
}

impl NativeSurface {
    pub(crate) fn new(
        mesh: Mesh,
        grid_pitch_mm: f64,
        max_grid_bytes: usize,
    ) -> Result<NativeSurface, ParityBuildError> {
        let source_triangle_soup_digest = mesh.triangle_soup_digest();
        let bvh = Bvh::build(&mesh).map_err(ParityBuildError::Other)?;
        let pv = ParityVote::build(&mesh, grid_pitch_mm, max_grid_bytes)?;
        Ok(NativeSurface {
            mesh,
            bvh,
            pv,
            grid_pitch_mm,
            max_grid_bytes,
            source_triangle_soup_digest,
            exterior_mask: OnceLock::new(),
            band_counter: AtomicU64::new(0),
            tie_counter: AtomicU64::new(0),
        })
    }

    /// Narrow this surface to global face IDs selected by the current view.
    /// Validation follows caller order; accepted IDs are canonicalized to
    /// ascending global order before building independent indexes and caches.
    pub fn subset(&self, face_ids: &[u32]) -> Result<NativeSurface, SurfaceSubsetError> {
        if face_ids.is_empty() {
            return Err(SurfaceSubsetError::EmptySelection);
        }

        let global_face_count = self.mesh.global_nfaces();
        let mut seen = BTreeSet::new();
        for (position, &face_id) in face_ids.iter().enumerate() {
            if face_id as usize >= global_face_count {
                return Err(SurfaceSubsetError::FaceIdOutOfBounds {
                    position,
                    face_id,
                    global_face_count,
                });
            }
            if !self.mesh.contains_face(face_id) {
                return Err(SurfaceSubsetError::FaceIdNotSelected { position, face_id });
            }
            if !seen.insert(face_id) {
                return Err(SurfaceSubsetError::DuplicateFaceId { face_id });
            }
        }

        let mut canonical = face_ids.to_vec();
        canonical.sort_unstable();
        let mesh = self.mesh.subset(canonical);
        let source_triangle_soup_digest = mesh.triangle_soup_digest();
        let (bvh, pv) = build_indexes(&mesh, self.grid_pitch_mm, self.max_grid_bytes)
            .map_err(|(index, _reason)| SurfaceSubsetError::IndexBuild { index })?;
        Ok(NativeSurface {
            mesh,
            bvh,
            pv,
            grid_pitch_mm: self.grid_pitch_mm,
            max_grid_bytes: self.max_grid_bytes,
            source_triangle_soup_digest,
            exterior_mask: OnceLock::new(),
            band_counter: AtomicU64::new(0),
            tie_counter: AtomicU64::new(0),
        })
    }

    /// Result-inert threshold-band telemetry (spec R3 / sec 4.2): count a
    /// CONTINUOUS decision quantity whose distance to its threshold is under
    /// `THRESHOLD_BAND_MM`. Relaxed atomic — the COUNT is a deterministic
    /// function of the inputs (each decision inspects only its own quantity),
    /// so it is thread-count invariant like `tie_counter`, and it NEVER feeds a
    /// result. Non-finite quantities (e.g. an infinite far-wall thickness or an
    /// empty-crop clearance) compare false and are not counted.
    #[inline]
    fn band_check(&self, quantity: f64, threshold: f64) {
        if (quantity - threshold).abs() < THRESHOLD_BAND_MM {
            self.band_counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    // ---- nearest triangle (query/project) -------------------------------
    /// Exact nearest via branch-and-bound with lexicographic (dist, idx),
    /// strict `>` pruning, seed (upper_bound, global_nfaces) (Amendment A12).
    pub(crate) fn nearest_one(&self, p: V3, upper_bound: f64) -> (f64, usize, V3) {
        STACK.with(|s| self.nearest_one_with_stack(p, upper_bound, &mut s.borrow_mut()))
    }

    fn nearest_one_with_stack(
        &self,
        p: V3,
        upper_bound: f64,
        stack: &mut Vec<i32>,
    ) -> (f64, usize, V3) {
        // Keep the historical full-surface face count as the no-hit sentinel.
        // A selected global ID can never collide with this value.
        let nf = self.mesh.global_nfaces();
        let mut best_d = upper_bound;
        let mut best_i = nf;
        let mut best_cp: V3 = [0.0; 3];
        stack.clear();
        stack.push(0);
        while let Some(ni) = stack.pop() {
            let node = &self.bvh.nodes[ni as usize];
            // prune STRICTLY (> best_d): keep equal-distance lower-index tris
            if point_box_dist(p, node.bmin, node.bmax) > best_d {
                continue;
            }
            if node.leaf {
                let start = node.a as usize;
                let cnt = node.b as usize;
                for &tid in &self.bvh.order[start..start + cnt] {
                    let t = tid as usize;
                    let a = self.mesh.v0(t);
                    let b = self.mesh.vertex_b(t);
                    let cc = self.mesh.vertex_c(t);
                    let cp = closest_on_tri(p, a, b, cc);
                    let d = norm(sub(p, cp));
                    if d < best_d || (d == best_d && t < best_i) {
                        best_d = d;
                        best_i = t;
                        best_cp = cp;
                    }
                }
            } else {
                // visit nearer child first => push farther first (LIFO)
                let l = node.a as usize;
                let r = node.b as usize;
                let dl = point_box_dist(p, self.bvh.nodes[l].bmin, self.bvh.nodes[l].bmax);
                let dr = point_box_dist(p, self.bvh.nodes[r].bmin, self.bvh.nodes[r].bmax);
                if dl <= dr {
                    stack.push(r as i32);
                    stack.push(l as i32);
                } else {
                    stack.push(l as i32);
                    stack.push(r as i32);
                }
            }
        }
        (best_d, best_i, best_cp)
    }

    /// query facade: (dist, idx) with over-bound => (inf, global_nfaces).
    pub fn query(&self, pts: &[V3], upper_bound: f64) -> (Vec<f64>, Vec<i64>) {
        let nf = self.mesh.global_nfaces();
        let res: Vec<(f64, i64)> = par::run(|| {
            pts.par_iter()
                .map(|&p| {
                    let (d, i, _) = self.nearest_one(p, upper_bound);
                    if i == nf {
                        (f64::INFINITY, nf as i64)
                    } else {
                        (d, i as i64)
                    }
                })
                .collect()
        });
        let mut dist = Vec::with_capacity(res.len());
        let mut idx = Vec::with_capacity(res.len());
        for (d, i) in res {
            dist.push(d);
            idx.push(i);
        }
        (dist, idx)
    }

    /// project: (dist, closest_point, tri_idx, face_normal).
    pub fn project(&self, pts: &[V3]) -> (Vec<f64>, Vec<V3>, Vec<i64>, Vec<V3>) {
        let res: Vec<(f64, V3, i64, V3)> = par::run(|| {
            pts.par_iter()
                .map(|&p| {
                    let (d, i, cp) = self.nearest_one(p, f64::INFINITY);
                    let fn_ = if i < self.mesh.global_nfaces() {
                        self.mesh.normal(i)
                    } else {
                        [0.0; 3]
                    };
                    (d, cp, i as i64, fn_)
                })
                .collect()
        });
        let mut dist = Vec::with_capacity(res.len());
        let mut cp = Vec::with_capacity(res.len());
        let mut idx = Vec::with_capacity(res.len());
        let mut nrm = Vec::with_capacity(res.len());
        for (d, c, i, n) in res {
            dist.push(d);
            cp.push(c);
            idx.push(i);
            nrm.push(n);
        }
        (dist, cp, idx, nrm)
    }

    fn allowed_projection_requirements(
        &self,
        points: usize,
        allowed_faces: usize,
    ) -> Result<AllowedFaceProjectionRequirements, AllowedFaceProjectionError> {
        let stack_capacity = self.bvh.depth.checked_add(1).ok_or(
            AllowedFaceProjectionError::ResourceArithmeticOverflow {
                resource: "traversal stack depth",
            },
        )?;
        let stack_slots =
            checked_allowed_projection_mul(points, stack_capacity, "traversal stack slots")?;

        let mut memory_bytes =
            allowed_projection_bytes::<u32>(allowed_faces, "gathered allowed IDs")?;
        memory_bytes = checked_allowed_projection_add(
            memory_bytes,
            allowed_projection_bytes::<u8>(self.mesh.global_nfaces(), "allowed-face mask")?,
            "batch memory",
        )?;
        memory_bytes = checked_allowed_projection_add(
            memory_bytes,
            allowed_projection_bytes::<u8>(self.bvh.nodes.len(), "allowed-subtree mask")?,
            "batch memory",
        )?;
        memory_bytes = checked_allowed_projection_add(
            memory_bytes,
            allowed_projection_bytes::<AllowedLeafRange>(
                self.bvh.nodes.len(),
                "allowed leaf ranges",
            )?,
            "batch memory",
        )?;
        memory_bytes = checked_allowed_projection_add(
            memory_bytes,
            allowed_projection_bytes::<i32>(stack_slots, "traversal stacks")?,
            "batch memory",
        )?;
        let result_bytes = allowed_projection_bytes::<AllowedFaceProjectionOutcome>(
            points,
            "projection outcomes",
        )?;
        memory_bytes = checked_allowed_projection_add(memory_bytes, result_bytes, "batch memory")?;
        memory_bytes = checked_allowed_projection_add(memory_bytes, result_bytes, "batch memory")?;

        Ok(AllowedFaceProjectionRequirements {
            stack_capacity,
            memory_bytes,
        })
    }

    fn nearest_allowed_one_with_stack(
        &self,
        point: V3,
        upper_bound: f64,
        allowed_leaf_faces: &[u32],
        allowed_leaf_ranges: &[AllowedLeafRange],
        allowed_subtrees: &[u8],
        stack: &mut [i32],
    ) -> (f64, usize, V3) {
        let no_face = self.mesh.global_nfaces();
        let mut best_distance = upper_bound;
        let mut best_face = no_face;
        let mut best_point = [0.0; 3];
        let mut stack_len = 1usize;
        stack[0] = 0;

        while stack_len > 0 {
            stack_len -= 1;
            let node_index = stack[stack_len] as usize;
            debug_assert_ne!(allowed_subtrees[node_index], 0);
            let node = &self.bvh.nodes[node_index];
            // Strict pruning preserves equal-distance candidates for the
            // stable global-face-ID tie break.
            if point_box_dist(point, node.bmin, node.bmax) > best_distance {
                continue;
            }
            if node.leaf {
                let range = allowed_leaf_ranges[node_index];
                for &face_id in &allowed_leaf_faces[range.start..range.start + range.count] {
                    let face = face_id as usize;
                    let closest = closest_on_tri(
                        point,
                        self.mesh.v0(face),
                        self.mesh.vertex_b(face),
                        self.mesh.vertex_c(face),
                    );
                    let distance = norm(sub(point, closest));
                    if distance < best_distance || (distance == best_distance && face < best_face) {
                        best_distance = distance;
                        best_face = face;
                        best_point = closest;
                    }
                }
                continue;
            }

            let left = node.a as usize;
            let right = node.b as usize;
            let left_allowed = allowed_subtrees[left] != 0;
            let right_allowed = allowed_subtrees[right] != 0;
            match (left_allowed, right_allowed) {
                (true, true) => {
                    let left_distance =
                        point_box_dist(point, self.bvh.nodes[left].bmin, self.bvh.nodes[left].bmax);
                    let right_distance = point_box_dist(
                        point,
                        self.bvh.nodes[right].bmin,
                        self.bvh.nodes[right].bmax,
                    );
                    debug_assert!(stack_len + 2 <= stack.len());
                    if left_distance <= right_distance {
                        stack[stack_len] = right as i32;
                        stack[stack_len + 1] = left as i32;
                    } else {
                        stack[stack_len] = left as i32;
                        stack[stack_len + 1] = right as i32;
                    }
                    stack_len += 2;
                }
                (true, false) => {
                    debug_assert!(stack_len < stack.len());
                    stack[stack_len] = left as i32;
                    stack_len += 1;
                }
                (false, true) => {
                    debug_assert!(stack_len < stack.len());
                    stack[stack_len] = right as i32;
                    stack_len += 1;
                }
                (false, false) => unreachable!("allowed subtree mask must be closed upward"),
            }
        }
        (best_distance, best_face, best_point)
    }

    /// Project a batch onto an exact allowed subset while reusing this
    /// surface's immutable BVH and face store.
    ///
    /// The allowed set is gathered once into a global-face mask and one compact
    /// vector grouped by existing BVH leaf. A bottom-up mask marks only BVH
    /// nodes containing an allowed face, so each row skips wholly disallowed
    /// subtrees and never scans disallowed leaf faces. It then applies the
    /// kernel's exact `(distance, global face ID)` rule. For `P` points, `A`
    /// allowed faces, `K` marked BVH nodes, and `N` existing BVH nodes,
    /// worst-case query work is `N + P*K` node visits and `P*A` triangle tests;
    /// both totals are preflight-capped.
    /// The one-time gather is `O(N + A)` for `N` existing BVH faces/nodes;
    /// `N` is bounded by the admitted surface and by this call's face/node mask
    /// memory charge.
    /// No geometry, BVH, parity grid, or per-point subset is constructed.
    pub fn project_allowed_faces_batch(
        &self,
        allowed_face_ids: &[u32],
        points: &[V3],
        policy: AllowedFaceProjectionPolicy,
        limits: AllowedFaceProjectionLimits,
    ) -> Result<Vec<AllowedFaceProjectionOutcome>, AllowedFaceProjectionError> {
        if allowed_face_ids.is_empty() {
            return Err(AllowedFaceProjectionError::EmptyAllowedFaces);
        }
        if points.len() > limits.max_points {
            return Err(AllowedFaceProjectionError::PointLimitExceeded {
                required: points.len(),
                limit: limits.max_points,
            });
        }
        if allowed_face_ids.len() > limits.max_allowed_faces {
            return Err(AllowedFaceProjectionError::AllowedFaceLimitExceeded {
                required: allowed_face_ids.len(),
                limit: limits.max_allowed_faces,
            });
        }
        if let Some(max_distance_mm) = policy.max_distance_mm {
            if !max_distance_mm.is_finite() || max_distance_mm < 0.0 {
                return Err(AllowedFaceProjectionError::InvalidMaxDistance { max_distance_mm });
            }
        }

        let global_face_count = self.mesh.global_nfaces();
        for (position, &face_id) in allowed_face_ids.iter().enumerate() {
            if face_id as usize >= global_face_count {
                return Err(AllowedFaceProjectionError::FaceIdOutOfBounds {
                    position,
                    face_id,
                    global_face_count,
                });
            }
            if !self.mesh.contains_face(face_id) {
                return Err(AllowedFaceProjectionError::FaceIdNotSelected { position, face_id });
            }
        }
        for (point, value) in points.iter().enumerate() {
            for (axis, component) in value.iter().enumerate() {
                if !component.is_finite() {
                    return Err(AllowedFaceProjectionError::NonFinitePoint { point, axis });
                }
            }
        }

        // This exact shape-only total is known before any masks, leaf scans,
        // or allocations. Refuse doomed batches at the earliest boundary.
        let total_triangle_tests = checked_allowed_projection_mul(
            points.len(),
            allowed_face_ids.len(),
            "total triangle tests",
        )?;
        if total_triangle_tests > limits.max_total_triangle_tests {
            return Err(AllowedFaceProjectionError::ResourceLimitExceeded {
                resource: "total triangle tests",
                required: total_triangle_tests,
                limit: limits.max_total_triangle_tests,
            });
        }

        // Building the allowed-subtree mask visits every existing node once.
        // Charge that exact setup work before allocating or scanning the BVH.
        let setup_node_visits = self.bvh.nodes.len();
        if setup_node_visits > limits.max_total_node_visits {
            return Err(AllowedFaceProjectionError::ResourceLimitExceeded {
                resource: "setup node visits",
                required: setup_node_visits,
                limit: limits.max_total_node_visits,
            });
        }

        let requirements =
            self.allowed_projection_requirements(points.len(), allowed_face_ids.len())?;
        if requirements.memory_bytes > limits.max_memory_bytes {
            return Err(AllowedFaceProjectionError::ResourceLimitExceeded {
                resource: "memory bytes",
                required: requirements.memory_bytes,
                limit: limits.max_memory_bytes,
            });
        }

        let mut allowed_faces =
            reserve_allowed_projection_vec(global_face_count, "allowed-face mask")?;
        allowed_faces.resize(global_face_count, 0u8);
        for &face_id in allowed_face_ids {
            let mark = &mut allowed_faces[face_id as usize];
            if *mark != 0 {
                return Err(AllowedFaceProjectionError::DuplicateFaceId { face_id });
            }
            *mark = 1;
        }

        let mut allowed_leaf_faces = reserve_allowed_projection_vec(
            allowed_face_ids.len(),
            "allowed IDs in BVH leaf order",
        )?;
        let mut allowed_leaf_ranges =
            reserve_allowed_projection_vec(self.bvh.nodes.len(), "allowed leaf ranges")?;
        allowed_leaf_ranges.resize(self.bvh.nodes.len(), AllowedLeafRange::default());
        let mut allowed_subtrees =
            reserve_allowed_projection_vec(self.bvh.nodes.len(), "allowed-subtree mask")?;
        allowed_subtrees.resize(self.bvh.nodes.len(), 0u8);
        for node_index in (0..self.bvh.nodes.len()).rev() {
            let node = &self.bvh.nodes[node_index];
            allowed_subtrees[node_index] = if node.leaf {
                let start = node.a as usize;
                let count = node.b as usize;
                let allowed_start = allowed_leaf_faces.len();
                allowed_leaf_faces.extend(
                    self.bvh.order[start..start + count]
                        .iter()
                        .copied()
                        .filter(|&face_id| allowed_faces[face_id as usize] != 0),
                );
                let allowed_count = allowed_leaf_faces.len() - allowed_start;
                allowed_leaf_ranges[node_index] = AllowedLeafRange {
                    start: allowed_start,
                    count: allowed_count,
                };
                u8::from(allowed_count != 0)
            } else {
                u8::from(
                    allowed_subtrees[node.a as usize] != 0
                        || allowed_subtrees[node.b as usize] != 0,
                )
            };
        }
        debug_assert_ne!(allowed_subtrees[0], 0);
        debug_assert_eq!(allowed_leaf_faces.len(), allowed_face_ids.len());
        let marked_nodes = allowed_subtrees.iter().filter(|&&mark| mark != 0).count();
        let query_node_visits =
            checked_allowed_projection_mul(points.len(), marked_nodes, "total node visits")?;
        let total_node_visits = checked_allowed_projection_add(
            setup_node_visits,
            query_node_visits,
            "total node visits",
        )?;
        if total_node_visits > limits.max_total_node_visits {
            return Err(AllowedFaceProjectionError::ResourceLimitExceeded {
                resource: "total node visits",
                required: total_node_visits,
                limit: limits.max_total_node_visits,
            });
        }

        let stack_slots = checked_allowed_projection_mul(
            points.len(),
            requirements.stack_capacity,
            "traversal stack slots",
        )?;
        let mut traversal_stacks = reserve_allowed_projection_vec(stack_slots, "traversal stacks")?;
        traversal_stacks.resize(stack_slots, 0i32);
        let mut rows = reserve_allowed_projection_vec(points.len(), "parallel row results")?;
        let mut outcomes = reserve_allowed_projection_vec(points.len(), "ordered outcomes")?;
        let upper_bound = policy.max_distance_mm.unwrap_or(f64::INFINITY);
        par::run(|| {
            points
                .par_iter()
                .copied()
                .zip(traversal_stacks.par_chunks_mut(requirements.stack_capacity))
                .map(|(point, stack)| {
                    let (distance_mm, face, closest) = self.nearest_allowed_one_with_stack(
                        point,
                        upper_bound,
                        &allowed_leaf_faces,
                        &allowed_leaf_ranges,
                        &allowed_subtrees,
                        stack,
                    );
                    if face < global_face_count {
                        AllowedFaceProjectionOutcome::Projected(AllowedFaceProjection {
                            distance_mm,
                            point: closest,
                            face_id: face as u32,
                        })
                    } else {
                        let max_distance_mm = policy.max_distance_mm.expect(
                            "uncapped projection over a nonempty valid allowed set must win",
                        );
                        AllowedFaceProjectionOutcome::NoWinner(
                            AllowedFaceProjectionNoWinner::BeyondMaxDistance { max_distance_mm },
                        )
                    }
                })
                .collect_into_vec(&mut rows);
        });

        // Keep scatter ordered and serial. This is deliberately separate from
        // the parallel query stage so scheduling can never affect row order.
        outcomes.extend(rows);
        Ok(outcomes)
    }

    // ---- inside -----------------------------------------------------------
    pub fn inside(&self, pts: &[V3]) -> Vec<bool> {
        par::run(|| {
            pts.par_iter()
                .map(|&p| ZSCRATCH.with(|z| self.pv.inside(p, &mut z.borrow_mut())))
                .collect()
        })
    }

    #[inline]
    fn inside_serial(&self, p: V3) -> bool {
        ZSCRATCH.with(|z| self.pv.inside(p, &mut z.borrow_mut()))
    }

    // ---- ray casting ------------------------------------------------------
    /// Collect ALL Möller crossings for one ray (superset broad phase + exact
    /// filter). Returns unsorted (t, face) pairs.
    pub(crate) fn collect_hits(&self, o: V3, d: V3, t_max: f64) -> Vec<(f64, u32)> {
        let mut out: Vec<(f64, u32)> = Vec::new();
        self.collect_hits_into(o, d, t_max, &mut out);
        out
    }

    /// Shared allocation-free traversal for ordinary ray casting and the
    /// bounded penetration path. Callers which need a hard allocation ceiling
    /// reserve `out` before entering this routine.
    fn collect_hits_into(&self, o: V3, d: V3, t_max: f64, out: &mut Vec<(f64, u32)>) {
        STACK.with(|s| {
            self.collect_hits_with_stack(o, d, t_max, out, &mut s.borrow_mut());
        });
    }

    /// Returns the EXECUTED traversal effort of this walk — one per BVH node
    /// popped plus one per Möller evaluation — in exactly the units
    /// [`Self::count_hits_with_stack`] counts, so the census pass's `tests`
    /// and this walk's return value are the same quantity measured by the
    /// two walks. The metered penetration path
    /// ([`Self::penetration_intervals_batch_bounded_metered`], FU-8g H2)
    /// compares them; every other caller may ignore it.
    fn collect_hits_with_stack(
        &self,
        o: V3,
        d: V3,
        t_max: f64,
        out: &mut Vec<(f64, u32)>,
        stack: &mut Vec<i32>,
    ) -> usize {
        out.clear();
        let t0 = -EPS;
        let t1 = t_max + EPS;
        stack.clear();
        stack.push(0);
        let mut tests = 0usize;
        while let Some(ni) = stack.pop() {
            tests += 1;
            let node = &self.bvh.nodes[ni as usize];
            if !ray_box(o, d, node.bmin, node.bmax, node.pad, t0, t1) {
                continue;
            }
            if node.leaf {
                let start = node.a as usize;
                let cnt = node.b as usize;
                for &tid in &self.bvh.order[start..start + cnt] {
                    let t = tid as usize;
                    tests += 1;
                    let (hit, tv) = moller(
                        o,
                        d,
                        self.mesh.v0(t),
                        self.mesh.e1(t),
                        self.mesh.e2(t),
                        t_max,
                    );
                    if hit {
                        out.push((tv, tid));
                    }
                }
            } else {
                stack.push(node.a);
                stack.push(node.b);
            }
        }
        tests
    }

    /// Counting twin of [`Self::collect_hits_with_stack`]: the SAME traversal
    /// over the same immutable BVH with the same predicates, but it counts
    /// instead of pushing, so it needs the `O(depth)` stack and nothing else.
    ///
    /// Returns `(crossings, tests)` where `crossings` is exactly the number of
    /// `(t, tid)` pairs the collecting walk would push, and `tests` is the
    /// executed traversal effort: one per BVH node popped (its `ray_box`
    /// evaluation) plus one per Möller evaluation in a reached leaf.
    ///
    /// Any edit to one walk must be mirrored in the other; the fill pass
    /// refuses with [`PenetrationBatchError::CensusDrift`] if they disagree.
    fn count_hits_with_stack(
        &self,
        o: V3,
        d: V3,
        t_max: f64,
        stack: &mut Vec<i32>,
    ) -> (usize, usize) {
        let t0 = -EPS;
        let t1 = t_max + EPS;
        stack.clear();
        stack.push(0);
        let mut crossings = 0usize;
        let mut tests = 0usize;
        while let Some(ni) = stack.pop() {
            tests += 1;
            let node = &self.bvh.nodes[ni as usize];
            if !ray_box(o, d, node.bmin, node.bmax, node.pad, t0, t1) {
                continue;
            }
            if node.leaf {
                let start = node.a as usize;
                let cnt = node.b as usize;
                for &tid in &self.bvh.order[start..start + cnt] {
                    let t = tid as usize;
                    tests += 1;
                    let (hit, _tv) = moller(
                        o,
                        d,
                        self.mesh.v0(t),
                        self.mesh.e1(t),
                        self.mesh.e2(t),
                        t_max,
                    );
                    if hit {
                        crossings += 1;
                    }
                }
            } else {
                stack.push(node.a);
                stack.push(node.b);
            }
        }
        (crossings, tests)
    }

    /// cast_rays for one ray: deduped by arc location; representative t = first
    /// in group, representative face = LOWEST index in group (Amendment A13).
    fn cast_one(&self, o: V3, d: V3, t_max: f64) -> (Vec<f64>, Vec<i64>) {
        let seg_len = norm(d) * t_max;
        let mut hits = self.collect_hits(o, d, t_max);
        if hits.is_empty() {
            return (Vec::new(), Vec::new());
        }
        // stable sort by t
        hits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let dt = DEDUP_LOC_MM / seg_len.max(1e-9);
        let mut ts: Vec<f64> = Vec::new();
        let mut fs: Vec<i64> = Vec::new();
        let mut group_kept = hits[0].0;
        let mut group_min_face = hits[0].1;
        for &(t, f) in hits.iter().skip(1) {
            if t - group_kept > dt {
                // close current group
                ts.push(group_kept);
                fs.push(group_min_face as i64);
                group_kept = t;
                group_min_face = f;
            } else if f < group_min_face {
                group_min_face = f;
            }
        }
        ts.push(group_kept);
        fs.push(group_min_face as i64);
        (ts, fs)
    }

    /// Allocation-free crossing census for [`Self::cast_rays`]: the same BVH
    /// walk that call's collecting pass performs, counting instead of pushing,
    /// so it needs the `O(depth)` traversal stack per worker and nothing else.
    ///
    /// Element `i` is exactly the number of `(t, face)` pairs [`Self::cast_rays`]
    /// collects for ray `i` BEFORE arc-location dedup. Dedup only merges
    /// neighbours, so the same figure also bounds that ray's retained `ts`/`fs`
    /// pair and its contribution to the two flattened arrays.
    ///
    /// This is the COUNT of count-before-charge-before-allocate for ray
    /// casting, the same discipline [`Self::penetration_census_batch`] gives
    /// the penetration path. The conservative every-ray-crosses-every-face
    /// shape is a mesh-density ceiling, not a charge: a ray through a closed
    /// hollow shell crosses the surface a handful of times however finely that
    /// surface is tessellated.
    pub fn cast_rays_census(
        &self,
        origins: &[V3],
        dirs: &[V3],
        t_max: f64,
    ) -> Result<Vec<usize>, BatchInputError> {
        let n = origins.len();
        BatchInputError::require_len("cast_rays_census", "dirs", n, dirs.len())?;
        let mut per = vec![0usize; n];
        par::run(|| {
            (0..n)
                .into_par_iter()
                .map(|i| {
                    let (crossings, _tests) = STACK.with(|s| {
                        self.count_hits_with_stack(origins[i], dirs[i], t_max, &mut s.borrow_mut())
                    });
                    crossings
                })
                .collect_into_vec(&mut per);
        });
        Ok(per)
    }

    /// Batched cast_rays -> ragged (offsets, t_all, face_all).
    pub fn cast_rays(
        &self,
        origins: &[V3],
        dirs: &[V3],
        t_max: f64,
    ) -> Result<CastRaysBatch, BatchInputError> {
        let n = origins.len();
        BatchInputError::require_len("cast_rays", "dirs", n, dirs.len())?;
        let per: Vec<(Vec<f64>, Vec<i64>)> = par::run(|| {
            (0..n)
                .into_par_iter()
                .map(|i| self.cast_one(origins[i], dirs[i], t_max))
                .collect()
        });
        // serial scan into offsets + flat arrays (fixed index order)
        let mut offsets = Vec::with_capacity(n + 1);
        offsets.push(0i64);
        let mut t_all = Vec::new();
        let mut f_all = Vec::new();
        for (ts, fs) in &per {
            t_all.extend_from_slice(ts);
            f_all.extend_from_slice(fs);
            offsets.push(t_all.len() as i64);
        }
        Ok((offsets, t_all, f_all))
    }

    // ---- segment / capsule minimum distance ------------------------------
    /// Exact segment [a,b] vs one triangle: min over endpoints, 3 edges, pierce.
    /// Mirrors _seg_tri_dist (geometry_exact.py:596-630).
    #[inline]
    pub(crate) fn seg_tri_dist(&self, a: V3, b: V3, t: usize) -> (f64, V3) {
        let v0 = self.mesh.v0(t);
        let p = v0;
        let q = add(v0, self.mesh.e1(t));
        let r = add(v0, self.mesh.e2(t));
        let mut best_d = f64::INFINITY;
        let mut best_pt: V3 = [0.0; 3];
        // (a) endpoints vs triangle
        let cpa = closest_on_tri(a, p, q, r);
        let da = norm(sub(a, cpa));
        if da < best_d {
            best_d = da;
            best_pt = cpa;
        }
        let cpb = closest_on_tri(b, p, q, r);
        let db = norm(sub(b, cpb));
        if db < best_d {
            best_d = db;
            best_pt = cpb;
        }
        // (b) segment vs each triangle edge
        for &(x, y) in &[(p, q), (q, r), (r, p)] {
            let (d, _c1, c2) = segseg(a, b, x, y);
            if d < best_d {
                best_d = d;
                best_pt = c2;
            }
        }
        // (c) interior pierce -> distance 0 at crossing point
        let dv = sub(b, a);
        let (hit, tv) = moller(a, dv, v0, self.mesh.e1(t), self.mesh.e2(t), 1.0);
        if hit {
            let pierce = add(a, scale(dv, tv));
            if 0.0 < best_d {
                best_d = 0.0;
                best_pt = pierce;
            }
        }
        (best_d, best_pt)
    }

    /// Exact minimum distance from segment [a,b] to the surface (branch & bound).
    pub(crate) fn seg_surface_min(&self, a: V3, b: V3) -> (f64, V3) {
        let smin = box_min(a, b);
        let smax = box_max(a, b);
        let mut best_d = f64::INFINITY;
        let mut best_pt: V3 = [f64::NAN; 3];
        STACK.with(|s| {
            let mut stack = s.borrow_mut();
            stack.clear();
            stack.push(0);
            while let Some(ni) = stack.pop() {
                let node = &self.bvh.nodes[ni as usize];
                if box_box_dist(smin, smax, node.bmin, node.bmax) > best_d {
                    continue;
                }
                if node.leaf {
                    let start = node.a as usize;
                    let cnt = node.b as usize;
                    for &tid in &self.bvh.order[start..start + cnt] {
                        let (d, pt) = self.seg_tri_dist(a, b, tid as usize);
                        if d < best_d {
                            best_d = d;
                            best_pt = pt;
                            // pierce early-out allowed (spec sec 3.3)
                        }
                    }
                } else {
                    let l = node.a as usize;
                    let r = node.b as usize;
                    let dl =
                        box_box_dist(smin, smax, self.bvh.nodes[l].bmin, self.bvh.nodes[l].bmax);
                    let dr =
                        box_box_dist(smin, smax, self.bvh.nodes[r].bmin, self.bvh.nodes[r].bmax);
                    if dl <= dr {
                        stack.push(r as i32);
                        stack.push(l as i32);
                    } else {
                        stack.push(l as i32);
                        stack.push(r as i32);
                    }
                }
            }
        });
        (best_d, best_pt)
    }

    /// One segment clearance. Tier-1 open-space branch is NOT reproduced
    /// (Amendment A1): always exact segment-to-surface clearance.
    // The 7-argument geometry-query signature (endpoints, radii, standoff,
    // insets) is the inherent shape of this API, mirrored 1:1 by
    // `segment_clearance`'s own batched signature just below and by the
    // independent verify.py/census callers this kernel serves (SPEC-EMBED-
    // POLICY, SCAR-AREA-CAP) -- bundling them into a struct would just move
    // the same fields one level of indirection away with no clarity gain,
    // and every field is read at every call site. Not a candidate for the
    // batch-first hot-path rule either: this is the per-item primitive the
    // BATCHED caller already amortizes over (see `segment_clearance` below).
    #[allow(clippy::too_many_arguments)]
    fn seg_clear_one(
        &self,
        p0: V3,
        p1: V3,
        r0: f64,
        r1: f64,
        standoff: f64,
        inset0: f64,
        inset1: f64,
    ) -> (f64, V3) {
        let nan3: V3 = [f64::NAN; 3];
        let d = sub(p1, p0);
        let l = norm(d);
        if l < 1e-9 {
            return (f64::INFINITY, nan3);
        }
        let u = scale(d, 1.0 / l);
        let s_lo = inset0;
        let s_hi = l - inset1;
        if s_hi - s_lo <= 1e-9 {
            return (f64::INFINITY, nan3);
        }
        let a = add(p0, scale(u, s_lo));
        let b = add(p0, scale(u, s_hi));
        let r_lo = r0 + (r1 - r0) * (s_lo / l);
        let r_hi = r0 + (r1 - r0) * (s_hi / l);
        let r_max = r_lo.max(r_hi);
        let thresh = r_max + standoff;
        let (best_d, best_pt) = self.seg_surface_min(a, b);
        let clr = best_d - thresh;
        // clearance-vs-0 threshold band (spec sec 5.4 item 3)
        self.band_check(clr, 0.0);
        (clr, best_pt)
    }

    // Same rationale as `seg_clear_one` immediately above: this is that
    // primitive's batched (SoA) entry point, one argument per per-member
    // input array plus the shared `standoff` -- the established batch shape,
    // not a candidate for bundling.
    #[allow(clippy::too_many_arguments)]
    pub fn segment_clearance(
        &self,
        p0s: &[V3],
        p1s: &[V3],
        r0s: &[f64],
        r1s: &[f64],
        standoff: f64,
        inset0s: &[f64],
        inset1s: &[f64],
    ) -> Result<(Vec<f64>, Vec<V3>), BatchInputError> {
        let n = p0s.len();
        BatchInputError::require_len("segment_clearance", "p1s", n, p1s.len())?;
        BatchInputError::require_len("segment_clearance", "r0s", n, r0s.len())?;
        BatchInputError::require_len("segment_clearance", "r1s", n, r1s.len())?;
        BatchInputError::require_len("segment_clearance", "inset0s", n, inset0s.len())?;
        BatchInputError::require_len("segment_clearance", "inset1s", n, inset1s.len())?;
        let res: Vec<(f64, V3)> = par::run(|| {
            (0..n)
                .into_par_iter()
                .map(|i| {
                    self.seg_clear_one(
                        p0s[i], p1s[i], r0s[i], r1s[i], standoff, inset0s[i], inset1s[i],
                    )
                })
                .collect()
        });
        let mut clr = Vec::with_capacity(n);
        let mut pt = Vec::with_capacity(n);
        for (c, p) in res {
            clr.push(c);
            pt.push(p);
        }
        Ok((clr, pt))
    }

    // ---- penetration intervals -------------------------------------------
    // Same rationale as `seg_clear_one` above: the per-member primitive
    // behind the batched `penetration_intervals_batch` entry point
    // (SPEC-EMBED-POLICY rule-4 transit detection) -- every argument is the
    // geometry + sampling config a single interval extraction needs.
    #[allow(clippy::too_many_arguments)]
    fn pen_intervals_one(
        &self,
        p0: V3,
        p1: V3,
        r0: f64,
        r1: f64,
        n_offset_rays: usize,
        majority_step_mm: f64,
        cap: usize,
        min_illegal_len_mm: f64,
        rays_all: bool,
        budget: PenetrationRowBudget,
        row: usize,
    ) -> Result<(Vec<Interval>, PenetrationRowMeter), PenetrationBatchError> {
        let l = norm(sub(p1, p0));
        let mut meter = PenetrationRowMeter::default();
        let mut out = reserve_penetration_vec(budget.intervals, Some(row), "interval output")?;
        if l < 1e-9 {
            return Ok((out, meter));
        }
        let mut traversal_stack =
            reserve_penetration_vec(budget.traversal_stack, Some(row), "BVH traversal stack")?;
        let mut parity_crossings =
            reserve_penetration_vec(budget.parity_crossings, Some(row), "parity crossings")?;
        // Shared construction keeps query geometry identical to consumers'
        // interval-bound validation.
        let mut raylist = reserve_penetration_vec(budget.rays, Some(row), "ray list")?;
        raylist.extend(capsule_rays(p0, p1, r0, r1, n_offset_rays, rays_all));

        for ray in raylist {
            let label = ray.label();
            let o = ray.origin();
            let dv = ray.direction();
            let seg = ray.length_mm();
            if seg < 1e-9 {
                continue;
            }
            // crossings along this ray
            let mut hits = reserve_penetration_vec(budget.hits_per_ray, Some(row), "hits")?;
            meter.work += self.collect_hits_with_stack(o, dv, 1.0, &mut hits, &mut traversal_stack);
            meter.hits += hits.len();
            if hits.len() > budget.hits_per_ray {
                return Err(PenetrationBatchError::CensusDrift {
                    row,
                    resource: "hits",
                    counted: budget.hits_per_ray,
                    executed: hits.len(),
                });
            }
            let mut tvals =
                reserve_penetration_vec(budget.hits_per_ray, Some(row), "crossing parameters")?;
            tvals.extend(hits.iter().map(|h| h.0));
            tvals.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mut ts =
                reserve_penetration_vec(budget.hits_per_ray, Some(row), "deduplicated crossings")?;
            dedup_loc_into(&tvals, DEDUP_LOC_MM / seg.max(1e-9), &mut ts);
            // boundaries [0, ts..., 1]
            let mut b_bounds = reserve_penetration_vec(
                checked_penetration_add(budget.hits_per_ray, 2, "boundary capacity")?,
                Some(row),
                "boundaries",
            )?;
            b_bounds.push(0.0);
            b_bounds.extend_from_slice(&ts);
            b_bounds.push(1.0);

            // sample each sub-segment, gather all sample points
            let mut sample_pts =
                reserve_penetration_vec(budget.samples_per_ray, Some(row), "sample points")?;
            let mut slices =
                reserve_penetration_vec(budget.subsegments_per_ray, Some(row), "sample slices")?;
            let mut sub_bounds = reserve_penetration_vec(
                budget.subsegments_per_ray,
                Some(row),
                "subsegment bounds",
            )?;
            let nb = b_bounds.len();
            if nb - 1 > budget.subsegments_per_ray {
                return Err(PenetrationBatchError::CensusDrift {
                    row,
                    resource: "subsegments",
                    counted: budget.subsegments_per_ray,
                    executed: nb - 1,
                });
            }
            for si in 0..nb - 1 {
                let ta = b_bounds[si];
                let tb = b_bounds[si + 1];
                let lsub = (tb - ta) * seg;
                let ns = cap.min((3usize).max((lsub / majority_step_mm).trunc() as usize));
                // Interior linear spacing: step=(tb-ta)/(ns+1), i=1..ns.
                let step = (tb - ta) / ((ns + 1) as f64);
                let run0 = sample_pts.len();
                for i in 1..=ns {
                    let ti = ta + (i as f64) * step;
                    sample_pts.push(add(o, scale(dv, ti)));
                }
                slices.push((run0, sample_pts.len()));
                sub_bounds.push((ta, tb, si != 0, (si + 1) != (nb - 1)));
            }
            if sample_pts.len() > budget.samples_per_ray {
                return Err(PenetrationBatchError::CensusDrift {
                    row,
                    resource: "samples",
                    counted: budget.samples_per_ray,
                    executed: sample_pts.len(),
                });
            }
            meter.samples += sample_pts.len();

            // inside classification + depth (nearest dist) for inside samples
            let mut ins_all =
                reserve_penetration_vec(budget.samples_per_ray, Some(row), "inside samples")?;
            ins_all.resize(sample_pts.len(), false);
            let mut dist_all =
                reserve_penetration_vec(budget.samples_per_ray, Some(row), "sample depths")?;
            dist_all.resize(sample_pts.len(), 0.0f64);
            for (i, &sp) in sample_pts.iter().enumerate() {
                let ins = self.pv.inside(sp, &mut parity_crossings);
                ins_all[i] = ins;
                if ins {
                    let (di, _, _) =
                        self.nearest_one_with_stack(sp, f64::INFINITY, &mut traversal_stack);
                    dist_all[i] = di;
                }
            }

            // per sub-segment inside vote + depth
            let nsub = slices.len();
            let mut insub = reserve_penetration_vec(
                budget.subsegments_per_ray,
                Some(row),
                "subsegment inside votes",
            )?;
            insub.resize(nsub, false);
            let mut depsub = reserve_penetration_vec(
                budget.subsegments_per_ray,
                Some(row),
                "subsegment depths",
            )?;
            depsub.resize(nsub, 0.0f64);
            for (k, &(a0, a1)) in slices.iter().enumerate() {
                if a1 > a0 {
                    let cnt = a1 - a0;
                    let mut nin = 0usize;
                    let mut dmax = 0.0f64;
                    for i in a0..a1 {
                        if ins_all[i] {
                            nin += 1;
                            if dist_all[i] > dmax {
                                dmax = dist_all[i];
                            }
                        }
                    }
                    // mean >= 0.5
                    insub[k] = (nin as f64) / (cnt as f64) >= 0.5;
                    depsub[k] = dmax;
                }
            }

            // merge consecutive inside runs into intervals
            let mut i = 0usize;
            while i < nsub {
                if insub[i] {
                    let mut jrun = i;
                    while jrun + 1 < nsub && insub[jrun + 1] {
                        jrun += 1;
                    }
                    let ta = sub_bounds[i].0;
                    let tb = sub_bounds[jrun].1;
                    let entered = sub_bounds[i].2;
                    let exited = sub_bounds[jrun].3;
                    let mut depth = 0.0f64;
                    for d in &depsub[i..=jrun] {
                        if *d > depth {
                            depth = *d;
                        }
                    }
                    let s_a = ta * seg;
                    let s_b = tb * seg;
                    // interval-length vs min_illegal_len threshold band (sec 5.4 item 3)
                    self.band_check(s_b - s_a, min_illegal_len_mm);
                    if (s_b - s_a) > min_illegal_len_mm {
                        out.push(Interval {
                            s_a,
                            s_b,
                            entered,
                            exited,
                            max_depth: depth,
                            ray: label,
                        });
                    }
                    i = jrun + 1;
                } else {
                    i += 1;
                }
            }
            if out.len() > budget.intervals {
                return Err(PenetrationBatchError::CensusDrift {
                    row,
                    resource: "intervals",
                    counted: budget.intervals,
                    executed: out.len(),
                });
            }
        }
        meter.intervals = out.len();
        Ok((out, meter))
    }

    fn penetration_requirements(
        &self,
        rows: usize,
        n_offset_rays: usize,
        cap: usize,
        rays_all: bool,
    ) -> Result<PenetrationBatchRequirements, PenetrationBatchError> {
        if rows == 0 {
            return Ok(PenetrationBatchRequirements {
                rows: 0,
                work: 0,
                hits: 0,
                samples: 0,
                intervals: 0,
                scratch_bytes: 0,
            });
        }
        if rays_all && n_offset_rays > i32::MAX as usize {
            return Err(PenetrationBatchError::ArithmeticOverflow {
                resource: "offset ray labels",
            });
        }
        let rays = if rays_all {
            checked_penetration_add(n_offset_rays, 1, "rays per row")?
        } else {
            1
        };
        let hits_per_ray = self.mesh.nfaces();
        let subsegments_per_ray = checked_penetration_add(hits_per_ray, 1, "subsegments per ray")?;
        let samples_per_ray = checked_penetration_mul(subsegments_per_ray, cap, "samples per ray")?;
        let intervals_per_row =
            checked_penetration_mul(rays, subsegments_per_ray, "intervals per row")?;
        // `work` is charged as executed traversal tests (nodes popped plus
        // Möller evaluations) since FU-8g H1, and a walk can pop every BVH
        // node and test every face, so the sufficient ceiling is their sum —
        // `nfaces` alone UNDER-states tiny meshes, where node-visit overhead
        // is not amortized away (H0 finding 1: `make_box` 0.82x). `hits`
        // stays `nfaces`: the two quantities are severed here exactly as in
        // the census derivation.
        let tests_per_ray =
            checked_penetration_add(hits_per_ray, self.bvh.nodes.len(), "tests per ray")?;
        let work_per_row = checked_penetration_mul(rays, tests_per_ray, "work per row")?;
        let samples_per_row = checked_penetration_mul(rays, samples_per_ray, "samples per row")?;
        let traversal_stack = checked_penetration_add(self.bvh.depth, 1, "BVH traversal stack")?;
        let parity_crossings = self
            .pv
            .grids
            .iter()
            .map(|grid| grid.max_occ)
            .max()
            .unwrap_or(0);

        let row_scratch = penetration_row_scratch_bytes(
            rays,
            hits_per_ray,
            subsegments_per_ray,
            samples_per_ray,
            traversal_stack,
            parity_crossings,
        )?;
        let hits_per_row = checked_penetration_mul(rays, hits_per_ray, "hits per row")?;
        Ok(PenetrationBatchRequirements {
            rows,
            work: checked_penetration_mul(rows, work_per_row, "total work")?,
            hits: checked_penetration_mul(rows, hits_per_row, "total hits")?,
            samples: checked_penetration_mul(rows, samples_per_row, "total samples")?,
            intervals: checked_penetration_mul(rows, intervals_per_row, "total intervals")?,
            scratch_bytes: checked_penetration_mul(rows, row_scratch, "total scratch bytes")?,
        })
    }

    /// The counted demand of one row: every ray a fill walk would execute is
    /// walked once, counting instead of pushing. Degenerate rows and rays are
    /// skipped exactly as `pen_intervals_one` skips them.
    fn census_one(
        &self,
        p0: V3,
        p1: V3,
        r0: f64,
        r1: f64,
        n_offset_rays: usize,
        rays_all: bool,
    ) -> PenetrationRowCensus {
        let l = norm(sub(p1, p0));
        let mut census = PenetrationRowCensus {
            crossings: 0,
            tests: 0,
            seg_mm: 0.0,
            max_ray_crossings: 0,
            max_ray_seg_mm: 0.0,
        };
        if l < 1e-9 {
            return census;
        }
        for ray in capsule_rays(p0, p1, r0, r1, n_offset_rays, rays_all) {
            let seg = ray.length_mm();
            if seg < 1e-9 {
                continue;
            }
            let (crossings, tests) = STACK.with(|s| {
                self.count_hits_with_stack(ray.origin(), ray.direction(), 1.0, &mut s.borrow_mut())
            });
            census.crossings += crossings;
            census.tests += tests;
            census.seg_mm += seg;
            census.max_ray_crossings = census.max_ray_crossings.max(crossings);
            if seg > census.max_ray_seg_mm {
                census.max_ray_seg_mm = seg;
            }
        }
        census
    }

    /// Batched crossing/traversal census: one allocation-free counting pass
    /// per ray, index-parallel over rows, with NO per-hit storage — the BVH
    /// walk is identical to the fill walk's but counts instead of pushing, so
    /// it needs `O(bvh.depth)` memory per worker and nothing else.
    ///
    /// This is the COUNT of count-before-charge-before-allocate: feed the
    /// result to [`Self::penetration_intervals_batch_census_limits`] to state
    /// the batch's honest demand before any interval-sized allocation exists.
    pub fn penetration_census_batch(
        &self,
        p0s: &[V3],
        p1s: &[V3],
        r0s: &[f64],
        r1s: &[f64],
        n_offset_rays: usize,
        rays_all: bool,
    ) -> Result<Vec<PenetrationRowCensus>, PenetrationBatchError> {
        let n = p0s.len();
        Self::validate_penetration_inputs(p0s, p1s, r0s, r1s)?;
        if rays_all && n_offset_rays > i32::MAX as usize {
            return Err(PenetrationBatchError::ArithmeticOverflow {
                resource: "offset ray labels",
            });
        }
        let mut per = reserve_penetration_vec(n, None, "census rows")?;
        par::run(|| {
            (0..n)
                .into_par_iter()
                .map(|i| self.census_one(p0s[i], p1s[i], r0s[i], r1s[i], n_offset_rays, rays_all))
                .collect_into_vec(&mut per);
        });
        Ok(per)
    }

    /// Measured sibling of [`Self::penetration_requirements`]: per-row budgets
    /// from counted crossings, the closed-form sample bound, `work` charged as
    /// traversal tests and `hits` as crossings (severed — they measure
    /// different things), and totals that are SUMS, never `rows x max`.
    ///
    /// A row budget's per-ray fields hold the row's worst SINGLE ray (the
    /// buffers are re-created per ray, so that is the sound reservation),
    /// while the interval budget and the batch totals hold the row sums.
    /// Every census figure is bounded above by the corresponding worst-case
    /// figure, so a worst-case-derived limit always admits the counted
    /// demand.
    fn penetration_requirements_from_census(
        &self,
        census: &[PenetrationRowCensus],
        n_offset_rays: usize,
        cap: usize,
        majority_step_mm: f64,
        rays_all: bool,
    ) -> Result<PenetrationCensusRequirements, PenetrationBatchError> {
        if rays_all && n_offset_rays > i32::MAX as usize {
            return Err(PenetrationBatchError::ArithmeticOverflow {
                resource: "offset ray labels",
            });
        }
        let rays = if rays_all {
            checked_penetration_add(n_offset_rays, 1, "rays per row")?
        } else {
            1
        };
        let traversal_stack = checked_penetration_add(self.bvh.depth, 1, "BVH traversal stack")?;
        let parity_crossings = self
            .pv
            .grids
            .iter()
            .map(|grid| grid.max_occ)
            .max()
            .unwrap_or(0);
        let mut row_budgets = reserve_penetration_vec(census.len(), None, "row budgets")?;
        let mut work = 0usize;
        let mut hits = 0usize;
        let mut samples = 0usize;
        let mut intervals = 0usize;
        let mut scratch_bytes = 0usize;
        for row in census {
            // Per-ray reservations come from the row's worst single ray (the
            // buffers are re-created per ray); the batch totals come from the
            // row's sums. Per ray, deduplicated crossings + 1 bound that
            // ray's subsegments.
            let subsegments_per_ray =
                checked_penetration_add(row.max_ray_crossings, 1, "subsegments per ray")?;
            let samples_per_ray = census_row_sample_budget(
                subsegments_per_ray,
                row.max_ray_seg_mm,
                majority_step_mm,
                cap,
            )?;
            // Summed over the row's rays: subsegments <= crossings + rays,
            // and the sample sum obeys the same closed-form bound on the
            // summed arc length.
            let row_subsegments = checked_penetration_add(row.crossings, rays, "row subsegments")?;
            let row_samples =
                census_row_sample_budget(row_subsegments, row.seg_mm, majority_step_mm, cap)?;
            let row_scratch = penetration_row_scratch_bytes(
                rays,
                row.max_ray_crossings,
                subsegments_per_ray,
                samples_per_ray,
                traversal_stack,
                parity_crossings,
            )?;
            work = checked_penetration_add(work, row.tests, "total work")?;
            hits = checked_penetration_add(hits, row.crossings, "total hits")?;
            samples = checked_penetration_add(samples, row_samples, "total samples")?;
            intervals = checked_penetration_add(intervals, row_subsegments, "total intervals")?;
            scratch_bytes =
                checked_penetration_add(scratch_bytes, row_scratch, "total scratch bytes")?;
            row_budgets.push(PenetrationRowBudget {
                rays,
                hits_per_ray: row.max_ray_crossings,
                samples_per_ray,
                subsegments_per_ray,
                intervals: row_subsegments,
                traversal_stack,
                parity_crossings,
            });
        }
        Ok(PenetrationCensusRequirements {
            rows: census.len(),
            work,
            hits,
            samples,
            intervals,
            scratch_bytes,
            row_budgets,
        })
    }

    /// Honest, counted limits for this batch shape — the census-derived
    /// sibling of [`Self::penetration_intervals_batch_worst_case_limits`].
    /// `max_total_work` is summed traversal tests (what the ray loop
    /// executes); `max_total_hits` is summed crossings (what the hit buffers
    /// store); every total is a sum over the counted rows.
    pub fn penetration_intervals_batch_census_limits(
        &self,
        census: &[PenetrationRowCensus],
        n_offset_rays: usize,
        cap: usize,
        majority_step_mm: f64,
        rays_all: bool,
    ) -> Result<PenetrationBatchLimits, PenetrationBatchError> {
        let requirements = self.penetration_requirements_from_census(
            census,
            n_offset_rays,
            cap,
            majority_step_mm,
            rays_all,
        )?;
        Ok(PenetrationBatchLimits {
            max_rows: requirements.rows,
            max_total_work: requirements.work,
            max_total_hits: requirements.hits,
            max_total_samples: requirements.samples,
            max_total_intervals: requirements.intervals,
            max_scratch_bytes: requirements.scratch_bytes,
        })
    }

    fn check_penetration_limits(
        requirements: &PenetrationCensusRequirements,
        limits: PenetrationBatchLimits,
    ) -> Result<(), PenetrationBatchError> {
        for (resource, required, limit) in [
            ("rows", requirements.rows, limits.max_rows),
            ("work", requirements.work, limits.max_total_work),
            ("hits", requirements.hits, limits.max_total_hits),
            ("samples", requirements.samples, limits.max_total_samples),
            (
                "intervals",
                requirements.intervals,
                limits.max_total_intervals,
            ),
            (
                "scratch bytes",
                requirements.scratch_bytes,
                limits.max_scratch_bytes,
            ),
        ] {
            if required > limit {
                return Err(PenetrationBatchError::ResourceLimitExceeded {
                    resource,
                    required,
                    limit,
                });
            }
        }
        Ok(())
    }

    fn validate_penetration_inputs(
        p0s: &[V3],
        p1s: &[V3],
        r0s: &[f64],
        r1s: &[f64],
    ) -> Result<(), PenetrationBatchError> {
        let n = p0s.len();
        BatchInputError::require_len("penetration_intervals_batch", "p1s", n, p1s.len())?;
        BatchInputError::require_len("penetration_intervals_batch", "r0s", n, r0s.len())?;
        BatchInputError::require_len("penetration_intervals_batch", "r1s", n, r1s.len())?;
        Ok(())
    }

    /// Conservative limits sufficient for this surface and batch shape. This
    /// is a sizing helper, not a production policy: callers normally lower one
    /// or more fields to their own admitted workload budget.
    pub fn penetration_intervals_batch_worst_case_limits(
        &self,
        rows: usize,
        n_offset_rays: usize,
        cap: usize,
        rays_all: bool,
    ) -> Result<PenetrationBatchLimits, PenetrationBatchError> {
        let requirements = self.penetration_requirements(rows, n_offset_rays, cap, rays_all)?;
        Ok(PenetrationBatchLimits {
            max_rows: requirements.rows,
            max_total_work: requirements.work,
            max_total_hits: requirements.hits,
            max_total_samples: requirements.samples,
            max_total_intervals: requirements.intervals,
            max_scratch_bytes: requirements.scratch_bytes,
        })
    }

    /// Bounded, one-shot batch extraction of penetration intervals: count,
    /// charge, allocate. An allocation-free census pass counts the batch's
    /// demand first, the counted totals are checked against `limits` before
    /// any interval-sized allocation exists, and each row then executes under
    /// its own counted budget (pre-reserved, fallible scratch buffers, with
    /// [`PenetrationBatchError::CensusDrift`] as the typed refusal should
    /// execution ever exceed its count). Input gathering, one index-parallel
    /// map per pass, and ordered scatter are retained.
    ///
    /// This entry point discards the execution receipt. A caller that charges
    /// a CAP from this batch's counted demand should call
    /// [`Self::penetration_intervals_batch_bounded_metered`] instead and check
    /// the receipt against what it charged (FU-8g H2).
    #[allow(clippy::too_many_arguments)]
    pub fn penetration_intervals_batch_bounded(
        &self,
        p0s: &[V3],
        p1s: &[V3],
        r0s: &[f64],
        r1s: &[f64],
        n_offset_rays: usize,
        majority_step_mm: f64,
        cap: usize,
        min_illegal_len_mm: f64,
        rays_all: bool,
        limits: PenetrationBatchLimits,
    ) -> Result<Vec<Vec<Interval>>, PenetrationBatchError> {
        self.penetration_intervals_batch_bounded_metered(
            p0s,
            p1s,
            r0s,
            r1s,
            n_offset_rays,
            majority_step_mm,
            cap,
            min_illegal_len_mm,
            rays_all,
            limits,
        )
        .map(|(intervals, _meter)| intervals)
    }

    /// [`Self::penetration_intervals_batch_bounded`], plus the receipt: what
    /// the fill pass ACTUALLY executed, so a caller that charged a cap from
    /// the counted demand can assert `executed <= charged` instead of
    /// assuming it (FU-8g H2, the G3/G6c charged-vs-executed idiom).
    ///
    /// The receipt is checked here too, against this batch's OWN counted
    /// requirements, and a batch that outruns its count is refused with
    /// [`PenetrationBatchError::ChargeDrift`] rather than returned. The
    /// returned meter is therefore always within the census totals; a caller
    /// re-checks it against ITS charge because the caller's charge was
    /// derived from a separate counting pass, and only the caller knows which
    /// number it spent.
    #[allow(clippy::too_many_arguments)]
    pub fn penetration_intervals_batch_bounded_metered(
        &self,
        p0s: &[V3],
        p1s: &[V3],
        r0s: &[f64],
        r1s: &[f64],
        n_offset_rays: usize,
        majority_step_mm: f64,
        cap: usize,
        min_illegal_len_mm: f64,
        rays_all: bool,
        limits: PenetrationBatchLimits,
    ) -> Result<(Vec<Vec<Interval>>, PenetrationBatchMeter), PenetrationBatchError> {
        let n = p0s.len();
        Self::validate_penetration_inputs(p0s, p1s, r0s, r1s)?;
        let census = self.penetration_census_batch(p0s, p1s, r0s, r1s, n_offset_rays, rays_all)?;
        let requirements = self.penetration_requirements_from_census(
            &census,
            n_offset_rays,
            cap,
            majority_step_mm,
            rays_all,
        )?;
        Self::check_penetration_limits(&requirements, limits)?;
        let mut per = reserve_penetration_vec(n, None, "ordered row results")?;
        let row_budgets = &requirements.row_budgets;
        par::run(|| {
            (0..n)
                .into_par_iter()
                .map(|i| {
                    self.pen_intervals_one(
                        p0s[i],
                        p1s[i],
                        r0s[i],
                        r1s[i],
                        n_offset_rays,
                        majority_step_mm,
                        cap,
                        min_illegal_len_mm,
                        rays_all,
                        row_budgets[i],
                        i,
                    )
                })
                .collect_into_vec(&mut per);
        });
        // Rayon completes all rows before this ordered scan. A resource or
        // allocator failure therefore always reports the lowest failing row,
        // regardless of which worker happened to finish first.
        let first_error = per.iter().position(Result::is_err);
        if let Some(index) = first_error {
            return match per.swap_remove(index) {
                Err(error) => Err(error),
                Ok(_) => unreachable!("checked above"),
            };
        }
        let mut ordered = reserve_penetration_vec(n, None, "ordered interval rows")?;
        let mut meter = PenetrationBatchMeter {
            rows: n,
            ..PenetrationBatchMeter::default()
        };
        for row in per {
            let (intervals, executed) = row.expect("checked above");
            meter.work += executed.work;
            meter.hits += executed.hits;
            meter.samples += executed.samples;
            meter.intervals += executed.intervals;
            ordered.push(intervals);
        }
        Self::check_penetration_meter(&requirements, meter)?;
        Ok((ordered, meter))
    }

    /// The executed receipt against the counted charge, resource by resource.
    /// `work` and `hits` are the same quantities the census counted, measured
    /// by the other walk, so on an undrifted pair they are EQUAL; `samples`
    /// and `intervals` land at or under their counted budgets.
    fn check_penetration_meter(
        requirements: &PenetrationCensusRequirements,
        meter: PenetrationBatchMeter,
    ) -> Result<(), PenetrationBatchError> {
        for (resource, charged, executed) in [
            ("rows", requirements.rows, meter.rows),
            ("work", requirements.work, meter.work),
            ("hits", requirements.hits, meter.hits),
            ("samples", requirements.samples, meter.samples),
            ("intervals", requirements.intervals, meter.intervals),
        ] {
            if executed > charged {
                return Err(PenetrationBatchError::ChargeDrift {
                    resource,
                    charged,
                    executed,
                });
            }
        }
        Ok(())
    }

    /// Crate-internal compatibility entry point retained solely for the inline
    /// PyO3 frozen-oracle surface. Delete it with that Python surface in M9.
    /// Rust production callers cannot bypass an explicit resource policy.
    #[allow(
        dead_code,
        reason = "retained until the inline PyO3 frozen-oracle surface is deleted in M9"
    )]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn penetration_intervals_batch(
        &self,
        p0s: &[V3],
        p1s: &[V3],
        r0s: &[f64],
        r1s: &[f64],
        n_offset_rays: usize,
        majority_step_mm: f64,
        cap: usize,
        min_illegal_len_mm: f64,
        rays_all: bool,
    ) -> Result<Vec<Vec<Interval>>, PenetrationBatchError> {
        Self::validate_penetration_inputs(p0s, p1s, r0s, r1s)?;
        let limits = self.penetration_intervals_batch_worst_case_limits(
            p0s.len(),
            n_offset_rays,
            cap,
            rays_all,
        )?;
        self.penetration_intervals_batch_bounded(
            p0s,
            p1s,
            r0s,
            r1s,
            n_offset_rays,
            majority_step_mm,
            cap,
            min_illegal_len_mm,
            rays_all,
            limits,
        )
    }

    // ---- sphere breach ----------------------------------------------------
    /// Collect faces whose exact point-distance to `c` is within `tol` of `nd`
    /// (for measured-tie detection). Branch & bound, ascending face index.
    fn faces_at_distance(&self, c: V3, nd: f64, tol: f64) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        let thr = nd + tol;
        STACK.with(|s| {
            let mut stack = s.borrow_mut();
            stack.clear();
            stack.push(0);
            while let Some(ni) = stack.pop() {
                let node = &self.bvh.nodes[ni as usize];
                if point_box_dist(c, node.bmin, node.bmax) > thr {
                    continue;
                }
                if node.leaf {
                    let start = node.a as usize;
                    let cnt = node.b as usize;
                    for &tid in &self.bvh.order[start..start + cnt] {
                        let t = tid as usize;
                        let cp = closest_on_tri(
                            c,
                            self.mesh.v0(t),
                            self.mesh.vertex_b(t),
                            self.mesh.vertex_c(t),
                        );
                        let d = norm(sub(c, cp));
                        if d <= thr {
                            out.push(t);
                        }
                    }
                } else {
                    stack.push(node.a);
                    stack.push(node.b);
                }
            }
        });
        out.sort_unstable();
        out
    }

    /// far-wall thickness along the inward cone for one orientation.
    /// inf if no far wall beyond the near-wall skip.
    fn cone_thickness(
        &self,
        c: V3,
        r: f64,
        inward: V3,
        nd: f64,
        half_deg: f64,
        n_cone: usize,
    ) -> f64 {
        let mut thick = f64::INFINITY;
        for dcone in cone_dirs(inward, half_deg, n_cone) {
            let dir = scale(dcone, r);
            let hits = self.collect_hits(c, dir, 1.0);
            let mut dmin = f64::INFINITY;
            for (tv, _f) in hits {
                let dd = tv * r;
                if dd > nd + NEAR_WALL_SKIP && dd < dmin {
                    dmin = dd;
                }
            }
            if dmin < thick {
                thick = dmin;
            }
        }
        thick
    }

    fn breach_one(
        &self,
        c: V3,
        r: f64,
        eps_mm: f64,
        half_deg: f64,
        n_cone: usize,
    ) -> (bool, f64, f64) {
        let (nd, _cp, tri, fn_) = {
            let (d, i, cp) = self.nearest_one(c, f64::INFINITY);
            let n = if i < self.mesh.global_nfaces() {
                self.mesh.normal(i)
            } else {
                [0.0; 3]
            };
            (d, cp, i, n)
        };
        let _ = tri;
        // reach precondition r−nd vs eps_mm threshold band (sec 5.4 item 3)
        self.band_check(r - nd, eps_mm);
        if r - nd <= eps_mm {
            return (false, f64::INFINITY, 0.0);
        }
        let inward = {
            let iv = scale(fn_, -1.0);
            let nrm = norm(iv);
            if nrm == 0.0 {
                iv
            } else {
                scale(iv, 1.0 / nrm)
            }
        };
        let t1 = self.cone_thickness(c, r, inward, nd, half_deg, n_cone);

        // measured-tie detection (Amendment A15): opposing-normal face at dist nd
        let mut t2 = f64::INFINITY;
        let mut tie = false;
        let tied = self.faces_at_distance(c, nd, 1e-12);
        if tied.len() > 1 {
            for &f in &tied {
                if dot(self.mesh.normal(f), fn_) < 0.0 {
                    let inw2 = {
                        let iv = scale(self.mesh.normal(f), -1.0);
                        let nrm = norm(iv);
                        if nrm == 0.0 {
                            iv
                        } else {
                            scale(iv, 1.0 / nrm)
                        }
                    };
                    t2 = self.cone_thickness(c, r, inw2, nd, half_deg, n_cone);
                    tie = true;
                    break;
                }
            }
        }

        // thickness-vs-(r−eps) threshold band, per breaching orientation (sec 5.4 item 3)
        self.band_check(t1, r - eps_mm);
        if tie {
            self.band_check(t2, r - eps_mm);
        }
        let breach1 = t1.is_finite() && t1 < r - eps_mm;
        let breach2 = tie && t2.is_finite() && t2 < r - eps_mm;
        if tie {
            self.tie_counter.fetch_add(1, Ordering::Relaxed);
        }
        let breached = breach1 || breach2;
        if !breached {
            // report primary thickness (matches non-tie oracle behavior)
            return (false, t1, 0.0);
        }
        // breaching orientation's thickness; smaller finite when both breach
        let thick = if breach1 && breach2 {
            t1.min(t2)
        } else if breach1 {
            t1
        } else {
            t2
        };
        (true, thick, r - thick)
    }

    pub fn sphere_breach(
        &self,
        centers: &[V3],
        radii: &[f64],
        eps_mm: f64,
        half_deg: f64,
        n_cone: usize,
    ) -> Result<SphereBreachBatch, BatchInputError> {
        let n = centers.len();
        BatchInputError::require_len("sphere_breach", "radii", n, radii.len())?;
        let res: Vec<(bool, f64, f64)> = par::run(|| {
            (0..n)
                .into_par_iter()
                .map(|i| self.breach_one(centers[i], radii[i], eps_mm, half_deg, n_cone))
                .collect()
        });
        let mut br = Vec::with_capacity(n);
        let mut th = Vec::with_capacity(n);
        let mut po = Vec::with_capacity(n);
        for (b, t, p) in res {
            br.push(b);
            th.push(t);
            po.push(p);
        }
        Ok((br, th, po))
    }

    // ---- exterior-face mask ----------------------------------------------
    /// Classifies the faces selected by this surface in selected-face order.
    ///
    /// Entry `i` corresponds one-to-one to `self.face_ids()[i]`. Those IDs are
    /// deterministic ascending global face IDs, including for subsets. The
    /// returned length is therefore [`Self::nfaces`], not the global immutable
    /// geometry's face count.
    pub fn exterior_face_mask(&self) -> &[bool] {
        self.exterior_mask.get_or_init(|| {
            let bary: [[f64; 3]; 3] = [
                [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0],
                [0.6, 0.2, 0.2],
                [0.2, 0.6, 0.2],
            ];
            let eps_ladder = [0.05f64, 0.15f64];
            par::run(|| {
                self.mesh
                    .face_ids()
                    .par_iter()
                    .map(|&face_id| {
                        let f = face_id as usize;
                        let a = self.mesh.v0(f);
                        let b = self.mesh.vertex_b(f);
                        let cc = self.mesh.vertex_c(f);
                        let fn_ = self.mesh.normal(f);
                        let mut votes = 0u32;
                        for w in &bary {
                            let base = add(add(scale(a, w[0]), scale(b, w[1])), scale(cc, w[2]));
                            for &eps in &eps_ladder {
                                let sp = add(base, scale(fn_, eps));
                                if self.inside_serial(sp) {
                                    votes += 1;
                                }
                            }
                        }
                        votes < 4 // exterior iff NOT interior(>=4)
                    })
                    .collect::<Vec<bool>>()
            })
        })
    }
}

// ---- ray-basis helpers (ported perp_basis / _cone_dirs) ------------------

/// geometry_exact.py:147-151 perp_basis
fn perp_basis(d: V3) -> (V3, V3) {
    let a: V3 = if d[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let mut e1 = cross(d, a);
    let n1 = norm(e1);
    let s1 = if n1 == 0.0 { 1.0 } else { n1 };
    e1 = scale(e1, 1.0 / s1);
    let mut e2 = cross(d, e1);
    let n2 = norm(e2);
    let s2 = if n2 == 0.0 { 1.0 } else { n2 };
    e2 = scale(e2, 1.0 / s2);
    (e1, e2)
}

/// geometry_exact.py:935-945 _cone_dirs
fn cone_dirs(axis: V3, half_deg: f64, n: usize) -> Vec<V3> {
    let na = norm(axis);
    let sa = if na == 0.0 { 1.0 } else { na };
    let axis = scale(axis, 1.0 / sa);
    let e1 = perp_basis(axis).0;
    let e2 = cross(axis, e1);
    let th = half_deg.to_radians();
    let mut dirs = vec![axis];
    for k in 0..n {
        let ph = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
        let d = add(
            scale(axis, th.cos()),
            scale(add(scale(e1, ph.cos()), scale(e2, ph.sin())), th.sin()),
        );
        let nd = norm(d);
        dirs.push(scale(d, 1.0 / nd));
    }
    dirs
}
