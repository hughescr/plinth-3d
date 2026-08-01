//! Disk-only M1 score assembly and the as-good-or-better comparator.
//!
//! This module never accepts producer memory.  Callers gather every manifest
//! row, reopen it through [`crate::load_artifacts`], then submit that whole
//! ordered batch here.  Score assembly deliberately consumes the independent
//! topology, support, and scar evidence explicitly; a missing observation is a
//! named error rather than an invented zero.

use std::collections::{BTreeMap, BTreeSet};

use chitubox_io::Project;
use plinth_core::{
    m0_constants, CalendarDate, EvidenceSource, EvidenceSourceView, M0ConstantsError, MetricId,
    MetricSet, MetricValue, ModelId, PrinterProfile,
};
use thiserror::Error;

use crate::{
    scar_metric_rows, support_metric_rows, topology_metric_rows, ArtifactLoad, DiskArtifact,
    ScarMetricError, ScarMetricInput, SupportMetricError, SupportMetricInput, TopologyMetricError,
    TopologyMetricInput,
};

/// The M1 metric IDs in the normative, non-lexical order of guide §3.
///
/// `max_x_group` is deliberately absent: it is a new M5 metric, not a new
/// interpretation of M1's per-arm `max_x`.
pub const M1_METRIC_ORDER: [&str; 15] = [
    "islands",
    "cavities",
    "boundary_edges",
    "negative_shells",
    "winding",
    "max_x",
    "support_count",
    "resin_model",
    "resin_support",
    "resin_total",
    "scar_area",
    "scar_worst_patch",
    "scar_weighted",
    "z_height",
    "loadable",
];

const FORMAT_METRICS: [&str; 5] = [
    "boundary_edges",
    "negative_shells",
    "winding",
    "cavities",
    "loadable",
];
const SCAR_METRICS: [&str; 3] = ["scar_area", "scar_worst_patch", "scar_weighted"];
const RESIN_METRICS: [&str; 3] = ["resin_model", "resin_support", "resin_total"];

/// All independently supplied evidence required to score one successful disk
/// artifact.  Each nested slice must cover every model in that artifact in its
/// on-disk source order.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactMetricInputs {
    pub topology: Vec<TopologyMetricInput>,
    pub supports: Vec<SupportMetricInput>,
    pub scars: Vec<ScarMetricInput>,
}

/// An ordered score or an explicit producer failure for one manifest model.
#[derive(Clone, Debug, PartialEq)]
pub enum ScoredArtifact {
    Success(ScoredModel),
    ProducerFailed { model: ModelId },
}

impl ScoredArtifact {
    #[must_use]
    pub fn model(&self) -> &ModelId {
        match self {
            Self::Success(score) => score.model(),
            Self::ProducerFailed { model } => model,
        }
    }
}

/// One complete model score.  The private array retains the normative order;
/// [`Self::metric_set`] is only the passive, deterministic core DTO.
#[derive(Clone, Debug, PartialEq)]
pub struct ScoredModel {
    model: ModelId,
    values: [MetricValue; M1_METRIC_ORDER.len()],
}

impl ScoredModel {
    #[must_use]
    pub const fn model(&self) -> &ModelId {
        &self.model
    }

    #[must_use]
    pub fn entries(&self) -> [(MetricId, MetricValue); M1_METRIC_ORDER.len()] {
        std::array::from_fn(|index| (MetricId::from(M1_METRIC_ORDER[index]), self.values[index]))
    }

    #[must_use]
    pub fn metric_set(&self) -> MetricSet {
        MetricSet::new(self.entries())
    }

    #[must_use]
    pub fn value(&self, id: &str) -> Option<MetricValue> {
        M1_METRIC_ORDER
            .iter()
            .position(|candidate| *candidate == id)
            .map(|index| self.values[index])
    }
}

/// Semantic score-assembly failures.
#[derive(Debug, Error)]
pub enum ScoreError {
    #[error("score input count {actual} does not match loaded artifact count {expected}")]
    InputCount { expected: usize, actual: usize },
    #[error("successful artifact for {model} contains {model_count} project models; one artifact per corpus model is required")]
    ArtifactModelCount { model: ModelId, model_count: usize },
    #[error("duplicate scored model {model} at rows {first_row} and {duplicate_row}")]
    DuplicateModel {
        model: ModelId,
        first_row: usize,
        duplicate_row: usize,
    },
    #[error("M0 constants registry is unavailable: {0}")]
    Constants(#[from] M0ConstantsError),
    #[error("artifact for {model} at {path} failed the fresh ChituBox structural audit: {source}")]
    FreshFormat {
        model: ModelId,
        path: std::path::PathBuf,
        #[source]
        source: chitubox_io::ChituboxError,
    },
    #[error("artifact for {model} at {path} changed bytes during its fresh ChituBox audit")]
    FreshFormatBytesChanged {
        model: ModelId,
        path: std::path::PathBuf,
    },
    #[error("artifact for {model} has a non-finite world {axis} bound")]
    NonFiniteBound { model: ModelId, axis: &'static str },
    #[error("artifact for {model} exceeds the Saturn 4 Ultra {axis} build extent: observed {observed_mm} mm, allowed {allowed_mm} mm")]
    BuildVolumeExceeded {
        model: ModelId,
        axis: &'static str,
        observed_mm: f64,
        allowed_mm: f64,
    },
    #[error("artifact for {model} has no finite geometry to establish z height")]
    EmptyBounds { model: ModelId },
    #[error("artifact for {model} overflowed while assembling {metric}")]
    ArithmeticOverflow {
        model: ModelId,
        metric: &'static str,
    },
    #[error(transparent)]
    Topology(#[from] TopologyMetricError),
    #[error(transparent)]
    Supports(#[from] SupportMetricError),
    #[error(transparent)]
    Scars(#[from] ScarMetricError),
}

/// Scores a complete ordered batch against the official Saturn 4 Ultra profile.
pub fn score_saturn_artifacts(
    artifacts: &[ArtifactLoad],
    inputs: &[ArtifactMetricInputs],
) -> Result<Vec<ScoredArtifact>, ScoreError> {
    let constants = m0_constants().map_err(|error| ScoreError::Constants(error.clone()))?;
    score_artifacts(artifacts, inputs, constants.saturn4_ultra())
}

/// Scores a complete ordered batch against the supplied validated printer profile.
///
/// This is useful for future printer selection, while the public M1 entry point
/// above ensures Saturn's build volume is never duplicated as a local number.
pub fn score_artifacts(
    artifacts: &[ArtifactLoad],
    inputs: &[ArtifactMetricInputs],
    profile: &PrinterProfile,
) -> Result<Vec<ScoredArtifact>, ScoreError> {
    if artifacts.len() != inputs.len() {
        return Err(ScoreError::InputCount {
            expected: artifacts.len(),
            actual: inputs.len(),
        });
    }
    let mut seen = BTreeMap::new();
    let mut scores = Vec::with_capacity(artifacts.len());
    for (row, (load, evidence)) in artifacts.iter().zip(inputs).enumerate() {
        let model = load.model().clone();
        if let Some(first_row) = seen.insert(model.clone(), row) {
            return Err(ScoreError::DuplicateModel {
                model,
                first_row,
                duplicate_row: row,
            });
        }
        match load {
            ArtifactLoad::ProducerFailed { .. } => {
                scores.push(ScoredArtifact::ProducerFailed { model })
            }
            ArtifactLoad::Artifact(artifact) => {
                scores.push(ScoredArtifact::Success(score_one(
                    artifact, evidence, profile,
                )?));
            }
        }
    }
    Ok(scores)
}

fn score_one(
    artifact: &DiskArtifact,
    inputs: &ArtifactMetricInputs,
    profile: &PrinterProfile,
) -> Result<ScoredModel, ScoreError> {
    if artifact.models().len() != 1 {
        return Err(ScoreError::ArtifactModelCount {
            model: artifact.model().clone(),
            model_count: artifact.models().len(),
        });
    }
    fresh_format_audit(artifact)?;
    let topology_rows = topology_metric_rows(artifact, &inputs.topology)?;
    let support_rows = support_metric_rows(artifact, &inputs.supports)?;
    let scar_rows = scar_metric_rows(artifact, &inputs.scars)?;
    let topology_row = topology_rows
        .first()
        .ok_or_else(|| ScoreError::ArtifactModelCount {
            model: artifact.model().clone(),
            model_count: 0,
        })?;
    let support_row = support_rows
        .first()
        .ok_or_else(|| ScoreError::ArtifactModelCount {
            model: artifact.model().clone(),
            model_count: 0,
        })?;
    let scar_row = scar_rows
        .first()
        .ok_or_else(|| ScoreError::ArtifactModelCount {
            model: artifact.model().clone(),
            model_count: 0,
        })?;
    let z_height = z_height_and_build_bounds(artifact, profile)?;
    let loadable = MetricValue::new(1.0).map_err(|_| ScoreError::ArithmeticOverflow {
        model: artifact.model().clone(),
        metric: "loadable",
    })?;

    let mut values = BTreeMap::new();
    for (id, value) in topology_row.entries() {
        values.insert(id, value);
    }
    for (id, value) in support_row.entries() {
        values.insert(id, value);
    }
    for (id, value) in scar_row.entries() {
        values.insert(id, value);
    }
    values.insert(MetricId::from("z_height"), z_height);
    values.insert(MetricId::from("loadable"), loadable);
    let values = std::array::from_fn(|index| {
        // Every entry is gathered from a fixed nonempty set immediately above.
        // `expect` names that static programming invariant rather than input data.
        values
            .remove(&MetricId::from(M1_METRIC_ORDER[index]))
            .expect("the complete fixed M1 metric table was assembled")
    });
    Ok(ScoredModel {
        model: artifact.model().clone(),
        values,
    })
}

fn fresh_format_audit(artifact: &DiskArtifact) -> Result<(), ScoreError> {
    let bytes = std::fs::read(artifact.path()).map_err(|source| ScoreError::FreshFormat {
        model: artifact.model().clone(),
        path: artifact.path().to_owned(),
        source: chitubox_io::ChituboxError::Io(source),
    })?;
    let project = Project::from_bytes(bytes.clone()).map_err(|source| ScoreError::FreshFormat {
        model: artifact.model().clone(),
        path: artifact.path().to_owned(),
        source,
    })?;
    let rebuilt = project
        .to_bytes()
        .map_err(|source| ScoreError::FreshFormat {
            model: artifact.model().clone(),
            path: artifact.path().to_owned(),
            source,
        })?;
    if rebuilt != bytes {
        return Err(ScoreError::FreshFormatBytesChanged {
            model: artifact.model().clone(),
            path: artifact.path().to_owned(),
        });
    }
    Ok(())
}

fn z_height_and_build_bounds(
    artifact: &DiskArtifact,
    profile: &PrinterProfile,
) -> Result<MetricValue, ScoreError> {
    let volume = profile.build_volume_mm().value();
    let mut bounds = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for model in artifact.models() {
        let placement = model.placement();
        for point in model.mesh().vertices().iter().chain(
            model
                .supports()
                .iter()
                .flat_map(|support| support.mesh().vertices().iter()),
        ) {
            extend_world_bounds(&mut bounds, artifact.model(), *point, placement)?;
        }
        for point in model
            .supports()
            .iter()
            .flat_map(|support| [support.start(), support.end()])
        {
            extend_world_bounds(&mut bounds, artifact.model(), point, placement)?;
        }
    }
    if bounds[0] == f64::INFINITY {
        return Err(ScoreError::EmptyBounds {
            model: artifact.model().clone(),
        });
    }
    let x_extent = bounds[1] - bounds[0];
    let y_extent = bounds[3] - bounds[2];
    let z_extent = bounds[5] - bounds[4];
    if x_extent > volume.x() {
        return Err(ScoreError::BuildVolumeExceeded {
            model: artifact.model().clone(),
            axis: "x",
            observed_mm: x_extent,
            allowed_mm: volume.x(),
        });
    }
    if y_extent > volume.y() {
        return Err(ScoreError::BuildVolumeExceeded {
            model: artifact.model().clone(),
            axis: "y",
            observed_mm: y_extent,
            allowed_mm: volume.y(),
        });
    }
    if z_extent > volume.z() {
        return Err(ScoreError::BuildVolumeExceeded {
            model: artifact.model().clone(),
            axis: "z",
            observed_mm: z_extent,
            allowed_mm: volume.z(),
        });
    }
    MetricValue::new(z_extent).map_err(|_| ScoreError::ArithmeticOverflow {
        model: artifact.model().clone(),
        metric: "z_height",
    })
}

fn extend_world_bounds(
    bounds: &mut [f64; 6],
    model: &ModelId,
    point: crate::Vec3,
    placement: crate::Vec3,
) -> Result<(), ScoreError> {
    let world = [
        point[0] + placement[0],
        point[1] + placement[1],
        point[2] + placement[2],
    ];
    for (axis, coordinate) in ["x", "y", "z"].iter().zip(world) {
        if !coordinate.is_finite() {
            return Err(ScoreError::NonFiniteBound {
                model: model.clone(),
                axis,
            });
        }
    }
    bounds[0] = bounds[0].min(world[0]);
    bounds[1] = bounds[1].max(world[0]);
    bounds[2] = bounds[2].min(world[1]);
    bounds[3] = bounds[3].max(world[1]);
    bounds[4] = bounds[4].min(world[2]);
    bounds[5] = bounds[5].max(world[2]);
    Ok(())
}

/// The milestone at which comparison is taking place.  It controls candidate
/// hard-bound activation; it never changes the M1 metric table itself.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ComparisonMilestone {
    M1,
    M2,
    M3,
    M4,
    M5,
    M6,
    M7,
    M8,
    M9,
}

/// An explicit comparison shape.  Hard bounds are prohibited for an actual
/// self-comparison even when its values happen to violate later bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparisonScope {
    SelfComparison,
    Candidate,
}

/// Owner-signed waiver evidence.  The signature must carry the core's strict
/// `OwnerDecision` provenance; a bare string cannot authorise a regression.
#[derive(Clone, Debug, PartialEq)]
pub struct SignedWaiver {
    model: ModelId,
    target: String,
    signature: EvidenceSource,
    expires_on: CalendarDate,
}

impl SignedWaiver {
    #[must_use]
    pub fn new(
        model: ModelId,
        target: impl Into<String>,
        signature: EvidenceSource,
        expires_on: CalendarDate,
    ) -> Self {
        Self {
            model,
            target: target.into(),
            signature,
            expires_on,
        }
    }
}

/// Comparator settings supplied by the manifest/gate-table owner.
#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonPolicy<'a> {
    pub milestone: ComparisonMilestone,
    pub scope: ComparisonScope,
    pub as_of: &'a CalendarDate,
    pub waivers: &'a [SignedWaiver],
}

/// One deterministic per-model / per-metric result.
#[derive(Clone, Debug, PartialEq)]
pub struct MetricComparison {
    pub model: ModelId,
    pub id: MetricId,
    pub baseline: MetricValue,
    pub candidate: MetricValue,
    pub waived: bool,
    pub outcome: MetricOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetricOutcome {
    Equal,
    Better,
    Worse,
    HardBoundViolation,
    Waived,
}

/// Full deterministic comparison result, in baseline model order then M1 table order.
#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonReport {
    rows: Vec<MetricComparison>,
    producer_rows: Vec<ProducerComparison>,
}

impl ComparisonReport {
    #[must_use]
    pub fn rows(&self) -> &[MetricComparison] {
        &self.rows
    }
    #[must_use]
    pub fn producer_rows(&self) -> &[ProducerComparison] {
        &self.producer_rows
    }
    #[must_use]
    pub fn accepted(&self) -> bool {
        self.rows.iter().all(|row| {
            !matches!(
                row.outcome,
                MetricOutcome::Worse | MetricOutcome::HardBoundViolation
            )
        }) && self
            .producer_rows
            .iter()
            .all(|row| row.outcome == ProducerOutcome::BaselineFailedCandidateSucceeded)
    }
}

/// An explicit comparison for a model where at least one producer failed.
#[derive(Clone, Debug, PartialEq)]
pub struct ProducerComparison {
    pub model: ModelId,
    pub outcome: ProducerOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProducerOutcome {
    BaselineFailedCandidateSucceeded,
    CandidateFailed,
    BothFailed,
}

/// Comparator and waiver-validation failures.
#[derive(Debug, Error)]
pub enum ComparisonError {
    #[error("baseline and candidate model counts differ: {baseline} versus {candidate}")]
    ModelCount { baseline: usize, candidate: usize },
    #[error("duplicate {side} model {model} at rows {first_row} and {duplicate_row}")]
    DuplicateModel {
        side: &'static str,
        model: ModelId,
        first_row: usize,
        duplicate_row: usize,
    },
    #[error("candidate model at row {row} is {candidate}, expected baseline model {baseline}")]
    ModelOrder {
        row: usize,
        baseline: ModelId,
        candidate: ModelId,
    },
    #[error("waiver for unknown corpus model {model}")]
    WaiverUnknownModel { model: ModelId },
    #[error("waiver target {target} is not a known M1 metric or ratified group alias")]
    UnknownWaiverTarget { target: String },
    #[error("format metric {id} may never be waived")]
    UnwaivableFormat { id: String },
    #[error("waiver for model {model} / metric {id} duplicates an already expanded waiver")]
    DuplicateWaiver { model: ModelId, id: String },
    #[error("waiver for model {model} / target {target} expired on {expires_on}")]
    ExpiredWaiver {
        model: ModelId,
        target: String,
        expires_on: String,
    },
    #[error("waiver for model {model} / target {target} is not signed by an OwnerDecision evidence source")]
    UnsignedWaiver { model: ModelId, target: String },
}

/// Compares ordered baseline/candidate score batches without any implicit
/// tolerance.  The guide specifies exact self-comparison and no numeric
/// comparison tolerance; accepting an epsilon here would be an invented gate.
pub fn compare_artifacts(
    baseline: &[ScoredArtifact],
    candidate: &[ScoredArtifact],
    policy: &ComparisonPolicy<'_>,
) -> Result<ComparisonReport, ComparisonError> {
    validate_model_order(baseline, candidate)?;
    let waived = expand_waivers(baseline, policy)?;
    let mut rows = Vec::new();
    let mut producer_rows = Vec::new();
    for (baseline, candidate) in baseline.iter().zip(candidate) {
        match (baseline, candidate) {
            (ScoredArtifact::ProducerFailed { model }, ScoredArtifact::Success(_)) => {
                producer_rows.push(ProducerComparison {
                    model: model.clone(),
                    outcome: ProducerOutcome::BaselineFailedCandidateSucceeded,
                });
            }
            (ScoredArtifact::Success(_), ScoredArtifact::ProducerFailed { model }) => {
                producer_rows.push(ProducerComparison {
                    model: model.clone(),
                    outcome: ProducerOutcome::CandidateFailed,
                });
            }
            (ScoredArtifact::ProducerFailed { model }, ScoredArtifact::ProducerFailed { .. }) => {
                producer_rows.push(ProducerComparison {
                    model: model.clone(),
                    outcome: ProducerOutcome::BothFailed,
                });
            }
            (ScoredArtifact::Success(baseline), ScoredArtifact::Success(candidate)) => {
                for (index, id) in M1_METRIC_ORDER.iter().enumerate() {
                    let baseline_value = baseline.values[index];
                    let candidate_value = candidate.values[index];
                    let key = (baseline.model.clone(), (*id).to_owned());
                    let is_waived = waived.contains(&key);
                    let outcome =
                        compare_metric(id, baseline_value, candidate_value, is_waived, policy);
                    rows.push(MetricComparison {
                        model: baseline.model.clone(),
                        id: MetricId::from(*id),
                        baseline: baseline_value,
                        candidate: candidate_value,
                        waived: is_waived,
                        outcome,
                    });
                }
            }
        }
    }
    Ok(ComparisonReport {
        rows,
        producer_rows,
    })
}

fn compare_metric(
    id: &str,
    baseline: MetricValue,
    candidate: MetricValue,
    waived: bool,
    policy: &ComparisonPolicy<'_>,
) -> MetricOutcome {
    if waived {
        return MetricOutcome::Waived;
    }
    if hard_bound_active(id, policy) && violates_hard_bound(id, candidate.get()) {
        return MetricOutcome::HardBoundViolation;
    }
    match id {
        "loadable" => match (baseline.get() == 1.0, candidate.get() == 1.0) {
            (true, true) => MetricOutcome::Equal,
            (false, true) => MetricOutcome::Better,
            (true, false) | (false, false) => MetricOutcome::Worse,
        },
        _ if candidate.get() == baseline.get() => MetricOutcome::Equal,
        _ if candidate.get() < baseline.get() => MetricOutcome::Better,
        _ => MetricOutcome::Worse,
    }
}

fn hard_bound_active(id: &str, policy: &ComparisonPolicy<'_>) -> bool {
    policy.scope == ComparisonScope::Candidate
        && match id {
            "boundary_edges" | "negative_shells" | "winding" | "loadable" => {
                policy.milestone >= ComparisonMilestone::M4
            }
            "max_x" => policy.milestone >= ComparisonMilestone::M7,
            _ => false,
        }
}

fn violates_hard_bound(id: &str, value: f64) -> bool {
    match id {
        "loadable" => value != 1.0,
        "max_x" => value > 12.0,
        _ => value != 0.0,
    }
}

fn validate_model_order(
    baseline: &[ScoredArtifact],
    candidate: &[ScoredArtifact],
) -> Result<(), ComparisonError> {
    if baseline.len() != candidate.len() {
        return Err(ComparisonError::ModelCount {
            baseline: baseline.len(),
            candidate: candidate.len(),
        });
    }
    for (side, rows) in [("baseline", baseline), ("candidate", candidate)] {
        let mut seen = BTreeMap::new();
        for (row, entry) in rows.iter().enumerate() {
            if let Some(first_row) = seen.insert(entry.model().clone(), row) {
                return Err(ComparisonError::DuplicateModel {
                    side,
                    model: entry.model().clone(),
                    first_row,
                    duplicate_row: row,
                });
            }
        }
    }
    for (row, (baseline, candidate)) in baseline.iter().zip(candidate).enumerate() {
        if baseline.model() != candidate.model() {
            return Err(ComparisonError::ModelOrder {
                row,
                baseline: baseline.model().clone(),
                candidate: candidate.model().clone(),
            });
        }
    }
    Ok(())
}

fn expand_waivers(
    baseline: &[ScoredArtifact],
    policy: &ComparisonPolicy<'_>,
) -> Result<BTreeSet<(ModelId, String)>, ComparisonError> {
    let models = baseline
        .iter()
        .map(|row| row.model().clone())
        .collect::<BTreeSet<_>>();
    let mut expanded = BTreeSet::new();
    for waiver in policy.waivers {
        if !models.contains(&waiver.model) {
            return Err(ComparisonError::WaiverUnknownModel {
                model: waiver.model.clone(),
            });
        }
        if !matches!(
            waiver.signature.view(),
            EvidenceSourceView::OwnerDecision { .. }
        ) {
            return Err(ComparisonError::UnsignedWaiver {
                model: waiver.model.clone(),
                target: waiver.target.clone(),
            });
        }
        if waiver.expires_on.as_str() < policy.as_of.as_str() {
            return Err(ComparisonError::ExpiredWaiver {
                model: waiver.model.clone(),
                target: waiver.target.clone(),
                expires_on: waiver.expires_on.as_str().to_owned(),
            });
        }
        let targets = expand_target(&waiver.target)?;
        for id in targets {
            if FORMAT_METRICS.contains(&id) {
                return Err(ComparisonError::UnwaivableFormat { id: id.to_owned() });
            }
            if !expanded.insert((waiver.model.clone(), id.to_owned())) {
                return Err(ComparisonError::DuplicateWaiver {
                    model: waiver.model.clone(),
                    id: id.to_owned(),
                });
            }
        }
    }
    Ok(expanded)
}

fn expand_target(target: &str) -> Result<Vec<&'static str>, ComparisonError> {
    let ids = match target {
        "scar" => SCAR_METRICS.to_vec(),
        "resin" => RESIN_METRICS.to_vec(),
        "format" => FORMAT_METRICS.to_vec(),
        "resin_model" => vec!["resin_model", "resin_total"],
        "resin_support" => vec!["resin_support", "resin_total"],
        _ => M1_METRIC_ORDER
            .into_iter()
            .find(|id| *id == target)
            .map(|id| vec![id])
            .ok_or_else(|| ComparisonError::UnknownWaiverTarget {
                target: target.to_owned(),
            })?,
    };
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{load_artifacts, ArtifactDisposition, ArtifactRequest, ContactArmInput};
    use plinth_core::{EvidenceDate, EvidenceId, EvidenceSource};

    fn values(values: [f64; 15]) -> ScoredArtifact {
        ScoredArtifact::Success(ScoredModel {
            model: ModelId::from("Sphere"),
            values: values.map(|value| MetricValue::new(value).unwrap()),
        })
    }

    fn policy(scope: ComparisonScope, milestone: ComparisonMilestone) -> ComparisonPolicy<'static> {
        let as_of = Box::leak(Box::new(CalendarDate::new("2026-08-01").unwrap()));
        ComparisonPolicy {
            milestone,
            scope,
            as_of,
            waivers: &[],
        }
    }

    #[test]
    fn m1_table_is_exact_and_has_no_group_x() {
        assert_eq!(M1_METRIC_ORDER.len(), 15);
        assert!(!M1_METRIC_ORDER.contains(&"max_x_group"));
        assert_eq!(M1_METRIC_ORDER[5], "max_x");
    }

    #[test]
    fn exact_self_comparison_is_green_even_before_later_hard_bounds() {
        let score = values([
            0.0, 0.0, 1.0, 1.0, 1.0, 3.0, 2.0, 1.0, 1.0, 2.0, 1.0, 1.0, 1.0, 20.0, 1.0,
        ]);
        let report = compare_artifacts(
            std::slice::from_ref(&score),
            std::slice::from_ref(&score),
            &policy(ComparisonScope::SelfComparison, ComparisonMilestone::M4),
        )
        .unwrap();
        assert!(report.accepted());
        assert!(report
            .rows()
            .iter()
            .all(|row| row.outcome == MetricOutcome::Equal));
    }

    #[test]
    fn every_numeric_direction_is_lower_and_loadable_is_pass() {
        let baseline = values([2.0; 15]);
        let mut candidate_values = [2.0; 15];
        candidate_values[14] = 1.0;
        let candidate = values(candidate_values);
        let report = compare_artifacts(
            std::slice::from_ref(&baseline),
            std::slice::from_ref(&candidate),
            &policy(ComparisonScope::Candidate, ComparisonMilestone::M1),
        )
        .unwrap();
        assert!(report
            .rows()
            .iter()
            .take(14)
            .all(|row| row.outcome == MetricOutcome::Equal));
        assert_eq!(report.rows()[14].outcome, MetricOutcome::Better);
    }

    #[test]
    fn candidate_hard_bounds_begin_at_m4_only() {
        let baseline = values([0.0; 15]);
        let mut candidate_values = [0.0; 15];
        candidate_values[2] = 1.0;
        let candidate = values(candidate_values);
        let early = compare_artifacts(
            std::slice::from_ref(&baseline),
            std::slice::from_ref(&candidate),
            &policy(ComparisonScope::Candidate, ComparisonMilestone::M1),
        )
        .unwrap();
        assert_eq!(early.rows()[2].outcome, MetricOutcome::Worse);
        let late = compare_artifacts(
            std::slice::from_ref(&baseline),
            std::slice::from_ref(&candidate),
            &policy(ComparisonScope::Candidate, ComparisonMilestone::M4),
        )
        .unwrap();
        assert_eq!(late.rows()[2].outcome, MetricOutcome::HardBoundViolation);
    }

    #[test]
    fn per_arm_x_absolute_cap_begins_only_at_m7() {
        let baseline = values([0.0; 15]);
        let mut candidate_values = [0.0; 15];
        candidate_values[5] = 12.1;
        let candidate = values(candidate_values);
        let m6 = compare_artifacts(
            std::slice::from_ref(&baseline),
            std::slice::from_ref(&candidate),
            &policy(ComparisonScope::Candidate, ComparisonMilestone::M6),
        )
        .unwrap();
        assert_eq!(m6.rows()[5].outcome, MetricOutcome::Worse);
        let m7 = compare_artifacts(
            std::slice::from_ref(&baseline),
            std::slice::from_ref(&candidate),
            &policy(ComparisonScope::Candidate, ComparisonMilestone::M7),
        )
        .unwrap();
        assert_eq!(m7.rows()[5].outcome, MetricOutcome::HardBoundViolation);
    }

    #[test]
    fn python_failure_is_an_unconditional_rust_success_win() {
        let baseline = ScoredArtifact::ProducerFailed {
            model: ModelId::from("Sphere"),
        };
        let candidate = values([0.0; 15]);
        let report = compare_artifacts(
            &[baseline],
            &[candidate],
            &policy(ComparisonScope::Candidate, ComparisonMilestone::M1),
        )
        .unwrap();
        assert!(report.accepted());
        assert_eq!(
            report.producer_rows()[0].outcome,
            ProducerOutcome::BaselineFailedCandidateSucceeded
        );
    }

    #[test]
    fn format_is_never_waivable_and_resin_component_carries_total() {
        let signature = EvidenceSource::owner_decision(
            EvidenceId::new("test-owner-signature").unwrap(),
            "docs/rust-migration/06-conventions.md",
            "gate table",
            EvidenceDate::exact(CalendarDate::new("2026-08-01").unwrap()),
        )
        .unwrap();
        let expires = CalendarDate::new("2026-08-02").unwrap();
        let format = SignedWaiver::new(
            ModelId::from("Sphere"),
            "format",
            signature.clone(),
            expires.clone(),
        );
        assert!(matches!(
            expand_waivers(
                &[values([0.0; 15])],
                &ComparisonPolicy {
                    milestone: ComparisonMilestone::M1,
                    scope: ComparisonScope::Candidate,
                    as_of: &CalendarDate::new("2026-08-01").unwrap(),
                    waivers: &[format]
                }
            ),
            Err(ComparisonError::UnwaivableFormat { .. })
        ));
        let resin = SignedWaiver::new(ModelId::from("Sphere"), "resin_support", signature, expires);
        let date = CalendarDate::new("2026-08-01").unwrap();
        let expanded = expand_waivers(
            &[values([0.0; 15])],
            &ComparisonPolicy {
                milestone: ComparisonMilestone::M1,
                scope: ComparisonScope::Candidate,
                as_of: &date,
                waivers: &[resin],
            },
        )
        .unwrap();
        assert!(expanded.contains(&(ModelId::from("Sphere"), "resin_total".to_owned())));
    }

    #[test]
    fn committed_sphere_is_scored_from_fresh_disk_with_all_fifteen_rows() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("corpus/baseline/Sphere.chitubox");
        let loads = load_artifacts(&[ArtifactRequest::new(
            ModelId::from("Sphere"),
            ArtifactDisposition::Produced { path },
        )])
        .unwrap();
        let artifact = loads[0].as_artifact().unwrap();
        let contact_arms = artifact.models()[0]
            .supports()
            .iter()
            .enumerate()
            .filter(|(_, support)| support.segment_type() == chitubox_io::SegmentType::TAPER)
            .enumerate()
            .map(|(taper_index, (support_record_index, _))| ContactArmInput {
                support_record_index,
                taper_index,
                voronoi_area_mm2: 0.0,
            })
            .collect();
        let scores = score_saturn_artifacts(
            &loads,
            &[ArtifactMetricInputs {
                topology: vec![TopologyMetricInput {
                    source_index: 0,
                    islands: 0,
                }],
                supports: vec![SupportMetricInput {
                    source_index: 0,
                    contact_arms,
                }],
                scars: vec![ScarMetricInput { source_index: 0 }],
            }],
        )
        .unwrap();
        let ScoredArtifact::Success(score) = &scores[0] else {
            panic!("Sphere baseline is an artifact")
        };
        assert_eq!(
            score
                .entries()
                .map(|(id, _)| id)
                .map(|id| id.as_str().to_owned()),
            M1_METRIC_ORDER.map(str::to_owned)
        );
        assert_eq!(score.value("loadable").unwrap().get(), 1.0);
        assert_eq!(
            score.entries().map(|(_, value)| value.get()),
            [
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                47.0,
                4.174_747_768_289_511,
                0.093_536_727_217_749_03,
                4.268_284_495_507_260_5,
                18.207_723_309_160_54,
                3.339_084_784_039_538,
                16.101_929_450_319_485,
                22.996_585_845_947_266,
                1.0,
            ]
        );
    }
}
