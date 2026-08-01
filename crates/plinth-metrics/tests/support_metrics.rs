use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use chitubox_io::{
    ModelGeometry, ModelSource, Placement, Project, SegmentType, SupportBatch, SupportMode,
    SupportRecord, TriangleSoup,
};
use plinth_core::ModelId;
use plinth_metrics::{
    load_artifacts, support_metric_rows, ArmLoadInput, ArtifactDisposition, ArtifactLoad,
    ArtifactRequest, DiskArtifact, SupportMetricError, SupportMetricInput, VolumeSubject,
    SUPPORT_METRIC_ORDER,
};

static TEMP_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

fn tetrahedron(scale: f32, reverse: bool) -> Vec<f32> {
    let vertices = [
        [0.0, 0.0, 0.0],
        [scale, 0.0, 0.0],
        [0.0, scale, 0.0],
        [0.0, 0.0, scale],
    ];
    let faces = if reverse {
        [[0, 1, 2], [0, 3, 1], [1, 3, 2], [2, 3, 0]]
    } else {
        [[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]]
    };
    faces
        .into_iter()
        .flat_map(|face| face.into_iter().flat_map(|index| vertices[index]))
        .collect()
}

fn frustum(r0: f32, r1: f32, height: f32) -> Vec<f32> {
    let ring0 = [
        [r0, 0.0, 0.0],
        [0.0, r0, 0.0],
        [-r0, 0.0, 0.0],
        [0.0, -r0, 0.0],
    ];
    let ring1 = [
        [r1, 0.0, height],
        [0.0, r1, height],
        [-r1, 0.0, height],
        [0.0, -r1, height],
    ];
    let mut triangles = Vec::new();
    for index in 0..4 {
        let next = (index + 1) % 4;
        triangles.extend_from_slice(&[ring0[index], ring1[next], ring1[index]]);
        triangles.extend_from_slice(&[ring0[index], ring0[next], ring1[next]]);
        triangles.extend_from_slice(&[[0.0, 0.0, 0.0], ring0[next], ring0[index]]);
        triangles.extend_from_slice(&[[0.0, 0.0, height], ring1[index], ring1[next]]);
    }
    triangles.into_iter().flatten().collect()
}

fn record(kind: SegmentType, r0: f32, r1: f32, height: f32, reverse: bool) -> SupportRecord {
    let mut mesh = frustum(r0, r1, height);
    if reverse {
        for triangle in mesh.chunks_exact_mut(9) {
            triangle.swap(3, 6);
            triangle.swap(4, 7);
            triangle.swap(5, 8);
        }
    }
    SupportRecord::new(
        kind,
        [0.0, 0.0, 0.0],
        [0.0, 0.0, height],
        r0,
        r1,
        TriangleSoup::new(mesh),
    )
}

fn write_artifact(path: &Path, models: &[(Vec<f32>, Vec<SupportRecord>)]) {
    let mut project = Project::new_empty();
    let sources = models
        .iter()
        .enumerate()
        .map(|(index, (model, _))| ModelSource {
            name: format!("support-metric-{index}"),
            geometry: ModelGeometry::Triangles(TriangleSoup::new(model.clone())),
            placement: Placement::default(),
        })
        .collect::<Vec<_>>();
    project.add_models(&sources).expect("write analytic models");
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
        .expect("attach analytic support records as one batch");
    project.save(path).expect("write analytic disk artifact");
}

fn reopen(path: &Path, name: &str) -> DiskArtifact {
    let rows = load_artifacts(&[ArtifactRequest::new(
        ModelId::from(name),
        ArtifactDisposition::Produced {
            path: path.to_owned(),
        },
    )])
    .expect("fresh read of test artifact");
    match rows.into_iter().next() {
        Some(ArtifactLoad::Artifact(artifact)) => artifact,
        _ => panic!("test artifact must be a successful disk artifact"),
    }
}

fn value(row: &plinth_metrics::SupportMetricRow, id: &str) -> f64 {
    row.entries()
        .into_iter()
        .find_map(|(candidate, value)| (candidate.as_str() == id).then_some(value.get()))
        .unwrap_or_else(|| panic!("missing metric {id}"))
}

#[test]
fn resin_uses_actual_record_meshes_and_keeps_total_arithmetic_exact() {
    let temp = TempDir::new();
    let path = temp.path().join("frusta.chitubox");
    // A four-sided capped frustum has half the circular-frustum volume.  The
    // expected value below is therefore derived from the emitted mesh itself,
    // not from a radius/diameter interpretation of record metadata.
    write_artifact(
        &path,
        &[(
            tetrahedron(1.0, false),
            vec![
                record(SegmentType::TAPER, 1.0, 1.0, 2.0, false),
                record(SegmentType::TAPER, 2.0, 1.0, 3.0, false),
            ],
        )],
    );
    let artifact = reopen(&path, "frusta");
    let rows = support_metric_rows(
        &artifact,
        &[SupportMetricInput {
            source_index: 0,
            arms: vec![
                ArmLoadInput {
                    support_index: 0,
                    voronoi_area_mm2: std::f64::consts::PI * 5.0,
                },
                ArmLoadInput {
                    support_index: 1,
                    voronoi_area_mm2: std::f64::consts::PI * 4.0 * 7.0,
                },
            ],
        }],
    )
    .expect("batch metric computation");
    let row = &rows[0];
    assert_eq!(
        SUPPORT_METRIC_ORDER,
        [
            "max_x",
            "support_count",
            "resin_model",
            "resin_support",
            "resin_total"
        ]
    );
    assert!(
        (value(row, "max_x") - 7.0).abs() < 1e-12,
        "per-arm maximum, not a group average"
    );
    assert_eq!(value(row, "support_count"), 2.0);
    assert!((value(row, "resin_model") - (1.0 / 6_000.0)).abs() < 1e-12);
    // The four-point diamond cylinder is 4 mm^3; its tapered companion is
    // 14 mm^3.  This confirms actual emitted mesh volume, not a circular
    // metadata-frustum approximation.
    assert!(
        (value(row, "resin_support") - 18.0 / 1_000.0).abs() < 1e-12,
        "actual support mL: {}",
        value(row, "resin_support")
    );
    assert_eq!(
        value(row, "resin_total"),
        value(row, "resin_model") + value(row, "resin_support"),
        "total is constructed only from its two component metrics"
    );
}

#[test]
fn arm_inputs_fail_closed_for_zero_radius_negative_area_and_non_contact_records() {
    let temp = TempDir::new();
    let path = temp.path().join("invalid-arms.chitubox");
    let mut zero_radius = record(SegmentType::TAPER, 1.0, 1.0, 1.0, false);
    // The disk reader must accept this record's non-degenerate cached mesh
    // before the metric boundary rejects its semantic zero tip radius.
    zero_radius.r_start = 0.0;
    write_artifact(
        &path,
        &[(
            tetrahedron(1.0, false),
            vec![zero_radius, record(SegmentType::TIP, 1.0, 1.0, 1.0, false)],
        )],
    );
    let artifact = reopen(&path, "invalid-arms");
    let zero_radius = support_metric_rows(
        &artifact,
        &[SupportMetricInput {
            source_index: 0,
            arms: vec![ArmLoadInput {
                support_index: 0,
                voronoi_area_mm2: 1.0,
            }],
        }],
    );
    assert!(matches!(
        zero_radius,
        Err(SupportMetricError::InvalidTipRadius { .. })
    ));

    let negative_area = support_metric_rows(
        &artifact,
        &[SupportMetricInput {
            source_index: 0,
            arms: vec![ArmLoadInput {
                support_index: 0,
                voronoi_area_mm2: -1.0,
            }],
        }],
    );
    assert!(matches!(
        negative_area,
        Err(SupportMetricError::InvalidVoronoiArea { .. })
    ));

    let non_contact = support_metric_rows(
        &artifact,
        &[SupportMetricInput {
            source_index: 0,
            arms: vec![ArmLoadInput {
                support_index: 1,
                voronoi_area_mm2: 1.0,
            }],
        }],
    );
    assert!(matches!(
        non_contact,
        Err(SupportMetricError::ArmSupportNotContactTaper { .. })
    ));
}

#[test]
fn negative_mesh_winding_is_a_named_error_not_absolute_resin() {
    let temp = TempDir::new();
    let path = temp.path().join("negative.chitubox");
    write_artifact(
        &path,
        &[(
            tetrahedron(1.0, false),
            vec![record(SegmentType::TAPER, 1.0, 1.0, 1.0, true)],
        )],
    );
    let artifact = reopen(&path, "negative");
    assert!(matches!(
        support_metric_rows(
            &artifact,
            &[SupportMetricInput {
                source_index: 0,
                arms: vec![]
            }]
        ),
        Err(SupportMetricError::NegativeSignedVolume {
            subject: VolumeSubject::SupportRecord { support_index: 0 },
            ..
        })
    ));
}

#[test]
fn batch_rows_preserve_disk_model_order() {
    let temp = TempDir::new();
    let path = temp.path().join("ordered-batch.chitubox");
    write_artifact(
        &path,
        &[
            (
                tetrahedron(1.0, false),
                vec![record(SegmentType::TAPER, 1.0, 1.0, 1.0, false)],
            ),
            (
                tetrahedron(2.0, false),
                vec![
                    record(SegmentType::TAPER, 1.0, 1.0, 1.0, false),
                    record(SegmentType::TAPER, 1.0, 1.0, 1.0, false),
                ],
            ),
        ],
    );
    let artifact = reopen(&path, "ordered-batch");
    let rows = support_metric_rows(
        &artifact,
        &[
            SupportMetricInput {
                source_index: 0,
                arms: vec![],
            },
            SupportMetricInput {
                source_index: 1,
                arms: vec![],
            },
        ],
    )
    .expect("whole-artifact batch metrics preserve source ordering");
    assert_eq!(
        rows.iter().map(|row| row.source_index).collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(value(&rows[0], "support_count"), 1.0);
    assert_eq!(value(&rows[1], "support_count"), 2.0);
    assert!(value(&rows[1], "resin_model") > value(&rows[0], "resin_model"));
}

#[test]
fn ordered_batch_reopens_committed_sphere_without_a_producer_memory_escape_hatch() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sphere = root.join("corpus/baseline/Sphere.chitubox");
    assert!(
        sphere.is_file(),
        "required committed Sphere artifact missing: {}",
        sphere.display()
    );
    let artifact = reopen(&sphere, "Sphere");
    let rows = support_metric_rows(
        &artifact,
        &[SupportMetricInput {
            source_index: 0,
            arms: vec![],
        }],
    )
    .expect("committed artifact scores from a fresh disk reopen");
    let row = &rows[0];
    assert!(value(row, "support_count") > 0.0);
    assert!(value(row, "resin_model").is_finite() && value(row, "resin_model") >= 0.0);
    assert!(value(row, "resin_support").is_finite() && value(row, "resin_support") > 0.0);
    assert_eq!(
        value(row, "resin_total"),
        value(row, "resin_model") + value(row, "resin_support")
    );
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "plinth-metrics-supports-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create test directory");
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
