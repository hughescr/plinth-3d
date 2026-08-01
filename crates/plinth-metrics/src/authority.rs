//! Fresh, file-authoritative validation of score waivers.
//!
//! A waiver is not an `EvidenceSource` supplied by a caller.  It exists only
//! after this module reopens the selected manifest and the repository's single
//! append-only decision-gate table, then finds one exact, signed owner row on
//! each side.  This is procedural repository authority, not cryptography: its
//! trust boundary is the reviewed checkout and a recorded Craig Hughes signoff.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use plinth_core::{CalendarDate, ModelId};
use serde_json::Value;
use thiserror::Error;

use crate::score::{expand_target, ValidatedWaiver};
use crate::ValidatedWaivers;

const GATE_TABLE_PATH: &str = "docs/rust-migration/06-conventions.md";
const GATE_HEADER: [&str; 10] = [
    "id",
    "kind",
    "recorded",
    "authority",
    "status",
    "evidence_date",
    "resolution",
    "source",
    "locator",
    "waiver",
];
const FORMAT_METRICS: [&str; 5] = [
    "boundary_edges",
    "negative_shells",
    "winding",
    "cavities",
    "loadable",
];
const EXPECTED_MODELS: [&str; 8] = [
    "Sphere",
    "Cylinder",
    "make_box",
    "make_overhang_table",
    "make_toed_slab",
    "make_raking_blade",
    "make_thin_fin",
    "Weeping Treant v1.2",
];
const EXPECTED_METRICS: [(&str, &str, Option<&str>); 15] = [
    ("islands", "lower_is_better", None),
    ("cavities", "lower_is_better", None),
    ("boundary_edges", "lower_is_better", Some("0")),
    ("negative_shells", "lower_is_better", Some("0")),
    ("winding", "lower_is_better", Some("0")),
    ("max_x", "lower_is_better", None),
    ("support_count", "lower_is_better", None),
    ("resin_model", "lower_is_better", None),
    ("resin_support", "lower_is_better", None),
    ("resin_total", "lower_is_better", None),
    ("scar_area", "lower_is_better", None),
    ("scar_worst_patch", "lower_is_better", None),
    ("scar_weighted", "lower_is_better", None),
    ("z_height", "lower_is_better", None),
    ("loadable", "pass_fail", Some("pass")),
];

/// Failures while deriving an opaque waiver set from the canonical two-file
/// authority.  Neither an in-memory source nor a caller-created DTO can pass
/// this boundary.
#[derive(Debug, Error)]
pub enum AuthorityError {
    #[error("repository root {path} is not a canonical directory")]
    RepositoryRoot { path: PathBuf },
    #[error("selected manifest {manifest} is outside canonical repository root {root}")]
    ManifestOutsideRoot { root: PathBuf, manifest: PathBuf },
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("manifest {path} is not valid JSON: {source}")]
    ManifestJson {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("manifest field {field} is missing or has the wrong JSON shape")]
    ManifestField { field: &'static str },
    #[error("manifest schema is not plinth/corpus-manifest version 1")]
    ManifestSchema,
    #[error("manifest metric table does not have the required M1 row at index {index}")]
    ManifestMetric { index: usize },
    #[error("manifest model table does not have the required corpus member at index {index}")]
    ManifestModel { index: usize },
    #[error("decision gate table heading is missing from {path}")]
    GateHeading { path: PathBuf },
    #[error("decision gate table header is malformed at line {line}")]
    GateHeader { line: usize },
    #[error("decision gate table row at line {line} is malformed")]
    GateRow { line: usize },
    #[error("decision gate table ID {id} is duplicated")]
    DuplicateGateId { id: String },
    #[error("decision gate table row {id} has an invalid date in {field}")]
    GateDate { id: String, field: &'static str },
    #[error("decision gate table row {id} has an unknown kind")]
    GateKind { id: String },
    #[error("decision gate table row {id} has an invalid non-waiver payload")]
    GatePayload { id: String },
    #[error("gate-table waiver {id} is not a signed Craig Hughes owner waiver")]
    GateSignature { id: String },
    #[error("manifest waiver entry at index {index} is malformed")]
    ManifestWaiver { index: usize },
    #[error("manifest waiver {id} is duplicated")]
    DuplicateManifestWaiver { id: String },
    #[error("manifest waiver {id} has no matching signed gate-table row")]
    ManifestOnlyWaiver { id: String },
    #[error("signed gate-table waiver {id} is missing from the manifest")]
    TableOnlyWaiver { id: String },
    #[error("manifest waiver {id} does not exactly match its gate-table row")]
    WaiverMismatch { id: String },
    #[error("waiver {id} names a model outside the fixed corpus")]
    WaiverModel { id: String },
    #[error("waiver {id} has an unknown, protected, or duplicate metric target")]
    WaiverTarget { id: String },
    #[error("waiver {id} expired on {expires_on}")]
    WaiverExpired { id: String, expires_on: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GateRow {
    id: String,
    kind: String,
    authority: String,
    status: String,
    evidence_date: String,
    waiver: Option<WaiverFields>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WaiverFields {
    model: String,
    metric: String,
    scope: String,
    reason: String,
    expires_on: String,
}

/// Reopens `manifest_path` and `<repo>/docs/rust-migration/06-conventions.md`
/// from disk and derives the only nonempty [`ValidatedWaivers`] constructor.
/// `as_of` is explicit so expiry behaviour is deterministic and testable.
pub fn load_validated_waivers(
    repository_root: &Path,
    manifest_path: &Path,
    as_of: &CalendarDate,
) -> Result<ValidatedWaivers, AuthorityError> {
    let root = fs::canonicalize(repository_root).map_err(|_| AuthorityError::RepositoryRoot {
        path: repository_root.to_path_buf(),
    })?;
    if !root.is_dir() {
        return Err(AuthorityError::RepositoryRoot { path: root });
    }
    let manifest = fs::canonicalize(manifest_path).map_err(|source| AuthorityError::Read {
        path: manifest_path.to_path_buf(),
        source,
    })?;
    if !manifest.starts_with(&root) {
        return Err(AuthorityError::ManifestOutsideRoot { root, manifest });
    }
    let document = root.join(GATE_TABLE_PATH);
    let gate_text = fs::read_to_string(&document).map_err(|source| AuthorityError::Read {
        path: document,
        source,
    })?;
    let gate_rows = parse_gate_table(&gate_text, &root.join(GATE_TABLE_PATH))?;
    let manifest_text = fs::read_to_string(&manifest).map_err(|source| AuthorityError::Read {
        path: manifest.clone(),
        source,
    })?;
    let manifest_json =
        serde_json::from_str(&manifest_text).map_err(|source| AuthorityError::ManifestJson {
            path: manifest,
            source,
        })?;
    let manifest_waivers = validate_manifest(&manifest_json)?;
    validate_waivers(gate_rows, manifest_waivers, as_of)
}

fn parse_gate_table(text: &str, path: &Path) -> Result<Vec<GateRow>, AuthorityError> {
    let lines: Vec<(usize, &str)> = text
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line))
        .collect();
    let heading = lines
        .iter()
        .position(|(_, line)| *line == "## Decision gate table")
        .ok_or_else(|| AuthorityError::GateHeading {
            path: path.to_path_buf(),
        })?;
    let header_index = lines
        .iter()
        .enumerate()
        .skip(heading + 1)
        .find_map(|(index, (_, line))| line.starts_with('|').then_some(index))
        .ok_or(AuthorityError::GateHeader { line: heading + 2 })?;
    let (header_line, header) = lines
        .get(header_index)
        .ok_or(AuthorityError::GateHeader { line: heading + 2 })?;
    let header_line_number = *header_line;
    if cells(header).as_deref() != Some(&GATE_HEADER) {
        return Err(AuthorityError::GateHeader {
            line: header_line_number,
        });
    }
    let (separator_line, separator) =
        lines
            .get(header_index + 1)
            .ok_or(AuthorityError::GateHeader {
                line: header_line_number + 1,
            })?;
    if !cells(separator)
        .map(|row| row.iter().all(|cell| *cell == "---"))
        .unwrap_or(false)
    {
        return Err(AuthorityError::GateHeader {
            line: *separator_line,
        });
    }
    let mut rows = Vec::new();
    let mut ids = BTreeSet::new();
    for (line, raw) in lines.iter().copied().skip(header_index + 2) {
        if !raw.starts_with('|') {
            break;
        }
        let row = cells(raw).ok_or(AuthorityError::GateRow { line })?;
        if row.len() != GATE_HEADER.len() || row.iter().any(|cell| cell.is_empty()) {
            return Err(AuthorityError::GateRow { line });
        }
        let id = row[0].to_owned();
        if !ids.insert(id.clone()) {
            return Err(AuthorityError::DuplicateGateId { id });
        }
        CalendarDate::new(row[2]).map_err(|_| AuthorityError::GateDate {
            id: id.clone(),
            field: "recorded",
        })?;
        validate_evidence_date(&id, row[5])?;
        if !matches!(row[1], "decision" | "override" | "historical" | "waiver") {
            return Err(AuthorityError::GateKind { id });
        }
        let waiver = if row[1] == "waiver" {
            Some(parse_waiver_fields(&id, row[9])?)
        } else {
            if row[9] != "-" {
                return Err(AuthorityError::GatePayload { id });
            }
            None
        };
        rows.push(GateRow {
            id,
            kind: row[1].to_owned(),
            authority: row[3].to_owned(),
            status: row[4].to_owned(),
            evidence_date: row[5].to_owned(),
            waiver,
        });
    }
    if rows.is_empty() {
        return Err(AuthorityError::GateHeader {
            line: header_line_number,
        });
    }
    Ok(rows)
}

fn cells(line: &str) -> Option<Vec<&str>> {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
        return None;
    }
    let values: Vec<_> = trimmed[1..trimmed.len() - 1]
        .split('|')
        .map(str::trim)
        .collect();
    (values.len() == GATE_HEADER.len()).then_some(values)
}

fn validate_evidence_date(id: &str, value: &str) -> Result<(), AuthorityError> {
    let (precision, date) = if value == "unknown" {
        return Ok(());
    } else if let Some(date) = value.strip_prefix("exact:") {
        ("exact", date)
    } else if let Some(date) = value.strip_prefix("on_or_before:") {
        ("on_or_before", date)
    } else {
        return Err(AuthorityError::GateDate {
            id: id.to_owned(),
            field: "evidence_date",
        });
    };
    let _ = precision;
    CalendarDate::new(date).map_err(|_| AuthorityError::GateDate {
        id: id.to_owned(),
        field: "evidence_date",
    })?;
    Ok(())
}

fn parse_waiver_fields(id: &str, value: &str) -> Result<WaiverFields, AuthorityError> {
    let fields: Vec<_> = value.split(';').map(str::trim).collect();
    if fields.len() != 5 || fields.iter().any(|field| field.is_empty() || *field == "-") {
        return Err(AuthorityError::GatePayload { id: id.to_owned() });
    }
    CalendarDate::new(fields[4]).map_err(|_| AuthorityError::GateDate {
        id: id.to_owned(),
        field: "waiver expires",
    })?;
    Ok(WaiverFields {
        model: fields[0].to_owned(),
        metric: fields[1].to_owned(),
        scope: fields[2].to_owned(),
        reason: fields[3].to_owned(),
        expires_on: fields[4].to_owned(),
    })
}

fn validate_manifest(value: &Value) -> Result<Vec<(String, WaiverFields, String)>, AuthorityError> {
    let object = value
        .as_object()
        .ok_or(AuthorityError::ManifestField { field: "root" })?;
    if object.get("schema").and_then(Value::as_str) != Some("plinth/corpus-manifest")
        || object.get("version").and_then(Value::as_u64) != Some(1)
    {
        return Err(AuthorityError::ManifestSchema);
    }
    let metrics = object
        .get("metrics")
        .and_then(Value::as_array)
        .ok_or(AuthorityError::ManifestField { field: "metrics" })?;
    if metrics.len() != EXPECTED_METRICS.len() {
        return Err(AuthorityError::ManifestMetric {
            index: metrics.len(),
        });
    }
    for (index, (row, expected)) in metrics.iter().zip(EXPECTED_METRICS).enumerate() {
        let Some(row) = row.as_object() else {
            return Err(AuthorityError::ManifestMetric { index });
        };
        let hard_bound = match row.get("hard_bound") {
            Some(Value::Null) => None,
            Some(Value::Number(number)) => Some(number.to_string()),
            Some(Value::String(value)) => Some(value.clone()),
            _ => return Err(AuthorityError::ManifestMetric { index }),
        };
        if row.get("id").and_then(Value::as_str) != Some(expected.0)
            || row.get("direction").and_then(Value::as_str) != Some(expected.1)
            || hard_bound.as_deref() != expected.2
        {
            return Err(AuthorityError::ManifestMetric { index });
        }
    }
    let models = object
        .get("models")
        .and_then(Value::as_array)
        .ok_or(AuthorityError::ManifestField { field: "models" })?;
    if models.len() != EXPECTED_MODELS.len() {
        return Err(AuthorityError::ManifestModel {
            index: models.len(),
        });
    }
    for (index, (row, expected)) in models.iter().zip(EXPECTED_MODELS).enumerate() {
        if row
            .as_object()
            .and_then(|row| row.get("name"))
            .and_then(Value::as_str)
            != Some(expected)
        {
            return Err(AuthorityError::ManifestModel { index });
        }
    }
    let waivers = object
        .get("waivers")
        .and_then(Value::as_array)
        .ok_or(AuthorityError::ManifestField { field: "waivers" })?;
    let mut parsed = Vec::with_capacity(waivers.len());
    let mut ids = BTreeSet::new();
    for (index, raw) in waivers.iter().enumerate() {
        let Some(raw) = raw.as_object() else {
            return Err(AuthorityError::ManifestWaiver { index });
        };
        let field = |name| raw.get(name).and_then(Value::as_str).map(str::to_owned);
        let (
            Some(id),
            Some(date),
            Some(model),
            Some(metric),
            Some(scope),
            Some(reason),
            Some(expires_on),
        ) = (
            field("id"),
            field("date"),
            field("model"),
            field("metric"),
            field("scope"),
            field("reason"),
            field("expires_on"),
        )
        else {
            return Err(AuthorityError::ManifestWaiver { index });
        };
        if !ids.insert(id.clone()) {
            return Err(AuthorityError::DuplicateManifestWaiver { id });
        }
        CalendarDate::new(&date).map_err(|_| AuthorityError::ManifestWaiver { index })?;
        CalendarDate::new(&expires_on).map_err(|_| AuthorityError::ManifestWaiver { index })?;
        parsed.push((
            id,
            WaiverFields {
                model,
                metric,
                scope,
                reason,
                expires_on,
            },
            date,
        ));
    }
    Ok(parsed)
}

fn validate_waivers(
    gate_rows: Vec<GateRow>,
    manifest_waivers: Vec<(String, WaiverFields, String)>,
    as_of: &CalendarDate,
) -> Result<ValidatedWaivers, AuthorityError> {
    let mut gates = BTreeMap::new();
    for gate in gate_rows.into_iter().filter(|row| row.kind == "waiver") {
        if gate.authority != "owner:Craig Hughes" || gate.status != "signed" {
            return Err(AuthorityError::GateSignature { id: gate.id });
        }
        let Some(waiver) = gate.waiver else {
            return Err(AuthorityError::GatePayload { id: gate.id });
        };
        let date = gate
            .evidence_date
            .strip_prefix("exact:")
            .ok_or_else(|| AuthorityError::GateDate {
                id: gate.id.clone(),
                field: "evidence_date",
            })?
            .to_owned();
        gates.insert(gate.id, (waiver, date));
    }
    let manifest_ids: BTreeSet<_> = manifest_waivers
        .iter()
        .map(|(id, _, _)| id.as_str())
        .collect();
    for id in gates.keys() {
        if !manifest_ids.contains(id.as_str()) {
            return Err(AuthorityError::TableOnlyWaiver { id: id.clone() });
        }
    }
    let known_models: BTreeSet<_> = EXPECTED_MODELS.into_iter().collect();
    let mut entries = Vec::with_capacity(manifest_waivers.len());
    let mut expanded = BTreeSet::new();
    for (id, waiver, date) in manifest_waivers {
        let Some((gate, gate_date)) = gates.get(&id) else {
            return Err(AuthorityError::ManifestOnlyWaiver { id });
        };
        if &waiver != gate || date != *gate_date {
            return Err(AuthorityError::WaiverMismatch { id });
        }
        if !known_models.contains(waiver.model.as_str()) {
            return Err(AuthorityError::WaiverModel { id });
        }
        if waiver.expires_on.as_str() < as_of.as_str() {
            return Err(AuthorityError::WaiverExpired {
                id,
                expires_on: waiver.expires_on,
            });
        }
        let ids = expand_target(&waiver.metric)
            .map_err(|_| AuthorityError::WaiverTarget { id: id.clone() })?;
        if ids.iter().any(|metric| FORMAT_METRICS.contains(metric)) {
            return Err(AuthorityError::WaiverTarget { id });
        }
        let model = ModelId::from(waiver.model);
        for metric in &ids {
            if !expanded.insert((model.clone(), (*metric).to_owned())) {
                return Err(AuthorityError::WaiverTarget { id });
            }
        }
        entries.push(ValidatedWaiver { model, ids });
    }
    Ok(ValidatedWaivers::from_authority(entries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn date() -> CalendarDate {
        CalendarDate::new("2026-08-01").unwrap()
    }

    fn root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("plinth-authority-{name}-{nonce}"));
        fs::create_dir_all(root.join("docs/rust-migration")).unwrap();
        root
    }

    fn manifest(waivers: &str) -> String {
        format!(
            r#"{{"schema":"plinth/corpus-manifest","version":1,"metrics":[{}],"models":[{}],"waivers":{waivers}}}"#,
            EXPECTED_METRICS
                .iter()
                .map(|(id, direction, hard)| match hard {
                    None =>
                        format!(r#"{{"id":"{id}","direction":"{direction}","hard_bound":null}}"#),
                    Some("0") =>
                        format!(r#"{{"id":"{id}","direction":"{direction}","hard_bound":0}}"#),
                    Some(value) => format!(
                        r#"{{"id":"{id}","direction":"{direction}","hard_bound":"{value}"}}"#
                    ),
                })
                .collect::<Vec<_>>()
                .join(","),
            EXPECTED_MODELS
                .iter()
                .map(|name| format!(r#"{{"name":"{name}"}}"#))
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn table(extra: &str) -> String {
        format!(
            "## Decision gate table\n\ntext\n\n| {} |\n| {} |\n| H-1 | historical | 2026-08-01 | guide | guide-ratified | exact:2026-08-01 | historical record | docs/x.md | x | - |\n{extra}",
            GATE_HEADER.join(" | "),
            std::iter::repeat_n("---", GATE_HEADER.len()).collect::<Vec<_>>().join(" | ")
        )
    }

    fn write_fixture(root: &Path, doc: &str, waivers: &str) -> PathBuf {
        fs::write(root.join(GATE_TABLE_PATH), doc).unwrap();
        let manifest_path = root.join("corpus.json");
        fs::write(&manifest_path, manifest(waivers)).unwrap();
        manifest_path
    }

    #[test]
    fn current_canonical_files_are_empty_and_exact() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let manifest = root.join("corpus/manifest.json");
        assert_eq!(
            load_validated_waivers(&root, &manifest, &date()).unwrap(),
            ValidatedWaivers::empty()
        );
    }

    #[test]
    fn malformed_or_missing_table_fails() {
        let root = root("bad-table");
        let manifest = write_fixture(&root, "# no table", "[]");
        assert!(matches!(
            load_validated_waivers(&root, &manifest, &date()),
            Err(AuthorityError::GateHeading { .. })
        ));
        fs::write(
            root.join(GATE_TABLE_PATH),
            "## Decision gate table\n\ntext\n\n| nope |\n",
        )
        .unwrap();
        assert!(matches!(
            load_validated_waivers(&root, &manifest, &date()),
            Err(AuthorityError::GateHeader { .. })
        ));
    }

    #[test]
    fn manifest_schema_rejects_fabricated_owner_decision_json() {
        let root = root("fabricated");
        let manifest = write_fixture(
            &root,
            &table(""),
            r#"[{"id":"W-1","signature":{"kind":"owner_decision"}}]"#,
        );
        assert!(matches!(
            load_validated_waivers(&root, &manifest, &date()),
            Err(AuthorityError::ManifestWaiver { .. })
        ));
    }

    #[test]
    fn manifest_only_and_table_only_waivers_fail() {
        let root = root("one-sided");
        let one = r#"[{"id":"W-1","date":"2026-08-01","model":"Sphere","metric":"scar","scope":"M7","reason":"test","expires_on":"2026-08-02"}]"#;
        let manifest = write_fixture(&root, &table(""), one);
        assert!(matches!(
            load_validated_waivers(&root, &manifest, &date()),
            Err(AuthorityError::ManifestOnlyWaiver { .. })
        ));
        let signed = "| W-1 | waiver | 2026-08-01 | owner:Craig Hughes | signed | exact:2026-08-01 | signed test | docs/x.md | x | Sphere;scar;M7;test;2026-08-02 |\n";
        let manifest = write_fixture(&root, &table(signed), "[]");
        assert!(matches!(
            load_validated_waivers(&root, &manifest, &date()),
            Err(AuthorityError::TableOnlyWaiver { .. })
        ));
    }

    #[test]
    fn unsigned_and_nonowner_historical_rows_are_not_waivers() {
        let root = root("signature");
        let one = r#"[{"id":"W-1","date":"2026-08-01","model":"Sphere","metric":"scar","scope":"M7","reason":"test","expires_on":"2026-08-02"}]"#;
        let unsigned = "| W-1 | waiver | 2026-08-01 | guide | guide-ratified | exact:2026-08-01 | not signed | docs/x.md | x | Sphere;scar;M7;test;2026-08-02 |\n";
        let manifest = write_fixture(&root, &table(unsigned), one);
        assert!(matches!(
            load_validated_waivers(&root, &manifest, &date()),
            Err(AuthorityError::GateSignature { .. })
        ));
    }

    #[test]
    fn exact_valid_signed_waiver_is_opaque_and_comparator_honors_it() {
        let root = root("valid");
        let one = r#"[{"id":"W-1","date":"2026-08-01","model":"Sphere","metric":"resin","scope":"M7","reason":"test","expires_on":"2026-08-02"}]"#;
        let signed = "| W-1 | waiver | 2026-08-01 | owner:Craig Hughes | signed | exact:2026-08-01 | signed test | docs/x.md | x | Sphere;resin;M7;test;2026-08-02 |\n";
        let manifest = write_fixture(&root, &table(signed), one);
        let waivers = load_validated_waivers(&root, &manifest, &date()).unwrap();
        assert_ne!(waivers, ValidatedWaivers::empty());
        let baseline = crate::score::scored_for_authority_test("Sphere", 1.0);
        let candidate = crate::score::scored_for_authority_test("Sphere", 2.0);
        let report = crate::compare_artifacts(
            &[baseline],
            &[candidate],
            &crate::ComparisonPolicy {
                milestone: crate::ComparisonMilestone::M7,
                scope: crate::ComparisonScope::Candidate,
                waivers: &waivers,
            },
        )
        .unwrap();
        assert!(report
            .rows()
            .iter()
            .filter(|row| row.id.as_str().starts_with("resin_"))
            .all(|row| row.waived));
    }

    #[test]
    fn mismatch_duplicate_expiry_model_and_metric_are_rejected() {
        let root = root("reject");
        let signed = "| W-1 | waiver | 2026-08-01 | owner:Craig Hughes | signed | exact:2026-08-01 | signed test | docs/x.md | x | Sphere;scar;M7;test;2026-08-02 |\n";
        let mismatch = r#"[{"id":"W-1","date":"2026-08-01","model":"Cylinder","metric":"scar","scope":"M7","reason":"test","expires_on":"2026-08-02"}]"#;
        let manifest = write_fixture(&root, &table(signed), mismatch);
        assert!(matches!(
            load_validated_waivers(&root, &manifest, &date()),
            Err(AuthorityError::WaiverMismatch { .. })
        ));
        let expired = r#"[{"id":"W-1","date":"2026-08-01","model":"Sphere","metric":"scar","scope":"M7","reason":"test","expires_on":"2026-07-31"}]"#;
        let expired_table = "| W-1 | waiver | 2026-08-01 | owner:Craig Hughes | signed | exact:2026-08-01 | signed test | docs/x.md | x | Sphere;scar;M7;test;2026-07-31 |\n";
        let manifest = write_fixture(&root, &table(expired_table), expired);
        assert!(matches!(
            load_validated_waivers(&root, &manifest, &date()),
            Err(AuthorityError::WaiverExpired { .. })
        ));
        let target = r#"[{"id":"W-1","date":"2026-08-01","model":"Sphere","metric":"format","scope":"M7","reason":"test","expires_on":"2026-08-02"}]"#;
        let target_table = "| W-1 | waiver | 2026-08-01 | owner:Craig Hughes | signed | exact:2026-08-01 | signed test | docs/x.md | x | Sphere;format;M7;test;2026-08-02 |\n";
        let manifest = write_fixture(&root, &table(target_table), target);
        assert!(matches!(
            load_validated_waivers(&root, &manifest, &date()),
            Err(AuthorityError::WaiverTarget { .. })
        ));
        let unknown_model = r#"[{"id":"W-1","date":"2026-08-01","model":"not-in-corpus","metric":"scar","scope":"M7","reason":"test","expires_on":"2026-08-02"}]"#;
        let unknown_model_table = "| W-1 | waiver | 2026-08-01 | owner:Craig Hughes | signed | exact:2026-08-01 | signed test | docs/x.md | x | not-in-corpus;scar;M7;test;2026-08-02 |\n";
        let manifest = write_fixture(&root, &table(unknown_model_table), unknown_model);
        assert!(matches!(
            load_validated_waivers(&root, &manifest, &date()),
            Err(AuthorityError::WaiverModel { .. })
        ));
        let duplicate = r#"[{"id":"W-1","date":"2026-08-01","model":"Sphere","metric":"scar","scope":"M7","reason":"test","expires_on":"2026-08-02"},{"id":"W-1","date":"2026-08-01","model":"Sphere","metric":"scar","scope":"M7","reason":"test","expires_on":"2026-08-02"}]"#;
        let manifest = write_fixture(&root, &table(signed), duplicate);
        assert!(matches!(
            load_validated_waivers(&root, &manifest, &date()),
            Err(AuthorityError::DuplicateManifestWaiver { .. })
        ));
    }

    #[test]
    fn manifest_invariants_reject_order_directions_and_wrong_model() {
        let root = root("manifest");
        let manifest_path = write_fixture(&root, &table(""), "[]");
        let bad = manifest("[]").replace("lower_is_better", "pass_fail");
        fs::write(&manifest_path, bad).unwrap();
        assert!(matches!(
            load_validated_waivers(&root, &manifest_path, &date()),
            Err(AuthorityError::ManifestMetric { .. })
        ));
        let wrong_model = manifest("[]").replace("Weeping Treant v1.2", "Cone");
        fs::write(&manifest_path, wrong_model).unwrap();
        assert!(matches!(
            load_validated_waivers(&root, &manifest_path, &date()),
            Err(AuthorityError::ManifestModel { .. })
        ));
    }
}
