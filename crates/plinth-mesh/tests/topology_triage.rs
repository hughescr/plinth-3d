use plinth_mesh::{
    build_face_topology, make_box, mesh_fingerprint, triage_components, validate_face_topology,
    Mesh, TriageError, WeldPolicy, FACE_TOPOLOGY_SCHEMA_VERSION, HOLLOW_CONTAINMENT_SAMPLES,
    TOPOLOGY_CUT_BOUNDARY, TOPOLOGY_CUT_DEGENERATE, TOPOLOGY_CUT_DUPLICATE,
    TOPOLOGY_CUT_NONMANIFOLD, TOPOLOGY_CUT_POINT_ONLY, TOPOLOGY_CUT_SAME_DIRECTION,
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
        4,
        "exact bits keeps signed-zero distinct"
    );
    assert_eq!(
        epsilon.welded_vertex_count(),
        3,
        "epsilon performs real derived welding"
    );
    assert_ne!(
        exact.indices(),
        epsilon.indices(),
        "policy changes actual topology view"
    );

    let one_face = Mesh::new(
        vec![
            [-0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [11.0, 0.0, 0.0],
            [10.0, 1.0, 0.0],
        ],
        vec![[2, 3, 4]],
    )
    .expect("finite soup");
    let exact_one = build_face_topology(&one_face, WeldPolicy::ExactBits).expect("exact");
    let epsilon_one = build_face_topology(&one_face, WeldPolicy::Epsilon(1e-9)).expect("epsilon");
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
    let inner = make_box([4.0, 4.0, 4.0], [0.0, 0.0, 0.0]).expect("inner shell");
    assert!(inner.verts().len() <= HOLLOW_CONTAINMENT_SAMPLES);
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
