//! Regression guard: a face with no area has no normal, and the builder must
//! say so instead of storing a fabricated one.
//!
//! THE DEFECT THIS WOULD HAVE CAUGHT. `Mesh::from_indexed` normalised each
//! face's edge cross product by dividing by `length.max(1e-30)`. For a face
//! that spans no area the cross product is exactly `[0, 0, 0]`, so the three
//! divisions produced `0 / 1e-30 == 0.0` and the face was stored with a
//! "normal" of `[0, 0, 0]`. Nothing downstream could detect it: the guard
//! immediately below the division only checked `is_finite`, and zero is finite.
//!
//! Zero is not a direction, and the kernel treats stored normals AS directions:
//! the breach predicate takes `inward = -normal` and normalises it, the
//! exterior-face mask offsets sample points along `+normal`, and the contact
//! tests take `dot(normal, axis)` to get an incidence angle. Given `[0, 0, 0]`
//! each of those silently degrades -- the inward ray has no direction, the
//! offset sample lands exactly on the face it was meant to step off, and every
//! incidence angle reads as 90 degrees. None of them errors.
//!
//! A degenerate face is a fact about the CALLER's mesh, so it is reported as a
//! typed error naming the face and the reason, not as a value.
//!
//! These tests assert classification of meshes whose degeneracy is decided by
//! inspection; nothing here recomputes a cross product.

use geom_kernel::{DegenerateFaceCause, NativeSurface, NativeSurfaceConfig, SurfaceBuildError};

fn config() -> NativeSurfaceConfig {
    NativeSurfaceConfig {
        grid_pitch_mm: 2.0,
        max_grid_bytes: usize::MAX,
    }
}

fn build(
    vertices: &[[f64; 3]],
    faces: &[[u32; 3]],
) -> Result<NativeSurface, SurfaceBuildError> {
    NativeSurface::from_indexed(vertices, faces, config())
}

/// A face naming the same vertex twice is a line segment by construction.
///
/// This is the case that is impossible to rescue with better arithmetic: no
/// coordinate values make a triangle out of two distinct points.
#[test]
fn a_repeated_vertex_index_is_a_degenerate_face() {
    let vertices = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    // Corners 0 and 2 both name vertex 0.
    let faces = vec![[0, 1, 0]];

    let error = build(&vertices, &faces).err().expect(
        "a face with a repeated corner index spans no area and cannot be given a normal",
    );
    assert_eq!(
        error,
        SurfaceBuildError::DegenerateFace {
            face: 0,
            cause: DegenerateFaceCause::RepeatedVertexIndex {
                corners: (0, 2),
                index: 0
            }
        },
        "the error must name the face and both corner slots, so the caller can \
         find the bad triangle in its own data"
    );
}

/// Three distinct indices whose coordinates coincide: an unwelded mesh, or one
/// welded with a tolerance coarser than its own feature size.
#[test]
fn coincident_vertices_at_distinct_indices_are_a_degenerate_face() {
    let vertices = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0], // a separate index at the same place as vertex 1
    ];
    let faces = vec![[0, 1, 2]];

    let error = build(&vertices, &faces)
        .err()
        .expect("two corners at identical coordinates leave no area");
    assert_eq!(
        error,
        SurfaceBuildError::DegenerateFace {
            face: 0,
            cause: DegenerateFaceCause::CoincidentVertices { corners: (1, 2) }
        },
        "distinct indices at one point is a different fact from a repeated index \
         and must be reported as such -- one is an index bug, the other a weld bug"
    );
}

/// Three distinct, non-coincident corners on one line: a real triangle that has
/// collapsed to a sliver of exactly zero width.
#[test]
fn collinear_corners_are_a_degenerate_face() {
    let vertices = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
    let faces = vec![[0, 1, 2]];

    let error = build(&vertices, &faces)
        .err()
        .expect("three collinear corners span no area");
    assert_eq!(
        error,
        SurfaceBuildError::DegenerateFace {
            face: 0,
            cause: DegenerateFaceCause::CollinearVertices
        }
    );
}

/// The error names the offending face, not merely the fact that one exists.
#[test]
fn the_error_names_which_face_is_degenerate() {
    let vertices = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ];
    // Faces 0 and 1 are sound; face 2 is collinear along the x axis, using a
    // midpoint vertex so no two corners coincide.
    let vertices = {
        let mut v = vertices;
        v.push([0.5, 0.0, 0.0]);
        v
    };
    let faces = vec![[0, 1, 2], [0, 1, 3], [0, 4, 1]];

    let error = build(&vertices, &faces)
        .err()
        .expect("one degenerate face fails the build");
    assert_eq!(
        error,
        SurfaceBuildError::DegenerateFace {
            face: 2,
            cause: DegenerateFaceCause::CollinearVertices
        },
        "the caller needs the index of the face to fix, and faces 0 and 1 are fine"
    );
}

/// A degenerate face must NOT be filed under the arithmetic-failure variant.
///
/// `IndexedDerivedArithmetic` means an intermediate came out non-finite. A
/// degenerate face is finite arithmetic reporting a real geometric fact, and
/// merging the two would let a reader believe a precision problem had been
/// diagnosed when a modelling problem had.
#[test]
fn a_degenerate_face_is_not_reported_as_an_arithmetic_failure() {
    let vertices = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
    let error = build(&vertices, &[[0, 1, 2]])
        .err()
        .expect("collinear corners fail the build");
    assert!(
        !matches!(error, SurfaceBuildError::IndexedDerivedArithmetic { .. }),
        "got {error:?}"
    );
    let rendered = error.to_string();
    assert!(
        rendered.contains("spans no area"),
        "the message must say what is wrong with the mesh, got {rendered:?}"
    );
    assert!(
        rendered.contains("collinear"),
        "and which of the three degeneracies it is, got {rendered:?}"
    );
}

/// THE CONTROL, and the reason the tests above have teeth rather than merely
/// rejecting everything: a sound mesh still builds, and its faces still carry
/// unit normals.
///
/// Two faces of a tetrahedron, each with an area of 0.5 and a normal of unit
/// length. If the degeneracy predicate were too eager this test is what fails.
#[test]
fn a_sound_mesh_still_builds_and_its_normals_are_unit_length() {
    let vertices = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ];
    let faces = vec![[0, 1, 2], [0, 1, 3], [0, 2, 3]];
    let surface = build(&vertices, &faces).expect("a sound indexed mesh builds");
    assert_eq!(surface.nfaces(), 3);

    for (face, normal) in surface.geometry().normals().iter().enumerate() {
        let length =
            (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        assert!(
            (length - 1.0).abs() < 1e-12,
            "face {face} must carry a UNIT normal, got length {length}. A stored \
             length of 0 is what the `length.max(1e-30)` division used to produce \
             for a degenerate face, and it is indistinguishable from a direction \
             to every consumer that reads it"
        );
    }
}

/// A near-degenerate but genuinely non-zero sliver must still build.
///
/// The predicate is exact -- `length > 0.0` -- and carries no tolerance, so a
/// real triangle a nanometre wide is a real triangle. This test is what stops
/// anyone "fixing" the predicate by giving it an epsilon: a tolerance here
/// would be a magic number deciding accept/reject, which the project's
/// physics-first constants rule forbids.
#[test]
fn a_genuinely_thin_sliver_is_not_degenerate() {
    let vertices = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, 1.0e-9, 0.0]];
    let surface = build(&vertices, &[[0, 1, 2]])
        .expect("a sliver with non-zero area is a real face, however thin");
    let normal = surface.geometry().normals()[0];
    let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    assert!(
        (length - 1.0).abs() < 1e-12,
        "and it gets a unit normal like any other face, got length {length}"
    );
}
