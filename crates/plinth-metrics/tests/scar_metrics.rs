use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use chitubox_io::{
    ModelGeometry, ModelSource, Placement, Project, SegmentType, SupportBatch, SupportMode,
    SupportRecord, TriangleSoup,
};
use plinth_core::ModelId;
use plinth_metrics::{
    closest_points, contains_points, load_artifacts, ray_intersections, scar_metric_rows,
    ArtifactDisposition, ArtifactLoad, ArtifactRequest, DiskArtifact, Ray, ScarMetricError,
    ScarMetricInput, ScarMetricRow, SCAR_METRIC_ORDER,
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

fn translated_cube(side: f32, translation: [f32; 3]) -> Vec<f32> {
    cube(side)
        .chunks_exact(3)
        .flat_map(|point| {
            [
                point[0] + translation[0],
                point[1] + translation[1],
                point[2] + translation[2],
            ]
        })
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

fn stub_with_axis(contact_z: f32, end_z: f32) -> [SupportRecord; 2] {
    [
        ball(
            SegmentType::TIP,
            [0.0, 0.0, contact_z],
            [0.0, 0.0, contact_z + 0.6],
            0.3,
        ),
        record(
            SegmentType::TAPER,
            [0.0, 0.0, contact_z + 0.3],
            [0.0, 0.0, end_z],
            0.2,
            0.4,
        ),
    ]
}

fn edge_bound_stub() -> [SupportRecord; 2] {
    [
        ball(SegmentType::TIP, [-1.0, 1.0, 1.0], [-0.4, 1.0, 1.0], 0.3),
        record(
            SegmentType::TAPER,
            [-0.7, 1.0, 1.0],
            [1.5, 1.0, 1.0],
            0.2,
            0.4,
        ),
    ]
}

fn coincident_anchor_stubs() -> Vec<SupportRecord> {
    vec![
        ball(SegmentType::TIP, [0.0, 0.0, 1.0], [0.0, 0.0, 1.6], 0.3),
        record(
            SegmentType::TAPER,
            [0.0, 0.0, 1.3],
            [0.0, 0.0, 2.0],
            0.2,
            0.4,
        ),
        ball(SegmentType::TIP, [0.0, 0.0, 1.0], [0.36, 0.0, 1.48], 0.3),
        record(
            SegmentType::TAPER,
            [0.18, 0.0, 1.24],
            [0.78, 0.0, 2.04],
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
        record(
            SegmentType::TAPER,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.8],
            0.2,
            0.25,
        ),
        foot_ball(-0.5, 0.8, 0.35),
        foot_ball(0.5, 0.8, 0.28),
        foot_ball(0.0, 0.0, 0.28),
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
fn foot_candidates_ignore_incomplete_coincident_balls_then_use_the_unique_complete_relation() {
    let temp = TempDir::new();
    let path = temp.path().join("ordinary-at-other-end.chitubox");
    let records = vec![
        record(
            SegmentType::TAPER,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.8],
            0.2,
            0.25,
        ),
        // Coincident ordinary joint at the other endpoint: valid sphere and
        // poles, but the factor is wrong, so it is not a foot candidate.
        foot_ball(0.0, 0.0, 0.4),
        // Coincident center, factor, and poles, but deliberately non-spherical
        // soup: still an ordinary non-candidate, not an immediate error.
        record(
            SegmentType::SHAFT,
            [0.0, 0.0, 1.15],
            [0.0, 0.0, 0.45],
            0.35,
            0.35,
        ),
        foot_ball(0.0, 0.8, 0.35),
    ];
    write_artifact(&path, &[(cube(2.0), records)]);
    let rows = scar_metric_rows(
        &reopen(&path, "ordinary-at-other-end"),
        &[ScarMetricInput { source_index: 0 }],
    )
    .expect("only the complete center/factor/geometry relation is a candidate");
    assert!(value(&rows[0], "scar_area") > 0.0);

    let no_valid = temp.path().join("only-wrong-factor.chitubox");
    write_artifact(
        &no_valid,
        &[(
            cube(2.0),
            vec![
                record(
                    SegmentType::TAPER,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.8],
                    0.2,
                    0.25,
                ),
                foot_ball(0.0, 0.8, 0.4),
            ],
        )],
    );
    assert!(matches!(
        scar_metric_rows(
            &reopen(&no_valid, "only-wrong-factor"),
            &[ScarMetricInput { source_index: 0 }]
        ),
        Err(ScarMetricError::UnmatchedTaper { .. })
    ));
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
fn contact_identity_bijection_and_class_matrix_fails_closed() {
    let temp = TempDir::new();

    let member_two_balls = temp.path().join("member-two-balls.chitubox");
    write_artifact(
        &member_two_balls,
        &[(
            cube(2.0),
            vec![
                record(
                    SegmentType::TAPER,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.8],
                    0.2,
                    0.25,
                ),
                foot_ball(0.0, 0.8, 0.35),
                foot_ball(0.0, 0.8, 0.35),
            ],
        )],
    );
    assert!(matches!(
        scar_metric_rows(
            &reopen(&member_two_balls, "member-two-balls"),
            &[ScarMetricInput { source_index: 0 }]
        ),
        Err(ScarMetricError::AmbiguousContact { class: "foot", .. })
    ));

    let ball_two_members = temp.path().join("ball-two-members.chitubox");
    write_artifact(
        &ball_two_members,
        &[(
            cube(2.0),
            vec![
                record(
                    SegmentType::TAPER,
                    [-0.5, 0.0, 0.0],
                    [0.0, 0.0, 0.8],
                    0.2,
                    0.25,
                ),
                record(
                    SegmentType::TAPER,
                    [0.5, 0.0, 0.0],
                    [0.0, 0.0, 0.8],
                    0.2,
                    0.25,
                ),
                foot_ball(0.0, 0.8, 0.35),
            ],
        )],
    );
    assert!(matches!(
        scar_metric_rows(
            &reopen(&ball_two_members, "ball-two-members"),
            &[ScarMetricInput { source_index: 0 }]
        ),
        Err(ScarMetricError::AmbiguousContact { class: "foot", .. })
    ));

    let both_classes = temp.path().join("both-classes.chitubox");
    let pair = stub(0.0, 0.8);
    write_artifact(
        &both_classes,
        &[(
            cube(5.0),
            vec![pair[0].clone(), pair[1].clone(), foot_ball(0.0, 2.0, 0.56)],
        )],
    );
    assert!(matches!(
        scar_metric_rows(
            &reopen(&both_classes, "both-classes"),
            &[ScarMetricInput { source_index: 0 }]
        ),
        Err(ScarMetricError::BothContactClasses { .. })
    ));

    let nonconstant_joiner = temp.path().join("nonconstant-joiner.chitubox");
    write_artifact(
        &nonconstant_joiner,
        &[(
            cube(2.0),
            vec![
                record(
                    SegmentType::JOINER,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.8],
                    0.2,
                    0.25,
                ),
                foot_ball(0.0, 0.8, 0.35),
            ],
        )],
    );
    let rows = scar_metric_rows(
        &reopen(&nonconstant_joiner, "nonconstant-joiner"),
        &[ScarMetricInput { source_index: 0 }],
    )
    .expect("a nonconstant type-3 is not a foot candidate");
    assert_eq!(value(&rows[0], "scar_area"), 0.0);

    let wrong_order = temp.path().join("ball-before-member.chitubox");
    write_artifact(
        &wrong_order,
        &[(
            cube(2.0),
            vec![
                foot_ball(0.0, 0.8, 0.35),
                record(
                    SegmentType::TAPER,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 0.8],
                    0.2,
                    0.25,
                ),
            ],
        )],
    );
    assert!(matches!(
        scar_metric_rows(
            &reopen(&wrong_order, "ball-before-member"),
            &[ScarMetricInput { source_index: 0 }]
        ),
        Err(ScarMetricError::UnmatchedTaper { .. })
    ));

    let source_one = temp.path().join("source-one.chitubox");
    write_artifact(
        &source_one,
        &[
            (cube(2.0), Vec::new()),
            (
                cube(2.0),
                vec![record(
                    SegmentType::TAPER,
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0],
                    0.2,
                    0.2,
                )],
            ),
        ],
    );
    assert!(matches!(
        scar_metric_rows(
            &reopen(&source_one, "source-one"),
            &[
                ScarMetricInput { source_index: 0 },
                ScarMetricInput { source_index: 1 },
            ]
        ),
        Err(ScarMetricError::UnmatchedTaper {
            source_index: 1,
            ..
        })
    ));
}

#[test]
fn anchored_interval_requires_actual_solid_and_uses_the_tolerance_branch() {
    let temp = TempDir::new();
    let shallow = temp.path().join("shallow.chitubox");
    write_artifact(&shallow, &[(cube(2.0), stub_with_axis(0.98, 2.0).into())]);
    let shallow = &scar_metric_rows(
        &reopen(&shallow, "shallow"),
        &[ScarMetricInput { source_index: 0 }],
    )
    .expect("shallow interior interval")[0];
    // T=.02 <= TOL, so only the angle-independent ball disc remains.
    assert!((value(shallow, "scar_area") - std::f64::consts::PI * (0.09 - 0.0004)).abs() < 3.0e-7);

    let outside = temp.path().join("outside.chitubox");
    write_artifact(&outside, &[(cube(2.0), stub_with_axis(1.2, 2.2).into())]);
    let outside = &scar_metric_rows(
        &reopen(&outside, "outside"),
        &[ScarMetricInput { source_index: 0 }],
    )
    .expect("outside contact has a valid record identity")[0];
    // The ray never enters the model: no whole-member fallback is allowed.
    assert!((value(outside, "scar_area") - std::f64::consts::PI * 0.05).abs() < 3.0e-7);
}

#[test]
fn anchored_closed_segments_distinguish_surface_direction_inside_and_near_entry() {
    let temp = TempDir::new();
    let path = temp.path().join("anchored-closed-segments.chitubox");
    write_artifact(
        &path,
        &[
            // Designed pole z=1 and member points outward: T=0.
            (cube(2.0), stub_with_axis(1.0, 2.0).into()),
            // Designed pole z=1 and member points inward through the cube.
            (cube(2.0), stub_with_axis(0.4, -2.0).into()),
            // Both designed pole and other endpoint are strictly inside.
            (cube(2.0), stub_with_axis(0.5, 0.9).into()),
            // Designed pole is just outside; first interior entry is positive
            // but below TOL and therefore remains anchored.
            (cube(2.0), stub_with_axis(0.400_01, -2.0).into()),
        ],
    );
    let rows = scar_metric_rows(
        &reopen(&path, "anchored-closed-segments"),
        &[
            ScarMetricInput { source_index: 0 },
            ScarMetricInput { source_index: 1 },
            ScarMetricInput { source_index: 2 },
            ScarMetricInput { source_index: 3 },
        ],
    )
    .expect("closed-segment classifications");
    let outward = value(&rows[0], "scar_area");
    let inward = value(&rows[1], "scar_area");
    let wholly_inside = value(&rows[2], "scar_area");
    let near_entry = value(&rows[3], "scar_area");
    assert_eq!(outward, std::f64::consts::PI * f64::from(0.3_f32).powi(2));
    assert!(inward > outward);
    assert!(wholly_inside > outward);
    assert!(near_entry > outward);
}

#[test]
fn anchored_intervals_choose_first_eligible_component_and_name_surface_bound_ambiguity() {
    let temp = TempDir::new();
    let path = temp.path().join("multiple-intervals.chitubox");
    // The positive-z companion keeps ChituBox's bounding-box centering at
    // zero; the ray only encounters the negative-z component later.
    let mut disconnected_cubes = cube(2.0);
    disconnected_cubes.extend(translated_cube(2.0, [0.0, 0.0, -3.0]));
    disconnected_cubes.extend(translated_cube(2.0, [0.0, 0.0, 3.0]));
    write_artifact(
        &path,
        &[
            (cube(2.0), stub_with_axis(0.400_01, -5.0).into()),
            (disconnected_cubes, stub_with_axis(0.400_01, -5.0).into()),
        ],
    );
    let rows = scar_metric_rows(
        &reopen(&path, "multiple-intervals"),
        &[
            ScarMetricInput { source_index: 0 },
            ScarMetricInput { source_index: 1 },
        ],
    )
    .expect("the first anchored interior component wins");
    assert_eq!(
        value(&rows[0], "scar_area"),
        value(&rows[1], "scar_area"),
        "a later disconnected interior interval must not extend T"
    );

    let edge = temp.path().join("edge-bound.chitubox");
    write_artifact(&edge, &[(cube(2.0), edge_bound_stub().into())]);
    let edge_result = scar_metric_rows(
        &reopen(&edge, "edge-bound"),
        &[ScarMetricInput { source_index: 0 }],
    );
    assert!(matches!(
        edge_result,
        Err(ScarMetricError::BoundaryAmbiguity { .. })
    ));
}

#[test]
fn worst_patch_is_the_maximum_individual_patch_for_coincident_contacts() {
    let temp = TempDir::new();
    let path = temp.path().join("coincident-contacts.chitubox");
    write_artifact(&path, &[(cube(2.0), coincident_anchor_stubs())]);
    let rows = scar_metric_rows(
        &reopen(&path, "coincident-contacts"),
        &[ScarMetricInput { source_index: 0 }],
    )
    .expect("two geometrically distinct stubs may share one designed pole");
    let individual = std::f64::consts::PI * f64::from(0.3_f32).powi(2);
    assert_eq!(value(&rows[0], "scar_worst_patch"), individual);
    assert_eq!(value(&rows[0], "scar_area"), 2.0 * individual);
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

#[test]
fn committed_sphere_has_seven_reconstructed_stubs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sphere = root.join("corpus/baseline/Sphere.chitubox");
    assert!(
        sphere.is_file(),
        "required committed Sphere artifact missing: {}",
        sphere.display()
    );
    let artifact = reopen(&sphere, "Sphere");
    let rows = scar_metric_rows(&artifact, &[ScarMetricInput { source_index: 0 }])
        .expect("committed Sphere must satisfy the complete scar-record contract");
    let row = &rows[0];
    assert_eq!(
        artifact.models()[0]
            .supports()
            .iter()
            .filter(|support| support.segment_type() == SegmentType::TIP)
            .count(),
        7
    );
    // Pinned from the committed file, reopened fresh above.  This test keeps
    // its own record-count assertion and does not ask the producer for a
    // contact list, so changed contact reconstruction cannot self-certify.
    assert_eq!(value(row, "scar_area"), 18.207_723_309_160_54);
    assert_eq!(value(row, "scar_worst_patch"), 3.339_084_784_039_538);
}

#[test]
fn committed_sphere_raw_disk_oracle_pins_each_individual_patch() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sphere = root.join("corpus/baseline/Sphere.chitubox");
    assert!(
        sphere.is_file(),
        "required Sphere oracle missing: {}",
        sphere.display()
    );
    let artifact = reopen(&sphere, "Sphere-raw-oracle");
    let model = &artifact.models()[0];
    let supports = model.supports();

    // These are the seven record identities in the reopened file, not a list
    // obtained from production reconstruction.  Each pair is (type-7 ball,
    // type-1 member); current-emitter type-7.start is the designed pole.
    let pairs = [(0, 1), (2, 3), (4, 5), (6, 7), (8, 9), (10, 11), (12, 13)];
    let anchors = pairs
        .iter()
        .map(|&(ball, _)| supports[ball].start())
        .collect::<Vec<_>>();
    let rays = pairs
        .iter()
        .map(|&(ball, member)| Ray {
            origin: supports[ball].start(),
            direction: sub64(supports[member].end(), supports[ball].start()),
        })
        .collect::<Vec<_>>();
    let crossings = ray_intersections(model.mesh(), &rays).expect("raw oracle rays");

    #[derive(Clone, Copy)]
    struct Probe {
        contact: usize,
        start: f64,
        end: f64,
        exit_triangle: usize,
    }
    let mut probes = Vec::new();
    let mut probe_points = Vec::new();
    for (contact, hits) in crossings.iter().enumerate() {
        let length = norm64(rays[contact].direction);
        let axis = scale64(rays[contact].direction, 1.0 / length);
        let mut distances = hits
            .iter()
            .filter(|hit| hit.distance > 0.0 && hit.distance <= length)
            .map(|hit| (hit.distance, hit.triangle_index))
            .collect::<Vec<_>>();
        distances.dedup_by(|left, right| left.0.to_bits() == right.0.to_bits());
        let mut start = 0.0;
        for (end, triangle) in distances {
            if end > start {
                probe_points.push(add64(anchors[contact], scale64(axis, (start + end) * 0.5)));
                probes.push(Probe {
                    contact,
                    start,
                    end,
                    exit_triangle: triangle,
                });
            }
            start = end;
        }
    }
    let inside = contains_points(model.mesh(), &probe_points).expect("raw oracle containment");
    let closest = closest_points(model.mesh(), &anchors).expect("raw oracle closest points");
    let mut patches = Vec::new();
    for (contact, &(ball, member)) in pairs.iter().enumerate() {
        let interval = probes
            .iter()
            .zip(&inside)
            .find_map(|(probe, inside)| {
                (probe.contact == contact && *inside && probe.start <= 0.05).then_some(*probe)
            })
            .expect("each Sphere contact has an anchored interior interval");
        let member_length = norm64(rays[contact].direction);
        let anchored_length = interval.end - interval.start;
        let (r_end, r_root) = supports[member].radii();
        let r_x = r_end + (r_root - r_end) * (anchored_length / member_length).clamp(0.0, 1.0);
        let indices = model.mesh().triangles()[interval.exit_triangle];
        let [a, b, c] = indices.map(|index| model.mesh().vertices()[index]);
        let normal = unit64(cross64(sub64(b, a), sub64(c, a)));
        let cosine = dot64(unit64(rays[contact].direction), normal).abs();
        let shaft = if anchored_length <= 0.05 {
            0.0
        } else {
            (std::f64::consts::PI * r_x * r_x / cosine.max(1.0e-9)).min(2.0 * r_x * anchored_length)
        };
        let ball_radius = supports[ball].radii().0;
        let distance = closest[contact].squared_distance.sqrt();
        let disc =
            std::f64::consts::PI * (ball_radius * ball_radius - distance * distance).max(0.0);
        patches.push(shaft + disc);
    }

    // Independently derived from the raw fields and public triangle queries
    // above.  Pinning each patch prevents an unchanged sum from hiding a
    // reconstruction swap or a compensating formula error.
    let expected = [
        1.843_017_580_677_558_6,
        2.311_807_711_025_12,
        2.751_185_192_997_492_6,
        2.089_799_123_557_092_7,
        2.843_279_514_144_46,
        3.029_549_402_719_277_3,
        3.339_084_784_039_538,
    ];
    assert_eq!(patches, expected);
    let production = scar_metric_rows(&artifact, &[ScarMetricInput { source_index: 0 }])
        .expect("production Sphere scars");
    assert_eq!(
        patches.iter().sum::<f64>(),
        value(&production[0], "scar_area")
    );
    assert_eq!(
        patches.iter().copied().fold(0.0, f64::max),
        value(&production[0], "scar_worst_patch")
    );
}

fn add64(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}
fn sub64(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}
fn scale64(value: [f64; 3], factor: f64) -> [f64; 3] {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}
fn dot64(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}
fn cross64(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}
fn norm64(value: [f64; 3]) -> f64 {
    dot64(value, value).sqrt()
}
fn unit64(value: [f64; 3]) -> [f64; 3] {
    scale64(value, 1.0 / norm64(value))
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
