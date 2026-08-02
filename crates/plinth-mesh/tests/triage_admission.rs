use plinth_mesh::{
    make_box_default, triage_components, Mesh, TriageAdmissionError, TriageTopologyAdmission,
    TriageTopologyLimits, WeldPolicy,
};

fn limits_for(mesh: &Mesh, samples: usize) -> TriageTopologyLimits {
    let faces = mesh.tris().len();
    TriageTopologyLimits::new(
        mesh.verts().len(),
        faces,
        mesh.retained_buffer_bytes()
            .expect("representable source buffers"),
        faces.checked_mul(3).expect("small fixture"),
        faces.checked_mul(3).expect("small fixture"),
        faces.checked_mul(samples).expect("small fixture"),
        samples,
    )
}

#[test]
fn admission_accepts_exact_caps_and_records_checked_raw_work() {
    let mesh = make_box_default().expect("box");
    let admission = TriageTopologyAdmission::from_raw_mesh(&mesh, limits_for(&mesh, 32))
        .expect("exact cap accepts");

    assert_eq!(admission.raw_vertices(), mesh.verts().len());
    assert_eq!(admission.raw_faces(), mesh.tris().len());
    assert_eq!(admission.edge_occurrences(), 3 * mesh.tris().len());
    assert_eq!(admission.vertex_incidences(), 3 * mesh.tris().len());
    assert_eq!(admission.containment_probe_slots(), 32 * mesh.tris().len());
    assert_eq!(
        admission.retained_source_buffer_bytes(),
        mesh.retained_buffer_bytes()
            .expect("representable source buffers")
    );
}

#[test]
fn admission_rejects_cap_minus_one_before_topology_work_on_open_soup() {
    let closed = make_box_default().expect("box");
    let open = Mesh::new(closed.verts().to_vec(), closed.tris()[1..].to_vec()).expect("open soup");
    let limits = TriageTopologyLimits::new(
        open.verts().len().saturating_sub(1),
        open.tris().len(),
        open.retained_buffer_bytes()
            .expect("representable source buffers"),
        3 * open.tris().len(),
        3 * open.tris().len(),
        open.tris().len(),
        1,
    );

    assert!(matches!(
        TriageTopologyAdmission::from_raw_mesh(&open, limits),
        Err(TriageAdmissionError::LimitExceeded {
            field: "raw vertices",
            ..
        })
    ));
}

#[test]
fn admission_reports_checked_containment_slot_overflow() {
    let mesh = make_box_default().expect("box");
    let limits = TriageTopologyLimits::new(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
    );
    assert!(matches!(
        TriageTopologyAdmission::from_raw_mesh(&mesh, limits),
        Err(TriageAdmissionError::ArithmeticOverflow {
            field: "containment probe slots"
        })
    ));
}

#[test]
fn exact_bit_triage_output_cannot_expand_raw_voxel_grid_inputs() {
    let mesh = make_box_default().expect("box");
    let admission =
        TriageTopologyAdmission::from_raw_mesh(&mesh, limits_for(&mesh, 32)).expect("admitted");
    let triaged = triage_components(&mesh, WeldPolicy::ExactBits).expect("triaged");
    admission
        .verify_exact_bit_triage_output(&triaged)
        .expect("triage cannot grow raw counts or bounds");
}
