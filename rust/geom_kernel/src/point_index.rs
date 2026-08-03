//! Immutable, batch-only point-cloud spatial queries.
//!
//! The owned [`PointCloudIndex`] deliberately exposes no scalar query API.  A
//! caller gathers all queries, invokes one batch method, then scatters rows from
//! the returned CSR buffers.  The underlying tree is Kiddo's maintained,
//! immutable three-dimensional kd-tree; this module owns ordering and boundary
//! semantics at the kernel boundary.

use std::num::NonZeroUsize;

use kiddo::{ImmutableKdTree, NearestNeighbour, SquaredEuclidean};
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

/// Caller-owned hard limits for one radius-neighbor batch.
///
/// The kernel never offers an unbounded radius query: the CSR offset storage
/// is checked before searching, and each tree request is capped to the
/// remaining pair/byte budget plus one item so cap overflow is detected
/// before an unbounded result vector can be allocated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadiusQueryLimits {
    pub max_output_pairs: usize,
    pub max_output_bytes: usize,
}

/// Caller-owned hard limits for one planned k-nearest-neighbour batch.
///
/// There is deliberately no default: a caller must bound the source cloud,
/// query batch, requested neighbours, logical allocation peak, and the
/// `source_count * query_count` work estimate before gathering either input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KnnBatchLimits {
    pub max_source_points: usize,
    pub max_query_points: usize,
    pub max_neighbors: usize,
    pub max_logical_peak_bytes: usize,
    pub max_work: usize,
}

/// A checked, opaque resource plan for one [`PointCloudIndex`] kNN batch.
///
/// The byte count is a logical element-capacity accounting only: it excludes
/// allocator metadata, alignment, and RSS.  Its Kiddo terms depend on the
/// pinned `kiddo 5.3.3` immutable-tree layout and disabled default features:
/// leaf size 32, `L = ceil(N / 32)`, and `P = 0` when `L < 2`, otherwise
/// `next_power_of_two(L)`.  Updating that pin or its features requires review
/// of this formula.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnnBatchPlan {
    source_count: usize,
    query_count: usize,
    neighbors: usize,
    logical_peak_bytes: usize,
    work: usize,
}

impl KnnBatchPlan {
    /// Check all dimensions and logical capacities before source/query gather,
    /// immutable-tree construction, or query execution.
    pub fn new(
        source_count: usize,
        query_count: usize,
        neighbors: usize,
        limits: KnnBatchLimits,
    ) -> Result<Self, KnnBatchPlanError> {
        if source_count > limits.max_source_points {
            return Err(KnnBatchPlanError::SourceCountLimit {
                required: source_count,
                limit: limits.max_source_points,
            });
        }
        if query_count > limits.max_query_points {
            return Err(KnnBatchPlanError::QueryCountLimit {
                required: query_count,
                limit: limits.max_query_points,
            });
        }
        if neighbors == 0 {
            return Err(KnnBatchPlanError::ZeroNeighborsRequested);
        }
        if neighbors > limits.max_neighbors {
            return Err(KnnBatchPlanError::NeighborCountLimit {
                required: neighbors,
                limit: limits.max_neighbors,
            });
        }
        if neighbors > source_count {
            return Err(KnnBatchPlanError::RequestedNeighborsExceedPointCount {
                requested: neighbors,
                point_count: source_count,
            });
        }
        let work = source_count.checked_mul(query_count).ok_or(
            KnnBatchPlanError::ArithmeticOverflow {
                term: "source_count * query_count",
            },
        )?;
        if work > limits.max_work {
            return Err(KnnBatchPlanError::WorkLimit {
                required: work,
                limit: limits.max_work,
            });
        }
        let logical_peak_bytes = knn_logical_peak_bytes(source_count, query_count, neighbors)?;
        if logical_peak_bytes > limits.max_logical_peak_bytes {
            return Err(KnnBatchPlanError::LogicalPeakByteLimit {
                required: logical_peak_bytes,
                limit: limits.max_logical_peak_bytes,
            });
        }
        Ok(Self {
            source_count,
            query_count,
            neighbors,
            logical_peak_bytes,
            work,
        })
    }

    #[must_use]
    pub const fn source_count(&self) -> usize {
        self.source_count
    }
    #[must_use]
    pub const fn query_count(&self) -> usize {
        self.query_count
    }
    #[must_use]
    pub const fn neighbors(&self) -> usize {
        self.neighbors
    }
    #[must_use]
    pub const fn logical_peak_bytes(&self) -> usize {
        self.logical_peak_bytes
    }
    #[must_use]
    pub const fn work(&self) -> usize {
        self.work
    }
}

/// A kNN batch plan could not be represented within its caller-provided caps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnnBatchPlanError {
    SourceCountLimit { required: usize, limit: usize },
    QueryCountLimit { required: usize, limit: usize },
    ZeroNeighborsRequested,
    NeighborCountLimit { required: usize, limit: usize },
    RequestedNeighborsExceedPointCount { requested: usize, point_count: usize },
    LogicalPeakByteLimit { required: usize, limit: usize },
    WorkLimit { required: usize, limit: usize },
    ArithmeticOverflow { term: &'static str },
}

impl std::fmt::Display for KnnBatchPlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceCountLimit { required, limit } => write!(f, "kNN source count {required} exceeds limit {limit}"),
            Self::QueryCountLimit { required, limit } => write!(f, "kNN query count {required} exceeds limit {limit}"),
            Self::ZeroNeighborsRequested => write!(f, "kNN k must be greater than zero"),
            Self::NeighborCountLimit { required, limit } => write!(f, "kNN k {required} exceeds limit {limit}"),
            Self::RequestedNeighborsExceedPointCount { requested, point_count } => write!(f, "requested {requested} neighbors from a point cloud of {point_count} points"),
            Self::LogicalPeakByteLimit { required, limit } => write!(f, "kNN logical peak requires {required} bytes; limit is {limit}"),
            Self::WorkLimit { required, limit } => write!(f, "kNN work requires {required}; limit is {limit}"),
            Self::ArithmeticOverflow { term } => write!(f, "kNN resource arithmetic overflow in {term}"),
        }
    }
}

impl std::error::Error for KnnBatchPlanError {}

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
    KnnPlanSourceCountMismatch {
        planned: usize,
        actual: usize,
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
            Self::KnnPlanSourceCountMismatch { planned, actual } => write!(
                formatter,
                "kNN plan is for {planned} source points but construction received {actual}"
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
    RadiusOutputPairLimit {
        required: usize,
        limit: usize,
    },
    RadiusOutputByteLimit {
        required: usize,
        limit: usize,
    },
    KnnPlanQueryCountMismatch {
        planned: usize,
        actual: usize,
    },
    KnnPlanSourceCountMismatch {
        planned: usize,
        actual: usize,
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
            Self::RadiusOutputPairLimit { required, limit } => write!(
                formatter,
                "radius query requires at least {required} output pairs; limit is {limit}"
            ),
            Self::RadiusOutputByteLimit { required, limit } => write!(
                formatter,
                "radius query requires at least {required} output bytes; limit is {limit}"
            ),
            Self::KnnPlanQueryCountMismatch { planned, actual } => write!(
                formatter,
                "kNN plan is for {planned} queries but execution received {actual}"
            ),
            Self::KnnPlanSourceCountMismatch { planned, actual } => write!(
                formatter,
                "kNN plan is for {planned} source points but the index holds {actual}"
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

    /// Build an index only after a matching checked kNN plan has preflighted
    /// the source count and all logical capacities.
    pub fn new_for_knn_batch(
        points: &[[f64; 3]],
        plan: &KnnBatchPlan,
    ) -> Result<Self, PointIndexBuildError> {
        if points.len() != plan.source_count {
            return Err(PointIndexBuildError::KnnPlanSourceCountMismatch {
                planned: plan.source_count,
                actual: points.len(),
            });
        }
        Self::new(points)
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
        limits: RadiusQueryLimits,
    ) -> Result<NeighborCsr, PointQueryError> {
        if !radius.is_finite() || radius < 0.0 {
            return Err(PointQueryError::InvalidRadius { radius });
        }
        validate_queries(queries)?;
        let offsets_bytes = queries
            .len()
            .checked_add(1)
            .and_then(|count| count.checked_mul(std::mem::size_of::<usize>()))
            .ok_or(PointQueryError::RadiusOutputByteLimit {
                required: usize::MAX,
                limit: limits.max_output_bytes,
            })?;
        if offsets_bytes > limits.max_output_bytes {
            return Err(PointQueryError::RadiusOutputByteLimit {
                required: offsets_bytes,
                limit: limits.max_output_bytes,
            });
        }
        let radius_squared = radius * radius;
        let pair_bytes = std::mem::size_of::<u64>() + std::mem::size_of::<f64>();
        let byte_pair_limit = (limits.max_output_bytes - offsets_bytes) / pair_bytes;
        let mut rows = Vec::with_capacity(queries.len());
        let mut total = 0usize;
        for query in queries {
            let remaining = limits
                .max_output_pairs
                .min(byte_pair_limit)
                .saturating_sub(total);
            let request = remaining.saturating_add(1).min(self.point_count).max(1);
            let mut row = self
                .tree
                .nearest_n_within_exclusive::<SquaredEuclidean>(
                    query,
                    radius_squared,
                    NonZeroUsize::new(request).expect("request is at least one"),
                    false,
                    true,
                )
                .into_iter()
                .map(|neighbor| Neighbor {
                    point_id: neighbor.item,
                    squared_distance: neighbor.distance,
                })
                .collect::<Vec<_>>();
            let required_pairs = total.saturating_add(row.len());
            if required_pairs > limits.max_output_pairs {
                return Err(PointQueryError::RadiusOutputPairLimit {
                    required: required_pairs,
                    limit: limits.max_output_pairs,
                });
            }
            let required_bytes =
                offsets_bytes.saturating_add(required_pairs.saturating_mul(pair_bytes));
            if required_bytes > limits.max_output_bytes {
                return Err(PointQueryError::RadiusOutputByteLimit {
                    required: required_bytes,
                    limit: limits.max_output_bytes,
                });
            }
            sort_neighbors(&mut row);
            total = required_pairs;
            rows.push(row);
        }
        Ok(assemble(rows))
    }

    /// Execute one checked, planned k-nearest-neighbour batch.
    ///
    /// Execution is sequential: each query obtains `k` candidates with
    /// Kiddo's `nearest_n`, drops them after recording the boundary, then makes
    /// one inclusive tie fetch capped at the planned source count.  Rows are
    /// stable-sorted by `(distance.total_cmp, global_point_id)` and written
    /// directly into preallocated CSR buffers.
    pub fn knn_batch_planned(
        &self,
        queries: &[[f64; 3]],
        plan: &KnnBatchPlan,
    ) -> Result<NeighborCsr, PointQueryError> {
        if queries.len() != plan.query_count {
            return Err(PointQueryError::KnnPlanQueryCountMismatch {
                planned: plan.query_count,
                actual: queries.len(),
            });
        }
        if self.point_count != plan.source_count {
            return Err(PointQueryError::KnnPlanSourceCountMismatch {
                planned: plan.source_count,
                actual: self.point_count,
            });
        }
        validate_queries(queries)?;
        let pair_count = plan.query_count.checked_mul(plan.neighbors).expect("checked by KnnBatchPlan");
        let mut offsets = Vec::with_capacity(plan.query_count + 1);
        let mut point_ids = Vec::with_capacity(pair_count);
        let mut squared_distances = Vec::with_capacity(pair_count);
        offsets.push(0);
        let count = NonZeroUsize::new(plan.neighbors).expect("validated by KnnBatchPlan");
        let tie_cap = NonZeroUsize::new(plan.source_count).expect("validated by KnnBatchPlan");
        for query in queries {
            let boundary = {
                let candidates = self.tree.nearest_n::<SquaredEuclidean>(query, count);
                candidates
                    .iter()
                    .map(|neighbor| neighbor.distance)
                    .max_by(f64::total_cmp)
                    .expect("nonempty plan and immutable index")
            };
            let mut ties = self.tree.nearest_n_within_exclusive::<SquaredEuclidean>(
                query, boundary, tie_cap, false, true,
            );
            ties.sort_by(|left, right| {
                left.distance.total_cmp(&right.distance).then(left.item.cmp(&right.item))
            });
            for neighbor in ties.into_iter().take(plan.neighbors) {
                point_ids.push(neighbor.item);
                squared_distances.push(neighbor.distance);
            }
            offsets.push(point_ids.len());
        }
        Ok(NeighborCsr { offsets, point_ids, squared_distances })
    }
}

fn knn_logical_peak_bytes(
    source_count: usize,
    query_count: usize,
    neighbors: usize,
) -> Result<usize, KnnBatchPlanError> {
    // Kiddo 5.3.3 ImmutableKdTree<f64, u64, 3, 32> retained capacities.
    let leaf_count = source_count.checked_add(31).ok_or(KnnBatchPlanError::ArithmeticOverflow { term: "ceil(source_count / 32)" })? / 32;
    let stem_count = if leaf_count < 2 { 0 } else { leaf_count.checked_next_power_of_two().ok_or(KnnBatchPlanError::ArithmeticOverflow { term: "next_power_of_two(leaf_count)" })? };
    let source_coordinates = checked_bytes(source_count, std::mem::size_of::<[f64; 3]>(), "gathered source coordinates")?;
    let leaf_coordinates = checked_bytes(source_count, 3 * std::mem::size_of::<f64>(), "Kiddo leaf coordinates")?;
    let leaf_items = checked_bytes(source_count, std::mem::size_of::<u64>(), "Kiddo leaf items")?;
    let leaf_extents = checked_bytes(leaf_count, std::mem::size_of::<(u32, u32)>(), "Kiddo leaf extents")?;
    let stems = checked_bytes(stem_count, std::mem::size_of::<f64>(), "Kiddo stems")?;
    let sort_indices = checked_bytes(source_count, std::mem::size_of::<usize>(), "Kiddo build sort indices")?;
    let query_coordinates = checked_bytes(query_count, std::mem::size_of::<[f64; 3]>(), "gathered query coordinates")?;
    let output_pairs = query_count.checked_mul(neighbors).ok_or(KnnBatchPlanError::ArithmeticOverflow { term: "query_count * neighbors" })?;
    let offsets = checked_bytes(query_count.checked_add(1).ok_or(KnnBatchPlanError::ArithmeticOverflow { term: "query_count + 1" })?, std::mem::size_of::<usize>(), "CSR offsets")?;
    let ids = checked_bytes(output_pairs, std::mem::size_of::<u64>(), "CSR point IDs")?;
    let distances = checked_bytes(output_pairs, std::mem::size_of::<f64>(), "CSR squared distances")?;
    let tie_scratch = checked_bytes(source_count, std::mem::size_of::<NearestNeighbour<f64, u64>>(), "inclusive tie scratch")?;
    [source_coordinates, leaf_coordinates, leaf_items, leaf_extents, stems, sort_indices, query_coordinates, offsets, ids, distances, tie_scratch]
        .into_iter()
        .try_fold(0usize, |total, term| total.checked_add(term).ok_or(KnnBatchPlanError::ArithmeticOverflow { term: "logical peak byte sum" }))
}

fn checked_bytes(count: usize, element_bytes: usize, term: &'static str) -> Result<usize, KnnBatchPlanError> {
    count.checked_mul(element_bytes).ok_or(KnnBatchPlanError::ArithmeticOverflow { term })
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
