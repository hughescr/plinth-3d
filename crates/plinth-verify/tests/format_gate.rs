use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use chitubox_io::{ModelGeometry, ModelSource, Placement, Project, TriangleSoup};
use plinth_core::{ArtifactId, Severity, SiteId, ViolationKind};
use plinth_verify::{
    verify_artifact, ArtifactReadFailure, FormatFailureReason, GuiLoadabilityCheckpoint,
    StructuralFormatFailure, StructuralFormatStatus,
};

static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);

struct TempArtifact {
    path: PathBuf,
}

impl TempArtifact {
    fn new(bytes: &[u8]) -> Self {
        let sequence = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
        let path = PathBuf::from("/private/tmp").join(format!(
            "plinth-verify-format-gate-{}-{sequence}.chitubox",
            std::process::id()
        ));
        std::fs::write(&path, bytes).expect("write temporary artifact");
        Self { path }
    }
}

impl Drop for TempArtifact {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn artifact_id() -> ArtifactId {
    ArtifactId::from("format-gate-fixture")
}

fn assert_single_failure(
    report: &plinth_verify::FormatReport,
    kind: ViolationKind,
    reason: FormatFailureReason,
) {
    assert!(!report.accepted());
    assert_eq!(
        report.gui_loadability(),
        GuiLoadabilityCheckpoint::NotAssessed
    );
    assert_eq!(
        report.structural_status(),
        &StructuralFormatStatus::Failed { reason }
    );
    assert_eq!(report.defects().len(), 1);
    let defect = &report.defects()[0];
    assert_eq!(defect.kind, kind);
    assert_eq!(defect.site, SiteId::Artifact(artifact_id()));
    assert_eq!(defect.severity, Severity::Blocking);
    assert_eq!(defect.measure.get(), 1.0, "artifact failure count is +1.0");
    assert_ne!(defect.kind, ViolationKind::ChituboxLoadability);
}

fn fixture_path(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/tests/fixtures/chitubox")
        .join(name);
    assert!(
        path.is_file(),
        "missing committed fixture: {}",
        path.display()
    );
    path
}

fn source(name: &str) -> ModelSource {
    ModelSource {
        name: name.into(),
        geometry: ModelGeometry::Triangles(TriangleSoup::new(vec![
            1.0, 0.0, 0.0, -1.0, 1.0, 0.0, 0.0, -1.0, 0.0,
        ])),
        placement: Placement::default(),
    }
}

fn scratch_bytes(model_count: usize) -> Vec<u8> {
    let mut project = Project::new_empty();
    let models: Vec<_> = (0..model_count)
        .map(|index| source(&format!("model-{index}")))
        .collect();
    project.add_models(&models).expect("add models");
    project.to_bytes().expect("serialize scratch project")
}

fn corrupt_bytes() -> Vec<u8> {
    let mut bytes = scratch_bytes(1);
    bytes[..4].copy_from_slice(&0xDEAD_BEEF_u32.to_le_bytes());
    bytes
}

fn u32_at(bytes: &[u8], offset: usize) -> usize {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("u32 bytes")) as usize
}

#[test]
fn authoritative_native_fixtures_pass_structural_gate_without_claiming_gui_acceptance() {
    for name in [
        "morel_mudskipper_supports.chitubox",
        "two-supports.chitubox",
        "basic-cube-supports-raft.chitubox",
        "cylinder-supported-bottom-connected.chitubox",
    ] {
        let report = verify_artifact(artifact_id(), &fixture_path(name));
        assert!(report.accepted(), "{name}: {report:?}");
        assert!(report.defects().is_empty());
        assert_eq!(report.structural_status(), &StructuralFormatStatus::Passed);
        assert_eq!(
            report.gui_loadability(),
            GuiLoadabilityCheckpoint::NotAssessed
        );
    }
}

#[test]
fn scratch_projects_with_one_two_and_six_descriptors_pass() {
    for count in [1, 2, 6] {
        let artifact = TempArtifact::new(&scratch_bytes(count));
        let report = verify_artifact(artifact_id(), &artifact.path);
        assert!(report.accepted(), "{count} models: {report:?}");
    }
}

#[test]
fn corrupt_truncated_pointer_and_underlength_descriptor_cases_fail_by_named_reason() {
    let corrupt = TempArtifact::new(&corrupt_bytes());
    assert_single_failure(
        &verify_artifact(artifact_id(), &corrupt.path),
        ViolationKind::ChituboxStructuralFormat,
        FormatFailureReason::Structural(StructuralFormatFailure::NotChitubox),
    );

    let truncated = TempArtifact::new(&scratch_bytes(1)[..10]);
    assert_single_failure(
        &verify_artifact(artifact_id(), &truncated.path),
        ViolationKind::ChituboxStructuralFormat,
        FormatFailureReason::Structural(StructuralFormatFailure::Truncated),
    );

    let mut pointer_bytes =
        std::fs::read(fixture_path("two-supports.chitubox")).expect("read supported fixture");
    let table = u32_at(&pointer_bytes, 8);
    let support_index = u32_at(&pointer_bytes, table + 268);
    pointer_bytes[support_index + 4..support_index + 8]
        .copy_from_slice(&((support_index + 8) as u32).to_le_bytes());
    let pointer = TempArtifact::new(&pointer_bytes);
    assert_single_failure(
        &verify_artifact(artifact_id(), &pointer.path),
        ViolationKind::ChituboxStructuralFormat,
        FormatFailureReason::Structural(StructuralFormatFailure::SupportPointerInvalid),
    );

    let mut descriptors = scratch_bytes(6);
    let table = u32_at(&descriptors, 8);
    let table_end = u32_at(&descriptors, 20);
    descriptors[table + 272..table + 276].copy_from_slice(&((table_end + 48) as u32).to_le_bytes());
    let underlength = TempArtifact::new(&descriptors);
    assert_single_failure(
        &verify_artifact(artifact_id(), &underlength.path),
        ViolationKind::ChituboxStructuralFormat,
        FormatFailureReason::Structural(StructuralFormatFailure::HeaderInvariant),
    );
}

#[test]
fn missing_path_is_a_truthful_artifact_read_failure() {
    let sequence = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
    let path = PathBuf::from("/private/tmp").join(format!(
        "plinth-verify-format-missing-{}-{sequence}.chitubox",
        std::process::id()
    ));
    assert!(!path.exists(), "test missing path unexpectedly exists");
    assert_single_failure(
        &verify_artifact(artifact_id(), &path),
        ViolationKind::ArtifactUnreadable,
        FormatFailureReason::Unreadable(ArtifactReadFailure::NotFound),
    );
}

#[test]
fn replacement_at_the_same_path_is_reopened_and_judged_from_disk() {
    let artifact = TempArtifact::new(&scratch_bytes(1));
    assert!(verify_artifact(artifact_id(), &artifact.path).accepted());

    std::fs::write(&artifact.path, corrupt_bytes()).expect("replace artifact on disk");
    assert_single_failure(
        &verify_artifact(artifact_id(), &artifact.path),
        ViolationKind::ChituboxStructuralFormat,
        FormatFailureReason::Structural(StructuralFormatFailure::NotChitubox),
    );
}

#[test]
fn reports_are_deterministic_and_keep_logical_identity_separate_from_path() {
    let artifact = TempArtifact::new(&scratch_bytes(2));
    let first = verify_artifact(artifact_id(), &artifact.path);
    let second = verify_artifact(artifact_id(), &artifact.path);
    assert_eq!(first, second);
    let json = serde_json::to_string(&first).expect("serialize stable report");
    assert!(json.contains("format-gate-fixture"));
    assert!(!json.contains(&artifact.path.display().to_string()));
}
