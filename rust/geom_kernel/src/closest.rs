//! Exact point/segment vs triangle primitives, ported TERM-FOR-TERM from
//! geometry_exact.py with the pinned operand-construction sequences of
//! Amendments A6/A21. Idiomatic reassociation is a spec violation here even
//! when algebraically neutral — these outputs feed bit-parity pins.

use crate::consts::{CLOSEST_DENOM_REG, SEGSEG_DENOM_EPS_MM4, SEGSEG_LEN2_EPS_MM2};
use crate::vec3::{add, dot, scale, sub, V3};

/// Closest point on triangle (Ericson), geometry_exact.py:81-107.
///
/// `a`, `b`, `cc` are the RECONSTRUCTED vertices the oracle feeds in
/// (`A=V0`, `B=V0+E1`, `Cc=V0+E2`); this function recomputes `ab = b - a`,
/// `ac = cc - a` internally exactly as the oracle does — do NOT pass raw
/// E1/E2 (Amendment A6).
#[inline]
pub fn closest_on_tri(p: V3, a: V3, b: V3, cc: V3) -> V3 {
    closest_on_tri_edges(p, a, b, cc, sub(b, a), sub(cc, a))
}

/// [`closest_on_tri`] with the two edge vectors `ab = b - a` and `ac = cc - a`
/// handed in, for a caller that tests several points against one triangle.
/// They MUST be the reconstructed differences `sub(b, a)` and `sub(cc, a)`
/// (Amendment A6), never the raw `e1` and `e2`.
///
/// Every output is the same f64 expression as the oracle's, evaluated in the
/// same operand order; only the ORDER OF EVALUATION is lazy. The oracle builds
/// all seven candidate points and then picks one by mask priority. Each mask is
/// a pure function of the dot products, so this computes the masks first, in
/// priority order, and builds only the winning point. A vertex region (the
/// common case for a point far from a face) costs two to six dot products and no
/// division at all.
#[inline]
pub(crate) fn closest_on_tri_edges(p: V3, a: V3, b: V3, cc: V3, ab: V3, ac: V3) -> V3 {
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    // The oracle applies np.where in order base<-cBC<-cAC<-cAB<-C<-B<-A, so the
    // LAST-applied mask (mA) wins: reversed priority mA > mB > mC > mAB > mAC >
    // mBC > base.
    if (d1 <= 0.0) && (d2 <= 0.0) {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if (d3 >= 0.0) && (d4 <= d3) {
        return b;
    }
    let cp = sub(p, cc);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if (d6 >= 0.0) && (d5 <= d6) {
        return cc;
    }
    let vc = d1 * d4 - d3 * d2;
    let vb = d5 * d2 - d1 * d6;
    let va = d3 * d6 - d5 * d4;

    // edge parameters with the oracle's `where((den)!=0, num/den, 0.0)` guard
    if (vc <= 0.0) && (d1 >= 0.0) && (d3 <= 0.0) {
        let den_ab = d1 - d3;
        let t_ab = if den_ab != 0.0 { d1 / den_ab } else { 0.0 };
        return add(a, scale(ab, t_ab));
    }
    if (vb <= 0.0) && (d2 >= 0.0) && (d6 <= 0.0) {
        let den_ac = d2 - d6;
        let t_ac = if den_ac != 0.0 { d2 / den_ac } else { 0.0 };
        return add(a, scale(ac, t_ac));
    }
    if (va <= 0.0) && ((d4 - d3) >= 0.0) && ((d5 - d6) >= 0.0) {
        let den_bc = (d4 - d3) + (d5 - d6);
        let t_bc = if den_bc != 0.0 {
            (d4 - d3) / den_bc
        } else {
            0.0
        };
        return add(b, scale(sub(cc, b), t_bc)); // B + (Cc - B)*tBC
    }
    let denom = 1.0 / (va + vb + vc + CLOSEST_DENOM_REG);
    let v = vb * denom;
    let w = vc * denom;
    // base out = A + ab*v + ac*w  (association ((A + ab*v) + ac*w))
    add(add(a, scale(ab, v)), scale(ac, w))
}

/// Closest distance between two 3D segments, plus the closest point on each.
///
/// **Canonical for the workspace** (`docs/specs/geometry-canon.md` §1 OP-1,
/// plan item C3). `crates/plinth-hollow/src/drilling.rs`'s private
/// `segment_distance()` is retired in favour of this function: its parallel
/// guard compared an mm⁴ determinant against an mm constant
/// (`PROOF_EPSILON_MM = 1e-7`), forcing the `s = 0` branch on 35.98 % of pairs
/// at drain-keepout scale with no correct recovery, and answering wrong by up
/// to 4.494e-2 mm. This implementation is bit-identical to a 20,001²
/// brute-force sweep on that worst case.
///
/// Ported TERM-FOR-TERM from geometry_exact.py:287-310. **A21 pin, carried
/// verbatim from this module's header so a later "simplification" fails review
/// rather than parity:** *"ported TERM-FOR-TERM from geometry_exact.py with the
/// pinned operand-construction sequences of Amendments A6/A21. Idiomatic
/// reassociation is a spec violation here even when algebraically neutral —
/// these outputs feed bit-parity pins."* Concretely, the pinned sequence is:
/// `s` from the denominator then clamped; `t` computed from that `s` then
/// clamped; `s` RE-computed from `t` then clamped again. Collapsing the
/// re-clamp is what makes the retired copy wrong.
///
/// # Units and orientation
///
/// All four points are mm. Returns `(distance_mm, closest_on_p1q1,
/// closest_on_p2q2)` — the distance is `dist` of the two returned points, so a
/// caller never recomputes it. `p1q1` is the (broadcast) query segment and
/// `p2q2` the triangle edge in this kernel's own use; the function itself is
/// symmetric apart from which returned point belongs to which segment.
///
/// # Degenerate-input contract
///
/// Every finite input gets a finite answer. There is no refusal path and no
/// `Option`.
///
/// * **Parallel or near-parallel** (`denom = |u|²|v|² − (u·v)² ≤ 1e-14`): takes
///   the `s = 0` branch, and the A21 re-clamp then recovers the minimum. For
///   EXACTLY parallel pairs that minimum is exact. **For NEAR-parallel pairs it
///   is not exact, and the canon's OP-1 sentence overstates it**: the re-clamp
///   is one coordinate-descent step, so the answer can sit slightly ABOVE the
///   true minimum. Measured over 20,000 near-parallel pairs at drain-keepout
///   scale (0.005–0.05 mm segments, every case on this branch, brute-forced
///   against a 20,001-sample sweep): the excess is at most **5.145e-6 mm**
///   (2.49e-3 relative), and it shrinks with segment length — 4.10e-8 mm at
///   0.5–5 mm, 4.50e-9 mm at 5–50 mm.
///
///   The guard therefore still PRUNES rather than decides, but by a measured
///   margin, not by exactness: 5.145e-6 mm is 1/58,000 of the 0.3 mm hard
///   clearance limit and 1/117,000 of the 0.6 mm placement margin, and it is
///   8,700× smaller than the retired drilling copy's 4.494e-2 mm error. The
///   difference in kind remains the one the canon names: this branch lands
///   within a few parts in 10⁶ of the true answer, drilling's landed 2.55×
///   away from it.
/// * **Zero-length first segment** (`a = |p1q1|² ≤ 1e-14`): `s = 0`, and the
///   call degenerates to point-vs-segment. Correct finite answer, no refusal.
/// * **Zero-length second segment** (`e = |p2q2|² ≤ 1e-14`): `t = 0`;
///   point-vs-point when both segments are degenerate. Correct finite answer.
/// * **Non-finite input is NOT guarded**; the output is non-finite. Callers
///   that must refuse check before or after the call.
/// * **Refusal semantics stay at the caller.** This primitive returns a
///   distance for every input and never encodes one caller's refusal.
///   `validate_final_holes` keeps checking for zero-length keepout axes itself
///   and keeps raising `InvalidHoleGeometry` itself.
///
/// # DECISION-FEEDING CALLERS MUST NOT USE THIS FUNCTION
///
/// Use [`segment_segment_distance_min`], which performs the complete
/// minimisation and has no epsilon on its decision path. The one-step recovery
/// above is a one-sided OVERestimate, and an overestimated gap turns a refusal
/// into an acceptance. Measured on the §O2 counterexample
/// (`docs/specs/geometry-adjudication.md`, mirrored in `tests/segseg_min.rs`):
/// this function returns `0.09871890374623554` mm where the true minimum is
/// `0.09871809337695624` mm — an excess of `8.104e-7` mm, **8.1× the
/// `PROOF_EPSILON_MM = 1e-7` slack** of the drilling comparison that value used
/// to feed.
///
/// After §O2 and §EXT-2c this function's only remaining duties are the kernel's
/// oracle parity tests and cross-checking [`segment_segment_distance_min`].
///
/// # Constants
///
/// `SEGSEG_DENOM_EPS_MM4` and `SEGSEG_LEN2_EPS_MM2` are **chosen behavior
/// pins**, both carrying `1e-14` verbatim from `geometry_exact.py:298` for
/// parity; neither is derived. They were ONE constant compared against both an
/// mm⁴ determinant and an mm² squared length — see
/// `consts::SEGSEG_DENOM_EPS_MM4`'s documentation for the split, which is byte-neutral
/// by construction. Changing either VALUE is a parity break with no benefit.
#[inline]
#[allow(
    clippy::suspicious_operation_groupings,
    reason = "`a * e - b * b` is the segment-segment denominator |u|^2|v|^2 \
              - (u.v)^2, a determinant whose middle term is squared by \
              construction, not a mis-typed `a * b`"
)]
pub fn segment_segment_distance(p1: V3, q1: V3, p2: V3, q2: V3) -> (f64, V3, V3) {
    let d1 = sub(q1, p1);
    let d2 = sub(q2, p2);
    let r = sub(p1, p2);
    let a = dot(d1, d1);
    let e = dot(d2, d2);
    let f = dot(d2, r);
    let c = dot(d1, r);
    let b = dot(d1, d2);
    let denom = a * e - b * b;
    let mut s = if denom > SEGSEG_DENOM_EPS_MM4 {
        (b * f - c * e) / denom
    } else {
        0.0
    };
    s = s.clamp(0.0, 1.0);
    let e_safe = if e > SEGSEG_LEN2_EPS_MM2 { e } else { 1.0 };
    let mut t = (b * s + f) / e_safe;
    t = if e > SEGSEG_LEN2_EPS_MM2 { t } else { 0.0 };
    t = t.clamp(0.0, 1.0);
    let a_safe = if a > SEGSEG_LEN2_EPS_MM2 { a } else { 1.0 };
    s = if a > SEGSEG_LEN2_EPS_MM2 {
        (b * t - c) / a_safe
    } else {
        0.0
    };
    s = s.clamp(0.0, 1.0);
    let c1 = add(p1, scale(d1, s));
    let c2 = add(p2, scale(d2, t));
    let dist = crate::vec3::dist(c1, c2);
    (dist, c1, c2)
}

/// **The COMPLETE segment-vs-segment minimisation.** Canonical for every
/// decision-feeding caller (`docs/specs/geometry-adjudication.md` §O2).
///
/// Returns `(distance_mm, closest_on_p1q1, closest_on_p2q2)` — the same return
/// contract as [`segment_segment_distance`]: the distance IS `dist` of the two
/// returned points, so a caller never recomputes it.
///
/// # Why a second function rather than a fix to the first
///
/// [`segment_segment_distance`] is TERM-FOR-TERM parity-pinned (Amendment A21)
/// and consumed by this kernel's oracle parity path. Its near-parallel
/// `s = 0` branch is one coordinate-descent step, not a minimisation, and its
/// error is one-sided HIGH — it returns the distance between two points that
/// really do lie on the segments, so it can only ever OVERestimate. An
/// overestimated gap turns a refusal into an acceptance. Editing the pin was
/// ruled out; a caller-side guard would re-create private geometry in the file
/// the consolidation sweep just cleaned. So: a second primitive, and the
/// decision chains move onto it.
///
/// # The algorithm, and why it is complete
///
/// The squared distance `D(s, t) = |(p1 + s·d1) − (p2 + t·d2)|²` is a convex
/// quadratic on the unit square `[0,1]²`. A convex function on a box attains
/// its minimum either at the unconstrained critical point (when that point
/// lies in the box) or on the boundary; each of the four edge restrictions is
/// itself a 1-D convex quadratic with an exact closed-form minimiser. So five
/// candidates, each clamped into the box, cover every case:
///
/// 1. the interior critical point `((b·f − c·e)/denom, (a·f − b·c)/denom)`,
///    included only when `denom > 0.0` and both quotients are finite;
/// 2. `s = 0`, `t = clamp(f / e)`;
/// 3. `s = 1`, `t = clamp((b + f) / e)`;
/// 4. `t = 0`, `s = clamp(−c / a)`;
/// 5. `t = 1`, `s = clamp((b − c) / a)`.
///
/// **No epsilon constant appears anywhere.** The parallel case needs no guard:
/// for a parallel or near-parallel pair the minimum lies on a boundary edge,
/// where the closed forms are exact, and an ill-conditioned interior candidate
/// is merely one more evaluated point that can never make the minimum worse.
/// A clamped candidate is always a real point of the box, so every candidate
/// distance is an achievable distance and the least of them is the minimum.
///
/// Measured validation of exactly this candidate scheme (§O2, 20,000
/// deterministic near-parallel pairs at 0.005–0.05 mm scale, 19,990 of which
/// fire the pinned function's `s = 0` branch):
/// `max(candidates − converged alternating descent) = 0.0` exactly, while the
/// pinned function's excess on the same battery reached 1.754e-6 mm.
///
/// # Degenerate-input contract
///
/// * **Zero-length segments** fall out of the exact `a > 0.0` / `e > 0.0`
///   guards: a zero-length first segment pins `s = 0`, a zero-length second
///   pins `t = 0`, and both zero gives point-vs-point. Because the guards are
///   exact rather than an mm²-scaled epsilon, a genuinely nonzero segment of
///   ANY length is projected onto — the mm² mislabelling that
///   [`segment_segment_distance`] carried cannot occur here.
/// * **Non-finite input is NOT guarded**, matching the pinned function.
///   Non-finite candidates fail the `is_finite` interior test and lose the
///   `<` comparison against a finite candidate; with every input non-finite,
///   the answer is non-finite. Callers that must refuse check at their own
///   call site.
/// * **Deterministic ties.** Candidate 2 always exists and seeds the fold; the
///   others are accepted only on a strict `<`, in the fixed order 2, 1, 3, 4,
///   5. An exact tie therefore keeps the earlier candidate, and the result does
///   not depend on thread count or evaluation order.
/// * Never panics, never allocates, never returns an error.
#[inline]
#[allow(
    clippy::suspicious_operation_groupings,
    reason = "`a * e - b * b` is the segment-segment denominator |u|^2|v|^2 \
              - (u.v)^2, a determinant whose middle term is squared by \
              construction, not a mis-typed `a * b`"
)]
pub fn segment_segment_distance_min(p1: V3, q1: V3, p2: V3, q2: V3) -> (f64, V3, V3) {
    let d1 = sub(q1, p1);
    let d2 = sub(q2, p2);
    let r = sub(p1, p2);
    let a = dot(d1, d1);
    let e = dot(d2, d2);
    let f = dot(d2, r);
    let c = dot(d1, r);
    let b = dot(d1, d2);
    let denom = a * e - b * b;

    // Evaluate one candidate `(s, t)` into a real pair of points on the two
    // segments, plus their distance. Every candidate reaching here has been
    // clamped into `[0,1]^2`, so this is always an ACHIEVABLE distance.
    let at = |s: f64, t: f64| -> (f64, V3, V3) {
        let c1 = add(p1, scale(d1, s));
        let c2 = add(p2, scale(d2, t));
        (crate::vec3::dist(c1, c2), c1, c2)
    };

    // The four boundary-edge minimisers. `e > 0.0` guards the `t` forms and
    // `a > 0.0` the `s` forms; a degenerate segment collapses its parameter to
    // 0 rather than dividing. Exact guards, not epsilons.
    let t_at = |s: f64| -> f64 {
        if e > 0.0 {
            ((b * s + f) / e).clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    let s_at = |t: f64| -> f64 {
        if a > 0.0 {
            ((b * t - c) / a).clamp(0.0, 1.0)
        } else {
            0.0
        }
    };

    // Candidate 2 always exists, so it seeds the fold; every other candidate
    // must strictly beat the incumbent. A `NaN` distance therefore never
    // displaces a finite one, and an all-`NaN` input keeps a `NaN` answer whose
    // two returned points really do produce it.
    let (mut best_d, mut best_c1, mut best_c2) = at(0.0, t_at(0.0));
    let mut consider = |s: f64, t: f64| {
        let (d, c1, c2) = at(s, t);
        if d < best_d {
            best_d = d;
            best_c1 = c1;
            best_c2 = c2;
        }
    };

    // Candidate 1: the unconstrained critical point, when it exists.
    if denom > 0.0 {
        let s = (b * f - c * e) / denom;
        let t = (a * f - b * c) / denom;
        if s.is_finite() && t.is_finite() {
            consider(s.clamp(0.0, 1.0), t.clamp(0.0, 1.0));
        }
    }
    consider(1.0, t_at(1.0)); // candidate 3
    consider(s_at(0.0), 0.0); // candidate 4
    consider(s_at(1.0), 1.0); // candidate 5

    (best_d, best_c1, best_c2)
}
