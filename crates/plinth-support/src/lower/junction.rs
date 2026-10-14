//! **Landed junctions as hulls** (heads v19 fixes, L2, 2026-10-08).
//!
//! A landed branch used to end on the host's axis with a flat cap square to its
//! own axis, and the host carried a separate collar cone. On the Cylinder at v19
//! twelve branch ends stood 0.2 mm or more out of the host as a down-facing lens.
//! Craig's ruling (v18): the junction between members is the CONVEX HULL of the
//! end circles that meet there; a thin member buried in a thick post is the same
//! hull hidden inside and costs nothing.
//!
//! **What this pass does**, over the primitives the per-arm lowering produced:
//!
//! 1. For each landed branch (thickest first) it finds the host: the other arm
//!    whose drawn shaft has the branch's landing point inside its section.
//! 2. If every corner of the branch's end ring already lies inside the host's
//!    drawn solid, the junction is BURIED and nothing changes (the hull would be
//!    hidden inside the host).
//! 3. Otherwise the members that meet are the branch, the host below and, if the
//!    host continues, the host above. Each member `i` is pulled back from the
//!    landing point `K` along its own axis by `t_i = r_max * tan(d_i / 2)`, with
//!    `r_max` the largest meeting radius and `d_i` the member's largest
//!    deflection from straight-on against another member that lies on the far
//!    side of `K` (an angle between axes over 90 degrees). Two members whose axes
//!    make an acute angle overlap at `K`, leave no outer wedge to fill, and need
//!    no pull-back: counting them would pull a member back by several radii (a
//!    10 degree Y asks for `tan(85 deg)` = 11 radii).
//! 4. Each member's frustum now ends on a ring at `K + t_i u_i`. The collar cone
//!    that belongs to this junction (the host's widening-downward frusta just
//!    above `K`) is dropped; the host above is drawn at its own thin radius from
//!    its ring. The hull is the convex hull of the rings, emitted as one `Member`
//!    record whose rings are made, by `plinth-emit`, from the neighbour frusta's
//!    own `start`, `end` and radius with the same ring routine
//!    ([`crate::hull::frustum_ring`]), so a hull ring equals its neighbour's ring
//!    bit for bit.
//!
//! A junction that cannot be cut into the host this way (the host chain is too
//! crowded because an earlier hull already took the arc, or the host has nothing
//! above the landing point and begins with the contact taper, or a piece cannot be
//! drawn) is FILLED instead: the host and the branch stay as they are, and an added
//! hull of the branch's whole end ring and the host's own sections a pull-back
//! above and below the landing point covers the wedge the branch end leaves
//! outside the host. Only a junction that cannot even be filled (a flat hull, a
//! ring that cannot be made) keeps the old flat cap; it is counted, and the caller
//! warns. Emit and warn, never refuse.

use geom_kernel::vec3::{add, dot, norm, normalize, scale, sub};
use plinth_core::{Finite, PositiveFinite};

use crate::hull::{convex_hull, frustum_ring};
use crate::{FrustumRingNormal, HullRing, JunctionRings, SupportPrimitive, SupportSegmentKind};

type V = [f64; 3];

/// Readings the pass needs; all registered, read once by the caller.
#[derive(Clone, Copy, Debug)]
pub(super) struct JunctionRules {
    /// One registered XY pixel pitch: the shortest member worth a ring.
    pub pixel_pitch_mm: f64,
    /// The plan's layer height: how far above the landing point a collar may
    /// end and still belong to this junction (the adapter writes a load step one
    /// radius per layer slot).
    pub layer_height_mm: f64,
    /// `chitubox_emit_n_sides`, the polygon every ring is drawn with.
    pub sides: usize,
    /// The radius a cone may change per mm of height, the lean law's own
    /// `cot(theta_sat)` ([`super::DrawnSpanRules`]). A kept hull ring is met by a
    /// taper that never narrows faster than this going up. `0.0` (no law
    /// registered) draws no taper: the shaft is left as the arm's own.
    pub cone_slope: f64,
}

/// One arm as the pass sees it: whether it lands on something, and where its
/// shaft frusta are in the primitive list.
#[derive(Clone, Copy, Debug)]
pub(super) struct JunctionArm {
    pub landed: bool,
    /// `(first, len)` of the arm's drawn shaft frusta, top-down.
    pub shaft: Option<(usize, usize)>,
}

/// What the pass did, per arm, and how many junctions went each way.
#[derive(Debug, Default)]
pub(super) struct JunctionOutcome {
    /// The replacement shaft of an arm the pass edited.
    pub shafts: Vec<Option<Vec<SupportPrimitive>>>,
    /// The hull records to place right after an arm's last record.
    pub hulls: Vec<Vec<SupportPrimitive>>,
    /// Per arm, the arcs of its shaft (in the arm's own frame, from the first
    /// frustum's start) that the junctions made here replace: the host arc a
    /// hull replaces and the branch end it shortens. A filled junction edits no
    /// shaft and has none. [`keep_planned_hulls`] puts the shaft inside them, as
    /// this pass left it, into another field's shaft.
    pub windows: Vec<Vec<(f64, f64)>>,
    /// Per arm, the ring of its last frustum that a FILLED junction's added hull
    /// takes whole (the branch's end ring). A filled junction edits no shaft and
    /// has no window, but its hull still stands on that ring:
    /// [`keep_planned_hulls`] meets it as it meets the ring at a window's edge.
    pub end_rings: Vec<Option<EndRing>>,
    /// How the kept hull rings were met ([`keep_planned_hulls`] only).
    pub rings: RingStats,
    pub hulled: usize,
    /// Junctions the host could not be cut for (crowded by an earlier hull, or the
    /// host begins with the contact taper), filled with an ADDED hull of the
    /// branch's whole end ring and the host's sections beside the landing point.
    pub filled: usize,
    pub buried: usize,
    pub no_host: usize,
    /// Junctions neither cut nor filled, left with the old flat cap: the warning
    /// the caller prints.
    pub crowded: usize,
    pub undrawable: usize,
}

/// The end ring of an arm's last frustum, as a filled junction's hull takes it.
#[derive(Clone, Copy, Debug)]
pub(super) struct EndRing {
    pub centre: [Finite; 3],
    pub radius_mm: f64,
}

/// What happened at the kept hull rings that stand at a shaft's cut ([`keep_planned_hulls`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct RingStats {
    /// Rings the arm's shaft already met on the ring's own radius.
    pub exact: usize,
    /// Rings thicker than the arm's chord there: the shaft is drawn on the ring
    /// radius and tapers down to its chord, never thinner than the chord.
    pub thicker: usize,
    /// Rings thinner than the arm's chord there (the ring is the old law's
    /// radius): the shaft is drawn on the ring radius and tapers up to its chord.
    pub thinner: usize,
    /// Rings no taper could reach (no piece to taper, or the pieces ran out
    /// before the offset closed): the shaft meets them on its own radius.
    pub unmet: usize,
    /// Of those: no piece of the arm stands at the edge, the piece there is not
    /// one a taper may change (a sphere), and the pieces ran out.
    pub unmet_no_piece: usize,
    pub unmet_not_adjustable: usize,
    pub unmet_ran_out: usize,
    /// The largest radius difference closed at a ring, mm.
    pub max_gap_mm: f64,
    /// The largest difference between a ring's radius and the arm's chord that
    /// the taper left under the chord, mm (a thinner ring).
    pub max_under_chord_mm: f64,
}

impl RingStats {
    fn add(&mut self, other: Self) {
        self.exact += other.exact;
        self.thicker += other.thicker;
        self.thinner += other.thinner;
        self.unmet += other.unmet;
        self.unmet_no_piece += other.unmet_no_piece;
        self.unmet_not_adjustable += other.unmet_not_adjustable;
        self.unmet_ran_out += other.unmet_ran_out;
        self.max_gap_mm = self.max_gap_mm.max(other.max_gap_mm);
        self.max_under_chord_mm = self.max_under_chord_mm.max(other.max_under_chord_mm);
    }
}

fn pt(p: [Finite; 3]) -> V {
    p.map(Finite::get)
}

fn fin(p: V) -> Option<[Finite; 3]> {
    Some([
        Finite::new(p[0]).ok()?,
        Finite::new(p[1]).ok()?,
        Finite::new(p[2]).ok()?,
    ])
}

fn lerp(a: V, b: V, t: f64) -> V {
    add(a, scale(sub(b, a), t))
}

/// A shaft in a fixed frame: its first frustum's start, and the unit direction to
/// its last frustum's end. All of an arm's shaft frusta are collinear.
#[derive(Clone, Copy)]
struct Frame {
    origin: V,
    dir: V,
}

impl Frame {
    fn of(spans: &[SupportPrimitive]) -> Option<Self> {
        let first = spans.first()?;
        let last = spans.last()?;
        let origin = pt(first.start_mm);
        let dir = normalize(sub(pt(last.end_mm), origin))?;
        Some(Self { origin, dir })
    }

    fn arc(&self, p: V) -> f64 {
        dot(sub(p, self.origin), self.dir)
    }
}

/// `(start arc, end arc)` of each frustum in the frame.
fn arcs(frame: Frame, spans: &[SupportPrimitive]) -> Vec<(f64, f64)> {
    spans
        .iter()
        .map(|s| (frame.arc(pt(s.start_mm)), frame.arc(pt(s.end_mm))))
        .collect()
}

fn radii(s: &SupportPrimitive) -> (f64, f64) {
    (s.start_radius_mm.get(), s.end_radius_mm.get())
}

fn radius_at(s: &SupportPrimitive, arc: (f64, f64), at: f64) -> f64 {
    let (r0, r1) = radii(s);
    let span = arc.1 - arc.0;
    if span.abs() <= f64::MIN_POSITIVE {
        return r0.max(r1);
    }
    r0 + (r1 - r0) * ((at - arc.0) / span).clamp(0.0, 1.0)
}

/// The distance from `p` to a frustum's axis segment, the parameter along it and
/// the radius there.
fn locate(p: V, s: &SupportPrimitive) -> (f64, f64, f64) {
    let (a, b) = (pt(s.start_mm), pt(s.end_mm));
    let axis = sub(b, a);
    let length_sq = dot(axis, axis);
    let t = if length_sq > 0.0 {
        (dot(sub(p, a), axis) / length_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let foot = add(a, scale(axis, t));
    let (r0, r1) = radii(s);
    (norm(sub(p, foot)), t, r0 + (r1 - r0) * t)
}

/// Whether `p` lies inside the polygonal solid of the chain of frusta `spans`
/// (the polygon's flats stand at `cos(pi / sides)` of the radius).
fn inside_chain(p: V, spans: &[SupportPrimitive], flat: f64) -> bool {
    spans.iter().any(|s| {
        let (a, b) = (pt(s.start_mm), pt(s.end_mm));
        let axis = sub(b, a);
        let length_sq = dot(axis, axis);
        if length_sq <= 0.0 {
            return false;
        }
        let t = dot(sub(p, a), axis) / length_sq;
        if !(0.0..=1.0).contains(&t) {
            return false;
        }
        let (r0, r1) = radii(s);
        let foot = add(a, scale(axis, t));
        norm(sub(p, foot)) <= (r0 + (r1 - r0) * t) * flat * (1.0 - 1.0e-6)
    })
}

fn widening_down(s: &SupportPrimitive) -> bool {
    let (r0, r1) = radii(s);
    r1 > r0 * (1.0 + 1.0e-9)
}

/// The same frustum, ending at `end` with radius `end_radius`.
fn with_end(s: &SupportPrimitive, end: V, end_radius: f64) -> Option<SupportPrimitive> {
    let mut out = s.clone();
    out.end_mm = fin(end)?;
    out.end_radius_mm = PositiveFinite::new(end_radius).ok()?;
    Some(out)
}

/// The same frustum, starting at `start` with radius `start_radius`. A contact
/// taper cut in two keeps its taper (and its trim) in the upper piece only.
fn with_start(s: &SupportPrimitive, start: V, start_radius: f64) -> Option<SupportPrimitive> {
    let mut out = s.clone();
    out.start_mm = fin(start)?;
    out.start_radius_mm = PositiveFinite::new(start_radius).ok()?;
    if out.segment_kind == SupportSegmentKind::ContactTaper {
        out.segment_kind = SupportSegmentKind::Member;
        out.contact_depth_mm = None;
    }
    Some(out)
}

/// Pull-backs: `t_i = r_max * tan(d_i / 2)`, `d_i` the member's largest
/// deflection from straight-on against a member on the far side of `K`.
fn pull_backs(units: &[V], r_max: f64) -> Vec<f64> {
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

struct Member {
    unit: V,
}

/// The pass. `primitives` is the per-arm lowering's output, `arms` the matching
/// arm descriptions.
#[expect(
    clippy::too_many_lines,
    reason = "one junction is one decision: host, members, collar, pull-back, pieces, hull"
)]
pub(super) fn hull_junctions(
    primitives: &[SupportPrimitive],
    arms: &[JunctionArm],
    rules: JunctionRules,
) -> JunctionOutcome {
    let count = arms.len();
    let mut outcome = JunctionOutcome {
        shafts: vec![None; count],
        hulls: vec![Vec::new(); count],
        windows: vec![Vec::new(); count],
        end_rings: vec![None; count],
        ..JunctionOutcome::default()
    };
    let original = |arm: usize| -> &[SupportPrimitive] {
        arms[arm]
            .shaft
            .map_or(&[][..], |(first, len)| &primitives[first..first + len])
    };
    let frames: Vec<Option<Frame>> = (0..count).map(|i| Frame::of(original(i))).collect();
    let mut locked: Vec<Vec<(f64, f64)>> = vec![Vec::new(); count];
    let flat = (std::f64::consts::PI / rules.sides.max(3) as f64).cos();

    let mut order: Vec<usize> = (0..count)
        .filter(|&i| arms[i].landed && frames[i].is_some())
        .collect();
    let end_radius = |arm: usize| original(arm).last().map_or(0.0, |s| s.end_radius_mm.get());
    order.sort_by(|&a, &b| end_radius(b).total_cmp(&end_radius(a)).then(a.cmp(&b)));

    for a in order {
        let spans_a: Vec<SupportPrimitive> = outcome.shafts[a]
            .clone()
            .unwrap_or_else(|| original(a).to_vec());
        let (Some(last), Some(frame_a)) = (spans_a.last(), frames[a]) else {
            continue;
        };
        let k = pt(last.end_mm);
        let (r_b, len_b) = (
            last.end_radius_mm.get(),
            norm(sub(pt(last.start_mm), pt(last.end_mm))),
        );
        let Some(u_b) = normalize(sub(pt(last.start_mm), k)) else {
            continue;
        };
        // 1. THE HOST: the other arm whose current shaft holds K inside its section.
        let mut best: Option<(f64, usize)> = None;
        for h in (0..count).filter(|&h| h != a && arms[h].shaft.is_some()) {
            let spans: &[SupportPrimitive] = outcome.shafts[h].as_deref().unwrap_or(original(h));
            for s in spans {
                let (distance, _, radius) = locate(k, s);
                if distance <= radius && best.is_none_or(|(d, _)| distance < d) {
                    best = Some((distance, h));
                }
            }
        }
        let Some((_, h)) = best else {
            outcome.no_host += 1;
            continue;
        };
        let Some(frame_h) = frames[h] else {
            continue;
        };
        let host: Vec<SupportPrimitive> = outcome.shafts[h]
            .clone()
            .unwrap_or_else(|| original(h).to_vec());
        // 2. BURIED: the branch's end ring wholly inside the host's drawn solid.
        let Some(branch_ring) = frustum_ring(
            pt(last.start_mm),
            pt(last.end_mm),
            r_b,
            false,
            last.ring_normal == FrustumRingNormal::PlateZ,
            rules.sides,
        ) else {
            outcome.undrawable += 1;
            continue;
        };
        if branch_ring.iter().all(|&p| inside_chain(p, &host, flat)) {
            outcome.buried += 1;
            continue;
        }
        // 3. THE HOST CHAIN around K.
        let host_arcs = arcs(frame_h, &host);
        let (Some(&(top_arc, _)), Some(&(_, bottom_arc))) = (host_arcs.first(), host_arcs.last())
        else {
            continue;
        };
        let s_k = frame_h.arc(k);
        let f = (0..host.len())
            .rev()
            .find(|&i| host_arcs[i].0 <= s_k + 1.0e-9)
            .unwrap_or(0);
        let above_len = s_k - top_arc;
        let below_len = bottom_arc - s_k;
        let has_above = above_len > rules.pixel_pitch_mm;
        let has_below = below_len > rules.pixel_pitch_mm;
        // The landing point within a pixel of the contact taper's start: there is
        // no host above to keep the taper on, so the host cannot be cut here and
        // the junction is filled with an added hull instead.
        let taper_blocks = !has_above && host[0].segment_kind == SupportSegmentKind::ContactTaper;
        // The collar: widening-downward frusta directly above K's own frustum.
        let contiguous = |g: usize| (host_arcs[g].1 - host_arcs[g + 1].0).abs() <= 1.0e-9;
        let mut run: Option<(usize, usize)> = None;
        let k_in_widening = widening_down(&host[f]) && s_k > host_arcs[f].0 + 1.0e-9;
        if f > 0 && !k_in_widening && contiguous(f - 1) {
            let mut g = f;
            while g > 0
                && contiguous(g - 1)
                && widening_down(&host[g - 1])
                && host[g - 1].segment_kind != SupportSegmentKind::ContactTaper
                && host[g - 1].seat_trim.is_none()
            {
                g -= 1;
            }
            if g < f {
                run = Some((g, f - 1));
            }
        }
        let collar = run.filter(|&(_, bottom)| {
            s_k - host_arcs[bottom].1 <= 2.0 * rules.layer_height_mm + 1.0e-9
        });
        let r_thin = collar.map(|(top, _)| host[top].start_radius_mm.get());
        let r_l = radius_at(&host[f], host_arcs[f], s_k);
        let r_a_est = r_thin.unwrap_or(r_l);
        let r_max = r_b.max(r_l).max(r_a_est);
        // 4. THE MEMBERS and their pull-backs.
        let mut members = vec![Member { unit: u_b }];
        let (mut slot_below, mut slot_above) = (None, None);
        if has_below {
            slot_below = Some(members.len());
            members.push(Member { unit: frame_h.dir });
        }
        if has_above {
            slot_above = Some(members.len());
            members.push(Member {
                unit: scale(frame_h.dir, -1.0),
            });
        }
        let units: Vec<V> = members.iter().map(|m| m.unit).collect();
        let pulls = pull_backs(&units, r_max);
        let t_b = pulls[0].min(0.5 * len_b);
        let t_l = slot_below.map_or(0.0, |i| pulls[i].min(0.5 * below_len));
        let t_a = slot_above.map_or(0.0, |i| pulls[i].min(0.5 * above_len));
        let q_a = s_k - t_a;
        let q_b = s_k + t_l;
        // 5. CROWDING: the arcs this junction takes on the host and the branch.
        // Above this arc the host is kept as it is. With a collar, everything
        // from the collar's top to the ring is replaced by the thin host.
        let cut_top = collar.map_or(q_a, |(top, _)| q_a.min(host_arcs[top].0));
        let host_take = (cut_top, q_b);
        let len_a_total = frame_a.arc(pt(last.end_mm));
        let branch_take = (len_a_total - t_b, len_a_total);
        let overlaps = |taken: &[(f64, f64)], take: (f64, f64)| {
            taken
                .iter()
                .any(|&(lo, hi)| take.0 < hi + 1.0e-9 && take.1 > lo - 1.0e-9)
        };
        let crowded = overlaps(&locked[h], host_take) || overlaps(&locked[a], branch_take);
        // The junction cannot be cut into the host (crowded, or the taper is in the
        // way, or a piece is undrawable below): it is filled instead, with the hull
        // of the branch's whole end ring and the host's own sections a pull-back
        // above and below the landing point. The host and the branch stay as they
        // are, and the hull covers the wedge the branch end leaves outside the host.
        let fill_rings = FillRings {
            branch: last,
            host: &host,
            host_arcs: &host_arcs,
            frame: frame_h,
            s_k,
            above: slot_above.map(|i| pulls[i].max(rules.pixel_pitch_mm).min(0.5 * above_len)),
            below: slot_below.map(|i| pulls[i].max(rules.pixel_pitch_mm).min(0.5 * below_len)),
        };
        let mut skip = if taper_blocks {
            Some(Skip::Undrawable)
        } else if crowded {
            Some(Skip::Crowded)
        } else {
            None
        };
        // 6. THE PIECES. The host above (with the collar dropped and a thin
        // extension to its ring), the host below, and the branch's shortened end.
        if skip.is_none() {
            skip = 'build: {
                let mut upper: Vec<SupportPrimitive> = Vec::new();
                let mut lower: Vec<SupportPrimitive> = Vec::new();
                let mut ok = true;
                for (i, s) in host.iter().enumerate() {
                    let (a0, a1) = host_arcs[i];
                    let in_run = collar.is_some_and(|(top, bottom)| (top..=bottom).contains(&i));
                    if !in_run && a1 <= cut_top + 1.0e-12 {
                        upper.push(s.clone());
                    } else if !in_run && a0 < cut_top - 1.0e-12 && has_above {
                        let t = (cut_top - a0) / (a1 - a0);
                        let end = lerp(pt(s.start_mm), pt(s.end_mm), t);
                        let r = radius_at(s, host_arcs[i], cut_top);
                        match with_end(s, end, r) {
                            Some(piece) => upper.push(piece),
                            None => ok = false,
                        }
                    }
                    if a0 >= q_b - 1.0e-12 && !in_run {
                        lower.push(s.clone());
                    } else if a1 > q_b + 1.0e-12 && a0 < q_b && !in_run && has_below {
                        let t = (q_b - a0) / (a1 - a0);
                        let start = lerp(pt(s.start_mm), pt(s.end_mm), t);
                        let r = radius_at(s, host_arcs[i], q_b);
                        match with_start(s, start, r) {
                            Some(piece) => lower.push(piece),
                            None => ok = false,
                        }
                    }
                }
                // Collar dropped: the thin host runs on, at its own radius, to its ring.
                if let (Some((top, _)), true) = (collar, has_above) {
                    let reach = upper.last().map_or(top_arc, |u| frame_h.arc(pt(u.end_mm)));
                    if q_a > reach + 1.0e-12 {
                        let from = upper
                            .last()
                            .map_or_else(|| pt(host[top].start_mm), |u| pt(u.end_mm));
                        let to = add(frame_h.origin, scale(frame_h.dir, q_a));
                        let r = r_thin.unwrap_or(r_l);
                        let long_enough = norm(sub(to, from)) >= rules.pixel_pitch_mm;
                        match (upper.last().cloned(), long_enough) {
                            (Some(prev), false) => {
                                match with_end(&prev, to, prev.end_radius_mm.get()) {
                                    Some(stretched) => {
                                        let at = upper.len() - 1;
                                        upper[at] = stretched;
                                    }
                                    None => ok = false,
                                }
                            }
                            (prev, _) => {
                                let ring_normal = prev
                                    .as_ref()
                                    .map_or(host[top].ring_normal, |p| p.ring_normal);
                                match (fin(from), fin(to), PositiveFinite::new(r)) {
                                    (Some(start_mm), Some(end_mm), Ok(radius)) => {
                                        upper.push(SupportPrimitive {
                                            segment_kind: SupportSegmentKind::Member,
                                            start_mm,
                                            end_mm,
                                            start_radius_mm: radius,
                                            end_radius_mm: radius,
                                            contact_depth_mm: None,
                                            ring_normal,
                                            sphere_geometry: None,
                                            seat_trim: None,
                                        });
                                    }
                                    _ => ok = false,
                                }
                            }
                        }
                    }
                }
                // The branch's end, pulled back.
                let mut spans_a_new = spans_a.clone();
                if t_b > 0.0 {
                    let fraction = 1.0 - t_b / len_b;
                    let end = lerp(pt(last.start_mm), pt(last.end_mm), fraction);
                    let (r0, r1) = radii(last);
                    match (
                        with_end(last, end, r0 + (r1 - r0) * fraction),
                        spans_a_new.last_mut(),
                    ) {
                        (Some(piece), Some(slot)) => *slot = piece,
                        _ => ok = false,
                    }
                }
                if !ok || (has_above && upper.is_empty()) || (has_below && lower.is_empty()) {
                    break 'build Some(Skip::Undrawable);
                }
                // 7. THE RINGS and the hull.
                let Some(branch_last) = spans_a_new.last() else {
                    break 'build Some(Skip::Undrawable);
                };
                let rings = [
                    Some(ring_of(branch_last, false)),
                    lower
                        .first()
                        .filter(|_| has_below)
                        .map(|s| ring_of(s, true)),
                    upper
                        .last()
                        .filter(|_| has_above)
                        .map(|s| ring_of(s, false)),
                ];
                let Some(record) = hull_record(rings, rules.sides) else {
                    break 'build Some(Skip::Undrawable);
                };
                outcome.hulls[a].push(record);
                // 8. COMMIT.
                let mut host_new = upper;
                host_new.extend(lower);
                outcome.shafts[h] = Some(host_new);
                outcome.shafts[a] = Some(spans_a_new);
                locked[h].push(host_take);
                locked[a].push(branch_take);
                // A host with nothing to keep above (or below) the landing point
                // drops the sliver of host there: the window runs to its end.
                outcome.windows[h].push((
                    if has_above { host_take.0 } else { f64::NEG_INFINITY },
                    if has_below { host_take.1 } else { f64::INFINITY },
                ));
                outcome.windows[a].push(branch_take);
                outcome.hulled += 1;
                None
            };
        }
        // A junction that could not be cut into the host is filled, not left as the
        // old flat cap; only one that cannot even be filled is counted as skipped.
        if let Some(why) = skip {
            if let Some(record) = fill_rings.record(rules.sides) {
                outcome.hulls[a].push(record);
                // A filled junction edits no shaft: no window. Its hull takes the
                // branch's end ring whole, which a kept hull must meet.
                outcome.end_rings[a] = Some(EndRing {
                    centre: last.end_mm,
                    radius_mm: last.end_radius_mm.get(),
                });
                outcome.filled += 1;
            } else {
                match why {
                    Skip::Crowded => outcome.crowded += 1,
                    Skip::Undrawable => outcome.undrawable += 1,
                }
            }
        }
    }
    outcome
}

/// How far two shafts' frames may differ and still be one shaft, mm.
const SAME_SHAFT_TOLERANCE_MM: f64 = 1.0e-7;
/// Arc length below which a piece is not a piece, mm.
const ARC_EPSILON_MM: f64 = 1.0e-9;

/// **Keep the hulls the reference field planned** (R8, Craig 2026-10-11).
///
/// The pass above reads a shaft's radii at its cut arcs, its collar and its
/// last frustum's length, so a member whose radii moved gets different hosts,
/// pull-backs and rings, and the hull mesh moves with them. This runs the pass
/// on the REFERENCE field (`reference_*`, the arms as the old sizing drew
/// them) and returns its outcome with every hull record as it made it. The
/// shaft of each arm a junction cut is the arm's own, drawn from `primitives`,
/// except inside the junction's arcs ([`JunctionOutcome::windows`]), where it is
/// the reference's, as the pass left it (the thin run that replaces a dropped
/// collar). A filled junction edits no shaft.
///
/// **The ring is the reference's, to the bit.** Where a hull ring stands on a
/// cut of the shaft (the end of the host above the hull, the start of the host
/// below it, the end of the shortened branch), the arm's frustum there ends on
/// the ring's centre and on the ring's radius, so the hull ring and the drawn
/// ring are one ring and the file's boundary geometry is the old law's. The
/// arm's chord differs from the ring by the chord noise of the fit (the ring is
/// the old law's radius, the chord is the new sizing's); that difference is
/// closed by a taper along the arm's own pieces from the ring outward
/// ([`plan_transition`]): a ring thicker than the chord gives a shaft that is
/// never thinner than the chord (certified wherever the chord is); a ring
/// thinner than the chord gives a shaft that rises from the ring's radius (the
/// kept, old-law radius) to the chord. The taper's slope is limited so no piece
/// narrows going up faster than the lean law's step: where a cone already runs
/// at that limit the offset is carried unchanged along it and closed further on.
///
/// `None` when an arm that a junction edits has no shaft here, or its shaft is
/// not on the reference's line: nothing of the reference is used then.
pub(super) fn keep_planned_hulls(
    primitives: &[SupportPrimitive],
    arms: &[JunctionArm],
    reference_primitives: &[SupportPrimitive],
    reference_arms: &[JunctionArm],
    rules: JunctionRules,
) -> Option<JunctionOutcome> {
    let count = arms.len();
    if reference_arms.len() != count {
        return None;
    }
    let planned = hull_junctions(reference_primitives, reference_arms, rules);
    let shaft_of = |prims: &'_ [SupportPrimitive], arm: JunctionArm| -> Option<Vec<SupportPrimitive>> {
        let (first, len) = arm.shaft?;
        prims.get(first..first + len).map(<[SupportPrimitive]>::to_vec)
    };
    let mut kept = JunctionOutcome {
        shafts: vec![None; count],
        hulls: planned.hulls.clone(),
        windows: planned.windows.clone(),
        end_rings: planned.end_rings.clone(),
        rings: RingStats::default(),
        hulled: planned.hulled,
        filled: planned.filled,
        buried: planned.buried,
        no_host: planned.no_host,
        crowded: planned.crowded,
        undrawable: planned.undrawable,
    };
    for index in 0..count {
        let windows = merged_windows(&planned.windows[index]);
        let end_ring = planned.end_rings[index];
        if windows.is_empty() && end_ring.is_none() {
            continue;
        }
        let real = shaft_of(primitives, arms[index])?;
        let original = shaft_of(reference_primitives, reference_arms[index])?;
        let (frame, reference_frame) = (Frame::of(&real)?, Frame::of(&original)?);
        let end_of = |spans: &[SupportPrimitive]| spans.last().map(|s| pt(s.end_mm));
        if norm(sub(frame.origin, reference_frame.origin)) > SAME_SHAFT_TOLERANCE_MM
            || norm(sub(end_of(&real)?, end_of(&original)?)) > SAME_SHAFT_TOLERANCE_MM
        {
            return None;
        }
        let edited = planned.shafts[index].clone().unwrap_or(original);
        let (shaft, stats) = splice_shaft(&real, &edited, frame, &windows, end_ring, rules)?;
        kept.shafts[index] = Some(shaft);
        kept.rings.add(stats);
    }
    Some(kept)
}

/// Windows sorted and joined where they touch.
fn merged_windows(windows: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut sorted: Vec<(f64, f64)> = windows
        .iter()
        .map(|&(a, b)| (a.min(b), a.max(b)))
        .collect();
    sorted.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(sorted.len());
    for window in sorted {
        match out.last_mut() {
            Some(last) if window.0 <= last.1 + ARC_EPSILON_MM => last.1 = last.1.max(window.1),
            _ => out.push(window),
        }
    }
    out
}

/// The piece of `s` (spanning arcs `arc`) between arcs `lo` and `hi`; its radius
/// at an end it was cut at is `radius_lo` / `radius_hi`, else the frustum's own.
fn clip_piece(
    s: &SupportPrimitive,
    arc: (f64, f64),
    (lo, hi): (f64, f64),
    (radius_lo, radius_hi): (Option<f64>, Option<f64>),
) -> Option<SupportPrimitive> {
    let span = arc.1 - arc.0;
    let at = |x: f64| lerp(pt(s.start_mm), pt(s.end_mm), ((x - arc.0) / span).clamp(0.0, 1.0));
    let mut piece = s.clone();
    let cut_start = lo > arc.0 + ARC_EPSILON_MM;
    let cut_end = hi < arc.1 - ARC_EPSILON_MM;
    if cut_start {
        piece.start_mm = fin(at(lo))?;
        if piece.segment_kind == SupportSegmentKind::ContactTaper {
            piece.segment_kind = SupportSegmentKind::Member;
            piece.contact_depth_mm = None;
        }
    }
    if cut_end {
        piece.end_mm = fin(at(hi))?;
    }
    if let Some(radius) = radius_lo.or_else(|| cut_start.then(|| radius_at(s, arc, lo))) {
        piece.start_radius_mm = PositiveFinite::new(radius).ok()?;
    }
    if let Some(radius) = radius_hi.or_else(|| cut_end.then(|| radius_at(s, arc, hi))) {
        piece.end_radius_mm = PositiveFinite::new(radius).ok()?;
    }
    Some(piece)
}

/// The radius the reference chain has at arc `x`: of the frustum whose arc is
/// nearest (a chain that ends at `x` answers with its last radius). `starts`
/// prefers the frustum that begins at `x`, else the one that ends there.
fn chain_radius_at(chain: &[SupportPrimitive], arcs: &[(f64, f64)], x: f64, starts: bool) -> Option<f64> {
    let mut best: Option<(f64, bool, f64)> = None;
    for (s, &arc) in chain.iter().zip(arcs) {
        let at = x.clamp(arc.0, arc.1);
        let distance = (x - at).abs();
        let on_the_edge = if starts { (at - arc.0).abs() <= ARC_EPSILON_MM } else { (at - arc.1).abs() <= ARC_EPSILON_MM };
        let better = best.is_none_or(|(d, edge, _)| {
            distance < d - ARC_EPSILON_MM || (distance <= d + ARC_EPSILON_MM && on_the_edge && !edge)
        });
        if better {
            best = Some((distance, on_the_edge, radius_at(s, arc, at)));
        }
    }
    best.map(|(_, _, radius)| radius)
}

/// The shaft `real` outside every window and `edited` (the reference's, as the
/// junction pass left it) inside: one chain, top-down. Where a window meets the
/// arm and the reference has a piece inside it (the thin run that replaces a
/// dropped collar), that piece starts (or ends) on the arm's radius there, so
/// the chain is continuous. Where a window holds no piece of the reference, its
/// edge is a hull ring: the arm's frustum there ends on the ring's centre and
/// radius, and the arm's own pieces taper from the ring out to its chord
/// ([`bring_to_ring`]); so is the arm's last frustum when a filled junction's
/// added hull takes its end ring (`end_ring`, no window). Everywhere else the
/// arm's own frusta are untouched.
fn splice_shaft(
    real: &[SupportPrimitive],
    edited: &[SupportPrimitive],
    frame: Frame,
    windows: &[(f64, f64)],
    end_ring: Option<EndRing>,
    rules: JunctionRules,
) -> Option<(Vec<SupportPrimitive>, RingStats)> {
    let real_arcs = arcs(frame, real);
    let edited_arcs = arcs(frame, edited);
    let mut placed: Vec<Placed> = Vec::with_capacity(real.len() + edited.len() + 2);
    for (s, &arc) in real.iter().zip(&real_arcs) {
        let mut spans: Vec<(f64, f64)> = Vec::new();
        let mut x = arc.0;
        for &(lo, hi) in windows {
            if hi <= x + ARC_EPSILON_MM || lo >= arc.1 - ARC_EPSILON_MM {
                continue;
            }
            if lo > x + ARC_EPSILON_MM {
                spans.push((x, lo));
            }
            x = x.max(hi);
        }
        if arc.1 - x > ARC_EPSILON_MM {
            spans.push((x, arc.1));
        }
        for (lo, hi) in spans {
            placed.push(Placed {
                lo,
                hi,
                piece: clip_piece(s, arc, (lo, hi), (None, None))?,
                outside: true,
            });
        }
    }
    for &(lo, hi) in windows {
        // The arm's radius where it meets the window: its piece that ends on
        // the window's top, and its piece that starts on the window's foot.
        let on_top = lo.is_finite().then(|| chain_radius_at(real, &real_arcs, lo, false)).flatten();
        let on_foot = hi.is_finite().then(|| chain_radius_at(real, &real_arcs, hi, true)).flatten();
        for (s, &arc) in edited.iter().zip(&edited_arcs) {
            let (from, to) = (arc.0.max(lo), arc.1.min(hi));
            if to - from > ARC_EPSILON_MM {
                let start = if (from - lo).abs() <= ARC_EPSILON_MM { on_top } else { None };
                let end = if (to - hi).abs() <= ARC_EPSILON_MM { on_foot } else { None };
                placed.push(Placed {
                    lo: from,
                    hi: to,
                    piece: clip_piece(s, arc, (from, to), (start, end))?,
                    outside: false,
                });
            }
        }
    }
    placed.sort_by(|x, y| x.lo.total_cmp(&y.lo));
    let mut stats = RingStats::default();
    let taper = TaperRules {
        lean_slope_per_arc: rules.cone_slope * frame.dir[2].abs(),
        pitch_mm: rules.pixel_pitch_mm,
    };
    let mut edges = ring_edges(edited, &edited_arcs, windows);
    // A filled junction's hull stands on the arm's end ring (no window): the
    // arm's last frustum ends on it. An end inside a window is the reference's.
    if let (Some(ring), Some(&(_, end_arc))) = (end_ring, real_arcs.last()) {
        let inside_window = windows
            .iter()
            .any(|&(lo, hi)| lo - ARC_EPSILON_MM <= end_arc && end_arc <= hi + ARC_EPSILON_MM);
        if !inside_window {
            edges.push(RingEdge {
                arc: end_arc,
                shaft_above: true,
                centre: ring.centre,
                radius: ring.radius_mm,
            });
        }
    }
    for edge in edges {
        match bring_to_ring(&mut placed, &edge, taper) {
            RingOutcome::Met { gap } => {
                stats.max_gap_mm = stats.max_gap_mm.max(gap.abs());
                if gap.abs() <= RING_EXACT_MM {
                    stats.exact += 1;
                } else if gap > 0.0 {
                    stats.thicker += 1;
                } else {
                    stats.thinner += 1;
                    stats.max_under_chord_mm = stats.max_under_chord_mm.max(-gap);
                }
            }
            RingOutcome::Unmet(why) => {
                stats.unmet += 1;
                match why {
                    Unmet::NoPiece => stats.unmet_no_piece += 1,
                    Unmet::NotAdjustable => stats.unmet_not_adjustable += 1,
                    Unmet::RanOut => stats.unmet_ran_out += 1,
                }
            }
        }
    }
    Some((placed.into_iter().map(|p| p.piece).collect(), stats))
}

/// Radius difference below which a ring and the arm's chord are one radius, mm.
const RING_EXACT_MM: f64 = 1.0e-9;
/// How far an edge of a window may stand from an end of the reference's piece
/// and still be that piece's end, mm (the two frames agree to
/// [`SAME_SHAFT_TOLERANCE_MM`]).
const RING_TOLERANCE_MM: f64 = 1.0e-6;
/// The part of the room under the lean law's step a taper may use.
const ROOM_SAFETY: f64 = 0.999;

/// A piece of the spliced shaft and the arcs it spans; `outside` for a piece
/// of the arm's own, not of the reference.
struct Placed {
    lo: f64,
    hi: f64,
    piece: SupportPrimitive,
    outside: bool,
}

/// A hull ring that stands on a cut of the shaft: the reference's piece ends
/// (`shaft_above`, the arm lies above the cut) or starts there, on `centre` and
/// `radius`, and nothing of the reference lies inside the window past it.
struct RingEdge {
    arc: f64,
    shaft_above: bool,
    centre: [Finite; 3],
    radius: f64,
}

/// The ring edges of `windows` on a shaft: an edge where the reference's
/// shaft (`edited`) has a piece that ends (the window's top) or starts (its
/// foot) there and no piece that runs on into the window.
fn ring_edges(
    edited: &[SupportPrimitive],
    edited_arcs: &[(f64, f64)],
    windows: &[(f64, f64)],
) -> Vec<RingEdge> {
    let mut edges = Vec::new();
    let near = |a: f64, b: f64| (a - b).abs() <= RING_TOLERANCE_MM;
    for &(lo, hi) in windows {
        if lo.is_finite() {
            let runs_on = edited_arcs
                .iter()
                .any(|&(a, b)| near(a, lo) && b > lo + ARC_EPSILON_MM);
            let ends = edited_arcs.iter().rposition(|&(_, b)| near(b, lo));
            if let (false, Some(i)) = (runs_on, ends) {
                edges.push(RingEdge {
                    arc: lo,
                    shaft_above: true,
                    centre: edited[i].end_mm,
                    radius: edited[i].end_radius_mm.get(),
                });
            }
        }
        if hi.is_finite() {
            let runs_on = edited_arcs
                .iter()
                .any(|&(a, b)| near(b, hi) && a < hi - ARC_EPSILON_MM);
            let starts = edited_arcs.iter().position(|&(a, _)| near(a, hi));
            if let (false, Some(i)) = (runs_on, starts) {
                edges.push(RingEdge {
                    arc: hi,
                    shaft_above: false,
                    centre: edited[i].start_mm,
                    radius: edited[i].start_radius_mm.get(),
                });
            }
        }
    }
    edges
}

/// The taper's two readings.
#[derive(Clone, Copy)]
struct TaperRules {
    /// The most a piece may narrow going up, mm of radius per mm of its arc.
    lean_slope_per_arc: f64,
    /// A piece shorter than this is below what the printer resolves.
    pitch_mm: f64,
}

enum RingOutcome {
    /// The arm's frustum at the edge now ends on the ring; `gap` is the ring's
    /// radius minus the arm's chord there.
    Met { gap: f64 },
    Unmet(Unmet),
}

/// Why a ring was not met.
#[derive(Clone, Copy)]
enum Unmet {
    NoPiece,
    NotAdjustable,
    RanOut,
}

/// One piece of the arm walking away from a ring: its arc length, and its
/// radius at the end nearer the ring and at the other end.
#[derive(Clone, Copy, Debug)]
struct Step {
    len: f64,
    near: f64,
    far: f64,
    /// Whether the piece may be cut in two (a contact taper or a trimmed piece
    /// keeps its length: its offset closes at its far end instead).
    splittable: bool,
}

/// What the taper does to one [`Step`]: the radius added at its near end and at
/// its far end, and, when the offset closes inside the step, how far from the
/// near end (the step is cut there, and the rest is untouched).
#[derive(Clone, Copy, Debug, PartialEq)]
struct StepPlan {
    near_offset: f64,
    far_offset: f64,
    split: Option<f64>,
}

/// **The taper from a hull ring out to the arm's own chord.** The radius to add
/// to the arm's pieces, walking away from the ring: `e0` at the ring (the ring's
/// radius minus the arm's chord there), closing to zero.
///
/// The offset closes at a rate of at most `|e0| / pitch` per mm of arc (a
/// frustum shorter than the printer's pixel is not drawn), and no faster than
/// the room a piece has: the piece's narrowing going up, plus the offset's own
/// change, stays within the lean law's step (`lean_slope_per_arc`), the limit
/// the load-step cones are drawn at. Where a piece is already at that limit
/// the offset is carried along it unchanged and closed on the next piece that
/// has room. A closing that would leave a piece shorter than a pixel (the
/// closing point too near either end of its piece) is spread over the whole
/// piece instead. `None` when the pieces run out before the offset closes.
fn plan_transition(steps: &[Step], e0: f64, shaft_above: bool, taper: TaperRules) -> Option<Vec<StepPlan>> {
    if taper.lean_slope_per_arc.is_nan()
        || taper.pitch_mm.is_nan()
        || taper.lean_slope_per_arc <= 0.0
        || taper.pitch_mm <= 0.0
        || !e0.is_finite()
    {
        return None;
    }
    let sign = e0.signum();
    let target_rate = e0.abs() / taper.pitch_mm;
    // Closing the offset raises the narrowing going up when the arm lies above
    // a thicker ring (the offset falls going up) or below a thinner one (it
    // rises going down); otherwise it lowers it.
    let raises_narrowing = (shaft_above && e0 > 0.0) || (!shaft_above && e0 < 0.0);
    let mut plan = Vec::with_capacity(steps.len().min(4));
    let mut offset = e0;
    for step in steps {
        if step.len.is_nan() || step.len <= 0.0 {
            return None;
        }
        let narrowing = if shaft_above {
            (step.near - step.far) / step.len
        } else {
            (step.far - step.near) / step.len
        };
        let room = if raises_narrowing {
            taper.lean_slope_per_arc - narrowing
        } else {
            taper.lean_slope_per_arc + narrowing
        };
        let rate = target_rate.min((room * ROOM_SAFETY).max(0.0));
        if rate <= 0.0 {
            plan.push(StepPlan {
                near_offset: offset,
                far_offset: offset,
                split: None,
            });
            continue;
        }
        let magnitude = offset.abs();
        let reach = magnitude / rate;
        if reach > step.len * (1.0 + 1.0e-12) {
            let far = sign * (magnitude - rate * step.len);
            plan.push(StepPlan {
                near_offset: offset,
                far_offset: far,
                split: None,
            });
            offset = far;
            continue;
        }
        let split = (step.splittable && reach >= taper.pitch_mm && step.len - reach >= taper.pitch_mm)
            .then_some(reach);
        plan.push(StepPlan {
            near_offset: offset,
            far_offset: 0.0,
            split,
        });
        return Some(plan);
    }
    None
}

/// Draw the arm's frustum at a hull ring on the ring's centre and radius, and
/// taper the arm's own pieces from there to its chord ([`plan_transition`]).
/// Nothing changes (`Unmet`) when no piece of the arm stands at the edge, a piece
/// in the way is not a plain member, or the pieces run out first.
fn bring_to_ring(placed: &mut Vec<Placed>, edge: &RingEdge, taper: TaperRules) -> RingOutcome {
    let near_index = if edge.shaft_above {
        placed
            .iter()
            .rposition(|p| p.outside && (p.hi - edge.arc).abs() <= RING_TOLERANCE_MM)
    } else {
        placed
            .iter()
            .position(|p| p.outside && (p.lo - edge.arc).abs() <= RING_TOLERANCE_MM)
    };
    let Some(near_index) = near_index else {
        return RingOutcome::Unmet(Unmet::NoPiece);
    };
    // A piece a taper may change the radii of: a member or the contact taper
    // (never a sphere). Only a plain member may be cut in two.
    let plain = |p: &Placed| {
        p.outside
            && matches!(
                p.piece.segment_kind,
                SupportSegmentKind::Member | SupportSegmentKind::ContactTaper
            )
            && p.piece.sphere_geometry.is_none()
    };
    let splittable = |p: &Placed| {
        p.piece.segment_kind == SupportSegmentKind::Member && p.piece.seat_trim.is_none()
    };
    // The pieces walking away from the ring, while they join end to end.
    let mut chain: Vec<usize> = vec![near_index];
    loop {
        let last = chain[chain.len() - 1];
        let next = if edge.shaft_above {
            last.checked_sub(1)
        } else {
            Some(last + 1).filter(|&n| n < placed.len())
        };
        let Some(next) = next else { break };
        let gap = if edge.shaft_above {
            placed[last].lo - placed[next].hi
        } else {
            placed[next].lo - placed[last].hi
        };
        if !(plain(&placed[next]) && gap.abs() <= RING_TOLERANCE_MM) {
            break;
        }
        chain.push(next);
    }
    if !plain(&placed[near_index]) {
        return RingOutcome::Unmet(Unmet::NotAdjustable);
    }
    let steps: Vec<Step> = chain
        .iter()
        .map(|&i| {
            let p = &placed[i];
            let (start, end) = (p.piece.start_radius_mm.get(), p.piece.end_radius_mm.get());
            let (near, far) = if edge.shaft_above { (end, start) } else { (start, end) };
            Step {
                len: p.hi - p.lo,
                near,
                far,
                splittable: splittable(p),
            }
        })
        .collect();
    let gap = edge.radius - steps[0].near;
    if gap.abs() > RING_EXACT_MM {
        let Some(plan) = plan_transition(&steps, gap, edge.shaft_above, taper) else {
            return RingOutcome::Unmet(Unmet::RanOut);
        };
        let mut replaced: Vec<(usize, Vec<Placed>)> = Vec::with_capacity(plan.len());
        for (k, step_plan) in plan.iter().enumerate() {
            let Some(pieces) = retapered(&placed[chain[k]], step_plan, edge.shaft_above) else {
                return RingOutcome::Unmet(Unmet::NotAdjustable);
            };
            replaced.push((chain[k], pieces));
        }
        let mut next: Vec<Placed> = Vec::with_capacity(placed.len() + 1);
        let mut by_index: Vec<Option<Vec<Placed>>> = (0..placed.len()).map(|_| None).collect();
        for (i, pieces) in replaced {
            by_index[i] = Some(pieces);
        }
        for (i, p) in std::mem::take(placed).into_iter().enumerate() {
            match by_index[i].take() {
                Some(pieces) => next.extend(pieces),
                None => next.push(p),
            }
        }
        *placed = next;
    }
    // The frustum at the ring ends on the ring's centre and radius, to the bit.
    let at = if edge.shaft_above {
        placed
            .iter()
            .rposition(|p| p.outside && (p.hi - edge.arc).abs() <= RING_TOLERANCE_MM)
    } else {
        placed
            .iter()
            .position(|p| p.outside && (p.lo - edge.arc).abs() <= RING_TOLERANCE_MM)
    };
    let Some(at) = at else {
        return RingOutcome::Unmet(Unmet::NoPiece);
    };
    let Ok(radius) = PositiveFinite::new(edge.radius) else {
        return RingOutcome::Unmet(Unmet::NotAdjustable);
    };
    let piece = &mut placed[at].piece;
    if edge.shaft_above {
        piece.end_mm = edge.centre;
        piece.end_radius_mm = radius;
    } else {
        piece.start_mm = edge.centre;
        piece.start_radius_mm = radius;
    }
    RingOutcome::Met { gap }
}

/// The piece of the arm with the taper step applied: its radii moved by the
/// step's offsets and, when the step closes inside it, cut there in two.
fn retapered(placed: &Placed, plan: &StepPlan, shaft_above: bool) -> Option<Vec<Placed>> {
    let piece = &placed.piece;
    let (start_r, end_r) = (piece.start_radius_mm.get(), piece.end_radius_mm.get());
    let positive = |r: f64| PositiveFinite::new(r).ok();
    let Some(distance) = plan.split else {
        let mut whole = piece.clone();
        if shaft_above {
            whole.end_radius_mm = positive(end_r + plan.near_offset)?;
            whole.start_radius_mm = positive(start_r + plan.far_offset)?;
        } else {
            whole.start_radius_mm = positive(start_r + plan.near_offset)?;
            whole.end_radius_mm = positive(end_r + plan.far_offset)?;
        }
        return Some(vec![Placed {
            lo: placed.lo,
            hi: placed.hi,
            piece: whole,
            outside: true,
        }]);
    };
    let span = placed.hi - placed.lo;
    let cut_arc = if shaft_above { placed.hi - distance } else { placed.lo + distance };
    let t = ((cut_arc - placed.lo) / span).clamp(0.0, 1.0);
    let cut_point = fin(lerp(pt(piece.start_mm), pt(piece.end_mm), t))?;
    let cut_radius = positive(start_r + (end_r - start_r) * t)?;
    let (mut upper, mut lower) = (piece.clone(), piece.clone());
    upper.end_mm = cut_point;
    upper.end_radius_mm = cut_radius;
    lower.start_mm = cut_point;
    lower.start_radius_mm = cut_radius;
    if shaft_above {
        // The near part is the lower one.
        lower.end_radius_mm = positive(end_r + plan.near_offset)?;
    } else {
        upper.start_radius_mm = positive(start_r + plan.near_offset)?;
    }
    Some(vec![
        Placed {
            lo: placed.lo,
            hi: cut_arc,
            piece: upper,
            outside: true,
        },
        Placed {
            lo: cut_arc,
            hi: placed.hi,
            piece: lower,
            outside: true,
        },
    ])
}

/// Why a junction could not be cut into its host.
#[derive(Clone, Copy)]
enum Skip {
    Crowded,
    Undrawable,
}

fn ring_of(s: &SupportPrimitive, at_start: bool) -> HullRing {
    HullRing {
        frustum_start_mm: s.start_mm,
        frustum_end_mm: s.end_mm,
        radius_mm: if at_start {
            s.start_radius_mm
        } else {
            s.end_radius_mm
        },
        at_start,
        plate_z: s.ring_normal == FrustumRingNormal::PlateZ,
    }
}

/// The hull record of up to three rings: the convex hull of their corners, as a
/// `Member` record whose start and end are the centres of the two largest rings.
/// `None` when a ring cannot be made, the hull is flat, or the two largest rings
/// share a centre.
fn hull_record(rings: [Option<HullRing>; 3], sides: usize) -> Option<SupportPrimitive> {
    let mut points: Vec<V> = Vec::with_capacity(3 * sides);
    for ring in rings.iter().flatten() {
        points.extend(frustum_ring(
            pt(ring.frustum_start_mm),
            pt(ring.frustum_end_mm),
            ring.radius_mm.get(),
            ring.at_start,
            ring.plate_z,
            sides,
        )?);
    }
    if convex_hull(&points)?.volume() <= 0.0 {
        return None;
    }
    // The record's own start and end: the centres of the two largest rings.
    let mut present: Vec<&HullRing> = rings.iter().flatten().collect();
    present.sort_by(|x, y| y.radius_mm.get().total_cmp(&x.radius_mm.get()));
    let centre = |r: &HullRing| {
        if r.at_start {
            r.frustum_start_mm
        } else {
            r.frustum_end_mm
        }
    };
    let (first, second) = (*present.first()?, *present.get(1)?);
    if centre(first) == centre(second) {
        return None;
    }
    Some(SupportPrimitive {
        segment_kind: SupportSegmentKind::Member,
        start_mm: centre(first),
        end_mm: centre(second),
        start_radius_mm: first.radius_mm,
        end_radius_mm: second.radius_mm,
        contact_depth_mm: None,
        ring_normal: FrustumRingNormal::Junction(JunctionRings(rings)),
        sphere_geometry: None,
        seat_trim: None,
    })
}

/// What an ADDED hull is made from: the branch as it stands, the host as it
/// stands, and how far above and below the landing point the host's sections are
/// taken (the junction's own pull-backs, never less than one pixel pitch, `None`
/// where the host does not continue that way).
struct FillRings<'a> {
    branch: &'a SupportPrimitive,
    host: &'a [SupportPrimitive],
    host_arcs: &'a [(f64, f64)],
    frame: Frame,
    s_k: f64,
    above: Option<f64>,
    below: Option<f64>,
}

impl FillRings<'_> {
    /// The host's section at `arc`, drawn the way the host's own frustum draws its
    /// end rings: a virtual frustum on the host's axis that ends (above) or starts
    /// (below) there, at the host's radius there.
    fn host_ring(&self, arc: f64, above: bool) -> Option<HullRing> {
        let tolerance = 1.0e-12;
        let index = if above {
            (0..self.host.len())
                .find(|&i| self.host_arcs[i].0 < arc && arc <= self.host_arcs[i].1 + tolerance)
        } else {
            (0..self.host.len())
                .find(|&i| self.host_arcs[i].0 - tolerance <= arc && arc < self.host_arcs[i].1)
        }?;
        let s = &self.host[index];
        let (a0, a1) = self.host_arcs[index];
        let at = lerp(
            pt(s.start_mm),
            pt(s.end_mm),
            ((arc - a0) / (a1 - a0)).clamp(0.0, 1.0),
        );
        let radius = PositiveFinite::new(radius_at(s, self.host_arcs[index], arc)).ok()?;
        let plate_z = s.ring_normal == FrustumRingNormal::PlateZ;
        Some(if above {
            HullRing {
                frustum_start_mm: s.start_mm,
                frustum_end_mm: fin(at)?,
                radius_mm: radius,
                at_start: false,
                plate_z,
            }
        } else {
            HullRing {
                frustum_start_mm: fin(at)?,
                frustum_end_mm: s.end_mm,
                radius_mm: radius,
                at_start: true,
                plate_z,
            }
        })
    }

    fn record(&self, sides: usize) -> Option<SupportPrimitive> {
        let above = self.above.and_then(|t| self.host_ring(self.s_k - t, true));
        let below = self.below.and_then(|t| self.host_ring(self.s_k + t, false));
        hull_record([Some(ring_of(self.branch, false)), below, above], sides)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hull::ConvexHull;

    const SIDES: usize = 24;
    const PITCH_MM: f64 = 0.019;
    const LAYER_MM: f64 = 0.05;
    /// cot(32.5 degrees): the lean law's step per mm of height.
    const CONE_SLOPE: f64 = 1.5697;

    fn rules() -> JunctionRules {
        JunctionRules {
            pixel_pitch_mm: PITCH_MM,
            layer_height_mm: LAYER_MM,
            sides: SIDES,
            cone_slope: CONE_SLOPE,
        }
    }

    fn frustum(kind: SupportSegmentKind, start: V, end: V, r0: f64, r1: f64) -> SupportPrimitive {
        SupportPrimitive {
            segment_kind: kind,
            start_mm: fin(start).expect("finite"),
            end_mm: fin(end).expect("finite"),
            start_radius_mm: PositiveFinite::new(r0).expect("positive"),
            end_radius_mm: PositiveFinite::new(r1).expect("positive"),
            contact_depth_mm: (kind == SupportSegmentKind::ContactTaper)
                .then(|| PositiveFinite::new(0.1).expect("positive")),
            ring_normal: FrustumRingNormal::SegmentAxis,
            sphere_geometry: None,
            seat_trim: None,
        }
    }

    /// A vertical host `[taper r_thin down to z_collar, collar cone to r_wide over
    /// 0.35 mm, wide post]` and a branch landing `drop` below the collar's foot,
    /// leaning `lean_deg` from vertical, at radius `r_branch`.
    fn junction(
        r_thin: f64,
        r_wide: f64,
        r_branch: f64,
        lean_deg: f64,
        drop: f64,
    ) -> (Vec<SupportPrimitive>, Vec<JunctionArm>, V) {
        let top = -13.9;
        let collar_top = -16.4859;
        let collar_foot = collar_top - 0.35;
        let k = [0.0, 0.0, collar_foot - drop];
        let host = [
            frustum(
                SupportSegmentKind::ContactTaper,
                [0.0, 0.0, top],
                [0.0, 0.0, collar_top],
                r_thin,
                r_thin,
            ),
            frustum(
                SupportSegmentKind::Member,
                [0.0, 0.0, collar_top],
                [0.0, 0.0, collar_foot],
                r_thin,
                r_wide,
            ),
            frustum(
                SupportSegmentKind::Member,
                [0.0, 0.0, collar_foot],
                [0.0, 0.0, -18.79],
                r_wide,
                r_wide,
            ),
        ];
        let lean = lean_deg.to_radians();
        let length = 3.2;
        let branch_start = [k[0] + length * lean.sin(), k[1], k[2] + length * lean.cos()];
        let branch = frustum(
            SupportSegmentKind::Member,
            branch_start,
            k,
            r_branch,
            r_branch,
        );
        let mut primitives = host.to_vec();
        primitives.push(branch);
        let arms = vec![
            JunctionArm {
                landed: false,
                shaft: Some((0, 3)),
            },
            JunctionArm {
                landed: true,
                shaft: Some((3, 1)),
            },
        ];
        (primitives, arms, k)
    }

    fn hull_of(primitive: &SupportPrimitive) -> ConvexHull {
        let FrustumRingNormal::Junction(rings) = primitive.ring_normal else {
            panic!("not a hull record");
        };
        let mut points = Vec::new();
        for ring in rings.rings() {
            points.extend(
                frustum_ring(
                    pt(ring.frustum_start_mm),
                    pt(ring.frustum_end_mm),
                    ring.radius_mm.get(),
                    ring.at_start,
                    ring.plate_z,
                    SIDES,
                )
                .expect("ring"),
            );
        }
        convex_hull(&points).expect("hull")
    }

    /// The volume of the hull of the branch ring and the host-below ring alone
    /// (slots 0 and 1), the figure the plan quotes.
    fn branch_and_below_volume(primitive: &SupportPrimitive) -> f64 {
        let FrustumRingNormal::Junction(rings) = primitive.ring_normal else {
            panic!("not a hull record");
        };
        let points: Vec<V> = rings.0[..2]
            .iter()
            .flatten()
            .flat_map(|ring| {
                frustum_ring(
                    pt(ring.frustum_start_mm),
                    pt(ring.frustum_end_mm),
                    ring.radius_mm.get(),
                    ring.at_start,
                    ring.plate_z,
                    SIDES,
                )
                .expect("ring")
            })
            .collect();
        convex_hull(&points).expect("hull").volume()
    }

    fn closed_and_outward(hull: &ConvexHull) -> bool {
        let mut edges: std::collections::HashMap<(usize, usize), usize> =
            std::collections::HashMap::new();
        for face in &hull.triangles {
            for (a, b) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])] {
                *edges.entry((a, b)).or_default() += 1;
            }
        }
        let closed = edges
            .iter()
            .all(|(&(a, b), &count)| count == 1 && edges.get(&(b, a)) == Some(&1));
        closed && hull.volume() > 0.0
    }

    fn inside_hull(hull: &ConvexHull, p: V, slack: f64) -> bool {
        hull.triangles.iter().all(|face| {
            let [a, b, c] = face.map(|i| hull.vertices[i]);
            let normal = normalize(geom_kernel::vec3::cross(sub(b, a), sub(c, a)));
            normal.is_none_or(|n| dot(n, sub(p, a)) <= slack)
        })
    }

    #[test]
    fn v19_401_over_295_is_one_closed_outward_hull_with_the_neighbours_rings() {
        // Branch r 0.761 at 41 deg onto a host of r 0.761 that narrows above
        // to 0.2342 through a 0.35 mm collar ending 0.05 mm above the landing.
        let (primitives, arms, k) = junction(0.2342, 0.761, 0.761, 41.0, 0.05);
        let out = hull_junctions(&primitives, &arms, rules());
        assert_eq!(out.hulled, 1, "{out:?}");
        let hull_record = &out.hulls[1][0];
        let hull = hull_of(hull_record);
        assert!(
            closed_and_outward(&hull),
            "closed 2-manifold, positive volume"
        );
        let volume = hull.volume();
        let two_rings = branch_and_below_volume(hull_record);
        println!(
            "401/295 hull volume {volume:.4} mm3 (branch and host below alone {two_rings:.4})"
        );
        assert!(
            (0.89..0.91).contains(&two_rings),
            "the plan expects about 0.90 mm3 for the branch and the host below, got {two_rings}"
        );
        assert!(
            volume >= two_rings && volume < two_rings * 1.03,
            "the thin ring of the host above is buried in the hull: {volume} vs {two_rings}"
        );
        // The collar is gone: the host above is thin all the way to its ring,
        // the host below starts wide.
        let host = out.shafts[0].as_ref().expect("host edited");
        assert!(
            host.iter().all(|s| !widening_down(s)),
            "no collar cone is left on the host"
        );
        let upper = host
            .iter()
            .rev()
            .find(|s| s.end_radius_mm.get() < 0.5)
            .expect("a thin upper host");
        assert!((upper.end_radius_mm.get() - 0.2342).abs() < 1e-12);
        assert!(
            norm(sub(pt(upper.end_mm), k)) < 1e-9,
            "the host above runs on, thin, to a ring at the landing point"
        );
        // Every ring of the hull equals a neighbour frustum's own ring, bit for bit.
        let FrustumRingNormal::Junction(rings) = hull_record.ring_normal else {
            panic!("hull");
        };
        let branch = out.shafts[1].as_ref().expect("branch edited");
        let neighbours: Vec<&SupportPrimitive> = host.iter().chain(branch.iter()).collect();
        for ring in rings.rings() {
            let owner = neighbours
                .iter()
                .find(|s| {
                    s.start_mm == ring.frustum_start_mm
                        && s.end_mm == ring.frustum_end_mm
                        && (if ring.at_start {
                            s.start_radius_mm
                        } else {
                            s.end_radius_mm
                        }) == ring.radius_mm
                })
                .expect("the ring names a drawn frustum");
            let mine = frustum_ring(
                pt(owner.start_mm),
                pt(owner.end_mm),
                ring.radius_mm.get(),
                ring.at_start,
                ring.plate_z,
                SIDES,
            )
            .expect("ring");
            let buried_thin_ring = ring.radius_mm.get() < 0.5;
            for corner in &mine {
                // The branch and host-below rings ARE the hull's boundary; the
                // thin ring of the host above is buried in it.
                assert!(
                    inside_hull(&hull, *corner, 1e-9)
                        && (buried_thin_ring
                            || hull
                                .vertices
                                .iter()
                                .any(|v| v.map(f64::to_bits) == corner.map(f64::to_bits))),
                    "a ring corner is a hull vertex, bit for bit"
                );
            }
        }
        // No corner of the branch's end ring lies outside every solid: the hull
        // or the host. (v19: 12 of the Cylinder's landed branches had corners
        // 0.2 mm or more outside.)
        let branch_end = branch.last().expect("the branch");
        let end_ring = frustum_ring(
            pt(branch_end.start_mm),
            pt(branch_end.end_mm),
            branch_end.end_radius_mm.get(),
            false,
            false,
            SIDES,
        )
        .expect("ring");
        let flat = (std::f64::consts::PI / 24.0).cos();
        for corner in end_ring {
            assert!(
                inside_hull(&hull, corner, 1e-9) || inside_chain(corner, host, flat),
                "branch ring corner {corner:?} stands outside every solid"
            );
        }
        // The old flat cap would have stood out of the host by 0.2 mm or more.
        let old_cap = frustum_ring(
            pt(primitives[3].start_mm),
            pt(primitives[3].end_mm),
            0.761,
            false,
            false,
            SIDES,
        )
        .expect("ring");
        let old_host = &primitives[..3];
        // How far a corner stands from the host's surface, where the host's own
        // axis passes it (its projection falls inside a span).
        let excess = |corner: V| -> f64 {
            old_host
                .iter()
                .filter_map(|s| {
                    let (a, b) = (pt(s.start_mm), pt(s.end_mm));
                    let axis = sub(b, a);
                    let t = dot(sub(corner, a), axis) / dot(axis, axis);
                    (0.0..=1.0).contains(&t).then(|| {
                        let (distance, _, radius) = locate(corner, s);
                        distance - radius
                    })
                })
                .fold(f64::NEG_INFINITY, f64::max)
        };
        let proud = old_cap
            .iter()
            .map(|&corner| excess(corner))
            .fold(0.0_f64, f64::max);
        assert!(
            proud >= 0.2,
            "red before: the v19 cap rim stood {proud} mm out of the host"
        );
    }

    #[test]
    fn v19_423_over_352_hull_volume_matches_the_plan() {
        // Branch r 0.965 at 56.7 deg onto a host r 0.965 whose top is 0.05 mm
        // above the landing (no collar: the host starts at the joint).
        let k = [0.0, 0.0, -15.4901];
        let lean = 56.7_f64.to_radians();
        let host = frustum(
            SupportSegmentKind::Member,
            [0.0, 0.0, -15.4401],
            [0.0, 0.0, -18.79],
            0.9655,
            0.9655,
        );
        let branch = frustum(
            SupportSegmentKind::Member,
            [3.0 * lean.sin(), 0.0, k[2] + 3.0 * lean.cos()],
            k,
            0.9655,
            0.9655,
        );
        let arms = vec![
            JunctionArm {
                landed: false,
                shaft: Some((0, 1)),
            },
            JunctionArm {
                landed: true,
                shaft: Some((1, 1)),
            },
        ];
        let out = hull_junctions(&[host, branch], &arms, rules());
        assert_eq!(out.hulled, 1, "{out:?}");
        let hull = hull_of(&out.hulls[1][0]);
        assert!(closed_and_outward(&hull));
        let two_rings = branch_and_below_volume(&out.hulls[1][0]);
        println!(
            "423/352 hull volume {:.4} mm3 (branch and host below alone {two_rings:.4})",
            hull.volume()
        );
        assert!(
            (2.30..2.37).contains(&two_rings),
            "the plan expects about 2.34 mm3 for the branch and the host below, got {two_rings}"
        );
        assert!(hull.volume() >= two_rings);
    }

    #[test]
    fn a_crowded_junction_is_filled_with_an_added_hull_not_left_as_a_cap() {
        // Two branches land 0.1 mm apart on one vertical host: the first takes the
        // arc, the second cannot be cut into the host. v19 and the first version of
        // this pass left it as a flat cap whose rim stands out of the host.
        let host = frustum(
            SupportSegmentKind::Member,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, -10.0],
            0.9,
            0.9,
        );
        let lean = 40.0_f64.to_radians();
        let branch = |x_sign: f64, z: f64, r: f64| {
            frustum(
                SupportSegmentKind::Member,
                [x_sign * 3.0 * lean.sin(), 0.0, z + 3.0 * lean.cos()],
                [0.0, 0.0, z],
                r,
                r,
            )
        };
        let primitives = vec![host, branch(1.0, -5.0, 0.9), branch(-1.0, -5.1, 0.8)];
        let arms = vec![
            JunctionArm {
                landed: false,
                shaft: Some((0, 1)),
            },
            JunctionArm {
                landed: true,
                shaft: Some((1, 1)),
            },
            JunctionArm {
                landed: true,
                shaft: Some((2, 1)),
            },
        ];
        let out = hull_junctions(&primitives, &arms, rules());
        assert_eq!(out.hulled, 1, "{out:?}");
        assert_eq!(out.filled, 1, "the crowded junction is filled: {out:?}");
        assert_eq!(out.crowded + out.undrawable, 0, "{out:?}");
        // The thicker branch (arm 1) is cut in first; the thinner one is filled.
        let filled_arm = 2;
        assert_eq!(out.hulls[1].len(), 1, "{out:?}");
        let filled = out.hulls[filled_arm].first().expect("the added hull");
        let hull = hull_of(filled);
        assert!(
            closed_and_outward(&hull),
            "added hull is closed and outward"
        );
        assert!(
            out.shafts[filled_arm].is_none(),
            "the filled branch is not cut"
        );
        // Every corner of the filled branch's end ring is inside its added hull or
        // inside the host the junction left.
        let host_after = out.shafts[0]
            .as_ref()
            .expect("host edited by the first hull");
        let flat = (std::f64::consts::PI / 24.0).cos();
        let end = &primitives[filled_arm];
        for corner in frustum_ring(
            pt(end.start_mm),
            pt(end.end_mm),
            end.end_radius_mm.get(),
            false,
            false,
            SIDES,
        )
        .expect("ring")
        {
            assert!(
                inside_hull(&hull, corner, 1e-9) || inside_chain(corner, host_after, flat),
                "ring corner {corner:?} of the crowded branch stands outside every solid"
            );
        }
    }

    /// A FILLED junction has no window, but its added hull stands on the branch's
    /// end ring: the kept hull is met there on the reference's radius too.
    #[test]
    fn a_filled_hulls_end_ring_is_met_on_the_reference_radius() {
        let lean = 40.0_f64.to_radians();
        let field_of = |host_r: f64, filled_r: f64| {
            let branch = |x_sign: f64, z: f64, r: f64| {
                frustum(
                    SupportSegmentKind::Member,
                    [x_sign * 3.0 * lean.sin(), 0.0, z + 3.0 * lean.cos()],
                    [0.0, 0.0, z],
                    r,
                    r,
                )
            };
            vec![
                frustum(SupportSegmentKind::Member, [0.0, 0.0, 0.0], [0.0, 0.0, -10.0], host_r, host_r),
                branch(1.0, -5.0, 0.9),
                branch(-1.0, -5.1, filled_r),
            ]
        };
        let arms = vec![
            JunctionArm { landed: false, shaft: Some((0, 1)) },
            JunctionArm { landed: true, shaft: Some((1, 1)) },
            JunctionArm { landed: true, shaft: Some((2, 1)) },
        ];
        let (reference, field) = (field_of(0.9, 0.8), field_of(0.9, 0.76));
        let planned = hull_junctions(&reference, &arms, rules());
        assert_eq!(planned.filled, 1, "{planned:?}");
        let kept = keep_planned_hulls(&field, &arms, &reference, &arms, rules()).expect("the shafts line up");
        assert_eq!(kept.hulls, planned.hulls, "the filled hull is the reference's, bit for bit");
        let shaft = kept.shafts[2].as_ref().expect("the filled branch's shaft is met on the ring");
        let last = shaft.last().expect("a piece");
        assert_eq!(last.end_mm, reference[2].end_mm, "ends on the ring's centre");
        assert_eq!(last.end_radius_mm, reference[2].end_radius_mm, "ends on the reference's radius, to the bit");
        assert_eq!(shaft[0].start_radius_mm, field[2].start_radius_mm, "and is the field's own chord away from the ring");
        assert_eq!((kept.rings.thicker, kept.rings.unmet), (1, 0), "{:?}", kept.rings);
        // The same field with equal radii edits nothing there.
        let same = keep_planned_hulls(&reference, &arms, &reference, &arms, rules()).expect("lines up");
        assert_eq!(
            (same.rings.thicker, same.rings.thinner, same.rings.unmet),
            (0, 0, 0),
            "{:?}",
            same.rings
        );
        assert_eq!(same.rings.exact, kept.rings.exact + 1, "{:?} {:?}", same.rings, kept.rings);
    }

    fn ring_centre(ring: &HullRing) -> V {
        if ring.at_start {
            pt(ring.frustum_start_mm)
        } else {
            pt(ring.frustum_end_mm)
        }
    }

    /// R8: the hulls are the REFERENCE field's, record for record, on a field
    /// whose radii moved; the shaft meets each hull ring on the ring's radius.
    #[test]
    fn the_planned_hulls_are_kept_on_a_field_whose_radii_moved() {
        // The reference: the v19 401 over 295 junction. The field: the same
        // geometry with a thinner post and a thinner branch.
        let (reference, reference_arms, _) = junction(0.2342, 0.761, 0.761, 41.0, 0.05);
        let (field, field_arms, _) = junction(0.2342, 0.70, 0.66, 41.0, 0.05);
        let planned = hull_junctions(&reference, &reference_arms, rules());
        assert_eq!(planned.hulled, 1, "{planned:?}");
        let alone = hull_junctions(&field, &field_arms, rules());
        // SABOTAGE: the pass run on the moved field makes a different hull.
        assert_ne!(
            alone.hulls[1], planned.hulls[1],
            "the moved radii must move the hull, or this test proves nothing"
        );
        let kept = keep_planned_hulls(&field, &field_arms, &reference, &reference_arms, rules())
            .expect("the shafts line up");
        assert_eq!(kept.hulls, planned.hulls, "every hull record is the reference's, bit for bit");
        assert_eq!((kept.hulled, kept.filled), (planned.hulled, planned.filled));
        // Every ring of a kept hull is met by a drawn frustum end on the ring's
        // centre AND radius, to the bit (the arcs and the rings are the
        // reference's), though the field's radii are not the reference's.
        let host = kept.shafts[0].as_ref().expect("host kept");
        let branch = kept.shafts[1].as_ref().expect("branch kept");
        let FrustumRingNormal::Junction(rings) = kept.hulls[1][0].ring_normal else {
            panic!("a hull");
        };
        for ring in rings.rings() {
            let centre = ring_centre(ring);
            let found = host.iter().chain(branch.iter()).any(|s| {
                let (end, radius) = if ring.at_start {
                    (s.start_mm, s.start_radius_mm)
                } else {
                    (s.end_mm, s.end_radius_mm)
                };
                norm(sub(pt(end), centre)) < 1.0e-9 && radius == ring.radius_mm
            });
            assert!(found, "ring at {centre:?} is not met on its own radius {}", ring.radius_mm.get());
        }
        assert!(kept.rings.thicker >= 1 && kept.rings.unmet == 0, "{:?}", kept.rings);
        // Outside the junction the arm is its own: the host's last frustum
        // keeps the field's 0.70 where the reference had 0.761, and between the
        // ring (0.761) and there the host tapers down: nothing narrows going up
        // faster than the lean law's step.
        let last = host.last().expect("host");
        assert!((last.end_radius_mm.get() - 0.70).abs() < 1.0e-12, "{}", last.end_radius_mm.get());
        let below = host
            .iter()
            .find(|s| (pt(s.start_mm)[2] - ring_centre(rings.rings().nth(1).expect("ring"))[2]).abs() < 1.0e-9)
            .expect("the host below starts on its ring");
        assert!(
            (below.start_radius_mm.get() - 0.761).abs() < 1.0e-12,
            "it starts on the ring's radius"
        );
        for s in host.iter().chain(branch.iter()) {
            let length = norm(sub(pt(s.end_mm), pt(s.start_mm)));
            let narrowing_up = (s.end_radius_mm.get() - s.start_radius_mm.get()) / length;
            assert!(
                narrowing_up <= CONE_SLOPE * (1.0 + 1.0e-9),
                "a piece narrows going up by {narrowing_up} per mm: {s:?}"
            );
        }
        // The chain is unbroken top to bottom around the cut: the host's
        // frusta that stand above the ring end where the next one starts.
        let upper_ring = ring_centre(rings.rings().nth(2).expect("ring"));
        assert!(
            host.iter().any(|s| norm(sub(pt(s.end_mm), upper_ring)) < 1.0e-9),
            "the host above runs to its ring"
        );
        for pair in host.windows(2) {
            if norm(sub(pt(pair[0].end_mm), pt(pair[1].start_mm))) < 1.0e-9 {
                assert!(
                    (pair[0].end_radius_mm.get() - pair[1].start_radius_mm.get()).abs() < 1.0e-12,
                    "the host is not continuous in radius: {pair:?}"
                );
            }
        }
        // A shaft that is not the reference's line is not used.
        let (mut moved, moved_arms, _) = junction(0.2342, 0.70, 0.66, 41.0, 0.05);
        moved[0].start_mm = fin([0.01, 0.0, -13.9]).expect("finite");
        assert!(keep_planned_hulls(&moved, &moved_arms, &reference, &reference_arms, rules()).is_none());
    }

    fn taper() -> TaperRules {
        TaperRules {
            lean_slope_per_arc: CONE_SLOPE,
            pitch_mm: PITCH_MM,
        }
    }

    /// The narrowing going up, mm of radius per mm of arc, of a step after a plan
    /// (an unsplit step: both ends moved).
    fn narrowing_after(step: &Step, plan: &StepPlan, shaft_above: bool) -> f64 {
        let (near, far) = (step.near + plan.near_offset, step.far + plan.far_offset);
        if shaft_above {
            (near - far) / step.len
        } else {
            (far - near) / step.len
        }
    }

    /// The taper never narrows a piece going up faster than the lean law's step
    /// (where it started within it), on either side of the ring, for a ring
    /// thicker or thinner than the chord; a piece already at the limit carries the
    /// offset unchanged; the offset closes; and no piece it cuts is shorter than a
    /// pixel.
    #[test]
    fn the_ring_taper_stays_within_the_lean_laws_step() {
        let pieces = [
            // radius at the near end, at the far end, arc length
            (0.60, 0.60 - CONE_SLOPE * 0.10, 0.10), // a cone at the limit, narrowing away from a ring above
            (0.60 - CONE_SLOPE * 0.10, 0.30, 0.50),
            (0.30, 0.30, 1.0),
        ];
        let mut closed = 0;
        for shaft_above in [true, false] {
            for gap in [0.004, -0.004, 0.06, -0.06, 1.0e-5] {
                // Walking away from the ring: above the ring the far end is the
                // top (narrower), below it the far end is the bottom (wider).
                let steps: Vec<Step> = pieces
                    .iter()
                    .map(|&(near, far, len)| {
                        if shaft_above {
                            Step { len, near, far, splittable: true }
                        } else {
                            Step { len, near: far, far: near, splittable: true }
                        }
                    })
                    .collect();
                let plan = plan_transition(&steps, gap, shaft_above, taper())
                    .unwrap_or_else(|| panic!("above {shaft_above}, gap {gap}: pieces ran out"));
                closed += 1;
                let last = plan.last().expect("a plan");
                assert!(last.far_offset.abs() < 1.0e-15, "the offset closes");
                let mut previous = gap;
                for (step, step_plan) in steps.iter().zip(&plan) {
                    assert!((step_plan.near_offset - previous).abs() < 1.0e-12, "continuous");
                    previous = step_plan.far_offset;
                    if step_plan.split.is_none() {
                        let after = narrowing_after(step, step_plan, shaft_above);
                        assert!(after <= CONE_SLOPE * (1.0 + 1.0e-9), "narrows {after}");
                        assert!(after >= -CONE_SLOPE * (1.0 + 1.0e-9), "widens {after}");
                    }
                    if let Some(split) = step_plan.split {
                        assert!(split >= PITCH_MM && step.len - split >= PITCH_MM, "no fragment: {split}");
                    }
                }
            }
        }
        assert_eq!(closed, 10);
        // The first step is a cone at the limit: a ring thicker than the chord
        // above it cannot close there, the offset rides along it.
        let steps = [Step { len: 0.10, near: 0.60, far: 0.60 - CONE_SLOPE * 0.10, splittable: true }];
        let plan = plan_transition(&steps, 0.004, true, taper());
        assert!(plan.is_none(), "one piece at the limit cannot close an offset that raises its narrowing");
    }

    #[test]
    fn a_ring_taper_that_runs_out_of_pieces_is_unmet() {
        let steps = [Step { len: 0.02, near: 0.5, far: 0.5, splittable: true }];
        // 0.06 mm of radius needs 0.04 mm of 1.57 slope: more than the piece has.
        assert!(plan_transition(&steps, 0.06, true, taper()).is_none());
        assert!(plan_transition(&steps, 0.06, true, TaperRules { lean_slope_per_arc: 0.0, ..taper() }).is_none());
    }

    #[test]
    fn a_thin_branch_buried_in_a_thick_host_changes_nothing() {
        let (primitives, arms, _) = junction(0.9, 0.9, 0.1, 30.0, 0.5);
        let out = hull_junctions(&primitives, &arms, rules());
        assert_eq!(out.hulled, 0);
        assert_eq!(out.buried, 1, "{out:?}");
        assert!(out.shafts.iter().all(Option::is_none));
        assert!(out.hulls.iter().all(Vec::is_empty));
    }
}
