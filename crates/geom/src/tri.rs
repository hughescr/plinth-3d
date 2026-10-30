//! The point-to-triangle kernel. Both the BVH query and the scalar reference use this one
//! function, so the batched result can equal the reference exactly.

pub(crate) type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `a + d * s`.
fn along(a: V3, d: V3, s: f64) -> V3 {
    [a[0] + d[0] * s, a[1] + d[1] * s, a[2] + d[2] * s]
}

/// Squared distance between two points.
pub(crate) fn dist2(a: V3, b: V3) -> f64 {
    let d = sub(a, b);
    dot(d, d)
}

fn closest_on_segment(p: V3, a: V3, b: V3) -> V3 {
    let ab = sub(b, a);
    let len2 = dot(ab, ab);
    if len2 <= 0.0 {
        return a;
    }
    let t = (dot(sub(p, a), ab) / len2).clamp(0.0, 1.0);
    along(a, ab, t)
}

/// Closest of the three edges; the fallback for degenerate (zero-area) triangles.
fn closest_on_edges(p: V3, a: V3, b: V3, c: V3) -> V3 {
    let mut best = closest_on_segment(p, a, b);
    let mut best_d2 = dist2(p, best);
    for q in [closest_on_segment(p, b, c), closest_on_segment(p, c, a)] {
        let d2 = dist2(p, q);
        if d2 < best_d2 {
            best = q;
            best_d2 = d2;
        }
    }
    best
}

/// The point of triangle `abc` closest to `p` (Ericson, Real-Time Collision Detection
/// 5.1.5: vertex, edge, then face regions). Zero-area triangles fall back to their edges.
pub(crate) fn closest_on_triangle(p: V3, a: V3, b: V3, c: V3) -> V3 {
    let r = region_walk(p, a, b, c);
    if r.iter().all(|v| v.is_finite()) {
        r
    } else {
        closest_on_edges(p, a, b, c)
    }
}

fn region_walk(p: V3, a: V3, b: V3, c: V3) -> V3 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let d1 = dot(ab, sub(p, a));
    let d2 = dot(ac, sub(p, a));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return along(a, ab, d1 / (d1 - d3));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return along(a, ac, d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return along(b, sub(c, b), w);
    }
    let denom = va + vb + vc;
    if denom <= 0.0 {
        return [f64::NAN; 3];
    }
    let v = vb / denom;
    let w = vc / denom;
    along(along(a, ab, v), ac, w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::assert_pt;

    const A: V3 = [0.0, 0.0, 0.0];
    const B: V3 = [4.0, 0.0, 0.0];
    const C: V3 = [0.0, 4.0, 0.0];

    #[test]
    fn every_region_of_a_right_triangle() {
        // Face interior, from above and below.
        assert_pt(
            closest_on_triangle([1.0, 1.0, 3.0], A, B, C),
            [1.0, 1.0, 0.0],
        );
        assert_pt(
            closest_on_triangle([1.0, 1.0, -3.0], A, B, C),
            [1.0, 1.0, 0.0],
        );
        // Vertex regions.
        assert_pt(closest_on_triangle([-1.0, -1.0, 2.0], A, B, C), A);
        assert_pt(closest_on_triangle([6.0, -1.0, 0.0], A, B, C), B);
        assert_pt(closest_on_triangle([-1.0, 6.0, 0.0], A, B, C), C);
        // Edge regions: AB, AC, BC.
        assert_pt(
            closest_on_triangle([2.0, -3.0, 0.0], A, B, C),
            [2.0, 0.0, 0.0],
        );
        assert_pt(
            closest_on_triangle([-3.0, 2.0, 0.0], A, B, C),
            [0.0, 2.0, 0.0],
        );
        assert_pt(
            closest_on_triangle([4.0, 4.0, 0.0], A, B, C),
            [2.0, 2.0, 0.0],
        );
        // A point on the triangle is its own closest point.
        assert_pt(
            closest_on_triangle([1.0, 2.0, 0.0], A, B, C),
            [1.0, 2.0, 0.0],
        );
    }

    #[test]
    fn degenerate_triangles_use_their_edges() {
        // Collinear: the segment from (0,0,0) to (4,0,0) with a repeated middle point.
        let m = [2.0, 0.0, 0.0];
        assert_pt(
            closest_on_triangle([3.0, 5.0, 0.0], A, m, B),
            [3.0, 0.0, 0.0],
        );
        assert_pt(closest_on_triangle([9.0, 1.0, 0.0], A, m, B), B);
        // All three corners equal.
        assert_pt(closest_on_triangle([1.0, 1.0, 1.0], B, B, B), B);
        // Two equal corners.
        assert_pt(
            closest_on_triangle([2.0, 2.0, 0.0], A, A, B),
            [2.0, 0.0, 0.0],
        );
    }
}
