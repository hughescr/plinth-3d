//! The union of directed segments by a y-sweep (private to [`super`]).
//!
//! Input: directed segments that carry a winding weight. The filled region is "winding
//! greater than zero". The sweep cuts the plane into horizontal strips at every segment end
//! point and at every crossing; inside one strip the segments do not cross, so the filled
//! part is a list of trapezoids with exact corner x values. The trapezoids give the area; the
//! boundary pieces of the trapezoids, with interior shared pieces cancelled, link into rings.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use super::Rings;

/// Two x values closer than this (mm) are one place.
const TOL: f64 = 1e-9;
/// A ring vertex closer than this (mm) to the line of its neighbours is dropped.
const COLLINEAR: f64 = 1e-9;
/// A ring whose area is below this (mm²) is dropped.
const SLIVER: f64 = 1e-12;

/// A segment with `y0 < y1`. `d` is the winding change when a left-to-right scan crosses it:
/// +1 for a segment that was directed downward (the left edge of a counter-clockwise ring),
/// -1 for one directed upward.
#[derive(Debug, Clone, Copy)]
pub(super) struct Seg {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub d: i8,
}

impl Seg {
    /// The segment from `p` to `q` as directed; `None` when it is horizontal.
    pub(super) fn directed(p: [f64; 2], q: [f64; 2]) -> Option<Self> {
        match q[1].total_cmp(&p[1]) {
            Ordering::Greater => Some(Self {
                x0: p[0],
                y0: p[1],
                x1: q[0],
                y1: q[1],
                d: -1,
            }),
            Ordering::Less => Some(Self {
                x0: q[0],
                y0: q[1],
                x1: p[0],
                y1: p[1],
                d: 1,
            }),
            Ordering::Equal => None,
        }
    }

    /// x at height `y` (exact at the end points).
    pub(super) fn x_at(&self, y: f64) -> f64 {
        if y.total_cmp(&self.y0).is_eq() {
            self.x0
        } else if y.total_cmp(&self.y1).is_eq() {
            self.x1
        } else {
            self.x0 + (self.x1 - self.x0) * ((y - self.y0) / (self.y1 - self.y0))
        }
    }
}

/// One filled interval of a strip: left and right x at the strip's bottom (`a`) and top (`b`).
#[derive(Debug, Clone, Copy)]
pub(super) struct Interval {
    pub la: f64,
    pub lb: f64,
    pub ra: f64,
    pub rb: f64,
}

/// A horizontal strip `[ya, yb]` and its filled intervals, left to right.
#[derive(Debug, Clone)]
pub(super) struct Strip {
    pub ya: f64,
    pub yb: f64,
    pub intervals: Vec<Interval>,
}

/// One segment's x at the two ends of a (sub)strip.
#[derive(Clone, Copy)]
struct Cross {
    xa: f64,
    xb: f64,
    d: i8,
}

fn crosses(segs: &[Seg], active: &[usize], ya: f64, yb: f64) -> Vec<Cross> {
    let mut v: Vec<Cross> = active
        .iter()
        .map(|&i| Cross {
            xa: segs[i].x_at(ya),
            xb: segs[i].x_at(yb),
            d: segs[i].d,
        })
        .collect();
    v.sort_unstable_by(|p, q| p.xa.total_cmp(&q.xa).then(p.xb.total_cmp(&q.xb)));
    v
}

/// The lowest y inside `(lo, hi)` where two neighbouring segments swap order, or `hi`.
fn first_crossing(items: &[Cross], lo: f64, hi: f64) -> f64 {
    let mut cut = hi;
    for w in items.windows(2) {
        if w[0].xb > w[1].xb + TOL * 0.01 {
            let da = w[1].xa - w[0].xa;
            let db = w[0].xb - w[1].xb;
            let t = da / (da + db);
            let y = lo + t * (hi - lo);
            if y > lo && y < cut {
                cut = y;
            }
        }
    }
    cut
}

/// The filled intervals of one strip piece, by the winding scan.
fn intervals_of(items: &[Cross]) -> Vec<Interval> {
    let mut out: Vec<Interval> = Vec::new();
    let mut w = 0_i32;
    let mut start: Option<Cross> = None;
    for c in items {
        let before = w;
        w += i32::from(c.d);
        if before <= 0 && w > 0 {
            start = Some(*c);
        } else if before > 0
            && w <= 0
            && let Some(s) = start.take()
        {
            let mut iv = Interval {
                la: s.xa,
                lb: s.xb,
                ra: c.xa.max(s.xa),
                rb: c.xb.max(s.xb),
            };
            // Keep the list sorted and disjoint at both ends of the strip, whatever the
            // rounding of neighbouring segments: the boundary tracing relies on it.
            if let Some(prev) = out.last_mut() {
                iv.la = iv.la.max(prev.ra);
                iv.lb = iv.lb.max(prev.rb);
                iv.ra = iv.ra.max(iv.la);
                iv.rb = iv.rb.max(iv.lb);
                if iv.la - prev.ra <= TOL && iv.lb - prev.rb <= TOL {
                    prev.ra = iv.ra;
                    prev.rb = iv.rb;
                    continue;
                }
            }
            out.push(iv);
        }
    }
    out.retain(|iv| iv.ra - iv.la > 0.0 || iv.rb - iv.lb > 0.0);
    out
}

/// Cut the plane at every end point and crossing and list the filled intervals per strip.
pub(super) fn strips(segs: &[Seg]) -> Vec<Strip> {
    let mut ys: Vec<f64> = segs.iter().flat_map(|s| [s.y0, s.y1]).collect();
    ys.sort_unstable_by(f64::total_cmp);
    ys.dedup_by(|a, b| a.total_cmp(b).is_eq());
    let mut order: Vec<usize> = (0..segs.len()).collect();
    order.sort_unstable_by(|&a, &b| segs[a].y0.total_cmp(&segs[b].y0));
    let mut active: Vec<usize> = Vec::new();
    let mut next = 0;
    let mut out = Vec::with_capacity(ys.len());
    for w in ys.windows(2) {
        let (ya, yb) = (w[0], w[1]);
        active.retain(|&i| segs[i].y1 > ya);
        while next < order.len() && segs[order[next]].y0 <= ya {
            if segs[order[next]].y1 > ya {
                active.push(order[next]);
            }
            next += 1;
        }
        let mut lo = ya;
        while lo < yb {
            let items = crosses(segs, &active, lo, yb);
            let cut = first_crossing(&items, lo, yb);
            let (hi, items) = if cut < yb && cut > lo {
                (cut, crosses(segs, &active, lo, cut))
            } else {
                (yb, items)
            };
            out.push(Strip {
                ya: lo,
                yb: hi,
                intervals: intervals_of(&items),
            });
            lo = hi;
        }
    }
    out
}

/// The exact area of the filled region: the sum of the trapezoids (the ring area must equal
/// it; tests compare the two).
#[cfg(test)]
pub(super) fn area(strips: &[Strip]) -> f64 {
    strips
        .iter()
        .map(|s| {
            let h = s.yb - s.ya;
            s.intervals
                .iter()
                .map(|iv| 0.5 * h * ((iv.ra - iv.la) + (iv.rb - iv.lb)))
                .sum::<f64>()
        })
        .sum()
}

type Pt = [f64; 2];

/// The parts of the sorted interval list `a` that the sorted list `b` does not cover.
fn subtract(a: &[(f64, f64)], b: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for &(lo, hi) in a {
        let mut cur = lo;
        for &(c, d) in b {
            if d <= cur || c >= hi {
                continue;
            }
            if c > cur {
                out.push((cur, c));
            }
            cur = cur.max(d);
        }
        if cur < hi {
            out.push((cur, hi));
        }
    }
    out
}

/// Every boundary piece of the filled region, interior to the left.
fn boundary_pieces(strips: &[Strip]) -> Vec<(Pt, Pt)> {
    let mut pieces: Vec<(Pt, Pt)> = Vec::new();
    for (k, s) in strips.iter().enumerate() {
        for iv in &s.intervals {
            pieces.push(([iv.ra, s.ya], [iv.rb, s.yb]));
            pieces.push(([iv.lb, s.yb], [iv.la, s.ya]));
        }
        let bottom: Vec<(f64, f64)> = s.intervals.iter().map(|i| (i.la, i.ra)).collect();
        let below: Vec<(f64, f64)> = match k.checked_sub(1).map(|j| &strips[j]) {
            Some(p) if p.yb.total_cmp(&s.ya).is_eq() => {
                p.intervals.iter().map(|i| (i.lb, i.rb)).collect()
            }
            _ => Vec::new(),
        };
        for (p, q) in subtract(&bottom, &below) {
            pieces.push(([p, s.ya], [q, s.ya]));
        }
        let top: Vec<(f64, f64)> = s.intervals.iter().map(|i| (i.lb, i.rb)).collect();
        let above: Vec<(f64, f64)> = match strips.get(k + 1) {
            Some(n) if n.ya.total_cmp(&s.yb).is_eq() => {
                n.intervals.iter().map(|i| (i.la, i.ra)).collect()
            }
            _ => Vec::new(),
        };
        for (p, q) in subtract(&top, &above) {
            pieces.push(([q, s.yb], [p, s.yb]));
        }
    }
    pieces
}

fn key(p: Pt) -> (u64, u64) {
    (p[0].to_bits(), p[1].to_bits())
}

/// Link the pieces into closed rings. Pieces that meet end to start link; a pair of equal and
/// opposite pieces is interior and cancels.
fn link(pieces: &[(Pt, Pt)]) -> Vec<Vec<Pt>> {
    let mut by_start: BTreeMap<(u64, u64), Vec<usize>> = BTreeMap::new();
    for (i, (p, _)) in pieces.iter().enumerate() {
        by_start.entry(key(*p)).or_default().push(i);
    }
    let mut dead = vec![false; pieces.len()];
    for (i, (p, q)) in pieces.iter().enumerate() {
        if dead[i] {
            continue;
        }
        let twin = by_start.get(&key(*q)).and_then(|c| {
            c.iter()
                .copied()
                .find(|&j| !dead[j] && key(pieces[j].1) == key(*p))
        });
        if let Some(j) = twin {
            dead[i] = true;
            dead[j] = true;
        }
    }
    let mut rings = Vec::new();
    for i in 0..pieces.len() {
        if dead[i] {
            continue;
        }
        dead[i] = true;
        let start = pieces[i].0;
        let mut ring = vec![start];
        let mut cur = pieces[i].1;
        while key(cur) != key(start) {
            ring.push(cur);
            let next = by_start
                .get(&key(cur))
                .and_then(|c| c.iter().copied().find(|&j| !dead[j]));
            match next {
                Some(j) => {
                    dead[j] = true;
                    cur = pieces[j].1;
                }
                None => break,
            }
        }
        rings.push(ring);
    }
    rings
}

/// Drop repeated and collinear vertices.
fn simplify(ring: &[Pt]) -> Vec<Pt> {
    let mut pts: Vec<Pt> = Vec::with_capacity(ring.len());
    for &p in ring {
        if pts.last().is_none_or(|l| key(*l) != key(p)) {
            pts.push(p);
        }
    }
    let m = pts.len();
    if m < 3 {
        return pts;
    }
    // Start at the vertex with the largest x (then y): an extreme vertex is a true corner.
    let start = (0..m)
        .max_by(|&a, &b| {
            pts[a][0]
                .total_cmp(&pts[b][0])
                .then(pts[a][1].total_cmp(&pts[b][1]))
        })
        .unwrap_or(0);
    pts.rotate_left(start);
    // Sliding window: extend the chord from the last kept vertex while every vertex it skips
    // stays within COLLINEAR of it, so removals never add up.
    let mut kept = vec![pts[0]];
    let mut anchor = 0;
    for j in 2..=m {
        let end = pts[j % m];
        let ok = (anchor + 1..j).all(|t| dist_to_chord(pts[t], pts[anchor], end) <= COLLINEAR);
        if !ok {
            kept.push(pts[j - 1]);
            anchor = j - 1;
        }
    }
    kept
}

/// Distance from `b` to the line through `a` and `c` (to `a` when they coincide).
fn dist_to_chord(b: Pt, a: Pt, c: Pt) -> f64 {
    let (ux, uy) = (c[0] - a[0], c[1] - a[1]);
    let len = ux.hypot(uy);
    if len > 0.0 {
        ((b[0] - a[0]) * uy - (b[1] - a[1]) * ux).abs() / len
    } else {
        (b[0] - a[0]).hypot(b[1] - a[1])
    }
}

/// Shoelace area of one ring (counter-clockwise positive).
pub(super) fn ring_area(ring: &[Pt]) -> f64 {
    let n = ring.len();
    let mut a = 0.0;
    for i in 0..n {
        let (p, q) = (ring[i], ring[(i + 1) % n]);
        a += p[0] * q[1] - q[0] * p[1];
    }
    0.5 * a
}

/// The boundary rings of the filled region: outer rings counter-clockwise, holes clockwise.
pub(super) fn rings(strips: &[Strip]) -> Rings {
    let mut out = Rings::default();
    for r in link(&boundary_pieces(strips)) {
        let s = simplify(&r);
        if s.len() < 3 {
            continue;
        }
        let a = ring_area(&s);
        if a.abs() < SLIVER {
            continue;
        }
        out.area += a;
        out.rings.push(s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, s: f64, ccw: bool) -> Vec<Seg> {
        let mut c = [[x, y], [x + s, y], [x + s, y + s], [x, y + s]];
        if !ccw {
            c.reverse();
        }
        (0..4)
            .filter_map(|i| Seg::directed(c[i], c[(i + 1) % 4]))
            .collect()
    }

    #[test]
    fn two_overlapping_squares_have_the_union_area_and_one_ring() {
        let mut s = square(0.0, 0.0, 2.0, true);
        s.extend(square(1.0, 1.0, 2.0, true));
        let st = strips(&s);
        assert!((area(&st) - 7.0).abs() < 1e-12);
        let r = rings(&st);
        assert_eq!(r.rings.len(), 1);
        assert!((r.area - 7.0).abs() < 1e-9);
        assert_eq!(r.rings[0].len(), 8);
    }

    #[test]
    fn a_hole_is_a_clockwise_ring_and_subtracts() {
        let mut s = square(0.0, 0.0, 4.0, true);
        s.extend(square(1.0, 1.0, 2.0, false));
        let r = rings(&strips(&s));
        assert_eq!(r.rings.len(), 2);
        assert!((r.area - 12.0).abs() < 1e-9);
        assert!(r.rings.iter().any(|g| ring_area(g) < 0.0));
    }

    #[test]
    fn crossing_triangles_split_at_the_crossing() {
        // Two thin triangles crossing in an X: area is the union, not the sum.
        let t = |a: Pt, b: Pt, c: Pt| {
            [(a, b), (b, c), (c, a)]
                .iter()
                .filter_map(|&(p, q)| Seg::directed(p, q))
                .collect::<Vec<_>>()
        };
        let mut s = t([0.0, 0.0], [4.0, 1.0], [0.0, 1.0]);
        s.extend(t([4.0, 0.0], [4.0, 1.0], [0.0, 1.0]));
        let st = strips(&s);
        let a = area(&st);
        // Each triangle is 2.0; they overlap in a triangle of 1.0.
        assert!((a - (2.0 + 2.0 - 1.0)).abs() < 1e-9, "{a}");
        let r = rings(&st);
        assert!((r.area - a).abs() < 1e-9);
    }

    #[test]
    fn touching_squares_merge_and_the_shared_edge_cancels() {
        let mut s = square(0.0, 0.0, 1.0, true);
        s.extend(square(1.0, 0.0, 1.0, true));
        let r = rings(&strips(&s));
        assert_eq!(r.rings.len(), 1);
        assert!((r.area - 2.0).abs() < 1e-12);
        assert_eq!(r.rings[0].len(), 4);
    }
}
