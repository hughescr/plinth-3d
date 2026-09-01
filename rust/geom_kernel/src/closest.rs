//! Exact point/segment vs triangle primitives, ported TERM-FOR-TERM from
//! geometry_exact.py with the pinned operand-construction sequences of
//! Amendments A6/A21. Idiomatic reassociation is a spec violation here even
//! when algebraically neutral — these outputs feed bit-parity pins.

use crate::consts::{CLOSEST_DENOM_REG, SEGSEG_EPS};
use crate::vec3::{add, dot, scale, sub, V3};

/// Closest point on triangle (Ericson), geometry_exact.py:81-107.
///
/// `a`, `b`, `cc` are the RECONSTRUCTED vertices the oracle feeds in
/// (`A=V0`, `B=V0+E1`, `Cc=V0+E2`); this function recomputes `ab = b - a`,
/// `ac = cc - a` internally exactly as the oracle does — do NOT pass raw
/// E1/E2 (Amendment A6).
#[inline]
pub fn closest_on_tri(p: V3, a: V3, b: V3, cc: V3) -> V3 {
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
    let denom = 1.0 / (va + vb + vc + CLOSEST_DENOM_REG);
    let v = vb * denom;
    let w = vc * denom;
    // base out = A + ab*v + ac*w  (association ((A + ab*v) + ac*w))
    let base = add(add(a, scale(ab, v)), scale(ac, w));

    // edge parameters with the oracle's `where((den)!=0, num/den, 0.0)` guard
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
    let c_bc = add(b, scale(sub(cc, b), t_bc)); // B + (Cc - B)*tBC

    let m_bc = (va <= 0.0) && ((d4 - d3) >= 0.0) && ((d5 - d6) >= 0.0);
    let m_ac = (vb <= 0.0) && (d2 >= 0.0) && (d6 <= 0.0);
    let m_ab = (vc <= 0.0) && (d1 >= 0.0) && (d3 <= 0.0);
    let m_c = (d6 >= 0.0) && (d5 <= d6);
    let m_b = (d3 >= 0.0) && (d4 <= d3);
    let m_a = (d1 <= 0.0) && (d2 <= 0.0);

    // The oracle applies np.where in order base<-cBC<-cAC<-cAB<-C<-B<-A, so the
    // LAST-applied mask (mA) wins: reversed priority mA > mB > mC > mAB > mAC >
    // mBC > base.
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
/// `SEGSEG_EPS = 1e-14` is oracle-pinned; changing it is a parity break with no
/// benefit.
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
    let mut s = if denom > SEGSEG_EPS {
        (b * f - c * e) / denom
    } else {
        0.0
    };
    s = s.clamp(0.0, 1.0);
    let e_safe = if e > SEGSEG_EPS { e } else { 1.0 };
    let mut t = (b * s + f) / e_safe;
    t = if e > SEGSEG_EPS { t } else { 0.0 };
    t = t.clamp(0.0, 1.0);
    let a_safe = if a > SEGSEG_EPS { a } else { 1.0 };
    s = if a > SEGSEG_EPS {
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
