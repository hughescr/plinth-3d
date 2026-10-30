//! Morton (Z-order) keys for the cache-friendly reorder of vertices and triangles
//! (design section 8, "Cache locality").

use crate::aabb::Aabb;
use crate::counters::{self, Op};

const BITS: u32 = 21;
const MAX_CELL: f64 = ((1_u64 << BITS) - 1) as f64;

/// Spread the low 21 bits of `x` so there are two zero bits between each.
const fn spread3(mut x: u64) -> u64 {
    x &= 0x1f_ffff;
    x = (x | (x << 32)) & 0x001f_0000_0000_ffff;
    x = (x | (x << 16)) & 0x001f_0000_ff00_00ff;
    x = (x | (x << 8)) & 0x100f_00f0_0f00_f00f;
    x = (x | (x << 4)) & 0x10c3_0c30_c30c_30c3;
    x = (x | (x << 2)) & 0x1249_2492_4924_9249;
    x
}

/// The 21-bit grid cell of `v` along one axis of `bounds` (0 when the box is flat there).
fn cell(v: f64, lo: f64, extent: f64) -> u64 {
    if extent <= 0.0 {
        return 0;
    }
    let t = ((v - lo) / extent * MAX_CELL).clamp(0.0, MAX_CELL);
    t as u64
}

/// Morton keys (63 bits: 21 per axis) of a structure-of-arrays point set, quantised over
/// `bounds`. All four slices must have the same length. One batch call; counted as
/// [`Op::MortonKeys`] with one item per position.
pub fn morton_keys(xs: &[f64], ys: &[f64], zs: &[f64], bounds: &Aabb, out: &mut [u64]) {
    let n = out.len();
    assert!(
        xs.len() == n && ys.len() == n && zs.len() == n,
        "morton_keys: all slices must have the same length"
    );
    counters::record(Op::MortonKeys, n);
    let e = bounds.extent();
    for (((k, &x), &y), &z) in out.iter_mut().zip(xs).zip(ys).zip(zs) {
        let cx = cell(x, bounds.min[0], e[0]);
        let cy = cell(y, bounds.min[1], e[1]);
        let cz = cell(z, bounds.min[2], e[2]);
        *k = spread3(cx) | (spread3(cy) << 1) | (spread3(cz) << 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spread_puts_bits_three_apart() {
        assert_eq!(spread3(0b1), 0b1);
        assert_eq!(spread3(0b11), 0b1001);
        assert_eq!(spread3(0b101), 0b100_0001);
        assert_eq!(spread3(0x1f_ffff).count_ones(), 21);
    }

    #[test]
    fn corners_of_the_box_get_the_extreme_keys() {
        let b = Aabb {
            min: [0.0; 3],
            max: [1.0; 3],
        };
        let xs = [0.0, 1.0, 1.0, 0.0];
        let ys = [0.0, 1.0, 0.0, 1.0];
        let zs = [0.0, 1.0, 0.0, 0.0];
        let mut k = [0_u64; 4];
        morton_keys(&xs, &ys, &zs, &b, &mut k);
        assert_eq!(k[0], 0);
        assert_eq!(k[1], (1_u64 << 63) - 1);
        // x occupies bit 0 of each triple, y bit 1, z bit 2.
        assert_eq!(k[2], spread3(0x1f_ffff));
        assert_eq!(k[3], spread3(0x1f_ffff) << 1);
    }

    #[test]
    fn flat_box_and_batch_counter() {
        counters::reset();
        let b = Aabb {
            min: [2.0; 3],
            max: [2.0; 3],
        };
        let mut k = [9_u64; 2];
        morton_keys(&[2.0, 2.0], &[2.0, 2.0], &[2.0, 2.0], &b, &mut k);
        assert_eq!(k, [0, 0]);
        let c = counters::snapshot(Op::MortonKeys);
        assert_eq!((c.calls, c.items), (1, 2));
    }
}
