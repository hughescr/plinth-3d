//! I5 closed load path -- `docs/specs/audit-invariants.md` §3.5.
//!
//! Every support primitive must reach the plate or a valid model seat, no arm
//! may terminate in free space, and the rootward relation must be acyclic in
//! the honest sense the specification defines. This module reconstructs its
//! OWN connection graph from the analytic solids -- it does not reuse
//! [`crate::graph::SupportGraph`], because that graph is built from
//! *connective-point coincidence* (the design topology: a taper's end meets a
//! ball's centre), while §3.5 defines the connection graph by **solid
//! overlap** (`Primitive::separation_mm` strictly negative). The two normally
//! agree at a design joint, but only the overlap graph also catches bracing
//! that touches a neighbour mid-span rather than at a declared endpoint, and
//! only the overlap graph is literally what the specification asks for.
//!
//! ## Clause (d): no support island
//!
//! Connected is not held up. A support arm whose tip is seated in the model
//! and whose lower end stops in the air is connected through the model and is
//! grounded by clause (ii) below, yet its lowest layer prints on nothing.
//! [`crate::checks::support_below`] asks the DIRECTIONAL question — does every
//! primitive's lowest point rest on the plate, the model, or a primitive that
//! reaches lower, in a chain ending at the plate or the model — and each
//! primitive that fails it is a `PATH_NO_SUPPORT_BELOW` failure.
//!
//! ## Grounding
//!
//! A primitive is grounded if it touches the plate half-space (`z <=
//! z_plate + layer_height`, reconstructed exactly from each primitive's own
//! analytic bounding box -- see the note on [`Primitive::aabb`]) or if it is
//! a TIP record with a valid model seat. "Valid model seat" is reimplemented
//! here directly from the shared [`crate::seat::Seat`] reconstruction (I1's
//! predicate, `audit-invariants.md` §3.1, is fully derivable from `Seat`'s
//! already-computed fields) plus a **stated approximation of I6's far-side
//! clause**, [`seat_zone_breach_probe`] -- see that function's doc comment for
//! exactly how far it goes and where it stops. Neither reimplementation calls
//! into `checks::seat_depth` or `checks::intersection`: those modules are
//! owned by other agents editing this checkout right now, and coupling this
//! check to their internal state (rather than to the shared reconstructions
//! every check is meant to read) is exactly the kind of cross-file dependency
//! the project's module layout exists to avoid.
//!
//! ## Never-pass discipline
//!
//! A primitive whose analytic solid could not be built at all (`GeomError`) is
//! reported `PATH_INDETERMINATE` with an extension reason token,
//! `primitive_degenerate`, not one of the specification's own two listed
//! reasons -- the specification's reason enum does not anticipate a primitive
//! with no solid at all, and I8 is the invariant that names the defect
//! precisely. A pair of solids whose separation lands in the fusion-uncertain
//! band `(0, d3]` never resolves to an edge either way. A TIP whose seat
//! reconstruction itself could not be established is `PATH_INDETERMINATE`
//! before its grounding is even asked. None of these ever becomes a pass.

use crate::check::{Check, CheckOutcome, Failure};
use crate::checks::interval::{self, Verdict};
use crate::checks::support_below;
use crate::consts as c;
use crate::context::ModelContext;
use crate::json::Json;
use crate::mesh::Inside;
use crate::primitives::Primitive;
use crate::reader::RecordKind;
use crate::seat::Seat;
use crate::vec3::{self as v, Aabb, V3};
use std::collections::{HashSet, VecDeque};

pub struct LoadPath;

impl Check for LoadPath {
    fn id(&self) -> &'static str {
        "i5_closed_load_path"
    }

    fn title(&self) -> &'static str {
        "I5 closed load path"
    }

    fn scope(&self, ctx: &ModelContext) -> usize {
        ctx.model.records.len()
    }

    fn run(&self, ctx: &ModelContext) -> CheckOutcome {
        let file = ctx.file();
        let n = ctx.model.records.len();
        let mut out = CheckOutcome::new(n);
        let layer = ctx.inputs().layer_height_mm;
        let z_ground = ctx.z_plate_mm + layer;

        // -------------------------------------------------------------
        // 1. Every primitive that has an analytic solid at all.
        // -------------------------------------------------------------
        let mut ok: Vec<(usize, Primitive)> = Vec::new();
        let mut n_degenerate = 0usize;
        for (i, p) in ctx.primitives.iter().enumerate() {
            match p {
                Ok(pr) => ok.push((i, *pr)),
                Err(e) => {
                    n_degenerate += 1;
                    out.indeterminate(Failure::line(
                        "PATH_INDETERMINATE",
                        &file,
                        ctx.model_index,
                        None,
                        Some(i),
                        &[("reason", "primitive_degenerate".to_string())],
                        {
                            let mut o = Json::obj();
                            o.set("reason", Json::str("primitive_degenerate"));
                            o.set("geom_error", Json::str(e.reason()));
                            o
                        },
                    ));
                }
            }
        }
        let n_ok = ok.len();

        // -------------------------------------------------------------
        // 2. The overlap graph: edges from solid-to-solid separation, not
        //    from connective-point coincidence (`audit-invariants.md` §3.5).
        //
        //    **Deliberately NOT `Primitive::separation_mm`.** That function's
        //    own doc states it approximates a frustum as a capsule (segment
        //    distance minus both local radii), which is correct for a truly
        //    LATERAL closest approach but is wrong at a flat end cap: two
        //    frustums meeting axially, cap to cap, get `sum-of-local-radii`
        //    subtracted from the axial gap even though a flat disc has zero
        //    extent along its own axis. Measured on a synthetic coaxial
        //    taper/flare pair of radius 0.35 each: a genuine 0.11 mm axial
        //    gap is reported as -0.59 mm (a confident, wrong, OVERLAP) --
        //    almost the entire `ra + rb` is phantom closure. Since the design
        //    joints this graph exists to verify (taper->member->flare->raft)
        //    are exactly this cap-to-cap shape, using `separation_mm` here
        //    would make I5 blind to a genuinely severed joint up to roughly
        //    the sum of the two local radii wide -- a bigger gap than the
        //    corpus's own measured tightest contact spacing (0.049 mm). See
        //    [`overlap_gap_mm`] for the fix and its own honest limits.
        // -------------------------------------------------------------
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut adj_indet: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        let mut n_overlap_edges = 0usize;
        let mut n_indet_pairs = 0usize;

        for a in 0..n_ok {
            let (i, pi) = ok[a];
            let bbi = pi.aabb();
            for b in (a + 1)..n_ok {
                let (j, pj) = ok[b];
                let bbj = pj.aabb();
                if aabb_gap(&bbi, &bbj) > c::D3_MM {
                    // A valid lower bound on true separation: the body is
                    // contained in its own box, so box-to-box distance can
                    // only UNDERSTATE body-to-body distance. Safe to skip.
                    continue;
                }
                let sep = overlap_gap_mm(&pi, &pj);
                // Compare against `tol_geom`, not bare zero: `audit-invariants.md`
                // §3.5 says "strictly negative" is overlap and `[0, d3]` is the
                // band, but that is a statement about the PHYSICAL quantity, and
                // every other predicate in this specification reads a physical
                // zero through `tol_geom` because the evidence is f32 (§1.5).
                // The ubiquitous, by-DESIGN case -- a taper's end and a member's
                // start emitted at the identical coordinate, meant to touch
                // exactly -- computes a `sep` that is genuinely zero but lands on
                // either side of it by a few `eps32` depending on sample-ordering
                // rounding (measured on `broken_path_control.chitubox`: a
                // deliberately-clean fixture with `sep` printing as `-0.000000`,
                // i.e. a signed value on the order of 1e-9, roughly four orders of
                // magnitude below `tol_geom` at this coordinate scale). Reading
                // that as "strictly negative only" would make I5 report
                // fusion-uncertain on the design joint every real support tree
                // uses, not on a defect. `tol_geom` is exactly the resolution the
                // rest of this document already agreed the evidence carries.
                let tol = c::tol_geom_mag(
                    [bbi.min, bbi.max, bbj.min, bbj.max]
                        .into_iter()
                        .flatten()
                        .fold(1.0f64, |m, x| m.max(x.abs())),
                );
                // EXEMPT, and it is the MODEL the rest of this sweep copies
                // (`audit-interval-policy.md` §1.5): already the
                // three-outcome form. Connected, fusion-uncertain, or
                // disconnected -- doubt resolves to `indeterminate`, never
                // to an edge. Nothing to change.
                if sep <= tol {
                    adj[i].push(j);
                    adj[j].push(i);
                    n_overlap_edges += 1;
                } else if sep <= c::D3_MM {
                    adj_indet[i].push((j, sep));
                    adj_indet[j].push((i, sep));
                    n_indet_pairs += 1;
                    out.indeterminate(Failure::line(
                        "PATH_INDETERMINATE",
                        &file,
                        ctx.model_index,
                        None,
                        Some(i),
                        &[
                            ("other", j.to_string()),
                            ("separation_mm", format!("{sep:.6}")),
                            ("band_mm", format!("{:.6e}", c::D3_MM)),
                            ("reason", "fusion_uncertain".to_string()),
                        ],
                        {
                            let mut o = Json::obj();
                            o.set("other", Json::usize(j));
                            o.set("separation_mm", Json::num(sep));
                            o.set("band_mm", Json::num(c::D3_MM));
                            o.set("reason", Json::str("fusion_uncertain"));
                            o
                        },
                    ));
                }
            }
        }

        // -------------------------------------------------------------
        // 3. Grounding: clause (i) the plate, clause (ii) a valid model seat.
        // -------------------------------------------------------------
        let mut ground = vec![false; n];
        let mut ground_indet = vec![false; n];
        let mut n_ground_plate = 0usize;
        let mut n_ground_seat = 0usize;

        for &(i, p) in &ok {
            // EXEMPT from the interval policy (`audit-interval-policy.md`
            // §1.5, §3.4): an EQUALITY-BAND coincidence test, not a margin
            // test. It asks whether this solid's lowest stored coordinate is
            // the same coordinate as the plate, and `tol_geom` here is the
            // resolution of the f32 evidence on both sides -- there is no
            // delivered-versus-required relation, so there is no conservative
            // side to pick and no third outcome to add.
            let bb = p.aabb();
            let tol = c::tol_geom_mag(bb.min[2].abs().max(z_ground.abs()).max(1.0));
            if bb.min[2] <= z_ground + tol {
                ground[i] = true;
                n_ground_plate += 1;
            }
        }
        let tip_candidates: Vec<usize> = ok
            .iter()
            .filter(|&&(i, _)| ctx.model.records[i].kind == RecordKind::ContactSphere && !ground[i])
            .map(|&(i, _)| i)
            .collect();
        for i in tip_candidates {
            let Some(seat) = ctx.seat(i) else {
                ground_indet[i] = true;
                out.indeterminate(Failure::line(
                    "PATH_INDETERMINATE",
                    &file,
                    ctx.model_index,
                    None,
                    Some(i),
                    &[("reason", "seat_indeterminate".to_string())],
                    {
                        let mut o = Json::obj();
                        o.set("reason", Json::str("seat_indeterminate"));
                        o.set("detail", Json::str("no reconstructed seat for this TIP record"));
                        o
                    },
                ));
                continue;
            };
            match seat_ground_status(ctx, seat) {
                SeatGround::Valid => {
                    ground[i] = true;
                    n_ground_seat += 1;
                }
                SeatGround::NotValid => {}
                SeatGround::Indeterminate(reason) => {
                    ground_indet[i] = true;
                    out.indeterminate(Failure::line(
                        "PATH_INDETERMINATE",
                        &file,
                        ctx.model_index,
                        None,
                        Some(i),
                        &[("reason", "seat_indeterminate".to_string())],
                        {
                            let mut o = Json::obj();
                            o.set("reason", Json::str("seat_indeterminate"));
                            o.set("detail", Json::str(reason));
                            o
                        },
                    ));
                }
            }
        }

        // -------------------------------------------------------------
        // 4. Reachability. Clause (c), the acyclic rootward relation, falls
        //    out for free: a BFS distance to ground cannot exist without a
        //    neighbour strictly closer, by construction of BFS.
        // -------------------------------------------------------------
        let reachable_strict = bfs_reachable(n, &adj, |i| ground[i]);
        let mut adj_all = adj.clone();
        for i in 0..n {
            for &(j, _) in &adj_indet[i] {
                adj_all[i].push(j);
            }
        }
        let reachable_optimistic =
            bfs_reachable(n, &adj_all, |i| ground[i] || ground_indet[i]);

        let n_grounded_total = ground.iter().filter(|&&g| g).count();

        // -------------------------------------------------------------
        // 5. Per-primitive classification: orphan, or fine.
        // -------------------------------------------------------------
        let mut n_orphan = 0usize;
        let mut n_connected = 0usize;
        let mut n_indet_conn = 0usize; // grounded/reachable only optimistically
        let mut records_json = Vec::new();

        for &(i, p) in &ok {
            let rec = &ctx.model.records[i];
            let status: &str;
            if ground[i] {
                status = "grounded";
            } else if ground_indet[i] {
                status = "indeterminate_ground";
                n_indet_conn += 1;
            } else if reachable_strict[i] {
                status = "connected";
                n_connected += 1;
            } else if reachable_optimistic[i] {
                status = "indeterminate_path";
                n_indet_conn += 1;
                // Already reported as a PATH_INDETERMINATE pair in step 2 or a
                // seat finding in step 3; no second failure line for the same
                // root cause.
            } else {
                status = "orphan";
                n_orphan += 1;
                let comp = component_size(i, &adj);
                let nearest = nearest_ground_distance(&p, &ok, &ground, z_ground);
                out.fail(Failure::line(
                    "PATH_ORPHAN",
                    &file,
                    ctx.model_index,
                    None,
                    Some(i),
                    &[
                        ("type", rec.kind.label().to_string()),
                        ("component_size", comp.to_string()),
                        ("nearest_ground_mm", format!("{nearest:.6}")),
                    ],
                    {
                        let mut o = Json::obj();
                        o.set("type", Json::str(rec.kind.label()));
                        o.set("component_size", Json::usize(comp));
                        o.set("nearest_ground_mm", Json::num(nearest));
                        o
                    },
                ));
            }
            let mut rj = Json::obj();
            rj.set("record", Json::usize(i));
            rj.set("type", Json::str(rec.kind.label()));
            rj.set("status", Json::str(status));
            records_json.push(rj);
        }

        if n_ok > 0 && n_grounded_total == 0 && !ground_indet.iter().any(|&g| g) {
            out.fail(Failure::line(
                "PATH_ROOT_MISSING",
                &file,
                ctx.model_index,
                None,
                None,
                &[
                    ("grounded_primitives", "0".to_string()),
                    ("records", n.to_string()),
                ],
                {
                    let mut o = Json::obj();
                    o.set("grounded_primitives", Json::usize(0));
                    o.set("records", Json::usize(n));
                    o
                },
            ));
        }

        // -------------------------------------------------------------
        // 6. Free ends: clause (b), per frustum end disc.
        // -------------------------------------------------------------
        let mut n_free_end = 0usize;
        let mut n_fusion_end = 0usize;
        for &(i, p) in &ok {
            let rec = &ctx.model.records[i];
            if !rec.kind.is_frustum() {
                continue;
            }
            let Primitive::Frustum { a, ra, b, rb } = p else {
                continue;
            };
            for (end_point, radius, label) in [(a, ra, "start"), (b, rb, "end")] {
                match end_status(end_point, i, &ok, z_ground) {
                    EndStatus::Grounded | EndStatus::Overlapped(_) => {}
                    EndStatus::FusionUncertain(_, _) => {
                        // Already surfaced as a PATH_INDETERMINATE pair in
                        // step 2: the point lies on this primitive's own
                        // solid, so the solid-to-solid separation to the same
                        // neighbour is at most this value and was already
                        // tested there.
                        n_fusion_end += 1;
                    }
                    EndStatus::Free(nearest) => {
                        n_free_end += 1;
                        out.fail(Failure::line(
                            "PATH_FREE_END",
                            &file,
                            ctx.model_index,
                            None,
                            Some(i),
                            &[
                                ("type", rec.kind.label().to_string()),
                                ("end", label.to_string()),
                                (
                                    "point",
                                    format!(
                                        "{:.6},{:.6},{:.6}",
                                        end_point[0], end_point[1], end_point[2]
                                    ),
                                ),
                                ("radius_mm", format!("{radius:.6}")),
                                (
                                    "nearest_solid_mm",
                                    if nearest.is_finite() {
                                        format!("{nearest:.6}")
                                    } else {
                                        "inf".to_string()
                                    },
                                ),
                            ],
                            {
                                let mut o = Json::obj();
                                o.set("type", Json::str(rec.kind.label()));
                                o.set("end", Json::str(label));
                                o.set("point", Json::v3(end_point));
                                o.set("radius_mm", Json::num(radius));
                                o.set(
                                    "nearest_solid_mm",
                                    if nearest.is_finite() {
                                        Json::num(nearest)
                                    } else {
                                        Json::Null
                                    },
                                );
                                o
                            },
                        ));
                    }
                }
            }
        }

        // -------------------------------------------------------------
        // 6b. Clause (d), no support island: held up FROM BELOW. A tip
        //     seated in the model grounds only its own member's top, never
        //     what hangs beneath it.
        // -------------------------------------------------------------
        let kinds: Vec<RecordKind> = ctx.model.records.iter().map(|r| r.kind).collect();
        let below = support_below::classify(&ok, &kinds, ctx.z_plate_mm, layer, |p| {
            support_below::on_model_mesh(&ctx.model_mesh, p, layer)
        });
        for (k, &i) in below.unrooted.iter().enumerate() {
            let rec = &ctx.model.records[i];
            let low = below.unrooted_low[k];
            let nearest = below.unrooted_nearest_below_mm[k];
            let rests_on_nothing = below.rest[i] == Some(support_below::Rest::Nothing);
            out.fail(Failure::line(
                "PATH_NO_SUPPORT_BELOW",
                &file,
                ctx.model_index,
                None,
                Some(i),
                &[
                    ("type", rec.kind.label().to_string()),
                    (
                        "lowest_point",
                        format!("{:.6},{:.6},{:.6}", low[0], low[1], low[2]),
                    ),
                    (
                        "rests_on",
                        if rests_on_nothing { "nothing" } else { "unrooted_member" }.to_string(),
                    ),
                    (
                        "nearest_lower_solid_mm",
                        if nearest.is_finite() {
                            format!("{nearest:.6}")
                        } else {
                            "inf".to_string()
                        },
                    ),
                ],
                {
                    let mut o = Json::obj();
                    o.set("type", Json::str(rec.kind.label()));
                    o.set("lowest_point", Json::v3(low));
                    o.set(
                        "rests_on",
                        Json::str(if rests_on_nothing { "nothing" } else { "unrooted_member" }),
                    );
                    o.set(
                        "nearest_lower_solid_mm",
                        if nearest.is_finite() {
                            Json::num(nearest)
                        } else {
                            Json::Null
                        },
                    );
                    o
                },
            ));
        }
        let n_no_support_below = below.unrooted.len();

        // -------------------------------------------------------------
        // 7. Report.
        // -------------------------------------------------------------
        let mut measured = Json::obj();
        measured.set("no_support_below", Json::usize(n_no_support_below));
        measured.set("below_rooted_at_plate", Json::usize(below.rooted_at_plate));
        measured.set("below_rooted_at_raft_top", Json::usize(below.rooted_at_raft_top));
        measured.set("below_rooted_on_model", Json::usize(below.rooted_on_model));
        measured.set("below_resting_on_member", Json::usize(below.resting_on_member));
        measured.set("below_indeterminate", Json::usize(below.indeterminate));
        measured.set("records_total", Json::usize(n));
        measured.set("records_ok", Json::usize(n_ok));
        measured.set("records_degenerate", Json::usize(n_degenerate));
        measured.set("grounded_via_plate", Json::usize(n_ground_plate));
        measured.set("grounded_via_seat", Json::usize(n_ground_seat));
        measured.set("connected_not_grounded", Json::usize(n_connected));
        measured.set("orphaned", Json::usize(n_orphan));
        measured.set("free_ends", Json::usize(n_free_end));
        measured.set("fusion_uncertain_ends", Json::usize(n_fusion_end));
        measured.set("indeterminate_connectivity", Json::usize(n_indet_conn));
        measured.set("overlap_edges", Json::usize(n_overlap_edges));
        measured.set("fusion_uncertain_pairs", Json::usize(n_indet_pairs));
        measured.set("records", Json::Arr(records_json));
        measured.set(
            "headline",
            Json::str(format!(
                "{n_orphan} orphaned, {n_free_end} free ends, {n_no_support_below} with no \
                 support below, {} indeterminate (of {n} records)",
                out.indeterminate
            )),
        );
        out.measured = measured;
        out
    }
}

// ---------------------------------------------------------------------------
// Grounding via a valid model seat
// ---------------------------------------------------------------------------

enum SeatGround {
    Valid,
    NotValid,
    Indeterminate(&'static str),
}

/// I1's predicate (`audit-invariants.md` §3.1), reimplemented directly from
/// the already-shared [`Seat`] reconstruction, plus a bounded approximation of
/// I6's far-side clause (see [`seat_zone_breach_probe`]). Every quantity used
/// here (`h_delivered_mm`, `h_required_mm`, `r_tip_agrees`,
/// `declared_depth_mm`, `indeterminate`) is computed once in `seat.rs` and
/// shared by every check that needs it -- this function adds only the
/// pass/fail comparison, which is what I1 itself would add on top of the same
/// data.
fn seat_ground_status(ctx: &ModelContext, seat: &Seat) -> SeatGround {
    if let Some(reason) = seat.indeterminate {
        return SeatGround::Indeterminate(reason);
    }
    let Some(h) = seat.h_delivered_mm else {
        return SeatGround::Indeterminate("h_delivered_missing");
    };
    if h <= 0.0 {
        // A ball resting on or outside the surface: established, not
        // unknown, and it is I1's own hard failure -- not a valid seat.
        return SeatGround::NotValid;
    }
    if !seat.r_tip_agrees {
        return SeatGround::NotValid;
    }
    let tol = seat.tol_geom_mm();
    if let Some(declared) = seat.declared_depth_mm {
        if (declared - h).abs() > tol {
            return SeatGround::NotValid;
        }
    }
    let Some(h_req) = seat.h_required_mm else {
        return SeatGround::Indeterminate("h_required_missing");
    };
    // Conservative minimum-type form (`audit-interval-policy.md` §3.2
    // row 4 -- duplicate #4 of I1's predicate, carrying the same copied
    // bug). It replaces `if h < h_req - tol { NotValid }`, whose passing
    // branch was `h >= h_req - tol`: the delivered depth's UPPER bound
    // against the requirement, backwards for a minimum-type relation.
    //
    // This function already had the three outcomes; the fix is to route the
    // straddling band to the third one instead of letting it fall through to
    // `Valid`. A seat whose interval straddles its requirement has not
    // established a load path to the model.
    match interval::min_type(h, h_req, tol) {
        Verdict::Pass => {}
        Verdict::Fail => return SeatGround::NotValid,
        Verdict::Indeterminate => {
            return SeatGround::Indeterminate(interval::REASON_WITHIN_TOLERANCE_BAND)
        }
    }
    match seat_zone_breach_probe(ctx, seat, h) {
        BreachStatus::Clean => SeatGround::Valid,
        BreachStatus::Breach => SeatGround::NotValid,
        BreachStatus::Indeterminate => SeatGround::Indeterminate("seat_zone_probe_exhausted"),
    }
}

enum BreachStatus {
    Clean,
    Breach,
    Indeterminate,
}

/// A deliberately narrow, self-contained stand-in for I6 clause (b) (the
/// far-side breach test), used ONLY to decide whether a TIP's own seat may
/// ground it for I5's purposes.
///
/// **This is not I6.** The real I6 clause (b) counts connected components of
/// the tip group's cured solid minus the cured model raster -- a global
/// raster/boolean computation owned by `checks/intersection.rs`. Reproducing
/// that here would duplicate the other agent's entire invariant inside this
/// file. Instead this samples `SAMPLES` points on the TIP ball's own surface
/// (a Fibonacci-sphere lattice, deterministic so two runs over the same bytes
/// agree), keeps only the samples axially beyond the seat zone
/// (`h_delivered + layer_height`, `audit-invariants.md` §3.6), and asks
/// [`crate::mesh::Mesh::point_inside`] whether any of them are still buried:
/// if the ball's *intended-external* surface is found back inside the model,
/// that is real evidence of a far-side breach -- the ball re-enters material
/// it should have already left.
///
/// **What this narrower test misses, stated rather than hidden.** It samples
/// only the ball's surface, not the adjacent taper's; it is a finite sample
/// (`SAMPLES` points), so a breach smaller than the lattice spacing can fall
/// between samples and read as clean; and it tests the ball's OWN surface
/// points for containment rather than computing the true connected-component
/// count of the exterior material, so it cannot distinguish "one exterior
/// blob with an odd shape" from "two exterior blobs" the way the real I6
/// does. It is strictly a cheaper, honest partial substitute, used only to
/// decide grounding -- I5 never reports an `INTERSECT_FAR_SIDE_BREACH` code,
/// which is I6's alone.
fn seat_zone_breach_probe(ctx: &ModelContext, seat: &Seat, h_delivered: f64) -> BreachStatus {
    const SAMPLES: usize = 96;
    let Some(u) = seat.u else {
        return BreachStatus::Indeterminate;
    };
    let band_hi = h_delivered + ctx.inputs().layer_height_mm;
    let mut pts: Vec<V3> = Vec::new();
    for k in 0..SAMPLES {
        let dir = fibonacci_sphere_direction(k, SAMPLES);
        let p = v::add(seat.centre, v::mul(dir, seat.r_ball_mm));
        let axial = v::dot(v::sub(p, seat.p0), u);
        let tol = c::tol_geom_at(p);
        // **A candidate-SELECTION threshold, not a pass/fail gate**
        // (`audit-interval-policy.md` §3.2 row 7), and its conservative
        // direction is the opposite of a gate's. The goal is not to miss a
        // real breach, so the selection must be INCLUSIVE of every sample
        // that might lie beyond the seat zone: `band_hi - tol`, the band's
        // effective lower edge.
        //
        // The retired form was `axial > band_hi + tol`, which TIGHTENED the
        // candidate set -- the minimum-direction bug wearing a filter's
        // clothing. It silently dropped samples that could be genuine
        // breaches and biased this probe toward `Clean`, and `Clean` is the
        // outcome that lets a seat ground a load path.
        if axial > band_hi - tol {
            pts.push(p);
        }
    }
    if pts.is_empty() {
        // The ball never reaches past its own seat zone (a very deep or very
        // small tip): nothing to probe, and nothing to accuse.
        return BreachStatus::Clean;
    }
    let results = ctx.model_mesh.points_inside(&pts);
    if results.iter().any(|r| r.is_indeterminate()) {
        return BreachStatus::Indeterminate;
    }
    if results.iter().any(|r| matches!(r, Inside::Inside)) {
        return BreachStatus::Breach;
    }
    BreachStatus::Clean
}

/// Deterministic point `k` of an `n`-point Fibonacci-sphere lattice, as a unit
/// vector. Distinct from [`crate::vec3::golden_spiral_direction`], which is
/// fixed at 16 points for the ray-parity retry budget; this needs a denser,
/// independently-sized lattice for surface sampling.
fn fibonacci_sphere_direction(k: usize, n: usize) -> V3 {
    let n = n.max(1);
    let phi = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    let i = k as f64;
    let z = 1.0 - (2.0 * i + 1.0) / n as f64;
    let r = (1.0 - z * z).max(0.0).sqrt();
    let t = phi * i;
    v::normalize([r * t.cos(), r * t.sin(), z]).unwrap_or([0.0, 0.0, 1.0])
}

// ---------------------------------------------------------------------------
// Graph plumbing
// ---------------------------------------------------------------------------

/// A conservative lower bound on the distance between two solids: the boxes
/// contain the bodies, so box-to-box distance never overstates body-to-body
/// distance. Zero when the boxes overlap.
fn aabb_gap(a: &Aabb, b: &Aabb) -> f64 {
    let mut d2 = 0.0f64;
    for k in 0..3 {
        let gap = (a.min[k] - b.max[k]).max(b.min[k] - a.max[k]).max(0.0);
        d2 += gap * gap;
    }
    d2.sqrt()
}

fn bfs_reachable(n: usize, adj: &[Vec<usize>], is_source: impl Fn(usize) -> bool) -> Vec<bool> {
    let mut seen = vec![false; n];
    let mut q = VecDeque::new();
    for i in 0..n {
        if is_source(i) {
            seen[i] = true;
            q.push_back(i);
        }
    }
    while let Some(x) = q.pop_front() {
        for &y in &adj[x] {
            if !seen[y] {
                seen[y] = true;
                q.push_back(y);
            }
        }
    }
    seen
}

/// Size of the connected component (definite overlap edges only) containing
/// `i`, counting `i` itself.
fn component_size(i: usize, adj: &[Vec<usize>]) -> usize {
    let mut seen = HashSet::new();
    let mut stack = vec![i];
    seen.insert(i);
    while let Some(x) = stack.pop() {
        for &y in &adj[x] {
            if seen.insert(y) {
                stack.push(y);
            }
        }
    }
    seen.len()
}

/// The smallest gap from `p`'s solid to the plate half-space or to any
/// primitive already known to be grounded, mm. Zero would mean `p` was itself
/// grounded, which the caller only calls this for orphans it is not.
fn nearest_ground_distance(
    p: &Primitive,
    ok: &[(usize, Primitive)],
    ground: &[bool],
    z_ground: f64,
) -> f64 {
    let mut best = (p.aabb().min[2] - z_ground).max(0.0);
    for &(j, pj) in ok {
        if !ground[j] {
            continue;
        }
        let sep = overlap_gap_mm(p, &pj).max(0.0);
        if sep < best {
            best = sep;
        }
    }
    best
}

// ---------------------------------------------------------------------------
// An honest solid-to-solid separation estimate
// ---------------------------------------------------------------------------

/// A solid-to-solid gap estimate that does not inherit
/// [`Primitive::separation_mm`]'s documented capsule-approximation bias at a
/// flat end cap (see the note where this is called). Exact for a ball
/// pair. Otherwise: sample points on each solid's own surface -- both end
/// discs (perimeter and centre), a lateral ring grid, or a box's corners and
/// face centres, depending on the variant -- and evaluate the OTHER solid's
/// [`Primitive::signed_distance`] at each one, which is documented exact on a
/// frustum's lateral surface and end discs and exact everywhere for a ball or
/// a bridge box. Every sampled point genuinely lies on a real solid, so a
/// negative result is always a true overlap, never a phantom one.
///
/// **What this cannot see.** It is a finite sample, not an exact
/// convex-convex distance routine: the true closest pair of points can fall
/// between two samples, in which case this OVERSTATES the true separation.
/// That is the safe direction for a connectivity gate -- it can miss a
/// hairline true overlap and report a small gap instead, which lands the pair
/// in the fusion-uncertain band (indeterminate, never a silent pass) rather
/// than manufacturing a false join the way the capsule approximation does.
#[must_use]
fn overlap_gap_mm(p: &Primitive, q: &Primitive) -> f64 {
    if let (Primitive::Ball { centre: c1, r: r1 }, Primitive::Ball { centre: c2, r: r2 }) =
        (p, q)
    {
        return v::dist(*c1, *c2) - r1 - r2;
    }
    let mut best = f64::INFINITY;
    for pt in surface_samples(p) {
        let d = q.signed_distance(pt);
        if d < best {
            best = d;
        }
    }
    for pt in surface_samples(q) {
        let d = p.signed_distance(pt);
        if d < best {
            best = d;
        }
    }
    best
}

/// A deterministic surface sample of a primitive's own solid, dense enough to
/// catch a cap-to-cap or lateral near-miss without a per-pixel cost: a ball
/// gets a Fibonacci-sphere lattice; a frustum or a raft pad (itself a
/// zero-taper cone) gets end-disc rings across a handful of axial stations,
/// which is exactly where [`overlap_gap_mm`]'s target defect lives; a bridge
/// gets its eight corners.
fn surface_samples(p: &Primitive) -> Vec<V3> {
    const RING: usize = 12;
    const STATIONS: usize = 6;
    let mut pts = Vec::new();
    match *p {
        Primitive::Ball { centre, r } => {
            const N: usize = 32;
            for k in 0..N {
                pts.push(v::add(centre, v::mul(fibonacci_sphere_direction(k, N), r)));
            }
        }
        Primitive::Frustum { a, ra, b, rb } => cone_ring_samples(&mut pts, a, ra, b, rb, RING, STATIONS),
        Primitive::RaftPad { base, r, h } => {
            let top = v::add(base, [0.0, 0.0, h]);
            cone_ring_samples(&mut pts, base, r, top, r, RING, STATIONS);
        }
        Primitive::RaftBridge { a, b, width, h } => {
            let hw = width * 0.5;
            let ax = [b[0] - a[0], b[1] - a[1]];
            let len = ax[0].hypot(ax[1]).max(1e-9);
            let u = [ax[0] / len, ax[1] / len];
            let nrm = [-u[1], u[0]];
            let zlo = a[2].min(b[2]);
            for s in [0.0, 0.5, 1.0] {
                let cx = a[0] + (b[0] - a[0]) * s;
                let cy = a[1] + (b[1] - a[1]) * s;
                for sgn in [-1.0, 1.0] {
                    for zz in [zlo, zlo + h] {
                        pts.push([cx + sgn * nrm[0] * hw, cy + sgn * nrm[1] * hw, zz]);
                    }
                }
            }
        }
    }
    pts
}

/// Rings of points around a capped cone from `a` (radius `ra`) to `b`
/// (radius `rb`), at `stations` evenly spaced axial fractions including both
/// ends, `ring` points per station.
fn cone_ring_samples(pts: &mut Vec<V3>, a: V3, ra: f64, b: V3, rb: f64, ring: usize, stations: usize) {
    let axis = v::sub(b, a);
    let Some(u) = v::normalize(axis) else {
        pts.push(a);
        return;
    };
    let (e1, e2, _) = v::basis_from_axis(u);
    for si in 0..=stations {
        let s = si as f64 / stations as f64;
        let centre = v::add(a, v::mul(axis, s));
        let r = ra + (rb - ra) * s;
        if r <= 0.0 {
            pts.push(centre);
            continue;
        }
        for k in 0..ring {
            let theta = 2.0 * std::f64::consts::PI * (k as f64) / (ring as f64);
            let off = v::add(v::mul(e1, r * theta.cos()), v::mul(e2, r * theta.sin()));
            pts.push(v::add(centre, off));
        }
    }
}

// ---------------------------------------------------------------------------
// Free-end classification (clause (b))
// ---------------------------------------------------------------------------

enum EndStatus {
    Grounded,
    /// The nearest overlapping primitive. Kept for symmetry with
    /// [`EndStatus::FusionUncertain`] even though callers only match on the
    /// variant today; a future caller reporting "overlapped by record X" gets
    /// it for free.
    Overlapped(#[allow(dead_code)] usize),
    /// The nearest primitive within the fusion-uncertain band, and the
    /// separation. Already surfaced at the pair level by the overlap-graph
    /// loop; kept here so a future per-end report does not need to re-derive
    /// it.
    FusionUncertain(#[allow(dead_code)] usize, #[allow(dead_code)] f64),
    Free(f64),
}

fn end_status(end: V3, exclude: usize, ok: &[(usize, Primitive)], z_ground: f64) -> EndStatus {
    // EXEMPT, EQUALITY-BAND (`audit-interval-policy.md` §1.5, §3.4): the same
    // plate-coincidence test as the grounding loop above, and consistent with
    // it by construction. No delivered-versus-required relation, no
    // conservative side, no third outcome.
    let tol = c::tol_geom_at(end);
    if end[2] <= z_ground + tol {
        return EndStatus::Grounded;
    }
    let mut best_d = f64::INFINITY;
    let mut best_j = None;
    for &(j, pj) in ok {
        if j == exclude {
            continue;
        }
        let d = pj.signed_distance(end);
        if d < best_d {
            best_d = d;
            best_j = Some(j);
        }
    }
    match best_j {
        None => EndStatus::Free(f64::INFINITY),
        Some(j) => {
            // Same `tol_geom` reasoning as the pairwise overlap-graph loop:
            // an end meant to touch another solid exactly can compute a
            // signed distance a few `eps32` either side of zero.
            if best_d <= tol {
                EndStatus::Overlapped(j)
            } else if best_d <= c::D3_MM {
                EndStatus::FusionUncertain(j, best_d)
            } else {
                EndStatus::Free(best_d)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{AuditContext, AuditOptions};
    use crate::reader::ChituboxFile;
    use crate::synth::{SynthFile, SynthModel, SynthRecord};

    fn run_on(model: SynthModel) -> (CheckOutcome, ModelContext<'static>) {
        // Leaked deliberately: a unit test's whole process lifetime is the
        // borrow this needs, and the crate has no dependency that would let
        // us build a self-referential struct instead.
        let file = Box::leak(Box::new(SynthFile::new().with_model(model)));
        let bytes = file.to_bytes();
        let parsed = Box::leak(Box::new(
            ChituboxFile::parse(std::path::Path::new("test.chitubox"), &bytes)
                .expect("synth file parses"),
        ));
        let options = Box::leak(Box::new(AuditOptions::default()));
        let audit = Box::leak(Box::new(AuditContext {
            file: parsed,
            options,
            sidecar: None,
            sidecar_absence: Some("unit test".into()),
        }));
        let ctx = ModelContext::build(audit, 0);
        let outcome = LoadPath.run(&ctx);
        (outcome, ctx)
    }

    /// tip -> taper -> member -> flare -> pad, pad sitting exactly on the
    /// plate. A clean pass: the negative control every check should ship.
    fn chain_touching_plate() -> SynthModel {
        let translate = [0.0, 0.0, 5.0]; // z_plate = -translate_z = -5.0
        let z_plate = -translate[2];
        let tip = SynthRecord::new(
            RecordKind::ContactSphere,
            [0.0, 0.0, -0.3],
            [0.0, 0.0, 0.3],
            0.3,
            0.3,
        );
        let taper = SynthRecord::new(
            RecordKind::ContactTaper,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            0.3,
            0.35,
        )
        .with_declared_depth(0.1);
        let member = SynthRecord::new(
            RecordKind::Member,
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -4.0],
            0.35,
            0.35,
        );
        let flare = SynthRecord::new(
            RecordKind::RootFlare,
            [0.0, 0.0, -4.0],
            [0.0, 0.0, z_plate],
            0.35,
            1.1,
        );
        let mut pad = SynthRecord::new(
            RecordKind::RaftPadOrBridge,
            [0.0, 0.0, z_plate],
            [0.0, 0.0, z_plate],
            2.2,
            0.2,
        );
        pad = pad.as_pad();
        SynthModel {
            name: "m".into(),
            translate_mm: translate,
            mesh: Vec::new(),
            records: vec![tip, taper, member, flare, pad],
        }
    }

    #[test]
    fn a_chain_reaching_the_plate_is_not_orphaned_and_has_no_free_end() {
        let (out, _ctx) = run_on(chain_touching_plate());
        // The tip itself will not ground via a seat (there is no model mesh
        // in this fixture), but it MUST still be reachable via the chain down
        // to the pad, which sits on the plate.
        let orphan_lines: Vec<_> = out
            .failures
            .iter()
            .filter(|f| f.code == "PATH_ORPHAN" || f.code == "PATH_FREE_END")
            .collect();
        assert!(
            orphan_lines.is_empty(),
            "a fully connected chain to the plate must not orphan or free-end: {orphan_lines:?}"
        );
    }

    #[test]
    fn severing_the_member_from_the_flare_frees_an_end() {
        let mut m = chain_touching_plate();
        // Move the member's lower end far away from the flare's top: no
        // longer touching, no longer even in the fusion-uncertain band.
        m.records[2].end_mm = [5.0, 5.0, -4.0];
        let (out, _ctx) = run_on(m);
        assert!(
            out.failures.iter().any(|f| f.code == "PATH_FREE_END"),
            "a severed member end must report PATH_FREE_END: {:?}",
            out.failures
        );
        assert_eq!(out.status(), crate::check::Status::Fail);
    }

    #[test]
    fn a_pair_that_touches_nothing_is_orphaned_not_silently_dropped() {
        let mut m = chain_touching_plate();
        // Two extra members forming their own two-node chain, nowhere near
        // the plate or the rest of the tree.
        let a = SynthRecord::new(
            RecordKind::Member,
            [40.0, 40.0, 10.0],
            [40.0, 40.0, 12.0],
            0.35,
            0.35,
        );
        let b = SynthRecord::new(
            RecordKind::Member,
            [40.0, 40.0, 12.0],
            [40.0, 40.0, 14.0],
            0.35,
            0.35,
        );
        m.records.push(a);
        m.records.push(b);
        let (out, _ctx) = run_on(m);
        let orphan_records: Vec<usize> = out
            .failures
            .iter()
            .filter(|f| f.code == "PATH_ORPHAN")
            .filter_map(|f| f.record)
            .collect();
        assert!(
            orphan_records.contains(&5) && orphan_records.contains(&6),
            "both isolated members must be reported orphaned: {orphan_records:?}"
        );
        assert_eq!(out.status(), crate::check::Status::Fail);
    }

    #[test]
    fn a_bracing_cycle_that_still_reaches_ground_passes() {
        let mut m = chain_touching_plate();
        // Two members sharing both endpoints with the member already in the
        // chain: an undirected cycle, but every node in it still has a route
        // to ground through the shared endpoints.
        let brace = SynthRecord::new(
            RecordKind::Member,
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -4.0],
            0.2,
            0.2,
        );
        m.records.push(brace);
        let (out, _ctx) = run_on(m);
        assert!(
            out.failures
                .iter()
                .all(|f| f.code != "PATH_ORPHAN" && f.code != "PATH_FREE_END"),
            "a cycle that still reaches ground must not be treated as broken: {:?}",
            out.failures
        );
    }

    #[test]
    fn a_gap_inside_the_fusion_uncertain_band_is_indeterminate_never_a_pass() {
        let mut m = chain_touching_plate();
        // Shorten the member so it stops `gap` mm short of the flare's start
        // -- a genuine empty gap INSIDE (0, d3], not touching and not clearly
        // free. (Moving the endpoint to `-4.0 - gap` instead would drive it
        // INTO the flare's own body, since the flare occupies z in
        // [-5.0, -4.0]; the flare's own start is left untouched here.)
        let gap = c::D3_MM * 0.5;
        m.records[2].end_mm = [0.0, 0.0, -4.0 + gap];
        let (out, _ctx) = run_on(m);
        assert!(out.indeterminate > 0, "a band gap must count as indeterminate");
        assert_eq!(
            out.status(),
            crate::check::Status::Fail,
            "indeterminate must never map to pass"
        );
        assert!(
            out.failures
                .iter()
                .any(|f| f.code == "PATH_INDETERMINATE"
                    && f.values.get("reason").and_then(Json::as_str) == Some("fusion_uncertain")),
            "the band gap must be named fusion_uncertain: {:?}",
            out.failures
        );
    }

    #[test]
    fn a_gap_just_outside_the_band_is_a_free_end_not_indeterminate() {
        let mut m = chain_touching_plate();
        let gap = c::D3_MM * 2.0;
        m.records[2].end_mm = [0.0, 0.0, -4.0 + gap];
        let (out, _ctx) = run_on(m);
        assert!(
            out.failures.iter().any(|f| f.code == "PATH_FREE_END"),
            "a gap clearly outside the band must be a free end: {:?}",
            out.failures
        );
    }

    #[test]
    fn a_record_with_no_analytic_solid_is_indeterminate_never_a_pass() {
        let mut m = chain_touching_plate();
        // Zero-length axis: GeomError::ZeroLengthAxis, no analytic solid.
        m.records[2].end_mm = m.records[2].start_mm;
        let (out, _ctx) = run_on(m);
        assert!(
            out.failures.iter().any(|f| f.code == "PATH_INDETERMINATE"
                && f.values.get("reason").and_then(Json::as_str) == Some("primitive_degenerate")),
            "a record with no solid must be reported, never silently dropped: {:?}",
            out.failures
        );
        assert_eq!(out.status(), crate::check::Status::Fail);
    }

    #[test]
    fn a_completely_unsupported_model_reports_root_missing() {
        let m = SynthModel {
            name: "m".into(),
            translate_mm: [0.0, 0.0, 5.0],
            mesh: Vec::new(),
            records: vec![SynthRecord::new(
                RecordKind::Member,
                [0.0, 0.0, 10.0],
                [0.0, 0.0, 12.0],
                0.35,
                0.35,
            )],
        };
        let (out, _ctx) = run_on(m);
        assert!(
            out.failures.iter().any(|f| f.code == "PATH_ROOT_MISSING"),
            "a model with nothing grounded must say so at the model level: {:?}",
            out.failures
        );
    }

    #[test]
    fn a_model_with_no_support_records_is_vacuously_fine() {
        let m = SynthModel {
            name: "m".into(),
            translate_mm: [0.0, 0.0, 5.0],
            mesh: Vec::new(),
            records: Vec::new(),
        };
        let (out, _ctx) = run_on(m);
        assert_eq!(out.checked, 0);
        assert_eq!(out.status(), crate::check::Status::Pass);
    }

    // -----------------------------------------------------------------
    // Real emitted fixtures, `tools/plinth-audit/fixtures/broken_path/`.
    // These were planted by another agent's parallel work in this same
    // checkout, specifically for this invariant (per the directory name
    // and the record layouts below); this suite is read-only with respect
    // to that directory, per this task's file-ownership rule -- it opens
    // the `.chitubox` bytes directly rather than adding a fixture-harness
    // `.expect.json`.
    // -----------------------------------------------------------------

    fn open_fixture(rel: &str) -> ChituboxFile {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
        ChituboxFile::open(&path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    fn run_check(file: &'static ChituboxFile) -> CheckOutcome {
        let options = Box::leak(Box::new(AuditOptions::default()));
        let audit = Box::leak(Box::new(AuditContext {
            file,
            options,
            sidecar: None,
            sidecar_absence: Some("fixture, no sidecar".into()),
        }));
        let ctx = ModelContext::build(audit, 0);
        LoadPath.run(&ctx)
    }

    /// `broken_path_control.chitubox`: tip -> taper -> member -> pad, a
    /// straight chain to the plate. The negative control every check should
    /// ship: a real emitted file this invariant must NOT refuse.
    #[test]
    fn real_fixture_broken_path_control_has_no_orphan_or_free_end() {
        let file = Box::leak(Box::new(open_fixture(
            "fixtures/broken_path/broken_path_control.chitubox",
        )));
        let out = run_check(file);
        let broken: Vec<_> = out
            .failures
            .iter()
            .filter(|f| f.code == "PATH_ORPHAN" || f.code == "PATH_FREE_END")
            .collect();
        assert!(
            broken.is_empty(),
            "the control chain must not be reported broken: {broken:?}"
        );
    }

    /// `broken_path_free_end.chitubox`: the control chain plus one member
    /// branching sideways from the junction at z=-4 to `(4,0,-4)`, ending in
    /// open space. Must fire `PATH_FREE_END` on record 4's `end`.
    #[test]
    fn real_fixture_broken_path_free_end_fires_on_record_4() {
        let file = Box::leak(Box::new(open_fixture(
            "fixtures/broken_path/broken_path_free_end.chitubox",
        )));
        let out = run_check(file);
        assert!(
            out.failures.iter().any(|f| f.code == "PATH_FREE_END"
                && f.record == Some(4)
                && f.values.get("end").and_then(Json::as_str) == Some("end")),
            "record 4's dangling end must be reported free: {:?}",
            out.failures
        );
        assert_eq!(out.status(), crate::check::Status::Fail);
    }

    /// `broken_path_cycle.chitubox`: the control chain plus a THREE-node
    /// triangular brace (three joint balls, three members) far from the
    /// chain and from the plate, closed on itself but reaching nothing.
    /// Every one of those six records must be `PATH_ORPHAN` -- a legitimate
    /// undirected cycle is not, by itself, a route to ground.
    #[test]
    fn real_fixture_broken_path_cycle_orphans_the_disconnected_triangle() {
        let file = Box::leak(Box::new(open_fixture(
            "fixtures/broken_path/broken_path_cycle.chitubox",
        )));
        let out = run_check(file);
        let orphaned: std::collections::BTreeSet<usize> = out
            .failures
            .iter()
            .filter(|f| f.code == "PATH_ORPHAN")
            .filter_map(|f| f.record)
            .collect();
        assert_eq!(
            orphaned,
            std::collections::BTreeSet::from([4, 5, 6, 7, 8, 9]),
            "all six triangle records must be orphaned, and only them: {:?}",
            out.failures
        );
        // component_size=6 on each: the cycle IS one connected component,
        // just not one that reaches ground.
        for f in out.failures.iter().filter(|f| f.code == "PATH_ORPHAN") {
            assert_eq!(
                f.values.get("component_size").and_then(Json::as_f64),
                Some(6.0),
                "record {:?}: {:?}",
                f.record,
                f.values
            );
        }
        assert_eq!(out.status(), crate::check::Status::Fail);
    }

    #[test]
    fn fibonacci_lattice_produces_unit_vectors() {
        for k in 0..37 {
            let d = fibonacci_sphere_direction(k, 37);
            let len = v::norm(d);
            assert!((len - 1.0).abs() < 1e-9, "k={k} len={len}");
        }
    }

    #[test]
    fn aabb_gap_is_zero_when_overlapping_and_positive_when_apart() {
        let a = Aabb {
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 1.0],
        };
        let b = Aabb {
            min: [0.5, 0.5, 0.5],
            max: [1.5, 1.5, 1.5],
        };
        assert_eq!(aabb_gap(&a, &b), 0.0);
        let c_box = Aabb {
            min: [10.0, 0.0, 0.0],
            max: [11.0, 1.0, 1.0],
        };
        assert!((aabb_gap(&a, &c_box) - 9.0).abs() < 1e-12);
    }
}
