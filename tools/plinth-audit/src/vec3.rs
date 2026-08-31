//! Minimal `[f64; 3]` vector algebra and an axis-aligned box.
//!
//! Reimplemented rather than imported. It deliberately differs from
//! `geom_kernel` in one respect worth naming: **nothing here is robust
//! predicate arithmetic.** `geom_kernel` uses the `robust` crate for
//! orientation tests; the audit uses plain `f64` and states an explicit
//! tolerance at every comparison instead. A disagreement between the two on a
//! near-degenerate triangle is a finding to investigate, not a bug in either.

pub type V3 = [f64; 3];

#[must_use]
pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
#[must_use]
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[must_use]
pub fn mul(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
#[must_use]
pub fn dot(a: V3, b: V3) -> f64 {
    a[0].mul_add(b[0], a[1].mul_add(b[1], a[2] * b[2]))
}
#[must_use]
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1].mul_add(b[2], -(a[2] * b[1])),
        a[2].mul_add(b[0], -(a[0] * b[2])),
        a[0].mul_add(b[1], -(a[1] * b[0])),
    ]
}
#[must_use]
pub fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}
#[must_use]
pub fn dist(a: V3, b: V3) -> f64 {
    norm(sub(a, b))
}
#[must_use]
pub fn dist2(a: V3, b: V3) -> f64 {
    let d = sub(a, b);
    dot(d, d)
}
#[must_use]
pub fn midpoint(a: V3, b: V3) -> V3 {
    [
        0.5 * (a[0] + b[0]),
        0.5 * (a[1] + b[1]),
        0.5 * (a[2] + b[2]),
    ]
}
/// Unit vector, or `None` when the input has no direction to normalise.
#[must_use]
pub fn normalize(a: V3) -> Option<V3> {
    let n = norm(a);
    if n.is_finite() && n > 0.0 {
        Some([a[0] / n, a[1] / n, a[2] / n])
    } else {
        None
    }
}
#[must_use]
pub fn is_finite(a: V3) -> bool {
    a[0].is_finite() && a[1].is_finite() && a[2].is_finite()
}
#[must_use]
pub fn inf_norm(a: V3) -> f64 {
    a[0].abs().max(a[1].abs()).max(a[2].abs())
}

/// Twice the triangle area as a vector; its length is `2 * area`.
#[must_use]
pub fn tri_area_vec(a: V3, b: V3, c: V3) -> V3 {
    cross(sub(b, a), sub(c, a))
}
#[must_use]
pub fn tri_area(a: V3, b: V3, c: V3) -> f64 {
    0.5 * norm(tri_area_vec(a, b, c))
}
/// Facet normal from the winding, `normalize((v1-v0) x (v2-v0))`.
/// `None` for a triangle of exactly zero area.
#[must_use]
pub fn tri_normal(a: V3, b: V3, c: V3) -> Option<V3> {
    normalize(tri_area_vec(a, b, c))
}

/// Six times the signed volume of the tetrahedron `(origin, a, b, c)`.
/// Summing this over an outward-wound closed mesh gives `6 * volume`.
#[must_use]
pub fn signed_tet_volume6(a: V3, b: V3, c: V3) -> f64 {
    dot(a, cross(b, c))
}

/// Any unit vector perpendicular to `u`. Deterministic.
#[must_use]
pub fn any_perpendicular(u: V3) -> V3 {
    let alt = if u[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    normalize(cross(u, alt)).unwrap_or([1.0, 0.0, 0.0])
}

/// A right-handed orthonormal basis whose third axis is `u`.
#[must_use]
pub fn basis_from_axis(u: V3) -> (V3, V3, V3) {
    let e1 = any_perpendicular(u);
    let e2 = normalize(cross(u, e1)).unwrap_or([0.0, 1.0, 0.0]);
    (e1, e2, u)
}

/// The `k`-th direction of the deterministic golden-spiral sequence used by
/// the ray-parity inside test (`audit-invariants.md` §1.4). The sequence is
/// fixed and shared by every query so that two runs over the same bytes give
/// the same answer.
#[must_use]
pub fn golden_spiral_direction(k: usize) -> V3 {
    // Fibonacci sphere over 16 samples; k beyond 15 wraps deterministically.
    const N: f64 = 16.0;
    let i = (k % 16) as f64;
    let phi = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt()); // golden angle
    let z = 1.0 - (2.0 * i + 1.0) / N;
    let r = (1.0 - z * z).max(0.0).sqrt();
    let t = phi * i;
    normalize([r * t.cos(), r * t.sin(), z]).unwrap_or([0.0, 0.0, 1.0])
}

/// The fixed number of ray directions tried before a point query is declared
/// `indeterminate`. Sixteen exhausted retries fail whichever invariant asked;
/// **no perturbation of the query point is permitted** -- moving the point
/// changes the question (`audit-invariants.md` §1.4).
pub const RAY_DIRECTION_BUDGET: usize = 16;

/// Axis-aligned bounding box. `min > max` on any axis means empty.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: V3,
    pub max: V3,
}

impl Aabb {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        }
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        (0..3).any(|i| self.min[i] > self.max[i])
    }
    pub fn add_point(&mut self, p: V3) {
        for i in 0..3 {
            if p[i] < self.min[i] {
                self.min[i] = p[i];
            }
            if p[i] > self.max[i] {
                self.max[i] = p[i];
            }
        }
    }
    pub fn union(&mut self, other: &Self) {
        if !other.is_empty() {
            self.add_point(other.min);
            self.add_point(other.max);
        }
    }
    #[must_use]
    pub fn expanded(&self, by: f64) -> Self {
        if self.is_empty() {
            return *self;
        }
        Self {
            min: [self.min[0] - by, self.min[1] - by, self.min[2] - by],
            max: [self.max[0] + by, self.max[1] + by, self.max[2] + by],
        }
    }
    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        !self.is_empty()
            && !other.is_empty()
            && (0..3).all(|i| self.min[i] <= other.max[i] && other.min[i] <= self.max[i])
    }
    #[must_use]
    pub fn contains(&self, p: V3) -> bool {
        !self.is_empty() && (0..3).all(|i| p[i] >= self.min[i] && p[i] <= self.max[i])
    }
    #[must_use]
    pub fn extent(&self) -> V3 {
        if self.is_empty() {
            [0.0; 3]
        } else {
            sub(self.max, self.min)
        }
    }
    #[must_use]
    pub fn center(&self) -> V3 {
        midpoint(self.min, self.max)
    }
    #[must_use]
    pub fn from_points(points: impl IntoIterator<Item = V3>) -> Self {
        let mut b = Self::empty();
        for p in points {
            b.add_point(p);
        }
        b
    }
}

/// Closest point on triangle `(a, b, c)` to `p`, and the distance to it.
///
/// Ericson's region-based method, branch-complete: it handles all seven
/// Voronoi regions of a triangle and returns an exact closest point for a
/// non-degenerate triangle. For a triangle of exactly zero area it degrades to
/// the closest point on the longest edge, which is the right answer for a
/// sliver and is never silently treated as a face hit.
#[must_use]
pub fn point_triangle_closest(p: V3, a: V3, b: V3, c: V3) -> (f64, V3) {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return (dist(p, a), a);
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return (dist(p, b), b);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let denom = d1 - d3;
        let v = if denom.abs() > 0.0 { d1 / denom } else { 0.0 };
        let q = add(a, mul(ab, v));
        return (dist(p, q), q);
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return (dist(p, c), c);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let denom = d2 - d6;
        let w = if denom.abs() > 0.0 { d2 / denom } else { 0.0 };
        let q = add(a, mul(ac, w));
        return (dist(p, q), q);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let denom = (d4 - d3) + (d5 - d6);
        let w = if denom.abs() > 0.0 {
            (d4 - d3) / denom
        } else {
            0.0
        };
        let q = add(b, mul(sub(c, b), w));
        return (dist(p, q), q);
    }
    let denom_sum = va + vb + vc;
    if denom_sum.abs() == 0.0 {
        // Exactly degenerate triangle: fall back to the longest edge.
        let mut best = (dist(p, a), a);
        for (u, v) in [(a, b), (b, c), (c, a)] {
            let e = sub(v, u);
            let len2 = dot(e, e);
            let t = if len2 > 0.0 {
                (dot(sub(p, u), e) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let q = add(u, mul(e, t));
            let d = dist(p, q);
            if d < best.0 {
                best = (d, q);
            }
        }
        return best;
    }
    let denom = 1.0 / denom_sum;
    let v = vb * denom;
    let w = vc * denom;
    let q = add(add(a, mul(ab, v)), mul(ac, w));
    (dist(p, q), q)
}

/// Moller-Trumbore ray/triangle intersection.
///
/// Returns `(t, u, v)` where the hit is `origin + t*dir` and `(u, v)` are the
/// barycentric coordinates against `(b-a, c-a)`. **Culls nothing** -- back
/// faces hit too, because ray parity needs every crossing. `eps_par` is the
/// parallel-ray rejection threshold and `t` may be negative; callers filter.
#[must_use]
pub fn ray_triangle(origin: V3, dir: V3, a: V3, b: V3, c: V3) -> Option<(f64, f64, f64)> {
    let e1 = sub(b, a);
    let e2 = sub(c, a);
    let pvec = cross(dir, e2);
    let det = dot(e1, pvec);
    if det.abs() < 1e-300 {
        return None;
    }
    let inv_det = 1.0 / det;
    let tvec = sub(origin, a);
    let u = dot(tvec, pvec) * inv_det;
    let qvec = cross(tvec, e1);
    let v = dot(dir, qvec) * inv_det;
    let t = dot(e2, qvec) * inv_det;
    if !t.is_finite() || !u.is_finite() || !v.is_finite() {
        return None;
    }
    Some((t, u, v))
}

/// Squared distance between two segments, with the parameters that realise it.
/// Returns `(d2, s, t)` for points `p0 + s*(p1-p0)` and `q0 + t*(q1-q0)`.
#[must_use]
pub fn segment_segment_closest(p0: V3, p1: V3, q0: V3, q1: V3) -> (f64, f64, f64) {
    let d1 = sub(p1, p0);
    let d2v = sub(q1, q0);
    let r = sub(p0, q0);
    let a = dot(d1, d1);
    let e = dot(d2v, d2v);
    let f = dot(d2v, r);
    let (mut s, mut t);
    if a <= f64::MIN_POSITIVE && e <= f64::MIN_POSITIVE {
        return (dist2(p0, q0), 0.0, 0.0);
    }
    if a <= f64::MIN_POSITIVE {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = dot(d1, r);
        if e <= f64::MIN_POSITIVE {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = dot(d1, d2v);
            let denom = a.mul_add(e, -(b * b));
            s = if denom > 0.0 {
                (b.mul_add(f, -(c * e)) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            t = b.mul_add(s, f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
        }
    }
    let cp = add(p0, mul(d1, s));
    let cq = add(q0, mul(d2v, t));
    (dist2(cp, cq), s, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_point_lands_on_the_face_interior() {
        let (d, q) = point_triangle_closest(
            [0.25, 0.25, 1.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        );
        assert!((d - 1.0).abs() < 1e-12, "{d}");
        assert!((q[2]).abs() < 1e-12);
    }

    #[test]
    fn closest_point_lands_on_a_vertex() {
        let (d, q) = point_triangle_closest(
            [-1.0, -1.0, 0.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        );
        assert!((d - 2.0_f64.sqrt()).abs() < 1e-12);
        assert_eq!(q, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn ray_hits_the_unit_triangle_at_the_expected_parameter() {
        let hit = ray_triangle(
            [0.25, 0.25, -3.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        )
        .expect("hit");
        assert!((hit.0 - 3.0).abs() < 1e-12);
    }

    #[test]
    fn golden_spiral_directions_are_unit_and_distinct() {
        let dirs: Vec<V3> = (0..RAY_DIRECTION_BUDGET)
            .map(golden_spiral_direction)
            .collect();
        for d in &dirs {
            assert!((norm(*d) - 1.0).abs() < 1e-12);
        }
        for i in 0..dirs.len() {
            for j in (i + 1)..dirs.len() {
                assert!(dist(dirs[i], dirs[j]) > 1e-3, "{i} {j}");
            }
        }
    }

    #[test]
    fn segment_distance_matches_a_hand_case() {
        let (d2v, _, _) = segment_segment_closest(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
        );
        assert!((d2v - 2.0).abs() < 1e-12, "{d2v}");
    }
}
