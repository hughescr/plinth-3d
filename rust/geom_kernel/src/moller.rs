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

/// Merge ascending crossings closer than `dt` in arc length, keeping first.
/// `keep` is cleared before the stable pass and must be pre-reserved by the
/// caller when a hard allocation ceiling is required.
pub fn dedup_loc_into(ts: &[f64], dt: f64, keep: &mut Vec<f64>) {
    keep.clear();
    if let Some(&first) = ts.first() {
        keep.push(first);
        for &x in &ts[1..] {
            if x - *keep.last().unwrap() > dt {
                keep.push(x);
            }
        }
    }
}
