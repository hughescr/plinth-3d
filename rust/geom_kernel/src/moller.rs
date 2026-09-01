//! Möller-Trumbore ray/segment vs triangle, ported VERBATIM from
//! geometry_exact.py:110-131 (`_moller`). Consumes the RAW stored E1/E2 (NOT
//! reconstructed vertices — Amendment A6), different from closest_on_tri.

use crate::consts::{DET_EPS, EPS};
use crate::vec3::{cross, dot, sub, V3};

/// One ray/triangle result, carrying the barycentrics the pinned sequence has
/// already computed.
///
/// Published so a caller that needs edge-proximity policy (triage's
/// `Ambiguous`) builds it from these values instead of recomputing them with a
/// second formula — the divergence `docs/specs/geometry-canon.md` §1 OP-2
/// exists to close.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayTriangleHit {
    /// Whether the ray meets the acceptance region. **When this is `false`,
    /// `t`, `u` and `v` are meaningless and must not be read**: on a
    /// determinant miss they are the guarded `inv = 0` products, i.e. all
    /// exactly `0.0`.
    pub hit: bool,
    /// Ray parameter: the ray point is `o + t * dv`. In units of `|dv|`, so it
    /// is a millimetre only when `dv` is unit length.
    pub t: f64,
    /// First barycentric coordinate.
    pub u: f64,
    /// Second barycentric coordinate. The triangle interior is
    /// `u >= 0, v >= 0, u + v <= 1`.
    pub v: f64,
}

/// One ray (`o`, `dv`) vs one triangle (`v0`, `e1`, `e2`), Möller-Trumbore.
///
/// **Canonical for the workspace** (`docs/specs/geometry-canon.md` §1 OP-2,
/// plan item C5). `crates/plinth-mesh/src/triage.rs`'s private
/// `ray_triangle_distance()` is retired in favour of this function.
///
/// **A6 pin, carried verbatim from this module's header:** *"ported VERBATIM
/// from geometry_exact.py:110-131 (`_moller`). Consumes the RAW stored E1/E2
/// (NOT reconstructed vertices — Amendment A6), different from
/// closest_on_tri."* A caller holding only vertices computes
/// `e1 = vec3::sub(b, a)` and `e2 = vec3::sub(c, a)` — the same subtraction
/// this kernel's own surface build uses — and says in a comment at the call
/// site that it is reconstructing.
///
/// # Units
///
/// `o`, `v0`, `e1`, `e2` are mm. `t` is in units of `|dv|`: the ray point is
/// `o + t * dv`. The `±EPS` slack on `t` and the `t_max` bound are in those
/// same units, so **the canonical usage passes a unit-length `dv`, which makes
/// `t` a millimetre.** A caller passing a non-unit `dv` must scale `t_max`
/// itself and accept that the `t` slack scales with it. This is stated, not
/// enforced: enforcing it would put a normalize inside a pinned hot path.
///
/// # Acceptance region
///
/// `|det| > DET_EPS` (1e-14) and `u >= -EPS`, `v >= -EPS`, `u + v <= 1 + EPS`,
/// `t ∈ [-EPS, t_max + EPS]`, with `EPS = 1e-6`.
///
/// `EPS`'s job is **consistency at shared edges**: a ray crossing exactly
/// through an edge shared by two faces must register on at least one of them. A
/// zero-slack test can miss BOTH faces on such a crossing, which silently flips
/// parity for every point beyond it. The accepted cost is a 1e-6-wide
/// barycentric shell in which a ray just outside the surface registers on both
/// faces of a silhouette edge; both hits land at the same `t`, so any dedup
/// (see [`dedup_loc_into`]) merges them to one.
///
/// # Degenerate-input contract
///
/// * **Degenerate triangle, or a ray parallel to the triangle's plane**
///   (`|det| <= DET_EPS = 1e-14`): `hit = false`, and `t`, `u`, `v` are all
///   exactly `0.0` (the `inv = 0` guard). This is correct rather than
///   conservative: a ray parallel to a face's plane has its true crossings
///   registered on the adjacent non-parallel faces, and measuring through a
///   near-singular `1/det` would amplify noise into a fake crossing. `DET_EPS`
///   is therefore a PRUNE — the answer is taken from a neighbouring face — not
///   a verdict.
/// * **Zero-length `dv`**: `pvec = cross(dv, e2)` is `[0, 0, 0]`, so `det` is
///   `0.0` and the call is a miss by the rule above. No division by zero
///   escapes — `inv` is the guarded `0.0`.
/// * **Non-finite input is NOT guarded**; it propagates, and a `NaN` in `u`,
///   `v` or `t` fails every acceptance comparison, so `hit` is `false`. A
///   caller that must distinguish "refused because non-finite" from "missed"
///   checks `is_finite` itself, at its own call site, in its own vocabulary —
///   that is caller policy, not geometry (triage's existing checks move there).
/// * **Never panics, never allocates, never returns an error.**
#[inline]
pub fn ray_triangle(o: V3, dv: V3, v0: V3, e1: V3, e2: V3, t_max: f64) -> RayTriangleHit {
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
    RayTriangleHit { hit, t, u, v }
}

/// `(hit, t)` projection of [`ray_triangle`] for this kernel's own hot paths.
///
/// Adds no arithmetic — it is exactly [`ray_triangle`] with `u`/`v` dropped, so
/// the two are bit-identical by construction.
#[inline]
pub fn moller(o: V3, dv: V3, v0: V3, e1: V3, e2: V3, t_max: f64) -> (bool, f64) {
    let RayTriangleHit { hit, t, .. } = ray_triangle(o, dv, v0, e1, e2, t_max);
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
