use std::path::{Path, PathBuf};

use chitubox_io::Project;
use serde::de::{Error as _, IgnoredAny};
use serde::{Deserialize, Deserializer};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorpusManifest {
    schema: String,
    version: u32,
    frozen_at: FrozenAt,
    provisional: bool,
    baseline_tool: BaselineTool,
    baseline_command: FreezeCommand,
    baseline_git_rev: GitRevision,
    metrics: Vec<MetricDefinition>,
    models: Vec<CorpusModel>,
    waivers: Vec<NoWaiver>,
}

#[derive(Debug)]
struct CorpusModel {
    name: String,
    stl: PathBuf,
    source_digest: SourceDigest,
    report: PathBuf,
    baseline_artifact: PresentNullable<PathBuf>,
    python_exit_code: u8,
    python_seconds: f64,
    python_failed_stage: PresentNullable<StageName>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceDigest {
    algorithm: DigestAlgorithm,
    hex: Sha256Hex,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Copy, Clone)]
#[serde(rename_all = "lowercase")]
enum DigestAlgorithm {
    Sha256,
}

#[derive(Debug, PartialEq, Eq)]
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

#[derive(Debug, Deserialize, PartialEq, Eq, Copy, Clone)]
#[serde(rename_all = "snake_case")]
enum PipelineStatus {
    Ok,
    Failed,
}

struct PipelineReport {
    input_stl: PathBuf,
    out_path: PathBuf,
    prepped_stl: PathBuf,
    status: PipelineStatus,
    exit_code: u8,
    failed_stage: PresentNullable<StageName>,
    error: PresentNullable<String>,
    seconds: f64,
    report_path: PathBuf,
    settings: PipelineSettings,
    stages: Vec<PipelineStage>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PipelineSettings {
    wall_mm: f64,
    orient: bool,
    hollow: bool,
    lift_mm: f64,
    cfg_lift_mm: f64,
    p8_relocate_enable: bool,
    allow_failed_gates: bool,
    skip_existing: bool,
    internal_support_verifier_configured: bool,
    a5_internal_supports_enabled: bool,
}

struct PipelineStage {
    name: StageName,
    status: PipelineStatus,
    seconds: f64,
    report: IgnoredAny,
    error: PresentNullable<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Copy, Clone)]
#[serde(rename_all = "snake_case")]
enum StageName {
    Hollow,
    Supports,
    Emit,
}

#[derive(Debug, Deserialize)]
enum NoWaiver {}

#[derive(Debug, Deserialize, PartialEq, Eq, Copy, Clone)]
#[serde(rename_all = "snake_case")]
enum MetricId {
    Islands,
    Cavities,
    BoundaryEdges,
    NegativeShells,
    Winding,
    MaxX,
    SupportCount,
    ResinModel,
    ResinSupport,
    ResinTotal,
    ScarArea,
    ScarWorstPatch,
    ScarWeighted,
    ZHeight,
    Loadable,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Copy, Clone)]
#[serde(rename_all = "snake_case")]
enum MetricDirection {
    LowerIsBetter,
    PassFail,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Copy, Clone)]
#[serde(untagged)]
enum MetricHardBound {
    Zero(u8),
    Pass(PassBound),
}

#[derive(Debug, Deserialize, PartialEq, Eq, Copy, Clone)]
#[serde(rename_all = "lowercase")]
enum PassBound {
    Pass,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
struct MetricDefinition {
    id: MetricId,
    direction: MetricDirection,
    hard_bound: PresentNullable<MetricHardBound>,
}

/// A nullable JSON field that was present in the input. `Option<T>` alone
/// accepts an omitted key and JSON `null` alike, so the wire representation
/// below records absence separately before each parent type rejects it.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
struct PresentNullable<T>(Option<T>);

#[derive(Debug)]
struct RequiredNullable<T>(Option<Option<T>>);

impl<T> Default for RequiredNullable<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<'de, T> Deserialize<'de> for RequiredNullable<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(deserializer).map(|value| Self(Some(value)))
    }
}

impl<T> RequiredNullable<T> {
    fn require<E: serde::de::Error>(self, field: &'static str) -> Result<PresentNullable<T>, E> {
        self.0
            .map(PresentNullable)
            .ok_or_else(|| E::missing_field(field))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CorpusModelWire {
    name: String,
    stl: PathBuf,
    source_digest: SourceDigest,
    report: PathBuf,
    #[serde(default)]
    baseline_artifact: RequiredNullable<PathBuf>,
    python_exit_code: u8,
    python_seconds: f64,
    #[serde(default)]
    python_failed_stage: RequiredNullable<StageName>,
}

impl<'de> Deserialize<'de> for CorpusModel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CorpusModelWire::deserialize(deserializer)?;
        Ok(Self {
            name: wire.name,
            stl: wire.stl,
            source_digest: wire.source_digest,
            report: wire.report,
            baseline_artifact: wire.baseline_artifact.require("baseline_artifact")?,
            python_exit_code: wire.python_exit_code,
            python_seconds: wire.python_seconds,
            python_failed_stage: wire.python_failed_stage.require("python_failed_stage")?,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PipelineReportWire {
    input_stl: PathBuf,
    out_path: PathBuf,
    prepped_stl: PathBuf,
    status: PipelineStatus,
    exit_code: u8,
    #[serde(default)]
    failed_stage: RequiredNullable<StageName>,
    #[serde(default)]
    error: RequiredNullable<String>,
    seconds: f64,
    report_path: PathBuf,
    settings: PipelineSettings,
    stages: Vec<PipelineStage>,
}

impl<'de> Deserialize<'de> for PipelineReport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = PipelineReportWire::deserialize(deserializer)?;
        Ok(Self {
            input_stl: wire.input_stl,
            out_path: wire.out_path,
            prepped_stl: wire.prepped_stl,
            status: wire.status,
            exit_code: wire.exit_code,
            failed_stage: wire.failed_stage.require("failed_stage")?,
            error: wire.error.require("error")?,
            seconds: wire.seconds,
            report_path: wire.report_path,
            settings: wire.settings,
            stages: wire.stages,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PipelineStageWire {
    name: StageName,
    status: PipelineStatus,
    seconds: f64,
    report: IgnoredAny,
    #[serde(default)]
    error: RequiredNullable<String>,
}

impl<'de> Deserialize<'de> for PipelineStage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = PipelineStageWire::deserialize(deserializer)?;
        Ok(Self {
            name: wire.name,
            status: wire.status,
            seconds: wire.seconds,
            report: wire.report,
            error: wire.error.require("error")?,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MetricDefinitionWire {
    id: MetricId,
    direction: MetricDirection,
    #[serde(default)]
    hard_bound: RequiredNullable<MetricHardBound>,
}

impl<'de> Deserialize<'de> for MetricDefinition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = MetricDefinitionWire::deserialize(deserializer)?;
        Ok(Self {
            id: wire.id,
            direction: wire.direction,
            hard_bound: wire.hard_bound.require("hard_bound")?,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelNullableContractWire {
    #[serde(default)]
    baseline_artifact: RequiredNullable<PathBuf>,
    #[serde(default)]
    python_failed_stage: RequiredNullable<StageName>,
}

struct ModelNullableContract {
    baseline_artifact: PresentNullable<PathBuf>,
    python_failed_stage: PresentNullable<StageName>,
}

impl<'de> Deserialize<'de> for ModelNullableContract {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ModelNullableContractWire::deserialize(deserializer)?;
        Ok(Self {
            baseline_artifact: wire.baseline_artifact.require("baseline_artifact")?,
            python_failed_stage: wire.python_failed_stage.require("python_failed_stage")?,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportNullableContractWire {
    #[serde(default)]
    failed_stage: RequiredNullable<StageName>,
    #[serde(default)]
    error: RequiredNullable<String>,
}

struct ReportNullableContract {
    failed_stage: PresentNullable<StageName>,
    error: PresentNullable<String>,
}

impl<'de> Deserialize<'de> for ReportNullableContract {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ReportNullableContractWire::deserialize(deserializer)?;
        Ok(Self {
            failed_stage: wire.failed_stage.require("failed_stage")?,
            error: wire.error.require("error")?,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StageNullableContractWire {
    #[serde(default)]
    error: RequiredNullable<String>,
}

struct StageNullableContract {
    error: PresentNullable<String>,
}

impl<'de> Deserialize<'de> for StageNullableContract {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = StageNullableContractWire::deserialize(deserializer)?;
        Ok(Self {
            error: wire.error.require("error")?,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MetricNullableContractWire {
    #[serde(default)]
    hard_bound: RequiredNullable<MetricHardBound>,
}

struct MetricNullableContract {
    hard_bound: PresentNullable<MetricHardBound>,
}

impl<'de> Deserialize<'de> for MetricNullableContract {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = MetricNullableContractWire::deserialize(deserializer)?;
        Ok(Self {
            hard_bound: wire.hard_bound.require("hard_bound")?,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct FrozenAt {
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
}

impl<'de> Deserialize<'de> for FrozenAt {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        let bytes = encoded.as_bytes();
        if bytes.len() != 20
            || bytes[4] != b'-'
            || bytes[7] != b'-'
            || bytes[10] != b'T'
            || bytes[13] != b':'
            || bytes[16] != b':'
            || bytes[19] != b'Z'
        {
            return Err(D::Error::custom(
                "frozen_at must be UTC ISO-8601 YYYY-MM-DDTHH:MM:SSZ",
            ));
        }
        let numeric = |start, end| {
            bytes[start..end]
                .iter()
                .try_fold(0_u16, |value, byte| match byte {
                    b'0'..=b'9' => Ok(value * 10 + u16::from(byte - b'0')),
                    _ => Err(D::Error::custom(
                        "frozen_at contains a non-numeric date field",
                    )),
                })
        };
        let year = numeric(0, 4)?;
        let month = numeric(5, 7)? as u8;
        let day = numeric(8, 10)? as u8;
        let hour = numeric(11, 13)? as u8;
        let minute = numeric(14, 16)? as u8;
        let second = numeric(17, 19)? as u8;
        if !(1..=12).contains(&month)
            || day == 0
            || day > days_in_month(year, month)
            || hour > 23
            || minute > 59
            || second > 59
        {
            return Err(D::Error::custom(
                "frozen_at is not a real UTC calendar instant",
            ));
        }
        Ok(Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
        })
    }
}

const fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        _ => 0,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct BaselineTool;

impl<'de> Deserialize<'de> for BaselineTool {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let path = String::deserialize(deserializer)?;
        if path == "scripts/tools/prep_one.py" {
            Ok(Self)
        } else {
            Err(D::Error::custom(
                "baseline_tool must be scripts/tools/prep_one.py",
            ))
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct FreezeCommand;

impl<'de> Deserialize<'de> for FreezeCommand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let command = String::deserialize(deserializer)?;
        if command == "uv run --frozen python scripts/tools/prep_one.py --p8-relocate --out-dir corpus/baseline --skip-existing corpus/models/*.stl" {
            Ok(Self)
        } else {
            Err(D::Error::custom("baseline_command must preserve the exact M0 frozen batch invocation"))
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct GitRevision([u8; 20]);

impl<'de> Deserialize<'de> for GitRevision {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        if encoded.len() != 40 || !encoded.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(D::Error::custom(
                "baseline_git_rev must contain 40 hexadecimal digits",
            ));
        }
        if encoded.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return Err(D::Error::custom("baseline_git_rev must use lowercase hex"));
        }
        let mut bytes = [0_u8; 20];
        for (index, pair) in encoded.as_bytes().chunks_exact(2).enumerate() {
            let high =
                hex_nibble(pair[0]).ok_or_else(|| D::Error::custom("invalid baseline_git_rev"))?;
            let low =
                hex_nibble(pair[1]).ok_or_else(|| D::Error::custom("invalid baseline_git_rev"))?;
            bytes[index] = (high << 4) | low;
        }
        Ok(Self(bytes))
    }
}

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

const EXPECTED_FROZEN_AT: FrozenAt = FrozenAt {
    year: 2026,
    month: 8,
    day: 1,
    hour: 8,
    minute: 20,
    second: 35,
};

const EXPECTED_BASELINE_GIT_REV: GitRevision = GitRevision([
    0x6e, 0xb7, 0xf6, 0xa6, 0x5e, 0x1a, 0x34, 0x42, 0xaa, 0x86, 0x0d, 0x63, 0xd9, 0x20, 0x78, 0x8d,
    0xa7, 0x78, 0xfa, 0xed,
]);

const EXPECTED_METRICS: [MetricDefinition; 15] = [
    MetricDefinition {
        id: MetricId::Islands,
        direction: MetricDirection::LowerIsBetter,
        hard_bound: PresentNullable(None),
    },
    MetricDefinition {
        id: MetricId::Cavities,
        direction: MetricDirection::LowerIsBetter,
        hard_bound: PresentNullable(None),
    },
    MetricDefinition {
        id: MetricId::BoundaryEdges,
        direction: MetricDirection::LowerIsBetter,
        hard_bound: PresentNullable(Some(MetricHardBound::Zero(0))),
    },
    MetricDefinition {
        id: MetricId::NegativeShells,
        direction: MetricDirection::LowerIsBetter,
        hard_bound: PresentNullable(Some(MetricHardBound::Zero(0))),
    },
    MetricDefinition {
        id: MetricId::Winding,
        direction: MetricDirection::LowerIsBetter,
        hard_bound: PresentNullable(Some(MetricHardBound::Zero(0))),
    },
    MetricDefinition {
        id: MetricId::MaxX,
        direction: MetricDirection::LowerIsBetter,
        hard_bound: PresentNullable(None),
    },
    MetricDefinition {
        id: MetricId::SupportCount,
        direction: MetricDirection::LowerIsBetter,
        hard_bound: PresentNullable(None),
    },
    MetricDefinition {
        id: MetricId::ResinModel,
        direction: MetricDirection::LowerIsBetter,
        hard_bound: PresentNullable(None),
    },
    MetricDefinition {
        id: MetricId::ResinSupport,
        direction: MetricDirection::LowerIsBetter,
        hard_bound: PresentNullable(None),
    },
    MetricDefinition {
        id: MetricId::ResinTotal,
        direction: MetricDirection::LowerIsBetter,
        hard_bound: PresentNullable(None),
    },
    MetricDefinition {
        id: MetricId::ScarArea,
        direction: MetricDirection::LowerIsBetter,
        hard_bound: PresentNullable(None),
    },
    MetricDefinition {
        id: MetricId::ScarWorstPatch,
        direction: MetricDirection::LowerIsBetter,
        hard_bound: PresentNullable(None),
    },
    MetricDefinition {
        id: MetricId::ScarWeighted,
        direction: MetricDirection::LowerIsBetter,
        hard_bound: PresentNullable(None),
    },
    MetricDefinition {
        id: MetricId::ZHeight,
        direction: MetricDirection::LowerIsBetter,
        hard_bound: PresentNullable(None),
    },
    MetricDefinition {
        id: MetricId::Loadable,
        direction: MetricDirection::PassFail,
        hard_bound: PresentNullable(Some(MetricHardBound::Pass(PassBound::Pass))),
    },
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

fn assert_required_nullable_field<T>(label: &str, explicit_null: &str, missing: &[&str]) -> T
where
    T: for<'de> Deserialize<'de>,
{
    let parsed = serde_json::from_str(explicit_null)
        .unwrap_or_else(|error| panic!("{label}: explicit JSON null must deserialize: {error}"));
    for omitted in missing {
        assert!(
            serde_json::from_str::<T>(omitted).is_err(),
            "{label}: omission must fail: {omitted}"
        );
    }
    parsed
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

fn expected_prepped_stl(name: &str) -> PathBuf {
    PathBuf::from("corpus/baseline").join(format!("{name}.prepped.stl"))
}

#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
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
    assert!(
        !settings.internal_support_verifier_configured,
        "{name}: baseline unexpectedly configured an internal-support verifier"
    );
    assert!(
        settings.a5_internal_supports_enabled,
        "{name}: baseline unexpectedly disabled A5 internal supports"
    );
}

fn stage_index(stage: StageName) -> usize {
    match stage {
        StageName::Hollow => 0,
        StageName::Supports => 1,
        StageName::Emit => 2,
    }
}

fn assert_report_stages(name: &str, report: &PipelineReport) {
    const ORDER: [StageName; 3] = [StageName::Hollow, StageName::Supports, StageName::Emit];

    assert!(
        report.seconds.is_finite() && report.seconds >= 0.0,
        "{name}: report total seconds must be finite and nonnegative"
    );
    let terminal_index = match report.failed_stage.0 {
        None => {
            assert_eq!(report.exit_code, 0, "{name}: success must have exit code 0");
            assert!(
                report.error.0.is_none(),
                "{name}: success must not have an error"
            );
            ORDER.len() - 1
        }
        Some(stage) => {
            assert_ne!(
                report.exit_code, 0,
                "{name}: failure must have nonzero exit code"
            );
            assert!(
                report
                    .error
                    .0
                    .as_deref()
                    .is_some_and(|error| !error.is_empty()),
                "{name}: failed report must carry a nonempty error"
            );
            stage_index(stage)
        }
    };
    assert_eq!(
        report.stages.len(),
        terminal_index + 1,
        "{name}: stage sequence must end at its terminal stage"
    );

    let mut summed_seconds = 0.0;
    for (index, stage) in report.stages.iter().enumerate() {
        assert_eq!(stage.name, ORDER[index], "{name}: stage order");
        let expected_status = if report.failed_stage.0.is_some() && index == terminal_index {
            PipelineStatus::Failed
        } else {
            PipelineStatus::Ok
        };
        assert_eq!(stage.status, expected_status, "{name}: stage status");
        assert!(
            stage.seconds.is_finite() && stage.seconds >= 0.0,
            "{name}: stage timing must be finite and nonnegative"
        );
        if matches!(stage.status, PipelineStatus::Ok) {
            assert!(
                stage.error.0.is_none(),
                "{name}: successful stage has an error"
            );
        } else {
            assert!(
                stage
                    .error
                    .0
                    .as_deref()
                    .is_some_and(|error| !error.is_empty()),
                "{name}: failed stage lacks an error"
            );
        }
        let _ = &stage.report;
        summed_seconds += stage.seconds;
    }

    // Pipeline reports store each stage and the total independently as decimal
    // seconds. Each persisted stage can be rounded to centiseconds, so permit
    // half a centisecond per stage plus the independently rounded total.
    let allowed_rounding = (report.stages.len() as f64 + 1.0) * 0.005 + f64::EPSILON;
    assert!(
        (summed_seconds - report.seconds).abs() <= allowed_rounding,
        "{name}: stage timings {summed_seconds} disagree with total {} beyond persisted rounding",
        report.seconds
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
fn required_nullable_contract_keys_accept_null_and_reject_omission() {
    let model: ModelNullableContract = assert_required_nullable_field(
        "model nullable keys",
        r#"{"baseline_artifact":null,"python_failed_stage":null}"#,
        &[
            r#"{"python_failed_stage":null}"#,
            r#"{"baseline_artifact":null}"#,
        ],
    );
    assert!(model.baseline_artifact.0.is_none());
    assert!(model.python_failed_stage.0.is_none());

    let report: ReportNullableContract = assert_required_nullable_field(
        "report nullable keys",
        r#"{"failed_stage":null,"error":null}"#,
        &[r#"{"error":null}"#, r#"{"failed_stage":null}"#],
    );
    assert!(report.failed_stage.0.is_none());
    assert!(report.error.0.is_none());

    let stage: StageNullableContract =
        assert_required_nullable_field("stage error", r#"{"error":null}"#, &[r#"{}"#]);
    assert!(stage.error.0.is_none());

    let metric: MetricNullableContract =
        assert_required_nullable_field("metric hard_bound", r#"{"hard_bound":null}"#, &[r#"{}"#]);
    assert!(metric.hard_bound.0.is_none());
}

#[test]
fn frozen_baseline_manifest_and_artifacts_are_complete_and_truthful() {
    let root = repo_root();
    let manifest_path = root.join("corpus/manifest.json");
    let manifest: CorpusManifest = read_json(&manifest_path, "frozen corpus manifest");

    assert_eq!(manifest.schema, "plinth/corpus-manifest");
    assert_eq!(manifest.version, 1);
    assert_eq!(
        manifest.frozen_at, EXPECTED_FROZEN_AT,
        "baseline freeze instant"
    );
    assert_eq!(
        manifest.baseline_tool, BaselineTool,
        "baseline tool identity changed"
    );
    assert_eq!(
        manifest.baseline_command, FreezeCommand,
        "baseline command must retain --p8-relocate and the binding batch glob"
    );
    assert_eq!(
        manifest.baseline_git_rev, EXPECTED_BASELINE_GIT_REV,
        "baseline source revision changed"
    );
    assert_eq!(
        manifest.metrics.as_slice(),
        EXPECTED_METRICS,
        "M0 metric set changed; max_x_group belongs only at M5"
    );
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
            report.failed_stage.0, model.python_failed_stage.0,
            "{name}: failed stage"
        );
        assert_eq!(
            report.seconds.to_bits(),
            model.python_seconds.to_bits(),
            "{name}: manifest seconds must be the exact stored report seconds"
        );
        assert_eq!(
            report.out_path,
            expected_artifact(name),
            "{name}: output path"
        );
        assert_eq!(
            report.prepped_stl,
            expected_prepped_stl(name),
            "{name}: prepped STL path"
        );
        assert_freeze_settings(name, &report.settings);
        assert_report_stages(name, &report);

        let disk_artifact = root.join(&report.out_path);
        if model.python_exit_code == 0 {
            assert!(
                matches!(report.status, PipelineStatus::Ok),
                "{name}: successful exit has non-ok status"
            );
            assert!(
                model.python_failed_stage.0.is_none(),
                "{name}: successful exit has a failed stage"
            );
            assert_eq!(
                model.baseline_artifact.0.as_deref(),
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
                model.python_failed_stage.0.is_some(),
                "{name}: failed exit lacks a named failed stage"
            );
            assert!(
                model.baseline_artifact.0.is_none(),
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
