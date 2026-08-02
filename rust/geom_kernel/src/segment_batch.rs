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
        }
    }
}

impl std::error::Error for PointSegmentBatchError {}

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
    point_count
        .checked_mul(segment_count)
        .ok_or(PointSegmentBatchError::OutputSizeOverflow {
            points: point_count,
            segments: segment_count,
        })?;

    let (effective_threads, rows) = par::run(|| {
        let effective_threads = rayon::current_num_threads();
        let rows = points
            .par_iter()
            .map(|point| {
                segment_starts
                    .iter()
                    .zip(segment_ends)
                    .map(|(start, end)| point_segment_distance(*point, *start, *end))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        (effective_threads, rows)
    });
    let distances_mm = rows.into_iter().flatten().collect();
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
    use super::*;

    #[test]
    fn rows_preserve_point_and_segment_order_and_zero_length_behavior() {
        let result = point_segment_distances_batch(
            &[[0.0, 1.0, 0.0], [3.0, 0.0, 0.0]],
            &[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            &[[1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
        )
        .unwrap();
        assert_eq!(result.point_count(), 2);
        assert_eq!(result.segment_count(), 2);
        assert_eq!(result.row(0), Some([1.0, 5.0f64.sqrt()].as_slice()));
        assert_eq!(result.row(1), Some([2.0, 1.0].as_slice()));
        assert!(result.row(2).is_none());
    }

    #[test]
    fn malformed_batches_fail_before_evaluation() {
        assert_eq!(
            point_segment_distances_batch(&[], &[[0.0; 3]], &[]),
            Err(PointSegmentBatchError::SegmentEndpointCountMismatch { starts: 1, ends: 0 })
        );
        assert!(matches!(
            point_segment_distances_batch(&[[f64::NAN, 0.0, 0.0]], &[], &[]),
            Err(PointSegmentBatchError::NonFinitePoint {
                collection: "points",
                item: 0,
                axis: 0
            })
        ));
    }
}
