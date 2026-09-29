//! Regression guards for the five adversarially-found defects (three gold-
//! invariant blockers + two validation-bypass majors). Each test failed on the
//! pre-fix HEAD and must stay green:
//!   1 — rotation state survived `attach_supports`, cumulatively re-applied.
//!   2 — parser panicked (slice OOB) when `rec_off` fell inside the index quad.
//!   3 — dead/unaligned smesh bytes silently dropped by a rotate rebake.
//!   4 — non-finite STL geometry bypassed the finiteness check.
//!   5 — zero-triangle STL accepted.
//!   6 — parse/verify asymmetry: an empty support group header parsed OK but a
//!       clean, unmodified `to_bytes()` failed the write audit, breaking gold
//!       invariant (a) (parse-then-save of an unmodified file is byte-identical).
use chitubox_io::*;

fn tri_centered() -> Vec<f32> {
    // bbox center is origin already
    vec![1.0, 0.0, 0.0, -1.0, 1.0, 0.0, 0.0, -1.0, 0.0]
}

fn one_model_project() -> Project {
    let mut p = Project::new_empty();
    p.add_models(&[ModelSource {
        name: "probe".into(),
        geometry: ModelGeometry::Triangles(TriangleSoup::new(tri_centered())),
        placement: Placement::default(),
    }])
    .unwrap();
    p
}

fn two_model_project() -> Project {
    let mut p = Project::new_empty();
    p.add_models(&[
        ModelSource {
            name: "a".into(),
            geometry: ModelGeometry::Triangles(TriangleSoup::new(tri_centered())),
            placement: Placement::default(),
        },
        ModelSource {
            name: "b".into(),
            geometry: ModelGeometry::Triangles(TriangleSoup::new(tri_centered())),
            placement: Placement::default(),
        },
    ])
    .unwrap();
    p
}

fn support_rec() -> SupportRecord {
    SupportRecord::new(
        SegmentType::SHAFT,
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        0.5,
        0.5,
        TriangleSoup::new(vec![0.1, 0.0, 0.0, 0.0, 0.1, 0.0, 0.0, 0.0, 0.1]),
    )
}

#[test]
fn probe1_double_rotation_after_attach_supports() {
    let mut p = one_model_project();
    p.rotate_model_z(0, 90.0).unwrap();
    p.attach_supports(&[SupportBatch {
        model_index: 0,
        records: vec![support_rec()],
        mode: SupportMode::Replace,
        lift_mm: None,
    }])
    .unwrap();
    p.rotate_model_z(0, 90.0).unwrap();
    // vertex (1,0,0) rotated 180 deg total should be (-1,0,0)
    let m = p.model_triangles(0).unwrap();
    println!(
        "PROBE1 first vertex after 90+attach+90: ({}, {}, {})",
        m[0], m[1], m[2]
    );
    assert!(
        (m[0] - (-1.0)).abs() < 1e-4 && m[1].abs() < 1e-4,
        "expected (-1,0,0); got ({}, {}, {}) => rotation state corrupted by attach_supports",
        m[0],
        m[1],
        m[2]
    );
}

#[test]
#[expect(
    clippy::cast_possible_truncation,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn probe2_parse_panics_on_rec_off_inside_index_quad() {
    let mut p = one_model_project();
    p.attach_supports(&[SupportBatch {
        model_index: 0,
        records: vec![support_rec()],
        mode: SupportMode::Replace,
        lift_mm: None,
    }])
    .unwrap();
    let mut bytes = p.to_bytes().unwrap();
    // locate sptr: model record at off8, support ptr at +268
    let off8 = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let sptr = u32::from_le_bytes(bytes[off8 + 268..off8 + 272].try_into().unwrap()) as usize;
    assert!(sptr != 0);
    // patch index.rec_off (sptr+4) to sptr+8 (inside the 16-byte index quad)
    bytes[sptr + 4..sptr + 8].copy_from_slice(&((sptr + 8) as u32).to_le_bytes());
    // A robust parser must return Err, never panic.
    let r = std::panic::catch_unwind(|| Project::from_bytes(bytes));
    match r {
        Ok(Ok(_)) => panic!("PROBE2: malformed file parsed OK (bad)"),
        Ok(Err(e)) => println!("PROBE2 OK: graceful error: {e}"),
        Err(_) => panic!("PROBE2: parse PANICKED on malformed input"),
    }
}

#[test]
#[expect(
    clippy::cast_possible_truncation,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn probe3_dead_smesh_tail_bytes_silently_dropped() {
    let mut p = one_model_project();
    p.attach_supports(&[SupportBatch {
        model_index: 0,
        records: vec![support_rec()],
        mode: SupportMode::Replace,
        lift_mm: None,
    }])
    .unwrap();
    let bytes = p.to_bytes().unwrap();
    let off8 = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let sptr = u32::from_le_bytes(bytes[off8 + 268..off8 + 272].try_into().unwrap()) as usize;
    let smesh_off = u32::from_le_bytes(bytes[sptr + 8..sptr + 12].try_into().unwrap()) as usize;
    let smesh_sz = u32::from_le_bytes(bytes[sptr + 12..sptr + 16].try_into().unwrap()) as usize;
    // build a new file with 2 extra dead bytes at the end of smesh
    let mut nb = Vec::new();
    nb.extend_from_slice(&bytes[..smesh_off + smesh_sz]);
    nb.extend_from_slice(&[0xAA, 0xBB]); // unknown dead bytes
    nb.extend_from_slice(&bytes[smesh_off + smesh_sz..]);
    let shift = 2u32;
    // fix model-record sptr
    let nsptr = (sptr as u32 + shift) as usize;
    nb[off8 + 268..off8 + 272].copy_from_slice(&(nsptr as u32).to_le_bytes());
    // fix index quad: rec_off += 2, mesh_size += 2 (index itself moved by 2)
    let rec_off = u32::from_le_bytes(bytes[sptr + 4..sptr + 8].try_into().unwrap()) + shift;
    nb[nsptr + 4..nsptr + 8].copy_from_slice(&rec_off.to_le_bytes());
    let nsz = (smesh_sz as u32 + shift).to_le_bytes();
    nb[nsptr + 12..nsptr + 16].copy_from_slice(&nsz);
    let total_in = nb.len();
    let p2 = Project::from_bytes(nb).expect("patched file with dead smesh tail should parse");
    // clean save must round-trip the dead bytes (invariant a) — check
    let clean = p2.to_bytes().expect("clean save");
    assert_eq!(clean.len(), total_in, "clean save lost bytes");
    // now rotate: dead bytes are unknown data; invariant (c) says preserve
    let mut p3 = p2.clone();
    p3.rotate_models_z(&[ZRotation {
        model_index: 0,
        delta_deg: 90.0,
    }])
    .unwrap();
    match p3.to_bytes() {
        Ok(out) => {
            println!(
                "PROBE3: rotate+save on dead-tail file: in {} bytes, out {} bytes",
                total_in,
                out.len()
            );
            assert_eq!(
                out.len(),
                total_in,
                "PROBE3: unknown dead smesh bytes silently destroyed, verify passed"
            );
        }
        Err(e) => println!("PROBE3: save refused (acceptable): {e}"),
    }
}

#[test]
fn probe4_nan_stl_bypasses_finite_check() {
    // craft a binary STL with a NaN vertex
    let mut stl = vec![0u8; 80];
    stl.extend_from_slice(&1u32.to_le_bytes());
    let mut tri = Vec::new();
    for _ in 0..3 {
        tri.extend_from_slice(&0.0f32.to_le_bytes());
    } // normal
    let verts: [f32; 9] = [f32::NAN, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0];
    for v in verts {
        tri.extend_from_slice(&v.to_le_bytes());
    }
    tri.extend_from_slice(&[0u8, 0u8]);
    stl.extend_from_slice(&tri);
    let dir = std::env::temp_dir();
    let path = dir.join("probe_nan.stl");
    std::fs::write(&path, &stl).unwrap();
    let mut p = Project::new_empty();
    let r = p.add_models(&[ModelSource {
        name: "nan".into(),
        geometry: ModelGeometry::StlPath(path.clone()),
        placement: Placement::default(),
    }]);
    match r {
        Err(e) => println!("PROBE4 OK: rejected: {e}"),
        Ok(_) => {
            let out = p.to_bytes();
            panic!(
                "PROBE4: NaN STL accepted; to_bytes => {:?} (NaN geometry written, finite check bypassed)",
                out.map(|b| b.len())
            );
        }
    }
}

#[test]
#[expect(
    clippy::cast_possible_truncation,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn probe6_empty_group_header_parses_and_saves_byte_identical() {
    // Build a normal supported file (synthetic writes emit a NON-empty group
    // header, so the empty case is otherwise unexercised).
    let mut p = one_model_project();
    p.attach_supports(&[SupportBatch {
        model_index: 0,
        records: vec![support_rec()],
        mode: SupportMode::Replace,
        lift_mm: None,
    }])
    .unwrap();
    let bytes = p.to_bytes().unwrap();

    // locate the support index quad
    let off8 = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let sptr = u32::from_le_bytes(bytes[off8 + 268..off8 + 272].try_into().unwrap()) as usize;
    assert!(sptr != 0);
    let idx_end = sptr + 16; // SUPPORT_INDEX_LEN
    let rec_off = u32::from_le_bytes(bytes[sptr + 4..sptr + 8].try_into().unwrap()) as usize;
    let header_len = rec_off - idx_end;
    assert!(
        header_len > 0,
        "expected a non-empty group header to excise"
    );

    // Excise the group header and repoint rec_off at idx_end -> empty header.
    // Records are the tail region; nothing after them is referenced by offset,
    // and record blob offsets point into smesh (before the index), so removing
    // these bytes needs only the rec_off field patched.
    let mut nb = Vec::with_capacity(bytes.len() - header_len);
    nb.extend_from_slice(&bytes[..idx_end]);
    nb.extend_from_slice(&bytes[rec_off..]);
    nb[sptr + 4..sptr + 8].copy_from_slice(&(idx_end as u32).to_le_bytes());

    // Parser accepts the empty-header file...
    let p2 = Project::from_bytes(nb.clone())
        .expect("PROBE6: empty group header must parse (rec_off == idx_end)");
    // ...and a clean, UNMODIFIED save must succeed AND be byte-identical.
    let out = p2
        .to_bytes()
        .expect("PROBE6: clean save of empty-header file failed => invariant (a) broken");
    assert_eq!(out, nb, "PROBE6: parse-then-save not byte-identical");
}

// ---- second adversarial wave: two confirmed blockers + two majors ----------

/// BLOCKER 1 — two support records aliasing the SAME blob byte-range must be
/// rejected at parse. Otherwise `rotate_models_z` rotates the shared range once
/// per referencing record (twice here), leaving the blob at 2x the angle of its
/// record endpoints — a silently corrupted, invariant-passing file.
#[test]
fn blocker1_aliased_support_blob_ranges_rejected_at_parse() {
    // record layout: S_MESH_OFF = 40, S_MESH_LEN = 44 within each 72-byte record
    const S_MESH_OFF: usize = 40;
    const SEG_RECORD_LEN: usize = 72;
    let mut p = one_model_project();
    p.attach_supports(&[SupportBatch {
        model_index: 0,
        records: vec![support_rec(), support_rec()],
        mode: SupportMode::Replace,
        lift_mm: None,
    }])
    .unwrap();
    let mut bytes = p.to_bytes().unwrap();
    let off8 = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let sptr = u32::from_le_bytes(bytes[off8 + 268..off8 + 272].try_into().unwrap()) as usize;
    assert!(sptr != 0);
    let rec_off = u32::from_le_bytes(bytes[sptr + 4..sptr + 8].try_into().unwrap()) as usize;
    // point record 1's (off,len) at record 0's blob -> exact alias
    let r0 = rec_off + S_MESH_OFF;
    let r1 = rec_off + SEG_RECORD_LEN + S_MESH_OFF;
    let alias: [u8; 8] = bytes[r0..r0 + 8].try_into().unwrap();
    bytes[r1..r1 + 8].copy_from_slice(&alias);
    // the parser must reject the aliased section rather than accept a file that
    // rotate would silently corrupt.
    assert!(
        Project::from_bytes(bytes).is_err(),
        "BLOCKER1: aliased support-blob ranges accepted at parse"
    );
}

/// BLOCKER 2a — finite-in / non-finite-out: a TriangleSoup with coordinates
/// near f32::MAX (same sign) overflows the bbox-center `(min+max)*0.5` to inf,
/// baking -inf into every vertex. The post-center finiteness barrier must
/// reject it (the intake all_finite gate passes such input).
#[test]
fn blocker2a_center_overflow_rejected() {
    let big = 3.0e38f32;
    let mut p = Project::new_empty();
    let r = p.add_models(&[ModelSource {
        name: "overflow".into(),
        geometry: ModelGeometry::Triangles(TriangleSoup::new(vec![big; 9])),
        placement: Placement::default(),
    }]);
    assert!(
        r.is_err(),
        "BLOCKER2a: center overflow produced finite-in/non-finite-out geometry"
    );
    // and nothing was committed
    assert_eq!(p.model_count(), 0, "BLOCKER2a: partial mutation on failure");
}

/// BLOCKER 2b — rotation rebake overflow: a finite, symmetric, large-extent
/// mesh centers fine (bbox center = 0), but `x*c - y*s` at 45 deg overflows
/// f32::MAX. The rebake must re-assert finiteness and reject, with NO partial
/// mutation (the model's geometry is unchanged after the failed rotate).
#[test]
fn blocker2b_rotation_overflow_rejected_no_partial_mutation() {
    let big = 3.0e38f32;
    // verts span ±big on x and y so bbox center is 0 (centering is a no-op)
    let soup = vec![big, -big, 0.0, -big, big, 0.0, 0.0, 0.0, 0.0];
    let mut p = Project::new_empty();
    p.add_models(&[ModelSource {
        name: "rot".into(),
        geometry: ModelGeometry::Triangles(TriangleSoup::new(soup)),
        placement: Placement::default(),
    }])
    .expect("centered symmetric large mesh is finite");
    let before = p.model_triangles(0).unwrap();
    let r = p.rotate_model_z(0, 45.0);
    assert!(
        r.is_err(),
        "BLOCKER2b: rotation overflow baked non-finite geometry"
    );
    let after = p.model_triangles(0).unwrap();
    assert_eq!(
        before, after,
        "BLOCKER2b: failed rotate left partial mutation"
    );
}

/// MAJOR 1 — a zero-triangle support-record mesh must be rejected on attach
/// (mirrors the model path and probe5): its mesh_len=0 record has unprobed
/// ChituBox semantics.
#[test]
fn major1_empty_support_record_mesh_rejected() {
    let mut p = one_model_project();
    let empty = SupportRecord::new(
        SegmentType::SHAFT,
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        0.5,
        0.5,
        TriangleSoup::new(vec![]),
    );
    let r = p.attach_supports(&[SupportBatch {
        model_index: 0,
        records: vec![empty],
        mode: SupportMode::Replace,
        lift_mm: None,
    }]);
    assert!(r.is_err(), "MAJOR1: zero-triangle support record accepted");
}

/// MAJOR 2 — opaque ids are keyed on (kind, model_index), not enumeration
/// position: after removing model 0's supports, model 0's stale group_header id
/// must ERROR, never silently retarget model 1's group_header.
#[test]
fn major2_stale_opaque_id_errors_not_retargets() {
    let mut p = two_model_project();
    // attach supports to BOTH models so two group_header regions exist
    p.attach_supports(&[
        SupportBatch {
            model_index: 0,
            records: vec![support_rec()],
            mode: SupportMode::Replace,
            lift_mm: None,
        },
        SupportBatch {
            model_index: 1,
            records: vec![support_rec()],
            mode: SupportMode::Replace,
            lift_mm: None,
        },
    ])
    .unwrap();
    // grab model 0's group_header id
    let regions = p.opaque_regions();
    let gh0 = regions
        .iter()
        .find(|r| r.kind == "group_header" && r.model_index == Some(0))
        .expect("model 0 group_header region");
    let stale_id = gh0.id;
    // capture model 1's group_header bytes (must stay untouched)
    let gh1_before = {
        let gh1 = regions
            .iter()
            .find(|r| r.kind == "group_header" && r.model_index == Some(1))
            .expect("model 1 group_header region");
        p.opaque_region_bytes(gh1.id).unwrap()
    };
    // remove model 0's supports -> the enumeration shifts; a positional id
    // scheme would now map stale_id onto model 1's group_header.
    p.attach_supports(&[SupportBatch {
        model_index: 0,
        records: vec![],
        mode: SupportMode::Replace,
        lift_mm: None,
    }])
    .unwrap();
    let sentinel = vec![0xAAu8; gh1_before.len()];
    let r = p.overwrite_opaque_region_unchecked(stale_id, &sentinel);
    assert!(
        r.is_err(),
        "MAJOR2: stale opaque id silently retargeted after supports removed"
    );
    // model 1's group_header must be intact (not clobbered by the sentinel)
    let gh1 = p
        .opaque_regions()
        .into_iter()
        .find(|r| r.kind == "group_header" && r.model_index == Some(1))
        .expect("model 1 group_header still present");
    assert_eq!(
        p.opaque_region_bytes(gh1.id).unwrap(),
        gh1_before,
        "MAJOR2: model 1 group_header was clobbered via stale id"
    );
}

/// ROTATION CONSISTENCY (Blocker-1 positive guard + per-primitive coverage) —
/// after ONE Z-rotation the three rotation primitives (model mesh, support-blob
/// bytes, record endpoints) must all reflect the SAME single rotation. If a
/// blob were rotated more than once (the aliasing bug), it would sit at a
/// different angle than its endpoints; pinning all three to one angle closes
/// that gap. Runs synthetically (no corpus), unlike the corpus-gated positive
/// assembly test.
#[test]
fn rotation_consistency_all_primitives_agree() {
    let mut p = one_model_project(); // model mesh first vertex = (1,0,0)
                                     // Endpoints and blob first vertex are OFF the rotation axis so a 90 deg turn
                                     // is observable on every primitive.
    let rec = SupportRecord::new(
        SegmentType::SHAFT,
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        0.5,
        0.5,
        TriangleSoup::new(vec![2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0]),
    );
    p.attach_supports(&[SupportBatch {
        model_index: 0,
        records: vec![rec],
        mode: SupportMode::Replace,
        lift_mm: None,
    }])
    .unwrap();

    // angle-0 (pristine) values
    let m0 = p.model_triangles(0).unwrap();
    let s0 = p.supports(0).unwrap();
    let mesh0 = [m0[0], m0[1]];
    let blob0 = [s0[0].mesh.xyz[0], s0[0].mesh.xyz[1]];
    let start0 = s0[0].start;
    let end0 = s0[0].end;

    p.rotate_model_z(0, 90.0).unwrap();

    // 90 deg about +Z: (x, y) -> (-y, x). Every primitive must match this.
    let rot90 = |x: f32, y: f32| (-y, x);
    let m1 = p.model_triangles(0).unwrap();
    let s1 = p.supports(0).unwrap();

    let (emx, emy) = rot90(mesh0[0], mesh0[1]);
    assert!(
        (m1[0] - emx).abs() < 1e-4 && (m1[1] - emy).abs() < 1e-4,
        "model mesh did not rotate 90 deg: got ({},{})",
        m1[0],
        m1[1]
    );
    let (ebx, eby) = rot90(blob0[0], blob0[1]);
    assert!(
        (s1[0].mesh.xyz[0] - ebx).abs() < 1e-4 && (s1[0].mesh.xyz[1] - eby).abs() < 1e-4,
        "support blob angle disagrees with model mesh (double-rotation?): got ({},{})",
        s1[0].mesh.xyz[0],
        s1[0].mesh.xyz[1]
    );
    let (esx, esy) = rot90(start0[0], start0[1]);
    assert!(
        (s1[0].start[0] - esx).abs() < 1e-4 && (s1[0].start[1] - esy).abs() < 1e-4,
        "record start endpoint angle disagrees: got {:?}",
        s1[0].start
    );
    let (eex, eey) = rot90(end0[0], end0[1]);
    assert!(
        (s1[0].end[0] - eex).abs() < 1e-4 && (s1[0].end[1] - eey).abs() < 1e-4,
        "record end endpoint angle disagrees: got {:?}",
        s1[0].end
    );
}

#[test]
fn probe5_zero_triangle_stl_rejected() {
    let mut stl = vec![0u8; 80];
    stl.extend_from_slice(&0u32.to_le_bytes());
    let dir = std::env::temp_dir();
    let path = dir.join("probe_empty.stl");
    std::fs::write(&path, &stl).unwrap();
    let mut p = Project::new_empty();
    let r = p.add_models(&[ModelSource {
        name: "empty".into(),
        geometry: ModelGeometry::StlPath(path.clone()),
        placement: Placement::default(),
    }]);
    assert!(
        r.is_err(),
        "PROBE5: zero-triangle STL must be rejected (unprobed ChituBox behavior)"
    );
}
