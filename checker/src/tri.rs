//! Triangles: closest points, ray hits and a bounding-volume tree (all f64).

use crate::v3::{Aabb, V, add, cross, dot, mul, sub};

/// A triangle.
pub type Tri = [V; 3];

/// The allowance, in mm, for the f64 arithmetic of one measurement on the written numbers,
/// which are exact f32 values read as f64: 1e-6 mm (about 1e10 units in the last place of an f64
/// near 1 mm; near-parallel edges lose a few times 1e-8 mm) plus 2^22 units in the last place of
/// an f64 at `scale`. It grows with the coordinates the file holds, so it is not a fixed number
/// that a large coordinate outgrows (a coordinate beyond the build volume is refused anyway).
/// It stands for rounding only. How far a written mesh stands from the canonical mesh of its
/// fields is a separate term, `Solid::dev` (the record's measured deviation, set from
/// `canon::deviation_mm`), which every bound that reads a mesh adds to this one. Every lower
/// bound is lowered, and every upper bound raised, by at least both where a computed
/// distance, width or area stands in for an exact one.
#[must_use]
pub fn allowance_mm(scale: f64) -> f64 {
    1e-6 + f64::from(1u32 << 22) * f64::EPSILON * scale
}

#[must_use]
pub fn tri_box(t: &Tri) -> Aabb {
    Aabb::of_points(t.iter().copied())
}

/// The point of triangle `t` nearest to `p` (Ericson, "Real-Time Collision Detection" 5.1.5).
#[must_use]
pub fn closest_point(p: V, t: &Tri) -> V {
    let [a, b, c] = *t;
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
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
        return add(a, mul(ab, d1 / (d1 - d3)));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add(a, mul(ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return add(b, mul(sub(c, b), w));
    }
    let denom = 1.0 / (va + vb + vc);
    add(add(a, mul(ab, vb * denom)), mul(ac, vc * denom))
}

/// Result of a ray against a triangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RayHit {
    Miss,
    /// A clean crossing at ray parameter `t > 0`.
    Hit(f64),
    /// The ray passes within `eps` of an edge or runs parallel to the plane: not decidable.
    Ambiguous,
}

/// Ray `o + t d` against `tri` (Moeller-Trumbore, two-sided). Hits within `eps` (in
/// barycentric terms) of an edge, and near-parallel rays, are `Ambiguous`.
#[must_use]
pub fn ray_tri(o: V, d: V, tri: &Tri, eps: f64) -> RayHit {
    let e1 = sub(tri[1], tri[0]);
    let e2 = sub(tri[2], tri[0]);
    let pv = cross(d, e2);
    let det = dot(e1, pv);
    let scale = (dot(e1, e1) * dot(e2, e2)).sqrt().max(1e-300);
    if det.abs() < 1e-12 * scale {
        // Parallel: only matters if the ray lies in the plane.
        let n = cross(e1, e2);
        let off = dot(n, sub(o, tri[0])).abs() / dot(n, n).sqrt().max(1e-300);
        return if off < 1e-9 {
            RayHit::Ambiguous
        } else {
            RayHit::Miss
        };
    }
    let inv = 1.0 / det;
    let tv = sub(o, tri[0]);
    let u = dot(tv, pv) * inv;
    let qv = cross(tv, e1);
    let v = dot(d, qv) * inv;
    let t = dot(e2, qv) * inv;
    let w = 1.0 - u - v;
    if u < -eps || v < -eps || w < -eps {
        return RayHit::Miss;
    }
    if t < -1e-9 {
        return RayHit::Miss;
    }
    if u < eps || v < eps || w < eps || t < 1e-9 {
        return RayHit::Ambiguous;
    }
    RayHit::Hit(t)
}

/// Smallest distance between segments `p1-q1` and `p2-q2` (Ericson 5.1.9): the distance of two
/// points that lie on the segments, so never under the true distance but for rounding.
fn seg_seg_dist(p1: V, q1: V, p2: V, q2: V) -> f64 {
    let (d1, d2, r) = (sub(q1, p1), sub(q2, p2), sub(p1, p2));
    let (a, e, f) = (dot(d1, d1), dot(d2, d2), dot(d2, r));
    let (s, t) = if a <= 1e-300 && e <= 1e-300 {
        (0.0, 0.0)
    } else if a <= 1e-300 {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = dot(d1, r);
        if e <= 1e-300 {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = dot(d1, d2);
            let denom = a * e - b * b;
            let s = if denom > 0.0 {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let t = (b * s + f) / e;
            if t < 0.0 {
                (((-c) / a).clamp(0.0, 1.0), 0.0)
            } else if t > 1.0 {
                (((b - c) / a).clamp(0.0, 1.0), 1.0)
            } else {
                (s, t)
            }
        }
    };
    crate::v3::dist(add(p1, mul(d1, s)), add(p2, mul(d2, t)))
}

/// Distance from the point where segment `p-q` crosses the plane of `t` to `t` (an upper bound
/// of the segment-to-triangle distance that is exact when the segment pierces the triangle);
/// infinite when the segment does not cross the plane.
fn pierce_dist(p: V, q: V, t: &Tri) -> f64 {
    let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    let (sp, sq) = (dot(n, sub(p, t[0])), dot(n, sub(q, t[0])));
    if (sp > 0.0 && sq > 0.0) || (sp < 0.0 && sq < 0.0) || (sp - sq).abs() < f64::MIN_POSITIVE {
        return f64::INFINITY;
    }
    let x = add(p, mul(sub(q, p), sp / (sp - sq)));
    crate::v3::dist(x, closest_point(x, t))
}

/// Smallest distance between two triangles, 0 when they meet (all f64).
///
/// The minimum of two triangles is attained between a vertex and the other triangle, between
/// two edges, or is 0 where an edge pierces the other triangle: every candidate below is the
/// distance of two points that lie on the triangles, so the smallest candidate is the distance
/// up to rounding (about 1e-12 mm for coordinates of this file's size; near-parallel edges
/// lose at most a few times 1e-8 mm).
#[must_use]
pub fn tri_tri_dist(a: &Tri, b: &Tri) -> f64 {
    let mut best = f64::INFINITY;
    for (x, y) in [(a, b), (b, a)] {
        for &p in x {
            best = best.min(crate::v3::dist(p, closest_point(p, y)));
        }
        for k in 0..3 {
            best = best.min(pierce_dist(x[k], x[(k + 1) % 3], y));
        }
    }
    for i in 0..3 {
        for j in 0..3 {
            best = best.min(seg_seg_dist(a[i], a[(i + 1) % 3], b[j], b[(j + 1) % 3]));
        }
    }
    best
}

/// Distance between two boxes (0 when they overlap).
#[must_use]
pub fn box_dist(a: &Aabb, b: &Aabb) -> f64 {
    (0..3)
        .map(|k| (a.lo[k] - b.hi[k]).max(b.lo[k] - a.hi[k]).max(0.0).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// Three fixed, unrelated ray directions for crossing-parity tests.
pub const RAY_DIRS: [V; 3] = [
    [
        0.577_350_269_189_6,
        0.332_011_692_273_7,
        0.745_355_992_499_9,
    ],
    [
        -0.267_261_241_912_4,
        0.801_783_725_737_3,
        -0.534_522_483_824_8,
    ],
    [
        0.123_091_490_979_3,
        -0.492_365_963_917_3,
        0.861_640_436_855_3,
    ],
];

/// Is `p` inside the closed, oriented mesh `tris`? `None` when no direction of [`RAY_DIRS`]
/// gives a clean crossing count.
#[must_use]
pub fn point_in_mesh(p: V, tris: &[Tri]) -> Option<bool> {
    RAY_DIRS.iter().find_map(|&d| {
        let mut hits = 0usize;
        for t in tris {
            match ray_tri(p, d, t, 1e-9) {
                RayHit::Hit(_) => hits += 1,
                RayHit::Ambiguous => return None,
                RayHit::Miss => {}
            }
        }
        Some(hits % 2 == 1)
    })
}

const LEAF: usize = 4;

#[derive(Debug, Clone, Copy)]
struct Node {
    bb: Aabb,
    /// Leaf: first index into `order`. Internal: left child (right is `left + 1`... see
    /// `count`).
    a: u32,
    /// Leaf: number of items (> 0). Internal: 0, and `a`/`b` are the children.
    count: u32,
    b: u32,
}

/// A bounding-volume tree over a list of boxes.
#[derive(Debug, Clone)]
pub struct Bvh {
    nodes: Vec<Node>,
    order: Vec<u32>,
    boxes: Vec<Aabb>,
}

impl Bvh {
    /// Build over `boxes` (median split on the widest axis of the centres).
    #[must_use]
    pub fn build(boxes: &[Aabb]) -> Self {
        let mut order: Vec<u32> = (0..boxes.len())
            .map(|i| u32::try_from(i).unwrap_or(u32::MAX))
            .collect();
        let mut nodes = Vec::new();
        if !boxes.is_empty() {
            build_rec(boxes, &mut order, 0, boxes.len(), &mut nodes);
        }
        Self {
            nodes,
            order,
            boxes: boxes.to_vec(),
        }
    }

    /// Indices of boxes that overlap `q`, appended to `out`.
    pub fn query(&self, q: &Aabb, out: &mut Vec<u32>) {
        if self.nodes.is_empty() {
            return;
        }
        let mut stack = vec![0usize];
        while let Some(i) = stack.pop() {
            let n = &self.nodes[i];
            if !n.bb.overlaps(q) {
                continue;
            }
            if n.count > 0 {
                let s = n.a as usize;
                out.extend(
                    self.order[s..s + n.count as usize]
                        .iter()
                        .copied()
                        .filter(|&i| self.boxes[i as usize].overlaps(q)),
                );
            } else {
                stack.push(n.a as usize);
                stack.push(n.b as usize);
            }
        }
    }

    /// Indices of boxes the ray `o + t d` (t >= 0) may hit, appended to `out`.
    pub fn query_ray(&self, o: V, d: V, out: &mut Vec<u32>) {
        if self.nodes.is_empty() {
            return;
        }
        let mut stack = vec![0usize];
        while let Some(i) = stack.pop() {
            let n = &self.nodes[i];
            if !ray_box(o, d, &n.bb) {
                continue;
            }
            if n.count > 0 {
                let s = n.a as usize;
                out.extend(
                    self.order[s..s + n.count as usize]
                        .iter()
                        .copied()
                        .filter(|&i| ray_box(o, d, &self.boxes[i as usize])),
                );
            } else {
                stack.push(n.a as usize);
                stack.push(n.b as usize);
            }
        }
    }
}

fn ray_box(o: V, d: V, b: &Aabb) -> bool {
    let (mut t0, mut t1) = (0.0f64, f64::INFINITY);
    for k in 0..3 {
        if d[k].abs() < 1e-300 {
            if o[k] < b.lo[k] - 1e-9 || o[k] > b.hi[k] + 1e-9 {
                return false;
            }
        } else {
            let (mut a, mut c) = (
                (b.lo[k] - 1e-9 - o[k]) / d[k],
                (b.hi[k] + 1e-9 - o[k]) / d[k],
            );
            if a > c {
                std::mem::swap(&mut a, &mut c);
            }
            t0 = t0.max(a);
            t1 = t1.min(c);
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

fn build_rec(
    boxes: &[Aabb],
    order: &mut [u32],
    start: usize,
    end: usize,
    nodes: &mut Vec<Node>,
) -> u32 {
    let mut bb = Aabb::empty();
    for &i in &order[start..end] {
        bb.grow_box(&boxes[i as usize]);
    }
    let me = nodes.len();
    let to32 = |x: usize| u32::try_from(x).unwrap_or(u32::MAX);
    nodes.push(Node {
        bb,
        a: to32(start),
        count: to32(end - start),
        b: 0,
    });
    if end - start <= LEAF {
        return to32(me);
    }
    let mut cb = Aabb::empty();
    for &i in &order[start..end] {
        cb.grow(boxes[i as usize].centre());
    }
    let axis = (0..3)
        .max_by(|&p, &q| cb.axis_len(p).total_cmp(&cb.axis_len(q)))
        .unwrap_or(0);
    order[start..end].sort_by(|&p, &q| {
        boxes[p as usize].centre()[axis].total_cmp(&boxes[q as usize].centre()[axis])
    });
    let mid = start + (end - start) / 2;
    let l = build_rec(boxes, order, start, mid, nodes);
    let r = build_rec(boxes, order, mid, end, nodes);
    nodes[me].a = l;
    nodes[me].b = r;
    nodes[me].count = 0;
    to32(me)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_point_covers_face_edge_and_vertex_regions() {
        let t: Tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let near = |p: V, want: V| crate::v3::dist(closest_point(p, &t), want) < 1e-12;
        assert!(near([0.2, 0.2, 1.0], [0.2, 0.2, 0.0]));
        assert!(near([-1.0, -1.0, 0.0], [0.0, 0.0, 0.0]));
        assert!(near([2.0, 2.0, 0.0], [0.5, 0.5, 0.0]));
        assert!(near([0.5, -1.0, 0.0], [0.5, 0.0, 0.0]));
    }

    #[test]
    fn triangle_distance_covers_face_edge_vertex_piercing_and_coplanar_cases() {
        let t: Tri = [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [0.0, 4.0, 0.0]];
        let up = |h: f64, dx: f64| -> Tri { [[dx, 0.0, h], [dx + 1.0, 0.0, h], [dx, 1.0, h]] };
        // Parallel, directly above (face against vertices).
        assert!((tri_tri_dist(&t, &up(0.3, 0.5)) - 0.3).abs() < 1e-12);
        // Beyond the hypotenuse, 0.3 up: vertex (3, 3) against the edge x + y = 4.
        let past: Tri = [[3.0, 3.0, 0.3], [4.0, 3.0, 0.3], [3.0, 4.0, 0.3]];
        let want = 0.3f64.hypot(2.0 / 2.0f64.sqrt());
        assert!((tri_tri_dist(&t, &past) - want).abs() < 1e-12);
        // Skew edges crossing at right angles, 0.25 apart.
        let skew: Tri = [[1.0, -1.0, 0.25], [1.0, 1.0, 0.25], [1.0, 0.0, 3.0]];
        assert!((tri_tri_dist(&t, &skew) - 0.25).abs() < 1e-12);
        // An edge that pierces the middle of the big triangle: 0.
        let spike: Tri = [[1.0, 1.0, -1.0], [1.0, 1.0, 1.0], [1.5, 1.0, 1.0]];
        assert!(tri_tri_dist(&t, &spike) < 1e-12);
        // Coplanar, overlapping and disjoint.
        let flat: Tri = [[1.0, 1.0, 0.0], [6.0, 1.0, 0.0], [1.0, 2.0, 0.0]];
        assert!(tri_tri_dist(&t, &flat) < 1e-12);
        let apart: Tri = [[6.0, 0.0, 0.0], [7.0, 0.0, 0.0], [6.0, 1.0, 0.0]];
        assert!((tri_tri_dist(&t, &apart) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn rays_hit_miss_and_graze() {
        let t: Tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let d = [0.0, 0.0, 1.0];
        assert!(matches!(
            ray_tri([0.2, 0.2, -1.0], d, &t, 1e-9),
            RayHit::Hit(_)
        ));
        assert_eq!(ray_tri([0.9, 0.9, -1.0], d, &t, 1e-9), RayHit::Miss);
        assert_eq!(ray_tri([0.5, 0.0, -1.0], d, &t, 1e-9), RayHit::Ambiguous);
        assert_eq!(ray_tri([0.2, 0.2, 1.0], d, &t, 1e-9), RayHit::Miss);
    }

    #[test]
    fn bvh_returns_exactly_the_overlapping_boxes() {
        let boxes: Vec<Aabb> = (0..50)
            .map(|i| {
                let x = f64::from(i);
                Aabb {
                    lo: [x, 0.0, 0.0],
                    hi: [x + 0.5, 1.0, 1.0],
                }
            })
            .collect();
        let bvh = Bvh::build(&boxes);
        let mut out = Vec::new();
        bvh.query(
            &Aabb {
                lo: [9.9, 0.2, 0.2],
                hi: [11.2, 0.4, 0.4],
            },
            &mut out,
        );
        out.sort_unstable();
        assert_eq!(out, vec![10, 11]);
        let mut hits = Vec::new();
        bvh.query_ray([20.25, 0.5, -1.0], [0.0, 0.0, 1.0], &mut hits);
        hits.sort_unstable();
        assert_eq!(hits, vec![20]);
    }
}
