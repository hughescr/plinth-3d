//! Independent support and resin metrics over artifacts reopened from disk.
//!
//! The public boundary is deliberately whole-artifact and ordered: callers
//! gather every arm-load observation for every model, make one call, then
//! scatter the returned rows.  There is no public per-support hot query.
//!
//! Resin is measured from the triangle soups that were actually written into
//! the `.chitubox` artifact.  That is important for type-7/type-9 spheres and
//! type-6 pads, whose record metadata is not a frustum.  The `start`, `end`,
//! and radii are used only for the M1 per-arm X rule, where a contact taper's
//! start radius is the physical tip radius.

use std::f64::consts::PI;

use chitubox_io::SegmentType;
use plinth_core::{MetricId, MetricSet, MetricValue};
use thiserror::Error;

use crate::{signed_shell_volumes, DiskArtifact, Mesh, MetricsError, ShellWinding};

/// The fixed M1 support/resin metric order from `04-migration-plan.md` §3.
pub const SUPPORT_METRIC_ORDER: [&str; 5] = [
    "max_x",
    "support_count",
    "resin_model",
    "resin_support",
    "resin_total",
];

/// Exact SI-derived conversion: one millilitre is one thousand cubic millimetres.
const CUBIC_MILLIMETRES_PER_MILLILITRE: f64 = 1_000.0;

/// One independently supplied Voronoi load cell for a disk contact taper.
///
/// The pair is the stable M1 arm identity: `support_record_index` names an
/// on-disk type-1 taper record and `taper_index` is that record's zero-based
/// ordinal amongst all such records in the same disk model.  Both are checked
/// so stale or shifted planner output cannot silently score a different arm.
/// Its
/// `r_start` is the planner's tip radius in millimetres; keeping the radius in
/// the written artifact prevents a caller from accidentally measuring a
/// contact sphere (`type 7`) or substituting its diameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactArmInput {
    pub support_record_index: usize,
    pub taper_index: usize,
    pub voronoi_area_mm2: f64,
}

/// One ordered model input to [`support_metric_rows`].
#[derive(Clone, Debug, PartialEq)]
pub struct SupportMetricInput {
    pub source_index: usize,
    pub contact_arms: Vec<ContactArmInput>,
}

/// One M1 support/resin metric row in the normative fixed order.
#[derive(Clone, Debug, PartialEq)]
pub struct SupportMetricRow {
    pub source_index: usize,
    values: [MetricValue; SUPPORT_METRIC_ORDER.len()],
}

impl SupportMetricRow {
    /// Returns entries in M1's stable table order, not lexical map order.
    #[must_use]
    pub fn entries(&self) -> [(MetricId, MetricValue); SUPPORT_METRIC_ORDER.len()] {
        std::array::from_fn(|index| {
            (
                MetricId::from(SUPPORT_METRIC_ORDER[index]),
                self.values[index],
            )
        })
    }

    /// Converts this ordered transport row into core's passive deterministic DTO.
    #[must_use]
    pub fn metric_set(&self) -> MetricSet {
        MetricSet::new(self.entries())
    }
}

/// Semantic and numerical failures for the independent support metrics.
#[derive(Debug, Error)]
pub enum SupportMetricError {
    #[error(
        "support metric input count {actual} does not match disk artifact model count {expected}"
    )]
    InputCount { expected: usize, actual: usize },
    #[error("support metric input row {row} names source model {actual}, expected {expected}")]
    InputSourceIndex {
        row: usize,
        expected: usize,
        actual: usize,
    },
    #[error("model {source_index} contact arm count {actual} does not cover its {expected} disk contact tapers")]
    ContactArmCount {
        source_index: usize,
        expected: usize,
        actual: usize,
    },
    #[error(
        "model {source_index} arm {arm} repeats disk contact taper record {support_record_index}"
    )]
    DuplicateContactTaper {
        source_index: usize,
        arm: usize,
        support_record_index: usize,
    },
    #[error("model {source_index} arm {arm} names support record {support_record_index}, but there are only {support_count}")]
    ContactTaperOutOfBounds {
        source_index: usize,
        arm: usize,
        support_record_index: usize,
        support_count: usize,
    },
    #[error("model {source_index} arm {arm} names record {support_record_index}, which is segment type {segment_type}, not a type-1 contact taper")]
    ContactTaperWrongKind {
        source_index: usize,
        arm: usize,
        support_record_index: usize,
        segment_type: u32,
    },
    #[error("model {source_index} arm {arm} names taper ordinal {actual}, but record {support_record_index} has canonical ordinal {expected}")]
    ContactTaperOrdinalMismatch {
        source_index: usize,
        arm: usize,
        support_record_index: usize,
        expected: usize,
        actual: usize,
    },
    #[error("model {source_index} arm {arm} Voronoi area must be finite and nonnegative")]
    InvalidVoronoiArea { source_index: usize, arm: usize },
    #[error("model {source_index} arm {arm} has a nonpositive or nonfinite contact-tip radius")]
    InvalidTipRadius { source_index: usize, arm: usize },
    #[error("model {source_index} {subject:?} signed volume is negative ({volume_mm3} mm^3)")]
    NegativeSignedVolume {
        source_index: usize,
        subject: VolumeSubject,
        volume_mm3: f64,
    },
    #[error("model {source_index} support record {support_index} shell {shell_index} is open or non-manifold")]
    SupportShellNotClosed {
        source_index: usize,
        support_index: usize,
        shell_index: usize,
    },
    #[error("model {source_index} support record {support_index} shell {shell_index} has invalid winding {winding:?}")]
    SupportShellInvalidWinding {
        source_index: usize,
        support_index: usize,
        shell_index: usize,
        winding: ShellWinding,
    },
    #[error("model {source_index} support record {support_index} shell {shell_index} has nonpositive or nonfinite signed volume {volume_mm3} mm^3")]
    SupportShellNonPositiveVolume {
        source_index: usize,
        support_index: usize,
        shell_index: usize,
        volume_mm3: f64,
    },
    #[error("model {source_index} {subject:?} volume is non-finite")]
    NonFiniteVolume {
        source_index: usize,
        subject: VolumeSubject,
    },
    #[error("model {source_index} has too many support records to report exactly as f64")]
    InexactSupportCount { source_index: usize },
    #[error("model {source_index} arithmetic overflow while computing {metric}")]
    ArithmeticOverflow {
        source_index: usize,
        metric: &'static str,
    },
    #[error(transparent)]
    Geometry(#[from] MetricsError),
}

/// The disk-resident component whose orientation or arithmetic was invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeSubject {
    ModelMesh,
    SupportRecord { support_index: usize },
}

/// Computes M1 support/resin rows for every model in one disk-read artifact.
///
/// `max_x` is permanently the M1 *per-arm* maximum:
/// `A_voronoi_cell / (pi * r_tip^2)`.  It deliberately has no arm-group
/// interpretation or fallback. Every on-disk type-1 contact taper must appear
/// exactly once in `contact_arms`; an empty list is valid only if the artifact
/// has no contact tapers. Thus a missing load partition is a named failure,
/// never a plausible zero score.
///
/// `resin_model` sums the artifact model soup's signed shell volumes.  This
/// preserves a correctly oriented hollow inner shell as a subtraction, while
/// treating an overall negative result as an orientation defect instead of
/// hiding it with `abs`.  `resin_support` separately sums each record's actual
/// cached mesh volume, so spheres, pads, and bridges are neither omitted nor
/// double-counted as model geometry.  All resin values are mL.
pub fn support_metric_rows(
    artifact: &DiskArtifact,
    inputs: &[SupportMetricInput],
) -> Result<Vec<SupportMetricRow>, SupportMetricError> {
    if artifact.models().len() != inputs.len() {
        return Err(SupportMetricError::InputCount {
            expected: artifact.models().len(),
            actual: inputs.len(),
        });
    }

    artifact
        .models()
        .iter()
        .zip(inputs)
        .enumerate()
        .map(|(row, (model, input))| {
            let source_index = model.source_index();
            if input.source_index != source_index {
                return Err(SupportMetricError::InputSourceIndex {
                    row,
                    expected: source_index,
                    actual: input.source_index,
                });
            }

            let max_x = max_per_arm_x(source_index, model.supports(), &input.contact_arms)?;
            let model_mm3 =
                signed_mesh_volume(source_index, VolumeSubject::ModelMesh, model.mesh())?;
            let support_mm3 = model.supports().iter().enumerate().try_fold(
                0.0,
                |sum, (support_index, support)| {
                    let volume = validated_support_record_volume(
                        source_index,
                        support_index,
                        support.mesh(),
                    )?;
                    checked_add(source_index, "resin_support", sum, volume)
                },
            )?;
            let support_count = exact_count(source_index, model.supports().len())?;
            let model_ml = checked_divide(source_index, "resin_model", model_mm3)?;
            let support_ml = checked_divide(source_index, "resin_support", support_mm3)?;
            // Preserve the normative metric-level invariant exactly: callers
            // see this very IEEE addition, not a separately rounded mm^3 sum.
            let total_ml = checked_add(source_index, "resin_total", model_ml, support_ml)?;

            let values = [
                metric_value(source_index, "max_x", max_x)?,
                metric_value(source_index, "support_count", support_count)?,
                metric_value(source_index, "resin_model", model_ml)?,
                metric_value(source_index, "resin_support", support_ml)?,
                metric_value(source_index, "resin_total", total_ml)?,
            ];
            Ok(SupportMetricRow {
                source_index,
                values,
            })
        })
        .collect()
}

fn max_per_arm_x(
    source_index: usize,
    supports: &[crate::DiskSupport],
    arms: &[ContactArmInput],
) -> Result<f64, SupportMetricError> {
    let taper_records = supports
        .iter()
        .enumerate()
        .filter_map(|(record_index, support)| {
            (support.segment_type() == SegmentType::TAPER).then_some(record_index)
        })
        .collect::<Vec<_>>();
    if arms.len() != taper_records.len() {
        return Err(SupportMetricError::ContactArmCount {
            source_index,
            expected: taper_records.len(),
            actual: arms.len(),
        });
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut max_x: f64 = 0.0;
    for (arm, input) in arms.iter().enumerate() {
        if !seen.insert(input.support_record_index) {
            return Err(SupportMetricError::DuplicateContactTaper {
                source_index,
                arm,
                support_record_index: input.support_record_index,
            });
        }
        let support = supports.get(input.support_record_index).ok_or(
            SupportMetricError::ContactTaperOutOfBounds {
                source_index,
                arm,
                support_record_index: input.support_record_index,
                support_count: supports.len(),
            },
        )?;
        if support.segment_type() != SegmentType::TAPER {
            return Err(SupportMetricError::ContactTaperWrongKind {
                source_index,
                arm,
                support_record_index: input.support_record_index,
                segment_type: support.segment_type().0,
            });
        }
        let expected_taper_index = taper_records
            .iter()
            .position(|record_index| *record_index == input.support_record_index)
            .expect("type-1 record was gathered from this support slice");
        if input.taper_index != expected_taper_index {
            return Err(SupportMetricError::ContactTaperOrdinalMismatch {
                source_index,
                arm,
                support_record_index: input.support_record_index,
                expected: expected_taper_index,
                actual: input.taper_index,
            });
        }
        if !input.voronoi_area_mm2.is_finite() || input.voronoi_area_mm2 < 0.0 {
            return Err(SupportMetricError::InvalidVoronoiArea { source_index, arm });
        }
        let tip_radius_mm = support.radii().0;
        if !tip_radius_mm.is_finite() || tip_radius_mm <= 0.0 {
            return Err(SupportMetricError::InvalidTipRadius { source_index, arm });
        }
        let area = PI * tip_radius_mm * tip_radius_mm;
        let x = input.voronoi_area_mm2 / area;
        if !x.is_finite() {
            return Err(SupportMetricError::ArithmeticOverflow {
                source_index,
                metric: "max_x",
            });
        }
        max_x = max_x.max(x);
    }
    debug_assert_eq!(seen.len(), taper_records.len());
    Ok(max_x)
}

fn signed_mesh_volume(
    source_index: usize,
    subject: VolumeSubject,
    mesh: &crate::Mesh,
) -> Result<f64, SupportMetricError> {
    let canonical = canonical_shell_mesh(mesh)?;
    let volume = signed_shell_volumes(&canonical)?
        .iter()
        .try_fold(0.0, |sum, shell| {
            checked_add(source_index, "signed_volume", sum, shell.signed_volume)
        })?;
    if !volume.is_finite() {
        return Err(SupportMetricError::NonFiniteVolume {
            source_index,
            subject,
        });
    }
    if volume < 0.0 {
        return Err(SupportMetricError::NegativeSignedVolume {
            source_index,
            subject,
            volume_mm3: volume,
        });
    }
    Ok(volume)
}

/// Rebuilds a triangle soup with exact coordinate-sharing before topology.
///
/// The ChituBox wire is a triangle soup, so `read.rs` retains each triangle's
/// three vertices.  Shell topology must deliberately canonicalize that soup;
/// it is a metric-local structural view, not a producer-side repair.  Signed
/// zero is normalized only for geometric connectivity: `-0.0` and `0.0`
/// denote the same point, while all nonzero f64 bit patterns remain exact.
fn canonical_shell_mesh(mesh: &Mesh) -> Result<Mesh, MetricsError> {
    let mut vertices = Vec::new();
    let mut indices = std::collections::BTreeMap::<[u64; 3], usize>::new();
    let mut triangles = Vec::with_capacity(mesh.triangles().len());
    for triangle in mesh.triangles() {
        let mut canonical_triangle = [0; 3];
        for (corner, source_index) in triangle.iter().enumerate() {
            let point = mesh.vertices()[*source_index];
            let key = point.map(canonical_coordinate_bits);
            let index = match indices.get(&key) {
                Some(index) => *index,
                None => {
                    let index = vertices.len();
                    vertices.push(point);
                    indices.insert(key, index);
                    index
                }
            };
            canonical_triangle[corner] = index;
        }
        triangles.push(canonical_triangle);
    }
    Mesh::new(vertices, triangles)
}

fn canonical_coordinate_bits(value: f64) -> u64 {
    if value == 0.0 {
        0
    } else {
        value.to_bits()
    }
}

fn validated_support_record_volume(
    source_index: usize,
    support_index: usize,
    mesh: &Mesh,
) -> Result<f64, SupportMetricError> {
    let canonical = canonical_shell_mesh(mesh)?;
    signed_shell_volumes(&canonical)?
        .into_iter()
        .enumerate()
        .try_fold(0.0, |sum, (shell_index, shell)| {
            if !shell.sealed {
                return Err(SupportMetricError::SupportShellNotClosed {
                    source_index,
                    support_index,
                    shell_index,
                });
            }
            if !matches!(
                &shell.winding,
                ShellWinding::Outward {
                    ref faces_requiring_flip
                } if faces_requiring_flip.is_empty()
            ) {
                return Err(SupportMetricError::SupportShellInvalidWinding {
                    source_index,
                    support_index,
                    shell_index,
                    winding: shell.winding,
                });
            }
            if !shell.signed_volume.is_finite() || shell.signed_volume <= 0.0 {
                return Err(SupportMetricError::SupportShellNonPositiveVolume {
                    source_index,
                    support_index,
                    shell_index,
                    volume_mm3: shell.signed_volume,
                });
            }
            checked_add(source_index, "resin_support", sum, shell.signed_volume)
        })
}

fn checked_add(
    source_index: usize,
    metric: &'static str,
    left: f64,
    right: f64,
) -> Result<f64, SupportMetricError> {
    let value = left + right;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(SupportMetricError::ArithmeticOverflow {
            source_index,
            metric,
        })
    }
}

fn checked_divide(
    source_index: usize,
    metric: &'static str,
    value_mm3: f64,
) -> Result<f64, SupportMetricError> {
    let value = value_mm3 / CUBIC_MILLIMETRES_PER_MILLILITRE;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(SupportMetricError::ArithmeticOverflow {
            source_index,
            metric,
        })
    }
}

fn exact_count(source_index: usize, count: usize) -> Result<f64, SupportMetricError> {
    let value = count as f64;
    if value as usize == count {
        Ok(value)
    } else {
        Err(SupportMetricError::InexactSupportCount { source_index })
    }
}

fn metric_value(
    source_index: usize,
    metric: &'static str,
    value: f64,
) -> Result<MetricValue, SupportMetricError> {
    MetricValue::new(value).map_err(|_| SupportMetricError::ArithmeticOverflow {
        source_index,
        metric,
    })
}
