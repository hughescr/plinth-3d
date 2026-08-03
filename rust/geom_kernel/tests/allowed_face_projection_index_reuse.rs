use geom_kernel::testkit::{build_from_tris, surface_index_construction_count};
use geom_kernel::{AllowedFaceProjectionLimits, AllowedFaceProjectionPolicy};

#[test]
fn allowed_projection_constructs_no_surface_or_index() {
    let surface = build_from_tris(vec![
        0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 2.0, 1.0, 0.0, 2.0, 0.0, 1.0, 2.0,
    ]);
    let before = surface_index_construction_count();
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
    assert_eq!(surface_index_construction_count(), before);
}
