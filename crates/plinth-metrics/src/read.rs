//! Disk-first `.chitubox` input for the independent metrics harness.
//!
//! The public boundary deliberately accepts a whole ordered corpus slice and
//! returns one outcome per input row.  A producer failure is evidence to score,
//! not a missing-file error.  Successful rows are reopened from their path,
//! parsed through `chitubox_io`, structurally round-tripped in memory, and
//! copied into this crate's owned f64 audit representation.  No producer-owned
//! object reaches the scorer and this module never writes or repairs a file.

use std::collections::BTreeMap;
use std::num::NonZeroU8;
use std::path::{Path, PathBuf};

use chitubox_io::{ChituboxError, ModelInfo, Project, SegmentType, SupportRecord};
use plinth_core::ModelId;
use thiserror::Error;

use crate::{Mesh, MetricsError, Vec3};

/// A stage named by the frozen Python report when its producer exited nonzero.
///
/// The baseline contract currently exposes only these three stages.  Keeping
/// this closed avoids treating an arbitrary report string as a stage protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProducerStage {
    Hollow,
    Supports,
    Emit,
}

/// A nonzero producer exit retained as a first-class corpus outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProducerFailure {
    exit_code: NonZeroU8,
    failed_stage: Option<ProducerStage>,
}

impl ProducerFailure {
    #[must_use]
    pub const fn exit_code(&self) -> NonZeroU8 {
        self.exit_code
    }

    #[must_use]
    pub const fn failed_stage(&self) -> Option<ProducerStage> {
        self.failed_stage
    }
}

/// The mutually exclusive artifact state described by one manifest row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArtifactDisposition {
    /// A zero-exit producer emitted this exact file path, which must be opened.
    Produced { path: PathBuf },
    /// A nonzero producer emitted no artifact and remains an explicit outcome.
    ProducerFailed(ProducerFailure),
}

impl ArtifactDisposition {
    /// Normalizes the three manifest fields which jointly describe output state.
    ///
    /// This is intentionally the only public way to build a disposition from
    /// loose exit/path fields, so a failed row cannot silently masquerade as a
    /// missing successful artifact.
    pub fn from_manifest_fields(
        exit_code: u8,
        failed_stage: Option<ProducerStage>,
        artifact_path: Option<PathBuf>,
    ) -> Result<Self, ArtifactRequestError> {
        match (NonZeroU8::new(exit_code), failed_stage, artifact_path) {
            (None, None, Some(path)) => Ok(Self::Produced { path }),
            (None, None, None) => Err(ArtifactRequestError::SuccessfulArtifactMissingPath),
            (None, Some(stage), _) => {
                Err(ArtifactRequestError::SuccessfulArtifactHasFailureStage { stage })
            }
            (Some(exit_code), failed_stage, None) => Ok(Self::ProducerFailed(ProducerFailure {
                exit_code,
                failed_stage,
            })),
            (Some(exit_code), _, Some(path)) => {
                Err(ArtifactRequestError::FailedProducerHasArtifact { exit_code, path })
            }
        }
    }
}

/// One ordered corpus row to reopen.  The loader preserves this order exactly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactRequest {
    model: ModelId,
    disposition: ArtifactDisposition,
}

impl ArtifactRequest {
    #[must_use]
    pub fn new(model: ModelId, disposition: ArtifactDisposition) -> Self {
        Self { model, disposition }
    }

    #[must_use]
    pub const fn model(&self) -> &ModelId {
        &self.model
    }

    #[must_use]
    pub const fn disposition(&self) -> &ArtifactDisposition {
        &self.disposition
    }
}

/// Structural contradictions in the manifest fields before the filesystem is read.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ArtifactRequestError {
    #[error("a zero-exit producer requires an artifact path")]
    SuccessfulArtifactMissingPath,
    #[error("a zero-exit producer cannot name failed stage {stage:?}")]
    SuccessfulArtifactHasFailureStage { stage: ProducerStage },
    #[error("failed producer exit {exit_code} unexpectedly names artifact {path}")]
    FailedProducerHasArtifact { exit_code: NonZeroU8, path: PathBuf },
}

/// A loaded input row, always in the same position as its request.
#[derive(Clone, Debug, PartialEq)]
pub enum ArtifactLoad {
    Artifact(DiskArtifact),
    ProducerFailed {
        model: ModelId,
        failure: ProducerFailure,
    },
}

impl ArtifactLoad {
    #[must_use]
    pub const fn model(&self) -> &ModelId {
        match self {
            Self::Artifact(artifact) => artifact.model(),
            Self::ProducerFailed { model, .. } => model,
        }
    }

    #[must_use]
    pub const fn as_artifact(&self) -> Option<&DiskArtifact> {
        match self {
            Self::Artifact(artifact) => Some(artifact),
            Self::ProducerFailed { .. } => None,
        }
    }
}

/// An immutable owned snapshot extracted from one successful on-disk artifact.
#[derive(Clone, Debug, PartialEq)]
pub struct DiskArtifact {
    model: ModelId,
    path: PathBuf,
    models: Vec<DiskModel>,
}

impl DiskArtifact {
    #[must_use]
    pub const fn model(&self) -> &ModelId {
        &self.model
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn models(&self) -> &[DiskModel] {
        &self.models
    }
}

/// One artifact model, including its local f64 triangle soup and support records.
#[derive(Clone, Debug, PartialEq)]
pub struct DiskModel {
    source_index: usize,
    name: String,
    placement: Vec3,
    mesh: Mesh,
    supports: Vec<DiskSupport>,
}

impl DiskModel {
    #[must_use]
    pub const fn source_index(&self) -> usize {
        self.source_index
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn placement(&self) -> Vec3 {
        self.placement
    }

    #[must_use]
    pub const fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub fn supports(&self) -> &[DiskSupport] {
        &self.supports
    }
}

/// The owned f64 portion of one support record needed by later metrics.
#[derive(Clone, Debug, PartialEq)]
pub struct DiskSupport {
    segment_type: SegmentType,
    start: Vec3,
    end: Vec3,
    r_start: f64,
    r_end: f64,
    mesh: Mesh,
    trailing: [u8; 24],
}

impl DiskSupport {
    #[must_use]
    pub const fn segment_type(&self) -> SegmentType {
        self.segment_type
    }

    #[must_use]
    pub const fn start(&self) -> Vec3 {
        self.start
    }

    #[must_use]
    pub const fn end(&self) -> Vec3 {
        self.end
    }

    #[must_use]
    pub const fn radii(&self) -> (f64, f64) {
        (self.r_start, self.r_end)
    }

    #[must_use]
    pub const fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub const fn trailing(&self) -> &[u8; 24] {
        &self.trailing
    }
}

/// Semantically distinct failures while reading or extracting an artifact.
#[derive(Debug, Error)]
pub enum DiskReadError {
    #[error("duplicate corpus model {model} at rows {first_row} and {duplicate_row}")]
    DuplicateCorpusModel {
        model: ModelId,
        first_row: usize,
        duplicate_row: usize,
    },
    #[error("successful artifact for model {model} is missing at {path}")]
    MissingArtifact { model: ModelId, path: PathBuf },
    #[error("cannot read artifact for model {model} at {path}: {source}")]
    ArtifactIo {
        model: ModelId,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("artifact for model {model} at {path} is not a valid ChituBox container: {source}")]
    ArtifactFormat {
        model: ModelId,
        path: PathBuf,
        #[source]
        source: ChituboxError,
    },
    #[error(
        "artifact for model {model} at {path} failed its structural round-trip audit: {source}"
    )]
    RoundTripAudit {
        model: ModelId,
        path: PathBuf,
        #[source]
        source: ChituboxError,
    },
    #[error("artifact for model {model} at {path} changed bytes during a clean reader round trip")]
    RoundTripBytesChanged { model: ModelId, path: PathBuf },
    #[error(
        "artifact for model {model} at {path} reported model index {reported_index} at slot {slot}"
    )]
    ModelIndexMismatch {
        model: ModelId,
        path: PathBuf,
        slot: usize,
        reported_index: usize,
    },
    #[error("artifact for model {model} at {path} model {source_model} reports {reported} triangles but exposes {actual} triangles")]
    ModelTriangleCountMismatch {
        model: ModelId,
        path: PathBuf,
        source_model: usize,
        reported: usize,
        actual: usize,
    },
    #[error("artifact for model {model} at {path} has malformed {subject:?} triangle soup of {float_count} f32 values for source model {source_model}")]
    TriangleSoupShape {
        model: ModelId,
        path: PathBuf,
        source_model: usize,
        subject: MeshSubject,
        float_count: usize,
    },
    #[error("artifact for model {model} at {path} model {source_model} reports supports={has_supports} and {reported} records but exposes {actual} records")]
    SupportCountMismatch {
        model: ModelId,
        path: PathBuf,
        source_model: usize,
        has_supports: bool,
        reported: usize,
        actual: usize,
    },
    #[error("artifact for model {model} at {path} has invalid {subject:?} mesh for source model {source_model}: {source}")]
    InvalidMesh {
        model: ModelId,
        path: PathBuf,
        source_model: usize,
        subject: MeshSubject,
        #[source]
        source: MetricsError,
    },
    #[error("artifact for model {model} at {path} has non-finite {coordinate:?} on support {support} for source model {source_model}")]
    NonFiniteSupportCoordinate {
        model: ModelId,
        path: PathBuf,
        source_model: usize,
        support: usize,
        coordinate: SupportCoordinate,
    },
    #[error("artifact for model {model} at {path} has non-finite placement coordinate {component} for source model {source_model}")]
    NonFinitePlacementCoordinate {
        model: ModelId,
        path: PathBuf,
        source_model: usize,
        component: usize,
    },
    #[error("artifact for model {model} at {path} has invalid radius {radius:?} on support {support} for source model {source_model}")]
    InvalidSupportRadius {
        model: ModelId,
        path: PathBuf,
        source_model: usize,
        support: usize,
        radius: SupportRadius,
    },
}

/// Which immutable mesh failed extraction validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeshSubject {
    Model,
    Support { support: usize },
}

/// Which non-mesh support coordinate was non-finite.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupportCoordinate {
    Start,
    End,
}

/// Which support radius was non-finite or negative.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupportRadius {
    Start,
    End,
}

/// Reopens every successful artifact in one ordered corpus pass.
///
/// There is deliberately no single-artifact public convenience API: callers
/// gather a manifest slice, make this one batched call, then scatter the ordered
/// results into their scoring work.
pub fn load_artifacts(requests: &[ArtifactRequest]) -> Result<Vec<ArtifactLoad>, DiskReadError> {
    let mut rows = BTreeMap::new();
    for (row, request) in requests.iter().enumerate() {
        if let Some(first_row) = rows.insert(request.model.clone(), row) {
            return Err(DiskReadError::DuplicateCorpusModel {
                model: request.model.clone(),
                first_row,
                duplicate_row: row,
            });
        }
    }

    requests.iter().map(load_one).collect()
}

fn load_one(request: &ArtifactRequest) -> Result<ArtifactLoad, DiskReadError> {
    match request.disposition() {
        ArtifactDisposition::ProducerFailed(failure) => Ok(ArtifactLoad::ProducerFailed {
            model: request.model.clone(),
            failure: failure.clone(),
        }),
        ArtifactDisposition::Produced { path } => load_successful(request.model(), path),
    }
}

fn load_successful(model: &ModelId, path: &Path) -> Result<ArtifactLoad, DiskReadError> {
    let bytes = std::fs::read(path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            DiskReadError::MissingArtifact {
                model: model.clone(),
                path: path.to_owned(),
            }
        } else {
            DiskReadError::ArtifactIo {
                model: model.clone(),
                path: path.to_owned(),
                source,
            }
        }
    })?;
    let project =
        Project::from_bytes(bytes.clone()).map_err(|source| DiskReadError::ArtifactFormat {
            model: model.clone(),
            path: path.to_owned(),
            source,
        })?;
    let round_trip = project
        .to_bytes()
        .map_err(|source| DiskReadError::RoundTripAudit {
            model: model.clone(),
            path: path.to_owned(),
            source,
        })?;
    if round_trip != bytes {
        return Err(DiskReadError::RoundTripBytesChanged {
            model: model.clone(),
            path: path.to_owned(),
        });
    }

    let infos = project.models();
    let mut models = Vec::with_capacity(infos.len());
    for (slot, info) in infos.iter().enumerate() {
        models.push(extract_model(model, path, &project, slot, info)?);
    }
    Ok(ArtifactLoad::Artifact(DiskArtifact {
        model: model.clone(),
        path: path.to_owned(),
        models,
    }))
}

fn extract_model(
    corpus_model: &ModelId,
    path: &Path,
    project: &Project,
    slot: usize,
    info: &ModelInfo,
) -> Result<DiskModel, DiskReadError> {
    if info.index != slot {
        return Err(DiskReadError::ModelIndexMismatch {
            model: corpus_model.clone(),
            path: path.to_owned(),
            slot,
            reported_index: info.index,
        });
    }
    let xyz = project
        .model_triangles(slot)
        .map_err(|source| DiskReadError::ArtifactFormat {
            model: corpus_model.clone(),
            path: path.to_owned(),
            source,
        })?;
    let actual_triangles = xyz.len() / 9;
    if xyz.is_empty() || !xyz.len().is_multiple_of(9) {
        return Err(DiskReadError::TriangleSoupShape {
            model: corpus_model.clone(),
            path: path.to_owned(),
            source_model: slot,
            subject: MeshSubject::Model,
            float_count: xyz.len(),
        });
    }
    if info.triangle_count != actual_triangles {
        return Err(DiskReadError::ModelTriangleCountMismatch {
            model: corpus_model.clone(),
            path: path.to_owned(),
            source_model: slot,
            reported: info.triangle_count,
            actual: actual_triangles,
        });
    }
    let mesh = mesh_from_triangle_soup(&xyz).map_err(|source| DiskReadError::InvalidMesh {
        model: corpus_model.clone(),
        path: path.to_owned(),
        source_model: slot,
        subject: MeshSubject::Model,
        source,
    })?;
    let records = project
        .supports(slot)
        .map_err(|source| DiskReadError::ArtifactFormat {
            model: corpus_model.clone(),
            path: path.to_owned(),
            source,
        })?;
    if info.has_supports != !records.is_empty() || info.support_record_count != records.len() {
        return Err(DiskReadError::SupportCountMismatch {
            model: corpus_model.clone(),
            path: path.to_owned(),
            source_model: slot,
            has_supports: info.has_supports,
            reported: info.support_record_count,
            actual: records.len(),
        });
    }
    let supports = records
        .iter()
        .enumerate()
        .map(|(support, record)| extract_support(corpus_model, path, slot, support, record))
        .collect::<Result<Vec<_>, _>>()?;
    let placement = [
        f64::from(info.placement.x),
        f64::from(info.placement.y),
        f64::from(info.placement.z_lift),
    ];
    if let Some(component) = placement
        .iter()
        .position(|coordinate| !coordinate.is_finite())
    {
        return Err(DiskReadError::NonFinitePlacementCoordinate {
            model: corpus_model.clone(),
            path: path.to_owned(),
            source_model: slot,
            component,
        });
    }
    Ok(DiskModel {
        source_index: slot,
        name: info.name.clone(),
        placement,
        mesh,
        supports,
    })
}

fn extract_support(
    corpus_model: &ModelId,
    path: &Path,
    source_model: usize,
    support: usize,
    record: &SupportRecord,
) -> Result<DiskSupport, DiskReadError> {
    let start = f64_vec(record.start);
    let end = f64_vec(record.end);
    validate_support_vec(
        corpus_model,
        path,
        source_model,
        support,
        start,
        SupportCoordinate::Start,
    )?;
    validate_support_vec(
        corpus_model,
        path,
        source_model,
        support,
        end,
        SupportCoordinate::End,
    )?;
    let r_start = f64::from(record.r_start);
    let r_end = f64::from(record.r_end);
    validate_radius(
        corpus_model,
        path,
        source_model,
        support,
        r_start,
        SupportRadius::Start,
    )?;
    validate_radius(
        corpus_model,
        path,
        source_model,
        support,
        r_end,
        SupportRadius::End,
    )?;
    if record.mesh.xyz.is_empty() || !record.mesh.xyz.len().is_multiple_of(9) {
        return Err(DiskReadError::TriangleSoupShape {
            model: corpus_model.clone(),
            path: path.to_owned(),
            source_model,
            subject: MeshSubject::Support { support },
            float_count: record.mesh.xyz.len(),
        });
    }
    let mesh =
        mesh_from_triangle_soup(&record.mesh.xyz).map_err(|source| DiskReadError::InvalidMesh {
            model: corpus_model.clone(),
            path: path.to_owned(),
            source_model,
            subject: MeshSubject::Support { support },
            source,
        })?;
    Ok(DiskSupport {
        segment_type: record.segment_type,
        start,
        end,
        r_start,
        r_end,
        mesh,
        trailing: record.trailing,
    })
}

fn validate_support_vec(
    model: &ModelId,
    path: &Path,
    source_model: usize,
    support: usize,
    value: Vec3,
    coordinate: SupportCoordinate,
) -> Result<(), DiskReadError> {
    if value.iter().all(|component| component.is_finite()) {
        return Ok(());
    }
    Err(DiskReadError::NonFiniteSupportCoordinate {
        model: model.clone(),
        path: path.to_owned(),
        source_model,
        support,
        coordinate,
    })
}

fn validate_radius(
    model: &ModelId,
    path: &Path,
    source_model: usize,
    support: usize,
    value: f64,
    radius: SupportRadius,
) -> Result<(), DiskReadError> {
    if value.is_finite() && value >= 0.0 {
        return Ok(());
    }
    Err(DiskReadError::InvalidSupportRadius {
        model: model.clone(),
        path: path.to_owned(),
        source_model,
        support,
        radius,
    })
}

fn mesh_from_triangle_soup(xyz: &[f32]) -> Result<Mesh, MetricsError> {
    let vertices = xyz
        .chunks_exact(3)
        .map(|point| f64_vec([point[0], point[1], point[2]]))
        .collect::<Vec<_>>();
    let triangles = (0..vertices.len() / 3)
        .map(|triangle| {
            let first = triangle * 3;
            [first, first + 1, first + 2]
        })
        .collect();
    Mesh::new(vertices, triangles)
}

fn f64_vec(value: [f32; 3]) -> Vec3 {
    [
        f64::from(value[0]),
        f64::from(value[1]),
        f64::from(value[2]),
    ]
}
