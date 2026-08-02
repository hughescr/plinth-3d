use std::sync::{Mutex, OnceLock};

use geom_kernel::{NeighborCsr, PointCloudIndex, PointIndexBuildError, PointQueryError};

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

    let at = index.radius_neighbors_batch(&[[0.0; 3]], 1.0).unwrap();
    assert_eq!(at.offsets, vec![0, 2]);
    assert_eq!(at.point_ids, vec![0, 1]);

    let just_below = index.radius_neighbors_batch(&[[0.0; 3]], below).unwrap();
    assert_eq!(just_below.point_ids, vec![0]);

    let just_above = index.radius_neighbors_batch(&[[0.0; 3]], above).unwrap();
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
        .radius_neighbors_batch(&[[0.0; 3], [10.0, 0.0, 0.0], [0.0, 2.0, 0.0]], 1.0)
        .unwrap();

    assert_eq!(result.offsets, vec![0, 3, 3, 3]);
    assert_eq!(result.point_ids, vec![0, 1, 2]);
    assert_eq!(result.squared_distances, vec![1.0, 1.0, 1.0]);
    assert_eq!(result.row_range(1), Some(3..3));
    assert_eq!(result.row_range(3), None);
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
        match index.radius_neighbors_batch(&[], radius) {
            Err(PointQueryError::InvalidRadius { radius: got }) => {
                assert_eq!(got.to_bits(), radius.to_bits());
            }
            _ => panic!("invalid radius unexpectedly accepted"),
        }
    }
    assert_eq!(
        index.radius_neighbors_batch(&[[0.0, f64::NEG_INFINITY, 0.0]], 1.0),
        Err(PointQueryError::NonFiniteQuery { query: 0, axis: 1 })
    );
}

#[test]
fn knn_has_typed_bounds_and_global_id_tie_order() {
    let index = PointCloudIndex::new(&[
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
    ])
    .unwrap();
    assert_eq!(
        index.knn_batch(&[[0.0; 3]], 0),
        Err(PointQueryError::ZeroNeighborsRequested)
    );
    assert_eq!(
        index.knn_batch(&[[0.0; 3]], 5),
        Err(PointQueryError::RequestedNeighborsExceedPointCount {
            requested: 5,
            point_count: 4,
        })
    );
    assert_eq!(
        rows(&index.knn_batch(&[[0.0; 3]], 2).unwrap()),
        vec![vec![(0, 1.0f64.to_bits()), (1, 1.0f64.to_bits())]]
    );
    assert_eq!(
        rows(&index.knn_batch(&[[0.0; 3]], 4).unwrap()),
        vec![vec![
            (0, 1.0f64.to_bits()),
            (1, 1.0f64.to_bits()),
            (2, 1.0f64.to_bits()),
            (3, 4.0f64.to_bits()),
        ]]
    );
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
    let actual_radius = index.radius_neighbors_batch(&queries, 2.0).unwrap();
    let actual_knn = index.knn_batch(&queries, 3).unwrap();

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
        index.radius_neighbors_batch(&[[0.0; 3], [1.0; 3]], 3.0)
    }

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let previous = std::env::var_os("PLINTH_THREADS");
    struct Restore(Option<std::ffi::OsString>);
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Some(value) = self.0.take() {
                std::env::set_var("PLINTH_THREADS", value);
            } else {
                std::env::remove_var("PLINTH_THREADS");
            }
        }
    }
    let _restore = Restore(previous);

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

    let mut baseline = None;
    for threads in [1, 4, 16] {
        std::env::set_var("PLINTH_THREADS", threads.to_string());
        let current = signature(&index.radius_neighbors_batch(&queries, 5.0).unwrap());
        let current_knn = signature(&index.knn_batch(&queries, 8).unwrap());
        if let Some((previous_radius, previous_knn)) = &baseline {
            assert_eq!(&current, previous_radius);
            assert_eq!(&current_knn, previous_knn);
        } else {
            baseline = Some((current, current_knn));
        }
    }
}
