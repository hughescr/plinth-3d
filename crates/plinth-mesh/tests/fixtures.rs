use plinth_mesh::{
    build_face_topology, certify_closed_components, load_binary_stl, make_box, make_box_default,
    make_dual_cavity_pawn, make_dual_cavity_pawn_with_throat_radius, make_overhang_table,
    make_raking_blade, make_thin_fin, make_toed_slab, mesh_fingerprint, write_binary_stl, Mesh,
    MeshError, WeldPolicy, DUAL_CAVITY_CAVITY_THROAT_AT_BORE_MM, DUAL_CAVITY_THROAT_ABOVE_BORE_MM,
    DUAL_CAVITY_THROAT_SEPARATED_MM, TOPOLOGY_CUT_DEGENERATE, TOPOLOGY_CUT_DUPLICATE,
    TOPOLOGY_CUT_NONMANIFOLD, TOPOLOGY_CUT_POINT_ONLY, TOPOLOGY_CUT_SAME_DIRECTION,
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
    assert_eq!(mesh.verts()[0], [2.0, 3.0, 7.0]);
    assert_eq!(mesh.verts()[7], [4.0, 7.0, 13.0]);
    assert_eq!(mesh.tris()[0], [1, 3, 0]);
    assert_eq!(mesh.tris()[11], [7, 5, 6]);
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
        assert_eq!(mesh.verts().len(), vertices, "{name} vertex count");
        assert_eq!(mesh.tris().len(), triangles, "{name} triangle count");
        let actual = mesh.bounds_z();
        assert!(
            (actual.0 - bounds.0).abs() < 1e-12,
            "{name} lower bound {actual:?}"
        );
        assert!(
            (actual.1 - bounds.1).abs() < 1e-12,
            "{name} upper bound {actual:?}"
        );
        assert_eq!(
            mesh.tris()[0],
            [1, 3, 0],
            "{name} changed first source face"
        );
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

// ---------------------------------------------------------------------------
// The E1 dual-cavity pawn.
// ---------------------------------------------------------------------------

/// The registered erosion `t = HOLLOW_WALL_DEFAULT_MM +
/// HOLLOW_CAVITY_MARCH_MARGIN_MM`, read live from the registry rather than
/// restated — the AT_BORE physical throat radius is `t + 1.45`.
fn registered_erosion_mm() -> f64 {
    let constants = plinth_core::m0_constants().expect("M0 constants");
    let placeholders = constants.placeholders();
    placeholders.hollow_wall_default_mm().diagnostic().get()
        + placeholders
            .hollow_cavity_march_margin_mm()
            .diagnostic()
            .get()
}

fn dual_cavity_settings() -> [(&'static str, f64); 3] {
    [
        ("separated", DUAL_CAVITY_THROAT_SEPARATED_MM),
        (
            "at_bore",
            registered_erosion_mm() + DUAL_CAVITY_CAVITY_THROAT_AT_BORE_MM,
        ),
        ("above_bore", DUAL_CAVITY_THROAT_ABOVE_BORE_MM),
    ]
}

fn xy_extent(mesh: &Mesh) -> ((f64, f64), (f64, f64)) {
    let mut x = (f64::INFINITY, f64::NEG_INFINITY);
    let mut y = (f64::INFINITY, f64::NEG_INFINITY);
    for vertex in mesh.verts() {
        x.0 = x.0.min(vertex[0]);
        x.1 = x.1.max(vertex[0]);
        y.0 = y.0.min(vertex[1]);
        y.1 = y.1.max(vertex[1]);
    }
    (x, y)
}

/// A1: the topology is frozen — same vertex/triangle counts and bounding box
/// at every throat setting, because the throat radius only ever moves where
/// two rings sit, never how many rings or slices exist.
#[test]
fn dual_cavity_pawn_has_frozen_counts_and_bounds() {
    for (name, throat_radius_mm) in dual_cavity_settings() {
        let mesh = make_dual_cavity_pawn_with_throat_radius(throat_radius_mm)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(mesh.verts().len(), 27_778, "{name} vertex count");
        assert_eq!(mesh.tris().len(), 55_552, "{name} triangle count");
        assert_eq!(mesh.bounds_z(), (0.0, 53.0), "{name} z bounds");
        // x/y bounds are close to, but not bit-exact at, +/-17.0: the lower
        // arc's LOWER_ARC_STEPS equal-theta sampling does not land a ring
        // exactly on the equator (theta = 90 degrees) for every throat
        // setting, so the faceted polygon's true max radius falls a hair
        // under the analytic sphere's 17.0 mm (a few hundredths of a
        // micron here, an order of magnitude under the module doc's own
        // 0.0016 mm azimuthal faceting bound). `low_x = low_y = -17.0`
        // bit-exact is not required for the fixture's lattice-phase
        // argument either -- only that a sample lands exactly at x = 0 and
        // y = 0, which `dual_cavity_pawn_is_a_single_closed_oriented_solid`
        // and the hollow-side doctrine tests both rely on and which does
        // hold exactly (slice 0 of every ring sits on the +x axis with
        // y = 0.0).
        let (x, y) = xy_extent(&mesh);
        const EXTENT_TOLERANCE_MM: f64 = 1e-3;
        assert!(
            (x.0 - -17.0).abs() < EXTENT_TOLERANCE_MM && (x.1 - 17.0).abs() < EXTENT_TOLERANCE_MM,
            "{name} x extent {x:?} not within {EXTENT_TOLERANCE_MM} mm of +/-17.0"
        );
        assert!(
            (y.0 - -17.0).abs() < EXTENT_TOLERANCE_MM && (y.1 - 17.0).abs() < EXTENT_TOLERANCE_MM,
            "{name} y extent {y:?} not within {EXTENT_TOLERANCE_MM} mm of +/-17.0"
        );
    }
    let default_mesh = make_dual_cavity_pawn().expect("default fixture");
    assert_eq!(default_mesh.verts().len(), 27_778);
    assert_eq!(default_mesh.tris().len(), 55_552);
}

/// A2: manifold and watertight by construction, at every throat setting —
/// exactly one closed, consistently oriented face component, no boundary
/// edges, and no face carrying any closed-solid-gate cut flag.
#[test]
fn dual_cavity_pawn_is_a_single_closed_oriented_solid() {
    let gate_mask: u8 = TOPOLOGY_CUT_NONMANIFOLD
        | TOPOLOGY_CUT_POINT_ONLY
        | TOPOLOGY_CUT_DUPLICATE
        | TOPOLOGY_CUT_SAME_DIRECTION
        | TOPOLOGY_CUT_DEGENERATE;
    for (name, throat_radius_mm) in dual_cavity_settings() {
        let mesh = make_dual_cavity_pawn_with_throat_radius(throat_radius_mm)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(topology.counts().boundary_edges, 0, "{name} boundary edges");
        assert!(
            topology.cut_flags().iter().all(|flags| *flags & gate_mask == 0),
            "{name}: a closed-solid-gate cut flag survived construction"
        );
        let certificate = certify_closed_components(&mesh, &topology)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            certificate.components().len(),
            1,
            "{name}: expected exactly one closed component"
        );
        assert!(
            certificate.components()[0].signed_volume_mm3() > 0.0,
            "{name}: expected outward winding"
        );
    }
}

/// A3: the mandatory f32 snap does not weld two vertices or create a
/// zero-area face — proved, not argued.
#[test]
fn dual_cavity_pawn_has_no_degenerate_face_after_the_f32_snap() {
    for (name, throat_radius_mm) in dual_cavity_settings() {
        let mesh = make_dual_cavity_pawn_with_throat_radius(throat_radius_mm)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        mesh.audit_strict()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}

/// A4: outward-winding positive volume, within 0.5 % of the analytic
/// reference volume at the default (ABOVE_BORE) setting — see the fixture
/// doc's derivation in `guaranteedHollow`.
#[test]
fn dual_cavity_pawn_encloses_positive_volume() {
    let mesh = make_dual_cavity_pawn().expect("default fixture");
    let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).expect("topology");
    let certificate = certify_closed_components(&mesh, &topology).expect("certificate");
    let volume = certificate.signed_volume_mm3();
    assert!(volume > 0.0, "expected outward winding, got {volume}");
    const REFERENCE_MM3: f64 = 25_644.2;
    let relative_error = (volume - REFERENCE_MM3).abs() / REFERENCE_MM3;
    assert!(
        relative_error < 0.005,
        "volume {volume} is not within 0.5% of the analytic reference {REFERENCE_MM3}"
    );
}

/// A5: within-run reproducibility. Two builds produce byte-identical arrays,
/// identical fingerprints, and identical STL bytes on disk. This is
/// deliberately **not** a cross-machine sha256 pin — `sin`/`cos` are
/// libm-dependent, so the honest claim is within-run reproducibility plus the
/// f32 snap that bounds any cross-platform drift below the STL wire
/// resolution.
#[test]
fn dual_cavity_pawn_is_byte_identical_across_runs() {
    let first = make_dual_cavity_pawn().expect("first build");
    let second = make_dual_cavity_pawn().expect("second build");
    assert_eq!(first.verts(), second.verts());
    assert_eq!(first.tris(), second.tris());
    assert_eq!(
        mesh_fingerprint(&first).expect("fingerprint"),
        mesh_fingerprint(&second).expect("fingerprint")
    );

    let dir = std::env::temp_dir();
    let path_a = dir.join(format!(
        "plinth-dual-cavity-{}-a.stl",
        std::process::id()
    ));
    let path_b = dir.join(format!(
        "plinth-dual-cavity-{}-b.stl",
        std::process::id()
    ));
    write_binary_stl(&path_a, &first).expect("write a");
    write_binary_stl(&path_b, &second).expect("write b");
    let bytes_a = std::fs::read(&path_a).expect("read a");
    let bytes_b = std::fs::read(&path_b).expect("read b");
    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);
    assert_eq!(bytes_a, bytes_b, "STL bytes differ across identical builds");
}

/// A6: the fixture is exactly f32-representable — `write_binary_stl` (the
/// strict writer) accepts it outright, and the reloaded triangle soup
/// expands to the same coordinates the source mesh named.
#[test]
fn dual_cavity_pawn_survives_the_stl_wire_round_trip() {
    let mesh = make_dual_cavity_pawn().expect("default fixture");
    let path = std::env::temp_dir().join(format!(
        "plinth-dual-cavity-wire-{}.stl",
        std::process::id()
    ));
    write_binary_stl(&path, &mesh).expect("strict writer accepts an f32-exact fixture");
    let reloaded = load_binary_stl(&path, WeldPolicy::None).expect("reload");
    let _ = std::fs::remove_file(&path);
    assert_eq!(reloaded.tris().len(), mesh.tris().len());
    for (triangle, indices) in mesh.tris().iter().enumerate() {
        for (corner, &index) in indices.iter().enumerate() {
            let source_vertex = mesh.verts()[index as usize];
            let reloaded_vertex = reloaded.verts()[triangle * 3 + corner];
            assert_eq!(
                source_vertex, reloaded_vertex,
                "triangle {triangle} corner {corner} did not round-trip"
            );
        }
    }
}

/// A7: the throat radius is validated up front. `0.0`, negative, `NaN`, and
/// infinity are all non-positive-or-non-finite; `DUAL_CAVITY_UPPER_SPHERE_R_MM`
/// and past it are refused by the added upper-bound domain (FU-8b: an added
/// bound, not a loosened one — there was no upper bound to loosen).
#[test]
fn dual_cavity_pawn_refuses_an_inadmissible_throat() {
    for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(
            matches!(
                make_dual_cavity_pawn_with_throat_radius(value),
                Err(MeshError::InvalidPositiveFixtureParameter { .. })
            ),
            "throat_radius_mm = {value} should be refused as non-positive/non-finite"
        );
    }
    for value in [11.0, 12.0] {
        assert!(
            matches!(
                make_dual_cavity_pawn_with_throat_radius(value),
                Err(MeshError::InvalidRangeFixtureParameter { .. })
            ),
            "throat_radius_mm = {value} should be refused as out of range"
        );
    }
}
