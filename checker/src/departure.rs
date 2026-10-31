//! Capped departure of a tip cone from the model (design 5.5 b, orchestrator ruling): beyond
//! the contact ball, the gap from the cone's cross-section to the model is monotone while it is
//! below the cap (0.35 mm); after it first reaches the cap it may vary but stays at or above
//! the floor (0.32 mm) for the rest of the region. It is reported as "capped departure", never
//! as "never decreases".
//!
//! **What is measured.** The gap function is `g(s)`, the distance from the cone's cross-section
//! at distance `s` along its axis to the model. It is measured in one representation at a time
//! (the cone of the written 16-gon rings gives the H6 verdict; the analytic cone of the fields
//! is measured for information only, `h6`). With `S` the
//! first `s` at which `g` reaches the cap, the rule asks for `g(s2) >= g(s1) - SLACK_MM` for
//! every `s1 < s2 < S`, and for `g(s) >= floor` for every `s >= S`. The floor takes no slack:
//! the intervals already hold every measurement uncertainty, so a certified lower bound under
//! it is not a pass (a certain fall under it is a violation, one that straddles it is
//! undecided). At the region end `g >= floor` is also asked whether or not the cap was reached:
//! a cone that stays close to the model all along never departs. The gaps that are reported are the measured intervals, never values clipped at the cap; a gap farther than
//! the tracked reach (`cap + TRACK_EXTRA_MM`) is reported as an "at least" lower bound with no
//! upper bound.
//!
//! **Why not whole-slab minima.** The minimum of a slab says nothing about an excursion inside
//! it: a recess in the wall that the cone passes over can raise the gap and drop it again within
//! one slab, and the slab minimum is the same as for a flat wall. The proof here is over the
//! intervals themselves.
//!
//! **The proof.** For one model triangle `T`, `d_T(s)`, the distance from the cross-section to
//! `T`, is a convex function of `s` (the cone is convex, so the set of `(s, x)` with `x` in the
//! section at `s` is convex, and the distance between a convex set that moves in this way and a
//! convex triangle is jointly convex). So from certified intervals of `d_T` at sample positions:
//!
//! * a chord between two samples bounds `d_T` from above between them;
//! * the line through two neighbouring samples, extended past them, bounds `d_T` from below;
//! * every `d_T` is also `speed`-Lipschitz (the section points move at most that fast).
//!
//! `g` is the minimum over triangles, so the same bounds hold for `g`. They are evaluated at
//! sub-steps of 0.5 microns between samples, and the Lipschitz constant of the true `g` carries
//! the bounds across a sub-step. A sample is a thin slab of the cone (the section's own
//! distance is at least the slab's, and at most the slab's plus the Lipschitz constant times
//! its length). Samples are added where the proof fails, up to a work limit: a decrease inside
//! the slack is certified without finer samples than the curvature of `g` needs; one that
//! cannot be bounded within the limit is undecided, never passed.
//!
//! A certain monotone fall is two positions `p < q` before the cap can have been reached
//! (every upper bound up to `q` is under the cap) whose bounds give `lower(p) - upper(q) >
//! SLACK_MM`. A certain floor fall is a position after a point whose lower bound is at the cap
//! (the cap is certainly reached) with `upper < floor`, or the region end with `upper < floor`.
//! Both are found on the same sub-step sweep as the proof. The work is one batch per cone:
//! every sample evaluates all tracked triangles together, and the cones of a file are measured
//! in parallel (`h6`).

use crate::gap::{self, Goal};
use crate::scene::Scene;
use crate::solid::Solid;
use crate::tri::Tri;
use crate::v3::{V, add, cross, dist, dot, lerp, mul, sub};
use std::collections::BTreeSet;

/// Width at which a sample's distance to one triangle is accepted, mm.
pub const WIDTH_MM: f64 = 0.001;
/// A fall in the gap of at most this (mm) is not a decrease: it is within the measurement
/// width of two samples (`WIDTH_MM` each) and the slack of the polygon bounds.
pub const SLACK_MM: f64 = 0.005;
/// Length of the thin slab that stands for a cross-section, mm.
const POINT_MM: f64 = 1e-4;
/// A triangle is tracked when it is within the cap plus this of the cone somewhere, mm.
const TRACK_EXTRA_MM: f64 = 0.2;
/// A sample stops refining a triangle once it is this far beyond the cap, mm.
const CLEAR_EXTRA_MM: f64 = 0.5;
/// Samples the cone starts with (end points included).
const FIRST_SAMPLES: usize = 9;
/// Most samples of one cone in one representation.
const MAX_SAMPLES: usize = 192;
/// Segments shorter than twice this are not split, mm.
const MIN_SEGMENT_MM: f64 = 0.004;
/// Spacing of the points at which the bounds are compared between two samples, mm.
const SUB_STEP_MM: f64 = 0.0005;

/// The shape of the cone in one representation.
#[derive(Debug, Clone)]
pub enum Shape {
    /// The analytic cone: radius `ra` at the ball end, `rb` at the far end.
    Circle { ra: f64, rb: f64 },
    /// The cone of the written rings (the ball-end ring first). Aligned canonical rings, so
    /// the section at a fraction is the blend of the rings.
    /// The second number is the record's measured deviation from its canonical mesh (mm),
    /// carried into every slab's distances.
    Rings([Vec<V>; 2], f64),
}

/// The cone beyond the ball, in one representation, as a function of the distance `s` from
/// the ball centre along the axis `u` (the far end is at `l`).
#[derive(Debug, Clone)]
pub struct Cone {
    pub name: &'static str,
    pub a: V,
    pub u: V,
    pub l: f64,
    /// Where the measurement starts: the first `s` at which the cone can be outside the ball.
    pub from: f64,
    pub shape: Shape,
}

impl Cone {
    /// The most any point of a cross-section moves per unit of `s`.
    fn speed(&self) -> f64 {
        let s = match &self.shape {
            Shape::Circle { ra, rb } => {
                let k = (rb - ra) / self.l;
                (1.0 + k * k).sqrt()
            }
            Shape::Rings(r, _) => r[0]
                .iter()
                .zip(&r[1])
                .map(|(p, q)| dist(*p, *q) / self.l)
                .fold(1.0, f64::max),
        };
        s * 1.001
    }

    /// The part of the cone between `s0` and `s1`.
    fn solid(&self, s0: f64, s1: f64) -> Option<Solid> {
        match &self.shape {
            Shape::Circle { ra, rb } => {
                let r_at = |s: f64| ra + (rb - ra) * s / self.l;
                Solid::frustum(
                    add(self.a, mul(self.u, s0)),
                    add(self.a, mul(self.u, s1)),
                    r_at(s0),
                    r_at(s1),
                )
            }
            Shape::Rings(rings, dev) => {
                Some(ring_slab(rings, s0 / self.l, s1 / self.l).with_dev(*dev))
            }
        }
    }

    /// The point of the axis at `s`.
    #[must_use]
    pub fn place(&self, s: f64) -> V {
        add(self.a, mul(self.u, s))
    }
}

/// The slab of the written cone between the axial fractions `f0` and `f1`: the convex polytope
/// of the written 16-gon rings blended to those fractions (aligned canonical rings, so every
/// side face is a planar trapezoid).
fn ring_slab(rings: &[Vec<V>; 2], f0: f64, f1: f64) -> Solid {
    let at = |f: f64| -> Vec<V> {
        rings[0]
            .iter()
            .zip(&rings[1])
            .map(|(p, q)| lerp(*p, *q, f))
            .collect()
    };
    let (r0, r1) = (at(f0), at(f1));
    let n = r0.len();
    let centre = mul(
        r0.iter().chain(&r1).fold([0.0; 3], |s, v| add(s, *v)),
        1.0 / (2 * n) as f64,
    );
    let mut tris: Vec<Tri> = Vec::with_capacity(4 * n);
    let mut push = |t: Tri| {
        let nrm = cross(sub(t[1], t[0]), sub(t[2], t[0]));
        let mid = mul(add(add(t[0], t[1]), t[2]), 1.0 / 3.0);
        if dot(nrm, sub(mid, centre)) >= 0.0 {
            tris.push(t);
        } else {
            tris.push([t[0], t[2], t[1]]);
        }
    };
    for j in 0..n {
        let k = (j + 1) % n;
        push([r0[j], r1[j], r1[k]]);
        push([r0[j], r1[k], r0[k]]);
    }
    for j in 1..n - 1 {
        push([r0[0], r0[j], r0[j + 1]]);
        push([r1[0], r1[j], r1[j + 1]]);
    }
    Solid::polytope(tris)
}

/// The two numbers of the capped departure rule, mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rule {
    /// Below this the gap must not decrease; once at it the gap may vary.
    pub cap: f64,
    /// After the gap has reached the cap it must stay at or above this.
    pub floor: f64,
}

/// Which part of the rule a fall breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fall {
    /// The gap fell while it was below the cap.
    Monotone,
    /// The gap fell under the floor after it had reached the cap.
    BelowFloor,
    /// The gap at the region end is under the floor (the end must reach it, cap or not).
    AtEnd,
}

/// What the measurement found for one cone.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// The capped departure rule holds (within the slack). The samples: `[s, lower, upper]`
    /// with the measured bounds (the upper bound is infinite where no triangle is tracked).
    Capped(Vec<[f64; 3]>),
    /// Two positions certainly break the rule by more than the slack.
    Decreasing {
        kind: Fall,
        from: [f64; 3],
        to: [f64; 3],
        at: V,
    },
    /// Not decided, and why.
    Undecided(String),
}

/// A cross-section's certified distance to each tracked triangle: `(lower, upper)`.
struct Sample {
    s: f64,
    v: Vec<(f64, f64)>,
}

impl Sample {
    /// The measured lower bound, with `far` as the bound for every triangle not tracked.
    fn lower_real(&self, far: f64) -> f64 {
        self.v.iter().fold(far, |m, x| m.min(x.0))
    }

    /// The measured upper bound (infinite when no triangle is tracked).
    fn upper_real(&self) -> f64 {
        self.v.iter().fold(f64::INFINITY, |m, x| m.min(x.1))
    }
}

/// Where the proof failed.
struct Failure {
    /// The segments to split: those that failed and those that set the earlier bound.
    segs: BTreeSet<usize>,
    at: f64,
    low: f64,
    bound: f64,
}

/// A fall the sweep found to be certain.
struct Certain {
    kind: Fall,
    from: [f64; 3],
    to: [f64; 3],
    at: f64,
}

/// What one sweep over the sub-steps found.
struct Swept {
    fail: Option<Failure>,
    certain: Option<Certain>,
    /// The gap at the region end is certainly under the floor.
    end_certain: Option<Certain>,
    /// The gap at the region end straddles the floor (`[s, lower, upper]`).
    end_unsure: Option<[f64; 3]>,
}

struct Run<'a> {
    cone: &'a Cone,
    tris: Vec<Tri>,
    cap: f64,
    floor: f64,
    /// Gap beyond which a triangle is not tracked: its distance is only known to be at least
    /// this.
    far: f64,
    lip: f64,
    samples: Vec<Sample>,
}

impl Run<'_> {
    /// The certified distances from the cross-section at `s` to each tracked triangle.
    fn take(&self, s: f64) -> Option<Sample> {
        let (s0, s1) = if s + POINT_MM <= self.cone.l {
            (s, s + POINT_MM)
        } else {
            (s - POINT_MM, s)
        };
        let solid = self.cone.solid(s0, s1)?;
        let goal = Goal {
            clear_at: self.cap + CLEAR_EXTRA_MM,
            violation_below: -1.0,
            width: WIDTH_MM,
        };
        let corr = self.lip * POINT_MM;
        let v = self
            .tris
            .iter()
            .map(|t| {
                let g = gap::solid_tri(&solid, t, goal);
                (g.lb, g.ub + corr)
            })
            .collect();
        Some(Sample { s, v })
    }

    /// Bounds on the capped gap at `s`, inside segment `j`, from the samples around it.
    fn point(&self, j: usize, s: f64) -> (f64, f64) {
        let (a, b) = (&self.samples[j], &self.samples[j + 1]);
        let h = b.s - a.s;
        let (da, db) = (s - a.s, b.s - s);
        let (mut lg, mut ug) = (self.cap, self.cap);
        for t in 0..a.v.len() {
            let ((loa, hia), (lob, hib)) = (a.v[t], b.v[t]);
            // Upper: the chord, and the Lipschitz bound from either end.
            let chord = hia + (hib - hia) * da / h;
            let u = chord.min(hia + self.lip * da).min(hib + self.lip * db);
            // Lower: the Lipschitz bound from either end, and the lines through the samples on
            // either side extended into this segment (convexity).
            let mut l = (loa - self.lip * da).max(lob - self.lip * db);
            if j >= 1 {
                let p = &self.samples[j - 1];
                let m = ((loa - p.v[t].1) / (a.s - p.s)).clamp(-self.lip, self.lip);
                l = l.max(loa + m * da);
            }
            if let Some(q) = self.samples.get(j + 2) {
                let m = ((q.v[t].1 - lob) / (q.s - b.s)).clamp(-self.lip, self.lip);
                l = l.max(lob - m * db);
            }
            lg = lg.min(l.max(0.0));
            ug = ug.min(u);
        }
        (lg, ug)
    }

    /// Compare, at every sub-step, the lower bound of the gap with what the rule asks there.
    ///
    /// Before any earlier position has a lower bound at the cap (the cap is not certainly
    /// reached), the rule asks the gap not to fall below the largest upper bound before it
    /// (itself at most the cap): a position that may be before the cap is reached is held to
    /// that, the stronger demand. After such a position the rule asks only for the floor.
    /// `fail` is where the proof fails; `certain` is a fall that is certain whatever the
    /// true gap between the bounds is.
    fn sweep(&self) -> Swept {
        let (cap, floor) = (self.cap, self.floor);
        let mut pg = 0.0f64;
        let mut pg_seg = 0usize;
        let mut fail: Option<Failure> = None;
        let mut certain: Option<Certain> = None;
        // The highest lower bound so far, and the first position at which the gap is certainly
        // at the cap, as `[s, lower, upper]`.
        let mut best: Option<[f64; 3]> = None;
        let mut reached: Option<[f64; 3]> = None;
        for j in 0..self.samples.len() - 1 {
            let (s0, s1) = (self.samples[j].s, self.samples[j + 1].s);
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a small positive count of sub-steps"
            )]
            let n = (((s1 - s0) / SUB_STEP_MM).ceil().max(1.0)) as usize;
            let q = (s1 - s0) / n as f64;
            let mut prev = self.point(j, s0);
            let mut prev_s = s0;
            for i in 1..=n {
                let at = s0 + q * i as f64;
                let cur = self.point(j, at);
                // The earlier position is certain ground for what follows it.
                if best.is_none_or(|b| prev.0 > b[1]) {
                    best = Some([prev_s, prev.0, prev.1]);
                }
                if reached.is_none() && prev.0 >= cap {
                    reached = Some([prev_s, prev.0, prev.1]);
                }
                // The true gap is `lip`-Lipschitz, so across one sub-step it stays within
                // `lip q / 2` of the larger upper bound and the smaller lower bound.
                let up = prev.1.max(cur.1) + self.lip * q / 2.0;
                if up > pg {
                    pg = up;
                    pg_seg = j;
                }
                let low = prev.0.min(cur.0) - self.lip * q / 2.0;
                // Before the cap is certainly reached the rule is the monotone one (within the
                // slack). After it, the floor is a hard minimum: the intervals already hold
                // every uncertainty, so no slack is taken from it.
                let (bound, slack) = if reached.is_some() {
                    (floor, 0.0)
                } else {
                    (pg.min(cap), SLACK_MM)
                };
                if low < bound - slack {
                    let f = fail.get_or_insert_with(|| Failure {
                        segs: BTreeSet::new(),
                        at,
                        low,
                        bound,
                    });
                    f.segs.insert(j);
                    f.segs.insert(pg_seg);
                }
                if certain.is_none() {
                    if let Some(r) = reached {
                        if cur.1 < floor {
                            certain = Some(Certain {
                                kind: Fall::BelowFloor,
                                from: r,
                                to: [at, cur.0, cur.1],
                                at,
                            });
                        }
                    } else if pg < cap
                        && let Some(b) = best
                        && b[1] - cur.1 > SLACK_MM
                    {
                        // Every upper bound so far is under the cap: the gap has been below
                        // it all along, so the monotone rule holds at both positions.
                        certain = Some(Certain {
                            kind: Fall::Monotone,
                            from: b,
                            to: [at, cur.0, cur.1],
                            at,
                        });
                    }
                }
                prev = cur;
                prev_s = at;
            }
        }
        // The region end: the gap there must be at least the floor, whether or not the cap was
        // reached before it (the cap only releases the monotone demand). The end is a sample,
        // so refining cannot narrow its interval: a straddle is undecided at once.
        let k = self.samples.len() - 1;
        let (lo, hi) = self.point(k - 1, self.samples[k].s);
        let end = self.triple(k);
        let (mut end_certain, mut end_unsure) = (None, None);
        if hi < floor {
            end_certain = Some(Certain {
                kind: Fall::AtEnd,
                from: end,
                to: end,
                at: self.samples[k].s,
            });
        } else if lo < floor {
            end_unsure = Some(end);
        }
        Swept {
            fail,
            certain,
            end_certain,
            end_unsure,
        }
    }

    /// Insert a sample in the middle of every segment of `segs` (largest index first).
    fn split(&mut self, segs: &[usize]) -> bool {
        for &j in segs.iter().rev() {
            let mid = 0.5 * (self.samples[j].s + self.samples[j + 1].s);
            let Some(sample) = self.take(mid) else {
                return false;
            };
            self.samples.insert(j + 1, sample);
        }
        true
    }

    /// The measured bounds at sample `k`, not clipped at the cap.
    fn triple(&self, k: usize) -> [f64; 3] {
        let s = &self.samples[k];
        [s.s, s.lower_real(self.far), s.upper_real()]
    }
}

/// Measure whether the gap from `cone` to the model follows the capped departure rule `rule`.
#[must_use]
pub fn judge(scene: &Scene, cone: &Cone, rule: Rule) -> Verdict {
    let cap = rule.cap;
    let Some(whole) = cone.solid(cone.from, cone.l) else {
        return Verdict::Undecided(format!(
            "the {} cone has no length past the ball",
            cone.name
        ));
    };
    let track = cap + TRACK_EXTRA_MM;
    // A triangle that stays farther than `track` from the whole cone never sets the capped gap.
    let goal = Goal {
        clear_at: track,
        violation_below: -1.0,
        width: 0.01,
    };
    let tris: Vec<Tri> = scene
        .model_near(&whole.aabb().inflated(track))
        .into_iter()
        .map(|i| scene.model[i as usize])
        .filter(|t| gap::solid_tri(&whole, t, goal).lb < track)
        .collect();
    let mut run = Run {
        cone,
        tris,
        cap,
        floor: rule.floor,
        far: track,
        lip: cone.speed(),
        samples: Vec::new(),
    };
    if run.tris.is_empty() {
        // No triangle within the tracked reach: the gap is at least that, at every position.
        return Verdict::Capped(vec![
            [cone.from, track, f64::INFINITY],
            [cone.l, track, f64::INFINITY],
        ]);
    }
    for i in 0..FIRST_SAMPLES {
        let s = cone.from + (cone.l - cone.from) * i as f64 / (FIRST_SAMPLES - 1) as f64;
        match run.take(s) {
            Some(sample) => run.samples.push(sample),
            None => return Verdict::Undecided(format!("the {} cone has no length", cone.name)),
        }
    }
    loop {
        let swept = run.sweep();
        if let Some(c) = swept.certain.or(swept.end_certain) {
            return Verdict::Decreasing {
                kind: c.kind,
                from: c.from,
                to: c.to,
                at: cone.place(c.at),
            };
        }
        if let Some(e) = swept.end_unsure {
            return Verdict::Undecided(format!(
                "the gap at the end of the {} tip cone, {:.3} mm from the ball centre, is in [{:.4}, {:.4}] mm: it cannot be shown to reach the {:.4} mm the region end must keep",
                cone.name, e[0], e[1], e[2], rule.floor
            ));
        }
        let Some(fail) = swept.fail else {
            return Verdict::Capped((0..run.samples.len()).map(|k| run.triple(k)).collect());
        };
        let wide: Vec<usize> = fail
            .segs
            .iter()
            .copied()
            .filter(|&j| run.samples[j + 1].s - run.samples[j].s > 2.0 * MIN_SEGMENT_MM)
            .collect();
        if wide.is_empty() || run.samples.len() + wide.len() > MAX_SAMPLES || !run.split(&wide) {
            return Verdict::Undecided(format!(
                "the gap along the {} tip cone cannot be shown to follow the capped departure rule: {:.3} mm from the ball centre its lower bound {:.4} mm is under the {:.4} mm it must keep there (what the gap may have reached earlier below the cap, or the floor once it may have reached the cap), after {} gap samples",
                cone.name,
                fail.at,
                fail.low,
                fail.bound,
                run.samples.len()
            ));
        }
    }
}
