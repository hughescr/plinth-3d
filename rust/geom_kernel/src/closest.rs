//! Exact point/segment vs triangle primitives, ported TERM-FOR-TERM from
//! geometry_exact.py with the pinned operand-construction sequences of
//! Amendments A6/A21. Idiomatic reassociation is a spec violation here even
//! when algebraically neutral — these outputs feed bit-parity pins.

use crate::consts::{CLOSEST_DENOM_REG, SEGSEG_EPS};
use crate::vec3::{add, cross, dot, scale, sub, V3};

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

/// Segment-vs-segment closest distance + points, geometry_exact.py:287-310.
/// P1Q1 is the (broadcast) query segment, P2Q2 the triangle edge. Returns
/// `(dist, c1_on_seg, c2_on_edge)`. The re-clamp sequence is pinned (A21).
#[inline]
pub fn segseg(p1: V3, q1: V3, p2: V3, q2: V3) -> (f64, V3, V3) {
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

// re-export cross for callers building normals if ever needed
#[allow(unused_imports)]
pub(crate) use crate::vec3::cross as _cross_reexport;
#[inline]
#[allow(dead_code)]
pub fn tri_cross(e1: V3, e2: V3) -> V3 {
    cross(e1, e2)
}
