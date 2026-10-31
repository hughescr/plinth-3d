//! Convex solid separation by support planes. Proposed directions are not proofs:
//! only outward-rounded projections of the complete finite solids establish a gap.
use super::{ARITHMETIC_MM, Solid, valid_point};
use crate::patch::{dot, sub};

fn endpoints(s: Solid) -> ([f64; 3], [f64; 3]) {
    match s {
        Solid::Ball(b) => (b.centre, b.centre),
        Solid::Frustum { start, end, .. } => (start, end),
    }
}
fn extent(s: Solid, n: [f64; 3]) -> Option<(f64, f64)> {
    let (a, b) = endpoints(s);
    if !valid_point(a) || !valid_point(b) {
        return None;
    }
    let (ra, rb, radial) = match s {
        Solid::Ball(b) => (b.radius, b.radius, 1.0),
        Solid::Frustum { r_start, r_end, .. } => {
            let d = sub(b, a);
            let length = dot(d, d).sqrt();
            if length <= 1e-10 {
                return None;
            }
            let axis = d.map(|v| v / length);
            // Cross-product avoids cancellation near parallel axes.
            let x = crate::patch::cross(axis, n);
            (r_start, r_end, dot(x, x).sqrt())
        }
    };
    if !ra.is_finite() || !rb.is_finite() || ra.min(rb) <= 0.0 || ra.max(rb) > 10.0 {
        return None;
    }
    Some((
        (dot(a, n) - ra * radial).min(dot(b, n) - rb * radial),
        (dot(a, n) + ra * radial).max(dot(b, n) + rb * radial),
    ))
}
fn project(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    let d = sub(b, a);
    let len2 = dot(d, d);
    let t = if len2 > 1e-20 {
        (dot(sub(p, a), d) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    std::array::from_fn(|k| a[k] + t * d[k])
}
/// Conservative distance lower bound for actual balls and finite capped frusta.
/// Inputs must already include any representation envelope. Unresolved or invalid
/// geometry returns zero, never a clearance pass. No iterative convergence claim.
#[must_use]
pub fn solid_pair_lower(a: Solid, b: Solid) -> f64 {
    let (a0, a1) = endpoints(a);
    let (b0, b1) = endpoints(b);
    let u = sub(a1, a0);
    let v = sub(b1, b0);
    let mut directions = vec![
        sub(a0, b0),
        sub(a0, b1),
        sub(a1, b0),
        sub(a1, b1),
        u,
        v,
        crate::patch::cross(u, v),
    ];
    directions.extend([
        sub(a0, project(a0, b0, b1)),
        sub(a1, project(a1, b0, b1)),
        sub(project(b0, a0, a1), b0),
        sub(project(b1, a0, a1), b1),
    ]);
    let aa = dot(u, u);
    let bb = dot(u, v);
    let cc = dot(v, v);
    let w = sub(a0, b0);
    let dd = dot(u, w);
    let ee = dot(v, w);
    let det = aa * cc - bb * bb;
    if det > 1e-12 * aa * cc {
        let s = (bb * ee - cc * dd) / det;
        let t = (aa * ee - bb * dd) / det;
        directions.push(std::array::from_fn(|k| w[k] + s * u[k] - t * v[k]));
    }
    let mut best = 0.0_f64;
    for d in directions {
        let length = dot(d, d).sqrt();
        if !length.is_finite() || length < 1e-12 {
            continue;
        }
        let n = d.map(|v| v / length);
        let (Some((alo, ahi)), Some((blo, bhi))) = (extent(a, n), extent(b, n)) else {
            return 0.0;
        };
        // <=1000 mm coordinates, <100 operations at these magnitudes. 1e-7
        // encloses normalization and projection rounding, in addition to envelopes.
        best = best.max((alo - bhi).max(blo - ahi) - 10.0 * ARITHMETIC_MM);
    }
    best.max(0.0)
}
/// Ordered batch API; scalar reference above is also the admission kernel.
pub fn solid_pair_lowers(pairs: &[(Solid, Solid)], out: &mut Vec<f64>) {
    out.clear();
    out.extend(pairs.iter().map(|&(a, b)| solid_pair_lower(a, b)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::Ball;
    #[test]
    fn boundaries_and_finite_caps_are_not_capsules() {
        for required in [0.012, 0.32] {
            for delta in [-0.0001, 0.0001] {
                let gap = required + delta;
                let a = Solid::Ball(Ball {
                    centre: [0.0; 3],
                    radius: 0.4,
                });
                let b = Solid::Ball(Ball {
                    centre: [0.8 + gap, 0.0, 0.0],
                    radius: 0.4,
                });
                let lower = solid_pair_lower(a, b);
                assert!(lower <= gap && gap - lower < 1e-6);
                assert_eq!(lower >= required, delta > 0.0);
                let rod = Solid::Frustum {
                    start: [0.0, 0.0, 0.4 + gap],
                    end: [0.0, 0.0, 4.0],
                    r_start: 0.6,
                    r_end: 0.6,
                };
                let lower = solid_pair_lower(a, rod);
                assert!(lower <= gap && gap - lower < 1e-6, "{lower} vs {gap}");
                assert_eq!(lower >= required, delta > 0.0);
            }
        }
    }
    #[test]
    fn region_interiors_must_not_overlap() {
        for (gap, separate) in [(-0.0001, false), (0.0001, true)] {
            let ball = |x| {
                Solid::Ball(Ball {
                    centre: [x, 0.0, 0.0],
                    radius: 0.2,
                })
            };
            assert_eq!(
                solid_pair_lower(ball(0.0), ball(0.4 + gap)) >= 1e-7,
                separate
            );
        }
    }
    #[test]
    fn batch_matches_scalar_and_crossing_solids_cannot_pass() {
        let rod = |x, z| Solid::Frustum {
            start: [x, 0.0, z],
            end: [x, 0.0, z + 1.84],
            r_start: 0.39,
            r_end: 0.585,
        };
        let pairs = [
            (rod(0.0, 0.0), rod(1.9, 0.0)),
            (rod(0.0, 0.0), rod(0.1, 0.0)),
        ];
        let mut out = Vec::new();
        solid_pair_lowers(&pairs, &mut out);
        assert!(out[0] > 0.32);
        assert!(out[1] <= 0.0);
        assert_eq!(out, pairs.map(|(a, b)| solid_pair_lower(a, b)));
    }
}
