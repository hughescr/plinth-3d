//! **A box tree over the down-facing faces of a mesh**, for the seat-face
//! rule's nearest ADMITTED face (perf lane FF, 2026-10-15).
//!
//! # The question
//!
//! The seat-face rule ([`crate::footprint_normal::sites_at`],
//! [`crate::footprint_normal::reseat_sites`]) picks, for a point, the face
//! nearest to it among the faces whose outward unit normal `n` has
//! `n.z < 0.0 && n.z < max_normal_z`, the squared distance read as
//! `|p - closest_on_tri(p, a, b, c)|²`, the LOWEST face id winning an exact
//! tie. Until this lane that was a scan of every face for every point
//! (180 of 778 samples of `footprint::apply` on Bat_Cat_Owl, plus 138 for
//! building the face table each call).
//!
//! # Why a generic nearest-triangle tree is not enough
//!
//! The admitted set depends on the query (`max_normal_z` differs between the
//! seat rule and its any-down-facing fallback), and the nearest face overall
//! is usually a wall that the rule does not admit. So every node keeps the
//! lowest unit-normal `z` in its subtree: a node whose lowest `z` is not below
//! the query's threshold holds no admitted face and is skipped whole. The tree
//! holds the faces with `n.z < 0.0` only (an admitted face is always one).
//!
//! # Why the answer is the scan's, bit for bit
//!
//! * Every face that is tested is tested with the scan's own arithmetic
//!   (`closest_on_tri`, then `sub`, then `dot`), on the same corner values.
//! * A node is skipped for distance only when its box lies STRICTLY farther
//!   from the point than the best face so far (`lower_bound > best`), so a
//!   face that ties the best is still reached.
//! * Visit order differs from id order, so the choice among exact ties is made
//!   by the rule itself: a face replaces the best when its distance is lower,
//!   or equal with a lower id. That is the scan's result, which keeps the
//!   first (lowest id) of equal distances.
//! * Face boxes are inflated by a margin far above the rounding of
//!   `closest_on_tri` (an absolute `1e-9` of the mesh's largest coordinate,
//!   and `1e-6` of the face's longest edge, the case of a sliver), so a box
//!   bound never exceeds the distance the scan computes for a face inside it.
//!   The shadow check (`PLINTH_SWEEP_VERIFY_FOOTPRINT=1`) and the oracle tests
//!   hold that to bits.

use geom_kernel::{closest_on_tri, vec3};

/// One face: its corners and outward unit normal.
pub(crate) type FaceCornersNormal = Option<([[f64; 3]; 3], [f64; 3])>;

/// Faces per leaf.
const LEAF: usize = 8;

/// A subtree this size or larger is built with `rayon::join`.
const PARALLEL_BUILD: usize = 8192;

/// A node: `count > 0` is a leaf over `order[first..first + count]`, else an
/// internal node with children `left` and `right`.
#[derive(Clone, Copy, Debug)]
struct Node {
    lo: [f64; 3],
    hi: [f64; 3],
    /// The lowest unit-normal `z` of any face in the subtree.
    min_z: f64,
    left: u32,
    right: u32,
    first: u32,
    count: u32,
}

/// The tree. A pure function of the face table it was built from.
pub(crate) struct AdmittedTree {
    nodes: Vec<Node>,
    /// Face ids, in leaf order.
    order: Vec<u32>,
}

/// One face's box and centroid, for the build.
#[derive(Clone, Copy)]
struct Item {
    id: u32,
    lo: [f64; 3],
    hi: [f64; 3],
    centre: [f64; 3],
    z: f64,
}

/// How a node of `items` is built: a subtree as nodes (root first), with the
/// ids it covers.
struct Built {
    nodes: Vec<Node>,
    order: Vec<u32>,
}

impl AdmittedTree {
    /// Build over the faces with a unit normal of `z < 0.0`.
    pub(crate) fn of(faces: &[FaceCornersNormal]) -> Self {
        let scale = faces
            .iter()
            .flatten()
            .flat_map(|(corners, _)| corners.iter().flatten())
            .fold(0.0_f64, |most, &value| most.max(value.abs()));
        let absolute = 1e-9 * (1.0 + scale);
        let mut items: Vec<Item> = faces
            .iter()
            .enumerate()
            .filter_map(|(id, face)| {
                let (corners, unit) = (*face)?;
                // NaN is not down-facing, as in the scan.
                let down_facing = unit[2] < 0.0;
                if !down_facing {
                    return None;
                }
                let id = u32::try_from(id).ok()?;
                let mut lo = corners[0];
                let mut hi = corners[0];
                for corner in &corners[1..] {
                    for axis in 0..3 {
                        lo[axis] = lo[axis].min(corner[axis]);
                        hi[axis] = hi[axis].max(corner[axis]);
                    }
                }
                let longest = [
                    vec3::dist(corners[0], corners[1]),
                    vec3::dist(corners[1], corners[2]),
                    vec3::dist(corners[2], corners[0]),
                ]
                .into_iter()
                .fold(0.0_f64, f64::max);
                let margin = absolute + 1e-6 * longest;
                let centre = std::array::from_fn(|axis| 0.5 * (lo[axis] + hi[axis]));
                for axis in 0..3 {
                    lo[axis] -= margin;
                    hi[axis] += margin;
                }
                Some(Item {
                    id,
                    lo,
                    hi,
                    centre,
                    z: unit[2],
                })
            })
            .collect();
        if items.is_empty() {
            return Self {
                nodes: Vec::new(),
                order: Vec::new(),
            };
        }
        let built = build(&mut items);
        Self {
            nodes: built.nodes,
            order: built.order,
        }
    }

    /// The nearest face to `point` among those with `n.z < 0.0 &&
    /// n.z < max_normal_z`: `(id, unit normal)`, and how many faces were
    /// tested. The scan's answer, bit for bit (module doc).
    pub(crate) fn nearest(
        &self,
        faces: &[FaceCornersNormal],
        point: [f64; 3],
        max_normal_z: f64,
    ) -> (Option<(u32, [f64; 3])>, u64) {
        let mut best = f64::INFINITY;
        let mut found: Option<(u32, [f64; 3])> = None;
        let mut tested = 0_u64;
        if self.nodes.is_empty() {
            return (None, tested);
        }
        // A face is one test, a node a box bound; the depth of a tree of
        // `LEAF`-sized leaves over a `u32` of faces fits 64 with room.
        let mut stack: Vec<u32> = Vec::with_capacity(64);
        stack.push(0);
        while let Some(at) = stack.pop() {
            let node = self.nodes[at as usize];
            // No face of the subtree is below the threshold (a NaN threshold
            // admits none, as in the scan), or the box lies strictly farther
            // than the best face.
            let any_admitted = node.min_z < max_normal_z;
            if !any_admitted || bound(&node, point) > best {
                continue;
            }
            if node.count > 0 {
                let from = node.first as usize;
                let to = from + node.count as usize;
                for &id in &self.order[from..to] {
                    let Some(([a, b, c], unit)) = faces[id as usize] else {
                        continue;
                    };
                    if !(unit[2] < 0.0 && unit[2] < max_normal_z) {
                        continue;
                    }
                    tested += 1;
                    let offset = vec3::sub(point, closest_on_tri(point, a, b, c));
                    let distance = vec3::dot(offset, offset);
                    let tied = distance.partial_cmp(&best) == Some(std::cmp::Ordering::Equal);
                    let better = distance < best
                        || (tied && found.is_some_and(|(held, _)| id < held));
                    if better {
                        best = distance;
                        found = Some((id, unit));
                    }
                }
            } else {
                let (left, right) = (node.left, node.right);
                let near_left =
                    bound(&self.nodes[left as usize], point) <= bound(&self.nodes[right as usize], point);
                // The nearer child is popped first.
                if near_left {
                    stack.push(right);
                    stack.push(left);
                } else {
                    stack.push(left);
                    stack.push(right);
                }
            }
        }
        (found, tested)
    }
}

/// The squared distance from `point` to the box, `0.0` inside it. A NaN
/// coordinate bounds nothing (`0.0`), so the face tests decide, as the scan
/// does.
fn bound(node: &Node, point: [f64; 3]) -> f64 {
    let mut sum = 0.0;
    for ((&lo, &hi), &at) in node.lo.iter().zip(&node.hi).zip(&point) {
        let gap = (lo - at).max(at - hi).max(0.0);
        sum += gap * gap;
    }
    sum
}

fn build(items: &mut [Item]) -> Built {
    let (lo, hi, min_z) = items.iter().fold(
        ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3], f64::INFINITY),
        |(mut lo, mut hi, z), item| {
            for axis in 0..3 {
                lo[axis] = lo[axis].min(item.lo[axis]);
                hi[axis] = hi[axis].max(item.hi[axis]);
            }
            (lo, hi, z.min(item.z))
        },
    );
    if items.len() <= LEAF {
        // A leaf's ids in ascending order, so ties inside a leaf resolve as
        // the scan does even before the id comparison.
        items.sort_unstable_by_key(|item| item.id);
        return Built {
            nodes: vec![Node {
                lo,
                hi,
                min_z,
                left: 0,
                right: 0,
                first: 0,
                count: u32::try_from(items.len()).unwrap_or(u32::MAX),
            }],
            order: items.iter().map(|item| item.id).collect(),
        };
    }
    // Split at the median centroid along the axis the centroids spread most;
    // the id breaks equal centroids, so the tree never depends on the order
    // the faces arrive in.
    let (mut c_lo, mut c_hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for item in items.iter() {
        for axis in 0..3 {
            c_lo[axis] = c_lo[axis].min(item.centre[axis]);
            c_hi[axis] = c_hi[axis].max(item.centre[axis]);
        }
    }
    let axis = (0..3)
        .max_by(|&x, &y| (c_hi[x] - c_lo[x]).total_cmp(&(c_hi[y] - c_lo[y])))
        .unwrap_or(0);
    let middle = items.len() / 2;
    items.select_nth_unstable_by(middle, |x, y| {
        x.centre[axis].total_cmp(&y.centre[axis]).then(x.id.cmp(&y.id))
    });
    let total = items.len();
    let (left_items, right_items) = items.split_at_mut(middle);
    let (left, right) = if total >= PARALLEL_BUILD {
        rayon::join(|| build(left_items), || build(right_items))
    } else {
        (build(left_items), build(right_items))
    };
    // Layout: this node, the left subtree, the right subtree. Child indices
    // inside a built subtree are relative to its own root; they are shifted
    // when the subtree is placed, and a leaf's `first` by the ids before it.
    let left_base = 1_u32;
    let right_base = left_base.saturating_add(u32::try_from(left.nodes.len()).unwrap_or(u32::MAX));
    let left_order = u32::try_from(left.order.len()).unwrap_or(u32::MAX);
    let mut nodes: Vec<Node> = Vec::with_capacity(1 + left.nodes.len() + right.nodes.len());
    nodes.push(Node {
        lo,
        hi,
        min_z,
        left: left_base,
        right: right_base,
        first: 0,
        count: 0,
    });
    nodes.extend(placed(&left.nodes, left_base, 0));
    nodes.extend(placed(&right.nodes, right_base, left_order));
    let mut order = left.order;
    order.extend(right.order);
    Built { nodes, order }
}

/// A built subtree placed at node index `base`, its leaves starting at
/// position `order_offset` of the final id order.
fn placed(nodes: &[Node], base: u32, order_offset: u32) -> impl Iterator<Item = Node> + '_ {
    nodes.iter().map(move |node| {
        if node.count > 0 {
            Node {
                first: node.first + order_offset,
                ..*node
            }
        } else {
            Node {
                left: node.left + base,
                right: node.right + base,
                ..*node
            }
        }
    })
}
