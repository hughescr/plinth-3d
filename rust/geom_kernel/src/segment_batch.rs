//! Batched point-to-segment-axis distances.
//!
//! This is the single kernel primitive for callers that gather a bounded
//! window of points and evaluate it against one shared segment-axis batch.
//! Parallelism is only across query rows; each row is scattered in original
//! segment order, so thread-count changes cannot alter floating-point order.

use rayon::prelude::*;

use crate::par;

/// Failure before a point/segment distance batch is evaluated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PointSegmentBatchError {
    SegmentEndpointCountMismatch {
        starts: usize,
        ends: usize,
    },
    NonFinitePoint {
        collection: &'static str,
        item: usize,
        axis: usize,
    },
    TaperRadiusCountMismatch {
        starts: usize,
        ends: usize,
        radii_at_start: usize,
        radii_at_end: usize,
    },
    NonFiniteRadius {
        collection: &'static str,
        item: usize,
    },
    OutputSizeOverflow {
        points: usize,
        segments: usize,
    },
    OutputPairLimitExceeded {
        required: usize,
        maximum: usize,
    },
    OutputByteLimitExceeded {
        required: usize,
        maximum: usize,
    },
    NonFiniteOutput {
        point: usize,
        segment: usize,
    },
}

impl std::fmt::Display for PointSegmentBatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SegmentEndpointCountMismatch { starts, ends } => write!(
                formatter,
                "segment start/end counts differ: {starts} starts, {ends} ends"
            ),
            Self::NonFinitePoint {
                collection,
                item,
                axis,
            } => write!(
                formatter,
                "{collection} item {item} axis {axis} is non-finite"
            ),
            Self::TaperRadiusCountMismatch {
                starts,
                ends,
                radii_at_start,
                radii_at_end,
            } => write!(
                formatter,
                "taper batch lengths differ: {starts} starts, {ends} ends, {radii_at_start} start radii, {radii_at_end} end radii"
            ),
            Self::NonFiniteRadius { collection, item } => {
                write!(formatter, "{collection} item {item} is non-finite")
            }
            Self::OutputSizeOverflow { points, segments } => write!(
                formatter,
                "point/segment output size overflows: {points} by {segments}"
            ),
            Self::OutputPairLimitExceeded { required, maximum } => write!(
                formatter,
                "point/segment output needs {required} pairs, limit is {maximum}"
            ),
            Self::OutputByteLimitExceeded { required, maximum } => write!(
                formatter,
                "point/segment output needs {required} bytes, limit is {maximum}"
            ),
            Self::NonFiniteOutput { point, segment } => write!(
                formatter,
                "point/segment distance is non-finite at point {point}, segment {segment}"
            ),
        }
    }
}

impl std::error::Error for PointSegmentBatchError {}

/// Caller-owned hard limits checked before the single row-major output
/// allocation. There is intentionally no unbounded/default request form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointSegmentBatchLimits {
    max_output_pairs: usize,
    max_output_bytes: usize,
}

impl PointSegmentBatchLimits {
    #[must_use]
    pub const fn new(max_output_pairs: usize, max_output_bytes: usize) -> Self {
        Self {
            max_output_pairs,
            max_output_bytes,
        }
    }

    /// Exact f64 output budget for a caller-owned rectangular window.
    pub fn for_dimensions(points: usize, segments: usize) -> Result<Self, PointSegmentBatchError> {
        let pairs = points
            .checked_mul(segments)
            .ok_or(PointSegmentBatchError::OutputSizeOverflow { points, segments })?;
        let bytes = pairs
            .checked_mul(std::mem::size_of::<f64>())
            .ok_or(PointSegmentBatchError::OutputSizeOverflow { points, segments })?;
        Ok(Self::new(pairs, bytes))
    }
}

/// Row-major point-to-segment distances plus the actual configured pool size
/// under which the batch ran. This reports pool configuration, not a flaky
/// claim that every worker happened to receive work.
#[derive(Clone, Debug, PartialEq)]
pub struct PointSegmentDistanceBatch {
    point_count: usize,
    segment_count: usize,
    distances_mm: Vec<f64>,
    effective_threads: usize,
}

/// Sealed request for the frozen joint arithmetic.  Private fields prevent a
/// caller from selecting an association order independently of this named
/// compatibility contract.
#[derive(Clone, Copy, Debug)]
pub struct FrozenJointTaperBatch<'a> {
    radii_at_start_mm: &'a [f64],
    radii_at_end_mm: &'a [f64],
}

impl<'a> FrozenJointTaperBatch<'a> {
    #[must_use]
    pub const fn new(radii_at_start_mm: &'a [f64], radii_at_end_mm: &'a [f64]) -> Self {
        Self {
            radii_at_start_mm,
            radii_at_end_mm,
        }
    }
}

/// Joint-mode output: the one shared distance batch plus the taper radius at
/// the exact same clamped projection parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct PointSegmentTaperBatch {
    distances: PointSegmentDistanceBatch,
    local_radii_mm: Vec<f64>,
}

impl PointSegmentTaperBatch {
    #[must_use]
    pub const fn point_count(&self) -> usize {
        self.distances.point_count
    }

    #[must_use]
    pub const fn segment_count(&self) -> usize {
        self.distances.segment_count
    }

    #[must_use]
    pub const fn effective_threads(&self) -> usize {
        self.distances.effective_threads
    }

    #[must_use]
    pub fn distance_row(&self, point: usize) -> Option<&[f64]> {
        self.distances.row(point)
    }

    #[must_use]
    pub fn local_radius_row(&self, point: usize) -> Option<&[f64]> {
        (point < self.point_count()).then(|| {
            let start = point * self.segment_count();
            &self.local_radii_mm[start..start + self.segment_count()]
        })
    }
}

impl PointSegmentDistanceBatch {
    #[must_use]
    pub const fn point_count(&self) -> usize {
        self.point_count
    }

    #[must_use]
    pub const fn segment_count(&self) -> usize {
        self.segment_count
    }

    #[must_use]
    pub const fn effective_threads(&self) -> usize {
        self.effective_threads
    }

    #[must_use]
    pub fn row(&self, point: usize) -> Option<&[f64]> {
        (point < self.point_count).then(|| {
            let start = point * self.segment_count;
            &self.distances_mm[start..start + self.segment_count]
        })
    }
}

/// Evaluate a complete Cartesian batch while retaining fixed per-pair operand
/// order. Callers bound memory by supplying a bounded point window.
pub fn point_segment_distances_batch(
    points: &[[f64; 3]],
    segment_starts: &[[f64; 3]],
    segment_ends: &[[f64; 3]],
    limits: PointSegmentBatchLimits,
) -> Result<PointSegmentDistanceBatch, PointSegmentBatchError> {
    let evaluated = evaluate_batch(points, segment_starts, segment_ends, None, limits)?;
    Ok(PointSegmentDistanceBatch {
        point_count: evaluated.point_count,
        segment_count: evaluated.segment_count,
        distances_mm: evaluated.distances_mm,
        effective_threads: evaluated.effective_threads,
    })
}

/// Evaluate the frozen joint distance association and local taper radius in
/// one shared, bounded Cartesian batch.
pub fn point_segment_taper_batch(
    points: &[[f64; 3]],
    segment_starts: &[[f64; 3]],
    segment_ends: &[[f64; 3]],
    taper: FrozenJointTaperBatch<'_>,
    limits: PointSegmentBatchLimits,
) -> Result<PointSegmentTaperBatch, PointSegmentBatchError> {
    let evaluated = evaluate_batch(points, segment_starts, segment_ends, Some(taper), limits)?;
    Ok(PointSegmentTaperBatch {
        distances: PointSegmentDistanceBatch {
            point_count: evaluated.point_count,
            segment_count: evaluated.segment_count,
            distances_mm: evaluated.distances_mm,
            effective_threads: evaluated.effective_threads,
        },
        local_radii_mm: evaluated
            .local_radii_mm
            .expect("joint taper mode always allocates local radii"),
    })
}

struct EvaluatedBatch {
    point_count: usize,
    segment_count: usize,
    distances_mm: Vec<f64>,
    local_radii_mm: Option<Vec<f64>>,
    effective_threads: usize,
}

fn evaluate_batch(
    points: &[[f64; 3]],
    segment_starts: &[[f64; 3]],
    segment_ends: &[[f64; 3]],
    taper: Option<FrozenJointTaperBatch<'_>>,
    limits: PointSegmentBatchLimits,
) -> Result<EvaluatedBatch, PointSegmentBatchError> {
    if segment_starts.len() != segment_ends.len() {
        return Err(PointSegmentBatchError::SegmentEndpointCountMismatch {
            starts: segment_starts.len(),
            ends: segment_ends.len(),
        });
    }
    validate("points", points)?;
    validate("segment_starts", segment_starts)?;
    validate("segment_ends", segment_ends)?;
    if let Some(taper) = taper {
        if taper.radii_at_start_mm.len() != segment_starts.len()
            || taper.radii_at_end_mm.len() != segment_starts.len()
        {
            return Err(PointSegmentBatchError::TaperRadiusCountMismatch {
                starts: segment_starts.len(),
                ends: segment_ends.len(),
                radii_at_start: taper.radii_at_start_mm.len(),
                radii_at_end: taper.radii_at_end_mm.len(),
            });
        }
        validate_radii("radii_at_start_mm", taper.radii_at_start_mm)?;
        validate_radii("radii_at_end_mm", taper.radii_at_end_mm)?;
    }
    let point_count = points.len();
    let segment_count = segment_starts.len();
    let output_pairs = point_count.checked_mul(segment_count).ok_or(
        PointSegmentBatchError::OutputSizeOverflow {
            points: point_count,
            segments: segment_count,
        },
    )?;
    if output_pairs > limits.max_output_pairs {
        return Err(PointSegmentBatchError::OutputPairLimitExceeded {
            required: output_pairs,
            maximum: limits.max_output_pairs,
        });
    }
    let output_channels = if taper.is_some() { 2 } else { 1 };
    let output_bytes = output_pairs
        .checked_mul(std::mem::size_of::<f64>())
        .and_then(|bytes| bytes.checked_mul(output_channels))
        .ok_or(PointSegmentBatchError::OutputSizeOverflow {
            points: point_count,
            segments: segment_count,
        })?;
    if output_bytes > limits.max_output_bytes {
        return Err(PointSegmentBatchError::OutputByteLimitExceeded {
            required: output_bytes,
            maximum: limits.max_output_bytes,
        });
    }

    let mut distances_mm = vec![0.0; output_pairs];
    let mut local_radii_mm = taper.map(|_| vec![0.0; output_pairs]);
    let effective_threads = par::run(|| {
        let effective_threads = rayon::current_num_threads();
        if segment_count != 0 {
            match (taper, local_radii_mm.as_mut()) {
                (Some(taper), Some(local_radii)) => distances_mm
                    .par_chunks_mut(segment_count)
                    .zip(local_radii.par_chunks_mut(segment_count))
                    .zip(points.par_iter())
                    .for_each(|((distance_row, radius_row), point)| {
                        for (segment, (distance, radius)) in
                            distance_row.iter_mut().zip(radius_row).enumerate()
                        {
                            let evaluation = point_segment_evaluation(
                                *point,
                                segment_starts[segment],
                                segment_ends[segment],
                                Association::FrozenJoint,
                            );
                            *distance = evaluation.distance;
                            *radius = if evaluation.degenerate {
                                taper.radii_at_start_mm[segment]
                            } else {
                                taper.radii_at_start_mm[segment]
                                    + (taper.radii_at_end_mm[segment]
                                        - taper.radii_at_start_mm[segment])
                                        * evaluation.t
                            };
                        }
                    }),
                (None, None) => distances_mm
                    .par_chunks_mut(segment_count)
                    .zip(points.par_iter())
                    .for_each(|(row, point)| {
                        for (output, (start, end)) in
                            row.iter_mut().zip(segment_starts.iter().zip(segment_ends))
                        {
                            *output = point_segment_evaluation(
                                *point,
                                *start,
                                *end,
                                Association::AcceptedDistance,
                            )
                            .distance;
                        }
                    }),
                _ => unreachable!("mode and output allocation are inseparable"),
            }
        }
        effective_threads
    });
    if let Some((flat_index, _)) = distances_mm
        .iter()
        .enumerate()
        .find(|(_, distance)| !distance.is_finite())
    {
        return Err(PointSegmentBatchError::NonFiniteOutput {
            point: flat_index / segment_count,
            segment: flat_index % segment_count,
        });
    }
    if let Some((flat_index, _)) = local_radii_mm.as_ref().and_then(|values| {
        values
            .iter()
            .enumerate()
            .find(|(_, radius)| !radius.is_finite())
    }) {
        return Err(PointSegmentBatchError::NonFiniteOutput {
            point: flat_index / segment_count,
            segment: flat_index % segment_count,
        });
    }
    Ok(EvaluatedBatch {
        point_count,
        segment_count,
        distances_mm,
        local_radii_mm,
        effective_threads,
    })
}

fn validate_radii(collection: &'static str, values: &[f64]) -> Result<(), PointSegmentBatchError> {
    for (item, value) in values.iter().enumerate() {
        if !value.is_finite() {
            return Err(PointSegmentBatchError::NonFiniteRadius { collection, item });
        }
    }
    Ok(())
}

fn validate(collection: &'static str, points: &[[f64; 3]]) -> Result<(), PointSegmentBatchError> {
    for (item, point) in points.iter().enumerate() {
        for (axis, coordinate) in point.iter().enumerate() {
            if !coordinate.is_finite() {
                return Err(PointSegmentBatchError::NonFinitePoint {
                    collection,
                    item,
                    axis,
                });
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Association {
    AcceptedDistance,
    FrozenJoint,
}

struct SegmentEvaluation {
    distance: f64,
    t: f64,
    degenerate: bool,
}

fn point_segment_evaluation(
    point: [f64; 3],
    start: [f64; 3],
    end: [f64; 3],
    association: Association,
) -> SegmentEvaluation {
    let axis = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
    let relative = [
        point[0] - start[0],
        point[1] - start[1],
        point[2] - start[2],
    ];
    let length_squared = dot(axis, axis);
    if length_squared < 1e-18 {
        return SegmentEvaluation {
            distance: norm(relative),
            t: 0.0,
            degenerate: true,
        };
    }
    let t = (dot(relative, axis) / length_squared).clamp(0.0, 1.0);
    let delta = match association {
        Association::AcceptedDistance => [
            relative[0] - t * axis[0],
            relative[1] - t * axis[1],
            relative[2] - t * axis[2],
        ],
        Association::FrozenJoint => [
            point[0] - (start[0] + t * axis[0]),
            point[1] - (start[1] + t * axis[1]),
            point[2] - (start[2] + t * axis[2]),
        ],
    };
    SegmentEvaluation {
        distance: norm(delta),
        t,
        degenerate: false,
    }
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn norm(value: [f64; 3]) -> f64 {
    (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt()
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    const THREAD_CHILD: &str = "GEOM_KERNEL_SEGMENT_BATCH_THREAD_CHILD";
    const THREAD_SIGNATURE: &str = "GEOM_KERNEL_SEGMENT_BATCH_BITS=";

    #[test]
    fn rows_preserve_point_and_segment_order_and_zero_length_behavior() {
        let result = point_segment_distances_batch(
            &[[0.0, 1.0, 0.0], [3.0, 0.0, 0.0]],
            &[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            &[[1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            PointSegmentBatchLimits::new(4, 32),
        )
        .unwrap();
        assert_eq!(result.point_count(), 2);
        assert_eq!(result.segment_count(), 2);
        assert_eq!(result.row(0), Some([1.0, 5.0f64.sqrt()].as_slice()));
        assert_eq!(result.row(1), Some([2.0, 1.0].as_slice()));
        assert!(result.row(2).is_none());
    }

    #[test]
    fn exact_pair_and_byte_caps_pass_and_cap_plus_one_refuses() {
        let points = [[0.0, 1.0, 0.0], [3.0, 0.0, 0.0]];
        let starts = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        let ends = [[1.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        assert!(point_segment_distances_batch(
            &points,
            &starts,
            &ends,
            PointSegmentBatchLimits::new(4, 32),
        )
        .is_ok());
        assert_eq!(
            point_segment_distances_batch(
                &points,
                &starts,
                &ends,
                PointSegmentBatchLimits::new(3, 32),
            ),
            Err(PointSegmentBatchError::OutputPairLimitExceeded {
                required: 4,
                maximum: 3,
            })
        );
        assert_eq!(
            point_segment_distances_batch(
                &points,
                &starts,
                &ends,
                PointSegmentBatchLimits::new(4, 31),
            ),
            Err(PointSegmentBatchError::OutputByteLimitExceeded {
                required: 32,
                maximum: 31,
            })
        );
        assert!(matches!(
            PointSegmentBatchLimits::for_dimensions(usize::MAX, 2),
            Err(PointSegmentBatchError::OutputSizeOverflow { .. })
        ));
    }

    #[test]
    fn frozen_joint_mode_pins_association_and_local_taper_bits() {
        let point = [[100_000_000.1, -199_999_999.8, 300_000_000.3]];
        let start = [[100_000_000.0, -200_000_000.0, 300_000_000.0]];
        let end = [[100_000_003.0, -199_999_996.0, 300_000_012.0]];
        let accepted =
            point_segment_distances_batch(&point, &start, &end, PointSegmentBatchLimits::new(1, 8))
                .unwrap();
        let joint = point_segment_taper_batch(
            &point,
            &start,
            &end,
            FrozenJointTaperBatch::new(&[0.25], &[1.25]),
            PointSegmentBatchLimits::new(1, 16),
        )
        .unwrap();
        assert_eq!(accepted.row(0).unwrap()[0].to_bits(), 0x3fb8_aca4_9aeb_5cf5);
        assert_eq!(
            joint.distance_row(0).unwrap()[0].to_bits(),
            0x3fb8_aca4_e980_10d7
        );
        assert_ne!(
            accepted.row(0).unwrap()[0].to_bits(),
            joint.distance_row(0).unwrap()[0].to_bits()
        );
        assert!((accepted.row(0).unwrap()[0] - joint.distance_row(0).unwrap()[0]).abs() < 1.0e-7);

        let endpoint_and_middle = point_segment_taper_batch(
            &[
                start[0],
                [100_000_001.5, -199_999_998.0, 300_000_006.0],
                end[0],
            ],
            &start,
            &end,
            FrozenJointTaperBatch::new(&[0.25], &[1.25]),
            PointSegmentBatchLimits::new(3, 48),
        )
        .unwrap();
        assert_eq!(
            endpoint_and_middle.local_radius_row(0).unwrap()[0].to_bits(),
            0x3fd0_0000_0000_0000
        );
        assert_eq!(
            endpoint_and_middle.local_radius_row(1).unwrap()[0].to_bits(),
            0x3fe8_0000_0000_0000
        );
        assert_eq!(
            endpoint_and_middle.local_radius_row(2).unwrap()[0].to_bits(),
            0x3ff4_0000_0000_0000
        );

        let degenerate = point_segment_taper_batch(
            &[[2.0, 0.0, 0.0]],
            &[[1.0, 0.0, 0.0]],
            &[[1.0, 0.0, 0.0]],
            FrozenJointTaperBatch::new(&[0.375], &[9.0]),
            PointSegmentBatchLimits::new(1, 16),
        )
        .unwrap();
        assert_eq!(
            degenerate.local_radius_row(0).unwrap()[0].to_bits(),
            0x3fd8_0000_0000_0000
        );
    }

    #[test]
    fn tapered_mode_caps_and_radius_validation_are_exact() {
        let points = [[0.0, 1.0, 0.0], [3.0, 0.0, 0.0]];
        let starts = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        let ends = [[1.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        let taper = FrozenJointTaperBatch::new(&[0.25, 0.5], &[1.25, 0.75]);
        assert!(point_segment_taper_batch(
            &points,
            &starts,
            &ends,
            taper,
            PointSegmentBatchLimits::new(4, 64)
        )
        .is_ok());
        assert_eq!(
            point_segment_taper_batch(
                &points,
                &starts,
                &ends,
                taper,
                PointSegmentBatchLimits::new(4, 63)
            ),
            Err(PointSegmentBatchError::OutputByteLimitExceeded {
                required: 64,
                maximum: 63,
            })
        );
        assert!(matches!(
            point_segment_taper_batch(
                &points,
                &starts,
                &ends,
                FrozenJointTaperBatch::new(&[f64::NAN, 0.5], &[1.25, 0.75]),
                PointSegmentBatchLimits::new(4, 64)
            ),
            Err(PointSegmentBatchError::NonFiniteRadius {
                collection: "radii_at_start_mm",
                item: 0
            })
        ));
    }

    #[test]
    fn finite_extreme_overflow_is_a_named_output_error() {
        assert_eq!(
            point_segment_distances_batch(
                &[[f64::MAX, 0.0, 0.0]],
                &[[-f64::MAX, 0.0, 0.0]],
                &[[-f64::MAX, 0.0, 0.0]],
                PointSegmentBatchLimits::new(1, 8),
            ),
            Err(PointSegmentBatchError::NonFiniteOutput {
                point: 0,
                segment: 0,
            })
        );
    }

    #[test]
    fn empty_dimensions_return_exact_empty_rows_under_zero_budget() {
        let no_points = point_segment_distances_batch(
            &[],
            &[[0.0; 3]],
            &[[1.0, 0.0, 0.0]],
            PointSegmentBatchLimits::new(0, 0),
        )
        .unwrap();
        assert_eq!(no_points.point_count(), 0);
        assert_eq!(no_points.segment_count(), 1);
        assert!(no_points.row(0).is_none());

        let no_segments = point_segment_distances_batch(
            &[[0.0; 3]],
            &[],
            &[],
            PointSegmentBatchLimits::new(0, 0),
        )
        .unwrap();
        assert_eq!(no_segments.point_count(), 1);
        assert_eq!(no_segments.segment_count(), 0);
        assert_eq!(no_segments.row(0), Some([].as_slice()));
    }

    #[test]
    fn malformed_batches_fail_before_evaluation() {
        assert_eq!(
            point_segment_distances_batch(
                &[],
                &[[0.0; 3]],
                &[],
                PointSegmentBatchLimits::new(0, 0),
            ),
            Err(PointSegmentBatchError::SegmentEndpointCountMismatch { starts: 1, ends: 0 })
        );
        assert!(matches!(
            point_segment_distances_batch(
                &[[f64::NAN, 0.0, 0.0]],
                &[],
                &[],
                PointSegmentBatchLimits::new(0, 0),
            ),
            Err(PointSegmentBatchError::NonFinitePoint {
                collection: "points",
                item: 0,
                axis: 0
            })
        ));
    }

    #[test]
    fn row_bits_are_identical_under_one_four_and_sixteen_configured_threads() {
        let test_name =
            "segment_batch::tests::row_bits_are_identical_under_one_four_and_sixteen_configured_threads";
        if std::env::var_os(THREAD_CHILD).is_none() {
            let signatures = ["1", "4", "16"].map(|threads| {
                let output = Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", test_name, "--nocapture"])
                    .env(THREAD_CHILD, "1")
                    .env("PLINTH_THREADS", threads)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .find(|line| line.starts_with(THREAD_SIGNATURE))
                    .unwrap()
                    .to_owned()
            });
            assert!(signatures.windows(2).all(|pair| pair[0] == pair[1]));
            return;
        }

        let expected_threads = std::env::var("PLINTH_THREADS")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        let result = point_segment_distances_batch(
            &[[0.25, 1.0, 0.0], [3.0, 4.0, 5.0], [-2.0, 0.5, 7.0]],
            &[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [-1.0, 2.0, 3.0]],
            &[[1.0, 0.0, 0.0], [2.0, 0.0, 0.0], [4.0, 5.0, 6.0]],
            PointSegmentBatchLimits::new(9, 72),
        )
        .unwrap();
        assert_eq!(result.effective_threads(), expected_threads);
        let bits = (0..result.point_count())
            .map(|point| {
                result
                    .row(point)
                    .unwrap()
                    .iter()
                    .map(|distance| distance.to_bits())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let taper = point_segment_taper_batch(
            &[[0.25, 1.0, 0.0], [3.0, 4.0, 5.0], [-2.0, 0.5, 7.0]],
            &[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [-1.0, 2.0, 3.0]],
            &[[1.0, 0.0, 0.0], [2.0, 0.0, 0.0], [4.0, 5.0, 6.0]],
            FrozenJointTaperBatch::new(&[0.2, 0.4, 0.6], &[0.8, 1.0, 1.2]),
            PointSegmentBatchLimits::new(9, 144),
        )
        .unwrap();
        assert_eq!(taper.effective_threads(), expected_threads);
        let taper_bits = (0..taper.point_count())
            .map(|point| {
                (
                    taper
                        .distance_row(point)
                        .unwrap()
                        .iter()
                        .map(|value| value.to_bits())
                        .collect::<Vec<_>>(),
                    taper
                        .local_radius_row(point)
                        .unwrap()
                        .iter()
                        .map(|value| value.to_bits())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        println!("{THREAD_SIGNATURE}{bits:?}:{taper_bits:?}");
    }
}
