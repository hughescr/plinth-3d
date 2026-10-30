//! Flat bounding-volume hierarchy over a Morton-ordered mesh, built once per frozen mesh.
//!
//! Nodes are 32 bytes: six f32 bounds (rounded outward, so they never shrink a triangle)
//! and two u32 fields. Triangles keep the mesh order, so a leaf is a contiguous range of
//! triangles and a node's children are found by index arithmetic:
//!
//! * leaf: `a` = first triangle, `b` = triangle count (> 0);
//! * internal: the left child is the next node, `a` = index of the right child, `b` = 0.

use crate::counters::{self, Op};
use crate::error::{GeomError, Result};
use crate::mesh::Mesh;
use crate::tri::{V3, closest_on_triangle, dist2};

const LEAF_SIZE: usize = 4;
const STACK: usize = 64;

/// Relative and absolute slack when pruning by box distance, so that a triangle whose
/// computed distance ties the best one is never skipped because of rounding.
const PRUNE_REL: f64 = 1e-12;
const PRUNE_ABS: f64 = 1e-20;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct Node {
    min: [f32; 3],
    max: [f32; 3],
    a: u32,
    b: u32,
}

const _: () = assert!(size_of::<Node>() == 32);

/// A BVH over one mesh. Borrowing the mesh keeps the pair frozen together.
#[derive(Debug)]
pub struct Bvh<'m> {
    mesh: &'m Mesh,
    nodes: Vec<Node>,
}

/// The nearest triangle to a point.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Nearest {
    pub tri: u32,
    pub point: V3,
    pub dist2: f64,
}

/// Largest f32 not above `v`.
fn round_down(v: f64) -> f32 {
    let f = v as f32;
    if f64::from(f) > v { f.next_down() } else { f }
}

/// Smallest f32 not below `v`.
fn round_up(v: f64) -> f32 {
    let f = v as f32;
    if f64::from(f) < v { f.next_up() } else { f }
}

struct TriBounds {
    min: Vec<[f32; 3]>,
    max: Vec<[f32; 3]>,
}

impl TriBounds {
    fn of(mesh: &Mesh) -> Self {
        let n = mesh.triangle_count();
        let (mut min, mut max) = (Vec::with_capacity(n), Vec::with_capacity(n));
        let axes = [mesh.xs(), mesh.ys(), mesh.zs()];
        for t in mesh.indices().chunks_exact(3) {
            let mut lo = [0.0_f32; 3];
            let mut hi = [0.0_f32; 3];
            for (k, axis) in axes.iter().enumerate() {
                let v = [
                    axis[t[0] as usize],
                    axis[t[1] as usize],
                    axis[t[2] as usize],
                ];
                lo[k] = round_down(v[0].min(v[1]).min(v[2]));
                hi[k] = round_up(v[0].max(v[1]).max(v[2]));
            }
            min.push(lo);
            max.push(hi);
        }
        Self { min, max }
    }

    fn union(&self, lo: usize, hi: usize) -> ([f32; 3], [f32; 3]) {
        let mut bmin = [f32::INFINITY; 3];
        let mut bmax = [f32::NEG_INFINITY; 3];
        for t in lo..hi {
            for k in 0..3 {
                bmin[k] = bmin[k].min(self.min[t][k]);
                bmax[k] = bmax[k].max(self.max[t][k]);
            }
        }
        (bmin, bmax)
    }
}

/// Build the subtree over triangles `lo..hi`; its root is written at the next free node.
fn build_range(nodes: &mut Vec<Node>, tb: &TriBounds, lo: usize, hi: usize) {
    let at = nodes.len();
    let (min, max) = tb.union(lo, hi);
    if hi - lo <= LEAF_SIZE {
        nodes.push(Node {
            min,
            max,
            a: lo as u32,
            b: (hi - lo) as u32,
        });
        return;
    }
    nodes.push(Node {
        min,
        max,
        a: 0,
        b: 0,
    });
    let mid = lo + (hi - lo) / 2;
    build_range(nodes, tb, lo, mid);
    nodes[at].a = nodes.len() as u32;
    build_range(nodes, tb, mid, hi);
}

/// Squared distance from `p` to a node's box (0 inside).
fn box_dist2(n: &Node, p: V3) -> f64 {
    let mut s = 0.0;
    for ((&lo, &hi), &c) in n.min.iter().zip(&n.max).zip(&p) {
        let d = (f64::from(lo) - c).max(c - f64::from(hi)).max(0.0);
        s += d * d;
    }
    s
}

impl<'m> Bvh<'m> {
    /// Build the hierarchy. One batch call, counted as [`Op::BvhBuild`] with one item per
    /// triangle.
    ///
    /// # Errors
    /// Returns [`GeomError::EmptyMesh`] when the mesh has no triangles.
    pub fn build(mesh: &'m Mesh) -> Result<Self> {
        let n = mesh.triangle_count();
        if n == 0 {
            return Err(GeomError::EmptyMesh);
        }
        counters::record(Op::BvhBuild, n);
        let tb = TriBounds::of(mesh);
        let mut nodes = Vec::with_capacity(2 * n / LEAF_SIZE + 2);
        build_range(&mut nodes, &tb, 0, n);
        Ok(Self { mesh, nodes })
    }

    /// The mesh this hierarchy was built over.
    #[must_use]
    pub fn mesh(&self) -> &'m Mesh {
        self.mesh
    }

    /// Number of nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    fn corners(&self, t: usize) -> [V3; 3] {
        let m = self.mesh;
        let at = |k: usize| {
            let v = m.indices()[3 * t + k] as usize;
            [m.xs()[v], m.ys()[v], m.zs()[v]]
        };
        [at(0), at(1), at(2)]
    }

    /// Nearest triangle to `p`; ties on distance go to the lowest triangle number. Private
    /// to the batch kernels (design section 8: no public single-point query).
    pub(crate) fn nearest(&self, p: V3) -> Nearest {
        let mut best = Nearest {
            tri: u32::MAX,
            point: p,
            dist2: f64::INFINITY,
        };
        let mut stack = [0_u32; STACK];
        let mut top = 1;
        while top > 0 {
            top -= 1;
            let i = stack[top] as usize;
            let node = &self.nodes[i];
            if box_dist2(node, p) > best.dist2 + best.dist2 * PRUNE_REL + PRUNE_ABS {
                continue;
            }
            if node.b > 0 {
                self.scan_leaf(node, p, &mut best);
                continue;
            }
            let (l, r) = (i + 1, node.a as usize);
            let (dl, dr) = (box_dist2(&self.nodes[l], p), box_dist2(&self.nodes[r], p));
            let (far, near) = if dl <= dr { (r, l) } else { (l, r) };
            stack[top] = far as u32;
            stack[top + 1] = near as u32;
            top += 2;
        }
        best
    }

    fn scan_leaf(&self, node: &Node, p: V3, best: &mut Nearest) {
        for t in node.a..node.a + node.b {
            let [a, b, c] = self.corners(t as usize);
            let q = closest_on_triangle(p, a, b, c);
            let d2 = dist2(p, q);
            let better = d2.total_cmp(&best.dist2).then(t.cmp(&best.tri)).is_lt();
            if better {
                *best = Nearest {
                    tri: t,
                    point: q,
                    dist2: d2,
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{assert_pt, cube_soup, plate_soup};

    #[test]
    fn nodes_are_32_bytes_and_cover_every_triangle_once() {
        assert_eq!(size_of::<Node>(), 32);
        let (x, y, z) = cube_soup();
        let m = Mesh::from_soup(&x, &y, &z).expect("cube");
        let b = Bvh::build(&m).expect("bvh");
        let mut covered = vec![0_u32; m.triangle_count()];
        for n in &b.nodes {
            if n.b > 0 {
                for t in n.a..n.a + n.b {
                    covered[t as usize] += 1;
                }
            }
        }
        assert!(covered.iter().all(|&c| c == 1), "{covered:?}");
    }

    #[test]
    fn leaf_boxes_contain_their_triangles_and_parents_contain_children() {
        let (x, y, z) = cube_soup();
        let m = Mesh::from_soup(&x, &y, &z).expect("cube");
        let b = Bvh::build(&m).expect("bvh");
        for (i, n) in b.nodes.iter().enumerate() {
            if n.b > 0 {
                for t in n.a..n.a + n.b {
                    for c in b.corners(t as usize) {
                        for ((&lo, &hi), &v) in n.min.iter().zip(&n.max).zip(&c) {
                            assert!(f64::from(lo) <= v && v <= f64::from(hi));
                        }
                    }
                }
            } else {
                for ch in [i + 1, n.a as usize] {
                    for k in 0..3 {
                        assert!(n.min[k] <= b.nodes[ch].min[k] && b.nodes[ch].max[k] <= n.max[k]);
                    }
                }
            }
        }
    }

    #[test]
    fn outward_rounding_never_shrinks() {
        let v = 0.1_f64; // not representable in f32
        assert!(f64::from(round_down(v)) <= v && f64::from(round_up(v)) >= v);
        assert!(f64::from(round_down(v)) < v && f64::from(round_up(v)) > v);
        let e = 0.5_f64; // exact
        assert_eq!((f64::from(round_down(e)), f64::from(round_up(e))), (e, e));
    }

    #[test]
    fn plate_nearest_and_empty_mesh() {
        let (x, y, z) = plate_soup();
        let m = Mesh::from_soup(&x, &y, &z).expect("plate");
        let b = Bvh::build(&m).expect("bvh");
        let n = b.nearest([1.0, 3.0, 2.5]);
        assert_pt(n.point, [1.0, 3.0, 0.0]);
        assert_eq!(n.dist2.to_bits(), 6.25_f64.to_bits());
        let empty = Mesh::new(vec![], vec![], vec![], vec![]).expect("empty mesh");
        assert!(matches!(Bvh::build(&empty), Err(GeomError::EmptyMesh)));
    }
}
