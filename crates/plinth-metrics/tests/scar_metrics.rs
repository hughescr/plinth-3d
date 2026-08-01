use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use chitubox_io::{
    ModelGeometry, ModelSource, Placement, Project, SegmentType, SupportBatch, SupportMode,
    SupportRecord, TriangleSoup,
};
use plinth_core::ModelId;
use plinth_metrics::{
    load_artifacts, scar_metric_rows, ArtifactDisposition, ArtifactLoad, ArtifactRequest,
    DiskArtifact, ScarMetricError, ScarMetricInput, ScarMetricRow, SCAR_METRIC_ORDER,
};

static TEMP_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

fn cube(side: f32) -> Vec<f32> {
    let h = side / 2.0;
    let points = [
        [-h, -h, -h],
        [h, -h, -h],
        [h, h, -h],
        [-h, h, -h],
        [-h, -h, h],
        [h, -h, h],
        [h, h, h],
        [-h, h, h],
    ];
    [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ]
    .into_iter()
    .flat_map(|triangle| triangle.into_iter().flat_map(|index| points[index]))
    .collect()
}

fn tetrahedron(scale: f32) -> Vec<f32> {
    let points = [
        [0.0, 0.0, 0.0],
        [scale, 0.0, 0.0],
        [0.0, scale, 0.0],
        [0.0, 0.0, scale],
    ];
    [[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]]
        .into_iter()
        .flat_map(|triangle| triangle.into_iter().flat_map(|index| points[index]))
        .collect()
}

fn record(kind: SegmentType, start: [f32; 3], end: [f32; 3], r0: f32, r1: f32) -> SupportRecord {
    SupportRecord::new(
        kind,
        start,
        end,
        r0,
        r1,
        TriangleSoup::new(tetrahedron(0.1)),
    )
}

fn contact_pair(contact_z: f32) -> [SupportRecord; 2] {
    contact_pair_at(0.0, contact_z)
}

fn contact_pair_at(x: f32, contact_z: f32) -> [SupportRecord; 2] {
    // This is the literal type-7 -> type-1 relation emitted by the frozen
    // writer: the taper starts at the type-7 sphere centre, not at its inner
    // pole, and its radius is the neck radius rather than the ball radius.
    [
        record(
            SegmentType::TIP,
            [x, 0.0, contact_z],
            [x, 0.0, contact_z + 0.6],
            0.3,
            0.3,
        ),
        record(
            SegmentType::TAPER,
            [x, 0.0, contact_z + 0.3],
            [x, 0.0, 2.0],
            0.2,
            0.4,
        ),
    ]
}

fn write_artifact(path: &Path, models: &[(Vec<f32>, Vec<SupportRecord>)]) {
    let mut project = Project::new_empty();
    let sources = models
        .iter()
        .enumerate()
        .map(|(index, (triangles, _))| ModelSource {
            name: format!("scar-{index}"),
            geometry: ModelGeometry::Triangles(TriangleSoup::new(triangles.clone())),
            placement: Placement::default(),
        })
        .collect::<Vec<_>>();
    project.add_models(&sources).expect("write analytic model");
    project
        .attach_supports(
            &models
                .iter()
                .enumerate()
                .map(|(index, (_, records))| SupportBatch {
                    model_index: index,
                    records: records.clone(),
                    mode: SupportMode::Replace,
                    lift_mm: None,
                })
                .collect::<Vec<_>>(),
        )
        .expect("attach analytic supports");
    project.save(path).expect("write disk artifact");
}

fn reopen(path: &Path, name: &str) -> DiskArtifact {
    let rows = load_artifacts(&[ArtifactRequest::new(
        ModelId::from(name),
        ArtifactDisposition::Produced {
            path: path.to_owned(),
        },
    )])
    .expect("fresh disk reopen");
    match rows.into_iter().next() {
        Some(ArtifactLoad::Artifact(artifact)) => artifact,
        _ => panic!("expected successful artifact"),
    }
}

fn value(row: &ScarMetricRow, id: &str) -> f64 {
    row.entries()
        .into_iter()
        .find_map(|(candidate, value)| (candidate.as_str() == id).then_some(value.get()))
        .unwrap_or_else(|| panic!("missing {id}"))
}

#[test]
fn disk_contact_pair_uses_scar_area_cap_geometry_not_sphere_diameter_or_support_mesh_area() {
    let temp = TempDir::new();
    let path = temp.path().join("one-contact.chitubox");
    write_artifact(&path, &[(cube(2.0), contact_pair(0.8).into())]);
    let rows = scar_metric_rows(
        &reopen(&path, "one-contact"),
        &[ScarMetricInput { source_index: 0 }],
    )
    .expect("well-formed disk contact pair");
    assert_eq!(
        SCAR_METRIC_ORDER,
        ["scar_area", "scar_worst_patch", "scar_weighted"]
    );
    let row = &rows[0];
    // Contact point z=.8, inward exit at z=-1, r(T)=.4.  SCAR-AREA-CAP:
    // min(pi*.4^2, 2*.4*1.8) + pi*(.3^2-.2^2).
    let expected = std::f64::consts::PI * 0.16 + std::f64::consts::PI * 0.05;
    assert!(
        (value(row, "scar_area") - expected).abs() < 1e-6,
        "actual={} expected={expected}",
        value(row, "scar_area")
    );
    assert!((value(row, "scar_worst_patch") - expected).abs() < 1e-6);
    assert!(value(row, "scar_weighted") >= 0.0 && value(row, "scar_weighted") <= expected);
}

#[test]
fn clustered_and_separated_contact_discs_preserve_total_but_change_worst_patch() {
    let temp = TempDir::new();
    let clustered_path = temp.path().join("clustered.chitubox");
    let mut clustered = contact_pair(0.8).into_iter().collect::<Vec<_>>();
    clustered.extend(contact_pair(0.8));
    write_artifact(&clustered_path, &[(cube(2.0), clustered)]);
    let clustered = scar_metric_rows(
        &reopen(&clustered_path, "clustered"),
        &[ScarMetricInput { source_index: 0 }],
    )
    .expect("clustered contact pairs");
    let separated_path = temp.path().join("separated.chitubox");
    let mut separated_records = contact_pair_at(-0.6, 0.8).into_iter().collect::<Vec<_>>();
    separated_records.extend(contact_pair_at(0.6, 0.8));
    write_artifact(&separated_path, &[(cube(2.0), separated_records)]);
    let separated = scar_metric_rows(
        &reopen(&separated_path, "separated"),
        &[ScarMetricInput { source_index: 0 }],
    )
    .expect("separated contact pairs");
    assert!((value(&clustered[0], "scar_area") - value(&separated[0], "scar_area")).abs() < 1e-6);
    assert_eq!(
        value(&clustered[0], "scar_worst_patch"),
        value(&clustered[0], "scar_area")
    );
    assert!(value(&separated[0], "scar_worst_patch") < value(&separated[0], "scar_area"));
}

#[test]
fn type_one_without_its_preceding_type_seven_fails_named_instead_of_becoming_zero_scar() {
    let temp = TempDir::new();
    let path = temp.path().join("orphan-taper.chitubox");
    write_artifact(
        &path,
        &[(
            cube(2.0),
            vec![record(
                SegmentType::TAPER,
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 2.0],
                0.2,
                0.4,
            )],
        )],
    );
    assert!(matches!(
        scar_metric_rows(
            &reopen(&path, "orphan-taper"),
            &[ScarMetricInput { source_index: 0 }]
        ),
        Err(ScarMetricError::OrphanTaper { .. })
    ));
}

#[test]
fn rows_are_ordered_and_source_indices_are_checked() {
    let temp = TempDir::new();
    let path = temp.path().join("two-models.chitubox");
    write_artifact(
        &path,
        &[
            (cube(2.0), contact_pair(0.8).into()),
            (cube(2.0), Vec::new()),
        ],
    );
    let artifact = reopen(&path, "two-models");
    let rows = scar_metric_rows(
        &artifact,
        &[
            ScarMetricInput { source_index: 0 },
            ScarMetricInput { source_index: 1 },
        ],
    )
    .expect("ordered batch");
    assert_eq!(rows[0].source_index, 0);
    assert_eq!(rows[1].source_index, 1);
    assert_eq!(value(&rows[1], "scar_area"), 0.0);
    assert!(matches!(
        scar_metric_rows(
            &artifact,
            &[
                ScarMetricInput { source_index: 1 },
                ScarMetricInput { source_index: 0 }
            ]
        ),
        Err(ScarMetricError::InputSourceIndex { row: 0, .. })
    ));
}

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "plinth-metrics-scar-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temporary directory");
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
