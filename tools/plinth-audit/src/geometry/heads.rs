//! Report 1: the heads.
//!
//! A head is a contact ball (type 7), its contact taper (type 1) and a chain
//! of smooth frusta, ending in a joint ball (type 9) at `K`. The post leaves
//! `K`. A tip whose chain does not reach a joint is a straight contact and
//! belongs to [`super::contacts`].
//!
//! Everything is measured from the drawn records:
//!
//! * `link_mm`: axial length (along the head polyline) from where the head
//!   leaves the ball surface to where the drawn radius first reaches the ball
//!   radius. It is 0 when the radius at the ball exit already is at least `R`.
//! * `ring_clearance_mm`: the smallest distance from 24 samples of the cone's
//!   narrow-end ring to the model triangles. The cone is the first frustum of
//!   the chain that widens (its start circle is the narrow end); a chain with
//!   no widening frustum uses the start circle of its first frustum.
//! * `post_lean_deg`: the steepest segment of the post chain below `K`, from
//!   vertical (+z).
//! * `phi_deg`: the angle between the head chord (ball centre to `K`) and the
//!   first post segment (`K` to its far end). 0 means the post continues the
//!   head's own axis.

use super::{is_chain_frustum, par_map, ring_points, GeoCtx, Seg};
use crate::graph::Link;
use crate::reader::RecordKind;
use crate::vec3::{self as v, V3};

/// Samples on the cone-start ring.
pub const RING_SAMPLES: usize = 24;
/// Longest chain walked from a taper before giving up (a head has at most
/// four frusta; the margin catches a long run that is not a head).
const MAX_CHAIN: usize = 16;
/// Longest post run measured below `K`.
const MAX_POST_SEGMENTS: usize = 32;

#[derive(Clone, Debug)]
pub struct HeadRow {
    pub tip: usize,
    pub ball_r_mm: f64,
    /// Signed height of the ball centre over the model surface; negative =
    /// inside the solid. `None` when the parity test was indeterminate.
    pub ball_height_mm: Option<f64>,
    pub link_mm: f64,
    pub ring_clearance_mm: f64,
    pub ring_points_inside: usize,
    pub joint: usize,
    pub joint_r_mm: f64,
    pub post: Option<usize>,
    pub post_lean_deg: Option<f64>,
    pub phi_deg: Option<f64>,
    pub frusta: Vec<usize>,
}

/// One head's walked structure, before measurement.
#[derive(Clone, Debug)]
pub struct HeadWalk {
    pub tip: usize,
    pub chain: Vec<Seg>,
    pub joint: usize,
}

/// Walk every tip's chain; keep the ones that reach a joint.
#[must_use]
pub fn walk_heads(ctx: &GeoCtx) -> Vec<HeadWalk> {
    let mut out = Vec::new();
    for tip in ctx.model.contact_records() {
        let Link::Unique(taper) = ctx.graph.taper_for_tip(ctx.model, tip) else {
            continue;
        };
        if let Some(w) = walk_one(ctx, tip, taper) {
            out.push(w);
        }
    }
    // A joint belongs to ONE head. A long straight shaft that lands on another
    // head's joint ball also "reaches a joint"; it is a straight contact. Of
    // the chains that reach one joint the shortest is the head.
    let length = |w: &HeadWalk| -> f64 { w.chain.iter().map(Seg::len).sum() };
    let mut keep = vec![true; out.len()];
    for i in 0..out.len() {
        for j in 0..out.len() {
            if i != j && out[i].joint == out[j].joint {
                let (li, lj) = (length(&out[i]), length(&out[j]));
                if li > lj || (li == lj && out[i].tip > out[j].tip) {
                    keep[i] = false;
                }
            }
        }
    }
    let mut it = keep.into_iter();
    out.retain(|_| it.next().unwrap_or(false));
    out
}

fn walk_one(ctx: &GeoCtx, tip: usize, taper: usize) -> Option<HeadWalk> {
    let t = ctx.rec(taper);
    // The contact taper starts at the ball centre; travel runs away from it.
    let centre = ctx.rec(tip).ball_centre()?;
    let enter_at_start = v::dist(t.start_mm, centre) <= v::dist(t.end_mm, centre);
    let mut chain = vec![Seg::from_record(taper, t, enter_at_start)];
    for _ in 0..MAX_CHAIN {
        let last = *chain.last()?;
        let at = ctx.joined_at(last.record, last.p1);
        if let Some(&(joint, _)) = at
            .iter()
            .find(|(r, _)| ctx.rec(*r).kind == RecordKind::JointSphere)
        {
            return Some(HeadWalk { tip, chain, joint });
        }
        let next: Vec<_> = at
            .iter()
            .filter(|(r, _)| ctx.rec(*r).kind == RecordKind::Member)
            .collect();
        if next.len() != 1 {
            return None;
        }
        let &(nr, slot) = next[0];
        if chain.iter().any(|s| s.record == nr) {
            return None;
        }
        chain.push(Seg::from_record(nr, ctx.rec(nr), slot == 0));
    }
    None
}

/// Axial link length along the polyline; see the module note.
#[must_use]
pub fn link_length_mm(chain: &[Seg], ball_r: f64) -> f64 {
    let total: f64 = chain.iter().map(Seg::len).sum();
    let radius_at = |s: f64| -> Option<f64> {
        let mut acc = 0.0;
        for seg in chain {
            let l = seg.len();
            if s <= acc + l || std::ptr::eq(seg, chain.last()?) {
                let f = if l > 0.0 { ((s - acc) / l).clamp(0.0, 1.0) } else { 0.0 };
                return Some(seg.r0 + (seg.r1 - seg.r0) * f);
            }
            acc += l;
        }
        None
    };
    if ball_r >= total {
        return 0.0;
    }
    if radius_at(ball_r).is_some_and(|r| r >= ball_r) {
        return 0.0;
    }
    // First s >= R where the radius reaches R, segment by segment.
    let mut acc = 0.0;
    for seg in chain {
        let l = seg.len();
        let (lo, hi) = (acc, acc + l);
        acc = hi;
        if hi <= ball_r || l <= 0.0 {
            continue;
        }
        let s_from = lo.max(ball_r);
        let r_from = seg.r0 + (seg.r1 - seg.r0) * ((s_from - lo) / l);
        if r_from >= ball_r {
            return s_from - ball_r;
        }
        if seg.r1 >= ball_r && seg.r1 > r_from {
            let s_cross = s_from + (ball_r - r_from) / (seg.r1 - r_from) * (hi - s_from);
            return s_cross - ball_r;
        }
    }
    total - ball_r
}

/// The post below `K`: its first segment's far end and the steepest lean.
fn post_below(ctx: &GeoCtx, w: &HeadWalk) -> (Option<usize>, Option<V3>, Option<f64>) {
    let k = ctx.rec(w.joint).ball_centre().unwrap_or([0.0; 3]);
    let last_head = w.chain.last().map(|s| s.record);
    let first = ctx
        .joined_at(w.joint, k)
        .into_iter()
        .filter(|(r, _)| ctx.rec(*r).kind == RecordKind::Member && Some(*r) != last_head)
        .min_by_key(|(r, _)| *r);
    let Some((pr, slot)) = first else {
        return (None, None, None);
    };
    let mut seg = Seg::from_record(pr, ctx.rec(pr), slot == 0);
    let far = seg.p1;
    let mut steepest = lean_deg(&seg);
    for _ in 0..MAX_POST_SEGMENTS {
        let next: Vec<_> = ctx
            .joined_at(seg.record, seg.p1)
            .into_iter()
            .filter(|(r, _)| is_chain_frustum(ctx.rec(*r).kind) && *r != seg.record)
            .collect();
        if next.len() != 1 {
            break;
        }
        let (nr, ns) = next[0];
        seg = Seg::from_record(nr, ctx.rec(nr), ns == 0);
        steepest = steepest.max(lean_deg(&seg));
    }
    (Some(pr), Some(far), Some(steepest))
}

/// Angle of a segment from vertical, degrees.
fn lean_deg(seg: &Seg) -> f64 {
    let d = v::sub(seg.p1, seg.p0);
    let n = v::norm(d);
    if n <= 0.0 {
        return 0.0;
    }
    (d[2].abs() / n).clamp(0.0, 1.0).acos().to_degrees()
}

/// Measure every head of the model. One batch: the per-head model queries are
/// independent and run in parallel against the single shared grid.
#[must_use]
pub fn measure_heads(ctx: &GeoCtx, walks: &[HeadWalk]) -> Vec<HeadRow> {
    par_map(walks.len(), ctx.params.threads, |i| {
        let w = &walks[i];
        let tip = ctx.rec(w.tip);
        let centre = tip.ball_centre().unwrap_or([0.0; 3]);
        let r = tip.r_start;
        let mut hits = Vec::new();
        let (d, inside) = ctx.signed_to_model(centre, &mut hits);
        let ball_height_mm = inside.map(|ins| if ins { -d } else { d });

        let narrow = w
            .chain
            .iter()
            .find(|s| s.r1 > s.r0 + 1e-6)
            .unwrap_or(&w.chain[0]);
        let axis = narrow.dir().unwrap_or([0.0, 0.0, 1.0]);
        let ring = ring_points(narrow.p0, axis, narrow.r0, RING_SAMPLES, 0.0);
        let mut ring_min = f64::INFINITY;
        let mut ring_inside = 0usize;
        for p in &ring {
            let (dd, ins) = ctx.signed_to_model(*p, &mut hits);
            ring_min = ring_min.min(dd);
            if ins == Some(true) {
                ring_inside += 1;
            }
        }

        let k = ctx.rec(w.joint).ball_centre().unwrap_or([0.0; 3]);
        let (post, far, lean) = post_below(ctx, w);
        let phi = far.and_then(|f| {
            let a = v::normalize(v::sub(k, centre))?;
            let b = v::normalize(v::sub(f, k))?;
            Some(v::dot(a, b).clamp(-1.0, 1.0).acos().to_degrees())
        });
        HeadRow {
            tip: w.tip,
            ball_r_mm: r,
            ball_height_mm,
            link_mm: link_length_mm(&w.chain, r),
            ring_clearance_mm: ring_min,
            ring_points_inside: ring_inside,
            joint: w.joint,
            joint_r_mm: ctx.rec(w.joint).r_start,
            post,
            post_lean_deg: lean,
            phi_deg: phi,
            frusta: w.chain.iter().map(|s| s.record).collect(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(l0: f64, r0: f64, l1: f64, r1: f64) -> Seg {
        Seg {
            record: 0,
            p0: [0.0, 0.0, l0],
            r0,
            p1: [0.0, 0.0, l1],
            r1,
        }
    }

    #[test]
    fn a_cone_wider_than_the_ball_at_exit_has_no_link() {
        let chain = [seg(0.0, 0.1, 1.0, 0.2)];
        assert!(link_length_mm(&chain, 0.07).abs() < 1e-12);
    }

    #[test]
    fn a_thin_cylinder_then_a_cone_has_the_thin_length_as_link() {
        // Ball R 0.2. Cylinder r 0.1 from s=0 to 1.0, then a cone 0.1 -> 0.4
        // over 1.0. The radius reaches 0.2 at s = 1.333; link = 1.333 - 0.2.
        let chain = [seg(0.0, 0.1, 1.0, 0.1), seg(1.0, 0.1, 2.0, 0.4)];
        let link = link_length_mm(&chain, 0.2);
        assert!((link - (1.0 + 1.0 / 3.0 - 0.2)).abs() < 1e-9, "{link}");
    }

    #[test]
    fn a_chain_that_never_reaches_the_ball_radius_is_all_link() {
        let chain = [seg(0.0, 0.1, 1.0, 0.1)];
        assert!((link_length_mm(&chain, 0.2) - 0.8).abs() < 1e-9);
    }
}
