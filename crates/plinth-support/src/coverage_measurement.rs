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

    #[allow(dead_code, reason = "reserved for the future verifier implementation")]
    pub(crate) const fn new(
        snapshot_digest: u64,
        request: CoverageMeasurementRequest,
        resource_plan: CoverageMeasurementResourcePlan,
        evidence: CoverageMeasurementEvidence,
    ) -> Self {
        Self {
            snapshot_digest,
            request,
            resource_plan,
            evidence,
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
