use geom_kernel::{
    BatchInputError, Interval, NativeSurface, NativeSurfaceConfig, PlaneIntersectionKind,
    PlaneSliceError, PlaneSliceLimits, SurfaceBuildError, SurfaceSubsetError, TriangleSoup,
};
use std::process::Command;

type SliceFingerprint = (u32, u8, Option<([u64; 3], [u64; 3])>);

const TRIANGLES: [[[f64; 3]; 3]; 1] = [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]];
const NORMALS: [[f64; 3]; 1] = [[0.0, 0.0, 1.0]];
const AREAS: [f64; 1] = [0.5];
const BOUNDS: [[f64; 3]; 2] = [[0.0, 0.0, 0.0], [1.0, 1.0, 0.0]];

fn surface() -> NativeSurface {
    NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(&TRIANGLES, &NORMALS, &AREAS, BOUNDS),
        2.0,
        usize::MAX,
    )
    .unwrap()
}

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

fn slice_limits(surface: &NativeSurface, planes: usize) -> PlaneSliceLimits {
    let work = surface.nfaces() * planes;
    PlaneSliceLimits {
        max_planes: planes,
        max_triangle_plane_tests: work,
        max_intersections: work,
        max_output_bytes: (planes + 1) * std::mem::size_of::<usize>()
            + work * std::mem::size_of::<geom_kernel::PlaneIntersection>(),
    }
}

#[test]
fn slice_z_batch_box_has_ordered_face_local_segments_and_offsets() {
    let surface = cube_surface();
    let batch = surface
        .slice_z_batch(&[0.25, 0.75], slice_limits(&surface, 2))
        .unwrap();
    assert_eq!(batch.plane_offsets, vec![0, 8, 16]);
    for plane in 0..2 {
        let z: f64 = [0.25, 0.75][plane];
        let events = batch.plane(plane);
        assert_eq!(
            events.iter().map(|event| event.face_id).collect::<Vec<_>>(),
            (4..12).collect::<Vec<_>>()
        );
        assert!(events
            .iter()
            .all(|event| event.kind == PlaneIntersectionKind::Transverse));
        for event in events {
            let [a, b] = event.segment.expect("side face crosses plane");
            assert_eq!(a[2].to_bits(), z.to_bits());
            assert_eq!(b[2].to_bits(), z.to_bits());
        }
    }
}

#[test]
fn slice_z_batch_tetrahedron_has_three_exact_midplane_segments() {
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
    let batch = surface
        .slice_z_batch(&[1.0], slice_limits(&surface, 1))
        .unwrap();
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
        .all(|event| event.kind == PlaneIntersectionKind::Transverse));
    assert_eq!(
        batch.plane(0)[0].segment.unwrap(),
        [[0.0, 0.0, 1.0], [1.0, 0.0, 1.0]]
    );
}

#[test]
fn slice_z_batch_classifies_coplanar_edge_vertex_and_tangent_without_hidden_dedup() {
    let triangles = [
        [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 1.0]],
        [[0.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 1.0, -1.0]],
        [[0.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 1.0, 1.0]],
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
    let batch = surface
        .slice_z_batch(&[0.0], slice_limits(&surface, 1))
        .unwrap();
    assert_eq!(batch.plane_offsets, vec![0, 4]);
    let events = batch.plane(0);
    assert_eq!(events[0].kind, PlaneIntersectionKind::Coplanar);
    assert_eq!(events[0].segment, None);
    assert_eq!(events[1].kind, PlaneIntersectionKind::Edge);
    assert_eq!(
        events[1].segment.unwrap(),
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]]
    );
    assert_eq!(events[2].kind, PlaneIntersectionKind::Vertex);
    assert_eq!(events[2].segment.unwrap()[0], [0.0, 0.0, 0.0]);
    assert_eq!(events[3].kind, PlaneIntersectionKind::Vertex);
    assert_eq!(events[3].segment, None);
}

#[test]
fn slice_z_batch_limits_are_checked_at_exact_cap_and_one_below() {
    let surface = cube_surface();
    let planes = [0.5];
    let limits = slice_limits(&surface, 1);
    surface
        .slice_z_batch(&planes, limits)
        .expect("exact worst-case cap accepted");
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
fn slicing_thread_identity_child() {
    let surface = cube_surface();
    let batch = surface
        .slice_z_batch(&[0.25, 0.5, 0.75], slice_limits(&surface, 3))
        .unwrap();
    let fingerprint: Vec<SliceFingerprint> = batch
        .intersections
        .iter()
        .map(|event| {
            (
                event.face_id,
                event.kind as u8,
                event
                    .segment
                    .map(|[a, b]| (a.map(f64::to_bits), b.map(f64::to_bits))),
            )
        })
        .collect();
    println!("SLICE_FINGERPRINT:{fingerprint:?}");
}

#[test]
fn slicing_thread_identity_is_subprocess_invariant() {
    let executable = std::env::current_exe().unwrap();
    let mut outputs = Vec::new();
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
        outputs.push(
            stdout
                .lines()
                .find(|line| line.starts_with("SLICE_FINGERPRINT:"))
                .unwrap()
                .to_owned(),
        );
    }
    assert!(
        outputs.windows(2).all(|pair| pair[0] == pair[1]),
        "{outputs:?}"
    );
}

fn assert_intervals_equal(left: &[Vec<Interval>], right: &[Vec<Interval>]) {
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
    assert!(matches!(below, Err(SurfaceBuildError::Kernel(_))));

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
        usize::MAX,
    )
    .unwrap();
    let owned = NativeSurface::from_triangle_soup(
        TriangleSoup::owned(TRIANGLES.to_vec(), NORMALS.to_vec(), AREAS.to_vec(), BOUNDS),
        2.0,
        usize::MAX,
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
    assert_eq!(identity.bounds(), full.bounds());
    assert_eq!(
        identity.query(&[[0.25, 0.25, 1.0]], f64::INFINITY),
        full.query(&[[0.25, 0.25, 1.0]], f64::INFINITY),
        "identity subsets preserve caller-owned soup bounds and query behavior"
    );

    let tied = full.subset(&[2, 0]).unwrap();
    let tied_reordered = full.subset(&[0, 2]).unwrap();
    assert_eq!(tied.face_ids(), &[0, 2]);
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
        tied.cast_rays(&[[0.25, 0.25, 1.0]], &[[0.0, 0.0, -1.0]], 2.0)
            .unwrap()
            .2,
        [0],
        "coincident-hit tie-break returns the lowest global face ID"
    );
    assert_eq!(
        tied.query(&[[0.25, 0.25, 1.0], [10.25, 0.25, 1.0]], f64::INFINITY),
        tied_reordered.query(&[[0.25, 0.25, 1.0], [10.25, 0.25, 1.0]], f64::INFINITY),
        "caller order cannot change canonical subset query bits"
    );
    assert_eq!(
        tied.cast_rays(&[[0.25, 0.25, 1.0]], &[[0.0, 0.0, -1.0]], 2.0),
        tied_reordered.cast_rays(&[[0.25, 0.25, 1.0]], &[[0.0, 0.0, -1.0]], 2.0),
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
        full.cast_rays(&origins, &dirs, 1.0).unwrap(),
        subset.cast_rays(&origins, &dirs, 1.0).unwrap()
    );
    assert_eq!(
        full.segment_clearance(&p0s, &p1s, &radii, &radii, 0.05, &insets, &insets)
            .unwrap(),
        subset
            .segment_clearance(&p0s, &p1s, &radii, &radii, 0.05, &insets, &insets)
            .unwrap()
    );
    let full_intervals = full
        .penetration_intervals_batch(&p0s, &p1s, &radii, &radii, 4, 0.12, 80, 0.05, true)
        .unwrap();
    let subset_intervals = subset
        .penetration_intervals_batch(&p0s, &p1s, &radii, &radii, 4, 0.12, 80, 0.05, true)
        .unwrap();
    assert_intervals_equal(&full_intervals, &subset_intervals);
    assert_eq!(
        full.sphere_breach(&points[..2], &radii, 0.05, 25.0, 8)
            .unwrap(),
        subset
            .sphere_breach(&points[..2], &radii, 0.05, 25.0, 8)
            .unwrap()
    );
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
        parent.cast_rays(&origins, &dirs, 1.0).unwrap(),
        child.cast_rays(&origins, &dirs, 1.0).unwrap()
    );
    assert_eq!(
        parent
            .segment_clearance(&p0s, &p1s, &radii, &radii, 0.05, &insets, &insets)
            .unwrap(),
        child
            .segment_clearance(&p0s, &p1s, &radii, &radii, 0.05, &insets, &insets)
            .unwrap()
    );
    let parent_intervals = parent
        .penetration_intervals_batch(&p0s, &p1s, &radii, &radii, 4, 0.12, 80, 0.05, true)
        .unwrap();
    let child_intervals = child
        .penetration_intervals_batch(&p0s, &p1s, &radii, &radii, 4, 0.12, 80, 0.05, true)
        .unwrap();
    assert_intervals_equal(&parent_intervals, &child_intervals);
    assert_eq!(
        parent
            .sphere_breach(&points[..2], &radii, 0.05, 25.0, 8)
            .unwrap(),
        child
            .sphere_breach(&points[..2], &radii, 0.05, 25.0, 8)
            .unwrap()
    );
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

#[test]
fn malformed_and_nonfinite_inputs_return_named_errors() {
    let empty = NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(&[], &[], &[], BOUNDS),
        2.0,
        usize::MAX,
    );
    assert!(matches!(empty, Err(SurfaceBuildError::EmptyTriangleSoup)));

    let mismatch = NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(&TRIANGLES, &[], &AREAS, BOUNDS),
        2.0,
        usize::MAX,
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
        usize::MAX,
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
        usize::MAX,
    );
    assert!(matches!(
        malformed_bounds,
        Err(SurfaceBuildError::MalformedBounds { axis: 0, .. })
    ));

    let invalid_pitch = NativeSurface::from_triangle_soup(
        TriangleSoup::borrowed(&TRIANGLES, &NORMALS, &AREAS, BOUNDS),
        f64::INFINITY,
        usize::MAX,
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
        surface.cast_rays(&points, &[], 1.0),
        "cast_rays",
        "dirs",
        1,
        0,
    );

    assert_batch_error(
        surface.segment_clearance(&points, &[], &scalars, &scalars, 0.05, &scalars, &scalars),
        "segment_clearance",
        "p1s",
        1,
        0,
    );
    assert_batch_error(
        surface.segment_clearance(&points, &points, &[], &scalars, 0.05, &scalars, &scalars),
        "segment_clearance",
        "r0s",
        1,
        0,
    );
    assert_batch_error(
        surface.segment_clearance(&points, &points, &scalars, &[], 0.05, &scalars, &scalars),
        "segment_clearance",
        "r1s",
        1,
        0,
    );
    assert_batch_error(
        surface.segment_clearance(&points, &points, &scalars, &scalars, 0.05, &[], &scalars),
        "segment_clearance",
        "inset0s",
        1,
        0,
    );
    assert_batch_error(
        surface.segment_clearance(&points, &points, &scalars, &scalars, 0.05, &scalars, &[]),
        "segment_clearance",
        "inset1s",
        1,
        0,
    );

    assert_batch_error(
        surface.penetration_intervals_batch(
            &points,
            &[],
            &scalars,
            &scalars,
            8,
            0.12,
            80,
            0.05,
            false,
        ),
        "penetration_intervals_batch",
        "p1s",
        1,
        0,
    );
    assert_batch_error(
        surface.penetration_intervals_batch(
            &points,
            &points,
            &[],
            &scalars,
            8,
            0.12,
            80,
            0.05,
            false,
        ),
        "penetration_intervals_batch",
        "r0s",
        1,
        0,
    );
    assert_batch_error(
        surface.penetration_intervals_batch(
            &points,
            &points,
            &scalars,
            &[],
            8,
            0.12,
            80,
            0.05,
            false,
        ),
        "penetration_intervals_batch",
        "r1s",
        1,
        0,
    );

    assert_batch_error(
        surface.sphere_breach(&points, &[], 0.05, 25.0, 8),
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
            max_grid_bytes: usize::MAX,
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
        usize::MAX,
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
    let (io, it, ifaces) = indexed.cast_rays(&origins, &dirs, 2.0).unwrap();
    let (so, st, sfaces) = soup.cast_rays(&origins, &dirs, 2.0).unwrap();
    assert_eq!(io, so);
    assert_eq!(ifaces, sfaces);
    for (&left, &right) in it.iter().zip(&st) {
        assert_eq!(left.to_bits(), right.to_bits());
    }
}

#[test]
fn indexed_constructor_reports_typed_input_and_derivation_failures() {
    let config = NativeSurfaceConfig {
        grid_pitch_mm: 2.0,
        max_grid_bytes: usize::MAX,
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
            matches!(NativeSurface::from_indexed(&[[0.0; 3]], &[[0,0,0]], NativeSurfaceConfig { grid_pitch_mm: value, max_grid_bytes: usize::MAX }), Err(SurfaceBuildError::InvalidGridPitch(got)) if got.to_bits() == value.to_bits())
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
    assert!(matches!(budget_indexed, Err(SurfaceBuildError::Kernel(_))));
    assert!(matches!(budget_soup, Err(SurfaceBuildError::Kernel(_))));
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
                max_grid_bytes: usize::MAX
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
fn indexed_repeated_index_face_keeps_kernel_degenerate_behavior() {
    let surface = NativeSurface::from_indexed(
        &[[0.0, 0.0, 0.0]],
        &[[0, 0, 0]],
        NativeSurfaceConfig {
            grid_pitch_mm: 2.0,
            max_grid_bytes: usize::MAX,
        },
    )
    .unwrap();
    assert_eq!(surface.nfaces(), 1);
    assert_eq!(surface.geometry().areas(), &[0.0]);
    assert_eq!(surface.geometry().normals(), &[[0.0, 0.0, 0.0]]);
}
