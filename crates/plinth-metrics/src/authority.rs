//! Fresh disk authority for the only waiver-capable comparison API.
//!
//! This is intentionally procedural rather than cryptographic. The trust
//! boundary is the canonical checked-out repository, its append-only gate
//! record, and Craig Hughes's signed row. Authority is reopened immediately
//! for every comparison; no public token can survive a later file edit.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use plinth_core::{CalendarDate, ModelId};
use serde::Deserialize;
use thiserror::Error;

use crate::score::{compare_artifacts_with_authorized_waivers, AuthorizedWaiver};
use crate::{
    ComparisonError, ComparisonMilestone, ComparisonPolicy, ComparisonReport, ScoredArtifact,
};

const GATE_DOCUMENT_RELATIVE: &str = "docs/rust-migration/06-conventions.md";
const MAIN_START: &str = "<!-- PLINTH_GATE_MAIN_START -->";
const MAIN_END: &str = "<!-- PLINTH_GATE_MAIN_END -->";
const CORRECTION_START: &str = "<!-- PLINTH_GATE_CORRECTIONS_START -->";
const CORRECTION_END: &str = "<!-- PLINTH_GATE_CORRECTIONS_END -->";
const MAIN_HEADER: [&str; 10] = [
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
const CORRECTION_HEADER: [&str; 12] = [
    "correction_id",
    "target_id",
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

#[derive(Debug, Error)]
pub enum AuthorityError {
    #[error("repository root {path} is not a canonical directory")]
    RepositoryRoot { path: PathBuf },
    #[error("authority file {path} does not resolve to the canonical repository document")]
    GatePath { path: PathBuf },
    #[error("selected manifest {manifest} is outside canonical repository root {root}")]
    ManifestOutsideRoot { root: PathBuf, manifest: PathBuf },
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("manifest {path} has an invalid strict schema: {source}")]
    ManifestSchema {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("manifest {path} strict schema is missing required nullable field {field}")]
    ManifestMissingField { path: PathBuf, field: &'static str },
    #[error("manifest metric table differs from the exact M1 table at index {index}")]
    ManifestMetric { index: usize },
    #[error("manifest model table differs from the exact corpus at index {index}")]
    ManifestModel { index: usize },
    #[error("gate table marker {marker} occurs {count} times")]
    MarkerCount { marker: &'static str, count: usize },
    #[error("gate table marker ordering is invalid for {table}")]
    MarkerOrder { table: &'static str },
    #[error("gate table heading {heading} occurs {count} times")]
    HeadingCount { heading: &'static str, count: usize },
    #[error("gate table {table} has a malformed header or separator at line {line}")]
    TableHeader { table: &'static str, line: usize },
    #[error("gate table {table} has non-row content or an invalid escaped row at line {line}")]
    TableRow { table: &'static str, line: usize },
    #[error("gate table ID {id} is duplicated")]
    DuplicateGateId { id: String },
    #[error("correction ID {id} is duplicated")]
    DuplicateCorrectionId { id: String },
    #[error("correction ID {id} collides with a main gate-row ID")]
    CorrectionIdCollision { id: String },
    #[error("correction {id} targets unknown entry {target}")]
    UnknownCorrectionTarget { id: String, target: String },
    #[error("correction chain contains a cycle at {id}")]
    CorrectionCycle { id: String },
    #[error("multiple correction chains effectively replace {target}")]
    DuplicateEffectiveReplacement { target: String },
    #[error("correction {id} does not preserve its target identity")]
    CorrectionIdentity { id: String },
    #[error("gate row {id} has invalid typed fields")]
    GateFields { id: String },
    #[error("manifest waiver {id} is duplicated")]
    DuplicateManifestWaiver { id: String },
    #[error("manifest waiver {id} has no exact signed gate row")]
    ManifestOnlyWaiver { id: String },
    #[error("signed gate waiver {id} is absent from the manifest")]
    TableOnlyWaiver { id: String },
    #[error("waiver {id} differs between manifest and effective gate row")]
    WaiverMismatch { id: String },
    #[error("waiver {id} is not a signed Craig Hughes owner decision")]
    WaiverSignature { id: String },
    #[error("waiver {id} has a non-candidate or invalid milestone scope")]
    WaiverScope { id: String },
    #[error("waiver {id} has an invalid model, metric, or protected metric")]
    WaiverTarget { id: String },
    #[error("waiver {id} expired on {expires_on}")]
    WaiverExpired { id: String, expires_on: String },
    #[error("waiver {id} was signed after the comparison date on {date}")]
    WaiverFutureDate { id: String, date: String },
    #[error("required correction {id} is absent or does not cite its record")]
    RequiredCorrection { id: &'static str },
}

#[derive(Debug, Error)]
pub enum AuthorityComparisonError {
    #[error(transparent)]
    Authority(#[from] AuthorityError),
    #[error(transparent)]
    Comparison(#[from] ComparisonError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GateRow {
    id: String,
    kind: String,
    recorded: String,
    authority: String,
    status: String,
    evidence_date: String,
    resolution: String,
    source: String,
    locator: String,
    waiver: Option<WaiverWire>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct WaiverWire {
    model: String,
    metric: String,
    scope: String,
    reason: String,
    expires_on: String,
}

#[derive(Clone, Debug)]
struct Correction {
    id: String,
    target: String,
    row: GateRow,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    version: u64,
    frozen_at: String,
    provisional: bool,
    baseline_tool: String,
    baseline_command: String,
    baseline_git_rev: String,
    metrics: Vec<ManifestMetric>,
    models: Vec<ManifestModel>,
    waivers: Vec<ManifestWaiver>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestMetric {
    id: String,
    direction: String,
    hard_bound: RequiredNullable<HardBound>,
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct RequiredNullable<T>(Option<T>);

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum HardBound {
    Number(u64),
    Text(String),
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestModel {
    name: String,
    stl: String,
    source_digest: SourceDigest,
    report: String,
    baseline_artifact: RequiredNullable<String>,
    python_exit_code: i64,
    python_seconds: f64,
    python_failed_stage: RequiredNullable<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceDigest {
    algorithm: String,
    hex: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestWaiver {
    id: String,
    date: String,
    model: String,
    metric: String,
    scope: String,
    reason: String,
    expires_on: String,
    source: String,
    locator: String,
}

/// The only public comparison route capable of applying a nonempty waiver.
/// It canonicalizes and rereads both authority files on this call before it
/// compares artifacts; a caller cannot retain an authorization token.
pub fn compare_artifacts_with_authority(
    repository_root: &Path,
    manifest_path: &Path,
    as_of: &CalendarDate,
    policy: ComparisonPolicy,
    baseline: &[ScoredArtifact],
    candidate: &[ScoredArtifact],
) -> Result<ComparisonReport, AuthorityComparisonError> {
    let waivers = load_authorized_waivers(repository_root, manifest_path, as_of)?;
    Ok(compare_artifacts_with_authorized_waivers(
        baseline, candidate, policy, &waivers,
    )?)
}

fn load_authorized_waivers(
    repository_root: &Path,
    manifest_path: &Path,
    as_of: &CalendarDate,
) -> Result<Vec<AuthorizedWaiver>, AuthorityError> {
    let root = fs::canonicalize(repository_root).map_err(|_| AuthorityError::RepositoryRoot {
        path: repository_root.to_path_buf(),
    })?;
    if !root.is_dir() {
        return Err(AuthorityError::RepositoryRoot { path: root });
    }
    let expected_gate = root.join(GATE_DOCUMENT_RELATIVE);
    let gate = fs::canonicalize(&expected_gate).map_err(|source| AuthorityError::Read {
        path: expected_gate.clone(),
        source,
    })?;
    if gate != expected_gate || !gate.starts_with(&root) {
        return Err(AuthorityError::GatePath { path: gate });
    }
    let manifest = fs::canonicalize(manifest_path).map_err(|source| AuthorityError::Read {
        path: manifest_path.to_path_buf(),
        source,
    })?;
    if !manifest.starts_with(&root) {
        return Err(AuthorityError::ManifestOutsideRoot { root, manifest });
    }
    let manifest_relative = manifest
        .strip_prefix(&root)
        .map_err(|_| AuthorityError::ManifestOutsideRoot {
            root: root.clone(),
            manifest: manifest.clone(),
        })?
        .to_string_lossy()
        .replace('\\', "/");
    let gate_text =
        fs::read_to_string(&gate).map_err(|source| AuthorityError::Read { path: gate, source })?;
    let manifest_text = fs::read_to_string(&manifest).map_err(|source| AuthorityError::Read {
        path: manifest.clone(),
        source,
    })?;
    let manifest_value: serde_json::Value =
        serde_json::from_str(&manifest_text).map_err(|source| AuthorityError::ManifestSchema {
            path: manifest.clone(),
            source,
        })?;
    validate_required_manifest_fields(&manifest_value, &manifest)?;
    let manifest_data: Manifest =
        serde_json::from_str(&manifest_text).map_err(|source| AuthorityError::ManifestSchema {
            path: manifest,
            source,
        })?;
    validate_manifest(&manifest_data)?;
    let effective = parse_effective_gate_rows(&gate_text)?;
    validate_required_corrections(&effective, &manifest_data, &root)?;
    match_waivers(&effective, &manifest_data, &manifest_relative, as_of)
}

fn validate_required_manifest_fields(
    manifest: &serde_json::Value,
    path: &Path,
) -> Result<(), AuthorityError> {
    for row in manifest
        .get("metrics")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        if !row
            .as_object()
            .is_some_and(|row| row.contains_key("hard_bound"))
        {
            return Err(AuthorityError::ManifestMissingField {
                path: path.to_path_buf(),
                field: "hard_bound",
            });
        }
    }
    for row in manifest
        .get("models")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        for field in ["baseline_artifact", "python_failed_stage"] {
            if !row.as_object().is_some_and(|row| row.contains_key(field)) {
                return Err(AuthorityError::ManifestMissingField {
                    path: path.to_path_buf(),
                    field,
                });
            }
        }
    }
    Ok(())
}

fn validate_manifest(manifest: &Manifest) -> Result<(), AuthorityError> {
    if manifest.schema != "plinth/corpus-manifest" || manifest.version != 1 {
        return Err(AuthorityError::ManifestMetric { index: 0 });
    }
    if manifest.metrics.len() != EXPECTED_METRICS.len() {
        return Err(AuthorityError::ManifestMetric {
            index: manifest.metrics.len(),
        });
    }
    for (index, (row, expected)) in manifest.metrics.iter().zip(EXPECTED_METRICS).enumerate() {
        let actual_bound = match &row.hard_bound.0 {
            None => None,
            Some(HardBound::Number(value)) => Some(value.to_string()),
            Some(HardBound::Text(value)) => Some(value.clone()),
        };
        if row.id != expected.0
            || row.direction != expected.1
            || actual_bound.as_deref() != expected.2
        {
            return Err(AuthorityError::ManifestMetric { index });
        }
    }
    if manifest.models.len() != EXPECTED_MODELS.len() {
        return Err(AuthorityError::ManifestModel {
            index: manifest.models.len(),
        });
    }
    for (index, (row, expected)) in manifest.models.iter().zip(EXPECTED_MODELS).enumerate() {
        if row.name != expected {
            return Err(AuthorityError::ManifestModel { index });
        }
    }
    Ok(())
}

fn parse_effective_gate_rows(text: &str) -> Result<BTreeMap<String, GateRow>, AuthorityError> {
    for heading in ["## Decision gate table", "## Decision gate corrections"] {
        let occurrences = count(text, heading);
        if occurrences != 1 {
            return Err(AuthorityError::HeadingCount {
                heading,
                count: occurrences,
            });
        }
    }
    let rows = parse_main_rows(marked_lines(text, MAIN_START, MAIN_END, "main")?)?;
    let corrections = parse_corrections(marked_lines(
        text,
        CORRECTION_START,
        CORRECTION_END,
        "correction",
    )?)?;
    let mut originals = BTreeMap::new();
    for row in rows {
        let id = row.id.clone();
        if originals.insert(id.clone(), row).is_some() {
            return Err(AuthorityError::DuplicateGateId { id });
        }
    }
    let mut correction_map = BTreeMap::new();
    let mut direct_targets = BTreeSet::new();
    for correction in corrections {
        if !direct_targets.insert(correction.target.clone()) {
            return Err(AuthorityError::DuplicateEffectiveReplacement {
                target: correction.target,
            });
        }
        let id = correction.id.clone();
        if originals.contains_key(&id) {
            return Err(AuthorityError::CorrectionIdCollision { id });
        }
        if correction_map.insert(id.clone(), correction).is_some() {
            return Err(AuthorityError::DuplicateCorrectionId { id });
        }
    }
    let known_ids: BTreeSet<String> = originals
        .keys()
        .chain(correction_map.keys())
        .cloned()
        .collect();
    for correction in correction_map.values() {
        if !known_ids.contains(&correction.target) {
            return Err(AuthorityError::UnknownCorrectionTarget {
                id: correction.id.clone(),
                target: correction.target.clone(),
            });
        }
    }
    let mut roots = BTreeMap::new();
    for correction in correction_map.values() {
        let root_id = resolve_root_id(
            &correction.target,
            &originals,
            &correction_map,
            &mut BTreeSet::new(),
        )?;
        if correction.row.id != root_id {
            return Err(AuthorityError::CorrectionIdentity {
                id: correction.id.clone(),
            });
        }
        roots.insert(correction.id.clone(), root_id);
    }
    let mut effective = originals.clone();
    let mut replaced = BTreeSet::new();
    for correction in correction_map
        .values()
        .filter(|correction| !direct_targets.contains(&correction.id))
    {
        let root_id = roots
            .get(&correction.id)
            .expect("every validated correction has a root");
        if !replaced.insert(root_id.clone()) {
            return Err(AuthorityError::DuplicateEffectiveReplacement {
                target: root_id.clone(),
            });
        }
        effective.insert(root_id.clone(), correction.row.clone());
    }
    Ok(effective)
}

fn resolve_root_id(
    target: &str,
    originals: &BTreeMap<String, GateRow>,
    corrections: &BTreeMap<String, Correction>,
    visiting: &mut BTreeSet<String>,
) -> Result<String, AuthorityError> {
    if originals.contains_key(target) {
        return Ok(target.to_owned());
    }
    if !visiting.insert(target.to_owned()) {
        return Err(AuthorityError::CorrectionCycle {
            id: target.to_owned(),
        });
    }
    let correction =
        corrections
            .get(target)
            .ok_or_else(|| AuthorityError::UnknownCorrectionTarget {
                id: target.to_owned(),
                target: target.to_owned(),
            })?;
    let root = resolve_root_id(&correction.target, originals, corrections, visiting)?;
    visiting.remove(target);
    Ok(root)
}

fn marked_lines<'a>(
    text: &'a str,
    start: &'static str,
    end: &'static str,
    table: &'static str,
) -> Result<Vec<(usize, &'a str)>, AuthorityError> {
    let starts = positions(text, start);
    if starts.len() != 1 {
        return Err(AuthorityError::MarkerCount {
            marker: start,
            count: starts.len(),
        });
    }
    let ends = positions(text, end);
    if ends.len() != 1 {
        return Err(AuthorityError::MarkerCount {
            marker: end,
            count: ends.len(),
        });
    }
    if starts[0] >= ends[0] {
        return Err(AuthorityError::MarkerOrder { table });
    }
    let lines: Vec<_> = text
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line))
        .collect();
    let start_line = lines
        .iter()
        .position(|(_, line)| *line == start)
        .ok_or(AuthorityError::MarkerOrder { table })?;
    let end_line = lines
        .iter()
        .position(|(_, line)| *line == end)
        .ok_or(AuthorityError::MarkerOrder { table })?;
    if start_line >= end_line {
        return Err(AuthorityError::MarkerOrder { table });
    }
    Ok(lines[start_line + 1..end_line].to_vec())
}

fn positions(text: &str, needle: &str) -> Vec<usize> {
    text.match_indices(needle).map(|(index, _)| index).collect()
}

fn count(text: &str, needle: &str) -> usize {
    positions(text, needle).len()
}

fn parse_main_rows(lines: Vec<(usize, &str)>) -> Result<Vec<GateRow>, AuthorityError> {
    parse_table(lines, &MAIN_HEADER, "main")?
        .into_iter()
        .map(gate_row)
        .collect()
}

fn parse_corrections(lines: Vec<(usize, &str)>) -> Result<Vec<Correction>, AuthorityError> {
    parse_table(lines, &CORRECTION_HEADER, "correction")?
        .into_iter()
        .map(|row| {
            let correction_id = field(&row, 0, "correction")?;
            let target = field(&row, 1, "correction")?;
            let gate = gate_row(row[2..].to_vec())?;
            Ok(Correction {
                id: correction_id,
                target,
                row: gate,
            })
        })
        .collect()
}

fn parse_table(
    lines: Vec<(usize, &str)>,
    header: &[&str],
    table: &'static str,
) -> Result<Vec<Vec<String>>, AuthorityError> {
    if lines.len() < 3 {
        return Err(AuthorityError::MarkerOrder { table });
    }
    let parsed_header = parse_markdown_row(lines[0].1).ok_or(AuthorityError::TableHeader {
        table,
        line: lines[0].0,
    })?;
    if parsed_header
        .iter()
        .map(String::as_str)
        .ne(header.iter().copied())
    {
        return Err(AuthorityError::TableHeader {
            table,
            line: lines[0].0,
        });
    }
    let separator = parse_markdown_row(lines[1].1).ok_or(AuthorityError::TableHeader {
        table,
        line: lines[1].0,
    })?;
    if separator.len() != header.len() || separator.iter().any(|cell| cell != "---") {
        return Err(AuthorityError::TableHeader {
            table,
            line: lines[1].0,
        });
    }
    let mut out = Vec::new();
    for (line, raw) in lines.into_iter().skip(2) {
        let row = parse_markdown_row(raw).ok_or(AuthorityError::TableRow { table, line })?;
        if row.len() != header.len() || row.iter().any(String::is_empty) {
            return Err(AuthorityError::TableRow { table, line });
        }
        out.push(row);
    }
    if out.is_empty() {
        return Err(AuthorityError::MarkerOrder { table });
    }
    Ok(out)
}

/// Exact Markdown row grammar: only a literal backslash and a literal pipe may
/// be escaped. A semicolon is ordinary cell text; a waiver field is one strict
/// JSON object and contains exactly one metric.
fn parse_markdown_row(line: &str) -> Option<Vec<String>> {
    let line = line.trim();
    if !line.starts_with('|') || !line.ends_with('|') {
        return None;
    }
    let mut values = Vec::new();
    let mut cell = String::new();
    let mut escaped = false;
    for character in line[1..line.len() - 1].chars() {
        if escaped {
            match character {
                '\\' | '|' => cell.push(character),
                _ => return None,
            }
            escaped = false;
        } else {
            match character {
                '\\' => escaped = true,
                '|' => {
                    values.push(cell.trim().to_owned());
                    cell.clear();
                }
                value => cell.push(value),
            }
        }
    }
    if escaped {
        return None;
    }
    values.push(cell.trim().to_owned());
    Some(values)
}

fn field(row: &[String], index: usize, table: &'static str) -> Result<String, AuthorityError> {
    row.get(index)
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or(AuthorityError::TableRow { table, line: 0 })
}

fn gate_row(row: Vec<String>) -> Result<GateRow, AuthorityError> {
    let id = field(&row, 0, "main")?;
    if !matches!(
        row[1].as_str(),
        "decision" | "override" | "historical" | "waiver"
    ) || CalendarDate::new(&row[2]).is_err()
        || !valid_evidence_date(&row[5])
    {
        return Err(AuthorityError::GateFields { id });
    }
    let waiver = if row[1] == "waiver" {
        let payload: WaiverWire = serde_json::from_str(&row[9])
            .map_err(|_| AuthorityError::GateFields { id: id.clone() })?;
        if payload.metric.is_empty()
            || payload.model.is_empty()
            || payload.scope.is_empty()
            || payload.reason.is_empty()
            || CalendarDate::new(&payload.expires_on).is_err()
        {
            return Err(AuthorityError::GateFields { id });
        }
        Some(payload)
    } else if row[9] == "-" {
        None
    } else {
        return Err(AuthorityError::GateFields { id });
    };
    Ok(GateRow {
        id,
        kind: row[1].clone(),
        recorded: row[2].clone(),
        authority: row[3].clone(),
        status: row[4].clone(),
        evidence_date: row[5].clone(),
        resolution: row[6].clone(),
        source: row[7].clone(),
        locator: row[8].clone(),
        waiver,
    })
}

fn valid_evidence_date(value: &str) -> bool {
    if value == "unknown" {
        return true;
    }
    ["exact:", "on_or_before:"]
        .iter()
        .find_map(|prefix| value.strip_prefix(prefix))
        .is_some_and(|date| CalendarDate::new(date).is_ok())
}

fn validate_required_corrections(
    effective: &BTreeMap<String, GateRow>,
    manifest: &Manifest,
    root: &Path,
) -> Result<(), AuthorityError> {
    effective
        .get("O-1")
        .filter(|row| {
            row.source == "crates/plinth-solid/vendor/manifold/UPSTREAM.toml"
                && row.locator == "version=3.5.2;tag=v3.5.2"
        })
        .ok_or(AuthorityError::RequiredCorrection { id: "C-1" })?;
    let expected_upstream = root.join("crates/plinth-solid/vendor/manifold/UPSTREAM.toml");
    let upstream_path = fs::canonicalize(&expected_upstream)
        .map_err(|_| AuthorityError::RequiredCorrection { id: "C-1" })?;
    if upstream_path != expected_upstream || !upstream_path.starts_with(root) {
        return Err(AuthorityError::RequiredCorrection { id: "C-1" });
    }
    let upstream = fs::read_to_string(&upstream_path)
        .map_err(|_| AuthorityError::RequiredCorrection { id: "C-1" })?;
    let (version, tag) = parse_upstream_identity(&upstream)
        .ok_or(AuthorityError::RequiredCorrection { id: "C-1" })?;
    if version != "3.5.2" || tag != "v3.5.2" {
        return Err(AuthorityError::RequiredCorrection { id: "C-1" });
    }
    let expected_h6 = format!(
        "frozen_at={};baseline_git_rev={}",
        manifest.frozen_at, manifest.baseline_git_rev
    );
    effective
        .get("H-6")
        .filter(|row| {
            row.source == "corpus/manifest.json"
                && row.evidence_date == "exact:2026-08-01"
                && row.locator == expected_h6
        })
        .ok_or(AuthorityError::RequiredCorrection { id: "C-2" })?;
    Ok(())
}

fn parse_upstream_identity(text: &str) -> Option<(String, String)> {
    let mut version = None;
    let mut tag = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            break;
        }
        let Some((raw_key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let key = raw_key.trim();
        let slot = match key {
            "version" => &mut version,
            "tag" => &mut tag,
            _ => continue,
        };
        let value = parse_plain_toml_string(raw_value.trim())?;
        if slot.replace(value).is_some() {
            return None;
        }
    }
    Some((version?, tag?))
}

fn parse_plain_toml_string(value: &str) -> Option<String> {
    let content = value.strip_prefix('"')?.strip_suffix('"')?;
    if content.contains(['"', '\\', '\n', '\r']) {
        return None;
    }
    Some(content.to_owned())
}

fn match_waivers(
    effective: &BTreeMap<String, GateRow>,
    manifest: &Manifest,
    manifest_relative: &str,
    as_of: &CalendarDate,
) -> Result<Vec<AuthorizedWaiver>, AuthorityError> {
    let mut manifest_ids = BTreeSet::new();
    let mut authorized = Vec::with_capacity(manifest.waivers.len());
    for waiver in &manifest.waivers {
        if !manifest_ids.insert(waiver.id.clone()) {
            return Err(AuthorityError::DuplicateManifestWaiver {
                id: waiver.id.clone(),
            });
        }
        let Some(gate) = effective.get(&waiver.id) else {
            return Err(AuthorityError::ManifestOnlyWaiver {
                id: waiver.id.clone(),
            });
        };
        let expected_pointer = format!("{manifest_relative}#/waivers/{}", waiver.id);
        if gate.kind != "waiver"
            || gate.authority != "owner:Craig Hughes"
            || gate.status != "signed"
        {
            return Err(AuthorityError::WaiverSignature {
                id: waiver.id.clone(),
            });
        }
        let Some(gate_waiver) = &gate.waiver else {
            return Err(AuthorityError::WaiverMismatch {
                id: waiver.id.clone(),
            });
        };
        if gate.recorded != waiver.date
            || gate.evidence_date != format!("exact:{}", waiver.date)
            || gate.source != manifest_relative
            || gate.locator != expected_pointer
            || waiver.source != manifest_relative
            || waiver.locator != expected_pointer
            || gate.resolution != format!("Waiver {}: {}", waiver.id, waiver.reason)
            || gate_waiver.model != waiver.model
            || gate_waiver.metric != waiver.metric
            || gate_waiver.scope != waiver.scope
            || gate_waiver.reason != waiver.reason
            || gate_waiver.expires_on != waiver.expires_on
        {
            return Err(AuthorityError::WaiverMismatch {
                id: waiver.id.clone(),
            });
        }
        let milestone = parse_scope(&waiver.id, &waiver.scope)?;
        if CalendarDate::new(&waiver.date).is_err()
            || CalendarDate::new(&waiver.expires_on).is_err()
        {
            return Err(AuthorityError::WaiverMismatch {
                id: waiver.id.clone(),
            });
        }
        if waiver.date.as_str() > as_of.as_str() {
            return Err(AuthorityError::WaiverFutureDate {
                id: waiver.id.clone(),
                date: waiver.date.clone(),
            });
        }
        if waiver.expires_on.as_str() < as_of.as_str() {
            return Err(AuthorityError::WaiverExpired {
                id: waiver.id.clone(),
                expires_on: waiver.expires_on.clone(),
            });
        }
        if !EXPECTED_MODELS.contains(&waiver.model.as_str())
            || !crate::M1_METRIC_ORDER.contains(&waiver.metric.as_str())
            || matches!(
                waiver.metric.as_str(),
                "boundary_edges" | "negative_shells" | "winding" | "cavities" | "loadable"
            )
        {
            return Err(AuthorityError::WaiverTarget {
                id: waiver.id.clone(),
            });
        }
        let metric = crate::M1_METRIC_ORDER
            .into_iter()
            .find(|metric| *metric == waiver.metric)
            .ok_or_else(|| AuthorityError::WaiverTarget {
                id: waiver.id.clone(),
            })?;
        authorized.push(AuthorizedWaiver {
            model: ModelId::from(waiver.model.clone()),
            metric,
            milestone,
        });
    }
    for gate in effective.values().filter(|row| row.kind == "waiver") {
        if !manifest_ids.contains(&gate.id) {
            return Err(AuthorityError::TableOnlyWaiver {
                id: gate.id.clone(),
            });
        }
    }
    Ok(authorized)
}

fn parse_scope(id: &str, scope: &str) -> Result<ComparisonMilestone, AuthorityError> {
    match scope.strip_prefix("candidate:") {
        Some("M1") => Ok(ComparisonMilestone::M1),
        Some("M2") => Ok(ComparisonMilestone::M2),
        Some("M3") => Ok(ComparisonMilestone::M3),
        Some("M4") => Ok(ComparisonMilestone::M4),
        Some("M5") => Ok(ComparisonMilestone::M5),
        Some("M6") => Ok(ComparisonMilestone::M6),
        Some("M7") => Ok(ComparisonMilestone::M7),
        Some("M8") => Ok(ComparisonMilestone::M8),
        Some("M9") => Ok(ComparisonMilestone::M9),
        _ => Err(AuthorityError::WaiverScope { id: id.to_owned() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComparisonScope;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn date() -> CalendarDate {
        CalendarDate::new("2026-08-01").unwrap()
    }

    fn root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("plinth-authority-{label}-{nonce}"));
        fs::create_dir_all(root.join("docs/rust-migration")).unwrap();
        root
    }

    fn manifest_document(waivers: &str) -> String {
        let metrics = EXPECTED_METRICS
            .iter()
            .map(|(id, direction, hard)| match hard {
                None => format!(r#"{{"id":"{id}","direction":"{direction}","hard_bound":null}}"#),
                Some("0") => format!(r#"{{"id":"{id}","direction":"{direction}","hard_bound":0}}"#),
                Some(value) => {
                    format!(r#"{{"id":"{id}","direction":"{direction}","hard_bound":"{value}"}}"#)
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        let models = EXPECTED_MODELS
            .iter()
            .map(|name| format!(r#"{{"name":"{name}","stl":"x","source_digest":{{"algorithm":"sha256","hex":"x"}},"report":"x","baseline_artifact":null,"python_exit_code":0,"python_seconds":0.0,"python_failed_stage":null}}"#))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"schema":"plinth/corpus-manifest","version":1,"frozen_at":"2026-08-01T08:20:35Z","provisional":false,"baseline_tool":"x","baseline_command":"x","baseline_git_rev":"6eb7f6a65e1a3442aa860d63d920788da778faed","metrics":[{metrics}],"models":[{models}],"waivers":{waivers}}}"#
        )
    }

    fn docs(main_rows: &str, correction_rows: &str) -> String {
        format!(
            "## Decision gate table\n\n<!-- PLINTH_GATE_MAIN_START -->\n| {} |\n| {} |\n{main_rows}<!-- PLINTH_GATE_MAIN_END -->\n\n## Decision gate corrections\n\n<!-- PLINTH_GATE_CORRECTIONS_START -->\n| {} |\n| {} |\n{correction_rows}<!-- PLINTH_GATE_CORRECTIONS_END -->\n",
            MAIN_HEADER.join(" | "),
            std::iter::repeat_n("---", MAIN_HEADER.len()).collect::<Vec<_>>().join(" | "),
            CORRECTION_HEADER.join(" | "),
            std::iter::repeat_n("---", CORRECTION_HEADER.len()).collect::<Vec<_>>().join(" | "),
        )
    }

    fn corrections() -> String {
        "| C-1 | O-1 | O-1 | override | 2026-08-01 | owner:Craig Hughes | documented-owner-approval | exact:2026-07-31 | Bounded C++ Manifold 3.5.2 FFI exception | crates/plinth-solid/vendor/manifold/UPSTREAM.toml | version=3.5.2;tag=v3.5.2 | - |\n| C-2 | H-6 | H-6 | historical | 2026-08-01 | owner:Craig Hughes | documented-owner-approval | exact:2026-08-01 | Python M0 freeze recorded before Rust future refreeze | corpus/manifest.json | frozen_at=2026-08-01T08:20:35Z;baseline_git_rev=6eb7f6a65e1a3442aa860d63d920788da778faed | - |\n".to_owned()
    }

    fn main_rows(extra: &str) -> String {
        format!("| O-1 | override | 2026-08-01 | owner:Craig Hughes | documented-owner-approval | exact:2026-07-31 | old O-1 | docs/x.md | x | - |\n| H-6 | historical | 2026-08-01 | owner:Craig Hughes | documented-owner-approval | exact:2026-08-01 | old H-6 | docs/x.md | x | - |\n{extra}")
    }

    fn write(root: &Path, doc: &str, waivers: &str) -> PathBuf {
        fs::write(root.join(GATE_DOCUMENT_RELATIVE), doc).unwrap();
        fs::create_dir_all(root.join("corpus")).unwrap();
        let upstream = root.join("crates/plinth-solid/vendor/manifold/UPSTREAM.toml");
        fs::create_dir_all(upstream.parent().unwrap()).unwrap();
        fs::write(&upstream, "version = \"3.5.2\"\ntag = \"v3.5.2\"\n").unwrap();
        let manifest_path = root.join("corpus/manifest.json");
        fs::write(&manifest_path, manifest_document(waivers)).unwrap();
        manifest_path
    }

    #[test]
    fn committed_files_parse_as_empty_and_exact() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let manifest = root.join("corpus/manifest.json");
        assert!(load_authorized_waivers(&root, &manifest, &date())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn parser_rejects_markers_partial_prefixes_nonrows_and_bad_escapes() {
        let root = root("syntax");
        let base = docs(&main_rows(""), &corrections());
        let manifest = write(&root, &base, "[]");
        assert!(load_authorized_waivers(&root, &manifest, &date()).is_ok());
        for bad in [
            base.replacen(MAIN_START, "", 1),
            base.replacen(MAIN_END, "text\n<!-- PLINTH_GATE_MAIN_END -->", 1),
            base.replacen("old O-1", "bad\\q", 1),
            format!("{base}<!-- PLINTH_GATE_MAIN_START -->"),
        ] {
            fs::write(root.join(GATE_DOCUMENT_RELATIVE), bad).unwrap();
            assert!(load_authorized_waivers(&root, &manifest, &date()).is_err());
        }
    }

    #[test]
    fn strict_manifest_rejects_unknown_signature_shape_and_wrong_membership() {
        let root = root("manifest");
        let manifest_path = write(&root, &docs(&main_rows(""), &corrections()), "[]");
        let unknown =
            manifest_document("[]").replace("\"waivers\":[]", "\"signature\":{},\"waivers\":[]");
        fs::write(&manifest_path, unknown).unwrap();
        assert!(matches!(
            load_authorized_waivers(&root, &manifest_path, &date()),
            Err(AuthorityError::ManifestSchema { .. })
        ));
        fs::write(
            &manifest_path,
            manifest_document("[]").replace("Weeping Treant v1.2", "Cone"),
        )
        .unwrap();
        assert!(matches!(
            load_authorized_waivers(&root, &manifest_path, &date()),
            Err(AuthorityError::ManifestModel { .. })
        ));
        for (field, missing) in [
            (
                "hard_bound",
                manifest_document("[]").replacen(",\"hard_bound\":null", "", 1),
            ),
            (
                "baseline_artifact",
                manifest_document("[]").replacen(",\"baseline_artifact\":null", "", 1),
            ),
            (
                "python_failed_stage",
                manifest_document("[]").replacen(",\"python_failed_stage\":null", "", 1),
            ),
        ] {
            fs::write(&manifest_path, missing).unwrap();
            let result = load_authorized_waivers(&root, &manifest_path, &date());
            assert!(matches!(
                result,
                Err(AuthorityError::ManifestMissingField {
                    field: missing_field,
                    ..
                }) if missing_field == field
            ));
        }
    }

    #[test]
    fn corrections_are_effective_and_must_have_exact_citations() {
        let root = root("correction");
        let document = docs(&main_rows(""), &corrections());
        let manifest = write(&root, &document, "[]");
        assert!(load_authorized_waivers(&root, &manifest, &date()).is_ok());
        let bad = document.replace("version=3.5.2;tag=v3.5.2", "version=3.5.1;tag=v3.5.1");
        fs::write(root.join(GATE_DOCUMENT_RELATIVE), bad).unwrap();
        assert!(matches!(
            load_authorized_waivers(&root, &manifest, &date()),
            Err(AuthorityError::RequiredCorrection { id: "C-1" })
        ));
        let bad_h6 = docs(&main_rows(""), &corrections()).replace(
            "baseline_git_rev=6eb7f6a65e1a3442aa860d63d920788da778faed",
            "baseline_git_rev=0000000000000000000000000000000000000000",
        );
        fs::write(root.join(GATE_DOCUMENT_RELATIVE), bad_h6).unwrap();
        assert!(matches!(
            load_authorized_waivers(&root, &manifest, &date()),
            Err(AuthorityError::RequiredCorrection { id: "C-2" })
        ));
        fs::write(root.join(GATE_DOCUMENT_RELATIVE), &document).unwrap();
        let upstream = root.join("crates/plinth-solid/vendor/manifold/UPSTREAM.toml");
        fs::write(
            &upstream,
            "version = \"3.5.1\"\ntag = \"v3.5.1\"\n# version = \"3.5.2\"\n# tag = \"v3.5.2\"\n",
        )
        .unwrap();
        assert!(matches!(
            load_authorized_waivers(&root, &manifest, &date()),
            Err(AuthorityError::RequiredCorrection { id: "C-1" })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn required_upstream_rejects_a_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = root("upstream-symlink");
        let document = docs(&main_rows(""), &corrections());
        let manifest = write(&root, &document, "[]");
        let upstream = root.join("crates/plinth-solid/vendor/manifold/UPSTREAM.toml");
        let outside = std::env::temp_dir().join(format!(
            "plinth-upstream-outside-{}.toml",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&outside, "version = \"3.5.2\"\ntag = \"v3.5.2\"\n").unwrap();
        fs::remove_file(&upstream).unwrap();
        symlink(&outside, &upstream).unwrap();
        assert!(matches!(
            load_authorized_waivers(&root, &manifest, &date()),
            Err(AuthorityError::RequiredCorrection { id: "C-1" })
        ));
    }

    fn correction_row(correction_id: &str, target: &str, source: &str) -> String {
        format!(
            "| {correction_id} | {target} | O-1 | override | 2026-08-01 | owner:Craig Hughes | documented-owner-approval | exact:2026-07-31 | replacement | {source} | x | - |\n"
        )
    }

    #[test]
    fn correction_graph_selects_the_leaf_independent_of_lexical_order() {
        let za = format!(
            "{}{}",
            correction_row("Z", "O-1", "docs/z.md"),
            correction_row("A", "Z", "docs/a.md")
        );
        let effective = parse_effective_gate_rows(&docs(&main_rows(""), &za)).unwrap();
        assert_eq!(effective["O-1"].source, "docs/a.md");

        let numbered = format!(
            "{}{}",
            correction_row("C-10", "O-1", "docs/c10.md"),
            correction_row("C-9", "C-10", "docs/c9.md")
        );
        let effective = parse_effective_gate_rows(&docs(&main_rows(""), &numbered)).unwrap();
        assert_eq!(effective["O-1"].source, "docs/c9.md");
    }

    #[test]
    fn correction_graph_rejects_cycles_dangling_targets_and_ambiguity() {
        let cycle = format!(
            "{}{}",
            correction_row("C-A", "C-B", "docs/a.md"),
            correction_row("C-B", "C-A", "docs/b.md")
        );
        assert!(matches!(
            parse_effective_gate_rows(&docs(&main_rows(""), &cycle)),
            Err(AuthorityError::CorrectionCycle { .. })
        ));
        let dangling = correction_row("C-D", "missing", "docs/d.md");
        assert!(matches!(
            parse_effective_gate_rows(&docs(&main_rows(""), &dangling)),
            Err(AuthorityError::UnknownCorrectionTarget { .. })
        ));
        let ambiguous = format!(
            "{}{}",
            correction_row("C-A", "O-1", "docs/a.md"),
            correction_row("C-B", "O-1", "docs/b.md")
        );
        assert!(matches!(
            parse_effective_gate_rows(&docs(&main_rows(""), &ambiguous)),
            Err(AuthorityError::DuplicateEffectiveReplacement { .. })
        ));
    }

    #[test]
    fn correction_ids_must_be_disjoint_from_main_ids_before_graph_resolution() {
        let self_collision = correction_row("O-1", "O-1", "docs/self.md");
        assert!(matches!(
            parse_effective_gate_rows(&docs(&main_rows(""), &self_collision)),
            Err(AuthorityError::CorrectionIdCollision { id }) if id == "O-1"
        ));

        let cycle_masked_by_collision = format!(
            "{}{}",
            correction_row("O-1", "C-A", "docs/collision.md"),
            correction_row("C-A", "O-1", "docs/cycle.md")
        );
        assert!(matches!(
            parse_effective_gate_rows(&docs(&main_rows(""), &cycle_masked_by_collision)),
            Err(AuthorityError::CorrectionIdCollision { id }) if id == "O-1"
        ));
    }

    #[test]
    fn canonical_repository_boundary_rejects_an_outside_manifest() {
        let root = root("outside");
        let _inside = write(&root, &docs(&main_rows(""), &corrections()), "[]");
        let outside = std::env::temp_dir().join(format!(
            "plinth-authority-outside-{}.json",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&outside, manifest_document("[]")).unwrap();
        assert!(matches!(
            load_authorized_waivers(&root, &outside, &date()),
            Err(AuthorityError::ManifestOutsideRoot { .. })
        ));
    }

    fn waiver_json_on(id: &str, signed_on: &str, scope: &str, expires_on: &str) -> String {
        format!(
            r#"[{{"id":"{id}","date":"{signed_on}","model":"Sphere","metric":"scar_area","scope":"{scope}","reason":"semicolon; is ordinary","expires_on":"{expires_on}","source":"corpus/manifest.json","locator":"corpus/manifest.json#/waivers/{id}"}}]"#
        )
    }

    fn waiver_json(id: &str, scope: &str, expires_on: &str) -> String {
        waiver_json_on(id, "2026-08-01", scope, expires_on)
    }

    fn waiver_row_on(id: &str, signed_on: &str, scope: &str, expires_on: &str) -> String {
        let payload = format!(
            r#"{{"model":"Sphere","metric":"scar_area","scope":"{scope}","reason":"semicolon; is ordinary","expires_on":"{expires_on}"}}"#
        );
        format!(
            "| {id} | waiver | {signed_on} | owner:Craig Hughes | signed | exact:{signed_on} | Waiver {id}: semicolon; is ordinary | corpus/manifest.json | corpus/manifest.json#/waivers/{id} | {payload} |\n"
        )
    }

    fn waiver_row(id: &str, scope: &str, expires_on: &str) -> String {
        waiver_row_on(id, "2026-08-01", scope, expires_on)
    }

    #[test]
    fn authority_is_fresh_scope_exact_and_never_self() {
        let root = root("fresh");
        let row = waiver_row("W-1", "candidate:M7", "2026-08-02");
        let manifest = write(
            &root,
            &docs(&main_rows(&row), &corrections()),
            &waiver_json("W-1", "candidate:M7", "2026-08-02"),
        );
        let baseline = crate::score::scored_for_authority_test("Sphere", 1.0);
        let candidate = crate::score::scored_for_authority_test("Sphere", 2.0);
        let m6 = compare_artifacts_with_authority(
            &root,
            &manifest,
            &date(),
            ComparisonPolicy {
                milestone: ComparisonMilestone::M6,
                scope: ComparisonScope::Candidate,
            },
            std::slice::from_ref(&baseline),
            std::slice::from_ref(&candidate),
        )
        .unwrap();
        assert_eq!(m6.rows()[10].outcome, crate::MetricOutcome::Worse);
        let m7 = compare_artifacts_with_authority(
            &root,
            &manifest,
            &date(),
            ComparisonPolicy {
                milestone: ComparisonMilestone::M7,
                scope: ComparisonScope::Candidate,
            },
            std::slice::from_ref(&baseline),
            std::slice::from_ref(&candidate),
        )
        .unwrap();
        assert_eq!(m7.rows()[10].outcome, crate::MetricOutcome::Waived);
        let self_scope = compare_artifacts_with_authority(
            &root,
            &manifest,
            &date(),
            ComparisonPolicy {
                milestone: ComparisonMilestone::M7,
                scope: ComparisonScope::SelfComparison,
            },
            std::slice::from_ref(&baseline),
            std::slice::from_ref(&candidate),
        )
        .unwrap();
        assert_eq!(self_scope.rows()[10].outcome, crate::MetricOutcome::Worse);
        let expired_row = waiver_row("W-1", "candidate:M7", "2026-07-31");
        fs::write(
            root.join(GATE_DOCUMENT_RELATIVE),
            docs(&main_rows(&expired_row), &corrections()),
        )
        .unwrap();
        fs::write(
            &manifest,
            manifest_document(&waiver_json("W-1", "candidate:M7", "2026-07-31")),
        )
        .unwrap();
        assert!(matches!(
            compare_artifacts_with_authority(
                &root,
                &manifest,
                &date(),
                ComparisonPolicy {
                    milestone: ComparisonMilestone::M7,
                    scope: ComparisonScope::Candidate,
                },
                std::slice::from_ref(&baseline),
                std::slice::from_ref(&candidate),
            ),
            Err(AuthorityComparisonError::Authority(
                AuthorityError::WaiverExpired { .. }
            ))
        ));
    }

    #[test]
    fn one_sided_duplicate_and_forged_waivers_fail_closed() {
        let root = root("waiver-errors");
        let manifest = write(
            &root,
            &docs(&main_rows(""), &corrections()),
            &waiver_json("W-1", "candidate:M7", "2026-08-02"),
        );
        assert!(matches!(
            load_authorized_waivers(&root, &manifest, &date()),
            Err(AuthorityError::ManifestOnlyWaiver { .. })
        ));
        let row = waiver_row("W-1", "candidate:M7", "2026-08-02");
        let manifest = write(&root, &docs(&main_rows(&row), &corrections()), "[]");
        assert!(matches!(
            load_authorized_waivers(&root, &manifest, &date()),
            Err(AuthorityError::TableOnlyWaiver { .. })
        ));
        let manifest = write(
            &root,
            &docs(&main_rows(&row), &corrections()),
            &waiver_json("W-1", "M7", "2026-08-02"),
        );
        assert!(matches!(
            load_authorized_waivers(&root, &manifest, &date()),
            Err(AuthorityError::WaiverMismatch { .. })
        ));

        let duplicate_item = waiver_json("W-1", "candidate:M7", "2026-08-02");
        let item = &duplicate_item[1..duplicate_item.len() - 1];
        let duplicates = format!("[{item},{item}]");
        let manifest = write(&root, &docs(&main_rows(&row), &corrections()), &duplicates);
        assert!(matches!(
            load_authorized_waivers(&root, &manifest, &date()),
            Err(AuthorityError::DuplicateManifestWaiver { .. })
        ));

        for forged in [
            row.replace("owner:Craig Hughes", "owner:Mallory"),
            row.replace("| signed |", "| documented-owner-approval |"),
        ] {
            let manifest = write(
                &root,
                &docs(&main_rows(&forged), &corrections()),
                &waiver_json("W-1", "candidate:M7", "2026-08-02"),
            );
            assert!(matches!(
                load_authorized_waivers(&root, &manifest, &date()),
                Err(AuthorityError::WaiverSignature { .. })
            ));
        }

        let evidence_mismatch = row.replace("exact:2026-08-01", "exact:2026-07-31");
        let manifest = write(
            &root,
            &docs(&main_rows(&evidence_mismatch), &corrections()),
            &waiver_json("W-1", "candidate:M7", "2026-08-02"),
        );
        assert!(matches!(
            load_authorized_waivers(&root, &manifest, &date()),
            Err(AuthorityError::WaiverMismatch { .. })
        ));

        let future_row = waiver_row_on("W-1", "2026-08-02", "candidate:M7", "2026-08-03");
        let manifest = write(
            &root,
            &docs(&main_rows(&future_row), &corrections()),
            &waiver_json_on("W-1", "2026-08-02", "candidate:M7", "2026-08-03"),
        );
        assert!(matches!(
            load_authorized_waivers(&root, &manifest, &date()),
            Err(AuthorityError::WaiverFutureDate { .. })
        ));
    }
}
