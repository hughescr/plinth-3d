use geom_kernel::{
    indexed_triangle_soup_digest, triangle_soup_digest, BatchInputError, NativeSurface,
    NativeSurfaceConfig, NearFaceBatchLimits, PenetrationBatchError, PenetrationBatchLimits,
    PlaneIntersectionKind, PlaneSliceError, PlaneSliceLimits, SurfaceBuildError,
    SurfaceSubsetError, TriangleSoup,
};
use std::process::Command;

const TRIANGLES: [[[f64; 3]; 3]; 1] = [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]];
const NORMALS: [[f64; 3]; 1] = [[0.0, 0.0, 1.0]];
const AREAS: [f64; 1] = [0.5];
const BOUNDS: [[f64; 3]; 2] = [[0.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
const TEST_UNBOUNDED_GRID_BYTES: usize = usize::MAX;

fn surface() -> NativeSurface {
    NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(&TRIANGLES, &NORMALS, &AREAS, BOUNDS),
        2.0,
        TEST_UNBOUNDED_GRID_BYTES,
    )
    .unwrap()
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn cubes_surface(origins: &[[f64; 3]]) -> NativeSurface {
    const VERTICES: [[f64; 3]; 8] = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ];
    const FACES: [[u32; 3]; 12] = [
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
    ];
    let mut vertices = Vec::with_capacity(origins.len() * VERTICES.len());
    let mut faces = Vec::with_capacity(origins.len() * FACES.len());
    for &origin in origins {
        let offset = vertices.len() as u32;
        vertices.extend(VERTICES.map(|vertex| {
            [
                vertex[0] + origin[0],
                vertex[1] + origin[1],
                vertex[2] + origin[2],
            ]
        }));
        faces.extend(FACES.map(|[a, b, c]| [a + offset, b + offset, c + offset]));
    }
    NativeSurface::from_indexed(
        &vertices,
        &faces,
        NativeSurfaceConfig {
            grid_pitch_mm: 2.0,
            max_grid_bytes: usize::MAX,
        },
    )
    .unwrap()
}

fn cube_surface() -> NativeSurface {
    cubes_surface(&[[0.0; 3]])
}

fn slice_limits() -> PlaneSliceLimits {
    PlaneSliceLimits {
        max_planes: usize::MAX,
        max_triangle_plane_tests: usize::MAX,
        max_intersections: usize::MAX,
        max_output_bytes: usize::MAX,
    }
}

#[test]
fn slice_z_batch_matches_frozen_box_multiplane_vectors() {
    let surface = cube_surface();
    let batch = surface
        .slice_z_batch(&[0.25, 0.75], slice_limits())
        .unwrap();
    assert_eq!(batch.plane_offsets, vec![0, 8, 16]);
    assert_eq!(batch.candidate_work, 16);
    let expected = [
        [[0.25, 0.0, 0.25], [1.0, 0.0, 0.25]],
        [[0.25, 0.0, 0.25], [0.0, 0.0, 0.25]],
        [[1.0, 0.25, 0.25], [1.0, 1.0, 0.25]],
        [[1.0, 0.25, 0.25], [1.0, 0.0, 0.25]],
        [[0.75, 1.0, 0.25], [0.0, 1.0, 0.25]],
        [[0.75, 1.0, 0.25], [1.0, 1.0, 0.25]],
        [[0.0, 0.75, 0.25], [0.0, 0.0, 0.25]],
        [[0.0, 0.75, 0.25], [0.0, 1.0, 0.25]],
    ];
    assert_eq!(
        batch
            .plane(0)
            .iter()
            .map(|event| event.face_id)
            .collect::<Vec<_>>(),
        (4..12).collect::<Vec<_>>()
    );
    assert_eq!(
        batch
            .plane(0)
            .iter()
            .map(|event| event.segment)
            .collect::<Vec<_>>(),
        expected
    );
    assert!(batch
        .intersections
        .iter()
        .all(|event| event.kind == PlaneIntersectionKind::Basic));
}

#[test]
fn slice_z_batch_matches_frozen_tetrahedron_case_order_and_segments() {
    let triangles = [
        [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
        [[0.0, 0.0, 0.0], [0.0, 0.0, 2.0], [2.0, 0.0, 0.0]],
        [[2.0, 0.0, 0.0], [0.0, 0.0, 2.0], [0.0, 2.0, 0.0]],
        [[0.0, 2.0, 0.0], [0.0, 0.0, 2.0], [0.0, 0.0, 0.0]],
    ];
    let surface = NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(
            &triangles,
            &[[0.0, 0.0, 1.0]; 4],
            &[1.0; 4],
            [[0.0; 3], [2.0; 3]],
        ),
        2.0,
        usize::MAX,
    )
    .unwrap();
    let batch = surface.slice_z_batch(&[1.0], slice_limits()).unwrap();
    assert_eq!(batch.plane_offsets, vec![0, 3]);
    assert_eq!(
        batch
            .plane(0)
            .iter()
            .map(|event| event.face_id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert!(batch
        .plane(0)
        .iter()
        .all(|event| event.kind == PlaneIntersectionKind::Basic));
    assert_eq!(
        batch.plane(0)[0].segment,
        [[1.0, 0.0, 1.0], [0.0, 0.0, 1.0]]
    );
    assert_eq!(
        batch.plane(0)[1].segment,
        [[0.0, 1.0, 1.0], [1.0, 0.0, 1.0]]
    );
    assert_eq!(
        batch.plane(0)[2].segment,
        [[0.0, 0.0, 1.0], [0.0, 1.0, 1.0]]
    );
}

#[test]
fn slice_z_batch_matches_frozen_shared_edge_coplanar_vertex_and_tolerance_fixture() {
    let vertices = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 1.0],
        [0.0, -1.0, -1.0],
        [2.0, 0.0, 0.0],
        [2.0, 1.0, 0.0],
        [0.0, 2.0, 1.0],
        [0.0, 3.0, -1.0],
        [4.0, 0.0, 0.0],
        [5.0, 0.0, 5.0e-9],
        [4.0, 1.0, 1.0],
    ];
    let faces = [
        [0, 1, 2],  // positive owner: shared edge is emitted
        [1, 0, 3],  // negative owner: shared edge is suppressed
        [0, 1, 5],  // fully coplanar patch
        [1, 5, 4],  // fully coplanar patch
        [0, 6, 7],  // vertex crossing
        [0, 2, 6],  // vertex tangent, suppressed
        [8, 9, 10], // 5e-9 is on-plane under frozen tol.merge
    ];
    let surface = NativeSurface::from_indexed(
        &vertices,
        &faces,
        NativeSurfaceConfig {
            grid_pitch_mm: 2.0,
            max_grid_bytes: usize::MAX,
        },
    )
    .unwrap();
    let batch = surface.slice_z_batch(&[0.0], slice_limits()).unwrap();
    assert_eq!(batch.plane_offsets, vec![0, 3]);
    assert_eq!(
        batch
            .plane(0)
            .iter()
            .map(|event| event.face_id)
            .collect::<Vec<_>>(),
        vec![4, 0, 6]
    );
    assert_eq!(batch.plane(0)[0].kind, PlaneIntersectionKind::Vertex);
    assert_eq!(
        batch.plane(0)[0].segment,
        [[0.0, 0.0, 0.0], [0.0, 2.5, 0.0]]
    );
    assert_eq!(batch.plane(0)[1].kind, PlaneIntersectionKind::PositiveEdge);
    assert_eq!(
        batch.plane(0)[1].segment,
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]]
    );
    assert_eq!(batch.plane(0)[2].kind, PlaneIntersectionKind::PositiveEdge);
    assert_eq!(
        batch.plane(0)[2].segment,
        [[4.0, 0.0, 0.0], [5.0, 0.0, 5.0e-9]]
    );
}

#[test]
#[expect(
    clippy::float_cmp,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn slice_z_batch_uses_exact_source_vertex_bits_at_tol_merge_boundary_neighbors() {
    const PLANE_BITS: u64 = 0x3eb0_9c04_82f1_8c75;
    const LOW_Z_BITS: u64 = 0x3eb0_c6f7_a0b5_ed8d;
    const EXPECTED_B: [u64; 3] = [0x3ff0_0000_0000_0000, 0, LOW_Z_BITS];
    const EXPECTED_C: [u64; 3] = [0, 0x3ff0_0000_0000_0000, LOW_Z_BITS];

    let plane = f64::from_bits(PLANE_BITS);
    assert_eq!(plane, 9.9e-7);
    let planes = [
        f64::from_bits(PLANE_BITS - 1),
        plane,
        f64::from_bits(PLANE_BITS + 1),
    ];
    let triangle = [
        [0.0, 0.0, 0.1],
        [1.0, 0.0, f64::from_bits(LOW_Z_BITS)],
        [0.0, 1.0, f64::from_bits(LOW_Z_BITS)],
    ];
    let indexed = NativeSurface::from_indexed(
        &triangle,
        &[[0, 1, 2]],
        NativeSurfaceConfig {
            grid_pitch_mm: 2.0,
            max_grid_bytes: usize::MAX,
        },
    )
    .unwrap();
    let soup = NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(
            &[triangle],
            &[[0.0, 0.0, 1.0]],
            &[0.5],
            [[0.0, 0.0, f64::from_bits(LOW_Z_BITS)], [1.0, 1.0, 0.1]],
        ),
        2.0,
        usize::MAX,
    )
    .unwrap();

    for surface in [&indexed, &soup] {
        let batch = surface.slice_z_batch(&planes, slice_limits()).unwrap();
        // Frozen Trimesh: one ULP below makes both low vertices positive;
        // the exact plane and one ULP above classify them on-plane.
        assert_eq!(batch.plane_offsets, vec![0, 0, 1, 2]);
        assert_eq!(batch.intersections.len(), 2);
        for event in &batch.intersections {
            assert_eq!(event.kind, PlaneIntersectionKind::PositiveEdge);
            assert_eq!(event.segment[0].map(f64::to_bits), EXPECTED_B);
            assert_eq!(event.segment[1].map(f64::to_bits), EXPECTED_C);
        }
    }
}

#[test]
fn slice_z_batch_preserves_both_frozen_face_owners_for_duplicate_positive_edge() {
    let vertices = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 1.0],
        [0.0, -1.0, 2.0],
    ];
    let surface = NativeSurface::from_indexed(
        &vertices,
        &[[0, 1, 2], [1, 0, 3]],
        NativeSurfaceConfig {
            grid_pitch_mm: 2.0,
            max_grid_bytes: usize::MAX,
        },
    )
    .unwrap();
    let batch = surface.slice_z_batch(&[0.0], slice_limits()).unwrap();
    assert_eq!(batch.plane_offsets, vec![0, 2]);
    assert_eq!(
        batch.plane(0),
        [
            geom_kernel::PlaneIntersection {
                face_id: 0,
                kind: PlaneIntersectionKind::PositiveEdge,
                segment: [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            },
            geom_kernel::PlaneIntersection {
                face_id: 1,
                kind: PlaneIntersectionKind::PositiveEdge,
                segment: [[1.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
            },
        ]
    );
}

#[test]
fn slice_z_batch_preflights_bvh_candidate_caps_at_exact_boundary() {
    let surface = cube_surface();
    let planes = [0.5];
    let candidates = 8;
    let output_bytes = 2 * std::mem::size_of::<usize>()
        + candidates * std::mem::size_of::<geom_kernel::PlaneIntersection>();
    let limits = PlaneSliceLimits {
        max_planes: 1,
        max_triangle_plane_tests: candidates,
        max_intersections: candidates,
        max_output_bytes: output_bytes,
    };
    surface
        .slice_z_batch(&planes, limits)
        .expect("exact candidate-derived cap accepted");
    assert!(matches!(
        surface.slice_z_batch(
            &planes,
            PlaneSliceLimits {
                max_planes: 0,
                ..limits
            }
        ),
        Err(PlaneSliceError::PlaneCountCap { .. })
    ));
    assert!(matches!(
        surface.slice_z_batch(
            &planes,
            PlaneSliceLimits {
                max_triangle_plane_tests: limits.max_triangle_plane_tests - 1,
                ..limits
            }
        ),
        Err(PlaneSliceError::TrianglePlaneWorkCap { .. })
    ));
    assert!(matches!(
        surface.slice_z_batch(
            &planes,
            PlaneSliceLimits {
                max_intersections: limits.max_intersections - 1,
                ..limits
            }
        ),
        Err(PlaneSliceError::IntersectionCap { .. })
    ));
    assert!(matches!(
        surface.slice_z_batch(
            &planes,
            PlaneSliceLimits {
                max_output_bytes: limits.max_output_bytes - 1,
                ..limits
            }
        ),
        Err(PlaneSliceError::OutputBytesCap { .. })
    ));
}

#[test]
fn slice_z_batch_outside_mesh_has_zero_candidate_work_not_faces_times_planes() {
    let surface = cube_surface();
    let limits = PlaneSliceLimits {
        max_planes: 2,
        max_triangle_plane_tests: 0,
        max_intersections: 0,
        max_output_bytes: 3 * std::mem::size_of::<usize>(),
    };
    let batch = surface.slice_z_batch(&[-10.0, 10.0], limits).unwrap();
    assert_eq!(batch.candidate_work, 0);
    assert_eq!(batch.plane_offsets, vec![0, 0, 0]);
    assert!(batch.intersections.is_empty());
}

fn update_slice_hash(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(1_099_511_628_211);
}

#[test]
fn slicing_thread_identity_child() {
    let surface = cube_surface();
    let planes: Vec<f64> = (0..8192)
        .map(|index| if index % 2 == 0 { 0.25 } else { 0.75 })
        .collect();
    let batch = surface.slice_z_batch(&planes, slice_limits()).unwrap();
    let mut hash = 1_469_598_103_934_665_603u64;
    for &offset in &batch.plane_offsets {
        update_slice_hash(&mut hash, offset as u64);
    }
    for event in &batch.intersections {
        update_slice_hash(&mut hash, event.face_id as u64);
        update_slice_hash(&mut hash, event.kind as u64);
        for point in event.segment {
            for value in point {
                update_slice_hash(&mut hash, value.to_bits());
            }
        }
    }
    println!(
        "SLICE_FINGERPRINT:{}:{}:{}:{hash}",
        batch.effective_threads, batch.participating_threads, batch.candidate_work
    );
}

#[test]
fn slicing_thread_identity_is_subprocess_invariant() {
    let executable = std::env::current_exe().unwrap();
    let mut fingerprints = Vec::new();
    for threads in ["1", "4", "16"] {
        let output = Command::new(&executable)
            .args(["--exact", "slicing_thread_identity_child", "--nocapture"])
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
            .find(|line| line.starts_with("SLICE_FINGERPRINT:"))
            .unwrap();
        let fields: Vec<&str> = line.split(':').collect();
        assert_eq!(fields[1], threads, "effective pool mismatch: {line}");
        let participating: usize = fields[2].parse().unwrap();
        let configured: usize = threads.parse().unwrap();
        assert!(
            (1..=configured).contains(&participating),
            "actual participation is outside the configured pool: {line}"
        );
        fingerprints.push((fields[3].to_owned(), fields[4].to_owned()));
    }
    assert!(
        fingerprints.windows(2).all(|pair| pair[0] == pair[1]),
        "{fingerprints:?}"
    );
}

fn assert_intervals_equal(
    left: &geom_kernel::PenetrationIntervalsClassified,
    right: &geom_kernel::PenetrationIntervalsClassified,
) {
    // The uncertainty tallies are part of the answer, not decoration: two runs
    // that agree on every interval but disagree on which rows were undecidable
    // have NOT produced the same result.
    assert_eq!(left.near_parallel, right.near_parallel);
    let (left, right) = (&left.rows, &right.rows);
    assert_eq!(left.len(), right.len());
    for (left_row, right_row) in left.iter().zip(right) {
        assert_eq!(left_row.len(), right_row.len());
        for (left, right) in left_row.iter().zip(right_row) {
            assert_eq!(left.s_a.to_bits(), right.s_a.to_bits());
            assert_eq!(left.s_b.to_bits(), right.s_b.to_bits());
            assert_eq!(left.entered, right.entered);
            assert_eq!(left.exited, right.exited);
            assert_eq!(left.max_depth.to_bits(), right.max_depth.to_bits());
            assert_eq!(left.ray, right.ray);
        }
    }
}

fn bounded_penetration(
    surface: &NativeSurface,
    p0s: &[[f64; 3]],
    p1s: &[[f64; 3]],
    limits: PenetrationBatchLimits,
) -> Result<geom_kernel::PenetrationIntervalsClassified, PenetrationBatchError> {
    surface.penetration_intervals_batch_bounded_classified(
        p0s,
        p1s,
        &vec![0.0; p0s.len()],
        &vec![0.0; p0s.len()],
        0,
        0.12,
        8,
        0.05,
        false,
        limits,
    )
}

fn generous_penetration_limits() -> PenetrationBatchLimits {
    PenetrationBatchLimits {
        max_rows: usize::MAX,
        max_total_work: usize::MAX,
        max_total_hits: usize::MAX,
        max_total_samples: usize::MAX,
        max_total_intervals: usize::MAX,
        max_scratch_bytes: usize::MAX,
    }
}

fn generous_near_face_limits() -> NearFaceBatchLimits {
    NearFaceBatchLimits {
        max_rows: usize::MAX,
        max_faces_per_row: usize::MAX,
        max_total_faces: usize::MAX,
    }
}

/// A probe held 0.1 mm above the top of the unit cube, over the half of that
/// top face the FIRST of its two triangles owns.
///
/// Every distance the probe can reach is separated from every other one: 0.1 mm
/// to that triangle, sqrt(0.02) = 0.1414 mm to the nearest `y = 0` side
/// triangle, sqrt(0.03) = 0.1732 mm to the top face's OTHER triangle, and more
/// than 0.4 mm to everything else. One triangle therefore wins each answer
/// outright, so two views of the same geometry cannot disagree merely because
/// their BVH leaf order broke a tie differently -- the equality assertions below
/// are about the face IDs the views speak, not about traversal luck.
const SEAT_PROBE_P0: [f64; 3] = [0.5, 0.1, 1.1];
const SEAT_PROBE_P1: [f64; 3] = [0.5, 0.3, 1.1];
/// Between 0.1 (the top triangle, IN) and 0.1414 (the nearest side, OUT).
const SEAT_PROBE_BAND_MM: f64 = 0.12;
/// Probe-to-top-triangle distance, with radii, standoff and insets all zero.
const SEAT_PROBE_OPEN_CLEARANCE_MM: f64 = 0.1;
/// Probe-to-nearest-`y = 0`-side-triangle distance, once the band is exempt:
/// sqrt(0.1^2 + 0.1^2).
const SEAT_PROBE_EXEMPT_CLEARANCE_MM: f64 = std::f64::consts::SQRT_2 * 0.1;

/// What one view of the geometry answered when the seat-exemption pair was run
/// against it.
#[derive(Clone, Debug, PartialEq)]
struct SeatExemptionAnswer {
    band: Vec<u32>,
    open_clearance_mm: f64,
    open_point: [f64; 3],
    exempt_clearance_mm: f64,
    exempt_point: [f64; 3],
}

/// The production seat-exemption pair, run against one view of the geometry.
///
/// `faces_near_segments_batch` names the seat's own band and those exact global
/// face IDs become the exclusion set of the `segment_clearance_excluding` row --
/// the pairing `plinth-support` runs (seat_zone.rs -> brace.rs), and the pairing
/// whose two halves both have to agree on what a face ID MEANS.
fn seat_exemption_pair(surface: &NativeSurface) -> SeatExemptionAnswer {
    let p0s = [SEAT_PROBE_P0];
    let p1s = [SEAT_PROBE_P1];
    let zero = [0.0];
    let band = surface
        .faces_near_segments_batch_classified(
            &p0s,
            &p1s,
            &[SEAT_PROBE_BAND_MM],
            generous_near_face_limits(),
        )
        .unwrap();
    let open = surface
        .segment_clearance_classified(&p0s, &p1s, &zero, &zero, 0.0, &zero, &zero)
        .unwrap();
    let exempt_rows: Vec<&[u32]> = band.rows.iter().map(Vec::as_slice).collect();
    let exempt = surface
        .segment_clearance_excluding_classified(
            &p0s,
            &p1s,
            &zero,
            &zero,
            0.0,
            &zero,
            &zero,
            &exempt_rows,
        )
        .unwrap();
    // Both halves must be MEASURED rows. An `UpperBound` here would mean the
    // seat-exemption numbers below are one-sided, and grading them as if they
    // were measurements is precisely the defect the third state exists to
    // expose, so it panics rather than unwrapping to a plausible number.
    let measured = |row: geom_kernel::SegmentClearanceClassified| match row {
        geom_kernel::SegmentClearanceClassified::Measured {
            clearance_mm,
            witness,
        } => (clearance_mm, witness),
        other => panic!("seat exemption probe expected a measured row, got {other:?}"),
    };
    let (open_clearance_mm, open_point) = measured(open[0]);
    let (exempt_clearance_mm, exempt_point) = measured(exempt[0]);
    assert_eq!(
        band.near_parallel,
        [0],
        "the seat probe fixture must not be undecidable, or the band below is \
         a proof-only subset of the faces the exemption meant to name"
    );
    SeatExemptionAnswer {
        band: band.rows[0].clone(),
        open_clearance_mm,
        open_point,
        exempt_clearance_mm,
        exempt_point,
    }
}

/// Assert the pair behaved: the exemption was NOT vacuous -- it moved the answer
/// off the face it forgave -- and both halves landed on the frozen distances.
fn assert_seat_exemption_answers(answer: &SeatExemptionAnswer, what: &str) {
    let open = answer.open_clearance_mm;
    let exempt = answer.exempt_clearance_mm;
    assert!(
        (open - SEAT_PROBE_OPEN_CLEARANCE_MM).abs() < 1e-12,
        "{what}: unexempt clearance {open} is not the 0.1 mm gap to the top triangle"
    );
    assert!(
        (exempt - SEAT_PROBE_EXEMPT_CLEARANCE_MM).abs() < 1e-12,
        "{what}: exempting the band left {exempt}, not the sqrt(0.02) mm gap to the side"
    );
}

/// The batch's counted (census-derived) limits — since FU-8g H1 these are the
/// numbers the bounded batch charges, so exact/minus-one refusal probes must
/// derive from them rather than from the worst-case sizing helper.
#[allow(clippy::too_many_arguments)]
fn census_penetration_limits(
    surface: &NativeSurface,
    p0s: &[[f64; 3]],
    p1s: &[[f64; 3]],
    r0s: &[f64],
    r1s: &[f64],
    n_offset_rays: usize,
    cap: usize,
    majority_step_mm: f64,
    rays_all: bool,
) -> PenetrationBatchLimits {
    let census = surface
        .penetration_census_batch_classified(p0s, p1s, r0s, r1s, n_offset_rays, rays_all)
        .unwrap();
    surface
        .penetration_intervals_batch_census_limits_classified(
            &census,
            n_offset_rays,
            cap,
            majority_step_mm,
            rays_all,
        )
        .unwrap()
}

const HETEROGENEOUS_P0: [[f64; 3]; 3] = [[-1.0, 0.5, 0.5], [0.25, 0.25, -1.0], [1.25, 2.0, 0.5]];
const HETEROGENEOUS_P1: [[f64; 3]; 3] = [[4.0, 0.5, 0.5], [0.25, 0.25, 2.0], [3.75, 2.0, 0.5]];
const HETEROGENEOUS_RADII: [f64; 3] = [0.1, 0.05, 0.2];

fn heterogeneous_penetration(
    surface: &NativeSurface,
    limits: PenetrationBatchLimits,
) -> Result<geom_kernel::PenetrationIntervalsClassified, PenetrationBatchError> {
    surface.penetration_intervals_batch_bounded_classified(
        &HETEROGENEOUS_P0,
        &HETEROGENEOUS_P1,
        &HETEROGENEOUS_RADII,
        &HETEROGENEOUS_RADII,
        2,
        0.12,
        8,
        0.05,
        true,
        limits,
    )
}

#[test]
fn bounded_penetration_frozen_heterogeneous_rows_are_bit_exact_and_ordered() {
    let surface = cubes_surface(&[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]]);
    let rows = heterogeneous_penetration(&surface, generous_penetration_limits()).unwrap();
    type FrozenInterval = (i32, u64, u64, u64, bool, bool);
    let expected: [&[FrozenInterval]; 3] = [
        &[
            (
                -1,
                4_607_182_418_800_017_408,
                4_611_686_018_427_387_904,
                4_601_678_019_255_453_472,
                true,
                true,
            ),
            (
                -1,
                4_613_937_818_241_073_153,
                4_616_189_618_054_758_400,
                4_601_678_019_255_453_480,
                true,
                true,
            ),
            (
                0,
                4_607_182_418_800_017_408,
                4_611_686_018_427_387_904,
                4_600_877_379_321_698_714,
                true,
                true,
            ),
            (
                0,
                4_613_937_818_241_073_153,
                4_616_189_618_054_758_400,
                4_600_877_379_321_698_714,
                true,
                true,
            ),
            (
                1,
                4_607_182_418_800_017_408,
                4_611_686_018_427_387_904,
                4_600_877_379_321_698_714,
                true,
                true,
            ),
            (
                1,
                4_613_937_818_241_073_153,
                4_616_189_618_054_758_400,
                4_600_877_379_321_698_714,
                true,
                true,
            ),
        ],
        &[
            (
                -1,
                4_607_182_418_800_017_408,
                4_611_686_018_427_387_904,
                4_598_175_219_545_276_416,
                true,
                true,
            ),
            (
                0,
                4_607_182_418_800_017_408,
                4_611_686_018_427_387_904,
                4_598_175_219_545_276_416,
                true,
                true,
            ),
            (
                1,
                4_607_182_418_800_017_408,
                4_611_686_018_427_387_904,
                4_596_373_779_694_328_218,
                true,
                true,
            ),
        ],
        &[],
    ];
    assert_eq!(rows.rows.len(), expected.len());
    assert_eq!(
        rows.near_parallel,
        vec![0; expected.len()],
        "frozen fixture must be fully decided"
    );
    for (row, expected_row) in rows.rows.iter().zip(expected) {
        let actual = row
            .iter()
            .map(|interval| {
                (
                    interval.ray,
                    interval.s_a.to_bits(),
                    interval.s_b.to_bits(),
                    interval.max_depth.to_bits(),
                    interval.entered,
                    interval.exited,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected_row);
    }
}

#[test]
fn bounded_penetration_preflights_every_resource_at_exact_cap() {
    let surface = cube_surface();
    let p0s = [[-1.0, 0.5, 0.5]];
    let p1s = [[2.0, 0.5, 0.5]];
    // FU-8g H1 re-pin: the bounded batch now charges the census pass's
    // counted totals, so the exact cap is the counted demand, not the
    // worst-case shape. The worst-case sizing helper must still be a
    // sufficient ceiling, and both admissions must produce bit-identical
    // intervals — the shared-path identity claim.
    let radii = vec![0.0; p0s.len()];
    let limits = census_penetration_limits(&surface, &p0s, &p1s, &radii, &radii, 0, 8, 0.12, false);
    let worst_case = surface
        .penetration_intervals_batch_worst_case_limits(1, 0, 8, false)
        .unwrap();
    assert!(limits.max_total_work <= worst_case.max_total_work);
    assert!(limits.max_total_hits <= worst_case.max_total_hits);
    assert!(limits.max_total_samples <= worst_case.max_total_samples);
    assert!(limits.max_total_intervals <= worst_case.max_total_intervals);
    assert!(limits.max_scratch_bytes <= worst_case.max_scratch_bytes);
    assert_intervals_equal(
        &bounded_penetration(&surface, &p0s, &p1s, limits).unwrap(),
        &bounded_penetration(&surface, &p0s, &p1s, worst_case).unwrap(),
    );
    let bounded = bounded_penetration(&surface, &p0s, &p1s, limits).unwrap();
    assert_eq!(bounded.rows.len(), 1);
    assert_eq!(bounded.near_parallel, vec![0]);
    assert_eq!(bounded.rows[0].len(), 1);
    let interval = &bounded.rows[0][0];
    assert_eq!(interval.s_a.to_bits(), 1.0f64.to_bits());
    assert_eq!(interval.s_b.to_bits(), 2.0f64.to_bits());
    assert_eq!(interval.max_depth.to_bits(), 4_601_678_019_255_453_472);
    assert!(interval.entered);
    assert!(interval.exited);
    assert_eq!(interval.ray, -1);

    for (resource, reduced) in [
        (
            "rows",
            geom_kernel::PenetrationBatchLimits {
                max_rows: limits.max_rows - 1,
                ..limits
            },
        ),
        (
            "work",
            geom_kernel::PenetrationBatchLimits {
                max_total_work: limits.max_total_work - 1,
                ..limits
            },
        ),
        (
            "hits",
            geom_kernel::PenetrationBatchLimits {
                max_total_hits: limits.max_total_hits - 1,
                ..limits
            },
        ),
        (
            "samples",
            geom_kernel::PenetrationBatchLimits {
                max_total_samples: limits.max_total_samples - 1,
                ..limits
            },
        ),
        (
            "intervals",
            geom_kernel::PenetrationBatchLimits {
                max_total_intervals: limits.max_total_intervals - 1,
                ..limits
            },
        ),
        (
            "scratch bytes",
            geom_kernel::PenetrationBatchLimits {
                max_scratch_bytes: limits.max_scratch_bytes - 1,
                ..limits
            },
        ),
    ] {
        assert!(matches!(
            bounded_penetration(&surface, &p0s, &p1s, reduced),
            Err(PenetrationBatchError::ResourceLimitExceeded { resource: actual, .. }) if actual == resource
        ));
    }
}

#[test]
fn bounded_penetration_handles_zero_rows_and_checked_overflow() {
    let surface = cube_surface();
    let zero = geom_kernel::PenetrationBatchLimits {
        max_rows: 0,
        max_total_work: 0,
        max_total_hits: 0,
        max_total_samples: 0,
        max_total_intervals: 0,
        max_scratch_bytes: 0,
    };
    assert_eq!(
        surface
            .penetration_intervals_batch_bounded_classified(
                &[],
                &[],
                &[],
                &[],
                0,
                0.12,
                8,
                0.05,
                false,
                zero,
            )
            .unwrap()
            .rows
            .len(),
        0
    );
    assert!(matches!(
        surface.penetration_intervals_batch_worst_case_limits(1, usize::MAX, 8, true),
        Err(PenetrationBatchError::ArithmeticOverflow { .. })
    ));
}

#[test]
fn bounded_penetration_high_crossing_mesh_has_one_ordered_refusal() {
    let surface = cubes_surface(&[
        [0.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [4.0, 0.0, 0.0],
        [6.0, 0.0, 0.0],
    ]);
    let p0s = [[-1.0, 0.5, 0.5], [-1.0, 0.5, 0.5]];
    let p1s = [[8.0, 0.5, 0.5], [8.0, 0.5, 0.5]];
    // FU-8g H1 re-pin: the exact hits cap is the census's counted crossings.
    let radii = vec![0.0; p0s.len()];
    let limits = census_penetration_limits(&surface, &p0s, &p1s, &radii, &radii, 0, 8, 0.12, false);
    let intervals = bounded_penetration(&surface, &p0s, &p1s, limits).unwrap();
    assert_eq!(intervals.rows.len(), 2);
    assert_eq!(intervals.near_parallel, vec![0, 0]);
    assert!(intervals.rows[0].len() >= 4, "all four cubes are crossed");
    let below = geom_kernel::PenetrationBatchLimits {
        max_total_hits: limits.max_total_hits - 1,
        ..limits
    };
    assert!(matches!(
        bounded_penetration(&surface, &p0s, &p1s, below),
        Err(PenetrationBatchError::ResourceLimitExceeded {
            resource: "hits",
            required,
            limit,
        }) if required == limits.max_total_hits && limit == limits.max_total_hits - 1
    ));
}

#[test]
fn bounded_penetration_deep_bvh_and_parity_scratch_obey_exact_cap() {
    let origins = (0..128)
        .map(|index| [2.0 * index as f64, 0.0, 0.0])
        .collect::<Vec<_>>();
    let surface = cubes_surface(&origins);
    assert!(surface.bvh.depth > 4);
    assert!(surface.pv.grids.iter().any(|grid| grid.max_occ > 0));
    let p0s = [[-1.0, 0.5, 0.5]];
    let p1s = [[256.0, 0.5, 0.5]];
    // FU-8g H1 re-pin: the exact scratch cap is the census-derived per-row
    // sum, which still carries the deep-BVH traversal stack and parity terms.
    let radii = vec![0.0; p0s.len()];
    let limits = census_penetration_limits(&surface, &p0s, &p1s, &radii, &radii, 0, 8, 0.12, false);
    assert!(bounded_penetration(&surface, &p0s, &p1s, limits).is_ok());
    let below = PenetrationBatchLimits {
        max_scratch_bytes: limits.max_scratch_bytes - 1,
        ..limits
    };
    assert!(matches!(
        bounded_penetration(&surface, &p0s, &p1s, below),
        Err(PenetrationBatchError::ResourceLimitExceeded {
            resource: "scratch bytes",
            required,
            limit,
        }) if required == limits.max_scratch_bytes && limit == limits.max_scratch_bytes - 1
    ));
}

#[test]
fn penetration_census_severs_work_from_hits_and_prices_the_counted_row() {
    let surface = cubes_surface(&[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]]);
    let p0s = [[-1.0, 0.5, 0.5]];
    let p1s = [[4.0, 0.5, 0.5]];
    let radii = [0.0];
    let census = surface
        .penetration_census_batch_classified(&p0s, &p1s, &radii, &radii, 0, false)
        .unwrap();
    assert_eq!(census.len(), 1);
    let row = census[0];
    // The ray crosses two unit cubes: at least four crossings, and the
    // traversal visits nodes as well as triangles, so tests strictly exceeds
    // crossings — the two quantities measure different things.
    assert!(row.crossings >= 4);
    assert!(row.tests > row.crossings);
    assert_eq!(row.seg_mm.to_bits(), 5.0f64.to_bits());
    let limits = surface
        .penetration_intervals_batch_census_limits_classified(&census, 0, 8, 0.12, false)
        .unwrap();
    // work is charged as traversal tests, hits as crossings: severed.
    assert_eq!(limits.max_total_work, row.tests);
    assert_eq!(limits.max_total_hits, row.crossings);
    assert_ne!(limits.max_total_work, limits.max_total_hits);
    // The counted numbers are the exact admission boundary.
    assert!(bounded_penetration(&surface, &p0s, &p1s, limits).is_ok());
    assert!(matches!(
        bounded_penetration(
            &surface,
            &p0s,
            &p1s,
            PenetrationBatchLimits {
                max_total_work: limits.max_total_work - 1,
                ..limits
            }
        ),
        Err(PenetrationBatchError::ResourceLimitExceeded {
            resource: "work",
            required,
            ..
        }) if required == row.tests
    ));
}

#[test]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn penetration_census_sample_budget_is_the_closed_form_bound() {
    let surface = cubes_surface(&[[0.0, 0.0, 0.0]]);
    let p0s = [[-1.0, 0.5, 0.5]];
    let p1s = [[2.0, 0.5, 0.5]];
    let radii = [0.0];
    let census = surface
        .penetration_census_batch_classified(&p0s, &p1s, &radii, &radii, 0, false)
        .unwrap();
    let row = census[0];
    // At cap = 80 the F10 arm governs: 3 x subsegments + ceil(seg / step)
    // beats cap x subsegments, so the sample charge is the closed-form honest
    // bound rather than the cap-saturated product.
    let subsegments = row.crossings + 1;
    let expected = (3 * subsegments + (row.seg_mm / 0.12).ceil() as usize).min(80 * subsegments);
    assert!(expected < 80 * subsegments);
    let limits = surface
        .penetration_intervals_batch_census_limits_classified(&census, 0, 80, 0.12, false)
        .unwrap();
    assert_eq!(limits.max_total_samples, expected);
    // The bound admits the execution it prices: the batch runs at exactly it.
    let rows = surface
        .penetration_intervals_batch_bounded_classified(
            &p0s, &p1s, &radii, &radii, 0, 0.12, 80, 0.05, false, limits,
        )
        .unwrap();
    assert_eq!(rows.rows.len(), 1);
    assert_eq!(rows.near_parallel, vec![0]);
}

#[test]
fn penetration_census_degenerate_row_counts_zero_and_executes_empty() {
    let surface = cube_surface();
    let p0s = [[5.0, 5.0, 5.0]];
    let p1s = [[5.0, 5.0, 5.0]];
    let radii = [0.0];
    let census = surface
        .penetration_census_batch_classified(&p0s, &p1s, &radii, &radii, 0, false)
        .unwrap();
    assert_eq!(
        census[0],
        geom_kernel::PenetrationRowCensus {
            crossings: 0,
            tests: 0,
            seg_mm: 0.0,
            max_ray_crossings: 0,
            max_ray_seg_mm: 0.0,
            near_parallel: 0
        }
    );
    let limits = surface
        .penetration_intervals_batch_census_limits_classified(&census, 0, 8, 0.12, false)
        .unwrap();
    assert_eq!(limits.max_total_work, 0);
    assert_eq!(limits.max_total_hits, 0);
    let rows = bounded_penetration(&surface, &p0s, &p1s, limits).unwrap();
    assert_eq!(rows.rows.len(), 1);
    assert!(rows.rows[0].is_empty());
    assert_eq!(rows.near_parallel, vec![0]);
}

/// FU-8g H2 — the charged-vs-executed receipt. `work` and `hits` are the same
/// two quantities the census counted, measured by the OTHER walk, so an
/// undrifted pair meters EQUAL to its charge rather than merely under it. The
/// bounded quantities (samples, intervals) land at or under their counted
/// budgets, and the metered entry point returns bit-identical geometry to the
/// plain one.
#[test]
fn penetration_meter_receipt_matches_the_counted_charge() {
    let surface = cubes_surface(&[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]]);
    let census = surface
        .penetration_census_batch_classified(
            &HETEROGENEOUS_P0,
            &HETEROGENEOUS_P1,
            &HETEROGENEOUS_RADII,
            &HETEROGENEOUS_RADII,
            2,
            true,
        )
        .unwrap();
    let counted_tests: usize = census.iter().map(|row| row.tests).sum();
    let counted_crossings: usize = census.iter().map(|row| row.crossings).sum();
    let limits = surface
        .penetration_intervals_batch_census_limits_classified(&census, 2, 8, 0.12, true)
        .unwrap();
    let (rows, meter) = surface
        .penetration_intervals_batch_bounded_metered_classified(
            &HETEROGENEOUS_P0,
            &HETEROGENEOUS_P1,
            &HETEROGENEOUS_RADII,
            &HETEROGENEOUS_RADII,
            2,
            0.12,
            8,
            0.05,
            true,
            limits,
        )
        .unwrap();
    assert_eq!(meter.rows, HETEROGENEOUS_P0.len());
    assert_eq!(meter.work, counted_tests);
    assert_eq!(meter.work, limits.max_total_work);
    assert_eq!(meter.hits, counted_crossings);
    assert_eq!(meter.hits, limits.max_total_hits);
    // The D3 severance is visible in the receipt, not just in the charge.
    assert!(meter.work > meter.hits);
    // Non-vacuity: this batch really does sample and really does emit.
    assert!(meter.samples > 0);
    assert!(meter.intervals > 0);
    assert!(meter.samples <= limits.max_total_samples);
    assert!(meter.intervals <= limits.max_total_intervals);
    assert_intervals_equal(&rows, &heterogeneous_penetration(&surface, limits).unwrap());
}

fn update_penetration_hash(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(1_099_511_628_211);
}

#[test]
#[expect(
    clippy::cast_sign_loss,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn bounded_penetration_thread_identity_child() {
    let surface = cubes_surface(&[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]]);
    let limits = surface
        .penetration_intervals_batch_worst_case_limits(HETEROGENEOUS_P0.len(), 2, 8, true)
        .unwrap();
    let rows = heterogeneous_penetration(&surface, limits).unwrap();
    let mut hash = 1_469_598_103_934_665_603u64;
    assert!(
        rows.near_parallel.iter().all(|&n| n == 0),
        "the thread-identity fixture must be fully decided: {:?}",
        rows.near_parallel
    );
    for (row_index, row) in rows.rows.into_iter().enumerate() {
        update_penetration_hash(&mut hash, 0x524f_575f_4245_4749 ^ row_index as u64);
        update_penetration_hash(&mut hash, row.len() as u64);
        for interval in row {
            update_penetration_hash(&mut hash, interval.s_a.to_bits());
            update_penetration_hash(&mut hash, interval.s_b.to_bits());
            update_penetration_hash(&mut hash, interval.max_depth.to_bits());
            update_penetration_hash(&mut hash, interval.entered as u64);
            update_penetration_hash(&mut hash, interval.exited as u64);
            update_penetration_hash(&mut hash, interval.ray as u64);
        }
        update_penetration_hash(&mut hash, 0x524f_575f_454e_4421 ^ row_index as u64);
    }
    let configured = geom_kernel::current_threads();
    let effective = geom_kernel::ordered_index_map(
        &[0u8],
        geom_kernel::OrderedMapLimits::new(1, std::mem::size_of::<u8>()),
        |_, value| *value,
    )
    .unwrap()
    .effective_threads();
    println!("PENETRATION_FINGERPRINT:{configured}:{effective}:{hash}");
}

#[test]
fn bounded_penetration_thread_identity_is_subprocess_invariant() {
    let executable = std::env::current_exe().unwrap();
    let mut fingerprints = Vec::new();
    for threads in ["1", "4", "16"] {
        let output = Command::new(&executable)
            .args([
                "--exact",
                "bounded_penetration_thread_identity_child",
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
            .find(|line| line.starts_with("PENETRATION_FINGERPRINT:"))
            .unwrap();
        let fields: Vec<&str> = line.split(':').collect();
        assert_eq!(fields[1], threads, "configured count mismatch: {line}");
        assert_eq!(fields[2], threads, "effective pool mismatch: {line}");
        fingerprints.push(fields[3].to_owned());
    }
    assert!(fingerprints.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn indexed_memory_estimate_is_checked_and_is_the_exact_grid_cap_boundary() {
    let vertices = [[0.0, 0.0, 0.0], [8.0, 0.0, 0.0], [0.0, 8.0, 0.0]];
    let faces = [[0, 1, 2]];
    let estimate = NativeSurface::estimate_index_memory_from_indexed(&vertices, &faces, 0.5)
        .expect("allocation-free estimate");
    assert!(estimate.peak_bytes() >= estimate.retained_bytes());
    assert!(estimate.max_axis_grid_bytes() > 0);
    assert!(estimate
        .axis_references()
        .iter()
        .all(|references| *references > 0));
    NativeSurface::from_indexed(
        &vertices,
        &faces,
        NativeSurfaceConfig {
            grid_pitch_mm: 0.5,
            max_grid_bytes: estimate.max_axis_grid_bytes(),
        },
    )
    .expect("the estimated cap accepts the indexed surface");
    let below = NativeSurface::from_indexed(
        &vertices,
        &faces,
        NativeSurfaceConfig {
            grid_pitch_mm: 0.5,
            max_grid_bytes: estimate.max_axis_grid_bytes() - 1,
        },
    );
    assert!(matches!(
        below,
        Err(SurfaceBuildError::GridBudget {
            projected_bytes,
            limit_bytes,
            ..
        }) if projected_bytes == estimate.max_axis_grid_bytes()
            && limit_bytes == estimate.max_axis_grid_bytes() - 1
    ));

    assert!(matches!(
        NativeSurface::estimate_index_memory_from_indexed(&vertices, &faces, 0.0),
        Err(SurfaceBuildError::InvalidGridPitch(0.0))
    ));
}

#[test]
fn indexed_estimator_mirrors_edge_reconstruction_at_floating_cell_boundaries() {
    // `a + (b - a)` lands just below the source bit-pattern at this scale.
    // The indexed store intentionally retains edges, so its allocation
    // preflight must bin the reconstructed vertex exactly like the builder.
    let vertices = vec![
        [1.0, 0.0, 0.0],
        [0.1, 0.0, 0.0],
        [0.0, 1.0, 1.0],
        [0.0, 0.0, 2.0],
    ];
    let faces = vec![[0, 1, 2], [1, 3, 2]];
    let estimate = NativeSurface::estimate_index_memory_from_indexed(&vertices, &faces, 0.1)
        .expect("finite indexed estimate");
    NativeSurface::from_indexed(
        &vertices,
        &faces,
        NativeSurfaceConfig {
            grid_pitch_mm: 0.1,
            max_grid_bytes: estimate.max_axis_grid_bytes(),
        },
    )
    .expect("exact reconstructed-coordinate cap");
    assert!(NativeSurface::from_indexed(
        &vertices,
        &faces,
        NativeSurfaceConfig {
            grid_pitch_mm: 0.1,
            max_grid_bytes: estimate.max_axis_grid_bytes() - 1,
        },
    )
    .is_err());
}

fn assert_batch_error<T>(
    result: Result<T, BatchInputError>,
    operation: &'static str,
    field: &'static str,
    expected: usize,
    actual: usize,
) {
    match result {
        Err(error) => assert_eq!(
            error,
            BatchInputError {
                operation,
                field,
                expected,
                actual,
            }
        ),
        Ok(_) => panic!("{operation} accepted mismatched {field}"),
    }
}

fn assert_penetration_input_error<T>(
    result: Result<T, PenetrationBatchError>,
    operation: &'static str,
    field: &'static str,
    expected: usize,
    actual: usize,
) {
    match result {
        Err(error) => assert_eq!(
            error,
            PenetrationBatchError::Input(BatchInputError {
                operation,
                field,
                expected,
                actual,
            })
        ),
        Ok(_) => panic!("{operation} accepted mismatched {field}"),
    }
}

fn subset_error(result: Result<NativeSurface, SurfaceSubsetError>) -> SurfaceSubsetError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("invalid subset selection unexpectedly succeeded"),
    }
}

#[test]
fn owned_and_borrowed_triangle_soups_expose_the_same_batch_api() {
    let borrowed = NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(&TRIANGLES, &NORMALS, &AREAS, BOUNDS),
        2.0,
        TEST_UNBOUNDED_GRID_BYTES,
    )
    .unwrap();
    let owned = NativeSurface::from_triangle_soup(
        TriangleSoup::owned(TRIANGLES.to_vec(), NORMALS.to_vec(), AREAS.to_vec(), BOUNDS),
        2.0,
        TEST_UNBOUNDED_GRID_BYTES,
    )
    .unwrap();

    let points = [[0.25, 0.25, 1.0], [2.0, 2.0, 0.0]];
    assert_eq!(borrowed.nfaces(), 1);
    assert_eq!(
        borrowed.query(&points, f64::INFINITY),
        owned.query(&points, f64::INFINITY)
    );
}

#[test]
fn source_triangle_soup_digest_binds_face_vertex_and_ieee_order() {
    let triangles = [
        [[-0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
        [[0.0, 0.0, 1.0], [2.0, 0.0, 1.0], [0.0, 2.0, 1.0]],
    ];
    let canonical = triangle_soup_digest(&triangles);

    let mut face_reordered = triangles;
    face_reordered.swap(0, 1);
    assert_ne!(canonical, triangle_soup_digest(&face_reordered));

    let mut vertex_permuted = triangles;
    vertex_permuted[0].rotate_left(1);
    assert_ne!(canonical, triangle_soup_digest(&vertex_permuted));

    let mut winding_reversed = triangles;
    winding_reversed[0].swap(1, 2);
    assert_ne!(canonical, triangle_soup_digest(&winding_reversed));

    let mut positive_zero = triangles;
    positive_zero[0][0][0] = 0.0;
    assert_ne!(canonical, triangle_soup_digest(&positive_zero));

    let vertices = [
        [-0.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [0.0, 2.0, 0.0],
        [0.0, 0.0, 1.0],
        [2.0, 0.0, 1.0],
        [0.0, 2.0, 1.0],
    ];
    let faces = [[0, 1, 2], [3, 4, 5]];
    assert_eq!(
        indexed_triangle_soup_digest(&vertices, &faces),
        Some(canonical)
    );
    let indexed = NativeSurface::from_indexed(
        &vertices,
        &faces,
        NativeSurfaceConfig {
            grid_pitch_mm: 2.0,
            max_grid_bytes: TEST_UNBOUNDED_GRID_BYTES,
        },
    )
    .unwrap();
    assert_eq!(indexed.source_triangle_soup_digest(), canonical);

    let soup = NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(
            &triangles,
            &[[0.0, 0.0, 1.0]; 2],
            &[2.0, 2.0],
            [[-0.0, 0.0, 0.0], [2.0, 2.0, 1.0]],
        ),
        2.0,
        TEST_UNBOUNDED_GRID_BYTES,
    )
    .unwrap();
    assert_eq!(soup.source_triangle_soup_digest(), canonical);
}

#[test]
fn subsets_validate_caller_order_and_return_global_face_ids() {
    let triangles = [
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        [[10.0, 0.0, 0.0], [11.0, 0.0, 0.0], [10.0, 1.0, 0.0]],
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
    ];
    let full = NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(
            &triangles,
            &[[0.0, 0.0, 1.0]; 3],
            &[0.5; 3],
            [[-3.0, -4.0, -5.0], [20.0, 21.0, 22.0]],
        ),
        2.0,
        usize::MAX,
    )
    .unwrap();

    assert!(matches!(
        full.subset(&[]),
        Err(SurfaceSubsetError::EmptySelection)
    ));
    assert_eq!(
        subset_error(full.subset(&[2, 3, 2])),
        SurfaceSubsetError::FaceIdOutOfBounds {
            position: 1,
            face_id: 3,
            global_face_count: 3,
        }
    );
    assert_eq!(
        subset_error(full.subset(&[2, 0, 2])),
        SurfaceSubsetError::DuplicateFaceId { face_id: 2 }
    );

    let identity = full.subset(&[2, 1, 0]).unwrap();
    assert_eq!(
        identity.source_triangle_soup_digest(),
        full.source_triangle_soup_digest()
    );
    assert_eq!(identity.bounds(), full.bounds());
    assert_eq!(
        identity.query(&[[0.25, 0.25, 1.0]], f64::INFINITY),
        full.query(&[[0.25, 0.25, 1.0]], f64::INFINITY),
        "identity subsets preserve caller-owned soup bounds and query behavior"
    );

    let tied = full.subset(&[2, 0]).unwrap();
    let tied_reordered = full.subset(&[0, 2]).unwrap();
    assert_eq!(tied.face_ids(), &[0, 2]);
    assert_ne!(
        tied.source_triangle_soup_digest(),
        full.source_triangle_soup_digest()
    );
    assert_eq!(tied.face_ids(), tied_reordered.face_ids());
    assert_eq!(tied.nfaces(), 2);
    assert_eq!(tied.geometry().nfaces(), 3);
    assert_eq!(tied.bounds(), [[0.0, 0.0, 0.0], [1.0, 1.0, 0.0]]);
    assert_eq!(tied.query(&[[0.25, 0.25, 1.0]], f64::INFINITY).1, [0]);
    assert_eq!(
        tied.query(&[[0.25, 0.25, 1.0]], 0.5).1,
        [3],
        "no-hit sentinel remains the immutable global face count"
    );
    assert_eq!(
        tied.cast_rays_classified(&[[0.25, 0.25, 1.0]], &[[0.0, 0.0, -1.0]], 2.0)
            .unwrap()
            .face_all,
        [0],
        "coincident-hit tie-break returns the lowest global face ID"
    );
    assert_eq!(
        tied.query(&[[0.25, 0.25, 1.0], [10.25, 0.25, 1.0]], f64::INFINITY),
        tied_reordered.query(&[[0.25, 0.25, 1.0], [10.25, 0.25, 1.0]], f64::INFINITY),
        "caller order cannot change canonical subset query bits"
    );
    assert_eq!(
        tied.cast_rays_classified(&[[0.25, 0.25, 1.0]], &[[0.0, 0.0, -1.0]], 2.0),
        tied_reordered.cast_rays_classified(&[[0.25, 0.25, 1.0]], &[[0.0, 0.0, -1.0]], 2.0),
        "caller order cannot change canonical subset ray ties"
    );
    assert_eq!(
        full.subset(&[2])
            .unwrap()
            .query(&[[0.25, 0.25, 1.0]], f64::INFINITY)
            .1,
        [2]
    );

    let nested = tied.subset(&[2]).unwrap();
    assert_eq!(nested.face_ids(), &[2]);
    assert_eq!(
        subset_error(nested.subset(&[1])),
        SurfaceSubsetError::FaceIdNotSelected {
            position: 0,
            face_id: 1,
        }
    );
    assert_eq!(
        subset_error(nested.subset(&[2, 1, 2])),
        SurfaceSubsetError::FaceIdNotSelected {
            position: 1,
            face_id: 1,
        },
        "caller-order parent membership failure precedes a later duplicate"
    );
}

#[test]
#[expect(
    clippy::cast_possible_truncation,
    reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn identity_subset_matches_every_native_query_family() {
    let full = cube_surface();
    let subset = full
        .subset(&(0..full.nfaces() as u32).rev().collect::<Vec<_>>())
        .unwrap();
    let points = [[0.5, 0.5, 0.5], [0.25, 0.25, 1.5], [2.0, 2.0, 2.0]];
    let origins = [[0.5, 0.5, 2.0], [-1.0, 0.5, 0.5]];
    let dirs = [[0.0, 0.0, -2.0], [2.0, 0.0, 0.0]];
    let p0s = [[-1.0, 0.5, 0.5], [0.5, 0.5, 2.0]];
    let p1s = [[2.0, 0.5, 0.5], [0.5, 0.5, -1.0]];
    let radii = [0.1, 0.2];
    let insets = [0.0, 0.05];

    assert_eq!(subset.face_ids(), &(0..12).collect::<Vec<_>>());
    assert_eq!(
        full.query(&points, f64::INFINITY),
        subset.query(&points, f64::INFINITY)
    );
    assert_eq!(full.project(&points), subset.project(&points));
    assert_eq!(full.inside(&points), subset.inside(&points));
    assert_eq!(
        full.cast_rays_classified(&origins, &dirs, 1.0).unwrap(),
        subset.cast_rays_classified(&origins, &dirs, 1.0).unwrap()
    );
    assert_eq!(
        full.segment_clearance_classified(&p0s, &p1s, &radii, &radii, 0.05, &insets, &insets)
            .unwrap(),
        subset
            .segment_clearance_classified(&p0s, &p1s, &radii, &radii, 0.05, &insets, &insets)
            .unwrap()
    );
    let full_intervals = full
        .penetration_intervals_batch_bounded_classified(
            &p0s,
            &p1s,
            &radii,
            &radii,
            4,
            0.12,
            80,
            0.05,
            true,
            generous_penetration_limits(),
        )
        .unwrap();
    let subset_intervals = subset
        .penetration_intervals_batch_bounded_classified(
            &p0s,
            &p1s,
            &radii,
            &radii,
            4,
            0.12,
            80,
            0.05,
            true,
            generous_penetration_limits(),
        )
        .unwrap();
    assert_intervals_equal(&full_intervals, &subset_intervals);
    assert_eq!(
        full.sphere_breach_classified(&points[..2], &radii, 0.05, 25.0, 8)
            .unwrap(),
        subset
            .sphere_breach_classified(&points[..2], &radii, 0.05, 25.0, 8)
            .unwrap()
    );
    let full_seat = seat_exemption_pair(&full);
    let subset_seat = seat_exemption_pair(&subset);
    assert_eq!(
        full_seat.band,
        vec![2],
        "the band names the top face's first triangle by its GLOBAL ID"
    );
    assert_eq!(full_seat, subset_seat);
    assert_seat_exemption_answers(&full_seat, "identity subset");
    assert_eq!(full.exterior_face_mask(), subset.exterior_face_mask());
}

#[test]
fn sparse_nested_subset_matches_every_native_query_family_and_owns_its_cache() {
    let full = cubes_surface(&[[-20.0, 0.0, 0.0], [0.0, 0.0, 0.0]]);
    let mut parent_ids = vec![0];
    parent_ids.extend(12..24);
    let parent = full.subset(&parent_ids).unwrap();
    let parent_mask = parent.exterior_face_mask().to_vec();
    let child = parent.subset(&(12..24).collect::<Vec<_>>()).unwrap();

    let points = [[0.5, 0.5, 0.5], [0.25, 0.25, 1.5], [2.0, 2.0, 2.0]];
    let origins = [[0.5, 0.5, 2.0], [-1.0, 0.5, 0.5]];
    let dirs = [[0.0, 0.0, -2.0], [2.0, 0.0, 0.0]];
    let p0s = [[-1.0, 0.5, 0.5], [0.5, 0.5, 2.0]];
    let p1s = [[2.0, 0.5, 0.5], [0.5, 0.5, -1.0]];
    let radii = [0.1, 0.2];
    let insets = [0.0, 0.05];

    assert_eq!(
        parent.face_ids(),
        &[0, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23]
    );
    assert_eq!(child.face_ids(), &(12..24).collect::<Vec<_>>());
    assert_eq!(child.nfaces(), 12);
    assert_eq!(child.geometry().nfaces(), 24);
    assert_eq!(
        parent.query(&points, f64::INFINITY),
        child.query(&points, f64::INFINITY)
    );
    let parent_project = parent.project(&points);
    let child_project = child.project(&points);
    assert_eq!(parent_project, child_project);
    assert!(child_project.2.iter().all(|&face_id| face_id >= 12));
    assert_eq!(parent.inside(&points), child.inside(&points));
    assert_eq!(
        parent.cast_rays_classified(&origins, &dirs, 1.0).unwrap(),
        child.cast_rays_classified(&origins, &dirs, 1.0).unwrap()
    );
    assert_eq!(
        parent
            .segment_clearance_classified(&p0s, &p1s, &radii, &radii, 0.05, &insets, &insets)
            .unwrap(),
        child
            .segment_clearance_classified(&p0s, &p1s, &radii, &radii, 0.05, &insets, &insets)
            .unwrap()
    );
    let parent_intervals = parent
        .penetration_intervals_batch_bounded_classified(
            &p0s,
            &p1s,
            &radii,
            &radii,
            4,
            0.12,
            80,
            0.05,
            true,
            generous_penetration_limits(),
        )
        .unwrap();
    let child_intervals = child
        .penetration_intervals_batch_bounded_classified(
            &p0s,
            &p1s,
            &radii,
            &radii,
            4,
            0.12,
            80,
            0.05,
            true,
            generous_penetration_limits(),
        )
        .unwrap();
    assert_intervals_equal(&parent_intervals, &child_intervals);
    assert_eq!(
        parent
            .sphere_breach_classified(&points[..2], &radii, 0.05, 25.0, 8)
            .unwrap(),
        child
            .sphere_breach_classified(&points[..2], &radii, 0.05, 25.0, 8)
            .unwrap()
    );
    // The seat-exemption pair, on views that are NOT the identity map. Both
    // halves walk the BVH, whose leaf order carries GLOBAL face IDs (bvh.rs
    // "Leaf order carries global face IDs even for a narrowed mesh view"), so
    // both must report and match those IDs directly. Reading `face_ids()` --
    // the slot-to-global map, whose length is the SUBSET size -- with a global
    // ID is out of bounds here, and was silently the identity on every full
    // mesh the suite tested before.
    let full_seat = seat_exemption_pair(&full);
    let parent_seat = seat_exemption_pair(&parent);
    let child_seat = seat_exemption_pair(&child);
    assert_eq!(
        full_seat.band,
        vec![14],
        "the band names the second cube's first top triangle by its GLOBAL ID"
    );
    assert_eq!(
        parent_seat, full_seat,
        "a 13-face view of a 24-face mesh answers the seat pair exactly as the whole mesh does"
    );
    assert_eq!(
        child_seat, full_seat,
        "a 12-face view of a 24-face mesh answers the seat pair exactly as the whole mesh does"
    );
    assert_seat_exemption_answers(&full_seat, "whole mesh");
    assert_seat_exemption_answers(&parent_seat, "13-face parent view");
    assert_seat_exemption_answers(&child_seat, "12-face child view");

    assert_eq!(child.exterior_face_mask(), &parent_mask[1..]);
    assert_eq!(child.exterior_face_mask().len(), child.nfaces());
    assert_eq!(
        child
            .face_ids()
            .iter()
            .copied()
            .zip(child.exterior_face_mask().iter().copied())
            .collect::<Vec<_>>(),
        (12..24)
            .zip(parent_mask[1..].iter().copied())
            .collect::<Vec<_>>(),
        "sparse mask entries pair one-to-one with ascending global face IDs"
    );
}

/// The seat-exemption pair on a view chosen so that reading `face_ids()` with a
/// global face ID stays IN BOUNDS and answers with the wrong face.
///
/// The nested-subset test above catches the mistake as an out-of-bounds panic,
/// which is the loud half. This is the quiet half: 15 face slots, and the band's
/// winning face -- global 14 -- lands on slot 14, which holds global 16. A
/// slot-lookup answers `[16]`, a face this view really does own, sorted,
/// deduplicated, inside every documented postcondition, and simply not the face
/// the walk found. Downstream that face ID becomes a clearance exemption, so the
/// wrong answer forgives a face nobody asked to forgive.
#[test]
fn a_narrowed_view_reports_the_face_the_walk_found_not_the_slot_it_sat_in() {
    let full = cubes_surface(&[[-20.0, 0.0, 0.0], [0.0, 0.0, 0.0]]);
    let mut view_ids: Vec<u32> = (0..12).collect();
    view_ids.extend([14, 15, 16]);
    let view = full.subset(&view_ids).unwrap();
    assert_eq!(view.nfaces(), 15);
    assert_eq!(view.geometry().nfaces(), 24);
    assert_eq!(
        view.face_ids()[14],
        16,
        "slot 14 of this view holds global face 16, so a slot lookup on global 14 \
         is in bounds and wrong -- which is what gives this test its teeth"
    );

    let p0s = [SEAT_PROBE_P0];
    let p1s = [SEAT_PROBE_P1];
    let zero = [0.0];
    let band = view
        .faces_near_segments_batch_classified(
            &p0s,
            &p1s,
            &[SEAT_PROBE_BAND_MM],
            generous_near_face_limits(),
        )
        .unwrap();
    assert_eq!(
        band.rows,
        vec![vec![14]],
        "the band holds the top triangle the walk actually measured at 0.1 mm"
    );

    let exempt_rows: Vec<&[u32]> = band.rows.iter().map(Vec::as_slice).collect();
    let open_rows = view
        .segment_clearance_classified(&p0s, &p1s, &zero, &zero, 0.0, &zero, &zero)
        .unwrap();
    let exempt_result = view
        .segment_clearance_excluding_classified(
            &p0s,
            &p1s,
            &zero,
            &zero,
            0.0,
            &zero,
            &zero,
            &exempt_rows,
        )
        .unwrap();
    let open_clearance = open_rows[0]
        .clearance_mm()
        .expect("the seat probe view is fully decided; a one-sided row would not be gradable");
    let exempt_clearance = exempt_result[0]
        .clearance_mm()
        .expect("the seat probe view is fully decided; a one-sided row would not be gradable");
    assert!(
        (open_clearance - SEAT_PROBE_OPEN_CLEARANCE_MM).abs() < 1e-12,
        "unexempt clearance {open_clearance} is not the 0.1 mm gap to the top triangle"
    );
    // This view does not own the `y = 0` side triangle the fuller views fall back
    // to, so exempting face 14 leaves the top face's OTHER triangle nearest, at
    // sqrt(0.1^2 + 0.1^2 + 0.1^2) mm.
    assert!(
        (exempt_clearance - 0.03_f64.sqrt()).abs() < 1e-12,
        "exempting face 14 left {exempt_clearance}, not the sqrt(0.03) mm gap to \
         the top face's other triangle"
    );
}

#[test]
fn malformed_and_nonfinite_inputs_return_named_errors() {
    let empty = NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(&[], &[], &[], BOUNDS),
        2.0,
        TEST_UNBOUNDED_GRID_BYTES,
    );
    assert!(matches!(empty, Err(SurfaceBuildError::EmptyTriangleSoup)));

    let mismatch = NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(&TRIANGLES, &[], &AREAS, BOUNDS),
        2.0,
        TEST_UNBOUNDED_GRID_BYTES,
    );
    assert!(matches!(
        mismatch,
        Err(SurfaceBuildError::LengthMismatch {
            field: "normals",
            expected: 1,
            actual: 0,
        })
    ));

    let mut nonfinite = TRIANGLES;
    nonfinite[0][1][2] = f64::NAN;
    let error = NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(&nonfinite, &NORMALS, &AREAS, BOUNDS),
        2.0,
        TEST_UNBOUNDED_GRID_BYTES,
    );
    assert!(matches!(
        error,
        Err(SurfaceBuildError::NonFinite {
            field: "triangles",
            index: 5,
        })
    ));

    let malformed_bounds = NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(
            &TRIANGLES,
            &NORMALS,
            &AREAS,
            [[2.0, 0.0, 0.0], [1.0, 1.0, 0.0]],
        ),
        2.0,
        TEST_UNBOUNDED_GRID_BYTES,
    );
    assert!(matches!(
        malformed_bounds,
        Err(SurfaceBuildError::MalformedBounds { axis: 0, .. })
    ));

    let invalid_pitch = NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(&TRIANGLES, &NORMALS, &AREAS, BOUNDS),
        f64::INFINITY,
        TEST_UNBOUNDED_GRID_BYTES,
    );
    assert!(matches!(
        invalid_pitch,
        Err(SurfaceBuildError::InvalidGridPitch(value)) if value.is_infinite()
    ));
}

#[test]
fn every_multi_slice_batch_rejects_each_mismatched_companion() {
    let surface = surface();
    let points = [[0.25, 0.25, 1.0]];
    let scalars = [0.1];

    assert_batch_error(
        surface.cast_rays_classified(&points, &[], 1.0),
        "cast_rays",
        "dirs",
        1,
        0,
    );

    assert_batch_error(
        surface.segment_clearance_classified(
            &points,
            &[],
            &scalars,
            &scalars,
            0.05,
            &scalars,
            &scalars,
        ),
        "segment_clearance",
        "p1s",
        1,
        0,
    );
    assert_batch_error(
        surface.segment_clearance_classified(
            &points,
            &points,
            &[],
            &scalars,
            0.05,
            &scalars,
            &scalars,
        ),
        "segment_clearance",
        "r0s",
        1,
        0,
    );
    assert_batch_error(
        surface.segment_clearance_classified(
            &points,
            &points,
            &scalars,
            &[],
            0.05,
            &scalars,
            &scalars,
        ),
        "segment_clearance",
        "r1s",
        1,
        0,
    );
    assert_batch_error(
        surface.segment_clearance_classified(
            &points,
            &points,
            &scalars,
            &scalars,
            0.05,
            &[],
            &scalars,
        ),
        "segment_clearance",
        "inset0s",
        1,
        0,
    );
    assert_batch_error(
        surface.segment_clearance_classified(
            &points,
            &points,
            &scalars,
            &scalars,
            0.05,
            &scalars,
            &[],
        ),
        "segment_clearance",
        "inset1s",
        1,
        0,
    );

    assert_penetration_input_error(
        surface.penetration_intervals_batch_bounded_classified(
            &points,
            &[],
            &scalars,
            &scalars,
            8,
            0.12,
            80,
            0.05,
            false,
            generous_penetration_limits(),
        ),
        "penetration_intervals_batch",
        "p1s",
        1,
        0,
    );
    assert_penetration_input_error(
        surface.penetration_intervals_batch_bounded_classified(
            &points,
            &points,
            &[],
            &scalars,
            8,
            0.12,
            80,
            0.05,
            false,
            generous_penetration_limits(),
        ),
        "penetration_intervals_batch",
        "r0s",
        1,
        0,
    );
    assert_penetration_input_error(
        surface.penetration_intervals_batch_bounded_classified(
            &points,
            &points,
            &scalars,
            &[],
            8,
            0.12,
            80,
            0.05,
            false,
            generous_penetration_limits(),
        ),
        "penetration_intervals_batch",
        "r1s",
        1,
        0,
    );

    assert_batch_error(
        surface.sphere_breach_classified(&points, &[], 0.05, 25.0, 8),
        "sphere_breach",
        "radii",
        1,
        0,
    );
}

#[test]
fn indexed_geometry_and_batched_results_match_equivalent_soup() {
    let vertices = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ];
    let faces = [[0, 1, 2], [1, 3, 2], [4, 2, 1]];
    let indexed = NativeSurface::from_indexed(
        &vertices,
        &faces,
        NativeSurfaceConfig {
            grid_pitch_mm: 2.0,
            max_grid_bytes: TEST_UNBOUNDED_GRID_BYTES,
        },
    )
    .unwrap();
    let root_three = 3.0_f64.sqrt();
    let soup = NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(
            &[
                [vertices[0], vertices[1], vertices[2]],
                [vertices[1], vertices[3], vertices[2]],
                [vertices[4], vertices[2], vertices[1]],
            ],
            &[
                [0.0, 0.0, 1.0],
                [0.0, -0.0, 1.0],
                [-1.0 / root_three, -1.0 / root_three, -1.0 / root_three],
            ],
            &[0.5, 0.5, root_three / 2.0],
            [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]],
        ),
        2.0,
        TEST_UNBOUNDED_GRID_BYTES,
    )
    .unwrap();
    assert_eq!(indexed.geometry().nfaces(), faces.len());
    assert_eq!(soup.geometry().nfaces(), faces.len());
    for geometry in [indexed.geometry(), soup.geometry()] {
        assert_eq!(geometry.normals().len(), faces.len());
        assert_eq!(geometry.areas().len(), faces.len());
        assert_eq!(geometry.centroids().len(), faces.len());
        assert_eq!(geometry.face_bounds().len(), faces.len());
    }
    for (left, right) in indexed
        .geometry()
        .normals()
        .iter()
        .zip(soup.geometry().normals())
    {
        for axis in 0..3 {
            assert_eq!(left[axis].to_bits(), right[axis].to_bits());
        }
    }
    let indexed_bounds = indexed.geometry().bounds();
    let soup_bounds = soup.geometry().bounds();
    for endpoint in 0..2 {
        for axis in 0..3 {
            assert_eq!(
                indexed_bounds[endpoint][axis].to_bits(),
                soup_bounds[endpoint][axis].to_bits()
            );
        }
    }
    for (&left, &right) in indexed
        .geometry()
        .areas()
        .iter()
        .zip(soup.geometry().areas())
    {
        assert_eq!(left.to_bits(), right.to_bits());
    }
    for (left, right) in indexed
        .geometry()
        .centroids()
        .iter()
        .zip(soup.geometry().centroids())
    {
        for axis in 0..3 {
            assert_eq!(left[axis].to_bits(), right[axis].to_bits());
        }
    }
    for (left, right) in indexed
        .geometry()
        .face_bounds()
        .iter()
        .zip(soup.geometry().face_bounds())
    {
        for endpoint in 0..2 {
            for axis in 0..3 {
                assert_eq!(
                    left[endpoint][axis].to_bits(),
                    right[endpoint][axis].to_bits()
                );
            }
        }
    }

    let points = [[0.5, 0.5, 0.0], [0.25, 0.25, 1.0], [2.0, 2.0, 0.0]];
    let origins = [[0.5, 0.5, 2.0], [0.25, 0.25, 1.0], [2.0, 2.0, 1.0]];
    let dirs = [[0.0, 0.0, -1.0], [0.0, 0.0, -1.0], [0.0, 0.0, -1.0]];
    let (id, ii) = indexed.query(&points, f64::INFINITY);
    let (sd, si) = soup.query(&points, f64::INFINITY);
    assert_eq!(ii, si);
    for (&left, &right) in id.iter().zip(&sd) {
        assert_eq!(left.to_bits(), right.to_bits());
    }
    let (id, ip, ii, inorm) = indexed.project(&points);
    let (sd, sp, si, snorm) = soup.project(&points);
    assert_eq!(ii, si);
    for ((&left, &right), (lp, rp)) in id.iter().zip(&sd).zip(ip.iter().zip(&sp)) {
        assert_eq!(left.to_bits(), right.to_bits());
        for axis in 0..3 {
            assert_eq!(lp[axis].to_bits(), rp[axis].to_bits());
        }
    }
    for (ln, rn) in inorm.iter().zip(&snorm) {
        for axis in 0..3 {
            assert_eq!(ln[axis].to_bits(), rn[axis].to_bits());
        }
    }
    assert_eq!(indexed.inside(&points), soup.inside(&points));
    let indexed_rays = indexed.cast_rays_classified(&origins, &dirs, 2.0).unwrap();
    let soup_rays = soup.cast_rays_classified(&origins, &dirs, 2.0).unwrap();
    assert_eq!(indexed_rays.offsets, soup_rays.offsets);
    assert_eq!(indexed_rays.face_all, soup_rays.face_all);
    assert_eq!(
        indexed_rays.near_parallel, soup_rays.near_parallel,
        "two constructions of the same geometry must agree on which rays were \
         undecidable, not merely on the crossings they kept"
    );
    for (&left, &right) in indexed_rays.t_all.iter().zip(&soup_rays.t_all) {
        assert_eq!(left.to_bits(), right.to_bits());
    }
}

#[test]
fn indexed_constructor_reports_typed_input_and_derivation_failures() {
    let config = NativeSurfaceConfig {
        grid_pitch_mm: 2.0,
        max_grid_bytes: TEST_UNBOUNDED_GRID_BYTES,
    };
    assert!(matches!(
        NativeSurface::from_indexed(&[], &[[0, 0, 0]], config),
        Err(SurfaceBuildError::EmptyVertices)
    ));
    for (value, axis) in [(f64::NAN, 0), (f64::INFINITY, 1), (f64::NEG_INFINITY, 2)] {
        let mut vertices = [[0.0; 3]; 2];
        vertices[1][axis] = value;
        assert!(
            matches!(NativeSurface::from_indexed(&vertices, &[[0, 0, 0]], config), Err(SurfaceBuildError::NonFiniteVertex { vertex: 1, axis: got }) if got == axis)
        );
    }
    for corner in 0..3 {
        let mut face = [0, 0, 0];
        face[corner] = 2;
        assert!(
            matches!(NativeSurface::from_indexed(&[[0.0; 3]], &[face, [0, 0, 3]], config), Err(SurfaceBuildError::FaceIndexOutOfBounds { face: 0, corner: got, .. }) if got == corner)
        );
    }
    assert!(matches!(
        NativeSurface::from_indexed(&[[0.0; 3]], &[[0, 0, 0], [0, 0, 1]], config),
        Err(SurfaceBuildError::FaceIndexOutOfBounds {
            face: 1,
            corner: 2,
            ..
        })
    ));
    for value in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            matches!(NativeSurface::from_indexed(&[[0.0; 3]], &[[0,0,0]], NativeSurfaceConfig { grid_pitch_mm: value, max_grid_bytes: TEST_UNBOUNDED_GRID_BYTES }), Err(SurfaceBuildError::InvalidGridPitch(got)) if got.to_bits() == value.to_bits())
        );
    }
    let budget_indexed = NativeSurface::from_indexed(
        &TRIANGLES[0],
        &[[0, 1, 2]],
        NativeSurfaceConfig {
            grid_pitch_mm: 2.0,
            max_grid_bytes: 0,
        },
    );
    let budget_soup = NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(&TRIANGLES, &NORMALS, &AREAS, BOUNDS),
        2.0,
        0,
    );
    assert!(matches!(
        budget_indexed,
        Err(SurfaceBuildError::GridBudget { limit_bytes: 0, .. })
    ));
    assert!(matches!(
        budget_soup,
        Err(SurfaceBuildError::GridBudget { limit_bytes: 0, .. })
    ));
    assert!(matches!(
        NativeSurface::from_indexed(&[[0.0; 3]], &[], config),
        Err(SurfaceBuildError::EmptyFaces)
    ));
    assert!(matches!(
        NativeSurface::from_indexed(&[[0.0, f64::NAN, 0.0]], &[[0, 0, 0]], config),
        Err(SurfaceBuildError::NonFiniteVertex { vertex: 0, axis: 1 })
    ));
    assert!(matches!(
        NativeSurface::from_indexed(&[[0.0; 3]], &[[0, 1, 0]], config),
        Err(SurfaceBuildError::FaceIndexOutOfBounds {
            face: 0,
            corner: 1,
            index: 1,
            vertex_count: 1
        })
    ));
    assert!(matches!(
        NativeSurface::from_indexed(
            &[[0.0; 3]],
            &[[0, 0, 0]],
            NativeSurfaceConfig {
                grid_pitch_mm: 0.0,
                max_grid_bytes: TEST_UNBOUNDED_GRID_BYTES
            }
        ),
        Err(SurfaceBuildError::InvalidGridPitch(0.0))
    ));
    assert!(matches!(
        NativeSurface::from_indexed(
            &[[f64::MAX, 0.0, 0.0], [-f64::MAX, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[[0, 1, 2]],
            config
        ),
        Err(SurfaceBuildError::IndexedDerivedArithmetic {
            face: 0,
            operation: "edge subtraction"
        })
    ));
}

#[test]
fn indexed_repeated_index_face_is_refused_rather_than_given_a_zero_normal() {
    assert_eq!(
        NativeSurface::from_indexed(
            &[[0.0, 0.0, 0.0]],
            &[[0, 0, 0]],
            NativeSurfaceConfig {
                grid_pitch_mm: 2.0,
                max_grid_bytes: TEST_UNBOUNDED_GRID_BYTES,
            },
        )
        .err(),
        Some(SurfaceBuildError::DegenerateFace {
            face: 0,
            cause: geom_kernel::DegenerateFaceCause::RepeatedVertexIndex {
                corners: (0, 1),
                index: 0
            },
        })
    );
}
