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
                outcome.hulled += 1;
                None
            };
        }
        // A junction that could not be cut into the host is filled, not left as the
        // old flat cap; only one that cannot even be filled is counted as skipped.
        if let Some(why) = skip {
            if let Some(record) = fill_rings.record(rules.sides) {
                outcome.hulls[a].push(record);
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

    fn rules() -> JunctionRules {
        JunctionRules {
            pixel_pitch_mm: PITCH_MM,
            layer_height_mm: LAYER_MM,
            sides: SIDES,
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
                        && (buried_thin_ring || hull.vertices.iter().any(|v| v == corner)),
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
