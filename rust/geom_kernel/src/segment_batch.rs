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
    if segment_starts.len() != segment_ends.len() {
        return Err(PointSegmentBatchError::SegmentEndpointCountMismatch {
            starts: segment_starts.len(),
            ends: segment_ends.len(),
        });
    }
    validate("points", points)?;
    validate("segment_starts", segment_starts)?;
    validate("segment_ends", segment_ends)?;
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
    let output_bytes = output_pairs.checked_mul(std::mem::size_of::<f64>()).ok_or(
        PointSegmentBatchError::OutputSizeOverflow {
            points: point_count,
            segments: segment_count,
        },
    )?;
    if output_bytes > limits.max_output_bytes {
        return Err(PointSegmentBatchError::OutputByteLimitExceeded {
            required: output_bytes,
            maximum: limits.max_output_bytes,
        });
    }

    let mut distances_mm = vec![0.0; output_pairs];
    let effective_threads = par::run(|| {
        let effective_threads = rayon::current_num_threads();
        if segment_count != 0 {
            distances_mm
                .par_chunks_mut(segment_count)
                .zip(points.par_iter())
                .for_each(|(row, point)| {
                    for (output, (start, end)) in
                        row.iter_mut().zip(segment_starts.iter().zip(segment_ends))
                    {
                        *output = point_segment_distance(*point, *start, *end);
                    }
                });
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
    Ok(PointSegmentDistanceBatch {
        point_count,
        segment_count,
        distances_mm,
        effective_threads,
    })
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

fn point_segment_distance(point: [f64; 3], start: [f64; 3], end: [f64; 3]) -> f64 {
    let axis = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
    let relative = [
        point[0] - start[0],
        point[1] - start[1],
        point[2] - start[2],
    ];
    let length_squared = dot(axis, axis);
    if length_squared < 1e-18 {
        return norm(relative);
    }
    let t = (dot(relative, axis) / length_squared).clamp(0.0, 1.0);
    let delta = [
        relative[0] - t * axis[0],
        relative[1] - t * axis[1],
        relative[2] - t * axis[2],
    ];
    norm(delta)
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn norm(value: [f64; 3]) -> f64 {
    (value[0].powi(2) + value[1].powi(2) + value[2].powi(2)).sqrt()
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
        println!("{THREAD_SIGNATURE}{bits:?}");
    }
}
