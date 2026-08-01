//! NativeSurface: the query engine. All methods are pure functions of immutable
//! kernel state + their own inputs, parallelized index-wise (spec sec 4). No
//! cross-query reductions anywhere.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use rayon::prelude::*;

use crate::bvh::Bvh;
use crate::closest::{closest_on_tri, segseg};
use crate::consts::{DEDUP_LOC_MM, EPS, NEAR_WALL_SKIP, THRESHOLD_BAND_MM};
use crate::mesh::Mesh;
use crate::moller::{dedup_loc, moller};
use crate::par;
use crate::parity::ParityVote;
use crate::vec3::{add, cross, dot, norm, scale, sub, V3};
use crate::{SurfaceIndexKind, SurfaceSubsetError};

thread_local! {
    static STACK: RefCell<Vec<i32>> = RefCell::new(Vec::with_capacity(128));
    static ZSCRATCH: RefCell<Vec<f64>> = RefCell::new(Vec::with_capacity(64));
}

pub struct Interval {
    pub s_a: f64,
    pub s_b: f64,
    pub entered: bool,
    pub exited: bool,
    pub max_depth: f64,
    /// -1 = axis ray; k >= 0 = offset ray k
    pub ray: i32,
}

/// A companion input slice did not match its batch's authoritative length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchInputError {
    pub operation: &'static str,
    pub field: &'static str,
    pub expected: usize,
    pub actual: usize,
}

impl BatchInputError {
    fn require_len(
        operation: &'static str,
        field: &'static str,
        expected: usize,
        actual: usize,
    ) -> Result<(), Self> {
        if actual == expected {
            Ok(())
        } else {
            Err(Self {
                operation,
                field,
                expected,
                actual,
            })
        }
    }
}

impl std::fmt::Display for BatchInputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}: {} has {} items; expected {}",
            self.operation, self.field, self.actual, self.expected
        )
    }
}

impl std::error::Error for BatchInputError {}

pub type CastRaysBatch = (Vec<i64>, Vec<f64>, Vec<i64>);
pub type SphereBreachBatch = (Vec<bool>, Vec<f64>, Vec<f64>);

pub struct NativeSurface {
    pub(crate) mesh: Mesh,
    pub bvh: Bvh,
    pub pv: ParityVote,
    pub grid_pitch_mm: f64,
    max_grid_bytes: usize,
    exterior_mask: OnceLock<Vec<bool>>,
    // result-inert telemetry (spec sec 4.2): decisions within 1e-6 of a
    // threshold, and measured-tie breach disagreements.
    pub band_counter: AtomicU64,
    pub tie_counter: AtomicU64,
}

#[inline]
fn box_min(a: V3, b: V3) -> V3 {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])]
}
#[inline]
fn box_max(a: V3, b: V3) -> V3 {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]
}

/// Distance from point to node AABB = sqrt(sum of clamped axis gaps squared)
/// (spec sec 3.3). sqrt is correctly rounded and monotone => pruning sound.
#[inline]
fn point_box_dist(p: V3, bmin: V3, bmax: V3) -> f64 {
    let mut s = 0.0f64;
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
    s.sqrt()
}

/// Distance lower bound between a segment's AABB and a node AABB
/// (Amendment A17): sqrt(sum max(0, axis-gap)^2). Sound: each axis gap <= any
/// point-pair coordinate difference.
#[inline]
fn box_box_dist(amin: V3, amax: V3, bmin: V3, bmax: V3) -> f64 {
    let mut s = 0.0f64;
    for k in 0..3 {
        let g = (bmin[k] - amax[k]).max(amin[k] - bmax[k]).max(0.0);
        s += g * g;
    }
    s.sqrt()
}

/// Ray (bounded param interval [t0,t1]) vs padded node AABB (slab test).
#[inline]
fn ray_box(o: V3, d: V3, bmin: V3, bmax: V3, pad: f64, t0: f64, t1: f64) -> bool {
    let mut tmin = t0;
    let mut tmax = t1;
    for k in 0..3 {
        let lo = bmin[k] - pad;
        let hi = bmax[k] + pad;
        if d[k] == 0.0 {
            if o[k] < lo || o[k] > hi {
                return false;
            }
        } else {
            let inv = 1.0 / d[k];
            let mut ta = (lo - o[k]) * inv;
            let mut tb = (hi - o[k]) * inv;
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            if ta > tmin {
                tmin = ta;
            }
            if tb < tmax {
                tmax = tb;
            }
            if tmin > tmax {
                return false;
            }
        }
    }
    true
}

fn build_indexes(
    mesh: &Mesh,
    grid_pitch_mm: f64,
    max_grid_bytes: usize,
) -> Result<(Bvh, ParityVote), (SurfaceIndexKind, String)> {
    let bvh = Bvh::build(mesh).map_err(|reason| (SurfaceIndexKind::Bvh, reason))?;
    let pv = ParityVote::build(mesh, grid_pitch_mm, max_grid_bytes)
        .map_err(|reason| (SurfaceIndexKind::Parity, reason))?;
    Ok((bvh, pv))
}

impl NativeSurface {
    pub fn new(
        mesh: Mesh,
        grid_pitch_mm: f64,
        max_grid_bytes: usize,
    ) -> Result<NativeSurface, String> {
        // Preserve the established construction error text. Typed index
        // attribution is specific to the subset boundary below.
        let (bvh, pv) = build_indexes(&mesh, grid_pitch_mm, max_grid_bytes)
            .map_err(|(_index, reason)| reason)?;
        Ok(NativeSurface {
            mesh,
            bvh,
            pv,
            grid_pitch_mm,
            max_grid_bytes,
            exterior_mask: OnceLock::new(),
            band_counter: AtomicU64::new(0),
            tie_counter: AtomicU64::new(0),
        })
    }

    /// Narrow this surface to global face IDs selected by the current view.
    /// Validation follows caller order; accepted IDs are canonicalized to
    /// ascending global order before building independent indexes and caches.
    pub fn subset(&self, face_ids: &[u32]) -> Result<NativeSurface, SurfaceSubsetError> {
        if face_ids.is_empty() {
            return Err(SurfaceSubsetError::EmptySelection);
        }

        let global_face_count = self.mesh.global_nfaces();
        let mut seen = BTreeSet::new();
        for (position, &face_id) in face_ids.iter().enumerate() {
            if face_id as usize >= global_face_count {
                return Err(SurfaceSubsetError::FaceIdOutOfBounds {
                    position,
                    face_id,
                    global_face_count,
                });
            }
            if !self.mesh.contains_face(face_id) {
                return Err(SurfaceSubsetError::FaceIdNotSelected { position, face_id });
            }
            if !seen.insert(face_id) {
                return Err(SurfaceSubsetError::DuplicateFaceId { face_id });
            }
        }

        let mut canonical = face_ids.to_vec();
        canonical.sort_unstable();
        let mesh = self.mesh.subset(canonical);
        let (bvh, pv) = build_indexes(&mesh, self.grid_pitch_mm, self.max_grid_bytes)
            .map_err(|(index, _reason)| SurfaceSubsetError::IndexBuild { index })?;
        Ok(NativeSurface {
            mesh,
            bvh,
            pv,
            grid_pitch_mm: self.grid_pitch_mm,
            max_grid_bytes: self.max_grid_bytes,
            exterior_mask: OnceLock::new(),
            band_counter: AtomicU64::new(0),
            tie_counter: AtomicU64::new(0),
        })
    }

    /// Result-inert threshold-band telemetry (spec R3 / sec 4.2): count a
    /// CONTINUOUS decision quantity whose distance to its threshold is under
    /// `THRESHOLD_BAND_MM`. Relaxed atomic — the COUNT is a deterministic
    /// function of the inputs (each decision inspects only its own quantity),
    /// so it is thread-count invariant like `tie_counter`, and it NEVER feeds a
    /// result. Non-finite quantities (e.g. an infinite far-wall thickness or an
    /// empty-crop clearance) compare false and are not counted.
    #[inline]
    fn band_check(&self, quantity: f64, threshold: f64) {
        if (quantity - threshold).abs() < THRESHOLD_BAND_MM {
            self.band_counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    // ---- nearest triangle (query/project) -------------------------------
    /// Exact nearest via branch-and-bound with lexicographic (dist, idx),
    /// strict `>` pruning, seed (upper_bound, global_nfaces) (Amendment A12).
    pub(crate) fn nearest_one(&self, p: V3, upper_bound: f64) -> (f64, usize, V3) {
        // Keep the historical full-surface face count as the no-hit sentinel.
        // A selected global ID can never collide with this value.
        let nf = self.mesh.global_nfaces();
        let mut best_d = upper_bound;
        let mut best_i = nf;
        let mut best_cp: V3 = [0.0; 3];
        STACK.with(|s| {
            let mut stack = s.borrow_mut();
            stack.clear();
            stack.push(0);
            while let Some(ni) = stack.pop() {
                let node = &self.bvh.nodes[ni as usize];
                // prune STRICTLY (> best_d): keep equal-distance lower-index tris
                if point_box_dist(p, node.bmin, node.bmax) > best_d {
                    continue;
                }
                if node.leaf {
                    let start = node.a as usize;
                    let cnt = node.b as usize;
                    for &tid in &self.bvh.order[start..start + cnt] {
                        let t = tid as usize;
                        let a = self.mesh.v0(t);
                        let b = self.mesh.vertex_b(t);
                        let cc = self.mesh.vertex_c(t);
                        let cp = closest_on_tri(p, a, b, cc);
                        let d = norm(sub(p, cp));
                        if d < best_d || (d == best_d && t < best_i) {
                            best_d = d;
                            best_i = t;
                            best_cp = cp;
                        }
                    }
                } else {
                    // visit nearer child first => push farther first (LIFO)
                    let l = node.a as usize;
                    let r = node.b as usize;
                    let dl = point_box_dist(p, self.bvh.nodes[l].bmin, self.bvh.nodes[l].bmax);
                    let dr = point_box_dist(p, self.bvh.nodes[r].bmin, self.bvh.nodes[r].bmax);
                    if dl <= dr {
                        stack.push(r as i32);
                        stack.push(l as i32);
                    } else {
                        stack.push(l as i32);
                        stack.push(r as i32);
                    }
                }
            }
        });
        (best_d, best_i, best_cp)
    }

    /// query facade: (dist, idx) with over-bound => (inf, global_nfaces).
    pub fn query(&self, pts: &[V3], upper_bound: f64) -> (Vec<f64>, Vec<i64>) {
        let nf = self.mesh.global_nfaces();
        let res: Vec<(f64, i64)> = par::run(|| {
            pts.par_iter()
                .map(|&p| {
                    let (d, i, _) = self.nearest_one(p, upper_bound);
                    if i == nf {
                        (f64::INFINITY, nf as i64)
                    } else {
                        (d, i as i64)
                    }
                })
                .collect()
        });
        let mut dist = Vec::with_capacity(res.len());
        let mut idx = Vec::with_capacity(res.len());
        for (d, i) in res {
            dist.push(d);
            idx.push(i);
        }
        (dist, idx)
    }

    /// project: (dist, closest_point, tri_idx, face_normal).
    pub fn project(&self, pts: &[V3]) -> (Vec<f64>, Vec<V3>, Vec<i64>, Vec<V3>) {
        let res: Vec<(f64, V3, i64, V3)> = par::run(|| {
            pts.par_iter()
                .map(|&p| {
                    let (d, i, cp) = self.nearest_one(p, f64::INFINITY);
                    let fn_ = if i < self.mesh.global_nfaces() {
                        self.mesh.normal(i)
                    } else {
                        [0.0; 3]
                    };
                    (d, cp, i as i64, fn_)
                })
                .collect()
        });
        let mut dist = Vec::with_capacity(res.len());
        let mut cp = Vec::with_capacity(res.len());
        let mut idx = Vec::with_capacity(res.len());
        let mut nrm = Vec::with_capacity(res.len());
        for (d, c, i, n) in res {
            dist.push(d);
            cp.push(c);
            idx.push(i);
            nrm.push(n);
        }
        (dist, cp, idx, nrm)
    }

    // ---- inside -----------------------------------------------------------
    pub fn inside(&self, pts: &[V3]) -> Vec<bool> {
        par::run(|| {
            pts.par_iter()
                .map(|&p| ZSCRATCH.with(|z| self.pv.inside(p, &mut z.borrow_mut())))
                .collect()
        })
    }

    #[inline]
    fn inside_serial(&self, p: V3) -> bool {
        ZSCRATCH.with(|z| self.pv.inside(p, &mut z.borrow_mut()))
    }

    // ---- ray casting ------------------------------------------------------
    /// Collect ALL Möller crossings for one ray (superset broad phase + exact
    /// filter). Returns unsorted (t, face) pairs.
    pub(crate) fn collect_hits(&self, o: V3, d: V3, t_max: f64) -> Vec<(f64, u32)> {
        let mut out: Vec<(f64, u32)> = Vec::new();
        let t0 = -EPS;
        let t1 = t_max + EPS;
        STACK.with(|s| {
            let mut stack = s.borrow_mut();
            stack.clear();
            stack.push(0);
            while let Some(ni) = stack.pop() {
                let node = &self.bvh.nodes[ni as usize];
                if !ray_box(o, d, node.bmin, node.bmax, node.pad, t0, t1) {
                    continue;
                }
                if node.leaf {
                    let start = node.a as usize;
                    let cnt = node.b as usize;
                    for &tid in &self.bvh.order[start..start + cnt] {
                        let t = tid as usize;
                        let (hit, tv) = moller(
                            o,
                            d,
                            self.mesh.v0(t),
                            self.mesh.e1(t),
                            self.mesh.e2(t),
                            t_max,
                        );
                        if hit {
                            out.push((tv, tid));
                        }
                    }
                } else {
                    stack.push(node.a);
                    stack.push(node.b);
                }
            }
        });
        out
    }

    /// cast_rays for one ray: deduped by arc location; representative t = first
    /// in group, representative face = LOWEST index in group (Amendment A13).
    fn cast_one(&self, o: V3, d: V3, t_max: f64) -> (Vec<f64>, Vec<i64>) {
        let seg_len = norm(d) * t_max;
        let mut hits = self.collect_hits(o, d, t_max);
        if hits.is_empty() {
            return (Vec::new(), Vec::new());
        }
        // stable sort by t
        hits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let dt = DEDUP_LOC_MM / seg_len.max(1e-9);
        let mut ts: Vec<f64> = Vec::new();
        let mut fs: Vec<i64> = Vec::new();
        let mut group_kept = hits[0].0;
        let mut group_min_face = hits[0].1;
        for &(t, f) in hits.iter().skip(1) {
            if t - group_kept > dt {
                // close current group
                ts.push(group_kept);
                fs.push(group_min_face as i64);
                group_kept = t;
                group_min_face = f;
            } else if f < group_min_face {
                group_min_face = f;
            }
        }
        ts.push(group_kept);
        fs.push(group_min_face as i64);
        (ts, fs)
    }

    /// Batched cast_rays -> ragged (offsets, t_all, face_all).
    pub fn cast_rays(
        &self,
        origins: &[V3],
        dirs: &[V3],
        t_max: f64,
    ) -> Result<CastRaysBatch, BatchInputError> {
        let n = origins.len();
        BatchInputError::require_len("cast_rays", "dirs", n, dirs.len())?;
        let per: Vec<(Vec<f64>, Vec<i64>)> = par::run(|| {
            (0..n)
                .into_par_iter()
                .map(|i| self.cast_one(origins[i], dirs[i], t_max))
                .collect()
        });
        // serial scan into offsets + flat arrays (fixed index order)
        let mut offsets = Vec::with_capacity(n + 1);
        offsets.push(0i64);
        let mut t_all = Vec::new();
        let mut f_all = Vec::new();
        for (ts, fs) in &per {
            t_all.extend_from_slice(ts);
            f_all.extend_from_slice(fs);
            offsets.push(t_all.len() as i64);
        }
        Ok((offsets, t_all, f_all))
    }

    // ---- segment / capsule minimum distance ------------------------------
    /// Exact segment [a,b] vs one triangle: min over endpoints, 3 edges, pierce.
    /// Mirrors _seg_tri_dist (geometry_exact.py:596-630).
    #[inline]
    pub(crate) fn seg_tri_dist(&self, a: V3, b: V3, t: usize) -> (f64, V3) {
        let v0 = self.mesh.v0(t);
        let p = v0;
        let q = add(v0, self.mesh.e1(t));
        let r = add(v0, self.mesh.e2(t));
        let mut best_d = f64::INFINITY;
        let mut best_pt: V3 = [0.0; 3];
        // (a) endpoints vs triangle
        let cpa = closest_on_tri(a, p, q, r);
        let da = norm(sub(a, cpa));
        if da < best_d {
            best_d = da;
            best_pt = cpa;
        }
        let cpb = closest_on_tri(b, p, q, r);
        let db = norm(sub(b, cpb));
        if db < best_d {
            best_d = db;
            best_pt = cpb;
        }
        // (b) segment vs each triangle edge
        for &(x, y) in &[(p, q), (q, r), (r, p)] {
            let (d, _c1, c2) = segseg(a, b, x, y);
            if d < best_d {
                best_d = d;
                best_pt = c2;
            }
        }
        // (c) interior pierce -> distance 0 at crossing point
        let dv = sub(b, a);
        let (hit, tv) = moller(a, dv, v0, self.mesh.e1(t), self.mesh.e2(t), 1.0);
        if hit {
            let pierce = add(a, scale(dv, tv));
            if 0.0 < best_d {
                best_d = 0.0;
                best_pt = pierce;
            }
        }
        (best_d, best_pt)
    }

    /// Exact minimum distance from segment [a,b] to the surface (branch & bound).
    pub(crate) fn seg_surface_min(&self, a: V3, b: V3) -> (f64, V3) {
        let smin = box_min(a, b);
        let smax = box_max(a, b);
        let mut best_d = f64::INFINITY;
        let mut best_pt: V3 = [f64::NAN; 3];
        STACK.with(|s| {
            let mut stack = s.borrow_mut();
            stack.clear();
            stack.push(0);
            while let Some(ni) = stack.pop() {
                let node = &self.bvh.nodes[ni as usize];
                if box_box_dist(smin, smax, node.bmin, node.bmax) > best_d {
                    continue;
                }
                if node.leaf {
                    let start = node.a as usize;
                    let cnt = node.b as usize;
                    for &tid in &self.bvh.order[start..start + cnt] {
                        let (d, pt) = self.seg_tri_dist(a, b, tid as usize);
                        if d < best_d {
                            best_d = d;
                            best_pt = pt;
                            // pierce early-out allowed (spec sec 3.3)
                        }
                    }
                } else {
                    let l = node.a as usize;
                    let r = node.b as usize;
                    let dl =
                        box_box_dist(smin, smax, self.bvh.nodes[l].bmin, self.bvh.nodes[l].bmax);
                    let dr =
                        box_box_dist(smin, smax, self.bvh.nodes[r].bmin, self.bvh.nodes[r].bmax);
                    if dl <= dr {
                        stack.push(r as i32);
                        stack.push(l as i32);
                    } else {
                        stack.push(l as i32);
                        stack.push(r as i32);
                    }
                }
            }
        });
        (best_d, best_pt)
    }

    /// One segment clearance. Tier-1 open-space branch is NOT reproduced
    /// (Amendment A1): always exact segment-to-surface clearance.
    // The 7-argument geometry-query signature (endpoints, radii, standoff,
    // insets) is the inherent shape of this API, mirrored 1:1 by
    // `segment_clearance`'s own batched signature just below and by the
    // independent verify.py/census callers this kernel serves (SPEC-EMBED-
    // POLICY, SCAR-AREA-CAP) -- bundling them into a struct would just move
    // the same fields one level of indirection away with no clarity gain,
    // and every field is read at every call site. Not a candidate for the
    // batch-first hot-path rule either: this is the per-item primitive the
    // BATCHED caller already amortizes over (see `segment_clearance` below).
    #[allow(clippy::too_many_arguments)]
    fn seg_clear_one(
        &self,
        p0: V3,
        p1: V3,
        r0: f64,
        r1: f64,
        standoff: f64,
        inset0: f64,
        inset1: f64,
    ) -> (f64, V3) {
        let nan3: V3 = [f64::NAN; 3];
        let d = sub(p1, p0);
        let l = norm(d);
        if l < 1e-9 {
            return (f64::INFINITY, nan3);
        }
        let u = scale(d, 1.0 / l);
        let s_lo = inset0;
        let s_hi = l - inset1;
        if s_hi - s_lo <= 1e-9 {
            return (f64::INFINITY, nan3);
        }
        let a = add(p0, scale(u, s_lo));
        let b = add(p0, scale(u, s_hi));
        let r_lo = r0 + (r1 - r0) * (s_lo / l);
        let r_hi = r0 + (r1 - r0) * (s_hi / l);
        let r_max = r_lo.max(r_hi);
        let thresh = r_max + standoff;
        let (best_d, best_pt) = self.seg_surface_min(a, b);
        let clr = best_d - thresh;
        // clearance-vs-0 threshold band (spec sec 5.4 item 3)
        self.band_check(clr, 0.0);
        (clr, best_pt)
    }

    // Same rationale as `seg_clear_one` immediately above: this is that
    // primitive's batched (SoA) entry point, one argument per per-member
    // input array plus the shared `standoff` -- the established batch shape,
    // not a candidate for bundling.
    #[allow(clippy::too_many_arguments)]
    pub fn segment_clearance(
        &self,
        p0s: &[V3],
        p1s: &[V3],
        r0s: &[f64],
        r1s: &[f64],
        standoff: f64,
        inset0s: &[f64],
        inset1s: &[f64],
    ) -> Result<(Vec<f64>, Vec<V3>), BatchInputError> {
        let n = p0s.len();
        BatchInputError::require_len("segment_clearance", "p1s", n, p1s.len())?;
        BatchInputError::require_len("segment_clearance", "r0s", n, r0s.len())?;
        BatchInputError::require_len("segment_clearance", "r1s", n, r1s.len())?;
        BatchInputError::require_len("segment_clearance", "inset0s", n, inset0s.len())?;
        BatchInputError::require_len("segment_clearance", "inset1s", n, inset1s.len())?;
        let res: Vec<(f64, V3)> = par::run(|| {
            (0..n)
                .into_par_iter()
                .map(|i| {
                    self.seg_clear_one(
                        p0s[i], p1s[i], r0s[i], r1s[i], standoff, inset0s[i], inset1s[i],
                    )
                })
                .collect()
        });
        let mut clr = Vec::with_capacity(n);
        let mut pt = Vec::with_capacity(n);
        for (c, p) in res {
            clr.push(c);
            pt.push(p);
        }
        Ok((clr, pt))
    }

    // ---- penetration intervals -------------------------------------------
    // Same rationale as `seg_clear_one` above: the per-member primitive
    // behind the batched `penetration_intervals_batch` entry point
    // (SPEC-EMBED-POLICY rule-4 transit detection) -- every argument is the
    // geometry + sampling config a single interval extraction needs.
    #[allow(clippy::too_many_arguments)]
    fn pen_intervals_one(
        &self,
        p0: V3,
        p1: V3,
        r0: f64,
        r1: f64,
        n_offset_rays: usize,
        majority_step_mm: f64,
        cap: usize,
        min_illegal_len_mm: f64,
        rays_all: bool,
    ) -> Vec<Interval> {
        let l = norm(sub(p1, p0));
        let mut out = Vec::new();
        if l < 1e-9 {
            return out;
        }
        // build ray list: (ray_id, O, Dv, seg)
        let mut raylist: Vec<(i32, V3, V3, f64)> = Vec::new();
        if !rays_all {
            raylist.push((-1, p0, sub(p1, p0), l));
        } else {
            // member_rays: axis + n offset rays
            raylist.push((-1, p0, sub(p1, p0), l));
            let du = scale(sub(p1, p0), 1.0 / l);
            let (e1, e2) = perp_basis(du);
            for k in 0..n_offset_rays {
                let th = 2.0 * std::f64::consts::PI * (k as f64) / (n_offset_rays as f64);
                let uu = add(scale(e1, th.cos()), scale(e2, th.sin()));
                let a = add(p0, scale(uu, r0));
                let b = add(p1, scale(uu, r1));
                let dv = sub(b, a);
                raylist.push((k as i32, a, dv, norm(dv)));
            }
        }

        for (label, o, dv, seg) in raylist {
            if seg < 1e-9 {
                continue;
            }
            // crossings along this ray
            let hits = self.collect_hits(o, dv, 1.0);
            let mut tvals: Vec<f64> = hits.iter().map(|h| h.0).collect();
            tvals.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let ts = dedup_loc(&tvals, seg, DEDUP_LOC_MM);
            // boundaries [0, ts..., 1]
            let mut b_bounds: Vec<f64> = Vec::with_capacity(ts.len() + 2);
            b_bounds.push(0.0);
            b_bounds.extend_from_slice(&ts);
            b_bounds.push(1.0);

            // sample each sub-segment, gather all sample points
            let mut sample_pts: Vec<V3> = Vec::new();
            let mut slices: Vec<(usize, usize)> = Vec::new();
            let mut sub_bounds: Vec<(f64, f64, bool, bool)> = Vec::new();
            let nb = b_bounds.len();
            for si in 0..nb - 1 {
                let ta = b_bounds[si];
                let tb = b_bounds[si + 1];
                let lsub = (tb - ta) * seg;
                let ns = cap.min((3usize).max((lsub / majority_step_mm).trunc() as usize));
                // Interior linear spacing: step=(tb-ta)/(ns+1), i=1..ns.
                let step = (tb - ta) / ((ns + 1) as f64);
                let run0 = sample_pts.len();
                for i in 1..=ns {
                    let ti = ta + (i as f64) * step;
                    sample_pts.push(add(o, scale(dv, ti)));
                }
                slices.push((run0, sample_pts.len()));
                sub_bounds.push((ta, tb, si != 0, (si + 1) != (nb - 1)));
            }

            // inside classification + depth (nearest dist) for inside samples
            let mut ins_all = vec![false; sample_pts.len()];
            let mut dist_all = vec![0.0f64; sample_pts.len()];
            for (i, &sp) in sample_pts.iter().enumerate() {
                let ins = self.inside_serial(sp);
                ins_all[i] = ins;
                if ins {
                    let (di, _, _) = self.nearest_one(sp, f64::INFINITY);
                    dist_all[i] = di;
                }
            }

            // per sub-segment inside vote + depth
            let nsub = slices.len();
            let mut insub = vec![false; nsub];
            let mut depsub = vec![0.0f64; nsub];
            for (k, &(a0, a1)) in slices.iter().enumerate() {
                if a1 > a0 {
                    let cnt = a1 - a0;
                    let mut nin = 0usize;
                    let mut dmax = 0.0f64;
                    for i in a0..a1 {
                        if ins_all[i] {
                            nin += 1;
                            if dist_all[i] > dmax {
                                dmax = dist_all[i];
                            }
                        }
                    }
                    // mean >= 0.5
                    insub[k] = (nin as f64) / (cnt as f64) >= 0.5;
                    depsub[k] = dmax;
                }
            }

            // merge consecutive inside runs into intervals
            let mut i = 0usize;
            while i < nsub {
                if insub[i] {
                    let mut jrun = i;
                    while jrun + 1 < nsub && insub[jrun + 1] {
                        jrun += 1;
                    }
                    let ta = sub_bounds[i].0;
                    let tb = sub_bounds[jrun].1;
                    let entered = sub_bounds[i].2;
                    let exited = sub_bounds[jrun].3;
                    let mut depth = 0.0f64;
                    for d in &depsub[i..=jrun] {
                        if *d > depth {
                            depth = *d;
                        }
                    }
                    let s_a = ta * seg;
                    let s_b = tb * seg;
                    // interval-length vs min_illegal_len threshold band (sec 5.4 item 3)
                    self.band_check(s_b - s_a, min_illegal_len_mm);
                    if (s_b - s_a) > min_illegal_len_mm {
                        out.push(Interval {
                            s_a,
                            s_b,
                            entered,
                            exited,
                            max_depth: depth,
                            ray: label,
                        });
                    }
                    i = jrun + 1;
                } else {
                    i += 1;
                }
            }
        }
        out
    }

    /// Batched penetration_intervals (Amendment A7). Returns per-member interval
    /// lists as ragged SoA.
    #[allow(clippy::too_many_arguments)]
    pub fn penetration_intervals_batch(
        &self,
        p0s: &[V3],
        p1s: &[V3],
        r0s: &[f64],
        r1s: &[f64],
        n_offset_rays: usize,
        majority_step_mm: f64,
        cap: usize,
        min_illegal_len_mm: f64,
        rays_all: bool,
    ) -> Result<Vec<Vec<Interval>>, BatchInputError> {
        let n = p0s.len();
        BatchInputError::require_len("penetration_intervals_batch", "p1s", n, p1s.len())?;
        BatchInputError::require_len("penetration_intervals_batch", "r0s", n, r0s.len())?;
        BatchInputError::require_len("penetration_intervals_batch", "r1s", n, r1s.len())?;
        Ok(par::run(|| {
            (0..n)
                .into_par_iter()
                .map(|i| {
                    self.pen_intervals_one(
                        p0s[i],
                        p1s[i],
                        r0s[i],
                        r1s[i],
                        n_offset_rays,
                        majority_step_mm,
                        cap,
                        min_illegal_len_mm,
                        rays_all,
                    )
                })
                .collect()
        }))
    }

    // ---- sphere breach ----------------------------------------------------
    /// Collect faces whose exact point-distance to `c` is within `tol` of `nd`
    /// (for measured-tie detection). Branch & bound, ascending face index.
    fn faces_at_distance(&self, c: V3, nd: f64, tol: f64) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        let thr = nd + tol;
        STACK.with(|s| {
            let mut stack = s.borrow_mut();
            stack.clear();
            stack.push(0);
            while let Some(ni) = stack.pop() {
                let node = &self.bvh.nodes[ni as usize];
                if point_box_dist(c, node.bmin, node.bmax) > thr {
                    continue;
                }
                if node.leaf {
                    let start = node.a as usize;
                    let cnt = node.b as usize;
                    for &tid in &self.bvh.order[start..start + cnt] {
                        let t = tid as usize;
                        let cp = closest_on_tri(
                            c,
                            self.mesh.v0(t),
                            self.mesh.vertex_b(t),
                            self.mesh.vertex_c(t),
                        );
                        let d = norm(sub(c, cp));
                        if d <= thr {
                            out.push(t);
                        }
                    }
                } else {
                    stack.push(node.a);
                    stack.push(node.b);
                }
            }
        });
        out.sort_unstable();
        out
    }

    /// far-wall thickness along the inward cone for one orientation.
    /// inf if no far wall beyond the near-wall skip.
    fn cone_thickness(
        &self,
        c: V3,
        r: f64,
        inward: V3,
        nd: f64,
        half_deg: f64,
        n_cone: usize,
    ) -> f64 {
        let mut thick = f64::INFINITY;
        for dcone in cone_dirs(inward, half_deg, n_cone) {
            let dir = scale(dcone, r);
            let hits = self.collect_hits(c, dir, 1.0);
            let mut dmin = f64::INFINITY;
            for (tv, _f) in hits {
                let dd = tv * r;
                if dd > nd + NEAR_WALL_SKIP && dd < dmin {
                    dmin = dd;
                }
            }
            if dmin < thick {
                thick = dmin;
            }
        }
        thick
    }

    fn breach_one(
        &self,
        c: V3,
        r: f64,
        eps_mm: f64,
        half_deg: f64,
        n_cone: usize,
    ) -> (bool, f64, f64) {
        let (nd, _cp, tri, fn_) = {
            let (d, i, cp) = self.nearest_one(c, f64::INFINITY);
            let n = if i < self.mesh.global_nfaces() {
                self.mesh.normal(i)
            } else {
                [0.0; 3]
            };
            (d, cp, i, n)
        };
        let _ = tri;
        // reach precondition r−nd vs eps_mm threshold band (sec 5.4 item 3)
        self.band_check(r - nd, eps_mm);
        if r - nd <= eps_mm {
            return (false, f64::INFINITY, 0.0);
        }
        let inward = {
            let iv = scale(fn_, -1.0);
            let nrm = norm(iv);
            if nrm == 0.0 {
                iv
            } else {
                scale(iv, 1.0 / nrm)
            }
        };
        let t1 = self.cone_thickness(c, r, inward, nd, half_deg, n_cone);

        // measured-tie detection (Amendment A15): opposing-normal face at dist nd
        let mut t2 = f64::INFINITY;
        let mut tie = false;
        let tied = self.faces_at_distance(c, nd, 1e-12);
        if tied.len() > 1 {
            for &f in &tied {
                if dot(self.mesh.normal(f), fn_) < 0.0 {
                    let inw2 = {
                        let iv = scale(self.mesh.normal(f), -1.0);
                        let nrm = norm(iv);
                        if nrm == 0.0 {
                            iv
                        } else {
                            scale(iv, 1.0 / nrm)
                        }
                    };
                    t2 = self.cone_thickness(c, r, inw2, nd, half_deg, n_cone);
                    tie = true;
                    break;
                }
            }
        }

        // thickness-vs-(r−eps) threshold band, per breaching orientation (sec 5.4 item 3)
        self.band_check(t1, r - eps_mm);
        if tie {
            self.band_check(t2, r - eps_mm);
        }
        let breach1 = t1.is_finite() && t1 < r - eps_mm;
        let breach2 = tie && t2.is_finite() && t2 < r - eps_mm;
        if tie {
            self.tie_counter.fetch_add(1, Ordering::Relaxed);
        }
        let breached = breach1 || breach2;
        if !breached {
            // report primary thickness (matches non-tie oracle behavior)
            return (false, t1, 0.0);
        }
        // breaching orientation's thickness; smaller finite when both breach
        let thick = if breach1 && breach2 {
            t1.min(t2)
        } else if breach1 {
            t1
        } else {
            t2
        };
        (true, thick, r - thick)
    }

    pub fn sphere_breach(
        &self,
        centers: &[V3],
        radii: &[f64],
        eps_mm: f64,
        half_deg: f64,
        n_cone: usize,
    ) -> Result<SphereBreachBatch, BatchInputError> {
        let n = centers.len();
        BatchInputError::require_len("sphere_breach", "radii", n, radii.len())?;
        let res: Vec<(bool, f64, f64)> = par::run(|| {
            (0..n)
                .into_par_iter()
                .map(|i| self.breach_one(centers[i], radii[i], eps_mm, half_deg, n_cone))
                .collect()
        });
        let mut br = Vec::with_capacity(n);
        let mut th = Vec::with_capacity(n);
        let mut po = Vec::with_capacity(n);
        for (b, t, p) in res {
            br.push(b);
            th.push(t);
            po.push(p);
        }
        Ok((br, th, po))
    }

    // ---- exterior-face mask ----------------------------------------------
    pub fn exterior_face_mask(&self) -> &[bool] {
        self.exterior_mask.get_or_init(|| {
            let bary: [[f64; 3]; 3] = [
                [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0],
                [0.6, 0.2, 0.2],
                [0.2, 0.6, 0.2],
            ];
            let eps_ladder = [0.05f64, 0.15f64];
            par::run(|| {
                self.mesh
                    .face_ids()
                    .par_iter()
                    .map(|&face_id| {
                        let f = face_id as usize;
                        let a = self.mesh.v0(f);
                        let b = self.mesh.vertex_b(f);
                        let cc = self.mesh.vertex_c(f);
                        let fn_ = self.mesh.normal(f);
                        let mut votes = 0u32;
                        for w in &bary {
                            let base = add(add(scale(a, w[0]), scale(b, w[1])), scale(cc, w[2]));
                            for &eps in &eps_ladder {
                                let sp = add(base, scale(fn_, eps));
                                if self.inside_serial(sp) {
                                    votes += 1;
                                }
                            }
                        }
                        votes < 4 // exterior iff NOT interior(>=4)
                    })
                    .collect::<Vec<bool>>()
            })
        })
    }
}

// ---- ray-basis helpers (ported perp_basis / _cone_dirs) ------------------

/// geometry_exact.py:147-151 perp_basis
fn perp_basis(d: V3) -> (V3, V3) {
    let a: V3 = if d[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let mut e1 = cross(d, a);
    let n1 = norm(e1);
    let s1 = if n1 == 0.0 { 1.0 } else { n1 };
    e1 = scale(e1, 1.0 / s1);
    let mut e2 = cross(d, e1);
    let n2 = norm(e2);
    let s2 = if n2 == 0.0 { 1.0 } else { n2 };
    e2 = scale(e2, 1.0 / s2);
    (e1, e2)
}

/// geometry_exact.py:935-945 _cone_dirs
fn cone_dirs(axis: V3, half_deg: f64, n: usize) -> Vec<V3> {
    let na = norm(axis);
    let sa = if na == 0.0 { 1.0 } else { na };
    let axis = scale(axis, 1.0 / sa);
    let e1 = perp_basis(axis).0;
    let e2 = cross(axis, e1);
    let th = half_deg.to_radians();
    let mut dirs = vec![axis];
    for k in 0..n {
        let ph = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
        let d = add(
            scale(axis, th.cos()),
            scale(add(scale(e1, ph.cos()), scale(e2, ph.sin())), th.sin()),
        );
        let nd = norm(d);
        dirs.push(scale(d, 1.0 / nd));
    }
    dirs
}
