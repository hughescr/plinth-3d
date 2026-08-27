//! Disk-first ChituBox container verification.
//!
//! This crate judges precisely the bytes freshly read from the supplied path.
//! It has no producer state, cache, mutation path, logging side effect, or
//! repair behavior. A structural pass is intentionally not a claim that the
//! ChituBox GUI will load the artifact.

#![forbid(unsafe_code)]

use std::path::Path;

use chitubox_io::{ChituboxError, Project};
use plinth_core::{ArtifactId, Defect, Finite, Severity, SiteId, ViolationKind};
use plinth_support::{
    CoverageGap, CoverageMeasurementError, CoverageMeasurementRequest, SupportVerificationError,
    SupportVerificationSnapshot,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A fail-closed coverage-verification result, bound to one sealed support
/// snapshot and one explicit measurement request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoverageReport {
    artifact: ArtifactId,
    snapshot_digest: u64,
    request: CoverageRequestRecord,
    resources: CoverageResourceRecord,
    evidence: CoverageEvidenceRecord,
    defects: Vec<Defect>,
}

impl CoverageReport {
    #[must_use]
    pub fn artifact(&self) -> &ArtifactId {
        &self.artifact
    }

    #[must_use]
    pub const fn snapshot_digest(&self) -> u64 {
        self.snapshot_digest
    }

    #[must_use]
    pub fn defects(&self) -> &[Defect] {
        &self.defects
    }

    /// Coverage acceptance has no waiver, score, or producer-side override.
    #[must_use]
    pub fn accepted(&self) -> bool {
        self.defects.is_empty()
    }
}

/// A measurement failure prevents construction of any coverage report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Error)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum CoverageVerificationError {
    #[error("sealed support snapshot integrity refused coverage verification")]
    SnapshotIntegrity,
    #[error("coverage measurement refused due to invalid snapshot binding")]
    SnapshotBinding,
    #[error("coverage measurement exceeded resource limit: {resource}")]
    ResourceLimit { resource: String },
    #[error("coverage measurement arithmetic overflow: {operation}")]
    ArithmeticOverflow { operation: String },
    #[error("coverage measurement kernel refusal: {operation}")]
    KernelRefusal { operation: String },
    #[error("coverage measurement assessment refusal: {reason}")]
    AssessmentRefusal { reason: String },
    #[error("coverage verifier arithmetic overflow while summing island area")]
    DefectMeasureOverflow,
    #[error("coverage verifier produced a non-finite island defect measure")]
    NonFiniteDefectMeasure,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct CoverageRequestRecord {
    radius_mm: Finite,
    patch_area_floor_mm2: Finite,
    max_route_memberships: usize,
    max_indexed_points: usize,
    max_query_rows: usize,
    max_distance_work: usize,
    max_logical_peak_bytes: usize,
    max_assessment_work: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct CoverageResourceRecord {
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct CoverageEvidenceRecord {
    islands: Vec<CoverageIslandEvidenceRecord>,
    patches: Vec<CoveragePatchEvidenceRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct CoverageIslandEvidenceRecord {
    island_id: u32,
    component_id: u32,
    representative_face_id: u32,
    sorted_face_ids: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct CoveragePatchEvidenceRecord {
    island_id: u32,
    component_id: u32,
    representative_face_id: u32,
    sorted_face_ids: Vec<u32>,
    uncovered_area_mm2: Finite,
    cut_ambiguous: bool,
    assessment_failed: bool,
    worst_gap: CoverageGapRecord,
}

/// The no-route condition remains evidence; it is never used as a defect
/// measure, which is always a finite uncovered-area sum.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "gap_mm", rename_all = "snake_case")]
enum CoverageGapRecord {
    Finite(Finite),
    NoRoutedContact,
}

/// Measure sealed support coverage once, then make the verifier-owned verdict.
///
/// The report contains a value projection of the realized measurement only;
/// it never exposes snapshot route authority or provides a mutation path.
pub fn verify_coverage(
    artifact: ArtifactId,
    snapshot: &SupportVerificationSnapshot<'_>,
    request: CoverageMeasurementRequest,
) -> Result<CoverageReport, CoverageVerificationError> {
    snapshot.validate_integrity().map_err(map_snapshot_error)?;
    let measurement = snapshot
        .measure_coverage(request)
        .map_err(map_measurement_error)?;
    if measurement.snapshot_digest() != snapshot.digest() || measurement.request() != request {
        return Err(CoverageVerificationError::SnapshotBinding);
    }

    coverage_report_from_measurement_inputs(
        artifact,
        measurement.snapshot_digest(),
        request_record(request),
        measurement.resource_plan(),
        measurement.evidence().islands(),
        measurement.evidence().patches(),
    )
}

/// Projects immutable support-measurement evidence into the verifier-owned
/// report and verdict.  This remains private: callers may only supply live
/// sealed measurement output through [`verify_coverage`].
fn coverage_report_from_measurement_inputs(
    artifact: ArtifactId,
    snapshot_digest: u64,
    request: CoverageRequestRecord,
    plan: plinth_support::CoverageMeasurementResourcePlan,
    measured_islands: &[plinth_support::CoverageMeasurementIslandEvidence],
    measured_patches: &[plinth_support::CoverageMeasurementPatchEvidence],
) -> Result<CoverageReport, CoverageVerificationError> {
    let resources = CoverageResourceRecord {
        route_membership_sum: plan.route_membership_sum(),
        sequential_route_membership_peak: plan.sequential_route_membership_peak(),
        indexed_point_count: plan.indexed_point_count(),
        sequential_indexed_point_peak: plan.sequential_indexed_point_peak(),
        query_row_count: plan.query_row_count(),
        distance_work: plan.distance_work(),
        logical_peak_bytes: plan.logical_peak_bytes(),
        assessment_work: plan.assessment_work(),
        assessment_scratch_bytes: plan.assessment_scratch_bytes(),
        assessment_evidence_bytes: plan.assessment_evidence_bytes(),
        assessment_phase_peak_bytes: plan.assessment_phase_peak_bytes(),
    };
    let mut islands = measured_islands
        .iter()
        .map(|island| CoverageIslandEvidenceRecord {
            island_id: island.island_id(),
            component_id: island.component_id(),
            representative_face_id: island.representative_face_id(),
            sorted_face_ids: island.sorted_face_ids().to_vec(),
        })
        .collect::<Vec<_>>();
    islands.sort_unstable_by(|left, right| {
        (
            left.island_id,
            left.component_id,
            left.representative_face_id,
            &left.sorted_face_ids,
        )
            .cmp(&(
                right.island_id,
                right.component_id,
                right.representative_face_id,
                &right.sorted_face_ids,
            ))
    });
    let mut patches = measured_patches
        .iter()
        .map(|patch| CoveragePatchEvidenceRecord {
            island_id: patch.island_id(),
            component_id: patch.component_id(),
            representative_face_id: patch.representative_face_id(),
            sorted_face_ids: patch.sorted_face_ids().to_vec(),
            uncovered_area_mm2: patch.uncovered_area_mm2(),
            cut_ambiguous: patch.cut_ambiguous(),
            assessment_failed: patch.assessment_failed(),
            worst_gap: match patch.worst_gap() {
                CoverageGap::Finite(value) => CoverageGapRecord::Finite(value),
                CoverageGap::NoRoutedContact => CoverageGapRecord::NoRoutedContact,
            },
        })
        .collect::<Vec<_>>();
    patches.sort_unstable_by(|left, right| {
        (
            left.island_id,
            left.component_id,
            left.representative_face_id,
            &left.sorted_face_ids,
        )
            .cmp(&(
                right.island_id,
                right.component_id,
                right.representative_face_id,
                &right.sorted_face_ids,
            ))
    });
    coverage_report_from_projection(
        artifact,
        snapshot_digest,
        request,
        resources,
        CoverageEvidenceRecord { islands, patches },
    )
}

/// Final verifier-owned projection.  Unit tests exercise this with immutable
/// evidence records, while production reaches it only after one live
/// measurement and the projection above.
fn coverage_report_from_projection(
    artifact: ArtifactId,
    snapshot_digest: u64,
    request: CoverageRequestRecord,
    resources: CoverageResourceRecord,
    evidence: CoverageEvidenceRecord,
) -> Result<CoverageReport, CoverageVerificationError> {
    // The censor comes off the request this very report carries, so the verdict
    // is censored at exactly the area the measurement that produced the evidence
    // used. See `plinth_support::coverage_patch_blocks_verdict` for the spec text.
    let defects = defects_for_patches(&evidence.patches, request.patch_area_floor_mm2.get())?;
    Ok(CoverageReport {
        artifact,
        snapshot_digest,
        request,
        resources,
        evidence,
        defects,
    })
}

fn request_record(request: CoverageMeasurementRequest) -> CoverageRequestRecord {
    let limits = request.limits();
    CoverageRequestRecord {
        radius_mm: Finite::new(request.radius_mm().get())
            .expect("positive finite radius is finite"),
        patch_area_floor_mm2: Finite::new(request.patch_area_floor_mm2().get())
            .expect("positive finite patch floor is finite"),
        max_route_memberships: limits.max_route_memberships(),
        max_indexed_points: limits.max_indexed_points(),
        max_query_rows: limits.max_query_rows(),
        max_distance_work: limits.max_distance_work(),
        max_logical_peak_bytes: limits.max_logical_peak_bytes(),
        max_assessment_work: limits.max_assessment_work(),
    }
}

/// One blocking defect per island, measuring the summed area of that island's
/// BLOCKING patches only.
///
/// `censor_area_mm2` is the request's own `voxel_pitch_mm²`. Per
/// docs/specs/coverage-repair.md §4 it bounds EACH PATCH, so a censored patch is not
/// blocking and contributes nothing to its island's measure; an island whose
/// every patch is censored raises no defect at all. See
/// `plinth_support::coverage_patch_blocks_verdict` for the quoted spec text and
/// the reason the bound is per patch rather than per island or per model.
fn defects_for_patches(
    patches: &[CoveragePatchEvidenceRecord],
    censor_area_mm2: f64,
) -> Result<Vec<Defect>, CoverageVerificationError> {
    let mut defects = Vec::new();
    let mut start = 0;
    while start < patches.len() {
        let island_id = patches[start].island_id;
        let mut end = start;
        let mut area = 0.0_f64;
        while end < patches.len() && patches[end].island_id == island_id {
            let patch = &patches[end];
            // **One implementation, called — not a second statement of the
            // same rule.** This is a VERDICT: it decides whether the model is
            // refused, so it is censored, and the coverage-closure controller
            // (FU-56) decides when it may stop with exactly this predicate. If
            // the two ever drifted the controller would close plans this gate
            // still refuses. It used to be restated here and guarded only by
            // a frozen fixture on which all nine patches were blocking for
            // the SAME reason, so two of the predicate's three axes were
            // unguarded; calling the shared function removes the drift rather
            // than watching for it.
            //
            // The controller's REPAIR WORKLIST is a different question and
            // uses the uncensored `plinth_support::coverage_patch_needs_repair`
            // instead — censoring what to work on, rather than what to refuse,
            // was measured to leave real above-censor holes unrepaired. See
            // the call-site table on `coverage_patch_blocks_verdict`.
            if plinth_support::coverage_patch_blocks_verdict(
                patch.uncovered_area_mm2.get(),
                patch.cut_ambiguous,
                patch.assessment_failed,
                censor_area_mm2,
            ) {
                let next = area + patch.uncovered_area_mm2.get();
                if !next.is_finite() {
                    return Err(CoverageVerificationError::DefectMeasureOverflow);
                }
                area = next;
            }
            end += 1;
        }
        if area != 0.0
            || patches[start..end]
                .iter()
                .any(|patch| patch.assessment_failed || patch.cut_ambiguous)
        {
            if !area.is_finite() {
                return Err(CoverageVerificationError::NonFiniteDefectMeasure);
            }
            let detail = format!("coverage verification failed for island {island_id}; uncovered patch area sum = {area}");
            defects.push(
                Defect::new(
                    ViolationKind::UncoveredIsland,
                    SiteId::Island(island_id),
                    Severity::Blocking,
                    area,
                    detail,
                )
                .map_err(|_| CoverageVerificationError::NonFiniteDefectMeasure)?,
            );
        }
        start = end;
    }
    Ok(defects)
}

fn map_snapshot_error(_error: SupportVerificationError) -> CoverageVerificationError {
    CoverageVerificationError::SnapshotIntegrity
}

fn map_measurement_error(error: CoverageMeasurementError) -> CoverageVerificationError {
    match error {
        CoverageMeasurementError::SnapshotBinding => CoverageVerificationError::SnapshotBinding,
        CoverageMeasurementError::ResourceLimit { resource } => {
            CoverageVerificationError::ResourceLimit {
                resource: resource.to_owned(),
            }
        }
        CoverageMeasurementError::ArithmeticOverflow { operation } => {
            CoverageVerificationError::ArithmeticOverflow {
                operation: operation.to_owned(),
            }
        }
        CoverageMeasurementError::KernelRefusal { operation } => {
            CoverageVerificationError::KernelRefusal {
                operation: operation.to_owned(),
            }
        }
        CoverageMeasurementError::AssessmentRefusal { reason } => {
            CoverageVerificationError::AssessmentRefusal {
                reason: reason.to_owned(),
            }
        }
        CoverageMeasurementError::InvalidLimits | CoverageMeasurementError::InvalidRequest => {
            CoverageVerificationError::SnapshotBinding
        }
    }
}

/// The status of the format checks this crate actually performed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuralFormatStatus {
    Passed,
    Failed { reason: FormatFailureReason },
}

/// ChituBox GUI/native loading is an independent acceptance checkpoint.
///
/// The format gate never fabricates that evidence from a self-consistent
/// structural parse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GuiLoadabilityCheckpoint {
    NotAssessed,
}

/// Stable classification for a path-read failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactReadFailure {
    NotFound,
    PermissionDenied,
    IsDirectory,
    Interrupted,
    InvalidData,
    Other,
}

/// Stable classification for a structural ChituBox-format failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuralFormatFailure {
    NotChitubox,
    Truncated,
    HeaderInvariant,
    SupportPointerInvalid,
    SupportSectionMalformed,
    ArithmeticOverflow,
    FileTooLarge,
    FormatDrift,
    ParserRejected,
    IndependentAuditRejected,
    RoundtripDrift,
}

/// The named reason for the one failed artifact/gate represented by a defect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "reason", rename_all = "snake_case")]
pub enum FormatFailureReason {
    Unreadable(ArtifactReadFailure),
    Structural(StructuralFormatFailure),
}

/// Result of checking one path as it existed when this function read it.
///
/// `artifact` is a logical identity supplied by the caller, never a serialized
/// absolute filesystem path. On failure `defects` contains exactly one blocking
/// defect with a measure of `1.0` artifact failures.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FormatReport {
    artifact: ArtifactId,
    defects: Vec<Defect>,
    structural_status: StructuralFormatStatus,
    gui_loadability: GuiLoadabilityCheckpoint,
}

impl FormatReport {
    #[must_use]
    pub fn artifact(&self) -> &ArtifactId {
        &self.artifact
    }

    #[must_use]
    pub fn defects(&self) -> &[Defect] {
        &self.defects
    }

    #[must_use]
    pub const fn structural_status(&self) -> &StructuralFormatStatus {
        &self.structural_status
    }

    #[must_use]
    pub const fn gui_loadability(&self) -> GuiLoadabilityCheckpoint {
        self.gui_loadability
    }

    #[must_use]
    pub fn accepted(&self) -> bool {
        self.defects.is_empty()
    }
}

/// Verify an artifact as written at `path`.
///
/// This performs one fresh filesystem read, parses those bytes, invokes
/// `chitubox_io`'s serialization path (which re-parses and runs its independent
/// structural interval audit), then requires byte-for-byte equality with the
/// fresh disk bytes. It intentionally does not reopen any cached producer
/// state or modify the artifact.
///
/// **CURRENTLY UNREACHED IN PRODUCTION.** `plinth-pipeline`'s `prepare` route
/// never calls this function; the only format check that actually runs today
/// is `plinth-emit`'s own fresh-open disk audit. The only callers in this
/// workspace are this crate's own tests and the
/// `plinth-pipeline/examples/shared/first_artifact_chain.rs` example. This is
/// not dead code to delete: it is the structural format gate the pre-write
/// gate porting work (see the remediation plan's Phase 7, "schedule the
/// remaining nine safety gates") will want to wire in as a real, judged-from-
/// recorded-evidence check. Keep it, and give it a production caller when that
/// work lands.
#[must_use]
pub fn verify_artifact(artifact: ArtifactId, path: &Path) -> FormatReport {
    let disk_bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return unreadable_report(artifact, read_failure(error.kind())),
    };

    let project = match Project::from_bytes(disk_bytes.clone()) {
        Ok(project) => project,
        Err(error) => return structural_failure_report(artifact, parse_failure(&error)),
    };

    let serialized = match project.to_bytes() {
        Ok(bytes) => bytes,
        Err(error) => return structural_failure_report(artifact, serialization_failure(&error)),
    };

    report_for_roundtrip(artifact, &disk_bytes, serialized)
}

fn report_for_roundtrip(
    artifact: ArtifactId,
    disk_bytes: &[u8],
    serialized: Vec<u8>,
) -> FormatReport {
    if serialized != disk_bytes {
        return structural_failure_report(artifact, StructuralFormatFailure::RoundtripDrift);
    }

    FormatReport {
        artifact,
        defects: Vec::new(),
        structural_status: StructuralFormatStatus::Passed,
        gui_loadability: GuiLoadabilityCheckpoint::NotAssessed,
    }
}

fn unreadable_report(artifact: ArtifactId, reason: ArtifactReadFailure) -> FormatReport {
    let detail = format!("artifact read failed: {reason:?}");
    failure_report(
        artifact,
        ViolationKind::ArtifactUnreadable,
        FormatFailureReason::Unreadable(reason),
        detail,
    )
}

fn structural_failure_report(
    artifact: ArtifactId,
    reason: StructuralFormatFailure,
) -> FormatReport {
    let detail = format!("ChituBox structural format check failed: {reason:?}");
    failure_report(
        artifact,
        ViolationKind::ChituboxStructuralFormat,
        FormatFailureReason::Structural(reason),
        detail,
    )
}

fn failure_report(
    artifact: ArtifactId,
    kind: ViolationKind,
    reason: FormatFailureReason,
    detail: String,
) -> FormatReport {
    let defect = Defect::new(
        kind,
        SiteId::Artifact(artifact.clone()),
        Severity::Blocking,
        1.0,
        detail,
    )
    .expect("the canonical artifact-failure count of +1.0 is finite");
    FormatReport {
        artifact,
        defects: vec![defect],
        structural_status: StructuralFormatStatus::Failed { reason },
        gui_loadability: GuiLoadabilityCheckpoint::NotAssessed,
    }
}

fn read_failure(kind: std::io::ErrorKind) -> ArtifactReadFailure {
    match kind {
        std::io::ErrorKind::NotFound => ArtifactReadFailure::NotFound,
        std::io::ErrorKind::PermissionDenied => ArtifactReadFailure::PermissionDenied,
        std::io::ErrorKind::IsADirectory => ArtifactReadFailure::IsDirectory,
        std::io::ErrorKind::Interrupted => ArtifactReadFailure::Interrupted,
        std::io::ErrorKind::InvalidData => ArtifactReadFailure::InvalidData,
        _ => ArtifactReadFailure::Other,
    }
}

fn parse_failure(error: &ChituboxError) -> StructuralFormatFailure {
    match error {
        ChituboxError::NotChitubox { .. } => StructuralFormatFailure::NotChitubox,
        ChituboxError::Truncated { .. } => StructuralFormatFailure::Truncated,
        ChituboxError::HeaderInvariant { .. } => StructuralFormatFailure::HeaderInvariant,
        ChituboxError::SupportPointerInvalid { .. } => {
            StructuralFormatFailure::SupportPointerInvalid
        }
        ChituboxError::SupportSectionMalformed { .. } => {
            StructuralFormatFailure::SupportSectionMalformed
        }
        ChituboxError::ArithmeticOverflow { .. } => StructuralFormatFailure::ArithmeticOverflow,
        ChituboxError::FileTooLarge { .. } => StructuralFormatFailure::FileTooLarge,
        ChituboxError::FormatDrift { .. } => StructuralFormatFailure::FormatDrift,
        _ => StructuralFormatFailure::ParserRejected,
    }
}

fn serialization_failure(error: &ChituboxError) -> StructuralFormatFailure {
    match error {
        ChituboxError::CleanRoundtripDrift => StructuralFormatFailure::RoundtripDrift,
        ChituboxError::VerifyFailed { .. } => StructuralFormatFailure::IndependentAuditRejected,
        ChituboxError::FormatDrift { .. } => StructuralFormatFailure::FormatDrift,
        _ => StructuralFormatFailure::ParserRejected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registered production censor, `voxel_pitch_mm²` = 0.16 mm². Read,
    /// never written down: a fixture that hardcoded 0.16 would keep passing if
    /// the registered pitch moved and the gate stopped agreeing with it.
    fn censor() -> f64 {
        plinth_support::registered_coverage_censor_area_mm2().expect("registered censor")
    }

    fn patch(
        island_id: u32,
        area: f64,
        cut_ambiguous: bool,
        no_route: bool,
    ) -> CoveragePatchEvidenceRecord {
        // Exactly how a real measurement stamps the flag
        // (`assess_coverage_with_mode`): area strictly greater than the censor,
        // or an ambiguous cut.
        let assessment_failed = area > censor() || cut_ambiguous;
        patch_exact(island_id, area, cut_ambiguous, assessment_failed, no_route)
    }

    /// The same fixture with each predicate axis set INDEPENDENTLY.
    ///
    /// `patch` couples `assessment_failed` to the other two, which is what a
    /// real measurement does — and is exactly why it cannot exercise the
    /// predicate's axes separately. Anything testing the blocking rule itself
    /// must be able to set them apart.
    fn patch_exact(
        island_id: u32,
        area: f64,
        cut_ambiguous: bool,
        assessment_failed: bool,
        no_route: bool,
    ) -> CoveragePatchEvidenceRecord {
        CoveragePatchEvidenceRecord {
            island_id,
            component_id: 0,
            representative_face_id: island_id,
            sorted_face_ids: vec![island_id],
            uncovered_area_mm2: Finite::new(area).expect("finite fixture area"),
            cut_ambiguous,
            assessment_failed,
            worst_gap: if no_route {
                CoverageGapRecord::NoRoutedContact
            } else {
                CoverageGapRecord::Finite(Finite::new(0.5).expect("finite fixture gap"))
            },
        }
    }

    /// **The blocking predicate's three axes, exercised separately.**
    ///
    /// `defects_for_patches` calls `plinth_support::coverage_patch_blocks_verdict`,
    /// and the FU-56 controller decides when it may STOP with the same call —
    /// so a drift between this verdict and that one is impossible by
    /// construction. (What the controller must WORK ON is a different
    /// question, answered by the uncensored
    /// `plinth_support::coverage_patch_needs_repair`.) What is still
    /// worth pinning is the RULE, on each axis independently, because the
    /// frozen fixture this crate's integration test walks has seven of its nine
    /// patches blocking on area with `cut_ambiguous` false everywhere: a
    /// predicate that dropped the `cut_ambiguous` term or dropped the
    /// `assessment_failed` term would leave that fixture unchanged.
    #[test]
    fn a_patch_blocks_on_each_axis_independently_and_only_blocking_area_is_summed() {
        // Axis 1: cut_ambiguous alone, zero area, assessment NOT failed.
        let ambiguous_only = vec![patch_exact(1, 0.0, true, false, false)];
        assert_eq!(
            defects_for_patches(&ambiguous_only, censor())
                .expect("defects")
                .len(),
            1,
            "a zero-area, assessment-passed, CUT-AMBIGUOUS patch must still block"
        );

        // Axis 2: assessment_failed alone, zero area, not ambiguous.
        let failed_only = vec![patch_exact(1, 0.0, false, true, false)];
        assert_eq!(
            defects_for_patches(&failed_only, censor())
                .expect("defects")
                .len(),
            1,
            "a zero-area, unambiguous, ASSESSMENT-FAILED patch must still block"
        );

        // Axis 3: area alone, ABOVE the censor.
        let above_censor = vec![patch_exact(1, censor() * 2.0, false, false, false)];
        let above = defects_for_patches(&above_censor, censor()).expect("defects");
        assert_eq!(
            above.len(),
            1,
            "uncovered area above the censor blocks on its own"
        );
        assert!((above[0].measure.get() - censor() * 2.0).abs() < f64::EPSILON);

        // Nothing on any axis: not a defect at all.
        assert!(
            defects_for_patches(&[patch_exact(1, 0.0, false, false, false)], censor())
                .expect("defects")
                .is_empty()
        );

        // And the aggregation rule: on an island holding one blocking and one
        // NON-blocking patch, only the blocking patch's area is summed.
        let mixed = vec![
            patch_exact(4, 2.5, false, true, false),
            patch_exact(4, 0.0, false, false, false),
        ];
        let defects = defects_for_patches(&mixed, censor()).expect("defects");
        assert_eq!(defects.len(), 1);
        assert!((defects[0].measure.get() - 2.5).abs() < f64::EPSILON);
    }

    /// **The censor, both ways, at the verdict layer.**
    ///
    /// docs/specs/coverage-repair.md §4: "A patch fails if and only if either: 1. its
    /// uncovered area is greater than `voxel_pitch_mm**2`; or 2. it touches an
    /// ambiguous topology cut. Area at or below `voxel_pitch_mm**2` is
    /// **censored A3 uncertainty**."
    ///
    /// This is the pair that would go red if the censor were removed (the dust
    /// cases start raising defects again) or widened to swallow everything (the
    /// real island stops raising one). The four asserts are deliberately on the
    /// two sides of the same boundary, with a real measured area on each side.
    #[test]
    fn sub_censor_dust_raises_no_defect_and_a_real_uncovered_patch_still_does() {
        // Deinonychus' one-triangle 0.00047 mm² patch: three orders of
        // magnitude below one 0.4 mm voxel footprint. A real measurement stamps
        // `assessment_failed = false` on it, and the verdict must agree.
        let dust = vec![patch(11, 0.000_47, false, false)];
        assert!(!dust[0].assessment_failed);
        assert!(
            defects_for_patches(&dust, censor())
                .expect("defects")
                .is_empty(),
            "a 0.00047 mm² patch is censored A3 uncertainty, not a blocking defect"
        );

        // Thirteen separate sub-censor patches on thirteen islands are thirteen
        // sub-resolution unknowns, never one 1.3 mm² hole: the spec bounds each
        // patch, so summing them across islands must not manufacture a defect.
        let scattered: Vec<_> = (0..13).map(|id| patch(id, 0.1, false, false)).collect();
        assert!(
            defects_for_patches(&scattered, censor())
                .expect("defects")
                .is_empty(),
            "the censor bounds each patch; thirteen censored patches are still thirteen censored \
             patches"
        );

        // Warhorse's whole 0.6505 mm² deficit landing on ONE patch is four
        // voxel footprints of genuinely uncovered surface and must still fail.
        let real = vec![patch(11, 0.650_5, false, false)];
        assert!(real[0].assessment_failed);
        let defects = defects_for_patches(&real, censor()).expect("defects");
        assert_eq!(
            defects.len(),
            1,
            "a genuinely uncovered patch above the censor must still be a blocking defect"
        );
        assert!((defects[0].measure.get() - 0.650_5).abs() < f64::EPSILON);

        // The mixed case that matters most in the field: one island carrying a
        // real hole AND dust. It fails, and its measure is the real hole alone.
        let mixed = vec![
            patch_exact(3, 0.650_5, false, true, false),
            patch_exact(3, 0.000_47, false, false, false),
        ];
        let defects = defects_for_patches(&mixed, censor()).expect("defects");
        assert_eq!(defects.len(), 1);
        assert!(
            (defects[0].measure.get() - 0.650_5).abs() < f64::EPSILON,
            "censored dust must not inflate the blocking island's reported measure"
        );
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn exact_roundtrip_drift_is_a_named_structural_failure() {
        let report = report_for_roundtrip(ArtifactId::from("test-artifact"), &[1, 2], vec![1, 3]);
        assert_eq!(
            report.structural_status(),
            &StructuralFormatStatus::Failed {
                reason: FormatFailureReason::Structural(StructuralFormatFailure::RoundtripDrift)
            }
        );
        assert_eq!(
            report.defects()[0].kind,
            ViolationKind::ChituboxStructuralFormat
        );
        assert_eq!(report.defects()[0].measure.get(), 1.0);
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn coverage_defects_are_one_finite_area_sum_per_island_and_keep_gap_as_evidence() {
        let patches = vec![
            patch(2, 1.5, false, true),
            patch(2, 2.25, false, false),
            patch(7, 0.0, true, false),
        ];
        let defects = defects_for_patches(&patches, censor()).expect("finite checked area sums");
        assert_eq!(defects.len(), 2);
        assert_eq!(defects[0].site, SiteId::Island(2));
        assert_eq!(defects[0].measure.get(), 3.75);
        assert_eq!(defects[1].site, SiteId::Island(7));
        assert_eq!(defects[1].measure.get(), 0.0);
        assert!(matches!(
            patches[0].worst_gap,
            CoverageGapRecord::NoRoutedContact
        ));
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn immutable_projection_keeps_same_island_aggregation_order_and_gap_evidence() {
        let patches = vec![
            patch(2, 1.5, false, true),
            patch(2, 2.25, false, false),
            patch(7, 0.0, true, false),
        ];
        let report = coverage_report_from_projection(
            ArtifactId::from("projection-fixture"),
            99,
            CoverageRequestRecord {
                radius_mm: Finite::new(1.0).unwrap(),
                patch_area_floor_mm2: Finite::new(1.0).unwrap(),
                max_route_memberships: 1,
                max_indexed_points: 1,
                max_query_rows: 1,
                max_distance_work: 1,
                max_logical_peak_bytes: 1,
                max_assessment_work: 1,
            },
            CoverageResourceRecord {
                route_membership_sum: 1,
                sequential_route_membership_peak: 1,
                indexed_point_count: 1,
                sequential_indexed_point_peak: 1,
                query_row_count: 1,
                distance_work: 1,
                logical_peak_bytes: 1,
                assessment_work: 1,
                assessment_scratch_bytes: 1,
                assessment_evidence_bytes: 1,
                assessment_phase_peak_bytes: 1,
            },
            CoverageEvidenceRecord {
                islands: vec![
                    CoverageIslandEvidenceRecord {
                        island_id: 2,
                        component_id: 0,
                        representative_face_id: 2,
                        sorted_face_ids: vec![2],
                    },
                    CoverageIslandEvidenceRecord {
                        island_id: 7,
                        component_id: 0,
                        representative_face_id: 7,
                        sorted_face_ids: vec![7],
                    },
                ],
                patches,
            },
        )
        .expect("finite immutable projection");
        assert!(!report.accepted());
        assert_eq!(
            report
                .defects()
                .iter()
                .map(|defect| defect.site.clone())
                .collect::<Vec<_>>(),
            vec![SiteId::Island(2), SiteId::Island(7)]
        );
        assert_eq!(report.defects()[0].measure.get(), 3.75);
        assert_eq!(report.defects()[1].measure.get(), 0.0);
        let json = serde_json::to_string(&report).expect("exact projection json");
        assert!(json.contains("\"state\":\"no_routed_contact\""));
        assert!(json.find("\"island_id\":2").unwrap() < json.find("\"island_id\":7").unwrap());
    }

    #[test]
    fn coverage_report_json_roundtrips_exactly() {
        let report = coverage_report_from_projection(
            ArtifactId::from("coverage-fixture"),
            42,
            CoverageRequestRecord {
                radius_mm: Finite::new(1.0).expect("finite"),
                patch_area_floor_mm2: Finite::new(0.25).expect("finite"),
                max_route_memberships: 1,
                max_indexed_points: 2,
                max_query_rows: 3,
                max_distance_work: 4,
                max_logical_peak_bytes: 5,
                max_assessment_work: 6,
            },
            CoverageResourceRecord {
                route_membership_sum: 1,
                sequential_route_membership_peak: 1,
                indexed_point_count: 1,
                sequential_indexed_point_peak: 1,
                query_row_count: 1,
                distance_work: 1,
                logical_peak_bytes: 1,
                assessment_work: 1,
                assessment_scratch_bytes: 1,
                assessment_evidence_bytes: 1,
                assessment_phase_peak_bytes: 1,
            },
            CoverageEvidenceRecord {
                islands: Vec::new(),
                patches: Vec::new(),
            },
        )
        .expect("immutable verifier projection");
        let json = serde_json::to_string(&report).expect("serialize report");
        assert_eq!(
            serde_json::from_str::<CoverageReport>(&json).expect("deserialize report"),
            report
        );
        assert!(report.accepted());
    }
}
