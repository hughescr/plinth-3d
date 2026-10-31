//! Small f64 vector helpers and boxes. The checker's own; nothing is shared with the generator.

/// A point or vector, mm.
pub type V = [f64; 3];

#[must_use]
pub fn f(v: [f32; 3]) -> V {
    [f64::from(v[0]), f64::from(v[1]), f64::from(v[2])]
}

#[must_use]
pub fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[must_use]
pub fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[must_use]
pub fn mul(a: V, s: f64) -> V {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[must_use]
pub fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[must_use]
pub fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[must_use]
pub fn len(a: V) -> f64 {
    dot(a, a).sqrt()
}

#[must_use]
pub fn dist(a: V, b: V) -> f64 {
    len(sub(a, b))
}

/// `a + (b - a) * t`.
#[must_use]
pub fn lerp(a: V, b: V, t: f64) -> V {
    add(a, mul(sub(b, a), t))
}

/// The unit vector along `a`; the zero vector for a zero (or non-finite) input.
#[must_use]
pub fn unit(a: V) -> V {
    let n = len(a);
    if n > 0.0 && n.is_finite() {
        mul(a, 1.0 / n)
    } else {
        [0.0; 3]
    }
}

/// The centroid of a triangle.
#[must_use]
pub fn centroid(t: &[V; 3]) -> V {
    mul(add(add(t[0], t[1]), t[2]), 1.0 / 3.0)
}

/// Two unit vectors that, with the unit vector `n`, form a right-handed frame
/// (`e1 x e2 = n`).
#[must_use]
pub fn frame(n: V) -> (V, V) {
    let helper = if n[0].abs() < 0.6 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let e1 = unit(cross(helper, n));
    let e2 = cross(n, e1);
    (e1, e2)
}

/// An axis-aligned box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub lo: V,
    pub hi: V,
}

impl Aabb {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            lo: [f64::INFINITY; 3],
            hi: [f64::NEG_INFINITY; 3],
        }
    }

    pub fn grow(&mut self, p: V) {
        for ((lo, hi), x) in self.lo.iter_mut().zip(self.hi.iter_mut()).zip(p) {
            *lo = lo.min(x);
            *hi = hi.max(x);
        }
    }

    pub fn grow_box(&mut self, b: &Aabb) {
        self.grow(b.lo);
        self.grow(b.hi);
    }

    #[must_use]
    pub fn inflated(&self, e: f64) -> Self {
        Self {
            lo: sub(self.lo, [e; 3]),
            hi: add(self.hi, [e; 3]),
        }
    }

    #[must_use]
    pub fn overlaps(&self, o: &Aabb) -> bool {
        (0..3).all(|k| self.lo[k] <= o.hi[k] && o.lo[k] <= self.hi[k])
    }

    #[must_use]
    pub fn of_points(points: impl IntoIterator<Item = V>) -> Self {
        let mut b = Self::empty();
        for p in points {
            b.grow(p);
        }
        b
    }

    #[must_use]
    pub fn centre(&self) -> V {
        mul(add(self.lo, self.hi), 0.5)
    }

    #[must_use]
    pub fn axis_len(&self, k: usize) -> f64 {
        self.hi[k] - self.lo[k]
    }

    /// The largest absolute coordinate of the box (the scale of the numbers in it).
    #[must_use]
    pub fn max_abs(&self) -> f64 {
        self.lo
            .iter()
            .chain(&self.hi)
            .fold(0.0, |m, x| m.max(x.abs()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_is_right_handed_and_orthonormal() {
        for n in [
            [0.0, 0.0, 1.0],
            unit([1.0, 2.0, 3.0]),
            unit([-1.0, 0.1, -0.2]),
        ] {
            let (a, b) = frame(n);
            assert!((len(a) - 1.0).abs() < 1e-12 && (len(b) - 1.0).abs() < 1e-12);
            assert!(dot(a, b).abs() < 1e-12 && dot(a, n).abs() < 1e-12);
            let c = cross(a, b);
            assert!(dist(c, n) < 1e-12);
        }
    }

    #[test]
    fn boxes_grow_and_overlap() {
        let mut b = Aabb::empty();
        b.grow([0.0, 0.0, 0.0]);
        b.grow([1.0, 2.0, 3.0]);
        assert!(b.overlaps(&Aabb::of_points([[0.5, 0.5, 0.5]])));
        assert!(!b.overlaps(&Aabb::of_points([[1.5, 0.5, 0.5]])));
        assert!(
            b.inflated(1.0)
                .overlaps(&Aabb::of_points([[1.5, 0.5, 0.5]]))
        );
    }
}
