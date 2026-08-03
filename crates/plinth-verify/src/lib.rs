//! Disk-first ChituBox container verification.
//!
//! This crate judges precisely the bytes freshly read from the supplied path.
//! It has no producer state, cache, mutation path, logging side effect, or
//! repair behavior. A structural pass is intentionally not a claim that the
//! ChituBox GUI will load the artifact.

#![forbid(unsafe_code)]

use std::path::Path;

use chitubox_io::{ChituboxError, Project};
use plinth_core::{ArtifactId, Defect, Severity, SiteId, ViolationKind};
use serde::Serialize;

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

    #[test]
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
}
