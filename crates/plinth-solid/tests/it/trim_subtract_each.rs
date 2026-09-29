//! `ManifoldBackend::trim_and_subtract_each`: many pieces, each cut to its own
//! half-space and then minus one shared solid, in ONE native call.
//!
//! Graded on axis-aligned boxes, whose every answer is a box volume a reader
//! can check by hand, so nothing here restates the backend's arithmetic.

use plinth_solid::{BooleanLimits, KeepHalfSpace, ManifoldBackend, Mesh};

fn cuboid(min: [f64; 3], max: [f64; 3]) -> Mesh {
    let vertices = vec![
        [min[0], min[1], min[2]],
        [max[0], min[1], min[2]],
        [max[0], max[1], min[2]],
        [min[0], max[1], min[2]],
        [min[0], min[1], max[2]],
        [max[0], min[1], max[2]],
        [max[0], max[1], max[2]],
        [min[0], max[1], max[2]],
    ];
    let triangles = vec![
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
    Mesh::new(vertices, triangles).expect("fixture cuboid must be structurally valid")
}

/// Keep `z <= height`.
fn below(height: f64) -> KeepHalfSpace {
    KeepHalfSpace {
        normal: [0.0, 0.0, -1.0],
        offset: -height,
    }
}

fn bounds(mesh: &Mesh) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for vertex in mesh.vertices() {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(vertex[axis]);
            hi[axis] = hi[axis].max(vertex[axis]);
        }
    }
    (lo, hi)
}

#[test]
fn each_piece_keeps_its_own_half_space_minus_the_shared_solid() {
    // The shared solid: everything with x >= 0.5 over the pieces' extent.
    let model = cuboid([0.5, -1.0, -1.0], [3.0, 2.0, 2.0]);
    let pieces = [
        // Cut by BOTH: plane keeps z <= 0.5, model removes x >= 0.5.
        cuboid([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
        // Cut by the plane only: wholly at x < 0.5.
        cuboid([-1.0, 0.0, 0.0], [0.0, 1.0, 1.0]),
        // Untouched by either.
        cuboid([-1.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        // Wholly inside the model: nothing survives.
        cuboid([1.0, 0.0, 0.0], [2.0, 1.0, 1.0]),
    ];
    let keep = [below(0.5), below(0.5), below(0.5), below(0.5)];
    let out = ManifoldBackend
        .trim_and_subtract_each(&model, &pieces, &keep, BooleanLimits::UNLIMITED)
        .expect("a valid batch");
    assert_eq!(out.len(), pieces.len());

    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    // Piece 0: [0, 0.5] x [0, 1] x [0, 0.5].
    assert!(close(out[0].whole_volume, 1.0));
    assert!(close(out[0].after_half_space_volume, 0.5));
    assert!(close(out[0].after_subtraction_volume, 0.25));
    let (lo, hi) = bounds(out[0].mesh.as_ref().expect("piece 0 survives"));
    assert!(
        close(lo[0], 0.0) && close(hi[0], 0.5),
        "x span {lo:?}..{hi:?}"
    );
    assert!(
        close(lo[2], 0.0) && close(hi[2], 0.5),
        "z span {lo:?}..{hi:?}"
    );
    // Piece 1: the plane halves it, the model does not touch it.
    assert!(close(out[1].after_half_space_volume, 0.5));
    assert!(close(out[1].after_subtraction_volume, 0.5));
    // Piece 2: nothing to remove.
    assert!(close(out[2].whole_volume, 1.0));
    assert!(close(out[2].after_subtraction_volume, 1.0));
    // Piece 3: inside the model, so nothing survives, and that is reported,
    // not raised.
    assert!(
        out[3].mesh.is_none(),
        "a piece inside the solid comes out empty"
    );
    assert!(close(out[3].after_subtraction_volume, 0.0));
}

#[test]
fn an_empty_batch_crosses_nothing_and_a_short_plane_list_is_refused() {
    let model = cuboid([0.0; 3], [1.0; 3]);
    let none = ManifoldBackend
        .trim_and_subtract_each(&model, &[], &[], BooleanLimits::UNLIMITED)
        .expect("an empty batch is a valid batch");
    assert!(none.is_empty());
    let piece = cuboid([2.0; 3], [3.0; 3]);
    assert!(ManifoldBackend
        .trim_and_subtract_each(&model, &[piece], &[], BooleanLimits::UNLIMITED)
        .is_err());
}
