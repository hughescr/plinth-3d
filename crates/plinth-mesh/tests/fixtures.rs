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

/// The exact `|x|` (and `|y|`) extent the generator's own sampling produces,
/// **derived** rather than tolerated.
///
/// The extreme lies on whichever lower-arc ring sits closest to the sphere's
/// equator, at that ring's own radius `R1 * sin(theta)`, shrunk by nothing
/// azimuthally because slice 0 of every ring sits exactly on the `+x` axis.
/// So the answer is `R1 * sin(theta_k)` maximized over the same equal-theta
/// sampling `make_dual_cavity_pawn_with_throat_radius` walks — no ring lands
/// exactly on `theta = pi/2`, which is precisely why this is a computed
/// figure and not `DUAL_CAVITY_LOWER_SPHERE_R_MM`.
fn expected_xy_extent_mm(throat_radius_mm: f64) -> f64 {
    let r1 = plinth_mesh::DUAL_CAVITY_LOWER_SPHERE_R_MM;
    let z1 = plinth_mesh::DUAL_CAVITY_LOWER_SPHERE_Z_MM;
    let steps = plinth_mesh::DUAL_CAVITY_LOWER_ARC_STEPS;
    let (z_a, _z_b) = plinth_mesh::dual_cavity_junction_heights_mm(throat_radius_mm);
    let theta_foot = (-z1 / r1).acos();
    let theta_a = ((z_a - z1) / r1).acos();
    (1..steps)
        .map(|k| {
            let t = k as f64 / steps as f64;
            let theta = theta_foot + (theta_a - theta_foot) * t;
            r1 * theta.sin()
        })
        .fold(0.0_f64, f64::max)
}

/// A1: the topology is frozen — same vertex/triangle counts and bounding box
/// at every throat setting, because the throat radius only ever moves where
/// two rings sit, never how many rings or slices exist.
#[test]
fn dual_cavity_pawn_has_frozen_counts_and_bounds() {
    let r1 = plinth_mesh::DUAL_CAVITY_LOWER_SPHERE_R_MM;
    let top_z = plinth_mesh::DUAL_CAVITY_LOWER_SPHERE_Z_MM
        + plinth_mesh::DUAL_CAVITY_CENTRE_SPACING_MM
        + plinth_mesh::DUAL_CAVITY_UPPER_SPHERE_R_MM;
    for (name, throat_radius_mm) in dual_cavity_settings() {
        let mesh = make_dual_cavity_pawn_with_throat_radius(throat_radius_mm)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(mesh.verts().len(), 6_978, "{name} vertex count");
        assert_eq!(mesh.tris().len(), 13_952, "{name} triangle count");
        assert_eq!(mesh.bounds_z(), (0.0, top_z), "{name} z bounds");

        // The x/y extents are the *inscribed* polygon's, not the analytic
        // sphere's: no lower-arc ring lands exactly on the equator. Rather
        // than tolerate that gap with a hand-picked epsilon, compute exactly
        // where the sampling puts the extreme and assert against it -- so a
        // change to DUAL_CAVITY_LOWER_ARC_STEPS or the silhouette moves both
        // sides of the comparison together instead of silently eating into
        // an epsilon nobody derived. The only slack left is the mandatory
        // f32 snap, one ulp at this magnitude.
        let expected = expected_xy_extent_mm(throat_radius_mm);
        let snap_ulp_mm = f64::from(f32::EPSILON) * r1;
        let (x, y) = xy_extent(&mesh);
        for (axis, (low, high)) in [("x", x), ("y", y)] {
            assert!(
                (high - expected).abs() <= snap_ulp_mm && (low + expected).abs() <= snap_ulp_mm,
                "{name} {axis} extent ({low}, {high}) is not the computed inscribed extent \
                 +/-{expected} (f32 snap slack {snap_ulp_mm})"
            );
        }
        // ... and that extent really is a hair under the analytic radius, in
        // the direction an inscribed polygon must miss.
        assert!(
            expected < r1 && r1 - expected < 0.01,
            "{name}: the sampled extent {expected} is not just inside the silhouette radius {r1}"
        );
    }
    let default_mesh = make_dual_cavity_pawn().expect("default fixture");
    assert_eq!(default_mesh.verts().len(), 6_978);
    assert_eq!(default_mesh.tris().len(), 13_952);
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

/// A4: outward-winding positive volume, and — at every throat setting — the
/// **signed** relationship to the exact analytic solid the lathe approximates.
///
/// The mesh is strictly inscribed in that solid (ring vertices on the exact
/// surfaces, chords inside a profile concave in `z`, each ring's regular
/// polygon inside its own circle), so the deficit must be positive and must
/// be no larger than the two faceting terms that produce it:
/// - azimuthal, `1 - sinc(pi / SLICES)`, exact for the polygon-versus-circle
///   cross-section;
/// - polar, bounded by the largest chord sagitta over the profile relative to
///   the silhouette radius.
///
/// Asserting the *sign* is what a bare "within 0.5 %" band could not do: a
/// mesh 0.4 % too **large** is not an inscribed lathe at all, and the earlier
/// version of this test — which compared against a reference that
/// double-counted both spheres' cut-off caps — would have accepted it.
#[test]
fn dual_cavity_pawn_encloses_positive_volume() {
    let slices = plinth_mesh::DUAL_CAVITY_SLICES as f64;
    let azimuthal_deficit = 1.0 - (std::f64::consts::TAU / slices).sin() * slices / std::f64::consts::TAU;
    // The polar chord deficit: the coarsest arc step is the upper arc's, and
    // a chord's sagitta on a circle of radius R across an angle `d` is
    // `R (1 - cos(d/2))`, a relative volume loss under `2 (1 - cos(d/2))`.
    let upper_step = std::f64::consts::PI / plinth_mesh::DUAL_CAVITY_UPPER_ARC_STEPS as f64;
    let polar_deficit = 2.0 * (1.0 - (upper_step / 2.0).cos());
    let deficit_ceiling = azimuthal_deficit + polar_deficit;

    for (name, throat_radius_mm) in dual_cavity_settings() {
        let mesh = make_dual_cavity_pawn_with_throat_radius(throat_radius_mm)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).expect("topology");
        let certificate = certify_closed_components(&mesh, &topology).expect("certificate");
        let volume = certificate.signed_volume_mm3();
        assert!(volume > 0.0, "{name}: expected outward winding, got {volume}");

        let analytic = plinth_mesh::dual_cavity_pawn_analytic_volume_mm3(throat_radius_mm);
        let deficit = (analytic - volume) / analytic;
        assert!(
            deficit > 0.0,
            "{name}: the mesh encloses {volume} mm^3, which is not inside the analytic solid's \
             {analytic} mm^3 -- an inscribed lathe cannot do that"
        );
        assert!(
            deficit < deficit_ceiling,
            "{name}: the mesh's volume deficit {deficit} exceeds the faceting ceiling \
             {deficit_ceiling} (azimuthal {azimuthal_deficit} + polar {polar_deficit}); mesh \
             {volume} mm^3 vs analytic {analytic} mm^3"
        );
    }
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

    let path_a = ScratchStl::new("a");
    let path_b = ScratchStl::new("b");
    write_binary_stl(path_a.path(), &first).expect("write a");
    write_binary_stl(path_b.path(), &second).expect("write b");
    let bytes_a = std::fs::read(path_a.path()).expect("read a");
    let bytes_b = std::fs::read(path_b.path()).expect("read b");
    assert_eq!(bytes_a, bytes_b, "STL bytes differ across identical builds");
}

/// A scratch STL path that is removed when it goes out of scope — including
/// on the unwinding path, which a bare `remove_file` after the assertions
/// does not cover.
struct ScratchStl(std::path::PathBuf);

impl ScratchStl {
    fn new(tag: &str) -> Self {
        Self(std::env::temp_dir().join(format!(
            "plinth-dual-cavity-{}-{tag}.stl",
            std::process::id()
        )))
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for ScratchStl {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A6: the fixture is exactly f32-representable — `write_binary_stl` (the
/// strict writer) accepts it outright, and the reloaded triangle soup
/// expands to the same coordinates the source mesh named.
#[test]
fn dual_cavity_pawn_survives_the_stl_wire_round_trip() {
    let mesh = make_dual_cavity_pawn().expect("default fixture");
    let scratch = ScratchStl::new("wire");
    write_binary_stl(scratch.path(), &mesh).expect("strict writer accepts an f32-exact fixture");
    let reloaded = load_binary_stl(scratch.path(), WeldPolicy::None).expect("reload");
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
/// infinity are all non-positive-or-non-finite; anything outside
/// (`DUAL_CAVITY_MIN_THROAT_R_MM`, `DUAL_CAVITY_UPPER_SPHERE_R_MM`) is refused
/// by the added range domain (FU-8b: added bounds, not loosened ones — there
/// were none to loosen).
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
    let min = plinth_mesh::DUAL_CAVITY_MIN_THROAT_R_MM;
    let max = plinth_mesh::DUAL_CAVITY_UPPER_SPHERE_R_MM;
    for value in [max, max + 1.0, min / 2.0, 1e-30] {
        assert!(
            matches!(
                make_dual_cavity_pawn_with_throat_radius(value),
                Err(MeshError::InvalidRangeFixtureParameter { .. })
            ),
            "throat_radius_mm = {value} should be refused as out of range ({min}, {max})"
        );
    }
    // The bound is exactly where it says it is: the smallest admitted radius
    // still builds, and still builds something with no degenerate face --
    // which is the property the bound exists to keep true.
    let smallest = make_dual_cavity_pawn_with_throat_radius(min)
        .expect("the minimum admitted throat radius must still build");
    smallest
        .audit_strict()
        .expect("the minimum admitted throat radius must survive the f32 snap non-degenerately");
}
