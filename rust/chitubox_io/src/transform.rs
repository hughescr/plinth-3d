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

/// Rotate a byte slice interpreted as consecutive little-endian f32 vertices
/// (3 floats / 12 bytes each), in place. Any trailing remainder of fewer than
/// 12 bytes (a non-vertex-aligned tail) is left untouched — never dropped —
/// so unknown bytes survive verbatim (gold invariant c). Used to rotate a
/// support record's referenced mesh blob without disturbing surrounding
/// padding / dead bytes in the shared smesh region.
pub(crate) fn rotate_z_bytes_in_place(bytes: &mut [u8], s: f32, c: f32) {
    let mut o = 0;
    while o + 12 <= bytes.len() {
        let x = f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        let y = f32::from_le_bytes([bytes[o + 4], bytes[o + 5], bytes[o + 6], bytes[o + 7]]);
        let nx = x * c - y * s;
        let ny = x * s + y * c;
        bytes[o..o + 4].copy_from_slice(&nx.to_le_bytes());
        bytes[o + 4..o + 8].copy_from_slice(&ny.to_le_bytes());
        // z (bytes[o+8..o+12]) unchanged
        o += 12;
    }
}
