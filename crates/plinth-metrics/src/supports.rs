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

use crate::{signed_shell_volumes, DiskArtifact, MetricsError};

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

/// One independently supplied Voronoi load cell for a contact taper.
///
/// `support_index` names a type-1 taper record in the disk-read model.  Its
/// `r_start` is the planner's tip radius in millimetres; keeping the radius in
/// the written artifact prevents a caller from accidentally measuring a
/// contact sphere (`type 7`) or substituting its diameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArmLoadInput {
    pub support_index: usize,
    pub voronoi_area_mm2: f64,
}

/// One ordered model input to [`support_metric_rows`].
#[derive(Clone, Debug, PartialEq)]
pub struct SupportMetricInput {
    pub source_index: usize,
    pub arms: Vec<ArmLoadInput>,
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
    #[error("model {source_index} arm {arm} repeats support record {support_index}")]
    DuplicateArmSupport {
        source_index: usize,
        arm: usize,
        support_index: usize,
    },
    #[error("model {source_index} arm {arm} names support record {support_index}, but there are only {support_count}")]
    ArmSupportOutOfBounds {
        source_index: usize,
        arm: usize,
        support_index: usize,
        support_count: usize,
    },
    #[error("model {source_index} arm {arm} must name a type-1 contact taper, not segment type {segment_type}")]
    ArmSupportNotContactTaper {
        source_index: usize,
        arm: usize,
        segment_type: u32,
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
/// interpretation or fallback.  An empty arm batch has no loaded arm, so its
/// maximum is zero; any nonempty batch is fully validated and cannot silently
/// degrade to zero.
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

            let max_x = max_per_arm_x(source_index, model.supports(), &input.arms)?;
            let model_mm3 =
                signed_mesh_volume(source_index, VolumeSubject::ModelMesh, model.mesh())?;
            let support_mm3 = model.supports().iter().enumerate().try_fold(
                0.0,
                |sum, (support_index, support)| {
                    let volume = signed_mesh_volume(
                        source_index,
                        VolumeSubject::SupportRecord { support_index },
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
    arms: &[ArmLoadInput],
) -> Result<f64, SupportMetricError> {
    let mut seen = std::collections::BTreeSet::new();
    let mut max_x: f64 = 0.0;
    for (arm, input) in arms.iter().enumerate() {
        if !seen.insert(input.support_index) {
            return Err(SupportMetricError::DuplicateArmSupport {
                source_index,
                arm,
                support_index: input.support_index,
            });
        }
        let support =
            supports
                .get(input.support_index)
                .ok_or(SupportMetricError::ArmSupportOutOfBounds {
                    source_index,
                    arm,
                    support_index: input.support_index,
                    support_count: supports.len(),
                })?;
        if support.segment_type() != SegmentType::TAPER {
            return Err(SupportMetricError::ArmSupportNotContactTaper {
                source_index,
                arm,
                segment_type: support.segment_type().0,
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
    Ok(max_x)
}

fn signed_mesh_volume(
    source_index: usize,
    subject: VolumeSubject,
    mesh: &crate::Mesh,
) -> Result<f64, SupportMetricError> {
    let volume = signed_shell_volumes(mesh)?
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
