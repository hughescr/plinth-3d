//! NativeSurface: the query engine. All methods are pure functions of immutable
//! kernel state + their own inputs, parallelized index-wise (spec sec 4). No
//! cross-query reductions anywhere.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use rayon::prelude::*;

use crate::bvh::{Bvh, LeafTri, BALL_SLACK_MM};
use crate::closest::{closest_on_tri, closest_on_tri_edges, segment_segment_distance_min};
use crate::consts::{DEDUP_LOC_MM, DET_EPS, EPS, THRESHOLD_BAND_MM};
use crate::mesh::Mesh;
use crate::moller::{dedup_loc_into, ray_triangle_classified, RayTriangleClass};
use crate::par;
use crate::parity::{ParityBuildError, ParityVote};
use crate::telemetry::{charge_near_parallel, NearParallelSite};
use crate::vec3::{add, cross, dot, norm, scale, sub, V3};
use crate::{SurfaceIndexKind, SurfaceSubsetError, SurfaceTriangleSoupDigest};

/// Two radii closer than this are one radius: a row handed `r` twice (or two
/// values that differ by rounding) takes the constant-radius form, which is bit
/// for bit the form every such caller had before the frustum reading. Far below
/// the 1 um the emitted files can resolve.
pub const TAPER_EPS_MM: f64 = 1e-12;

/// `(sqrt(5) - 1) / 2`, the golden-section ratio.
const GOLDEN_INV: f64 = 0.618_033_988_749_894_9;

/// Golden-section steps of the frustum search. The bracket shrinks by
/// `GOLDEN_INV` each step, so 60 steps leave `0.618^60 = 3e-13` of the axis
/// length (under 1e-10 mm on a 40 mm member): numerically exact for a distance
/// the files hold to a micrometre.
const GOLDEN_STEPS: usize = 60;

thread_local! {
    static STACK: RefCell<Vec<i32>> = RefCell::new(Vec::with_capacity(128));
    /// The distance walk's stack: a node and its distance to the segment's box,
    /// so a node popped later is not measured a second time.
    static WALK_STACK: RefCell<Vec<(i32, f64)>> = RefCell::new(Vec::with_capacity(128));
    static ZSCRATCH: RefCell<Vec<f64>> = RefCell::new(Vec::with_capacity(64));
}

/// Segment `[a, b]` and the two numbers every face test of one walk shares.
#[derive(Clone, Copy)]
pub(crate) struct SegPre {
    a: V3,
    b: V3,
    /// `b - a`, the very vector [`seg_tri_dist_geom`] builds.
    ab: V3,
    len2: f64,
}

impl SegPre {
    #[inline]
    pub(crate) fn new(a: V3, b: V3) -> Self {
        let ab = sub(b, a);
        Self {
            a,
            b,
            ab,
            len2: dot(ab, ab),
        }
    }

    /// Whether the face's bounding ball proves its distance from the segment
    /// at least `bound`, so the face cannot take a strict minimum below `bound`
    /// (the walk's `d < best_d`) nor enter a band that ends at `bound`.
    ///
    /// Every candidate [`seg_tri_candidates`] weighs is the distance between a
    /// point on the segment and a point on the face (the endpoint terms pass
    /// through `closest_on_tri`, the edge terms through
    /// `segment_segment_distance_min`; both return points of their operands), so
    /// each is at least `dist(segment, centre) - radius`. The test therefore
    /// demands that distance exceed `bound` by the ball's slack: a fixed
    /// [`BALL_SLACK_MM`] for rounding of the distances themselves, plus the
    /// face's `rounding * D^2` for the interior branch of `closest_on_tri`
    /// (`D` the largest endpoint-to-vertex distance, bounded as
    /// `D^2 <= 2 (|end - centre|^2 + radius^2)`). A face that could tie `bound`
    /// is never skipped. NaN anywhere makes the comparison false, so a
    /// non-finite input is never skipped; a needle face carries an infinite
    /// `rounding` and is never skipped either.
    ///
    /// The pierce candidate (distance 0) is NOT covered here: the caller
    /// classifies the ray first and sends a `Hit` down the full path.
    #[inline]
    fn ball_clears(&self, tri: &LeafTri, bound: f64) -> bool {
        let w = sub(tri.centre, self.a);
        let s = if self.len2 > 0.0 {
            (dot(w, self.ab) / self.len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let foot = add(self.a, scale(self.ab, s));
        let off = sub(tri.centre, foot);
        let dist2 = dot(off, off);
        let wb = sub(tri.centre, self.b);
        let far2 = dot(w, w).max(dot(wb, wb));
        let slack =
            BALL_SLACK_MM + tri.rounding * 2.0 * (far2 + tri.radius * tri.radius);
        let reach = bound + tri.radius + slack;
        reach >= 0.0 && dist2 > reach * reach * (1.0 + 1e-12)
    }
}

/// The distance candidates of one segment against one face: the least of the
/// two endpoint distances and the three edge distances, each as the
/// `norm`/`dist` of two real points (see [`SegPre::ball_clears`]). The pierce
/// candidate is [`seg_tri_dist_geom`]'s.
#[inline]
fn seg_tri_candidates(a: V3, b: V3, v0: V3, e1: V3, e2: V3) -> (f64, V3) {
    let p = v0;
    let q = add(v0, e1);
    let r = add(v0, e2);
    // The reconstructed edges (Amendment A6), once for both endpoints.
    let ab = sub(q, p);
    let ac = sub(r, p);
    let mut best_d = f64::INFINITY;
    let mut best_pt: V3 = [0.0; 3];
    // (a) endpoints vs triangle
    let cpa = closest_on_tri_edges(a, p, q, r, ab, ac);
    let da = norm(sub(a, cpa));
    if da < best_d {
        best_d = da;
        best_pt = cpa;
    }
    let cpb = closest_on_tri_edges(b, p, q, r, ab, ac);
    let db = norm(sub(b, cpb));
    if db < best_d {
        best_d = db;
        best_pt = cpb;
    }
    // (b) segment vs each triangle edge — complete minimisation, no epsilon
    for &(x, y) in &[(p, q), (q, r), (r, p)] {
        let (d, _c1, c2) = segment_segment_distance_min(a, b, x, y);
        if d < best_d {
            best_d = d;
            best_pt = c2;
        }
    }
    (best_d, best_pt)
}

/// [`ray_triangle_classified`] of the segment's ray (`t_max` 1) against the
/// face, with the common answer reached early. A determinant above `DET_EPS` is
/// `Hit` or `Miss` and nothing else, and `Hit` needs every one of `u`, `v`,
/// `u + v` to pass; each is read from the same expression the full function
/// reads, so a failure is the same `Miss` it would return. Anything not
/// rejected here goes to the full function, so a `Hit`, and every non-valid
/// determinant, are its bits.
#[inline]
fn seg_ray_class(o: V3, dv: V3, v0: V3, e1: V3, e2: V3) -> RayTriangleClass {
    let pvec = cross(dv, e2);
    let det = dot(e1, pvec);
    if det.abs() > DET_EPS {
        let inv = 1.0 / det;
        let tvec = sub(o, v0);
        let u = dot(tvec, pvec) * inv;
        // Written as a conjunction that is false for NaN, like `hit` itself.
        let passes = u >= -EPS && {
            let qvec = cross(tvec, e1);
            let v = dot(dv, qvec) * inv;
            v >= -EPS && u + v <= 1.0 + EPS
        };
        if !passes {
            return RayTriangleClass::Miss;
        }
    }
    ray_triangle_classified(o, dv, v0, e1, e2, 1.0)
}

/// Segment `[a, b]` vs the face `(v0, e1, e2)`: see [`NativeSurface::seg_tri_dist`].
#[inline]
fn seg_tri_dist_geom(a: V3, b: V3, v0: V3, e1: V3, e2: V3) -> (f64, V3, bool) {
    let (mut best_d, mut best_pt) = seg_tri_candidates(a, b, v0, e1, e2);
    // (c) interior pierce -> distance 0 at crossing point
    let dv = sub(b, a);
    let mut near_parallel = false;
    match seg_ray_class(a, dv, v0, e1, e2) {
        RayTriangleClass::Hit(hit) => {
            let pierce = add(a, scale(dv, hit.t));
            if 0.0 < best_d {
                best_d = 0.0;
                best_pt = pierce;
            }
        }
        // Undecidable: the segment MIGHT pass through this face's interior,
        // in which case the true distance is 0 and everything above it is an
        // overestimate. Claiming 0 would be inventing a crossing; staying
        // silent is the defect being removed. So: keep the bound, flag it.
        RayTriangleClass::NearParallel => {
            near_parallel = true;
            charge_near_parallel(NearParallelSite::SegTriPierce, 1);
        }
        RayTriangleClass::ExactParallel | RayTriangleClass::Miss => {}
    }
    (best_d, best_pt, near_parallel)
}

/// [`seg_tri_dist_geom`] for a caller that only needs the answer when it is
/// below `bound` (a walk's least distance so far, or the edge of a band).
///
/// Returns the very triple `seg_tri_dist_geom` returns whenever that distance
/// is below `bound`; when the face's ball proves the distance is not, the
/// distance is `INFINITY` (a point of zeros) and the flag is still exact, so a
/// caller that reads the triple only through `d < bound` / `d <= bound` and
/// counts the flag for every face sees no difference. The pierce is decided
/// first and a `Hit` takes the full path, so the ball only ever replaces the
/// endpoint and edge candidates.
#[inline]
fn seg_tri_dist_ball(seg: &SegPre, tri: &LeafTri, bound: f64) -> (f64, V3, bool) {
    let class = seg_ray_class(seg.a, seg.ab, tri.v0, tri.e1, tri.e2);
    if matches!(class, RayTriangleClass::Hit(_)) {
        return seg_tri_dist_geom(seg.a, seg.b, tri.v0, tri.e1, tri.e2);
    }
    let near_parallel = matches!(class, RayTriangleClass::NearParallel);
    if near_parallel {
        charge_near_parallel(NearParallelSite::SegTriPierce, 1);
    }
    if seg.ball_clears(tri, bound) {
        return (f64::INFINITY, [0.0; 3], near_parallel);
    }
    let (d, pt) = seg_tri_candidates(seg.a, seg.b, tri.v0, tri.e1, tri.e2);
    (d, pt, near_parallel)
}

#[derive(Clone, Copy, Debug, PartialEq)]
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

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged \
              as a class, and the one architecture item the class carried (geom_kernel's 32-bit BVH \
              index width) is now a named, enforced limit. TODO(cast-width-audit): retire site by \
              site.  See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
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
        let (cos_th, sin_th) = turn_cos_sin(offset, self.offset_count);
        let unit = add(scale(self.e1, cos_th), scale(self.e2, sin_th));
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

/// Caller-owned hard limits for [`NativeSurface::faces_near_segments_batch`].
///
/// The per-row cap is what bounds peak memory: the batch cannot hold more than
/// `max_rows * max_faces_per_row * 4` bytes of face IDs, and both numbers are
/// the caller's own, so the ceiling is knowable before the call. The total cap
/// charges the REALIZED sum rather than that product, so an ordinary batch is
/// not refused for a worst case it never reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NearFaceBatchLimits {
    pub max_rows: usize,
    pub max_faces_per_row: usize,
    pub max_total_faces: usize,
}

/// Why [`NativeSurface::faces_near_segments_batch`] refused.
///
/// Every variant is a refusal BEFORE or DURING collection, never a truncation:
/// a partial band would silently shrink a seat exemption and turn a blind spot
/// back on, so the batch fails loudly instead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NearFaceBatchError {
    /// A companion slice did not match the batch length.
    BatchInput(BatchInputError),
    /// More rows than the caller's own row cap.
    RowLimitExceeded { required: usize, limit: usize },
    /// One row's band exceeded the per-row face cap mid-walk.
    RowFaceLimitExceeded { row: usize, limit: usize },
    /// The realized total across all rows exceeded the batch cap.
    TotalFaceLimitExceeded { required: usize, limit: usize },
    /// The realized total did not fit a `usize`.
    TotalFaceArithmeticOverflow,
    /// A band radius was negative or not finite.
    InvalidRadius { row: usize, within_mm: f64 },
    /// A segment endpoint carried a non-finite component.
    NonFinitePoint { row: usize, axis: usize },
}

impl From<BatchInputError> for NearFaceBatchError {
    fn from(error: BatchInputError) -> Self {
        Self::BatchInput(error)
    }
}

impl std::fmt::Display for NearFaceBatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BatchInput(error) => write!(formatter, "{error}"),
            Self::RowLimitExceeded { required, limit } => write!(
                formatter,
                "faces_near_segments_batch: {required} rows exceeds the row limit {limit}"
            ),
            Self::RowFaceLimitExceeded { row, limit } => write!(
                formatter,
                "faces_near_segments_batch: row {row} collected more than {limit} faces"
            ),
            Self::TotalFaceLimitExceeded { required, limit } => write!(
                formatter,
                "faces_near_segments_batch: {required} collected faces exceeds the batch limit {limit}"
            ),
            Self::TotalFaceArithmeticOverflow => write!(
                formatter,
                "faces_near_segments_batch: collected face total does not fit a usize"
            ),
            Self::InvalidRadius { row, within_mm } => write!(
                formatter,
                "faces_near_segments_batch: row {row} band radius {within_mm} is negative or not finite"
            ),
            Self::NonFinitePoint { row, axis } => write!(
                formatter,
                "faces_near_segments_batch: row {row} endpoint axis {axis} is not finite"
            ),
        }
    }
}

impl std::error::Error for NearFaceBatchError {}

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
    /// Möller evaluations on this row's rays classified
    /// [`RayTriangleClass::NearParallel`] — undecidable crossings, summed over
    /// the row's rays.
    ///
    /// **This is not a charge.** The census's `crossings` figure is unchanged by
    /// classification (identical hit predicate in both walks), so no
    /// census-driven budget or refusal moves. The tally rides along so a caller
    /// can compare it against the fill pass's tally for the same row — a
    /// comparison it must make EXPLICITLY, since `CensusDrift` covers hits,
    /// subsegments, samples and intervals and has never covered tallies.
    pub near_parallel: u32,
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
    near_parallel: u32,
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

/// One batched ray cast, ragged, plus the per-ray uncertainty tally.
///
/// Replaces the retired `CastRaysBatch` tuple alias. The first three fields are
/// exactly that alias's three arrays, unchanged in meaning and in element
/// order; the fourth is the channel it did not have.
#[derive(Clone, Debug, PartialEq)]
pub struct CastRaysClassified {
    /// Ragged row starts, length `rays + 1`; row `i` is
    /// `[offsets[i], offsets[i + 1])` in [`Self::t_all`] and [`Self::face_all`].
    pub offsets: Vec<i64>,
    /// Flattened, per-row-ascending crossing parameters.
    pub t_all: Vec<f64>,
    /// Flattened global face IDs, parallel to [`Self::t_all`].
    pub face_all: Vec<i64>,
    /// **Per RAY** (length `rays`, parallel to the caller's row order, NOT to
    /// the flattened arrays): how many faces this ray reached whose crossing was
    /// undecidable. Those faces contributed no entry to `t_all`/`face_all`, so a
    /// nonzero entry means that row's crossing list is incomplete in an unknown
    /// direction.
    pub near_parallel: Vec<u32>,
}

impl CastRaysClassified {
    /// Did any ray in this batch reach an undecidable face?
    #[must_use]
    pub fn any_near_parallel(&self) -> bool {
        self.near_parallel.iter().any(|&n| n > 0)
    }

    /// Every ray's tally summed. A batch total, for a run report; a per-row
    /// DECISION must read its own row.
    #[must_use]
    pub fn total_near_parallel(&self) -> u64 {
        self.near_parallel.iter().map(|&n| u64::from(n)).sum()
    }
}

/// The counted sibling of [`CastRaysClassified`]: crossings per ray, plus the
/// same per-ray uncertainty tally. Replaces the retired `Vec<usize>` return of
/// `cast_rays_census`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CastRaysCensusClassified {
    /// Crossings the collecting walk would push for each ray, before dedup.
    pub crossings: Vec<usize>,
    /// Per-ray undecidable-face tally, equal by construction to the collecting
    /// walk's for the same ray.
    pub near_parallel: Vec<u32>,
}

impl CastRaysCensusClassified {
    /// Did any ray in this batch reach an undecidable face?
    #[must_use]
    pub fn any_near_parallel(&self) -> bool {
        self.near_parallel.iter().any(|&n| n > 0)
    }

    /// Every ray's tally summed.
    #[must_use]
    pub fn total_near_parallel(&self) -> u64 {
        self.near_parallel.iter().map(|&n| u64::from(n)).sum()
    }
}

/// One batched sphere-breach result, plus the per-sphere uncertainty tally.
///
/// Replaces the retired `SphereBreachBatch` tuple alias; the first three fields
/// are that alias's three arrays unchanged.
#[derive(Clone, Debug, PartialEq)]
pub struct SphereBreachClassified {
    /// Whether each sphere breaches the surface.
    pub breached: Vec<bool>,
    /// Breaching-orientation wall thickness in mm, or `INFINITY`.
    pub thickness_mm: Vec<f64>,
    /// Protrusion in mm (`r - thickness`), or `0.0` when not breaching.
    pub protrusion_mm: Vec<f64>,
    /// **Per SPHERE**: the tally summed over every cone ray this sphere cast.
    /// Any cone ray reaching an undecidable face marks the sphere's row —
    /// `cone_thickness` takes a MINIMUM over rays, so one skipped crossing can
    /// only make the reported thickness too LARGE, and too large is the
    /// permissive direction for a breach test.
    pub near_parallel: Vec<u32>,
}

impl SphereBreachClassified {
    /// Did any sphere in this batch reach an undecidable face?
    #[must_use]
    pub fn any_near_parallel(&self) -> bool {
        self.near_parallel.iter().any(|&n| n > 0)
    }

    /// Every sphere's tally summed.
    #[must_use]
    pub fn total_near_parallel(&self) -> u64 {
        self.near_parallel.iter().map(|&n| u64::from(n)).sum()
    }
}

/// One batched penetration-interval extraction, plus the per-row uncertainty
/// tally. Replaces the retired `Vec<Vec<Interval>>` return.
#[derive(Clone, Debug, PartialEq)]
pub struct PenetrationIntervalsClassified {
    /// Intervals per row, in row order — the retired return value, unchanged.
    pub rows: Vec<Vec<Interval>>,
    /// **Per ROW**: the tally summed over every ray of that row. A skipped
    /// crossing UNDERSTATES the interval set, so an affected row's illegal
    /// overlap is a LOWER BOUND — the true overlap can only be equal or higher,
    /// and a caller that condemns on a large overlap must not read an
    /// understated one as a pass.
    pub near_parallel: Vec<u32>,
}

impl PenetrationIntervalsClassified {
    /// Did any row in this batch reach an undecidable face?
    #[must_use]
    pub fn any_near_parallel(&self) -> bool {
        self.near_parallel.iter().any(|&n| n > 0)
    }

    /// Every row's tally summed.
    #[must_use]
    pub fn total_near_parallel(&self) -> u64 {
        self.near_parallel.iter().map(|&n| u64::from(n)).sum()
    }
}

/// Every global face ID near each segment, plus the per-row uncertainty tally.
/// Replaces the retired `Vec<Vec<u32>>` return of `faces_near_segments_batch`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FacesNearSegmentsClassified {
    /// Per row, sorted ascending and deduplicated global face IDs — the retired
    /// return value, unchanged. **Membership is proof-only:** a face admitted
    /// here was MEASURED within the band.
    pub rows: Vec<Vec<u32>>,
    /// **Per ROW**: faces whose pierce candidate was undecidable and which were
    /// therefore NOT admitted to the band unless a measured candidate put them
    /// there anyway.
    ///
    /// These bands are EXEMPTIONS — a face in a band is excluded from later
    /// clearance walks — and an exemption may grow only on proof
    /// (`docs/specs/geometry-adjudication.md` §EXT-2 justification 12). Omitting
    /// the face keeps it in later clearance tests, where a genuine touch fails
    /// the row: the refusal direction. The tally makes the omission visible
    /// instead of silent.
    pub near_parallel: Vec<u32>,
}

impl FacesNearSegmentsClassified {
    /// Did any row in this batch reach an undecidable face?
    #[must_use]
    pub fn any_near_parallel(&self) -> bool {
        self.near_parallel.iter().any(|&n| n > 0)
    }

    /// Every row's tally summed.
    #[must_use]
    pub fn total_near_parallel(&self) -> u64 {
        self.near_parallel.iter().map(|&n| u64::from(n)).sum()
    }
}

/// Why a clearance row carries NO measurement.
///
/// Both reasons are properties of the row's own geometry, decided before any
/// surface is touched. Neither is an error: a caller can legitimately hand this
/// batch a row it has already masked out of existence. What a caller must NOT
/// do is read the absence of a measurement as a good measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnmeasuredSegment {
    /// `p0` and `p1` are the same point to within the kernel's axis-degeneracy
    /// guard (`|p1 - p0| < 1e-9`). There is no axis to sweep, so no length of
    /// it was tested.
    DegenerateAxis,
    /// `inset0` and `inset1` together consume the whole axis
    /// (`length - inset1 - inset0 <= 1e-9`). The masks left nothing to test.
    ///
    /// This is the erasure case that matters in practice: a mask wide enough to
    /// swallow the member makes the row pass by having nothing to fail on.
    InsetsConsumeAxis,
    /// The row's exclusion set removed EVERY face the distance walk could
    /// reach, so the walk had no candidate to measure against.
    ///
    /// This one does not come from the row's endpoints; it comes from the
    /// exclusion set, and it is reported by the walk rather than predicted
    /// before it. It is the third way to get no measurement and by far the
    /// easiest to mistake for a good one: the walk's "no candidate found"
    /// marker IS `f64::INFINITY`, so the demanded-gap subtraction
    /// `INFINITY - thresh` is also `INFINITY`, a perfectly finite-LOOKING
    /// arithmetic result that satisfies every `>= threshold` test ever written
    /// against it. Erasure by exemption is exactly the failure the per-row face
    /// exclusion was introduced to avoid, so the predicate must not be able to
    /// launder it into a measurement.
    AllFacesExcluded,
}

impl std::fmt::Display for UnmeasuredSegment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DegenerateAxis => {
                write!(formatter, "segment endpoints coincide; no axis to test")
            }
            Self::InsetsConsumeAxis => write!(
                formatter,
                "insets consume the whole axis; no length left to test"
            ),
            Self::AllFacesExcluded => write!(
                formatter,
                "the row's exclusion set removed every face; nothing to measure against"
            ),
        }
    }
}

/// One row's answer from the clearance predicate — measured, one-sided, or
/// not measured at all.
///
/// **This type exists because the three answers below are not the same answer,
/// and a bare `f64` cannot tell them apart.** The original float form encoded
/// [`Self::Unmeasured`] as `f64::INFINITY` -- in band, orderable, and greater
/// than every threshold a caller could compare it against -- so
/// `clearance >= 0.0` silently reported "we did not look" as "enormous
/// clearance, definitely fine". `INFINITY` is exactly the wrong sentinel:
/// the safe reading of "not measured" is the STRICTEST verdict, and the value
/// chosen was the most permissive one representable. Those float shims
/// (`segment_clearance`, `segment_clearance_excluding`) are now REMOVED, not
/// merely discouraged: they could not carry the third state either.
///
/// The enum removes the choice. A caller cannot compare a row against a
/// threshold without first saying, in code, what an unmeasured row and what a
/// one-sided row each mean to it -- see [`Self::clears`], which takes both
/// fallbacks and defaults neither.
/// # The third state
///
/// The two-state `SegmentClearance` this replaces could say "measured" and "not
/// measured" and nothing else, so a walk that DID measure but had to skip an
/// undecidable pierce candidate reported its number as a `Measured` — a
/// one-sided answer wearing a two-sided type. The walk's error is one-sided by
/// construction: endpoints are exact via `closest_on_tri`, edges are the
/// complete minimisation, and a SKIPPED pierce turns a true `0` into something
/// positive. So the number is an upper bound and the true clearance may be
/// lower, including below zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SegmentClearanceClassified {
    /// The axis was swept against the surface and every candidate was decided.
    /// `clearance_mm` is `nearest_surface_distance - (r_max + standoff)`
    /// (`r_max` the wider of the row's two radii) from the capsule entry points,
    /// and `min over the axis of (distance to the surface - radius) - standoff`,
    /// the radius linear between the row's end radii, from
    /// [`NativeSurface::segment_frustum_clearance_classified`]; the two agree on
    /// a constant radius. Negative means the demanded gap is violated,
    /// non-negative means it is met. `witness` is the surface point that produced the nearest distance.
    Measured { clearance_mm: f64, witness: V3 },
    /// The axis WAS swept, but at least one face's pierce candidate was
    /// undecidable ([`RayTriangleClass::NearParallel`]), so the true clearance
    /// is **AT MOST** `clearance_mm` and may be lower, including below zero.
    ///
    /// This is deliberately neither [`Self::Measured`] (it cannot affirm a gap)
    /// nor [`Self::Unmeasured`] (a number does exist; it is one-sided).
    UpperBound {
        /// The one-sided bound: the true clearance is `<=` this.
        clearance_mm: f64,
        /// The surface point that produced the bounding distance.
        witness: V3,
        /// How many faces on this row had an undecidable pierce candidate.
        near_parallel_faces: u32,
    },
    /// No part of the axis was tested. There is no number, and inventing one is
    /// the defect this type exists to prevent.
    Unmeasured(UnmeasuredSegment),
}

impl SegmentClearanceClassified {
    /// The measured gap in mm, or `None` for every other state.
    ///
    /// An [`Self::UpperBound`] row deliberately answers `None` here: its number
    /// is not a measured gap, and a caller that wants the bound asks for it by
    /// name via [`Self::upper_bound_mm`].
    #[must_use]
    pub const fn clearance_mm(self) -> Option<f64> {
        match self {
            Self::Measured { clearance_mm, .. } => Some(clearance_mm),
            Self::UpperBound { .. } | Self::Unmeasured(_) => None,
        }
    }

    /// The one-sided bound in mm, or `None` when the row is not an
    /// [`Self::UpperBound`].
    #[must_use]
    pub const fn upper_bound_mm(self) -> Option<f64> {
        match self {
            Self::UpperBound { clearance_mm, .. } => Some(clearance_mm),
            Self::Measured { .. } | Self::Unmeasured(_) => None,
        }
    }

    /// The nearest surface point, or `None` when nothing was measured. An
    /// [`Self::UpperBound`] row has one: the point that produced the bound.
    #[must_use]
    pub const fn witness(self) -> Option<V3> {
        match self {
            Self::Measured { witness, .. } | Self::UpperBound { witness, .. } => Some(witness),
            Self::Unmeasured(_) => None,
        }
    }

    /// Why the row was not measured, or `None` when it was.
    #[must_use]
    pub const fn unmeasured(self) -> Option<UnmeasuredSegment> {
        match self {
            Self::Measured { .. } | Self::UpperBound { .. } => None,
            Self::Unmeasured(reason) => Some(reason),
        }
    }

    /// How many faces on this row had an undecidable pierce candidate.
    #[must_use]
    pub const fn near_parallel_faces(self) -> u32 {
        match self {
            Self::UpperBound {
                near_parallel_faces,
                ..
            } => near_parallel_faces,
            Self::Measured { .. } | Self::Unmeasured(_) => 0,
        }
    }

    /// Does this row meet its demanded gap?
    ///
    /// **BOTH fallbacks are stated explicitly and NEITHER has a default.** The
    /// caller must say, at the call site where the consequence is visible,
    /// which way an untested row falls and which way a one-sided row falls.
    ///
    /// * `when_unmeasured` — a row that was never tested. A stage that REFUSES
    ///   passes `false`: an untested row has earned nothing. A stage that
    ///   merely PRUNES a search may pass `true`, because keeping an untested
    ///   candidate alive costs a later test rather than shipping an
    ///   unsupported claim.
    /// * `when_upper_bound` — a row whose number is one-sided. A refusing stage
    ///   passes `false`: the bound cannot affirm a gap, and the true clearance
    ///   may be negative. A pruning stage may pass `true` **only if a later
    ///   REFUSING stage retests the survivor**, and must name that stage in a
    ///   comment at the call site.
    #[must_use]
    pub fn clears(self, when_unmeasured: bool, when_upper_bound: bool) -> bool {
        match self {
            Self::Measured { clearance_mm, .. } => clearance_mm >= 0.0,
            Self::UpperBound { .. } => when_upper_bound,
            Self::Unmeasured(_) => when_unmeasured,
        }
    }
}

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
        // The content digest hashes every face (about a fifth of a build) and
        // reads nothing the two indexes write, so it runs beside them. The BVH
        // and the parity grid stay one after the other: the memory estimate
        // bills their scratch as the larger of the two, not the sum.
        let (source_triangle_soup_digest, indexes) = par::run(|| {
            rayon::join(
                || mesh.triangle_soup_digest(),
                || -> Result<(Bvh, ParityVote), ParityBuildError> {
                    let bvh = Bvh::build(&mesh).map_err(ParityBuildError::Other)?;
                    let pv = ParityVote::build(&mesh, grid_pitch_mm, max_grid_bytes)?;
                    Ok((bvh, pv))
                },
            )
        });
        let (bvh, pv) = indexes?;
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

    #[allow(
        clippy::float_cmp,
        reason = "the equal-distance/lowest-index tie-break IS the determinism \
                  contract (see the strict-pruning comment below); an epsilon here \
                  would make the answer depend on traversal order"
    )]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged \
              as a class, and the one architecture item the class carried (geom_kernel's 32-bit BVH \
              index width) is now a named, enforced limit. TODO(cast-width-audit): retire site by \
              site.  See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
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
    #[expect(
        clippy::cast_possible_wrap,
        reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged \
              as a class, and the one architecture item the class carried (geom_kernel's 32-bit BVH \
              index width) is now a named, enforced limit. TODO(cast-width-audit): retire site by \
              site.  See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
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
    #[expect(
        clippy::cast_possible_wrap,
        reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged \
              as a class, and the one architecture item the class carried (geom_kernel's 32-bit BVH \
              index width) is now a named, enforced limit. TODO(cast-width-audit): retire site by \
              site.  See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
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

    #[allow(
        clippy::float_cmp,
        reason = "the equal-distance/lowest-index tie-break IS the determinism \
                  contract; an epsilon here would make the answer depend on \
                  traversal order"
    )]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged \
              as a class, and the one architecture item the class carried (geom_kernel's 32-bit BVH \
              index width) is now a named, enforced limit. TODO(cast-width-audit): retire site by \
              site.  See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
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
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged \
              as a class, and the one architecture item the class carried (geom_kernel's 32-bit BVH \
              index width) is now a named, enforced limit. TODO(cast-width-audit): retire site by \
              site.  See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
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
    /// filter). Returns unsorted `(t, face)` pairs plus the ray's
    /// `NearParallel` tally.
    ///
    /// The tally is the number of faces this ray reached whose crossing was
    /// UNDECIDABLE ([`RayTriangleClass::NearParallel`]). Those faces are absent
    /// from the returned pairs — the arithmetic cannot say whether they should
    /// be there — so a nonzero tally means the pair list is INCOMPLETE in an
    /// unknown direction, and a caller that decides anything must treat it as
    /// such.
    pub(crate) fn collect_hits(&self, o: V3, d: V3, t_max: f64) -> (Vec<(f64, u32)>, u32) {
        let mut out: Vec<(f64, u32)> = Vec::new();
        let near_parallel = self.collect_hits_into(o, d, t_max, &mut out);
        (out, near_parallel)
    }

    /// Shared allocation-free traversal for ordinary ray casting and the
    /// bounded penetration path. Callers which need a hard allocation ceiling
    /// reserve `out` before entering this routine. Returns the ray's
    /// `NearParallel` tally.
    fn collect_hits_into(&self, o: V3, d: V3, t_max: f64, out: &mut Vec<(f64, u32)>) -> u32 {
        STACK.with(|s| {
            let (_tests, near_parallel) =
                self.collect_hits_with_stack(o, d, t_max, out, &mut s.borrow_mut());
            near_parallel
        })
    }

    /// Returns `(tests, near_parallel)`.
    ///
    /// `tests` is the EXECUTED traversal effort of this walk — one per BVH node
    /// popped plus one per Möller evaluation — in exactly the units
    /// [`Self::count_hits_with_stack`] counts, so the census pass's `tests`
    /// and this walk's return value are the same quantity measured by the
    /// two walks. The metered penetration path
    /// ([`Self::penetration_intervals_batch_bounded_metered_classified`],
    /// FU-8g H2) compares them; every other caller may ignore it.
    ///
    /// `near_parallel` is the number of faces this ray reached whose Möller
    /// evaluation was classified [`RayTriangleClass::NearParallel`]: nonzero but
    /// in-band determinant, so whether the ray crosses that face is undecidable
    /// at this scale. **Those faces are not in `out`.** The accepted-hit set is
    /// bit-identical to the pre-classification walk's — same arithmetic, same
    /// acceptance predicate — so any row whose tally is zero is byte-identical
    /// to the old behaviour, and any row whose tally is nonzero is a row where
    /// the old behaviour was silently guessing.
    ///
    /// The tally is also charged to the process-global counter for
    /// [`NearParallelSite::CollectingWalk`], once per ray rather than once per
    /// face, so the atomic stays off the innermost loop.
    #[expect(
        clippy::cast_sign_loss,
        reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged \
              as a class, and the one architecture item the class carried (geom_kernel's 32-bit BVH \
              index width) is now a named, enforced limit. TODO(cast-width-audit): retire site by \
              site.  See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn collect_hits_with_stack(
        &self,
        o: V3,
        d: V3,
        t_max: f64,
        out: &mut Vec<(f64, u32)>,
        stack: &mut Vec<i32>,
    ) -> (usize, u32) {
        out.clear();
        let t0 = -EPS;
        let t1 = t_max + EPS;
        stack.clear();
        stack.push(0);
        let mut tests = 0usize;
        let mut near_parallel = 0u32;
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
                    match ray_triangle_classified(
                        o,
                        d,
                        self.mesh.v0(t),
                        self.mesh.e1(t),
                        self.mesh.e2(t),
                        t_max,
                    ) {
                        RayTriangleClass::Hit(hit) => out.push((hit.t, tid)),
                        // A nonzero-but-in-band determinant: this face MIGHT be
                        // crossed and the arithmetic cannot say. It is not
                        // pushed (a manufactured `t` through a near-singular
                        // `1/det` would be noise), and the ray's tally carries
                        // the uncertainty out to the caller.
                        RayTriangleClass::NearParallel => near_parallel += 1,
                        // `det == 0.0` is a genuine prune (edge crossings live
                        // on adjacent faces), and a definite miss is a definite
                        // miss.
                        RayTriangleClass::ExactParallel | RayTriangleClass::Miss => {}
                    }
                }
            } else {
                stack.push(node.a);
                stack.push(node.b);
            }
        }
        charge_near_parallel(NearParallelSite::CollectingWalk, u64::from(near_parallel));
        (tests, near_parallel)
    }

    /// Counting twin of [`Self::collect_hits_with_stack`]: the SAME traversal
    /// over the same immutable BVH with the same predicates, but it counts
    /// instead of pushing, so it needs the `O(depth)` stack and nothing else.
    ///
    /// Returns `(crossings, tests, near_parallel)` where `crossings` is exactly
    /// the number of `(t, tid)` pairs the collecting walk would push, `tests` is
    /// the executed traversal effort (one per BVH node popped, plus one per
    /// Möller evaluation in a reached leaf), and `near_parallel` is exactly the
    /// tally the collecting walk would report for the same ray.
    ///
    /// **The tally is part of the mirror.** Both walks run the identical
    /// traversal, the identical acceptance predicate and the identical
    /// classification, so on any ray the two walks agree on all three numbers.
    /// Any edit to one walk must be mirrored in the other; the fill pass refuses
    /// with [`PenetrationBatchError::CensusDrift`] if the counted and executed
    /// figures disagree.
    ///
    /// **Standing correction, from §EXT-2 of the adjudication:** there is NO
    /// existing drift enforcement between the census tally and the fill tally.
    /// `CensusDrift` covers hits, subsegments, samples and intervals — not the
    /// near-parallel counts. Anything that wants those two tallies compared must
    /// compare them EXPLICITLY rather than assume a check already exists;
    /// `tests/walk_classified.rs` is where this kernel does that comparison.
    ///
    /// The tally is charged to the process-global counter for
    /// [`NearParallelSite::CountingWalk`] — a SEPARATE counter from the
    /// collecting walk's, so a run's two totals are independently readable
    /// rather than summed into one number that hides which walk fired.
    #[expect(
        clippy::cast_sign_loss,
        reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged \
              as a class, and the one architecture item the class carried (geom_kernel's 32-bit BVH \
              index width) is now a named, enforced limit. TODO(cast-width-audit): retire site by \
              site.  See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn count_hits_with_stack(
        &self,
        o: V3,
        d: V3,
        t_max: f64,
        stack: &mut Vec<i32>,
    ) -> (usize, usize, u32) {
        let t0 = -EPS;
        let t1 = t_max + EPS;
        stack.clear();
        stack.push(0);
        let mut crossings = 0usize;
        let mut tests = 0usize;
        let mut near_parallel = 0u32;
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
                    match ray_triangle_classified(
                        o,
                        d,
                        self.mesh.v0(t),
                        self.mesh.e1(t),
                        self.mesh.e2(t),
                        t_max,
                    ) {
                        RayTriangleClass::Hit(_) => crossings += 1,
                        RayTriangleClass::NearParallel => near_parallel += 1,
                        RayTriangleClass::ExactParallel | RayTriangleClass::Miss => {}
                    }
                }
            } else {
                stack.push(node.a);
                stack.push(node.b);
            }
        }
        charge_near_parallel(NearParallelSite::CountingWalk, u64::from(near_parallel));
        (crossings, tests, near_parallel)
    }

    /// cast_rays for one ray: deduped by arc location; representative t = first
    /// in group, representative face = LOWEST index in group (Amendment A13).
    ///
    /// Third element is the ray's `NearParallel` tally. Dedup cannot change it:
    /// an undecidable face produced no crossing to merge.
    fn cast_one(&self, o: V3, d: V3, t_max: f64) -> (Vec<f64>, Vec<i64>, u32) {
        let seg_len = norm(d) * t_max;
        let (mut hits, near_parallel) = self.collect_hits(o, d, t_max);
        if hits.is_empty() {
            return (Vec::new(), Vec::new(), near_parallel);
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
        (ts, fs, near_parallel)
    }

    /// Allocation-free crossing census for [`Self::cast_rays_classified`]: the
    /// same BVH walk that call's collecting pass performs, counting instead of
    /// pushing, so it needs the `O(depth)` traversal stack per worker and
    /// nothing else.
    ///
    /// `crossings[i]` is exactly the number of `(t, face)` pairs
    /// [`Self::cast_rays_classified`] collects for ray `i` BEFORE arc-location
    /// dedup. Dedup only merges neighbours, so the same figure also bounds that
    /// ray's retained `ts`/`fs` pair and its contribution to the two flattened
    /// arrays.
    ///
    /// `near_parallel[i]` is that ray's undecidable-face tally, equal by
    /// construction to the collecting walk's for the same ray. **Nothing in
    /// this kernel enforces that equality on your behalf** — `CensusDrift`
    /// covers hits, subsegments, samples and intervals, not tallies — so a
    /// consumer comparing the two must compare them explicitly.
    ///
    /// This is the COUNT of count-before-charge-before-allocate for ray
    /// casting, the same discipline
    /// [`Self::penetration_census_batch_classified`] gives the penetration
    /// path. The conservative every-ray-crosses-every-face shape is a
    /// mesh-density ceiling, not a charge: a ray through a closed hollow shell
    /// crosses the surface a handful of times however finely that surface is
    /// tessellated.
    pub fn cast_rays_census_classified(
        &self,
        origins: &[V3],
        dirs: &[V3],
        t_max: f64,
    ) -> Result<CastRaysCensusClassified, BatchInputError> {
        let n = origins.len();
        BatchInputError::require_len("cast_rays_census", "dirs", n, dirs.len())?;
        let mut per = vec![(0usize, 0u32); n];
        par::run(|| {
            (0..n)
                .into_par_iter()
                .map(|i| {
                    let (crossings, _tests, near_parallel) = STACK.with(|s| {
                        self.count_hits_with_stack(origins[i], dirs[i], t_max, &mut s.borrow_mut())
                    });
                    (crossings, near_parallel)
                })
                .collect_into_vec(&mut per);
        });
        let mut crossings = Vec::with_capacity(n);
        let mut near_parallel = Vec::with_capacity(n);
        for (c, np) in per {
            crossings.push(c);
            near_parallel.push(np);
        }
        Ok(CastRaysCensusClassified {
            crossings,
            near_parallel,
        })
    }

    /// Batched ray cast, ragged, **with the per-ray uncertainty tally**.
    ///
    /// Replaces the retired `cast_rays`, whose `(offsets, t_all, face_all)`
    /// triple had no channel for "this ray reached a face whose crossing the
    /// arithmetic could not decide" — so it reported a silent miss and the
    /// caller could not tell that from a real absence.
    ///
    /// The crossing payload is bit-identical to the retired call's on every ray
    /// whose `near_parallel` is zero: same traversal, same acceptance
    /// predicate, same dedup. A ray whose tally is NONZERO is one where the old
    /// answer was a guess, and the caller must route it to its own conservative
    /// side per the propagation contract
    /// (`docs/specs/geometry-adjudication.md` §EXT-2).
    #[expect(
        clippy::cast_possible_wrap,
        reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged \
              as a class, and the one architecture item the class carried (geom_kernel's 32-bit BVH \
              index width) is now a named, enforced limit. TODO(cast-width-audit): retire site by \
              site.  See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    pub fn cast_rays_classified(
        &self,
        origins: &[V3],
        dirs: &[V3],
        t_max: f64,
    ) -> Result<CastRaysClassified, BatchInputError> {
        let n = origins.len();
        BatchInputError::require_len("cast_rays", "dirs", n, dirs.len())?;
        let per: Vec<(Vec<f64>, Vec<i64>, u32)> = par::run(|| {
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
        let mut near_parallel = Vec::with_capacity(n);
        for (ts, fs, np) in &per {
            t_all.extend_from_slice(ts);
            f_all.extend_from_slice(fs);
            offsets.push(t_all.len() as i64);
            near_parallel.push(*np);
        }
        Ok(CastRaysClassified {
            offsets,
            t_all,
            face_all: f_all,
            near_parallel,
        })
    }

    /// Batched "is this ray blocked" test, exactly the verdict a caller derives
    /// from [`Self::cast_rays_classified`] and its own face-identity rule, with
    /// no per-ray allocation and an early exit.
    ///
    /// Ray `i` is BLOCKED when either
    ///
    /// * its undecidable-face tally is nonzero, or
    /// * after the cast's arc-location dedup, some retained crossing has a
    ///   finite `t > 0` and its (group-minimum) face's global id (the id the
    ///   cast reports in `face_all`) differs from `self_faces[i]`.
    ///
    /// That is what a visibility caller computes from the ragged cast: it
    /// skips a ray with a tally, skips crossings at `t <= 0` and crossings of
    /// the ray's own face, and treats any other crossing as an occluder. Here
    /// the walk stops at the first crossing that CANNOT be merged into a group
    /// containing the origin's own face, because that crossing's group is
    /// retained, is past `t = 0`, and is not the own face; whatever else the
    /// walk would have found (more crossings, a tally) leaves the ray blocked
    /// either way. The test never assumes where the own face crosses: the
    /// method computes the own face's actual crossing `t_s` with the arithmetic
    /// the walk uses. A crossing ends the walk only when it is past twice the
    /// dedup window from `t = 0` (so its group's retained `t` is positive) and,
    /// when the own face does cross, more than twice the window from `t_s` (so
    /// the own face is not in its group and cannot make the group's minimum
    /// face the own face). Any other ray takes the full collect, sort and group
    /// pass, which is the cast's own. The verdict equals the ragged cast's for
    /// every input, not only for rays that start on their own face.
    ///
    /// # Errors
    ///
    /// [`BatchInputError`] when the three slices differ in length.
    pub fn rays_blocked_classified(
        &self,
        origins: &[V3],
        dirs: &[V3],
        t_max: f64,
        self_faces: &[u32],
    ) -> Result<Vec<bool>, BatchInputError> {
        let n = origins.len();
        BatchInputError::require_len("rays_blocked", "dirs", n, dirs.len())?;
        BatchInputError::require_len("rays_blocked", "self_faces", n, self_faces.len())?;
        let mut blocked = vec![false; n];
        par::run(|| {
            (0..n)
                .into_par_iter()
                .map(|i| self.ray_blocked_one(origins[i], dirs[i], t_max, self_faces[i]))
                .collect_into_vec(&mut blocked);
        });
        Ok(blocked)
    }

    #[expect(
        clippy::cast_sign_loss,
        reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged \
              as a class, and the one architecture item the class carried (geom_kernel's 32-bit BVH \
              index width) is now a named, enforced limit. TODO(cast-width-audit): retire site by \
              site.  See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn ray_blocked_one(&self, o: V3, d: V3, t_max: f64, self_face: u32) -> bool {
        thread_local! {
            static HITS: RefCell<Vec<(f64, u32)>> = RefCell::new(Vec::with_capacity(64));
        }
        let dir_len = norm(d);
        let seg_len = dir_len * t_max;
        let dt = DEDUP_LOC_MM / seg_len.max(1e-9);
        // See the method comment. `sep` is twice the dedup window, so a crossing
        // more than `sep` from another is certainly in a different group (the
        // window test is on rounded differences; the factor two covers that),
        // and a crossing past `sep` certainly starts, or sits in, a group whose
        // retained `t` is positive. A window that is not finite disables the
        // early exit: every such ray takes the exact pass.
        let sep = 2.0 * dt;
        let early_exit_ok = sep.is_finite();
        // The own face's actual crossing, from the same arithmetic the walk uses.
        // An own face that crosses nowhere (miss, parallel, an id past the mesh)
        // can never share a group. A subset that lacks the own face only makes
        // this conservative: such a ray sends crossings near `self_t` down the
        // exact pass instead of ending early.
        let self_t = if (self_face as usize) < self.mesh.global_nfaces() {
            let s = self_face as usize;
            match ray_triangle_classified(
                o,
                d,
                self.mesh.v0(s),
                self.mesh.e1(s),
                self.mesh.e2(s),
                t_max,
            ) {
                RayTriangleClass::Hit(hit) => Some(hit.t),
                _ => None,
            }
        } else {
            None
        };
        STACK.with(|stack| {
            HITS.with(|hits| {
                let stack = &mut *stack.borrow_mut();
                let hits = &mut *hits.borrow_mut();
                hits.clear();
                stack.clear();
                stack.push(0);
                let t0 = -EPS;
                let t1 = t_max + EPS;
                let mut near_parallel = 0u32;
                while let Some(ni) = stack.pop() {
                    let node = &self.bvh.nodes[ni as usize];
                    if !ray_box(o, d, node.bmin, node.bmax, node.pad, t0, t1) {
                        continue;
                    }
                    if node.leaf {
                        let start = node.a as usize;
                        let cnt = node.b as usize;
                        for &tid in &self.bvh.order[start..start + cnt] {
                            let t = tid as usize;
                            match ray_triangle_classified(
                                o,
                                d,
                                self.mesh.v0(t),
                                self.mesh.e1(t),
                                self.mesh.e2(t),
                                t_max,
                            ) {
                                RayTriangleClass::Hit(hit) => {
                                    if early_exit_ok
                                        && tid != self_face
                                        && hit.t.is_finite()
                                        && hit.t > sep
                                        && self_t.is_none_or(|s| s.is_finite() && (hit.t - s).abs() > sep)
                                    {
                                        charge_near_parallel(
                                            NearParallelSite::CollectingWalk,
                                            u64::from(near_parallel),
                                        );
                                        return true;
                                    }
                                    hits.push((hit.t, tid));
                                }
                                RayTriangleClass::NearParallel => near_parallel += 1,
                                RayTriangleClass::ExactParallel | RayTriangleClass::Miss => {}
                            }
                        }
                    } else {
                        stack.push(node.a);
                        stack.push(node.b);
                    }
                }
                charge_near_parallel(NearParallelSite::CollectingWalk, u64::from(near_parallel));
                if near_parallel > 0 {
                    return true;
                }
                if hits.is_empty() {
                    return false;
                }
                // The cast's own dedup, streamed: groups of crossings within
                // `dt` of the group's first, keeping the first `t` and the
                // smallest face; blocked when a retained group is an occluder.
                hits.sort_unstable_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                let occludes = |t: f64, face: u32| t.is_finite() && t > 0.0 && face != self_face;
                let mut group_kept = hits[0].0;
                let mut group_min_face = hits[0].1;
                for &(t, f) in hits.iter().skip(1) {
                    if t - group_kept > dt {
                        if occludes(group_kept, group_min_face) {
                            return true;
                        }
                        group_kept = t;
                        group_min_face = f;
                    } else if f < group_min_face {
                        group_min_face = f;
                    }
                }
                occludes(group_kept, group_min_face)
            })
        })
    }

    // ---- segment / capsule minimum distance ------------------------------
    /// Segment `[a, b]` vs one triangle: min over endpoints, 3 edges, pierce.
    /// Returns `(distance_mm, witness_point, near_parallel)`.
    ///
    /// # Two deliberate departures from the oracle mirror (§EXT-2c)
    ///
    /// This used to mirror `_seg_tri_dist` (`geometry_exact.py:596-630`) term
    /// for term. Byte parity with the oracle on near-parallel configurations is
    /// now retired **on purpose** for this decision chain, because every error
    /// the mirror carried was one-sided HIGH — and this distance is compared
    /// against a demanded gap, where too high means a refusal becomes an
    /// acceptance:
    ///
    /// 1. **Edge candidates use [`segment_segment_distance_min`]**, the
    ///    complete five-candidate minimisation, instead of the pinned
    ///    `segment_segment_distance` whose near-parallel one-step recovery
    ///    OVERestimates by up to a measured 1.754e-6 mm (8.104e-7 mm on the §O2
    ///    counterexample — 8.1x the 1e-7 mm slack of the comparison it fed).
    ///    Distances only ever tighten, so band membership only grows toward
    ///    truth.
    /// 2. **The pierce candidate is classified.** A `NearParallel` pierce does
    ///    NOT set the distance to zero — no crossing was established — and does
    ///    NOT leave silently either. `near_parallel` comes back `true`, and the
    ///    returned distance is then an UPPER BOUND: the true distance may be
    ///    zero. Endpoints stay exact via `closest_on_tri`, so that is the only
    ///    one-sided term left.
    ///
    /// The event is charged to the process-global counter for
    /// [`NearParallelSite::SegTriPierce`].
    #[inline]
    pub(crate) fn seg_tri_dist(&self, a: V3, b: V3, t: usize) -> (f64, V3, bool) {
        seg_tri_dist_geom(a, b, self.mesh.v0(t), self.mesh.e1(t), self.mesh.e2(t))
    }

    /// Segment `[a, b]` against one triangle, for a member whose radius runs
    /// linearly from `r_a` at `a` to `r_b` at `b`: the least of
    /// `distance(axis(t), triangle) - radius(t)` over the axis, with `radius(t) =
    /// r_a + t (r_b - r_a)`. Returns `(cost_mm, witness_point, near_parallel)`.
    ///
    /// This is the exact clearance of a frustum (a union of balls whose radius
    /// is linear along the axis) to a convex face. `distance(axis(t), triangle)`
    /// is the distance from a point moving on a line to a convex set, so it is
    /// convex in `t`; minus a linear radius it stays convex, and a golden-section
    /// search over `[0, 1]` finds its minimum. The two end points are tried on
    /// their own, so a minimum at an end is exact.
    ///
    /// A pierce needs no special case: at a crossing the distance is 0, so the
    /// cost there is `-radius`, and the search finds it. `near_parallel` is the
    /// plain segment distance's flag for an undecided pierce, carried over so
    /// that the row is an upper bound, as for the constant-radius walk.
    ///
    /// COST. About 64 closest-point evaluations per face, and only for a
    /// tapered row; the walk prunes subtrees at `box_distance - r_max`, so only
    /// faces near the axis reach this.
    pub(crate) fn seg_tri_frustum_cost(
        &self,
        a: V3,
        b: V3,
        r_a: f64,
        r_b: f64,
        t: usize,
    ) -> (f64, V3, bool) {
        let (_, _, near_parallel) = self.seg_tri_dist(a, b, t);
        let v0 = self.mesh.v0(t);
        let p = v0;
        let q = add(v0, self.mesh.e1(t));
        let r = add(v0, self.mesh.e2(t));
        let ab = sub(b, a);
        let cost_at = |u: f64| -> (f64, V3) {
            let x = add(a, scale(ab, u));
            let c = closest_on_tri(x, p, q, r);
            (norm(sub(x, c)) - (r_a + (r_b - r_a) * u), c)
        };
        // Both ends are exact candidates. The minimum of the convex cost sits at
        // one of them, or inside, where the search finds it.
        let (cost_a, point_a) = cost_at(0.0);
        let (cost_b, point_b) = cost_at(1.0);
        let (mut best, mut best_point) = if cost_b < cost_a {
            (cost_b, point_b)
        } else {
            (cost_a, point_a)
        };
        let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
        let mut x1 = hi - GOLDEN_INV * (hi - lo);
        let mut x2 = lo + GOLDEN_INV * (hi - lo);
        let (mut f1, mut c1) = cost_at(x1);
        let (mut f2, mut c2) = cost_at(x2);
        for _ in 0..GOLDEN_STEPS {
            if f1 < f2 {
                hi = x2;
                x2 = x1;
                f2 = f1;
                c2 = c1;
                x1 = hi - GOLDEN_INV * (hi - lo);
                (f1, c1) = cost_at(x1);
            } else {
                lo = x1;
                x1 = x2;
                f1 = f2;
                c1 = c2;
                x2 = lo + GOLDEN_INV * (hi - lo);
                (f2, c2) = cost_at(x2);
            }
        }
        for (cost, point) in [(f1, c1), (f2, c2)] {
            if cost < best {
                best = cost;
                best_point = point;
            }
        }
        (best, best_point, near_parallel)
    }

    /// Exact minimum distance from segment [a,b] to the surface (branch & bound).
    ///
    /// One statement, two callers: this is [`Self::seg_surface_min_excluding`]
    /// with nothing excluded. The whole-surface and the face-excluded distance
    /// can therefore never disagree by arithmetic, only by the face set they
    /// were handed -- which is the property the honest-exemption rule rests on.
    pub(crate) fn seg_surface_min(&self, a: V3, b: V3) -> (f64, V3, u32) {
        self.seg_surface_min_excluding(a, b, &[])
    }

    /// Exact minimum distance from segment `[a,b]` to the surface MINUS a set
    /// of global face IDs.
    ///
    /// `excluded` must be SORTED ascending; membership is one binary search per
    /// triangle actually tested, and an empty set costs one `is_empty` branch.
    ///
    /// Returns `(distance_mm, witness_point, near_parallel_faces)`. A nonzero
    /// third element makes the distance an UPPER BOUND: that many faces had an
    /// undecidable pierce candidate, and the true minimum over the surface may
    /// be lower, including zero. The count is over FACES, so it is comparable
    /// across rows.
    ///
    /// The branch-and-bound prune stays exact under exclusion. `box_box_dist`
    /// is a lower bound over EVERY face in a subtree, so it is still a lower
    /// bound over the subset that survives exclusion; dropping faces can only
    /// raise the true minimum, never lower it, so a subtree the full-surface
    /// walk could skip is a subtree this walk can skip too.
    ///
    /// It also stays SOUND under the classified pierce. `best_d` can now be an
    /// over-estimate of a face's true distance (a skipped pierce leaves the
    /// endpoint/edge minimum standing), and the prune is `bound > best_d`, so a
    /// larger `best_d` prunes LESS. Uncertainty therefore costs traversal work,
    /// never a missed face.
    pub(crate) fn seg_surface_min_excluding(
        &self,
        a: V3,
        b: V3,
        excluded: &[u32],
    ) -> (f64, V3, u32) {
        let seg = SegPre::new(a, b);
        let (best_d, best_pt, near_parallel, _) =
            self.seg_surface_walk_excluding(a, b, excluded, 0.0, |_| false, |_, slot, bound| {
                seg_tri_dist_ball(&seg, &self.bvh.tris[slot], bound)
            });
        (best_d, best_pt, near_parallel)
    }

    /// The branch-and-bound walk behind [`Self::seg_surface_min_excluding`],
    /// generic in what one face costs.
    ///
    /// `face_cost(tid)` returns `(cost, witness, near_parallel)` for the global
    /// face `tid`; the walk keeps the face with the least cost. `slack` is how
    /// far BELOW the segment-to-box distance a face's cost may fall: a subtree is
    /// skipped when `box_distance - slack > best`. With `slack = 0` and the
    /// plain segment-to-face distance as the cost this is the distance walk
    /// exactly, bit for bit. A tapered member's cost is
    /// `distance - radius_at_the_nearest_point`, which is at least
    /// `distance - r_max`, so `slack = r_max` keeps the prune a true lower
    /// bound.
    ///
    /// `stop(best)` is asked each time the least cost falls; once it says
    /// yes the walk ends there and the fourth element is `true`. The least
    /// cost only ever falls, so a `stop` that holds for every cost at or
    /// below one it held for (a "below this bound" test) proves the full
    /// walk's answer lies there too. `|_| false` is the full walk, bit for
    /// bit: the prune and the faces tested do not depend on `stop`.
    fn seg_surface_walk_excluding(
        &self,
        a: V3,
        b: V3,
        excluded: &[u32],
        slack: f64,
        stop: impl Fn(f64) -> bool,
        face_cost: impl FnMut(usize, usize, f64) -> (f64, V3, bool),
    ) -> (f64, V3, u32, bool) {
        let (best_d, best_pt, near_parallel, stopped, [nodes_visited, nodes_pruned, tris_tested]) =
            self.seg_surface_walk_core(a, b, excluded, slack, stop, face_cost);
        if crate::perf::enabled() {
            crate::perf::add(crate::perf::Work::SegWalkQueries, 1);
            crate::perf::add(crate::perf::Work::SegWalkNodesVisited, nodes_visited);
            crate::perf::add(crate::perf::Work::SegWalkNodesPruned, nodes_pruned);
            crate::perf::add(crate::perf::Work::SegWalkTrisTested, tris_tested);
            // The batch loop that called this walk reads it back (per-row
            // share of the batch's faces); thread-local, so no row sees
            // another's.
            crate::perf::note_walk_tris(tris_tested);
        }
        (best_d, best_pt, near_parallel, stopped)
    }

    /// The walk itself, without the `perf` charge: the answer and the three
    /// tallies `[nodes visited, nodes pruned, faces tested]`. The kernel oracle
    /// (`kernel_speed_tests`) calls this, so its parallel tests never write the
    /// process-global counters the `perf` unit tests sum.
    #[expect(
        clippy::cast_sign_loss,
        reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged \
              as a class, and the one architecture item the class carried (geom_kernel's 32-bit BVH \
              index width) is now a named, enforced limit. TODO(cast-width-audit): retire site by \
              site.  See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn seg_surface_walk_core(
        &self,
        a: V3,
        b: V3,
        excluded: &[u32],
        slack: f64,
        stop: impl Fn(f64) -> bool,
        mut face_cost: impl FnMut(usize, usize, f64) -> (f64, V3, bool),
    ) -> (f64, V3, u32, bool, [u64; 3]) {
        let smin = box_min(a, b);
        let smax = box_max(a, b);
        let mut best_d = f64::INFINITY;
        let mut best_pt: V3 = [f64::NAN; 3];
        let mut near_parallel = 0u32;
        let mut stopped = false;
        // Write-only tallies for `perf` (nothing reads them back): integer
        // increments on locals, one charge per walk.
        let mut nodes_visited = 0u64;
        let mut nodes_pruned = 0u64;
        let mut tris_tested = 0u64;
        WALK_STACK.with(|s| {
            let mut stack = s.borrow_mut();
            stack.clear();
            let root = &self.bvh.nodes[0];
            stack.push((0, box_box_dist(smin, smax, root.bmin, root.bmax)));
            // Each entry carries its node's distance from the segment's box,
            // measured when it was pushed: the same call on the same operands
            // the pop used to repeat, so the prune reads the same bits.
            while let Some((ni, node_dist)) = stack.pop() {
                nodes_visited += 1;
                let node = &self.bvh.nodes[ni as usize];
                if node_dist - slack > best_d {
                    nodes_pruned += 1;
                    continue;
                }
                if node.leaf {
                    let start = node.a as usize;
                    let cnt = node.b as usize;
                    for slot in start..start + cnt {
                        let tid = self.bvh.order[slot];
                        // `tid` IS the global face ID: leaf order carries global
                        // IDs even for a narrowed view (bvh.rs `build`), which is
                        // the same fact `seg_tri_dist` relies on one line below to
                        // index the shared geometry. `face_ids()` is the
                        // slot-to-global map and is indexed by SLOT, so looking
                        // `tid` up in it is out of bounds on a narrowed view and a
                        // no-op only on a full one.
                        if !excluded.is_empty() && excluded.binary_search(&tid).is_ok() {
                            continue;
                        }
                        tris_tested += 1;
                        // The face test is told the least cost so far: a face it
                        // can show is not below it may answer `INFINITY`, which
                        // `d < best_d` rejects exactly as the true cost would.
                        let (d, pt, np) = face_cost(tid as usize, slot, best_d);
                        if np {
                            near_parallel += 1;
                        }
                        if d < best_d {
                            best_d = d;
                            best_pt = pt;
                            // pierce early-out allowed (spec sec 3.3)
                            if stop(best_d) {
                                stopped = true;
                                break;
                            }
                        }
                    }
                    if stopped {
                        break;
                    }
                } else {
                    let l = node.a;
                    let r = node.b;
                    let dl = box_box_dist(
                        smin,
                        smax,
                        self.bvh.nodes[l as usize].bmin,
                        self.bvh.nodes[l as usize].bmax,
                    );
                    let dr = box_box_dist(
                        smin,
                        smax,
                        self.bvh.nodes[r as usize].bmin,
                        self.bvh.nodes[r as usize].bmax,
                    );
                    if dl <= dr {
                        stack.push((r, dr));
                        stack.push((l, dl));
                    } else {
                        stack.push((l, dl));
                        stack.push((r, dr));
                    }
                }
            }
        });
        (
            best_d,
            best_pt,
            near_parallel,
            stopped,
            [nodes_visited, nodes_pruned, tris_tested],
        )
    }

    /// **Whether some face proves the capsule row `[a, b]` below its bound**:
    /// a face whose segment distance `d` (the walk's own [`Self::seg_tri_dist`],
    /// so the very number the walk would take) reads `d - thresh < at_least`.
    /// The walk's answer is the least such `d` over every face, so one such
    /// face proves its clearance below the bound too, whatever else it finds.
    ///
    /// The walk itself bounds a subtree by its box's distance from the
    /// segment's own BOX, which for a long slanted segment is most of the
    /// model, so a row near a wall can test thousands of faces before it
    /// reaches the near one. This search cuts the segment into pieces about as
    /// long as the distance it is looking within (`at_least + thresh`), bounds
    /// each subtree by its distance from each piece's box, and skips one that
    /// cannot hold a face within that distance; it stops at the first face
    /// that proves the row below. `false` proves nothing (the caller runs the
    /// full walk): finding no face is never read as a pass.
    fn seg_face_below(&self, a: V3, b: V3, thresh: f64, at_least: f64) -> bool {
        let Some((found, tris_tested)) = self.seg_face_below_core(a, b, thresh, at_least) else {
            return false;
        };
        if crate::perf::enabled() {
            crate::perf::add(crate::perf::Work::SegProveQueries, 1);
            crate::perf::add(crate::perf::Work::SegProveRefused, u64::from(found));
            crate::perf::add(crate::perf::Work::SegProveTrisTested, tris_tested);
            if found {
                crate::perf::note_walk_tris(tris_tested);
            }
        }
        found
    }

    /// The search itself, without the `perf` charge: `None` when the bound or
    /// the segment is unusable (no search ran, nothing to charge), else whether
    /// a face proves the row below and how many faces it tested.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a piece count of a finite segment over a positive length, clamped to 1..=u16::MAX; \
                  BVH node indices are 32-bit by the kernel's named limit, as in the walk"
    )]
    fn seg_face_below_core(
        &self,
        a: V3,
        b: V3,
        thresh: f64,
        at_least: f64,
    ) -> Option<(bool, u64)> {
        let within = at_least + thresh;
        if !(within > 0.0 && within.is_finite()) {
            return None;
        }
        let length = norm(sub(b, a));
        if !length.is_finite() {
            return None;
        }
        let pieces = (length / within).ceil().clamp(1.0, f64::from(u16::MAX)) as usize;
        // A face whose distance is at least `reach` reads `d - thresh >=
        // at_least` (`reach` is `within` raised by 1e-12 of itself and 1e-12,
        // far above the rounding of `within` and of the subtraction), so the
        // ball test may answer `INFINITY` for it.
        let seg = SegPre::new(a, b);
        let reach = within * (1.0 + 1e-12) + 1e-12;
        let mut tris_tested = 0u64;
        let mut found = false;
        STACK.with(|s| {
            let mut stack = s.borrow_mut();
            'pieces: for piece in 0..pieces {
                let t0 = piece as f64 / pieces as f64;
                let t1 = (piece + 1) as f64 / pieces as f64;
                let p = add(a, scale(sub(b, a), t0));
                let q = add(a, scale(sub(b, a), t1));
                let (pmin, pmax) = (box_min(p, q), box_max(p, q));
                stack.clear();
                stack.push(0);
                while let Some(ni) = stack.pop() {
                    let node = &self.bvh.nodes[ni as usize];
                    if box_box_dist(pmin, pmax, node.bmin, node.bmax) - thresh >= at_least {
                        continue;
                    }
                    if node.leaf {
                        let start = node.a as usize;
                        let cnt = node.b as usize;
                        for tri in &self.bvh.tris[start..start + cnt] {
                            tris_tested += 1;
                            let (d, _, _) = seg_tri_dist_ball(&seg, tri, reach);
                            if d - thresh < at_least {
                                found = true;
                                break 'pieces;
                            }
                        }
                    } else {
                        let l = node.a as usize;
                        let r = node.b as usize;
                        let dl = box_box_dist(pmin, pmax, self.bvh.nodes[l].bmin, self.bvh.nodes[l].bmax);
                        let dr = box_box_dist(pmin, pmax, self.bvh.nodes[r].bmin, self.bvh.nodes[r].bmax);
                        if dl <= dr {
                            stack.push(r as i32);
                            stack.push(l as i32);
                        } else {
                            stack.push(l as i32);
                            stack.push(r as i32);
                        }
                    }
                }
            }
        });
        Some((found, tris_tested))
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
    ///
    /// ONE PREDICATE. `excluded` empty is the whole-surface test, so the
    /// demanded-gap arithmetic (`best_d - (r + standoff)`, or the per-face
    /// frustum cost for a tapered row) has exactly one
    /// statement in the kernel and cannot fork between a stage that exempts
    /// faces and one that does not. Only two things vary per caller: which
    /// faces are in the set, and what number `standoff` carries.
    ///
    /// TWO READINGS OF A TAPERED ROW, one per `frustum` value. `false` is the
    /// capsule of the wider end (`r_max` for the whole segment): every caller
    /// before the census rule, and still every caller that is not the census.
    /// `true` is the frustum the file draws (see below). A row whose two radii
    /// are equal reads the same either way, bit for bit.
    ///
    /// `at_least`: a row whose walk PROVES its clearance below this number
    /// (the least cost so far, read as a clearance, is already under it; the
    /// least cost only falls) ends there and comes back `None`. Every other
    /// row is the full answer, bit for bit: the walk prunes and tests the
    /// same faces whatever `at_least` is. `f64::NEG_INFINITY` never ends a
    /// walk.
    #[allow(clippy::too_many_arguments)]
    fn seg_clear_one_excluding(
        &self,
        p0: V3,
        p1: V3,
        r0: f64,
        r1: f64,
        standoff: f64,
        inset0: f64,
        inset1: f64,
        excluded: &[u32],
        frustum: bool,
        at_least: f64,
    ) -> Option<SegmentClearanceClassified> {
        let d = sub(p1, p0);
        let l = norm(d);
        if l < 1e-9 {
            return Some(SegmentClearanceClassified::Unmeasured(UnmeasuredSegment::DegenerateAxis));
        }
        let u = scale(d, 1.0 / l);
        let s_lo = inset0;
        let s_hi = l - inset1;
        if s_hi - s_lo <= 1e-9 {
            return Some(SegmentClearanceClassified::Unmeasured(UnmeasuredSegment::InsetsConsumeAxis));
        }
        let a = add(p0, scale(u, s_lo));
        let b = add(p0, scale(u, s_hi));
        let r_lo = r0 + (r1 - r0) * (s_lo / l);
        let r_hi = r0 + (r1 - r0) * (s_hi / l);
        // THE FRUSTUM THE FILE DRAWS (rule R3, clearance-census-diagnosis.md).
        // The radius is linear along the axis, `r_lo` at `a` to `r_hi` at `b`.
        // A face beside the thin end is not as near as a face beside the thick
        // end, and subtracting `r_max` for the whole segment read every face at
        // the thick end's radius. A constant radius (`r_lo == r_hi`, every caller
        // that hands one radius twice) keeps the one-subtraction form below,
        // bit for bit; a tapered one, when the caller asks for the frustum, is
        // measured per face, exactly.
        let tapered = frustum && (r_hi - r_lo).abs() > TAPER_EPS_MM;
        // For a tapered row `best_d` is already `distance - radius` at the
        // nearest point, so only the standoff is left to subtract.
        let thresh = if tapered {
            standoff
        } else {
            r_lo.max(r_hi) + standoff
        };
        // The clearance below is `best_d - thresh`, and a floating-point
        // subtraction of one number is monotone in the other, so a least cost
        // that already reads under `at_least` proves the walk's answer does.
        let below = |best: f64| best - thresh < at_least;
        // A bounded capsule row first looks for ONE face that proves it below
        // its bound, near the segment only; the full walk runs only when none
        // is found (see `seg_face_below`).
        if !tapered
            && excluded.is_empty()
            && at_least > f64::NEG_INFINITY
            && self.seg_face_below(a, b, thresh, at_least)
        {
            return None;
        }
        let (best_d, best_pt, near_parallel_faces, stopped) = if tapered {
            let (r_a, r_b) = (r_lo, r_hi);
            self.seg_surface_walk_excluding(a, b, excluded, r_a.max(r_b), below, |tid, _, _| {
                self.seg_tri_frustum_cost(a, b, r_a, r_b, tid)
            })
        } else {
            // `seg_surface_min_excluding`'s walk, with the stop.
            let seg = SegPre::new(a, b);
            self.seg_surface_walk_excluding(a, b, excluded, 0.0, below, |_, slot, bound| {
                seg_tri_dist_ball(&seg, &self.bvh.tris[slot], bound)
            })
        };
        if stopped {
            return None;
        }
        // NO CANDIDATE IS NOT A MEASUREMENT. `seg_surface_min_excluding` seeds
        // `best_d` with `f64::INFINITY` and only ever lowers it, so a
        // non-finite `best_d` means the walk found no face to measure against.
        // With a non-empty mesh (`SurfaceBuildError::EmptyFaces` forbids the
        // other case) and a walk whose prune is `> best_d` -- never true while
        // `best_d` is infinite, so every leaf is descended into -- the only way
        // to reach here with no candidate is that `excluded` covered every
        // face. Subtracting `thresh` from that infinity would yield an infinite
        // "clearance" that reads as the best possible result. It is the worst:
        // nothing was tested.
        if !best_d.is_finite() {
            return Some(SegmentClearanceClassified::Unmeasured(UnmeasuredSegment::AllFacesExcluded));
        }
        let clr = best_d - thresh;
        // clearance-vs-0 threshold band (spec sec 5.4 item 3)
        self.band_check(clr, 0.0);
        // AN UNDECIDED PIERCE IS NOT A MEASUREMENT EITHER. The walk did produce
        // a number, but at least one face's pierce candidate was skipped, and a
        // skipped pierce turns a true distance of 0 into something positive. The
        // number is therefore one-sided HIGH and cannot affirm a gap. Reporting
        // it as `Measured` would be the `INFINITY` defect again, one variant
        // along: a value that passes every `>= threshold` test standing in for
        // an answer nobody has.
        if near_parallel_faces > 0 {
            return Some(SegmentClearanceClassified::UpperBound {
                clearance_mm: clr,
                witness: best_pt,
                near_parallel_faces,
            });
        }
        Some(SegmentClearanceClassified::Measured {
            clearance_mm: clr,
            witness: best_pt,
        })
    }

    // Same rationale as `seg_clear_one_excluding` immediately above: this is
    // that primitive.s batched (SoA) entry point, one argument per per-member
    // input array plus the shared `standoff` -- the established batch shape,
    // not a candidate for bundling.
    /// **THE CLEARANCE PREDICATE.** One [`SegmentClearanceClassified`] per row,
    /// in row order.
    ///
    /// This is [`Self::segment_clearance_excluding_classified`] with an empty
    /// exclusion set, i.e. the whole-surface test.
    ///
    /// Replaces both `segment_clearance_typed` and the lossy float shim
    /// `segment_clearance`. The float form could not say "this row was never
    /// tested" — it said `INFINITY`, which every `>= threshold` comparison
    /// reads as a pass — and could not carry the third state either, so it is
    /// removed rather than deprecated.
    #[allow(clippy::too_many_arguments)]
    pub fn segment_clearance_classified(
        &self,
        p0s: &[V3],
        p1s: &[V3],
        r0s: &[f64],
        r1s: &[f64],
        standoff: f64,
        inset0s: &[f64],
        inset1s: &[f64],
    ) -> Result<Vec<SegmentClearanceClassified>, BatchInputError> {
        self.segment_clearance_excluding_classified(
            p0s,
            p1s,
            r0s,
            r1s,
            standoff,
            inset0s,
            inset1s,
            &[], //
        )
    }

    /// **THE CLEARANCE PREDICATE, with exclusions.**
    /// [`Self::segment_clearance_classified`] with a PER-ROW set of global face
    /// IDs the distance test must not see. Every other clearance entry point in
    /// this file delegates here.
    ///
    /// `excluded_faces` is either empty -- which is exactly
    /// [`Self::segment_clearance_classified`], and the reason that entry point
    /// delegates here rather than repeating the loop -- or one SORTED,
    /// deduplicated slice of global face IDs per row, in row order.
    ///
    /// One [`SegmentClearanceClassified`] per row, in row order. A row the
    /// predicate never tested comes back as
    /// [`SegmentClearanceClassified::Unmeasured`]; a row whose walk skipped an
    /// undecidable pierce candidate comes back as
    /// [`SegmentClearanceClassified::UpperBound`]. The type system will not let
    /// a caller compare either against a threshold without stating what it
    /// means: see [`SegmentClearanceClassified::clears`], which takes both
    /// fallbacks and defaults neither.
    ///
    /// WHAT THIS IS FOR, and what it deliberately is not. A contact-declaring
    /// arm is legitimately inside the material it is seated in, so its own seat
    /// zone must be exempt or every seated arm refuses itself. The exemption
    /// that does that honestly names the FACES it forgives and tests the rest of
    /// the axis in full. The exemption it replaces named a LENGTH of axis --
    /// a radius-proportional skip measured from the seat -- and everything
    /// inside that length, foreign features included, went untested. A length
    /// is not a target: a fatter arm skipped further, so the same axis could
    /// pass at a radius that put it 0.242 mm inside the model and refuse at a
    /// thinner one with 0.569 mm of real gap (`.git/mask-honesty-evidence`).
    /// Exempting a face set cannot invert that way, because the set does not
    /// move when the radius does.
    ///
    /// COST. One batched call per row set, the same shape and the same
    /// parallelism as the unrestricted entry point; the exclusion costs one
    /// binary search per triangle the walk would otherwise have tested, and
    /// nothing at all on a row with an empty set.
    ///
    /// The caller owns sortedness. An unsorted slice is not rejected here --
    /// this is the inner loop of the clearance stage and a per-row validation
    /// pass would price a check the two production callers already satisfy by
    /// construction -- it simply fails to exclude, which is the SAFE direction:
    /// an unexcluded face can only make the verdict stricter.
    // Same signature rationale as `segment_clearance_classified` above, plus one
    // slice of per-row exclusions; every field is read at every call site.
    #[allow(clippy::too_many_arguments)]
    pub fn segment_clearance_excluding_classified(
        &self,
        p0s: &[V3],
        p1s: &[V3],
        r0s: &[f64],
        r1s: &[f64],
        standoff: f64,
        inset0s: &[f64],
        inset1s: &[f64],
        excluded_faces: &[&[u32]],
    ) -> Result<Vec<SegmentClearanceClassified>, BatchInputError> {
        self.clearance_rows_excluding(
            p0s,
            p1s,
            r0s,
            r1s,
            standoff,
            inset0s,
            inset1s,
            excluded_faces,
            false,
        )
    }

    /// **THE CLEARANCE PREDICATE, read as the FRUSTUM the file draws** (rule
    /// R3 of `clearance-census-diagnosis.md`, 2026-10-04).
    ///
    /// [`Self::segment_clearance_classified`] reads a row whose radius runs
    /// from `r0` to `r1` as a capsule of the WIDER radius all along. That reading
    /// errs only toward refusal, and it is the reading the support engine's
    /// placement gates (`plinth-support`'s brace, rewire and CCO stages) were
    /// built, pinned and measured against; it is left as it was. This entry
    /// point reads the same row as the solid the emitted file draws: a frustum,
    /// radius linear along the axis (between the row's two radii at its two
    /// clipped ends), so the clearance is
    /// `min over the axis of (distance to the surface - radius) - standoff`.
    /// A face beside the thin end of a long taper is no longer read at the thick
    /// end's radius. A row with equal radii is bit-identical to the capsule
    /// entry point. The result is never lower than the capsule reading.
    ///
    /// For the member-against-model CENSUS, which reports what the file draws.
    /// COST: about 64 closest-point evaluations per candidate face of a tapered
    /// row, only for faces the walk cannot prune at `box distance - r_max`.
    #[allow(clippy::too_many_arguments)]
    pub fn segment_frustum_clearance_classified(
        &self,
        p0s: &[V3],
        p1s: &[V3],
        r0s: &[f64],
        r1s: &[f64],
        standoff: f64,
        inset0s: &[f64],
        inset1s: &[f64],
    ) -> Result<Vec<SegmentClearanceClassified>, BatchInputError> {
        self.clearance_rows_excluding(p0s, p1s, r0s, r1s, standoff, inset0s, inset1s, &[], true)
    }

    /// **THE CLEARANCE PREDICATE, asked only whether each row reaches a bound**
    /// (speed lane B, 2026-10-13). [`Self::segment_clearance_classified`],
    /// row for row, except that a row whose walk proves its clearance below
    /// `at_least[i]` stops there and comes back `None`. Every row that comes
    /// back `Some` is the full answer, bit for bit (the walk prunes and tests
    /// the same faces); a `None` row's full answer is a measured clearance
    /// below the bound, or an upper bound below it, so a caller that only
    /// admits a MEASURED clearance at or above the bound refuses it either
    /// way. For a search that refuses most of what it reads: a near face
    /// usually ends the walk within its first leaves.
    #[allow(clippy::too_many_arguments)]
    pub fn segment_clearance_classified_at_least(
        &self,
        p0s: &[V3],
        p1s: &[V3],
        r0s: &[f64],
        r1s: &[f64],
        standoff: f64,
        inset0s: &[f64],
        inset1s: &[f64],
        at_least: &[f64],
    ) -> Result<Vec<Option<SegmentClearanceClassified>>, BatchInputError> {
        BatchInputError::require_len("segment_clearance", "at_least", p0s.len(), at_least.len())?;
        self.clearance_rows_bounded(
            p0s, p1s, r0s, r1s, standoff, inset0s, inset1s, &[], false, at_least,
        )
    }

    /// The batch loop behind the clearance entry points: one row per input, in
    /// order, `frustum` choosing the reading of a tapered row (see
    /// [`Self::seg_clear_one_excluding`]).
    #[allow(clippy::too_many_arguments)]
    fn clearance_rows_excluding(
        &self,
        p0s: &[V3],
        p1s: &[V3],
        r0s: &[f64],
        r1s: &[f64],
        standoff: f64,
        inset0s: &[f64],
        inset1s: &[f64],
        excluded_faces: &[&[u32]],
        frustum: bool,
    ) -> Result<Vec<SegmentClearanceClassified>, BatchInputError> {
        Ok(self
            .clearance_rows_bounded(
                p0s, p1s, r0s, r1s, standoff, inset0s, inset1s, excluded_faces, frustum, &[],
            )?
            .into_iter()
            // With no bound a walk never stops, so every row is `Some`; the
            // fallback is the "nothing was tested" answer, never a pass.
            .map(|row| {
                row.unwrap_or(SegmentClearanceClassified::Unmeasured(
                    UnmeasuredSegment::AllFacesExcluded,
                ))
            })
            .collect())
    }

    /// [`Self::clearance_rows_excluding`] with a per-row bound (`at_least`,
    /// empty for none; see [`Self::segment_clearance_classified_at_least`]).
    #[allow(clippy::too_many_arguments)]
    fn clearance_rows_bounded(
        &self,
        p0s: &[V3],
        p1s: &[V3],
        r0s: &[f64],
        r1s: &[f64],
        standoff: f64,
        inset0s: &[f64],
        inset1s: &[f64],
        excluded_faces: &[&[u32]],
        frustum: bool,
        at_least: &[f64],
    ) -> Result<Vec<Option<SegmentClearanceClassified>>, BatchInputError> {
        let n = p0s.len();
        BatchInputError::require_len("segment_clearance", "p1s", n, p1s.len())?;
        BatchInputError::require_len("segment_clearance", "r0s", n, r0s.len())?;
        BatchInputError::require_len("segment_clearance", "r1s", n, r1s.len())?;
        BatchInputError::require_len("segment_clearance", "inset0s", n, inset0s.len())?;
        BatchInputError::require_len("segment_clearance", "inset1s", n, inset1s.len())?;
        if !excluded_faces.is_empty() {
            BatchInputError::require_len(
                "segment_clearance",
                "excluded_faces",
                n,
                excluded_faces.len(),
            )?;
        }
        // Write-only batch tally for `perf`: per row, faces tested and time,
        // folded with integer sums and maxima (order-free), one charge and one
        // row per batch. Off, `counting` is false and the loop is the plain one.
        // The clocks and the row are the full trace's; the face tallies and
        // the batch counters are kept in light mode too.
        let counting = crate::perf::enabled();
        let timing = crate::perf::traced();
        let _batch_span = crate::perf::span(crate::perf::Span::ClearBatch);
        let batch_started = timing.then(std::time::Instant::now);
        let tris_sum = std::sync::atomic::AtomicU64::new(0);
        let tris_max = std::sync::atomic::AtomicU64::new(0);
        let ns_sum = std::sync::atomic::AtomicU64::new(0);
        let ns_max = std::sync::atomic::AtomicU64::new(0);
        let rows = Ok(par::run(|| {
            (0..n)
                .into_par_iter()
                .map(|i| {
                    let excluded = excluded_faces.get(i).copied().unwrap_or(&[]);
                    let row_started = timing.then(std::time::Instant::now);
                    let result = self.seg_clear_one_excluding(
                        p0s[i],
                        p1s[i],
                        r0s[i],
                        r1s[i],
                        standoff,
                        inset0s[i],
                        inset1s[i],
                        excluded,
                        frustum,
                        at_least.get(i).copied().unwrap_or(f64::NEG_INFINITY),
                    );
                    if counting {
                        let tris = crate::perf::walk_tris();
                        let relaxed = std::sync::atomic::Ordering::Relaxed;
                        tris_sum.fetch_add(tris, relaxed);
                        tris_max.fetch_max(tris, relaxed);
                        if let Some(started) = row_started {
                            let ns = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
                            ns_sum.fetch_add(ns, relaxed);
                            ns_max.fetch_max(ns, relaxed);
                        }
                    }
                    result
                })
                .collect()
        }));
        if counting {
            let relaxed = std::sync::atomic::Ordering::Relaxed;
            let (rows_n, tris, tris_top) = (
                crate::perf::count(n),
                tris_sum.load(relaxed),
                tris_max.load(relaxed),
            );
            crate::perf::add(crate::perf::Work::ClearBatches, 1);
            crate::perf::add(crate::perf::Work::ClearBatchRows, rows_n);
            crate::perf::add(crate::perf::Work::ClearBatchTris, tris);
            crate::perf::add(crate::perf::Work::ClearBatchTrisMaxRow, tris_top);
        }
        if let Some(started) = batch_started {
            let relaxed = std::sync::atomic::Ordering::Relaxed;
            let (rows_n, tris, tris_top) = (
                crate::perf::count(n),
                tris_sum.load(relaxed),
                tris_max.load(relaxed),
            );
            crate::perf::row(
                crate::perf::Rows::ClearBatch,
                &[
                    rows_n,
                    tris,
                    tris_top,
                    u64::from(frustum),
                    u64::from(!excluded_faces.is_empty()),
                    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
                    ns_sum.load(relaxed),
                    ns_max.load(relaxed),
                    crate::perf::current_clear_site(),
                ],
            );
        }
        rows
    }

    /// Every global face ID within `within_mm[i]` of segment `i`, one batched
    /// BVH walk per row and ONE call for the whole row set.
    ///
    /// This is the band step of the honest contact exemption: the seat's own
    /// contact zone is the connected component, containing the seat's own face,
    /// of the faces this returns. The flood fill that picks that component out
    /// belongs to the caller, which owns the mesh's face adjacency; the kernel
    /// owns only the geometric band.
    ///
    /// Each row's IDs come back SORTED ascending and deduplicated, which is the
    /// order [`Self::segment_clearance_excluding_classified`] requires.
    ///
    /// # Membership is PROOF-ONLY
    ///
    /// A band is an EXEMPTION: a face in it is excluded from later clearance
    /// walks. So a face joins a band only on a MEASURED `d <= within_mm`. When a
    /// face's pierce candidate is undecidable, the face is counted in that row's
    /// `near_parallel` tally and admitted only if its measured
    /// endpoint/edge distance put it in the band anyway. Adding it on the
    /// unproven pierce would grow an exemption on uncertainty — the permissive
    /// direction — while omitting it keeps the face in later clearance tests,
    /// where a genuine touch fails the row (§EXT-2 justification 12).
    ///
    /// Note the asymmetry with the clearance entry points: the SAME
    /// `seg_tri_dist` uncertainty is band-tolerable and clearance-intolerable,
    /// because the two consumers sit on opposite sides of the exemption.
    ///
    /// LIMITS. `max_rows` and `max_faces_per_row` are checked before and during
    /// the walk, so the batch's peak memory is bounded by
    /// `rows * max_faces_per_row * 4` bytes and is knowable from the caller's
    /// own numbers before any allocation. `max_total_faces` is checked against
    /// the REALIZED total, not against `rows * max`, so it charges what the walk
    /// actually collected.
    pub fn faces_near_segments_batch_classified(
        &self,
        p0s: &[V3],
        p1s: &[V3],
        within_mm: &[f64],
        limits: NearFaceBatchLimits,
    ) -> Result<FacesNearSegmentsClassified, NearFaceBatchError> {
        let n = p0s.len();
        BatchInputError::require_len("faces_near_segments_batch", "p1s", n, p1s.len())?;
        BatchInputError::require_len("faces_near_segments_batch", "within_mm", n, within_mm.len())?;
        if n > limits.max_rows {
            return Err(NearFaceBatchError::RowLimitExceeded {
                required: n,
                limit: limits.max_rows,
            });
        }
        for (row, within) in within_mm.iter().enumerate() {
            if !within.is_finite() || *within < 0.0 {
                return Err(NearFaceBatchError::InvalidRadius {
                    row,
                    within_mm: *within,
                });
            }
        }
        for (row, point) in p0s.iter().chain(p1s.iter()).enumerate() {
            for (axis, component) in point.iter().enumerate() {
                if !component.is_finite() {
                    return Err(NearFaceBatchError::NonFinitePoint {
                        row: row % n.max(1),
                        axis,
                    });
                }
            }
        }

        let rows: Vec<Result<(Vec<u32>, u32), NearFaceBatchError>> = par::run(|| {
            (0..n)
                .into_par_iter()
                .map(|row| {
                    self.faces_near_segment(
                        p0s[row],
                        p1s[row],
                        within_mm[row],
                        row,
                        limits.max_faces_per_row,
                    )
                })
                .collect()
        });

        let mut out = Vec::with_capacity(n);
        let mut near_parallel = Vec::with_capacity(n);
        let mut total = 0_usize;
        for row in rows {
            let (faces, np) = row?;
            total = total
                .checked_add(faces.len())
                .ok_or(NearFaceBatchError::TotalFaceArithmeticOverflow)?;
            out.push(faces);
            near_parallel.push(np);
        }
        if total > limits.max_total_faces {
            return Err(NearFaceBatchError::TotalFaceLimitExceeded {
                required: total,
                limit: limits.max_total_faces,
            });
        }
        Ok(FacesNearSegmentsClassified {
            rows: out,
            near_parallel,
        })
    }

    /// One row of [`Self::faces_near_segments_batch`].
    ///
    /// `box_box_dist` is a lower bound over every face in a subtree, so a
    /// subtree whose bound already exceeds `within_mm` cannot contain a face in
    /// the band and is skipped whole. The test is `<=` so a face exactly at the
    /// band edge is IN the band -- the exemption must not depend on a knife
    /// edge, and admitting the boundary face only ever makes the exemption
    /// larger by one face of the seat's own zone, never smaller.
    #[expect(
        clippy::cast_sign_loss,
        reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged \
              as a class, and the one architecture item the class carried (geom_kernel's 32-bit BVH \
              index width) is now a named, enforced limit. TODO(cast-width-audit): retire site by \
              site.  See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn faces_near_segment(
        &self,
        a: V3,
        b: V3,
        within_mm: f64,
        row: usize,
        max_faces: usize,
    ) -> Result<(Vec<u32>, u32), NearFaceBatchError> {
        let smin = box_min(a, b);
        let smax = box_max(a, b);
        let mut found: Vec<u32> = Vec::new();
        let mut near_parallel = 0u32;
        let seg = SegPre::new(a, b);
        let mut stack: Vec<i32> = vec![0];
        while let Some(ni) = stack.pop() {
            let node = &self.bvh.nodes[ni as usize];
            if box_box_dist(smin, smax, node.bmin, node.bmax) > within_mm {
                continue;
            }
            if node.leaf {
                let start = node.a as usize;
                let cnt = node.b as usize;
                for slot in start..start + cnt {
                    let tid = self.bvh.order[slot];
                    // A face the ball proves beyond the band answers `INFINITY`,
                    // which `d <= within_mm` rejects as the true distance would;
                    // its `np` flag is exact either way.
                    let (d, _, np) = seg_tri_dist_ball(&seg, &self.bvh.tris[slot], within_mm);
                    if np {
                        // Counted, NOT admitted on the strength of the
                        // undecidable pierce: an exemption grows only on proof.
                        // The face still joins the band if its MEASURED distance
                        // puts it there, which the `d <= within_mm` test below
                        // decides on its own.
                        near_parallel += 1;
                    }
                    if d <= within_mm {
                        if found.len() >= max_faces {
                            return Err(NearFaceBatchError::RowFaceLimitExceeded {
                                row,
                                limit: max_faces,
                            });
                        }
                        // Global already -- see `seg_surface_min_excluding`:
                        // BVH leaf order carries global face IDs, so `tid` is
                        // the ID to report, not a slot to resolve.
                        found.push(tid);
                    }
                }
            } else {
                stack.push(node.a);
                stack.push(node.b);
            }
        }
        found.sort_unstable();
        found.dedup();
        Ok((found, near_parallel))
    }

    // ---- penetration intervals -------------------------------------------
    // Same rationale as `seg_clear_one_excluding` above: the per-member primitive
    // behind the batched `penetration_intervals_batch` entry point
    // (SPEC-EMBED-POLICY rule-4 transit detection) -- every argument is the
    // geometry + sampling config a single interval extraction needs.
    #[allow(clippy::too_many_arguments)]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "float-to-integer casts on lattice or step arithmetic: Rust's `as` saturates at the \
              integer bounds and maps NaN to 0, and every site in this function is range-checked \
              before the cast or clamped into range after it (read site by site, 2026-08-26). See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
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
            let (executed, near_parallel) =
                self.collect_hits_with_stack(o, dv, 1.0, &mut hits, &mut traversal_stack);
            meter.work += executed;
            meter.near_parallel = meter.near_parallel.saturating_add(near_parallel);
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
            near_parallel: 0,
        };
        if l < 1e-9 {
            return census;
        }
        for ray in capsule_rays(p0, p1, r0, r1, n_offset_rays, rays_all) {
            let seg = ray.length_mm();
            if seg < 1e-9 {
                continue;
            }
            let (crossings, tests, near_parallel) = STACK.with(|s| {
                self.count_hits_with_stack(ray.origin(), ray.direction(), 1.0, &mut s.borrow_mut())
            });
            census.crossings += crossings;
            census.tests += tests;
            census.near_parallel = census.near_parallel.saturating_add(near_parallel);
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
    pub fn penetration_census_batch_classified(
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
    pub fn penetration_intervals_batch_census_limits_classified(
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
    /// This entry point discards the execution receipt — **but not the
    /// uncertainty tally**, which rides on the returned
    /// [`PenetrationIntervalsClassified`]. A caller that charges a CAP from
    /// this batch's counted demand should call
    /// [`Self::penetration_intervals_batch_bounded_metered_classified`] instead
    /// and check the receipt against what it charged (FU-8g H2).
    #[allow(clippy::too_many_arguments)]
    pub fn penetration_intervals_batch_bounded_classified(
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
    ) -> Result<PenetrationIntervalsClassified, PenetrationBatchError> {
        self.penetration_intervals_batch_bounded_metered_classified(
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

    /// [`Self::penetration_intervals_batch_bounded_classified`], plus the
    /// receipt: what
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
    pub fn penetration_intervals_batch_bounded_metered_classified(
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
    ) -> Result<(PenetrationIntervalsClassified, PenetrationBatchMeter), PenetrationBatchError>
    {
        let n = p0s.len();
        Self::validate_penetration_inputs(p0s, p1s, r0s, r1s)?;
        let census =
            self.penetration_census_batch_classified(p0s, p1s, r0s, r1s, n_offset_rays, rays_all)?;
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
        let mut near_parallel = reserve_penetration_vec(n, None, "row uncertainty tallies")?;
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
            near_parallel.push(executed.near_parallel);
        }
        Self::check_penetration_meter(&requirements, meter)?;
        Ok((
            PenetrationIntervalsClassified {
                rows: ordered,
                near_parallel,
            },
            meter,
        ))
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

    // ---- sphere breach ----------------------------------------------------
    /// Collect faces whose exact point-distance to `c` is within `tol` of `nd`
    /// (for measured-tie detection). Branch & bound, ascending face index.
    #[expect(
        clippy::cast_sign_loss,
        reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged \
              as a class, and the one architecture item the class carried (geom_kernel's 32-bit BVH \
              index width) is now a named, enforced limit. TODO(cast-width-audit): retire site by \
              site.  See docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
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

    /// Distance from `c` to the far side of the wall it is resting against,
    /// minimised over an inward cone. `INFINITY` when no ray in the cone leaves
    /// the material within `r`.
    ///
    /// **WHAT THE FILTER IS FOR, and why it is no longer a distance.** Every
    /// ray here starts at the ball centre and points into the wall, so its
    /// first crossing is the ball's own ENTRY through the near face — the face
    /// the ball is already sitting on. That crossing is not a far side and must
    /// not be reported as one.
    ///
    /// This used to be discarded by absolute distance: throw away any crossing
    /// closer than `nd + NEAR_WALL_SKIP`, with `NEAR_WALL_SKIP = 0.05 mm`. A
    /// distance cannot tell "this is the near face again" from "this is the far
    /// face, and the wall is thin", so it threw away both. A wall thinner than
    /// the skip lost its far face along with its near face, `dmin` stayed
    /// infinite, and `breach_one` reported NO BREACH — a support foot admitted
    /// straight through a sub-0.05 mm feature such as a fin or a cape edge, and
    /// reported safe. The predicate was not merely coarse there, it was
    /// INVERTED: a 0.10 mm wall breached and a 0.02 mm wall at the same standoff
    /// did not. `tests/near_wall_thin_feature.rs` is that witness.
    ///
    /// The replacement asks the question the skip was always a proxy for. A
    /// crossing where the ray runs AGAINST the face's outward normal is the ray
    /// entering material; one where it runs WITH the outward normal is the ray
    /// leaving it. The far side of the wall is the first crossing the ray
    /// leaves through, at any thickness whatsoever. The test is a sign, so it
    /// carries no tolerance, no length scale and no tuned number — it cannot be
    /// wrong by a little for a thin wall the way a distance was.
    ///
    /// `nd` is retained in the signature because the caller's tie detection
    /// and reach precondition are stated in terms of it; this routine no longer
    /// reads it.
    fn cone_thickness(
        &self,
        c: V3,
        r: f64,
        inward: V3,
        _nd: f64,
        half_deg: f64,
        n_cone: usize,
    ) -> (f64, u32) {
        let mut thick = f64::INFINITY;
        let mut near_parallel = 0u32;
        for dcone in cone_dirs(inward, half_deg, n_cone) {
            let dir = scale(dcone, r);
            let (hits, np) = self.collect_hits(c, dir, 1.0);
            near_parallel = near_parallel.saturating_add(np);
            let mut dmin = f64::INFINITY;
            for (tv, face) in hits {
                let dd = tv * r;
                // `collect_hits` yields GLOBAL face IDs (BVH leaf order carries
                // them even for a narrowed view), which is what `mesh.normal`
                // is indexed by.
                let leaving = dot(dcone, self.mesh.normal(face as usize)) > 0.0;
                if leaving && dd < dmin {
                    dmin = dd;
                }
            }
            if dmin < thick {
                thick = dmin;
            }
        }
        // `thick` is a MINIMUM over cone rays, so a skipped crossing can only
        // leave it too LARGE — and a too-large thickness is the permissive
        // direction for `t < r - eps` breach test above. The tally is what
        // stops that from being silent.
        (thick, near_parallel)
    }

    fn breach_one(
        &self,
        c: V3,
        r: f64,
        eps_mm: f64,
        half_deg: f64,
        n_cone: usize,
    ) -> (bool, f64, f64, u32) {
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
            // No reach: no cone ray is cast, so no face was even evaluated.
            return (false, f64::INFINITY, 0.0, 0);
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
        let (t1, np1) = self.cone_thickness(c, r, inward, nd, half_deg, n_cone);

        // measured-tie detection (Amendment A15): opposing-normal face at dist nd
        let mut t2 = f64::INFINITY;
        let mut np2 = 0u32;
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
                    let (t2v, np2v) = self.cone_thickness(c, r, inw2, nd, half_deg, n_cone);
                    t2 = t2v;
                    np2 = np2v;
                    tie = true;
                    break;
                }
            }
        }

        // Every cone ray of both orientations charges the sphere's row.
        let near_parallel = np1.saturating_add(np2);

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
            return (false, t1, 0.0, near_parallel);
        }
        // breaching orientation's thickness; smaller finite when both breach
        let thick = if breach1 && breach2 {
            t1.min(t2)
        } else if breach1 {
            t1
        } else {
            t2
        };
        (true, thick, r - thick, near_parallel)
    }

    /// Batched sphere-vs-wall breach test, **with the per-sphere uncertainty
    /// tally**.
    ///
    /// Replaces the retired `sphere_breach`, whose `(breached, thickness,
    /// protrusion)` triple could not say that a cone ray had reached a face
    /// whose crossing the arithmetic could not decide. That matters here in one
    /// direction only, and it is the dangerous one: `cone_thickness` takes a
    /// MINIMUM over crossings, so a skipped crossing leaves the reported
    /// thickness too LARGE, and too large reads as "no breach".
    pub fn sphere_breach_classified(
        &self,
        centers: &[V3],
        radii: &[f64],
        eps_mm: f64,
        half_deg: f64,
        n_cone: usize,
    ) -> Result<SphereBreachClassified, BatchInputError> {
        let n = centers.len();
        BatchInputError::require_len("sphere_breach", "radii", n, radii.len())?;
        let res: Vec<(bool, f64, f64, u32)> = par::run(|| {
            (0..n)
                .into_par_iter()
                .map(|i| self.breach_one(centers[i], radii[i], eps_mm, half_deg, n_cone))
                .collect()
        });
        let mut br = Vec::with_capacity(n);
        let mut th = Vec::with_capacity(n);
        let mut po = Vec::with_capacity(n);
        let mut near_parallel = Vec::with_capacity(n);
        for (b, t, p, np) in res {
            br.push(b);
            th.push(t);
            po.push(p);
            near_parallel.push(np);
        }
        Ok(SphereBreachClassified {
            breached: br,
            thickness_mm: th,
            protrusion_mm: po,
            near_parallel,
        })
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
    let (cos_th, sin_th) = deg_cos_sin(half_deg);
    let mut dirs = vec![axis];
    for k in 0..n {
        let (cos_ph, sin_ph) = turn_cos_sin(k, n);
        let d = add(
            scale(axis, cos_th),
            scale(add(scale(e1, cos_ph), scale(e2, sin_ph)), sin_th),
        );
        let nd = norm(d);
        dirs.push(scale(d, 1.0 / nd));
    }
    dirs
}

// ---- exact trig at the cardinal angles ------------------------------------

/// Cosine and sine of the azimuth `2*PI*k/n`, EXACT at the four quarter turns.
///
/// `f64::cos(std::f64::consts::PI / 2.0)` is `6.123233995736766e-17`, not zero,
/// and the library is not wrong: `PI / 2.0` is not the real quarter turn, so
/// the answer is the correct cosine of a slightly different angle. The residue
/// is ours, introduced by asking the question in radians.
///
/// It matters because a ring of rays built this way is never exactly axis
/// aligned, and a ray that should be EXACTLY parallel to an axis-aligned face
/// instead meets it at an angle of about 1e-17 radians. That lands in
/// `ray_triangle_classified`'s undecidable band — `docs/specs/geometry-canon.md`
/// §1 OP-2 wants a prune there, and gets a manufactured uncertainty instead.
/// `tests/near_wall_thin_feature.rs` measured six such rays on a plain
/// axis-aligned box and asserts zero of them now.
///
/// The quarter turn is selected by INTEGER arithmetic on `k` and `n`, never by
/// dividing first and asking whether the quotient came out whole — see
/// [`deg_cos_sin`], where that shortcut is wrong. Away from the cardinals
/// nothing is representable anyway and the library function is used unchanged,
/// so no non-cardinal direction moves.
fn turn_cos_sin(k: usize, n: usize) -> (f64, f64) {
    // `k` is a cardinal azimuth exactly when `n` divides `4 * k` — but forming
    // `4 * k` is what makes that test fail for large `n`, so divide instead.
    // With `d = gcd(n, 4)`, write `n = d * n2` and `4 = d * m`; then
    // `n | 4k  <=>  n2 | m * k  <=>  n2 | k`, because `gcd(n2, m) = 1`. The
    // quotient `4k/n` is `m * (k / n2)`, and only its residue mod 4 is wanted,
    // so it is reduced before multiplying and nothing can overflow.
    if n != 0 {
        let d = if n.is_multiple_of(4) {
            4
        } else if n.is_multiple_of(2) {
            2
        } else {
            1
        };
        let n2 = n / d;
        if k.is_multiple_of(n2) {
            let quarters = ((4 / d) % 4) * ((k / n2) % 4) % 4;
            return match quarters {
                0 => (1.0, 0.0),
                1 => (0.0, 1.0),
                2 => (-1.0, 0.0),
                _ => (0.0, -1.0),
            };
        }
    }
    let ph = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
    (ph.cos(), ph.sin())
}

/// Cosine and sine of an angle given in DEGREES, exact at every multiple of 90.
///
/// Same defect and same remedy as [`turn_cos_sin`], reached from the other
/// direction: a half-angle of exactly 90 degrees is a right angle whose cosine
/// is zero, and `90.0_f64.to_radians().cos()` is `6.123233995736766e-17`.
///
/// # The test is divisibility in DEGREES, not a whole quotient
///
/// Asking whether `deg / 90.0` came out whole is a DIFFERENT question, and at
/// large magnitudes it answers yes for angles that are nothing like a right
/// angle: `deg = 39999999992000128.0` leaves a remainder of 88 degrees, yet the
/// quotient rounds to an exact integer and a `fract() == 0.0` test on it passes.
/// That would return an exact cardinal direction for a non-cardinal angle —
/// the opposite of this function's purpose, and a wrong answer rather than an
/// imprecise one.
///
/// So the whole-number and range checks come first, on `deg` itself, and the
/// divisibility is then decided in `i64` where it is exact. Below `1e15` every
/// integer is exactly representable in an `f64`, so the cast cannot lose a
/// digit; above it, the fallback is taken and nothing is claimed.
#[expect(
    clippy::cast_possible_truncation,
    reason = "guarded: the cast runs only when `deg` is a whole number below 1e15, where every \
              integer is exactly representable in f64 and the truncation cannot lose a digit"
)]
fn deg_cos_sin(deg: f64) -> (f64, f64) {
    if deg.fract() == 0.0 && deg.abs() < 1e15 {
        let whole = deg as i64;
        if whole % 90 == 0 {
            return match (whole / 90).rem_euclid(4) {
                0 => (1.0, 0.0),
                1 => (0.0, 1.0),
                2 => (-1.0, 0.0),
                _ => (0.0, -1.0),
            };
        }
    }
    let rad = deg.to_radians();
    (rad.cos(), rad.sin())
}

#[cfg(test)]
mod cardinal_trig_tests {
    use super::{deg_cos_sin, turn_cos_sin};

    /// The quarter turns are EXACT, which is the whole reason these helpers
    /// exist: `(2.0 * PI * 1.0 / 4.0).cos()` is `6.123233995736766e-17`.
    #[test]
    fn every_quarter_turn_is_exact_and_the_rest_is_unchanged() {
        assert_eq!(turn_cos_sin(0, 4), (1.0, 0.0));
        assert_eq!(turn_cos_sin(1, 4), (0.0, 1.0));
        assert_eq!(turn_cos_sin(2, 4), (-1.0, 0.0));
        assert_eq!(turn_cos_sin(3, 4), (0.0, -1.0));
        // A ring of 12 hits the cardinals at k = 0, 3, 6, 9 and nowhere else.
        assert_eq!(turn_cos_sin(3, 12), (0.0, 1.0));
        assert_eq!(turn_cos_sin(9, 12), (0.0, -1.0));

        // Everything OFF a cardinal must be bit-identical to the library call
        // the code made before, so no non-cardinal ray moves.
        for (k, n) in [(1_usize, 12_usize), (5, 12), (1, 5), (3, 7), (17, 360)] {
            let ph = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
            assert_eq!(turn_cos_sin(k, n), (ph.cos(), ph.sin()), "k={k} n={n}");
        }

        // A true cardinal must be found even when `4 * k` would overflow: with
        // n = 2^63 and k = 3 * 2^61 the angle is exactly three quarter turns.
        assert_eq!(turn_cos_sin(3 << 61, 1 << 63), (0.0, -1.0));
        assert_eq!(turn_cos_sin(1 << 61, 1 << 63), (0.0, 1.0));
        assert_eq!(turn_cos_sin(1 << 62, 1 << 63), (-1.0, 0.0));
        // A half turn on a ring whose size is not divisible by four.
        assert_eq!(turn_cos_sin(3, 6), (-1.0, 0.0));
        assert_eq!(turn_cos_sin(5, 10), (-1.0, 0.0));
        // And a non-cardinal at a magnitude that would overflow must NOT be
        // claimed as exact.
        // n = 5 is chosen because (2^63 - 1) mod 5 = 2: this k is genuinely NOT a
        // cardinal. With n = 7 it would be, since 2^63 = 1 (mod 7).
        let huge = usize::MAX / 2;
        let ph = 2.0 * std::f64::consts::PI * (huge as f64) / (5.0_f64);
        assert_eq!(turn_cos_sin(huge, 5), (ph.cos(), ph.sin()));
        // ...and where it IS a cardinal, it must be claimed: (2^63 - 1) is a
        // multiple of 7, so that angle is a whole number of turns.
        assert_eq!(turn_cos_sin(huge, 7), (1.0, 0.0));
    }

    /// Divisibility is decided in DEGREES. Dividing first and asking whether
    /// the quotient came out whole is a different question that answers YES for
    /// angles nowhere near a right angle.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "the exactness IS the assertion: these two lines pin that the counterexample is \
                  88 degrees off a cardinal AND that dividing by 90 nevertheless yields a whole \
                  quotient, which is the whole reason the divisibility test cannot be written that \
                  way. An approximate comparison here would assert nothing."
    )]
    fn the_cardinal_test_is_divisibility_and_not_a_whole_quotient() {
        assert_eq!(deg_cos_sin(0.0), (1.0, 0.0));
        assert_eq!(deg_cos_sin(90.0), (0.0, 1.0));
        assert_eq!(deg_cos_sin(180.0), (-1.0, 0.0));
        assert_eq!(deg_cos_sin(270.0), (0.0, -1.0));
        assert_eq!(deg_cos_sin(-90.0), (0.0, -1.0));
        assert_eq!(deg_cos_sin(360.0), (1.0, 0.0));

        // THE COUNTEREXAMPLE. `deg / 90.0` is an exact integer here, and the
        // angle is 88 degrees away from a cardinal. It must take the fallback.
        let liar = 39_999_999_992_000_128.0_f64;
        assert_eq!(liar % 360.0, 88.0, "the fixture is 88 degrees off cardinal");
        assert_eq!((liar / 90.0).fract(), 0.0, "and the quotient reads whole");
        let rad = liar.to_radians();
        assert_eq!(deg_cos_sin(liar), (rad.cos(), rad.sin()));

        // Ordinary non-cardinal angles are untouched.
        for deg in [1.0_f64, 30.0, 45.0, 89.9, 91.0, 137.5] {
            let rad = deg.to_radians();
            assert_eq!(deg_cos_sin(deg), (rad.cos(), rad.sin()), "deg={deg}");
        }
    }
}

/// Kc: the faster segment-to-surface kernel against verbatim copies of the one
/// it replaced (bit-exact distance, witness, tally, stop flag and winning face).
#[cfg(all(test, feature = "testkit"))]
#[path = "kernel_speed_tests.rs"]
mod kernel_speed_tests;
