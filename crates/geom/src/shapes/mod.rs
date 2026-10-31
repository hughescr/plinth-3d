//! The pure f32 kernel of the native ChituBox v1.4.0 support shapes (design
//! `step11-construction.md` sections 6.2 and 10).
//!
//! It builds the exact vertices ChituBox writes for a ball, a 16-sided frustum, a raft pad
//! and a raft bridge, from the parameters that decide them. It knows nothing of records or
//! files: `plinth-emit` wraps these meshes in record fields, and `plinth-support` builds the
//! same vertices to check them, so both sides see the numbers that are written.
//!
//! * Mesh coordinates are within [`MESH_TOLERANCE_ULPS`] of the native f32 values. The
//!   native vertices come out of a single-precision rotation whose exact order of
//!   operations is not known, so 91.8 percent of coordinates match bit for bit and the
//!   rest differ by at most 1.5 units in the last place. The comparison against the native
//!   samples is `crates/emit/tests/it/native_shapes.rs`.
//! * [`mesh_error_ulps`] measures a difference in the unit the tolerance is stated in.

pub mod math;
mod solids;

pub use math::V3;
pub use solids::{
    BRIDGE_TRIANGLES, FRUSTUM_SIDES, FRUSTUM_TRIANGLES, PAD_SIDES, PAD_TRIANGLES, SPHERE_RINGS,
    SPHERE_SIDES, SPHERE_TRIANGLES, bridge, frustum, pad, sphere,
};

/// One mesh triangle: three vertices of three f32 coordinates, mm.
pub type Triangle = [[f32; 3]; 3];

/// The largest distance, in f32 units in the last place of the part's largest coordinate
/// (see [`mesh_error_ulps`]), between a built mesh and the native one. The measured maximum
/// over every native record is 1.5; the justification is in the format document, section
/// 19. Two ULP is under 4 nanometres for coordinates below 32 mm, about 25,000 times finer
/// than the smallest limit the checker applies (0.1 mm).
pub const MESH_TOLERANCE_ULPS: f32 = 2.0;

/// The spacing between `x` and the next larger f32 (one unit in the last place).
#[must_use]
pub fn ulp(x: f32) -> f32 {
    let a = x.abs();
    f32::from_bits(a.to_bits() + 1) - a
}

/// The largest difference between two meshes of the same shape, in units in the last place
/// of the largest coordinate magnitude in either mesh. `None` when the triangle counts
/// differ or a value is not finite. 0.0 means bit-equal.
///
/// The unit is the ULP of the mesh's largest coordinate, not of each coordinate, because
/// the native vertices are an f32 sum "part position + rotated offset": the sum rounds at
/// the size of the position, so a coordinate that lands near 0 still carries an error of
/// that size.
#[must_use]
pub fn mesh_error_ulps(a: &[Triangle], b: &[Triangle]) -> Option<f32> {
    if a.len() != b.len() {
        return None;
    }
    let pairs = || {
        a.iter()
            .flatten()
            .flatten()
            .zip(b.iter().flatten().flatten())
    };
    if pairs().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return None;
    }
    let scale = pairs().fold(0.0f32, |m, (x, y)| m.max(x.abs()).max(y.abs()));
    let unit = ulp(scale.max(f32::MIN_POSITIVE));
    let worst = pairs().fold(0.0f32, |m, (x, y)| m.max((x - y).abs()));
    Some(worst / unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_error_is_measured_at_the_scale_of_the_largest_coordinate() {
        assert_eq!(ulp(1.0).to_bits(), f32::EPSILON.to_bits());
        let a = [[[10.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]];
        assert!(mesh_error_ulps(&a, &a).unwrap().abs() < 1e-9);
        // Moving a near-zero coordinate by 2 ULP of the biggest coordinate (10.0) is 2.0.
        let mut b = a;
        b[0][1][0] = 2.0 * ulp(10.0);
        let e = mesh_error_ulps(&a, &b).unwrap();
        assert!((e - 2.0).abs() < 1e-4, "{e}");
        assert!(mesh_error_ulps(&a, &[]).is_none());
        b[0][0][0] = f32::NAN;
        assert!(mesh_error_ulps(&a, &b).is_none());
    }
}
