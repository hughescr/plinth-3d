//! **The joint is sized for what the contact CAN pass** (Craig 2026-10-11:
//! "If the joint's mis-sized, let's fix THAT problem").
//!
//! A LOCAL capacity certificate, not a network-strength claim: the resized
//! section never fails before the contacts that feed it. The old head law sized
//! the joint for a load line the contact cannot pass (the lean's `c` and the
//! head-to-post angle `phi'` price a force the link and the seat weld would
//! break first). This module sizes the head's cone, the joint and the post
//! below it for the largest load the feeding fuses can transmit.
//!
//! Units: `sigma_allow * pi = 1`. A solid round section of radius `r` under
//! axial force `N`, bending moment `Mb` and twist `T` has the normal-stress
//! index `s = |N|/r^2 + 4|Mb|/r^3` and the shear index at the rim
//! `tau = 2|T|/r^3`. Photopolymer resin is brittle, so the failure criterion is
//! the maximum principal stress (Rankine, ruling R3):
//! `s/2 + sqrt(s^2/4 + tau^2) <= 1`, which is `s + tau^2 <= 1`. It is the same
//! `sigma_allow` the area law uses; no new constant enters.
//!
//! **Lemma T.** With `M` the WHOLE moment vector (bending and twist),
//! `|N|/r^2 + 4|M|/r^3 <= 1` implies Rankine (`b' = |Mb|/r^3` and
//! `t' = |T|/r^3 <= 1/4`, so `2b' + t'^2 < 1`).
//!
//! **A source** is what feeds a support: two real sections in series that
//! carry all of its load, both normal to its axis `u`: fuse A (radius `r_a`) at
//! the origin `O`, fuse B (radius `r_b`) at distance `a` along `u`. The loads
//! that break neither fuse are exactly the polytope
//! `alpha/rA^2 + 4 m0/rA^3 + 4 q/rA^6 <= 1` and
//! `alpha/rB^2 + 4 m1/rB^3 + 4 q/rB^6 <= 1` (`q` = twist squared), with the
//! transverse force `beta <= (m0 + m1)/a` and the moment along the axis
//! `|M(s)| <= |1 - s/a| m0 + |s/a| m1`. Its worst loads are two vertices, affine
//! in `q`. Sections of the source left out enlarge the set: conservative.
//!
//! **Certified maxima (R4).** On a straight frustum the demand is
//! `A/r^2 + B/r^3 + C/r^6`; its stationary points are the sign changes of
//! `2A r^4 + 3B r^3 + 6C`, which has at most two monotone pieces, so the
//! maximum is analytic ([`poly_max`]). Along a straight post record the Lemma-T
//! bound is `U(t, r) = sum_s max_p (A_sp/r^2 + B_sp(t)/r^3)` with `B_sp(t)`
//! convex in `t` (Lemma Q), so at a fixed `r` the worst point of an interval is
//! an end: a slot whose radius is at least the larger of its two end needs holds
//! everywhere on it, and the lowering's chords never draw below the slot step.

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn len(a: V3) -> f64 {
    dot(a, a).sqrt()
}

/// Pieces per vertex curve of the Lemma-T bound: on each piece every term
/// reaches its maximum at an end of the piece, so the sum of the per-term end
/// maxima is a certified bound; more pieces only tighten it.
const PIECES: usize = 64;

/// A source: fuse A (radius `r_a`) at `o`, fuse B (radius `r_b`) at distance
/// `a` along the unit `u`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Src {
    pub o: V3,
    pub u: V3,
    pub r_a: f64,
    pub r_b: f64,
    pub a: f64,
}

/// One extreme load of a source: axial force, the two end moments and the
/// squared twist.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Load {
    pub al: f64,
    pub m0: f64,
    pub m1: f64,
    pub q: f64,
}

impl Src {
    /// The largest squared twist either fuse passes (`tau = 1` at the rim).
    #[must_use]
    pub fn q_max(&self) -> f64 {
        self.r_a.min(self.r_b).powi(6) / 4.0
    }

    /// The worst load at squared twist `q`: vertex 0 has no axial force and
    /// both moments at capacity, vertex 1 the most axial force with the rest
    /// as moment. Both are affine in `q` on all of `[0, q_max]`.
    #[must_use]
    pub fn vertex_at(&self, q: f64, which: usize) -> Load {
        let b_a = (1.0 - 4.0 * q / self.r_a.powi(6)).max(0.0);
        let b_b = (1.0 - 4.0 * q / self.r_b.powi(6)).max(0.0);
        if which == 0 {
            return Load {
                al: 0.0,
                m0: b_a * self.r_a.powi(3) / 4.0,
                m1: b_b * self.r_b.powi(3) / 4.0,
                q,
            };
        }
        let al = (b_a * self.r_a.powi(2)).min(b_b * self.r_b.powi(2));
        Load {
            al,
            m0: (b_a - al / self.r_a.powi(2)).max(0.0) * self.r_a.powi(3) / 4.0,
            m1: (b_b - al / self.r_b.powi(2)).max(0.0) * self.r_b.powi(3) / 4.0,
            q,
        }
    }

    /// The worst loads for a section on the source's own axis (normal `u`, no
    /// offset): exact Rankine is linear in `(alpha, m0, m1, q)`, so the worst
    /// load is a polytope vertex, `q` at 0 or `q_max`.
    #[must_use]
    pub fn axis_loads(&self) -> Vec<Load> {
        let mut out: Vec<Load> = Vec::with_capacity(4);
        for q in [0.0, self.q_max()] {
            for which in 0..2 {
                let v = self.vertex_at(q, which);
                if !out.iter().any(|o| {
                    (o.al - v.al).abs() <= 1e-15 * (1.0 + v.al.abs())
                        && (o.m0 - v.m0).abs() <= 1e-15 * (1.0 + v.m0.abs())
                        && (o.m1 - v.m1).abs() <= 1e-15 * (1.0 + v.m1.abs())
                        && (o.q - v.q).abs() <= 1e-15 * (1.0 + v.q.abs())
                }) {
                    out.push(v);
                }
            }
        }
        out
    }
}

/// Geometry of a section (centroid `p`, unit normal `n`) seen from a source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecGeo {
    pub k0: f64,
    pub k1: f64,
    pub d: f64,
    pub cn: f64,
    pub sn: f64,
}

#[must_use]
pub fn sec_geo(s: &Src, p: V3, n: V3) -> SecGeo {
    let w = sub(p, s.o);
    let sp = dot(w, s.u);
    let d = len(sub(w, mul(s.u, sp)));
    let cn = dot(s.u, n).abs();
    SecGeo {
        k0: (1.0 - sp / s.a).abs(),
        k1: (sp / s.a).abs(),
        d,
        cn,
        sn: (1.0 - cn * cn).max(0.0).sqrt(),
    }
}

/// The bound `A/r^2 + B/r^3` of one source piece.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ab {
    pub a: f64,
    pub b: f64,
}

/// A general section (Lemma T): demand `<= A/r^2 + B/r^3` with
/// `A = alpha cn + beta sn` (`any_normal`: `sqrt(alpha^2 + beta^2)`, the worst
/// normal through the point), `B = 4 (sqrt(q + Mbar^2) + d sqrt(alpha^2 +
/// beta^2))`, `Mbar = k0 m0 + k1 m1`, `beta = (m0 + m1)/a`. Along each vertex
/// curve every term reaches its maximum over a `q`-interval at an end, so the
/// per-piece sums of end maxima are CERTIFIED upper bounds.
#[must_use]
pub fn general_pieces(s: &Src, g: &SecGeo, any_normal: bool) -> Vec<Ab> {
    let qm = s.q_max();
    let terms = |v: &Load| -> [f64; 3] {
        let be = (v.m0 + v.m1) / s.a;
        let f = v.al.hypot(be);
        [
            if any_normal { f } else { v.al * g.cn + be * g.sn },
            (v.q + (g.k0 * v.m0 + g.k1 * v.m1).powi(2)).sqrt(),
            g.d * f,
        ]
    };
    let mut out = Vec::with_capacity(2 * PIECES);
    for which in 0..2 {
        let mut prev: Option<[f64; 3]> = None;
        for i in 0..=PIECES {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a piece index of at most 64 is exact in f64"
            )]
            let t = terms(&s.vertex_at(qm * i as f64 / PIECES as f64, which));
            if let Some(p) = prev {
                out.push(Ab {
                    a: p[0].max(t[0]),
                    b: 4.0 * (p[1].max(t[1]) + p[2].max(t[2])),
                });
            }
            prev = Some(t);
        }
    }
    out
}

/// The largest `A/r^2 + B/r^3` over a set of pieces at radius `r`.
#[must_use]
pub fn eval_ab(pieces: &[Ab], r: f64) -> f64 {
    pieces
        .iter()
        .map(|p| p.a / (r * r) + p.b / (r * r * r))
        .fold(0.0, f64::max)
}

/// The smallest `r` with `f(r) <= 1` for a demand that falls as `r` grows
/// (bisection on a geometric bracket; numerical resolution only).
pub fn solve_r(f: impl Fn(f64) -> f64, lo: f64, hi: f64) -> f64 {
    let (mut lo, mut hi) = (lo, hi);
    if f(lo) <= 1.0 {
        return lo;
    }
    for _ in 0..80 {
        let m = (lo * hi).sqrt();
        if f(m) <= 1.0 {
            hi = m;
        } else {
            lo = m;
        }
    }
    hi
}

/// The analytic maximum of `D(r) = A/r^2 + B/r^3 + C/r^6` on `[rl, rh]`
/// (`r > 0`): `D' = -p(r)/r^7` with `p = 2A r^4 + 3B r^3 + 6C`, and
/// `p' = r^2 (8A r + 9B)` has one root, so `p` is monotone on at most two
/// pieces and changes sign at most once on each. A sign change from - to + is an
/// interior maximum of `D`; it is bracketed by bisection and bounded on the
/// bracket term by term (each term is monotone in `r`). The ends are exact.
/// Returns the certified maximum.
#[must_use]
pub fn poly_max(a: f64, b: f64, c: f64, rl: f64, rh: f64) -> f64 {
    let d = |r: f64| a / r.powi(2) + b / r.powi(3) + c / r.powi(6);
    let p = |r: f64| 2.0 * a * r.powi(4) + 3.0 * b * r.powi(3) + 6.0 * c;
    let mut ub = d(rl).max(d(rh));
    let mut cuts = vec![rl];
    if a != 0.0 {
        let rc = -9.0 * b / (8.0 * a);
        if rc > rl && rc < rh {
            cuts.push(rc);
        }
    }
    cuts.push(rh);
    for pair in cuts.windows(2) {
        let (mut x1, mut x2) = (pair[0], pair[1]);
        if !(p(x1) < 0.0 && p(x2) > 0.0) {
            continue;
        }
        for _ in 0..200 {
            if x2 - x1 <= 1e-15 * x2 {
                break;
            }
            let m = 0.5 * (x1 + x2);
            if p(m) < 0.0 {
                x1 = m;
            } else {
                x2 = m;
            }
        }
        let bound = (a / x1.powi(2)).max(a / x2.powi(2))
            + (b / x1.powi(3)).max(b / x2.powi(3))
            + (c / x1.powi(6)).max(c / x2.powi(6));
        ub = ub.max(bound);
    }
    ub
}

/// R4: the certified maximum over one straight frustum of the cone,
/// `(sa, ra) -> (sb, rb)` with `s` measured from the source origin on its own
/// axis and `s >= a` (beyond fuse B, so `M(s) = mu s - m0`, `mu = (m0 + m1)/a`),
/// for one load `v`: `alpha/r^2 + 4 (mu s - m0)/r^3 + 4 q/r^6`.
#[must_use]
pub fn frustum_max(s: &Src, v: &Load, sa: f64, ra: f64, sb: f64, rb: f64) -> f64 {
    let mu = (v.m0 + v.m1) / s.a;
    let dem = |sx: f64, r: f64| v.al / r.powi(2) + 4.0 * (mu * sx - v.m0) / r.powi(3) + 4.0 * v.q / r.powi(6);
    if (rb - ra).abs() < 1e-15 {
        return dem(sa, ra).max(dem(sb, rb));
    }
    let k = (rb - ra) / (sb - sa);
    let a = v.al + 4.0 * mu / k;
    let b = 4.0 * (mu * sa - mu * ra / k - v.m0);
    let c = 4.0 * v.q;
    poly_max(a, b, c, ra.min(rb), ra.max(rb))
}

/// The registered XY pixel (the shortest frustum the lowering keeps) is passed
/// in by the caller; the stations of the cone are
/// `s_i = a + (L - a) (i/n)^2`, `n <= 4`, dense next to the link where the
/// demand bends most.
#[must_use]
pub fn stations(a: f64, l: f64, pitch_mm: f64) -> Vec<f64> {
    let mut n = 4_usize;
    while n > 1 && (l - a) / ((n * n) as f64) < pitch_mm {
        n -= 1;
    }
    (0..=n)
        .map(|i| {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a station index of at most 4 is exact in f64"
            )]
            let x = i as f64 / n as f64;
            a + (l - a) * x * x
        })
        .collect()
}

/// A foreign source that enters the head at station `s_f` along the head axis
/// (R1: every contact that enters a head upstream of a section loads it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Foreign {
    pub src: Src,
    pub s_f: f64,
}

/// The head: contact centre `D`, unit axis `u`, the seat disc radius `rho0`,
/// the link radius `r_t`, the link's far end `a`, its near end `s1`, and the
/// joint distance `L = |DK|`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadSeat {
    pub d: V3,
    pub u: V3,
    pub rho0: f64,
    pub r_t: f64,
    pub link_start: f64,
    pub link_end: f64,
    pub length: f64,
}

impl HeadSeat {
    /// The head's own source: the seat disc (weld, `rho0`) at `D` and the
    /// link's far end (`r_t`) at `a`.
    #[must_use]
    pub fn own_source(&self) -> Src {
        Src {
            o: self.d,
            u: self.u,
            r_a: self.rho0,
            r_b: self.r_t,
            a: self.link_end,
        }
    }
}

/// The certified profile of one head: the cone knots `(s, r)` from `(a, r_t)`
/// to `(L, r_K)` and the joint's radius, from ONE solve (R5).
#[derive(Clone, Debug, PartialEq)]
pub struct CertifiedProfile {
    /// `(s, r)` knots from the link's far end to `K`, `s` from `D` along `u`.
    pub knots: Vec<[f64; 2]>,
    /// `r_K`: the larger of the last knot's radius and the demand at `K`.
    pub joint_radius_mm: f64,
    /// The demand at `K` alone (the any-normal section through `K`).
    pub k_demand_mm: f64,
    /// The largest demand on any frustum or at `K` (at most 1 + 1e-12).
    pub certified_max: f64,
}

/// The certified demand of foreign sources on the head axis over `[s0, s1]` at
/// the smaller radius: each lever coefficient and the offset are largest at an
/// end, and the demand rises with each.
fn foreign_span(seat: &HeadSeat, foreign: &[Foreign], s0: f64, s1: f64, r_min: f64) -> f64 {
    let at = |s: f64| add(seat.d, mul(seat.u, s));
    let mut total = 0.0;
    for f in foreign {
        if s1 <= f.s_f {
            continue;
        }
        let a0 = sec_geo(&f.src, at(s0), seat.u);
        let a1 = sec_geo(&f.src, at(s1), seat.u);
        let g = SecGeo {
            k0: a0.k0.max(a1.k0),
            k1: a0.k1.max(a1.k1),
            d: a0.d.max(a1.d),
            cn: a0.cn,
            sn: a0.sn,
        };
        total += eval_ab(&general_pieces(&f.src, &g, false), r_min);
    }
    total
}

/// The demand at `K`, as a function of the radius, for the own source and the
/// foreign ones, on the worst plane through `K`.
fn k_demand(seat: &HeadSeat, own: &Src, foreign: &[Foreign]) -> Vec<Vec<Ab>> {
    let k = add(seat.d, mul(seat.u, seat.length));
    std::iter::once(own)
        .chain(foreign.iter().map(|f| &f.src))
        .map(|s| general_pieces(s, &sec_geo(s, k, seat.u), true))
        .collect()
}

fn frustum_ok(
    seat: &HeadSeat,
    own: &Src,
    foreign: &[Foreign],
    (s0, r0): (f64, f64),
    (s1, r1): (f64, f64),
) -> f64 {
    let off = dot(sub(own.o, seat.d), seat.u);
    let mut m = 0.0_f64;
    for v in own.axis_loads() {
        m = m.max(frustum_max(own, &v, s0 - off, r0, s1 - off, r1));
    }
    m + if foreign.is_empty() {
        0.0
    } else {
        foreign_span(seat, foreign, s0, s1, r0.min(r1))
    }
}

/// The smallest `r` in `[lo, cap]` with `ok(r)`, or `None` when even `cap`
/// fails (a span whose smaller end is the previous knot can be infeasible).
fn smallest(ok: impl Fn(f64) -> bool, lo: f64, cap: f64) -> Option<f64> {
    let mut hi = lo * 1.5 + 0.02;
    while !ok(hi) {
        hi *= 1.5;
        if hi > cap {
            return None;
        }
    }
    let mut lo = lo;
    for _ in 0..60 {
        let m = 0.5 * (lo + hi);
        if ok(m) {
            hi = m;
        } else {
            lo = m;
        }
    }
    Some(hi)
}

/// **The profile (R5):** knots `[(s, r)]` from `(a, r_t)` to `(L, r_K)`, each
/// the smallest radius for which the frustum from the previous knot is certified
/// for every axis load of the own source (plus the foreign sources that have
/// entered). `r_K` is the larger of the last knot and the any-normal demand at
/// `K`. `None`: no certified profile exists, the head stays on the old law.
#[must_use]
pub fn certified_profile(
    seat: &HeadSeat,
    foreign: &[Foreign],
    pitch_mm: f64,
) -> Option<CertifiedProfile> {
    let own = seat.own_source();
    if !(seat.link_end > seat.link_start
        && seat.length > seat.link_end
        && seat.r_t > 0.0
        && seat.rho0 > 0.0
        && own.a > 0.0)
    {
        return None;
    }
    let st = stations(seat.link_end, seat.length, pitch_mm);
    let mut knots: Vec<[f64; 2]> = vec![[st[0], seat.r_t]];
    let cap = 20.0 * seat.length + 10.0;
    for &s in &st[1..] {
        let prev = *knots.last()?;
        let r = smallest(
            |x| frustum_ok(seat, &own, foreign, (prev[0], prev[1]), (s, x)) <= 1.0 + 1e-12,
            prev[1],
            cap,
        )?;
        knots.push([s, r]);
    }
    let k_pieces = k_demand(seat, &own, foreign);
    let k_at = |r: f64| k_pieces.iter().map(|p| eval_ab(p, r)).sum::<f64>();
    let k_need = solve_r(k_at, 1e-4, 50.0);
    let last = knots.last_mut()?;
    let r_k = last[1].max(k_need);
    last[1] = r_k;
    let mut certified_max = k_at(r_k);
    for pair in knots.windows(2) {
        certified_max = certified_max.max(frustum_ok(
            seat,
            &own,
            foreign,
            (pair[0][0], pair[0][1]),
            (pair[1][0], pair[1][1]),
        ));
    }
    Some(CertifiedProfile {
        knots,
        joint_radius_mm: r_k,
        k_demand_mm: k_need,
        certified_max,
    })
}

/// One source's section pieces at a post point, for [`u_sum`].
#[must_use]
pub fn pieces_at(s: &Src, p: V3, n: V3) -> Vec<Ab> {
    general_pieces(s, &sec_geo(s, p, n), false)
}

/// `U(r)`: the sum over sources of the worst piece at radius `r`.
#[must_use]
pub fn u_sum(sources: &[Vec<Ab>], r: f64) -> f64 {
    sources.iter().map(|p| eval_ab(p, r)).sum()
}

/// The smallest radius at which the sources' summed demand is at most 1, never
/// below `floor`.
#[must_use]
pub fn r_need(sources: &[Vec<Ab>], floor: f64) -> f64 {
    if sources.is_empty() {
        return floor;
    }
    floor.max(solve_r(|r| u_sum(sources, r), 1e-4, 50.0))
}

/// The certified maximum of `U` along a straight frustum of a post:
/// `P(t) = p0 + t (p1 - p0)`, `r(t) = r0 + t (r1 - r0)`, `t` in `[ta, tb]`,
/// by branch and bound with the interval bound (each `B` at its larger end, `r`
/// at its smaller end). `ub` is the certified maximum.
#[must_use]
pub fn span_max(
    sources: &[Src],
    p0: V3,
    p1: V3,
    n: V3,
    (r0, r1): (f64, f64),
    (ta, tb): (f64, f64),
) -> f64 {
    let point = |t: f64| add(p0, mul(sub(p1, p0), t));
    let radius = |t: f64| r0 + (r1 - r0) * t;
    let pieces_of = |t: f64| -> Vec<Vec<Ab>> { sources.iter().map(|s| pieces_at(s, point(t), n)).collect() };
    let value = |t: f64, p: &[Vec<Ab>]| u_sum(p, radius(t));
    let interval = |pa: &[Vec<Ab>], pb: &[Vec<Ab>], rmin: f64| -> f64 {
        let mut total = 0.0;
        for (a, b) in pa.iter().zip(pb) {
            let mut m = 0.0_f64;
            for (x, y) in a.iter().zip(b) {
                m = m.max(x.a / (rmin * rmin) + x.b.max(y.b) / (rmin * rmin * rmin));
            }
            total += m;
        }
        total
    };
    struct Iv {
        a: f64,
        b: f64,
        pa: Vec<Vec<Ab>>,
        pb: Vec<Vec<Ab>>,
        ub: f64,
    }
    let pa = pieces_of(ta);
    let pb = pieces_of(tb);
    let mut lb = value(ta, &pa).max(value(tb, &pb));
    let ub0 = interval(&pa, &pb, radius(ta).min(radius(tb)));
    let mut heap = vec![Iv { a: ta, b: tb, pa, pb, ub: ub0 }];
    for _ in 0..1500 {
        let Some(bi) = heap
            .iter()
            .enumerate()
            .max_by(|x, y| x.1.ub.total_cmp(&y.1.ub))
            .map(|(i, _)| i)
        else {
            break;
        };
        if heap[bi].ub - lb <= 1e-10 * lb.max(1.0) || heap[bi].b - heap[bi].a < 1e-12 {
            break;
        }
        let j = heap.swap_remove(bi);
        let m = 0.5 * (j.a + j.b);
        let pm = pieces_of(m);
        lb = lb.max(value(m, &pm));
        let ub_l = interval(&j.pa, &pm, radius(j.a).min(radius(m)));
        let ub_r = interval(&pm, &j.pb, radius(m).min(radius(j.b)));
        heap.push(Iv { a: j.a, b: m, pa: j.pa, pb: pm.clone(), ub: ub_l });
        heap.push(Iv { a: m, b: j.b, pa: pm, pb: j.pb, ub: ub_r });
    }
    heap.iter().fold(lb, |acc, j| acc.max(j.ub))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Support A on Deinonychus (the Q12 file, tip 650): `r_t` 0.428,
    /// `rho0` 0.404, link end `a` 1.081, `|DK|` 1.668 (measured from the emitted
    /// file, JOINT-DESIGN.md "Support A"). The design's cone knots are
    /// 0.437 / 0.462 / 0.498 / 0.560 at s 1.117 / 1.227 / 1.411 / 1.668.
    fn support_a() -> HeadSeat {
        HeadSeat {
            d: [3.93, -5.89, 9.43],
            u: [0.0, 0.0, -1.0],
            rho0: 0.404,
            r_t: 0.428,
            link_start: 0.78,
            link_end: 1.081,
            length: 1.668,
        }
    }

    #[test]
    fn support_a_joint_is_what_the_link_can_pass() {
        let p = certified_profile(&support_a(), &[], 0.019).expect("a certified profile");
        assert_eq!(p.knots.len(), 5, "n = 4 frusta");
        let want = [(1.081, 0.428), (1.117, 0.437), (1.227, 0.462), (1.411, 0.498), (1.668, 0.560)];
        for (knot, (s, r)) in p.knots.iter().zip(want) {
            assert!((knot[0] - s).abs() < 2e-3, "station {} vs {s}", knot[0]);
            assert!((knot[1] - r).abs() < 2e-3, "radius {} at {s} vs {r}", knot[1]);
        }
        assert!((p.joint_radius_mm - 0.560).abs() < 2e-3, "joint {}", p.joint_radius_mm);
        assert!(p.certified_max <= 1.0 + 1e-9, "certified maximum {}", p.certified_max);
        // The old law drew 1.283 here; the certified joint is well under half.
        assert!(p.joint_radius_mm < 0.5 * 1.283);
    }

    #[test]
    fn every_frustum_of_a_profile_is_certified_and_one_straight_cone_is_not() {
        let seat = support_a();
        let own = seat.own_source();
        let p = certified_profile(&seat, &[], 0.019).expect("profile");
        for pair in p.knots.windows(2) {
            let m: f64 = own
                .axis_loads()
                .iter()
                .map(|v| frustum_max(&own, v, pair[0][0], pair[0][1], pair[1][0], pair[1][1]))
                .fold(0.0, f64::max);
            assert!(m <= 1.0 + 1e-9, "frustum {pair:?} reaches {m}");
        }
        // Sabotage: one straight cone to the joint's radius leaves the
        // certified region next to the link (the profile is a polyline for a reason).
        let straight = own
            .axis_loads()
            .iter()
            .map(|v| frustum_max(&own, v, 1.081, 0.428, 1.668, p.joint_radius_mm))
            .fold(0.0, f64::max);
        assert!(straight > 1.0 + 1e-6, "a straight cone reaches {straight}, which must exceed 1");
        // Sabotage: a 1 % thinner knot fails.
        let mut thin = p.knots.clone();
        thin[2][1] *= 0.99;
        let m: f64 = (1..thin.len())
            .map(|i| {
                own.axis_loads()
                    .iter()
                    .map(|v| frustum_max(&own, v, thin[i - 1][0], thin[i - 1][1], thin[i][0], thin[i][1]))
                    .fold(0.0, f64::max)
            })
            .fold(0.0, f64::max);
        assert!(m > 1.0 + 1e-6, "a 1 % thinner knot reaches {m}");
    }

    #[test]
    fn a_foreign_source_at_the_joint_grows_the_joint_and_nothing_before_it() {
        let seat = support_a();
        let alone = certified_profile(&seat, &[], 0.019).expect("alone");
        let k = add(seat.d, mul(seat.u, seat.length));
        // A load entering at the joint loads the joint's section and nothing
        // upstream of it.
        let friend = Foreign {
            src: Src { o: add(k, [0.4, 0.0, 0.0]), u: [0.0, 0.0, -1.0], r_a: 0.08, r_b: 0.06, a: 0.6 },
            s_f: seat.length,
        };
        let shared = certified_profile(&seat, &[friend], 0.019).expect("shared");
        assert!(shared.joint_radius_mm > alone.joint_radius_mm, "the joint grows");
        let n = alone.knots.len();
        for (a, s) in alone.knots[..n - 1].iter().zip(&shared.knots) {
            assert!((s[1] - a[1]).abs() < 1e-12, "upstream knots are unchanged");
        }
    }

    /// A load that enters partway along the cone cannot be carried by the
    /// tight knots upstream of it: R1 leaves such a head on the old law.
    #[test]
    fn a_load_entering_mid_cone_leaves_no_certified_profile() {
        let seat = support_a();
        let k = add(seat.d, mul(seat.u, 1.3));
        let friend = Foreign {
            src: Src { o: add(k, [0.4, 0.0, 0.0]), u: [0.0, 0.0, -1.0], r_a: 0.08, r_b: 0.06, a: 0.6 },
            s_f: 1.3,
        };
        assert!(certified_profile(&seat, &[friend], 0.019).is_none());
    }

    #[test]
    fn polynomial_maximum_agrees_with_dense_sampling() {
        // A/r^2 + B/r^3 + C/r^6 with an interior maximum.
        let (a, b, c) = (0.1, -0.05, 0.002);
        let (rl, rh) = (0.2, 1.0);
        let analytic = poly_max(a, b, c, rl, rh);
        let sampled = (0..=100_000)
            .map(|i| {
                let r = rl + (rh - rl) * f64::from(i) / 100_000.0;
                a / r.powi(2) + b / r.powi(3) + c / r.powi(6)
            })
            .fold(f64::MIN, f64::max);
        assert!(analytic >= sampled - 1e-12, "{analytic} < {sampled}");
        assert!(analytic <= sampled + 1e-6, "{analytic} way above {sampled}");
    }

    #[test]
    fn post_need_rises_with_the_lever_and_a_certified_span_holds() {
        let seat = support_a();
        let own = seat.own_source();
        // A post leaving K at 56.6 degrees from vertical, 3.48 mm long.
        let k = add(seat.d, mul(seat.u, seat.length));
        let n = {
            let a = 56.6_f64.to_radians();
            [a.sin(), 0.0, -a.cos()]
        };
        let near = r_need(&[pieces_at(&own, add(k, mul(n, 0.043)), n)], 0.0);
        let end = add(k, mul(n, 3.48));
        let far = r_need(&[pieces_at(&own, end, n)], 0.0);
        assert!(far > near, "the demand grows down the post ({near} -> {far})");
        let m = span_max(&[own], k, end, n, (far, far), (0.0, 1.0));
        assert!(m <= 1.0 + 1e-6, "a post at its largest need holds everywhere ({m})");
    }
}
