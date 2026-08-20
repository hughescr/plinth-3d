//! Sealed data contract for later routed-support coverage measurement.
//!
//! This module deliberately records neither a coverage algorithm nor an
//! acceptance judgment.  It is the immutable boundary between the verified
//! support snapshot and a future verifier-owned assessment.

use plinth_core::{Finite, PositiveFinite};
use serde::Serialize;
use thiserror::Error;

/// Checked resource ceilings for one coverage measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoverageMeasurementLimits {
    max_route_memberships: usize,
    max_indexed_points: usize,
    max_query_rows: usize,
    max_distance_work: usize,
    max_logical_peak_bytes: usize,
    max_assessment_work: usize,
    max_route_knn_peak_bytes: usize,
}

impl CoverageMeasurementLimits {
    /// All ceilings are explicit and must be nonzero.
    pub fn new(
        max_route_memberships: usize,
        max_indexed_points: usize,
        max_query_rows: usize,
        max_distance_work: usize,
        max_logical_peak_bytes: usize,
        max_assessment_work: usize,
    ) -> Result<Self, CoverageMeasurementError> {
        let limits = Self {
            max_route_memberships,
            max_indexed_points,
            max_query_rows,
            max_distance_work,
            max_logical_peak_bytes,
            max_assessment_work,
            // Absent an explicit owner for the route kNN batch's own peak, the
            // measurement's retained-peak ceiling stands in for it. That is the
            // behavior this constructor has always had; `with_route_knn_peak_bytes`
            // is how a caller that *has* a registered owner names it.
            max_route_knn_peak_bytes: max_logical_peak_bytes,
        };
        if limits.max_route_memberships == 0
            || limits.max_indexed_points == 0
            || limits.max_query_rows == 0
            || limits.max_distance_work == 0
            || limits.max_logical_peak_bytes == 0
            || limits.max_assessment_work == 0
        {
            return Err(CoverageMeasurementError::InvalidLimits);
        }
        Ok(limits)
    }

    /// Names the registered owner of the coverage-route kNN batch's own
    /// logical peak, which is a *different* cap from this measurement's
    /// retained-peak ceiling.
    ///
    /// The two answer different questions. `max_logical_peak_bytes` bounds the
    /// whole measurement's retained peak — the nearest vector, the normalized
    /// route projection, and whichever of the route or assessment phase peaks
    /// is larger. The kNN batch's own ceiling bounds one summand of that, and
    /// in production it has its own registered owner
    /// (`coverage_route_knn.max_logical_peak_bytes`), which is not the
    /// specialized stage's coverage scratch ceiling. Passing the scratch
    /// ceiling to the kNN batch made one cap silently do two jobs.
    ///
    /// # Errors
    ///
    /// Returns [`CoverageMeasurementError::InvalidLimits`] if the ceiling is zero.
    pub const fn with_route_knn_peak_bytes(
        self,
        max_route_knn_peak_bytes: usize,
    ) -> Result<Self, CoverageMeasurementError> {
        if max_route_knn_peak_bytes == 0 {
            return Err(CoverageMeasurementError::InvalidLimits);
        }
        Ok(Self {
            max_route_knn_peak_bytes,
            ..self
        })
    }

    #[must_use]
    pub const fn max_route_memberships(self) -> usize {
        self.max_route_memberships
    }
    #[must_use]
    pub const fn max_indexed_points(self) -> usize {
        self.max_indexed_points
    }
    #[must_use]
    pub const fn max_query_rows(self) -> usize {
        self.max_query_rows
    }
    #[must_use]
    pub const fn max_distance_work(self) -> usize {
        self.max_distance_work
    }
    #[must_use]
    pub const fn max_logical_peak_bytes(self) -> usize {
        self.max_logical_peak_bytes
    }
    #[must_use]
    pub const fn max_assessment_work(self) -> usize {
        self.max_assessment_work
    }
    /// The coverage-route kNN batch's own logical-peak ceiling. Equal to
    /// [`Self::max_logical_peak_bytes`] unless a caller named its registered
    /// owner through [`Self::with_route_knn_peak_bytes`].
    #[must_use]
    pub const fn max_route_knn_peak_bytes(self) -> usize {
        self.max_route_knn_peak_bytes
    }
}

/// Caller-selected geometric thresholds and the explicit work ceilings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoverageMeasurementRequest {
    radius_mm: PositiveFinite,
    patch_area_floor_mm2: PositiveFinite,
    limits: CoverageMeasurementLimits,
}

impl CoverageMeasurementRequest {
    /// Checked convenience constructor at the public floating-point boundary.
    pub fn from_f64(
        radius_mm: f64,
        patch_area_floor_mm2: f64,
        limits: CoverageMeasurementLimits,
    ) -> Result<Self, CoverageMeasurementError> {
        Ok(Self::new(
            PositiveFinite::new(radius_mm).map_err(|_| CoverageMeasurementError::InvalidRequest)?,
            PositiveFinite::new(patch_area_floor_mm2)
                .map_err(|_| CoverageMeasurementError::InvalidRequest)?,
            limits,
        ))
    }

    #[must_use]
    pub const fn new(
        radius_mm: PositiveFinite,
        patch_area_floor_mm2: PositiveFinite,
        limits: CoverageMeasurementLimits,
    ) -> Self {
        Self {
            radius_mm,
            patch_area_floor_mm2,
            limits,
        }
    }

    #[must_use]
    pub const fn radius_mm(self) -> PositiveFinite {
        self.radius_mm
    }
    #[must_use]
    pub const fn patch_area_floor_mm2(self) -> PositiveFinite {
        self.patch_area_floor_mm2
    }
    #[must_use]
    pub const fn limits(self) -> CoverageMeasurementLimits {
        self.limits
    }
}

/// A worst routed-contact gap, with an explicit state for no available route.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "state", content = "gap_mm", rename_all = "snake_case")]
pub enum CoverageGap {
    Finite(Finite),
    NoRoutedContact,
}

/// Deterministic source-island evidence retained for a later report.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverageMeasurementIslandEvidence {
    island_id: u32,
    component_id: u32,
    representative_face_id: u32,
    sorted_face_ids: Box<[u32]>,
}

impl CoverageMeasurementIslandEvidence {
    #[must_use]
    pub const fn island_id(&self) -> u32 {
        self.island_id
    }
    #[must_use]
    pub const fn component_id(&self) -> u32 {
        self.component_id
    }
    #[must_use]
    pub const fn representative_face_id(&self) -> u32 {
        self.representative_face_id
    }
    #[must_use]
    pub fn sorted_face_ids(&self) -> &[u32] {
        &self.sorted_face_ids
    }

    #[allow(dead_code, reason = "reserved for the future verifier implementation")]
    pub(crate) fn new(
        island_id: u32,
        component_id: u32,
        representative_face_id: u32,
        sorted_face_ids: Box<[u32]>,
    ) -> Self {
        Self {
            island_id,
            component_id,
            representative_face_id,
            sorted_face_ids,
        }
    }
}

/// Deterministic uncovered-patch evidence retained for a later report.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverageMeasurementPatchEvidence {
    island_id: u32,
    component_id: u32,
    representative_face_id: u32,
    sorted_face_ids: Box<[u32]>,
    uncovered_area_mm2: Finite,
    cut_ambiguous: bool,
    assessment_failed: bool,
    worst_gap: CoverageGap,
}

impl CoverageMeasurementPatchEvidence {
    #[must_use]
    pub const fn island_id(&self) -> u32 {
        self.island_id
    }
    #[must_use]
    pub const fn component_id(&self) -> u32 {
        self.component_id
    }
    #[must_use]
    pub const fn representative_face_id(&self) -> u32 {
        self.representative_face_id
    }
    #[must_use]
    pub fn sorted_face_ids(&self) -> &[u32] {
        &self.sorted_face_ids
    }
    #[must_use]
    pub const fn uncovered_area_mm2(&self) -> Finite {
        self.uncovered_area_mm2
    }
    #[must_use]
    pub const fn cut_ambiguous(&self) -> bool {
        self.cut_ambiguous
    }
    #[must_use]
    pub const fn assessment_failed(&self) -> bool {
        self.assessment_failed
    }
    #[must_use]
    pub const fn worst_gap(&self) -> CoverageGap {
        self.worst_gap
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(dead_code, reason = "reserved for the future verifier implementation")]
    pub(crate) fn new(
        island_id: u32,
        component_id: u32,
        representative_face_id: u32,
        sorted_face_ids: Box<[u32]>,
        uncovered_area_mm2: Finite,
        cut_ambiguous: bool,
        assessment_failed: bool,
        worst_gap: CoverageGap,
    ) -> Self {
        Self {
            island_id,
            component_id,
            representative_face_id,
            sorted_face_ids,
            uncovered_area_mm2,
            cut_ambiguous,
            assessment_failed,
            worst_gap,
        }
    }
}

/// Immutable report evidence. Constructors remain verifier-private.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverageMeasurementEvidence {
    islands: Box<[CoverageMeasurementIslandEvidence]>,
    patches: Box<[CoverageMeasurementPatchEvidence]>,
}

impl CoverageMeasurementEvidence {
    #[must_use]
    pub fn islands(&self) -> &[CoverageMeasurementIslandEvidence] {
        &self.islands
    }
    #[must_use]
    pub fn patches(&self) -> &[CoverageMeasurementPatchEvidence] {
        &self.patches
    }

    #[allow(dead_code, reason = "reserved for the future verifier implementation")]
    pub(crate) fn new(
        islands: Box<[CoverageMeasurementIslandEvidence]>,
        patches: Box<[CoverageMeasurementPatchEvidence]>,
    ) -> Self {
        Self { islands, patches }
    }
}

/// Exact realized resource use; sums and sequential high-water values are distinct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoverageMeasurementResourcePlan {
    route_membership_sum: usize,
    sequential_route_membership_peak: usize,
    indexed_point_count: usize,
    sequential_indexed_point_peak: usize,
    query_row_count: usize,
    distance_work: usize,
    logical_peak_bytes: usize,
    assessment_work: usize,
    assessment_scratch_bytes: usize,
    assessment_evidence_bytes: usize,
    assessment_phase_peak_bytes: usize,
}

/// Verifier-private realized resources supplied as one typed summary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CoverageMeasurementResourceSummary {
    pub(crate) route_membership_sum: usize,
    pub(crate) sequential_route_membership_peak: usize,
    pub(crate) indexed_point_count: usize,
    pub(crate) sequential_indexed_point_peak: usize,
    pub(crate) query_row_count: usize,
    pub(crate) distance_work: usize,
    pub(crate) logical_peak_bytes: usize,
    pub(crate) assessment_work: usize,
    pub(crate) assessment_scratch_bytes: usize,
    pub(crate) assessment_evidence_bytes: usize,
    pub(crate) assessment_phase_peak_bytes: usize,
}

impl CoverageMeasurementResourcePlan {
    #[must_use]
    pub const fn route_membership_sum(self) -> usize {
        self.route_membership_sum
    }
    #[must_use]
    pub const fn sequential_route_membership_peak(self) -> usize {
        self.sequential_route_membership_peak
    }
    #[must_use]
    pub const fn indexed_point_count(self) -> usize {
        self.indexed_point_count
    }
    #[must_use]
    pub const fn sequential_indexed_point_peak(self) -> usize {
        self.sequential_indexed_point_peak
    }
    #[must_use]
    pub const fn query_row_count(self) -> usize {
        self.query_row_count
    }
    #[must_use]
    pub const fn distance_work(self) -> usize {
        self.distance_work
    }
    #[must_use]
    pub const fn logical_peak_bytes(self) -> usize {
        self.logical_peak_bytes
    }
    #[must_use]
    pub const fn assessment_work(self) -> usize {
        self.assessment_work
    }
    /// Scratch storage retained while assessing coverage evidence.
    #[must_use]
    pub const fn assessment_scratch_bytes(self) -> usize {
        self.assessment_scratch_bytes
    }
    /// Immutable evidence storage realized by the assessment.
    #[must_use]
    pub const fn assessment_evidence_bytes(self) -> usize {
        self.assessment_evidence_bytes
    }
    /// High-water storage for the assessment phase.
    #[must_use]
    pub const fn assessment_phase_peak_bytes(self) -> usize {
        self.assessment_phase_peak_bytes
    }
    pub(crate) const fn from_summary(summary: CoverageMeasurementResourceSummary) -> Self {
        Self {
            route_membership_sum: summary.route_membership_sum,
            sequential_route_membership_peak: summary.sequential_route_membership_peak,
            indexed_point_count: summary.indexed_point_count,
            sequential_indexed_point_peak: summary.sequential_indexed_point_peak,
            query_row_count: summary.query_row_count,
            distance_work: summary.distance_work,
            logical_peak_bytes: summary.logical_peak_bytes,
            assessment_work: summary.assessment_work,
            assessment_scratch_bytes: summary.assessment_scratch_bytes,
            assessment_evidence_bytes: summary.assessment_evidence_bytes,
            assessment_phase_peak_bytes: summary.assessment_phase_peak_bytes,
        }
    }
}

/// Immutable coverage measurement bound to one sealed verification snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverageMeasurement {
    snapshot_digest: u64,
    request: CoverageMeasurementRequest,
    resource_plan: CoverageMeasurementResourcePlan,
    evidence: CoverageMeasurementEvidence,
    uncovered_sample_indices: Vec<usize>,
    objective: crate::CoverageObjective,
    uncensored_uncovered_mm2: f64,
}

impl CoverageMeasurement {
    #[must_use]
    pub const fn snapshot_digest(&self) -> u64 {
        self.snapshot_digest
    }
    #[must_use]
    pub const fn request(&self) -> CoverageMeasurementRequest {
        self.request
    }
    #[must_use]
    pub const fn resource_plan(&self) -> CoverageMeasurementResourcePlan {
        self.resource_plan
    }
    #[must_use]
    pub const fn evidence(&self) -> &CoverageMeasurementEvidence {
        &self.evidence
    }

    /// Every coverage sample this measurement found uncovered — nearest
    /// routed contact farther than the request radius — as ascending indices
    /// into the measured snapshot's own `coverage_samples()` slice.
    ///
    /// This is the measurement's answer to "which samples still need
    /// covering", and it is the ONLY admissible source for that question:
    /// re-deriving it beside the measurement would be a second, independently
    /// drifting definition of coverage. The coverage-closure controller
    /// (FU-56) scores its repair-row candidates against exactly these rows,
    /// never against every sample on a blocking patch's faces — a face can be
    /// partially covered, and counting its covered area would rank a row
    /// beside the covered region above the row that closes the residual gap.
    ///
    /// It costs no new allocation and no new charge: see
    /// `CoverageAssessment::uncovered_sample_indices`.
    #[must_use]
    pub fn uncovered_sample_indices(&self) -> &[usize] {
        &self.uncovered_sample_indices
    }

    /// **SPEC-A4 §6 clause 1's own quantity, no longer discarded.**
    ///
    /// The assessment behind every measurement already computes this
    /// lexicographic four-component objective — total failing area, largest
    /// failing patch, failing-patch count, worst gap — and until now
    /// `measure_coverage` threw it away and kept only the patches. The A4
    /// controller's accept test IS this key strictly improving, so it is
    /// retained rather than recomputed beside the measurement: a second
    /// derivation is a second definition, and the two would drift.
    ///
    /// **Censored**, exactly as §4 and §6 say: a patch is "failing" only above
    /// the 0.16 mm² A3 censor or on an ambiguous cut. Costs one struct copy.
    #[must_use]
    pub const fn objective(&self) -> crate::CoverageObjective {
        self.objective
    }

    /// **SPEC-A4 §B.5 clause 3's quantity: total UNCENSORED uncovered sample
    /// area, in mm², over every route.**
    ///
    /// The sum of the area weights of every sample this measurement found
    /// uncovered — no censor, no patch grouping, no island identity. That is
    /// what makes it Goodhart-robust and it is why the sum is taken over
    /// SAMPLES rather than over patch areas: the sample set and its weights are
    /// frozen at attempt 0 for the whole run, so a repair can change only WHICH
    /// samples are uncovered. It cannot split a patch to move this number, it
    /// cannot push area under the censor to move this number, and it cannot
    /// relabel an island to move this number. The only way down is to cover a
    /// sample.
    ///
    /// (The patch-area sum agrees with it to floating-point noise, because
    /// patches partition the uncovered samples. The sample sum is the one
    /// implemented because it is the one the clause names.)
    #[must_use]
    pub const fn uncensored_uncovered_mm2(&self) -> f64 {
        self.uncensored_uncovered_mm2
    }

    #[allow(dead_code, reason = "reserved for the future verifier implementation")]
    pub(crate) const fn new(
        snapshot_digest: u64,
        request: CoverageMeasurementRequest,
        resource_plan: CoverageMeasurementResourcePlan,
        evidence: CoverageMeasurementEvidence,
        uncovered_sample_indices: Vec<usize>,
        objective: crate::CoverageObjective,
        uncensored_uncovered_mm2: f64,
    ) -> Self {
        Self {
            snapshot_digest,
            request,
            resource_plan,
            evidence,
            uncovered_sample_indices,
            objective,
            uncensored_uncovered_mm2,
        }
    }
}

/// Failures a later measurement implementation can expose without importing verdict vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum CoverageMeasurementError {
    #[error("coverage measurement limits must all be nonzero")]
    InvalidLimits,
    #[error("coverage measurement request is invalid")]
    InvalidRequest,
    #[error("verification snapshot binding is invalid")]
    SnapshotBinding,
    #[error("coverage measurement resource limit exceeded: {resource}")]
    ResourceLimit { resource: &'static str },
    #[error("coverage measurement arithmetic overflow: {operation}")]
    ArithmeticOverflow { operation: &'static str },
    #[error("geometry kernel refused coverage measurement: {operation}")]
    KernelRefusal { operation: &'static str },
    #[error("coverage assessment was refused: {reason}")]
    AssessmentRefusal { reason: &'static str },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> CoverageMeasurementLimits {
        CoverageMeasurementLimits::new(1, 2, 3, 4, 5, 6).expect("nonzero limits")
    }

    #[test]
    fn limits_and_request_reject_invalid_numeric_boundaries() {
        assert_eq!(
            CoverageMeasurementLimits::new(0, 1, 1, 1, 1, 1),
            Err(CoverageMeasurementError::InvalidLimits)
        );
        assert_eq!(
            CoverageMeasurementRequest::from_f64(f64::NAN, 1.0, limits()),
            Err(CoverageMeasurementError::InvalidRequest)
        );
        assert_eq!(
            CoverageMeasurementRequest::from_f64(0.2, f64::INFINITY, limits()),
            Err(CoverageMeasurementError::InvalidRequest)
        );
        let request = CoverageMeasurementRequest::new(
            PositiveFinite::new(0.2).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            limits(),
        );
        assert_eq!(request.radius_mm().get(), 0.2);
        assert_eq!(request.patch_area_floor_mm2().get(), 1.0);
    }

    #[test]
    fn overflow_error_is_typed_for_internal_measurement_arithmetic() {
        assert_eq!(
            CoverageMeasurementError::ArithmeticOverflow {
                operation: "distance work"
            }
            .to_string(),
            "coverage measurement arithmetic overflow: distance work"
        );
    }

    #[test]
    fn finite_and_no_route_gaps_remain_distinct_on_the_wire() {
        let finite = CoverageGap::Finite(Finite::new(1.25).unwrap());
        assert_eq!(format!("{finite:?}"), "Finite(Finite(1.25))");
        assert_eq!(
            serde_json::to_string(&finite).unwrap(),
            r#"{"state":"finite","gap_mm":1.25}"#
        );
        assert_eq!(
            serde_json::to_string(&CoverageGap::NoRoutedContact).unwrap(),
            r#"{"state":"no_routed_contact"}"#
        );
    }
}
