use plinth_mesh::{
    build_face_topology, make_box, mesh_fingerprint, triage_components, triage_settings,
    validate_face_topology, Mesh, TopologyError, TriageError, WeldPolicy,
    FACE_TOPOLOGY_SCHEMA_VERSION, TOPOLOGY_CUT_BOUNDARY, TOPOLOGY_CUT_DEGENERATE,
    TOPOLOGY_CUT_DUPLICATE, TOPOLOGY_CUT_NONMANIFOLD, TOPOLOGY_CUT_POINT_ONLY,
    TOPOLOGY_CUT_SAME_DIRECTION,
};

#[test]
fn topology_builds_canonical_adjacency_components_and_v4_digest_once() {
    let mesh = Mesh::new(
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
        ],
        vec![[0, 1, 2], [3, 4, 5]],
    )
    .expect("finite soup");
    let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).expect("topology");
    assert_eq!(FACE_TOPOLOGY_SCHEMA_VERSION, 4);
    assert_eq!(topology.indptr(), &[0, 1, 2]);
    assert_eq!(topology.indices(), &[1, 0]);
    assert_eq!(topology.component_ids(), &[0, 0]);
    assert_eq!(
        topology.cut_flags(),
        &[TOPOLOGY_CUT_BOUNDARY, TOPOLOGY_CUT_BOUNDARY]
    );
    assert_eq!(topology.neighbors(0), Some(&[1][..]));
    assert_eq!(topology.welded_vertex_count(), 4);
    assert_eq!(topology.weld_policy(), WeldPolicy::ExactBits);
    assert_eq!(topology.content_digest().len(), 64);
    assert_eq!(
        topology.content_digest(),
        "928d34b542052e283fd38c3fd5a07c918d9643ab75111e75c01298c8c549aafb",
        "schema-v4 preimage pins domain, version, field order, lengths, and little-endian arrays"
    );
    validate_face_topology(&topology).expect("optional diagnostic validates immutable sidecar");
}

#[test]
fn fingerprint_canonicalizes_signed_zero_but_welding_remains_an_explicit_choice() {
    let negative = Mesh::new(
        vec![[-0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        vec![[0, 1, 2]],
    )
    .expect("finite");
    let positive = Mesh::new(
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        vec![[0, 1, 2]],
    )
    .expect("finite");
    assert_eq!(mesh_fingerprint(&negative), mesh_fingerprint(&positive));
    assert_ne!(
        plinth_core::digest_f64(-0.0).expect("finite"),
        plinth_core::digest_f64(0.0).expect("finite"),
        "generic core digests preserve signed zero; topology is the narrow exception"
    );

    let soup = Mesh::new(
        vec![
            [-0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
        ],
        vec![[0, 1, 2], [3, 4, 5]],
    )
    .expect("finite soup");
    let exact = build_face_topology(&soup, WeldPolicy::ExactBits).expect("exact topology");
    let epsilon = build_face_topology(&soup, WeldPolicy::Epsilon(1e-9)).expect("epsilon topology");
    assert_eq!(
        exact.welded_vertex_count(),
        3,
        "ExactBits topology keys canonicalize signed zero"
    );
    assert_eq!(
        epsilon.welded_vertex_count(),
        3,
        "epsilon has the same signed-zero normalization"
    );
    assert_eq!(
        exact.source_vertex_to_welded(),
        epsilon.source_vertex_to_welded()
    );

    let signed_zero_tetra = Mesh::new(
        vec![
            [-0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ],
        vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
    )
    .expect("finite tetra");
    for policy in [WeldPolicy::ExactBits, WeldPolicy::Epsilon(1e-9)] {
        let triaged = triage_components(&signed_zero_tetra, policy).expect("triaged tetra");
        let representative = triaged
            .mesh()
            .verts()
            .iter()
            .find(|point| point[0] == 0.0 && point[1] == 0.0 && point[2] == 0.0)
            .expect("zero representative");
        assert_eq!(
            representative[0].to_bits(),
            0.0_f64.to_bits(),
            "{policy:?} emits a positive-zero stored representative"
        );
    }

    let near = Mesh::new(
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.000_5, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
        ],
        vec![[0, 1, 2], [3, 4, 5]],
    )
    .expect("finite soup");
    let exact_near = build_face_topology(&near, WeldPolicy::ExactBits).expect("exact");
    let epsilon_near = build_face_topology(&near, WeldPolicy::Epsilon(0.001)).expect("epsilon");
    assert_ne!(
        exact_near.indices(),
        epsilon_near.indices(),
        "epsilon changes the real derived view"
    );

    let one_face = Mesh::new(
        vec![
            [0.001, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [11.0, 0.0, 0.0],
            [10.0, 1.0, 0.0],
        ],
        vec![[2, 3, 4]],
    )
    .expect("finite soup");
    let exact_one = build_face_topology(&one_face, WeldPolicy::ExactBits).expect("exact");
    let epsilon_one = build_face_topology(&one_face, WeldPolicy::Epsilon(0.01)).expect("epsilon");
    assert_ne!(
        exact_one.source_vertex_to_welded(),
        epsilon_one.source_vertex_to_welded()
    );
    assert_eq!(
        exact_one.content_digest(),
        epsilon_one.content_digest(),
        "v4 digest is only fingerprint plus topology arrays, never weld-policy metadata"
    );
}

#[test]
fn triage_settings_are_read_from_the_atomic_typed_m0_publication() {
    let settings = triage_settings().expect("M0 constants");
    let placeholders = plinth_core::m0_constants()
        .expect("same M0 publication")
        .placeholders();
    assert_eq!(
        settings.shard_max_faces() as u64,
        placeholders.hollow_shard_max_faces().diagnostic().get()
    );
    assert_eq!(
        settings.shard_max_dim_mm(),
        placeholders.hollow_shard_max_dim_mm().diagnostic().get()
    );
    assert_eq!(
        settings.shard_max_volume_mm3(),
        placeholders
            .hollow_shard_max_volume_mm3()
            .diagnostic()
            .get()
    );
    assert_eq!(
        settings.containment_samples() as u64,
        placeholders.hollow_containment_samples().diagnostic().get()
    );
    assert_eq!(
        settings.containment_min_fraction(),
        placeholders
            .hollow_containment_min_fraction()
            .diagnostic()
            .get()
    );
}

#[test]
fn epsilon_grid_enforces_the_exact_i64_bounds_without_saturating_casts() {
    const TWO_TO_63: f64 = 9_223_372_036_854_775_808.0;
    for coordinate in [TWO_TO_63 - 1024.0, -TWO_TO_63 + 2048.0, -TWO_TO_63] {
        build_face_topology(&grid_mesh(coordinate), WeldPolicy::Epsilon(1.0))
            .expect("an in-range grid cell");
    }

    for coordinate in [TWO_TO_63, 1e308, -1e308] {
        let mesh = grid_mesh(coordinate);
        assert!(matches!(
            build_face_topology(&mesh, WeldPolicy::Epsilon(1.0)),
            Err(TopologyError::GridCellOutOfRange {
                vertex: 0,
                axis: 0,
                ..
            })
        ));
    }
}

fn grid_mesh(x: f64) -> Mesh {
    Mesh::new(
        vec![[x, 0.0, 0.0], [x, 1.0, 0.0], [x, 0.0, 1.0]],
        vec![[0, 1, 2]],
    )
    .expect("finite")
}

#[test]
fn huge_finite_coordinate_has_a_distinct_grid_range_error() {
    let mesh = grid_mesh(1e308);
    assert!(matches!(
        build_face_topology(&mesh, WeldPolicy::Epsilon(1e-300)),
        Err(TopologyError::GridCellOutOfRange {
            vertex: 0,
            axis: 0,
            ..
        })
    ));
}

#[test]
fn every_ambiguous_edge_and_point_only_contact_is_a_named_cut() {
    let cases: [(&str, Mesh, u8); 5] = [
        (
            "same direction",
            mesh(&[[0, 1, 2], [0, 1, 3]]),
            TOPOLOGY_CUT_SAME_DIRECTION,
        ),
        (
            "duplicate",
            mesh(&[[0, 1, 2], [0, 1, 2]]),
            TOPOLOGY_CUT_DUPLICATE,
        ),
        (
            "nonmanifold",
            mesh(&[[0, 1, 2], [1, 0, 3], [0, 1, 4]]),
            TOPOLOGY_CUT_NONMANIFOLD,
        ),
        ("degenerate", mesh(&[[0, 0, 1]]), TOPOLOGY_CUT_DEGENERATE),
        (
            "point only",
            mesh(&[[0, 1, 2], [0, 3, 4]]),
            TOPOLOGY_CUT_POINT_ONLY,
        ),
    ];
    for (name, mesh, flag) in cases {
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).expect(name);
        assert!(
            topology.cut_flags().iter().all(|value| value & flag != 0),
            "{name}"
        );
        assert!(topology
            .cut_flags()
            .iter()
            .all(|value| value & TOPOLOGY_CUT_BOUNDARY != 0 || flag != TOPOLOGY_CUT_BOUNDARY));
    }
}

#[test]
fn triage_strips_only_a_qualified_pinched_shard_and_keeps_detached_body() {
    let main = make_box([10.0, 10.0, 10.0], [0.0, 0.0, 0.0]).expect("main box");
    let shard = tetra_at([5.0, 5.0, 0.0], 0.05, true);
    let joined = join(&main, &shard);
    let triaged =
        triage_components(&joined, WeldPolicy::ExactBits).expect("qualified shard strips");
    assert_eq!(triaged.summary().stripped_components().len(), 1);
    assert_eq!(triaged.mesh().tris().len(), main.tris().len());
    assert_eq!(triaged.mesh().weld_policy(), WeldPolicy::ExactBits);

    let detached = make_box([0.1, 0.1, 0.1], [30.0, 0.0, 0.0]).expect("detached part");
    let kept = triage_components(&join(&main, &detached), WeldPolicy::ExactBits)
        .expect("detached authored body stays");
    assert!(kept.summary().stripped_components().is_empty());
    assert_eq!(
        kept.mesh().tris().len(),
        main.tris().len() + detached.tris().len()
    );
}

#[test]
fn triage_batches_bounded_samples_and_refuses_a_pre_hollowed_shell() {
    let outer = make_box([10.0, 10.0, 10.0], [0.0, 0.0, 0.0]).expect("outer shell");
    let inner = make_box([4.0, 4.0, 4.0], [0.0, 0.0, 2.0]).expect("inner shell");
    assert!(inner.verts().len() <= triage_settings().expect("settings").containment_samples());
    let error = triage_components(&join(&outer, &inner), WeldPolicy::ExactBits)
        .expect_err("sealed interior must refuse");
    assert!(
        matches!(error, TriageError::PreHollowed(refusal) if refusal.cavity_components().len() == 1)
    );
    assert!(matches!(
        triage_components(&outer, WeldPolicy::None),
        Err(TriageError::UnweldedInput)
    ));
}

#[test]
fn triage_refuses_an_inner_shell_in_a_genuinely_concave_container() {
    let outer = concave_l_prism();
    let inner = make_box([0.25, 0.25, 0.25], [0.5, 0.5, 0.5]).expect("inner cube");
    assert!(matches!(
        triage_components(&join(&outer, &inner), WeldPolicy::ExactBits),
        Err(TriageError::PreHollowed(_))
    ));
}

#[test]
fn triage_treats_a_shell_touching_its_container_boundary_as_ambiguous_not_inside() {
    let outer = make_box([10.0, 10.0, 10.0], [0.0, 0.0, 0.0]).expect("outer");
    let touching = make_box([4.0, 4.0, 4.0], [0.0, 0.0, 0.0]).expect("touching");
    assert!(triage_components(&join(&outer, &touching), WeldPolicy::ExactBits).is_ok());
}

fn mesh(tris: &[[u32; 3]]) -> Mesh {
    Mesh::new(
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, -1.0, 0.0],
        ],
        tris.to_vec(),
    )
    .expect("fixture")
}

fn join(left: &Mesh, right: &Mesh) -> Mesh {
    let mut verts = left.verts().to_vec();
    let offset = u32::try_from(verts.len()).expect("small fixture");
    verts.extend_from_slice(right.verts());
    let mut tris = left.tris().to_vec();
    tris.extend(right.tris().iter().map(|triangle| {
        [
            triangle[0] + offset,
            triangle[1] + offset,
            triangle[2] + offset,
        ]
    }));
    Mesh::new(verts, tris).expect("joined finite mesh")
}

fn tetra_at(origin: [f64; 3], side: f64, share_first: bool) -> Mesh {
    let [x, y, z] = origin;
    let first = if share_first {
        origin
    } else {
        [x + side, y + side, z + side]
    };
    Mesh::new(
        vec![first, [x - side, y, z], [x, y - side, z], [x, y, z + side]],
        vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
    )
    .expect("tetra")
}

fn concave_l_prism() -> Mesh {
    let outline = [
        [0.0, 0.0],
        [4.0, 0.0],
        [4.0, 1.0],
        [1.0, 1.0],
        [1.0, 4.0],
        [0.0, 4.0],
    ];
    let mut verts = outline
        .iter()
        .map(|point| [point[0], point[1], 0.0])
        .collect::<Vec<_>>();
    verts.extend(outline.iter().map(|point| [point[0], point[1], 4.0]));
    let caps = [[0, 1, 3], [1, 2, 3], [0, 3, 5], [3, 4, 5]];
    let mut tris = Vec::new();
    for [a, b, c] in caps {
        tris.push([c, b, a]);
        tris.push([a + 6, b + 6, c + 6]);
    }
    for index in 0..6_u32 {
        let next = (index + 1) % 6;
        tris.push([index, next, index + 6]);
        tris.push([next, next + 6, index + 6]);
    }
    Mesh::new(verts, tris).expect("closed concave prism")
}
