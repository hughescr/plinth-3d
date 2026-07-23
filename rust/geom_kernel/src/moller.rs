//! Möller-Trumbore ray/segment vs triangle, ported VERBATIM from
//! geometry_exact.py:110-131 (`_moller`). Consumes the RAW stored E1/E2 (NOT
//! reconstructed vertices — Amendment A6), different from closest_on_tri.

use crate::consts::{DET_EPS, EPS};
use crate::vec3::{cross, dot, sub, V3};

/// One ray (`o`, `dv`) vs one triangle (`v0`, `e1`, `e2`). Ray point =
/// `o + t*dv`. Returns `(hit, t)`; `t` is meaningful only when `hit`.
/// Acceptance region is the oracle's tolerant test: `u,v >= -EPS`,
/// `u+v <= 1+EPS`, `t in [-EPS, t_max+EPS]`.
#[inline]
pub fn moller(o: V3, dv: V3, v0: V3, e1: V3, e2: V3, t_max: f64) -> (bool, f64) {
    let pvec = cross(dv, e2);
    let det = dot(e1, pvec);
    let valid = det.abs() > DET_EPS;
    let inv = if valid { 1.0 / det } else { 0.0 };
    let tvec = sub(o, v0);
    let u = dot(tvec, pvec) * inv;
    let qvec = cross(tvec, e1);
    let v = dot(dv, qvec) * inv;
    let t = dot(e2, qvec) * inv;
    let hit = valid
        && (u >= -EPS)
        && (v >= -EPS)
        && (u + v <= 1.0 + EPS)
        && (t >= -EPS)
        && (t <= t_max + EPS);
    (hit, t)
}

/// Merge crossings closer than `tol_mm` in arc length, keep-first
/// (geometry_exact.py:134-144 `dedup_loc`). `ts` MUST be ascending.
/// Returns the kept parameter values in order.
pub fn dedup_loc(ts: &[f64], seg_len: f64, tol_mm: f64) -> Vec<f64> {
    if ts.is_empty() {
        return Vec::new();
    }
    let dt = tol_mm / seg_len.max(1e-9);
    let mut keep = Vec::with_capacity(ts.len());
    keep.push(ts[0]);
    for &x in &ts[1..] {
        if x - *keep.last().unwrap() > dt {
            keep.push(x);
        }
    }
    keep
}
