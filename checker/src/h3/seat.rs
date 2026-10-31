//! H3, the end-event proof: the start of every part that stands neither on the plate nor on the
//! model is proved to stand on other support, over the whole stretch in which it prints.
//!
//! No set of sample planes excludes an air gap between two sample heights (a gap of 1e-6 to
//! 9e-5 mm between coaxial rods lies between all of them), and none sees a thin start that
//! hangs beside its carrier until a later, wider section reaches it. So, besides the walk, the
//! first stretch of every such part is proved, by sections of convex bodies, which are continuous
//! in the height:
//!
//! * The **seat** of a part is the first height at which its section is wide enough to be
//!   rooted (`first_thick`). Its **print start** is the first height at which its section is not
//!   a sliver (`printable_start`): from there to the seat the part prints a thin piece that must
//!   already stand on something. Everything is read on the written triangles.
//! * A part is **seated** by a supporter that is itself seated (a chain that starts at the
//!   plate: two floating parts that start together and overlap each other do not seat one
//!   another), whose seat is no higher than its own, that is alive over the whole stretch from
//!   the print start to the seat, and whose section holds the part's over that stretch. At the
//!   seat, the sections overlap by the overlap depth. Over the stretch, either every section
//!   of the part lies inside the supporter's (the part's whole slab, seen from above, lies
//!   inside both end sections of the supporter, and so inside every section between them), or
//!   the stretch splits at a height from which the depth of the overlap of the two sections is
//!   at least the overlap depth, with the containment below the split. That depth is a
//!   minimum, over directions, of concave functions of the height (the support function of a
//!   section of a convex body is concave in the height), so it is concave: a depth of at
//!   least the overlap depth at both ends of a stretch holds throughout it. The supporter's
//!   section must be wider than a sliver at both ends of the stretch, and its width is concave
//!   in the height, so it is throughout. A supporter's printable stretch is what is used, not
//!   its mesh's end faces.
//! * The thresholds are read at the two extreme readings of the rounding of the arithmetic
//!   (`Reading`): strict overlaps for a pass; lenient overlaps for a violation, which needs the
//!   part's section at the first height it prints to overlap no part at all. A start whose
//!   stretch cannot be proved held, and is not certainly floating, is unverified. Faces that
//!   meet exactly, with no overlap along the axis to cover the rounding, are likewise
//!   unverified, never a pass.

use super::{
    EDGE_FACE_MM, ON_PLATE_MM, OVERLAP_MM, P2, PartInfo, Reading, Sec, feet_of, overlap_by,
    part_info, poly_width, section, slab_points,
};
use crate::outcome::at;
use crate::parts::Part;
use crate::patch::PatchOut;
use crate::scene::{LAYER_MM, Scene};
use crate::status::Located;
use std::collections::BTreeSet;

#[cfg(test)]
mod tests;

/// Where a part's start is proved to stand on other support (the end-event proof).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Seat {
    /// Stands on the plate, or is a foot that stands on the model (the walk says which).
    Skip,
    /// Never as wide as the width the proof needs: not decided.
    Thin,
    /// The part prints a section wide enough to be rooted from `seat` on.
    At {
        /// The first height at which its section is that wide.
        seat: f64,
        /// The first height at which its section is not a sliver under the strict reading
        /// (the most printing): the stretch to prove held for a pass starts here.
        strict_start: f64,
        /// The same under the lenient reading (the least printing): what certainly prints
        /// starts here.
        lenient_start: f64,
    },
}

/// The width of the section of `p` at `z` (zero when it has none; a section is convex).
fn width_at(p: &Part, z: f64) -> f64 {
    section(&p.tris, z, 0.0).map_or(0.0, |s| poly_width(&s.poly))
}

/// The lowest height at which the section of `p` is at least `need` wide. The width of the
/// section of a convex body is concave in the height (the sections of a convex body add under
/// the Minkowski sum, and the width of a sum is at least the sum of the widths), so the first
/// such height is found by locating the widest section and bisecting below it. A part that is
/// never `need` wide is held to 0.9 of its widest section instead; `None` only when it has no
/// section at all.
fn first_thick(p: &Part, need: f64) -> Option<f64> {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in p.tris.iter().flatten() {
        lo = lo.min(v[2]);
        hi = hi.max(v[2]);
    }
    if hi.partial_cmp(&lo) != Some(std::cmp::Ordering::Greater) {
        return None;
    }
    let inner = EDGE_FACE_MM.min(0.5 * (hi - lo));
    let a = lo + inner;
    if width_at(p, a) >= need {
        return Some(a);
    }
    let peak = if p.role.is_ball() {
        0.5 * (lo + hi)
    } else {
        let (mut x0, mut x1) = (a, hi - inner);
        for _ in 0..48 {
            let (m1, m2) = (x0 + (x1 - x0) / 3.0, x1 - (x1 - x0) / 3.0);
            if width_at(p, m1) < width_at(p, m2) {
                x0 = m1;
            } else {
                x1 = m2;
            }
        }
        0.5 * (x0 + x1)
    };
    // A part that is never `need` wide is held to 0.9 of its widest section.
    let need = need.min(0.9 * width_at(p, peak));
    if need <= 0.0 {
        return None;
    }
    let (mut x0, mut x1) = (a, peak);
    for _ in 0..48 {
        let m = 0.5 * (x0 + x1);
        if width_at(p, m) >= need {
            x1 = m;
        } else {
            x0 = m;
        }
    }
    Some(x1)
}

/// The first height at which the section of `p` is not a sliver (at least `sliver` wide), and
/// no higher than its `seat`. A part that is a sliver throughout prints nothing: its print
/// start is its seat.
fn printable_start(p: &Part, sliver: f64, seat: f64) -> f64 {
    match first_thick(p, sliver) {
        Some(s) if width_at(p, s) >= sliver => s.min(seat),
        _ => seat,
    }
}

/// The seat height and print start of every part, in one batch: the first height at which the
/// part is `need` wide, so that its section there can overlap what it stands on by the overlap
/// depth, and the first heights at which it is not a sliver (`slivers` is the strict and the
/// lenient sliver width). A part that stands on the plate or is a foot is skipped.
fn seat_heights(
    scene: &Scene,
    feet: &BTreeSet<usize>,
    z0: f64,
    need: f64,
    slivers: [f64; 2],
) -> Vec<Seat> {
    let ix: Vec<usize> = (0..scene.parts.len()).collect();
    crate::par::par_map(&ix, scene.threads, |&i| {
        let p = &scene.parts[i];
        let lowest = p
            .tris
            .iter()
            .flatten()
            .map(|v| v[2])
            .fold(f64::INFINITY, f64::min);
        if lowest - z0 <= ON_PLATE_MM || feet.contains(&i) {
            return Seat::Skip;
        }
        first_thick(p, need).map_or(Seat::Thin, |seat| Seat::At {
            seat,
            strict_start: printable_start(p, slivers[0], seat),
            lenient_start: printable_start(p, slivers[1], seat),
        })
    })
}

/// The penetration depth of two convex polygons: the least, over the directions of their edges,
/// of the length the two extents share (negative when the polygons are apart). A polygon wholly
/// inside the other has the depth of its own least extent.
fn pen_depth(a: &Sec, b: &Sec) -> f64 {
    let mut depth = f64::INFINITY;
    for poly in [&a.poly, &b.poly] {
        for i in 0..poly.len() {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            let n = [q[1] - p[1], p[0] - q[0]];
            let l = n[0].hypot(n[1]);
            if l < 1e-300 {
                continue;
            }
            let n = [n[0] / l, n[1] / l];
            let range = |s: &Sec| {
                s.poly
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                        let d = n[0] * v[0] + n[1] * v[1];
                        (lo.min(d), hi.max(d))
                    })
            };
            let (ra, rb) = (range(a), range(b));
            depth = depth.min(ra.1.min(rb.1) - ra.0.max(rb.0));
        }
    }
    depth
}

/// Is `p` inside the counter-clockwise convex polygon by at least `margin`?
fn inside_by(p: P2, poly: &[P2], margin: f64) -> bool {
    (0..poly.len()).all(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        let l = ex.hypot(ey).max(1e-300);
        (ex * (p[1] - a[1]) - ey * (p[0] - a[0])) / l >= margin
    })
}

/// Which side of the pass/violation split a run of `unseated` serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    /// Prove every start held, over its whole printing stretch: what cannot be proved is
    /// returned (a violation if `Refute` agrees, otherwise unverified).
    Prove,
    /// Find the starts that are certainly not held: a part that is returned has, at the seat or
    /// at the first height it certainly prints, no part under it at all.
    Refute,
}

/// The stretch of a part's start to hold: from where it prints to where it is wide enough to
/// be rooted, under a reading.
struct Span {
    start: f64,
    seat: f64,
    /// The part's own section at the seat.
    own: Sec,
}

/// What the proof reads under one reading.
struct Ctx<'a> {
    scene: &'a Scene,
    infos: Vec<PartInfo>,
    reading: Reading,
    /// A supporting section thinner than this holds nothing.
    seat_sliver: f64,
}

impl Ctx<'_> {
    fn inner(&self, i: usize) -> f64 {
        EDGE_FACE_MM.min(0.5 * (self.infos[i].zmax - self.infos[i].zmin))
    }

    /// `z` kept inside the written extent of part `i`, a hair from its end faces.
    fn clamp(&self, i: usize, z: f64) -> f64 {
        z.clamp(
            self.infos[i].zmin + self.inner(i),
            self.infos[i].zmax - self.inner(i),
        )
    }

    fn at_height(&self, i: usize, h: f64, sliver: f64) -> Option<Sec> {
        section(&self.scene.parts[i].tris, self.clamp(i, h), sliver)
    }

    /// Is part `j` alive from `lo` to `hi`: begun below `lo` and, under the reading, still there
    /// at `hi` (its top end may fall short of `hi` by the reading's `reach`)?
    fn alive_over(&self, j: usize, lo: f64, hi: f64) -> bool {
        let info = &self.infos[j];
        info.zmin <= info.zmax && info.zmin < lo - 1e-9 && info.zmax >= hi - self.reading.reach
    }

    fn apart(&self, i: usize, j: usize) -> bool {
        let (a, b) = (self.scene.parts[i].bbox, self.scene.parts[j].bbox);
        a.lo[0] > b.hi[0] || b.lo[0] > a.hi[0] || a.lo[1] > b.hi[1] || b.lo[1] > a.hi[1]
    }

    /// The stretch of part `i`: its seat (`seat`) and print start (`start0`), as the written
    /// mesh has them.
    fn span(&self, i: usize, seat: f64, start0: f64) -> Option<Span> {
        let own = self.at_height(i, seat, 0.0)?;
        Some(Span {
            start: start0.min(seat),
            seat,
            own,
        })
    }

    /// Does part `j` have a section at `z` that overlaps `own`, the section of part `i` there,
    /// by the overlap depth of the reading?
    fn supports_at(&self, j: usize, i: usize, z: f64, own: &Sec) -> bool {
        j != i
            && self.alive_over(j, z, z)
            && !self.apart(i, j)
            && self
                .at_height(j, z, self.seat_sliver)
                .is_some_and(|q| overlap_by(&q, own, self.reading.depth))
    }

    /// Does every section of part `i` between `z1` and `z2` lie inside the section of part `j`
    /// at the same height, by the margin of the rounding? The
    /// slab of `i`, seen from above, lies inside the sections of `j` at both heights, which
    /// lie inside every section of `j` between them (the section of a convex body at a height
    /// between two others contains the intersection of those two).
    fn nested_over(&self, i: usize, j: usize, z1: f64, z2: f64) -> bool {
        let (Some(a), Some(b)) = (
            self.at_height(j, z1, self.seat_sliver),
            self.at_height(j, z2, self.seat_sliver),
        ) else {
            return false;
        };
        let margin = (self.reading.depth - OVERLAP_MM).max(1e-7);
        slab_points(
            &self.scene.parts[i].tris,
            self.clamp(i, z1),
            self.clamp(i, z2),
        )
        .iter()
        .all(|&q| inside_by(q, &a.poly, margin) && inside_by(q, &b.poly, margin))
    }

    /// Is part `i` held by part `j` over its whole stretch? `j` is alive over it, overlaps `i`
    /// by the depth at the seat, and from the seat down to a height `zp` the depth of the
    /// overlap stays at the overlap depth (it is concave, so the two ends settle the stretch
    /// between); below `zp` the sections of `i` lie inside those of `j`.
    fn holds(&self, j: usize, i: usize, span: &Span) -> bool {
        if !self.supports_at(j, i, span.seat, &span.own) {
            return false;
        }
        if span.seat - span.start <= 1e-9 {
            return true;
        }
        if !self.alive_over(j, span.start, span.seat) {
            return false;
        }
        if self.nested_over(i, j, span.start, span.seat) {
            return true;
        }
        let depth = self.reading.depth;
        let Some(top) = self.at_height(j, span.seat, self.seat_sliver) else {
            return false;
        };
        if pen_depth(&top, &span.own) < depth {
            return false;
        }
        let deep = |z: f64| {
            matches!(
                (self.at_height(i, z, 0.0), self.at_height(j, z, self.seat_sliver)),
                (Some(o), Some(q)) if pen_depth(&q, &o) >= depth
            )
        };
        let (mut lo, mut hi) = (span.start, span.seat);
        if deep(lo) {
            return true;
        }
        for _ in 0..40 {
            let m = 0.5 * (lo + hi);
            if deep(m) {
                hi = m;
            } else {
                lo = m;
            }
        }
        self.nested_over(i, j, span.start, hi)
    }

    /// Is the section of part `i` at the first height it certainly prints
    /// under no part at all? (Only when that height is below its seat.)
    fn floats_at_start(&self, i: usize, span: &Span) -> bool {
        if span.seat - span.start <= 1e-9 {
            return false;
        }
        let Some(own) = self.at_height(i, span.start, 0.0) else {
            return false;
        };
        !(0..self.scene.parts.len()).any(|j| self.supports_at(j, i, span.start, &own))
    }
}

/// The parts that the proof of `mode` leaves unseated under `reading`. A part is seated only
/// by a supporter that is itself seated: parts that stand on the plate (or on the model, or,
/// when refuting, that are never wide enough to be told) start the chain. A supporter's seat
/// height must be no higher than the seated part's: the supporter's thin start is not printed
/// in the air over the part it holds. `seat_sliver` is the width under which the supporting
/// part's section (not the seated part's own) holds nothing.
pub(super) fn unseated(
    scene: &Scene,
    z0: f64,
    seats: &[Seat],
    reading: Reading,
    mode: Mode,
    seat_sliver: f64,
) -> Vec<usize> {
    let ctx = Ctx {
        scene,
        infos: scene.parts.iter().map(|p| part_info(p, z0)).collect(),
        reading,
        seat_sliver,
    };
    let (n, thin_fails) = (scene.parts.len(), mode == Mode::Prove);
    let mut seated: Vec<bool> = seats
        .iter()
        .map(|s| match s {
            Seat::Skip => true,
            Seat::Thin => !thin_fails,
            Seat::At { .. } => false,
        })
        .collect();
    let seat_h = |i: usize| match seats[i] {
        Seat::At { seat, .. } => seat,
        _ => f64::NEG_INFINITY,
    };
    let spans: Vec<Option<Span>> = (0..n)
        .map(|i| match seats[i] {
            Seat::At {
                seat,
                strict_start,
                lenient_start,
            } => ctx.span(
                i,
                seat,
                if mode == Mode::Prove {
                    strict_start
                } else {
                    lenient_start
                },
            ),
            _ => None,
        })
        .collect();
    let mut order: Vec<usize> = (0..n).filter(|&i| spans[i].is_some()).collect();
    order.sort_by(|&a, &b| seat_h(a).total_cmp(&seat_h(b)).then(a.cmp(&b)));
    // A part that is certainly in the air where it first prints is never seated.
    let floating: Vec<bool> = (0..n)
        .map(|i| {
            mode == Mode::Refute
                && spans[i]
                    .as_ref()
                    .is_some_and(|span| ctx.floats_at_start(i, span))
        })
        .collect();
    loop {
        let mut changed = false;
        for &i in &order {
            let Some(span) = spans[i].as_ref().filter(|_| !seated[i] && !floating[i]) else {
                continue;
            };
            let held = (0..n).any(|j| {
                j != i
                    && seated[j]
                    && seat_h(j) <= seat_h(i) + 1e-9
                    && match mode {
                        Mode::Prove => ctx.holds(j, i, span),
                        Mode::Refute => ctx.supports_at(j, i, span.seat, &span.own),
                    }
            });
            if held {
                seated[i] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (0..n)
        .filter(|&i| match seats[i] {
            Seat::Skip => false,
            Seat::Thin => thin_fails,
            Seat::At { .. } => !seated[i],
        })
        .collect()
}

/// What the end-event proof found.
pub(super) struct SeatFindings {
    /// Parts whose start was held to the proof.
    pub(super) checked: usize,
    /// Of those, the parts that print a thin stretch before they are wide enough to be rooted:
    /// the stretch was proved held as well.
    pub(super) stretches: usize,
    /// Starts with no support under them.
    pub(super) located: Vec<Located>,
    /// Starts that are undecided (not proved held, and not certainly in the air).
    pub(super) unverified: Vec<String>,
}

/// Prove the start of every part that does not stand on the plate or the model (see the module
/// comment). The thresholds are read at the two extreme readings: strict overlaps for a pass,
/// lenient overlaps for a violation.
pub(super) fn seat_findings(
    scene: &Scene,
    patches: &PatchOut,
    z0: f64,
    strict: Reading,
    lenient: Reading,
) -> SeatFindings {
    let need = (4.0 * OVERLAP_MM).max(2.0 * strict.depth);
    let feet = feet_of(scene, patches);
    let seats = seat_heights(scene, &feet, z0, need, [strict.sliver, lenient.sliver]);
    // The seating section of the pessimistic reading drops thin slivers as the lenient reading
    // does (they hold less); the optimistic one keeps them as the strict one does.
    let pess = unseated(scene, z0, &seats, strict, Mode::Prove, lenient.sliver);
    let opt = unseated(scene, z0, &seats, lenient, Mode::Refute, strict.sliver);
    let mut out = SeatFindings {
        checked: seats.iter().filter(|s| **s != Seat::Skip).count(),
        stretches: seats
            .iter()
            .filter(
                |s| matches!(s, Seat::At { seat, strict_start, .. } if seat - strict_start > 1e-9),
            )
            .count(),
        located: Vec::new(),
        unverified: Vec::new(),
    };
    for &i in &pess {
        let p = &scene.parts[i];
        let e = if p.start[2] <= p.end[2] {
            p.start
        } else {
            p.end
        };
        let h = match seats[i] {
            Seat::At { lenient_start, .. } => Some(lenient_start),
            _ => None,
        };
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a layer index"
        )]
        let layer = h.and_then(|h| u32::try_from(((h - z0) / LAYER_MM).max(0.0) as usize).ok());
        let height = h.map_or_else(|| "?".to_owned(), |h| format!("{:.5}", h - z0));
        // A part beside a foot may stand on material the checker does not slice (the model):
        // the walk treats such a chain as reliant on the model, and so is this.
        let beside_foot = feet.iter().any(|&f| {
            let (a, b) = (scene.parts[f].bbox, p.bbox);
            f != i && (0..3).all(|k| a.lo[k] <= b.hi[k] + 1e-3 && b.lo[k] <= a.hi[k] + 1e-3)
        });
        if beside_foot {
            out.unverified.push(format!(
                "record {} ({}) starts {height} mm above the plate beside a foot that stands on the model: what carries it is the model, which is not sliced to root it",
                p.rec,
                p.role.name()
            ));
        } else if opt.contains(&i) {
            out.located.push(at(
                Some(p.rec),
                Some(scene.world(e)),
                layer,
                format!(
                    "this {} starts {height} mm above the plate on no other support: no support part is alive at that height under its footprint (an air gap between it and the part below, on the written mesh)",
                    p.role.name()
                ),
            ));
        } else {
            out.unverified.push(format!(
                "record {} ({}) starts {height} mm above the plate: whether the support part below it holds it from where it first prints to where it is wide enough to be rooted is not proved on the written mesh (the overlap is within the rounding of its threshold, or a thin first stretch is not shown to lie on one support)",
                p.rec,
                p.role.name()
            ));
        }
    }
    out
}
