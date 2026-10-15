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

impl Bvh {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "float-to-integer casts on lattice or step arithmetic: Rust's `as` saturates at the \
              integer bounds and maps NaN to 0, and every site in this function is range-checked \
              before the cast or clamped into range after it (read site by site, 2026-08-26). See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    pub fn build(mesh: &Mesh) -> Result<Bvh, String> {
        BVH_BUILD_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
        let n = mesh.nfaces();
        if n > MAX_FACES {
            return Err(format!(
                "BVH face count {n} exceeds the {MAX_FACES} representable in a \
                 node's i32 index fields"
            ));
        }
        // per-triangle Möller pad = 2*EPS*(||E1||_inf + ||E2||_inf) (A11)
        let tri_pad: Vec<f64> = mesh
            .face_ids()
            .iter()
            .map(|&face_id| {
                let e1 = mesh.e1(face_id as usize);
                let e2 = mesh.e2(face_id as usize);
                let ni = e1[0].abs().max(e1[1].abs()).max(e1[2].abs());
                let mi = e2[0].abs().max(e2[1].abs()).max(e2[2].abs());
                2.0 * EPS * (ni + mi)
            })
            .collect();

        // Leaf order carries global face IDs even for a narrowed mesh view.
        let mut order = mesh.face_ids().to_vec();
        let mut nodes: Vec<Node> = Vec::with_capacity(2 * n / LEAF_MAX + 1);
        let mut max_depth = 0usize;
        let depth_cap = (2.0 * (n as f64).log2()).ceil() as usize + 64;

        // Recursive build over order[lo..hi]; returns node index.
        build_range(
            mesh,
            &tri_pad,
            &mut order,
            0,
            n,
            0,
            &mut nodes,
            &mut max_depth,
            depth_cap,
        )?;

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
    tri_pad: &[f64],
    order: &mut Vec<u32>,
    lo: usize,
    hi: usize,
    depth: usize,
    nodes: &mut Vec<Node>,
    max_depth: &mut usize,
    depth_cap: usize,
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
        let slot = mesh
            .face_slot(p)
            .expect("BVH order must contain only selected global face IDs");
        if tri_pad[slot] > pad {
            pad = tri_pad[slot];
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
            nodes[node_idx].a = lo as i32;
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

    let left = build_range(
        mesh,
        tri_pad,
        order,
        lo,
        split_pos,
        depth + 1,
        nodes,
        max_depth,
        depth_cap,
    )?;
    let right = build_range(
        mesh,
        tri_pad,
        order,
        split_pos,
        hi,
        depth + 1,
        nodes,
        max_depth,
        depth_cap,
    )?;
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
