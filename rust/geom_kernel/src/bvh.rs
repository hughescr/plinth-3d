//! Deterministic AABB BVH over triangle indices (spec sec 3.2). No RNG, no
//! hash/pointer order: binned SAH on the largest-centroid-extent axis with
//! fixed tie rules and an index-median fallback. Two builds of the same mesh
//! are structurally identical.

use crate::consts::EPS;
use crate::mesh::Mesh;
use crate::vec3::V3;
use std::sync::atomic::{AtomicU64, Ordering};

// Result-inert instrumentation at the actual construction boundary. Count
// attempts, not only retained NativeSurface instances, so failed/transient and
// direct testkit builds cannot escape the reuse audit.
static BVH_BUILD_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

pub const LEAF_MAX: usize = 8;
/// Face count at and above which a BVH range builds its two halves side by
/// side. Execution only: the tree is the same at every value, so this moves
/// time, never a node. Below it a range is too small for the hand-off to pay.
const PARALLEL_MIN_FACES: usize = 8192;
const N_BINS: usize = 16;

/// The largest face count `Bvh::build` can represent, DERIVED from the storage
/// it has to represent it in rather than picked.
///
/// [`Node::a`] and [`Node::b`] are `i32`.  They carry two different things: for
/// a leaf, a start offset and a count into `order` (both `< n`); for an internal
/// node, the indices of its two children (both `< nodes.len()`).  A binary tree
/// whose every leaf holds at least one face has at most `2n - 1` nodes, so
/// `2n - 1 <= i32::MAX` bounds BOTH uses at once, and that is this value.
///
/// Before this bound was named, the four `as i32` writes below simply wrapped:
/// a mesh past the ceiling produced a silently corrupt tree with negative node
/// indices and no error.  The ceiling is ~1.07e9 faces, roughly 2,500x the
/// largest corpus model this project handles (the 429,120-face treant), so it
/// refuses nothing real -- but it is now a REFUSAL rather than a wrap, and it
/// is written where a reader can find it instead of being implied by a field
/// type.
pub const MAX_FACES: usize = (i32::MAX as usize).div_ceil(2);

#[derive(Clone)]
pub struct Node {
    pub bmin: V3,
    pub bmax: V3,
    /// max per-triangle Möller ray pad over this subtree (Amendment A11)
    pub pad: f64,
    /// leaf: a=start into `order`, b=count. internal: a=left node, b=right node.
    pub a: i32,
    pub b: i32,
    pub leaf: bool,
}

pub struct Bvh {
    pub nodes: Vec<Node>,
    /// Global face IDs, reordered by the build (leaf ranges index into this).
    pub order: Vec<u32>,
    pub depth: usize,
    /// One record per slot of `order`, in the same order: the face's geometry
    /// copied next to its neighbours', plus a bounding ball. A leaf scan reads
    /// one contiguous run instead of three scattered per-face arrays.
    pub tris: Vec<LeafTri>,
}

/// The kernel's slack, in mm, on top of every bounding-ball distance test
/// (see [`LeafTri`]); the part that does not scale with the query.
pub const BALL_SLACK_MM: f64 = 1e-7;
/// A face whose `|e1 x e2|^2 / L^4` (`L` its longest edge) is below this gets
/// no ball: it is a needle, on which `closest_on_tri`'s interior branch can
/// return a point far outside the face (nearest.rs, `tri_is_well_conditioned`).
const BALL_MIN_SHAPE: f64 = 1e-4;
/// Coefficient of the rounding term of the ball slack, `k * D^2` mm for an
/// endpoint at distance `D` from the face: about 100 times the measured
/// worst-case position error `1e-16 * D^2 / (L * shape)` of the interior branch.
const BALL_ROUNDING_COEF: f64 = 1e-13;

/// One face, copied in leaf order.
///
/// `v0`, `e1`, `e2` are the mesh's own bits. `centre` and `radius` bound the
/// face in a ball (`radius` is the largest centre-to-vertex distance, rounded
/// UP by a relative `1e-12`). `rounding` is `BALL_ROUNDING_COEF * L^3 / |n|^2`:
/// the per-`D^2` part of the slack that covers the interior branch of
/// `closest_on_tri`, or infinity for a needle face, which switches the ball
/// test off for that face (a NaN or infinite slack never skips anything).
#[derive(Clone, Copy)]
pub struct LeafTri {
    pub v0: V3,
    pub e1: V3,
    pub e2: V3,
    pub centre: V3,
    pub radius: f64,
    pub rounding: f64,
}

impl LeafTri {
    pub(crate) fn new(v0: V3, e1: V3, e2: V3) -> Self {
        let v1 = [v0[0] + e1[0], v0[1] + e1[1], v0[2] + e1[2]];
        let v2 = [v0[0] + e2[0], v0[1] + e2[1], v0[2] + e2[2]];
        let mut lo = v0;
        let mut hi = v0;
        for v in [v1, v2] {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        let centre = [
            0.5 * (lo[0] + hi[0]),
            0.5 * (lo[1] + hi[1]),
            0.5 * (lo[2] + hi[2]),
        ];
        let mut r2 = 0.0f64;
        for v in [v0, v1, v2] {
            let d = [v[0] - centre[0], v[1] - centre[1], v[2] - centre[2]];
            r2 = r2.max(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
        }
        let radius = r2.sqrt() * (1.0 + 1e-12);
        let cr = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let n2 = cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2];
        let e12 = [e2[0] - e1[0], e2[1] - e1[1], e2[2] - e1[2]];
        let l2 = (e1[0] * e1[0] + e1[1] * e1[1] + e1[2] * e1[2])
            .max(e2[0] * e2[0] + e2[1] * e2[1] + e2[2] * e2[2])
            .max(e12[0] * e12[0] + e12[1] * e12[1] + e12[2] * e12[2]);
        // `shape = n2 / l2^2 >= BALL_MIN_SHAPE` also rejects NaN and zero.
        let rounding = if n2 >= BALL_MIN_SHAPE * l2 * l2 && n2 > 0.0 && radius.is_finite() {
            BALL_ROUNDING_COEF * l2 * l2.sqrt() / n2
        } else {
            f64::INFINITY
        };
        LeafTri {
            v0,
            e1,
            e2,
            centre,
            radius,
            rounding,
        }
    }
}

#[inline]
fn union_box(mn: &mut V3, mx: &mut V3, tmn: V3, tmx: V3) {
    for k in 0..3 {
        if tmn[k] < mn[k] {
            mn[k] = tmn[k];
        }
        if tmx[k] > mx[k] {
            mx[k] = tmx[k];
        }
    }
}

#[inline]
fn surface_area(mn: V3, mx: V3) -> f64 {
    let dx = (mx[0] - mn[0]).max(0.0);
    let dy = (mx[1] - mn[1]).max(0.0);
    let dz = (mx[2] - mn[2]).max(0.0);
    // half-surface-area is fine for SAH comparison (monotone); use full SA.
    2.0 * (dx * dy + dy * dz + dz * dx)
}

/// A mesh whose global face count is at most this many times its selected face
/// count builds its pad table indexed by global id; a sparser subset does not.
const DENSE_PAD_RATIO: usize = 8;

/// Per-triangle Möller pads, by one of two layouts that return the same value
/// for every selected face.
enum PadTable {
    /// One entry per GLOBAL face id; only selected faces are filled.
    Global(Vec<f64>),
    /// One entry per selected face, in the mesh's (sorted) selected order.
    Selected(Vec<f64>),
}

impl PadTable {
    #[inline]
    fn get(&self, mesh: &Mesh, face_id: u32) -> f64 {
        match self {
            PadTable::Global(by_global) => by_global[face_id as usize],
            PadTable::Selected(by_slot) => {
                let slot = mesh
                    .face_ids()
                    .binary_search(&face_id)
                    .expect("BVH order must contain only selected global face IDs");
                by_slot[slot]
            }
        }
    }
}

impl Bvh {
    pub fn build(mesh: &Mesh) -> Result<Bvh, String> {
        Self::build_with_cutoff(mesh, PARALLEL_MIN_FACES)
    }

    /// [`Self::build`] with the face count at and above which a range builds
    /// its two halves side by side. The cutoff only chooses who does the work;
    /// the tree is the same for every value (the tests build one mesh at
    /// several and compare every field).
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "float-to-integer casts on lattice or step arithmetic: Rust's `as` saturates at the \
              integer bounds and maps NaN to 0, and every site in this function is range-checked \
              before the cast or clamped into range after it (read site by site, 2026-08-26). See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    pub(crate) fn build_with_cutoff(mesh: &Mesh, parallel_min: usize) -> Result<Bvh, String> {
        Self::build_with_layout(mesh, parallel_min, None)
    }

    /// [`Self::build_with_cutoff`] with the pad table's layout chosen by the
    /// caller (`Some(true)` global, `Some(false)` selected-order) instead of by
    /// the subset's share of the surface. The tree is the same either way.
    pub(crate) fn build_with_layout(
        mesh: &Mesh,
        parallel_min: usize,
        force_dense: Option<bool>,
    ) -> Result<Bvh, String> {
        BVH_BUILD_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
        let n = mesh.nfaces();
        if n > MAX_FACES {
            return Err(format!(
                "BVH face count {n} exceeds the {MAX_FACES} representable in a \
                 node's i32 index fields"
            ));
        }
        // per-triangle Möller pad = 2*EPS*(||E1||_inf + ||E2||_inf) (A11)
        //
        // A mesh that holds a sizeable share of the global faces indexes the
        // table by GLOBAL face id (selected faces only are filled), so the node
        // pass below reads a face's pad directly. A small subset of a large
        // surface keeps one entry per selected face, in selected order, and
        // finds a face's entry by binary search: scratch stays proportional to
        // the subset. Either way the pad of a face is the same value.
        let n_global = mesh.global_nfaces();
        let dense = force_dense.unwrap_or(n_global <= n.saturating_mul(DENSE_PAD_RATIO));
        let pad_of = |face_id: u32| {
            let e1 = mesh.e1(face_id as usize);
            let e2 = mesh.e2(face_id as usize);
            let ni = e1[0].abs().max(e1[1].abs()).max(e1[2].abs());
            let mi = e2[0].abs().max(e2[1].abs()).max(e2[2].abs());
            2.0 * EPS * (ni + mi)
        };
        let tri_pad = if dense {
            let mut by_global = vec![0.0; n_global];
            for &face_id in mesh.face_ids() {
                by_global[face_id as usize] = pad_of(face_id);
            }
            PadTable::Global(by_global)
        } else {
            PadTable::Selected(mesh.face_ids().iter().map(|&face_id| pad_of(face_id)).collect())
        };

        // Leaf order carries global face IDs even for a narrowed mesh view.
        let mut order = mesh.face_ids().to_vec();
        let mut nodes: Vec<Node> = Vec::with_capacity(2 * n / LEAF_MAX + 1);
        let mut max_depth = 0usize;
        let depth_cap = (2.0 * (n as f64).log2()).ceil() as usize + 64;

        // Recursive build over order[lo..hi]; returns node index. A large range
        // builds its two halves on the kernel pool and splices their node
        // lists after its own node, left then right: the numbering of a
        // preorder walk, which is what the serial recursion produces.
        if n >= parallel_min.max(2) {
            crate::par::run(|| {
                build_range(
                    mesh,
                    &tri_pad,
                    &mut order,
                    0,
                    0,
                    n,
                    0,
                    &mut nodes,
                    &mut max_depth,
                    depth_cap,
                    parallel_min,
                )
            })?;
        } else {
            build_range(
                mesh,
                &tri_pad,
                &mut order,
                0,
                0,
                n,
                0,
                &mut nodes,
                &mut max_depth,
                depth_cap,
                usize::MAX,
            )?;
        }

        let tris = order
            .iter()
            .map(|&face_id| {
                let face = face_id as usize;
                LeafTri::new(mesh.v0(face), mesh.e1(face), mesh.e2(face))
            })
            .collect();

        Ok(Bvh {
            nodes,
            order,
            depth: max_depth,
            tris,
        })
    }
}

pub(crate) fn build_attempt_count() -> u64 {
    BVH_BUILD_ATTEMPTS.load(Ordering::Relaxed)
}

#[allow(clippy::too_many_arguments)]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "float-to-integer casts on lattice or step arithmetic: Rust's `as` saturates at the integer \
          bounds and maps NaN to 0, and every site in this function is range-checked before the cast \
          or clamped into range after it (read site by site, 2026-08-26); integer-width casts in \
          mesh/voxel index arithmetic, NOT individually audited -- triaged as a class, and the one \
          architecture item the class carried (geom_kernel's 32-bit BVH index width) is now a named, \
          enforced limit. TODO(cast-width-audit): retire site by site.  See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn build_range(
    mesh: &Mesh,
    tri_pad: &PadTable,
    // The slice of the whole face order that starts at index `base` of it;
    // `lo` and `hi` index this slice. Leaves record `base + lo`, so a range
    // built on its own slice writes the offsets the whole build would.
    order: &mut [u32],
    base: usize,
    lo: usize,
    hi: usize,
    depth: usize,
    nodes: &mut Vec<Node>,
    max_depth: &mut usize,
    depth_cap: usize,
    parallel_min: usize,
) -> Result<usize, String> {
    if depth > *max_depth {
        *max_depth = depth;
    }
    if depth > depth_cap {
        return Err(format!(
            "BVH depth {} exceeded cap {} — pathological soup (spec sec 3.5)",
            depth, depth_cap
        ));
    }

    // node AABB + pad over [lo,hi)
    let mut bmin = [f64::INFINITY; 3];
    let mut bmax = [f64::NEG_INFINITY; 3];
    let mut pad = 0.0f64;
    for &p in &order[lo..hi] {
        let pi = p as usize;
        union_box(&mut bmin, &mut bmax, mesh.tri_min(pi), mesh.tri_max(pi));
        // `order` holds only selected global face ids (`tri_pad` is filled for
        // exactly those).
        let face_pad = tri_pad.get(mesh, p);
        if face_pad > pad {
            pad = face_pad;
        }
    }

    let count = hi - lo;
    let node_idx = nodes.len();
    nodes.push(Node {
        bmin,
        bmax,
        pad,
        a: 0,
        b: 0,
        leaf: true,
    });

    if count <= LEAF_MAX {
        // `lo` and `count` are both `<= n <= MAX_FACES`, which `Bvh::build`
        // refuses above precisely so these two writes cannot wrap.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            reason = "bounded by MAX_FACES, enforced at the Bvh::build boundary"
        )]
        {
            nodes[node_idx].a = (base + lo) as i32;
            nodes[node_idx].b = count as i32;
        }
        nodes[node_idx].leaf = true;
        return Ok(node_idx);
    }

    // centroid bounds over the range
    let mut cmin = [f64::INFINITY; 3];
    let mut cmax = [f64::NEG_INFINITY; 3];
    for &p in &order[lo..hi] {
        let c = mesh.centroid(p as usize);
        for k in 0..3 {
            if c[k] < cmin[k] {
                cmin[k] = c[k];
            }
            if c[k] > cmax[k] {
                cmax[k] = c[k];
            }
        }
    }
    // axis = largest centroid extent, ties x<y<z
    let ext = [cmax[0] - cmin[0], cmax[1] - cmin[1], cmax[2] - cmin[2]];
    let mut axis = 0usize;
    if ext[1] > ext[axis] {
        axis = 1;
    }
    if ext[2] > ext[axis] {
        axis = 2;
    }

    let mut split_pos = lo + count / 2; // default = index median
    let mut use_median = ext[axis] <= 0.0;

    if !use_median {
        // binned SAH
        let clo = cmin[axis];
        let inv = N_BINS as f64 / ext[axis];
        // bin AABBs and counts
        let mut bin_min = [[f64::INFINITY; 3]; N_BINS];
        let mut bin_max = [[f64::NEG_INFINITY; 3]; N_BINS];
        let mut bin_cnt = [0usize; N_BINS];
        let mut bin_of = vec![0usize; count];
        for (j, &p) in order[lo..hi].iter().enumerate() {
            let pi = p as usize;
            let c = mesh.centroid(pi)[axis];
            let mut bi = ((c - clo) * inv) as usize;
            if bi >= N_BINS {
                bi = N_BINS - 1;
            }
            bin_of[j] = bi;
            union_box(
                &mut bin_min[bi],
                &mut bin_max[bi],
                mesh.tri_min(pi),
                mesh.tri_max(pi),
            );
            bin_cnt[bi] += 1;
        }
        // prefix (left) and suffix (right) sweeps over the 15 planes
        let mut best_cost = f64::INFINITY;
        let mut best_plane: isize = -1;
        // left accumulation
        let mut l_min = [[f64::INFINITY; 3]; N_BINS];
        let mut l_max = [[f64::NEG_INFINITY; 3]; N_BINS];
        let mut l_cnt = [0usize; N_BINS];
        {
            let mut mn = [f64::INFINITY; 3];
            let mut mx = [f64::NEG_INFINITY; 3];
            let mut c = 0usize;
            for i in 0..N_BINS {
                if bin_cnt[i] > 0 {
                    union_box(&mut mn, &mut mx, bin_min[i], bin_max[i]);
                }
                c += bin_cnt[i];
                l_min[i] = mn;
                l_max[i] = mx;
                l_cnt[i] = c;
            }
        }
        let mut r_min = [[f64::INFINITY; 3]; N_BINS];
        let mut r_max = [[f64::NEG_INFINITY; 3]; N_BINS];
        let mut r_cnt = [0usize; N_BINS];
        {
            let mut mn = [f64::INFINITY; 3];
            let mut mx = [f64::NEG_INFINITY; 3];
            let mut c = 0usize;
            for i in (0..N_BINS).rev() {
                if bin_cnt[i] > 0 {
                    union_box(&mut mn, &mut mx, bin_min[i], bin_max[i]);
                }
                c += bin_cnt[i];
                r_min[i] = mn;
                r_max[i] = mx;
                r_cnt[i] = c;
            }
        }
        // plane p splits bins [0..=p] | [p+1..]
        for p in 0..(N_BINS - 1) {
            let lc = l_cnt[p];
            let rc = r_cnt[p + 1];
            if lc == 0 || rc == 0 {
                continue;
            }
            let cost = surface_area(l_min[p], l_max[p]) * lc as f64
                + surface_area(r_min[p + 1], r_max[p + 1]) * rc as f64;
            if cost < best_cost {
                best_cost = cost;
                best_plane = p as isize;
            }
        }

        if best_plane < 0 {
            use_median = true;
        } else {
            // stable partition preserving index order within each side.
            // `l_cnt[plane]`/`r_cnt[plane + 1]` are this exact plane's exact
            // left/right counts, already computed by the sweep above, and
            // sum to `count` -- reserving `count` for each of two vectors
            // whose combined length is `count` would double-book capacity
            // this partition never uses.
            let plane = best_plane as usize;
            let mut left: Vec<u32> = Vec::with_capacity(l_cnt[plane]);
            let mut right: Vec<u32> = Vec::with_capacity(r_cnt[plane + 1]);
            for (j, &p) in order[lo..hi].iter().enumerate() {
                if bin_of[j] <= plane {
                    left.push(p);
                } else {
                    right.push(p);
                }
            }
            if left.is_empty() || right.is_empty() {
                use_median = true;
            } else {
                split_pos = lo + left.len();
                order[lo..lo + left.len()].copy_from_slice(&left);
                order[lo + left.len()..hi].copy_from_slice(&right);
            }
        }
    }

    if use_median {
        // index-median split: order[lo..hi] is already ascending by face index
        // -- the root order is `mesh.face_ids()`, contractually ascending
        // (`Mesh::subset`'s own debug_assert), and every recursive range
        // reaching here is either that root range or the untouched or
        // stably-repartitioned output of an earlier split, both of which
        // preserve per-side ascending order. So a midpoint split IS the
        // index median (spec sec 3.2) with no sort needed to make it one;
        // an earlier version of this function re-sorted the range here
        // "to be sure", which was strictly redundant work on data already
        // known sorted by construction.
        split_pos = lo + count / 2;
    }

    let (left, right) = if count >= parallel_min {
        // The two halves own disjoint slices of `order` and build into node
        // lists of their own (indices relative to each list's first node, which
        // is the numbering the serial recursion gives that subtree), then the
        // lists are appended here, left first, with child indices moved to
        // where the lists land: the serial numbering of the whole tree.
        let (left_order, right_order) = order[lo..hi].split_at_mut(split_pos - lo);
        let (left_len, right_len) = (left_order.len(), right_order.len());
        let half = |half_order: &mut [u32], half_base: usize, half_len: usize| {
            let mut half_nodes: Vec<Node> = Vec::new();
            let mut half_depth = 0usize;
            build_range(
                mesh,
                tri_pad,
                half_order,
                half_base,
                0,
                half_len,
                depth + 1,
                &mut half_nodes,
                &mut half_depth,
                depth_cap,
                parallel_min,
            )
            .map(|_| (half_nodes, half_depth))
        };
        let (left_result, right_result) = rayon::join(
            || half(left_order, base + lo, left_len),
            || half(right_order, base + split_pos, right_len),
        );
        // The serial walk reports the left half's refusal first.
        let (left_nodes, left_depth) = left_result?;
        let (right_nodes, right_depth) = right_result?;
        if left_depth.max(right_depth) > *max_depth {
            *max_depth = left_depth.max(right_depth);
        }
        let mut splice = |half_nodes: Vec<Node>| -> usize {
            let first = nodes.len();
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_possible_wrap,
                reason = "bounded by MAX_FACES, enforced at the Bvh::build boundary"
            )]
            let shift = first as i32;
            nodes.extend(half_nodes.into_iter().map(|mut node| {
                if !node.leaf {
                    node.a += shift;
                    node.b += shift;
                }
                node
            }));
            first
        };
        let left = splice(left_nodes);
        let right = splice(right_nodes);
        (left, right)
    } else {
        let left = build_range(
            mesh,
            tri_pad,
            order,
            base,
            lo,
            split_pos,
            depth + 1,
            nodes,
            max_depth,
            depth_cap,
            parallel_min,
        )?;
        let right = build_range(
            mesh,
            tri_pad,
            order,
            base,
            split_pos,
            hi,
            depth + 1,
            nodes,
            max_depth,
            depth_cap,
            parallel_min,
        )?;
        (left, right)
    };
    // Child node indices are `< nodes.len() <= 2n - 1 <= i32::MAX`, which
    // `Bvh::build`'s MAX_FACES refusal is exactly what bounds.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "bounded by MAX_FACES, enforced at the Bvh::build boundary"
    )]
    {
        nodes[node_idx].a = left as i32;
        nodes[node_idx].b = right as i32;
    }
    nodes[node_idx].leaf = false;
    Ok(node_idx)
}

#[cfg(test)]
mod instrumentation_tests {
    use super::*;

    /// The tree does not depend on the face count at which halves are built
    /// side by side: one mesh at several cutoffs (serial, every range, and in
    /// between) gives the same nodes, leaf order, depth and leaf records, bit
    /// for bit. A cloud of random triangles (SAH splits) and a mesh of many
    /// coincident triangles (the index-median fallback) are both covered.
    #[test]
    fn the_tree_is_the_same_at_every_parallel_cutoff() {
        fn lcg(state: &mut u64) -> f64 {
            *state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (*state >> 11) as f64 / (1_u64 << 53) as f64
        }
        let mut state = 0x5EED_B0B5_u64;
        let mut cloud_vertices = Vec::new();
        let mut cloud_faces = Vec::new();
        for face in 0..6000_u32 {
            let centre = [
                lcg(&mut state) * 40.0,
                lcg(&mut state) * 40.0,
                lcg(&mut state) * 40.0,
            ];
            for _ in 0..3 {
                cloud_vertices.push([
                    centre[0] + lcg(&mut state),
                    centre[1] + lcg(&mut state),
                    centre[2] + lcg(&mut state),
                ]);
            }
            cloud_faces.push([3 * face, 3 * face + 1, 3 * face + 2]);
        }
        let mut stack_vertices = Vec::new();
        let mut stack_faces = Vec::new();
        for face in 0..3000_u32 {
            // Many faces on the same three points: equal centroids everywhere.
            let lift = f64::from(face % 3);
            stack_vertices.push([0.0, 0.0, lift]);
            stack_vertices.push([1.0, 0.0, lift]);
            stack_vertices.push([0.0, 1.0, lift]);
            stack_faces.push([3 * face, 3 * face + 1, 3 * face + 2]);
        }
        for (vertices, faces) in [(cloud_vertices, cloud_faces), (stack_vertices, stack_faces)] {
            let mesh = Mesh::from_indexed(&vertices, &faces).unwrap();
            let serial = Bvh::build_with_cutoff(&mesh, usize::MAX).unwrap();
            for cutoff in [1, 9, 100, 1500, 4000] {
                let parallel = Bvh::build_with_cutoff(&mesh, cutoff).unwrap();
                assert_eq!(parallel.depth, serial.depth, "cutoff {cutoff}");
                assert_eq!(parallel.order, serial.order, "cutoff {cutoff}");
                assert_eq!(parallel.nodes.len(), serial.nodes.len(), "cutoff {cutoff}");
                for (index, (left, right)) in
                    parallel.nodes.iter().zip(&serial.nodes).enumerate()
                {
                    let bits = |node: &Node| {
                        (
                            node.bmin.map(f64::to_bits),
                            node.bmax.map(f64::to_bits),
                            node.pad.to_bits(),
                            node.a,
                            node.b,
                            node.leaf,
                        )
                    };
                    assert_eq!(bits(left), bits(right), "cutoff {cutoff}, node {index}");
                }
                for (index, (left, right)) in parallel.tris.iter().zip(&serial.tris).enumerate() {
                    let bits = |tri: &LeafTri| {
                        (
                            tri.v0.map(f64::to_bits),
                            tri.e1.map(f64::to_bits),
                            tri.e2.map(f64::to_bits),
                            tri.centre.map(f64::to_bits),
                            tri.radius.to_bits(),
                            tri.rounding.to_bits(),
                        )
                    };
                    assert_eq!(bits(left), bits(right), "cutoff {cutoff}, leaf {index}");
                }
            }
        }
    }

    /// A subset's tree does not depend on how the pad table is laid out: a
    /// sparse subset (one face, a few, every seventh) built with the
    /// selected-order table equals the same subset built with the global one,
    /// and the default choice follows the subset's share of the surface.
    #[test]
    fn the_tree_is_the_same_for_both_pad_layouts_on_a_subset() {
        let mut vertices = Vec::new();
        let mut faces = Vec::new();
        for face in 0..400_u32 {
            let x = f64::from(face % 20);
            let y = f64::from(face / 20);
            let size = 0.25 + f64::from(face % 7) * 0.5;
            vertices.push([x, y, 0.0]);
            vertices.push([x + size, y, 0.1 * f64::from(face % 5)]);
            vertices.push([x, y + size, 0.0]);
            faces.push([3 * face, 3 * face + 1, 3 * face + 2]);
        }
        let whole = Mesh::from_indexed(&vertices, &faces).unwrap();
        for selected in [
            vec![17_u32],
            vec![3, 4, 399],
            (0..400).step_by(7).collect::<Vec<u32>>(),
            (0..400).collect::<Vec<u32>>(),
        ] {
            let mesh = whole.subset(selected.clone());
            let global = Bvh::build_with_layout(&mesh, usize::MAX, Some(true)).unwrap();
            let slots = Bvh::build_with_layout(&mesh, usize::MAX, Some(false)).unwrap();
            let default = Bvh::build_with_cutoff(&mesh, usize::MAX).unwrap();
            for other in [&slots, &default] {
                assert_eq!(other.depth, global.depth, "{} faces", selected.len());
                assert_eq!(other.order, global.order, "{} faces", selected.len());
                assert_eq!(other.nodes.len(), global.nodes.len());
                for (left, right) in other.nodes.iter().zip(&global.nodes) {
                    assert_eq!(left.pad.to_bits(), right.pad.to_bits());
                    assert_eq!(left.bmin.map(f64::to_bits), right.bmin.map(f64::to_bits));
                    assert_eq!(left.bmax.map(f64::to_bits), right.bmax.map(f64::to_bits));
                    assert_eq!((left.a, left.b, left.leaf), (right.a, right.b, right.leaf));
                }
            }
        }
    }

    #[test]
    fn direct_build_crosses_attempt_boundary() {
        let mesh = Mesh::from_indexed(
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[[0, 1, 2]],
        )
        .unwrap();
        let before = build_attempt_count();
        Bvh::build(&mesh).unwrap();
        assert!(build_attempt_count() > before);
    }

    /// `MAX_FACES` exists to keep the `i32` node fields from wrapping, and it
    /// has to be the LAST face count for which that holds -- one face lower and
    /// it needlessly refuses a mesh a 32-bit tree could carry; one face higher
    /// and a build wraps into negative node indices with no error.
    ///
    /// This checks the property, not the arithmetic: the worst-case node count
    /// of a binary tree whose every leaf holds at least one face is `2n - 1`,
    /// and the question asked of each side of the boundary is simply whether
    /// that count still converts to `i32`.
    #[test]
    fn max_faces_is_the_last_count_whose_worst_case_tree_fits_i32() {
        let worst_case_nodes = |faces: usize| 2 * faces - 1;
        assert!(
            i32::try_from(worst_case_nodes(MAX_FACES)).is_ok(),
            "MAX_FACES refuses a mesh whose tree would have fitted"
        );
        assert!(
            i32::try_from(worst_case_nodes(MAX_FACES + 1)).is_err(),
            "one face past MAX_FACES must be the count that stops fitting"
        );
    }
}
