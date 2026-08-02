//! Immutable, batch-only point-cloud spatial queries.
//!
//! The owned [`PointCloudIndex`] deliberately exposes no scalar query API.  A
//! caller gathers all queries, invokes one batch method, then scatters rows from
//! the returned CSR buffers.  The underlying tree is Kiddo's maintained,
//! immutable three-dimensional kd-tree; this module owns ordering and boundary
//! semantics at the kernel boundary.

use std::num::NonZeroUsize;

use kiddo::{ImmutableKdTree, SquaredEuclidean};
use rayon::prelude::*;

use crate::par;

/// A point stored in a [`NeighborCsr`] row.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Neighbor {
    point_id: u64,
    squared_distance: f64,
}

/// CSR output for a point-cloud batch query.
///
/// Row `q` occupies `point_ids[offsets[q]..offsets[q + 1]]`, with the same
/// range in `squared_distances`.  Rows are deterministically ordered by
/// `(squared_distance.total_cmp, global_point_id)`.  Point IDs are the input
/// order supplied to [`PointCloudIndex::new`].
#[derive(Clone, Debug, PartialEq)]
pub struct NeighborCsr {
    pub offsets: Vec<usize>,
    pub point_ids: Vec<u64>,
    pub squared_distances: Vec<f64>,
}

impl NeighborCsr {
    /// Number of query rows represented by this CSR payload.
    pub fn query_count(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    /// The point-ID/distance range for one query row, if present.
    pub fn row_range(&self, query: usize) -> Option<std::ops::Range<usize>> {
        self.offsets
            .get(query)
            .zip(self.offsets.get(query + 1))
            .map(|(&start, &end)| start..end)
    }
}

/// A point cloud could not be represented by the immutable spatial index.
#[derive(Clone, Debug, PartialEq)]
pub enum PointIndexBuildError {
    EmptyPointCloud,
    NonFinitePoint {
        point: usize,
        axis: usize,
    },
    /// Kiddo's immutable tree stores leaf offsets as `u32`; reject before its
    /// constructor would panic on a too-large source slice.
    PointCountExceedsIndexRange {
        count: usize,
        maximum: usize,
    },
}

impl std::fmt::Display for PointIndexBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPointCloud => write!(formatter, "point cloud is empty"),
            Self::NonFinitePoint { point, axis } => {
                write!(
                    formatter,
                    "point {point} has a non-finite coordinate on axis {axis}"
                )
            }
            Self::PointCountExceedsIndexRange { count, maximum } => write!(
                formatter,
                "point cloud has {count} points; immutable index supports at most {maximum}"
            ),
        }
    }
}

impl std::error::Error for PointIndexBuildError {}

/// Invalid arguments to a point-cloud query batch.
#[derive(Clone, Debug, PartialEq)]
pub enum PointQueryError {
    NonFiniteQuery {
        query: usize,
        axis: usize,
    },
    InvalidRadius {
        radius: f64,
    },
    ZeroNeighborsRequested,
    RequestedNeighborsExceedPointCount {
        requested: usize,
        point_count: usize,
    },
}

impl std::fmt::Display for PointQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteQuery { query, axis } => {
                write!(
                    formatter,
                    "query {query} has a non-finite coordinate on axis {axis}"
                )
            }
            Self::InvalidRadius { radius } => {
                write!(
                    formatter,
                    "radius must be finite and non-negative, got {radius}"
                )
            }
            Self::ZeroNeighborsRequested => write!(formatter, "k must be greater than zero"),
            Self::RequestedNeighborsExceedPointCount {
                requested,
                point_count,
            } => write!(
                formatter,
                "requested {requested} neighbors from a point cloud of {point_count} points"
            ),
        }
    }
}

impl std::error::Error for PointQueryError {}

/// An immutable index of exact finite `f64` points.
///
/// The index preserves every input point and maps its original input position
/// to the output's global point ID.  Duplicate coordinates are valid.
pub struct PointCloudIndex {
    tree: ImmutableKdTree<f64, 3>,
    point_count: usize,
}

impl PointCloudIndex {
    /// Build one immutable index for an ordered, nonempty, finite point cloud.
    pub fn new(points: &[[f64; 3]]) -> Result<Self, PointIndexBuildError> {
        if points.is_empty() {
            return Err(PointIndexBuildError::EmptyPointCloud);
        }
        if points.len() > u32::MAX as usize {
            return Err(PointIndexBuildError::PointCountExceedsIndexRange {
                count: points.len(),
                maximum: u32::MAX as usize,
            });
        }
        for (point, coordinates) in points.iter().enumerate() {
            for (axis, value) in coordinates.iter().enumerate() {
                if !value.is_finite() {
                    return Err(PointIndexBuildError::NonFinitePoint { point, axis });
                }
            }
        }

        Ok(Self {
            tree: points.into(),
            point_count: points.len(),
        })
    }

    /// Number of points held by this immutable index.
    pub fn point_count(&self) -> usize {
        self.point_count
    }

    /// Find all points with squared Euclidean distance no greater than
    /// `radius * radius`.
    ///
    /// The boundary is inclusive.  `radius` is one value for the entire batch;
    /// a finite, non-negative radius is required.  The returned rows are
    /// deterministically sorted even where the underlying index has geometric
    /// ties.
    pub fn radius_neighbors_batch(
        &self,
        queries: &[[f64; 3]],
        radius: f64,
    ) -> Result<NeighborCsr, PointQueryError> {
        if !radius.is_finite() || radius < 0.0 {
            return Err(PointQueryError::InvalidRadius { radius });
        }
        validate_queries(queries)?;
        let radius_squared = radius * radius;
        let rows = par::run(|| {
            queries
                .par_iter()
                .map(|query| {
                    let mut row = self
                        .tree
                        .within_exclusive::<SquaredEuclidean>(query, radius_squared, true)
                        .into_iter()
                        .map(|neighbor| Neighbor {
                            point_id: neighbor.item,
                            squared_distance: neighbor.distance,
                        })
                        .collect::<Vec<_>>();
                    sort_neighbors(&mut row);
                    row
                })
                .collect::<Vec<_>>()
        });
        Ok(assemble(rows))
    }

    /// Find exactly `k` nearest points for every query in the batch.
    ///
    /// `k` must be in `1..=point_count`.  Ties at the final distance are
    /// resolved by global point ID, so equal-coordinate inputs remain stable.
    pub fn knn_batch(
        &self,
        queries: &[[f64; 3]],
        k: usize,
    ) -> Result<NeighborCsr, PointQueryError> {
        if k == 0 {
            return Err(PointQueryError::ZeroNeighborsRequested);
        }
        if k > self.point_count {
            return Err(PointQueryError::RequestedNeighborsExceedPointCount {
                requested: k,
                point_count: self.point_count,
            });
        }
        validate_queries(queries)?;
        let count = NonZeroUsize::new(k).expect("k was validated above");
        let rows = par::run(|| {
            queries
                .par_iter()
                .map(|query| {
                    // `nearest_n` is the indexed fast path.  Its final tie may
                    // be implementation-ordered, so fetch that inclusive
                    // boundary once more and impose the kernel's ID tie rule.
                    let candidates = self.tree.nearest_n::<SquaredEuclidean>(query, count);
                    let boundary = candidates
                        .iter()
                        .map(|neighbor| neighbor.distance)
                        .max_by(f64::total_cmp)
                        .expect("k is nonzero and the index is nonempty");
                    let mut row = self
                        .tree
                        .within_exclusive::<SquaredEuclidean>(query, boundary, true)
                        .into_iter()
                        .map(|neighbor| Neighbor {
                            point_id: neighbor.item,
                            squared_distance: neighbor.distance,
                        })
                        .collect::<Vec<_>>();
                    sort_neighbors(&mut row);
                    row.truncate(k);
                    row
                })
                .collect::<Vec<_>>()
        });
        Ok(assemble(rows))
    }
}

fn validate_queries(queries: &[[f64; 3]]) -> Result<(), PointQueryError> {
    for (query, coordinates) in queries.iter().enumerate() {
        for (axis, value) in coordinates.iter().enumerate() {
            if !value.is_finite() {
                return Err(PointQueryError::NonFiniteQuery { query, axis });
            }
        }
    }
    Ok(())
}

fn sort_neighbors(neighbors: &mut [Neighbor]) {
    neighbors.sort_unstable_by(|left, right| {
        left.squared_distance
            .total_cmp(&right.squared_distance)
            .then(left.point_id.cmp(&right.point_id))
    });
}

fn assemble(rows: Vec<Vec<Neighbor>>) -> NeighborCsr {
    let item_count = rows.iter().map(Vec::len).sum();
    let mut offsets = Vec::with_capacity(rows.len() + 1);
    let mut point_ids = Vec::with_capacity(item_count);
    let mut squared_distances = Vec::with_capacity(item_count);
    offsets.push(0);
    for row in rows {
        point_ids.extend(row.iter().map(|neighbor| neighbor.point_id));
        squared_distances.extend(row.iter().map(|neighbor| neighbor.squared_distance));
        offsets.push(point_ids.len());
    }
    NeighborCsr {
        offsets,
        point_ids,
        squared_distances,
    }
}
