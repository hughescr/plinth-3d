//! Kc oracle (clearance kernel speed, 2026-10-15): the faster segment-to-surface
//! distance kernel returns the SAME bits and the SAME winning face as the kernel
//! it replaced.
//!
//! This file carries VERBATIM COPIES of the code that was replaced, as the
//! reference: the eager `closest_on_tri`, `seg_tri_dist`, the stack walk, the
//! band walk and `seg_face_below`. They are not to be "improved"; they are the
//! definition of the answer. Every test compares new against reference by
//! `to_bits()`.
//!
//! What can differ between the two, and what each test pins:
//!
//! * `closest_on_tri` builds only the winning point now. Same point, bit for bit
//!   (`closest_lazy_is_the_eager_function`).
//! * `seg_tri_dist_geom` runs the lazy `closest_on_tri`, shares the edge
//!   vectors, and rejects most rays early. Same triple
//!   (`face_triple_is_the_reference_triple`).
//! * `seg_tri_dist_ball` may answer `INFINITY` for a face a bounding ball proves
//!   is not below the caller's bound. The property is: it agrees with the
//!   reference on `d < bound`, on `np`, and, when `d < bound`, on every bit
//!   (`ball_answer_agrees_where_it_matters`).
//! * The walks (distance, band, face-below) use that answer. Same distance bits,
//!   witness bits, near-parallel tally, stop flag and WINNING FACE ID
//!   (`walk_*`, `band_*`, `face_below_*`).
//!
//! `KC_ORACLE_SEGMENTS` sets the segments per mesh (default small; the
//! `million` test runs a million over the mesh set). `KC_STL` adds a binary STL.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::float_cmp,
    clippy::too_many_arguments,
    reason = "test code: integer index casts on small meshes, and bit-exact float comparison IS the assertion"
)]

use rayon::prelude::*;

use super::{
    box_box_dist, box_max, box_min, seg_ray_class, seg_tri_dist_ball, seg_tri_dist_geom, SegPre,
};
use crate::bvh::LeafTri;
use crate::closest::{closest_on_tri, segment_segment_distance_min};
use crate::moller::{ray_triangle_classified, RayTriangleClass};
use crate::queries::NativeSurface;
use crate::testkit::build_from_tris;
use crate::vec3::{add, dot, norm, scale, sub, V3};

// ---------------------------------------------------------------------------
// The reference: verbatim copies of the code the lane replaced.
// ---------------------------------------------------------------------------

/// The eager `closest_on_tri`: all seven candidates built, then chosen by mask.
fn ref_closest_on_tri(p: V3, a: V3, b: V3, cc: V3) -> V3 {
    let ab = sub(b, a);
    let ac = sub(cc, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    let cp = sub(p, cc);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    let vc = d1 * d4 - d3 * d2;
    let vb = d5 * d2 - d1 * d6;
    let va = d3 * d6 - d5 * d4;
    let denom = 1.0 / (va + vb + vc + crate::consts::CLOSEST_DENOM_REG);
    let v = vb * denom;
    let w = vc * denom;
    let base = add(add(a, scale(ab, v)), scale(ac, w));
    let den_ab = d1 - d3;
    let t_ab = if den_ab != 0.0 { d1 / den_ab } else { 0.0 };
    let den_ac = d2 - d6;
    let t_ac = if den_ac != 0.0 { d2 / den_ac } else { 0.0 };
    let den_bc = (d4 - d3) + (d5 - d6);
    let t_bc = if den_bc != 0.0 {
        (d4 - d3) / den_bc
    } else {
        0.0
    };
    let c_ab = add(a, scale(ab, t_ab));
    let c_ac = add(a, scale(ac, t_ac));
    let c_bc = add(b, scale(sub(cc, b), t_bc));
    let m_bc = (va <= 0.0) && ((d4 - d3) >= 0.0) && ((d5 - d6) >= 0.0);
    let m_ac = (vb <= 0.0) && (d2 >= 0.0) && (d6 <= 0.0);
    let m_ab = (vc <= 0.0) && (d1 >= 0.0) && (d3 <= 0.0);
    let m_c = (d6 >= 0.0) && (d5 <= d6);
    let m_b = (d3 >= 0.0) && (d4 <= d3);
    let m_a = (d1 <= 0.0) && (d2 <= 0.0);
    if m_a {
        a
    } else if m_b {
        b
    } else if m_c {
        cc
    } else if m_ab {
        c_ab
    } else if m_ac {
        c_ac
    } else if m_bc {
        c_bc
    } else {
        base
    }
}

/// The old `seg_tri_dist` body, on the face's own `(v0, e1, e2)`.
fn ref_seg_tri_dist(a: V3, b: V3, v0: V3, e1: V3, e2: V3) -> (f64, V3, bool) {
    let p = v0;
    let q = add(v0, e1);
    let r = add(v0, e2);
    let mut best_d = f64::INFINITY;
    let mut best_pt: V3 = [0.0; 3];
    let cpa = ref_closest_on_tri(a, p, q, r);
    let da = norm(sub(a, cpa));
    if da < best_d {
        best_d = da;
        best_pt = cpa;
    }
    let cpb = ref_closest_on_tri(b, p, q, r);
    let db = norm(sub(b, cpb));
    if db < best_d {
        best_d = db;
        best_pt = cpb;
    }
    for &(x, y) in &[(p, q), (q, r), (r, p)] {
        let (d, _c1, c2) = segment_segment_distance_min(a, b, x, y);
        if d < best_d {
            best_d = d;
            best_pt = c2;
        }
    }
    let dv = sub(b, a);
    let mut near_parallel = false;
    match ray_triangle_classified(a, dv, v0, e1, e2, 1.0) {
        RayTriangleClass::Hit(hit) => {
            let pierce = add(a, scale(dv, hit.t));
            if 0.0 < best_d {
                best_d = 0.0;
                best_pt = pierce;
            }
        }
        RayTriangleClass::NearParallel => {
            near_parallel = true;
        }
        RayTriangleClass::ExactParallel | RayTriangleClass::Miss => {}
    }
    (best_d, best_pt, near_parallel)
}

struct RefWalk {
    d: f64,
    pt: V3,
    np: u32,
    stopped: bool,
    winner: Option<u32>,
    tested: u64,
    visited: u64,
    pruned: u64,
}

/// The old distance walk: `STACK` of node ids, the child distance measured twice.
fn ref_walk(
    s: &NativeSurface,
    a: V3,
    b: V3,
    excluded: &[u32],
    slack: f64,
    stop: &dyn Fn(f64) -> bool,
) -> RefWalk {
    let smin = box_min(a, b);
    let smax = box_max(a, b);
    let mut out = RefWalk {
        d: f64::INFINITY,
        pt: [f64::NAN; 3],
        np: 0,
        stopped: false,
        winner: None,
        tested: 0,
        visited: 0,
        pruned: 0,
    };
    let mut stack: Vec<i32> = vec![0];
    while let Some(ni) = stack.pop() {
        out.visited += 1;
        let node = &s.bvh.nodes[ni as usize];
        if box_box_dist(smin, smax, node.bmin, node.bmax) - slack > out.d {
            out.pruned += 1;
            continue;
        }
        if node.leaf {
            let start = node.a as usize;
            let cnt = node.b as usize;
            for &tid in &s.bvh.order[start..start + cnt] {
                if !excluded.is_empty() && excluded.binary_search(&tid).is_ok() {
                    continue;
                }
                out.tested += 1;
                let f = tid as usize;
                let (d, pt, np) =
                    ref_seg_tri_dist(a, b, s.mesh.v0(f), s.mesh.e1(f), s.mesh.e2(f));
                if np {
                    out.np += 1;
                }
                if d < out.d {
                    out.d = d;
                    out.pt = pt;
                    out.winner = Some(tid);
                    if stop(out.d) {
                        out.stopped = true;
                        break;
                    }
                }
            }
            if out.stopped {
                break;
            }
        } else {
            let l = node.a as usize;
            let r = node.b as usize;
            let dl = box_box_dist(smin, smax, s.bvh.nodes[l].bmin, s.bvh.nodes[l].bmax);
            let dr = box_box_dist(smin, smax, s.bvh.nodes[r].bmin, s.bvh.nodes[r].bmax);
            if dl <= dr {
                stack.push(r as i32);
                stack.push(l as i32);
            } else {
                stack.push(l as i32);
                stack.push(r as i32);
            }
        }
    }
    out
}

/// The old `faces_near_segment` traversal (no limit), returning faces and the tally.
fn ref_band(s: &NativeSurface, a: V3, b: V3, within: f64) -> (Vec<u32>, u32) {
    let smin = box_min(a, b);
    let smax = box_max(a, b);
    let mut found: Vec<u32> = Vec::new();
    let mut np_total = 0u32;
    let mut stack: Vec<i32> = vec![0];
    while let Some(ni) = stack.pop() {
        let node = &s.bvh.nodes[ni as usize];
        if box_box_dist(smin, smax, node.bmin, node.bmax) > within {
            continue;
        }
        if node.leaf {
            let start = node.a as usize;
            let cnt = node.b as usize;
            for &tid in &s.bvh.order[start..start + cnt] {
                let f = tid as usize;
                let (d, _, np) =
                    ref_seg_tri_dist(a, b, s.mesh.v0(f), s.mesh.e1(f), s.mesh.e2(f));
                if np {
                    np_total += 1;
                }
                if d <= within {
                    found.push(tid);
                }
            }
        } else {
            stack.push(node.a);
            stack.push(node.b);
        }
    }
    found.sort_unstable();
    found.dedup();
    (found, np_total)
}

/// The old `seg_face_below`.
fn ref_face_below(s: &NativeSurface, a: V3, b: V3, thresh: f64, at_least: f64) -> bool {
    let within = at_least + thresh;
    if !(within > 0.0 && within.is_finite()) {
        return false;
    }
    let length = norm(sub(b, a));
    if !length.is_finite() {
        return false;
    }
    let pieces = (length / within).ceil().clamp(1.0, f64::from(u16::MAX)) as usize;
    let mut stack: Vec<i32> = Vec::new();
    for piece in 0..pieces {
        let t0 = piece as f64 / pieces as f64;
        let t1 = (piece + 1) as f64 / pieces as f64;
        let p = add(a, scale(sub(b, a), t0));
        let q = add(a, scale(sub(b, a), t1));
        let (pmin, pmax) = (box_min(p, q), box_max(p, q));
        stack.clear();
        stack.push(0);
        while let Some(ni) = stack.pop() {
            let node = &s.bvh.nodes[ni as usize];
            if box_box_dist(pmin, pmax, node.bmin, node.bmax) - thresh >= at_least {
                continue;
            }
            if node.leaf {
                let start = node.a as usize;
                let cnt = node.b as usize;
                for &tid in &s.bvh.order[start..start + cnt] {
                    let f = tid as usize;
                    let (d, _, _) =
                        ref_seg_tri_dist(a, b, s.mesh.v0(f), s.mesh.e1(f), s.mesh.e2(f));
                    if d - thresh < at_least {
                        return true;
                    }
                }
            } else {
                let l = node.a as usize;
                let r = node.b as usize;
                let dl = box_box_dist(pmin, pmax, s.bvh.nodes[l].bmin, s.bvh.nodes[l].bmax);
                let dr = box_box_dist(pmin, pmax, s.bvh.nodes[r].bmin, s.bvh.nodes[r].bmax);
                if dl <= dr {
                    stack.push(r as i32);
                    stack.push(l as i32);
                } else {
                    stack.push(l as i32);
                    stack.push(r as i32);
                }
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Fixtures.
// ---------------------------------------------------------------------------

/// xorshift64*, so every run draws the same numbers.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn unit(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let bits = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11;
        (bits as f64) / ((1u64 << 53) as f64)
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
    fn below(&mut self, n: usize) -> usize {
        ((self.unit() * n as f64) as usize).min(n - 1)
    }
    fn dir(&mut self) -> V3 {
        loop {
            let v = [
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
            ];
            let n = norm(v);
            if n > 1e-3 && n <= 1.0 {
                return scale(v, 1.0 / n);
            }
        }
    }
}

struct Fixture {
    name: &'static str,
    surface: NativeSurface,
    tris: Vec<f64>,
    lo: V3,
    hi: V3,
}

fn fixture(name: &'static str, tris: Vec<f64>) -> Fixture {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for v in tris.chunks_exact(3) {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    Fixture {
        name,
        surface: build_from_tris(tris.clone()),
        tris,
        lo,
        hi,
    }
}

fn push_tri(tris: &mut Vec<f64>, a: V3, b: V3, c: V3) {
    tris.extend_from_slice(&a);
    tris.extend_from_slice(&b);
    tris.extend_from_slice(&c);
}

fn hills(n: usize) -> Vec<f64> {
    let side = 12.0;
    let step = side / n as f64;
    let at = |i: usize, j: usize| -> V3 {
        let x = -6.0 + step * i as f64;
        let y = -6.0 + step * j as f64;
        [x, y, 0.7 * (0.9 * x).sin() * (0.7 * y).cos()]
    };
    let mut t = Vec::new();
    for i in 0..n {
        for j in 0..n {
            push_tri(&mut t, at(i, j), at(i + 1, j), at(i + 1, j + 1));
            push_tri(&mut t, at(i, j), at(i + 1, j + 1), at(i, j + 1));
        }
    }
    t
}

/// Icosphere with radial noise: a closed, creature-like surface of mixed face sizes.
fn noisy_sphere(subdiv: usize, radius: f64, noise: f64, seed: u64) -> Vec<f64> {
    let g = (1.0 + 5.0_f64.sqrt()) / 2.0;
    let mut verts: Vec<V3> = vec![
        [-1.0, g, 0.0],
        [1.0, g, 0.0],
        [-1.0, -g, 0.0],
        [1.0, -g, 0.0],
        [0.0, -1.0, g],
        [0.0, 1.0, g],
        [0.0, -1.0, -g],
        [0.0, 1.0, -g],
        [g, 0.0, -1.0],
        [g, 0.0, 1.0],
        [-g, 0.0, -1.0],
        [-g, 0.0, 1.0],
    ];
    let mut faces: Vec<[usize; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    for _ in 0..subdiv {
        let mut cache = std::collections::BTreeMap::new();
        let mut next = Vec::with_capacity(faces.len() * 4);
        let mut mid = |verts: &mut Vec<V3>, i: usize, j: usize| -> usize {
            let key = (i.min(j), i.max(j));
            *cache.entry(key).or_insert_with(|| {
                let m = scale(add(verts[i], verts[j]), 0.5);
                verts.push(m);
                verts.len() - 1
            })
        };
        for f in &faces {
            let ab = mid(&mut verts, f[0], f[1]);
            let bc = mid(&mut verts, f[1], f[2]);
            let ca = mid(&mut verts, f[2], f[0]);
            next.push([f[0], ab, ca]);
            next.push([f[1], bc, ab]);
            next.push([f[2], ca, bc]);
            next.push([ab, bc, ca]);
        }
        faces = next;
    }
    let mut rng = Rng::new(seed);
    let pts: Vec<V3> = verts
        .iter()
        .map(|&v| {
            let u = scale(v, 1.0 / norm(v));
            // Smooth lobes plus per-vertex roughness.
            let lobe = 0.5 * (3.0 * u[0]).sin() * (2.0 * u[1]).cos() + 0.3 * (4.0 * u[2]).sin();
            scale(u, radius * (1.0 + noise * (lobe + rng.range(-0.3, 0.3))))
        })
        .collect();
    let mut t = Vec::new();
    for f in &faces {
        push_tri(&mut t, pts[f[0]], pts[f[1]], pts[f[2]]);
    }
    t
}

/// A box lattice on a 0.5 mm grid: axis-aligned faces, exact coordinates, ties.
fn lattice(cells: usize) -> Vec<f64> {
    let h = 0.5;
    let n = cells as f64 * h;
    let mut t = Vec::new();
    for i in 0..cells {
        for j in 0..cells {
            let (x0, x1) = (i as f64 * h, (i + 1) as f64 * h);
            let (y0, y1) = (j as f64 * h, (j + 1) as f64 * h);
            // six sides of a cube, two triangles each, plus an interior wall
            for &z in &[0.0, n] {
                push_tri(&mut t, [x0, y0, z], [x1, y0, z], [x1, y1, z]);
                push_tri(&mut t, [x0, y0, z], [x1, y1, z], [x0, y1, z]);
                push_tri(&mut t, [x0, z, y0], [x1, z, y0], [x1, z, y1]);
                push_tri(&mut t, [x0, z, y0], [x1, z, y1], [x0, z, y1]);
                push_tri(&mut t, [z, x0, y0], [z, x1, y0], [z, x1, y1]);
                push_tri(&mut t, [z, x0, y0], [z, x1, y1], [z, x0, y1]);
            }
            let z = n * 0.5;
            push_tri(&mut t, [x0, y0, z], [x1, y0, z], [x1, y1, z]);
            push_tri(&mut t, [x0, y0, z], [x1, y1, z], [x0, y1, z]);
        }
    }
    t
}

/// A soup of every awkward face: slivers, needles, zero area, repeats, tiny, huge.
fn awkward(n: usize, seed: u64) -> Vec<f64> {
    let mut rng = Rng::new(seed);
    let mut t = Vec::new();
    for k in 0..n {
        let c = [
            rng.range(-8.0, 8.0),
            rng.range(-8.0, 8.0),
            rng.range(-8.0, 8.0),
        ];
        let u = rng.dir();
        let v = rng.dir();
        match k % 11 {
            0 => {
                // ordinary
                let s = rng.range(0.2, 3.0);
                push_tri(&mut t, c, add(c, scale(u, s)), add(c, scale(v, s)));
            }
            1 => {
                // needle: 1e-6 wide, 6 long
                push_tri(&mut t, c, add(c, scale(u, 6.0)), add(c, scale(v, 1e-6)));
            }
            2 => {
                // zero area: collinear
                push_tri(
                    &mut t,
                    c,
                    add(c, scale(u, 1.0)),
                    add(c, scale(u, 2.5)),
                );
            }
            3 => {
                // repeated vertex
                push_tri(&mut t, c, c, add(c, scale(u, 1.5)));
            }
            4 => {
                // tiny
                push_tri(
                    &mut t,
                    c,
                    add(c, scale(u, 1e-6)),
                    add(c, scale(v, 1e-6)),
                );
            }
            5 => {
                // huge
                push_tri(
                    &mut t,
                    c,
                    add(c, scale(u, 60.0)),
                    add(c, scale(v, 60.0)),
                );
            }
            6 => {
                // sliver: aspect 1:1e4
                push_tri(&mut t, c, add(c, scale(u, 4.0)), add(c, scale(v, 4e-4)));
            }
            7 => {
                // identical pair (exact ties between two faces)
                let s = rng.range(0.3, 2.0);
                let tri = [c, add(c, scale(u, s)), add(c, scale(v, s))];
                push_tri(&mut t, tri[0], tri[1], tri[2]);
                push_tri(&mut t, tri[0], tri[1], tri[2]);
            }
            8 => {
                // far away
                let f = [c[0] + 900.0, c[1] - 700.0, c[2] + 300.0];
                push_tri(&mut t, f, add(f, scale(u, 2.0)), add(f, scale(v, 2.0)));
            }
            9 => {
                // thin but not a needle: aspect 1:50
                push_tri(&mut t, c, add(c, scale(u, 2.0)), add(c, scale(v, 0.04)));
            }
            _ => {
                // axis-aligned, exact coordinates
                let q = [c[0].round(), c[1].round(), c[2].round()];
                push_tri(
                    &mut t,
                    q,
                    [q[0] + 1.0, q[1], q[2]],
                    [q[0], q[1] + 1.0, q[2]],
                );
            }
        }
    }
    t
}

/// A binary STL, when `KC_STL` names one (bench and oracle on a real model).
fn read_stl(path: &str) -> Option<Vec<f64>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 84 {
        return None;
    }
    let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
    if bytes.len() < 84 + 50 * n {
        return None;
    }
    let mut t = Vec::with_capacity(n * 9);
    for i in 0..n {
        let base = 84 + 50 * i + 12;
        for k in 0..9 {
            let o = base + 4 * k;
            t.push(f64::from(f32::from_le_bytes([
                bytes[o],
                bytes[o + 1],
                bytes[o + 2],
                bytes[o + 3],
            ])));
        }
    }
    Some(t)
}

fn fixtures() -> Vec<Fixture> {
    let mut v = vec![
        fixture("hills", hills(40)),
        fixture("sphere", noisy_sphere(4, 20.0, 0.15, 1)),
        fixture("lattice", lattice(8)),
        fixture("awkward", awkward(1500, 7)),
        fixture("small-sphere", noisy_sphere(2, 3.0, 0.3, 2)),
    ];
    if let Ok(path) = std::env::var("KC_STL") {
        if let Some(t) = read_stl(&path) {
            v.push(fixture("stl", t));
        }
    }
    v
}

fn segments_per_mesh(default: usize) -> usize {
    std::env::var("KC_ORACLE_SEGMENTS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// One test segment, drawn near the fixture's surface or at a special place.
fn draw_segment(f: &Fixture, rng: &mut Rng) -> (V3, V3) {
    let nf = f.tris.len() / 9;
    let span = norm(sub(f.hi, f.lo)).max(1e-3);
    let face = rng.below(nf);
    let tri = |k: usize| -> V3 {
        [
            f.tris[face * 9 + 3 * k],
            f.tris[face * 9 + 3 * k + 1],
            f.tris[face * 9 + 3 * k + 2],
        ]
    };
    let (p, q, r) = (tri(0), tri(1), tri(2));
    let on_face = |rng: &mut Rng| -> V3 {
        let (mut u, mut v) = (rng.unit(), rng.unit());
        if u + v > 1.0 {
            u = 1.0 - u;
            v = 1.0 - v;
        }
        add(add(p, scale(sub(q, p), u)), scale(sub(r, p), v))
    };
    let reach = (0.05 * span).min(6.0);
    match rng.below(10) {
        // uniform in the box (far from the surface mostly)
        0 => {
            let pt = |rng: &mut Rng| -> V3 {
                [
                    rng.range(f.lo[0] - 0.2 * span, f.hi[0] + 0.2 * span),
                    rng.range(f.lo[1] - 0.2 * span, f.hi[1] + 0.2 * span),
                    rng.range(f.lo[2] - 0.2 * span, f.hi[2] + 0.2 * span),
                ]
            };
            (pt(rng), pt(rng))
        }
        // near the surface, short (a clearance arm)
        1..=4 => {
            let a = add(on_face(rng), scale(rng.dir(), rng.range(0.0, reach)));
            let b = add(a, scale(rng.dir(), rng.range(0.1, 2.0 * reach)));
            (a, b)
        }
        // near the surface, long
        5 => {
            let a = add(on_face(rng), scale(rng.dir(), rng.range(0.0, reach)));
            let b = add(a, scale(rng.dir(), rng.range(0.2 * span, 0.8 * span)));
            (a, b)
        }
        // endpoint exactly on a vertex or an edge
        6 => {
            let a = match rng.below(4) {
                0 => p,
                1 => q,
                2 => r,
                _ => {
                    let t = rng.unit();
                    add(p, scale(sub(q, p), t))
                }
            };
            let b = add(a, scale(rng.dir(), rng.range(0.05, reach.max(0.1))));
            (a, b)
        }
        // parallel to the face plane, at a small height (0 = coplanar)
        7 => {
            let e = sub(q, p);
            let h = [0.0, 1e-9, 1e-6, 0.01, 0.3][rng.below(5)];
            let n = {
                let c = crate::vec3::cross(e, sub(r, p));
                let l = norm(c);
                if l > 0.0 {
                    scale(c, 1.0 / l)
                } else {
                    [0.0, 0.0, 1.0]
                }
            };
            let a = add(add(on_face(rng), scale(n, h)), scale(e, rng.range(-1.0, 0.0)));
            let b = add(a, scale(e, rng.range(0.5, 3.0)));
            (a, b)
        }
        // axis-aligned on the 0.5 grid (exact ties on the lattice)
        8 => {
            let g = |rng: &mut Rng| -> f64 { (rng.range(-1.0, 5.0) * 2.0).round() * 0.5 };
            let a = [g(rng), g(rng), g(rng)];
            let mut b = a;
            b[rng.below(3)] += (rng.range(-3.0, 3.0) * 2.0).round() * 0.5;
            (a, b)
        }
        // zero or tiny length
        _ => {
            let a = add(on_face(rng), scale(rng.dir(), rng.range(0.0, reach)));
            let len = [0.0, 1e-12, 1e-9, 1e-4][rng.below(4)];
            (a, add(a, scale(rng.dir(), len)))
        }
    }
}

fn same_bits(x: f64, y: f64) -> bool {
    x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan())
}

fn same_pt(x: V3, y: V3) -> bool {
    same_bits(x[0], y[0]) && same_bits(x[1], y[1]) && same_bits(x[2], y[2])
}

fn next_up(x: f64) -> f64 {
    if x.is_finite() {
        f64::from_bits(if x >= 0.0 { x.to_bits() + 1 } else { x.to_bits() - 1 })
    } else {
        x
    }
}

fn next_down(x: f64) -> f64 {
    -next_up(-x)
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

/// `closest_on_tri` is the oracle's point, bit for bit, in every region.
#[test]
fn closest_lazy_is_the_eager_function() {
    let mut rng = Rng::new(11);
    let mut regions = [0u64; 7];
    for k in 0..400_000 {
        let c = [rng.range(-5.0, 5.0), rng.range(-5.0, 5.0), rng.range(-5.0, 5.0)];
        let s = [0.5, 1.0, 1e-3, 1e-7, 30.0][k % 5];
        let (a, b, cc) = match k % 7 {
            // needle and sliver
            0 => (c, add(c, scale(rng.dir(), s)), add(c, scale(rng.dir(), 1e-6 * s))),
            // collinear
            1 => {
                let u = rng.dir();
                (c, add(c, scale(u, s)), add(c, scale(u, 2.0 * s)))
            }
            // axis-aligned exact
            2 => ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            // repeated vertex
            3 => (c, c, add(c, scale(rng.dir(), s))),
            _ => (
                c,
                add(c, scale(rng.dir(), s)),
                add(c, scale(rng.dir(), s)),
            ),
        };
        let p = match rng.below(6) {
            0 => a,
            1 => b,
            2 => cc,
            3 => scale(add(a, b), 0.5),
            4 => [rng.range(-2.0, 2.0), rng.range(-2.0, 2.0), 0.0],
            _ => add(c, scale(rng.dir(), rng.range(0.0, 300.0))),
        };
        let new = closest_on_tri(p, a, b, cc);
        let old = ref_closest_on_tri(p, a, b, cc);
        assert!(same_pt(new, old), "p={p:?} a={a:?} b={b:?} c={cc:?}: {new:?} vs {old:?}");
        let ab = sub(b, a);
        let ac = sub(cc, a);
        let region = if new == a {
            0
        } else if new == b {
            1
        } else if new == cc {
            2
        } else {
            3
        };
        regions[region] += 1;
        let lazy_edges = crate::closest::closest_on_tri_edges(p, a, b, cc, ab, ac);
        assert!(same_pt(lazy_edges, old));
    }
    assert!(regions.iter().take(4).all(|&n| n > 1000), "regions hit {regions:?}");
}

/// Face-level: `seg_tri_dist_geom` is the old triple; the ball answer agrees
/// where it matters, for bounds on both sides of the true distance.
fn check_face(a: V3, b: V3, v0: V3, e1: V3, e2: V3, skips: &mut u64, cases: &mut u64) {
    let old = ref_seg_tri_dist(a, b, v0, e1, e2);
    let new = seg_tri_dist_geom(a, b, v0, e1, e2);
    assert!(
        same_bits(new.0, old.0) && same_pt(new.1, old.1) && new.2 == old.2,
        "triple differs: a={a:?} b={b:?} v0={v0:?} e1={e1:?} e2={e2:?}: new {new:?} old {old:?}"
    );
    // The ray fast path is the full function's class.
    let class = |c: RayTriangleClass| match c {
        RayTriangleClass::Hit(h) => (0, h.t.to_bits()),
        RayTriangleClass::Miss => (1, 0),
        RayTriangleClass::NearParallel => (2, 0),
        RayTriangleClass::ExactParallel => (3, 0),
    };
    let dv = sub(b, a);
    assert_eq!(
        class(seg_ray_class(a, dv, v0, e1, e2)),
        class(ray_triangle_classified(a, dv, v0, e1, e2, 1.0))
    );
    let tri = LeafTri::new(v0, e1, e2);
    let seg = SegPre::new(a, b);
    let mut bounds = vec![f64::INFINITY, 0.0, 1e-9, 0.3, 1.0, 5.0, 40.0];
    if old.0.is_finite() {
        let d = old.0;
        bounds.extend_from_slice(&[
            d,
            next_up(d),
            next_down(d),
            d + 1e-9,
            d - 1e-9,
            d + 1e-5,
            d - 1e-5,
            d * 0.5,
            d * 0.999,
            d * 1.001,
            d + 0.01,
            d - 0.01,
        ]);
    }
    for bound in bounds {
        let got = seg_tri_dist_ball(&seg, &tri, bound);
        let below_old = old.0 < bound;
        let below_got = got.0 < bound;
        assert_eq!(below_old, below_got, "bound {bound} d_old {} d_got {}: a={a:?} b={b:?} v0={v0:?} e1={e1:?} e2={e2:?}", old.0, got.0);
        assert_eq!(old.2, got.2, "np differs at bound {bound}");
        if below_old {
            assert!(
                same_bits(got.0, old.0) && same_pt(got.1, old.1),
                "bound {bound}: {got:?} vs {old:?}"
            );
        }
        *cases += 1;
        if got.0 == f64::INFINITY && old.0.is_finite() {
            *skips += 1;
            // a skip must be a real "at least": the old distance is not below the bound
            assert!(old.0 >= bound || old.0.is_nan());
        }
    }
}

fn random_tri(rng: &mut Rng) -> (V3, V3, V3) {
    let c = [rng.range(-10.0, 10.0), rng.range(-10.0, 10.0), rng.range(-10.0, 10.0)];
    let s = [0.05, 0.5, 2.0, 10.0, 1e-3][rng.below(5)];
    let mut e1 = scale(rng.dir(), s * rng.range(0.1, 1.0));
    let mut e2 = scale(rng.dir(), s * rng.range(0.1, 1.0));
    match rng.below(8) {
        0 => e2 = scale(e1, rng.range(-2.0, 2.0)), // collinear
        1 => e2 = add(scale(e1, 0.5), scale(rng.dir(), 1e-6 * s)), // needle
        2 => {
            e1 = [s, 0.0, 0.0];
            e2 = [0.0, s, 0.0];
        }
        3 => e1 = [0.0; 3], // repeated vertex
        _ => {}
    }
    (c, e1, e2)
}

#[test]
fn face_triple_is_the_reference_triple() {
    let mut rng = Rng::new(21);
    let (mut skips, mut cases) = (0u64, 0u64);
    for _ in 0..60_000 {
        let (v0, e1, e2) = random_tri(&mut rng);
        let tri = [v0, add(v0, e1), add(v0, e2)];
        let a = match rng.below(6) {
            0 => tri[rng.below(3)],
            1 => add(tri[0], scale(sub(tri[1], tri[0]), rng.unit())),
            2 => add(v0, scale(rng.dir(), rng.range(0.0, 3.0))),
            3 => add(v0, scale(rng.dir(), rng.range(0.0, 200.0))),
            4 => add(add(v0, scale(e1, rng.unit() * 0.5)), scale(e2, rng.unit() * 0.5)),
            _ => v0,
        };
        let b = match rng.below(6) {
            0 => tri[rng.below(3)],
            1 => add(a, scale(e1, rng.range(-1.5, 1.5))),
            2 => add(a, scale(rng.dir(), rng.range(0.0, 4.0))),
            3 => a,
            4 => add(a, scale(rng.dir(), 1e-9)),
            _ => add(v0, scale(rng.dir(), rng.range(0.0, 50.0))),
        };
        check_face(a, b, v0, e1, e2, &mut skips, &mut cases);
    }
    assert!(skips * 20 > cases, "the ball skipped only {skips} of {cases}: the test misses the skip");
}

/// A segment through the middle of a face (Hit), and one that grazes it.
#[test]
fn adversarial_faces() {
    let (mut s, mut c) = (0, 0);
    let v0 = [0.0, 0.0, 0.0];
    let e1 = [1.0, 0.0, 0.0];
    let e2 = [0.0, 1.0, 0.0];
    let cases: Vec<(V3, V3)> = vec![
        ([0.25, 0.25, -1.0], [0.25, 0.25, 1.0]),        // pierce
        ([0.25, 0.25, 0.0], [0.25, 0.25, 1.0]),         // endpoint in the face
        ([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),             // endpoint on a vertex
        ([0.5, 0.5, 0.0], [0.7, 0.7, 0.0]),             // on the hypotenuse, coplanar
        ([-1.0, 0.25, 0.0], [2.0, 0.25, 0.0]),          // coplanar through
        ([-1.0, 0.25, 1e-15], [2.0, 0.25, 1e-15]),      // almost coplanar
        ([-1.0, 0.25, 1e-9], [2.0, 0.25, 1e-9]),
        ([0.25, -1.0, 0.3], [0.25, 2.0, 0.3]),          // parallel above
        ([0.2, 0.2, 0.5], [0.2, 0.2, 0.5]),             // zero length
        ([5.0, 5.0, 5.0], [5.0, 5.0, 5.0]),             // zero length, far
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),             // along the hypotenuse
        ([0.5, 0.5, 1e-300], [0.5, 0.5, -1e-300]),      // vanishing crossing
        ([0.25, 0.25, 1.0], [0.25, 0.25, 1.0 + 1e-14]), // sliver length
        ([1e6, 0.0, 0.0], [-1e6, 0.0, 0.3]),            // huge segment
    ];
    for (a, b) in cases {
        check_face(a, b, v0, e1, e2, &mut s, &mut c);
        // and the same with every face corner as the origin and the edges rotated
        check_face(b, a, v0, e1, e2, &mut s, &mut c);
        check_face(a, b, [0.0, 0.0, 0.0], e2, e1, &mut s, &mut c);
        check_face(a, b, [1.0, 0.0, 0.0], [-1.0, 1.0, 0.0], [-1.0, 0.0, 0.0], &mut s, &mut c);
    }
    // degenerate faces
    for (v0, e1, e2) in [
        ([0.0; 3], [0.0; 3], [0.0; 3]),
        ([0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]),
        ([0.0; 3], [1.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
        ([0.0; 3], [1.0, 0.0, 0.0], [0.5, 1e-12, 0.0]),
        ([1e3; 3], [1e-3, 0.0, 0.0], [0.0, 1e-3, 0.0]),
    ] {
        for (a, b) in [
            ([0.3, 0.0, 0.0], [0.3, 0.0, 1.0]),
            ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            ([0.0, 0.0, 0.5], [3.0, 0.0, 0.5]),
            ([0.5, 1.0, 0.0], [0.5, -1.0, 0.0]),
        ] {
            check_face(a, b, v0, e1, e2, &mut s, &mut c);
        }
    }
}

/// Segment, mesh: the whole walk against the old walk.
fn check_walk(f: &Fixture, a: V3, b: V3, rng: &mut Rng, stats: &mut Stats) {
    let s = &f.surface;
    let nf = f.tris.len() / 9;
    // exclusions: none, or a random sorted set including (maybe) the winner
    let base = ref_walk(s, a, b, &[], 0.0, &|_| false);
    let mut excluded: Vec<u32> = Vec::new();
    if rng.below(4) == 0 {
        for _ in 0..rng.below(40) + 1 {
            excluded.push(rng.below(nf) as u32);
        }
        if rng.below(2) == 0 {
            if let Some(w) = base.winner {
                excluded.push(w);
            }
        }
        excluded.sort_unstable();
        excluded.dedup();
    }
    let stops: Vec<f64> = {
        let mut v = vec![f64::NEG_INFINITY];
        if base.d.is_finite() {
            let d = base.d;
            v.extend_from_slice(&[d, next_up(d), next_down(d), d + 1e-9, d - 1e-9, d * 1.5, 0.0]);
        }
        v
    };
    let seg = SegPre::new(a, b);
    for thr in stops {
        let stop = move |best: f64| best < thr;
        let old = ref_walk(s, a, b, &excluded, 0.0, &stop);
        let mut winner: Option<u32> = None;
        // The core, not the `perf`-charging wrapper: these tests run in
        // parallel with the `perf` unit tests, which sum the global counters.
        let new = s.seg_surface_walk_core(a, b, &excluded, 0.0, stop, |tid, slot, bound| {
            let r = seg_tri_dist_ball(&seg, &s.bvh.tris[slot], bound);
            if r.0 < bound {
                winner = Some(tid as u32);
            }
            r
        });
        assert!(
            same_bits(new.0, old.d) && same_pt(new.1, old.pt) && new.2 == old.np && new.3 == old.stopped,
            "{}: walk differs a={a:?} b={b:?} thr={thr}: new {new:?} old ({}, {:?}, {}, {})",
            f.name, old.d, old.pt, old.np, old.stopped
        );
        assert_eq!(winner, old.winner, "{}: winning face a={a:?} b={b:?} thr={thr}", f.name);
        // The perf tallies are the old walk's, too: nodes visited and pruned,
        // faces tested.
        assert_eq!(
            new.4,
            [old.visited, old.pruned, old.tested],
            "{}: perf tallies a={a:?} b={b:?} thr={thr}",
            f.name
        );
        stats.walks += 1;
        stats.tested += old.tested;
    }
}

#[derive(Default)]
struct Stats {
    walks: u64,
    tested: u64,
}

/// Band and face-below, for the same segment.
fn check_band_and_below(f: &Fixture, a: V3, b: V3, rng: &mut Rng) {
    let s = &f.surface;
    let base = ref_walk(s, a, b, &[], 0.0, &|_| false);
    let span = norm(sub(f.hi, f.lo)).max(1e-3);
    let mut withins = vec![0.0, 1e-9, 0.05, 0.3, 0.001 * span, 0.05 * span];
    if base.d.is_finite() {
        let d = base.d;
        withins.extend_from_slice(&[d, next_up(d), next_down(d), d + 1e-9, d - 1e-9]);
    }
    for within in withins {
        if within < 0.0 {
            continue;
        }
        let want = ref_band(s, a, b, within);
        let got = s
            .faces_near_segment(a, b, within, 0, usize::MAX)
            .expect("no face limit");
        assert_eq!(got.0, want.0, "{}: band faces a={a:?} b={b:?} within={within}", f.name);
        assert_eq!(got.1, want.1, "{}: band np", f.name);
    }
    let thresh = [0.0, 0.1, 0.3, 0.6][rng.below(4)];
    let mut at_leasts = vec![0.05, 0.3, 1.0];
    if base.d.is_finite() {
        let c = base.d - thresh;
        at_leasts.extend_from_slice(&[c, next_up(c), next_down(c), c + 1e-9, c - 1e-9, c + 0.01]);
    }
    for at_least in at_leasts {
        let want = ref_face_below(s, a, b, thresh, at_least);
        let got = s
            .seg_face_below_core(a, b, thresh, at_least)
            .is_some_and(|(found, _)| found);
        assert_eq!(got, want, "{}: face_below a={a:?} b={b:?} thresh={thresh} at_least={at_least}", f.name);
    }
}

fn run_oracle(per_mesh: usize) -> Stats {
    let fixtures = fixtures();
    let mut total = Stats::default();
    for f in &fixtures {
        let chunks = 64usize;
        let per_chunk = per_mesh.div_ceil(chunks);
        let results: Vec<Stats> = (0..chunks)
            .into_par_iter()
            .map(|c| {
                let mut rng = Rng::new(0xC0DE ^ (c as u64 * 7919) ^ (f.name.len() as u64 * 104_729));
                let mut stats = Stats::default();
                for _ in 0..per_chunk {
                    let (a, b) = draw_segment(f, &mut rng);
                    check_walk(f, a, b, &mut rng, &mut stats);
                    if rng.below(4) == 0 {
                        check_band_and_below(f, a, b, &mut rng);
                    }
                }
                stats
            })
            .collect();
        for r in results {
            total.walks += r.walks;
            total.tested += r.tested;
        }
    }
    total
}

#[test]
fn walk_band_and_face_below_match_the_reference() {
    let n = segments_per_mesh(1500);
    let stats = run_oracle(n);
    assert!(stats.walks > 0 && stats.tested > 0);
}

/// 10^6 random segments over the mesh set (run with `--ignored`).
#[test]
#[ignore = "a million segments: run on demand"]
fn million() {
    let meshes = fixtures().len();
    let stats = run_oracle(1_000_000_usize.div_ceil(meshes));
    eprintln!("walks {} faces tested {}", stats.walks, stats.tested);
    assert!(stats.walks >= 1_000_000);
}

/// The skip must actually fire in the walks, or the oracle proves nothing.
#[test]
fn the_ball_skips_most_faces_of_a_real_walk() {
    let f = fixture("sphere", noisy_sphere(4, 20.0, 0.15, 1));
    let s = &f.surface;
    let mut rng = Rng::new(5);
    let (mut tested, mut skipped) = (0u64, 0u64);
    for _ in 0..300 {
        let (a, b) = draw_segment(&f, &mut rng);
        let seg = SegPre::new(a, b);
        s.seg_surface_walk_core(a, b, &[], 0.0, |_| false, |_, slot, bound| {
            let r = seg_tri_dist_ball(&seg, &s.bvh.tris[slot], bound);
            tested += 1;
            if r.0 == f64::INFINITY {
                skipped += 1;
            }
            r
        });
    }
    assert!(skipped * 2 > tested, "skipped {skipped} of {tested}");
}
