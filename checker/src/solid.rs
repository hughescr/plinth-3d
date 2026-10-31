//! The convex solids a support part can be: a ball, a cone frustum (analytic shapes from the
//! record fields) or a convex polytope (a record mesh). Each gives its exact distance and
//! the direction of steepest increase, which is a subgradient of the (convex) distance
//! function: that is what the certified gap search in `gap` needs.

use crate::tri::{Tri, closest_point};
use crate::v3::{Aabb, V, add, cross, dist, dot, frame, len, mul, sub};

/// One half-space: `n . x <= o` (`n` a unit vector).
pub type HalfSpace = (V, f64);

#[derive(Debug, Clone)]
pub enum Solid {
    Ball {
        c: V,
        r: f64,
    },
    /// Cone frustum from `a` (radius `ra`) along unit `u` for length `l` (radius `rb`).
    Frust {
        a: V,
        u: V,
        l: f64,
        ra: f64,
        rb: f64,
    },
    /// A closed mesh that validation holds to be convex within a small allowance (it may stand
    /// a few ULPs off exactly convex): outward half-spaces, the slack of each (how far its
    /// plane may be off because the vertices are f32 and the triangle is thin), its triangles
    /// and vertices. Distances to it are taken on its triangles, never from its planes.
    Poly {
        planes: Vec<HalfSpace>,
        slack: Vec<f64>,
        tris: Vec<Tri>,
        verts: Vec<V>,
        /// How far the mesh's own vertices stand outside its triangle planes (0 for an exactly
        /// convex mesh): the real convexity error. Every vertex, so the whole mesh, lies in
        /// the planes moved out by it.
        viol: f64,
        /// The measured deviation of the written vertices from the canonical mesh of the
        /// record's fields (mm; 0 for a mesh built here or not tied to a record). Every bound
        /// of a rule that reads the mesh allows for it, so a verdict holds for the canonical
        /// mesh as well as the written one.
        dev: f64,
    },
}

/// Nearest point on segment `p-q` to `x` (2D), as the point.
fn seg2(x: [f64; 2], p: [f64; 2], q: [f64; 2]) -> [f64; 2] {
    let d = [q[0] - p[0], q[1] - p[1]];
    let dd = d[0] * d[0] + d[1] * d[1];
    let t = if dd > 0.0 {
        (((x[0] - p[0]) * d[0] + (x[1] - p[1]) * d[1]) / dd).clamp(0.0, 1.0)
    } else {
        0.0
    };
    [p[0] + t * d[0], p[1] + t * d[1]]
}

impl Solid {
    /// A frustum from its end points and radii; `None` for a zero-length axis.
    #[must_use]
    pub fn frustum(a: V, b: V, ra: f64, rb: f64) -> Option<Self> {
        let l = dist(a, b);
        (l > 0.0).then(|| Self::Frust {
            a,
            u: mul(sub(b, a), 1.0 / l),
            l,
            ra,
            rb,
        })
    }

    /// A convex polytope from outward-wound triangles.
    ///
    /// Each triangle's plane comes from its three f32 vertices, each good to about
    /// `eps = 3` units in the last place of the largest coordinate (native meshes are within
    /// 1.5). The plane's tilt is uncertain by `2 eps / h` for a triangle whose smallest
    /// altitude is `h`, and that tilt moves the plane by `tilt * reach` at the far vertices
    /// of the solid, so each plane gets `slack = eps + 2 eps reach / h`.
    #[must_use]
    pub fn polytope(tris: Vec<Tri>) -> Self {
        let verts: Vec<V> = tris.iter().flatten().copied().collect();
        let max_abs = verts.iter().flatten().fold(0.0f64, |m, x| m.max(x.abs()));
        let eps = 3.0 * max_abs * f64::from(f32::EPSILON) / 2.0;
        let mut planes = Vec::with_capacity(tris.len());
        let mut slack = Vec::with_capacity(tris.len());
        for t in &tris {
            let c = cross(sub(t[1], t[0]), sub(t[2], t[0]));
            let area2 = len(c);
            let longest = (0..3)
                .map(|k| dist(t[k], t[(k + 1) % 3]))
                .fold(0.0, f64::max);
            if area2 <= 1e-18 || longest <= 0.0 {
                continue;
            }
            let n = mul(c, 1.0 / area2);
            let h = area2 / longest;
            let centre = mul(add(add(t[0], t[1]), t[2]), 1.0 / 3.0);
            let reach = verts.iter().map(|v| dist(*v, centre)).fold(0.0, f64::max);
            planes.push((n, dot(n, t[0])));
            slack.push(eps + 2.0 * eps * reach / h);
        }
        let viol = planes
            .iter()
            .map(|(n, o)| verts.iter().map(|v| dot(*n, *v) - o).fold(0.0, f64::max))
            .fold(0.0, f64::max);
        Self::Poly {
            planes,
            slack,
            tris,
            verts,
            viol,
            dev: 0.0,
        }
    }

    /// The same polytope with the deviation of its vertices from the record's canonical mesh
    /// (mm) recorded; no effect on any other solid.
    #[must_use]
    pub fn with_dev(mut self, deviation: f64) -> Self {
        if let Self::Poly { dev, .. } = &mut self {
            *dev = deviation;
        }
        self
    }

    /// The recorded deviation of a mesh from its canonical mesh (0 for any other solid).
    #[must_use]
    pub fn dev(&self) -> f64 {
        match self {
            Self::Poly { dev, .. } => *dev,
            _ => 0.0,
        }
    }

    /// Distance from `p` to the solid (0 inside) and the unit direction away from the nearest
    /// point (the zero vector inside).
    #[must_use]
    pub fn dist_grad(&self, p: V) -> (f64, V) {
        match self {
            Self::Ball { c, r } => {
                let d = dist(p, *c);
                if d <= *r {
                    (0.0, [0.0; 3])
                } else {
                    (d - r, mul(sub(p, *c), 1.0 / d))
                }
            }
            Self::Frust { a, u, l, ra, rb } => frustum_dist(p, *a, *u, *l, *ra, *rb),
            Self::Poly { planes, tris, .. } => {
                if planes.iter().all(|(n, o)| dot(*n, p) <= *o) {
                    return (0.0, [0.0; 3]);
                }
                let mut best = (f64::INFINITY, p);
                for t in tris {
                    let q = closest_point(p, t);
                    let d = dist(p, q);
                    if d < best.0 {
                        best = (d, q);
                    }
                }
                if best.0 > 0.0 {
                    (best.0, mul(sub(p, best.1), 1.0 / best.0))
                } else {
                    (0.0, [0.0; 3])
                }
            }
        }
    }

    /// The smallest and largest value of `n . x` over the solid (`n` a unit vector).
    #[must_use]
    pub fn support(&self, n: V) -> (f64, f64) {
        match self {
            Self::Ball { c, r } => (dot(n, *c) - r, dot(n, *c) + r),
            Self::Frust { a, u, l, ra, rb } => {
                let nu = dot(n, *u);
                let perp = (1.0 - nu * nu).max(0.0).sqrt();
                let b = add(*a, mul(*u, *l));
                let (na, nb) = (dot(n, *a), dot(n, b));
                (
                    (na - ra * perp).min(nb - rb * perp),
                    (na + ra * perp).max(nb + rb * perp),
                )
            }
            Self::Poly { verts, .. } => verts
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                    (lo.min(dot(n, *v)), hi.max(dot(n, *v)))
                }),
        }
    }

    /// A box that contains the solid.
    #[must_use]
    pub fn aabb(&self) -> Aabb {
        match self {
            Self::Ball { c, r } => Aabb::of_points([sub(*c, [*r; 3]), add(*c, [*r; 3])]),
            Self::Frust { a, u, l, ra, rb } => {
                let b = add(*a, mul(*u, *l));
                let r = ra.max(*rb);
                Aabb::of_points([a, &b].map(|p| *p)).inflated(r)
            }
            Self::Poly { verts, .. } => Aabb::of_points(verts.iter().copied()),
        }
    }
}

fn frustum_dist(p: V, a: V, u: V, l: f64, ra: f64, rb: f64) -> (f64, V) {
    let ap = sub(p, a);
    let t = dot(ap, u);
    let q = sub(ap, mul(u, t));
    let rho = len(q);
    let on_axis = ra + (rb - ra) * t / l;
    if (0.0..=l).contains(&t) && rho <= on_axis {
        return (0.0, [0.0; 3]);
    }
    let x = [t, rho];
    let cands = [
        seg2(x, [0.0, 0.0], [0.0, ra]),
        seg2(x, [0.0, ra], [l, rb]),
        seg2(x, [l, rb], [l, 0.0]),
    ];
    let best = cands
        .iter()
        .copied()
        .min_by(|p1, p2| {
            let d1 = (p1[0] - x[0]).hypot(p1[1] - x[1]);
            let d2 = (p2[0] - x[0]).hypot(p2[1] - x[1]);
            d1.total_cmp(&d2)
        })
        .unwrap_or([0.0, 0.0]);
    let w = if rho > 1e-300 {
        mul(q, 1.0 / rho)
    } else {
        frame(u).0
    };
    let near = add(add(a, mul(u, best[0])), mul(w, best[1]));
    let d = dist(p, near);
    if d > 0.0 {
        (d, mul(sub(p, near), 1.0 / d))
    } else {
        (0.0, [0.0; 3])
    }
}

/// The half-spaces of a polytope with `sides` lateral faces that lies just inside
/// (`inner = true`, chords of the circles) or just outside (tangent planes) the cone frustum
/// from `a` along unit `u`, length `l`, radii `ra` and `rb`, with flat ends.
#[must_use]
pub fn frustum_polytope(
    (a, u, l): (V, V, f64),
    (ra, rb): (f64, f64),
    sides: usize,
    inner: bool,
) -> Vec<HalfSpace> {
    let (e1, e2) = frame(u);
    let k = if inner {
        (std::f64::consts::PI / sides as f64).cos()
    } else {
        1.0
    };
    let s = (rb - ra) / l;
    let mut out = Vec::with_capacity(sides + 2);
    for j in 0..sides {
        let phi = std::f64::consts::TAU * (j as f64 + 0.5) / sides as f64;
        let w = add(mul(e1, phi.cos()), mul(e2, phi.sin()));
        let nv = sub(w, mul(u, k * s));
        let nn = len(nv);
        let n = mul(nv, 1.0 / nn);
        out.push((n, (dot(nv, a) + k * ra) / nn));
    }
    out.push((mul(u, -1.0), -dot(u, a)));
    out.push((u, dot(u, add(a, mul(u, l)))));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cyl() -> Solid {
        Solid::frustum([0.0; 3], [0.0, 0.0, 2.0], 0.5, 0.5).expect("axis")
    }

    #[test]
    fn ball_and_frustum_distances_are_exact() {
        let b = Solid::Ball {
            c: [0.0; 3],
            r: 1.0,
        };
        assert!((b.dist_grad([3.0, 0.0, 0.0]).0 - 2.0).abs() < 1e-12);
        assert!(b.dist_grad([0.5, 0.0, 0.0]).0.abs() < 1e-12);
        let c = cyl();
        assert!((c.dist_grad([1.5, 0.0, 1.0]).0 - 1.0).abs() < 1e-12);
        assert!((c.dist_grad([0.0, 0.0, 3.0]).0 - 1.0).abs() < 1e-12);
        let corner = c.dist_grad([1.0, 0.0, 3.0]).0;
        assert!((corner - (0.5f64 * 0.5 + 1.0).sqrt()).abs() < 1e-12);
        assert!(c.dist_grad([0.2, 0.1, 1.0]).0.abs() < 1e-12);
        // The gradient points away from the solid.
        let (d, g) = c.dist_grad([1.5, 0.0, 1.0]);
        assert!(d > 0.0 && (g[0] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn polytope_distance_matches_a_box() {
        let q = |a: V, b: V, c: V| [a, b, c];
        // Unit cube [0,1]^3, outward winding.
        let p = |i: u8| {
            [
                f64::from(i & 1),
                f64::from((i >> 1) & 1),
                f64::from((i >> 2) & 1),
            ]
        };
        let quads = [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ];
        let mut tris = Vec::new();
        for f in quads {
            tris.push(q(p(f[0]), p(f[1]), p(f[2])));
            tris.push(q(p(f[0]), p(f[2]), p(f[3])));
        }
        let s = Solid::polytope(tris);
        assert!((s.dist_grad([2.0, 0.5, 0.5]).0 - 1.0).abs() < 1e-12);
        assert!(s.dist_grad([0.5, 0.5, 0.5]).0.abs() < 1e-12);
        let d = s.dist_grad([2.0, 2.0, 0.5]).0;
        assert!((d - 2.0f64.sqrt()).abs() < 1e-12);
        let (lo, hi) = s.support([1.0, 0.0, 0.0]);
        assert!((lo - 0.0).abs() < 1e-12 && (hi - 1.0).abs() < 1e-12);
    }

    #[test]
    fn frustum_polytopes_bracket_the_cone() {
        let (a, u, l) = ([0.0; 3], [0.0, 0.0, 1.0], 2.0);
        let inner = frustum_polytope((a, u, l), (0.3, 0.6), 360, true);
        let outer = frustum_polytope((a, u, l), (0.3, 0.6), 360, false);
        let inside = |hs: &[HalfSpace], p: V| hs.iter().all(|(n, o)| dot(*n, p) <= *o);
        // A point on the cone surface (radius 0.45 at z = 1) is inside the outer polytope
        // and outside or on the inner one; a point at 0.99 of the radius is inside both.
        assert!(inside(&outer, [0.45, 0.0, 1.0]));
        assert!(inside(&inner, [0.45 * 0.9999, 0.0, 1.0]));
        assert!(!inside(&inner, [0.45 * 1.0001, 0.0, 1.0]));
        assert!(!inside(&outer, [0.45 * 1.01, 0.0, 1.0]));
    }
}
