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
    /// A definite miss, decided one of two ways.
    ///
    /// Either `|det| > DET_EPS` and acceptance failed — the determinant is well
    /// clear of the band, so the arithmetic decided it — **or** the determinant
    /// is inside the band and the ray provably does not reach the triangle at
    /// all. The band says the ray is nearly parallel to the triangle's plane
    /// and says nothing whatever about whether the ray goes near the triangle,
    /// so a ray passing far to one side of a wall it runs alongside is a miss
    /// on the geometry no matter what its determinant does. Two predicates
    /// decide that second case, and each declines — leaving
    /// [`Self::NearParallel`] — whenever its own arithmetic cannot settle the
    /// question: `span_clears_plane` (the whole accepted span lies strictly on
    /// one side of the triangle's plane, under a derived error bound) and
    /// `ray_clears_triangle` (the ray clears the triangle's bounding sphere).
    Miss,
    /// `0.0 < |det| <= DET_EPS`, AND the ray is not shown to clear the triangle:
    /// the determinant is nonzero but inside the uncertainty band, and the
    /// triangle is within reach. Whether the ray crosses this face is
    /// **undecidable at this scale** — the caller chooses the conservative side,
    /// and counts the event.
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
    } else if span_clears_plane(tvec, dv, e1, e2, t_max) {
        // Nearly parallel, and the whole accepted span stays strictly on one
        // side of the triangle's plane: it cannot meet the plane, so it cannot
        // meet the triangle. The bore wall beside a drilled axis is this case.
        RayTriangleClass::Miss
    } else if ray_clears_triangle(tvec, dv, e1, e2, t_max) {
        // Nearly parallel, and never within reach of this triangle at all:
        // there is nothing here to be uncertain about.
        RayTriangleClass::Miss
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

/// Does the whole accepted ray span lie strictly on ONE side of the triangle's
/// plane?
///
/// The span is the one the acceptance test reads, `t ∈ [-EPS, t_max + EPS]`,
/// widened outward by one rounding of its upper end. If the ray never reaches
/// the plane over that span, it cannot cross the triangle, which lies in the
/// plane — so a `true` here is a definite miss **on the geometry**, whatever
/// the determinant is doing. This is the case the bounding-sphere test
/// ([`ray_clears_triangle`]) cannot settle: a long face that runs alongside
/// the ray close by, such as the wall of a drilled bore around that bore's own
/// axis. The face is longer than its distance from the axis, so its bounding
/// sphere contains the axis, but its plane does not.
///
/// **This can only move `NearParallel` to `Miss`.** It is called only after
/// the determinant has failed `|det| > DET_EPS`, where `hit` is already false,
/// so the agreement contract with the pinned [`ray_triangle`] is untouched.
///
/// # The test, and its error bound (no tolerance is chosen here)
///
/// Everything is in the `v0`-local frame: `p = o - v0` (the caller's `tvec`),
/// and the triangle is `0, e1, e2`. With `N = e1 × e2`, the signed side of the
/// ray point at parameter `s` is the affine function
/// `side(s) = A + s·B`, `A = p·N`, `B = dv·N` — the orient3d determinant of
/// the triangle and that point. An affine function with the same strict sign
/// at both ends of an interval has that sign on the whole interval, so two
/// evaluations decide the span.
///
/// Each evaluation is compared with a FORWARD ERROR BOUND on itself, derived
/// from the standard floating-point model `fl(a ∘ b) = (a ∘ b)(1 + δ)`,
/// `|δ| <= u = f64::EPSILON / 2`, with `γ_k = k·u / (1 - k·u)`. Write
/// `m_i = |e1_j·e2_k| + |e1_k·e2_j|` for the absolute value of each cross
/// component's two products:
///
/// * `n = cross(e1, e2)`: `|n_i - N_i| <= γ_2·m_i`, and `|n_i| <= (1 + γ_2)·m_i`.
/// * `p` was rounded once by `o - v0`: `|p_i - P_i| <= γ_1·|p_i|`.
/// * `dot` is a left-to-right sum of three products:
///   `|fl(p·n) - p·n| <= γ_3·Σ|p_i||n_i|`, and the same for `dv·n`.
/// * Together, `|A - A*| <= (γ_3(1 + γ_2) + γ_2 + γ_1)·Σ|p_i|·m_i
///   ≈ 6u·Σ|p_i|·m_i`, and `|B - B*| <= ≈ 5u·Σ|dv_i|·m_i` (`dv` is exact
///   input).
/// * `side(s) = fl(A + fl(s·B))` adds at most `≈ 2u·(|A| + |s|·|B|)`.
///
/// So `|side(s) - side*(s)| <= ≈ 8u·(Σ|p_i|·m_i + |s|·Σ|dv_i|·m_i)`. The code
/// uses `8·f64::EPSILON = 16u` times the computed magnitudes: double the
/// first-order bound, which covers the `O(u²)` terms and the rounding of the
/// magnitudes themselves. It is a property of the arithmetic, not an opinion
/// about closeness.
///
/// Gradual underflow is the one place the relative model fails: a product
/// that lands below `f64::MIN_POSITIVE` carries an ABSOLUTE error of up to
/// half the smallest subnormal, `η = 2^-1075`. There are two such products per
/// cross component, three per dot, and one in `s·B`, and the cross components'
/// errors are then multiplied by `|p_i|` or `|dv_i|`, so they add at most
/// `η·(2Σ|p_i| + 2|s|·Σ|dv_i| + 3 + 3|s| + 1)`. The code adds
/// `4·2^-1074·(1 + Σ|p_i| + |s|·(1 + Σ|dv_i|))`, which is more than that.
///
/// # It declines whenever the arithmetic is unsound
///
/// The lesson of [`ray_clears_triangle`] holds here: a predicate that refines
/// an uncertainty must decline when it cannot settle the question. An
/// infinite or NaN `t_max`, an overflow anywhere, or a side value within its
/// own bound of zero (a ray IN the plane, or one that really crosses it) all
/// return `false` and leave the case `NearParallel`.
fn span_clears_plane(p: V3, dv: V3, e1: V3, e2: V3, t_max: f64) -> bool {
    let n = cross(e1, e2);
    let m = [
        (e1[1] * e2[2]).abs() + (e1[2] * e2[1]).abs(),
        (e1[2] * e2[0]).abs() + (e1[0] * e2[2]).abs(),
        (e1[0] * e2[1]).abs() + (e1[1] * e2[0]).abs(),
    ];
    let a = dot(p, n);
    let b = dot(dv, n);
    let a_mag = p[0].abs() * m[0] + p[1].abs() * m[1] + p[2].abs() * m[2];
    let b_mag = dv[0].abs() * m[0] + dv[1].abs() * m[1] + dv[2].abs() * m[2];
    let p_sum = p[0].abs() + p[1].abs() + p[2].abs();
    let dv_sum = dv[0].abs() + dv[1].abs() + dv[2].abs();
    // The acceptance span's two ends. The upper end is rounded once by the
    // addition, so it is pushed outward by one more ulp-sized step: covering a
    // slightly longer span can only make this test decline more often.
    let s_lo = -EPS;
    let s_hi_rounded = t_max + EPS;
    let s_hi = s_hi_rounded + s_hi_rounded.abs() * f64::EPSILON;
    let smallest_subnormal = f64::from_bits(1);
    let side = |s: f64| -> Option<f64> {
        let value = a + s * b;
        let bound = 8.0 * f64::EPSILON * (a_mag + s.abs() * b_mag)
            + 4.0 * smallest_subnormal * (1.0 + p_sum + s.abs() * (1.0 + dv_sum));
        // A side value that is not finite, or a bound that is not, decides
        // nothing; neither does one inside its own error bound.
        if !(value.is_finite() && bound.is_finite()) || value.abs() <= bound {
            return None;
        }
        Some(value)
    };
    match (side(s_lo), side(s_hi)) {
        (Some(lo), Some(hi)) => (lo > 0.0) == (hi > 0.0),
        _ => false,
    }
}

/// Does this ray provably not reach this triangle at all?
///
/// The determinant band says the ray runs nearly parallel to the triangle's
/// plane. It says NOTHING about whether the ray goes anywhere near the
/// triangle. A ray running twenty millimetres to one side of a wall it happens
/// to be parallel to lands in the band exactly as a ray grazing that wall's
/// edge does, and calling both undecidable turns a clear miss into a refusal.
/// On a finely tessellated round base — a vertical support ray alongside a
/// vertical wall of many small facets — that is most of the traffic.
///
/// So bound the triangle by the sphere through its own vertices and ask whether
/// the ray's closest approach clears it. If it does, the ray cannot meet the
/// triangle whatever the determinant is doing, and the honest answer is a
/// definite miss. **This can only ever move `NearParallel` to `Miss`**, so the
/// agreement contract with the pinned [`ray_triangle`] — which partitions
/// `Hit` against everything else — is untouched.
///
/// Everything is in the `v0`-local frame the caller already computed: `p` is
/// `o - v0`, and the vertices are `0`, `e1`, `e2`.
///
/// # Why there is no tolerance to choose here
///
/// The comparison is `d - r > slack`, and `slack` bounds the ERROR in `d` and
/// `r`, rather than expressing anyone's opinion about closeness. Each
/// coordinate of the closest-approach vector is a short signed sum over
/// intermediates no larger than `mag`, so its rounding error is under
/// `8 * f64::EPSILON * mag` — a generous multiple of the standard `n * u` bound
/// for a sum this length — and `d` and `r` inherit it. Doubling covers both
/// sides of the comparison, and it covers the error in `t` as well: a relative
/// error in `t` displaces the sample point along the ray by about
/// `f64::EPSILON * |t * dv|`, which is one of the magnitudes `mag` is taken
/// over.
///
/// # The arithmetic must be sound before its answer is used
///
/// **These guards are load-bearing, and the first version of this function
/// lacked them while asserting it could never manufacture a miss.** It could.
/// Three demonstrated counterexamples, each a genuine intersection reported as
/// a definite `Miss`:
///
/// * `dd` is SUBNORMAL or zero. `dv = (2e-162, 0, 0)` gives the smallest
///   subnormal, which has one significant bit; `dv = (1e-200, 0, 0)` underflows
///   to zero outright, although
///   the direction is not. With `o = (-2, 0.25, 0.25)`, edges `(0, 1, 0)` and
///   `(0, 0, 1)`, and `t_max = INFINITY` — a value a real caller passes — the
///   ray crosses at `t = 2e200`.
/// * `dd` OVERFLOWS. `dv = (1e155, 0, 0)` makes `dot(dv, dv)` infinite, so
///   `t = finite / INFINITY = 0`. With edges scaled to 1e-100 and `t_max = 1`
///   the ray crosses at `t = 1e-155`.
/// * `t_max` is NaN. `f64::max` returns its non-NaN operand, so `t_max.max(0.0)`
///   quietly reports an EMPTY parameter range rather than an unusable one, and
///   `t` clamps to a perfectly finite `0.0` that passes every later check.
///
/// In all three, `t` came back as `0` — a claim that the closest approach is at
/// the ray's origin, which is simply untrue — and the distant origin then read
/// as clearance. The lesson is not the magnitudes, which no miniature produces;
/// it is that a predicate whose only job is to REFINE an uncertainty must
/// decline whenever its own arithmetic is unsound. Anything it cannot settle
/// returns `false` and stays `NearParallel`, which is the correct answer for
/// "the arithmetic cannot tell".
fn ray_clears_triangle(p: V3, dv: V3, e1: V3, e2: V3, t_max: f64) -> bool {
    // Bounding sphere: the centroid, and the vertex furthest from it. Every
    // vertex lies within that distance of the centre by construction and the
    // triangle is their convex hull, so the sphere encloses the triangle.
    let c = [
        (e1[0] + e2[0]) / 3.0,
        (e1[1] + e2[1]) / 3.0,
        (e1[2] + e2[2]) / 3.0,
    ];
    let to_e1 = sub(e1, c);
    let to_e2 = sub(e2, c);
    let r_sq = dot(c, c).max(dot(to_e1, to_e1)).max(dot(to_e2, to_e2));

    // The parameter range must BE a range. A NaN bound is unusable, not empty.
    if t_max.is_nan() {
        return false;
    }

    // The closest approach is only computable when `dd` is NORMAL. Positive and
    // finite is not enough: a subnormal `dd` — `dv = (2e-162, 0, 0)` gives the
    // smallest subnormal, 5e-324 — carries almost no significant bits, so the
    // `t` divided by it is not the closest approach in any useful sense.
    // A genuinely zero direction never arrives here — it makes the determinant
    // exactly zero, which is `ExactParallel` — so this branch is the
    // pathological one, not the ordinary one.
    let dd = dot(dv, dv);
    if !dd.is_normal() {
        return false;
    }

    let to_c = sub(c, p);
    let t = (dot(to_c, dv) / dd).clamp(0.0, t_max.max(0.0));
    let step = [t * dv[0], t * dv[1], t * dv[2]];
    let w = sub(to_c, step);
    let d_sq = dot(w, w);

    let mag = [c, p, step]
        .iter()
        .flatten()
        .fold(0.0_f64, |acc, x| if x.abs() > acc { x.abs() } else { acc });

    // Every quantity the comparison rests on, checked before it is trusted.
    if !(t.is_finite() && d_sq.is_finite() && r_sq.is_finite() && mag.is_finite()) {
        return false;
    }
    d_sq.sqrt() - r_sq.sqrt() > 16.0 * f64::EPSILON * mag
}
