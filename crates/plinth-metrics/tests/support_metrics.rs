use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use chitubox_io::{
    ModelGeometry, ModelSource, Placement, Project, SegmentType, SupportBatch, SupportMode,
    SupportRecord, TriangleSoup,
};
use plinth_core::ModelId;
use plinth_metrics::{
    load_artifacts, support_metric_rows, ArtifactDisposition, ArtifactLoad, ArtifactRequest,
    ContactArmInput, DiskArtifact, SupportMetricError, SupportMetricInput, SUPPORT_METRIC_ORDER,
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

fn octahedron(radius: f32) -> Vec<f32> {
    let vertices = [
        [radius, 0.0, 0.0],
        [-radius, 0.0, 0.0],
        [0.0, radius, 0.0],
        [0.0, -radius, 0.0],
        [0.0, 0.0, radius],
        [0.0, 0.0, -radius],
    ];
    [
        [4, 0, 2],
        [4, 2, 1],
        [4, 1, 3],
        [4, 3, 0],
        [5, 2, 0],
        [5, 1, 2],
        [5, 3, 1],
        [5, 0, 3],
    ]
    .into_iter()
    .flat_map(|face| face.into_iter().flat_map(|index| vertices[index]))
    .collect()
}

fn cuboid(x: f32, y: f32, z: f32) -> Vec<f32> {
    let vertices = [
        [0.0, 0.0, 0.0],
        [x, 0.0, 0.0],
        [x, y, 0.0],
        [0.0, y, 0.0],
        [0.0, 0.0, z],
        [x, 0.0, z],
        [x, y, z],
        [0.0, y, z],
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
    .flat_map(|face| face.into_iter().flat_map(|index| vertices[index]))
    .collect()
}

fn translated(mut triangles: Vec<f32>, offset: [f32; 3]) -> Vec<f32> {
    for point in triangles.chunks_exact_mut(3) {
        for (coordinate, shift) in point.iter_mut().zip(offset) {
            *coordinate += shift;
        }
    }
    triangles
}

fn joined(mut left: Vec<f32>, right: Vec<f32>) -> Vec<f32> {
    left.extend(right);
    left
}

fn record_with_mesh(
    kind: SegmentType,
    r0: f32,
    r1: f32,
    height: f32,
    mesh: Vec<f32>,
) -> SupportRecord {
    SupportRecord::new(
        kind,
        [0.0, 0.0, 0.0],
        [0.0, 0.0, height],
        r0,
        r1,
        TriangleSoup::new(mesh),
    )
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
    record_with_mesh(kind, r0, r1, height, mesh)
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
            contact_arms: vec![
                ContactArmInput {
                    support_record_index: 0,
                    taper_index: 0,
                    voronoi_area_mm2: std::f64::consts::PI * 5.0,
                },
                ContactArmInput {
                    support_record_index: 1,
                    taper_index: 1,
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
fn typed_spheres_stub_pair_raft_records_and_multishell_supports_count_once_each() {
    let temp = TempDir::new();
    let path = temp.path().join("all-native-shapes.chitubox");
    let multi_shell = joined(
        tetrahedron(1.0, false),
        translated(tetrahedron(2.0, false), [10.0, 0.0, 0.0]),
    );
    write_artifact(
        &path,
        &[(
            tetrahedron(1.0, false),
            vec![
                // Emitter's type-7 half of the contact pair: an actual
                // octahedral sphere, not a metadata-frustum stand-in.
                record_with_mesh(SegmentType::TIP, 1.0, 1.0, 2.0, octahedron(1.0)),
                // The adjacent type-1 half of that same contact pair.
                record(SegmentType::TAPER, 1.0, 1.0, 1.0, false),
                // Standalone type-9 joint sphere.
                record_with_mesh(SegmentType::SHAFT, 2.0, 2.0, 4.0, octahedron(2.0)),
                // Type-6 pad (equal endpoints) and bridge (distinct endpoints)
                // both have their actual closed cached meshes counted.
                record_with_mesh(
                    SegmentType::PAD_OR_BRIDGE,
                    0.5,
                    0.5,
                    0.0,
                    cuboid(1.0, 1.0, 1.0),
                ),
                record_with_mesh(
                    SegmentType::PAD_OR_BRIDGE,
                    0.5,
                    0.5,
                    2.0,
                    cuboid(2.0, 1.0, 1.0),
                ),
                // One record may own multiple closed positive shells.
                record_with_mesh(SegmentType::JOINER, 1.0, 1.0, 1.0, multi_shell),
            ],
        )],
    );
    let artifact = reopen(&path, "all-native-shapes");
    let rows = support_metric_rows(
        &artifact,
        &[SupportMetricInput {
            source_index: 0,
            contact_arms: vec![ContactArmInput {
                support_record_index: 1,
                taper_index: 0,
                voronoi_area_mm2: std::f64::consts::PI,
            }],
        }],
    )
    .expect("all emitted support shape classes have valid independent shells");
    let row = &rows[0];
    assert_eq!(value(row, "support_count"), 6.0);
    assert!((value(row, "max_x") - 1.0).abs() < 1e-12);
    // type-7 octahedron: 4/3; taper: 2; type-9 octahedron: 32/3;
    // pad: 1; bridge: 2; multi-shell joiner: 1/6 + 8/6 = 3/2 mm^3.
    assert!((value(row, "resin_support") - 18.5 / 1_000.0).abs() < 1e-12);
}

#[test]
fn support_shells_reject_open_zero_and_negative_components_without_cancellation() {
    let temp = TempDir::new();
    let model = tetrahedron(1.0, false);

    let mixed_path = temp.path().join("mixed-shells.chitubox");
    write_artifact(
        &mixed_path,
        &[(
            (model.clone()),
            vec![record_with_mesh(
                SegmentType::JOINER,
                1.0,
                1.0,
                1.0,
                joined(
                    tetrahedron(2.0, false),
                    translated(tetrahedron(1.0, true), [10.0, 0.0, 0.0]),
                ),
            )],
        )],
    );
    assert!(matches!(
        support_metric_rows(
            &reopen(&mixed_path, "mixed-shells"),
            &[SupportMetricInput {
                source_index: 0,
                contact_arms: vec![]
            }]
        ),
        Err(SupportMetricError::SupportShellInvalidWinding { shell_index: 1, .. })
    ));

    let open_path = temp.path().join("open-shell.chitubox");
    write_artifact(
        &open_path,
        &[(
            model.clone(),
            vec![record_with_mesh(
                SegmentType::JOINER,
                1.0,
                1.0,
                1.0,
                vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            )],
        )],
    );
    assert!(matches!(
        support_metric_rows(
            &reopen(&open_path, "open-shell"),
            &[SupportMetricInput {
                source_index: 0,
                contact_arms: vec![]
            }]
        ),
        Err(SupportMetricError::SupportShellNotClosed { .. })
    ));

    let zero_path = temp.path().join("zero-shell.chitubox");
    let planar = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
    ];
    let zero_mesh = [[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]]
        .into_iter()
        .flat_map(|face| face.into_iter().flat_map(|index| planar[index]))
        .collect();
    write_artifact(
        &zero_path,
        &[(
            (model),
            vec![record_with_mesh(
                SegmentType::JOINER,
                1.0,
                1.0,
                1.0,
                zero_mesh,
            )],
        )],
    );
    assert!(matches!(
        support_metric_rows(
            &reopen(&zero_path, "zero-shell"),
            &[SupportMetricInput {
                source_index: 0,
                contact_arms: vec![]
            }]
        ),
        Err(SupportMetricError::SupportShellInvalidWinding { .. })
            | Err(SupportMetricError::SupportShellNonPositiveVolume { .. })
    ));
}

#[test]
fn hollow_model_retains_inward_cavity_as_a_signed_material_subtraction() {
    let temp = TempDir::new();
    let path = temp.path().join("hollow-model.chitubox");
    let hollow = joined(
        tetrahedron(4.0, false),
        translated(tetrahedron(0.5, true), [0.75, 0.75, 0.75]),
    );
    write_artifact(&path, &[(hollow, vec![])]);
    let rows = support_metric_rows(
        &reopen(&path, "hollow-model"),
        &[SupportMetricInput {
            source_index: 0,
            contact_arms: vec![],
        }],
    )
    .expect("correctly oriented inner shell is material subtraction, not an error");
    assert!((value(&rows[0], "resin_model") - (63.875 / 6_000.0)).abs() < 1e-12);
    assert_eq!(value(&rows[0], "resin_support"), 0.0);
    assert_eq!(
        value(&rows[0], "resin_total"),
        value(&rows[0], "resin_model")
    );
}

#[test]
fn contact_arm_mapping_is_a_complete_bijection_over_disk_type_one_records() {
    let temp = TempDir::new();
    let path = temp.path().join("contact-bijection.chitubox");
    write_artifact(
        &path,
        &[(
            tetrahedron(1.0, false),
            vec![
                record(SegmentType::TAPER, 1.0, 1.0, 1.0, false),
                record(SegmentType::TAPER, 2.0, 1.0, 1.0, false),
                record(SegmentType::JOINER, 1.0, 1.0, 1.0, false),
            ],
        )],
    );
    let artifact = reopen(&path, "contact-bijection");
    let input = |contact_arms| {
        support_metric_rows(
            &artifact,
            &[SupportMetricInput {
                source_index: 0,
                contact_arms,
            }],
        )
    };
    assert!(matches!(
        input(vec![ContactArmInput {
            support_record_index: 0,
            taper_index: 0,
            voronoi_area_mm2: 1.0,
        }]),
        Err(SupportMetricError::ContactArmCount {
            expected: 2,
            actual: 1,
            ..
        })
    ));
    assert!(matches!(
        input(vec![
            ContactArmInput {
                support_record_index: 0,
                taper_index: 0,
                voronoi_area_mm2: 1.0,
            },
            ContactArmInput {
                support_record_index: 0,
                taper_index: 0,
                voronoi_area_mm2: 1.0,
            },
        ]),
        Err(SupportMetricError::DuplicateContactTaper { .. })
    ));
    assert!(matches!(
        input(vec![
            ContactArmInput {
                support_record_index: 0,
                taper_index: 0,
                voronoi_area_mm2: 1.0,
            },
            ContactArmInput {
                support_record_index: 99,
                taper_index: 1,
                voronoi_area_mm2: 1.0,
            },
        ]),
        Err(SupportMetricError::ContactTaperOutOfBounds { .. })
    ));
    assert!(matches!(
        input(vec![
            ContactArmInput {
                support_record_index: 0,
                taper_index: 1,
                voronoi_area_mm2: 1.0,
            },
            ContactArmInput {
                support_record_index: 1,
                taper_index: 1,
                voronoi_area_mm2: 1.0,
            },
        ]),
        Err(SupportMetricError::ContactTaperOrdinalMismatch { .. })
    ));
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
            contact_arms: vec![ContactArmInput {
                support_record_index: 0,
                taper_index: 0,
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
            contact_arms: vec![ContactArmInput {
                support_record_index: 0,
                taper_index: 0,
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
            contact_arms: vec![ContactArmInput {
                support_record_index: 1,
                taper_index: 0,
                voronoi_area_mm2: 1.0,
            }],
        }],
    );
    assert!(matches!(
        non_contact,
        Err(SupportMetricError::ContactTaperWrongKind { .. })
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
                contact_arms: vec![ContactArmInput {
                    support_record_index: 0,
                    taper_index: 0,
                    voronoi_area_mm2: 1.0,
                }]
            }]
        ),
        Err(SupportMetricError::SupportShellInvalidWinding {
            support_index: 0,
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
                contact_arms: vec![ContactArmInput {
                    support_record_index: 0,
                    taper_index: 0,
                    voronoi_area_mm2: 0.0,
                }],
            },
            SupportMetricInput {
                source_index: 1,
                contact_arms: vec![
                    ContactArmInput {
                        support_record_index: 0,
                        taper_index: 0,
                        voronoi_area_mm2: 0.0,
                    },
                    ContactArmInput {
                        support_record_index: 1,
                        taper_index: 1,
                        voronoi_area_mm2: 0.0,
                    },
                ],
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
    let error = support_metric_rows(
        &artifact,
        &[SupportMetricInput {
            source_index: 0,
            contact_arms: vec![],
        }],
    );
    assert!(matches!(
        error,
        Err(SupportMetricError::ContactArmCount {
            expected: 7,
            actual: 0,
            ..
        })
    ));
    let analysis = plinth_metrics::analyze_artifact_topology(&artifact)
        .expect("independent topology volume derivation over fresh Sphere disk data");
    let model_mm3 = analysis.models[0]
        .model
        .shells
        .iter()
        .map(|shell| shell.signed_volume)
        .sum::<f64>();
    let support_mm3 = analysis.models[0]
        .supports
        .iter()
        .flat_map(|support| support.shells.iter())
        .map(|shell| shell.signed_volume)
        .sum::<f64>();
    // Pinned from committed corpus/baseline/Sphere.chitubox, re-opened above;
    // these are independently summed from raw signed tetrahedra, then mm^3
    // divided by the exact 1,000 mm^3/mL conversion.
    assert_eq!(artifact.models()[0].supports().len(), 47);
    assert_eq!(model_mm3 / 1_000.0, 4.174747768289511);
    assert_eq!(support_mm3 / 1_000.0, 0.09353672721774903);
    assert_eq!(
        model_mm3 / 1_000.0 + support_mm3 / 1_000.0,
        4.2682844955072605
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
