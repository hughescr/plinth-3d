use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use chitubox_io::Project;
use plinth_core::ModelId;
use plinth_metrics::{
    load_artifacts, ArtifactDisposition, ArtifactLoad, ArtifactRequest, ArtifactRequestError,
    DiskReadError, ProducerStage,
};
use serde::Deserialize;

static TEMP_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Deserialize)]
struct Manifest {
    models: Vec<ManifestModel>,
}

#[derive(Debug, Deserialize)]
struct ManifestModel {
    name: String,
    baseline_artifact: Option<PathBuf>,
    python_exit_code: u8,
    python_failed_stage: Option<ManifestStage>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ManifestStage {
    Hollow,
    Supports,
    Emit,
}

impl From<ManifestStage> for ProducerStage {
    fn from(value: ManifestStage) -> Self {
        match value {
            ManifestStage::Hollow => Self::Hollow,
            ManifestStage::Supports => Self::Supports,
            ManifestStage::Emit => Self::Emit,
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("crate lives two directories below the committed repository root")
}

fn committed_requests() -> Vec<ArtifactRequest> {
    let root = repo_root();
    let bytes = fs::read(root.join("corpus/manifest.json")).expect("committed corpus manifest");
    let manifest: Manifest =
        serde_json::from_slice(&bytes).expect("valid committed corpus manifest");
    manifest
        .models
        .into_iter()
        .map(|entry| {
            let disposition = ArtifactDisposition::from_manifest_fields(
                entry.python_exit_code,
                entry.python_failed_stage.map(ProducerStage::from),
                entry.baseline_artifact.map(|path| root.join(path)),
            )
            .expect("committed manifest fields have a coherent output disposition");
            ArtifactRequest::new(ModelId::from(entry.name), disposition)
        })
        .collect()
}

#[test]
fn committed_manifest_loads_sphere_from_disk_and_retains_all_failed_rows_in_order() {
    let requests = committed_requests();
    let sphere_path = match requests[0].disposition() {
        ArtifactDisposition::Produced { path } => path.clone(),
        ArtifactDisposition::ProducerFailed(_) => {
            panic!("Sphere must be the frozen successful row")
        }
    };
    let bytes_before = fs::read(&sphere_path).expect("committed Sphere artifact");

    let loads = load_artifacts(&requests).expect("fresh disk read of frozen corpus manifest");

    assert_eq!(
        loads
            .iter()
            .map(|row| row.model().as_str())
            .collect::<Vec<_>>(),
        [
            "Sphere",
            "Cylinder",
            "make_box",
            "make_overhang_table",
            "make_toed_slab",
            "make_raking_blade",
            "make_thin_fin",
            "Weeping Treant v1.2",
        ]
    );
    assert_eq!(
        loads.len(),
        8,
        "the full corpus remains ordered and complete"
    );

    let sphere = loads[0]
        .as_artifact()
        .expect("zero-exit Sphere must be a loaded artifact");
    assert_eq!(sphere.path(), sphere_path);
    assert_eq!(sphere.models().len(), 1);
    assert!(!sphere.models()[0].mesh().triangles().is_empty());
    assert_eq!(sphere.models()[0].source_index(), 0);

    let direct = Project::open(&sphere_path).expect("reader fixture itself parses fresh from disk");
    let direct_first = direct.model_triangles(0).unwrap()[0];
    assert_eq!(
        sphere.models()[0].mesh().vertices()[0][0],
        f64::from(direct_first),
        "f32 container coordinates are copied losslessly into owned f64 audit input"
    );
    assert_eq!(
        sphere.models()[0].supports().len(),
        direct.supports(0).unwrap().len(),
        "support records are copied as owned audit data after the fresh disk parse"
    );

    let expected_failures = [
        (2, Some(ProducerStage::Hollow)),
        (2, Some(ProducerStage::Hollow)),
        (3, Some(ProducerStage::Emit)),
        (5, Some(ProducerStage::Supports)),
        (5, Some(ProducerStage::Supports)),
        (5, Some(ProducerStage::Supports)),
        (3, Some(ProducerStage::Emit)),
    ];
    for (row, (exit_code, stage)) in (1..8).zip(expected_failures) {
        match &loads[row] {
            ArtifactLoad::ProducerFailed { failure, .. } => {
                assert_eq!(failure.exit_code().get(), exit_code);
                assert_eq!(failure.failed_stage(), stage);
            }
            ArtifactLoad::Artifact(_) => {
                panic!("failed corpus row {row} must not be treated as an artifact")
            }
        }
    }

    assert_eq!(
        fs::read(&sphere_path).expect("artifact remains present after read"),
        bytes_before,
        "disk audit never repairs, rewrites, or otherwise mutates the emitted file"
    );
}

#[test]
fn missing_successful_artifact_fails_by_model_and_path() {
    let path = unique_temp_dir().join("absent.chitubox");
    let request = ArtifactRequest::new(
        ModelId::from("missing-success"),
        ArtifactDisposition::Produced { path: path.clone() },
    );
    assert!(matches!(
        load_artifacts(&[request]),
        Err(DiskReadError::MissingArtifact { model, path: actual })
            if model.as_str() == "missing-success" && actual == path
    ));
}

#[test]
fn failed_producer_cannot_carry_an_artifact_path() {
    let path = PathBuf::from("should-not-exist.chitubox");
    assert!(matches!(
        ArtifactDisposition::from_manifest_fields(5, Some(ProducerStage::Supports), Some(path.clone())),
        Err(ArtifactRequestError::FailedProducerHasArtifact { exit_code, path: actual })
            if exit_code.get() == 5 && actual == path
    ));
    assert!(matches!(
        ArtifactDisposition::from_manifest_fields(0, None, None),
        Err(ArtifactRequestError::SuccessfulArtifactMissingPath)
    ));
}

#[test]
fn corrupt_successful_artifact_is_a_format_error_and_is_not_mutated() {
    let temp = TempDir::new();
    let path = temp.path().join("corrupt.chitubox");
    let corrupt = vec![0x13, 0x37, 0x00];
    fs::write(&path, &corrupt).expect("self-contained corrupt fixture");
    let request = ArtifactRequest::new(
        ModelId::from("corrupt-success"),
        ArtifactDisposition::Produced { path: path.clone() },
    );
    assert!(matches!(
        load_artifacts(&[request]),
        Err(DiskReadError::ArtifactFormat { model, path: actual, .. })
            if model.as_str() == "corrupt-success" && actual == path
    ));
    assert_eq!(
        fs::read(path).unwrap(),
        corrupt,
        "reader does not repair corrupt files"
    );
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = unique_temp_dir();
        fs::create_dir_all(&path).expect("create test-only directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn unique_temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "plinth-metrics-disk-reader-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}
