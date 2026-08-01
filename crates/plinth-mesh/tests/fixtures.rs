use std::path::Path;

use plinth_mesh::{
    load_binary_stl, make_box, make_box_default, make_overhang_table, make_raking_blade,
    make_thin_fin, make_toed_slab, Mesh, MeshError, WeldPolicy,
};

type FixtureExpectation = (
    &'static str,
    Result<Mesh, MeshError>,
    usize,
    usize,
    (f64, f64),
);

#[test]
fn box_matches_frozen_trimesh_order_and_bottom_translation() {
    let mesh = make_box([2.0, 4.0, 6.0], [3.0, 5.0, 7.0]).expect("valid box");
    assert_eq!(mesh.verts[0], [2.0, 3.0, 7.0]);
    assert_eq!(mesh.verts[7], [4.0, 7.0, 13.0]);
    assert_eq!(mesh.tris[0], [1, 3, 0]);
    assert_eq!(mesh.tris[11], [7, 5, 6]);
    assert_eq!(mesh.bounds_z(), (7.0, 13.0));
    assert_eq!(mesh.weld_policy(), WeldPolicy::None);
}

#[test]
fn native_fixture_shapes_have_frozen_counts_bounds_and_face_order() {
    let fixtures: [FixtureExpectation; 6] = [
        ("box", make_box_default(), 8, 12, (0.0, 10.0)),
        ("table", make_overhang_table(), 40, 60, (0.0, 24.0)),
        ("toed slab", make_toed_slab(), 76, 140, (9.0, 19.0)),
        (
            "raking blade",
            make_raking_blade(),
            16,
            24,
            (9.549_278_365_790_332, 14.0),
        ),
        ("thin fin", make_thin_fin(), 16, 24, (0.0, 19.0)),
        (
            "another default box",
            make_box_default(),
            8,
            12,
            (0.0, 10.0),
        ),
    ];
    for (name, mesh, vertices, triangles, bounds) in fixtures {
        let mesh = mesh.unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(mesh.verts.len(), vertices, "{name} vertex count");
        assert_eq!(mesh.tris.len(), triangles, "{name} triangle count");
        let actual = mesh.bounds_z();
        assert!(
            (actual.0 - bounds.0).abs() < 1e-12,
            "{name} lower bound {actual:?}"
        );
        assert!(
            (actual.1 - bounds.1).abs() < 1e-12,
            "{name} upper bound {actual:?}"
        );
        assert_eq!(mesh.tris[0], [1, 3, 0], "{name} changed first source face");
    }
}

#[test]
fn face_normal_z_is_batched_and_reports_degenerate_triangles() {
    let mesh = make_box_default().expect("valid box");
    let normals = mesh.face_normals_z().expect("box normals");
    assert_eq!(normals.len(), 12);
    assert_eq!(normals[3], -1.0);
    assert_eq!(normals[4], 1.0);
    let degenerate = Mesh::new(
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
        vec![[0, 1, 2]],
    )
    .expect("index-valid degenerate mesh is a legal raw soup");
    assert!(matches!(
        degenerate.face_normals_z(),
        Err(MeshError::DegenerateTriangle { triangle: 0 })
    ));
}

#[test]
fn mesh_and_fixture_input_validation_is_typed() {
    assert!(matches!(
        Mesh::new(vec![[f64::NAN, 0.0, 0.0]], vec![[0, 0, 0]]),
        Err(MeshError::NonFiniteVertex { .. })
    ));
    assert!(matches!(
        Mesh::new(vec![[0.0, 0.0, 0.0]], vec![[1, 0, 0]]),
        Err(MeshError::IndexOutOfBounds { .. })
    ));
    assert!(matches!(
        make_box([0.0, 1.0, 1.0], [0.0; 3]),
        Err(MeshError::InvalidPositiveFixtureParameter {
            parameter: "size.x",
            ..
        })
    ));
}

#[test]
fn native_synthetic_builders_match_frozen_stl_geometry_and_triangle_order() {
    let corpus = workspace_root().join("corpus/models");
    let fixtures: [(&str, Result<Mesh, MeshError>); 5] = [
        ("make_box.stl", make_box_default()),
        ("make_overhang_table.stl", make_overhang_table()),
        ("make_toed_slab.stl", make_toed_slab()),
        ("make_raking_blade.stl", make_raking_blade()),
        ("make_thin_fin.stl", make_thin_fin()),
    ];
    for (name, native) in fixtures {
        let path = corpus.join(name);
        assert!(
            path.is_file(),
            "required synthetic fixture missing: {}",
            path.display()
        );
        let frozen = load_binary_stl(&path, WeldPolicy::None)
            .unwrap_or_else(|error| panic!("could not load {}: {error}", path.display()));
        assert_builder_matches_stl(&native.expect("valid native fixture"), &frozen, name);
    }
}

fn assert_builder_matches_stl(native: &Mesh, frozen: &Mesh, name: &str) {
    assert_eq!(
        native.tris.len(),
        frozen.tris.len(),
        "{name} triangle count differs from frozen Python fixture"
    );
    for (face, indices) in native.tris.iter().enumerate() {
        for (corner, index) in indices.iter().enumerate() {
            let actual = native.verts[*index as usize];
            let expected = frozen.verts[face * 3 + corner];
            for axis in 0..3 {
                assert_eq!(
                    f64::from(actual[axis] as f32).to_bits(),
                    expected[axis].to_bits(),
                    "{name} face {face} corner {corner} axis {axis} differs from frozen Python STL"
                );
            }
        }
    }
}

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate has workspace parent")
        .to_path_buf()
}
