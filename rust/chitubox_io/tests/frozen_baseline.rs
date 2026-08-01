use std::path::{Path, PathBuf};

use chitubox_io::Project;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

#[derive(Debug, Deserialize)]
struct CorpusManifest {
    schema: String,
    version: u32,
    provisional: bool,
    models: Vec<CorpusModel>,
    waivers: Vec<NoWaiver>,
}

#[derive(Debug, Deserialize)]
struct CorpusModel {
    name: String,
    stl: PathBuf,
    source_digest: SourceDigest,
    report: PathBuf,
    baseline_artifact: Option<PathBuf>,
    python_exit_code: u8,
    python_failed_stage: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SourceDigest {
    algorithm: DigestAlgorithm,
    hex: Sha256Hex,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum DigestAlgorithm {
    Sha256,
}

#[derive(Debug)]
struct Sha256Hex([u8; 32]);

impl<'de> Deserialize<'de> for Sha256Hex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        if encoded.len() != 64 || !encoded.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(D::Error::custom(
                "SHA-256 digest must contain exactly 64 hexadecimal digits",
            ));
        }
        if encoded.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return Err(D::Error::custom("SHA-256 digest must use lowercase hex"));
        }

        let mut bytes = [0_u8; 32];
        for (index, pair) in encoded.as_bytes().chunks_exact(2).enumerate() {
            let high = hex_nibble(pair[0]).ok_or_else(|| D::Error::custom("invalid SHA-256"))?;
            let low = hex_nibble(pair[1]).ok_or_else(|| D::Error::custom("invalid SHA-256"))?;
            bytes[index] = (high << 4) | low;
        }
        Ok(Self(bytes))
    }
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PipelineStatus {
    Ok,
    Failed,
}

#[derive(Debug, Deserialize)]
struct PipelineReport {
    input_stl: PathBuf,
    out_path: PathBuf,
    status: PipelineStatus,
    exit_code: u8,
    failed_stage: Option<String>,
    report_path: PathBuf,
    settings: PipelineSettings,
}

#[derive(Debug, Deserialize)]
struct PipelineSettings {
    wall_mm: f64,
    orient: bool,
    hollow: bool,
    lift_mm: f64,
    cfg_lift_mm: f64,
    p8_relocate_enable: bool,
    allow_failed_gates: bool,
    skip_existing: bool,
}

#[derive(Debug, Deserialize)]
enum NoWaiver {}

const EXPECTED_NAMES: [&str; 8] = [
    "Sphere",
    "Cylinder",
    "make_box",
    "make_overhang_table",
    "make_toed_slab",
    "make_raking_blade",
    "make_thin_fin",
    "Weeping Treant v1.2",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("chitubox_io must remain two directories below the repository root")
        .to_path_buf()
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path, label: &str) -> T {
    let bytes = std::fs::read(path).unwrap_or_else(|error| {
        panic!(
            "required {label} is unreadable ({}): {error}",
            path.display()
        )
    });
    serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "required {label} is invalid JSON ({}): {error}",
            path.display()
        )
    })
}

fn expected_stl(name: &str) -> PathBuf {
    PathBuf::from("corpus/models").join(format!("{name}.stl"))
}

fn expected_report(name: &str) -> PathBuf {
    PathBuf::from("corpus/baseline").join(format!("{name}.pipeline.json"))
}

fn expected_artifact(name: &str) -> PathBuf {
    PathBuf::from("corpus/baseline").join(format!("{name}.chitubox"))
}

fn assert_freeze_settings(name: &str, settings: &PipelineSettings) {
    assert_eq!(settings.wall_mm, 2.5, "{name}: baseline wall differs");
    assert!(settings.orient, "{name}: orientation was disabled");
    assert!(settings.hollow, "{name}: hollowing was disabled");
    assert_eq!(settings.lift_mm, 3.0, "{name}: baseline lift differs");
    assert_eq!(settings.cfg_lift_mm, 3.0, "{name}: config lift differs");
    assert!(
        settings.p8_relocate_enable,
        "{name}: baseline omitted mandatory P8 relocation"
    );
    assert!(
        !settings.allow_failed_gates,
        "{name}: baseline allowed failed gates"
    );
    assert!(
        settings.skip_existing,
        "{name}: baseline command lost --skip-existing"
    );
}

fn assert_artifact_as_written(name: &str, path: &Path) {
    assert!(
        path.is_file(),
        "{name}: required successful baseline artifact is missing: {}",
        path.display()
    );
    let disk_bytes = std::fs::read(path).unwrap_or_else(|error| {
        panic!(
            "{name}: baseline artifact is unreadable ({}): {error}",
            path.display()
        )
    });
    let project = Project::open(path).unwrap_or_else(|error| {
        panic!("{name}: baseline artifact does not parse fresh from disk: {error}")
    });
    let audited_bytes = project
        .to_bytes()
        .unwrap_or_else(|error| panic!("{name}: fresh structural audit failed: {error}"));
    assert_eq!(
        audited_bytes, disk_bytes,
        "{name}: fresh structural audit must preserve original artifact bytes without repair"
    );
}

#[test]
fn frozen_baseline_manifest_and_artifacts_are_complete_and_truthful() {
    let root = repo_root();
    let manifest_path = root.join("corpus/manifest.json");
    let manifest: CorpusManifest = read_json(&manifest_path, "frozen corpus manifest");

    assert_eq!(manifest.schema, "plinth/corpus-manifest");
    assert_eq!(manifest.version, 1);
    assert!(
        !manifest.provisional,
        "the committed baseline must include Treant"
    );
    assert!(
        manifest.waivers.is_empty(),
        "M0 baseline has no signed waivers"
    );
    assert_eq!(
        manifest.models.len(),
        EXPECTED_NAMES.len(),
        "baseline model count"
    );

    let actual_names: Vec<&str> = manifest
        .models
        .iter()
        .map(|model| model.name.as_str())
        .collect();
    assert_eq!(
        actual_names, EXPECTED_NAMES,
        "baseline binding order or membership changed"
    );
    assert!(
        !actual_names.contains(&"Cone"),
        "Cone is not a sanctioned corpus member"
    );

    for model in manifest.models {
        let name = model.name.as_str();
        assert_eq!(model.stl, expected_stl(name), "{name}: source STL path");
        match model.source_digest.algorithm {
            DigestAlgorithm::Sha256 => {}
        }
        assert_eq!(
            model.source_digest.hex.0.len(),
            32,
            "{name}: SHA-256 digest byte length"
        );
        // Source STLs are ignored local inputs. The test deliberately validates
        // digest shape without requiring the bytes in a clean checkout.

        assert_eq!(model.report, expected_report(name), "{name}: report path");
        let report_path = root.join(&model.report);
        assert!(
            report_path.is_file(),
            "{name}: required committed baseline report is missing: {}",
            report_path.display()
        );
        let report: PipelineReport = read_json(&report_path, "baseline pipeline report");

        assert_eq!(report.input_stl, model.stl, "{name}: report input STL");
        assert_eq!(report.report_path, model.report, "{name}: report self path");
        assert_eq!(
            report.exit_code, model.python_exit_code,
            "{name}: exit code"
        );
        assert_eq!(
            report.failed_stage, model.python_failed_stage,
            "{name}: failed stage"
        );
        assert_eq!(
            report.out_path,
            expected_artifact(name),
            "{name}: output path"
        );
        assert_freeze_settings(name, &report.settings);

        let disk_artifact = root.join(&report.out_path);
        if model.python_exit_code == 0 {
            assert!(
                matches!(report.status, PipelineStatus::Ok),
                "{name}: successful exit has non-ok status"
            );
            assert!(
                model.python_failed_stage.is_none(),
                "{name}: successful exit has a failed stage"
            );
            assert_eq!(
                model.baseline_artifact.as_deref(),
                Some(report.out_path.as_path()),
                "{name}: successful exit must name its emitted artifact"
            );
            assert_artifact_as_written(name, &disk_artifact);
        } else {
            assert!(
                matches!(report.status, PipelineStatus::Failed),
                "{name}: failed exit has non-failed status"
            );
            assert!(
                model.python_failed_stage.is_some(),
                "{name}: failed exit lacks a named failed stage"
            );
            assert!(
                model.baseline_artifact.is_none(),
                "{name}: failed exit must carry a null baseline_artifact"
            );
            assert!(
                !disk_artifact.exists(),
                "{name}: failed exit unexpectedly left an emitted artifact: {}",
                disk_artifact.display()
            );
        }
    }
}
