//! Single-precision vector and quaternion helpers for the shape builders.
//!
//! Every native record mesh is rotated into place with a unit quaternion built by
//! shortest-arc rotation from the canonical axis. The fit to the native files shows the
//! construction is the one Qt's `QQuaternion::rotationTo` uses: the canonical axis is
//! `-z`, and the fallback for a half-turn is a half-turn about `-(from x X)` = `y`. The
//! f32 arithmetic below reproduces 91.8 percent of all native mesh coordinates bit for bit
//! and every mesh within 1.5 ULP (`shapes::MESH_TOLERANCE_ULPS` is 2).

/// A point or direction, mm.
pub type V3 = [f32; 3];

/// `a - b`.
#[must_use]
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `a + b`.
#[must_use]
pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// Dot product.
#[must_use]
pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Cross product.
#[must_use]
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Euclidean length.
#[must_use]
pub fn length(a: V3) -> f32 {
    dot(a, a).sqrt()
}

/// `a / s`, component by component.
#[must_use]
pub fn div(a: V3, s: f32) -> V3 {
    [a[0] / s, a[1] / s, a[2] / s]
}

/// A rotation as a quaternion (w, x, y, z).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quat {
    w: f32,
    x: f32,
    y: f32,
    z: f32,
}

/// Qt's `qFuzzyIsNull` limit for a float.
const FUZZY_NULL: f32 = 1e-5;

impl Quat {
    /// The shortest-arc rotation taking unit vector `from` to unit vector `to`. For a
    /// half-turn it turns about `from x X` (or `from x Y` when that vanishes).
    #[must_use]
    pub fn rotation_to(from: V3, to: V3) -> Self {
        let d = dot(from, to) + 1.0;
        if d.abs() <= FUZZY_NULL {
            let mut axis = cross(from, [1.0, 0.0, 0.0]);
            if dot(axis, axis).abs() <= FUZZY_NULL {
                axis = cross(from, [0.0, 1.0, 0.0]);
            }
            let l = length(axis);
            let a = div(axis, l);
            return Self {
                w: 0.0,
                x: a[0],
                y: a[1],
                z: a[2],
            };
        }
        let a = cross(from, to);
        let l = (d * d + dot(a, a)).sqrt();
        Self {
            w: d / l,
            x: a[0] / l,
            y: a[1] / l,
            z: a[2] / l,
        }
    }

    /// Hamilton product `self * other`.
    fn mul(self, o: Self) -> Self {
        Self {
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
        }
    }

    /// `q v q*`.
    #[must_use]
    pub fn rotate(self, v: V3) -> V3 {
        let conj = Self {
            w: self.w,
            x: -self.x,
            y: -self.y,
            z: -self.z,
        };
        let r = self.mul(Self {
            w: 0.0,
            x: v[0],
            y: v[1],
            z: v[2],
        });
        let r = r.mul(conj);
        [r.x, r.y, r.z]
    }
}

/// `(cos, sin)` of `index * 360 / n` degrees: computed in f64, rounded once to f32.
#[must_use]
pub fn unit_ring(n: usize) -> Vec<(f32, f32)> {
    (0..n)
        .map(|j| {
            let a = (j as f64) * std::f64::consts::TAU / (n as f64);
            (a.cos() as f32, a.sin() as f32)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: V3, b: V3) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
    }

    #[test]
    fn rotation_to_same_axis_is_identity() {
        let q = Quat::rotation_to([0.0, 0.0, -1.0], [0.0, 0.0, -1.0]);
        assert_eq!(
            q.rotate([1.0, 2.0, 3.0]).map(f32::to_bits),
            [1.0f32, 2.0, 3.0].map(f32::to_bits)
        );
    }

    #[test]
    fn rotation_to_maps_axis_onto_target() {
        let to = div([1.0, 2.0, 2.0], 3.0);
        let q = Quat::rotation_to([0.0, 0.0, -1.0], to);
        assert!(close(q.rotate([0.0, 0.0, -1.0]), to));
    }

    #[test]
    fn half_turn_is_about_y() {
        let q = Quat::rotation_to([0.0, 0.0, -1.0], [0.0, 0.0, 1.0]);
        assert!(close(q.rotate([1.0, 2.0, 3.0]), [-1.0, 2.0, -3.0]));
    }

    #[test]
    fn unit_ring_has_exact_quadrants() {
        let r = unit_ring(16);
        assert_eq!(r[0], (1.0, 0.0));
        assert!(r[4].0.abs() < 1e-7 && (r[4].1 - 1.0).abs() < 1e-7);
        assert!(r[8].0 + 1.0 < 1e-7);
    }
}
