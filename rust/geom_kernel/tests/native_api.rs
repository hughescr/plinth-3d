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
    let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let faces = [[0, 1, 2]];
    let indexed = NativeSurface::from_indexed(
        &vertices,
        &faces,
        NativeSurfaceConfig {
            grid_pitch_mm: 2.0,
            max_grid_bytes: usize::MAX,
        },
    )
    .unwrap();
    let soup = surface();
    assert_eq!(indexed.geometry().normals(), soup.geometry().normals());
    assert_eq!(indexed.geometry().areas(), soup.geometry().areas());
    assert_eq!(indexed.geometry().centroids(), soup.geometry().centroids());
    assert_eq!(
        indexed.geometry().face_bounds(),
        soup.geometry().face_bounds()
    );
    assert_eq!(indexed.geometry().bounds(), soup.geometry().bounds());

    let points = [[0.25, 0.25, 1.0], [2.0, 2.0, 0.0]];
    let origins = [[0.25, 0.25, 1.0], [2.0, 2.0, 1.0]];
    let dirs = [[0.0, 0.0, -1.0], [0.0, 0.0, -1.0]];
    assert_eq!(
        indexed.query(&points, f64::INFINITY),
        soup.query(&points, f64::INFINITY)
    );
    assert_eq!(indexed.project(&points), soup.project(&points));
    assert_eq!(indexed.inside(&points), soup.inside(&points));
    assert_eq!(
        indexed.cast_rays(&origins, &dirs, 2.0),
        soup.cast_rays(&origins, &dirs, 2.0)
    );
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
