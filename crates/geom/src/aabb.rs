//! Axis-aligned bounding box.

/// An axis-aligned box in f64. The empty box has `min = +inf` and `max = -inf`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    /// Smallest corner.
    pub min: [f64; 3],
    /// Largest corner.
    pub max: [f64; 3],
}

impl Aabb {
    /// The box that contains nothing.
    pub const EMPTY: Self = Self {
        min: [f64::INFINITY; 3],
        max: [f64::NEG_INFINITY; 3],
    };

    /// The box of a structure-of-arrays point set. One pass per axis over a contiguous
    /// slice. The slices must have equal length and hold finite values.
    #[must_use]
    pub fn of_points(xs: &[f64], ys: &[f64], zs: &[f64]) -> Self {
        let (x0, x1) = min_max(xs);
        let (y0, y1) = min_max(ys);
        let (z0, z1) = min_max(zs);
        Self {
            min: [x0, y0, z0],
            max: [x1, y1, z1],
        }
    }

    /// True when the box contains nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.min[0] > self.max[0]
    }

    /// Side lengths (zero for the empty box).
    #[must_use]
    pub fn extent(&self) -> [f64; 3] {
        if self.is_empty() {
            return [0.0; 3];
        }
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }
}

fn min_max(v: &[f64]) -> (f64, f64) {
    v.iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &x| {
            (lo.min(x), hi.max(x))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::assert_pt;

    #[test]
    fn box_of_points_and_empty() {
        let b = Aabb::of_points(&[1.0, -2.0, 3.0], &[0.0, 5.0, 1.0], &[7.0, 7.0, 7.0]);
        assert_pt(b.min, [-2.0, 0.0, 7.0]);
        assert_pt(b.max, [3.0, 5.0, 7.0]);
        assert_pt(b.extent(), [5.0, 5.0, 0.0]);
        assert!(!b.is_empty());
        let e = Aabb::of_points(&[], &[], &[]);
        assert!(e.is_empty());
        assert_pt(e.extent(), [0.0; 3]);
        assert_eq!(e, Aabb::EMPTY);
    }
}
