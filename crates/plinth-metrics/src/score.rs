//! Disk-only M1 score assembly and the as-good-or-better comparator.
//!
//! This module never accepts producer memory or caller-held disk snapshots.
//! Callers submit ordered manifest requests and this module reopens the batch
//! exactly once through [`crate::load_artifacts`]. Score assembly deliberately consumes the independent
//! topology, support, and scar evidence explicitly; a missing observation is a
//! named error rather than an invented zero.

use std::collections::{BTreeMap, BTreeSet};

use plinth_core::{
    m0_constants, M0ConstantsError, MetricId, MetricSet, MetricValue, ModelId, PrinterProfile,
};
use thiserror::Error;

use crate::{
    load_artifacts, scar_metric_rows, support_metric_rows, topology_metric_rows, ArtifactLoad,
    ArtifactRequest, DiskArtifact, DiskReadError, ScarMetricError, ScarMetricInput,
    SupportMetricError, SupportMetricInput, TopologyMetricError, TopologyMetricInput,
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

// This second literal is intentionally independent of `M1_METRIC_ORDER`.
// It is the score-assembly oracle: a reordered, missing, duplicate, or extra
// producer row must be an input failure rather than silently becoming a
// differently-labelled score.
const M1_SCORE_ORDER_ORACLE: [&str; 15] = [
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

#[cfg(test)]
const FORMAT_METRICS: [&str; 5] = [
    "boundary_edges",
    "negative_shells",
    "winding",
    "cavities",
    "loadable",
];
#[cfg(test)]
const SCAR_METRICS: [&str; 3] = ["scar_area", "scar_worst_patch", "scar_weighted"];
#[cfg(test)]
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
    #[error(transparent)]
    DiskRead(#[from] DiskReadError),
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
    #[error("artifact for {model} assembled {actual} metric rows; the M1 score table requires {expected}")]
    MetricRowCount {
        model: ModelId,
        expected: usize,
        actual: usize,
    },
    #[error(
        "artifact for {model} has duplicate metric {id} at rows {first_row} and {duplicate_row}"
    )]
    DuplicateMetric {
        model: ModelId,
        id: MetricId,
        first_row: usize,
        duplicate_row: usize,
    },
    #[error("artifact for {model} metric row {row} is {actual}, expected {expected}")]
    MetricOrder {
        model: ModelId,
        row: usize,
        actual: MetricId,
        expected: &'static str,
    },
    #[error("artifact for {model} metric {id} is non-finite")]
    NonFiniteMetric { model: ModelId, id: MetricId },
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
    requests: &[ArtifactRequest],
    inputs: &[ArtifactMetricInputs],
) -> Result<Vec<ScoredArtifact>, ScoreError> {
    let constants = m0_constants().map_err(|error| ScoreError::Constants(error.clone()))?;
    score_artifacts(requests, inputs, constants.saturn4_ultra())
}

/// Scores a complete ordered batch against the supplied validated printer profile.
///
/// This is useful for future printer selection, while the public M1 entry point
/// above ensures Saturn's build volume is never duplicated as a local number.
pub fn score_artifacts(
    requests: &[ArtifactRequest],
    inputs: &[ArtifactMetricInputs],
    profile: &PrinterProfile,
) -> Result<Vec<ScoredArtifact>, ScoreError> {
    // The only disk read in this public scoring path. All format, geometry and
    // metric values below derive from this single owned snapshot.
    let artifacts = load_artifacts(requests)?;
    score_loaded_artifacts(&artifacts, inputs, profile)
}

fn score_loaded_artifacts(
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

    let mut rows = Vec::with_capacity(M1_SCORE_ORDER_ORACLE.len());
    for (id, value) in topology_row.entries() {
        rows.push((id, value.get()));
    }
    for (id, value) in support_row.entries() {
        rows.push((id, value.get()));
    }
    for (id, value) in scar_row.entries() {
        rows.push((id, value.get()));
    }
    rows.push((MetricId::from("z_height"), z_height.get()));
    rows.push((MetricId::from("loadable"), loadable.get()));
    let values = validate_metric_rows(artifact.model(), &rows)?;
    Ok(ScoredModel {
        model: artifact.model().clone(),
        values,
    })
}

fn validate_metric_rows(
    model: &ModelId,
    rows: &[(MetricId, f64)],
) -> Result<[MetricValue; M1_SCORE_ORDER_ORACLE.len()], ScoreError> {
    if rows.len() != M1_SCORE_ORDER_ORACLE.len() {
        return Err(ScoreError::MetricRowCount {
            model: model.clone(),
            expected: M1_SCORE_ORDER_ORACLE.len(),
            actual: rows.len(),
        });
    }
    let mut seen = BTreeMap::new();
    for (row, (id, value)) in rows.iter().enumerate() {
        if let Some(first_row) = seen.insert(id.clone(), row) {
            return Err(ScoreError::DuplicateMetric {
                model: model.clone(),
                id: id.clone(),
                first_row,
                duplicate_row: row,
            });
        }
        let expected = M1_SCORE_ORDER_ORACLE[row];
        if id.as_str() != expected {
            return Err(ScoreError::MetricOrder {
                model: model.clone(),
                row,
                actual: id.clone(),
                expected,
            });
        }
        if !value.is_finite() {
            return Err(ScoreError::NonFiniteMetric {
                model: model.clone(),
                id: id.clone(),
            });
        }
    }
    Ok(std::array::from_fn(|row| {
        MetricValue::new(rows[row].1).expect("finite metric rows are validated")
    }))
}

fn z_height_and_build_bounds(
    artifact: &DiskArtifact,
    profile: &PrinterProfile,
) -> Result<MetricValue, ScoreError> {
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
        // ChituBox meshes and support records share a model-centred local
        // frame. The plate plane is not `placement.z`: it is the local model
        // minimum less the header lift. This makes the physical build frame
        // Z=[0, build_z] while X/Y remain centred on the plate.
        let local_plate_z = model
            .mesh()
            .vertices()
            .iter()
            .map(|point| point[2])
            .min_by(f64::total_cmp)
            .ok_or_else(|| ScoreError::EmptyBounds {
                model: artifact.model().clone(),
            })?
            - placement[2];
        for point in model.mesh().vertices().iter().chain(
            model
                .supports()
                .iter()
                .flat_map(|support| support.mesh().vertices().iter()),
        ) {
            extend_world_bounds(
                &mut bounds,
                artifact.model(),
                *point,
                placement,
                local_plate_z,
            )?;
        }
        for point in model
            .supports()
            .iter()
            .flat_map(|support| [support.start(), support.end()])
        {
            extend_world_bounds(
                &mut bounds,
                artifact.model(),
                point,
                placement,
                local_plate_z,
            )?;
        }
    }
    if bounds[0] == f64::INFINITY {
        return Err(ScoreError::EmptyBounds {
            model: artifact.model().clone(),
        });
    }
    validate_build_bounds(artifact.model(), bounds, profile)
}

fn validate_build_bounds(
    model: &ModelId,
    bounds: [f64; 6],
    profile: &PrinterProfile,
) -> Result<MetricValue, ScoreError> {
    let volume = profile.build_volume_mm().value();
    if bounds[0] == f64::INFINITY {
        return Err(ScoreError::EmptyBounds {
            model: model.clone(),
        });
    }
    let x_half_extent = volume.x() / 2.0;
    let y_half_extent = volume.y() / 2.0;
    if bounds[0] < -x_half_extent || bounds[1] > x_half_extent {
        return Err(ScoreError::BuildVolumeExceeded {
            model: model.clone(),
            axis: "x",
            observed_mm: bounds[0].abs().max(bounds[1].abs()),
            allowed_mm: x_half_extent,
        });
    }
    if bounds[2] < -y_half_extent || bounds[3] > y_half_extent {
        return Err(ScoreError::BuildVolumeExceeded {
            model: model.clone(),
            axis: "y",
            observed_mm: bounds[2].abs().max(bounds[3].abs()),
            allowed_mm: y_half_extent,
        });
    }
    if bounds[4] < 0.0 || bounds[5] > volume.z() {
        return Err(ScoreError::BuildVolumeExceeded {
            model: model.clone(),
            axis: "z",
            observed_mm: if bounds[4] < 0.0 {
                -bounds[4]
            } else {
                bounds[5]
            },
            allowed_mm: volume.z(),
        });
    }
    MetricValue::new(bounds[5]).map_err(|_| ScoreError::ArithmeticOverflow {
        model: model.clone(),
        metric: "z_height",
    })
}

fn extend_world_bounds(
    bounds: &mut [f64; 6],
    model: &ModelId,
    point: crate::Vec3,
    placement: crate::Vec3,
    local_plate_z: f64,
) -> Result<(), ScoreError> {
    let world = [
        point[0] + placement[0],
        point[1] + placement[1],
        point[2] - local_plate_z,
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

/// A validated, opaque waiver set.
///
/// The only public constructor is the safe empty set. Nonempty construction is
/// deliberately reserved for the upcoming canonical manifest plus §06 gate
/// table validator; caller-provided DTOs cannot authorise regressions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ValidatedWaivers {
    entries: Vec<ValidatedWaiver>,
}

impl ValidatedWaivers {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ValidatedWaiver {
    model: ModelId,
    ids: Vec<&'static str>,
}

// Intentionally private: no CLI or library caller can turn a raw owner claim
// into a waiver until the canonical cross-validator exists.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
struct RawWaiver {
    model: ModelId,
    target: String,
    signature: plinth_core::EvidenceSource,
    expires_on: plinth_core::CalendarDate,
}

/// Comparator settings supplied by the manifest/gate-table owner.
#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonPolicy<'a> {
    pub milestone: ComparisonMilestone,
    pub scope: ComparisonScope,
    pub waivers: &'a ValidatedWaivers,
}

/// One deterministic per-model / per-metric result.
#[derive(Clone, Debug, PartialEq)]
pub struct MetricComparison {
    pub model: ModelId,
    pub id: MetricId,
    pub baseline: MetricValue,
    pub candidate: MetricValue,
    /// Candidate minus baseline in the metric's native unit. `None` for
    /// categorical outcomes such as `loadable`.
    pub raw_delta: Option<MetricValue>,
    /// Positive-only amount by which the candidate regresses. This is the
    /// direction-normalised gate delta; `None` for categorical outcomes.
    pub regression_delta: Option<MetricValue>,
    pub waived: bool,
    pub outcome: MetricOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetricOutcome {
    /// A metric deliberately emitted but not judged at this milestone.
    Reported,
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
    #[error("metric delta for {model} / {id} is non-finite")]
    MetricDelta { model: ModelId, id: MetricId },
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
    let waived = expand_waivers(policy.waivers);
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
                    let (raw_delta, regression_delta) =
                        metric_deltas(id, baseline_value, candidate_value, baseline.model())?;
                    let outcome =
                        compare_metric(id, baseline_value, candidate_value, is_waived, policy);
                    rows.push(MetricComparison {
                        model: baseline.model.clone(),
                        id: MetricId::from(*id),
                        baseline: baseline_value,
                        candidate: candidate_value,
                        raw_delta,
                        regression_delta,
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
    // A self-comparison is a reproducibility check, not an opportunity to
    // back-apply a later candidate gate. Its fixed contract is exact green
    // rows for all fifteen metrics.
    if policy.scope == ComparisonScope::SelfComparison {
        return MetricOutcome::Equal;
    }
    if report_only(id, policy) {
        return MetricOutcome::Reported;
    }
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

fn metric_deltas(
    id: &str,
    baseline: MetricValue,
    candidate: MetricValue,
    model: &ModelId,
) -> Result<(Option<MetricValue>, Option<MetricValue>), ComparisonError> {
    if id == "loadable" {
        return Ok((None, None));
    }
    let raw = candidate.get() - baseline.get();
    let raw_delta = MetricValue::new(raw).map_err(|_| ComparisonError::MetricDelta {
        model: model.clone(),
        id: MetricId::from(id),
    })?;
    let regression_delta =
        MetricValue::new(raw.max(0.0)).map_err(|_| ComparisonError::MetricDelta {
            model: model.clone(),
            id: MetricId::from(id),
        })?;
    Ok((Some(raw_delta), Some(regression_delta)))
}

fn report_only(id: &str, policy: &ComparisonPolicy<'_>) -> bool {
    policy.scope == ComparisonScope::Candidate
        && id == "max_x"
        && policy.milestone <= ComparisonMilestone::M4
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

fn expand_waivers(waivers: &ValidatedWaivers) -> BTreeSet<(ModelId, String)> {
    let mut expanded = BTreeSet::new();
    for waiver in &waivers.entries {
        for id in &waiver.ids {
            expanded.insert((waiver.model.clone(), (*id).to_owned()));
        }
    }
    expanded
}

#[cfg(test)]
fn validate_waivers(
    raw: &[RawWaiver],
    known_models: &BTreeSet<ModelId>,
    as_of: &plinth_core::CalendarDate,
) -> Result<ValidatedWaivers, ComparisonError> {
    let mut entries = Vec::with_capacity(raw.len());
    let mut expanded = BTreeSet::new();
    for waiver in raw {
        if !known_models.contains(&waiver.model) {
            return Err(ComparisonError::WaiverUnknownModel {
                model: waiver.model.clone(),
            });
        }
        if !matches!(
            waiver.signature.view(),
            plinth_core::EvidenceSourceView::OwnerDecision { .. }
        ) {
            return Err(ComparisonError::UnsignedWaiver {
                model: waiver.model.clone(),
                target: waiver.target.clone(),
            });
        }
        if waiver.expires_on.as_str() < as_of.as_str() {
            return Err(ComparisonError::ExpiredWaiver {
                model: waiver.model.clone(),
                target: waiver.target.clone(),
                expires_on: waiver.expires_on.as_str().to_owned(),
            });
        }
        let ids = expand_target(&waiver.target)?;
        for id in &ids {
            if FORMAT_METRICS.contains(id) {
                return Err(ComparisonError::UnwaivableFormat {
                    id: (*id).to_owned(),
                });
            }
            if !expanded.insert((waiver.model.clone(), (*id).to_owned())) {
                return Err(ComparisonError::DuplicateWaiver {
                    model: waiver.model.clone(),
                    id: (*id).to_owned(),
                });
            }
        }
        entries.push(ValidatedWaiver {
            model: waiver.model.clone(),
            ids,
        });
    }
    Ok(ValidatedWaivers { entries })
}

#[cfg(test)]
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
    use chitubox_io::{
        ModelGeometry, ModelSource, Placement, Project, SegmentType, SupportBatch, SupportMode,
        SupportRecord, TriangleSoup,
    };
    use plinth_core::{CalendarDate, EvidenceDate, EvidenceId, EvidenceSource};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_FILE_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

    fn tetrahedron(size: f32) -> Vec<f32> {
        let vertices = [
            [0.0, 0.0, 0.0],
            [size, 0.0, 0.0],
            [0.0, size, 0.0],
            [0.0, 0.0, size],
        ];
        [[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]]
            .into_iter()
            .flat_map(|face| face.into_iter().flat_map(|vertex| vertices[vertex]))
            .collect()
    }

    fn translated(mut triangles: Vec<f32>, offset: [f32; 3]) -> Vec<f32> {
        for point in triangles.chunks_exact_mut(3) {
            for (coordinate, shift) in point.iter_mut().zip(offset) {
                *coordinate += shift;
            }
        }
        triangles
    }

    fn frame_artifact(placement: Placement, support: Option<SupportRecord>) -> DiskArtifact {
        let sequence = TEST_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("plinth-metrics-frame-{sequence}.chitubox"));
        let mut project = Project::new_empty();
        project
            .add_models(&[ModelSource {
                name: "frame".to_owned(),
                geometry: ModelGeometry::Triangles(TriangleSoup::new(tetrahedron(10.0))),
                placement,
            }])
            .unwrap();
        if let Some(record) = support {
            project
                .attach_supports(&[SupportBatch {
                    model_index: 0,
                    records: vec![record],
                    mode: SupportMode::Replace,
                    lift_mm: Some(placement.z_lift),
                }])
                .unwrap();
        }
        project.save(&path).unwrap();
        let rows = load_artifacts(&[ArtifactRequest::new(
            ModelId::from("Frame"),
            ArtifactDisposition::Produced { path },
        )])
        .unwrap();
        rows.into_iter()
            .next()
            .unwrap()
            .as_artifact()
            .unwrap()
            .clone()
    }

    fn values_for(model: &str, values: [f64; 15]) -> ScoredArtifact {
        ScoredArtifact::Success(ScoredModel {
            model: ModelId::from(model),
            values: values.map(|value| MetricValue::new(value).unwrap()),
        })
    }

    fn values(values: [f64; 15]) -> ScoredArtifact {
        values_for("Sphere", values)
    }

    fn policy(scope: ComparisonScope, milestone: ComparisonMilestone) -> ComparisonPolicy<'static> {
        ComparisonPolicy {
            milestone,
            scope,
            waivers: Box::leak(Box::new(ValidatedWaivers::empty())),
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
            .enumerate()
            .all(|(index, row)| {
                (index == 5 && row.outcome == MetricOutcome::Reported)
                    || (index != 5 && row.outcome == MetricOutcome::Equal)
            }));
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
    fn every_m4_format_absolute_is_inclusive_at_zero_and_rejects_the_next_value() {
        for index in [2_usize, 3, 4] {
            let baseline = values([0.0; 15]);
            let zero = values([0.0; 15]);
            let accepted = compare_artifacts(
                std::slice::from_ref(&baseline),
                std::slice::from_ref(&zero),
                &policy(ComparisonScope::Candidate, ComparisonMilestone::M4),
            )
            .unwrap();
            assert_ne!(
                accepted.rows()[index].outcome,
                MetricOutcome::HardBoundViolation
            );
            let mut outside = [0.0; 15];
            outside[index] = 1.0;
            let rejected = compare_artifacts(
                std::slice::from_ref(&baseline),
                std::slice::from_ref(&values(outside)),
                &policy(ComparisonScope::Candidate, ComparisonMilestone::M4),
            )
            .unwrap();
            assert_eq!(
                rejected.rows()[index].outcome,
                MetricOutcome::HardBoundViolation
            );
        }
        let baseline = values([0.0; 15]);
        let mut loadable = [0.0; 15];
        loadable[14] = 1.0;
        let pass = compare_artifacts(
            std::slice::from_ref(&baseline),
            std::slice::from_ref(&values(loadable)),
            &policy(ComparisonScope::Candidate, ComparisonMilestone::M4),
        )
        .unwrap();
        assert_ne!(pass.rows()[14].outcome, MetricOutcome::HardBoundViolation);
        let fail = compare_artifacts(
            std::slice::from_ref(&baseline),
            std::slice::from_ref(&baseline),
            &policy(ComparisonScope::Candidate, ComparisonMilestone::M4),
        )
        .unwrap();
        assert_eq!(fail.rows()[14].outcome, MetricOutcome::HardBoundViolation);
    }

    #[test]
    fn cavities_remain_relative_not_worse_at_m4() {
        let mut one = [0.0; 15];
        one[1] = 1.0;
        let baseline = values(one);
        let same = compare_artifacts(
            std::slice::from_ref(&baseline),
            std::slice::from_ref(&baseline),
            &policy(ComparisonScope::Candidate, ComparisonMilestone::M4),
        )
        .unwrap();
        assert_eq!(same.rows()[1].outcome, MetricOutcome::Equal);
        let mut two = one;
        two[1] = 2.0;
        let worse = compare_artifacts(
            std::slice::from_ref(&baseline),
            std::slice::from_ref(&values(two)),
            &policy(ComparisonScope::Candidate, ComparisonMilestone::M4),
        )
        .unwrap();
        assert_eq!(worse.rows()[1].outcome, MetricOutcome::Worse);
        let mut zero = one;
        zero[1] = 0.0;
        let better = compare_artifacts(
            std::slice::from_ref(&baseline),
            std::slice::from_ref(&values(zero)),
            &policy(ComparisonScope::Candidate, ComparisonMilestone::M4),
        )
        .unwrap();
        assert_eq!(better.rows()[1].outcome, MetricOutcome::Better);
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
        let mut at_boundary = [0.0; 15];
        at_boundary[5] = 12.0;
        let boundary = compare_artifacts(
            std::slice::from_ref(&baseline),
            std::slice::from_ref(&values(at_boundary)),
            &policy(ComparisonScope::Candidate, ComparisonMilestone::M7),
        )
        .unwrap();
        assert_eq!(boundary.rows()[5].outcome, MetricOutcome::Worse);
    }

    #[test]
    fn max_x_is_reported_through_m4_then_becomes_relative_before_m7_cap() {
        let baseline = values([0.0; 15]);
        let mut candidate_values = [0.0; 15];
        candidate_values[5] = 12.1;
        let candidate = values(candidate_values);
        for milestone in [
            ComparisonMilestone::M1,
            ComparisonMilestone::M2,
            ComparisonMilestone::M3,
            ComparisonMilestone::M4,
        ] {
            let report = compare_artifacts(
                std::slice::from_ref(&baseline),
                std::slice::from_ref(&candidate),
                &policy(ComparisonScope::Candidate, milestone),
            )
            .unwrap();
            assert_eq!(report.rows()[5].outcome, MetricOutcome::Reported);
        }
        let m5 = compare_artifacts(
            std::slice::from_ref(&baseline),
            std::slice::from_ref(&candidate),
            &policy(ComparisonScope::Candidate, ComparisonMilestone::M5),
        )
        .unwrap();
        assert_eq!(m5.rows()[5].outcome, MetricOutcome::Worse);
    }

    #[test]
    fn numeric_deltas_and_directions_are_exact() {
        for index in 0..14 {
            let baseline = values([2.0; 15]);
            let mut better = [2.0; 15];
            better[index] = 1.0;
            let report = compare_artifacts(
                std::slice::from_ref(&baseline),
                std::slice::from_ref(&values(better)),
                &policy(
                    ComparisonScope::Candidate,
                    if index == 5 {
                        ComparisonMilestone::M5
                    } else {
                        ComparisonMilestone::M3
                    },
                ),
            )
            .unwrap();
            let row = &report.rows()[index];
            assert_eq!(row.outcome, MetricOutcome::Better);
            assert_eq!(row.raw_delta.unwrap().get(), -1.0);
            assert_eq!(row.regression_delta.unwrap().get(), 0.0);

            let mut worse = [2.0; 15];
            worse[index] = 3.0;
            let report = compare_artifacts(
                std::slice::from_ref(&baseline),
                std::slice::from_ref(&values(worse)),
                &policy(
                    ComparisonScope::Candidate,
                    if index == 5 {
                        ComparisonMilestone::M5
                    } else {
                        ComparisonMilestone::M3
                    },
                ),
            )
            .unwrap();
            let row = &report.rows()[index];
            assert_eq!(row.outcome, MetricOutcome::Worse);
            assert_eq!(row.raw_delta.unwrap().get(), 1.0);
            assert_eq!(row.regression_delta.unwrap().get(), 1.0);
        }
    }

    #[test]
    fn frame_is_centred_xy_and_plate_anchored_z_for_models_and_support_geometry() {
        let profile = m0_constants().unwrap().saturn4_ultra();
        let elevated = frame_artifact(
            Placement {
                x: 0.0,
                y: 0.0,
                z_lift: 10.0,
            },
            None,
        );
        assert_eq!(
            z_height_and_build_bounds(&elevated, profile).unwrap().get(),
            20.0
        );

        let x_half = profile.build_volume_mm().value().x() as f32 / 2.0;
        let boundary = frame_artifact(
            Placement {
                x: x_half - 5.0,
                y: 0.0,
                z_lift: 0.0,
            },
            None,
        );
        assert!(z_height_and_build_bounds(&boundary, profile).is_ok());
        let outside = frame_artifact(
            Placement {
                x: x_half - 4.999,
                y: 0.0,
                z_lift: 0.0,
            },
            None,
        );
        assert!(matches!(
            z_height_and_build_bounds(&outside, profile),
            Err(ScoreError::BuildVolumeExceeded { axis: "x", .. })
        ));
        let y_outside = frame_artifact(
            Placement {
                x: 0.0,
                y: profile.build_volume_mm().value().y() as f32 / 2.0 - 4.999,
                z_lift: 0.0,
            },
            None,
        );
        assert!(matches!(
            z_height_and_build_bounds(&y_outside, profile),
            Err(ScoreError::BuildVolumeExceeded { axis: "y", .. })
        ));

        let support_mesh_outside = SupportRecord::new(
            SegmentType::FLARE,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1.0,
            1.0,
            TriangleSoup::new(translated(tetrahedron(1.0), [110.0, 0.0, 0.0])),
        );
        let mesh = frame_artifact(Placement::default(), Some(support_mesh_outside));
        assert!(matches!(
            z_height_and_build_bounds(&mesh, profile),
            Err(ScoreError::BuildVolumeExceeded { axis: "x", .. })
        ));
        let endpoint_outside = SupportRecord::new(
            SegmentType::FLARE,
            [0.0, 0.0, -1.0],
            [0.0, -100.0, 1.0],
            1.0,
            1.0,
            TriangleSoup::new(tetrahedron(1.0)),
        );
        let endpoints = frame_artifact(Placement::default(), Some(endpoint_outside));
        assert!(matches!(
            z_height_and_build_bounds(&endpoints, profile),
            Err(ScoreError::BuildVolumeExceeded { axis: "y", .. })
        ));
        let below_plate = SupportRecord::new(
            SegmentType::FLARE,
            [0.0, 0.0, -10.0],
            [0.0, 0.0, 1.0],
            1.0,
            1.0,
            TriangleSoup::new(tetrahedron(1.0)),
        );
        let negative_z = frame_artifact(Placement::default(), Some(below_plate));
        assert!(matches!(
            z_height_and_build_bounds(&negative_z, profile),
            Err(ScoreError::BuildVolumeExceeded { axis: "z", .. })
        ));
    }

    #[test]
    fn physical_build_bounds_are_inclusive_then_fail_at_the_next_representable_value() {
        let profile = m0_constants().unwrap().saturn4_ultra();
        let volume = profile.build_volume_mm().value();
        let x_half = volume.x() / 2.0;
        let y_half = volume.y() / 2.0;
        let at_boundary = [-x_half, x_half, -y_half, y_half, 0.0, volume.z()];
        assert_eq!(
            validate_build_bounds(&ModelId::from("Frame"), at_boundary, profile)
                .unwrap()
                .get(),
            volume.z()
        );
        let mut y_positive = at_boundary;
        y_positive[3] = f64::from_bits(y_half.to_bits() + 1);
        assert!(matches!(
            validate_build_bounds(&ModelId::from("Frame"), y_positive, profile),
            Err(ScoreError::BuildVolumeExceeded { axis: "y", .. })
        ));
        let mut y_negative = at_boundary;
        y_negative[2] = -f64::from_bits(y_half.to_bits() + 1);
        assert!(matches!(
            validate_build_bounds(&ModelId::from("Frame"), y_negative, profile),
            Err(ScoreError::BuildVolumeExceeded { axis: "y", .. })
        ));
        let mut z_next = at_boundary;
        z_next[5] = f64::from_bits(volume.z().to_bits() + 1);
        assert!(matches!(
            validate_build_bounds(&ModelId::from("Frame"), z_next, profile),
            Err(ScoreError::BuildVolumeExceeded { axis: "z", .. })
        ));
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
    fn producer_failure_outcomes_and_comparator_batch_shape_fail_loudly() {
        let failed = ScoredArtifact::ProducerFailed {
            model: ModelId::from("Sphere"),
        };
        let successful = values([0.0; 15]);
        let candidate_failed = compare_artifacts(
            std::slice::from_ref(&successful),
            std::slice::from_ref(&failed),
            &policy(ComparisonScope::Candidate, ComparisonMilestone::M1),
        )
        .unwrap();
        assert_eq!(
            candidate_failed.producer_rows()[0].outcome,
            ProducerOutcome::CandidateFailed
        );
        assert!(!candidate_failed.accepted());
        let both_failed = compare_artifacts(
            std::slice::from_ref(&failed),
            std::slice::from_ref(&failed),
            &policy(ComparisonScope::Candidate, ComparisonMilestone::M1),
        )
        .unwrap();
        assert_eq!(
            both_failed.producer_rows()[0].outcome,
            ProducerOutcome::BothFailed
        );
        assert!(!both_failed.accepted());
        assert!(matches!(
            compare_artifacts(
                std::slice::from_ref(&successful),
                &[],
                &policy(ComparisonScope::Candidate, ComparisonMilestone::M1),
            ),
            Err(ComparisonError::ModelCount { .. })
        ));
        assert!(matches!(
            compare_artifacts(
                &[values([0.0; 15]), values([0.0; 15])],
                &[values([0.0; 15]), values([0.0; 15])],
                &policy(ComparisonScope::Candidate, ComparisonMilestone::M1),
            ),
            Err(ComparisonError::DuplicateModel { .. })
        ));
        assert!(matches!(
            compare_artifacts(
                &[
                    values_for("Sphere", [0.0; 15]),
                    values_for("Treant", [0.0; 15])
                ],
                &[
                    values_for("Treant", [0.0; 15]),
                    values_for("Sphere", [0.0; 15])
                ],
                &policy(ComparisonScope::Candidate, ComparisonMilestone::M1),
            ),
            Err(ComparisonError::ModelOrder { .. })
        ));
    }

    #[test]
    fn scoring_input_count_and_duplicate_requests_are_named_failures() {
        let evidence = ArtifactMetricInputs {
            topology: Vec::new(),
            supports: Vec::new(),
            scars: Vec::new(),
        };
        assert!(matches!(
            score_saturn_artifacts(&[], &[evidence]),
            Err(ScoreError::InputCount { .. })
        ));
        let requests = [
            ArtifactRequest::new(
                ModelId::from("Sphere"),
                ArtifactDisposition::Produced {
                    path: std::path::PathBuf::from("unused-a.chitubox"),
                },
            ),
            ArtifactRequest::new(
                ModelId::from("Sphere"),
                ArtifactDisposition::Produced {
                    path: std::path::PathBuf::from("unused-b.chitubox"),
                },
            ),
        ];
        assert!(matches!(
            score_saturn_artifacts(&requests, &[]),
            Err(ScoreError::DiskRead(
                DiskReadError::DuplicateCorpusModel { .. }
            ))
        ));
    }

    #[test]
    fn raw_waiver_validation_is_private_and_checks_authority_scope_and_expansion() {
        let signature = EvidenceSource::owner_decision(
            EvidenceId::new("test-owner-signature").unwrap(),
            "docs/rust-migration/06-conventions.md",
            "gate table",
            EvidenceDate::exact(CalendarDate::new("2026-08-01").unwrap()),
        )
        .unwrap();
        let expires = CalendarDate::new("2026-08-02").unwrap();
        let format = RawWaiver {
            model: ModelId::from("Sphere"),
            target: "format".to_owned(),
            signature: signature.clone(),
            expires_on: expires.clone(),
        };
        assert!(matches!(
            validate_waivers(
                &[format],
                &BTreeSet::from([ModelId::from("Sphere")]),
                &CalendarDate::new("2026-08-01").unwrap(),
            ),
            Err(ComparisonError::UnwaivableFormat { .. })
        ));
        let resin = RawWaiver {
            model: ModelId::from("Sphere"),
            target: "resin_support".to_owned(),
            signature,
            expires_on: expires,
        };
        let date = CalendarDate::new("2026-08-01").unwrap();
        let validated =
            validate_waivers(&[resin], &BTreeSet::from([ModelId::from("Sphere")]), &date).unwrap();
        let expanded = expand_waivers(&validated);
        assert!(expanded.contains(&(ModelId::from("Sphere"), "resin_total".to_owned())));
    }

    #[test]
    fn raw_waiver_validation_rejects_unknown_duplicate_and_expired_entries() {
        let signature = EvidenceSource::owner_decision(
            EvidenceId::new("test-owner-signature-2").unwrap(),
            "docs/rust-migration/06-conventions.md",
            "gate table",
            EvidenceDate::exact(CalendarDate::new("2026-08-01").unwrap()),
        )
        .unwrap();
        let known = BTreeSet::from([ModelId::from("Sphere")]);
        let unknown = RawWaiver {
            model: ModelId::from("Other"),
            target: "scar_area".to_owned(),
            signature: signature.clone(),
            expires_on: CalendarDate::new("2026-08-02").unwrap(),
        };
        assert!(matches!(
            validate_waivers(
                &[unknown],
                &known,
                &CalendarDate::new("2026-08-01").unwrap()
            ),
            Err(ComparisonError::WaiverUnknownModel { .. })
        ));
        let valid = RawWaiver {
            model: ModelId::from("Sphere"),
            target: "scar_area".to_owned(),
            signature: signature.clone(),
            expires_on: CalendarDate::new("2026-08-02").unwrap(),
        };
        assert!(matches!(
            validate_waivers(
                &[valid.clone(), valid],
                &known,
                &CalendarDate::new("2026-08-01").unwrap()
            ),
            Err(ComparisonError::DuplicateWaiver { .. })
        ));
        let expired = RawWaiver {
            model: ModelId::from("Sphere"),
            target: "scar_area".to_owned(),
            signature,
            expires_on: CalendarDate::new("2026-07-31").unwrap(),
        };
        assert!(matches!(
            validate_waivers(
                &[expired],
                &known,
                &CalendarDate::new("2026-08-01").unwrap()
            ),
            Err(ComparisonError::ExpiredWaiver { .. })
        ));
    }

    #[test]
    fn metric_row_oracle_rejects_count_duplicates_order_and_nonfinite_values() {
        let model = ModelId::from("Sphere");
        let ordered = M1_SCORE_ORDER_ORACLE.map(MetricId::from);
        let rows = ordered
            .iter()
            .cloned()
            .map(|id| (id, 0.0))
            .collect::<Vec<_>>();
        assert!(matches!(
            validate_metric_rows(&model, &rows[..14]),
            Err(ScoreError::MetricRowCount { .. })
        ));
        let mut duplicate = rows.clone();
        duplicate[1].0 = duplicate[0].0.clone();
        assert!(matches!(
            validate_metric_rows(&model, &duplicate),
            Err(ScoreError::DuplicateMetric { .. })
        ));
        let mut reordered = rows.clone();
        reordered.swap(0, 1);
        assert!(matches!(
            validate_metric_rows(&model, &reordered),
            Err(ScoreError::MetricOrder { .. })
        ));
        let mut nonfinite = rows;
        nonfinite[0].1 = f64::NAN;
        assert!(matches!(
            validate_metric_rows(&model, &nonfinite),
            Err(ScoreError::NonFiniteMetric { .. })
        ));
    }

    #[test]
    fn scoring_reopens_the_request_and_never_accepts_a_prior_snapshot() {
        let sequence = TEST_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("plinth-metrics-replace-{sequence}.chitubox"));
        let baseline = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("corpus/baseline/Sphere.chitubox");
        std::fs::copy(baseline, &path).unwrap();
        let requests = [ArtifactRequest::new(
            ModelId::from("Sphere"),
            ArtifactDisposition::Produced { path: path.clone() },
        )];
        let snapshot = load_artifacts(&requests).unwrap();
        assert!(snapshot[0].as_artifact().is_some());
        std::fs::write(&path, []).unwrap();
        assert!(matches!(
            score_saturn_artifacts(&requests, &[]),
            Err(ScoreError::DiskRead(DiskReadError::ArtifactFormat { .. }))
        ));
    }

    #[test]
    fn committed_sphere_is_scored_from_fresh_disk_with_all_fifteen_rows() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("corpus/baseline/Sphere.chitubox");
        let requests = [ArtifactRequest::new(
            ModelId::from("Sphere"),
            ArtifactDisposition::Produced { path },
        )];
        let loads = load_artifacts(&requests).unwrap();
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
            &requests,
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
