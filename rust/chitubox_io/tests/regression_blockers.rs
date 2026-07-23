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
    println!("PROBE1 first vertex after 90+attach+90: ({}, {}, {})", m[0], m[1], m[2]);
    assert!(
        (m[0] - (-1.0)).abs() < 1e-4 && m[1].abs() < 1e-4,
        "expected (-1,0,0); got ({}, {}, {}) => rotation state corrupted by attach_supports",
        m[0], m[1], m[2]
    );
}

#[test]
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
    p3.rotate_models_z(&[ZRotation { model_index: 0, delta_deg: 90.0 }]).unwrap();
    match p3.to_bytes() {
        Ok(out) => {
            println!(
                "PROBE3: rotate+save on dead-tail file: in {} bytes, out {} bytes",
                total_in,
                out.len()
            );
            assert_eq!(out.len(), total_in, "PROBE3: unknown dead smesh bytes silently destroyed, verify passed");
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
    for _ in 0..3 { tri.extend_from_slice(&0.0f32.to_le_bytes()); } // normal
    let verts: [f32; 9] = [f32::NAN, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0];
    for v in verts { tri.extend_from_slice(&v.to_le_bytes()); }
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
    assert!(header_len > 0, "expected a non-empty group header to excise");

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
