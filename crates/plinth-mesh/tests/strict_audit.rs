use plinth_mesh::{Mesh, StrictMeshAuditError};

#[test]
fn constructor_preserves_repeated_indices_but_strict_audit_names_each_pair() {
    for (indices, first_corner, second_corner, vertex) in [
        ([0, 0, 1], 0, 1, 0),
        ([0, 1, 1], 1, 2, 1),
        ([0, 1, 0], 2, 0, 0),
    ] {
        let mesh = Mesh::new(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]], vec![indices])
            .expect("finite, indexed raw soup remains constructible");
        assert_eq!(
            mesh.audit_strict(),
            Err(StrictMeshAuditError::RepeatedVertexIndex {
                triangle: 0,
                first_corner,
                second_corner,
                vertex,
            })
        );
    }
}

#[test]
fn strict_audit_rejects_distinct_index_geometric_degeneracy() {
    for vertices in [
        vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [2.0, 2.0, 2.0]],
        vec![[0.0, 0.0, 0.0], [1.0, 2.0, 3.0], [0.0, 0.0, 0.0]],
    ] {
        let mesh = Mesh::new(vertices, vec![[0, 1, 2]])
            .expect("distinct indices and finite coordinates are raw soup");
        assert_eq!(
            mesh.audit_strict(),
            Err(StrictMeshAuditError::ZeroAreaOrCollinear { triangle: 0 })
        );
    }
}

#[test]
fn strict_audit_is_scale_safe_for_tiny_and_huge_valid_triangles() {
    let tiny = f64::from_bits(1);
    let tiny_mesh = Mesh::new(
        vec![[0.0, 0.0, 0.0], [tiny, 0.0, 0.0], [0.0, tiny, 0.0]],
        vec![[0, 1, 2]],
    )
    .expect("subnormal coordinates are finite raw soup");
    assert_eq!(tiny_mesh.audit_strict(), Ok(()));

    let huge_mesh = Mesh::new(
        vec![[0.0, 0.0, 0.0], [f64::MAX, 0.0, 0.0], [0.0, f64::MAX, 0.0]],
        vec![[0, 1, 2]],
    )
    .expect("huge finite coordinates are raw soup");
    assert_eq!(huge_mesh.audit_strict(), Ok(()));
}

#[test]
fn strict_audit_names_unrepresentable_derived_arithmetic() {
    let mesh = Mesh::new(
        vec![
            [-f64::MAX, 0.0, 0.0],
            [f64::MAX, 0.0, 0.0],
            [-f64::MAX, 1.0, 0.0],
        ],
        vec![[0, 1, 2]],
    )
    .expect("finite endpoints are constructible raw soup");
    assert_eq!(
        mesh.audit_strict(),
        Err(StrictMeshAuditError::DerivedArithmeticOverflow { triangle: 0 })
    );
}

#[test]
fn strict_audit_leaves_valid_source_order_and_bits_unchanged() {
    let vertices = vec![
        [-0.0, 0.0, 0.0],
        [1.0, f64::from_bits(1), 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ];
    let triangles = vec![[2, 0, 1], [3, 2, 1]];
    let mesh = Mesh::new(vertices, triangles).expect("valid raw soup");
    let before_vertices = mesh
        .verts()
        .iter()
        .map(|point| point.map(f64::to_bits))
        .collect::<Vec<_>>();
    let before_triangles = mesh.tris().to_vec();

    assert_eq!(mesh.audit_strict(), Ok(()));
    assert_eq!(
        mesh.verts()
            .iter()
            .map(|point| point.map(f64::to_bits))
            .collect::<Vec<_>>(),
        before_vertices
    );
    assert_eq!(mesh.tris(), before_triangles);
}
