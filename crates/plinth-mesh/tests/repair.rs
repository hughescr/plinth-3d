//! The repair pre-pass, one census defect class at a time.
//!
//! Each of the three classes gets the same two questions asked of it: did the
//! pass remove the defect the census named, and does the repaired mesh carry
//! **no** cut flag at all afterwards? The second question is asked with
//! `build_face_topology(mesh, WeldPolicy::ExactBits)` and a
//! `cut_flags().iter().all(|f| *f == 0)` — which is not a paraphrase of the
//! closed-solid gate, it is the same two lines
//! `plinth_hollow::validate_closed_mesh` runs. `plinth-hollow`'s own
//! `repair_prepass_rescues_each_census_defect_class` then puts the identical
//! fixtures through the real gate function, so the claim does not rest on this
//! file agreeing with production by inspection.

use plinth_mesh::{
    build_face_topology, make_box_default, make_doubled_sheet_box, make_duplicated_face_box,
    make_dust_shard_box, make_edge_shared_boxes, make_overhang_table, make_pinched_boxes,
    make_thin_fin, make_toed_slab, make_triply_wound_face_box, repair_mesh, write_binary_stl,
    DuplicateDisposition, Mesh, UnrepairedPinch, WeldPolicy, TOPOLOGY_CUT_DUPLICATE,
    TOPOLOGY_CUT_POINT_ONLY,
};

/// The exact condition the closed-solid gate applies, so a fixture cannot pass
/// this file and be refused in production.
fn gate_clean(mesh: &Mesh) -> bool {
    mesh.audit_strict().is_ok()
        && build_face_topology(mesh, WeldPolicy::ExactBits)
            .expect("fixture topology")
            .cut_flags()
            .iter()
            .all(|flags| *flags == 0)
}

fn cut_faces(mesh: &Mesh, mask: u8) -> usize {
    build_face_topology(mesh, WeldPolicy::ExactBits)
        .expect("fixture topology")
        .cut_flags()
        .iter()
        .filter(|flags| **flags & mask != 0)
        .count()
}

fn stl_bytes(mesh: &Mesh, name: &str) -> Vec<u8> {
    let path =
        std::env::temp_dir().join(format!("plinth-repair-{}-{name}.stl", std::process::id()));
    write_binary_stl(&path, mesh).expect("fixture writes");
    let bytes = std::fs::read(&path).expect("fixture reads back");
    let _ = std::fs::remove_file(&path);
    bytes
}

/// **The contract that keeps this stage from being a liability.** A model with
/// none of the three census defects is returned as it arrived — same arrays,
/// same order, and the same bytes on disk after the f32 STL round trip that
/// every downstream consumer will perform on it.
///
/// Proved through the round trip rather than through `Mesh` equality alone,
/// because "equal in f64" and "equal on the wire" are different claims and the
/// pinch nudge lives precisely in the gap between them.
#[test]
fn a_clean_mesh_passes_through_byte_identical() {
    // `write_binary_stl` refuses any coordinate that is not exactly f32, so
    // the wire half of the claim is made with the two fixtures whose
    // dimensions are exact powers-of-two-friendly decimals. The array half is
    // asserted for every fixture, including the ones that cannot be written.
    for (name, mesh, on_the_wire) in [
        ("box", make_box_default().expect("box"), true),
        ("overhang", make_overhang_table().expect("overhang"), true),
        ("toed_slab", make_toed_slab().expect("slab"), false),
        ("thin_fin", make_thin_fin().expect("fin"), false),
    ] {
        let repaired = repair_mesh(&mesh);
        assert!(
            !repaired.evidence().changed(),
            "{name}: a clean mesh must give the pre-pass nothing to do, got {}",
            repaired.evidence().summary_line()
        );
        assert_eq!(repaired.mesh().verts(), mesh.verts(), "{name}: vertices");
        assert_eq!(repaired.mesh().tris(), mesh.tris(), "{name}: triangles");
        if on_the_wire {
            assert_eq!(
                stl_bytes(repaired.mesh(), name),
                stl_bytes(&mesh, name),
                "{name}: emitted bytes"
            );
        }
        assert!(
            repaired.evidence().residual.is_none(),
            "{name}: an untouched mesh is not re-measured"
        );
    }
}

/// Class 1. The shard fixture is refused before repair — it kisses the body at
/// one vertex, which is a pinch — and is a plain box afterwards.
#[test]
fn a_dust_shard_is_stripped_and_the_result_passes_the_gate() {
    let mesh = make_dust_shard_box().expect("shard fixture");
    assert!(
        !gate_clean(&mesh),
        "the fixture must actually be refused, or it proves nothing"
    );
    assert!(cut_faces(&mesh, TOPOLOGY_CUT_POINT_ONLY) > 0);

    let repaired = repair_mesh(&mesh);
    let evidence = repaired.evidence();
    assert_eq!(
        evidence.shards.faces_removed.len(),
        4,
        "the tetrahedron's four faces, and nothing else: {}",
        evidence.summary_line()
    );
    assert_eq!(evidence.shards.components_seen, 2);
    assert!(evidence.shards.skipped.is_none());
    assert!(evidence.duplicates.groups.is_empty());
    assert!(
        evidence.pinches.splits.is_empty(),
        "removing the shard removes the pinch; nothing should need splitting"
    );
    assert_eq!(repaired.mesh().tris().len(), 12);
    assert!(gate_clean(repaired.mesh()));
    assert!(evidence
        .residual
        .expect("a changed mesh is re-measured")
        .is_clean());
}

/// Class 2. A repeated face collapses to one, and the surviving face is the
/// first in source order.
#[test]
fn a_duplicate_face_is_dropped_and_the_result_passes_the_gate() {
    let mesh = make_duplicated_face_box().expect("duplicate fixture");
    assert!(!gate_clean(&mesh));
    assert!(cut_faces(&mesh, TOPOLOGY_CUT_DUPLICATE) > 0);

    let repaired = repair_mesh(&mesh);
    let evidence = repaired.evidence();
    assert_eq!(evidence.duplicates.groups.len(), 1);
    let group = &evidence.duplicates.groups[0];
    assert_eq!(group.forward, 2);
    assert_eq!(group.reverse, 0);
    assert_eq!(group.kept, vec![0], "the first by source order survives");
    assert_eq!(group.removed, vec![12]);
    assert!(evidence.shards.faces_removed.is_empty());
    assert!(evidence.pinches.splits.is_empty());
    assert_eq!(repaired.mesh().tris().len(), 12);
    assert!(gate_clean(repaired.mesh()));
}

/// Class 2, in the shape that decides between the two candidate removal rules.
///
/// **A net-multiplicity rule would delete all three of these faces** — `f = 2`,
/// `r = 1`, so `|f - r| = 1`... of the majority winding, which sounds right
/// until the group is a load-bearing box face and the rule's cancellation
/// branch fires on the pair inside it. Measured before the fix: the box came
/// out with three boundary edges and a hole where a face used to be. The
/// disposition is decided per group by what the rest of the mesh does with
/// the group's three edges, so exactly one member survives here and the box
/// stays watertight.
#[test]
fn a_mixed_winding_duplicate_group_on_real_surface_keeps_one_face() {
    let mesh = make_triply_wound_face_box().expect("triple fixture");
    assert!(!gate_clean(&mesh));

    let repaired = repair_mesh(&mesh);
    let evidence = repaired.evidence();
    assert_eq!(evidence.duplicates.groups.len(), 1);
    let group = &evidence.duplicates.groups[0];
    assert_eq!((group.forward, group.reverse), (2, 1));
    assert_eq!(group.disposition, DuplicateDisposition::KeepOne);
    assert_eq!(group.kept, vec![0], "the face wound with its neighbours");
    assert_eq!(group.removed, vec![12, 13]);
    assert_eq!(
        repaired.mesh().tris().len(),
        12,
        "a box has twelve faces, not nine"
    );
    assert!(
        evidence
            .residual
            .expect("a changed mesh is re-measured")
            .boundary_faces
            == 0,
        "cancelling the group would have opened a hole: {:?}",
        evidence.residual
    );
    assert!(gate_clean(repaired.mesh()));
}

/// **The pass may not delete a model's geometry to make it pass.**
///
/// A free-floating 8 × 2 mm double-sided sheet is four duplicate groups whose
/// neighbours are themselves duplicate groups. A cancellation rule judged only
/// by "do the local edges come out clean" fires on every one of them
/// independently — measured: all four groups cancelled, the whole sheet was
/// deleted, the residual came out **clean**, and a model the gate refuses
/// became one it passes with 8 mm of the owner's geometry silently gone. The
/// shard strip cannot catch it either; the sheet is thirty times the dust
/// ceiling across.
///
/// What happens instead: the sheet is halved to one-sided by the one rule the
/// spec authorized, every group keeps its first member, the sheet's extent is
/// still in the mesh, and the open boundary that results is handed to the
/// **unchanged** gate, which refuses it. A refusal that stays a refusal is the
/// correct outcome here; it is FU-46's work item, not a licence to delete.
#[test]
fn a_doubled_sheet_is_halved_rather_than_deleted() {
    let mesh = make_doubled_sheet_box().expect("doubled-sheet fixture");
    assert!(!gate_clean(&mesh), "the fixture is refused to begin with");
    let sheet_faces = mesh.tris().len() - 12;
    assert_eq!(sheet_faces, 8, "four triangles, each doubled");

    let repaired = repair_mesh(&mesh);
    let evidence = repaired.evidence();
    assert_eq!(evidence.duplicates.groups.len(), 4);
    for group in &evidence.duplicates.groups {
        assert_eq!(
            group.kept.len(),
            1,
            "every group keeps a survivor; there is no disposition that does not: {group:?}"
        );
        assert_eq!(
            group.disposition,
            DuplicateDisposition::KeptFirstUnresolved,
            "and says outright that keeping it did not resolve the shape: {group:?}"
        );
    }
    assert_eq!(
        repaired.mesh().tris().len(),
        12 + 4,
        "the box, plus the sheet halved to one-sided: {}",
        evidence.summary_line()
    );

    // The geometry is still there: the sheet's far corner is 28 mm out, and a
    // deletion would have taken the whole strip with it.
    let far_x = repaired
        .mesh()
        .verts()
        .iter()
        .fold(f64::NEG_INFINITY, |largest, vertex| largest.max(vertex[0]));
    assert!(far_x > 27.0, "the sheet's extent survived: {far_x}");

    // And the model is still refused — by the unchanged gate, on the boundary
    // edges an open sheet genuinely has.
    assert!(
        !gate_clean(repaired.mesh()),
        "deleting the evidence is not the same as repairing the model"
    );
    assert!(
        evidence
            .residual
            .expect("a changed mesh is re-measured")
            .boundary_faces
            > 0,
        "{:?}",
        evidence.residual
    );
}

/// **No action this pass performs can empty a model, and this is the mesh that
/// proves it.**
///
/// A whole model that is nothing but one free-floating coincident flap is the
/// smallest input for which "delete what is removable" and "delete everything"
/// are the same instruction. Under an earlier rule that deleted an edge-isolated
/// coincident group outright it planned its way to zero triangles and had to be
/// rolled back; under the one rule the spec authorized — keep the first — one
/// face survives, the group is recorded as unresolved, and the single-triangle
/// result is handed to the **unchanged** gate, which refuses it for the three
/// boundary edges an unattached triangle genuinely has.
///
/// That refusal is the correct outcome and the work item (FU-46), not a
/// licence: deleting the flap would take a model from gate-refused to
/// gate-passed by removing the geometry being judged.
#[test]
fn a_model_that_is_only_a_coincident_flap_keeps_a_face_and_is_left_to_the_gate() {
    let flap = Mesh::new(
        vec![[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [0.0, 4.0, 0.0]],
        vec![[0, 1, 2], [0, 2, 1]],
    )
    .expect("a two-face flap is a mesh");
    assert!(!gate_clean(&flap), "the fixture is refused to begin with");

    let repaired = repair_mesh(&flap);
    let evidence = repaired.evidence();
    assert!(
        evidence.refused_to_empty.is_none(),
        "nothing planned its way to an empty mesh: {evidence:?}"
    );
    assert_eq!(evidence.duplicates.groups.len(), 1);
    let group = &evidence.duplicates.groups[0];
    assert_eq!(group.kept, vec![0], "the first by source order survives");
    assert_eq!(group.removed, vec![1]);
    assert_eq!(
        group.disposition,
        DuplicateDisposition::KeptFirstUnresolved,
        "and the pass records that keeping it did not close anything"
    );
    assert_eq!(
        repaired.mesh().tris().len(),
        1,
        "one triangle, not zero: {}",
        evidence.summary_line()
    );
    assert!(
        !gate_clean(repaired.mesh()),
        "an unattached triangle is still refused, and that refusal is the work item"
    );
    assert!(
        evidence
            .residual
            .expect("a changed mesh is re-measured")
            .boundary_faces
            > 0
    );
}

/// **A defect class this pass does not own, refused instead of performed.**
///
/// Two boxes sharing one whole edge have a non-manifold edge, which fan
/// connectivity cannot cross — so both of its endpoints present as pinches
/// even though the sheets meet along a line. Splitting one would take
/// `nonmanifold_edges` 1 -> 0 and `boundary_edges` 2 -> 3 and call it a
/// repair. The census found zero non-manifold edges and this module
/// implements no speculative fourth class, so the pass names the shape, leaves
/// the mesh exactly as it arrived, and lets the unchanged gate refuse it.
#[test]
fn fans_severed_by_a_non_manifold_edge_are_refused_not_torn() {
    let mesh = make_edge_shared_boxes().expect("edge-shared fixture");
    let before = build_face_topology(&mesh, WeldPolicy::ExactBits).expect("fixture topology");
    assert_eq!(
        before.counts().nonmanifold_edges,
        1,
        "the fixture must actually carry the defect it is named for"
    );
    assert!(cut_faces(&mesh, TOPOLOGY_CUT_POINT_ONLY) > 0);

    let repaired = repair_mesh(&mesh);
    let evidence = repaired.evidence();
    assert!(
        evidence.pinches.splits.is_empty(),
        "no split may be performed here: {}",
        evidence.summary_line()
    );
    assert_eq!(
        evidence.pinches.unrepaired.len(),
        2,
        "both endpoints of the shared edge are refused: {:?}",
        evidence.pinches.unrepaired
    );
    for refusal in &evidence.pinches.unrepaired {
        assert!(
            matches!(refusal, UnrepairedPinch::FanSeveredByEdge { .. }),
            "{refusal:?}"
        );
    }
    assert!(
        !evidence.changed(),
        "a pass that repaired nothing must not have changed anything"
    );
    assert_eq!(repaired.mesh().verts(), mesh.verts());
    assert_eq!(repaired.mesh().tris(), mesh.tris());
    let after =
        build_face_topology(repaired.mesh(), WeldPolicy::ExactBits).expect("repaired topology");
    assert_eq!(
        after.counts().nonmanifold_edges,
        1,
        "the junction is still a junction"
    );
    assert_eq!(
        after.counts().boundary_edges,
        0,
        "and no crack was opened in it"
    );
}

/// Class 3. Two boxes joined at a point become two boxes joined at nothing,
/// and the displacement that separates them is smaller than a printed voxel by
/// four orders of magnitude.
#[test]
fn a_pinch_vertex_is_split_and_the_result_passes_the_gate() {
    let mesh = make_pinched_boxes().expect("pinch fixture");
    assert!(!gate_clean(&mesh));
    assert_eq!(
        cut_faces(&mesh, TOPOLOGY_CUT_POINT_ONLY),
        9,
        "five triangles of one box and four of the other meet at the shared corner"
    );

    let repaired = repair_mesh(&mesh);
    let evidence = repaired.evidence();
    assert!(evidence.shards.faces_removed.is_empty());
    assert!(evidence.duplicates.groups.is_empty());
    assert_eq!(evidence.pinches.splits.len(), 1);
    assert!(evidence.pinches.unrepaired.is_empty());

    let split = &evidence.pinches.splits[0];
    assert_eq!(split.position_mm, [5.0, 5.0, 10.0]);
    assert_eq!(
        split.anchored_faces, 5,
        "the fan holding the lowest face id never moves"
    );
    assert_eq!(split.fans.len(), 1);
    let moved = &split.fans[0];
    assert_eq!(moved.faces, 4);
    assert!(
        moved.ulp_steps.is_power_of_two(),
        "the escalation doubles: {moved:?}"
    );
    // Bounded by REPAIR_PINCH_SPLIT_MAX_MM (5e-5 mm) and, in fact, far below
    // it: at 10 mm coordinates two f32 ULPs is about 2 nanometres, roughly
    // 1/7000 of the printer's 14 um narrow pixel.
    assert!(
        moved.displacement_mm > 0.0 && moved.displacement_mm < 1.0e-5,
        "{moved:?}"
    );

    // No face is lost, and the vertex count is unchanged: the fan that moved
    // released its old copy of the shared corner and took a new one, and the
    // released copy is compacted away. What used to be one welded point is now
    // two points a couple of nanometres apart.
    assert_eq!(repaired.mesh().tris().len(), mesh.tris().len());
    assert_eq!(repaired.mesh().verts().len(), mesh.verts().len());
    let corner = [5.0, 5.0, 10.0];
    let near = |vertex: &[f64; 3]| (0..3).all(|axis| (vertex[axis] - corner[axis]).abs() < 1.0e-4);
    assert_eq!(
        repaired
            .mesh()
            .verts()
            .iter()
            .filter(|vertex| **vertex == corner)
            .count(),
        1,
        "the anchored fan keeps the original position exactly"
    );
    assert_eq!(
        repaired.mesh().verts().iter().filter(|v| near(v)).count(),
        2,
        "and the moved fan sits beside it, not on it"
    );
    assert!(gate_clean(repaired.mesh()));
}

/// The split has to survive the *two* erasures that can undo it: the exact-bit
/// weld, which keys on `to_bits()`, and the f32 binary-STL round trip. Proving
/// the second means reading the mesh back off disk and re-running the gate on
/// what was actually written — the audit-the-file-as-written rule applied to a
/// repair.
#[test]
fn a_split_pinch_survives_the_f32_stl_round_trip() {
    let repaired = repair_mesh(&make_pinched_boxes().expect("pinch fixture"));
    let path = std::env::temp_dir().join(format!("plinth-repair-pinch-{}.stl", std::process::id()));
    write_binary_stl(&path, repaired.mesh()).expect("write");
    let reloaded = plinth_mesh::load_binary_stl(&path, WeldPolicy::None).expect("reload");
    let _ = std::fs::remove_file(&path);
    assert!(
        gate_clean(&reloaded),
        "a split that only exists in f64 is not a split"
    );
}

/// Nothing here reads a clock, an environment variable, or an RNG, and nothing
/// here is parallel — so the property a thread sweep would be testing for is
/// pinned directly: the same input gives the same output, arrays and evidence
/// alike, every time.
#[test]
fn repairing_twice_is_bit_identical() {
    for mesh in [
        make_dust_shard_box().expect("shard"),
        make_duplicated_face_box().expect("duplicate"),
        make_pinched_boxes().expect("pinch"),
        make_box_default().expect("clean"),
    ] {
        let first = repair_mesh(&mesh);
        let second = repair_mesh(&mesh);
        assert_eq!(first.mesh().verts(), second.mesh().verts());
        assert_eq!(first.mesh().tris(), second.mesh().tris());
        assert_eq!(first.evidence(), second.evidence());
    }
}

/// The pass is idempotent for the reason that matters: a repaired mesh is a
/// clean mesh, so running the stage again does nothing and changes nothing.
#[test]
fn repairing_a_repaired_mesh_is_a_no_op() {
    for mesh in [
        make_dust_shard_box().expect("shard"),
        make_duplicated_face_box().expect("duplicate"),
        make_pinched_boxes().expect("pinch"),
    ] {
        let once = repair_mesh(&mesh).into_mesh();
        let twice = repair_mesh(&once);
        assert!(
            !twice.evidence().changed(),
            "{}",
            twice.evidence().summary_line()
        );
        assert_eq!(twice.mesh().verts(), once.verts());
        assert_eq!(twice.mesh().tris(), once.tris());
    }
}

/// A cap is a boundary, not a knob. With the registered ceiling in force there
/// is no fixture in this workspace that reaches it, so the reporting path is
/// exercised through the type: an unrepairable pinch is *evidence*, never an
/// error and never a reason to displace further.
#[test]
fn an_unrepairable_pinch_is_reported_rather_than_forced() {
    let capped = UnrepairedPinch::DisplacementCapped {
        welded_vertex: 7,
        cap_mm: 5.0e-5,
        largest_attempt_mm: 5.0e-5,
    };
    let rendered = serde_json::to_string(&capped).expect("evidence serializes");
    assert!(rendered.contains("displacement_capped"), "{rendered}");
    assert_eq!(
        serde_json::from_str::<UnrepairedPinch>(&rendered).expect("round trip"),
        capped
    );
}
