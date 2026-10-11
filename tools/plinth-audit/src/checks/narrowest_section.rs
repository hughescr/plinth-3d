//! I3 min section -- `docs/specs/audit-invariants.md` §3.3.
//!
//! The area law at the arm's own narrowest cross-section, not at the tip.
//! `docs/physics.md` §2(b) names this as the true failure section, and it has
//! never been checkable because the emitted stream carries no arm-to-contact
//! link. This module reconstructs it from the graph the audit already builds:
//! walk from each contact's TIP record rootward until a second live load path
//! merges (in-degree > 1) or ground is reached, sample the resulting
//! centreline at the finest station the printer can render, cut perpendicular
//! to the local tangent at each station, and take the area of the connected
//! component of the **cured support raster** containing that station -- the
//! same solid I2 grades, so the two invariants never disagree about what
//! "the support" is.
//!
//! ## The load-path walk
//!
//! Ground is exactly `audit-invariants.md` §3.5's two clauses: (i) the
//! primitive's own solid reaches the plate (`aabb().min[2] <= z_plate +
//! layer_height`, which is exact because every [`Primitive::aabb`] here is the
//! TIGHT box), or (ii) the primitive is itself a TIP whose own seat already
//! satisfies I1's predicate (`h_delivered > 0` and, in the conservative
//! minimum-type form of `audit-interval-policy.md` §3.2,
//! `h_delivered - tol_geom >= h_req`), recomputed here from
//! [`crate::seat::Seat`] rather than trusted
//! from anywhere else. **This is narrower than the full clause (ii)**: the
//! specification also requires the seat zone to pass I6, which this file does
//! not reimplement (I6 owns that machinery, in its own file). A tip that
//! passes I1 alone but would fail I6's far-side test is treated as grounded
//! here when it should not be -- see the disclosure at the end of this module.
//!
//! **Clause (ii) excludes the contact's own tip from its own walk.** A
//! walk that starts at TIP `c` and asks "is `c` itself grounded" would say
//! yes whenever `c`'s own seat is adequate, terminating the walk at distance
//! zero before it ever reaches the arm -- which is exactly the tip-only
//! measurement I3 exists to NOT make. Measured on `sphere-first-artifact`
//! while building this check: every one of 9 properly-embedded contacts
//! (`h_delivered = 0.300 mm` against `h_required` of roughly 0.17-0.22 mm)
//! satisfied clause (ii) on itself and reported `path_records: [contact_id]`
//! and `station_mm: 0` -- the tip's own patch, over and over, never the arm.
//! So the ground set used for contact `c`'s own walk is `ground(r) && r !=
//! c`: a tip's valid seat can ground a DIFFERENT primitive's path (the
//! accepted case is a brace that lands a second, independent anchor
//! elsewhere in the model), never terminate its own.
//!
//! `in_degree(r)` is the number of neighbours of `r` one step FARTHER from
//! ground (i.e. the number of primitives that would step into `r` walking
//! rootward). Rootward stepping moves to the unique neighbour one step closer
//! to ground; on the rare tie the lowest record index wins, mirroring the
//! project's own tie-break convention for the ownership partition
//! (`audit-invariants.md` §2.2). The walk stops -- inclusive of the primitive
//! it lands on -- at the first primitive whose in-degree exceeds one, or at a
//! grounded primitive.
//!
//! ## The section itself
//!
//! Exact polygon-on-voxel clipping of an oblique plane against the whole
//! per-contact raster was tried and rejected: `RunRaster::voxels_in_box` has
//! no way to express "thin along an arbitrary direction", so an axis-aligned
//! query wide enough to bound the plane's footprint enumerates the raster's
//! full local volume rather than the thin sheet actually cut, and doing that
//! at every one of a few thousand stations per contact does not finish in a
//! reasonable time. Instead each station's area is a **flood-fill point
//! quadrature** of the same cured raster: seed at the station itself (which
//! must already be cured, or the station -- and the whole contact -- is
//! `section_component_empty`), expand in the plane's own orthonormal basis at
//! a step of `sqrt(pixel_pitch_x * pixel_pitch_y)`, 4-connectivity, summing
//! `step^2` per visited cell. This reads the SAME cured solid I2 reads
//! (`ModelContext`'s analytic-primitive raster, not a re-rasterisation), and
//! its cost is bounded per station by a search radius sized off the local
//! primitive's own nominal radius, not a global bound -- see
//! [`section_budget`]. The discretisation error this introduces is disclosed,
//! not hidden: see "the smallest defect this check cannot see" below.
//!
//! ## What is already built for you, that this file leans on
//!
//! [`crate::graph::SupportGraph::distance_to_ground`] and
//! `neighbours_of` for the walk; [`Primitive::centreline`] and the
//! primitive's own two end radii for the polyline; [`RunRaster::from_primitives`]
//! built fresh per contact from exactly `path(c)`'s own primitives (never the
//! whole model's support raster, per the specification's "the analytic union
//! of path(c)'s primitives"); [`RunRaster::contains_point`] as the one cured
//! read the quadrature needs; [`crate::ownership`] for `A_owned`; and
//! [`crate::consts::tol_area`] for the law's own tolerance.
//!
//! ## The failure codes this check emits
//!
//! `MIN_SECTION_SHORT`, `MIN_SECTION_INDETERMINATE` (reason `path_open`,
//! `path_cycles`, `ownership_indeterminate` or `section_component_empty`).
//!
//! ## The smallest defect this check cannot see
//!
//! Everything §3.3's own "smallest defect" paragraph names (the elbow
//! optimism of a perpendicular-only cut, a thin-ribbon section reading as
//! adequate, and the un-gated shared-trunk sum) still applies unchanged here;
//! read it there. Beyond that, three things are specific to THIS
//! implementation and are disclosed rather than smoothed over:
//!
//! 1. **The area quadrature is a discretised point sample, not an exact
//!    polygon area.** Its resolution is `sqrt(pixel_pitch_x*pixel_pitch_y) ≈
//!    0.0163 mm`, and unlike `tol_area` (which is a derived summation bound on
//!    `A_owned`), this sampling error is NOT folded into the predicate's
//!    tolerance -- it is an additional, undisclosed-by-the-specification slop
//!    on `A_min` of very roughly one cell's width along the section's true
//!    boundary. A margin narrower than a few times `step^2` per mm of
//!    perimeter is not proven by this number alone.
//! 2. **Grounding via clause (ii) is I1's predicate alone, not I1 AND I6.** A
//!    tip that punches through a thin wall (which only I6's far-side test
//!    catches) can still be treated as "grounded" here, silently truncating a
//!    walk that should have continued past it. This narrows the walk in the
//!    unsafe direction (a shorter path is graded, so a genuinely thin
//!    downstream member is never reached) rather than the safe one.
//! 3. **The rootward tie-break (lowest record index) is not stated by the
//!    specification for I3.** §3.5 clause (c) only requires "at least one"
//!    closer neighbour, which is correct for I5's undirected connectivity
//!    question but leaves I3's single-path arc-length walk genuinely
//!    ambiguous whenever a primitive has more than one neighbour exactly one
//!    step closer to ground. This file picks deterministically rather than
//!    reporting `path_cycles`, on the view that a brace (the case the
//!    specification names as legitimate) should not make every contact
//!    downstream of it indeterminate; a reviewer may reasonably prefer the
//!    stricter reading.

use crate::check::{Check, CheckOutcome, Failure};
use crate::checks::interval::{self, Verdict};
use crate::consts as c;
use crate::context::ModelContext;
use crate::graph::SupportGraph;
use crate::json::Json;
use crate::primitives::{GeomError, Primitive};
use crate::raster::{Budget, RasterGrid, RunRaster};
use crate::reader::{RecordKind, SupportRecord};
use crate::seat::Seat;
use crate::vec3::{self as v, V3};
use std::collections::{HashSet, VecDeque};

pub struct NarrowestSection;

impl Check for NarrowestSection {
    fn id(&self) -> &'static str {
        "i3_min_section"
    }

    fn title(&self) -> &'static str {
        "I3 min section"
    }

    fn scope(&self, ctx: &ModelContext) -> usize {
        ctx.seats.len()
    }

    fn run(&self, ctx: &ModelContext) -> CheckOutcome {
        let file = ctx.file();
        let scope = self.scope(ctx);
        let mut out = CheckOutcome::new(scope);

        let step = ctx
            .inputs()
            .pixel_pitch_x_mm
            .min(ctx.inputs().pixel_pitch_y_mm)
            .min(ctx.inputs().layer_height_mm)
            / 2.0;
        out.measured.set("arc_length_step_mm", Json::num(step));
        out.measured.set(
            "section_sampling_method",
            Json::str(
                "flood-fill point quadrature of the cured per-contact support raster; \
                 in-plane step sqrt(pixel_pitch_x*pixel_pitch_y), 4-connectivity; \
                 this discretisation error is NOT folded into tol_area -- see module docs",
            ),
        );

        if scope == 0 {
            out.measured
                .set("headline", Json::str("no contacts on this model"));
            out.measured.set("contacts", Json::Arr(Vec::new()));
            return out;
        }

        let ground = ground_flags(ctx);
        let x_allowed = ctx.inputs().x_allowed;

        let mut rows = Vec::with_capacity(scope);
        // (margin, a_min, a_required) for the worst contact actually measured.
        let mut worst: Option<(f64, f64, f64)> = None;
        // Sections measured but not resolvable against their own requirement
        // (`audit-interval-policy.md` §3.2 row 10), kept apart from the
        // could-not-measure-at-all indeterminates below.
        let mut band_indeterminate_count = 0usize;

        // Every contact is evaluated on its own (its walk, its per-contact raster and its
        // stations read nothing another contact writes), so the contacts are evaluated in
        // parallel and the loop below reads the results in contact order: the outcome is the
        // sequential one, bit for bit. The shared products (the ownership partition) are
        // built first, here, because the context builds them lazily and is not `Sync`.
        let ownership_exceeded = ctx.ownership().budget_exceeded.is_some();
        let a_owned: Vec<Option<f64>> = ctx.seats.iter().map(|s| ctx.a_owned_mm2(s.contact_id)).collect();
        let shared = Shared {
            graph: &ctx.graph,
            primitives: &ctx.primitives,
            records: &ctx.model.records,
            grid: ctx.grid,
            budget: ctx.audit.options.raster_budget,
            ownership_exceeded,
            ground: &ground,
        };
        let seats = &ctx.seats;
        let evals = par_map(seats.len(), crate::geometry::default_threads(), |i| {
            evaluate_contact(&shared, &seats[i], a_owned[i], step)
        });

        for (seat, ev) in ctx.seats.iter().zip(evals) {
            let cid = seat.contact_id;
            match ev {
                Ok(r) => {
                    let a_required_mm2 = r.a_owned_mm2 / x_allowed;
                    let tol = c::tol_area(r.a_owned_mm2, x_allowed);
                    let verdict = area_law_verdict(r.a_min_mm2, r.a_owned_mm2, x_allowed);
                    let x_delivered =
                        r.a_min_mm2 / (std::f64::consts::PI * r.r_at_min_mm.max(1.0e-9).powi(2));
                    let margin = r.a_min_mm2 - a_required_mm2;
                    if worst.is_none_or(|(m, ..)| margin < m) {
                        worst = Some((margin, r.a_min_mm2, a_required_mm2));
                    }
                    // The straddling band: the section was measured, and the
                    // instrument cannot resolve it against its own
                    // requirement. It gates as a failure and carries its
                    // interval (`audit-interval-policy.md` §5).
                    if verdict == Verdict::Indeterminate {
                        let (edge_lo, edge_hi) = interval::edges(r.a_min_mm2, tol);
                        band_indeterminate_count += 1;
                        out.indeterminate(Failure::line(
                            "MIN_SECTION_INDETERMINATE",
                            &file,
                            ctx.model_index,
                            Some(cid),
                            Some(r.primitive_record),
                            &[
                                (
                                    "reason",
                                    interval::REASON_WITHIN_TOLERANCE_BAND.to_string(),
                                ),
                                ("a_min_mm2", fmt(r.a_min_mm2)),
                                (
                                    "a_min_interval_mm2",
                                    interval::fmt_interval(r.a_min_mm2, tol),
                                ),
                                ("a_required_mm2", fmt(a_required_mm2)),
                                ("tolerance_mm2", fmt(tol)),
                            ],
                            {
                                let mut v = Json::obj();
                                v.set(
                                    "reason",
                                    Json::str(interval::REASON_WITHIN_TOLERANCE_BAND),
                                );
                                v.set("station_mm", Json::num(r.station_mm));
                                v.set("primitive_record", Json::usize(r.primitive_record));
                                v.set("a_min_mm2", Json::num(r.a_min_mm2));
                                v.set("a_min_interval_mm2", Json::nums([edge_lo, edge_hi]));
                                v.set("a_required_mm2", Json::num(a_required_mm2));
                                v.set("a_owned_mm2", Json::num(r.a_owned_mm2));
                                v.set("tolerance_mm2", Json::num(tol));
                                v
                            },
                        ));
                    }
                    if verdict == Verdict::Fail {
                        let deficit = ((a_required_mm2 - tol) - r.a_min_mm2).max(0.0);
                        out.fail(Failure::line(
                            "MIN_SECTION_SHORT",
                            &file,
                            ctx.model_index,
                            Some(cid),
                            Some(r.primitive_record),
                            &[
                                ("station_mm", fmt(r.station_mm)),
                                ("primitive_record", r.primitive_record.to_string()),
                                ("a_min_mm2", fmt(r.a_min_mm2)),
                                ("a_required_mm2", fmt(a_required_mm2)),
                                ("a_owned_mm2", fmt(r.a_owned_mm2)),
                                ("x_delivered", fmt(x_delivered)),
                                ("r_at_min_mm", fmt(r.r_at_min_mm)),
                            ],
                            {
                                let mut v = Json::obj();
                                v.set("station_mm", Json::num(r.station_mm));
                                v.set("primitive_record", Json::usize(r.primitive_record));
                                v.set("a_min_mm2", Json::num(r.a_min_mm2));
                                v.set("a_required_mm2", Json::num(a_required_mm2));
                                v.set("a_owned_mm2", Json::num(r.a_owned_mm2));
                                v.set("x_delivered", Json::num(x_delivered));
                                v.set("r_at_min_mm", Json::num(r.r_at_min_mm));
                                v.set("deficit_mm2", Json::num(deficit));
                                v.set("tolerance_mm2", Json::num(tol));
                                v
                            },
                        ));
                    }
                    rows.push(contact_row(
                        cid,
                        verdict.as_str(),
                        (verdict == Verdict::Indeterminate)
                            .then_some(interval::REASON_WITHIN_TOLERANCE_BAND),
                        Some(r.a_min_mm2),
                        Some(a_required_mm2),
                        Some(r.a_owned_mm2),
                        Some(x_delivered),
                        Some(r.r_at_min_mm),
                        Some(r.station_mm),
                        Some(r.primitive_record),
                        r.stations_evaluated,
                        r.stations_unseedable,
                        r.unseedable_span_mm,
                        &r.path,
                    ));
                }
                Err(e) => {
                    out.indeterminate(Failure::line(
                        "MIN_SECTION_INDETERMINATE",
                        &file,
                        ctx.model_index,
                        Some(cid),
                        None,
                        &[("reason", e.reason.to_string())],
                        {
                            let mut v = Json::obj();
                            v.set("reason", Json::str(e.reason));
                            v
                        },
                    ));
                    rows.push(contact_row(
                        cid,
                        "indeterminate",
                        Some(e.reason),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        0,
                        0,
                        0.0,
                        &e.path,
                    ));
                }
            }
        }

        out.measured.set("contacts", Json::Arr(rows));
        out.measured.set(
            "area_law_band_indeterminate_count",
            Json::usize(band_indeterminate_count),
        );
        let headline = worst.map_or_else(
            || format!("0 of {scope} contacts had a measurable section"),
            |(_, a_min, a_req)| format!("worst A_min {a_min:.4} mm2 needs {a_req:.4}"),
        );
        out.measured.set("headline", Json::str(headline));
        out
    }
}

// ---------------------------------------------------------------------------
// The predicate, pure and separately testable
// ---------------------------------------------------------------------------

/// The area law of `audit-invariants.md` §3.3,
/// `A_min(c) >= A_owned(c) / x_allowed`, in the conservative
/// **minimum-type** form of `audit-interval-policy.md` §3.2 row 10: the
/// delivered narrowest section must reach the required one, so the
/// conservative bound of `A_min` is its LOWER bound.
///
/// ```text
/// PASS           a_min - tol >= required
/// FAIL           a_min + tol <  required
/// INDETERMINATE  otherwise -- the interval straddles the requirement
/// ```
///
/// **This replaces `a_min >= required - tol`**, which graded `A_min`'s
/// *upper* bound against the requirement and so certified a section
/// genuinely thinner than the area law demands, by up to `tol_area`.
///
/// Any non-finite or otherwise unmeasurable input is
/// [`Verdict::Indeterminate`], never a pass — including the degenerate
/// `x_allowed <= 0` and the physically impossible negative area, which used
/// to return a bare `false` that the caller could not tell apart from a
/// measured shortfall.
#[must_use]
pub fn area_law_verdict(a_min_mm2: f64, a_owned_mm2: f64, x_allowed: f64) -> Verdict {
    if !a_min_mm2.is_finite() || !a_owned_mm2.is_finite() || !x_allowed.is_finite() {
        return Verdict::Indeterminate;
    }
    if a_min_mm2 < 0.0 || a_owned_mm2 < 0.0 || x_allowed <= 0.0 {
        return Verdict::Indeterminate;
    }
    let required = a_owned_mm2 / x_allowed;
    let tol = c::tol_area(a_owned_mm2, x_allowed);
    interval::min_type(a_min_mm2, required, tol)
}

// ---------------------------------------------------------------------------
// Grounding (`audit-invariants.md` §3.5, narrowed -- see module docs)
// ---------------------------------------------------------------------------

fn ground_flags(ctx: &ModelContext) -> Vec<bool> {
    let n = ctx.model.records.len();
    let mut g = vec![false; n];
    let plate_z = ctx.z_plate_mm + ctx.inputs().layer_height_mm;
    for i in 0..n {
        if let Ok(p) = &ctx.primitives[i] {
            // EQUALITY-BAND coincidence test, not a margin test
            // (`audit-interval-policy.md` §3.4): "does this solid's lowest
            // point coincide with the plate", where both sides are stored
            // f32 coordinates. It is NOT converted to the three-outcome
            // interval form -- there is no safety margin under test, so
            // there is no conservative side and no third outcome to add.
            //
            // The one genuine gap §3.4 names is here: this copy carried no
            // tolerance at all while the structurally identical tests in
            // `load_path.rs:212` and `dead_geometry.rs:267` carry `+
            // tol_geom`. Added for consistency across the three copies. It
            // is a widening, so it can only ground MORE records; grounding
            // terminates another contact's walk, so the direction is
            // checked: `seat_reaches_required_depth` above is the clause
            // that had to tighten, and this one is a coincidence test whose
            // tolerance is the resolution of the evidence, not slack.
            let bb = p.aabb();
            let tol = c::tol_geom_mag(bb.min[2].abs().max(plate_z.abs()).max(1.0));
            if bb.min[2] <= plate_z + tol {
                g[i] = true;
            }
        }
        if ctx.model.records[i].kind == RecordKind::ContactSphere {
            if let Some(seat) = ctx.seat(i) {
                if seat_reaches_required_depth(seat) {
                    g[i] = true;
                }
            }
        }
    }
    g
}

/// I1's own predicate (`audit-invariants.md` §3.1), recomputed from the shared
/// [`Seat`] reconstruction rather than trusted from I1's own verdict -- each
/// invariant establishes what it needs independently.
///
/// **Conservative minimum-type form** (`audit-interval-policy.md` §3.2
/// row 2; this is duplicate #2 of the same predicate, and it carried the same
/// copied bug). It replaces `h >= h_req - tol`.
///
/// Grounding is the *permissive* outcome here — a grounded record terminates
/// another contact's load-path walk — so only [`Verdict::Pass`] may ground.
/// An interval that straddles the requirement has not established a valid
/// seat and must not be allowed to end a walk.
fn seat_reaches_required_depth(seat: &Seat) -> bool {
    let (Some(h), Some(h_req)) = (seat.h_delivered_mm, seat.h_required_mm) else {
        return false;
    };
    if h <= 0.0 {
        return false;
    }
    interval::min_type(h, h_req, seat.tol_geom_mm()).is_pass()
}

/// `in_degree(r)`: the count of neighbours one step FARTHER from ground, i.e.
/// the number of primitives that would step into `r` walking rootward.
fn in_degree_of(graph: &SupportGraph, dist: &[Option<u32>]) -> Vec<usize> {
    let n = graph.neighbours.len();
    let mut deg = vec![0usize; n];
    for r in 0..n {
        let Some(dr) = dist[r] else { continue };
        for &nb in graph.neighbours_of(r) {
            if dist[nb] == Some(dr + 1) {
                deg[r] += 1;
            }
        }
    }
    deg
}

// ---------------------------------------------------------------------------
// The load-path walk
// ---------------------------------------------------------------------------

enum PathOutcome {
    Path(Vec<usize>),
    Open,
    Cycle,
}

/// Walk from `tip` rootward, stopping (inclusive) at the first primitive whose
/// in-degree exceeds one, or at a grounded primitive.
fn walk_path(
    graph: &SupportGraph,
    dist: &[Option<u32>],
    in_degree: &[usize],
    tip: usize,
) -> PathOutcome {
    let Some(d0) = dist[tip] else {
        return PathOutcome::Open;
    };
    let mut path = vec![tip];
    let mut current = tip;
    let mut cur_d = d0;
    // A generous cap: every step strictly decreases the remaining distance,
    // so a correct graph never gets close to this. Anything that does is a
    // finding, not a legitimate long arm.
    let cap = graph.neighbours.len() + 2;
    let mut steps = 0usize;
    while cur_d > 0 {
        steps += 1;
        if steps > cap {
            return PathOutcome::Cycle;
        }
        let mut candidates: Vec<usize> = graph
            .neighbours_of(current)
            .iter()
            .copied()
            .filter(|&nb| dist[nb] == Some(cur_d - 1))
            .collect();
        if candidates.is_empty() {
            // BFS distance guarantees at least one such neighbour for a
            // non-ground node; reaching here means the graph and `dist`
            // disagree, which is exactly a "the path could not be
            // established" finding.
            return PathOutcome::Open;
        }
        candidates.sort_unstable();
        let next = candidates[0];
        path.push(next);
        current = next;
        cur_d -= 1;
        if in_degree[current] > 1 {
            break;
        }
    }
    PathOutcome::Path(path)
}

// ---------------------------------------------------------------------------
// The centreline polyline
// ---------------------------------------------------------------------------

/// One piece of the load path's centreline: from `p0` to `p1`, `len` mm long,
/// belonging to `record`, with the underlying primitive's own radius
/// interpolated linearly from `r0` (at `p0`) to `r1` (at `p1`).
struct Seg {
    p0: V3,
    p1: V3,
    len: f64,
    record: usize,
    r0: f64,
    r1: f64,
}

/// Build the centreline of `path(c)`, starting at the delivered seat entry
/// point (`h_delivered` along the tip's own axis, per §3.3: "the first
/// station lies at `h_delivered` inside the ball"), through the tip ball's
/// centre, then through each subsequent primitive oriented by nearest-point
/// continuity. A pass-through joint ball (in-degree <= 1, so not itself the
/// walk's stopping point) is degenerate along its own centreline and
/// contributes no arc length of its own -- its solid is still part of the
/// per-contact raster, so nearby stations see its bulk.
fn build_segments(ctx: &Shared, seat: &Seat, path: &[usize]) -> Option<Vec<Seg>> {
    let u = seat.u?;
    let h = seat.h_delivered_mm?;
    let r_ball = seat.r_ball_mm;
    let e0 = v::add(seat.p0, v::mul(u, h));
    let tip_rec = &ctx.records[path[0]];
    let centre = v::midpoint(tip_rec.start_mm, tip_rec.end_mm);

    let mut segs: Vec<Seg> = Vec::with_capacity(path.len());
    let d0 = v::dist(e0, centre);
    if d0 > 1.0e-9 {
        // The ball's own chord radius at the entry station, `a =
        // sqrt(2*f*r*h - h*h)` with `f*r_tip = r_ball` -- the same ideal cut
        // disc formula I2 reports as `A_ideal`.
        let chord2 = (2.0 * r_ball * h - h * h).max(0.0);
        segs.push(Seg {
            p0: e0,
            p1: centre,
            len: d0,
            record: path[0],
            r0: chord2.sqrt(),
            r1: r_ball,
        });
    }

    let mut last = centre;
    for &r in &path[1..] {
        let Ok(prim) = ctx.primitive(r) else {
            return None;
        };
        let (a, b, ra, rb) = match *prim {
            Primitive::Ball { centre: cc, r: rr } => (cc, cc, rr, rr),
            Primitive::Frustum { a, ra, b, rb } => (a, b, ra, rb),
            Primitive::RaftPad { base, r: rr, h: hh } => {
                (base, [base[0], base[1], base[2] + hh], rr, rr)
            }
            Primitive::RaftBridge { a, b, width, .. } => (a, b, width * 0.5, width * 0.5),
        };
        let da = v::dist(a, last);
        let db = v::dist(b, last);
        let (near, far, rn, rf) = if da <= db { (a, b, ra, rb) } else { (b, a, rb, ra) };
        let seg_len = v::dist(near, far);
        if seg_len > 1.0e-9 {
            segs.push(Seg {
                p0: near,
                p1: far,
                len: seg_len,
                record: r,
                r0: rn,
                r1: rf,
            });
            last = far;
        } else {
            last = near;
        }
    }

    if segs.is_empty() {
        // h_delivered ~= r_ball and the path is the tip alone: still give the
        // walk exactly one station, at the entry point.
        let chord2 = (2.0 * r_ball * h - h * h).max(0.0).sqrt();
        segs.push(Seg {
            p0: e0,
            p1: e0,
            len: 0.0,
            record: path[0],
            r0: chord2,
            r1: chord2,
        });
    }
    Some(segs)
}

/// Locate arc-length `s` on the polyline: the segment it falls in, and the
/// local fraction `[0, 1]` along that segment.
fn locate(segs: &[Seg], cum: &[f64], s: f64) -> (usize, f64) {
    let mut idx = segs.len() - 1;
    for (i, _) in segs.iter().enumerate() {
        if s <= cum[i + 1] + 1.0e-9 {
            idx = i;
            break;
        }
    }
    let seg = &segs[idx];
    let local = if seg.len > 0.0 {
        ((s - cum[idx]) / seg.len).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (idx, local)
}

// ---------------------------------------------------------------------------
// The section area at one station
// ---------------------------------------------------------------------------

/// A multiplicative hash for the flood fill's visited set. The set is only asked "is this cell in
/// it" and "how many", so the hash cannot change a result; the default SipHash was most of the
/// cost of a station.
#[derive(Default, Clone, Copy)]
struct CellHasher(u64);

impl std::hash::Hasher for CellHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ u64::from(b)).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
    }
    fn write_i32(&mut self, i: i32) {
        self.0 = (self.0.rotate_left(5) ^ u64::from(i as u32)).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

type CellSet = HashSet<(i32, i32), std::hash::BuildHasherDefault<CellHasher>>;

/// How many flood-fill cells one station's quadrature may visit before it is
/// declared unmeasurable rather than left to run away. Sized off the local
/// primitive's own nominal radius (with generous headroom for a bulge near a
/// joint), never off a global bound -- a thin member's stations stay cheap
/// regardless of how long the whole path is.
fn section_budget(step: f64, radius_hint: f64) -> usize {
    let r = (radius_hint * 4.0 + 0.2).max(step * 3.0);
    let area = std::f64::consts::PI * r * r;
    let cells = (area / (step * step)).ceil();
    (cells as usize).clamp(256, 300_000)
}

/// Flood-fill quadrature of the cured raster's connected component containing
/// `pos`, on the plane through `pos` perpendicular to `tangent`. `None` means
/// the station could not be established: either `pos` itself is not cured
/// (the raster's full-containment fill rule eroded it away, or the polyline
/// drifted off the true centreline), or the budget was exhausted.
fn cross_section_area(
    raster: &RunRaster,
    pos: V3,
    tangent: V3,
    step: f64,
    budget_cells: usize,
) -> Option<f64> {
    let u = v::normalize(tangent)?;
    let (e1, e2, _) = v::basis_from_axis(u);
    if !raster.contains_point(pos) {
        return None;
    }
    let mut visited: CellSet = HashSet::default();
    let mut frontier: VecDeque<(i32, i32)> = VecDeque::new();
    visited.insert((0, 0));
    frontier.push_back((0, 0));
    while let Some((i, j)) = frontier.pop_front() {
        for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let ni = i + di;
            let nj = j + dj;
            if visited.contains(&(ni, nj)) {
                continue;
            }
            let p = v::add(
                pos,
                v::add(
                    v::mul(e1, f64::from(ni) * step),
                    v::mul(e2, f64::from(nj) * step),
                ),
            );
            if raster.contains_point(p) {
                visited.insert((ni, nj));
                if visited.len() > budget_cells {
                    return None;
                }
                frontier.push_back((ni, nj));
            }
        }
    }
    Some(visited.len() as f64 * step * step)
}

// ---------------------------------------------------------------------------
// Per-contact evaluation
// ---------------------------------------------------------------------------

struct EvalOk {
    a_min_mm2: f64,
    a_owned_mm2: f64,
    station_mm: f64,
    primitive_record: usize,
    r_at_min_mm: f64,
    stations_evaluated: usize,
    /// Stations whose seed point was not cured, so no component existed to
    /// measure. Disclosed on every row; see the `None` arm of the station
    /// loop for why they are counted rather than fatal.
    stations_unseedable: usize,
    unseedable_span_mm: f64,
    path: Vec<usize>,
}

struct EvalIndeterminate {
    reason: &'static str,
    path: Vec<usize>,
}

fn indet(reason: &'static str, path: Vec<usize>) -> EvalIndeterminate {
    EvalIndeterminate { reason, path }
}

/// What one contact's evaluation reads of the model, as plain shared data (`Sync`), so the
/// contacts can be evaluated on several threads. Everything lazy in the context (the
/// ownership partition) is built before this is made.
struct Shared<'a> {
    graph: &'a SupportGraph,
    primitives: &'a [Result<Primitive, GeomError>],
    records: &'a [SupportRecord],
    grid: RasterGrid,
    budget: Budget,
    ownership_exceeded: bool,
    ground: &'a [bool],
}

impl Shared<'_> {
    fn primitive(&self, record: usize) -> Result<&Primitive, GeomError> {
        self.primitives[record].as_ref().map_err(|e| *e)
    }
}

/// `f(0..n)` on up to `threads` threads, results in index order. Work is handed out by an atomic
/// counter, so uneven items (a long stilt, a short post) balance themselves.
fn par_map<T: Send>(n: usize, threads: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let workers = threads.clamp(1, n.max(1));
    if workers == 1 {
        return (0..n).map(f).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut parts: Vec<Vec<(usize, T)>> = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..workers)
            .map(|_| {
                sc.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if i >= n {
                            break;
                        }
                        mine.push((i, f(i)));
                    }
                    mine
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap_or_default()).collect()
    });
    let mut all: Vec<(usize, T)> = parts.drain(..).flatten().collect();
    all.sort_by_key(|x| x.0);
    all.into_iter().map(|x| x.1).collect()
}

fn evaluate_contact(
    ctx: &Shared,
    seat: &Seat,
    a_owned_mm2: Option<f64>,
    step: f64,
) -> Result<EvalOk, EvalIndeterminate> {
    let cid = seat.contact_id;

    // Ownership first: it is cheap relative to the walk and gates everything
    // downstream. A contact whose own routing never established a surface
    // component (§2.2's `component_routing_failed`) has no defined A_owned,
    // never merely a zero one.
    if seat.entry_component.is_none() {
        return Err(indet("ownership_indeterminate", vec![cid]));
    }
    if ctx.ownership_exceeded {
        return Err(indet("ownership_indeterminate", vec![cid]));
    }
    let Some(a_owned_mm2) = a_owned_mm2 else {
        return Err(indet("ownership_indeterminate", vec![cid]));
    };
    if !a_owned_mm2.is_finite() || a_owned_mm2 < 0.0 {
        return Err(indet("ownership_indeterminate", vec![cid]));
    }

    // Ground is computed FRESH per contact, excluding `cid` itself
    // from clause (ii): a tip's own valid seat grounds OTHER
    // primitives' walks (a brace that lands a second, independent
    // anchor in the model), never its own -- otherwise every
    // properly-embedded tip terminates its own walk at distance
    // zero and I3 degenerates into re-grading the tip alone, which
    // is exactly the defect this invariant exists to catch. See the
    // module docs.
    let is_ground_for_this_walk = |r: usize| ctx.ground[r] && r != cid;
    let dist = ctx.graph.distance_to_ground(&is_ground_for_this_walk);
    let in_degree = in_degree_of(ctx.graph, &dist);
    let (dist, in_degree) = (&dist[..], &in_degree[..]);

    let path = match walk_path(ctx.graph, dist, in_degree, cid) {
        PathOutcome::Path(p) => p,
        PathOutcome::Open => return Err(indet("path_open", vec![cid])),
        PathOutcome::Cycle => return Err(indet("path_cycles", vec![cid])),
    };

    let mut path_prims: Vec<Primitive> = Vec::with_capacity(path.len());
    for &r in &path {
        match ctx.primitive(r) {
            Ok(p) => path_prims.push(*p),
            Err(_) => return Err(indet("section_component_empty", path)),
        }
    }
    let raster_path =
        match RunRaster::from_primitives(&path_prims, ctx.grid, &ctx.budget)
        {
            Ok(r) => r,
            Err(_) => return Err(indet("section_component_empty", path)),
        };

    let Some(segs) = build_segments(ctx, seat, &path) else {
        return Err(indet("section_component_empty", path));
    };
    if segs.is_empty() {
        return Err(indet("section_component_empty", path));
    }

    let mut cum = vec![0.0f64; segs.len() + 1];
    for i in 0..segs.len() {
        cum[i + 1] = cum[i] + segs[i].len;
    }
    let total = *cum.last().unwrap_or(&0.0);
    let safe_step = step.max(1.0e-6);
    let n_steps = (total / safe_step).ceil() as usize;

    let mut best: Option<(f64, f64, usize, f64)> = None;
    let mut evaluated = 0usize;
    let mut unseedable = 0usize;
    let mut unseedable_span_mm = 0.0f64;
    let mut fallback_tangent = seat.u.unwrap_or([0.0, 0.0, 1.0]);

    for k in 0..=n_steps {
        let s = (k as f64 * safe_step).min(total);
        let (idx, t) = locate(&segs, &cum, s);
        let seg = &segs[idx];
        let pos = v::add(seg.p0, v::mul(v::sub(seg.p1, seg.p0), t));
        let tangent = v::normalize(v::sub(seg.p1, seg.p0)).unwrap_or(fallback_tangent);
        fallback_tangent = tangent;
        let r_local = seg.r0 + (seg.r1 - seg.r0) * t;
        let budget = section_budget(safe_step, r_local);
        match cross_section_area(&raster_path, pos, tangent, safe_step, budget) {
            Some(a) => {
                evaluated += 1;
                if best.is_none_or(|(ba, ..)| a < ba) {
                    best = Some((a, s, seg.record, r_local));
                }
            }
            None => {
                // The seed point is not cured, so this station has no
                // component to measure. Under the full-containment fill rule
                // this is what happens at the two ENDS of the polyline: the
                // terminal cap of the grounded primitive, and the entry point
                // of a barely-embedded ball, both lie within one layer of a
                // free surface, and no voxel there is fully contained. It is
                // an artefact of the rasteriser's erosion, not a property of
                // the arm.
                //
                // Such a station is COUNTED and disclosed, never silently
                // dropped and never allowed to stand in for a measurement.
                // If NO station on the whole path is seedable the contact is
                // still `section_component_empty` below, so "measured
                // nothing" continues to fail.
                unseedable += 1;
                unseedable_span_mm += safe_step;
            }
        }
        if s >= total {
            break;
        }
    }

    let Some((a_min_mm2, station_mm, primitive_record, r_at_min_mm)) = best else {
        return Err(indet("section_component_empty", path));
    };

    Ok(EvalOk {
        a_min_mm2,
        a_owned_mm2,
        station_mm,
        primitive_record,
        r_at_min_mm,
        stations_evaluated: evaluated,
        stations_unseedable: unseedable,
        unseedable_span_mm,
        path,
    })
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

/// One number formatter, so a failure line and its JSON row never drift
/// (mirrors `checks::load_ownership::num`, private to that file).
fn fmt(v: f64) -> String {
    if v.is_nan() {
        "nan".to_string()
    } else if v.is_infinite() {
        if v > 0.0 { "inf" } else { "-inf" }.to_string()
    } else {
        format!("{v}")
    }
}

#[allow(clippy::too_many_arguments)]
fn contact_row(
    contact_id: usize,
    status: &str,
    reason: Option<&str>,
    a_min_mm2: Option<f64>,
    a_required_mm2: Option<f64>,
    a_owned_mm2: Option<f64>,
    x_delivered: Option<f64>,
    r_at_min_mm: Option<f64>,
    station_mm: Option<f64>,
    primitive_record: Option<usize>,
    stations_evaluated: usize,
    stations_unseedable: usize,
    unseedable_span_mm: f64,
    path: &[usize],
) -> Json {
    let mut o = Json::obj();
    o.set("contact", Json::usize(contact_id));
    o.set("status", Json::str(status));
    if let Some(r) = reason {
        o.set("reason", Json::str(r));
    }
    o.set("a_min_mm2", Json::num_or_null(a_min_mm2));
    o.set("a_required_mm2", Json::num_or_null(a_required_mm2));
    o.set("a_owned_mm2", Json::num_or_null(a_owned_mm2));
    o.set("x_delivered", Json::num_or_null(x_delivered));
    o.set("r_at_min_mm", Json::num_or_null(r_at_min_mm));
    o.set("station_mm", Json::num_or_null(station_mm));
    o.set(
        "primitive_record",
        primitive_record.map_or(Json::Null, Json::usize),
    );
    o.set("stations_evaluated", Json::usize(stations_evaluated));
    o.set("stations_unseedable", Json::usize(stations_unseedable));
    o.set("unseedable_span_mm", Json::num(unseedable_span_mm));
    o.set(
        "path_records",
        Json::Arr(path.iter().map(|&r| Json::usize(r)).collect()),
    );
    o
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{AuditContext, AuditOptions, ModelContext};
    use crate::reader::ChituboxFile;
    use crate::synth::{box_soup, SynthFile, SynthModel, SynthRecord};
    use std::path::Path;

    // -- the predicate's boundary --------------------------------------

    /// **This test asserted the OPPOSITE before `audit-interval-policy.md`
    /// §3.2 row 10.** It pinned `passes_area_law(required - tol) == true` --
    /// a section genuinely thinner than the area law demands, certified
    /// because the shortfall was smaller than the measurement uncertainty.
    /// That is Finding 3 written down as a passing test, which is how the
    /// defect survived review. The conservative form inverts it.
    #[test]
    fn the_area_law_refuses_a_section_short_by_less_than_its_own_tolerance() {
        let a_owned = 4.8; // mm^2, an ordinary value
        let x_allowed = c::X_ALLOWED;
        let required = a_owned / x_allowed;
        let tol = c::tol_area(a_owned, x_allowed);

        // The retired predicate's own boundary case. It passed; it must not.
        assert_eq!(
            area_law_verdict(required - tol, a_owned, x_allowed),
            Verdict::Indeterminate,
            "a section short by exactly tol used to certify"
        );
        // Exactly AT the bare requirement is also unresolved: the evidence
        // cannot tell `required` from `required - tol`.
        assert_eq!(
            area_law_verdict(required, a_owned, x_allowed),
            Verdict::Indeterminate
        );
        // Clear of the band on the high side: a real pass.
        assert_eq!(
            area_law_verdict(required + 2.0 * tol, a_owned, x_allowed),
            Verdict::Pass
        );
        // Clear of the band on the low side: a measured defect, and it must
        // stay distinguishable from the unresolved cases above.
        assert_eq!(
            area_law_verdict(required - 2.0 * tol, a_owned, x_allowed),
            Verdict::Fail
        );
    }

    /// The pass boundary is `a_min - tol >= required`, exactly.
    #[test]
    fn the_area_law_passes_exactly_at_the_conservative_edge() {
        let a_owned = 4.8;
        let x_allowed = c::X_ALLOWED;
        let required = a_owned / x_allowed;
        let tol = c::tol_area(a_owned, x_allowed);
        assert_eq!(
            area_law_verdict(required + tol, a_owned, x_allowed),
            Verdict::Pass
        );
        assert_eq!(
            area_law_verdict(required + tol * 0.5, a_owned, x_allowed),
            Verdict::Indeterminate
        );
    }

    #[test]
    fn the_predicate_never_passes_on_an_unmeasurable_input() {
        for v in [
            area_law_verdict(f64::NAN, 1.0, c::X_ALLOWED),
            area_law_verdict(1.0, f64::INFINITY, c::X_ALLOWED),
            area_law_verdict(1.0, 1.0, 0.0),
            area_law_verdict(-0.1, 1.0, c::X_ALLOWED),
        ] {
            assert_eq!(v, Verdict::Indeterminate, "unmeasurable is never a pass");
            assert!(!v.is_pass());
        }
    }

    // -- fixtures --------------------------------------------------------

    /// A closed model, one seated contact, a full chain to a plate-grounded
    /// pad. `half_width` controls the model's footprint and therefore how
    /// much down-facing demand this file's one contact ends up owning.
    fn chain_file(half_width: f64) -> ChituboxFile {
        let f = SynthFile::new().with_model(SynthModel {
            name: "chain".into(),
            translate_mm: [0.0, 0.0, 5.0],
            mesh: box_soup(
                [-half_width, -half_width, -2.0],
                [half_width, half_width, 2.0],
            ),
            records: vec![
                SynthRecord::new(
                    RecordKind::ContactSphere,
                    [0.0, 0.0, -1.9],
                    [0.0, 0.0, -2.8],
                    0.45,
                    0.45,
                )
                .with_matching_soup(),
                SynthRecord::new(
                    RecordKind::ContactTaper,
                    [0.0, 0.0, -2.35],
                    [0.0, 0.0, -4.0],
                    0.3,
                    0.35,
                )
                .with_declared_depth(0.1)
                .with_matching_soup(),
                SynthRecord::new(
                    RecordKind::Member,
                    [0.0, 0.0, -4.0],
                    [0.0, 0.0, -7.0],
                    0.35,
                    0.35,
                )
                .with_matching_soup(),
                SynthRecord::new(
                    RecordKind::RaftPadOrBridge,
                    [0.0, 0.0, -7.0],
                    [0.0, 0.0, -7.0],
                    c::RAFT_PAD_DIAMETER_MM,
                    c::RAFT_PAD_HEIGHT_MM,
                )
                .as_pad(),
            ],
        });
        ChituboxFile::parse(Path::new("chain.chitubox"), &f.to_bytes()).expect("parse")
    }

    /// A contact that seats validly into a small model but has no taper, no
    /// member and no pad at all -- a genuinely open load path.
    fn orphan_seated_tip_file() -> ChituboxFile {
        let f = SynthFile::new().with_model(SynthModel {
            name: "orphan".into(),
            translate_mm: [0.0, 0.0, 5.0],
            mesh: box_soup([-0.3, -0.3, -2.0], [0.3, 0.3, 2.0]),
            records: vec![SynthRecord::new(
                RecordKind::ContactSphere,
                [0.0, 0.0, -1.95],
                [0.0, 0.0, -2.5],
                0.3,
                0.3,
            )
            .with_matching_soup()],
        });
        ChituboxFile::parse(Path::new("orphan.chitubox"), &f.to_bytes()).expect("parse")
    }

    fn run_on(file: &ChituboxFile) -> CheckOutcome {
        let options = AuditOptions::default();
        let audit = AuditContext {
            file,
            options: &options,
            sidecar: None,
            sidecar_absence: Some("unit test".into()),
        };
        let ctx = ModelContext::build(&audit, 0);
        NarrowestSection.run(&ctx)
    }

    #[test]
    fn a_small_demand_and_an_ordinary_arm_passes() {
        let file = chain_file(0.3);
        let outcome = run_on(&file);
        assert_eq!(outcome.checked, 1);
        assert_eq!(
            outcome.indeterminate, 0,
            "every quantity should be established: {:?}",
            outcome.failures
        );
        assert_eq!(
            outcome.failed, 0,
            "a thin arm under a tiny demand must pass: {:?}",
            outcome.failures
        );
        assert!(outcome.status().is_pass());
    }

    #[test]
    fn a_large_demand_on_the_same_arm_trips_min_section_short() {
        // Same chain, but the model presents a 10x10 mm downward face, so the
        // sole contact owns far more area than a 0.35 mm-radius arm can carry.
        let file = chain_file(5.0);
        let outcome = run_on(&file);
        assert_eq!(outcome.checked, 1);
        assert_eq!(outcome.indeterminate, 0);
        assert_eq!(outcome.failed, 1);
        assert!(!outcome.status().is_pass());
        assert!(outcome
            .failures
            .iter()
            .any(|f| f.code == "MIN_SECTION_SHORT"));
        let f = outcome
            .failures
            .iter()
            .find(|f| f.code == "MIN_SECTION_SHORT")
            .expect("the failure line");
        assert!(
            f.message.starts_with("MIN_SECTION_SHORT"),
            "{}",
            f.message
        );
    }

    #[test]
    fn an_open_load_path_is_indeterminate_never_a_pass() {
        let file = orphan_seated_tip_file();
        let outcome = run_on(&file);
        assert_eq!(outcome.checked, 1);
        assert_eq!(outcome.indeterminate, 1);
        assert!(!outcome.status().is_pass());
        let f = &outcome.failures[0];
        assert_eq!(f.code, "MIN_SECTION_INDETERMINATE");
        assert!(f.message.contains("reason=path_open"), "{}", f.message);
    }

    // -- small internal helpers -------------------------------------------

    #[test]
    fn section_budget_scales_with_the_local_radius_and_stays_bounded() {
        let step = 0.007;
        let thin = section_budget(step, 0.05);
        let thick = section_budget(step, 1.1);
        assert!(thin < thick, "{thin} vs {thick}");
        assert!(thick <= 300_000);
        assert!(thin >= 256);
    }

    #[test]
    fn locate_finds_the_right_segment_and_clamps_local_fraction() {
        let segs = vec![
            Seg {
                p0: [0.0, 0.0, 0.0],
                p1: [1.0, 0.0, 0.0],
                len: 1.0,
                record: 0,
                r0: 0.1,
                r1: 0.2,
            },
            Seg {
                p0: [1.0, 0.0, 0.0],
                p1: [3.0, 0.0, 0.0],
                len: 2.0,
                record: 1,
                r0: 0.2,
                r1: 0.2,
            },
        ];
        let cum = [0.0, 1.0, 3.0];
        assert_eq!(locate(&segs, &cum, 0.0), (0, 0.0));
        assert_eq!(locate(&segs, &cum, 0.5), (0, 0.5));
        let (idx, t) = locate(&segs, &cum, 1.5);
        assert_eq!(idx, 1);
        assert!((t - 0.25).abs() < 1e-12);
        // Past the end clamps to the last segment's far end, never panics.
        let (idx, t) = locate(&segs, &cum, 10.0);
        assert_eq!(idx, 1);
        assert!((t - 1.0).abs() < 1e-9);
    }
}
