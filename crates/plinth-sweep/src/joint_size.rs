//! **The certified sizing of a head and its post** (Craig 2026-10-11): the pass
//! that puts [`crate::joint_cert`]'s bound into the strut set. It runs inside
//! [`crate::down_sweep::resize_heads_to_drawn_posts`], after the old law has
//! sized every head, and replaces the old law's cone, joint and post radii on
//! every head it can certify. A head it cannot certify keeps the old law and is
//! counted (emit and warn, never refuse).
//!
//! * **One authoritative profile (R5).** [`crate::joint_cert::certified_profile`]
//!   gives the cone knots and the joint radius together; they are stored on the
//!   head ([`crate::heads::Head::certified`]) and the adapter and the lowering
//!   read them, not the old law.
//! * **Which heads.** A head is certified when nothing else loads its link, cone
//!   or joint: no member lands on it above the joint, and no other member's drawn
//!   solid touches the certified head solids. A head so entered is "shared" and,
//!   when the entry topology is not fully known here, stays on the old law
//!   (R1: leave it on the old law and count it).
//! * **The post (R6).** Each post slot (one ledger layer) takes the larger of its
//!   two END needs (Lemma Q: at a fixed radius the demand is convex along a
//!   straight record, so an interval's worst point is an end), summed over every
//!   source that loads it from its entry down: the head's own contact, every
//!   member landed on it with what is routed through that member, every brace,
//!   and every member whose drawn solid is fused into it. Top down the radius is
//!   the running maximum (never thinner below). A slot whose need is above
//!   today's radius, or that holds a junction hull's ring (R8), keeps today's
//!   radius. The lowering's chords never draw below the slot step, so every
//!   drawn section is certified.
//! * **Hulls keep today's geometry (R8).** A slot the junction pass cuts into, a
//!   branch end ring's window, and a buried junction's host where a thinner host
//!   would let the branch ring out, all keep today's radius.
//!
//! Batch-first: one `tip_contact_discs` call for the field, one drawn-profile
//! pass, one geometry table; per post one pass over its slots.

use std::collections::HashMap;

use geom_kernel::segment_segment_distance_min;
use plinth_core::LayerPlan;
use plinth_support::{drawn_root_radius_mm, drawn_spans, tip_contact_discs, DrawnSpan, TipContactDisc};

use crate::down_sweep::{DrawnArm, SizingPolicy};
use crate::heads::{CertifiedHead, OldLawReason};
use crate::joint_cert::{self as jc, Ab, HeadSeat, Src};
use crate::{Landing, StrutKind, SweepStrut};

type V3 = [f64; 3];

/// A micron: the degenerate-frustum guard the lowering uses.
const ZERO_LENGTH_MM: f64 = 1.0e-6;
/// The sphere's polygon vertex over its radius (the drawn joint is a polyhedron
/// inscribed in the sphere; used for overlap tests only, always toward more
/// overlap).
const SPHERE_VERTEX_OVER_RADIUS: f64 = 1.04;

/// What the pass reads besides the strut set.
#[derive(Clone, Copy)]
pub(crate) struct JointContext<'a> {
    pub layers: &'a LayerPlan,
    pub taper: crate::adapter::LoadStepTaper,
    /// The registered slenderness ratio `S` (0 when the lane is off).
    pub slenderness_ratio: f64,
}

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
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: V3) -> Option<V3> {
    let n = norm(a);
    (n > ZERO_LENGTH_MM && n.is_finite()).then(|| mul(a, 1.0 / n))
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn lerp(a: V3, b: V3, t: f64) -> V3 {
    add(a, mul(sub(b, a), t))
}

fn registered_emit_sides() -> usize {
    static SIDES: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *SIDES.get_or_init(|| {
        plinth_core::m0_constants().map_or(24, |constants| {
            usize::try_from(constants.placeholders().chitubox_emit_n_sides().diagnostic().get())
                .ok()
                .filter(|sides| *sides >= 3)
                .unwrap_or(24)
        })
    })
}

/// A conservative solid: a segment and the widest radius along it.
#[derive(Clone, Copy, Debug)]
struct Frust {
    a: V3,
    b: V3,
    r: f64,
}

impl Frust {
    fn touches(&self, other: &Frust) -> bool {
        let (d, _, _) = segment_segment_distance_min(self.a, self.b, other.a, other.b);
        d < self.r + other.r
    }
    fn bounds(&self) -> (V3, V3) {
        (
            std::array::from_fn(|k| self.a[k].min(self.b[k]) - self.r),
            std::array::from_fn(|k| self.a[k].max(self.b[k]) + self.r),
        )
    }
}

fn boxes_apart(a: &(V3, V3), b: &(V3, V3)) -> bool {
    (0..3).any(|k| a.0[k] > b.1[k] || b.0[k] > a.1[k])
}

/// Everything the pass knows about one live member, as the lowering draws it.
struct Geo {
    arm: plinth_support::SweepArm,
    /// The drawn shaft: top (ball centre, joint `K`, or a brace's seat) and
    /// bottom (the boss top of a plate member, else the landing).
    top: V3,
    bottom: V3,
    /// The ledger the lowering gets (junction raises included), post slots
    /// only for a headed member.
    profile: Vec<f64>,
    /// The disc the lowering seats the tip with (`None`: no tip ball).
    disc: Option<TipContactDisc>,
    /// The head, for a headed member whose seat is known.
    seat: Option<HeadSeat>,
    /// Today's solids (the old-law head and the drawn shaft), for overlap tests.
    frusta: Vec<Frust>,
    bounds: (V3, V3),
    /// A straight tip's taper: the first drawn span's length along the axis and
    /// the radius at its far end.
    taper: Option<(f64, f64)>,
}

/// The drawn shaft of a member as conservative solids, and the radius at the end
/// of its first drawn span (a straight tip's taper).
#[expect(
    clippy::cast_precision_loss,
    reason = "a profile length bounded by the plan's layer count is exact in f64"
)]
fn shaft_frusta(g: &Geo, h: f64, tip_radius: f64, head_joint: Option<f64>) -> (Vec<Frust>, Option<f64>) {
    let (top, bottom) = (g.top, g.bottom);
    if top[2].partial_cmp(&(bottom[2] + ZERO_LENGTH_MM)) != Some(std::cmp::Ordering::Greater) {
        return (Vec::new(), None);
    }
    let base_z = g.arm.base_mm[2];
    let profile_top_z = if head_joint.is_some() {
        (base_z + g.profile.len() as f64 * h).max(top[2])
    } else {
        g.arm.seat_mm[2]
    };
    let tip = head_joint.unwrap_or(tip_radius);
    let root = drawn_root_radius_mm(&g.profile, tip_radius);
    let spans = drawn_spans(&g.profile, base_z, profile_top_z, top[2], bottom[2], tip, root);
    let at = |z: f64| lerp(top, bottom, (top[2] - z) / (top[2] - bottom[2]));
    let first_end = spans.first().map(|s| s.bottom_radius_mm);
    let frusta = spans
        .iter()
        .map(|s: &DrawnSpan| Frust {
            a: at(s.top_z_mm),
            b: at(s.bottom_z_mm),
            r: s.top_radius_mm.max(s.bottom_radius_mm),
        })
        .collect();
    (frusta, first_end)
}

/// The head of `strut` as the lowering derives it from the disc: the seat disc,
/// the link's near and far ends and `|DK|`.
pub(crate) fn head_seat_of(strut: &SweepStrut, disc: &TipContactDisc) -> Option<HeadSeat> {
    let head = strut.head.as_ref().filter(|head| !head.drawn_straight)?;
    if head.link_mm <= 0.0 {
        return None;
    }
    let u = unit(head.unit)?;
    let d = head.contact_mm;
    let length = norm(sub(head.joint_mm, d));
    let r_t = strut.tip_radius_mm();
    let ball = disc.ball_radius_mm;
    if !(r_t > 0.0 && ball > 0.0 && length.is_finite() && length > 0.0) {
        return None;
    }
    let centre_s = dot(u, sub(disc.ball_centre_mm, d));
    let height = dot(u, sub(disc.ball_centre_mm, disc.centre_mm));
    let s_start = if height < 0.0 { 0.0 } else { centre_s.max(0.0) };
    let exit = (ball * ball - r_t * r_t).max(0.0).sqrt();
    let link_start = centre_s + exit;
    let link_end = (centre_s + exit + head.link_mm)
        .max(s_start + ZERO_LENGTH_MM)
        .min(length - ZERO_LENGTH_MM);
    Some(HeadSeat {
        d,
        u,
        rho0: disc.radius_mm,
        r_t,
        link_start,
        link_end,
        length,
    })
}

/// The head's solids (link, cone, joint) from knots `(s, r)` along the axis.
fn head_frusta(seat: &HeadSeat, knots: &[[f64; 2]], joint_radius: f64, ball: Option<(V3, f64)>) -> Vec<Frust> {
    let at = |s: f64| add(seat.d, mul(seat.u, s));
    let mut out = Vec::with_capacity(knots.len() + 3);
    if let Some((c, r)) = ball {
        out.push(Frust { a: c, b: c, r });
    }
    out.push(Frust {
        a: at(seat.link_start),
        b: at(seat.link_end),
        r: seat.r_t,
    });
    for pair in knots.windows(2) {
        out.push(Frust {
            a: at(pair[0][0]),
            b: at(pair[1][0]),
            r: pair[0][1].max(pair[1][1]),
        });
    }
    let k = at(seat.length);
    out.push(Frust {
        a: k,
        b: k,
        r: joint_radius * SPHERE_VERTEX_OVER_RADIUS,
    });
    out
}

/// A source id: the member's own contact (`2 i`) or a brace's section (`2 i + 1`).
type SourceId = usize;

#[derive(Clone, Copy, Debug)]
struct Tagged {
    id: SourceId,
    src: Src,
}

/// One source entering a post at parameter `t` (0 at the top, 1 at the bottom).
#[derive(Clone, Copy, Debug)]
struct Entry {
    tagged: Tagged,
    t: f64,
}

struct Field<'a> {
    struts: &'a [SweepStrut],
    geo: Vec<Option<Geo>>,
    id_index: HashMap<u32, usize>,
    landers: Vec<Vec<usize>>,
    seated: Vec<Vec<usize>>,
    h: f64,
}

impl Field<'_> {
    /// The member's own contact as a source.
    fn own_source(&self, i: usize) -> Option<Tagged> {
        let g = self.geo[i].as_ref()?;
        let strut = &self.struts[i];
        if let Some(seat) = &g.seat {
            return Some(Tagged { id: 2 * i, src: seat.own_source() });
        }
        if strut.kind != StrutKind::Arm {
            return None;
        }
        // A straight tip: the ball's equator at its centre, and the taper's far
        // end (the first drawn span).
        let disc = g.disc.as_ref()?;
        let axis = unit(sub(g.bottom, g.top))?;
        let (a, far_radius) = g.taper?;
        Some(Tagged {
            id: 2 * i,
            src: Src {
                o: disc.ball_centre_mm,
                u: axis,
                r_a: disc.ball_radius_mm,
                r_b: far_radius,
                a: a.max(ZERO_LENGTH_MM),
            },
        })
    }

    /// A brace, mast or tie attached to a post at `near`: its own section bounds
    /// what it passes.
    fn record_source(&self, i: usize, near: V3) -> Option<Tagged> {
        let g = self.geo[i].as_ref()?;
        let (p, q) = (g.top, g.bottom);
        let length = norm(sub(q, p));
        if length <= ZERO_LENGTH_MM {
            return None;
        }
        let (o, far) = if norm(sub(p, near)) <= norm(sub(q, near)) { (p, q) } else { (q, p) };
        let r = self.struts[i]
            .radius_mm
            .iter()
            .copied()
            .fold(self.struts[i].min_radius_mm, f64::max)
            .max(g.profile.iter().copied().fold(0.0, f64::max));
        Some(Tagged {
            id: 2 * i + 1,
            src: Src { o, u: unit(sub(far, o))?, r_a: r, r_b: r, a: length },
        })
    }

    /// Everything routed through member `c`: its own contact and what its
    /// landers and seated braces bring. `depth` guards a cycle in a corrupt set.
    fn routed(&self, c: usize, depth: usize, out: &mut Vec<Tagged>) {
        if depth > 48 {
            return;
        }
        if let Some(t) = self.own_source(c) {
            out.push(t);
        }
        for &l in self.landers[c].iter().chain(&self.seated[c]) {
            self.attached(l, c, depth + 1, out);
        }
    }

    /// A member attached to `host`: an arm brings all that is routed through it;
    /// a brace, mast or tie only its own section.
    fn attached(&self, l: usize, host: usize, depth: usize, out: &mut Vec<Tagged>) {
        if self.struts[l].kind == StrutKind::Arm {
            self.routed(l, depth, out);
        } else if let Some(gl) = self.geo[l].as_ref() {
            // The end that meets the host: a seated brace's top, a landed one's base.
            let near = if self.struts[l].seat_on_strut == Some(self.struts[host].id) { gl.top } else { gl.bottom };
            if let Some(t) = self.record_source(l, near) {
                out.push(t);
            }
        }
    }
}

/// How the pass fared, for the report.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Tally {
    pub certified: usize,
    pub slots_certified: usize,
    pub slots_kept: usize,
    pub slots_pinned: usize,
    pub worst_drawn_max: f64,
    pub over_one: usize,
}

/// The pull-backs of a junction (`t_i = r_max tan(d_i / 2)`, `d_i` the
/// member's largest deflection from straight-on against a member on the far
/// side): lower/junction.rs::pull_backs, which this reads to keep the hulls.
fn pull_backs(units: &[V3], r_max: f64) -> Vec<f64> {
    units
        .iter()
        .enumerate()
        .map(|(i, &u)| {
            let deflection = units
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, &w)| dot(u, w))
                .filter(|&c| c < 0.0)
                .map(|c| (-c).clamp(0.0, 1.0).acos())
                .fold(0.0_f64, f64::max);
            r_max * (0.5 * deflection).tan()
        })
        .collect()
}

/// A z-interval of a member's shaft that holds a hull ring: slots touching it
/// keep today's radius (R8).
#[derive(Clone, Copy, Debug)]
struct Pin {
    z_lo: f64,
    z_hi: f64,
}

/// A buried landed end: its branch ring corners must stay inside the host.
struct Buried {
    corners: Vec<V3>,
}

struct Plan {
    pins: Vec<Vec<Pin>>,
    buried: Vec<Vec<Buried>>,
}

/// Index of the slot holding height `z` of a member whose profile starts at
/// `base_z` with `slots` layers of `h`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a floored non-negative slot index clamped to the profile length"
)]
fn slot_of(z: f64, base_z: f64, h: f64, slots: usize) -> usize {
    let index = ((z - base_z) / h).floor();
    if index <= 0.0 {
        0
    } else {
        (index as usize).min(slots.saturating_sub(1))
    }
}

/// The radius of a member's chain at height `z`, read as the step profile.
fn step_radius_at(profile: &[f64], base_z: f64, h: f64, z: f64) -> f64 {
    profile.get(slot_of(z, base_z, h, profile.len())).copied().unwrap_or(0.0)
}

fn plan_hulls(field: &Field<'_>, ratio_cos_flat: f64, pitch_mm: f64) -> Plan {
    let n = field.struts.len();
    let h = field.h;
    let mut pins: Vec<Vec<Pin>> = vec![Vec::new(); n];
    let mut buried: Vec<Vec<Buried>> = (0..n).map(|_| Vec::new()).collect();
    let sides = registered_emit_sides();
    for (j, strut) in field.struts.iter().enumerate() {
        let Landing::Strut { id, .. } = strut.landing else { continue };
        let Some(&host) = field.id_index.get(&id) else { continue };
        if host == j {
            continue;
        }
        let (Some(gj), Some(gh)) = (field.geo[j].as_ref(), field.geo[host].as_ref()) else { continue };
        let k = gj.bottom;
        let Some(u_b) = unit(sub(gj.top, k)) else { continue };
        let Some(dir_h) = unit(sub(gh.bottom, gh.top)) else { continue };
        let len_b = norm(sub(gj.top, gj.bottom));
        let host_len = norm(sub(gh.bottom, gh.top));
        let s_k = dot(sub(k, gh.top), dir_h).clamp(0.0, host_len);
        // The radii that meet: the branch's end, the host around `K` (a collar
        // widens downward above it, so look a few layers either side).
        let r_b = gj.profile.first().copied().unwrap_or(0.0).max(strut.radius_mm.first().copied().unwrap_or(0.0));
        let host_base = gh.arm.base_mm[2];
        let host_slots = gh.profile.len().max(1);
        let around = slot_of(k[2], host_base, h, host_slots);
        let r_host = (around.saturating_sub(2)..=(around + 2).min(host_slots - 1))
            .filter_map(|slot| gh.profile.get(slot).copied())
            .fold(0.0, f64::max);
        let r_max = r_b.max(r_host);
        let has_above = s_k > pitch_mm;
        let has_below = host_len - s_k > pitch_mm;
        let mut units = vec![u_b];
        let mut slot_below = None;
        let mut slot_above = None;
        if has_below {
            slot_below = Some(units.len());
            units.push(dir_h);
        }
        if has_above {
            slot_above = Some(units.len());
            units.push(mul(dir_h, -1.0));
        }
        let pulls = pull_backs(&units, r_max);
        let t_b = pulls[0].min(0.5 * len_b);
        let t_a = slot_above.map_or(0.0, |i| pulls[i].min(0.5 * s_k));
        let t_l = slot_below.map_or(0.0, |i| pulls[i].min(0.5 * (host_len - s_k)));
        // Host window: from above the collar (a pull-back, the widest collar a
        // radius allows, two layers) down to the pull-back below `K`.
        let above = t_a + r_max + 2.0 * h;
        let below = t_l + 2.0 * h;
        let z_a = at_s(gh, (s_k - above).max(0.0));
        let z_b = at_s(gh, (s_k + below).min(host_len));
        pins[host].push(Pin { z_lo: z_a.min(z_b), z_hi: z_a.max(z_b) });
        // Branch window: its end up to the pull-back (and at least two layers).
        let cos_up = ((gj.top[2] - gj.bottom[2]) / len_b.max(ZERO_LENGTH_MM)).clamp(0.0, 1.0);
        pins[j].push(Pin {
            z_lo: gj.bottom[2],
            z_hi: gj.bottom[2] + (t_b + 2.0 * h) * cos_up.clamp(1.0e-3, 1.0) + h,
        });
        // Is the junction BURIED today (every corner of the end ring inside the
        // host's step solid)? Then a thinner host must keep it buried.
        let r_end = r_b;
        let Some(axis) = unit(sub(gj.top, gj.bottom)) else { continue };
        let probe = if axis[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let Some(e1) = unit(cross(axis, probe)) else { continue };
        let e2 = cross(axis, e1);
        let corners: Vec<V3> = (0..sides)
            .map(|c| {
                #[expect(clippy::cast_precision_loss, reason = "a corner index of a small polygon")]
                let angle = std::f64::consts::TAU * c as f64 / sides as f64;
                add(gj.bottom, add(mul(e1, r_end * angle.cos()), mul(e2, r_end * angle.sin())))
            })
            .collect();
        let inside_today = |p: V3| inside_host(gh, p, h, ratio_cos_flat);
        if corners.iter().all(|&p| inside_today(p)) {
            buried[host].push(Buried { corners });
        }
    }
    Plan { pins, buried }
}

/// The point at arc distance `s` from a member's top, as a height.
fn at_s(g: &Geo, s: f64) -> f64 {
    let len = norm(sub(g.bottom, g.top));
    if len <= ZERO_LENGTH_MM {
        return g.top[2];
    }
    g.top[2] + (g.bottom[2] - g.top[2]) * (s / len)
}

/// Whether `p` lies inside the member's shaft solid read as its step profile
/// (the drawn chords stand on or above it).
fn inside_host(g: &Geo, p: V3, h: f64, flat: f64) -> bool {
    inside_by_profile(g, &g.profile, p, h, flat)
}

fn inside_by_profile(g: &Geo, profile: &[f64], p: V3, h: f64, flat: f64) -> bool {
    let axis = sub(g.bottom, g.top);
    let l2 = dot(axis, axis);
    if l2 <= ZERO_LENGTH_MM {
        return false;
    }
    let t = dot(sub(p, g.top), axis) / l2;
    if !(0.0..=1.0).contains(&t) {
        return false;
    }
    let foot = add(g.top, mul(axis, t));
    let r = step_radius_at(profile, g.arm.base_mm[2], h, foot[2]);
    norm(sub(p, foot)) <= r * flat * (1.0 - 1.0e-6)
}

/// One post's slot geometry: heights and parameters `t` (0 at the top).
struct SlotGeo {
    slot: usize,
    t_hi: f64,
    t_lo: f64,
}

fn slots_of(g: &Geo, h: f64, joint_slot: usize) -> Vec<SlotGeo> {
    let (top, bottom) = (g.top, g.bottom);
    let dz = top[2] - bottom[2];
    if dz.partial_cmp(&ZERO_LENGTH_MM) != Some(std::cmp::Ordering::Greater) {
        return Vec::new();
    }
    let base_z = g.arm.base_mm[2];
    let mut out = Vec::new();
    for slot in 0..=joint_slot.min(g.profile.len().saturating_sub(1)) {
        #[expect(clippy::cast_precision_loss, reason = "a slot index bounded by the profile length")]
        let (zb, zt) = (base_z + slot as f64 * h, base_z + (slot + 1) as f64 * h);
        let zhi = zt.min(top[2]);
        let zlo = zb.max(bottom[2]);
        if zhi - zlo < 1.0e-9 {
            continue;
        }
        out.push(SlotGeo { slot, t_hi: (top[2] - zhi) / dz, t_lo: (top[2] - zlo) / dz });
    }
    out
}

/// The certified pass. `arms[i]` is the member as the lowering gets it (`None`
/// for a dead member). The head and post radii of every certifiable head are
/// rewritten; the rest keep the old law. Returns the tally.
#[expect(clippy::too_many_lines, reason = "one pass: geometry, topology, profile, post slots, check")]
pub(crate) fn certify(
    struts: &mut [SweepStrut],
    sizing: SizingPolicy,
    ctx: &JointContext<'_>,
    arms: Vec<Option<DrawnArm>>,
    pitch_mm: f64,
) -> Tally {
    let n = struts.len();
    let h = sizing.layer_height_mm;
    for strut in struts.iter_mut() {
        if let Some(head) = strut.head.as_mut() {
            head.certified = None;
            head.old_law = None;
        }
    }
    let mut tally = Tally::default();
    // One batched disc call for the field.
    let order: Vec<usize> = (0..n).filter(|&i| arms[i].is_some()).collect();
    let arm_list: Vec<plinth_support::SweepArm> =
        order.iter().filter_map(|&i| arms[i].as_ref().map(|a| a.arm.clone())).collect();
    let Ok(discs) = tip_contact_discs(&arm_list, ctx.layers) else {
        mark_old_law(struts, OldLawReason::NoSeat);
        return tally;
    };
    let mut disc_of: Vec<Option<TipContactDisc>> = vec![None; n];
    for (&i, d) in order.iter().zip(discs) {
        disc_of[i] = d;
    }
    let (drawn, _) = crate::adapter::drawn_profiles(struts, ctx.taper);
    let id_index: HashMap<u32, usize> = struts.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
    let mut geo: Vec<Option<Geo>> = (0..n).map(|_| None).collect();
    for (i, drawn_arm) in arms.into_iter().enumerate() {
        let Some(da) = drawn_arm else { continue };
        let strut = &struts[i];
        let headed = da.arm.head.is_some();
        let mut profile = drawn[i].clone();
        if headed {
            profile.truncate(strut.post_top_layer().saturating_sub(strut.k_bottom).saturating_add(1));
        }
        let disc = disc_of[i];
        let mut top = da.top;
        if !headed {
            if let Some(d) = &disc {
                if strut.landing != Landing::Plate {
                    top = d.ball_centre_mm;
                }
            }
        }
        let seat = disc.as_ref().and_then(|d| head_seat_of(strut, d));
        geo[i] = Some(Geo {
            arm: da.arm,
            top,
            bottom: da.bottom,
            profile,
            disc,
            seat,
            frusta: Vec::new(),
            bounds: ([0.0; 3], [0.0; 3]),
            taper: None,
        });
    }
    // Today's solids, for the overlap tests.
    for i in 0..n {
        let Some(g) = geo[i].as_ref() else { continue };
        let strut = &struts[i];
        let headed_payload = crate::adapter::head_payload(strut);
        let joint_radius = headed_payload.as_ref().map(|p| p.joint_radius_mm);
        let (mut frusta, first_end) = shaft_frusta(g, h, strut.tip_radius_mm(), joint_radius);
        let taper = frusta.first().zip(first_end).map(|(f, r)| (norm(sub(f.b, f.a)), r));
        if let (Some(seat), Some(p)) = (&g.seat, &headed_payload) {
            let knots: Vec<[f64; 2]> = p.profile_knots_mm.clone();
            let ball = g.disc.as_ref().map(|d| (d.ball_centre_mm, d.ball_radius_mm));
            frusta.extend(head_frusta(seat, &knots, p.joint_radius_mm, ball));
        } else if let Some(d) = &g.disc {
            frusta.push(Frust { a: d.ball_centre_mm, b: d.ball_centre_mm, r: d.ball_radius_mm });
        }
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for f in &frusta {
            let (a, b) = f.bounds();
            for k in 0..3 {
                lo[k] = lo[k].min(a[k]);
                hi[k] = hi[k].max(b[k]);
            }
        }
        if let Some(g) = geo[i].as_mut() {
            g.frusta = frusta;
            g.bounds = (lo, hi);
            g.taper = taper;
        }
    }
    let mut landers: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut seated: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, strut) in struts.iter().enumerate() {
        if geo[i].is_none() {
            continue;
        }
        if let Landing::Strut { id, .. } = strut.landing {
            if let Some(&host) = id_index.get(&id) {
                if host != i {
                    landers[host].push(i);
                }
            }
        }
        if let Some(host) = strut.seat_on_strut.and_then(|id| id_index.get(&id).copied()) {
            if host != i {
                seated[host].push(i);
            }
        }
    }
    let field = Field { struts: &*struts, geo, id_index, landers, seated, h };
    let flat = (std::f64::consts::PI / registered_emit_sides() as f64).cos();
    let plan = plan_hulls(&field, flat, pitch_mm);
    // Which members are candidates: a live, drawn headed member with a seat.
    let mut results: Vec<Certified> = Vec::new();
    for i in 0..n {
        let Some(head) = field.struts[i].head.as_ref().filter(|head| !head.drawn_straight) else { continue };
        let Some(g) = field.geo[i].as_ref() else { continue };
        let slots = field.struts[i].radius_mm.len();
        if head.ledger_radius_mm.len() != slots || slots < 2 {
            continue;
        }
        let Some(seat) = g.seat else {
            results.push((i, Vec::new(), None, Some(OldLawReason::NoSeat)));
            continue;
        };
        match certify_one(&field, i, &seat, &plan, ctx, pitch_mm, flat, sizing.plate_z_mm) {
            Ok((radii, cert)) => results.push((i, radii, Some(cert), None)),
            Err(reason) => results.push((i, Vec::new(), None, Some(reason))),
        }
    }
    drop(plan);
    drop(field);
    for (i, radii, cert, reason) in results {
        let Some(head) = struts[i].head.as_mut() else { continue };
        head.old_law = reason;
        if let Some(cert) = cert {
            tally.certified += 1;
            tally.slots_certified += cert.slots_certified;
            tally.slots_kept += cert.slots_kept;
            tally.slots_pinned += cert.slots_pinned;
            tally.worst_drawn_max = tally.worst_drawn_max.max(cert.drawn_max);
            if cert.drawn_max > 1.0 + 1.0e-9 {
                tally.over_one += 1;
            }
            head.certified = Some(cert);
            for (slot, radius) in radii.into_iter().enumerate() {
                if radius > 0.0 {
                    struts[i].radius_mm[slot] = radius;
                }
            }
        }
    }
    tally
}

fn mark_old_law(struts: &mut [SweepStrut], reason: OldLawReason) {
    for strut in struts.iter_mut() {
        if let Some(head) = strut.head.as_mut().filter(|head| !head.drawn_straight) {
            head.old_law = Some(reason);
        }
    }
}

/// One head's result: its radii (0 where untouched), its certificate or why not.
type Certified = (usize, Vec<f64>, Option<CertifiedHead>, Option<OldLawReason>);

/// Certify one head and size its post. `Err` leaves the head on the old law.
#[expect(
    clippy::too_many_arguments,
    reason = "one head needs the field, its seat, the hull plan, the context, the registered pixel, the polygon flat and the plate"
)]
fn certify_one(
    field: &Field<'_>,
    i: usize,
    seat: &HeadSeat,
    plan: &Plan,
    ctx: &JointContext<'_>,
    pitch_mm: f64,
    flat: f64,
    plate_z: f64,
) -> Result<(Vec<f64>, CertifiedHead), OldLawReason> {
    let strut = &field.struts[i];
    let head = strut.head.as_ref().ok_or(OldLawReason::NoSeat)?;
    let g = field.geo[i].as_ref().ok_or(OldLawReason::NoSeat)?;
    let h = field.h;
    let slots = strut.radius_mm.len();
    let joint_slot = head.k_joint.checked_sub(strut.k_bottom).ok_or(OldLawReason::NoSeat)?;
    if joint_slot + 1 >= slots {
        return Err(OldLawReason::NoSeat);
    }
    // ---- the head: one solve for the cone knots and the joint radius (R5).
    let profile = jc::certified_profile(seat, &[], pitch_mm).ok_or(OldLawReason::Infeasible)?;
    // A member that lands on the head (above the joint) loads the cone: shared.
    for &l in field.landers[i].iter().chain(&field.seated[i]) {
        if let Landing::Strut { layer, .. } = field.struts[l].landing {
            if layer > head.k_joint {
                return Err(OldLawReason::Shared);
            }
        }
        if field.struts[l].seat_on_strut == Some(strut.id) {
            if let Some(gl) = field.geo[l].as_ref() {
                if gl.top[2] > g.top[2] - 1.0e-9 {
                    return Err(OldLawReason::Shared);
                }
            }
        }
    }
    // Anything else touching the certified head solids loads it too.
    let new_head = head_frusta(seat, &profile.knots, profile.joint_radius_mm, None);
    let head_box = {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for f in &new_head {
            let (a, b) = f.bounds();
            for k in 0..3 {
                lo[k] = lo[k].min(a[k]);
                hi[k] = hi[k].max(b[k]);
            }
        }
        (lo, hi)
    };
    let related = related_set(field, i);
    for (j, gj) in field.geo.iter().enumerate() {
        let Some(gj) = gj else { continue };
        if j == i || related.contains(&j) || boxes_apart(&gj.bounds, &head_box) {
            continue;
        }
        if gj.frusta.iter().any(|f| new_head.iter().any(|m| m.touches(f))) {
            return Err(OldLawReason::Shared);
        }
    }
    // ---- the post.
    let today = strut.radius_mm.clone();
    let ledger = &head.ledger_radius_mm;
    let geo_slots = slots_of(g, h, joint_slot);
    if geo_slots.is_empty() {
        return Err(OldLawReason::NoSeat);
    }
    let post_len = norm(sub(g.bottom, g.top));
    let n_axis = unit(sub(g.bottom, g.top)).ok_or(OldLawReason::NoSeat)?;
    let point = |t: f64| lerp(g.top, g.bottom, t);
    // Sources, with their entry parameters.
    let mut entries: Vec<Entry> = Vec::new();
    let push_entry = |entries: &mut Vec<Entry>, tagged: Tagged, t: f64| {
        let t = t.clamp(0.0, 1.0);
        if let Some(e) = entries.iter_mut().find(|e| e.tagged.id == tagged.id) {
            e.t = e.t.min(t);
        } else {
            entries.push(Entry { tagged, t });
        }
    };
    let own = field.own_source(i).ok_or(OldLawReason::NoSeat)?;
    push_entry(&mut entries, own, 0.0);
    let t_of = |p: V3| (dot(sub(p, g.top), n_axis) / post_len.max(ZERO_LENGTH_MM)).clamp(0.0, 1.0);
    for &l in &field.landers[i] {
        let gl = field.geo[l].as_ref().ok_or(OldLawReason::NoSeat)?;
        let mut v = Vec::new();
        field.attached(l, i, 0, &mut v);
        for t in v {
            push_entry(&mut entries, t, t_of(gl.bottom));
        }
    }
    for &l in &field.seated[i] {
        let gl = field.geo[l].as_ref().ok_or(OldLawReason::NoSeat)?;
        let mut v = Vec::new();
        field.attached(l, i, 0, &mut v);
        for t in v {
            push_entry(&mut entries, t, t_of(gl.top));
        }
    }
    // Fusions (R1: every body fused into the post loads it from the overlap
    // down) are found at the radii the post is DRAWN with, which depend on the
    // sources found so far: the solve below repeats until no new body touches
    // the post (the radii only grow with the sources, so it ends).
    let post_box = {
        let widest = today.iter().copied().fold(0.0, f64::max).max(profile.joint_radius_mm);
        Frust { a: g.top, b: g.bottom, r: widest }.bounds()
    };
    let neighbours: Vec<usize> = field
        .geo
        .iter()
        .enumerate()
        .filter(|(j, gj)| {
            *j != i && !related.contains(j) && gj.as_ref().is_some_and(|gj| !boxes_apart(&gj.bounds, &post_box))
        })
        .map(|(j, _)| j)
        .collect();
    let mut fused: std::collections::HashSet<usize> = std::collections::HashSet::new();
    // The slenderness floor of each run between holds (braces, ties).
    let mut holds: Vec<f64> = vec![0.0, post_len];
    for &l in field.landers[i].iter().chain(&field.seated[i]) {
        if field.struts[l].kind != StrutKind::Arm {
            if let Some(gl) = field.geo[l].as_ref() {
                let p = if field.struts[l].seat_on_strut == Some(strut.id) { gl.top } else { gl.bottom };
                holds.push(t_of(p) * post_len);
            }
        }
    }
    holds.sort_by(f64::total_cmp);
    let floor_at = |t: f64| -> f64 {
        if ctx.slenderness_ratio <= 0.0 {
            return 0.0;
        }
        let s = t * post_len;
        let upper = holds.iter().copied().find(|&x| x >= s - 1.0e-12).unwrap_or(post_len);
        let lower = holds.iter().rev().copied().find(|&x| x < s - 1.0e-12).unwrap_or(0.0);
        (upper - lower) / ctx.slenderness_ratio
    };
    // The needs, per slot: the larger of the two end needs, with every source
    // that has entered above the point.
    let need_at = |entries: &[Entry], t: f64, active_until: f64| -> f64 {
        let pieces: Vec<Vec<Ab>> = entries
            .iter()
            .filter(|e| e.t <= active_until + 1.0e-9)
            .map(|e| jc::pieces_at(&e.tagged.src, point(t), n_axis))
            .collect();
        jc::r_need(&pieces, 0.0)
    };
    let mut needs: Vec<f64> = vec![0.0; slots];
    // ---- top down: the running maximum, today's radius where the need is
    // above it or a hull ring holds the slot.
    let mut pinned: Vec<bool> = vec![false; slots];
    for p in &plan.pins[i] {
        for sg in &geo_slots {
            let z_hi = at_z_param(g, sg.t_hi);
            let z_lo = at_z_param(g, sg.t_lo);
            if z_lo <= p.z_hi + 1.0e-9 && z_hi >= p.z_lo - 1.0e-9 {
                pinned[sg.slot] = true;
            }
        }
    }
    let mut radii = vec![0.0; slots];
    let mut stats = [0_usize; 3];
    for round in 0..24 {
        // The needs with the sources found so far.
        entries.sort_by(|a, b| a.t.total_cmp(&b.t));
        for sg in &geo_slots {
            let mut worst = need_at(&entries, sg.t_hi, sg.t_hi).max(need_at(&entries, sg.t_lo, sg.t_lo));
            for e in &entries {
                if e.t > sg.t_hi + 1.0e-9 && e.t < sg.t_lo - 1.0e-9 {
                    worst = worst.max(need_at(&entries, e.t, e.t));
                }
            }
            needs[sg.slot] = worst.max(floor_at(0.5 * (sg.t_hi + sg.t_lo)));
        }
        radii = vec![0.0; slots];
        stats = [0; 3];
        let mut running = profile.joint_radius_mm;
        // Top down: the joint slot first, the post's base last.
        // A slot below the drawn shaft (under a plate post's boss top) is drawn
        // only through the root the flare is sized from: it follows the running
        // maximum like the rest, with no need of its own.
        for slot in (0..=joint_slot).rev() {
            let todays = today[slot];
            // A post is never thinner than what stands above it (Craig's
            // not-thinner-below rule): a slot that keeps today's radius keeps
            // it unless the running maximum is wider.
            if pinned[slot] {
                running = running.max(todays);
                radii[slot] = running;
                stats[2] += 1;
                continue;
            }
            let want = running.max(needs[slot]).max(ledger[slot]);
            if want <= todays * (1.0 + 1.0e-12) {
                radii[slot] = want;
                running = want;
                stats[0] += 1;
            } else {
                running = running.max(todays);
                radii[slot] = running;
                stats[1] += 1;
            }
        }
        // R8, no NEW hull: a buried end must stay buried in the thinner host.
        let mut candidate = g.profile.clone();
        for (slot, &r) in radii.iter().enumerate() {
            if r > 0.0 && slot < candidate.len() {
                candidate[slot] = r;
            }
        }
        let mut surfaced = false;
        for b in &plan.buried[i] {
            if b.corners.iter().all(|&p| inside_by_profile(g, &candidate, p, h, flat)) {
                continue;
            }
            for &p in &b.corners {
                let z = p[2];
                let slot = slot_of(z, g.arm.base_mm[2], h, g.profile.len());
                let (lo, hi) = (slot.saturating_sub(1), (slot + 1).min(slots - 1));
                for flag in &mut pinned[lo..=hi] {
                    if !*flag {
                        *flag = true;
                        surfaced = true;
                    }
                }
            }
        }
        // R1: every body whose drawn solid touches the post as it is drawn
        // loads it from the overlap down (what is routed through it, one step).
        let mut found = false;
        for &j in &neighbours {
            if fused.contains(&j) {
                continue;
            }
            let Some(gj) = field.geo[j].as_ref() else { continue };
            let mut t_min: Option<f64> = None;
            // The topmost slot it touches: the slots run bottom first.
            for sg in geo_slots.iter().rev() {
                let r = if radii[sg.slot] > 0.0 { radii[sg.slot] } else { today[sg.slot] };
                let post = Frust { a: point(sg.t_hi), b: point(sg.t_lo), r };
                if gj.frusta.iter().any(|f| post.touches(f)) {
                    t_min = Some(sg.t_hi);
                    break;
                }
            }
            let Some(t) = t_min else { continue };
            fused.insert(j);
            let mut v = Vec::new();
            field.routed(j, 0, &mut v);
            if v.is_empty() {
                if let Some(rec) = field.record_source(j, point(t)) {
                    v.push(rec);
                }
            }
            for s in v {
                let before = entries.len();
                let known = entries.iter().find(|e| e.tagged.id == s.id).map(|e| e.t);
                push_entry(&mut entries, s, t);
                if entries.len() != before || known.is_some_and(|k| t < k - 1.0e-9) {
                    found = true;
                }
            }
        }
        if !surfaced && !found {
            break;
        }
        if round == 23 {
            return Err(OldLawReason::Infeasible);
        }
    }
    if std::env::var_os("PLINTH_JOINT_TRACE").is_some() {
        eprintln!(
            "[joint-trace] member {} (id {}): joint {:.4} sources {:?}",
            i,
            strut.id,
            profile.joint_radius_mm,
            entries
                .iter()
                .map(|e| (e.tagged.id, (e.t * 1000.0).round() / 1000.0, e.tagged.src.r_a, e.tagged.src.r_b))
                .collect::<Vec<_>>()
        );
        for sg in &geo_slots {
            eprintln!(
                "[joint-trace]   slot {:>3} t {:.3}-{:.3} today {:.4} ledger {:.4} need {:.4} -> {:.4} {}",
                sg.slot,
                sg.t_hi,
                sg.t_lo,
                today[sg.slot],
                ledger[sg.slot],
                needs[sg.slot],
                radii[sg.slot],
                if pinned[sg.slot] { "PINNED" } else if radii[sg.slot] >= today[sg.slot] - 1e-9 { "kept" } else { "" }
            );
        }
    }
    // ---- the check (I): every drawn chord, every certified slot under it, at its
    // certified maximum. Reported, never refused.
    let mut drawn_max = profile.certified_max;
    {
        let mut cand = g.profile.clone();
        for (slot, &r) in radii.iter().enumerate() {
            if r > 0.0 && slot < cand.len() {
                cand[slot] = r;
            }
        }
        let tip = profile.joint_radius_mm;
        let root = drawn_root_radius_mm(&cand, strut.tip_radius_mm());
        #[expect(clippy::cast_precision_loss, reason = "a profile length bounded by the layer count")]
        let top_z = (g.arm.base_mm[2] + cand.len() as f64 * h).max(g.top[2]);
        let spans = drawn_spans(&cand, g.arm.base_mm[2], top_z, g.top[2], g.bottom[2], tip, root);
        for span in &spans {
            for sg in &geo_slots {
                let changed = radii[sg.slot] > 0.0 && radii[sg.slot] < today[sg.slot] - 1.0e-9 && !pinned[sg.slot];
                if !changed {
                    continue;
                }
                let z_hi = at_z_param(g, sg.t_hi).min(span.top_z_mm);
                let z_lo = at_z_param(g, sg.t_lo).max(span.bottom_z_mm);
                if z_hi - z_lo < 1.0e-9 {
                    continue;
                }
                let r_at = |z: f64| {
                    let f = (span.top_z_mm - z) / (span.top_z_mm - span.bottom_z_mm).max(ZERO_LENGTH_MM);
                    span.top_radius_mm + (span.bottom_radius_mm - span.top_radius_mm) * f
                };
                let t_a = (g.top[2] - z_hi) / (g.top[2] - g.bottom[2]);
                let t_b = (g.top[2] - z_lo) / (g.top[2] - g.bottom[2]);
                // A source loads only the part of the slot below its entry:
                // split the interval at every entry inside it.
                let mut cuts = vec![t_a, t_b];
                cuts.extend(entries.iter().map(|e| e.t).filter(|&t| t > t_a + 1.0e-9 && t < t_b - 1.0e-9));
                cuts.sort_by(f64::total_cmp);
                for w in cuts.windows(2) {
                    let active: Vec<Src> =
                        entries.iter().filter(|e| e.t <= w[0] + 1.0e-9).map(|e| e.tagged.src).collect();
                    if active.is_empty() {
                        continue;
                    }
                    let (za, zb) = (at_z_param(g, w[0]), at_z_param(g, w[1]));
                    let m = jc::span_max(&active, point(w[0]), point(w[1]), n_axis, (r_at(za), r_at(zb)), (0.0, 1.0));
                    drawn_max = drawn_max.max(m);
                }
            }
        }
    }
    // ---- the head's ledger slots (layers over the joint, under the contact
    // slot): the certified cone at the layer's axial distance, never under the
    // area-law ledger. The lowering draws the head from the payload; these
    // slots are what the clearance tests and the resin count read.
    let knots = &profile.knots;
    let cone_radius = |s: f64| -> f64 {
        let first = knots[0];
        if s <= first[0] {
            return first[1];
        }
        for pair in knots.windows(2) {
            if s <= pair[1][0] {
                let t = (s - pair[0][0]) / (pair[1][0] - pair[0][0]).max(f64::MIN_POSITIVE);
                return pair[0][1] + (pair[1][1] - pair[0][1]) * t;
            }
        }
        knots[knots.len() - 1][1]
    };
    let u_z = head.unit[2];
    for slot in (joint_slot + 1)..slots.saturating_sub(1) {
        #[expect(clippy::cast_precision_loss, reason = "a layer index bounded by the plan's layer count")]
        let z = plate_z + (strut.k_bottom + slot) as f64 * h;
        let s = if u_z.abs() > f64::EPSILON {
            ((z - head.contact_mm[2]) / u_z).clamp(0.0, seat.length)
        } else {
            0.0
        };
        radii[slot] = cone_radius(s).max(ledger[slot]);
    }
    let cert = CertifiedHead {
        profile,
        slots_certified: stats[0],
        slots_kept: stats[1],
        slots_pinned: stats[2],
        drawn_max,
    };
    Ok((radii, cert))
}

fn at_z_param(g: &Geo, t: f64) -> f64 {
    g.top[2] + (g.bottom[2] - g.top[2]) * t
}

/// Members already counted as sources of member `i` (it, and everything routed
/// through it): not "fused" bodies.
fn related_set(field: &Field<'_>, i: usize) -> std::collections::HashSet<usize> {
    let mut set = std::collections::HashSet::new();
    set.insert(i);
    // descendants: everything routed through `i`
    let mut stack = vec![i];
    while let Some(c) = stack.pop() {
        for &l in field.landers[c].iter().chain(&field.seated[c]) {
            if set.insert(l) {
                stack.push(l);
            }
        }
    }
    // The members S rests on are NOT excluded: a host whose solid is fused into
    // the post (the junction region, a stalk that ends inside it) shares the
    // load, so what is routed through it loads the post from the overlap down.
    set
}
