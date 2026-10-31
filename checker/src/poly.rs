//! Convex polygons in 3D (planar): clipping, area, union area and distance. A patch on the
//! model is a set of such polygons, one per (support part, model triangle).

use crate::solid::HalfSpace;
use crate::v3::{V, add, cross, dot, len, mul, sub, unit};

/// A convex polygon, vertices in order (counter-clockwise about the normal of the triangle
/// it was cut from).
pub type Poly = Vec<V>;

/// Keep the part of `poly` with `n . x <= o` (Sutherland-Hodgman against one half-space).
#[must_use]
pub fn clip(poly: &[V], (n, o): HalfSpace) -> Poly {
    let d: Vec<f64> = poly.iter().map(|p| dot(n, *p) - o).collect();
    if d.iter().all(|x| *x <= 0.0) {
        return poly.to_vec();
    }
    if d.iter().all(|x| *x >= 0.0) {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let j = (i + 1) % poly.len();
        let (a, b, da, db) = (poly[i], poly[j], d[i], d[j]);
        if da <= 0.0 {
            out.push(a);
        }
        if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
            let t = da / (da - db);
            out.push(add(a, mul(sub(b, a), t)));
        }
    }
    out
}

/// Clip by every half-space in turn.
#[must_use]
pub fn clip_all(poly: &[V], hs: &[HalfSpace]) -> Poly {
    let mut cur = poly.to_vec();
    for h in hs {
        if cur.is_empty() {
            break;
        }
        cur = clip(&cur, *h);
    }
    cur
}

/// Area of a planar polygon.
#[must_use]
pub fn area(p: &[V]) -> f64 {
    if p.len() < 3 {
        return 0.0;
    }
    let mut s = [0.0; 3];
    for i in 1..p.len() - 1 {
        s = add(s, cross(sub(p[i], p[0]), sub(p[i + 1], p[0])));
    }
    0.5 * len(s)
}

/// The inward half-spaces of the polygon's edges, as `n . x <= o` pieces, for counter-clockwise
/// order about `normal`.
fn edge_spaces(p: &[V], normal: V) -> Vec<HalfSpace> {
    (0..p.len())
        .filter_map(|i| {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            let out = unit(cross(sub(b, a), normal));
            (len(out) > 0.0).then(|| (out, dot(out, a)))
        })
        .collect()
}

/// The intersection of two convex polygons that lie in one plane with normal `normal`.
#[must_use]
pub fn intersect(a: &[V], b: &[V], normal: V) -> Poly {
    clip_all(a, &edge_spaces(b, normal))
}

/// Most polygons a union is computed for (inclusion-exclusion is 2^k terms).
pub const MAX_UNION: usize = 10;

/// Area of the union of coplanar convex polygons (inclusion-exclusion), or `None` when there
/// are more than [`MAX_UNION`].
#[must_use]
pub fn union_area(polys: &[Poly], normal: V) -> Option<f64> {
    if polys.len() > MAX_UNION {
        return None;
    }
    fn rec(polys: &[Poly], normal: V, start: usize, cur: &[V], sign: f64, acc: &mut f64) {
        for i in start..polys.len() {
            let next = if cur.is_empty() {
                polys[i].clone()
            } else {
                intersect(cur, &polys[i], normal)
            };
            let a = area(&next);
            if a <= 0.0 {
                continue;
            }
            *acc += sign * a;
            rec(polys, normal, i + 1, &next, -sign, acc);
        }
    }
    let mut acc = 0.0;
    rec(polys, normal, 0, &[], 1.0, &mut acc);
    Some(acc)
}

fn seg_seg(p1: V, q1: V, p2: V, q2: V) -> f64 {
    // Ericson, "Real-Time Collision Detection" 5.1.9.
    let d1 = sub(q1, p1);
    let d2 = sub(q2, p2);
    let r = sub(p1, p2);
    let (a, e, f) = (dot(d1, d1), dot(d2, d2), dot(d2, r));
    if a <= 1e-300 && e <= 1e-300 {
        return len(r);
    }
    let c = dot(d1, r);
    let (s, t) = if a <= 1e-300 {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else if e <= 1e-300 {
        ((-c / a).clamp(0.0, 1.0), 0.0)
    } else {
        let b = dot(d1, d2);
        let denom = a * e - b * b;
        let s0 = if denom > 1e-300 {
            ((b * f - c * e) / denom).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let t0 = (b * s0 + f) / e;
        if t0 < 0.0 {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else if t0 > 1.0 {
            (((b - c) / a).clamp(0.0, 1.0), 1.0)
        } else {
            (s0, t0)
        }
    };
    len(sub(add(p1, mul(d1, s)), add(p2, mul(d2, t))))
}

fn point_seg(p: V, a: V, b: V) -> f64 {
    seg_seg(p, p, a, b)
}

/// Distance from a point to a convex polygon (in its own plane).
fn point_poly(p: V, poly: &[V], normal: V) -> f64 {
    let off = dot(normal, sub(p, poly[0]));
    let foot = sub(p, mul(normal, off));
    let inside = (0..poly.len()).all(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        dot(cross(sub(b, a), sub(foot, a)), normal) >= -1e-12
    });
    if inside {
        return off.abs();
    }
    (0..poly.len())
        .map(|i| point_seg(p, poly[i], poly[(i + 1) % poly.len()]))
        .fold(f64::INFINITY, f64::min)
}

fn normal_of(p: &[V]) -> V {
    let mut s = [0.0; 3];
    for i in 1..p.len().saturating_sub(1) {
        s = add(s, cross(sub(p[i], p[0]), sub(p[i + 1], p[0])));
    }
    unit(s)
}

/// An edge of `a` that passes through the interior of `b`.
fn pierces(a: &[V], b: &[V], nb: V) -> bool {
    (0..a.len()).any(|i| {
        let (p, q) = (a[i], a[(i + 1) % a.len()]);
        let (dp, dq) = (dot(nb, sub(p, b[0])), dot(nb, sub(q, b[0])));
        if dp * dq >= 0.0 {
            return false;
        }
        let t = dp / (dp - dq);
        point_poly(add(p, mul(sub(q, p), t)), b, nb) <= 1e-12
    })
}

/// Distance between two convex planar polygons (0 if they touch or overlap).
#[must_use]
pub fn distance(a: &[V], b: &[V]) -> f64 {
    if a.len() < 3 || b.len() < 3 {
        return f64::INFINITY;
    }
    let (na, nb) = (normal_of(a), normal_of(b));
    if pierces(a, b, nb) || pierces(b, a, na) {
        return 0.0;
    }
    let mut best = f64::INFINITY;
    for p in a {
        best = best.min(point_poly(*p, b, nb));
    }
    for p in b {
        best = best.min(point_poly(*p, a, na));
    }
    for i in 0..a.len() {
        for j in 0..b.len() {
            best = best.min(seg_seg(
                a[i],
                a[(i + 1) % a.len()],
                b[j],
                b[(j + 1) % b.len()],
            ));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, s: f64) -> Poly {
        vec![
            [x, y, 0.0],
            [x + s, y, 0.0],
            [x + s, y + s, 0.0],
            [x, y + s, 0.0],
        ]
    }
    const Z: V = [0.0, 0.0, 1.0];

    #[test]
    fn clipping_and_area_of_a_square() {
        let sq = square(0.0, 0.0, 2.0);
        assert!((area(&sq) - 4.0).abs() < 1e-12);
        let half = clip(&sq, ([1.0, 0.0, 0.0], 1.0));
        assert!((area(&half) - 2.0).abs() < 1e-12);
        assert!(clip(&sq, ([1.0, 0.0, 0.0], -1.0)).is_empty());
    }

    #[test]
    fn union_area_by_inclusion_exclusion() {
        let a = square(0.0, 0.0, 2.0);
        let b = square(1.0, 1.0, 2.0);
        let c = square(5.0, 5.0, 1.0);
        let u = union_area(&[a.clone(), b.clone()], Z).expect("small");
        assert!((u - 7.0).abs() < 1e-12, "{u}");
        let u3 = union_area(&[a, b, c], Z).expect("small");
        assert!((u3 - 8.0).abs() < 1e-12, "{u3}");
        assert!(union_area(&vec![square(0.0, 0.0, 1.0); MAX_UNION + 1], Z).is_none());
    }

    #[test]
    fn distances_between_polygons() {
        let a = square(0.0, 0.0, 1.0);
        assert!((distance(&a, &square(1.5, 0.0, 1.0)) - 0.5).abs() < 1e-12);
        assert!(distance(&a, &square(0.5, 0.5, 1.0)).abs() < 1e-12);
        assert!(distance(&a, &square(1.0, 0.0, 1.0)).abs() < 1e-12);
        // A plus sign: no vertex inside the other, but the edges cross.
        let h = vec![
            [-1.0, 0.4, 0.0],
            [2.0, 0.4, 0.0],
            [2.0, 0.6, 0.0],
            [-1.0, 0.6, 0.0],
        ];
        let v = vec![
            [0.4, -1.0, 0.0],
            [0.6, -1.0, 0.0],
            [0.6, 2.0, 0.0],
            [0.4, 2.0, 0.0],
        ];
        assert!(distance(&h, &v).abs() < 1e-12);
        // Different planes: a square above, 0.25 mm up.
        let up: Poly = square(0.0, 0.0, 1.0)
            .into_iter()
            .map(|p| [p[0], p[1], 0.25])
            .collect();
        assert!((distance(&a, &up) - 0.25).abs() < 1e-12);
    }
}
