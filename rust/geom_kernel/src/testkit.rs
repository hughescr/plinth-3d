//! Test-only helpers exposed for the integration tests in tests/. Provides
//! deterministic synthetic meshes and brute-force references so the BVH /
//! primitives can be checked directly.

use crate::bvh::Bvh;
use crate::closest::closest_on_tri as _closest;
use crate::mesh::Mesh;
use crate::moller::moller as _moller;
use crate::queries::NativeSurface;
use crate::vec3::{norm, sub, V3};

const TEST_UNBOUNDED_GRID_BYTES: usize = usize::MAX;

pub struct TestMesh {
    pub tris: Vec<f64>, // n*9
    pub normals: Vec<f64>,
    pub areas: Vec<f64>,
    pub bounds: Vec<f64>,
    pub n: usize,
}

// simple deterministic LCG in [0,1)
struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed
            .wrapping_mul(2862933555777941757)
            .wrapping_add(3037000493))
    }
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
}

fn finalize(tris: Vec<f64>) -> TestMesh {
    let n = tris.len() / 9;
    let mut normals = Vec::with_capacity(n * 3);
    let mut areas = Vec::with_capacity(n);
    let mut bmin = [f64::INFINITY; 3];
    let mut bmax = [f64::NEG_INFINITY; 3];
    for f in 0..n {
        let b = f * 9;
        let a = [tris[b], tris[b + 1], tris[b + 2]];
        let bb = [tris[b + 3], tris[b + 4], tris[b + 5]];
        let c = [tris[b + 6], tris[b + 7], tris[b + 8]];
        let e1 = sub(bb, a);
        let e2 = sub(c, a);
        let cr = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let nn = norm(cr).max(1e-30);
        normals.push(cr[0] / nn);
        normals.push(cr[1] / nn);
        normals.push(cr[2] / nn);
        areas.push(0.5 * nn);
        for v in [a, bb, c] {
            for k in 0..3 {
                if v[k] < bmin[k] {
                    bmin[k] = v[k];
                }
                if v[k] > bmax[k] {
                    bmax[k] = v[k];
                }
            }
        }
    }
    let bounds = vec![bmin[0], bmin[1], bmin[2], bmax[0], bmax[1], bmax[2]];
    TestMesh {
        tris,
        normals,
        areas,
        bounds,
        n,
    }
}

pub fn make_grid_mesh(k: usize) -> TestMesh {
    // k*k random triangles in a 10mm cube
    let mut rng = Lcg::new(0xC0FFEE + k as u64);
    let n = k * k;
    let mut tris = Vec::with_capacity(n * 9);
    for _ in 0..n {
        for _ in 0..9 {
            tris.push(rng.next() * 10.0);
        }
    }
    finalize(tris)
}

pub fn make_identical_tris(n: usize) -> TestMesh {
    let mut tris = Vec::with_capacity(n * 9);
    for _ in 0..n {
        tris.extend_from_slice(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    }
    finalize(tris)
}

pub fn make_slivers(n: usize) -> TestMesh {
    let mut rng = Lcg::new(0x51117E);
    let mut tris = Vec::with_capacity(n * 9);
    for _ in 0..n {
        let x = rng.next() * 10.0;
        let y = rng.next() * 10.0;
        let z = rng.next() * 10.0;
        // near-degenerate: 1e-9-scale extent
        tris.extend_from_slice(&[x, y, z, x + 1e-9, y, z, x, y + 1e-9, z]);
    }
    finalize(tris)
}

pub fn build_surface(m: &TestMesh) -> NativeSurface {
    let mesh = Mesh::new(&m.tris, &m.normals, &m.areas, &m.bounds).unwrap();
    NativeSurface::new(mesh, 2.0, TEST_UNBOUNDED_GRID_BYTES).unwrap()
}

/// Build a surface directly from a flat n*9 triangle list (test fixtures).
pub fn build_from_tris(tris: Vec<f64>) -> NativeSurface {
    build_surface(&finalize(tris))
}

/// Build with an explicit parity-grid memory budget (A17 budget-failure test).
pub fn build_surface_budget(m: &TestMesh, max_grid_bytes: usize) -> Result<NativeSurface, String> {
    let mesh = Mesh::new(&m.tris, &m.normals, &m.areas, &m.bounds)?;
    NativeSurface::new(mesh, 2.0, max_grid_bytes).map_err(|error| error.to_string())
}

/// Public accessor for the make_* meshes so the budget test can reuse them.
pub fn grid_mesh(k: usize) -> TestMesh {
    make_grid_mesh(k)
}

/// Exact segment-to-surface minimum via the BVH-pruned branch & bound.
pub fn surf_seg_min(s: &NativeSurface, a: V3, b: V3) -> (f64, V3, u32) {
    s.seg_surface_min(a, b)
}

/// Brute-force segment-to-surface minimum over ALL triangles, using the SAME
/// per-triangle primitive as the pruned path — so a discrepancy isolates the
/// A17 pruning lower bound, not the triangle math.
pub fn brute_seg_min(s: &NativeSurface, a: V3, b: V3) -> (f64, V3, u32) {
    let mut best_d = f64::INFINITY;
    let mut best_pt: V3 = [f64::NAN; 3];
    let mut near_parallel = 0u32;
    for &face_id in s.mesh.face_ids() {
        let t = face_id as usize;
        let (d, pt, np) = s.seg_tri_dist(a, b, t);
        if np {
            near_parallel += 1;
        }
        if d < best_d {
            best_d = d;
            best_pt = pt;
        }
    }
    (best_d, best_pt, near_parallel)
}

/// Proves that two surface views retain the same immutable face allocation.
pub fn surfaces_share_geometry(a: &NativeSurface, b: &NativeSurface) -> bool {
    a.mesh.shares_geometry(&b.mesh)
}

/// Process-local construction-attempt counts at the actual BVH and parity-axis
/// index build boundaries.
///
/// These result-inert seams include successful, failed, transient, and direct
/// builds. A projection reuse test must assert that neither value changes.
pub fn index_construction_attempt_counts() -> (u64, u64) {
    (
        crate::bvh::build_attempt_count(),
        crate::parity::axis_build_attempt_count(),
    )
}

/// Brute-force allowed-face projection oracle for bounded generated tests.
/// Production callers must use `project_allowed_faces_batch`.
#[allow(
    clippy::float_cmp,
    reason = "the oracle must reproduce the BVH's equal-distance/lowest-index \
              tie-break bit for bit, or it stops being an oracle"
)]
#[expect(
    clippy::cast_possible_truncation,
    reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged as a \
          class, and the one architecture item the class carried (geom_kernel's 32-bit BVH index \
          width) is now a named, enforced limit. TODO(cast-width-audit): retire site by site.  See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
pub fn brute_allowed_projection(
    surface: &NativeSurface,
    allowed_face_ids: &[u32],
    point: V3,
    upper_bound: f64,
) -> Option<(f64, u32, V3)> {
    let mut best_distance = upper_bound;
    let mut best_face = surface.mesh.global_nfaces();
    let mut best_point = [0.0; 3];
    for &face_id in allowed_face_ids {
        let face = face_id as usize;
        let closest = _closest(
            point,
            surface.mesh.v0(face),
            surface.mesh.vertex_b(face),
            surface.mesh.vertex_c(face),
        );
        let delta = [
            point[0] - closest[0],
            point[1] - closest[1],
            point[2] - closest[2],
        ];
        let distance = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
        if distance < best_distance || (distance == best_distance && face < best_face) {
            best_distance = distance;
            best_face = face;
            best_point = closest;
        }
    }
    (best_face < surface.mesh.global_nfaces()).then_some((
        best_distance,
        best_face as u32,
        best_point,
    ))
}

pub fn surf_bvh_face_ids(s: &NativeSurface) -> Vec<u32> {
    let mut face_ids = s.bvh.order.clone();
    face_ids.sort_unstable();
    face_ids
}

pub fn surf_parity_face_ids(s: &NativeSurface) -> Vec<u32> {
    let mut face_ids =
        s.pv.grids
            .iter()
            .flat_map(|grid| grid.referenced_face_ids())
            .collect::<Vec<_>>();
    face_ids.sort_unstable();
    face_ids.dedup();
    face_ids
}

/// Structural index statistics for proving that a view owns fresh indexes.
pub fn surf_index_stats(s: &NativeSurface) -> (usize, usize, [(usize, usize, usize); 3]) {
    (
        s.bvh.nodes.len(),
        s.bvh.order.len(),
        s.pv.grids
            .each_ref()
            .map(|grid| (grid.n_cells, grid.n_refs, grid.max_occ)),
    )
}

pub fn sample_points(m: &TestMesh, n: usize) -> Vec<V3> {
    let mut rng = Lcg::new(0x5A3D);
    let lo = [m.bounds[0], m.bounds[1], m.bounds[2]];
    let hi = [m.bounds[3], m.bounds[4], m.bounds[5]];
    (0..n)
        .map(|_| {
            [
                lo[0] + (hi[0] - lo[0]) * (rng.next() * 1.5 - 0.25),
                lo[1] + (hi[1] - lo[1]) * (rng.next() * 1.5 - 0.25),
                lo[2] + (hi[2] - lo[2]) * (rng.next() * 1.5 - 0.25),
            ]
        })
        .collect()
}

pub fn sample_rays(m: &TestMesh, n: usize) -> Vec<(V3, V3)> {
    let mut rng = Lcg::new(0x9A17);
    let lo = [m.bounds[0], m.bounds[1], m.bounds[2]];
    let hi = [m.bounds[3], m.bounds[4], m.bounds[5]];
    (0..n)
        .map(|_| {
            let o = [
                lo[0] + (hi[0] - lo[0]) * rng.next(),
                lo[1] + (hi[1] - lo[1]) * rng.next(),
                lo[2] + (hi[2] - lo[2]) * rng.next(),
            ];
            let p1 = [
                lo[0] + (hi[0] - lo[0]) * rng.next(),
                lo[1] + (hi[1] - lo[1]) * rng.next(),
                lo[2] + (hi[2] - lo[2]) * rng.next(),
            ];
            (o, sub(p1, o))
        })
        .collect()
}

pub fn surf_nearest(s: &NativeSurface, p: V3) -> (f64, usize, V3) {
    s.nearest_one(p, f64::INFINITY)
}

pub fn surf_collect_hits(s: &NativeSurface, o: V3, d: V3, t_max: f64) -> Vec<u32> {
    let mut v: Vec<u32> = s
        .collect_hits(o, d, t_max)
        .0
        .into_iter()
        .map(|h| h.1)
        .collect();
    v.sort_unstable();
    v
}

#[expect(
    clippy::cast_sign_loss,
    reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged as a \
          class, and the one architecture item the class carried (geom_kernel's 32-bit BVH index \
          width) is now a named, enforced limit. TODO(cast-width-audit): retire site by site.  See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
pub fn surf_bvh_signature(s: &NativeSurface) -> Vec<u64> {
    let bvh: &Bvh = &s.bvh;
    let mut out = Vec::new();
    for &o in &bvh.order {
        out.push(o as u64);
    }
    for nd in &bvh.nodes {
        out.push(nd.bmin[0].to_bits());
        out.push(nd.bmin[1].to_bits());
        out.push(nd.bmin[2].to_bits());
        out.push(nd.bmax[0].to_bits());
        out.push(nd.bmax[1].to_bits());
        out.push(nd.bmax[2].to_bits());
        out.push(nd.a as u64);
        out.push(nd.b as u64);
    }
    out
}

#[allow(
    clippy::float_cmp,
    reason = "the oracle must reproduce the BVH's equal-distance/lowest-index \
              tie-break bit for bit, or it stops being an oracle"
)]
pub fn brute_nearest(m: &TestMesh, p: V3) -> (f64, usize) {
    let mut best = f64::INFINITY;
    let mut bi = m.n;
    for f in 0..m.n {
        let b = f * 9;
        let a = [m.tris[b], m.tris[b + 1], m.tris[b + 2]];
        let bb = [m.tris[b + 3], m.tris[b + 4], m.tris[b + 5]];
        let c = [m.tris[b + 6], m.tris[b + 7], m.tris[b + 8]];
        let cp = _closest(p, a, bb, c);
        let d = norm(sub(p, cp));
        if d < best || (d == best && f < bi) {
            best = d;
            bi = f;
        }
    }
    (best, bi)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "integer-width casts in mesh/voxel index arithmetic, NOT individually audited -- triaged as a \
          class, and the one architecture item the class carried (geom_kernel's 32-bit BVH index \
          width) is now a named, enforced limit. TODO(cast-width-audit): retire site by site.  See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
pub fn brute_moller_hits(m: &TestMesh, o: V3, d: V3, t_max: f64) -> Vec<u32> {
    let mut out = Vec::new();
    for f in 0..m.n {
        let b = f * 9;
        let a = [m.tris[b], m.tris[b + 1], m.tris[b + 2]];
        let e1 = [
            m.tris[b + 3] - a[0],
            m.tris[b + 4] - a[1],
            m.tris[b + 5] - a[2],
        ];
        let e2 = [
            m.tris[b + 6] - a[0],
            m.tris[b + 7] - a[1],
            m.tris[b + 8] - a[2],
        ];
        let (hit, _) = _moller(o, d, a, e1, e2, t_max);
        if hit {
            out.push(f as u32);
        }
    }
    out
}

pub fn kern_closest_on_tri(p: V3, a: V3, b: V3, c: V3) -> V3 {
    _closest(p, a, b, c)
}

pub fn kern_moller(o: V3, d: V3, v0: V3, e1: V3, e2: V3, t_max: f64) -> (bool, f64) {
    _moller(o, d, v0, e1, e2, t_max)
}
