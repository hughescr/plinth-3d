//! Certified distance between a solid and a triangle.
//!
//! * **A mesh** (what the verdicts are measured on) is measured on its written triangles:
//!   the smallest triangle-to-triangle distance, with a stated rounding allowance. Nothing
//!   about the mesh is assumed beyond what the bounds themselves say, so a mesh that validation
//!   lets stand a few ULPs off convex is bounded as written.
//! * **An analytic convex solid** (informational shapes only): `f(p)` = distance from `p` to
//!   the solid is convex, so at any point `c` the tangent plane `f(c) + g . (p - c)` (with `g`
//!   the unit direction away from the solid) lies under `f` everywhere. Over a sub-triangle
//!   that plane is smallest at a vertex, which gives a lower bound for the whole sub-triangle;
//!   `f` at sampled points gives an upper bound. The search splits the sub-triangle with the
//!   smallest lower bound until the answer is decided or the interval is narrow enough. Both
//!   bounds are plain f64 arithmetic: the rounding error (about 1e-15 relative) is far below
//!   every tolerance this checker states.

use crate::solid::{HalfSpace, Solid};
use crate::tri::{Tri, allowance_mm, box_dist, point_in_mesh, tri_box, tri_tri_dist};
use crate::v3::{V, centroid, dot, lerp, sub};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// What the search is for.
#[derive(Debug, Clone, Copy)]
pub struct Goal {
    /// Stop once the lower bound reaches this (the part is certified clear).
    pub clear_at: f64,
    /// Stop once the upper bound is below this (the part is certified too close).
    pub violation_below: f64,
    /// Stop once the interval is no wider than this.
    pub width: f64,
}

/// A certified interval for the distance, and how the search ended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gap {
    pub lb: f64,
    pub ub: f64,
    pub evals: u32,
    /// The work limit was reached before the goal.
    pub exhausted: bool,
}

const MAX_EVALS: u32 = 6000;

struct Node {
    lb: f64,
    v: [V; 3],
}

impl PartialEq for Node {
    fn eq(&self, o: &Self) -> bool {
        self.lb.total_cmp(&o.lb) == Ordering::Equal
    }
}
impl Eq for Node {}
impl PartialOrd for Node {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Node {
    /// Reversed, so the heap pops the smallest lower bound.
    fn cmp(&self, o: &Self) -> Ordering {
        o.lb.total_cmp(&self.lb)
    }
}

/// Lower bound over the triangle `v` from the tangent plane at `c`.
fn tangent_lb(c: V, fc: f64, g: V, v: &[V; 3]) -> f64 {
    v.iter()
        .map(|p| fc + dot(g, sub(*p, c)))
        .fold(f64::INFINITY, f64::min)
        .max(0.0)
}

/// Distance samples of one search: the solid, the count and the best (smallest) value.
struct Probe<'a> {
    solid: &'a Solid,
    evals: u32,
    ub: f64,
}

impl Probe<'_> {
    fn eval(&mut self, p: V) -> (f64, V) {
        self.evals += 1;
        let (d, g) = self.solid.dist_grad(p);
        self.ub = self.ub.min(d);
        (d, g)
    }
}

/// Certified distance interval between `solid` and triangle `t`.
///
/// A mesh (`Solid::Poly`) is measured on its triangles themselves, exactly (the smallest
/// triangle-to-triangle distance, `tri::tri_tri_dist`): no convexity of the mesh is assumed,
/// so a mesh that stands a few ULPs off convex (which validation allows) is bounded as it is
/// written. The analytic solids (informational shapes) use the convex search.
#[must_use]
pub fn solid_tri(solid: &Solid, t: &Tri, goal: Goal) -> Gap {
    match solid {
        Solid::Poly {
            planes,
            tris,
            viol,
            dev,
            ..
        } => mesh_tri(planes, tris, (*viol, *dev), t),
        _ => convex_tri(solid, t, goal),
    }
}

/// The distance from the written mesh `tris` (outward planes `planes`, vertices standing at
/// most `viol` outside them, and at most `dev` from the record's canonical mesh) to triangle
/// `t`, widened on both sides by the arithmetic allowance of `tri::allowance_mm` (at the
/// largest coordinate involved: the f64 arithmetic is good to about 1e-12 mm here and
/// near-parallel edges lose a few times 1e-8 mm, and the allowance grows with the coordinates,
/// so a file far from the origin is not given a fixed small one) and by the record's measured
/// deviation `dev` (a canonical vertex is within `dev` of the written one, so the canonical
/// mesh is within `dev` of the written one and the distance moves by at most that).
fn mesh_tri(planes: &[HalfSpace], tris: &[Tri], (viol, dev): (f64, f64), t: &Tri) -> Gap {
    let tb = tri_box(t);
    let scale = tris
        .iter()
        .flatten()
        .chain(t)
        .flatten()
        .fold(0.0f64, |m, x| m.max(x.abs()));
    let rounding = allowance_mm(scale) + dev;
    let mut best = f64::INFINITY;
    let mut evals = 0u32;
    for m in tris {
        // A mesh triangle farther than the best so far (by its box) cannot lower the minimum.
        if box_dist(&tri_box(m), &tb) >= best {
            continue;
        }
        evals += 1;
        best = best.min(tri_tri_dist(m, t));
    }
    let mut out = Gap {
        lb: (best - rounding).max(0.0),
        ub: best + rounding,
        evals,
        exhausted: false,
    };
    // A triangle that never meets the surface lies wholly outside or wholly inside the mesh.
    // Every point of the mesh is in its planes moved out by `viol`, so a centroid outside that
    // is outside the mesh; a centroid inside it is decided by crossing parity.
    let c = centroid(t);
    if out.lb > 0.0
        && planes
            .iter()
            .all(|(n, o)| dot(*n, c) <= o + viol + rounding)
    {
        match point_in_mesh(c, tris) {
            Some(false) => {}
            Some(true) => {
                out.lb = 0.0;
                out.ub = rounding;
            }
            None => out.lb = 0.0,
        }
    }
    out
}

fn convex_tri(solid: &Solid, t: &Tri, goal: Goal) -> Gap {
    let mut probe = Probe {
        solid,
        evals: 0,
        ub: f64::INFINITY,
    };
    let mut heap = BinaryHeap::new();
    // The root: tangent planes at the centroid and the three vertices.
    let mut best_root = 0.0f64;
    for p in [centroid(t), t[0], t[1], t[2]] {
        let (d, g) = probe.eval(p);
        best_root = best_root.max(tangent_lb(p, d, g, t));
    }
    heap.push(Node {
        lb: best_root,
        v: *t,
    });
    loop {
        let Some(node) = heap.pop() else {
            return Gap {
                lb: probe.ub,
                ub: probe.ub,
                evals: probe.evals,
                exhausted: false,
            };
        };
        let (glb, ub) = (node.lb, probe.ub);
        let done = glb >= goal.clear_at || ub < goal.violation_below || ub - glb <= goal.width;
        if done || probe.evals >= MAX_EVALS {
            return Gap {
                lb: glb.min(ub),
                ub,
                evals: probe.evals,
                exhausted: !done,
            };
        }
        let [a, b, c] = node.v;
        let (ab, bc, ca) = (lerp(a, b, 0.5), lerp(b, c, 0.5), lerp(c, a, 0.5));
        for child in [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]] {
            let cc = centroid(&child);
            let (d, g) = probe.eval(cc);
            let own = tangent_lb(cc, d, g, &child);
            heap.push(Node {
                lb: own.max(node.lb),
                v: child,
            });
        }
        // The midpoints are cheap extra samples for the upper bound.
        for p in [ab, bc, ca] {
            let _ = probe.eval(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal(clear: f64, viol: f64) -> Goal {
        Goal {
            clear_at: clear,
            violation_below: viol,
            width: 0.0025,
        }
    }

    #[test]
    fn a_cylinder_cap_over_a_perpendicular_plane_certifies_without_exhausting() {
        // Radius 0.25 cylinder pointing at the plane z = 0; its end cap is 0.33 above it.
        let cyl = Solid::frustum([0.0, 0.0, 5.0], [0.0, 0.0, 0.33], 0.25, 0.25).expect("axis");
        let tri: Tri = [[-3.0, -3.0, 0.0], [4.0, -2.0, 0.0], [0.0, 5.0, 0.0]];
        let g = solid_tri(&cyl, &tri, goal(0.3, 0.3));
        assert!(!g.exhausted, "{g:?}");
        assert!(g.lb >= 0.3 && g.ub <= 0.3300001 && g.ub >= 0.32999, "{g:?}");
    }

    #[test]
    fn a_ball_near_a_triangle_edge_gives_a_tight_interval() {
        let ball = Solid::Ball {
            c: [0.0, 0.0, 1.0],
            r: 0.4,
        };
        let tri: Tri = [[1.0, -1.0, 0.0], [2.0, 1.0, 0.0], [1.0, 2.0, 0.0]];
        // Nearest point of the triangle to the centre is on the edge x = 1.0 -> distance
        // sqrt(1 + 1) = 1.4142, minus the radius.
        let g = solid_tri(
            &ball,
            &tri,
            Goal {
                clear_at: 10.0,
                violation_below: 0.0,
                width: 1e-4,
            },
        );
        let truth = 2.0f64.sqrt() - 0.4;
        assert!(
            g.lb <= truth + 1e-12 && g.ub >= truth - 1e-12,
            "{g:?} {truth}"
        );
        assert!(g.ub - g.lb <= 1e-4 + 1e-12, "{g:?}");
    }

    /// Smallest distance from the solid to the triangle by dense sampling (test only).
    fn brute(solid: &Solid, t: &Tri, n: u32) -> f64 {
        let mut best = f64::INFINITY;
        for i in 0..=n {
            for j in 0..=(n - i) {
                let (u, v) = (f64::from(i) / f64::from(n), f64::from(j) / f64::from(n));
                let p = [
                    t[0][0] + u * (t[1][0] - t[0][0]) + v * (t[2][0] - t[0][0]),
                    t[0][1] + u * (t[1][1] - t[0][1]) + v * (t[2][1] - t[0][1]),
                    t[0][2] + u * (t[1][2] - t[0][2]) + v * (t[2][2] - t[0][2]),
                ];
                best = best.min(solid.dist_grad(p).0);
            }
        }
        best
    }

    #[test]
    fn a_tilted_frustum_and_an_oblique_end_cap_near_a_90_degree_edge_match_dense_sampling() {
        // Two faces meeting at the edge x = 0, z = 0 at 90 degrees, and tilted frusta whose
        // end caps and rims come near it from different directions.
        let faces: [Tri; 2] = [
            [[-3.0, -3.0, 0.0], [0.0, 3.0, 0.0], [0.0, -3.0, 0.0]],
            [[0.0, -3.0, 0.0], [0.0, 3.0, 0.0], [0.0, 3.0, 3.0]],
        ];
        for (a, b, ra, rb) in [
            ([0.6, 0.0, 0.9], [1.4, 0.3, 2.0], 0.25, 0.4),
            ([0.5, -0.2, 0.7], [0.9, 0.4, 1.3], 0.3, 0.1),
            ([-0.4, 0.1, 0.8], [-1.5, 0.5, 1.6], 0.2, 0.2),
        ] {
            let solid = Solid::frustum(a, b, ra, rb).expect("axis");
            for tri in &faces {
                let truth = brute(&solid, tri, 400);
                let g = solid_tri(
                    &solid,
                    tri,
                    Goal {
                        clear_at: 10.0,
                        violation_below: 0.0,
                        width: 0.0025,
                    },
                );
                // The dense sample is itself an upper bound of the true minimum, so the lower
                // bound must lie below it, and (the sample being fine) not far below.
                assert!(
                    g.lb <= truth + 1e-9,
                    "lb {} above the sampled {truth}",
                    g.lb
                );
                assert!(truth - g.lb <= 0.0025 + 0.01, "{g:?} vs {truth}");
                assert!(g.ub >= g.lb && g.ub - g.lb <= 0.0025 + 1e-9, "{g:?}");
                assert!(!g.exhausted, "{g:?}");
            }
        }
    }

    #[test]
    fn a_mesh_is_measured_triangle_to_triangle_outside_across_and_inside() {
        let cube = Solid::polytope(crate::testkit::closed_box([0.0; 3], [2.0; 3]));
        let g = goal(0.3, 0.3);
        let eps = 2.0 * allowance_mm(3.0);
        // 0.3 above the top face, wholly outside: the exact distance within the allowance.
        let above: Tri = [[0.2, 0.2, 2.3], [1.5, 0.3, 2.3], [0.4, 1.7, 2.3]];
        let o = solid_tri(&cube, &above, g);
        assert!(
            o.lb <= 0.3 && 0.3 <= o.ub && o.ub - o.lb <= eps + 1e-9,
            "{o:?}"
        );
        // Crossing the top face: distance 0.
        let across: Tri = [[0.2, 0.2, 1.0], [1.5, 0.3, 3.0], [0.4, 1.7, 3.0]];
        let o = solid_tri(&cube, &across, g);
        assert!(
            o.lb.abs() < 1e-12 && o.ub <= allowance_mm(3.0) + 1e-9,
            "{o:?}"
        );
        // Wholly inside, touching nothing: 0, decided by crossing parity.
        let inside: Tri = [[0.5, 0.5, 0.5], [1.5, 0.5, 0.5], [0.5, 1.5, 0.6]];
        let o = solid_tri(&cube, &inside, g);
        assert!(
            o.lb.abs() < 1e-12 && o.ub <= allowance_mm(3.0) + 1e-9,
            "{o:?}"
        );
        // Beside the cube and far: the bound is the distance (1.0 from the x = 2 face).
        let beside: Tri = [[3.0, 0.5, 0.5], [3.0, 1.5, 0.5], [3.0, 0.5, 1.5]];
        let o = solid_tri(&cube, &beside, g);
        assert!(
            o.lb <= 1.0 && 1.0 <= o.ub && o.ub - o.lb <= eps + 1e-9,
            "{o:?}"
        );
    }

    #[test]
    fn a_record_deviation_widens_both_ends_of_a_mesh_distance() {
        // Ruling: every bound carries the record's measured deviation from its canonical mesh.
        let dev = 1e-3;
        let cube = Solid::polytope(crate::testkit::closed_box([0.0; 3], [2.0; 3])).with_dev(dev);
        let above: Tri = [[0.2, 0.2, 2.3], [1.5, 0.3, 2.3], [0.4, 1.7, 2.3]];
        let o = solid_tri(&cube, &above, goal(0.3, 0.3));
        assert!(
            o.lb <= 0.3 - dev && 0.3 + dev <= o.ub,
            "the interval must hold the distance 0.3 +- the deviation: {o:?}"
        );
        // A wholly-inside triangle is within the deviation of the canonical surface.
        let inside: Tri = [[0.5, 0.5, 0.5], [1.5, 0.5, 0.5], [0.5, 1.5, 0.6]];
        let o = solid_tri(&cube, &inside, goal(0.3, 0.3));
        assert!(o.lb == 0.0 && o.ub >= dev, "{o:?}");
    }

    #[test]
    fn a_triangle_through_the_solid_has_distance_zero() {
        let ball = Solid::Ball {
            c: [0.2, 0.2, 0.1],
            r: 0.3,
        };
        let tri: Tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let g = solid_tri(&ball, &tri, goal(0.3, 0.3));
        assert!(g.ub.abs() < 1e-12 && g.lb.abs() < 1e-12, "{g:?}");
    }
}
