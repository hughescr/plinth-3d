//! H6: outside its contact-surrounding region, a support keeps at least 0.3 mm from the model.
//!
//! * **Region.** For a chain whose contact ball touches the model, the region is that ball and
//!   its tip cone, if the cone stays within the region length (3.0 mm) of the contact centre;
//!   it never includes the joint ball or anything after it. A cone that reaches farther is not
//!   exempt at all (a piece that crosses the boundary counts as outside).
//! * **Verdict scope.** Every verdict below is measured on the written triangles; it is valid
//!   for ChituBox v1.4.0 if it slices the stored support meshes, pending the H8 differential
//!   slice test. The solid the record fields name is reported as information only.
//! * **Incidental contact.** Anything that touches the model outside a region, and any patch of
//!   a region part that is not the contact's own patch (a far-wall touch), is an incidental
//!   contact and breaks the rule. The exemption of a region covers exactly one connected patch
//!   of the written mesh: if the contact ball touches the model in two places
//!   that certainly touch, at most one is the intended contact, so the others are incidental;
//!   if a second place only may touch, or the patch may be several, which patch is the
//!   intended one is undecided (unverified), never the favourable reading.
//! * **Gap.** The distance from each non-exempt part's written mesh to the model surface,
//!   measured triangle against triangle (`gap::solid_tri`: the exact smallest distance of the
//!   written triangles, widened by a stated 1e-6 mm rounding allowance on each side). No
//!   convexity of the mesh is assumed: a mesh a few ULPs off convex, as validation allows, is
//!   measured as written. A part passes when its lower bound reaches the limit, violates when
//!   its upper bound is under it, and is unverified when the interval straddles it.
//! * **Departure (capped).** Beyond the contact ball, the region's tip cone follows the capped
//!   departure rule (design 5.5 b): the gap to the model does not decrease while it is below
//!   0.35 mm (the cap); after it first reaches the cap it may vary but stays at or above
//!   0.32 mm (the floor, with no slack: a certified lower bound under it is not a pass). At the
//!   end of the region the gap must also reach 0.32 mm whether or not it reached the cap.
//!   The report calls it "capped departure (cap 0.35 mm)" and shows the
//!   measured gaps, never values clipped at the cap. The gap of the cone's cross-section is
//!   bounded over the intervals themselves (`departure`: convexity of each triangle's
//!   distance, Lipschitz bounds, samples added where the proof fails), not by the minimum of
//!   whole slabs, which hides a rise and fall inside one slab. It is measured on the cone of
//!   the written 16-gon rings (which stand a polygon sagitta inside the circle, a sagitta that
//!   falls along a tapered cone, so the written gap can fall where the analytic one holds; the
//!   analytic cone's result is reported as information and decides nothing). A certain break of
//!   the rule (by more than 0.005 mm while the gap is below the cap; any amount under the floor
//!   after it) is a violation (the part past that point is outside the region and under the
//!   limit); one that cannot be ruled out is unverified. A region with a cone is never passed
//!   without this measurement.
//! * **Inside the model.** A part with no surface within the limit may still be buried in the
//!   model; a ray-parity test on a point of the part catches that (it needs a closed model).

use crate::departure as dep;
use crate::gap::{self, Goal};
use crate::json::Json;
use crate::outcome::{MAX_LOCATED, MEASURED_ON, Outcome, VERDICT_SCOPE, at};
use crate::parts::{Geom, Part, Role};
use crate::patch::{Comp, PatchOut};
use crate::scene::Scene;
use crate::solid::Solid;
use crate::status::Located;
use crate::v3::{V, add, dist, mul};
use std::collections::BTreeSet;

/// Interval width at which a distance is accepted as measured, mm (design 7.2: 0.005 total).
pub const WIDTH_MM: f64 = 0.0025;

/// A certified distance from one part to the model.
#[derive(Debug, Clone, Copy)]
pub struct PartGap {
    /// Lower bound, mm (the rounding allowance already taken off).
    pub lb: f64,
    /// Upper bound, mm (infinite when no model triangle is near).
    pub ub: f64,
    pub tri: Option<u32>,
    /// The work limit was reached before the goal.
    pub exhausted: bool,
}

/// The distance from one solid to the model triangles in `cand`, with the goal of `goal`.
fn solid_gap(scene: &Scene, solid: &Solid, cand: &[u32], reach: f64, goal: Goal) -> PartGap {
    let mut out = PartGap {
        lb: if cand.is_empty() {
            reach
        } else {
            f64::INFINITY
        },
        ub: f64::INFINITY,
        tri: None,
        exhausted: false,
    };
    for &ti in cand {
        let g = gap::solid_tri(solid, &scene.model[ti as usize], goal);
        out.exhausted |= g.exhausted;
        out.lb = out.lb.min(g.lb);
        if g.ub < out.ub {
            out.ub = g.ub;
            out.tri = Some(ti);
        }
    }
    out
}

/// The distance from the written mesh of part `pi` to the model (triangle against triangle,
/// certified in f64 with a rounding allowance), over the model triangles near enough to matter
/// for the goal.
#[must_use]
pub fn part_gap(scene: &Scene, pi: usize, clear_at: f64, violation_below: f64) -> PartGap {
    let part = &scene.parts[pi];
    let Some(mesh) = &part.mesh else {
        return PartGap {
            lb: 0.0,
            ub: f64::INFINITY,
            tri: None,
            exhausted: true,
        };
    };
    let reach = if clear_at.is_finite() {
        clear_at + 1e-3
    } else {
        1.0
    };
    let cand = scene.model_near(&part.bbox.inflated(reach));
    let goal = Goal {
        clear_at,
        violation_below,
        width: WIDTH_MM,
    };
    solid_gap(scene, mesh, &cand, reach, goal)
}

/// A point inside the part (for the inside-the-model test).
fn inner_point(p: &Part) -> V {
    match p.geom {
        Geom::Ball { c, .. } => c,
        Geom::Frust { a, b, .. } => mul(add(a, b), 0.5),
        Geom::Raft => {
            let n = p.tris.len().max(1) as f64;
            let s = p.tris.iter().flatten().fold([0.0; 3], |s, v| add(s, *v));
            mul(s, 1.0 / (3.0 * n))
        }
    }
}

/// The parts that are exempt from the gap test because they form a region, and the regions'
/// descriptions.
struct Regions {
    exempt: BTreeSet<usize>,
    /// One line per region: contact record, reach.
    lines: Vec<(usize, f64)>,
    over_length: Vec<usize>,
    /// (contact ball, tip cone) part indices of every region that has an exempt cone.
    with_cone: Vec<(usize, usize)>,
}

fn find_regions(scene: &Scene, patches: &PatchOut) -> Regions {
    let mut r = Regions {
        exempt: BTreeSet::new(),
        lines: Vec::new(),
        over_length: Vec::new(),
        with_cone: Vec::new(),
    };
    for (ci, chain) in scene.chains.chains.iter().enumerate() {
        let Some(b) = chain.contact else { continue };
        // The ball is a contact if its written mesh touches the model.
        let touches = patches
            .mesh()
            .comps
            .iter()
            .any(|c| c.chain == ci && c.parts.contains(&b));
        if !touches {
            continue;
        }
        let ball = &scene.parts[b];
        let centre = inner_point(ball);
        r.exempt.insert(b);
        let mut reach = ball_reach(ball, centre);
        if let Some(ci_cone) = chain.cone {
            let cone = &scene.parts[ci_cone];
            // The length of the region is measured along the cone's centreline, from the
            // contact centre to the cone's far end point (the Heavy profile's cone is 3.0 mm).
            // The written cone may stand its validated envelope farther out than its fields.
            let cone_reach = dist(cone.start, centre).max(dist(cone.end, centre));
            if cone_reach + cone.tol <= scene.limits.region_length_mm + 1e-3 {
                r.exempt.insert(ci_cone);
                r.with_cone.push((b, ci_cone));
                reach = reach.max(cone_reach);
            } else {
                r.over_length.push(cone.rec);
            }
        }
        r.lines.push((ball.rec, reach));
    }
    r
}

fn ball_reach(ball: &Part, centre: V) -> f64 {
    ball.tris
        .iter()
        .flatten()
        .map(|v| dist(*v, centre))
        .fold(0.0, f64::max)
}

/// What the exemption of a region leaves open: certain incidental contacts, and contacts whose
/// ownership is not certain.
struct Incidental {
    located: Vec<Located>,
    unverified: Vec<String>,
}

/// The exemption of a region covers exactly one connected contact patch: the one the contact
/// ball makes. Read on the patches of the written mesh:
///
/// * a patch of the region's parts that does not contain the ball is incidental;
/// * if the ball is in two or more patches that certainly touch (two model sheets), at most one
///   is the intended contact, so every other one is incidental whichever it is: they are
///   located, the larger patch being taken as the intended one;
/// * a second patch that only may touch, or a patch that may be several, leaves the ownership
///   undecided (never the favourable reading).
fn incidental_in_regions(scene: &Scene, patches: &PatchOut, reg: &Regions) -> Incidental {
    let mut out = Incidental {
        located: Vec::new(),
        unverified: Vec::new(),
    };
    let rep = patches.mesh();
    for (ci, chain) in scene.chains.chains.iter().enumerate() {
        let Some(b) = chain.contact else { continue };
        if !reg.exempt.contains(&b) {
            continue;
        }
        let (own, other): (Vec<&Comp>, Vec<&Comp>) = rep
            .comps
            .iter()
            .filter(|c| c.chain == ci)
            .partition(|c| c.parts.contains(&b));
        for c in other
            .iter()
            .filter(|c| c.parts.iter().all(|p| reg.exempt.contains(p)))
        {
            let rec = scene.parts[c.parts[0]].rec;
            if c.certain_lo > 0.0 {
                out.located.push(at(
                    Some(rec),
                    Some(scene.world(c.at)),
                    None,
                    "a part of a contact-surrounding region touches the model away from the contact (written mesh; incidental contact)",
                ));
            } else {
                out.unverified.push(format!(
                    "record {rec}: a part of a contact-surrounding region may touch the model away from the contact (written mesh)"
                ));
            }
        }
        let mut sheets: Vec<&&Comp> = own.iter().filter(|c| c.certain_lo > 0.0).collect();
        sheets.sort_by(|x, y| y.certain_lo.total_cmp(&x.certain_lo));
        for c in sheets.iter().skip(1) {
            out.located.push(at(
                Some(scene.parts[b].rec),
                Some(scene.world(c.at)),
                None,
                format!(
                    "the contact ball touches the model in {} separate places (written mesh): at most one is the intended contact, the others are incidental contacts",
                    sheets.len()
                ),
            ));
        }
        let may_be_more = own.len() > sheets.len().max(1) || own.iter().any(|c| c.split);
        if sheets.len() <= 1 && may_be_more {
            out.unverified.push(format!(
                "record {}: the contact ball may touch the model in more than one patch, or in a patch that may be several (written mesh): which patch is the intended contact is not decided",
                scene.parts[b].rec
            ));
        }
    }
    out
}

/// The departure cap is the H6 limit plus this (0.35 mm for the 0.30 mm limit) [S]: below it the
/// gap of a tip cone must not decrease.
const DEPARTURE_CAP_ABOVE_LIMIT_MM: f64 = 0.05;
/// The departure floor is the H6 limit plus this (0.32 mm, the build gap) [S]: once the gap has
/// reached the cap it may vary but must stay at or above this.
const DEPARTURE_FLOOR_ABOVE_LIMIT_MM: f64 = 0.02;

/// What the departure measurement found for one region.
#[derive(Debug, Clone, PartialEq)]
enum Departure {
    /// The cone ends inside the contact ball: no part of it departs.
    InsideBall,
    /// The capped departure rule holds on the written cone (within the stated slack, over the
    /// intervals themselves, `departure` module): the samples `[s, lower, upper]`, as measured
    /// and never clipped at the cap.
    Capped { written: Vec<[f64; 3]> },
    /// Two positions of the written cone certainly break the rule by more than the slack.
    Decreasing {
        kind: dep::Fall,
        from: [f64; 3],
        to: [f64; 3],
        at: V,
    },
    /// Not decided, and why.
    Undecided(String),
}

/// The first axial position at which the cone's surface is outside the ball: the smallest `t`
/// with `t^2 + r(t)^2 >= R^2`, where `t` is measured from the ball centre along the cone.
fn exit_t(ra: f64, rb: f64, l: f64, big_r: f64) -> f64 {
    let k = (rb - ra) / l;
    let (qa, qb, qc) = (1.0 + k * k, 2.0 * ra * k, ra * ra - big_r * big_r);
    if qc >= 0.0 {
        return 0.0;
    }
    let disc = qb * qb - 4.0 * qa * qc;
    ((-qb + disc.max(0.0).sqrt()) / (2.0 * qa)).clamp(0.0, l)
}

/// The exposed neck of a contact: the area of the written 16-gon section of `cone` where its
/// surface leaves the contact ball `ball`, less the envelope the mesh may sit inside the
/// fields by. `None` when the cone does not start at the ball centre.
#[must_use]
pub fn neck_area(_scene: &Scene, ball: &Part, cone: &Part) -> Option<f64> {
    let (Geom::Ball { c, r: big_r }, Geom::Frust { a, b, ra, rb }) = (ball.geom, cone.geom) else {
        return None;
    };
    let (ra, rb) = if dist(a, c) <= crate::chains::JOIN_MM {
        (ra, rb)
    } else if dist(b, c) <= crate::chains::JOIN_MM {
        (rb, ra)
    } else {
        return None;
    };
    let l = dist(a, b);
    let t = exit_t(ra, rb, l, big_r);
    let r = (ra + (rb - ra) * t / l - cone.tol).max(0.0);
    Some(8.0 * r * r * (std::f64::consts::PI / 8.0).sin())
}

/// The first and last sample of a departure measurement as `[[s, lower, upper], ...]`. An
/// unbounded upper bound (no model triangle within the tracked reach) is null.
fn ends_json(samples: &[[f64; 3]]) -> Json {
    let row = |s: &[f64; 3]| {
        Json::Arr(
            s.iter()
                .map(|x| {
                    if x.is_finite() {
                        Json::Num(*x)
                    } else {
                        Json::Null
                    }
                })
                .collect(),
        )
    };
    Json::Arr(
        samples
            .first()
            .into_iter()
            .chain(samples.last())
            .map(row)
            .collect(),
    )
}

/// The measured gaps of one representation against the capped rule: the first sample whose
/// lower bound is at the cap (`[s, lower, upper]`, or null), and the smallest measured lower
/// bound among the samples from there on (or null). The values are the measured bounds, never
/// clipped.
fn cap_summary(samples: &[[f64; 3]], cap: f64) -> (Json, Json) {
    let row = |s: &[f64; 3]| {
        Json::Arr(
            s.iter()
                .map(|x| {
                    if x.is_finite() {
                        Json::Num(*x)
                    } else {
                        Json::Null
                    }
                })
                .collect(),
        )
    };
    let Some(k) = samples.iter().position(|s| s[1] >= cap) else {
        return (Json::Null, Json::Null);
    };
    let lowest = samples[k..]
        .iter()
        .map(|s| s[1])
        .fold(f64::INFINITY, f64::min);
    (row(&samples[k]), Json::Num(lowest))
}

/// Capped departure of the tip cone `cone` from the contact ball `ball` (design 5.5 b: along
/// the head beyond the ball the gap to the model is monotone while below the cap, and at or
/// above the floor once it has reached the cap). The gap of the cone's cross-section is
/// measured over the intervals themselves, not slab minima (`departure` module), on the cone of
/// the written 16-gon rings (which stand a polygon sagitta inside the circle, a sagitta that
/// shrinks along a tapered cone, so the written gap can fall where the analytic one holds). The
/// written cone starts where it can first leave the written ball (its certified in-sphere). A
/// fall that is certain is a violation; one that cannot be ruled out is undecided. The second
/// result is the analytic cone's outcome, as information only.
fn departure(scene: &Scene, ball: &Part, cone: &Part) -> (Departure, String) {
    let undecided = |why: &str| (Departure::Undecided(why.to_owned()), String::new());
    let (Geom::Ball { c, r: big_r }, Geom::Frust { a, b, ra, rb }) = (ball.geom, cone.geom) else {
        return undecided("the region parts are not a ball and a cone");
    };
    // Orient the cone so t = 0 is at the ball centre.
    let (a, b, ra, rb, flip) = if dist(a, c) <= crate::chains::JOIN_MM {
        (a, b, ra, rb, false)
    } else if dist(b, c) <= crate::chains::JOIN_MM {
        (b, a, rb, ra, true)
    } else {
        return undecided("the tip cone does not start at the contact ball centre");
    };
    let l = dist(a, b);
    let u = crate::v3::unit(crate::v3::sub(b, a));
    let t0 = exit_t(ra, rb, l, big_r);
    // The written cone can leave the written ball as early as the point where the cone's
    // surface leaves the ball's certified in-sphere. A cone that ends inside that sphere
    // departs nowhere.
    let t0_mesh = exit_t(ra, rb, l, (big_r - ball.sag).max(0.0)).min(t0);
    if t0_mesh >= l - 1e-9 {
        return (Departure::InsideBall, String::new());
    }
    if !cone.is_valid() || !ball.is_valid() || cone.rings.iter().any(|r| r.len() < 3) {
        return undecided(
            "a part of the region is not the native shape: its written rings are not known",
        );
    }
    let rings: [Vec<V>; 2] = if flip {
        [cone.rings[1].clone(), cone.rings[0].clone()]
    } else {
        [cone.rings[0].clone(), cone.rings[1].clone()]
    };
    let rule = departure_rule(scene);
    let analytic = dep::Cone {
        name: "analytic",
        a,
        u,
        l,
        from: t0,
        shape: dep::Shape::Circle { ra, rb },
    };
    let written = dep::Cone {
        name: "written",
        a,
        u,
        l,
        from: t0_mesh,
        shape: dep::Shape::Rings(rings, cone.mesh.as_ref().map_or(0.0, Solid::dev)),
    };
    // The analytic cone is judged for the report only; the verdict is the written cone's.
    let info = analytic_note(&dep::judge(scene, &analytic, rule));
    let found = match dep::judge(scene, &written, rule) {
        dep::Verdict::Decreasing { kind, from, to, at } => {
            Departure::Decreasing { kind, from, to, at }
        }
        dep::Verdict::Undecided(why) => Departure::Undecided(why),
        dep::Verdict::Capped(written) => Departure::Capped { written },
    };
    (found, info)
}

/// What the analytic cone's departure came to, as information (it decides nothing).
fn analytic_note(v: &dep::Verdict) -> String {
    match v {
        dep::Verdict::Capped(_) => {
            "the capped departure rule holds on the analytic cone (information only)".to_owned()
        }
        dep::Verdict::Decreasing { kind, from, to, .. } => format!(
            "the analytic cone would break the rule ({kind:?}): gap at least {:.4} mm at {:.3} mm from the ball centre, at most {:.4} mm at {:.3} mm",
            from[1], from[0], to[2], to[0]
        ),
        dep::Verdict::Undecided(why) => format!("not decided on the analytic cone: {why}"),
    }
}

/// The capped departure rule for this scene's H6 limit.
fn departure_rule(scene: &Scene) -> dep::Rule {
    dep::Rule {
        cap: scene.limits.h6_gap_mm + DEPARTURE_CAP_ABOVE_LIMIT_MM,
        floor: scene.limits.h6_gap_mm + DEPARTURE_FLOOR_ABOVE_LIMIT_MM,
    }
}

struct Tally {
    located: Vec<Located>,
    unverified: Vec<String>,
    worst: Option<(usize, PartGap)>,
    tested: usize,
}

fn test_parts(scene: &Scene, reg: &Regions) -> Tally {
    let limit = scene.limits.h6_gap_mm;
    let mut t = Tally {
        located: Vec::new(),
        unverified: Vec::new(),
        worst: None,
        tested: 0,
    };
    for (pi, part) in scene.parts.iter().enumerate() {
        if reg.exempt.contains(&pi) {
            continue;
        }
        if let Some(why) = &part.invalid {
            t.unverified
                .push(format!("record {} ({}): {why}", part.rec, part.role.name()));
            continue;
        }
        t.tested += 1;
        let g = part_gap(scene, pi, limit, limit);
        let place = part.bbox.centre();
        if g.ub < limit {
            let tri_note = g
                .tri
                .map_or(String::new(), |_| " from the model surface".to_owned());
            t.located.push(at(
                Some(part.rec),
                Some(scene.world(place)),
                None,
                format!(
                    "{} is at most {:.4} mm{tri_note}, under the {limit} mm limit",
                    part.role.name(),
                    g.ub
                ),
            ));
        } else if g.lb < limit {
            t.unverified.push(format!(
                "record {} ({}): the gap is in [{:.4}, {:.4}] mm, straddling the {limit} mm limit",
                part.rec,
                part.role.name(),
                g.lb,
                g.ub
            ));
        } else {
            // No surface within the limit: the part is clear, or buried in the model.
            match scene.point_in_model(inner_point(part)) {
                Some(true) => t.located.push(at(
                    Some(part.rec),
                    Some(scene.world(inner_point(part))),
                    None,
                    format!("{} lies inside the model", part.role.name()),
                )),
                Some(false) => {}
                None => t.unverified.push(format!(
                    "record {} ({}): whether it lies inside the model is not decided (the model is not a closed, oriented mesh, or the test ray was ambiguous)",
                    part.rec,
                    part.role.name()
                )),
            }
        }
        if t.worst.is_none_or(|(_, w)| g.lb < w.lb) {
            t.worst = Some((pi, g));
        }
    }
    t
}

/// The report entry of one region's departure, the located violation it makes, if any, and the
/// unverified note it makes, if any.
fn departure_entry(
    scene: &Scene,
    cone: &Part,
    found: Departure,
    analytic_info: String,
    rule: dep::Rule,
) -> (Json, Option<Located>, Option<String>) {
    let limit = scene.limits.h6_gap_mm;
    let (mut fall, mut undecided) = (None, None);
    let (state, extra) = match found {
        Departure::InsideBall => ("inside the ball".to_owned(), Json::Null),
        Departure::Capped { written } => {
            let (w_cap, w_low) = cap_summary(&written, rule.cap);
            (
                format!("capped departure (cap {:.2} mm)", rule.cap),
                Json::obj([
                    ("written_samples", Json::Uint(written.len() as u64)),
                    ("written_gap_ends_mm", ends_json(&written)),
                    ("written_first_sample_at_cap", w_cap),
                    ("written_lowest_lower_bound_from_cap_mm", w_low),
                ]),
            )
        }
        Departure::Decreasing {
            kind,
            from,
            to,
            at: place,
        } => {
            let reason = match kind {
                dep::Fall::Monotone => format!(
                    "the tip cone comes closer to the model after it leaves the contact ball, below the {:.2} mm departure cap (written mesh): the gap is at least {:.4} mm at {:.3} mm from the ball centre and at most {:.4} mm at {:.3} mm; the part past that point is outside the region and under the {limit} mm limit",
                    rule.cap, from[1], from[0], to[2], to[0]
                ),
                dep::Fall::BelowFloor => format!(
                    "the tip cone's gap to the model falls under the {:.2} mm departure floor after it reached the {:.2} mm cap (written mesh): the gap is at least {:.4} mm at {:.3} mm from the ball centre and at most {:.4} mm at {:.3} mm; the part past that point is outside the region and under the {limit} mm limit",
                    rule.floor, rule.cap, from[1], from[0], to[2], to[0]
                ),
                dep::Fall::AtEnd => format!(
                    "the tip cone's gap to the model at the end of the region is at most {:.4} mm ({:.3} mm from the ball centre, written mesh), under the {:.2} mm the gap must reach there (capped departure, cap {:.2} mm); the cone never leaves the model by that much",
                    to[2], to[0], rule.floor, rule.cap
                ),
            };
            fall = Some(at(Some(cone.rec), Some(scene.world(place)), None, reason));
            ("decreases".to_owned(), Json::Null)
        }
        Departure::Undecided(why) => {
            undecided = Some(format!(
                "record {} (tip cone): departure from the model not decided: {why}",
                cone.rec
            ));
            ("undecided".to_owned(), Json::str(why))
        }
    };
    let entry = Json::obj([
        ("record", Json::Uint(cone.rec as u64)),
        ("state", Json::str(state)),
        ("gap_samples", extra),
        ("analytic_information", Json::str(analytic_info)),
    ]);
    (entry, fall, undecided)
}

/// H6.
#[must_use]
pub fn measure(scene: &Scene, patches: &PatchOut) -> Outcome {
    if scene.parts.is_empty() {
        return Outcome::not_applicable(
            "the file has no support records: no support surface to keep clear of the model",
        );
    }
    let limit = scene.limits.h6_gap_mm;
    let reg = find_regions(scene, patches);
    let mut tally = test_parts(scene, &reg);
    let mut located_departure: Vec<Located> = Vec::new();
    let mut located = std::mem::take(&mut tally.located);
    let incidental = incidental_in_regions(scene, patches, &reg);
    located.extend(incidental.located);
    tally.unverified.extend(incidental.unverified);
    for rec in &reg.over_length {
        located.push(at(
            Some(*rec),
            None,
            None,
            format!(
                "the tip cone reaches farther than the {} mm region length from its contact centre, so it is tested as outside",
                scene.limits.region_length_mm
            ),
        ));
    }
    let mut departures: Vec<Json> = Vec::new();
    let rule = departure_rule(scene);
    // The departures of the regions are independent: one batch, results in region order.
    let found = crate::par::par_map(&reg.with_cone, scene.threads, |&(bi, ci)| {
        departure(scene, &scene.parts[bi], &scene.parts[ci])
    });
    for (&(_, ci), (dep_found, analytic_info)) in reg.with_cone.iter().zip(found) {
        let cone = &scene.parts[ci];
        let (entry, fall, undecided) = departure_entry(scene, cone, dep_found, analytic_info, rule);
        located_departure.extend(fall);
        tally.unverified.extend(undecided);
        departures.push(entry);
    }
    located.append(&mut located_departure);
    // A tighter interval for the part with the smallest gap, for the report.
    let worst = tally.worst.map(|(pi, g)| {
        if g.ub.is_finite() && g.lb < limit + 0.05 {
            (pi, part_gap(scene, pi, f64::INFINITY, 0.0))
        } else {
            (pi, g)
        }
    });
    let detail = Json::obj([
        ("limit_mm", Json::Num(limit)),
        ("measured_on", Json::str(MEASURED_ON)),
        ("verdict_scope", Json::str(VERDICT_SCOPE)),
        ("parts_tested", Json::Uint(tally.tested as u64)),
        ("regions", Json::Uint(reg.lines.len() as u64)),
        (
            "region_reach_max_mm",
            Json::Num(reg.lines.iter().map(|l| l.1).fold(0.0, f64::max)),
        ),
        (
            "smallest_gap_lower_mm",
            worst.map_or(Json::Null, |(_, g)| Json::Num(g.lb)),
        ),
        (
            "smallest_gap_upper_mm",
            worst.map_or(Json::Null, |(_, g)| Json::Num(g.ub)),
        ),
        (
            "smallest_gap_record",
            worst.map_or(Json::Null, |(pi, _)| Json::Uint(scene.parts[pi].rec as u64)),
        ),
        ("departure_cap_mm", Json::Num(rule.cap)),
        ("departure_floor_mm", Json::Num(rule.floor)),
        ("departure", Json::Arr(departures)),
        (
            "regions_without_cone",
            Json::Uint((reg.lines.len() - reg.with_cone.len()) as u64),
        ),
    ]);
    verdict(located, tally.unverified, limit, detail)
}

fn verdict(
    mut located: Vec<Located>,
    unverified: Vec<String>,
    limit: f64,
    detail: Json,
) -> Outcome {
    if !located.is_empty() {
        let n = located.len();
        located.truncate(MAX_LOCATED);
        return Outcome::violation(format!(
            "{n} support part(s) closer than {limit} mm to the model outside a contact-surrounding region, or touching it away from a contact"
        ))
        .with_located(located)
        .with_detail(detail);
    }
    if !unverified.is_empty() {
        let first = unverified[0].clone();
        return Outcome::unverified(format!(
            "{} part(s) could not be decided; first: {first}",
            unverified.len()
        ))
        .with_detail(detail);
    }
    Outcome::pass(format!(
        "every support part outside a contact-surrounding region keeps at least {limit} mm from the model (certified lower bound)"
    ))
    .with_detail(detail)
}

/// True for the parts that stand on the build plate (H4 uses their gap to the model).
#[must_use]
pub fn is_plate_part(p: &Part) -> bool {
    p.role.is_plate_part() && p.role != Role::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch;
    use crate::testkit::{self as kit, test_limits as limits};
    use crate::tri::Tri;

    /// A closed wall whose +x face is at x = 0 (outward normal +x), 20 mm square.
    fn wall() -> Vec<Tri> {
        kit::closed_box([-2.0, -10.0, -10.0], [0.0, 10.0, 10.0])
    }

    fn rod_at(x: f32, r: f32) -> Part {
        kit::part(0, 3, [x, 0.0, -5.0], [x, 0.0, 5.0], r, r)
    }

    fn run(parts: Vec<Part>) -> Outcome {
        let scene = Scene::from_parts(&limits(), wall(), [0.0; 3], parts);
        let p = patch::measure(&scene);
        measure(&scene, &p)
    }

    #[test]
    fn a_rod_is_clear_at_0_33_and_a_violation_at_0_28_and_0_295() {
        // Rod radius 0.25 with its surface at the given gap from the wall (it lies at x < 0
        // would be inside; the wall faces +x so the rod stands at positive x).
        let at_gap = |gap: f32| rod_at(0.25 + gap, 0.25);
        assert_eq!(run(vec![at_gap(0.33)]).status, crate::report::Status::Pass);
        for gap in [0.28f32, 0.29, 0.295] {
            let o = run(vec![at_gap(gap)]);
            assert_eq!(
                o.status,
                crate::report::Status::Violation,
                "gap {gap}: {o:?}"
            );
            assert!(!o.located.is_empty());
        }
    }

    #[test]
    fn a_mesh_larger_than_its_fields_by_more_than_the_rounding_is_refused() {
        // At y = 100 two units in the last place are 1.5e-5 mm. A rod whose fields put its
        // surface 0.30002 mm from the wall but whose written mesh is 4e-5 mm wider is not the
        // canonical mesh of its fields: it is refused, and H6 cannot pass on it.
        let wall = kit::closed_box([-2.0, -10.0, -10.0], [0.0, 110.0, 10.0]);
        let x = 0.550_02_f32;
        let (s, e) = ([x, 100.0, -5.0], [x, 100.0, 5.0]);
        let rec = kit::rec(3, s, e, 0.25, 0.25);
        let part = kit::part_with_mesh(0, &rec, &kit::frustum_coords(s, e, 0.250_04, 0.250_04));
        assert!(
            part.invalid.is_some(),
            "a radius 4e-5 mm off is not canonical"
        );
        let scene = Scene::from_parts(&limits(), wall, [0.0; 3], vec![part]);
        let p = patch::measure(&scene);
        assert_ne!(
            measure(&scene, &p).status,
            crate::report::Status::Pass,
            "a refused record never passes"
        );
    }

    #[test]
    fn a_mesh_larger_than_its_fields_inside_the_rounding_is_measured_not_trusted() {
        // At y = 3000 two units in the last place are 4.9e-4 mm. A rod whose fields put its
        // surface 0.3003 mm from the wall (clear) but whose written mesh is 4.5e-4 mm wider
        // reaches 0.29985 mm: the mesh is the one that must clear 0.3. The mesh stands 4.5e-4 mm
        // from its canonical mesh (the record's deviation, which the bounds carry), so the
        // clearance is [0.2994, 0.3003] and straddles the limit: Unverified, never a pass on
        // the fields.
        let wall = kit::closed_box([-2.0, 2990.0, -10.0], [0.0, 3010.0, 10.0]);
        let x = 0.550_3_f32;
        let (s, e) = ([x, 3000.0, -5.0], [x, 3000.0, 5.0]);
        let rec = kit::rec(3, s, e, 0.25, 0.25);
        let part = kit::part_with_mesh(0, &rec, &kit::frustum_coords(s, e, 0.250_45, 0.250_45));
        assert!(part.is_valid(), "{:?}", part.invalid);
        let scene = Scene::from_parts(&limits(), wall.clone(), [0.0; 3], vec![part]);
        let p = patch::measure(&scene);
        let o = measure(&scene, &p);
        assert_eq!(o.status, crate::report::Status::Unverified, "{o:?}");
        // The same rod, 0.31 mm off, with a mesh that matches its fields is clear.
        let (s, e) = ([0.56, 3000.0, -5.0], [0.56, 3000.0, 5.0]);
        let ok = kit::part(0, 3, s, e, 0.25, 0.25);
        let scene = Scene::from_parts(&limits(), wall, [0.0; 3], vec![ok]);
        let p = patch::measure(&scene);
        assert_eq!(
            measure(&scene, &p).status,
            crate::report::Status::Pass,
            "{:?}",
            measure(&scene, &p)
        );
    }

    /// Review round 7: a native cylinder cap whose one top-ring vertex stands two ULPs higher
    /// is still accepted by validation (it is inside the canonical allowance), but it is not
    /// convex. The model bottom stands 0.3 + 3e-6 mm over the unperturbed cap; the raised
    /// vertex leaves 0.29998779 mm, which the written triangles show. The raise is the record's
    /// own deviation from its canonical mesh (2 ULP, 1.5e-5 mm), which every bound carries
    /// (ruling a): a clearance within that of the limit is Unverified, whichever side of the
    /// limit it falls on (round-8 review: 0.30000305 mm used to pass), and one clear of it
    /// is decided.
    #[test]
    fn a_cap_vertex_raised_inside_the_validation_allowance_is_measured_as_written() {
        use crate::report::Status;
        let (a, b) = ([0.0, 0.0, 99.0], [0.0, 0.0, 100.0]);
        let record = kit::rec(3, a, b, 0.5, 0.5);
        let original = kit::frustum_coords(a, b, 0.5, 0.5);
        let mut top: Vec<[f32; 3]> = original
            .chunks_exact(3)
            .filter(|v| v[2].to_bits() == 100.0f32.to_bits())
            .map(|v| [v[0], v[1], v[2]])
            .collect();
        top.sort_by(|p, q| p[0].total_cmp(&q[0]).then(p[1].total_cmp(&q[1])));
        top.dedup();
        assert_eq!(top.len(), 16, "the top ring has 16 vertices");
        let raised = f32::from_bits(100.0f32.to_bits() + 2);
        let rod_with = |v: [f32; 3]| {
            let mut coords = original.clone();
            for p in coords.chunks_exact_mut(3) {
                if p.iter().zip(v).all(|(x, y)| x.to_bits() == y.to_bits()) {
                    p[2] = raised;
                }
            }
            let rod = kit::part_with_mesh(0, &record, &coords);
            assert!(rod.is_valid(), "validation allows it: {:?}", rod.invalid);
            rod
        };
        let status_with = |rod: Part, bottom: f32| {
            let model = kit::closed_box([-1.0, -1.0, f64::from(bottom)], [1.0, 1.0, 102.0]);
            let scene = Scene::from_parts(&limits(), model, [0.0; 3], vec![rod]);
            let g = part_gap(&scene, 0, 0.3, 0.3);
            let truth = f64::from(bottom) - f64::from(raised);
            assert!(
                g.lb <= truth && truth <= g.ub,
                "[{}, {}] must hold the written clearance {truth}",
                g.lb,
                g.ub
            );
            let dev = scene.parts[0].mesh.as_ref().map_or(f64::NAN, Solid::dev);
            assert!(
                g.lb <= truth - dev && truth + dev <= g.ub,
                "[{}, {}] must hold the written clearance {truth} +- the deviation {dev}",
                g.lb,
                g.ub
            );
            measure(&scene, &patch::measure(&scene)).status
        };
        // 0.29998779 mm written (under the limit) and 0.30000305 mm (over it): both inside the
        // deviation of the limit, so neither is decided.
        for v in top {
            for bottom in [100.3f32, 100.300_02] {
                let s = status_with(rod_with(v), bottom);
                assert_eq!(s, Status::Unverified, "vertex {v:?}, model bottom {bottom}");
            }
        }
        // Clear of the deviation either way: decided.
        let v = top_first(&original);
        assert_eq!(status_with(rod_with(v), 100.299), Status::Violation);
        assert_eq!(status_with(rod_with(v), 100.3002), Status::Pass);
    }

    /// The first top-ring vertex of a native cylinder cap at z = 100.
    fn top_first(coords: &[f32]) -> [f32; 3] {
        coords
            .chunks_exact(3)
            .find(|v| v[2].to_bits() == 100.0f32.to_bits())
            .map(|v| [v[0], v[1], v[2]])
            .expect("a top vertex")
    }

    /// A rigid turn about the z axis by `a` radians.
    fn turn_z(p: [f64; 3], a: f64) -> [f64; 3] {
        let (s, c) = a.sin_cos();
        [c * p[0] - s * p[1], s * p[0] + c * p[1], p[2]]
    }

    #[test]
    fn a_polygon_gap_that_shrinks_along_a_tapered_cone_is_not_a_departure() {
        // Round-3 review: a tip cone 0.45 -> 0.05 along a wall keeps its analytic gap (about
        // 0.010 mm) while its written 16-gon stands a sagitta r (1 - cos(pi / 16)) inside the
        // circle, a sagitta that falls from 0.0086 to 0.001 mm along the cone. With a flat
        // face of the polygon toward the wall (the whole scene is turned by half a ring step,
        // 11.25 degrees, about z, which moves the canonical vertices against the wall) the
        // written gap falls by about 0.007 mm, over the 0.005 mm slack, though the analytic
        // gap does not. The native mesh is built from the fields, so this is a canonical
        // mesh and the departure must be measured on it.
        let a = 11.25f64.to_radians();
        let turn = |p: [f64; 3]| turn_z(p, a);
        #[allow(clippy::cast_possible_truncation, reason = "f32 fields")]
        let f32v = |p: [f64; 3]| [p[0] as f32, p[1] as f32, p[2] as f32];
        let s = f32v(turn([0.455_855_816_602_706_9, 0.0, 0.0]));
        let e = f32v(turn([
            0.059_539_530_426_263_81,
            0.0,
            2.900_000_095_367_431_6,
        ]));
        let ball = kit::part(0, 7, [s[0], s[1], -0.5], [s[0], s[1], 0.5], 0.5, 0.5);
        let cone = kit::part(1, 1, s, e, 0.45, 0.05);
        assert!(ball.is_valid() && cone.is_valid());
        let wall: Vec<Tri> = kit::closed_box([-2.0, -10.0, -10.0], [0.0, 10.0, 10.0])
            .into_iter()
            .map(|t| t.map(turn))
            .collect();
        let scene = Scene::from_parts(&limits(), wall, [0.0; 3], vec![ball, cone]);
        let p = patch::measure(&scene);
        let o = measure(&scene, &p);
        let text = format!("{o:?}");
        assert_ne!(o.status, crate::report::Status::Pass, "{text}");
        assert!(
            text.contains("closer") || text.contains("departure"),
            "the departure must be what fires: {text}"
        );
    }

    #[test]
    fn a_tip_cone_that_closes_on_the_wall_after_the_ball_is_a_violation() {
        // Ball radius 0.2 centred 0.1 from the wall (it touches it); the cone along the wall
        // grows from radius 0.05 to 0.09 about the line x = 0.1, so its surface goes from
        // 0.05 to 0.01 mm from the wall: it approaches after leaving the ball.
        let ball = kit::part(0, 7, [0.1, 0.0, 4.8], [0.1, 0.0, 5.2], 0.2, 0.2);
        let closing = kit::part(1, 1, [0.1, 0.0, 5.0], [0.1, 0.0, 6.5], 0.05, 0.09);
        let o = run(vec![ball, closing]);
        assert_eq!(o.status, crate::report::Status::Violation, "{o:?}");
        assert!(
            o.located.iter().any(|l| l.reason.contains("comes closer")),
            "{o:?}"
        );
        // The same ball with a cone leaning away from the wall departs.
        let ball = kit::part(0, 7, [0.1, 0.0, 4.8], [0.1, 0.0, 5.2], 0.2, 0.2);
        let away = kit::part(1, 1, [0.1, 0.0, 5.0], [1.6, 0.0, 5.5], 0.05, 0.09);
        let ok = run(vec![ball, away]);
        assert_eq!(ok.status, crate::report::Status::Pass, "{ok:?}");
        let text = ok.detail.render();
        assert!(text.contains("capped departure (cap 0.35 mm)"), "{ok:?}");
        assert!(!text.contains("never decreases"), "{ok:?}");
    }

    /// A closed wall like `wall()` that ends at z = 2.0: a cone along it that reaches z = 2.9
    /// leaves the model there, so its gap reaches the region end's minimum.
    fn short_wall() -> Vec<Tri> {
        kit::closed_box([-2.0, -10.0, -10.0], [0.0, 10.0, 2.0])
    }

    /// The round-4 wall: closed, its face at x = 0, with a recess to x = -0.15 between z = 0.55
    /// and `0.55 + width`, across all of y, ending at z = 2.0 (three abutting boxes; the faces
    /// they share lie at x <= -0.15 and are farther from the cone than the exposed ones).
    fn recess_wall(width: f64) -> Vec<Tri> {
        let top = 0.55 + width;
        let mut w = kit::closed_box([-2.0, -10.0, -10.0], [0.0, 10.0, 0.55]);
        w.extend(kit::closed_box([-2.0, -10.0, 0.55], [-0.15, 10.0, top]));
        w.extend(kit::closed_box([-2.0, -10.0, top], [0.0, 10.0, 2.0]));
        w
    }

    /// A canonical radius 0.2 tip ball at [0.11, 0, 0] and a type-1 cone of radius 0.06 from
    /// its centre to z = 2.9, over `wall`: its surface is 0.05 mm from a wall face at x = 0.
    fn recess_scene(wall: Vec<Tri>) -> Scene {
        let ball = kit::part(0, 7, [0.11, 0.0, -0.2], [0.11, 0.0, 0.2], 0.2, 0.2);
        let cone = kit::part(1, 1, [0.11, 0.0, 0.0], [0.11, 0.0, 2.9], 0.06, 0.06);
        assert!(ball.is_valid() && cone.is_valid());
        Scene::from_parts(&limits(), wall, [0.0; 3], vec![ball, cone])
    }

    fn recess_outcome(width: f64) -> Outcome {
        let scene = recess_scene(recess_wall(width));
        let p = patch::measure(&scene);
        measure(&scene, &p)
    }

    #[test]
    fn a_recess_inside_one_slab_cannot_hide_a_decrease() {
        // Round-4 review: with a 0.2 mm recess between z = 0.55 and 0.75 the gap of the cone
        // rises from 0.05 to 0.108 mm and falls to 0.048 mm again, all inside one of the eight
        // slabs the old measurement cut the cone into. Its slab minimum was the same as for a
        // flat wall, so it passed and tested no part. Now the rise and the fall are bounded
        // themselves.
        let o = recess_outcome(0.2);
        assert_eq!(o.status, crate::report::Status::Violation, "{o:?}");
        assert!(
            o.located.iter().any(|l| l.reason.contains("comes closer")),
            "{o:?}"
        );
        // The same cone over a flat wall that ends at z = 2.0 departs there.
        let flat = recess_scene(short_wall());
        let p = patch::measure(&flat);
        let ok = measure(&flat, &p);
        assert_eq!(ok.status, crate::report::Status::Pass, "{ok:?}");
        assert!(
            ok.detail
                .render()
                .contains("capped departure (cap 0.35 mm)"),
            "{ok:?}"
        );
    }

    #[test]
    fn a_cone_that_stays_close_to_the_wall_to_the_region_end_never_departs() {
        // Round-5 review: a radius 0.06 cone parallel to the wall at a gap of 0.05 mm the whole
        // way never decreases, and used to pass. The rule asks for at least 0.32 mm at the
        // region end whether or not the cap was reached.
        let flat = recess_scene(wall());
        let p = patch::measure(&flat);
        let o = measure(&flat, &p);
        assert_eq!(o.status, crate::report::Status::Violation, "{o:?}");
        assert!(
            o.located
                .iter()
                .any(|l| l.reason.contains("end of the region")),
            "{o:?}"
        );
        // The same cone with its far end clear of the wall (the wall ends at z = 2.0): a pass.
        let (o, _) = departure_detail(&recess_scene(short_wall()));
        assert_eq!(o.status, crate::report::Status::Pass, "{o:?}");
        // A gap that stays at 0.39 mm to the end is a pass; one at 0.315 mm is not.
        let (o, _) = departure_detail(&far_scene(wall()));
        assert_eq!(o.status, crate::report::Status::Pass, "{o:?}");
        let near = |x: f32| {
            let ball = kit::part(0, 7, [x, 0.0, -0.5], [x, 0.0, 0.5], 0.5, 0.5);
            let cone = kit::part(1, 1, [x, 0.0, 0.0], [x, 0.0, 2.9], 0.06, 0.06);
            Scene::from_parts(&limits(), wall(), [0.0; 3], vec![ball, cone])
        };
        let (o, _) = departure_detail(&near(0.375));
        assert_eq!(o.status, crate::report::Status::Violation, "{o:?}");
        let (o, _) = departure_detail(&near(0.395));
        assert_eq!(o.status, crate::report::Status::Pass, "{o:?}");
    }

    /// Round-5 review: a canonical radius 0.5 contact ball at [0.45, 0, 0] (it reaches 0.05 mm
    /// into the wall) and a radius 0.06 tip cone to z = 2.9: the cone's surface is 0.39 mm from
    /// a wall face at x = 0, 0.54 mm from the recess floor.
    fn far_scene(wall: Vec<Tri>) -> Scene {
        let ball = kit::part(0, 7, [0.45, 0.0, -0.5], [0.45, 0.0, 0.5], 0.5, 0.5);
        let cone = kit::part(1, 1, [0.45, 0.0, 0.0], [0.45, 0.0, 2.9], 0.06, 0.06);
        assert!(ball.is_valid() && cone.is_valid());
        Scene::from_parts(&limits(), wall, [0.0; 3], vec![ball, cone])
    }

    fn departure_detail(scene: &Scene) -> (Outcome, Json) {
        let p = patch::measure(scene);
        let o = measure(scene, &p);
        let d = o
            .detail
            .get("departure")
            .and_then(|a| match a {
                Json::Arr(v) => v.first().cloned(),
                _ => None,
            })
            .expect("one departure");
        (o, d)
    }

    #[test]
    fn a_decrease_above_the_cap_is_capped_departure_with_the_real_gaps_reported() {
        // Round-5 review: the gap rises to 0.54 mm over the recess and falls back to 0.39 mm.
        // Above the 0.35 mm cap the rule allows that (the gap stays at or above 0.32 mm), so
        // it passes, but the report names the rule "capped departure (cap 0.35 mm)" and shows
        // the measured gaps (0.39 to 0.54 mm), never 0.35 mm clipped values.
        let scene = far_scene(recess_wall(0.2));
        let (o, d) = departure_detail(&scene);
        assert_eq!(o.status, crate::report::Status::Pass, "{o:?}");
        let text = d.render();
        assert!(text.contains("capped departure (cap 0.35 mm)"), "{text}");
        assert!(!text.contains("never decreases"), "{text}");
        assert!(
            matches!(d.get("analytic_information"), Some(Json::Str(s)) if s.contains("analytic cone")),
            "{text}"
        );
        {
            let rep = "written";
            let g = d.get("gap_samples").expect("samples");
            let Some(Json::Arr(ends)) = g.get(&format!("{rep}_gap_ends_mm")) else {
                panic!("no ends: {text}");
            };
            for row in ends {
                let Json::Arr(cells) = row else {
                    panic!("{text}")
                };
                let [Json::Num(_), Json::Num(lo), Json::Num(hi)] = cells.as_slice() else {
                    panic!("{text}");
                };
                // Measured, not clipped: the bounds are not both 0.35.
                assert!(*lo > 0.37 && *hi >= *lo, "{rep}: [{lo}, {hi}] {text}");
            }
            let Some(Json::Num(low)) = g.get(&format!("{rep}_lowest_lower_bound_from_cap_mm"))
            else {
                panic!("no lowest: {text}");
            };
            assert!(*low > 0.37 && *low < 0.40, "{rep}: {low} {text}");
        }
        // The flat wall has the gap at 0.39 throughout: also a pass, with those real values.
        let flat = far_scene(wall());
        let (o, d) = departure_detail(&flat);
        assert_eq!(o.status, crate::report::Status::Pass, "{o:?}");
        assert!(d.render().contains("capped departure (cap 0.35 mm)"));
    }

    /// The wall with a bump of height `h` (to x = h) between z = 1.0 and 1.2 across all of y.
    fn bump_wall(h: f64) -> Vec<Tri> {
        let mut w = kit::closed_box([-2.0, -10.0, -10.0], [0.0, 10.0, 1.0]);
        w.extend(kit::closed_box([-2.0, -10.0, 1.0], [h, 10.0, 1.2]));
        w.extend(kit::closed_box([-2.0, -10.0, 1.2], [0.0, 10.0, 10.0]));
        w
    }

    #[test]
    fn a_fall_under_the_floor_after_the_cap_is_a_violation_and_one_above_it_is_not() {
        // The gap sits at 0.39 mm, past the cap; a bump 0.04 mm high takes it to 0.35 mm (above
        // the 0.32 mm floor: allowed, though it is a decrease), one 0.10 mm high to 0.29 mm
        // (under the floor: a violation, whatever the cap).
        let ok = far_scene(bump_wall(0.04));
        let (o, _) = departure_detail(&ok);
        assert_eq!(o.status, crate::report::Status::Pass, "{o:?}");
        let bad = far_scene(bump_wall(0.10));
        let (o, _) = departure_detail(&bad);
        assert_eq!(o.status, crate::report::Status::Violation, "{o:?}");
        assert!(
            o.located
                .iter()
                .any(|l| l.reason.contains("departure floor")),
            "{o:?}"
        );
        // Sabotage guard: a bump that dips the gap by 0.05 mm below the cap's floor needs the
        // floor test, not the monotone one (the gap was already past the cap).
        assert!(
            !o.located.iter().any(|l| l.reason.contains("comes closer")),
            "{o:?}"
        );
    }

    #[test]
    fn the_floor_after_the_cap_takes_no_slack() {
        // Round-5 review: a bump 0.073 mm high takes the 0.39 mm gap to 0.317 mm, 0.003 mm
        // under the 0.32 mm floor (0.3160 mm as a lower bound). The intervals already hold the
        // measurement uncertainty, so 0.005 mm must not be taken off the floor again.
        for h in [0.073, 0.0705] {
            let (o, _) = departure_detail(&far_scene(bump_wall(h)));
            assert_eq!(
                o.status,
                crate::report::Status::Violation,
                "bump {h}: {o:?}"
            );
            assert!(
                o.located
                    .iter()
                    .any(|l| l.reason.contains("departure floor")),
                "bump {h}: {o:?}"
            );
        }
        // A bump that leaves 0.325 mm is a pass.
        let (o, _) = departure_detail(&far_scene(bump_wall(0.065)));
        assert_eq!(o.status, crate::report::Status::Pass, "{o:?}");
    }

    #[test]
    fn a_dip_over_the_slack_is_never_a_pass_and_one_under_it_is_not_a_violation() {
        // The gap at the middle of a recess of width w is sqrt(0.05^2 + (w / 2)^2): a 0.03 mm
        // recess raises it by 0.0022 mm (inside the 0.005 mm slack and the cone then falls
        // back by that much), a 0.05 mm recess by 0.0059 mm and a 0.3 mm one by 0.108 mm.
        let small = recess_outcome(0.03);
        assert_eq!(small.status, crate::report::Status::Pass, "{small:?}");
        let over = recess_outcome(0.05);
        assert_ne!(over.status, crate::report::Status::Pass, "{over:?}");
        let big = recess_outcome(0.3);
        assert_eq!(big.status, crate::report::Status::Violation, "{big:?}");
    }

    #[test]
    fn a_rod_inside_the_model_is_not_clear() {
        // A closed 20 mm cube, and a rod at its centre.
        let h = 10.0;
        let p = |i: u8| {
            [
                if i & 1 == 0 { -h } else { h },
                if i & 2 == 0 { -h } else { h },
                if i & 4 == 0 { -h } else { h },
            ]
        };
        let quads = [
            [0u8, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ];
        let mut cube = Vec::new();
        for q in quads {
            cube.push([p(q[0]), p(q[1]), p(q[2])]);
            cube.push([p(q[0]), p(q[2]), p(q[3])]);
        }
        let scene = Scene::from_parts(
            &limits(),
            cube,
            [0.0; 3],
            vec![kit::part(
                0,
                3,
                [0.0, 0.0, -3.0],
                [0.0, 0.0, 3.0],
                0.25,
                0.25,
            )],
        );
        assert!(scene.model_closed);
        let p = patch::measure(&scene);
        let o = measure(&scene, &p);
        assert_eq!(o.status, crate::report::Status::Violation, "{o:?}");
        assert!(o.located[0].reason.contains("inside the model"), "{o:?}");
    }

    #[test]
    fn a_contact_ball_that_touches_two_model_sheets_is_not_exempt_for_the_second() {
        // Two closed walls with a 0.25 mm gap (faces at x = 0 and x = 0.25); a radius 0.2 tip
        // ball centred in the gap touches both. Only one patch can be the intended contact.
        let mut walls = kit::closed_box([-2.0, -10.0, -10.0], [0.0, 10.0, 10.0]);
        walls.extend(kit::closed_box([0.25, -10.0, -10.0], [2.25, 10.0, 10.0]));
        let ball = kit::part(0, 7, [0.1, 0.0, 4.8], [0.1, 0.0, 5.2], 0.2, 0.2);
        let scene = Scene::from_parts(&limits(), walls, [0.0; 3], vec![ball]);
        let p = patch::measure(&scene);
        assert_eq!(p.reps[0].comps.len(), 2, "{:?}", p.reps[0].comps);
        let o = measure(&scene, &p);
        assert_eq!(o.status, crate::report::Status::Violation, "{o:?}");
        assert!(
            o.located
                .iter()
                .any(|l| l.reason.contains("separate places")),
            "{o:?}"
        );
        // One wall only: the same ball is the contact and nothing else.
        let wall = kit::closed_box([-2.0, -10.0, -10.0], [0.0, 10.0, 10.0]);
        let ball = kit::part(0, 7, [0.1, 0.0, 4.8], [0.1, 0.0, 5.2], 0.2, 0.2);
        let scene = Scene::from_parts(&limits(), wall, [0.0; 3], vec![ball]);
        let p = patch::measure(&scene);
        let ok = measure(&scene, &p);
        assert_eq!(ok.status, crate::report::Status::Pass, "{ok:?}");
    }

    #[test]
    fn a_model_only_file_is_not_applicable() {
        let o = run(Vec::new());
        assert_eq!(o.status, crate::report::Status::NotApplicable);
    }
}
