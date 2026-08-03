use std::process::Command;

use geom_kernel::testkit::{brute_allowed_projection, build_from_tris};
use geom_kernel::{
    AllowedFaceProjection, AllowedFaceProjectionError, AllowedFaceProjectionLimits,
    AllowedFaceProjectionNoWinner, AllowedFaceProjectionOutcome, AllowedFaceProjectionPolicy,
    NativeSurfaceConfig,
};

fn limits() -> AllowedFaceProjectionLimits {
    AllowedFaceProjectionLimits {
        max_points: usize::MAX,
        max_allowed_faces: usize::MAX,
        max_total_node_visits: usize::MAX,
        max_total_triangle_tests: usize::MAX,
        max_memory_bytes: usize::MAX,
    }
}

fn uncapped() -> AllowedFaceProjectionPolicy {
    AllowedFaceProjectionPolicy {
        max_distance_mm: None,
    }
}

fn separated_surface() -> geom_kernel::NativeSurface {
    build_from_tris(vec![
        // Component A, face 0.
        0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, 0.0, // Component B, face 1.
        0.0, 0.0, 10.0, 2.0, 0.0, 10.0, 0.0, 2.0, 10.0,
        // A large anisotropic but printable-scale face, face 2.
        -100.0, 20.0, 4.0, 100.0, 20.0, 4.0, -100.0, 20.001, 4.0,
    ])
}

#[test]
fn exact_allowed_restriction_preserves_global_face_ids_across_components() {
    let surface = separated_surface();
    let outcomes = surface
        .project_allowed_faces_batch(&[1], &[[0.25, 0.25, 0.1]], uncapped(), limits())
        .unwrap();
    assert_eq!(
        outcomes,
        vec![AllowedFaceProjectionOutcome::Projected(
            AllowedFaceProjection {
                distance_mm: 9.9,
                point: [0.25, 0.25, 10.0],
                face_id: 1,
            }
        )]
    );

    // The unrestricted surface chooses the disallowed component instead.
    let (_, _, unrestricted_faces, _) = surface.project(&[[0.25, 0.25, 0.1]]);
    assert_eq!(unrestricted_faces, vec![0]);
}

#[test]
fn tie_breaks_by_global_face_id_independent_of_allowed_order() {
    let surface = build_from_tris(vec![
        0.0, 0.0, -1.0, 2.0, 0.0, -1.0, 0.0, 2.0, -1.0, 0.0, 0.0, 1.0, 2.0, 0.0, 1.0, 0.0, 2.0, 1.0,
    ]);
    let left = surface
        .project_allowed_faces_batch(&[1, 0], &[[0.25, 0.25, 0.0]], uncapped(), limits())
        .unwrap();
    let right = surface
        .project_allowed_faces_batch(&[0, 1], &[[0.25, 0.25, 0.0]], uncapped(), limits())
        .unwrap();
    assert_eq!(left, right);
    assert!(matches!(
        left.as_slice(),
        [AllowedFaceProjectionOutcome::Projected(
            AllowedFaceProjection {
                distance_mm: 1.0,
                face_id: 0,
                ..
            }
        )]
    ));
}

#[test]
fn capped_no_winner_is_distinct_from_batch_failure() {
    let surface = separated_surface();
    let outcome = surface
        .project_allowed_faces_batch(
            &[1],
            &[[0.25, 0.25, 0.1]],
            AllowedFaceProjectionPolicy {
                max_distance_mm: Some(2.0),
            },
            limits(),
        )
        .unwrap();
    assert_eq!(
        outcome,
        vec![AllowedFaceProjectionOutcome::NoWinner(
            AllowedFaceProjectionNoWinner::BeyondMaxDistance {
                max_distance_mm: 2.0,
            }
        )]
    );

    // Equality at the declared cap is admitted.
    let at_cap = surface
        .project_allowed_faces_batch(
            &[1],
            &[[0.25, 0.25, 8.0]],
            AllowedFaceProjectionPolicy {
                max_distance_mm: Some(2.0),
            },
            limits(),
        )
        .unwrap();
    assert!(matches!(
        at_cap.as_slice(),
        [AllowedFaceProjectionOutcome::Projected(
            AllowedFaceProjection {
                distance_mm: 2.0,
                face_id: 1,
                ..
            }
        )]
    ));
}

#[test]
fn rejects_empty_duplicate_out_of_range_and_parent_unselected_ids() {
    let surface = separated_surface();
    assert_eq!(
        surface.project_allowed_faces_batch(&[], &[[0.0; 3]], uncapped(), limits()),
        Err(AllowedFaceProjectionError::EmptyAllowedFaces)
    );
    assert_eq!(
        surface.project_allowed_faces_batch(&[2, 0, 2], &[[0.0; 3]], uncapped(), limits()),
        Err(AllowedFaceProjectionError::DuplicateFaceId { face_id: 2 })
    );
    assert_eq!(
        surface.project_allowed_faces_batch(&[3], &[[0.0; 3]], uncapped(), limits()),
        Err(AllowedFaceProjectionError::FaceIdOutOfBounds {
            position: 0,
            face_id: 3,
            global_face_count: 3,
        })
    );

    let narrowed = surface.subset(&[0, 2]).unwrap();
    assert_eq!(
        narrowed.project_allowed_faces_batch(&[1], &[[0.0; 3]], uncapped(), limits()),
        Err(AllowedFaceProjectionError::FaceIdNotSelected {
            position: 0,
            face_id: 1,
        })
    );
}

#[test]
fn rejects_nonfinite_policy_points_and_oversized_batches_by_name() {
    let surface = separated_surface();
    assert_eq!(
        surface.project_allowed_faces_batch(&[0], &[[f64::NAN, 0.0, 0.0]], uncapped(), limits(),),
        Err(AllowedFaceProjectionError::NonFinitePoint { point: 0, axis: 0 })
    );
    assert_eq!(
        surface.project_allowed_faces_batch(
            &[0],
            &[[0.0; 3]],
            AllowedFaceProjectionPolicy {
                max_distance_mm: Some(f64::INFINITY),
            },
            limits(),
        ),
        Err(AllowedFaceProjectionError::InvalidMaxDistance {
            max_distance_mm: f64::INFINITY,
        })
    );

    let mut point_limited = limits();
    point_limited.max_points = 0;
    assert_eq!(
        surface.project_allowed_faces_batch(&[0], &[[0.0; 3]], uncapped(), point_limited),
        Err(AllowedFaceProjectionError::PointLimitExceeded {
            required: 1,
            limit: 0,
        })
    );
    let mut face_limited = limits();
    face_limited.max_allowed_faces = 1;
    assert_eq!(
        surface.project_allowed_faces_batch(&[0, 1], &[], uncapped(), face_limited),
        Err(AllowedFaceProjectionError::AllowedFaceLimitExceeded {
            required: 2,
            limit: 1,
        })
    );
}

#[test]
fn budget_refusal_happens_before_any_projection_result() {
    let surface = separated_surface();
    let mut memory_limited = limits();
    memory_limited.max_memory_bytes = 0;
    let required_memory =
        match surface.project_allowed_faces_batch(&[0], &[[0.0; 3]], uncapped(), memory_limited) {
            Err(AllowedFaceProjectionError::ResourceLimitExceeded {
                resource: "memory bytes",
                required,
                limit: 0,
            }) => required,
            other => panic!("unexpected memory refusal: {other:?}"),
        };
    assert!(required_memory > 0);
    memory_limited.max_memory_bytes = required_memory;
    assert!(surface
        .project_allowed_faces_batch(&[0], &[[0.0; 3]], uncapped(), memory_limited)
        .is_ok());

    let mut node_limited = limits();
    node_limited.max_total_node_visits = 0;
    node_limited.max_memory_bytes = 0;
    let required_nodes = match surface.project_allowed_faces_batch(
        &[0, 2],
        &[[0.0; 3], [1.0; 3]],
        uncapped(),
        node_limited,
    ) {
        Err(AllowedFaceProjectionError::ResourceLimitExceeded {
            resource: "setup node visits",
            required,
            limit: 0,
        }) => required,
        other => panic!("unexpected setup-node refusal: {other:?}"),
    };
    assert!(required_nodes > 0);
    node_limited.max_total_node_visits = required_nodes;
    node_limited.max_memory_bytes = usize::MAX;
    let required_total_nodes = match surface.project_allowed_faces_batch(
        &[0, 2],
        &[[0.0; 3], [1.0; 3]],
        uncapped(),
        node_limited,
    ) {
        Err(AllowedFaceProjectionError::ResourceLimitExceeded {
            resource: "total node visits",
            required,
            limit,
        }) if limit == required_nodes => required,
        other => panic!("unexpected total-node refusal: {other:?}"),
    };
    assert!(required_total_nodes > required_nodes);
    node_limited.max_total_node_visits = required_total_nodes;
    assert!(surface
        .project_allowed_faces_batch(&[0, 2], &[[0.0; 3], [1.0; 3]], uncapped(), node_limited,)
        .is_ok());

    let mut work_limited = limits();
    work_limited.max_total_triangle_tests = 1;
    work_limited.max_total_node_visits = 0;
    work_limited.max_memory_bytes = 0;
    assert_eq!(
        surface.project_allowed_faces_batch(
            &[0, 2],
            &[[0.0; 3], [1.0; 3]],
            uncapped(),
            work_limited,
        ),
        Err(AllowedFaceProjectionError::ResourceLimitExceeded {
            resource: "total triangle tests",
            required: 4,
            limit: 1,
        })
    );
    work_limited.max_total_node_visits = usize::MAX;
    work_limited.max_memory_bytes = usize::MAX;
    work_limited.max_total_triangle_tests = 4;
    assert!(surface
        .project_allowed_faces_batch(&[0, 2], &[[0.0; 3], [1.0; 3]], uncapped(), work_limited,)
        .is_ok());
}

fn generated_surface() -> (geom_kernel::NativeSurface, Vec<u32>, Vec<[f64; 3]>) {
    let mut triangles = Vec::new();
    for index in 0..48 {
        let x = f64::from(index % 8) * 4.0;
        let y = f64::from(index / 8) * 3.0;
        let z = f64::from((index * 17) % 11) * 0.37;
        let long = if index == 47 { 180.0 } else { 2.5 };
        let thin = if index == 47 { 0.2 } else { 1.75 };
        triangles.extend_from_slice(&[x, y, z, x + long, y, z + 0.03, x, y + thin, z + 0.07]);
    }
    let allowed = (0..48).filter(|index| index % 3 != 1).collect();
    let points = (0..96)
        .map(|index| {
            [
                f64::from((index * 13) % 53) * 0.61 - 3.0,
                f64::from((index * 7) % 37) * 0.47 - 2.0,
                f64::from((index * 19) % 29) * 0.29 - 1.0,
            ]
        })
        .collect();
    (build_from_tris(triangles), allowed, points)
}

#[test]
fn filtered_bvh_matches_bounded_brute_force_generated_oracle_exactly() {
    let (surface, allowed, points) = generated_surface();
    let actual = surface
        .project_allowed_faces_batch(&allowed, &points, uncapped(), limits())
        .unwrap();
    for (point, outcome) in points.iter().copied().zip(actual) {
        let (distance_mm, face_id, projected) =
            brute_allowed_projection(&surface, &allowed, point, f64::INFINITY).unwrap();
        assert_eq!(
            outcome,
            AllowedFaceProjectionOutcome::Projected(AllowedFaceProjection {
                distance_mm,
                point: projected,
                face_id,
            })
        );
    }
}

fn update_hash(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(1_099_511_628_211);
}

fn tight_budget_fingerprint() -> String {
    let (surface, allowed, points) = generated_surface();

    let mut memory_limits = limits();
    memory_limits.max_memory_bytes = 0;
    let memory_required =
        match surface.project_allowed_faces_batch(&allowed, &points, uncapped(), memory_limits) {
            Err(AllowedFaceProjectionError::ResourceLimitExceeded {
                resource: "memory bytes",
                required,
                limit: 0,
            }) => required,
            other => panic!("unexpected tight memory result: {other:?}"),
        };

    let mut setup_limits = limits();
    setup_limits.max_total_node_visits = 0;
    let setup_required =
        match surface.project_allowed_faces_batch(&allowed, &points, uncapped(), setup_limits) {
            Err(AllowedFaceProjectionError::ResourceLimitExceeded {
                resource: "setup node visits",
                required,
                limit: 0,
            }) => required,
            other => panic!("unexpected tight setup result: {other:?}"),
        };

    let mut node_limits = limits();
    node_limits.max_total_node_visits = setup_required;
    let node_required =
        match surface.project_allowed_faces_batch(&allowed, &points, uncapped(), node_limits) {
            Err(AllowedFaceProjectionError::ResourceLimitExceeded {
                resource: "total node visits",
                required,
                limit,
            }) if limit == setup_required => required,
            other => panic!("unexpected tight node result: {other:?}"),
        };

    let mut triangle_limits = limits();
    triangle_limits.max_total_triangle_tests = 0;
    let triangle_required =
        match surface.project_allowed_faces_batch(&allowed, &points, uncapped(), triangle_limits) {
            Err(AllowedFaceProjectionError::ResourceLimitExceeded {
                resource: "total triangle tests",
                required,
                limit: 0,
            }) => required,
            other => panic!("unexpected tight triangle result: {other:?}"),
        };

    // Pin the exact requirement values, not only cross-thread equality. This
    // fixture has 48 faces, 17 BVH nodes (all marked), depth 8, 32 allowed
    // faces, and 96 points under the public buffer layouts.
    assert_eq!(memory_required, 11_601);
    assert_eq!(setup_required, 17);
    assert_eq!(node_required, 1_649);
    assert_eq!(triangle_required, 3_072);
    assert_eq!(triangle_required, points.len() * allowed.len());
    format!(
        "memory:{memory_required}:0:setup:{setup_required}:0:node:{node_required}:{setup_required}:triangle:{triangle_required}:0"
    )
}

#[test]
fn allowed_projection_tight_budget_thread_identity_child() {
    println!(
        "ALLOWED_PROJECTION_BUDGET_FINGERPRINT:{}:{}",
        geom_kernel::current_threads(),
        tight_budget_fingerprint()
    );
}

#[test]
fn tight_budget_errors_are_exact_at_1_4_16_threads() {
    let expected = tight_budget_fingerprint();
    let executable = std::env::current_exe().unwrap();
    for threads in ["1", "4", "16"] {
        let output = Command::new(&executable)
            .args([
                "--exact",
                "allowed_projection_tight_budget_thread_identity_child",
                "--nocapture",
            ])
            .env("PLINTH_THREADS", threads)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "tight-budget child thread count {threads} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let line = stdout
            .lines()
            .find(|line| line.starts_with("ALLOWED_PROJECTION_BUDGET_FINGERPRINT:"))
            .unwrap();
        let expected_line = format!("ALLOWED_PROJECTION_BUDGET_FINGERPRINT:{threads}:{expected}");
        assert_eq!(line, expected_line);
    }
}

#[test]
fn allowed_projection_thread_identity_child() {
    let (surface, allowed, points) = generated_surface();
    let outcomes = surface
        .project_allowed_faces_batch(&allowed, &points, uncapped(), limits())
        .unwrap();
    let mut hash = 1_469_598_103_934_665_603u64;
    for outcome in outcomes {
        match outcome {
            AllowedFaceProjectionOutcome::Projected(projection) => {
                update_hash(&mut hash, 1);
                update_hash(&mut hash, projection.face_id.into());
                update_hash(&mut hash, projection.distance_mm.to_bits());
                for component in projection.point {
                    update_hash(&mut hash, component.to_bits());
                }
            }
            AllowedFaceProjectionOutcome::NoWinner(_) => update_hash(&mut hash, 0),
        }
    }
    println!(
        "ALLOWED_PROJECTION_FINGERPRINT:{}:{hash}",
        geom_kernel::current_threads()
    );
}

#[test]
fn allowed_projection_is_exact_and_digest_identical_at_1_4_16_threads() {
    let executable = std::env::current_exe().unwrap();
    let mut hashes = Vec::new();
    for threads in ["1", "4", "16"] {
        let output = Command::new(&executable)
            .args([
                "--exact",
                "allowed_projection_thread_identity_child",
                "--nocapture",
            ])
            .env("PLINTH_THREADS", threads)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child thread count {threads} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let line = stdout
            .lines()
            .find(|line| line.starts_with("ALLOWED_PROJECTION_FINGERPRINT:"))
            .unwrap();
        let fields: Vec<_> = line.split(':').collect();
        assert_eq!(fields[1], threads);
        hashes.push(fields[2].to_owned());
    }
    assert!(hashes.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn config_accessor_returns_exact_immutable_surface_authority() {
    let surface = geom_kernel::NativeSurface::from_indexed(
        &[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
        &[[0, 1, 2]],
        NativeSurfaceConfig {
            grid_pitch_mm: 0.037,
            max_grid_bytes: 12_345_678,
        },
    )
    .unwrap();
    assert_eq!(
        surface.config(),
        NativeSurfaceConfig {
            grid_pitch_mm: 0.037,
            max_grid_bytes: 12_345_678,
        }
    );
    assert_eq!(surface.subset(&[0]).unwrap().config(), surface.config());
}
