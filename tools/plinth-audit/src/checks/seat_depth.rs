//! I1 seat depth -- `docs/specs/audit-invariants.md` §3.1.
//!
//! Grades every contact's delivered seat depth, measured along the support
//! axis by [`crate::seat::reconstruct_seats`], against that contact's OWN
//! requirement `h_req = max(default_contact_depth_mm, layer_height_mm +
//! r_tip*sin_theta, ball_seat_factor(f)*r_tip)`. A tip at exactly zero depth
//! is a hard failure regardless of anything else. The three tip radii --
//! `R/f` from the TIP record, `r_start` of the adjacent TAPER, and the sphere
//! fitted to the TIP soup -- must agree within `tol_geom`. The TAPER's
//! declared depth must equal the delivered one; this is the check that turns
//! the known 0.300 mm lie into a per-contact finding.
//!
//! **`h_delivered`, `h_required`, the three-way radius comparison and every
//! indeterminate reason are read from [`ModelContext::seats`], never
//! recomputed here** -- I2, I3, I5 and I6 read the same value and four
//! private answers would grade four different solids. This module owns only
//! the predicate, the failure lines and the per-contact bookkeeping.
//!
//! Every one of the four checks below is evaluated independently per
//! contact, so a single contact can carry more than one finding (e.g. both a
//! shallow seat and a declared-depth lie): (A) the three-way tip-radius
//! agreement, always evaluable; (B) the depth predicate itself, which is
//! `indeterminate` before it is ever `pass` or `fail`; (C) the TAPER's
//! declared depth against the reconstructed one, whenever both exist.

use crate::check::{Check, CheckOutcome, Failure};
use crate::checks::interval::{self, Verdict};
use crate::context::ModelContext;
use crate::json::Json;
use crate::seat::{BindingTerm, Seat};
use crate::vec3::V3;

pub struct SeatDepth;

impl Check for SeatDepth {
    fn id(&self) -> &'static str {
        "i1_seat_depth"
    }

    fn title(&self) -> &'static str {
        "I1 seat depth"
    }

    fn scope(&self, ctx: &ModelContext) -> usize {
        ctx.seats.len()
    }

    fn run(&self, ctx: &ModelContext) -> CheckOutcome {
        let file = ctx.file();
        let model = ctx.model_index;
        let mut out = CheckOutcome::new(ctx.seats.len());

        let mut rows = Vec::with_capacity(ctx.seats.len());
        let mut worst_deficit_mm: Option<f64> = None;
        let mut zero_count = 0usize;
        let mut shallow_count = 0usize;
        let mut radius_mismatch_count = 0usize;
        let mut taper_mismatch_count = 0usize;
        let mut indeterminate_count = 0usize;
        let mut band_indeterminate_count = 0usize;

        for seat in &ctx.seats {
            let ev = evaluate_seat(&file, model, seat);
            if ev.zero {
                zero_count += 1;
            }
            if ev.shallow {
                shallow_count += 1;
            }
            if ev.radius_mismatch {
                radius_mismatch_count += 1;
            }
            if ev.taper_mismatch {
                taper_mismatch_count += 1;
            }
            if ev.band_indeterminate {
                band_indeterminate_count += 1;
            }
            if let Some(d) = ev.deficit_mm {
                worst_deficit_mm = Some(worst_deficit_mm.map_or(d, |w: f64| w.max(d)));
            }
            indeterminate_count += ev.indeterminates.len();
            for f in ev.failures {
                out.fail(f);
            }
            for f in ev.indeterminates {
                out.indeterminate(f);
            }
            rows.push(ev.row);
        }

        let mut measured = Json::obj();
        measured.set("contacts", Json::Arr(rows));
        measured.set("zero_count", Json::usize(zero_count));
        measured.set("shallow_count", Json::usize(shallow_count));
        measured.set(
            "radius_three_way_mismatch_count",
            Json::usize(radius_mismatch_count),
        );
        measured.set(
            "taper_declared_mismatch_count",
            Json::usize(taper_mismatch_count),
        );
        measured.set("indeterminate_count", Json::usize(indeterminate_count));
        // Broken out of `indeterminate_count` so a reader can tell "the seat
        // could not be reconstructed" from "the seat was reconstructed and
        // the instrument cannot resolve it against its own requirement"
        // (`audit-interval-policy.md` §5). Both gate as failures.
        measured.set(
            "depth_band_indeterminate_count",
            Json::usize(band_indeterminate_count),
        );
        measured.set("worst_deficit_mm", Json::num_or_null(worst_deficit_mm));
        measured.set(
            "default_contact_depth_mm",
            Json::num(ctx.inputs().default_contact_depth_mm),
        );
        measured.set(
            "ball_seat_factor",
            Json::num(crate::consts::ball_seat_factor(ctx.inputs().ball_factor_stub)),
        );

        let n = ctx.seats.len();
        let headline = if n == 0 {
            "no contacts on this model".to_string()
        } else if let Some(w) = worst_deficit_mm {
            format!("worst deficit {w:.4} mm")
        } else if zero_count > 0 {
            format!("{zero_count} contact(s) with zero seat depth")
        } else if radius_mismatch_count + taper_mismatch_count > 0 {
            format!(
                "{} declared/reconstructed mismatch(es)",
                radius_mismatch_count + taper_mismatch_count
            )
        } else if indeterminate_count > 0 {
            format!("{indeterminate_count} contact(s) indeterminate")
        } else {
            format!("{n} contacts meet their own seat requirement")
        };
        measured.set("headline", Json::str(headline));

        out.measured = measured;
        out
    }
}

/// What evaluating one seat produced: the per-contact JSON row (always, per
/// §5 rule 3), every failure and indeterminate line it fired, and the flags
/// the caller aggregates into the invariant-wide counts.
struct SeatEval {
    row: Json,
    failures: Vec<Failure>,
    indeterminates: Vec<Failure>,
    /// `h_required - h_delivered` when the shallow predicate fired.
    deficit_mm: Option<f64>,
    radius_mismatch: bool,
    taper_mismatch: bool,
    zero: bool,
    shallow: bool,
    /// The depth interval straddled the requirement: not established
    /// adequate, not established short (`audit-interval-policy.md` §3.2).
    band_indeterminate: bool,
}

/// Grade one contact's seat. Pure with respect to the reconstruction: every
/// input comes from `seat`, nothing is recomputed.
fn evaluate_seat(file: &str, model: usize, seat: &Seat) -> SeatEval {
    let contact = seat.contact_id;
    // `contact_id` IS the TIP record's own index (`audit-invariants.md` §3),
    // so the record under test for every I1 finding on this contact is that
    // same index.
    let record = seat.contact_id;

    let mut row = Json::obj();
    row.set("contact", Json::usize(contact));
    row.set("record", Json::usize(record));
    row.set("p0_mm", Json::v3(seat.p0));
    row.set("u", seat.u.map_or(Json::Null, Json::v3));
    row.set("r_ball_mm", Json::num(seat.r_ball_mm));
    row.set("r_tip_a_mm", Json::num(seat.r_tip_a_mm));
    row.set("r_tip_b_mm", Json::num_or_null(seat.r_tip_b_mm));
    row.set("r_tip_c_mm", Json::num_or_null(seat.r_tip_c_mm));
    row.set("r_tip_spread_mm", Json::num(seat.r_tip_spread_mm));
    row.set("r_tip_tol_mm", Json::num(seat.r_tip_tol_mm));
    row.set("r_tip_agrees", Json::Bool(seat.r_tip_agrees));
    row.set("h_delivered_mm", Json::num_or_null(seat.h_delivered_mm));
    row.set("h_perp_mm", Json::num_or_null(seat.h_perp_mm));
    row.set("h_required_mm", Json::num_or_null(seat.h_required_mm));
    row.set(
        "binding_term",
        seat.binding_term.map_or(Json::Null, |b| Json::str(b.token())),
    );
    row.set("sin_theta", Json::num_or_null(seat.sin_theta));
    row.set(
        "declared_depth_mm",
        Json::num_or_null(seat.declared_depth_mm),
    );
    row.set(
        "normal_disagreement_deg",
        Json::num_or_null(seat.normal_disagreement_deg),
    );
    row.set(
        "indeterminate_reason",
        seat.indeterminate.map_or(Json::Null, Json::str),
    );

    let mut failures = Vec::new();
    let mut indeterminates = Vec::new();
    let mut deficit_mm = None;
    let mut radius_mismatch = false;
    let mut taper_mismatch = false;
    let mut zero = false;
    let mut shallow = false;
    // The seat whose interval straddles its own requirement: neither
    // established adequate nor established short (`audit-interval-policy.md`
    // §3.2). Counted separately from the reconstruction-level indeterminates
    // so a reader can tell a blunt instrument from an unreadable seat.
    let mut band_indeterminate = false;

    // --- (A) The three-way tip-radius agreement. Always evaluable: it is
    // computed by the shared reconstruction before anything about the axis
    // crossing is known, so it fires independently of whether the depth
    // predicate below can be graded at all. ---
    if !seat.r_tip_agrees {
        radius_mismatch = true;
        let declared_mm = seat.r_tip_a_mm;
        // The genuinely RECONSTRUCTED comparator is the fitted-soup radius;
        // fall back to the adjacent TAPER's declared radius only when the fit
        // did not converge, so the line still names the pair that disagreed.
        let reconstructed_mm = seat
            .r_tip_c_mm
            .or(seat.r_tip_b_mm)
            .unwrap_or(seat.r_tip_a_mm);
        let tol = seat.r_tip_tol_mm;
        let mut values = Json::obj();
        values.set("declared_mm", Json::num(declared_mm));
        values.set("reconstructed_mm", Json::num(reconstructed_mm));
        values.set("tolerance_mm", Json::num(tol));
        values.set("reason", Json::str("tip_radius_three_way"));
        values.set("r_tip_a_mm", Json::num(seat.r_tip_a_mm));
        values.set("r_tip_b_mm", Json::num_or_null(seat.r_tip_b_mm));
        values.set("r_tip_c_mm", Json::num_or_null(seat.r_tip_c_mm));
        values.set("spread_mm", Json::num(seat.r_tip_spread_mm));
        failures.push(Failure::line(
            "SEAT_DEPTH_DECLARED_MISMATCH",
            file,
            model,
            Some(contact),
            Some(record),
            &[
                ("declared_mm", fmt(declared_mm)),
                ("reconstructed_mm", fmt(reconstructed_mm)),
                ("tolerance_mm", fmt(tol)),
                ("reason", "tip_radius_three_way".to_string()),
            ],
            values,
        ));
    }

    // --- (B) The depth predicate: `h_delivered > 0` and, in the
    // conservative minimum-type form of `audit-interval-policy.md` §3.2,
    // `h_delivered - tol_geom >= h_required`. Indeterminate dominates: an
    // unestablished quantity never gets a pass/fail verdict computed on top
    // of it. ---
    match seat.h_delivered_mm {
        None => {
            // Only reachable when `Seat::indeterminate` is set (the axis
            // crossing itself was never established); default the reason
            // defensively rather than emit an unreasoned indeterminate line.
            let reason = seat.indeterminate.unwrap_or("ray_parity_exhausted");
            indeterminates.push(indeterminate_line(file, model, contact, record, reason));
        }
        Some(h) if h <= 0.0 => {
            // Established and exactly zero: a ball resting on or outside the
            // surface transfers load across a tangent point and has no cut
            // disc. This is a hard failure, not an unknown.
            zero = true;
            let mut values = Json::obj();
            values.set("h_delivered_mm", Json::num(h));
            values.set("p0", Json::v3(seat.p0));
            values.set("u", seat.u.map_or(Json::Null, Json::v3));
            failures.push(Failure::line(
                "SEAT_DEPTH_ZERO",
                file,
                model,
                Some(contact),
                Some(record),
                &[
                    ("h_delivered_mm", fmt(h)),
                    ("p0", fmt_v3(seat.p0)),
                    ("u", seat.u.map_or_else(|| "?,?,?".to_string(), fmt_v3)),
                ],
                values,
            ));
        }
        Some(h) => {
            if let Some(reason) = seat.indeterminate {
                // A crossing was found but the incidence normal (or the
                // taper link, etc.) could not be trusted enough to grade it
                // -- e.g. `normal_ambiguous`. The depth is known; the
                // REQUIREMENT is not, so the predicate cannot be applied.
                indeterminates.push(indeterminate_line(file, model, contact, record, reason));
            } else {
                match (seat.h_required_mm, seat.sin_theta, seat.binding_term) {
                    (Some(h_req), Some(sin_theta), Some(term)) => {
                        let tol = seat.tol_geom_mm();
                        let (verdict, deficit) = shallow_verdict(h, h_req, tol);
                        row.set("depth_verdict", Json::str(verdict.as_str()));
                        let (edge_lo, edge_hi) = interval::edges(h, tol);
                        row.set("h_delivered_interval_mm", Json::nums([edge_lo, edge_hi]));
                        row.set("depth_tolerance_mm", Json::num(tol));
                        match verdict {
                            Verdict::Pass => {}
                            Verdict::Fail => {
                                shallow = true;
                                deficit_mm = Some(deficit);
                                let mut values = Json::obj();
                                values.set("h_delivered_mm", Json::num(h));
                                values.set("h_required_mm", Json::num(h_req));
                                values.set("binding_term", Json::str(term.token()));
                                values.set("r_tip_mm", Json::num(seat.r_tip_mm));
                                values.set("sin_theta", Json::num(sin_theta));
                                values.set("deficit_mm", Json::num(deficit));
                                failures.push(Failure::line(
                                    "SEAT_DEPTH_SHALLOW",
                                    file,
                                    model,
                                    Some(contact),
                                    Some(record),
                                    &[
                                        ("h_delivered_mm", fmt(h)),
                                        ("h_required_mm", fmt(h_req)),
                                        ("binding_term", term.token().to_string()),
                                        ("r_tip_mm", fmt(seat.r_tip_mm)),
                                        ("sin_theta", fmt(sin_theta)),
                                        ("deficit_mm", fmt(deficit)),
                                    ],
                                    values,
                                ));
                            }
                            // The interval straddles the requirement: the
                            // seat is neither established as adequate nor
                            // established as short. It gates as a failure
                            // (`audit-invariants.md` §0 rule 3) and reports
                            // its own interval so a reader can see how close
                            // it was (`audit-interval-policy.md` §5).
                            Verdict::Indeterminate => {
                                band_indeterminate = true;
                                indeterminates.push(band_indeterminate_line(
                                    file, model, contact, record, h, h_req, tol, term, seat,
                                ));
                            }
                        }
                    }
                    _ => {
                        // The geometry says "established" (no indeterminate
                        // reason set) but the requirement itself is missing a
                        // term. This should not happen given the shared
                        // reconstruction's own invariants, but the never-pass
                        // rule means a defensive gap here must still fail
                        // loudly rather than silently skip the predicate.
                        indeterminates.push(indeterminate_line(
                            file,
                            model,
                            contact,
                            record,
                            "normal_ambiguous",
                        ));
                    }
                }
            }
        }
    }

    // --- (C) Declared vs. reconstructed depth: the TAPER's trailing slot 0
    // against `h_delivered`. Independent of whether (B) could be graded --
    // it only needs `h_delivered` to be KNOWN, not the incidence term. ---
    if let (Some(declared), Some(h)) = (seat.declared_depth_mm, seat.h_delivered_mm) {
        // EXEMPT, EQUALITY-BAND (`audit-interval-policy.md` §1.1 row 6,
        // §3.4): the TAPER's DECLARED depth against the RECONSTRUCTED one --
        // two accounts of the same physical quantity, not a delivered value
        // against a design requirement. There is no safety margin under
        // test, so no side is the conservative one and there is no third
        // outcome to add. `|a - b| <= tol` is already symmetric and already
        // conservative.
        let tol = seat.tol_geom_mm();
        if (declared - h).abs() > tol {
            taper_mismatch = true;
            let mut values = Json::obj();
            values.set("declared_mm", Json::num(declared));
            values.set("reconstructed_mm", Json::num(h));
            values.set("tolerance_mm", Json::num(tol));
            values.set("reason", Json::str("taper_trailing"));
            failures.push(Failure::line(
                "SEAT_DEPTH_DECLARED_MISMATCH",
                file,
                model,
                Some(contact),
                Some(record),
                &[
                    ("declared_mm", fmt(declared)),
                    ("reconstructed_mm", fmt(h)),
                    ("tolerance_mm", fmt(tol)),
                    ("reason", "taper_trailing".to_string()),
                ],
                values,
            ));
        }
    }

    SeatEval {
        row,
        failures,
        indeterminates,
        deficit_mm,
        radius_mismatch,
        taper_mismatch,
        zero,
        shallow,
        band_indeterminate,
    }
}

/// The straddling-band line of `audit-interval-policy.md` §5: same code as
/// every other I1 indeterminate, a new reason token, and **the interval
/// itself**, because an indeterminate that reports only "indeterminate" is no
/// more evidence than a pass that reports no numbers (§5 pinned-rule 3).
#[allow(clippy::too_many_arguments)]
fn band_indeterminate_line(
    file: &str,
    model: usize,
    contact: usize,
    record: usize,
    h: f64,
    h_req: f64,
    tol: f64,
    term: BindingTerm,
    seat: &Seat,
) -> Failure {
    let (lo, hi) = interval::edges(h, tol);
    let mut values = Json::obj();
    values.set("reason", Json::str(interval::REASON_WITHIN_TOLERANCE_BAND));
    values.set("h_delivered_mm", Json::num(h));
    values.set("h_delivered_interval_mm", Json::nums([lo, hi]));
    values.set("h_required_mm", Json::num(h_req));
    values.set("tolerance_mm", Json::num(tol));
    values.set("binding_term", Json::str(term.token()));
    values.set("r_tip_mm", Json::num(seat.r_tip_mm));
    values.set("shortfall_mm", Json::num(h_req - h));
    Failure::line(
        "SEAT_DEPTH_INDETERMINATE",
        file,
        model,
        Some(contact),
        Some(record),
        &[
            ("reason", interval::REASON_WITHIN_TOLERANCE_BAND.to_string()),
            ("h_delivered_mm", fmt(h)),
            ("h_delivered_interval_mm", interval::fmt_interval(h, tol)),
            ("h_required_mm", fmt(h_req)),
            ("tolerance_mm", fmt(tol)),
        ],
        values,
    )
}

fn indeterminate_line(
    file: &str,
    model: usize,
    contact: usize,
    record: usize,
    reason: &'static str,
) -> Failure {
    let mut values = Json::obj();
    values.set("reason", Json::str(reason));
    Failure::line(
        "SEAT_DEPTH_INDETERMINATE",
        file,
        model,
        Some(contact),
        Some(record),
        &[("reason", reason.to_string())],
        values,
    )
}

/// The predicate of `audit-invariants.md` §3.1, in the conservative
/// **minimum-type** form of `audit-interval-policy.md` §3.2 row 1: the seat
/// must reach its own requirement, so the conservative bound of the delivered
/// depth is its LOWER bound.
///
/// ```text
/// PASS           h_delivered - tol >= h_required
/// FAIL           h_delivered + tol <  h_required
/// INDETERMINATE  otherwise -- the interval straddles h_required
/// ```
///
/// **This replaces `h_delivered >= h_required - tol`**, which is
/// algebraically `h_delivered + tol >= h_required` and therefore graded the
/// delivered depth's *upper* bound against the requirement. A seat genuinely
/// shallower than its requirement, by less than `tol_geom`, used to certify.
///
/// The deficit returned alongside is the shortfall against the bare
/// requirement, unchanged, so the failure line still reports how far short
/// the seat is rather than how far short its interval edge is.
fn shallow_verdict(h_delivered_mm: f64, h_required_mm: f64, tol_mm: f64) -> (Verdict, f64) {
    (
        interval::min_type(h_delivered_mm, h_required_mm, tol_mm),
        h_required_mm - h_delivered_mm,
    )
}

fn fmt(v: f64) -> String {
    format!("{v}")
}
fn fmt_v3(p: V3) -> String {
    format!("{},{},{}", p[0], p[1], p[2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consts as c;
    use crate::graph::Link;
    use crate::seat::BindingTerm;

    /// A seat that is established, agrees on radius, meets its own
    /// requirement, and whose declared taper depth matches the delivered
    /// one. Individual tests mutate one field at a time to isolate a single
    /// predicate.
    fn base_seat() -> Seat {
        Seat {
            contact_id: 7,
            taper: Link::Unique(8),
            p0: [0.0, 0.0, 0.0],
            u: Some([0.0, 0.0, 1.0]),
            centre: [0.0, 0.0, 0.3],
            r_ball_mm: 0.45,
            r_tip_a_mm: 0.3,
            r_tip_b_mm: Some(0.3),
            r_tip_c_mm: Some(0.3),
            fitted_centre_mm: Some([0.0, 0.0, 0.3]),
            r_tip_spread_mm: 0.0,
            r_tip_tol_mm: 1e-6,
            r_tip_agrees: true,
            r_tip_mm: 0.3,
            h_delivered_mm: Some(0.2),
            entry_point_mm: Some([0.0, 0.0, 0.2]),
            entry_normal: Some([0.0, 0.0, -1.0]),
            entry_triangle: Some(0),
            entry_on_mesh_boundary: false,
            entry_component: Some(0),
            h_perp_mm: Some(0.2),
            abs_cos: Some(1.0),
            sin_theta: Some(0.0),
            mean_normal: Some([0.0, 0.0, -1.0]),
            normal_disagreement_deg: Some(0.0),
            normal_ambiguous: false,
            h_required_mm: Some(0.1),
            binding_term: Some(BindingTerm::Default),
            h_terms_mm: Some([0.1, 0.05, 0.11]),
            declared_depth_mm: Some(0.2),
            indeterminate: None,
        }
    }

    #[test]
    fn a_seat_that_meets_every_predicate_passes_clean() {
        let seat = base_seat();
        let ev = evaluate_seat("f.chitubox", 0, &seat);
        assert!(ev.failures.is_empty(), "{:?}", ev.failures);
        assert!(ev.indeterminates.is_empty());
        assert!(!ev.zero && !ev.shallow && !ev.radius_mismatch && !ev.taper_mismatch);
    }

    /// **This test asserted the opposite before `audit-interval-policy.md`
    /// §3.2 row 1**, under the name `the_boundary_itself_passes`: a seat
    /// exactly one `tol_geom` SHORT of its requirement was pinned as a pass,
    /// on the reasoning that the retired predicate `h >= h_req - tol` is an
    /// equality at that point. That is Finding 3 written down as a passing
    /// test.
    ///
    /// It matters that this is asserted positively. `shallow` and
    /// `deficit_mm` are both false/None for an INDETERMINATE seat too, so
    /// the old assertions stayed green after the fix while no longer
    /// testing anything -- the verdict has to be read directly.
    #[test]
    fn a_seat_one_tolerance_short_is_indeterminate_not_a_pass() {
        let mut seat = base_seat();
        let tol = seat.tol_geom_mm();
        seat.h_delivered_mm = Some(0.2);
        seat.h_required_mm = Some(0.2 + tol);
        let ev = evaluate_seat("f", 0, &seat);
        assert!(!ev.shallow, "not a measured shortfall");
        assert!(ev.deficit_mm.is_none());
        assert!(
            ev.band_indeterminate,
            "a seat short by less than tol_geom must not certify"
        );
        assert_eq!(ev.indeterminates.len(), 1);
        let f = &ev.indeterminates[0];
        assert_eq!(f.code, "SEAT_DEPTH_INDETERMINATE");
        assert!(
            f.message.contains("reason=within_tolerance_band"),
            "{}",
            f.message
        );
        // §5: the interval must reach the reader, not just the word.
        assert!(
            f.message.contains("h_delivered_interval_mm=["),
            "{}",
            f.message
        );
        assert_eq!(
            ev.row.get("depth_verdict").and_then(Json::as_str),
            Some("indeterminate")
        );
    }

    /// The other side of the same boundary: a seat clear of the band by more
    /// than `tol_geom` still passes cleanly, so the fix tightened the gate
    /// rather than making it refuse everything.
    #[test]
    fn a_seat_clear_of_the_band_still_passes() {
        let mut seat = base_seat();
        let tol = seat.tol_geom_mm();
        seat.h_delivered_mm = Some(0.2);
        seat.h_required_mm = Some(0.2 - 2.0 * tol);
        let ev = evaluate_seat("f", 0, &seat);
        assert!(!ev.shallow && !ev.band_indeterminate);
        assert!(ev.indeterminates.is_empty(), "{:?}", ev.indeterminates);
        assert_eq!(
            ev.row.get("depth_verdict").and_then(Json::as_str),
            Some("pass")
        );
    }

    #[test]
    fn one_tolerance_short_of_the_boundary_fails_with_a_deficit() {
        let mut seat = base_seat();
        let tol = seat.tol_geom_mm();
        seat.h_delivered_mm = Some(0.2);
        seat.h_required_mm = Some(0.2 + 2.0 * tol);
        let ev = evaluate_seat("f", 0, &seat);
        assert!(ev.shallow);
        let deficit = ev.deficit_mm.expect("deficit reported");
        // h_required = h_delivered + 2*tol, so deficit = h_required - h_delivered = 2*tol.
        assert!((deficit - 2.0 * tol).abs() < 1e-15, "deficit={deficit} tol={tol}");
        assert!(ev
            .failures
            .iter()
            .any(|f| f.code == "SEAT_DEPTH_SHALLOW" && f.message.contains("deficit_mm=")));
    }

    #[test]
    fn zero_depth_is_a_hard_failure_even_when_the_requirement_is_trivially_zero() {
        let mut seat = base_seat();
        seat.h_delivered_mm = Some(0.0);
        seat.h_required_mm = Some(0.0);
        // Isolate: also make the declared depth agree with zero, so this
        // test exercises ONLY the zero-depth predicate and not (C) as well.
        seat.declared_depth_mm = Some(0.0);
        let ev = evaluate_seat("f", 0, &seat);
        assert!(ev.zero);
        assert_eq!(ev.failures.len(), 1);
        assert_eq!(ev.failures[0].code, "SEAT_DEPTH_ZERO");
        assert!(ev.indeterminates.is_empty());
    }

    #[test]
    fn zero_depth_also_surfaces_a_disagreeing_declared_taper_depth() {
        // The realistic corpus case: every emitted TAPER declares 0.3 mm
        // regardless of delivery, so a zero-depth contact fires BOTH
        // SEAT_DEPTH_ZERO and SEAT_DEPTH_DECLARED_MISMATCH -- two distinct,
        // independently true findings about the same contact.
        let mut seat = base_seat();
        seat.h_delivered_mm = Some(0.0);
        seat.h_required_mm = Some(0.0);
        seat.declared_depth_mm = Some(0.3);
        let ev = evaluate_seat("f", 0, &seat);
        assert!(ev.zero && ev.taper_mismatch);
        assert_eq!(ev.failures.len(), 2);
    }

    #[test]
    fn an_unestablished_axis_crossing_is_indeterminate_never_pass() {
        let mut seat = base_seat();
        seat.h_delivered_mm = None;
        seat.h_required_mm = None;
        seat.sin_theta = None;
        seat.binding_term = None;
        seat.indeterminate = Some("ray_parity_exhausted");
        let ev = evaluate_seat("f", 0, &seat);
        assert!(ev.failures.is_empty());
        assert_eq!(ev.indeterminates.len(), 1);
        assert_eq!(ev.indeterminates[0].code, "SEAT_DEPTH_INDETERMINATE");
        assert!(ev.indeterminates[0].message.contains("reason=ray_parity_exhausted"));
    }

    #[test]
    fn a_missing_taper_is_indeterminate_and_skips_the_declared_comparison() {
        let mut seat = base_seat();
        seat.taper = Link::Missing;
        seat.declared_depth_mm = None;
        seat.indeterminate = Some("no_adjacent_taper");
        let ev = evaluate_seat("f", 0, &seat);
        assert_eq!(ev.indeterminates.len(), 1);
        assert!(!ev.taper_mismatch, "no declared depth to disagree with");
    }

    #[test]
    fn normal_ambiguous_is_indeterminate_even_though_a_depth_value_exists() {
        // h_delivered IS known here -- the incidence angle is what could not
        // be trusted. The predicate must not silently grade against an
        // unverified incidence term.
        let mut seat = base_seat();
        seat.indeterminate = Some("normal_ambiguous");
        seat.normal_ambiguous = true;
        let ev = evaluate_seat("f", 0, &seat);
        assert!(ev.failures.iter().all(|f| f.code != "SEAT_DEPTH_SHALLOW"));
        assert_eq!(ev.indeterminates.len(), 1);
        assert!(ev.indeterminates[0].message.contains("reason=normal_ambiguous"));
    }

    #[test]
    fn a_radius_three_way_disagreement_is_a_hard_failure_independent_of_depth() {
        let mut seat = base_seat();
        seat.r_tip_agrees = false;
        seat.r_tip_c_mm = Some(0.5);
        seat.r_tip_spread_mm = 0.2;
        let ev = evaluate_seat("f", 0, &seat);
        assert!(ev.radius_mismatch);
        assert!(ev
            .failures
            .iter()
            .any(|f| f.code == "SEAT_DEPTH_DECLARED_MISMATCH"
                && f.message.contains("reason=tip_radius_three_way")));
        // And the (independent) depth predicate still passed.
        assert!(!ev.shallow && !ev.zero);
    }

    #[test]
    fn the_known_0_3mm_lie_is_caught_as_a_declared_mismatch() {
        let mut seat = base_seat();
        // The corpus fact this check exists to surface: every emitted TAPER
        // declares 0.3 mm regardless of what was actually delivered.
        seat.declared_depth_mm = Some(0.3);
        seat.h_delivered_mm = Some(0.05);
        seat.h_required_mm = Some(0.05); // isolate: the seat still meets its own req
        let ev = evaluate_seat("f", 0, &seat);
        assert!(!ev.shallow, "isolating the taper check from the depth predicate");
        assert!(ev.taper_mismatch);
        assert!(ev
            .failures
            .iter()
            .any(|f| f.code == "SEAT_DEPTH_DECLARED_MISMATCH"
                && f.message.contains("reason=taper_trailing")));
    }

    #[test]
    fn a_contact_can_fail_two_independent_ways_at_once() {
        let mut seat = base_seat();
        seat.r_tip_agrees = false;
        seat.r_tip_c_mm = Some(0.5);
        seat.h_delivered_mm = Some(0.0);
        seat.h_required_mm = Some(0.1);
        seat.declared_depth_mm = Some(0.0); // isolate: don't also trip (C)
        let ev = evaluate_seat("f", 0, &seat);
        assert!(ev.radius_mismatch && ev.zero);
        assert_eq!(ev.failures.len(), 2);
    }

    #[test]
    fn the_failure_line_and_the_json_row_carry_the_same_text() {
        let mut seat = base_seat();
        seat.h_delivered_mm = Some(0.0);
        let ev = evaluate_seat("/tmp/a.chitubox", 2, &seat);
        let f = &ev.failures[0];
        assert_eq!(
            f.to_json().get("message").and_then(Json::as_str),
            Some(&f.message[..])
        );
        assert!(f.message.starts_with("SEAT_DEPTH_ZERO file=/tmp/a.chitubox model=2 contact=7 record=7"));
    }

    #[test]
    fn ball_seat_factor_binding_matches_the_documented_switch_point() {
        // Sanity cross-check against the pinned constant this check reports
        // in `binding_term`; not a re-derivation, just wiring proof.
        let f = c::BALL_FACTOR_STUB;
        assert!((c::ball_seat_factor(f) - 0.381_966_011_250_105_1).abs() < 1e-15);
    }

    /// Runs the finished check against real emitted bytes and prints what it
    /// found, `cargo test --release --lib checks::seat_depth::tests::probe
    /// -- --nocapture`. Skips cleanly when `exports/` is absent, because run
    /// outputs are not in git (`tests/real_artifacts.rs` follows the same
    /// convention). Not an assertion of a specific corpus number -- this is
    /// the "run it and report what it found" step, not a regression pin.
    #[test]
    fn probe_against_a_real_emitted_artifact() {
        use crate::context::AuditContext;
        use crate::reader::ChituboxFile;
        use std::path::Path;

        let exports = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("exports");
        let Ok(entries) = std::fs::read_dir(&exports) else {
            eprintln!("skip: {} not present", exports.display());
            return;
        };
        let mut any = false;
        for e in entries.filter_map(Result::ok) {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("chitubox") {
                continue;
            }
            any = true;
            let file =
                ChituboxFile::open(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
            let options = crate::context::AuditOptions::default();
            let audit = AuditContext {
                file: &file,
                options: &options,
                sidecar: None,
                sidecar_absence: Some("none on disk".into()),
            };
            for mi in 0..file.models.len() {
                let ctx = ModelContext::build(&audit, mi);
                let outcome = SeatDepth.run(&ctx);
                eprintln!(
                    "{} model {mi}: {} -- {} checked, {} failed, {} indeterminate -- {}",
                    path.file_name().unwrap().to_string_lossy(),
                    outcome.status().as_str(),
                    outcome.checked,
                    outcome.failed,
                    outcome.indeterminate,
                    outcome.headline(),
                );
                for f in &outcome.failures {
                    eprintln!("    {}", f.message);
                }
            }
        }
        if !any {
            eprintln!("skip: no .chitubox files under {}", exports.display());
        }
    }
}
