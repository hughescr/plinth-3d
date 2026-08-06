use geom_kernel::{
    KnnBatchLimits, KnnBatchPlan, KnnBatchPlanError, NeighborCsr, PointCloudIndex,
    PointIndexBuildError, PointQueryError, RadiusQueryLimits,
};

const THREAD_SWEEP_CHILD: &str = "GEOM_KERNEL_POINT_INDEX_THREAD_SWEEP_CHILD";
const THREAD_SWEEP_SIGNATURE: &str = "GEOM_KERNEL_POINT_INDEX_THREAD_SWEEP_SIGNATURE=";

fn radius_limits() -> RadiusQueryLimits {
    RadiusQueryLimits {
        max_output_pairs: usize::MAX,
        max_output_bytes: usize::MAX,
    }
}

fn knn_plan(sources: usize, queries: usize, k: usize) -> KnnBatchPlan {
    KnnBatchPlan::new(
        sources,
        queries,
        k,
        KnnBatchLimits {
            max_source_points: sources,
            max_query_points: queries,
            max_neighbors: k,
            max_logical_peak_bytes: usize::MAX,
            max_work: usize::MAX,
        },
    )
    .unwrap()
}

fn sweep_threads(test_name: &str) -> Option<Vec<String>> {
    if std::env::var(THREAD_SWEEP_CHILD).as_deref() == Ok("1") {
        return None;
    }
    Some(
        [1, 4, 16]
            .into_iter()
            .map(|threads| {
                let output = std::process::Command::new(
                    std::env::current_exe().expect("current test executable"),
                )
                .args(["--exact", test_name, "--nocapture"])
                .env("PLINTH_THREADS", threads.to_string())
                .env(THREAD_SWEEP_CHILD, "1")
                .output()
                .expect("spawn isolated thread-sweep test");
                assert!(
                    output.status.success(),
                    "thread sweep child {threads} failed:\n{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                );
                let stdout = String::from_utf8_lossy(&output.stdout);
                let sentinel = format!("\n{THREAD_SWEEP_SIGNATURE}");
                assert_eq!(
                    stdout.matches(&sentinel).count(),
                    1,
                    "thread sweep child must emit exactly one signature sentinel:\n{stdout}"
                );
                stdout
                    .split_once(&sentinel)
                    .and_then(|(_, signature)| signature.lines().next())
                    .map(str::to_owned)
                    .expect("thread sweep child must report its deterministic signature")
            })
            .collect(),
    )
}

fn rows(result: &NeighborCsr) -> Vec<Vec<(u64, u64)>> {
    (0..result.query_count())
        .map(|query| {
            result
                .row_range(query)
                .unwrap()
                .map(|index| {
                    (
                        result.point_ids[index],
                        result.squared_distances[index].to_bits(),
                    )
                })
                .collect()
        })
        .collect()
}

fn signature(result: &NeighborCsr) -> (Vec<usize>, Vec<u64>, Vec<u64>) {
    (
        result.offsets.clone(),
        result.point_ids.clone(),
        result
            .squared_distances
            .iter()
            .map(|distance| distance.to_bits())
            .collect(),
    )
}

#[test]
fn radius_boundary_is_inclusive_at_and_distinguishes_adjacent_f64_values() {
    let below = f64::from_bits(1.0f64.to_bits() - 1);
    let above = f64::from_bits(1.0f64.to_bits() + 1);
    let index =
        PointCloudIndex::new(&[[below, 0.0, 0.0], [1.0, 0.0, 0.0], [above, 0.0, 0.0]]).unwrap();

    let at = index
        .radius_neighbors_batch(&[[0.0; 3]], 1.0, radius_limits())
        .unwrap();
    assert_eq!(at.offsets, vec![0, 2]);
    assert_eq!(at.point_ids, vec![0, 1]);

    let just_below = index
        .radius_neighbors_batch(&[[0.0; 3]], below, radius_limits())
        .unwrap();
    assert_eq!(just_below.point_ids, vec![0]);

    let just_above = index
        .radius_neighbors_batch(&[[0.0; 3]], above, radius_limits())
        .unwrap();
    assert_eq!(just_above.point_ids, vec![0, 1, 2]);
}

#[test]
fn radius_rows_are_csr_with_duplicate_and_equal_distance_id_ties() {
    let index = PointCloudIndex::new(&[
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [3.0, 0.0, 0.0],
    ])
    .unwrap();
    let result = index
        .radius_neighbors_batch(
            &[[0.0; 3], [10.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
            1.0,
            radius_limits(),
        )
        .unwrap();

    assert_eq!(result.offsets, vec![0, 3, 3, 3]);
    assert_eq!(result.point_ids, vec![0, 1, 2]);
    assert_eq!(result.squared_distances, vec![1.0, 1.0, 1.0]);
    assert_eq!(result.row_range(1), Some(3..3));
    assert_eq!(result.row_range(3), None);
}

#[test]
fn radius_output_caps_accept_exact_and_reject_cap_plus_one_before_unbounded_output() {
    let index = PointCloudIndex::new(&[[0.0; 3], [0.0; 3], [0.0; 3]]).unwrap();
    let queries = [[0.0; 3], [0.0; 3]];
    let exact = RadiusQueryLimits {
        max_output_pairs: 6,
        max_output_bytes: 3 * std::mem::size_of::<usize>()
            + 6 * (std::mem::size_of::<u64>() + std::mem::size_of::<f64>()),
    };
    assert_eq!(
        index
            .radius_neighbors_batch(&queries, 0.0, exact)
            .unwrap()
            .point_ids
            .len(),
        6
    );
    assert_eq!(
        index.radius_neighbors_batch(
            &queries,
            0.0,
            RadiusQueryLimits {
                max_output_pairs: 5,
                ..exact
            }
        ),
        Err(PointQueryError::RadiusOutputPairLimit {
            required: 6,
            limit: 5,
        })
    );
    assert_eq!(
        index.radius_neighbors_batch(
            &queries,
            0.0,
            RadiusQueryLimits {
                max_output_bytes: exact.max_output_bytes - 1,
                ..exact
            }
        ),
        Err(PointQueryError::RadiusOutputByteLimit {
            required: exact.max_output_bytes,
            limit: exact.max_output_bytes - 1,
        })
    );
}

#[test]
fn construction_and_batch_inputs_are_typed_and_reject_nonfinite_values() {
    assert!(matches!(
        PointCloudIndex::new(&[]),
        Err(PointIndexBuildError::EmptyPointCloud)
    ));
    assert!(matches!(
        PointCloudIndex::new(&[[0.0, f64::NAN, 0.0]]),
        Err(PointIndexBuildError::NonFinitePoint { point: 0, axis: 1 })
    ));
    let index = PointCloudIndex::new(&[[0.0; 3]]).unwrap();
    for radius in [f64::NAN, f64::INFINITY, -0.5] {
        match index.radius_neighbors_batch(&[], radius, radius_limits()) {
            Err(PointQueryError::InvalidRadius { radius: got }) => {
                assert_eq!(got.to_bits(), radius.to_bits());
            }
            _ => panic!("invalid radius unexpectedly accepted"),
        }
    }
    assert_eq!(
        index.radius_neighbors_batch(&[[0.0, f64::NEG_INFINITY, 0.0]], 1.0, radius_limits()),
        Err(PointQueryError::NonFiniteQuery { query: 0, axis: 1 })
    );
}

#[test]
fn planned_knn_has_typed_bounds_and_global_id_tie_order() {
    let index = PointCloudIndex::new(&[
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
    ])
    .unwrap();
    assert_eq!(
        KnnBatchPlan::new(
            4,
            1,
            0,
            KnnBatchLimits {
                max_source_points: 4,
                max_query_points: 1,
                max_neighbors: 4,
                max_logical_peak_bytes: usize::MAX,
                max_work: usize::MAX,
            }
        ),
        Err(KnnBatchPlanError::ZeroNeighborsRequested)
    );
    assert_eq!(
        KnnBatchPlan::new(
            4,
            1,
            5,
            KnnBatchLimits {
                max_source_points: 4,
                max_query_points: 1,
                max_neighbors: 5,
                max_logical_peak_bytes: usize::MAX,
                max_work: usize::MAX,
            }
        ),
        Err(KnnBatchPlanError::RequestedNeighborsExceedPointCount {
            requested: 5,
            point_count: 4,
        })
    );
    assert_eq!(
        rows(
            &index
                .knn_batch_planned(&[[0.0; 3]], &knn_plan(4, 1, 2))
                .unwrap()
        ),
        vec![vec![(0, 1.0f64.to_bits()), (1, 1.0f64.to_bits())]]
    );
    assert_eq!(
        rows(
            &index
                .knn_batch_planned(&[[0.0; 3]], &knn_plan(4, 1, 4))
                .unwrap()
        ),
        vec![vec![
            (0, 1.0f64.to_bits()),
            (1, 1.0f64.to_bits()),
            (2, 1.0f64.to_bits()),
            (3, 4.0f64.to_bits()),
        ]]
    );
}

#[test]
fn planned_knn_refuses_each_preflight_cap_and_overflow_at_the_boundary() {
    let baseline = knn_plan(4, 3, 2);
    let exact = KnnBatchLimits {
        max_source_points: 4,
        max_query_points: 3,
        max_neighbors: 2,
        max_logical_peak_bytes: baseline.logical_peak_bytes(),
        max_work: baseline.work(),
    };
    assert_eq!(KnnBatchPlan::new(4, 3, 2, exact).unwrap(), baseline);
    assert!(matches!(
        KnnBatchPlan::new(4, 3, 2, KnnBatchLimits { max_logical_peak_bytes: exact.max_logical_peak_bytes - 1, ..exact }),
        Err(KnnBatchPlanError::LogicalPeakByteLimit { required, limit })
            if required == exact.max_logical_peak_bytes && limit + 1 == required
    ));
    assert!(matches!(
        KnnBatchPlan::new(
            4,
            3,
            2,
            KnnBatchLimits {
                max_work: exact.max_work - 1,
                ..exact
            }
        ),
        Err(KnnBatchPlanError::WorkLimit {
            required: 12,
            limit: 11
        })
    ));
    assert!(matches!(
        KnnBatchPlan::new(
            4,
            3,
            2,
            KnnBatchLimits {
                max_source_points: 3,
                ..exact
            }
        ),
        Err(KnnBatchPlanError::SourceCountLimit {
            required: 4,
            limit: 3
        })
    ));
    assert!(matches!(
        KnnBatchPlan::new(
            4,
            3,
            2,
            KnnBatchLimits {
                max_query_points: 2,
                ..exact
            }
        ),
        Err(KnnBatchPlanError::QueryCountLimit {
            required: 3,
            limit: 2
        })
    ));
    assert!(matches!(
        KnnBatchPlan::new(
            4,
            3,
            2,
            KnnBatchLimits {
                max_neighbors: 1,
                ..exact
            }
        ),
        Err(KnnBatchPlanError::NeighborCountLimit {
            required: 2,
            limit: 1
        })
    ));
    assert_eq!(
        KnnBatchPlan::new(
            usize::MAX,
            2,
            1,
            KnnBatchLimits {
                max_source_points: usize::MAX,
                max_query_points: 2,
                max_neighbors: 1,
                max_logical_peak_bytes: usize::MAX,
                max_work: usize::MAX,
            },
        ),
        Err(KnnBatchPlanError::ArithmeticOverflow {
            term: "source_count * query_count"
        })
    );
}

#[test]
fn planned_knn_explicitly_rejects_empty_sources_and_preserves_empty_query_noop() {
    let limits = KnnBatchLimits {
        max_source_points: 1,
        max_query_points: 0,
        max_neighbors: 1,
        max_logical_peak_bytes: usize::MAX,
        max_work: usize::MAX,
    };
    assert_eq!(
        KnnBatchPlan::new(0, 0, 1, limits),
        Err(KnnBatchPlanError::ZeroSourcePoints)
    );
    let plan = KnnBatchPlan::new(1, 0, 1, limits).unwrap();
    let index = PointCloudIndex::new_for_knn_batch(&[[0.0; 3]], &plan).unwrap();
    assert_eq!(
        index.knn_batch_planned(&[], &plan).unwrap(),
        NeighborCsr {
            offsets: vec![0],
            point_ids: vec![],
            squared_distances: vec![],
        }
    );
}

#[test]
fn planned_knn_all_equal_points_k_one_is_bounded_and_uses_lowest_id() {
    let points = vec![[0.0; 3]; 97];
    let plan = knn_plan(points.len(), 2, 1);
    let index = PointCloudIndex::new_for_knn_batch(&points, &plan).unwrap();
    let result = index
        .knn_batch_planned(&[[0.0; 3], [0.0; 3]], &plan)
        .unwrap();
    assert_eq!(result.offsets, vec![0, 1, 2]);
    assert_eq!(result.point_ids, vec![0, 0]);
    assert_eq!(result.squared_distances, vec![0.0, 0.0]);
}

#[test]
fn planned_knn_rejects_plan_count_mismatches_before_build_or_query() {
    let plan = knn_plan(2, 2, 1);
    assert!(matches!(
        PointCloudIndex::new_for_knn_batch(&[[0.0; 3]], &plan),
        Err(PointIndexBuildError::KnnPlanSourceCountMismatch {
            planned: 2,
            actual: 1
        })
    ));
    let index = PointCloudIndex::new_for_knn_batch(&[[0.0; 3], [1.0; 3]], &plan).unwrap();
    assert_eq!(
        index.knn_batch_planned(&[[0.0; 3]], &plan),
        Err(PointQueryError::KnnPlanQueryCountMismatch {
            planned: 2,
            actual: 1
        })
    );
    assert!(matches!(
        KnnBatchPlan::new(
            10,
            1_000_000,
            1,
            KnnBatchLimits {
                max_source_points: 10,
                max_query_points: 1_000_000,
                max_neighbors: 1,
                max_logical_peak_bytes: usize::MAX,
                max_work: 9_999_999,
            }
        ),
        Err(KnnBatchPlanError::WorkLimit {
            required: 10_000_000,
            limit: 9_999_999
        })
    ));
}

#[test]
fn indexed_results_match_an_independent_brute_force_fixture() {
    let points = [
        [-2.0, 1.0, 0.0],
        [0.0, 0.0, 0.0],
        [2.0, -1.0, 3.0],
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0],
        [-1.0, -1.0, -1.0],
    ];
    let queries = [[0.0; 3], [1.0, 0.0, 1.0], [9.0, 9.0, 9.0]];
    let index = PointCloudIndex::new(&points).unwrap();
    let actual_radius = index
        .radius_neighbors_batch(&queries, 2.0, radius_limits())
        .unwrap();
    let actual_knn = index
        .knn_batch_planned(&queries, &knn_plan(points.len(), queries.len(), 3))
        .unwrap();

    let mut expected_radius = Vec::new();
    let mut expected_knn = Vec::new();
    for query in queries {
        let mut candidates = points
            .iter()
            .enumerate()
            .map(|(id, point)| {
                let dx = query[0] - point[0];
                let dy = query[1] - point[1];
                let dz = query[2] - point[2];
                (id as u64, dx * dx + dy * dy + dz * dz)
            })
            .collect::<Vec<_>>();
        candidates
            .sort_unstable_by(|left, right| left.1.total_cmp(&right.1).then(left.0.cmp(&right.0)));
        expected_radius.push(
            candidates
                .iter()
                .copied()
                .filter(|(_, distance)| *distance <= 4.0)
                .map(|(id, distance)| (id, distance.to_bits()))
                .collect::<Vec<_>>(),
        );
        expected_knn.push(
            candidates[..3]
                .iter()
                .map(|(id, distance)| (*id, distance.to_bits()))
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(rows(&actual_radius), expected_radius);
    assert_eq!(rows(&actual_knn), expected_knn);
}

#[test]
fn public_api_is_batch_only_and_batch_results_are_thread_identical() {
    // This compile-time use intentionally reaches only the batch entry points;
    // PointCloudIndex's Kiddo tree is private and no scalar query is exported.
    fn consume_batch_api(index: &PointCloudIndex) -> Result<NeighborCsr, PointQueryError> {
        index.radius_neighbors_batch(&[[0.0; 3], [1.0; 3]], 3.0, radius_limits())
    }

    if let Some(signatures) =
        sweep_threads("public_api_is_batch_only_and_batch_results_are_thread_identical")
    {
        assert!(signatures.windows(2).all(|pair| pair[0] == pair[1]));
        return;
    }
    let expected = std::env::var("PLINTH_THREADS")
        .expect("isolated sweep config")
        .parse::<usize>()
        .expect("positive isolated thread count");
    assert_eq!(geom_kernel::current_threads(), expected);

    let points = (0..4096)
        .map(|index| {
            let x = (index % 32) as f64;
            let y = ((index / 32) % 32) as f64;
            let z = (index / 1024) as f64;
            [x, y, z]
        })
        .collect::<Vec<_>>();
    let queries = (0..257)
        .map(|index| [index as f64 / 16.0, (index % 31) as f64, (index % 4) as f64])
        .collect::<Vec<_>>();
    let index = PointCloudIndex::new(&points).unwrap();
    let _ = consume_batch_api(&index).unwrap();

    let result = (
        signature(
            &index
                .radius_neighbors_batch(&queries, 5.0, radius_limits())
                .unwrap(),
        ),
        signature(
            &index
                .knn_batch_planned(&queries, &knn_plan(points.len(), queries.len(), 8))
                .unwrap(),
        ),
    );
    println!("\n{THREAD_SWEEP_SIGNATURE}{result:?}");
}

/// FU-8d (2026-08-06).  **Row 0 of a planned kNN batch does not depend on `k`.**
///
/// This pins the single riskiest claim in the FU-8d design: that the merge
/// gate's baseline coverage sweep can be raised from `k = 1` to
/// `k = min(3, source_count)` — so that it retains the witnesses a proposal
/// delta needs — **without moving a single answer**, because the answer every
/// caller reads is `squared_distances[row.start]`.
///
/// The argument is a property of `knn_batch_planned` itself, and it is
/// structural rather than statistical:
///
/// - the boundary is `max` over the `k` candidates Kiddo names, so it is
///   non-decreasing in `k`;
/// - the tie refetch is `nearest_n_within_exclusive(query, boundary,
///   tie_cap = source_count, …)` with the boundary **inclusive** and the cap at
///   the full source count, so a larger `k` can only *add* candidates — the
///   `k = 3` candidate set is a superset of the `k = 1` set;
/// - the row is then sorted by `(distance.total_cmp, item)`, a total order on
///   the whole set, so the first element of a superset that contains the first
///   element of the subset is the same element.
///
/// Element 0 is therefore bit-identical, ties or no ties.  Both halves of that
/// are exercised: an engineered cloud where every candidate distance is an
/// exact tie (so `item` alone breaks the order, and a larger `k` pulls in more
/// tied candidates), and pseudo-random clouds where it generally is not.
#[test]
fn fu8d_knn_row_zero_is_k_independent_bit_for_bit() {
    fn row_zero(index: &PointCloudIndex, queries: &[[f64; 3]], k: usize) -> Vec<(u64, u64)> {
        let result = index
            .knn_batch_planned(queries, &knn_plan(index.point_count(), queries.len(), k))
            .unwrap();
        (0..queries.len())
            .map(|query| {
                let row = result.row_range(query).unwrap();
                assert_eq!(row.end - row.start, k, "planned row width");
                (
                    result.point_ids[row.start],
                    result.squared_distances[row.start].to_bits(),
                )
            })
            .collect()
    }

    // (a) Engineered exact ties: sources on the axes of four shells about the
    // origin, so a query at the shell centre sees six sources at one distance,
    // then six more at the next.  `k = 1..6` all land inside the first tied
    // shell, which is precisely the case where the boundary refetch decides the
    // answer rather than the tree's own ordering.
    let mut tied = Vec::new();
    for shell in [1.0f64, 2.0, 3.0, 4.0] {
        for axis in 0..3 {
            for sign in [1.0f64, -1.0] {
                let mut point = [0.0f64; 3];
                point[axis] = sign * shell;
                tied.push(point);
            }
        }
    }
    let tied_index = PointCloudIndex::new(&tied).unwrap();
    let tied_queries = [[0.0, 0.0, 0.0], [0.0, 0.0, 0.25], [-0.125, 0.0625, 0.0]];
    // The fixture really is tied, or this case proves nothing.
    let first_row = tied_index
        .knn_batch_planned(&tied_queries[..1], &knn_plan(tied.len(), 1, 6))
        .unwrap();
    assert!(
        first_row.squared_distances[..6]
            .windows(2)
            .all(|pair| pair[0].to_bits() == pair[1].to_bits()),
        "engineered fixture must present exact distance ties at the boundary"
    );
    let tied_at_one = row_zero(&tied_index, &tied_queries, 1);
    for k in [2, 3, 4, 6] {
        assert_eq!(
            row_zero(&tied_index, &tied_queries, k),
            tied_at_one,
            "engineered ties: row 0 moved between k=1 and k={k}"
        );
    }

    // (b) Pseudo-random clouds, deterministic (a fixed 64-bit LCG, no crate
    // dependency and no wall-clock seed), over several cloud and batch shapes.
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((state >> 11) as f64) / ((1u64 << 53) as f64)
    };
    for (source_count, query_count) in [(3usize, 7usize), (17, 5), (64, 33), (257, 11)] {
        let sources = (0..source_count)
            .map(|_| [next() * 10.0, next() * 10.0, next() * 10.0])
            .collect::<Vec<_>>();
        let queries = (0..query_count)
            .map(|_| [next() * 10.0, next() * 10.0, next() * 10.0])
            .collect::<Vec<_>>();
        let index = PointCloudIndex::new(&sources).unwrap();
        let at_one = row_zero(&index, &queries, 1);
        for k in [2, 3] {
            assert_eq!(
                row_zero(&index, &queries, k),
                at_one,
                "random cloud {source_count}x{query_count}: row 0 moved between k=1 and k={k}"
            );
        }
    }
}
