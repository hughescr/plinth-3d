use plinth_metrics::{
    analyze_topology, boundary_edges, closest_points, contains_points, ray_intersections,
    signed_shell_volumes, triangle_intersections, Mesh, MetricsError, Ray,
};

fn tetrahedron(flipped: bool) -> Mesh {
    let vertices = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ];
    let mut triangles = vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]];
    if flipped {
        for triangle in &mut triangles {
            triangle.swap(1, 2);
        }
    }
    Mesh::new(vertices, triangles).unwrap()
}

fn cube() -> Mesh {
    let vertices = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ];
    Mesh::new(
        vertices,
        vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ],
    )
    .unwrap()
}

#[test]
fn closest_point_is_analytic_and_batch_ordered() {
    let mesh = Mesh::new(
        vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
        vec![[0, 1, 2]],
    )
    .unwrap();
    let nearest = closest_points(&mesh, &[[0.25, 0.25, 3.0], [3.0, 0.0, 0.0]]).unwrap();
    assert_eq!(nearest.len(), 2);
    assert_eq!(nearest[0].point, [0.25, 0.25, 0.0]);
    assert_eq!(nearest[0].squared_distance, 9.0);
    assert_eq!(nearest[1].point, [2.0, 0.0, 0.0]);
    assert_eq!(nearest[1].squared_distance, 1.0);
}

#[test]
fn parity_containment_votes_across_a_batch() {
    let mesh = cube();
    assert_eq!(
        contains_points(&mesh, &[[0.5, 0.5, 0.5], [2.0, 0.5, 0.5], [0.0, 0.5, 0.5]]).unwrap(),
        vec![true, false, true]
    );
    let hits = ray_intersections(
        &mesh,
        &[Ray {
            origin: [-1.0, 0.5, 0.5],
            direction: [1.0, 0.0, 0.0],
        }],
    )
    .unwrap();
    assert_eq!(
        hits[0].len(),
        4,
        "two triangles per cube face are retained before parity deduplication"
    );
}

#[test]
fn topology_detects_boundary_nonmanifold_and_winding() {
    let open = Mesh::new(
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        vec![[0, 1, 2]],
    )
    .unwrap();
    assert_eq!(boundary_edges(&open).len(), 3);

    let nonmanifold = Mesh::new(
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
        ],
        vec![[0, 1, 2], [1, 0, 3], [0, 1, 4]],
    )
    .unwrap();
    assert_eq!(analyze_topology(&nonmanifold).nonmanifold_edges.len(), 1);

    let inconsistent = Mesh::new(
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        vec![[0, 1, 2], [0, 1, 2]],
    )
    .unwrap();
    assert_eq!(
        analyze_topology(&inconsistent).inconsistent_winding_faces,
        vec![0, 1]
    );
}

#[test]
fn signed_volume_and_triangle_intersection_catch_wrong_sign_and_stubs() {
    let positive = signed_shell_volumes(&tetrahedron(false));
    assert_eq!(positive.len(), 1);
    assert!((positive[0].signed_volume - (1.0 / 6.0)).abs() < 1.0e-12);
    assert!(signed_shell_volumes(&tetrahedron(true))[0].signed_volume < 0.0);

    let mesh = Mesh::new(
        vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [0.0, 2.0, 0.0],
            [0.5, 0.5, 0.0],
            [2.5, 0.5, 0.0],
            [0.5, 2.5, 0.0],
        ],
        vec![[0, 1, 2], [3, 4, 5]],
    )
    .unwrap();
    assert_eq!(
        triangle_intersections(&mesh, &[(0, 1)]).unwrap(),
        vec![true]
    );
}

#[test]
fn rejects_nonfinite_degenerate_and_invalid_ray_data() {
    assert!(matches!(
        Mesh::new(vec![[f64::NAN, 0.0, 0.0]], vec![[0, 0, 0]]),
        Err(MetricsError::NonFiniteCoordinate { .. })
    ));
    assert!(matches!(
        Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            vec![[0, 1, 2]]
        ),
        Err(MetricsError::DegenerateTriangle { .. })
    ));
    let mesh = tetrahedron(false);
    assert!(matches!(
        ray_intersections(
            &mesh,
            &[Ray {
                origin: [0.0, 0.0, 0.0],
                direction: [0.0, 0.0, 0.0]
            }]
        ),
        Err(MetricsError::ZeroRayDirection { .. })
    ));
    assert!(matches!(
        triangle_intersections(&mesh, &[(0, 8)]),
        Err(MetricsError::QueryTriangleIndexOutOfBounds { .. })
    ));
}
