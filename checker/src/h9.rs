//! H9: no load-bearing support part thinner than 0.1 mm across.
//!
//! **Verdict scope.** Every width is measured on the written triangles; the verdict is valid
//! for ChituBox v1.4.0 if it slices the stored support meshes, pending the H8 differential
//! slice test. The solid the fields name (a circle of the field radius for a rod, a sphere for
//! a ball, concentric discs along an axis for the union of a cone and balls on it) is measured
//! for information only: where its width class (wide, thin, undecided) differs from the written
//! mesh's, the record is listed in the report as `analytic_information`, and nothing is decided
//! by it. The joins of parts not on one axis are established on the written meshes. No
//! envelope growth stands in for contact: `TOUCH_MM` is an arithmetic allowance (1e-9 mm) for
//! faces on one plane, so faces that meet exactly join and a real gap, however small an
//! end-face gap, does not.
//!
//! Every answer is an interval with a
//! certified lower end (`lb`, never above the true minimum width) and an upper end (`ub`, the
//! width of a real section or direction, never below it). A part passes only when `lb` reaches
//! the limit, is a violation only when `ub` is under it, and is unverified in between or when
//! its shape is not one a measurement here covers. Nothing passes on a field radius alone: the
//! polygons the file holds are measured too. Both ends also allow for the record's own measured
//! deviation from its canonical reconstruction (`Solid::dev`, set from `canon::deviation_mm`)
//! and for the arithmetic (`tri::allowance_mm`): a width within that of the limit is
//! unverified, never decided.
//!
//! * **Frustum, alone or with balls on its axis.** The written mesh is the hull of its 32
//!   vertices (two rings of 16, taken as written, never as the canonical rings). The plane
//!   at axial position `t` crosses the segment from ring vertex `j` to its partner `j` of the
//!   other ring at one point `Q_j(t)`, which moves in a straight line with `t`; the hull of the
//!   `Q_j(t)` lies in the section, so its width bounds the section's width from below. Over an
//!   axial interval the polygon moves at most `g` (the fastest `Q_j` speed, measured from the
//!   written vertices) per unit of `t`, so over `[t0, t1]` no section is narrower than
//!   `width(Q(mid)) - g (t1 - t0)`. That holds for any twist or offset of the rings the
//!   validation allows: the rings are the written ones. The mesh is a hull only up to `viol`
//!   (how far its vertices stand outside its triangle planes); the solid holds every point
//!   whose ball of radius `2 viol` lies in the hull, so each section loses at most
//!   `6 viol (1 + g)` more (a planar convex set has an inscribed disc of at least a third of
//!   its width). The sections measured are those between the two end faces' flatness bands
//!   (from the highest vertex of the lower ring to the lowest of the upper, less `2 viol`
//!   each); the faces themselves are measured as the ring polygons, which are the `ub` there.
//!   A ball whose centre is on the axis inside the frustum's
//!   length adds a concentric section: the union cannot be narrower than either, and the
//!   ball's section contains a disc of radius `sqrt(rho^2 - s^2)` for the ball mesh's
//!   certified in-sphere `rho` at distance `s` from the centre. The minimum over the axis is
//!   found by branch and bound on those bounds, with the real section polygons giving `ub`
//!   (the width of the hull of all the polygons' vertices, which is the union's extent in each
//!   direction, not the largest single polygon's). This covers the
//!   neck where a cone leaves a ball (design 7.2: the whole transition, not the centreline
//!   exit).
//! * **Isolated ball** (a joint or foot ball on no frustum): `lb` is twice the in-sphere radius,
//!   `ub` the exact minimum width of the convex mesh. A tip ball alone is a tip, not a part.
//! * **Raft pad or bridge:** the exact minimum width `w` of the hull of the vertices is the
//!   `ub` (the mesh's extent along a direction is that of its vertices); the `lb` is
//!   `w - 6 viol` less twice the record's deviation and the arithmetic allowance
//!   (`tri::allowance_mm` at the part's scale); the `ub` is raised by the same two.
//! * **Any other join** of two overlapping parts (two rods crossing, a rod and a flare, two
//!   pads): the union's thickness at the join is the radius of the largest ball that fits in
//!   `P u Q` about a point of `P n Q`. A witness point whose distance to the boundary of the
//!   union reaches half the limit certifies the join; a cover of `P n Q` by small cells, each
//!   with a boundary point nearer than half the limit (the distance is 1-Lipschitz), proves a
//!   thin lens. Anything else is unverified.
//!
//! A part whose mesh is not the native shape is unverified.

use crate::chains::JOIN_MM;
use crate::json::Json;
use crate::outcome::{MAX_LOCATED, MEASURED_ON, Outcome, VERDICT_SCOPE, at};
use crate::parts::{Geom, Part, Role};
use crate::poly;
use crate::scene::Scene;
use crate::solid::{HalfSpace, Solid};
use crate::status::Located;
use crate::tri::{Tri, allowance_mm, closest_point};
use crate::v3::{V, add, cross, dist, dot, frame, len, mul, sub, unit};

type P2 = [f64; 2];
/// A point in a frustum's axis frame: axial position, then the two sideways coordinates.
type R3 = [f64; 3];

/// The least span of an axial interval the branch and bound still splits, mm.
const MIN_SPAN_MM: f64 = 1e-7;
/// Work limit of the axial branch and bound, interval bound evaluations per frustum.
const MAX_EVALS: usize = 4000;
/// Edge of a cell of the cover of a join, mm.
const CELL_MM: f64 = 0.01;
/// Most cells the cover of one join may use.
const MAX_CELLS: usize = 6000;
/// Most clearance evaluations the climb for a witness may use in one join.
const MAX_CLIMB_EVALS: usize = 1500;
/// Arithmetic allowance for contact, mm: the planes of two faces that lie on one plane differ
/// by the rounding of their f64 offsets (about 1e-15), so a face that lies on another is
/// compared with this much room. It is far under any gap a coordinate can hold at the sizes
/// here (one unit in the last place of an f32 near 1 mm is 1e-7 mm), so it does not turn a gap
/// into a join: bodies whose faces are apart by more than this are not in contact. Whether
/// such near-contact bodies are a join is never settled by growing them (a gap that the
/// dilated meshes overlap but the meshes do not is undecided, see `pair_fate`).
const TOUCH_MM: f64 = 1e-9;

// ---- planar helpers ---------------------------------------------------------------------

fn cross2(o: P2, a: P2, b: P2) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// The convex hull (counter-clockwise, no repeated first point) of 2D points.
fn hull2(points: &[P2]) -> Vec<P2> {
    let mut p = points.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12);
    if p.len() < 3 {
        return Vec::new();
    }
    let mut h: Vec<P2> = Vec::new();
    for pass in 0..2 {
        let start = h.len();
        let order: Vec<P2> = if pass == 0 {
            p.clone()
        } else {
            p.iter().rev().copied().collect()
        };
        for q in order {
            while h.len() >= start + 2 && cross2(h[h.len() - 2], h[h.len() - 1], q) <= 0.0 {
                h.pop();
            }
            h.push(q);
        }
        h.pop();
    }
    h
}

/// The extent of a convex polygon along the unit direction `d`.
fn extent(poly: &[P2], d: P2) -> f64 {
    let (lo, hi) = poly
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| {
            let x = d[0] * v[0] + d[1] * v[1];
            (l.min(x), h.max(x))
        });
    hi - lo
}

/// Unit edge normals of a convex polygon.
fn edge_normals(poly: &[P2]) -> impl Iterator<Item = P2> + '_ {
    (0..poly.len()).filter_map(move |i| {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let n = [q[1] - p[1], p[0] - q[0]];
        let l = n[0].hypot(n[1]);
        (l > 1e-300).then(|| [n[0] / l, n[1] / l])
    })
}

/// The width of a convex polygon (the least extent over its edge directions).
fn hull_width(poly: &[P2]) -> f64 {
    if poly.len() < 3 {
        return 0.0;
    }
    edge_normals(poly)
        .map(|d| extent(poly, d))
        .fold(f64::INFINITY, f64::min)
}

/// An upper bound of the width of the union of overlapping convex polygons: the union's extent
/// along a direction is the largest projected coordinate over all the polygons less the
/// smallest over all of them (not the largest extent of one polygon, which is smaller when the
/// polygons' centres differ). That is the extent of the hull of all their vertices, so the
/// bound is the width of that hull, the least extent over its edge directions.
fn union_width_upper(polys: &[Vec<P2>]) -> f64 {
    let all: Vec<P2> = polys.iter().flatten().copied().collect();
    hull_width(&hull2(&all))
}

/// The section of a triangle soup (a convex mesh) by the plane perpendicular to `u` through
/// `a + u t`, as a convex polygon in the plane's frame; empty when the plane misses it.
fn section_poly(tris: &[Tri], a: V, u: V, t: f64) -> Vec<P2> {
    let (e1, e2) = frame(u);
    let proj = |p: V| [dot(sub(p, a), e1), dot(sub(p, a), e2)];
    let mut pts: Vec<P2> = Vec::new();
    for tri in tris {
        for k in 0..3 {
            let (p, q) = (tri[k], tri[(k + 1) % 3]);
            let (dp, dq) = (dot(sub(p, a), u) - t, dot(sub(q, a), u) - t);
            if dp.abs() <= 1e-12 {
                pts.push(proj(p));
            }
            if (dp < 0.0 && dq > 0.0) || (dp > 0.0 && dq < 0.0) {
                pts.push(proj(add(p, mul(sub(q, p), dp / (dp - dq)))));
            }
        }
    }
    hull2(&pts)
}

/// The end polygon of a frustum mesh at axial position `t` (0 or its length): the vertices on
/// that end plane, in the plane's frame.
fn ring_poly(p: &Part, a: V, u: V, t: f64) -> Vec<P2> {
    let (e1, e2) = frame(u);
    let pts: Vec<P2> = p
        .tris
        .iter()
        .flatten()
        .filter(|v| (dot(sub(**v, a), u) - t).abs() <= p.tol)
        .map(|v| [dot(sub(*v, a), e1), dot(sub(*v, a), e2)])
        .collect();
    hull2(&pts)
}

// ---- parts as convex polytopes ----------------------------------------------------------

/// The half-spaces, their slacks, the triangles and the vertices of a valid part's mesh.
struct Body<'a> {
    planes: &'a [HalfSpace],
    slack: &'a [f64],
    tris: &'a [Tri],
    /// How far the mesh's own vertices stand outside its triangle planes (0 for a mesh that is
    /// exactly convex): the real convexity error, which is much smaller than `slack`.
    viol: f64,
    /// The record's measured deviation from its canonical mesh (`Solid::dev`): every width and
    /// distance read from the mesh is allowed to differ by this much per side.
    dev: f64,
}

fn body(p: &Part) -> Option<Body<'_>> {
    body_of(p.mesh.as_ref()?)
}

fn body_of(s: &Solid) -> Option<Body<'_>> {
    match s {
        Solid::Poly {
            planes,
            slack,
            tris,
            viol,
            dev,
            ..
        } => Some(Body {
            planes,
            slack,
            tris,
            viol: *viol,
            dev: *dev,
        }),
        _ => None,
    }
}

/// The certified in-sphere radius of a convex mesh about `c`: the least distance from `c` to a
/// face plane, less that plane's slack and the record's deviation from its canonical mesh.
fn insphere(b: &Body, c: V) -> f64 {
    b.planes
        .iter()
        .zip(b.slack)
        .map(|((n, o), s)| o - dot(*n, c) - s - b.dev)
        .fold(f64::INFINITY, f64::min)
        .max(0.0)
}

/// The exact minimum width of a convex polytope over all directions: the minimum is at a face
/// normal or at the cross product of two edges.
fn polytope_min_width(tris: &[Tri]) -> f64 {
    let verts: Vec<V> = tris.iter().flatten().copied().collect();
    let mut dirs: Vec<V> = Vec::new();
    let mut edges: Vec<V> = Vec::new();
    for t in tris {
        dirs.push(unit(cross(sub(t[1], t[0]), sub(t[2], t[0]))));
        for k in 0..3 {
            edges.push(unit(sub(t[(k + 1) % 3], t[k])));
        }
    }
    edges.dedup_by(|a, b| len(sub(*a, *b)) < 1e-9 || len(add(*a, *b)) < 1e-9);
    for i in 0..edges.len() {
        for j in i + 1..edges.len() {
            let c = unit(cross(edges[i], edges[j]));
            if len(c) > 0.0 {
                dirs.push(c);
            }
        }
    }
    dirs.iter()
        .filter(|d| len(**d) > 0.0)
        .map(|d| {
            let (lo, hi) = verts
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| {
                    (l.min(dot(*d, *v)), h.max(dot(*d, *v)))
                });
            hi - lo
        })
        .fold(f64::INFINITY, f64::min)
}

// ---- axial scan: a frustum with the balls on its axis ------------------------------------

/// A ball whose centre is on a frustum's axis, inside its length.
struct Partner<'a> {
    part: &'a Part,
    /// The centre's position along the axis.
    tc: f64,
    /// Certified in-sphere radius.
    rho: f64,
    /// The radius the fields name (the analytic sphere).
    radius: f64,
}

/// A frustum's geometry along its axis.
struct Axis<'a> {
    part: &'a Part,
    a: V,
    u: V,
    l: f64,
    /// The written vertices of the two end rings in the axis frame (axial position, e1, e2);
    /// vertex `j` of one ring is joined to vertex `j` of the other.
    ring_a: Vec<R3>,
    ring_b: Vec<R3>,
    /// The fastest sideways speed of a point `Q_j(t)` (mm per mm of axis), from the written
    /// vertices.
    speed: f64,
    /// The axial range of the sections measured as sections: from the highest vertex of the
    /// start ring to the lowest of the end ring, less `2 viol` each.
    span: (f64, f64),
    /// How far the written mesh's vertices stand outside its triangle planes.
    viol: f64,
    /// The frustum's measured deviation from its canonical mesh, and the largest such
    /// deviation of the frustum and its balls (every section polygon of the union moves by at
    /// most that much per unit of `1 + speed` when the mesh is replaced by the canonical one).
    dev: f64,
    dev_union: f64,
    /// Allowance for the arithmetic of a width, mm (`tri::allowance_mm` at the part's scale).
    allow: f64,
    /// The radii the fields name at the two ends (the analytic cone).
    ra: f64,
    rb: f64,
    balls: Vec<Partner<'a>>,
}

/// Where a ball's centre sits along the axis of frustum `f`, if it is on the axis and inside
/// its length (so a plane perpendicular to the axis through the centre cuts the frustum).
fn axial_position(f: &Part, ball: &Part) -> Option<f64> {
    let (Geom::Frust { a, b, .. }, Geom::Ball { c, .. }) = (f.geom, ball.geom) else {
        return None;
    };
    let l = dist(a, b);
    let u = unit(sub(b, a));
    let ac = sub(c, a);
    let tc = dot(ac, u);
    let off = len(sub(ac, mul(u, tc)));
    (off <= JOIN_MM && tc >= -JOIN_MM && tc <= l + JOIN_MM).then_some(tc)
}

/// The result of the width measurement of one thing.
#[derive(Debug, Clone, PartialEq)]
enum Fate {
    /// Certified at least `lb`, with a measured section of width `ub`.
    Wide { lb: f64, ub: f64 },
    /// A real section or direction of width `ub` is under the limit.
    Thin { ub: f64, at: V },
    /// Not decided, and why.
    Undecided { lb: f64, why: String },
}

impl Axis<'_> {
    /// The hull of the points `Q_j(t)` where the plane at axial position `t` crosses the
    /// segments joining the written ring vertices: a polygon inside the section (valid for `t`
    /// inside `span`, where every segment crosses the plane).
    fn blend_poly(&self, t: f64) -> Vec<P2> {
        let pts: Vec<P2> = self
            .ring_a
            .iter()
            .zip(&self.ring_b)
            .map(|(a, b)| {
                let k = (t - a[0]) / (b[0] - a[0]);
                [a[1] + k * (b[1] - a[1]), a[2] + k * (b[2] - a[2])]
            })
            .collect();
        hull2(&pts)
    }

    /// Certified lower bound of the union width for axial positions in `[t0, t1]` (inside
    /// `span`): the width of the blend polygon at the middle, less what it can change over the
    /// interval, the convexity error of the mesh, the record's deviation from its canonical
    /// mesh (each blend vertex moves at most `dev (1 + speed)`, so the width loses twice that)
    /// and the arithmetic allowance. Valid for every accepted mesh, whatever the ring twist or
    /// offset: the rings are the written ones.
    fn lb(&self, t0: f64, t1: f64) -> f64 {
        let loss = self.speed * (t1 - t0)
            + 6.0 * self.viol * (1.0 + self.speed)
            + 2.0 * self.dev * (1.0 + self.speed)
            + self.allow;
        let own = hull_width(&self.blend_poly(0.5 * (t0 + t1))) - loss;
        self.balls.iter().fold(own, |best, b| {
            let s = (b.tc - t0).abs().max((b.tc - t1).abs());
            let w = 2.0 * (b.rho * b.rho - s * s).max(0.0).sqrt() - self.allow;
            best.max(w)
        })
    }

    /// An upper bound of the union width at axial position `t`: the width of the hull of all
    /// the real section polygons (the union's extent in each direction), raised by the
    /// record's deviation from its canonical mesh (twice `dev (1 + speed)`) and the
    /// arithmetic allowance.
    fn ub(&self, t: f64) -> f64 {
        self.ub_written(t) + 2.0 * self.dev_union * (1.0 + self.speed) + self.allow
    }

    /// The width of the hull of the written section polygons at axial position `t`.
    fn ub_written(&self, t: f64) -> f64 {
        let own = if t <= 0.0 || t >= self.l {
            ring_poly(
                self.part,
                self.a,
                self.u,
                if t <= 0.0 { 0.0 } else { self.l },
            )
        } else {
            section_poly(&self.part.tris, self.a, self.u, t)
        };
        let mut polys = vec![own];
        for b in &self.balls {
            let s = section_poly(&b.part.tris, self.a, self.u, t);
            if s.len() >= 3 {
                polys.push(s);
            }
        }
        polys.retain(|p| p.len() >= 3);
        union_width_upper(&polys)
    }

    fn point(&self, t: f64) -> V {
        add(self.a, mul(self.u, t))
    }

    /// Certified lower bound of the width of the analytic solid (the cone the fields name with
    /// the spheres they name) for axial positions in `[t0, t1]`. A section perpendicular to the
    /// axis is a disc, or the union of concentric discs, so its width is twice the largest
    /// radius.
    fn lb_analytic(&self, t0: f64, t1: f64) -> f64 {
        let r_at = |t: f64| self.ra + (self.rb - self.ra) * t / self.l;
        let own = r_at(t0).min(r_at(t1));
        let best = self.balls.iter().fold(own, |best, b| {
            let s = (b.tc - t0).abs().max((b.tc - t1).abs());
            best.max((b.radius * b.radius - s * s).max(0.0).sqrt())
        });
        2.0 * best - self.allow
    }

    /// The width of the analytic solid at axial position `t` (exact, up to rounding).
    fn ub_analytic(&self, t: f64) -> f64 {
        let own = self.ra + (self.rb - self.ra) * t / self.l;
        let best = self.balls.iter().fold(own, |best, b| {
            let s = t - b.tc;
            best.max((b.radius * b.radius - s * s).max(0.0).sqrt())
        });
        2.0 * best
    }

    fn partners_in_analytic(&self, t0: f64, t1: f64) -> bool {
        self.balls
            .iter()
            .any(|b| b.tc - b.radius <= t1 && b.tc + b.radius >= t0)
    }

    /// Branch and bound over the axial position on the written mesh, over `span`; every
    /// interval can be split, since a smaller interval always loses less to the speed term.
    fn scan(&self, limit: f64) -> Fate {
        self.scan_with(
            limit,
            self.span,
            &|t0, t1| self.lb(t0, t1),
            &|t| self.ub(t),
            &|_, _| true,
        )
    }

    /// Branch and bound over the axial position on the analytic solid the fields name.
    fn scan_analytic(&self, limit: f64) -> Fate {
        self.scan_with(
            limit,
            (0.0, self.l),
            &|t0, t1| self.lb_analytic(t0, t1),
            &|t| self.ub_analytic(t),
            &|t0, t1| self.partners_in_analytic(t0, t1),
        )
    }

    /// Branch and bound over the axial positions in `span` with the given bounds: `lbf` a
    /// certified lower bound of the width over an interval, `ubf` the width of a real section
    /// (also taken at the two end faces), `partners` whether splitting an interval can change
    /// its lower bound. An empty `span` leaves only the end faces measured, and the answer is
    /// undecided unless one of them is thin.
    fn scan_with(
        &self,
        limit: f64,
        span: (f64, f64),
        lbf: &dyn Fn(f64, f64) -> f64,
        ubf: &dyn Fn(f64) -> f64,
        partners: &dyn Fn(f64, f64) -> bool,
    ) -> Fate {
        let mut ub = (f64::INFINITY, 0.0);
        let note_ub = |t: f64, ub: &mut (f64, f64)| {
            let w = ubf(t);
            if w < ub.0 {
                *ub = (w, t);
            }
        };
        note_ub(0.0, &mut ub);
        note_ub(self.l, &mut ub);
        let (mut lb_ok, mut lb_open, mut open) = (f64::INFINITY, f64::INFINITY, 0usize);
        let mut stack = Vec::new();
        if span.0 < span.1 {
            stack.push(span);
        } else {
            (open, lb_open) = (1, 0.0);
        }
        let mut evals = 0usize;
        while let Some((t0, t1)) = stack.pop() {
            if ub.0 < limit {
                break;
            }
            let lb = lbf(t0, t1);
            if lb >= limit {
                lb_ok = lb_ok.min(lb);
                continue;
            }
            let mid = 0.5 * (t0 + t1);
            note_ub(mid, &mut ub);
            evals += 1;
            if ub.0 < limit {
                break;
            }
            if t1 - t0 < MIN_SPAN_MM || evals > MAX_EVALS || !partners(t0, t1) {
                open += 1;
                lb_open = lb_open.min(lb);
                continue;
            }
            stack.push((t0, mid));
            stack.push((mid, t1));
        }
        if ub.0 < limit {
            return Fate::Thin {
                ub: ub.0,
                at: self.point(ub.1),
            };
        }
        if span.0 >= span.1 {
            return Fate::Undecided {
                lb: 0.0,
                why: "the end rings of the written mesh leave no band of sections to measure (the part is no longer than the flatness of its end faces)".to_owned(),
            };
        }
        if open > 0 {
            return Fate::Undecided {
                lb: lb_open,
                why: format!(
                    "the union width along the axis is bracketed to [{lb_open:.5}, {:.5}] mm, which straddles the {limit} mm limit",
                    ub.0
                ),
            };
        }
        Fate::Wide {
            lb: lb_ok,
            ub: ub.0,
        }
    }
}

/// The axial model of frustum `p`, with the valid balls on its axis.
fn axis_of<'a>(scene: &'a Scene, p: &'a Part) -> Option<Axis<'a>> {
    let Geom::Frust { a, b, .. } = p.geom else {
        return None;
    };
    let u = unit(sub(b, a));
    let l = dist(a, b);
    let to_frame = |ring: &[V]| -> Vec<R3> {
        let (e1, e2) = frame(u);
        ring.iter()
            .map(|v| {
                let d = sub(*v, a);
                [dot(d, u), dot(d, e1), dot(d, e2)]
            })
            .collect()
    };
    let (ring_a, ring_b) = (to_frame(&p.rings[0]), to_frame(&p.rings[1]));
    let viol = body(p).map_or(f64::INFINITY, |b| b.viol);
    let dev = body(p).map_or(f64::INFINITY, |b| b.dev);
    let mut dev_union = dev;
    // The rings must pair up vertex for vertex, the start ring wholly under the end ring.
    let top_a = ring_a
        .iter()
        .map(|v| v[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom_b = ring_b.iter().map(|v| v[0]).fold(f64::INFINITY, f64::min);
    let span = if ring_a.len() == ring_b.len() && ring_a.len() >= 3 {
        (top_a + 2.0 * viol, bottom_b - 2.0 * viol)
    } else {
        (0.0, 0.0)
    };
    // Over the span every segment from a start vertex to its partner crosses the plane going
    // up its axial range, so the speed is finite.
    let speed = ring_a
        .iter()
        .zip(&ring_b)
        .map(|(x, y)| (y[1] - x[1]).hypot(y[2] - x[2]) / (y[0] - x[0]).max(f64::MIN_POSITIVE))
        .fold(0.0, f64::max);
    let mut balls = Vec::new();
    for q in scene.parts.iter().filter(|q| q.is_valid()) {
        let (Some(tc), Geom::Ball { c, .. }) = (axial_position(p, q), q.geom) else {
            continue;
        };
        let Some(bd) = body(q) else { continue };
        let Geom::Ball { r: radius, .. } = q.geom else {
            continue;
        };
        dev_union = dev_union.max(bd.dev);
        balls.push(Partner {
            part: q,
            tc,
            rho: insphere(&bd, c),
            radius,
        });
    }
    let Geom::Frust { ra, rb, .. } = p.geom else {
        return None;
    };
    Some(Axis {
        part: p,
        a,
        u,
        l,
        ring_a,
        ring_b,
        speed,
        span,
        viol,
        dev,
        dev_union,
        allow: allowance_mm(p.bbox.max_abs()),
        ra,
        rb,
        balls,
    })
}

// ---- joins of two overlapping parts ------------------------------------------------------

/// The parts of the triangles of `of` that lie outside the convex polytope `other` (given by
/// its half-spaces), within `r` of `x`, as convex polygons.
fn outside_pieces(of: &Body, other: &[HalfSpace], x: V, r: f64) -> Vec<poly::Poly> {
    let mut out = Vec::new();
    for t in of.tris {
        if dist(x, closest_point(x, t)) >= r {
            continue;
        }
        let mut rest: poly::Poly = t.to_vec();
        for (n, o) in other {
            out.push(poly::clip(&rest, (mul(*n, -1.0), -o)));
            rest = poly::clip(&rest, (*n, *o));
            if rest.is_empty() {
                break;
            }
        }
    }
    out.retain(|p| p.len() >= 3);
    out
}

/// Distance from `x` to a convex planar polygon.
fn point_poly_dist(x: V, p: &[V]) -> f64 {
    (1..p.len() - 1)
        .map(|i| dist(x, closest_point(x, &[p[0], p[i], p[i + 1]])))
        .fold(f64::INFINITY, f64::min)
}

/// The distance from `x` to the boundary of `P u Q`, capped at `r` (the boundary is the part of
/// each surface outside the other solid).
fn union_clearance(p: (&Body, &[HalfSpace]), q: (&Body, &[HalfSpace]), x: V, r: f64) -> f64 {
    let mut best = r;
    for piece in outside_pieces(p.0, q.1, x, r)
        .iter()
        .chain(&outside_pieces(q.0, p.1, x, r))
    {
        best = best.min(point_poly_dist(x, piece));
    }
    best
}

/// What a join is, or that there is none.
enum Join {
    /// The two solids do not share volume.
    Apart,
    /// The overlap, as a point inside it and its bounding box.
    Overlap { inside: V, lo: V, hi: V },
}

/// The overlap of `p` and `q`, each seen as the polytope of the (grown) half-spaces `gp`, `gq`.
fn overlap_of(p: &Body, q: &Body, gp: &[HalfSpace], gq: &[HalfSpace]) -> Join {
    let mut pts: Vec<V> = Vec::new();
    for (of, other) in [(p, gq), (q, gp)] {
        for t in of.tris {
            let piece = poly::clip_all(t, other);
            if poly::area(&piece) > 1e-12 {
                pts.extend(piece);
            }
        }
    }
    if pts.is_empty() {
        return Join::Apart;
    }
    let n = pts.len() as f64;
    let inside = mul(pts.iter().fold([0.0; 3], |s, v| add(s, *v)), 1.0 / n);
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for v in &pts {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    Join::Overlap { inside, lo, hi }
}

/// The half-spaces of a body moved outwards by `eta`.
fn grown(b: &Body, eta: f64) -> Vec<HalfSpace> {
    b.planes.iter().map(|(n, o)| (*n, o + eta)).collect()
}

/// A pair of bodies with their grown half-spaces and the error allowed in their distances.
struct Pair<'a> {
    p: &'a Body<'a>,
    q: &'a Body<'a>,
    gp: Vec<HalfSpace>,
    gq: Vec<HalfSpace>,
    /// Error of a boundary distance: plane slack, the growth, and rounding.
    delta: f64,
}

impl Pair<'_> {
    fn clearance(&self, x: V, r: f64) -> f64 {
        union_clearance((self.p, &self.gp), (self.q, &self.gq), x, r)
    }

    /// Is `x` in both (grown) solids, that is in the join? A point on the face where two solids
    /// meet is.
    fn in_both(&self, x: V) -> bool {
        let inside = |planes: &[HalfSpace]| planes.iter().all(|(n, o)| dot(*n, x) <= *o);
        inside(&self.gp) && inside(&self.gq)
    }
}

/// Points of the overlap's box to try as witnesses: its middle and a coarse grid of at most
/// 6 x 6 x 6.
fn candidates(inside: V, lo: V, hi: V) -> Vec<V> {
    let mut out = vec![inside];
    let n = 6usize;
    for i in 0..n {
        for j in 0..n {
            for k in 0..n {
                let at = |a: usize, d: usize| lo[d] + (a as f64 + 0.5) * (hi[d] - lo[d]) / n as f64;
                out.push([at(i, 0), at(j, 1), at(k, 2)]);
            }
        }
    }
    out
}

/// A point of the overlap whose distance to the boundary of the union reaches half the
/// limit, and so a ball of that radius inside the union about it.
fn witness(pair: &Pair, inside: V, lo: V, hi: V, half: f64) -> Option<f64> {
    let probe = half + 2.0 * pair.delta + 1e-6;
    let value = |c: V| {
        if pair.in_both(c) {
            pair.clearance(c, probe) - pair.delta
        } else {
            f64::NEG_INFINITY
        }
    };
    let mut scored: Vec<(f64, V)> = candidates(inside, lo, hi)
        .into_iter()
        .map(|c| (value(c), c))
        .collect();
    if let Some((w, _)) = scored.iter().find(|(w, _)| *w >= half) {
        return Some(*w);
    }
    // A thin part leaves room for a ball of the limit's size only on its axis: climb from the
    // best few points by a pattern search over the 26 neighbouring directions.
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let span = (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f64::max);
    let mut evals = 0usize;
    for &(w0, c0) in scored.iter().take(3).filter(|(w, _)| w.is_finite()) {
        let (mut best, mut at, mut step) = (w0, c0, 0.25 * span);
        while step > 1e-6 && evals < MAX_CLIMB_EVALS {
            let mut moved = false;
            for d in neighbours26() {
                let c = add(at, mul(d, step));
                evals += 1;
                let w = value(c);
                if w > best {
                    (best, at, moved) = (w, c, true);
                    if best >= half {
                        return Some(best);
                    }
                }
            }
            if !moved {
                step *= 0.5;
            }
        }
    }
    None
}

/// The 26 directions to the neighbours of a cube (unnormalised: axis, face diagonal, corner).
fn neighbours26() -> Vec<V> {
    let mut out = Vec::with_capacity(26);
    for i in -1i8..=1 {
        for j in -1i8..=1 {
            for k in -1i8..=1 {
                if (i, j, k) != (0, 0, 0) {
                    out.push([f64::from(i), f64::from(j), f64::from(k)]);
                }
            }
        }
    }
    out
}

/// A cover of the overlap's box by cells in which every cell has a boundary point nearer than
/// half the limit (the distance to the boundary changes by at most the move). `Ok` is the
/// largest bound found over the cells; `Err` says why there is no cover.
fn cover(pair: &Pair, lo: V, hi: V, half: f64) -> Result<f64, String> {
    let counts: Vec<usize> = (0..3)
        .map(|k| (((hi[k] - lo[k]) / CELL_MM).ceil().max(1.0)) as usize)
        .collect();
    let total = counts[0]
        .saturating_mul(counts[1])
        .saturating_mul(counts[2]);
    if total > MAX_CELLS {
        return Err(format!(
            "the overlap of two parts needs {total} cells to cover (over {MAX_CELLS})"
        ));
    }
    let rc = 3f64.sqrt() * CELL_MM * 0.5;
    let mut worst = 0.0f64;
    for i in 0..counts[0] {
        for j in 0..counts[1] {
            for k in 0..counts[2] {
                let at = |a: usize, d: usize| {
                    lo[d] + (a as f64 + 0.5) * (hi[d] - lo[d]) / counts[d] as f64
                };
                let c = [at(i, 0), at(j, 1), at(k, 2)];
                let apart = pair
                    .p
                    .planes
                    .iter()
                    .chain(pair.q.planes)
                    .any(|(n, o)| dot(*n, c) - o > rc + pair.delta);
                if apart {
                    continue;
                }
                let d = pair.clearance(c, half) + pair.delta + rc;
                if d >= half {
                    return Err("the join of two parts is neither certified wide by a point nor covered as thin by its cells".to_owned());
                }
                worst = worst.max(d);
            }
        }
    }
    Ok(worst)
}

/// The width of the join of `p` and `q`.
///
/// `touch` is only the arithmetic allowance of `TOUCH_MM`: faces that lie on one plane (a
/// cone's end on a rod's start) compare as in contact and are inside the union, not on its
/// boundary; a real gap is not closed by it. The allowance is part of the bound's error
/// (`delta`). A failed witness is followed by the cover that proves a thin join.
fn join_fate(p: &Body, q: &Body, limit: f64, touch: f64, allow: f64) -> Option<Fate> {
    let (gp, gq) = (grown(p, touch), grown(q, touch));
    let Join::Overlap { inside, lo, hi } = overlap_of(p, q, &gp, &gq) else {
        return None;
    };
    let half = 0.5 * limit;
    // The error of a boundary distance: the growth, rounding, and the convexity error of the
    // meshes amplified by at most 1 / sin(11.25 degrees) (neighbouring faces of the native
    // 16-gon shapes turn by at least that) -- not the much larger worst-case plane slack.
    // Each mesh boundary also stands up to its record's deviation from the canonical mesh.
    let delta = touch + allow + 5.2 * p.viol.max(q.viol) + p.dev.max(q.dev);
    let pair = Pair {
        p,
        q,
        gp,
        gq,
        delta,
    };
    if let Some(w) = witness(&pair, inside, lo, hi, half) {
        return Some(Fate::Wide {
            lb: 2.0 * w,
            ub: f64::INFINITY,
        });
    }
    Some(match cover(&pair, lo, hi, half) {
        Ok(worst) => Fate::Thin {
            ub: 2.0 * worst,
            at: inside,
        },
        Err(why) => Fate::Undecided { lb: 0.0, why },
    })
}

// ---- findings --------------------------------------------------------------------------

struct Finding {
    rec: usize,
    role: Role,
    what: &'static str,
    /// The width of the written mesh: the verdict.
    fate: Fate,
    /// The width of the solid the fields name: information only (none for a raft part).
    analytic: Option<Fate>,
    at: V,
}

impl Finding {
    fn lb(&self) -> f64 {
        match self.fate {
            Fate::Wide { lb, .. } | Fate::Undecided { lb, .. } => lb,
            Fate::Thin { .. } => 0.0,
        }
    }

    fn ub(&self) -> f64 {
        match self.fate {
            Fate::Wide { ub, .. } | Fate::Thin { ub, .. } => ub,
            Fate::Undecided { .. } => f64::INFINITY,
        }
    }
}

impl Fate {
    /// Wide, thin or undecided: what the analytic information compares.
    fn class(&self) -> &'static str {
        match self {
            Self::Wide { .. } => "wide",
            Self::Thin { .. } => "thin",
            Self::Undecided { .. } => "undecided",
        }
    }
}

fn frustum_finding(scene: &Scene, p: &Part, limit: f64) -> Option<Finding> {
    let axis = axis_of(scene, p)?;
    let fate = axis.scan(limit);
    let place = match &fate {
        Fate::Thin { at, .. } => *at,
        _ => p.bbox.centre(),
    };
    Some(Finding {
        rec: p.rec,
        role: p.role,
        what: "section",
        fate,
        analytic: Some(axis.scan_analytic(limit)),
        at: place,
    })
}

/// A ball on no frustum's axis: its own width.
fn ball_finding(scene: &Scene, p: &Part, limit: f64) -> Option<Finding> {
    let Geom::Ball { c, r: radius } = p.geom else {
        return None;
    };
    if p.role == Role::TipBall {
        return None;
    }
    let attached = scene
        .parts
        .iter()
        .any(|q| q.is_valid() && axial_position(q, p).is_some());
    if attached {
        return None;
    }
    let bd = body(p)?;
    // The in-sphere already allows for the record's deviation; the upper end adds twice it.
    let lb = 2.0 * insphere(&bd, c) - allowance_mm(p.bbox.max_abs());
    let ub = polytope_min_width(&p.tris) + 2.0 * bd.dev + allowance_mm(p.bbox.max_abs());
    let mesh_fate = if lb >= limit {
        Fate::Wide { lb, ub }
    } else if ub < limit {
        Fate::Thin { ub, at: c }
    } else {
        Fate::Undecided {
            lb,
            why: format!(
                "a ball is bracketed to [{lb:.5}, {ub:.5}] mm across, straddling the {limit} mm limit"
            ),
        }
    };
    // The analytic sphere is exactly 2r across (information only).
    let across = 2.0 * radius;
    let allow = allowance_mm(p.bbox.max_abs());
    let analytic_fate = if across - allow >= limit {
        Fate::Wide {
            lb: across - allow,
            ub: across,
        }
    } else if across < limit {
        Fate::Thin { ub: across, at: c }
    } else {
        Fate::Undecided {
            lb: across - allow,
            why: format!(
                "a ball is {across:.6} mm across, within rounding of the {limit} mm limit"
            ),
        }
    };
    Some(Finding {
        rec: p.rec,
        role: p.role,
        what: "ball",
        fate: mesh_fate,
        analytic: Some(analytic_fate),
        at: c,
    })
}

fn raft_finding(p: &Part, limit: f64) -> Finding {
    let w = polytope_min_width(&p.tris);
    // The mesh is its vertices' hull up to `viol`: the solid holds the hull's inner parallel
    // body at `2 viol`, which is `6 viol` narrower at most; the arithmetic adds an allowance.
    let allow = allowance_mm(p.bbox.max_abs());
    let viol = body(p).map_or(f64::INFINITY, |b| b.viol);
    // Each side may also stand the record's deviation from its canonical mesh away.
    let dev = body(p).map_or(f64::INFINITY, |b| b.dev);
    let (lb, ub) = (w - 6.0 * viol - 2.0 * dev - allow, w + 2.0 * dev + allow);
    let fate = if lb >= limit {
        Fate::Wide { lb, ub }
    } else if ub < limit {
        Fate::Thin {
            ub,
            at: p.bbox.centre(),
        }
    } else {
        Fate::Undecided {
            lb,
            why: format!(
                "a raft part is {w:.6} mm across, within {:.1e} mm of the {limit} mm limit",
                6.0 * viol + 2.0 * dev + allow
            ),
        }
    };
    Finding {
        rec: p.rec,
        role: p.role,
        what: "raft part",
        fate,
        analytic: None,
        at: p.bbox.centre(),
    }
}

/// Is the pair (`i`, `j`) already measured by an axial scan (a ball on a frustum's axis)?
fn accounted(a: &Part, b: &Part) -> bool {
    axial_position(a, b).is_some() || axial_position(b, a).is_some()
}

/// Findings for every overlapping pair of written meshes that no axial scan covers.
fn join_findings(scene: &Scene, limit: f64) -> Vec<Finding> {
    let mut out = Vec::new();
    let valid: Vec<&Part> = scene.parts.iter().filter(|p| p.is_valid()).collect();
    let bodies: Vec<Option<Body>> = valid.iter().map(|p| body(p)).collect();
    for (i, p) in valid.iter().enumerate() {
        for (j, q) in valid.iter().enumerate().skip(i + 1) {
            if accounted(p, q) || !p.bbox.inflated(1e-6).overlaps(&q.bbox) {
                continue;
            }
            let (Some(bp), Some(bq)) = (&bodies[i], &bodies[j]) else {
                continue;
            };
            let allow = allowance_mm(p.bbox.max_abs().max(q.bbox.max_abs()));
            let Some(fate) = join_fate(bp, bq, limit, TOUCH_MM, allow) else {
                continue;
            };
            out.push(Finding {
                rec: p.rec,
                role: p.role,
                what: "join",
                fate,
                analytic: None,
                at: p.bbox.centre(),
            });
        }
    }
    out
}

/// H9.
#[must_use]
pub fn measure(scene: &Scene) -> Outcome {
    if scene.parts.is_empty() {
        return Outcome::not_applicable(
            "the file has no support records: no support part to measure",
        );
    }
    let limit = scene.limits.h9_thickness_mm;
    let mut findings: Vec<Finding> = Vec::new();
    let mut unverified: Vec<String> = Vec::new();
    for p in &scene.parts {
        if let Some(why) = &p.invalid {
            unverified.push(format!("record {} ({}): {why}", p.rec, p.role.name()));
            continue;
        }
        match p.geom {
            Geom::Frust { .. } => findings.extend(frustum_finding(scene, p, limit)),
            Geom::Ball { .. } => findings.extend(ball_finding(scene, p, limit)),
            Geom::Raft => findings.push(raft_finding(p, limit)),
        }
    }
    // Joins decide nothing once a part or section is already certainly thinner than the limit
    // (the rule is violated whatever the joins are), so they are measured only for a file whose
    // parts and sections are not thin.
    let skip_joins = findings.iter().any(|f| matches!(f.fate, Fate::Thin { .. }));
    if !skip_joins {
        findings.extend(join_findings(scene, limit));
    }
    finish(scene, limit, &findings, unverified)
}

/// The smallest measured width (upper end) of the findings `keep` selects, or null.
fn least(findings: &[Finding], keep: impl Fn(&Finding) -> bool) -> Json {
    let w = findings
        .iter()
        .filter(|f| keep(f))
        .map(Finding::ub)
        .fold(f64::INFINITY, f64::min);
    if w.is_finite() {
        Json::Num(w)
    } else {
        Json::Null
    }
}

/// Information only: the records whose analytic solid falls in a different width class (wide,
/// thin, undecided) from their written mesh, and the analytic solid's thinnest section.
fn analytic_information(findings: &[Finding]) -> Json {
    let differs: Vec<Json> = findings
        .iter()
        .filter_map(|f| f.analytic.as_ref().map(|a| (f, a)))
        .filter(|(f, a)| f.fate.class() != a.class())
        .map(|(f, a)| {
            Json::obj([
                ("record", Json::Uint(f.rec as u64)),
                ("written_mesh", Json::str(f.fate.class())),
                ("analytic", Json::str(a.class())),
            ])
        })
        .collect();
    let thinnest = findings
        .iter()
        .filter_map(|f| match f.analytic.as_ref()? {
            Fate::Wide { ub, .. } | Fate::Thin { ub, .. } => Some(*ub),
            Fate::Undecided { .. } => None,
        })
        .fold(f64::INFINITY, f64::min);
    Json::obj([
        ("decides_nothing", Json::Bool(true)),
        (
            "thinnest_section_mm",
            if thinnest.is_finite() {
                Json::Num(thinnest)
            } else {
                Json::Null
            },
        ),
        ("records_in_another_class", Json::Arr(differs)),
    ])
}

fn finish(scene: &Scene, limit: f64, findings: &[Finding], mut unverified: Vec<String>) -> Outcome {
    let thin: Vec<&Finding> = findings
        .iter()
        .filter(|f| matches!(f.fate, Fate::Thin { .. }))
        .collect();
    for f in findings {
        if let Fate::Undecided { why, .. } = &f.fate {
            unverified.push(format!(
                "record {} ({} {}): {why}",
                f.rec,
                f.role.name(),
                f.what
            ));
        }
    }
    let thinnest = findings.iter().min_by(|a, b| a.ub().total_cmp(&b.ub()));
    let lowest = findings
        .iter()
        .map(Finding::lb)
        .fold(f64::INFINITY, f64::min);
    let detail = Json::obj([
        ("limit_mm", Json::Num(limit)),
        ("measured_on", Json::str(MEASURED_ON)),
        ("verdict_scope", Json::str(VERDICT_SCOPE)),
        ("analytic_information", analytic_information(findings)),
        ("measured", Json::Uint(findings.len() as u64)),
        ("thinnest_section_mm", least(findings, |f| f.what != "join")),
        ("thinnest_join_mm", least(findings, |f| f.what == "join")),
        (
            "thinnest_width_mm",
            thinnest
                .filter(|f| f.ub().is_finite())
                .map_or(Json::Null, |f| Json::Num(f.ub())),
        ),
        (
            "thinnest_record",
            thinnest.map_or(Json::Null, |f| Json::Uint(f.rec as u64)),
        ),
        (
            "certified_lower_mm",
            if lowest.is_finite() {
                Json::Num(lowest)
            } else {
                Json::Null
            },
        ),
        ("undecided", Json::Uint(unverified.len() as u64)),
        (
            "method",
            Json::str(
                "written-mesh sections and in-spheres; frustum axis by branch and bound with certified lower and upper widths; joins by an inscribed-ball witness or a cell cover on the written meshes",
            ),
        ),
    ]);
    if !thin.is_empty() {
        let located: Vec<Located> = thin
            .iter()
            .take(MAX_LOCATED)
            .map(|f| {
                at(
                    Some(f.rec),
                    Some(scene.world(f.at)),
                    None,
                    format!(
                        "{} ({}) is at most {:.5} mm across, under the {limit} mm limit",
                        f.role.name(),
                        f.what,
                        f.ub()
                    ),
                )
            })
            .collect();
        return Outcome::violation(format!(
            "{} support part(s) or join(s) thinner than {limit} mm; thinnest {:.5} mm",
            thin.len(),
            thin.iter().map(|f| f.ub()).fold(f64::INFINITY, f64::min)
        ))
        .with_located(located)
        .with_detail(detail);
    }
    if !unverified.is_empty() {
        return Outcome::unverified(format!(
            "{} part(s) or join(s) could not be certified; first: {}",
            unverified.len(),
            unverified[0]
        ))
        .with_detail(detail);
    }
    Outcome::pass(format!(
        "every support part and join is certified at least {limit} mm across (lower bound {lowest:.5} mm)"
    ))
    .with_detail(detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Status;
    use crate::testkit::{self as kit, test_limits};

    pub(super) fn run(parts: Vec<Part>) -> Outcome {
        measure(&Scene::from_parts(
            &test_limits(),
            Vec::new(),
            [0.0; 3],
            parts,
        ))
    }

    fn rod(r: f32) -> Part {
        kit::part(0, 3, [0.0, 0.0, 0.0], [0.0, 0.0, -5.0], r, r)
    }

    #[test]
    fn a_rod_of_radius_0_052_passes_and_0_050_does_not() {
        let ok = run(vec![rod(0.052)]);
        assert_eq!(ok.status, Status::Pass, "{ok:?}");
        let bad = run(vec![rod(0.050)]);
        assert_eq!(bad.status, Status::Violation, "{bad:?}");
        // 2 r cos(pi/16) = 0.09808 for r = 0.05.
        assert!(bad.reason.contains("0.098"), "{}", bad.reason);
        assert!(!bad.located.is_empty());
    }

    #[test]
    fn the_width_is_the_polygon_not_the_field_radius() {
        // Fields say 0.050975 (0.10195 across the circle); the canonical 16-gon of that radius
        // is 2 r cos(pi / 16) = 0.0999105 across flats: a violation of the written mesh, though
        // the analytic solid is wide enough.
        let part = kit::part(0, 3, [0.0; 3], [0.0, 0.0, -5.0], 0.050_975, 0.050_975);
        assert!(part.is_valid(), "{:?}", part.invalid);
        let o = run(vec![part]);
        assert_eq!(o.status, Status::Violation, "{o:?}");
        let Some(Json::Num(w)) = o.detail.get("thinnest_width_mm") else {
            panic!("no width in {o:?}");
        };
        let flats = 2.0 * f64::from(0.050_975f32) * (std::f64::consts::PI / 16.0).cos();
        assert!((w - flats).abs() < 2e-6, "{w} vs {flats}");
        // A radius whose polygon is over the limit passes.
        let ok = kit::part(0, 3, [0.0; 3], [0.0, 0.0, -5.0], 0.050_99, 0.050_99);
        assert_eq!(run(vec![ok]).status, Status::Pass);
        // A mesh that is smaller than its fields by more than the rounding (here 1e-5 mm at a
        // coordinate of 5: two units in the last place are 9.5e-7) is refused, not measured.
        let r = kit::rec(3, [0.0; 3], [0.0, 0.0, -5.0], 0.050_982_8, 0.050_982_8);
        let coords = kit::frustum_coords([0.0; 3], [0.0, 0.0, -5.0], 0.050_972_8, 0.050_972_8);
        let refused = kit::part_with_mesh(0, &r, &coords);
        assert!(refused.invalid.is_some());
        assert_eq!(run(vec![refused]).status, Status::Unverified);
    }

    #[test]
    fn a_mesh_wider_than_its_fields_by_more_than_the_rounding_is_refused_not_measured() {
        // Fields: radius 0.0499 (0.0998 across the circle). Mesh: radius 0.0511 (about 0.1002
        // across flats). At y = 3000 two units in the last place are 4.9e-4 mm and the radii
        // differ by 1.2e-3: the record is not the canonical mesh of its fields. It is refused,
        // so the wide mesh cannot vouch for a thin field radius.
        let (s, e) = ([0.0, 3000.0, 0.0], [0.0, 3000.0, -5.0]);
        let r = kit::rec(3, s, e, 0.0499, 0.0499);
        let part = kit::part_with_mesh(0, &r, &kit::frustum_coords(s, e, 0.0511, 0.0511));
        assert!(part.invalid.is_some());
        assert_eq!(run(vec![part]).status, Status::Unverified);
        // The same place with fields and mesh both over the limit pass.
        let ok = kit::part(0, 3, s, e, 0.0515, 0.0515);
        assert_eq!(run(vec![ok]).status, Status::Pass);
        // A ball narrower in its fields than its mesh is refused too.
        let (bs, be) = ([0.0, 3000.0, -0.0499], [0.0, 3000.0, 0.0499]);
        let rb = kit::rec(9, bs, be, 0.0499, 0.0499);
        let bm = kit::ball_coords([0.0, 3000.0, -0.051], [0.0, 3000.0, 0.051], 0.051);
        let ball = kit::part_with_mesh(0, &rb, &bm);
        assert!(ball.invalid.is_some());
        assert_eq!(run(vec![ball]).status, Status::Unverified);
    }

    #[test]
    fn a_fat_join_at_a_large_coordinate_is_certified_on_the_written_meshes() {
        // Two radius 0.3 rods 0.5 apart at y = 3000, where the rounding between mesh and
        // fields is about 1e-3 mm: the join is wide in the written meshes.
        let (s, e) = ([0.0, 3000.0, 0.0], [0.0, 3000.0, -5.0]);
        let a = kit::part(0, 3, s, e, 0.3, 0.3);
        let (s2, e2) = ([0.5, 3000.0, -0.5], [0.5, 3000.0, -4.5]);
        let rec2 = kit::rec(3, s2, e2, 0.3, 0.3);
        let b = kit::part_with_mesh(1, &rec2, &kit::frustum_coords(s2, e2, 0.3, 0.3));
        assert_eq!(run(vec![a, b]).status, Status::Pass);
    }

    #[test]
    fn a_join_that_only_the_written_meshes_have_is_a_join_of_the_verdict() {
        // Round-3 review: two radius 0.3 rods whose fields leave an end gap of 8e-4 mm at
        // y = 3000, each written mesh end moved 4.5e-4 mm toward the other (valid: 2 units in
        // the last place are 4.9e-4 mm there). The written meshes overlap by 1e-4 mm; the
        // analytic solids do not touch. The verdict is measured on the written triangles: they
        // are one fat join (two sections and one join), a pass.
        let (lo_top, hi_bot) = (-6.9755f32, -6.9747f32);
        let (s1, e1) = ([0.0, 3000.0, -9.85], [0.0, 3000.0, lo_top]);
        let (s2, e2) = ([0.0, 3000.0, hi_bot], [0.0, 3000.0, -4.95]);
        let low = kit::part_with_mesh(
            0,
            &kit::rec(3, s1, e1, 0.3, 0.3),
            &kit::frustum_coords(s1, [0.0, 3000.0, lo_top + 4.5e-4], 0.3, 0.3),
        );
        let high = kit::part_with_mesh(
            1,
            &kit::rec(3, s2, e2, 0.3, 0.3),
            &kit::frustum_coords([0.0, 3000.0, hi_bot - 4.5e-4], e2, 0.3, 0.3),
        );
        assert!(low.is_valid() && high.is_valid());
        let o = run(vec![low, high]);
        assert_eq!(o.status, Status::Pass, "{o:?}");
        let Some(Json::Uint(n)) = o.detail.get("measured") else {
            panic!("no count in {o:?}");
        };
        assert_eq!(*n, 3, "two sections and one join: {o:?}");
        // The same rods with the fields' ends touching (and the meshes within the rounding of
        // them) are one fat join too.
        let (s2, e2) = ([0.0, 3000.0, lo_top], [0.0, 3000.0, -4.95]);
        let low = kit::part(0, 3, s1, e1, 0.3, 0.3);
        let high = kit::part(1, 3, s2, e2, 0.3, 0.3);
        assert_eq!(run(vec![low, high]).status, Status::Pass);
    }
}

#[cfg(test)]
mod joins;
#[cfg(test)]
mod twist;
#[cfg(test)]
mod union;
