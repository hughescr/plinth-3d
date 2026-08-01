use plinth_mesh::{Mesh, StrictMeshAuditError};

const MIN_PREDICATE: f64 = f64::from_bits(623_u64 << 52); // 2^-400
const MAX_PREDICATE: f64 = f64::from_bits(1423_u64 << 52); // 2^400
const TOO_SMALL: f64 = f64::from_bits(622_u64 << 52); // 2^-401
const TOO_LARGE: f64 = f64::from_bits(1424_u64 << 52); // 2^401

fn one_face(vertices: Vec<[f64; 3]>) -> Mesh {
    Mesh::new(vertices, vec![[0, 1, 2]]).expect("finite, indexed raw soup remains constructible")
}

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
        vec![[0.0, 0.0, 0.0], [1.0, 3.0, 0.0], [15.0, 45.0, 0.0]],
    ] {
        assert_eq!(
            one_face(vertices).audit_strict(),
            Err(StrictMeshAuditError::ZeroAreaOrCollinear { triangle: 0 })
        );
    }
}

#[test]
fn strict_audit_uses_adaptive_predicate_for_near_collinearity() {
    let epsilon = f64::EPSILON;
    let a = [1.0 + epsilon, 1.0, 0.0];
    let b = [1.0 + 2.0 * epsilon, 1.0 + epsilon, 0.0];
    let c = [0.0, 0.0, 0.0];
    let naive = (a[0] - c[0]) * (b[1] - c[1]) - (a[1] - c[1]) * (b[0] - c[0]);
    assert_eq!(naive, 0.0, "fixture must defeat the naive determinant");
    assert_eq!(one_face(vec![a, b, c]).audit_strict(), Ok(()));
}

#[test]
fn each_projection_can_independently_prove_nonzero_area() {
    for vertices in [
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]],
    ] {
        assert_eq!(one_face(vertices).audit_strict(), Ok(()));
    }
}

#[test]
fn predicate_envelope_boundaries_pass_and_outside_values_name_the_first_coordinate() {
    for side in [MIN_PREDICATE, MAX_PREDICATE] {
        assert_eq!(
            one_face(vec![[0.0, 0.0, 0.0], [side, 0.0, 0.0], [0.0, side, 0.0]]).audit_strict(),
            Ok(())
        );
    }
    for side in [TOO_SMALL, TOO_LARGE] {
        assert_eq!(
            one_face(vec![[0.0, 0.0, 0.0], [side, 0.0, 0.0], [0.0, side, 0.0]]).audit_strict(),
            Err(StrictMeshAuditError::PredicateOutOfDomain {
                triangle: 0,
                corner: 1,
                axis: 0,
                value: side,
            })
        );
    }
}

#[test]
fn promoted_f32_extremes_stay_inside_the_predicate_envelope() {
    for side in [f32::from_bits(1) as f64, f32::MAX as f64] {
        assert_eq!(
            one_face(vec![[0.0, 0.0, 0.0], [side, 0.0, 0.0], [0.0, side, 0.0]]).audit_strict(),
            Ok(())
        );
    }
}

#[test]
fn unsafe_earlier_projection_does_not_hide_later_safe_nonzero_projection() {
    assert_eq!(
        one_face(vec![
            [TOO_LARGE, 0.0, 0.0],
            [TOO_LARGE, 1.0, 0.0],
            [TOO_LARGE, 0.0, 1.0],
        ])
        .audit_strict(),
        Ok(())
    );
}

#[test]
fn min_subnormal_and_max_counterexample_is_domain_error_not_collinearity() {
    assert_eq!(
        one_face(vec![
            [0.0, 0.0, 0.0],
            [f64::MAX, 0.0, 0.0],
            [0.0, f64::from_bits(1), 0.0],
        ])
        .audit_strict(),
        Err(StrictMeshAuditError::PredicateOutOfDomain {
            triangle: 0,
            corner: 1,
            axis: 0,
            value: f64::MAX,
        })
    );
}

#[test]
fn strict_audit_names_unrepresentable_derived_arithmetic_before_domain_checks() {
    assert_eq!(
        one_face(vec![
            [-f64::MAX, 0.0, 0.0],
            [f64::MAX, 0.0, 0.0],
            [-f64::MAX, 1.0, 0.0],
        ])
        .audit_strict(),
        Err(StrictMeshAuditError::DerivedArithmeticOverflow { triangle: 0 })
    );
}

#[test]
fn strict_audit_leaves_signed_zero_source_order_and_bits_unchanged() {
    let vertices = vec![
        [-0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
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
