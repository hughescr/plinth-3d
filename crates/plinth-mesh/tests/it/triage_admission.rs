use plinth_mesh::{
    make_box, triage_components, triage_settings, Mesh, TriageAdmissionError,
    TriageTopologyAdmission, TriageTopologyLimits, WeldPolicy,
};

#[derive(Clone, Copy)]
struct RawCaps {
    vertices: usize,
    faces: usize,
    retained_bytes: usize,
    edges: usize,
    incidences: usize,
    probes: usize,
}

impl RawCaps {
    fn measure(mesh: &Mesh) -> Self {
        let faces = mesh.tris().len();
        let samples = triage_settings()
            .expect("registered triage settings")
            .containment_samples();
        Self {
            vertices: mesh.verts().len(),
            faces,
            retained_bytes: mesh
                .retained_buffer_bytes()
                .expect("representable source buffers"),
            edges: faces.checked_mul(3).expect("small fixture"),
            incidences: faces.checked_mul(3).expect("small fixture"),
            probes: faces.checked_mul(samples).expect("small fixture"),
        }
    }

    fn exact(self) -> TriageTopologyLimits {
        TriageTopologyLimits::new(
            self.vertices,
            self.faces,
            self.retained_bytes,
            self.edges,
            self.incidences,
            self.probes,
        )
    }

    fn only(self, field: &'static str, cap: usize) -> TriageTopologyLimits {
        let mut limits = [usize::MAX; 6];
        let index = match field {
            "raw vertices" => 0,
            "raw faces" => 1,
            "retained source buffer bytes" => 2,
            "edge occurrences" => 3,
            "vertex incidences" => 4,
            "containment probe slots" => 5,
            _ => panic!("unknown cap field"),
        };
        limits[index] = cap;
        TriageTopologyLimits::new(
            limits[0], limits[1], limits[2], limits[3], limits[4], limits[5],
        )
    }

    fn fields(self) -> [(&'static str, usize); 6] {
        [
            ("raw vertices", self.vertices),
            ("raw faces", self.faces),
            ("retained source buffer bytes", self.retained_bytes),
            ("edge occurrences", self.edges),
            ("vertex incidences", self.incidences),
            ("containment probe slots", self.probes),
        ]
    }
}

#[test]
fn every_exact_cap_runs_and_cap_minus_one_refuses_the_actual_admitted_path() {
    let mesh = duplicate_vertex_tetra_with_spare_capacity();
    let caps = RawCaps::measure(&mesh);
    let logical_bytes = std::mem::size_of_val(mesh.verts()) + std::mem::size_of_val(mesh.tris());
    assert!(
        caps.retained_bytes > logical_bytes,
        "fixture must prove capacity, not length, controls retained bytes"
    );

    for (field, exact) in caps.fields() {
        let admission = TriageTopologyAdmission::from_raw_mesh(&mesh, caps.only(field, exact))
            .unwrap_or_else(|error| panic!("exact {field} cap rejected: {error}"));
        let triaged = triage_components(admission, WeldPolicy::ExactBits)
            .unwrap_or_else(|error| panic!("exact {field} admitted path failed: {error}"));
        assert_eq!(triaged.mesh().verts().len(), 4, "{field} exact-bit weld");

        assert!(matches!(
            TriageTopologyAdmission::from_raw_mesh(&mesh, caps.only(field, exact - 1)),
            Err(TriageAdmissionError::LimitExceeded {
                field: actual,
                limit,
                ..
            }) if actual == field && limit == exact - 1
        ));
    }
}

#[test]
fn exact_cap_duplicate_vertex_weld_reduction_preserves_automatic_invariants() {
    let mesh = duplicate_vertex_tetra_with_spare_capacity();
    let admission = TriageTopologyAdmission::from_raw_mesh(&mesh, RawCaps::measure(&mesh).exact())
        .expect("exact caps admit");
    let triaged = triage_components(admission, WeldPolicy::ExactBits)
        .expect("admitted path checks no-growth and raw-bounds invariants");
    assert_eq!(mesh.verts().len(), 12);
    assert_eq!(triaged.mesh().verts().len(), 4);
    assert_eq!(triaged.mesh().tris().len(), mesh.tris().len());
}

#[test]
fn exact_cap_qualified_shard_stripping_preserves_automatic_invariants() {
    let main = make_box([10.0, 10.0, 10.0], [0.0, 0.0, 0.0]).expect("main box");
    let shard = tetra_at([5.0, 5.0, 0.0], 0.05);
    let input = join(&main, &shard);
    let admission =
        TriageTopologyAdmission::from_raw_mesh(&input, RawCaps::measure(&input).exact())
            .expect("exact caps admit");
    let triaged = triage_components(admission, WeldPolicy::ExactBits)
        .expect("admitted path checks stripped output no-growth and raw bounds");
    assert_eq!(triaged.summary().stripped_components().len(), 1);
    assert_eq!(triaged.mesh().tris().len(), main.tris().len());
    assert!(triaged.mesh().verts().len() <= input.verts().len());
}

fn duplicate_vertex_tetra_with_spare_capacity() -> Mesh {
    let points = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ];
    let source_faces = [[0_usize, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
    let mut vertices = Vec::with_capacity(24);
    let mut faces = Vec::with_capacity(10);
    for source in source_faces {
        let base = u32::try_from(vertices.len()).expect("small fixture");
        vertices.extend(source.map(|index| points[index]));
        faces.push([base, base + 1, base + 2]);
    }
    assert!(vertices.capacity() > vertices.len());
    assert!(faces.capacity() > faces.len());
    Mesh::new(vertices, faces).expect("finite duplicate-coordinate tetra soup")
}

fn tetra_at(origin: [f64; 3], side: f64) -> Mesh {
    let [x, y, z] = origin;
    Mesh::new(
        vec![origin, [x - side, y, z], [x, y - side, z], [x, y, z + side]],
        vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
    )
    .expect("tetra")
}

fn join(left: &Mesh, right: &Mesh) -> Mesh {
    let mut vertices = left.verts().to_vec();
    let offset = u32::try_from(vertices.len()).expect("small fixture");
    vertices.extend_from_slice(right.verts());
    let mut faces = left.tris().to_vec();
    faces.extend(right.tris().iter().map(|face| {
        [
            face[0].checked_add(offset).expect("small fixture"),
            face[1].checked_add(offset).expect("small fixture"),
            face[2].checked_add(offset).expect("small fixture"),
        ]
    }));
    Mesh::new(vertices, faces).expect("joined mesh")
}
