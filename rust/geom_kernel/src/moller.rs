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
/// * **`det == 0.0` exactly** — a degenerate triangle, a zero direction, or a
///   ray lying IN the triangle's plane: `hit = false`, and `t`, `u`, `v` are
///   all exactly `0.0` (the `inv = 0` guard). This one is a genuine PRUNE. No
///   interior crossing exists; a ray in a face's plane crosses the closed
///   surface at that face's EDGES, and the adjacent non-parallel faces
///   register those crossings.
/// * **`0 < |det| <= DET_EPS = 1e-14`** — the ray is NEARLY parallel to the
///   plane: `hit = false` on the same guard. **This is not a prune, and the
///   paragraph that used to stand here saying it was is measured false.** A
///   ray nearly parallel to the plane, with its origin close to that plane,
///   pierces the face INTERIOR at a real finite `t`, and no adjacent face
///   contains that crossing point. Pinned by exact arithmetic
///   (`docs/specs/geometry-adjudication.md` §O4, and `tests/ray_class.rs`):
///   triangle `(0,0,0),(2,0,0),(0,2,0)` with `o = (0.4, 0.5, -1e-16)`,
///   `dv = (1, 0, 2e-15)` gives `det = -8e-15` and a genuine crossing at
///   `t ≈ 0.05`, barycentrics `u = 0.225, v = 0.25` — deep in the interior —
///   which this function reports as a miss. Whether the ray crosses is
///   UNDECIDABLE at this scale, and mapping undecidable to miss is a verdict,
///   not a prune.
///
///   **This function keeps that behaviour FOR ORACLE PARITY ONLY**
///   (`geometry_exact.py:110-131`). Any caller that can decide a gate, a
///   refusal, or an emitted byte must call [`ray_triangle_classified`]
///   instead and route [`RayTriangleClass::NearParallel`] to its own
///   conservative side. Measuring through a near-singular `1/det` would still
///   amplify noise into a fake crossing — which is why the answer is an
///   explicit uncertainty and not a manufactured hit.
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
///
/// **Parity pin only.** Like [`ray_triangle`], this projection maps the nonzero
/// determinant band to a silent miss. After
/// `docs/specs/geometry-adjudication.md` §EXT its only consumers are this
/// kernel's parity tests, the feature-gated `testkit`, and the agreement
/// battery that holds it bit-identical to [`ray_triangle_classified`]. No
/// production walk in this kernel calls it any more.
#[inline]
pub fn moller(o: V3, dv: V3, v0: V3, e1: V3, e2: V3, t_max: f64) -> (bool, f64) {
    let RayTriangleHit { hit, t, .. } = ray_triangle(o, dv, v0, e1, e2, t_max);
    (hit, t)
}

/// What one ray/triangle evaluation actually established — hit, definite miss,
/// or "undecidable at this scale".
///
/// This type exists because [`ray_triangle`]'s `bool` cannot say the third
/// thing, so it said "miss", and a real interior crossing vanished from every
/// parity count downstream (§O4's counterexample, reproduced in this module's
/// degenerate-input contract). **An uncertainty collapsed back to a boolean
/// anywhere in the chain is the same silent miss under a new name.**
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RayTriangleClass {
    /// `|det| > DET_EPS` and the full acceptance region is met. The payload is
    /// the pinned function's own result, `hit == true`, carrying the
    /// barycentrics the sequence already computed.
    Hit(RayTriangleHit),
    /// `|det| > DET_EPS` and acceptance failed: a definite miss. The
    /// determinant is well clear of the band, so the arithmetic decided this.
    Miss,
    /// `0.0 < |det| <= DET_EPS`: the determinant is nonzero but inside the
    /// uncertainty band. Whether the ray crosses this face is **undecidable at
    /// this scale** — the caller chooses the conservative side, and counts the
    /// event.
    ///
    /// A non-finite determinant (`NaN`, from non-finite input this kernel does
    /// not guard) lands here too: every band comparison is false for `NaN`, and
    /// the conservative side is where an unguarded input belongs.
    NearParallel,
    /// `det == 0.0` exactly: degenerate triangle, zero direction, or a ray
    /// exactly in the plane. No interior crossing exists; edge crossings live
    /// on adjacent faces. A genuine prune.
    ExactParallel,
}

impl RayTriangleClass {
    /// The accepted hit, or `None` for every other class.
    ///
    /// Deliberately NOT a `bool`: a caller that wants the hit set writes
    /// `.hit()`, and a caller that wants to know about uncertainty has to look
    /// at the class, because there is no predicate here that quietly folds
    /// [`Self::NearParallel`] into "no".
    #[must_use]
    pub const fn hit(self) -> Option<RayTriangleHit> {
        match self {
            Self::Hit(h) => Some(h),
            Self::Miss | Self::NearParallel | Self::ExactParallel => None,
        }
    }

    /// Is this the undecidable class?
    #[must_use]
    pub const fn is_near_parallel(self) -> bool {
        matches!(self, Self::NearParallel)
    }
}

/// [`ray_triangle`] with the determinant guard SURFACED instead of folded into
/// `hit`.
///
/// **Canonical for every decision-feeding caller** — anything that can decide a
/// gate, a refusal, or a byte of an emitted artifact
/// (`docs/specs/geometry-adjudication.md` §O4, §EXT-2). The pinned
/// [`ray_triangle`] stays canonical for the oracle parity path and nothing
/// else.
///
/// # Agreement contract
///
/// The arithmetic sequence below is the pinned function's, term for term, with
/// `valid` returned rather than `&&`-ed away. It is duplicated rather than
/// shared on purpose: the pinned body is a parity pin that this fix does not
/// edit, and the agreement is enforced by a test battery
/// (`tests/ray_class.rs`) rather than by refactoring. For every input:
///
/// * `ray_triangle(..).hit == matches!(ray_triangle_classified(..), Hit(_))`;
/// * on a hit, the payload's `t`, `u`, `v` are bit-identical to the pinned
///   function's.
///
/// # Units, acceptance region and non-finite input
///
/// Identical to [`ray_triangle`] — read its contract. The only difference is
/// which of the four outcomes above a non-`Hit` is reported as. `t`, `u` and
/// `v` from a non-`Hit` are the guarded `inv = 0` products and are NOT
/// reachable through this API: the classification, not a meaningless number, is
/// what propagates.
#[inline]
pub fn ray_triangle_classified(
    o: V3,
    dv: V3,
    v0: V3,
    e1: V3,
    e2: V3,
    t_max: f64,
) -> RayTriangleClass {
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
    if hit {
        RayTriangleClass::Hit(RayTriangleHit { hit, t, u, v })
    } else if valid {
        RayTriangleClass::Miss
    } else if det == 0.0 {
        RayTriangleClass::ExactParallel
    } else {
        // 0 < |det| <= DET_EPS, or a non-finite determinant: undecidable.
        RayTriangleClass::NearParallel
    }
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
