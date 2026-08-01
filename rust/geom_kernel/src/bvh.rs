//! Deterministic AABB BVH over triangle indices (spec sec 3.2). No RNG, no
//! hash/pointer order: binned SAH on the largest-centroid-extent axis with
//! fixed tie rules and an index-median fallback. Two builds of the same mesh
//! are structurally identical.

use crate::consts::EPS;
use crate::mesh::Mesh;
use crate::vec3::V3;

pub const LEAF_MAX: usize = 8;
const N_BINS: usize = 16;

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
    pub fn build(mesh: &Mesh) -> Result<Bvh, String> {
        let n = mesh.nfaces();
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

        Ok(Bvh {
            nodes,
            order,
            depth: max_depth,
        })
    }
}

#[allow(clippy::too_many_arguments)]
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
        nodes[node_idx].a = lo as i32;
        nodes[node_idx].b = count as i32;
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
            // stable partition preserving index order within each side
            let plane = best_plane as usize;
            let mut left: Vec<u32> = Vec::with_capacity(count);
            let mut right: Vec<u32> = Vec::with_capacity(count);
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
        // (stable partitions preserve the initial ascending order), so a
        // midpoint split IS the index median (spec sec 3.2).
        split_pos = lo + count / 2;
        // ensure the range is sorted by face index for a true index median
        order[lo..hi].sort_unstable();
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
    nodes[node_idx].a = left as i32;
    nodes[node_idx].b = right as i32;
    nodes[node_idx].leaf = false;
    Ok(node_idx)
}
