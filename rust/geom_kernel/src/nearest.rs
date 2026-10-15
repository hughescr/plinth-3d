//! Exact nearest triangle of a small explicit triangle set, for a batch of
//! points, through a bounding-box tree.
//!
//! This is the BVH form of the brute-force scan the tip-landing code ran
//! (`landings x faces`, one [`closest_on_tri`] per pair). It answers with the
//! SAME bits, not merely the same face:
//!
//! * the distance compared is the squared length `dot(p - c, p - c)` of the
//!   [`closest_on_tri`] result, exactly as the scan computed it (not the
//!   rounded `sqrt` the kernel's own `nearest_one` compares);
//! * the winner is the lexicographic minimum of `(squared distance, face id)`,
//!   which is what a scan in id order with a strict `<` produces: the lowest id
//!   among equal squared distances. A face whose squared distance is NaN or
//!   `inf` never wins, in both;
//! * the reported distance is `sqrt` of that squared distance.
//!
//! Visiting order therefore cannot change the answer, so the tree is free to
//! visit the near child first.
//!
//! # Why pruning by box distance is sound here
//!
//! A subtree is skipped only when the squared distance from the point to its
//! box exceeds the best squared distance so far by a margin: the best distance
//! plus `1e-9 * (1 + largest coordinate)` mm, squared. `closest_on_tri` rounds,
//! so a face in the box can compute a squared distance a few ULP of the
//! coordinate scale below the exact box bound; the margin is some seven orders
//! of magnitude above that. A face that could tie or beat the best is always
//! visited.
//!
//! The bound also assumes the point `closest_on_tri` returns lies inside the
//! face's own box. For a well-conditioned triangle it does. For a sliver the
//! interior branch divides by a near-zero sum and can return a point far
//! outside the box, so slivers (and any face with a non-finite vertex) are kept
//! out of the tree and scanned by every query instead. If any face vertex or
//! query point is non-finite, the whole batch takes the brute-force scan.

use rayon::prelude::*;

use crate::closest::closest_on_tri;
use crate::vec3::{cross, dot, sub, V3};

/// One triangle, as three vertices.
pub type Tri = [V3; 3];

/// A batch answer: per point the nearest face id and the distance to it, and
/// how many point-triangle distances were computed to get it.
#[derive(Clone, Debug, PartialEq)]
pub struct NearestBatch {
    /// In point order; `None` when no face has a finite squared distance.
    pub hits: Vec<Option<(usize, f64)>>,
    /// Number of [`closest_on_tri`] evaluations across the batch.
    pub tris_tested: u64,
}

/// Below this many points the tree does not pay for its own build.
pub const BVH_MIN_POINTS: usize = 16;
/// Below this many faces a scan is as cheap as building a tree.
pub const BVH_MIN_TRIS: usize = 64;

const LEAF_TRIS: usize = 4;
/// Subtrees at least this big are built on two threads.
const PAR_BUILD_MIN: usize = 16_384;
/// A face with `|e1 x e2|^2 < SLIVER_SIN2 * |e1|^2 * |e2|^2` goes to the scan list.
const SLIVER_SIN2: f64 = 1e-8;
/// Relative part of the prune margin; see the module docs.
const PRUNE_MARGIN: f64 = 1e-9;

#[derive(Clone)]
struct Node {
    bmin: V3,
    bmax: V3,
    /// Leaf: first index into the leaf-ordered face list. Internal: offset of
    /// the right child from this node (the left child is the next node).
    a: u32,
    /// Leaf: face count (always > 0). Internal: 0.
    n: u32,
}

/// The tree and the scan list of one triangle set.
struct Index {
    nodes: Vec<Node>,
    /// Leaf-ordered faces and their ids in the caller's slice.
    tris: Vec<Tri>,
    ids: Vec<u32>,
    /// Faces kept out of the tree, in id order.
    scanned: Vec<(u32, Tri)>,
    /// Largest absolute coordinate over all faces.
    scale: f64,
}

struct Item {
    id: u32,
    centroid: V3,
    bmin: V3,
    bmax: V3,
}

fn tri_is_finite(tri: &Tri) -> bool {
    tri.iter().all(|v| v.iter().all(|c| c.is_finite()))
}

fn point_is_finite(p: V3) -> bool {
    p.iter().all(|c| c.is_finite())
}

/// True when `closest_on_tri` is trustworthy to stay inside the face's box.
fn tri_is_well_conditioned(tri: &Tri) -> bool {
    let e1 = sub(tri[1], tri[0]);
    let e2 = sub(tri[2], tri[0]);
    let normal = cross(e1, e2);
    let area2 = dot(normal, normal);
    let lengths = dot(e1, e1) * dot(e2, e2);
    // `>=` so NaN and zero-length edges fail.
    area2 >= SLIVER_SIN2 * lengths && area2 > 0.0
}

fn make_item(id: u32, tri: &Tri) -> Item {
    let mut bmin = tri[0];
    let mut bmax = tri[0];
    for v in &tri[1..] {
        for k in 0..3 {
            bmin[k] = bmin[k].min(v[k]);
            bmax[k] = bmax[k].max(v[k]);
        }
    }
    let centroid = [
        0.5 * (bmin[0] + bmax[0]),
        0.5 * (bmin[1] + bmax[1]),
        0.5 * (bmin[2] + bmax[2]),
    ];
    Item {
        id,
        centroid,
        bmin,
        bmax,
    }
}

/// Build the subtree over `items` (preorder: the node, then the left subtree,
/// then the right). `base` is the position of `items[0]` in the leaf order.
fn build(items: &mut [Item], base: usize) -> Vec<Node> {
    let mut bmin = items[0].bmin;
    let mut bmax = items[0].bmax;
    let mut cmin = items[0].centroid;
    let mut cmax = items[0].centroid;
    for item in &items[1..] {
        for k in 0..3 {
            bmin[k] = bmin[k].min(item.bmin[k]);
            bmax[k] = bmax[k].max(item.bmax[k]);
            cmin[k] = cmin[k].min(item.centroid[k]);
            cmax[k] = cmax[k].max(item.centroid[k]);
        }
    }
    if items.len() <= LEAF_TRIS {
        return vec![Node {
            bmin,
            bmax,
            // `base + len` was checked against `u32::MAX` by the caller.
            a: u32::try_from(base).unwrap_or(u32::MAX),
            n: u32::try_from(items.len()).unwrap_or(u32::MAX),
        }];
    }
    let mut axis = 0;
    for k in 1..3 {
        if cmax[k] - cmin[k] > cmax[axis] - cmin[axis] {
            axis = k;
        }
    }
    let mid = items.len() / 2;
    items.select_nth_unstable_by(mid, |x, y| {
        x.centroid[axis]
            .total_cmp(&y.centroid[axis])
            .then(x.id.cmp(&y.id))
    });
    let (left_items, right_items) = items.split_at_mut(mid);
    let (left, right) = if left_items.len() + right_items.len() >= PAR_BUILD_MIN {
        rayon::join(
            || build(left_items, base),
            || build(right_items, base + mid),
        )
    } else {
        (build(left_items, base), build(right_items, base + mid))
    };
    let mut nodes = Vec::with_capacity(1 + left.len() + right.len());
    nodes.push(Node {
        bmin,
        bmax,
        a: u32::try_from(1 + left.len()).unwrap_or(u32::MAX),
        n: 0,
    });
    nodes.extend(left);
    nodes.extend(right);
    nodes
}

impl Index {
    /// `None` when the set cannot be indexed (non-finite vertex, or more faces
    /// than a `u32` id holds); the caller then scans.
    fn build(triangles: &[Tri]) -> Option<Index> {
        if u32::try_from(triangles.len()).is_err() || !triangles.iter().all(tri_is_finite) {
            return None;
        }
        let mut scale = 0.0_f64;
        let mut items = Vec::with_capacity(triangles.len());
        let mut scanned = Vec::new();
        for (id, tri) in triangles.iter().enumerate() {
            let id = u32::try_from(id).ok()?;
            for v in tri {
                for c in v {
                    scale = scale.max(c.abs());
                }
            }
            if tri_is_well_conditioned(tri) {
                items.push(make_item(id, tri));
            } else {
                scanned.push((id, *tri));
            }
        }
        let nodes = if items.is_empty() {
            Vec::new()
        } else {
            build(&mut items, 0)
        };
        let tris = items.iter().map(|item| triangles[item.id as usize]).collect();
        let ids = items.iter().map(|item| item.id).collect();
        Some(Index {
            nodes,
            tris,
            ids,
            scanned,
            scale,
        })
    }
}

/// Squared distance from `p` to the box.
#[inline]
fn point_box_dist2(p: V3, bmin: V3, bmax: V3) -> f64 {
    let mut s = 0.0_f64;
    for k in 0..3 {
        let d = if p[k] < bmin[k] {
            bmin[k] - p[k]
        } else if p[k] > bmax[k] {
            p[k] - bmax[k]
        } else {
            0.0
        };
        s += d * d;
    }
    s
}

/// The running best of one query.
struct Best {
    distance2: f64,
    id: Option<usize>,
    /// Boxes farther than this squared distance cannot hold a tie or a win.
    cut2: f64,
    margin: f64,
    tested: u64,
}

impl Best {
    fn new(margin: f64) -> Best {
        Best {
            distance2: f64::INFINITY,
            id: None,
            cut2: f64::INFINITY,
            margin,
            tested: 0,
        }
    }

    #[allow(
        clippy::float_cmp,
        reason = "equal squared distances resolve to the lowest face id; that exact tie-break \
                  is what makes this bit-identical to the id-order scan"
    )]
    #[inline]
    fn offer(&mut self, p: V3, id: usize, tri: &Tri) {
        let closest = closest_on_tri(p, tri[0], tri[1], tri[2]);
        let offset = sub(p, closest);
        let distance2 = dot(offset, offset);
        self.tested += 1;
        let better = distance2 < self.distance2
            || (distance2 == self.distance2 && self.id.is_some_and(|best| id < best));
        if better {
            self.distance2 = distance2;
            self.id = Some(id);
            let reach = distance2.sqrt() + self.margin;
            self.cut2 = reach * reach;
        }
    }

    fn finish(&self) -> Option<(usize, f64)> {
        self.id.map(|id| (id, self.distance2.sqrt()))
    }
}

fn query(index: &Index, p: V3, stack: &mut Vec<(u32, f64)>) -> (Option<(usize, f64)>, u64) {
    let reach = p.iter().fold(index.scale, |s, c| s.max(c.abs()));
    let mut best = Best::new(PRUNE_MARGIN * (1.0 + reach));
    for (id, tri) in &index.scanned {
        best.offer(p, *id as usize, tri);
    }
    stack.clear();
    if !index.nodes.is_empty() {
        stack.push((0, 0.0));
    }
    while let Some((at, bound2)) = stack.pop() {
        if bound2 > best.cut2 {
            continue;
        }
        let node = &index.nodes[at as usize];
        if node.n > 0 {
            let start = node.a as usize;
            let end = start + node.n as usize;
            for (tri, id) in index.tris[start..end].iter().zip(&index.ids[start..end]) {
                best.offer(p, *id as usize, tri);
            }
        } else {
            let left = at + 1;
            let right = at + node.a;
            let (ln, rn) = (&index.nodes[left as usize], &index.nodes[right as usize]);
            let left_bound2 = point_box_dist2(p, ln.bmin, ln.bmax);
            let right_bound2 = point_box_dist2(p, rn.bmin, rn.bmax);
            // The nearer child is pushed last, so it is popped first.
            if left_bound2 <= right_bound2 {
                stack.push((right, right_bound2));
                stack.push((left, left_bound2));
            } else {
                stack.push((left, left_bound2));
                stack.push((right, right_bound2));
            }
        }
    }
    (best.finish(), best.tested)
}

fn collect(rows: Vec<(Option<(usize, f64)>, u64)>) -> NearestBatch {
    let mut tris_tested = 0_u64;
    let mut hits = Vec::with_capacity(rows.len());
    for (hit, tested) in rows {
        tris_tested = tris_tested.saturating_add(tested);
        hits.push(hit);
    }
    NearestBatch { hits, tris_tested }
}

/// The nearest face of `triangles` to each of `points`, by testing every
/// face: the reference the tree is held to bit for bit.
#[must_use]
pub fn nearest_triangles_brute(triangles: &[Tri], points: &[V3]) -> NearestBatch {
    let rows = points
        .par_iter()
        .map(|&p| {
            let mut best = Best::new(0.0);
            for (id, tri) in triangles.iter().enumerate() {
                best.offer(p, id, tri);
            }
            (best.finish(), best.tested)
        })
        .collect();
    collect(rows)
}

/// The nearest face of `triangles` to each of `points`, through a
/// bounding-box tree built over the faces. Same answer as
/// [`nearest_triangles_brute`], bit for bit; falls back to it for a
/// non-finite input (see the module docs).
#[must_use]
pub fn nearest_triangles_bvh(triangles: &[Tri], points: &[V3]) -> NearestBatch {
    if !points.iter().copied().all(point_is_finite) {
        return nearest_triangles_brute(triangles, points);
    }
    let Some(index) = Index::build(triangles) else {
        return nearest_triangles_brute(triangles, points);
    };
    let rows = points
        .par_iter()
        .map_init(Vec::new, |stack, &p| query(&index, p, stack))
        .collect();
    collect(rows)
}

/// The nearest face of `triangles` to each of `points`: the tree when the
/// batch is big enough to pay for building it, else the scan.
#[must_use]
pub fn nearest_triangles(triangles: &[Tri], points: &[V3]) -> NearestBatch {
    if points.len() < BVH_MIN_POINTS || triangles.len() < BVH_MIN_TRIS {
        nearest_triangles_brute(triangles, points)
    } else {
        nearest_triangles_bvh(triangles, points)
    }
}
