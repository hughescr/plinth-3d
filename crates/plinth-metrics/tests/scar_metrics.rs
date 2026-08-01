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
        [3, 0, 4],
        [3, 4, 7],
    ]
    .into_iter()
    .flat_map(|triangle| triangle.into_iter().flat_map(|index| points[index]))
    .collect()
}

fn sphere_mesh(centre: [f32; 3], radius: f32) -> Vec<f32> {
    let [x, y, z] = centre;
    let points = [
        [x + radius, y, z],
        [x - radius, y, z],
        [x, y + radius, z],
        [x, y - radius, z],
        [x, y, z + radius],
        [x, y, z - radius],
    ];
    [
        [0, 2, 4],
        [2, 1, 4],
        [1, 3, 4],
        [3, 0, 4],
        [2, 0, 5],
        [1, 2, 5],
        [3, 1, 5],
        [0, 3, 5],
    ]
    .into_iter()
    .flat_map(|triangle| triangle.into_iter().flat_map(|index| points[index]))
    .collect()
}

fn record(kind: SegmentType, start: [f32; 3], end: [f32; 3], r0: f32, r1: f32) -> SupportRecord {
    SupportRecord::new(kind, start, end, r0, r1, TriangleSoup::new(cube(0.1)))
}

fn ball(kind: SegmentType, start: [f32; 3], end: [f32; 3], radius: f32) -> SupportRecord {
    let centre = [
        (start[0] + end[0]) * 0.5,
        (start[1] + end[1]) * 0.5,
        (start[2] + end[2]) * 0.5,
    ];
    SupportRecord::new(
        kind,
        start,
        end,
        radius,
        radius,
        TriangleSoup::new(sphere_mesh(centre, radius)),
    )
}

fn stub(x: f32, contact_z: f32) -> [SupportRecord; 2] {
    // Emitter order is not meaningful: these records are deliberately returned
    // in a pair only because the caller chooses where to put them in the file.
    [
        ball(
            SegmentType::TIP,
            [x, 0.0, contact_z],
            [x, 0.0, contact_z + 0.6],
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

fn foot_ball(x: f32, z: f32, radius: f32) -> SupportRecord {
    ball(
        SegmentType::SHAFT,
        [x, 0.0, z + radius],
        [x, 0.0, z - radius],
        radius,
    )
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
    project.add_models(&sources).expect("write model");
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
        .expect("attach supports");
    project.save(path).expect("write artifact");
}

fn reopen(path: &Path, name: &str) -> DiskArtifact {
    match load_artifacts(&[ArtifactRequest::new(
        ModelId::from(name),
        ArtifactDisposition::Produced {
            path: path.to_owned(),
        },
    )])
    .expect("reopen")
    .into_iter()
    .next()
    {
        Some(ArtifactLoad::Artifact(artifact)) => artifact,
        _ => panic!("expected artifact"),
    }
}

fn value(row: &ScarMetricRow, id: &str) -> f64 {
    row.entries()
        .into_iter()
        .find_map(|(candidate, value)| (candidate.as_str() == id).then_some(value.get()))
        .unwrap()
}

#[test]
fn nonadjacent_reversed_poles_reconstruct_the_stub_and_do_not_cap_patch_area() {
    let temp = TempDir::new();
    let path = temp.path().join("stub.chitubox");
    let mut pair = stub(0.0, 0.8);
    pair.swap(0, 1);
    // Put an unrelated record in the middle.  Reversing the ball poles is legal.
    let reversed = ball(SegmentType::TIP, [0.0, 0.0, 1.4], [0.0, 0.0, 0.8], 0.3);
    let records = vec![
        pair[0].clone(),
        record(
            SegmentType::JOINER,
            [4.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            0.1,
            0.1,
        ),
        reversed,
    ];
    write_artifact(&path, &[(cube(2.0), records)]);
    let row = &scar_metric_rows(
        &reopen(&path, "stub"),
        &[ScarMetricInput { source_index: 0 }],
    )
    .expect("nonadjacent stub")[0];
    assert_eq!(
        SCAR_METRIC_ORDER,
        ["scar_area", "scar_worst_patch", "scar_weighted"]
    );
    // The ball term alone is pi*(.3^2 - .2^2); a positive shaft term proves
    // the full member interval, rather than a sphere-diameter proxy, was used.
    assert!(value(row, "scar_area") > std::f64::consts::PI * 0.05);
    assert_eq!(value(row, "scar_area"), value(row, "scar_worst_patch"));
}

#[test]
fn standalone_taper_and_constant_joiner_feet_use_later_vertical_balls() {
    let temp = TempDir::new();
    let path = temp.path().join("feet.chitubox");
    let records = vec![
        record(
            SegmentType::TAPER,
            [-0.5, 0.0, 0.0],
            [-0.5, 0.0, 0.8],
            0.2,
            0.25,
        ),
        foot_ball(3.0, 0.0, 0.4), // ordinary, unpaired ball is ignored
        record(
            SegmentType::JOINER,
            [0.5, 0.0, 0.0],
            [0.5, 0.0, 0.8],
            0.2,
            0.2,
        ),
        foot_ball(-0.5, 0.8, 0.35),
        foot_ball(0.5, 0.8, 0.28),
    ];
    write_artifact(&path, &[(cube(2.0), records)]);
    let row = &scar_metric_rows(
        &reopen(&path, "feet"),
        &[ScarMetricInput { source_index: 0 }],
    )
    .expect("two feet")[0];
    assert!(value(row, "scar_area") > 0.0);
    assert!(value(row, "scar_worst_patch") < value(row, "scar_area"));
}

#[test]
fn unmatched_or_ambiguous_contact_records_fail_closed() {
    let temp = TempDir::new();
    let unmatched = temp.path().join("unmatched.chitubox");
    write_artifact(
        &unmatched,
        &[(
            cube(2.0),
            vec![record(
                SegmentType::TAPER,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                0.2,
                0.2,
            )],
        )],
    );
    assert!(matches!(
        scar_metric_rows(
            &reopen(&unmatched, "unmatched"),
            &[ScarMetricInput { source_index: 0 }]
        ),
        Err(ScarMetricError::UnmatchedTaper { .. })
    ));
    let ambiguous = temp.path().join("ambiguous.chitubox");
    let pair = stub(0.0, 0.8);
    write_artifact(
        &ambiguous,
        &[(
            cube(2.0),
            vec![pair[0].clone(), pair[1].clone(), pair[1].clone()],
        )],
    );
    assert!(matches!(
        scar_metric_rows(
            &reopen(&ambiguous, "ambiguous"),
            &[ScarMetricInput { source_index: 0 }]
        ),
        Err(ScarMetricError::AmbiguousContact { .. })
    ));
}

#[test]
fn rows_are_ordered_and_source_indices_are_checked() {
    let temp = TempDir::new();
    let path = temp.path().join("two.chitubox");
    write_artifact(
        &path,
        &[(cube(2.0), stub(0.0, 0.8).into()), (cube(2.0), Vec::new())],
    );
    let artifact = reopen(&path, "two");
    let rows = scar_metric_rows(
        &artifact,
        &[
            ScarMetricInput { source_index: 0 },
            ScarMetricInput { source_index: 1 },
        ],
    )
    .expect("ordered batch");
    assert_eq!(rows[0].source_index, 0);
    assert_eq!(value(&rows[1], "scar_area"), 0.0);
    assert!(matches!(
        scar_metric_rows(
            &artifact,
            &[
                ScarMetricInput { source_index: 1 },
                ScarMetricInput { source_index: 0 }
            ]
        ),
        Err(ScarMetricError::InputSourceIndex { .. })
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
        fs::create_dir_all(&path).unwrap();
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
