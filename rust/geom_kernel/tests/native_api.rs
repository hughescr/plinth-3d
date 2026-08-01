use geom_kernel::{
    BatchInputError, NativeSurface, NativeSurfaceConfig, SurfaceBuildError, TriangleSoup,
};

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
