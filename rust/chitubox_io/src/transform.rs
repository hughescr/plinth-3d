//! Z-rotation of a rigid assembly. The format has no rotation field —
//! rotation is baked into the centered vertex stream (CHITUBOX-FORMAT.md
//! §3.1). Rotation about local Z through the origin:
//! `(x,y,z) -> (x cosθ - y sinθ, x sinθ + y cosθ, z)`.

/// Rotate a stream given precomputed sin/cos (so one call rotates the model
/// mesh + all support blobs + all endpoints with a single trig evaluation).
pub(crate) fn rotate_z_with(xyz: &mut [f32], s: f32, c: f32) {
    for v in xyz.chunks_exact_mut(3) {
        let (x, y) = (v[0], v[1]);
        v[0] = x * c - y * s;
        v[1] = x * s + y * c;
        // v[2] unchanged
    }
}

/// Rotate a single point.
pub(crate) fn rotate_z_point(p: [f32; 3], s: f32, c: f32) -> [f32; 3] {
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2]]
}
