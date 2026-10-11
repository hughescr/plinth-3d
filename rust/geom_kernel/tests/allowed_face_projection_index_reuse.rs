//! Kept as its own test program on purpose. `index_construction_attempt_counts`
//! reads process-wide counters, so any other test that builds a surface in the
//! same process (tests run on parallel threads) lands inside this test's
//! before/after window. A process of its own, with a single test, has no such
//! neighbour. Do not add a second test to this file.

use geom_kernel::testkit::{
    build_from_tris, build_surface_budget, grid_mesh, index_construction_attempt_counts,
};
use geom_kernel::{AllowedFaceProjectionLimits, AllowedFaceProjectionPolicy};

#[test]
fn allowed_projection_constructs_no_surface_or_index() {
    let (bvh_initial, parity_initial) = index_construction_attempt_counts();
    let surface = build_from_tris(vec![
        0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 2.0, 1.0, 0.0, 2.0, 0.0, 1.0, 2.0,
    ]);
    let (bvh_before, parity_before) = index_construction_attempt_counts();
    assert_eq!(bvh_before, bvh_initial + 1);
    assert_eq!(parity_before, parity_initial + 3);
    let outcomes = surface
        .project_allowed_faces_batch(
            &[1],
            &[[0.2, 0.2, 0.1], [0.4, 0.4, 1.9]],
            AllowedFaceProjectionPolicy {
                max_distance_mm: None,
            },
            AllowedFaceProjectionLimits {
                max_points: 2,
                max_allowed_faces: 1,
                max_total_node_visits: usize::MAX,
                max_total_triangle_tests: 2,
                max_memory_bytes: usize::MAX,
            },
        )
        .unwrap();
    assert_eq!(outcomes.len(), 2);
    let (bvh_after, parity_after) = index_construction_attempt_counts();
    assert_eq!(bvh_after, bvh_before, "projection rebuilt a BVH");
    assert_eq!(
        parity_after, parity_before,
        "projection rebuilt a parity axis index"
    );

    // A failed NativeSurface construction still crosses the actual BVH and
    // first parity-axis boundaries. This proves the counters cover transient
    // attempts instead of counting only retained surfaces.
    assert!(build_surface_budget(&grid_mesh(1), 0).is_err());
    let (bvh_failed, parity_failed) = index_construction_attempt_counts();
    assert_eq!(bvh_failed, bvh_after + 1);
    assert_eq!(parity_failed, parity_after + 1);
}
