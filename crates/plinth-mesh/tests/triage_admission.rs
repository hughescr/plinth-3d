use plinth_mesh::{
    make_box_default, triage_components, triage_settings, Mesh, TriageAdmissionError,
    TriageTopologyAdmission, TriageTopologyLimits, WeldPolicy,
};

fn limits_for(mesh: &Mesh) -> TriageTopologyLimits {
    let faces = mesh.tris().len();
    let samples = triage_settings()
        .expect("registered triage settings")
        .containment_samples();
    TriageTopologyLimits::new(
        mesh.verts().len(),
        faces,
        mesh.retained_buffer_bytes()
            .expect("representable source buffers"),
        faces.checked_mul(3).expect("small fixture"),
        faces.checked_mul(3).expect("small fixture"),
        faces.checked_mul(samples).expect("small fixture"),
    )
}

#[test]
fn admission_accepts_exact_caps_and_records_checked_raw_work() {
    let mesh = make_box_default().expect("box");
    let admission = TriageTopologyAdmission::from_raw_mesh(&mesh, limits_for(&mesh))
        .expect("exact cap accepts");
    let samples = triage_settings()
        .expect("registered triage settings")
        .containment_samples();

    assert_eq!(admission.raw_vertices(), mesh.verts().len());
    assert_eq!(admission.raw_faces(), mesh.tris().len());
    assert_eq!(admission.edge_occurrences(), 3 * mesh.tris().len());
    assert_eq!(admission.vertex_incidences(), 3 * mesh.tris().len());
    assert_eq!(
        admission.containment_probe_slots(),
        samples * mesh.tris().len()
    );
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
        usize::MAX,
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
fn exact_cap_runs_the_actual_admitted_triage_path_and_checks_output_invariants() {
    let mesh = make_box_default().expect("box");
    let admission =
        TriageTopologyAdmission::from_raw_mesh(&mesh, limits_for(&mesh)).expect("exact cap admits");
    let triaged = triage_components(admission, WeldPolicy::ExactBits).expect("admitted triage");
    assert!(triaged.mesh().verts().len() <= mesh.verts().len());
    assert!(triaged.mesh().tris().len() <= mesh.tris().len());
}
