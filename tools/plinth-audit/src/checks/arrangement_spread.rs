//! I10 arrangement spread -- `docs/specs/audit-spread-criterion.md`.
//!
//! ## What this closes
//!
//! Cross-family review Finding 2: every other gating invariant is per-contact
//! or per-arm; there was no arrangement check at all. `docs/physics.md` §2(c):
//! *"X ≤ 12 is necessary, not sufficient. The ratio is blind to arrangement.
//! Three collinear arms have no lateral stability perpendicular to their line,
//! at any X. An arrangement check has to include a spread criterion, not just
//! a per-arm ratio."*
//!
//! ## The predicate, as implemented
//!
//! > The peel resultant on a **prefix-connected cured model body** `B` at layer
//! > `k` must lie in the interior of the convex hull of `B`'s **resisting lines
//! > of action**, taken in the plane of layer `k`, by more than the derived
//! > uncertainty `sigma = sigma_q + sigma_H + d2/2`.
//!
//! `sigma` is not a threshold. It is propagated measurement error plus the
//! printer's own placement quantum, and **it is subtracted, never added**: the
//! pass condition is `d > +sigma`, never `d >= -sigma` (specification §3.5,
//! and `docs/specs/audit-interval-policy.md` §3.2). A margin inside the
//! uncertainty band is `indeterminate`, and `indeterminate` fails.
//!
//! **This invariant introduces no chosen constant.** `X_ALLOWED` is quoted by
//! §4 of the specification for cross-reference and is read nowhere here.
//!
//! ## The object graded
//!
//! A connected component of MODEL material cured at layers `0..=k`, in-plane
//! 4-connected (not 8 -- the more-gating direction, with the merge count
//! disclosed), vertically same-column between `k` and `k-1`. Support material
//! is excluded from a body; supports enter only as forces at their contacts.
//! This is deliberately NOT the down-facing surface component (which collapses
//! to "the whole model" on a watertight mesh and would watch nothing) and NOT
//! the support-side load-path cluster (the wrong side of the contact).
//!
//! ## Outcome table (specification §3.4), evaluated in this order
//!
//! | # | condition | outcome |
//! |---|---|---|
//! | 1 | no line of action at all | REFUSE `SPREAD_NO_SUPPORT` |
//! | – | a contact whose taper join or axis is unusable | INDET `taper_join_ambiguous` / `axis_horizontal` |
//! | 2 | `dim(aff) <= 1` and `dist(q, aff) > sigma` | REFUSE `SPREAD_COLLINEAR_NO_EQUILIBRIUM` |
//! | 3 | `dim(aff) <= 1` and `dist(q, aff) <= sigma` | INDET `rank_deficient_load_on_axis` |
//! | 4 | `d > sigma` | PASS |
//! | 5 | `d < -sigma`, no non-resisting contact | REFUSE `SPREAD_NO_EQUILIBRIUM` |
//! | 6 | `d < -sigma`, some non-resisting contact | INDET `non_resisting_contact` |
//! | 7 | `abs(d) <= sigma` | INDET `margin_within_tolerance` |
//!
//! Rows 2 and 3 read the affine hull of **all** lines of action (`Gall`);
//! row 4 onward read the hull of the **resisting** ones (`Gplus`), because a
//! feasible subset is all satisfiability needs. Plate contact (§2.5) enters
//! both sets as one vertical line of action per plate pixel, reduced to the
//! hull of those pixel centres.
//!
//! ## Deviations from the specification, stated rather than hidden
//!
//! 1. **§8 assigns the body sweep to a new `src/bodies.rs` and the `§5`
//!    disclosure block to `report_metrics.rs`.** This agent owns exactly one
//!    file, so the sweep lives here (private, so the integrator can lift it
//!    without a caller change) and the §5 quantities this sweep produces for
//!    free -- mode C's normalised margin, mode D's slenderness, the rank
//!    numbers, sensitivity (a) and (c) -- are reported in this check's own
//!    `measured` block. `report.spread.horizontal_residual`,
//!    `report.spread.release_sweep` and sensitivity (b) are **not** computed
//!    anywhere yet; `measured.disclosures_not_computed` names them so their
//!    absence is never read as a zero. They are non-gating disclosures, so
//!    this does not make anything pass that would otherwise fail.
//! 2. **§7 asks the implementing agent to ship four `fixtures/` directories.**
//!    Fixture files are outside this agent's one-file ownership. The four
//!    planted defects are built and asserted in this file's own test module
//!    instead, with the same geometry and the same expectations -- including
//!    `spread_supported`, the mandatory negative control, and
//!    `spread_tilted_axes`, whose test additionally asserts the
//!    line-of-action-versus-contact-point sensitivity count is non-zero, so
//!    the §3.2 projection is measurably exercised rather than claimed.
//! 3. **Failure LINES are one per `(body, code, reason)`, at that body's worst
//!    layer**, capped at [`MAX_FAILURE_LINES`]. `checked`, `failed` and
//!    `indeterminate` still count every body-layer evaluation, so the counts
//!    are exact and only the printed evidence is summarised. §3.8's "a body's
//!    own verdict is its worst layer" is what this follows.
//!
//! ## What this gate cannot see
//!
//! The specification's §6 list governs; the sharpest items are that the body
//! is assumed **rigid** (a thin wing or a cavity ceiling does not distribute
//! its load to distant contacts -- that is the span criterion's object, not
//! this one), that the criterion is **scale-free** and so cannot tell a
//! marginal arrangement carrying 0.1 mm² from the same arrangement carrying
//! 100 mm², that member **bending and seat moment capacity are neglected** (an
//! error toward false refusal), and that a lobe **stops being watched once it
//! merges** into its parent, however thin the neck it merged through. A
//! passing I10 means "no infeasibility was proven at the load plane", never
//! "this arrangement is stable".

use crate::check::{Check, CheckOutcome, Failure};
use crate::consts as c;
use crate::context::ModelContext;
use crate::json::Json;
use crate::raster::{Layer, RasterError, RasterGrid, Run, RunRaster};
use crate::vec3 as v;
use std::collections::BTreeMap;

pub struct ArrangementSpread;

// ---------------------------------------------------------------------------
// Codes and reasons -- specification §3.4 and §3.6
// ---------------------------------------------------------------------------

const CODE_NO_SUPPORT: &str = "SPREAD_NO_SUPPORT";
const CODE_NO_EQUILIBRIUM: &str = "SPREAD_NO_EQUILIBRIUM";
const CODE_COLLINEAR: &str = "SPREAD_COLLINEAR_NO_EQUILIBRIUM";
const CODE_INDETERMINATE: &str = "SPREAD_INDETERMINATE";

const R_RANK_DEFICIENT: &str = "rank_deficient_load_on_axis";
const R_NON_RESISTING: &str = "non_resisting_contact";
const R_MARGIN_IN_TOL: &str = "margin_within_tolerance";
const R_AXIS_HORIZONTAL: &str = "axis_horizontal";
const R_TAPER_AMBIGUOUS: &str = "taper_join_ambiguous";
const R_RASTER_FAILED: &str = "raster_failed";
const R_BUDGET_EXCEEDED: &str = "budget_exceeded";
const R_NO_BODIES: &str = "no_bodies_found";

/// How many contact ids one failure line prints before it says `+N`. A display
/// cap only: `contacts=` on the same line is the exact count.
const CONTACT_ID_LINE_CAP: usize = 24;
/// How many failure lines this check will print. Counts are never capped.
const MAX_FAILURE_LINES: usize = 2_000;
/// How many distinct `(body, code, reason)` keys are retained for printing.
/// Counts are never capped; only the evidence table is.
const MAX_PENDING_KEYS: usize = 20_000;

// ---------------------------------------------------------------------------
// The check
// ---------------------------------------------------------------------------

impl Check for ArrangementSpread {
    fn id(&self) -> &'static str {
        "i10_arrangement_spread"
    }

    fn title(&self) -> &'static str {
        "I10 arrangement spread"
    }

    /// The count of cured model layers -- an under-estimate of the real scope,
    /// which is BODY-LAYER evaluations (a strict multiple whenever more than
    /// one body coexists). Used only for the whole-model abort paths, where
    /// the real count was never reached.
    fn scope(&self, ctx: &ModelContext) -> usize {
        if let Ok(r) = ctx.model_raster() {
            return r.layers.len().max(1);
        }
        let e = ctx.model_mesh.bbox.extent()[2];
        ((e / ctx.grid.layer_height_mm).ceil() as usize).max(1)
    }

    fn run(&self, ctx: &ModelContext) -> CheckOutcome {
        let file = ctx.file();
        let scope = self.scope(ctx);
        let raster = match ctx.model_raster() {
            Ok(r) => r,
            Err(e) => return whole_model_indeterminate(&file, ctx.model_index, scope, e),
        };
        let contacts = build_contact_lines(ctx);
        let budget = ctx.audit.options.spread_budget.max_body_layer_evaluations;
        let sweep = sweep(raster, &contacts, budget);
        finish(ctx, &file, scope, &contacts, raster, sweep)
    }
}

/// The raster this invariant needs could not be built. Never a pass.
fn whole_model_indeterminate(
    file: &str,
    model: usize,
    scope: usize,
    e: &RasterError,
) -> CheckOutcome {
    let scope = scope.max(1);
    let mut values = Json::obj();
    values.set("reason", Json::str(R_RASTER_FAILED));
    values.set("detail", Json::str(e.to_string()));
    let failure = Failure::line(
        CODE_INDETERMINATE,
        file,
        model,
        None,
        None,
        &[
            ("body", "n/a".to_string()),
            ("layer", "n/a".to_string()),
            ("reason", R_RASTER_FAILED.to_string()),
            ("margin_mm", "n/a".to_string()),
            ("sigma_mm", "n/a".to_string()),
            ("contacts", "n/a".to_string()),
        ],
        values,
    );
    let mut measured = Json::obj();
    measured.set(
        "headline",
        Json::str(format!(
            "model raster unavailable ({e}); no arrangement could be established"
        )),
    );
    measured.set("reason", Json::str(R_RASTER_FAILED));
    CheckOutcome {
        checked: scope,
        failed: 0,
        indeterminate: scope,
        measured,
        failures: vec![failure],
    }
}

// ---------------------------------------------------------------------------
// 2-D primitives
// ---------------------------------------------------------------------------

type P2 = [f64; 2];

fn sub2(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}
fn norm2(a: P2) -> f64 {
    a[0].hypot(a[1])
}
fn dist2(a: P2, b: P2) -> f64 {
    norm2(sub2(a, b))
}

/// Distance from `q` to the closed segment `a..b`.
fn point_segment_distance(q: P2, a: P2, b: P2) -> f64 {
    let e = sub2(b, a);
    let l2 = e[0] * e[0] + e[1] * e[1];
    if l2 <= 0.0 {
        return dist2(q, a);
    }
    let t = (((q[0] - a[0]) * e[0] + (q[1] - a[1]) * e[1]) / l2).clamp(0.0, 1.0);
    dist2(q, [a[0] + t * e[0], a[1] + t * e[1]])
}

/// Convex hull, counter-clockwise, monotone chain. A collinear input returns
/// its two extreme points; a single repeated point returns one.
fn convex_hull(pts: &[P2]) -> Vec<P2> {
    let mut p: Vec<P2> = pts.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup_by(|a, b| a[0] == b[0] && a[1] == b[1]);
    if p.len() < 3 {
        return p;
    }
    let cross = |o: P2, a: P2, b: P2| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut lower: Vec<P2> = Vec::with_capacity(p.len());
    for &q in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], q) <= 0.0 {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<P2> = Vec::with_capacity(p.len());
    for &q in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], q) <= 0.0 {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Area of a CCW convex polygon, mm². Zero for a degenerate hull.
fn polygon_area(hull: &[P2]) -> f64 {
    if hull.len() < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..hull.len() {
        let a = hull[i];
        let b = hull[(i + 1) % hull.len()];
        s += a[0] * b[1] - b[0] * a[1];
    }
    (s * 0.5).abs()
}

/// Signed distance from `q` to the boundary of a CCW convex hull: **positive
/// inside**, negative outside, and the true Euclidean distance in both
/// directions. A hull with an empty interior (0, 1 or 2 distinct vertices, or
/// a collinear set) can never return a positive value -- which is correct:
/// there is no interior to be inside of.
fn convex_signed_margin(hull: &[P2], q: P2) -> f64 {
    match hull.len() {
        0 => f64::NEG_INFINITY,
        1 => -dist2(q, hull[0]),
        2 => -point_segment_distance(q, hull[0], hull[1]),
        n => {
            let mut min_signed = f64::INFINITY;
            let mut inside = true;
            for i in 0..n {
                let a = hull[i];
                let b = hull[(i + 1) % n];
                let e = sub2(b, a);
                let len = norm2(e);
                if len <= 0.0 {
                    continue;
                }
                let s = (e[0] * (q[1] - a[1]) - e[1] * (q[0] - a[0])) / len;
                if s < 0.0 {
                    inside = false;
                }
                min_signed = min_signed.min(s);
            }
            if inside {
                min_signed
            } else {
                let mut best = f64::INFINITY;
                for i in 0..n {
                    best = best.min(point_segment_distance(q, hull[i], hull[(i + 1) % n]));
                }
                -best
            }
        }
    }
}

/// The best-fit line of a point set and the spread about it.
#[derive(Clone, Copy, Debug, Default)]
struct LineFit {
    centroid: P2,
    /// Unit direction of the major principal axis.
    dir: P2,
    /// RMS perpendicular spread about the line, mm. `report.spread.rank`'s
    /// `s_min`: the number a future arrangement coupon would put a threshold on.
    s_min_mm: f64,
    /// Largest perpendicular deviation, mm. This -- not the RMS -- is what
    /// `dim(aff)` is resolved against, because one outlier off the line is
    /// what makes the arrangement two-dimensional.
    max_perp_mm: f64,
    /// Half-extent along the line, mm.
    extent_mm: f64,
    /// `sqrt(lambda_min / lambda_max)` of the second-moment matrix.
    anisotropy: f64,
}

fn fit_line(pts: &[P2]) -> LineFit {
    let n = pts.len();
    if n == 0 {
        return LineFit::default();
    }
    let mut cx = 0.0;
    let mut cy = 0.0;
    for p in pts {
        cx += p[0];
        cy += p[1];
    }
    let centroid = [cx / n as f64, cy / n as f64];
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for p in pts {
        let d = sub2(*p, centroid);
        sxx += d[0] * d[0];
        syy += d[1] * d[1];
        sxy += d[0] * d[1];
    }
    let inv = 1.0 / n as f64;
    sxx *= inv;
    syy *= inv;
    sxy *= inv;
    let tr = sxx + syy;
    let det_disc = ((sxx - syy) * (sxx - syy) + 4.0 * sxy * sxy).max(0.0).sqrt();
    let lam_max = 0.5 * (tr + det_disc);
    let lam_min = (0.5 * (tr - det_disc)).max(0.0);
    // Eigenvector for lam_max; take the better-conditioned of the two forms.
    let e1 = [sxy, lam_max - sxx];
    let e2 = [lam_max - syy, sxy];
    let raw = if norm2(e1) >= norm2(e2) { e1 } else { e2 };
    let dir = if norm2(raw) > 0.0 {
        let l = norm2(raw);
        [raw[0] / l, raw[1] / l]
    } else {
        [1.0, 0.0]
    };
    let nrm = [-dir[1], dir[0]];
    let mut max_perp = 0.0f64;
    let mut extent = 0.0f64;
    for p in pts {
        let d = sub2(*p, centroid);
        max_perp = max_perp.max((d[0] * nrm[0] + d[1] * nrm[1]).abs());
        extent = extent.max((d[0] * dir[0] + d[1] * dir[1]).abs());
    }
    LineFit {
        centroid,
        dir,
        s_min_mm: lam_min.sqrt(),
        max_perp_mm: max_perp,
        extent_mm: extent,
        anisotropy: if lam_max > 0.0 {
            (lam_min / lam_max).sqrt()
        } else {
            0.0
        },
    }
}

// ---------------------------------------------------------------------------
// The predicate
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    Pass,
    /// Proven: no assignment of member tensions balances the peel resultant.
    Refuse(&'static str),
    /// Statics could not decide, or an input was unusable. Fails.
    Indeterminate(&'static str),
}

impl Verdict {
    const fn code(self) -> &'static str {
        match self {
            Self::Pass => "",
            Self::Refuse(code) => code,
            Self::Indeterminate(_) => CODE_INDETERMINATE,
        }
    }
    const fn reason(self) -> &'static str {
        match self {
            Self::Indeterminate(r) => r,
            _ => "",
        }
    }
}

/// The numbers one body-layer evaluation produces, whatever its verdict.
#[derive(Clone, Copy, Debug, Default)]
struct Metrics {
    /// Signed containment margin against the RESISTING hull, mm. Positive
    /// inside. `NEG_INFINITY` when the resisting set is empty.
    d_mm: f64,
    /// Dimension of the affine hull of ALL lines of action, resolved at sigma.
    aff_dim: u8,
    /// Distance from the load point to that affine hull, mm.
    dist_aff_mm: f64,
    hull_vertices: usize,
    hull_area_mm2: f64,
    /// `d / sqrt(hull_area)` -- mode C, dimensionless, reported never gated.
    m_hat: f64,
    fit: LineFit,
}

/// The one place the outcome table of §3.4 lives.
///
/// `g_plus` are the resisting lines of action at the layer plane, `g_all` all
/// of them; both already include one vertical line per plate-contact hull
/// vertex. `sigma` is the derived uncertainty of §3.5. No constant is read.
fn classify(
    q: P2,
    g_plus: &[P2],
    g_all: &[P2],
    has_non_resisting: bool,
    sigma: f64,
) -> (Verdict, Metrics) {
    let mut m = Metrics {
        d_mm: f64::NEG_INFINITY,
        dist_aff_mm: f64::NAN,
        // NOT zero. `m_hat` is mode C's statistic and a default of 0.0 would
        // enter it as a real measurement of "exactly on the boundary".
        m_hat: f64::NAN,
        ..Metrics::default()
    };

    // Row 1: nothing holds this body at all.
    if g_all.is_empty() {
        return (Verdict::Refuse(CODE_NO_SUPPORT), m);
    }

    // The affine hull of every line of action, resolved at sigma.
    let fit = fit_line(g_all);
    m.fit = fit;
    let (aff_dim, dist_aff) = if g_all.len() == 1 || fit.extent_mm <= sigma {
        // A point: every line of action lands in one pixel-sized spot.
        (0u8, dist2(q, fit.centroid))
    } else if fit.max_perp_mm <= sigma {
        // A line: perpendicular distance from the load point to it.
        let d = sub2(q, fit.centroid);
        (1u8, (d[0] * -fit.dir[1] + d[1] * fit.dir[0]).abs())
    } else {
        (2u8, f64::NAN)
    };
    m.aff_dim = aff_dim;
    m.dist_aff_mm = dist_aff;

    // Rows 2 and 3: rank deficiency. Refusable even with non-resisting
    // members present, because the reachable set of load points is the affine
    // hull WHATEVER the signs of the tensions.
    if aff_dim <= 1 {
        let hull = convex_hull(g_plus);
        m.hull_vertices = hull.len();
        m.hull_area_mm2 = polygon_area(&hull);
        m.d_mm = convex_signed_margin(&hull, q);
        return if dist_aff > sigma {
            (Verdict::Refuse(CODE_COLLINEAR), m)
        } else {
            (Verdict::Indeterminate(R_RANK_DEFICIENT), m)
        };
    }

    // Rows 4 to 7: containment in the RESISTING hull. A solution that sets
    // every non-resisting tension to zero is still a solution, so the pass
    // test reads `Hplus` and not `conv(Gall)`.
    let hull = convex_hull(g_plus);
    m.hull_vertices = hull.len();
    m.hull_area_mm2 = polygon_area(&hull);
    let d = convex_signed_margin(&hull, q);
    m.d_mm = d;
    if m.hull_area_mm2 > 0.0 && d.is_finite() {
        m.m_hat = d / m.hull_area_mm2.sqrt();
    }

    // **Deliberately outside `audit-interval-policy.md`'s sweep**, and
    // already its §3.1 triple: `docs/specs/audit-spread-criterion.md` §3.5
    // derives this polarity from the same review finding. `d` is the
    // delivered containment margin, the requirement is zero, and the
    // conservative bound of `d` is its lower bound `d - sigma`, so `d >
    // sigma` IS the minimum-type pass condition. A margin inside the band is
    // indeterminate, and indeterminate fails. Nothing to correct.
    if d > sigma {
        (Verdict::Pass, m)
    } else if d < -sigma {
        if has_non_resisting {
            (Verdict::Indeterminate(R_NON_RESISTING), m)
        } else {
            (Verdict::Refuse(CODE_NO_EQUILIBRIUM), m)
        }
    } else {
        (Verdict::Indeterminate(R_MARGIN_IN_TOL), m)
    }
}

// ---------------------------------------------------------------------------
// Contacts: lines of action, all RECONSTRUCTED
// ---------------------------------------------------------------------------

/// One contact's line of action, or the reason it has none.
///
/// Nothing here is DECLARED. There is no field in a `.chitubox` file that
/// states an arrangement, so I10 cannot catch a lying field -- there is no
/// field to lie (specification §3.3). The tip ball radius `r_mm` IS declared,
/// and it is used only to decide which layers the contact touches; the seat
/// reconstruction already cross-checks it three ways.
#[derive(Clone, Copy, Debug)]
struct ContactLine {
    /// The record index of the type-7 TIP record.
    id: usize,
    /// `midpoint(start, end)` of the TIP record: the ball centre.
    x: [f64; 3],
    r_mm: f64,
    /// Horizontal part of the rootward unit axis.
    h: P2,
    /// `-u_z`: positive for a resisting member.
    vz: f64,
    /// `2 * tol_geom(node, x) / L` -- the angular uncertainty of the axis, rad.
    delta_rad: f64,
    /// `tol_geom` at the ball centre, mm.
    tol_x_mm: f64,
    /// Set when the line of action could not be reconstructed at all.
    bad: Option<&'static str>,
}

impl ContactLine {
    const fn is_resisting(&self) -> bool {
        self.vz > 0.0
    }
    /// `g_c(k)` -- where this member's line of action crosses the plane `z_k`.
    fn g(&self, z_k: f64) -> P2 {
        let s = (self.x[2] - z_k) / self.vz;
        [self.x[0] + s * self.h[0], self.x[1] + s * self.h[1]]
    }
    /// `sigma_g(c)` -- this member's contribution to `sigma_H`.
    fn sigma_g(&self, z_k: f64) -> f64 {
        let s = ((self.x[2] - z_k) / self.vz).abs();
        self.tol_x_mm + s * self.delta_rad
    }
}

/// A vertical plate line of action, whose `g` is its own pixel centre at every
/// plane and whose axis is exactly `-z`.
fn plate_line(p: P2, z: f64) -> ContactLine {
    ContactLine {
        id: usize::MAX,
        x: [p[0], p[1], z],
        r_mm: 0.0,
        h: [0.0, 0.0],
        vz: 1.0,
        delta_rad: 0.0,
        tol_x_mm: c::tol_geom_at([p[0], p[1], z]),
        bad: None,
    }
}

/// Reconstruct every contact's line of action from the TIP record and the
/// type-1 TAPER joined to it (`audit-contract.md` §1.3 and §3.2).
fn build_contact_lines(ctx: &ModelContext) -> Vec<ContactLine> {
    let mut out = Vec::with_capacity(ctx.seats.len());
    for seat in &ctx.seats {
        let x = seat.centre;
        let tol_x = c::tol_geom_at(x);
        let mut cl = ContactLine {
            id: seat.contact_id,
            x,
            r_mm: seat.r_ball_mm,
            h: [0.0, 0.0],
            vz: 0.0,
            delta_rad: 0.0,
            tol_x_mm: tol_x,
            bad: Some(R_TAPER_AMBIGUOUS),
        };
        let Some(t) = seat.taper.unique() else {
            out.push(cl);
            continue;
        };
        let rec = &ctx.model.records[t];
        // The far endpoint of the taper from the ball centre is the first
        // structural node; the member runs from the ball centre to it.
        let node = if v::dist2(rec.start_mm, x) >= v::dist2(rec.end_mm, x) {
            rec.start_mm
        } else {
            rec.end_mm
        };
        let Some(u) = v::normalize(v::sub(node, x)) else {
            out.push(cl);
            continue;
        };
        let l = v::dist(node, x);
        if !(l.is_finite() && l > 0.0) {
            out.push(cl);
            continue;
        }
        let delta = 2.0 * c::tol_geom(node, x) / l;
        cl.h = [u[0], u[1]];
        cl.vz = -u[2];
        cl.delta_rad = delta;
        // `v_c = 0` is a horizontal member: its line of action never meets the
        // layer plane. The band is the axis's own angular uncertainty, so the
        // test is "the sign of v_c is not established", not a chosen epsilon.
        cl.bad = if cl.vz.abs() <= delta {
            Some(R_AXIS_HORIZONTAL)
        } else {
            None
        };
        out.push(cl);
    }
    out
}

// ---------------------------------------------------------------------------
// The prefix-connected body sweep
// ---------------------------------------------------------------------------

struct Uf {
    p: Vec<usize>,
}

impl Uf {
    const fn new() -> Self {
        Self { p: Vec::new() }
    }
    fn push(&mut self) -> usize {
        let i = self.p.len();
        self.p.push(i);
        i
    }
    fn find(&mut self, mut x: usize) -> usize {
        while self.p[x] != x {
            self.p[x] = self.p[self.p[x]];
            x = self.p[x];
        }
        x
    }
    /// Union with the **smaller** id winning, so a body's id is the index of
    /// its oldest layer-component and only ever decreases.
    fn union(&mut self, a: usize, b: usize) -> (usize, Option<usize>) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return (ra, None);
        }
        let (keep, drop) = if ra < rb { (ra, rb) } else { (rb, ra) };
        self.p[drop] = keep;
        (keep, Some(drop))
    }
}

/// One layer's runs, sorted by `(row, c0)`, with the row group boundaries.
struct LayerRuns {
    runs: Vec<(i64, Run)>,
    groups: Vec<(i64, usize, usize)>,
}

fn layer_runs(layer: &Layer) -> LayerRuns {
    let mut runs: Vec<(i64, Run)> = Vec::new();
    let mut groups: Vec<(i64, usize, usize)> = Vec::new();
    for (&row, rs) in &layer.rows {
        let start = runs.len();
        for r in rs {
            if !r.is_empty() {
                runs.push((row, *r));
            }
        }
        if runs.len() > start {
            groups.push((row, start, runs.len()));
        }
    }
    LayerRuns { runs, groups }
}

/// In-plane connected components of one layer's runs. `eight` switches
/// 4-connectivity for 8-connectivity, which is the sensitivity (a) probe and
/// never the gate. Linear in the run count: a two-pointer merge between
/// adjacent rows, not the quadratic all-pairs scan.
fn inplane_labels(lr: &LayerRuns, eight: bool) -> Vec<usize> {
    let n = lr.runs.len();
    let mut uf = Uf::new();
    for _ in 0..n {
        uf.push();
    }
    let pad = i64::from(eight);
    for w in lr.groups.windows(2) {
        let (r0, s0, e0) = w[0];
        let (r1, s1, e1) = w[1];
        if r1 - r0 != 1 {
            continue;
        }
        let (mut i, mut j) = (s0, s1);
        while i < e0 && j < e1 {
            let a = lr.runs[i].1;
            let b = lr.runs[j].1;
            if a.c0 - pad < b.c1 && b.c0 - pad < a.c1 {
                uf.union(i, j);
            }
            if a.c1 < b.c1 {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    (0..n).map(|i| uf.find(i)).collect()
}

/// Every `(cur index, prev index)` pair whose runs share a column in the same
/// row -- the vertical adjacency of §2.2, identical to I7's own attachment
/// definition. Linear, two-pointer, never per-pixel.
fn for_each_vertical_overlap(cur: &LayerRuns, prev: &LayerRuns, mut f: impl FnMut(usize, usize)) {
    let (mut gi, mut gj) = (0usize, 0usize);
    while gi < cur.groups.len() && gj < prev.groups.len() {
        let (rc, cs, ce) = cur.groups[gi];
        let (rp, ps, pe) = prev.groups[gj];
        if rc < rp {
            gi += 1;
            continue;
        }
        if rp < rc {
            gj += 1;
            continue;
        }
        let (mut i, mut j) = (cs, ps);
        while i < ce && j < pe {
            let a = cur.runs[i].1;
            let b = prev.runs[j].1;
            if a.c0 < b.c1 && b.c0 < a.c1 {
                f(i, j);
            }
            if a.c1 < b.c1 {
                i += 1;
            } else {
                j += 1;
            }
        }
        gi += 1;
        gj += 1;
    }
}

/// Body count only, at the requested in-plane connectivity. This is the
/// sensitivity (a) instrument of §5: the count of bodies that would merge
/// under 8-connectivity is the disclosed cost of choosing 4.
fn count_bodies(raster: &RunRaster, eight: bool) -> usize {
    let mut uf = Uf::new();
    let mut prev: Option<(LayerRuns, Vec<usize>)> = None;
    let mut prev_k = i64::MIN;
    for layer in &raster.layers {
        let lr = layer_runs(layer);
        if lr.runs.is_empty() {
            prev = None;
            continue;
        }
        let labels = inplane_labels(&lr, eight);
        let mut slot: Vec<usize> = vec![usize::MAX; lr.runs.len()];
        let mut comp_slot: BTreeMap<usize, usize> = BTreeMap::new();
        if let Some((p_lr, p_slot)) = prev.as_ref() {
            if layer.k == prev_k + 1 {
                for_each_vertical_overlap(&lr, p_lr, |i, j| {
                    let root = labels[i];
                    let below = p_slot[j];
                    match comp_slot.get(&root).copied() {
                        None => {
                            comp_slot.insert(root, below);
                        }
                        Some(existing) => {
                            let (keep, _) = uf.union(existing, below);
                            comp_slot.insert(root, keep);
                        }
                    }
                });
            }
        }
        for i in 0..lr.runs.len() {
            let root = labels[i];
            let s = *comp_slot.entry(root).or_insert_with(|| uf.push());
            slot[i] = uf.find(s);
        }
        prev_k = layer.k;
        prev = Some((lr, slot));
    }
    let n = uf.p.len();
    let mut roots: Vec<usize> = (0..n).map(|i| uf.find(i)).collect();
    roots.sort_unstable();
    roots.dedup();
    roots.len()
}

// ---------------------------------------------------------------------------
// Per-body bookkeeping
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct BodyState {
    birth_layer: i64,
    last_layer: i64,
    evals: u64,
    passed: u64,
    refused: u64,
    indeterminate: u64,
    max_contacts: usize,
    /// Hull of the plate pixel centres this body cures on, if any.
    plate_hull: Vec<P2>,
    /// `(contact index into the contact table, first layer it holds this body)`
    contacts: Vec<(usize, i64)>,
    worst_margin_mm: f64,
    worst_m_hat: f64,
    worst_layer: i64,
    s_min_at_worst_mm: f64,
    anisotropy_at_worst: f64,
    max_slenderness: f64,
}

impl BodyState {
    fn new(k: i64) -> Self {
        Self {
            birth_layer: k,
            last_layer: k,
            evals: 0,
            passed: 0,
            refused: 0,
            indeterminate: 0,
            max_contacts: 0,
            plate_hull: Vec::new(),
            contacts: Vec::new(),
            worst_margin_mm: f64::INFINITY,
            worst_m_hat: f64::INFINITY,
            worst_layer: k,
            s_min_at_worst_mm: f64::NAN,
            anisotropy_at_worst: f64::NAN,
            max_slenderness: 0.0,
        }
    }
    fn absorb(&mut self, other: &Self) {
        self.birth_layer = self.birth_layer.min(other.birth_layer);
        self.last_layer = self.last_layer.max(other.last_layer);
        self.evals += other.evals;
        self.passed += other.passed;
        self.refused += other.refused;
        self.indeterminate += other.indeterminate;
        self.max_contacts = self.max_contacts.max(other.max_contacts);
        for (cidx, k) in &other.contacts {
            match self.contacts.iter_mut().find(|(a, _)| a == cidx) {
                Some(slot) => slot.1 = slot.1.min(*k),
                None => self.contacts.push((*cidx, *k)),
            }
        }
        if !other.plate_hull.is_empty() {
            let mut pts = std::mem::take(&mut self.plate_hull);
            pts.extend_from_slice(&other.plate_hull);
            self.plate_hull = convex_hull(&pts);
        }
        if other.worst_margin_mm < self.worst_margin_mm {
            self.worst_margin_mm = other.worst_margin_mm;
            self.worst_layer = other.worst_layer;
            self.s_min_at_worst_mm = other.s_min_at_worst_mm;
            self.anisotropy_at_worst = other.anisotropy_at_worst;
        }
        self.worst_m_hat = self.worst_m_hat.min(other.worst_m_hat);
        self.max_slenderness = self.max_slenderness.max(other.max_slenderness);
    }
}

/// The worst evaluation seen for one `(body, code, reason)` key.
#[derive(Clone, Debug)]
struct Pending {
    severity: f64,
    fields: Vec<(&'static str, String)>,
    values: Json,
}

#[derive(Default)]
struct Counts {
    checked: u64,
    failed: u64,
    indeterminate: u64,
    passed: u64,
}

struct Sweep {
    counts: Counts,
    bodies: Vec<BodyState>,
    uf: Uf,
    pending: BTreeMap<(usize, &'static str, &'static str), Pending>,
    pending_dropped: u64,
    by_code: BTreeMap<&'static str, u64>,
    /// Evaluations whose verdict differs when the support polygon is built
    /// from CONTACT POINTS instead of LINES OF ACTION. Sensitivity (c), §5:
    /// the witness that the §3.2 projection is doing work.
    sensitivity_line_of_action: u64,
    layers_with_material: u64,
    budget_exceeded: bool,
    contacts_attached: usize,
}

/// One upward sweep over the layer stack. Everything is run-based; there is no
/// per-pixel loop and no per-(contact, layer) mesh query.
#[allow(clippy::too_many_lines)]
fn sweep(raster: &RunRaster, contacts: &[ContactLine], budget: u64) -> Sweep {
    let grid = raster.grid;
    let pixel_area = grid.pixel_area_mm2();
    // Plate layers: §2.5, "any cell at or below z_plate + layer_height".
    let k_plate_max = grid.layer_at_or_below(grid.z_plate_mm + grid.layer_height_mm);

    let mut s = Sweep {
        counts: Counts::default(),
        bodies: Vec::new(),
        uf: Uf::new(),
        pending: BTreeMap::new(),
        pending_dropped: 0,
        by_code: BTreeMap::new(),
        sensitivity_line_of_action: 0,
        layers_with_material: 0,
        budget_exceeded: false,
        contacts_attached: 0,
    };

    // Which layers each contact's rasterised ball can touch.
    let mut active: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for (ci, cl) in contacts.iter().enumerate() {
        if !(cl.r_mm.is_finite() && cl.r_mm > 0.0) || !v::is_finite(cl.x) {
            continue;
        }
        let k_lo = grid.layer_at_or_above(cl.x[2] - cl.r_mm);
        let k_hi = grid.layer_at_or_below(cl.x[2] + cl.r_mm);
        for k in k_lo..=k_hi {
            active.entry(k).or_default().push(ci);
        }
    }
    // Which body slots each contact has already been attached to.
    let mut contact_slots: Vec<Vec<usize>> = vec![Vec::new(); contacts.len()];

    let mut prev: Option<(LayerRuns, Vec<usize>)> = None;
    let mut prev_k = i64::MIN;

    for layer in &raster.layers {
        let lr = layer_runs(layer);
        if lr.runs.is_empty() {
            prev = None;
            continue;
        }
        s.layers_with_material += 1;
        let z_k = grid.layer_z_mm(layer.k);
        let labels = inplane_labels(&lr, false);

        // Attach this layer's in-plane components to the bodies below.
        let mut comp_slot: BTreeMap<usize, usize> = BTreeMap::new();
        if let Some((p_lr, p_slot)) = prev.as_ref() {
            if layer.k == prev_k + 1 {
                let mut merges: Vec<(usize, usize)> = Vec::new();
                for_each_vertical_overlap(&lr, p_lr, |i, j| {
                    merges.push((labels[i], p_slot[j]));
                });
                for (root, below) in merges {
                    match comp_slot.get(&root).copied() {
                        None => {
                            comp_slot.insert(root, below);
                        }
                        Some(existing) => {
                            let (keep, dropped) = s.uf.union(existing, below);
                            if let Some(d) = dropped {
                                let other = s.bodies[d].clone();
                                s.bodies[keep].absorb(&other);
                                s.bodies[d] = BodyState::new(layer.k);
                            }
                            comp_slot.insert(root, keep);
                        }
                    }
                }
            }
        }

        // Per-run body slot, and the per-body footprint at this layer.
        let mut slot_of_run: Vec<usize> = vec![usize::MAX; lr.runs.len()];
        let mut footprints: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for i in 0..lr.runs.len() {
            let root = labels[i];
            let raw = *comp_slot.entry(root).or_insert_with(|| {
                let slot = s.uf.push();
                s.bodies.push(BodyState::new(layer.k));
                slot
            });
            let body = s.uf.find(raw);
            slot_of_run[i] = body;
            footprints.entry(body).or_default().push(i);
        }

        // Plate contact: one vertical line of action per plate pixel, reduced
        // to the hull of those pixel centres.
        if layer.k <= k_plate_max {
            for (body, idxs) in &footprints {
                let mut pts: Vec<P2> = std::mem::take(&mut s.bodies[*body].plate_hull);
                for &i in idxs {
                    let (row, r) = lr.runs[i];
                    let y = (row as f64 + 0.5) * grid.pitch_y_mm;
                    pts.push([(r.c0 as f64 + 0.5) * grid.pitch_x_mm, y]);
                    pts.push([(r.c1 as f64 - 0.5) * grid.pitch_x_mm, y]);
                }
                s.bodies[*body].plate_hull = convex_hull(&pts);
            }
        }

        // Attach contacts whose rasterised ball shares a cell with a body.
        if let Some(list) = active.get(&layer.k) {
            let row_index: BTreeMap<i64, (usize, usize)> = lr
                .groups
                .iter()
                .map(|&(row, a, b)| (row, (a, b)))
                .collect();
            for &ci in list {
                let cl = &contacts[ci];
                let dz = z_k - cl.x[2];
                let rr = cl.r_mm * cl.r_mm - dz * dz;
                if rr <= 0.0 {
                    continue;
                }
                let rad = rr.sqrt();
                let j_lo = ((cl.x[1] - rad) / grid.pitch_y_mm - 0.5).ceil() as i64;
                let j_hi = ((cl.x[1] + rad) / grid.pitch_y_mm - 0.5).floor() as i64;
                for j in j_lo..=j_hi {
                    let Some(&(a, b)) = row_index.get(&j) else {
                        continue;
                    };
                    let dy = (j as f64 + 0.5) * grid.pitch_y_mm - cl.x[1];
                    let w2 = rad * rad - dy * dy;
                    if w2 <= 0.0 {
                        continue;
                    }
                    let w = w2.sqrt();
                    let c_lo = ((cl.x[0] - w) / grid.pitch_x_mm - 0.5).ceil() as i64;
                    let c_hi = ((cl.x[0] + w) / grid.pitch_x_mm - 0.5).floor() as i64;
                    if c_hi < c_lo {
                        continue;
                    }
                    for i in a..b {
                        let r = lr.runs[i].1;
                        if c_lo < r.c1 && r.c0 <= c_hi {
                            let body = slot_of_run[i];
                            let seen = contact_slots[ci]
                                .iter()
                                .any(|&sl| s.uf.find(sl) == s.uf.find(body));
                            if !seen {
                                contact_slots[ci].push(body);
                                s.bodies[body].contacts.push((ci, layer.k));
                                s.contacts_attached += 1;
                            }
                        }
                    }
                }
            }
        }

        // Evaluate every body that has a footprint at this layer.
        for (body, idxs) in &footprints {
            s.counts.checked += 1;
            if s.counts.checked > budget {
                s.budget_exceeded = true;
                return s;
            }
            let runs: Vec<(i64, Run)> = idxs.iter().map(|&i| lr.runs[i]).collect();
            evaluate_body_layer(&mut s, *body, layer.k, z_k, &runs, contacts, grid, pixel_area);
        }

        prev_k = layer.k;
        prev = Some((lr, slot_of_run));
    }
    s
}

/// Footprint statistics of one body at one layer, all from its runs.
struct Footprint {
    area_mm2: f64,
    centroid: P2,
    perimeter_mm: f64,
    rho_mm: f64,
    pixels: i64,
}

fn footprint_of(runs: &[(i64, Run)], grid: &RasterGrid) -> Footprint {
    let mut pixels = 0i64;
    let (mut sx, mut sy) = (0.0f64, 0.0f64);
    let (mut x0, mut x1) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY);
    // Vertical boundary: both ends of every run are exposed, because
    // `normalise` has already merged any two touching runs in a row.
    let mut perim = 0.0f64;
    let mut by_row: BTreeMap<i64, Vec<Run>> = BTreeMap::new();
    for (row, r) in runs {
        let cnt = r.len().max(0);
        if cnt <= 0 {
            continue;
        }
        pixels += cnt;
        let x_avg = (r.c0 as f64 + r.c1 as f64) * 0.5 * grid.pitch_x_mm;
        let y = (*row as f64 + 0.5) * grid.pitch_y_mm;
        sx += x_avg * cnt as f64;
        sy += y * cnt as f64;
        x0 = x0.min(r.c0 as f64 * grid.pitch_x_mm);
        x1 = x1.max(r.c1 as f64 * grid.pitch_x_mm);
        y0 = y0.min(*row as f64 * grid.pitch_y_mm);
        y1 = y1.max((*row + 1) as f64 * grid.pitch_y_mm);
        perim += 2.0 * grid.pitch_y_mm;
        by_row.entry(*row).or_default().push(*r);
    }
    // Horizontal boundary: the part of each row not covered by the row above
    // and the part not covered by the row below.
    for (row, rs) in &by_row {
        let len: i64 = rs.iter().map(|r| r.len().max(0)).sum();
        for nb in [row - 1, row + 1] {
            let cov = by_row
                .get(&nb)
                .map_or(0i64, |other| overlap_len(rs, other));
            perim += (len - cov).max(0) as f64 * grid.pitch_x_mm;
        }
    }
    let centroid = if pixels > 0 {
        [sx / pixels as f64, sy / pixels as f64]
    } else {
        [0.0, 0.0]
    };
    let mut rho = 0.0f64;
    if pixels > 0 {
        for cx in [x0, x1] {
            for cy in [y0, y1] {
                rho = rho.max(dist2(centroid, [cx, cy]));
            }
        }
    }
    Footprint {
        area_mm2: pixels as f64 * grid.pixel_area_mm2(),
        centroid,
        perimeter_mm: perim,
        rho_mm: rho,
        pixels,
    }
}

/// Total column overlap between two sorted disjoint run lists.
fn overlap_len(a: &[Run], b: &[Run]) -> i64 {
    let (mut i, mut j, mut total) = (0usize, 0usize, 0i64);
    while i < a.len() && j < b.len() {
        let lo = a[i].c0.max(b[j].c0);
        let hi = a[i].c1.min(b[j].c1);
        if hi > lo {
            total += hi - lo;
        }
        if a[i].c1 < b[j].c1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    total
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn evaluate_body_layer(
    s: &mut Sweep,
    body: usize,
    k: i64,
    z_k: f64,
    runs: &[(i64, Run)],
    contacts: &[ContactLine],
    grid: RasterGrid,
    _pixel_area: f64,
) {
    let fp = footprint_of(runs, &grid);
    let q = fp.centroid;

    // sigma_q -- the load point. The audit's fill rule cures a pixel only when
    // the solid contains its whole footprint, so the raster footprint is
    // eroded by a band of width at most one in-plane pixel diagonal.
    let r_band = fp.perimeter_mm * c::D2_MM;
    let sigma_q = if fp.area_mm2 + r_band > 0.0 {
        (r_band / (fp.area_mm2 + r_band)) * fp.rho_mm
    } else {
        0.0
    };

    // The lines of action holding this body at this layer.
    let held: Vec<(usize, i64)> = s.bodies[body].contacts.clone();
    let mut g_plus: Vec<P2> = Vec::new();
    let mut g_all: Vec<P2> = Vec::new();
    let mut cp_plus: Vec<P2> = Vec::new();
    let mut cp_all: Vec<P2> = Vec::new();
    let mut ids: Vec<usize> = Vec::new();
    let mut sigma_h = 0.0f64;
    let mut has_non_resisting = false;
    let mut unusable: Option<&'static str> = None;
    let mut n_contacts = 0usize;
    let mut n_resisting = 0usize;
    let mut mean_contact_z = 0.0f64;

    for (ci, k_c) in &held {
        if *k_c > k {
            continue;
        }
        let cl = &contacts[*ci];
        n_contacts += 1;
        ids.push(cl.id);
        mean_contact_z += cl.x[2];
        if let Some(bad) = cl.bad {
            // "Never a skip": an unusable line of action makes the whole
            // body-layer evaluation indeterminate.
            unusable = Some(unusable.map_or(bad, |prev| prev));
            continue;
        }
        let g = cl.g(z_k);
        let cp = [cl.x[0], cl.x[1]];
        sigma_h = sigma_h.max(cl.sigma_g(z_k));
        g_all.push(g);
        cp_all.push(cp);
        if cl.is_resisting() {
            n_resisting += 1;
            g_plus.push(g);
            cp_plus.push(cp);
        } else {
            has_non_resisting = true;
        }
    }
    if n_contacts > 0 {
        mean_contact_z /= n_contacts as f64;
    }

    // Plate lines of action (§2.5): vertical, bilateral, resisting.
    let plate_hull = s.bodies[body].plate_hull.clone();
    for p in &plate_hull {
        let pl = plate_line(*p, grid.z_plate_mm);
        sigma_h = sigma_h.max(pl.tol_x_mm);
        g_all.push(*p);
        g_plus.push(*p);
        cp_all.push(*p);
        cp_plus.push(*p);
    }

    let sigma = sigma_q + sigma_h + c::D2_MM * 0.5;

    let (verdict, m) = if let Some(reason) = unusable {
        (
            Verdict::Indeterminate(reason),
            Metrics {
                d_mm: f64::NAN,
                dist_aff_mm: f64::NAN,
                m_hat: f64::NAN,
                ..Metrics::default()
            },
        )
    } else {
        classify(q, &g_plus, &g_all, has_non_resisting, sigma)
    };

    // Sensitivity (c): would a CONTACT-POINT hull have returned a different
    // verdict? A count, not a claim (§7).
    if unusable.is_none() {
        let (cp_verdict, _) = classify(q, &cp_plus, &cp_all, has_non_resisting, sigma);
        if cp_verdict != verdict {
            s.sensitivity_line_of_action += 1;
        }
    }

    // Counts, and the per-body record.
    let st = &mut s.bodies[body];
    st.evals += 1;
    st.last_layer = k;
    st.max_contacts = st.max_contacts.max(n_contacts);
    if m.d_mm.is_finite() && m.d_mm < st.worst_margin_mm {
        st.worst_margin_mm = m.d_mm;
        st.worst_layer = k;
        st.s_min_at_worst_mm = m.fit.s_min_mm;
        st.anisotropy_at_worst = m.fit.anisotropy;
    }
    if m.m_hat.is_finite() && m.m_hat < st.worst_m_hat {
        st.worst_m_hat = m.m_hat;
    }
    if n_contacts > 0 && m.fit.s_min_mm > 0.0 {
        st.max_slenderness = st.max_slenderness.max((z_k - mean_contact_z) / m.fit.s_min_mm);
    }
    match verdict {
        Verdict::Pass => {
            st.passed += 1;
            s.counts.passed += 1;
            return;
        }
        Verdict::Refuse(_) => {
            st.refused += 1;
            s.counts.failed += 1;
        }
        Verdict::Indeterminate(_) => {
            st.indeterminate += 1;
            s.counts.indeterminate += 1;
        }
    }

    let code = verdict.code();
    let reason = verdict.reason();
    *s.by_code.entry(if reason.is_empty() { code } else { reason }).or_default() += 1;

    // One printed line per (body, code, reason), at that body's worst layer.
    let severity = match verdict {
        Verdict::Refuse(CODE_NO_SUPPORT) => fp.area_mm2,
        Verdict::Refuse(CODE_COLLINEAR) => m.dist_aff_mm,
        _ => {
            if m.d_mm.is_finite() {
                -m.d_mm
            } else {
                0.0
            }
        }
    };
    let key = (body, code, reason);
    let better = s.pending.get(&key).is_none_or(|p| severity > p.severity);
    if !better {
        return;
    }
    if s.pending.len() >= MAX_PENDING_KEYS && !s.pending.contains_key(&key) {
        s.pending_dropped += 1;
        return;
    }

    let ids_txt = format_ids(&ids);
    let mut values = Json::obj();
    values.set("body", Json::usize(body));
    values.set("layer", Json::int(k));
    values.set("z_mm", Json::num(z_k));
    values.set("footprint_mm2", Json::num(fp.area_mm2));
    values.set("footprint_pixels", Json::int(fp.pixels));
    values.set("centroid_mm", Json::nums([q[0], q[1]]));
    values.set("margin_mm", jnum(m.d_mm));
    values.set("sigma_mm", jnum(sigma));
    values.set("sigma_q_mm", jnum(sigma_q));
    values.set("sigma_h_mm", jnum(sigma_h));
    values.set("sigma_quantum_mm", Json::num(c::D2_MM * 0.5));
    values.set("sigma_q_form", Json::str("raster_erosion_bound"));
    values.set("hull_vertices", Json::usize(m.hull_vertices));
    values.set("hull_area_mm2", jnum(m.hull_area_mm2));
    values.set("aff_dim", Json::usize(m.aff_dim as usize));
    // `offset_mm` is the COLLINEAR line's own field name for this quantity;
    // both spellings are written so the line and the row agree either way.
    values.set("dist_to_affine_hull_mm", jnum(m.dist_aff_mm));
    values.set("offset_mm", jnum(m.dist_aff_mm));
    values.set("s_min_mm", jnum(m.fit.s_min_mm));
    values.set(
        "axis_deg",
        jnum(m.fit.dir[1].atan2(m.fit.dir[0]).to_degrees()),
    );
    values.set("contacts", Json::usize(n_contacts));
    values.set("resisting", Json::usize(n_resisting));
    values.set("plate_lines", Json::usize(plate_hull.len()));
    values.set("contact_ids", Json::str(ids_txt.clone()));
    if !reason.is_empty() {
        values.set("reason", Json::str(reason));
    }

    let fields: Vec<(&'static str, String)> = match verdict {
        Verdict::Refuse(CODE_NO_SUPPORT) => vec![
            ("body", body.to_string()),
            ("layer", k.to_string()),
            ("z_mm", fmt(z_k)),
            ("footprint_mm2", fmt(fp.area_mm2)),
            ("centroid", format!("{},{}", fmt(q[0]), fmt(q[1]))),
            ("contacts", "0".to_string()),
        ],
        Verdict::Refuse(CODE_COLLINEAR) => vec![
            ("body", body.to_string()),
            ("layer", k.to_string()),
            ("z_mm", fmt(z_k)),
            ("centroid", format!("{},{}", fmt(q[0]), fmt(q[1]))),
            ("s_min_mm", fmt(m.fit.s_min_mm)),
            ("sigma_mm", fmt(sigma)),
            (
                "axis_deg",
                fmt(m.fit.dir[1].atan2(m.fit.dir[0]).to_degrees()),
            ),
            ("offset_mm", fmt(m.dist_aff_mm)),
            ("contacts", n_contacts.to_string()),
            ("contact_ids", ids_txt),
        ],
        Verdict::Refuse(_) => vec![
            ("body", body.to_string()),
            ("layer", k.to_string()),
            ("z_mm", fmt(z_k)),
            ("centroid", format!("{},{}", fmt(q[0]), fmt(q[1]))),
            ("margin_mm", fmt(m.d_mm)),
            ("sigma_mm", fmt(sigma)),
            ("hull_vertices", m.hull_vertices.to_string()),
            ("hull_area_mm2", fmt(m.hull_area_mm2)),
            ("contacts", n_contacts.to_string()),
            ("resisting", n_resisting.to_string()),
            ("contact_ids", ids_txt),
        ],
        _ => vec![
            ("body", body.to_string()),
            ("layer", k.to_string()),
            ("reason", reason.to_string()),
            ("margin_mm", fmt(m.d_mm)),
            ("sigma_mm", fmt(sigma)),
            ("contacts", n_contacts.to_string()),
        ],
    };
    s.pending.insert(
        key,
        Pending {
            severity,
            fields,
            values,
        },
    );
}

/// A quantity that may not be a number. `Json::num` panics on a non-finite
/// value by design -- "report it as a finding, never as null" -- so a margin
/// against an empty hull, or a distance to an affine hull that does not exist,
/// is written as the STRING naming what it is. It is never omitted and never
/// silently zeroed.
fn jnum(x: f64) -> Json {
    if x.is_finite() {
        Json::num(x)
    } else {
        Json::str(fmt(x))
    }
}

fn fmt(x: f64) -> String {
    if x.is_finite() {
        format!("{x:.6}")
    } else if x == f64::NEG_INFINITY {
        "-inf".to_string()
    } else if x == f64::INFINITY {
        "inf".to_string()
    } else {
        "nan".to_string()
    }
}

fn format_ids(ids: &[usize]) -> String {
    if ids.len() <= CONTACT_ID_LINE_CAP {
        return ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
    }
    let head = ids[..CONTACT_ID_LINE_CAP]
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!("{head}+{}", ids.len() - CONTACT_ID_LINE_CAP)
}

// ---------------------------------------------------------------------------
// Assembling the outcome
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_lines)]
fn finish(
    ctx: &ModelContext,
    file: &str,
    scope: usize,
    contacts: &[ContactLine],
    raster: &RunRaster,
    mut sw: Sweep,
) -> CheckOutcome {
    let model = ctx.model_index;

    // §2.6: exceeding the budget fails, never a silent coarsening.
    if sw.budget_exceeded {
        let budget = ctx.audit.options.spread_budget.max_body_layer_evaluations;
        let mut values = Json::obj();
        values.set("reason", Json::str(R_BUDGET_EXCEEDED));
        values.set("max_body_layer_evaluations", Json::num(budget as f64));
        values.set("reached", Json::num(sw.counts.checked as f64));
        let mut measured = Json::obj();
        measured.set(
            "headline",
            Json::str(format!(
                "body-layer evaluation budget {budget} exceeded; arrangement not established"
            )),
        );
        measured.set("reason", Json::str(R_BUDGET_EXCEEDED));
        let checked = (sw.counts.checked as usize).max(scope).max(1);
        return CheckOutcome {
            checked,
            failed: 0,
            indeterminate: checked,
            measured,
            failures: vec![Failure::line(
                CODE_INDETERMINATE,
                file,
                model,
                None,
                None,
                &[
                    ("body", "n/a".to_string()),
                    ("layer", "n/a".to_string()),
                    ("reason", R_BUDGET_EXCEEDED.to_string()),
                    ("margin_mm", "n/a".to_string()),
                    ("sigma_mm", "n/a".to_string()),
                    ("contacts", "n/a".to_string()),
                ],
                values,
            )],
        };
    }

    // §3.7, the anti-vacuity rule. `checked == 0` is never evidence.
    let body_count = {
        let n = sw.uf.p.len();
        let mut roots: Vec<usize> = (0..n).map(|i| sw.uf.find(i)).collect();
        roots.sort_unstable();
        roots.dedup();
        roots.len()
    };
    if body_count == 0 || sw.counts.checked == 0 {
        let scope = scope.max(1);
        let mut values = Json::obj();
        values.set("reason", Json::str(R_NO_BODIES));
        values.set("bodies", Json::usize(body_count));
        values.set("layers", Json::usize(raster.layers.len()));
        values.set("contacts", Json::usize(contacts.len()));
        let mut measured = Json::obj();
        measured.set(
            "headline",
            Json::str(format!(
                "no prefix-connected model body found across {} raster layers; \
                 a check whose scope is zero passes vacuously and this one refuses to",
                raster.layers.len()
            )),
        );
        measured.set("reason", Json::str(R_NO_BODIES));
        measured.set("bodies", Json::usize(0));
        return CheckOutcome {
            checked: scope,
            failed: 0,
            indeterminate: scope,
            measured,
            failures: vec![Failure::line(
                CODE_INDETERMINATE,
                file,
                model,
                None,
                None,
                &[
                    ("body", "n/a".to_string()),
                    ("layer", "n/a".to_string()),
                    ("reason", R_NO_BODIES.to_string()),
                    ("bodies", "0".to_string()),
                    ("layers", raster.layers.len().to_string()),
                    ("contacts", contacts.len().to_string()),
                ],
                values,
            )],
        };
    }

    let mut out = CheckOutcome::new(sw.counts.checked as usize);
    out.failed = sw.counts.failed as usize;
    out.indeterminate = sw.counts.indeterminate as usize;

    // Failure lines, worst first, capped in print but never in count.
    let mut rows: Vec<((usize, &'static str, &'static str), Pending)> =
        std::mem::take(&mut sw.pending).into_iter().collect();
    rows.sort_by(|a, b| {
        b.1.severity
            .total_cmp(&a.1.severity)
            .then((a.0).0.cmp(&(b.0).0))
    });
    let printed = rows.len().min(MAX_FAILURE_LINES);
    for ((_, code, _), p) in rows.into_iter().take(MAX_FAILURE_LINES) {
        out.failures.push(Failure::line(
            code, file, model, None, None, &p.fields, p.values,
        ));
    }

    // ---- the numbers -----------------------------------------------------
    let mut per_body: Vec<&BodyState> = Vec::new();
    {
        let n = sw.uf.p.len();
        let mut roots: Vec<usize> = (0..n).map(|i| sw.uf.find(i)).collect();
        roots.sort_unstable();
        roots.dedup();
        for r in roots {
            per_body.push(&sw.bodies[r]);
        }
    }
    let lifetimes: Vec<f64> = per_body
        .iter()
        .filter(|b| b.evals > 0)
        .map(|b| (b.last_layer - b.birth_layer + 1) as f64)
        .collect();
    let m_hats: Vec<f64> = per_body
        .iter()
        .filter(|b| b.worst_m_hat.is_finite())
        .map(|b| b.worst_m_hat)
        .collect();
    let s_mins: Vec<f64> = per_body
        .iter()
        .filter(|b| b.s_min_at_worst_mm.is_finite())
        .map(|b| b.s_min_at_worst_mm)
        .collect();
    let anis: Vec<f64> = per_body
        .iter()
        .filter(|b| b.anisotropy_at_worst.is_finite())
        .map(|b| b.anisotropy_at_worst)
        .collect();
    let slender = per_body
        .iter()
        .map(|b| b.max_slenderness)
        .fold(0.0f64, f64::max);

    let bodies_8 = count_bodies(raster, true);

    let mut bodies_j = Json::obj();
    bodies_j.set("count", Json::usize(per_body.len()));
    bodies_j.set("evaluations", Json::num(sw.counts.checked as f64));
    bodies_j.set("layers_with_material", Json::num(sw.layers_with_material as f64));
    bodies_j.set("lifetime_layers", stats_json(&lifetimes));
    // Attachments are counted when they happen, and two bodies a contact
    // attached to separately may later merge, so this is a HISTORY count and
    // can exceed the contact total. The distinct count below is the one to
    // compare against `contacts_total`.
    bodies_j.set(
        "contact_body_attachments_at_attach_time",
        Json::usize(sw.contacts_attached),
    );
    let mut distinct: Vec<usize> = per_body
        .iter()
        .flat_map(|b| b.contacts.iter().map(|(ci, _)| *ci))
        .collect();
    distinct.sort_unstable();
    distinct.dedup();
    bodies_j.set("contacts_held_by_some_body", Json::usize(distinct.len()));
    bodies_j.set("contacts_total", Json::usize(contacts.len()));
    bodies_j.set(
        "contacts_unusable",
        Json::usize(contacts.iter().filter(|c| c.bad.is_some()).count()),
    );
    out.measured.set("bodies", bodies_j);

    let mut margin_j = Json::obj();
    margin_j.set("note", Json::str(
        "MODE C, measured and NOT gated: no measurement in this project fixes a \
         minimum normalised margin, so eccentricity above containment is a number here",
    ));
    margin_j.set("m_hat_worst_per_body", stats_json(&m_hats));
    margin_j.set("histogram_0p05", histogram_json(&m_hats, 0.05));
    out.measured.set("margin", margin_j);

    let mut rank_j = Json::obj();
    rank_j.set("s_min_mm_at_worst_layer", stats_json(&s_mins));
    rank_j.set("anisotropy_at_worst_layer", stats_json(&anis));
    rank_j.set(
        "note",
        Json::str("s_min is the axis a future arrangement coupon would put a threshold on"),
    );
    out.measured.set("rank", rank_j);

    let mut slender_j = Json::obj();
    slender_j.set("max_h_over_s_min", Json::num(slender));
    slender_j.set(
        "note",
        Json::str(
            "MODE D, measured and NOT gated: overturning needs a lateral force magnitude \
             and a green-resin modulus, and this project has neither",
        ),
    );
    out.measured.set("slenderness", slender_j);

    let mut sens_j = Json::obj();
    sens_j.set(
        "c_line_of_action_vs_contact_points",
        Json::num(sw.sensitivity_line_of_action as f64),
    );
    sens_j.set(
        "a_bodies_merged_under_8_connectivity",
        Json::num((per_body.len() as i64 - bodies_8 as i64) as f64),
    );
    sens_j.set("a_bodies_8_connectivity", Json::usize(bodies_8));
    sens_j.set(
        "b_pixel_centre_fill_rule",
        Json::str("not computed: it needs a second raster, which report_metrics.rs owns"),
    );
    out.measured.set("sensitivity", sens_j);

    let mut codes_j = Json::obj();
    for (k, n) in &sw.by_code {
        codes_j.set(k, Json::num(*n as f64));
    }
    out.measured.set("by_code", codes_j);

    let mut tol_j = Json::obj();
    tol_j.set("polarity", Json::str("pass requires d > +sigma; d within +/-sigma is indeterminate and fails"));
    tol_j.set("sigma_terms", Json::str("sigma_q (raster erosion bound) + sigma_H (max sigma_g) + d2/2"));
    tol_j.set("d2_over_2_mm", Json::num(c::D2_MM * 0.5));
    tol_j.set("chosen_constants", Json::usize(0));
    out.measured.set("tolerance", tol_j);

    out.measured.set(
        "disclosures_not_computed",
        Json::str(
            "report.spread.horizontal_residual (mode E), report.spread.release_sweep, and \
             sensitivity (b) are NOT computed anywhere yet -- report_metrics.rs owns them. \
             Their absence is not a zero.",
        ),
    );
    out.measured
        .set("failure_lines_printed", Json::usize(printed));
    out.measured
        .set("failure_line_keys_dropped", Json::num(sw.pending_dropped as f64));
    out.measured.set("passed", Json::num(sw.counts.passed as f64));

    let worst = per_body
        .iter()
        .filter(|b| b.worst_margin_mm.is_finite())
        .min_by(|a, b| a.worst_margin_mm.total_cmp(&b.worst_margin_mm));
    let headline = if out.failed + out.indeterminate == 0 {
        format!(
            "{} bodies, {} body-layer evaluations, every one in equilibrium by more than sigma",
            per_body.len(),
            sw.counts.checked
        )
    } else {
        // The dominant code names WHAT went wrong; the worst margin says by
        // how much, when a margin exists at all. A run in which every margin
        // is unmeasurable must say so rather than borrow the passing phrasing.
        let dominant = sw
            .by_code
            .iter()
            .max_by_key(|(_, n)| **n)
            .map_or_else(String::new, |(k, n)| format!("; dominant {k} x{n}"));
        let margin = worst.map_or_else(
            || "; no containment margin was measurable on any evaluation".to_string(),
            |b| {
                format!(
                    "; worst containment margin {:.4} mm at layer {}",
                    b.worst_margin_mm, b.worst_layer
                )
            },
        );
        format!(
            "{} bodies, {} body-layer evaluations: {} pass, {} refused, {} indeterminate{dominant}{margin}",
            per_body.len(),
            sw.counts.checked,
            sw.counts.passed,
            out.failed,
            out.indeterminate,
        )
    };
    out.measured.set("headline", Json::str(headline));
    out
}

fn stats_json(xs: &[f64]) -> Json {
    let xs: Vec<f64> = xs.iter().copied().filter(|x| x.is_finite()).collect();
    let xs = xs.as_slice();
    let mut o = Json::obj();
    o.set("n", Json::usize(xs.len()));
    if xs.is_empty() {
        o.set("min", Json::Null);
        o.set("p05", Json::Null);
        o.set("median", Json::Null);
        o.set("max", Json::Null);
        return o;
    }
    let mut v = xs.to_vec();
    v.sort_by(f64::total_cmp);
    let at = |f: f64| v[(((v.len() - 1) as f64) * f).round() as usize];
    o.set("min", Json::num(v[0]));
    o.set("p05", Json::num(at(0.05)));
    o.set("median", Json::num(at(0.5)));
    o.set("max", Json::num(v[v.len() - 1]));
    o
}

fn histogram_json(xs: &[f64], bin: f64) -> Json {
    let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
    for x in xs {
        if x.is_finite() {
            *counts.entry((x / bin).floor() as i64).or_default() += 1;
        }
    }
    let mut o = Json::obj();
    o.set("bin_width", Json::num(bin));
    let mut arr = Json::Arr(Vec::new());
    for (k, n) in counts {
        let mut row = Json::obj();
        row.set("lo", Json::num(k as f64 * bin));
        row.set("count", Json::usize(n));
        arr.push(row);
    }
    o.set("bins", arr);
    o
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::Status;
    use crate::context::{AuditContext, AuditOptions};
    use crate::reader::{ChituboxFile, RecordKind};
    use crate::synth::{box_soup, SynthFile, SynthModel, SynthRecord};
    use std::path::Path;

    #[test]
    fn the_schema_key_and_title_are_pinned() {
        assert_eq!(ArrangementSpread.id(), "i10_arrangement_spread");
        assert_eq!(ArrangementSpread.title(), "I10 arrangement spread");
    }

    // -- the predicate's boundary ------------------------------------------

    /// The polarity finding 3 of the review requires: a margin inside the
    /// uncertainty band is INDETERMINATE, and indeterminate fails. It is not
    /// `delivered >= required - tol`.
    #[test]
    fn a_margin_inside_the_uncertainty_band_never_passes() {
        let square = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
        let sigma = 0.05;
        // q is 0.04 mm inside the right edge: inside, but not by more than sigma.
        let q = [0.96, 0.0];
        let (verdict, m) = classify(q, &square, &square, false, sigma);
        assert!((m.d_mm - 0.04).abs() < 1e-12, "d={}", m.d_mm);
        assert_eq!(verdict, Verdict::Indeterminate(R_MARGIN_IN_TOL));
        // One quantum further in and it is a pass; the boundary is exactly
        // d > sigma, with no slack on the failing side.
        let (v2, m2) = classify([0.94, 0.0], &square, &square, false, sigma);
        assert!(m2.d_mm > sigma);
        assert_eq!(v2, Verdict::Pass);
        // And just outside the band on the low side is still indeterminate,
        // never a pass.
        let (v3, _) = classify([1.049, 0.0], &square, &square, false, sigma);
        assert_eq!(v3, Verdict::Indeterminate(R_MARGIN_IN_TOL));
    }

    #[test]
    fn no_line_of_action_at_all_is_a_refusal_not_a_skip() {
        let (verdict, _) = classify([0.0, 0.0], &[], &[], false, 0.01);
        assert_eq!(verdict, Verdict::Refuse(CODE_NO_SUPPORT));
    }

    /// Three collinear arms, load off the line: refusable at any X, which is
    /// exactly what `physics.md` §2(c) says the per-arm ratio cannot see.
    #[test]
    fn a_collinear_group_with_the_load_off_the_axis_is_refused() {
        let g = [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]];
        let (verdict, m) = classify([1.0, 1.5], &g, &g, false, 0.05);
        assert_eq!(m.aff_dim, 1);
        assert!((m.dist_aff_mm - 1.5).abs() < 1e-12);
        assert_eq!(verdict, Verdict::Refuse(CODE_COLLINEAR));
    }

    /// The plate-6 strip case: collinear, load ON the line. A neutral
    /// equilibrium whose restoring moment is unmeasured member bending, so it
    /// is INDETERMINATE -- not a proven collapse and not a pass.
    #[test]
    fn a_collinear_group_with_the_load_on_the_axis_is_indeterminate_not_refused() {
        let g = [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]];
        let (verdict, m) = classify([1.0, 0.01], &g, &g, false, 0.05);
        assert_eq!(m.aff_dim, 1);
        assert_eq!(verdict, Verdict::Indeterminate(R_RANK_DEFICIENT));
    }

    /// One contact is rank 0 and two are rank 1: the same mode, the same
    /// predicate, no special case.
    #[test]
    fn one_and_two_contact_limit_cases_are_the_same_mode() {
        let one = [[0.0, 0.0]];
        let (v1, m1) = classify([1.0, 0.0], &one, &one, false, 0.05);
        assert_eq!(m1.aff_dim, 0);
        assert_eq!(v1, Verdict::Refuse(CODE_COLLINEAR));
        let two = [[0.0, 0.0], [2.0, 0.0]];
        let (v2, m2) = classify([1.0, 1.0], &two, &two, false, 0.05);
        assert_eq!(m2.aff_dim, 1);
        assert_eq!(v2, Verdict::Refuse(CODE_COLLINEAR));
    }

    /// Outside the resisting hull with a member pointing away from the plate:
    /// statics cannot decide, because bounding the guy wire's tension needs a
    /// peel force this project never computes. Indeterminate, and it fails.
    #[test]
    fn a_non_resisting_member_turns_a_refusal_into_an_indeterminate() {
        let g = [[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]];
        let q = [5.0, 5.0];
        let (v_no, _) = classify(q, &g, &g, false, 0.05);
        assert_eq!(v_no, Verdict::Refuse(CODE_NO_EQUILIBRIUM));
        let (v_yes, _) = classify(q, &g, &g, true, 0.05);
        assert_eq!(v_yes, Verdict::Indeterminate(R_NON_RESISTING));
    }

    /// The unmeasurable input: no resisting line of action at all. The hull is
    /// empty, so the margin is not a number -- and an unmeasurable margin is
    /// never a pass.
    #[test]
    fn an_empty_resisting_hull_is_unmeasurable_and_never_passes() {
        let all = [[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]];
        let (verdict, m) = classify([0.5, 0.4], &[], &all, true, 0.05);
        assert_eq!(m.d_mm, f64::NEG_INFINITY);
        assert_eq!(verdict, Verdict::Indeterminate(R_NON_RESISTING));
        assert_ne!(verdict, Verdict::Pass);
    }

    /// A hull with no interior -- two distinct points -- can never return a
    /// positive margin, whatever the load point.
    #[test]
    fn a_degenerate_hull_has_no_interior_to_be_inside_of() {
        let seg = [[0.0, 0.0], [1.0, 0.0]];
        let hull = convex_hull(&seg);
        assert_eq!(hull.len(), 2);
        assert!(convex_signed_margin(&hull, [0.5, 0.0]) <= 0.0);
        assert!((convex_signed_margin(&hull, [0.5, 0.25]) + 0.25).abs() < 1e-12);
    }

    #[test]
    fn the_signed_margin_is_a_true_distance_in_both_directions() {
        let sq = convex_hull(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]);
        assert_eq!(sq.len(), 4);
        assert!((convex_signed_margin(&sq, [1.0, 1.0]) - 1.0).abs() < 1e-12);
        assert!((convex_signed_margin(&sq, [0.25, 1.0]) - 0.25).abs() < 1e-12);
        assert!((convex_signed_margin(&sq, [-0.5, 1.0]) + 0.5).abs() < 1e-12);
        // A corner: the true distance, not the half-plane violation.
        assert!((convex_signed_margin(&sq, [-3.0, -4.0]) + 5.0).abs() < 1e-12);
    }

    // -- end-to-end, on planted-defect models ------------------------------

    const R_BALL: f64 = 0.3;

    fn tip(x: f64, y: f64, z: f64) -> SynthRecord {
        // `start` is the ball's deepest point INSIDE the model (the slab is
        // above), `end` its outward pole; `centre = midpoint(start, end)`.
        SynthRecord::new(
            RecordKind::ContactSphere,
            [x, y, z + R_BALL],
            [x, y, z - R_BALL],
            R_BALL,
            R_BALL,
        )
        .with_matching_soup()
    }

    /// A taper leaving the ball centre for a node at `node`.
    fn taper(x: f64, y: f64, z: f64, node: [f64; 3]) -> SynthRecord {
        SynthRecord::new(RecordKind::ContactTaper, [x, y, z], node, 0.10, 0.20)
            .with_declared_depth(0.3)
            .with_matching_soup()
    }

    /// Vertical support under a tip at `(x, y)` seated on the slab's underside.
    fn vertical_support(x: f64, y: f64) -> [SynthRecord; 2] {
        [tip(x, y, 0.0), taper(x, y, 0.0, [x, y, -2.0])]
    }

    /// A member tilted so that its line of action at a plane above the seat is
    /// displaced by `+dz_to_dx` per mm of height.
    fn tilted_support(x: f64, y: f64, dx_over_dz: f64) -> [SynthRecord; 2] {
        let dz = 2.0;
        [
            tip(x, y, 0.0),
            taper(x, y, 0.0, [x - dx_over_dz * dz, y, -dz]),
        ]
    }

    fn slab_model(thickness: f64, records: Vec<SynthRecord>) -> SynthFile {
        SynthFile::new().with_model(SynthModel {
            name: "slab".into(),
            translate_mm: [0.0, 0.0, 5.0],
            mesh: box_soup([0.0, 0.0, 0.0], [4.0, 4.0, thickness]),
            records,
        })
    }

    fn run_on(sf: &SynthFile) -> CheckOutcome {
        let file = ChituboxFile::parse(Path::new("spread_test.chitubox"), &sf.to_bytes())
            .expect("synthetic file must parse");
        let options = AuditOptions::default();
        let audit = AuditContext {
            file: &file,
            options: &options,
            sidecar: None,
            sidecar_absence: None,
        };
        let ctx = ModelContext::build(&audit, 0);
        assert!(
            ctx.model_mesh.closure.is_closed(),
            "the planted model must be a closed solid: {}",
            ctx.model_mesh.closure.summary()
        );
        // The slab must float: z_plate is 5 mm below its underside, so no
        // plate line of action can widen the support polygon.
        assert!(
            ctx.z_plate_mm < -4.0,
            "z_plate={} -- the slab must not be plate-grounded",
            ctx.z_plate_mm
        );
        ArrangementSpread.run(&ctx)
    }

    fn codes(o: &CheckOutcome) -> Vec<&str> {
        o.failures.iter().map(|f| f.code).collect()
    }

    fn measured_num(o: &CheckOutcome, path: &[&str]) -> f64 {
        let mut cur = &o.measured;
        for k in path {
            cur = cur.get(k).unwrap_or_else(|| panic!("no measured.{k}"));
        }
        cur.as_f64().expect("a number")
    }

    /// `spread_supported` -- **the negative control, and it is not optional.**
    /// Without a passing control, a check that fails everything looks perfect.
    #[test]
    fn spread_supported_four_corner_tips_pass() {
        let mut recs = Vec::new();
        for (x, y) in [(0.6, 0.6), (3.4, 0.6), (3.4, 3.4), (0.6, 3.4)] {
            recs.extend(vertical_support(x, y));
        }
        let o = run_on(&slab_model(0.4, recs));
        assert_eq!(
            o.status(),
            Status::Pass,
            "control must pass; failures={:?}",
            o.failures.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
        assert!(o.checked > 0, "and it must have evaluated something");
        assert_eq!(o.failed, 0);
        assert_eq!(o.indeterminate, 0);
        assert!(measured_num(&o, &["bodies", "count"]) >= 1.0);
        assert!(measured_num(&o, &["passed"]) > 0.0);
    }

    /// `spread_collinear` -- three tips on one line, the slab's centroid off
    /// it. Refusable at any per-arm ratio.
    #[test]
    fn spread_collinear_three_tips_on_a_line_are_refused() {
        let mut recs = Vec::new();
        for x in [1.0, 2.0, 3.0] {
            recs.extend(vertical_support(x, 0.5));
        }
        let o = run_on(&slab_model(0.4, recs));
        assert_eq!(o.status(), Status::Fail);
        assert!(o.failed > 0, "a proven refusal, not an indeterminate");
        assert!(
            codes(&o).contains(&CODE_COLLINEAR),
            "codes={:?}",
            codes(&o)
        );
        let f = o
            .failures
            .iter()
            .find(|f| f.code == CODE_COLLINEAR)
            .unwrap();
        let offset = f
            .values
            .get("offset_mm")
            .and_then(Json::as_f64)
            .expect("the line states by how much");
        assert!(offset > 1.0, "offset_mm={offset}");
        assert!(f.message.contains("offset_mm="));
    }

    /// `spread_eccentric` -- three tips clustered at one end, so the load
    /// centroid falls outside their hull. No tension set balances it.
    #[test]
    fn spread_eccentric_a_cluster_at_one_end_is_refused() {
        let mut recs = Vec::new();
        for (x, y) in [(0.5, 0.5), (1.0, 0.5), (0.75, 1.0)] {
            recs.extend(vertical_support(x, y));
        }
        let o = run_on(&slab_model(0.4, recs));
        assert_eq!(o.status(), Status::Fail);
        assert!(
            codes(&o).contains(&CODE_NO_EQUILIBRIUM),
            "codes={:?}",
            codes(&o)
        );
        let f = o
            .failures
            .iter()
            .find(|f| f.code == CODE_NO_EQUILIBRIUM)
            .unwrap();
        let margin = f.values.get("margin_mm").and_then(Json::as_f64).unwrap();
        assert!(margin < 0.0, "margin_mm={margin} must state the shortfall");
    }

    /// `spread_tilted_axes` -- **the witness that §3.2 actually runs.** Ball
    /// centres project INSIDE the hull; the lines of action at the loaded
    /// layer project outside it. A contact-point implementation returns the
    /// opposite verdict, and the sensitivity count proves the branch fired.
    #[test]
    fn spread_tilted_axes_refuses_where_a_contact_point_hull_would_pass() {
        let mut recs = Vec::new();
        for (x, y) in [(0.5, 2.0), (3.5, 2.0), (2.0, 0.5), (2.0, 3.5)] {
            recs.extend(tilted_support(x, y, 0.6));
        }
        let o = run_on(&slab_model(3.0, recs));
        assert_eq!(o.status(), Status::Fail);
        assert!(
            codes(&o).contains(&CODE_NO_EQUILIBRIUM),
            "codes={:?}",
            codes(&o)
        );
        // A count, not a claim: the number of body-layer evaluations whose
        // verdict changes when the polygon is built from contact points.
        let sens = measured_num(
            &o,
            &["sensitivity", "c_line_of_action_vs_contact_points"],
        );
        assert!(
            sens > 0.0,
            "the line-of-action projection must be measurably load-bearing, got {sens}"
        );
        // And the same plant with UNTILTED members passes, so the difference
        // is the tilt and not the slab.
        let mut flat = Vec::new();
        for (x, y) in [(0.5, 2.0), (3.5, 2.0), (2.0, 0.5), (2.0, 3.5)] {
            flat.extend(vertical_support(x, y));
        }
        let control = run_on(&slab_model(3.0, flat));
        assert_eq!(
            control.status(),
            Status::Pass,
            "untilted control failures={:?}",
            control
                .failures
                .iter()
                .map(|f| &f.message)
                .collect::<Vec<_>>()
        );
    }

    /// A tip with no taper joined to it has no line of action. "Never a skip":
    /// the body-layer evaluation is indeterminate, and indeterminate fails.
    #[test]
    fn a_tip_with_no_taper_is_indeterminate_never_skipped() {
        let mut recs = Vec::new();
        for (x, y) in [(0.6, 0.6), (3.4, 0.6), (3.4, 3.4)] {
            recs.extend(vertical_support(x, y));
        }
        recs.push(tip(0.6, 3.4, 0.0)); // no taper for this one
        let o = run_on(&slab_model(0.4, recs));
        assert_eq!(o.status(), Status::Fail);
        assert!(o.indeterminate > 0);
        assert!(o
            .failures
            .iter()
            .any(|f| f.message.contains(R_TAPER_AMBIGUOUS)));
    }

    /// A horizontal member's line of action never meets the layer plane, so
    /// `g_c(k)` does not exist. Indeterminate, with its own reason.
    #[test]
    fn a_horizontal_member_is_indeterminate_with_its_own_reason() {
        let mut recs = Vec::new();
        for (x, y) in [(0.6, 0.6), (3.4, 0.6), (3.4, 3.4)] {
            recs.extend(vertical_support(x, y));
        }
        recs.push(tip(0.6, 3.4, 0.0));
        recs.push(taper(0.6, 3.4, 0.0, [2.6, 3.4, 0.0]));
        let o = run_on(&slab_model(0.4, recs));
        assert_eq!(o.status(), Status::Fail);
        assert!(o.indeterminate > 0);
        assert!(
            o.failures
                .iter()
                .any(|f| f.message.contains(R_AXIS_HORIZONTAL)),
            "messages={:?}",
            o.failures.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }

    /// A slab with no supports at all: every body-layer is refused for the
    /// stated reason, and the count is the layer count, not zero.
    #[test]
    fn an_unheld_body_is_refused_with_its_footprint_named() {
        let o = run_on(&slab_model(0.4, Vec::new()));
        assert_eq!(o.status(), Status::Fail);
        assert!(codes(&o).contains(&CODE_NO_SUPPORT));
        assert!(o.checked >= 7, "checked={}", o.checked);
        assert_eq!(o.failed, o.checked, "every layer of an unheld body");
        let f = o
            .failures
            .iter()
            .find(|f| f.code == CODE_NO_SUPPORT)
            .unwrap();
        assert!(f.message.contains("footprint_mm2="));
    }

    /// §2.6: the budget is a hard stop, never a silent coarsening.
    #[test]
    fn exceeding_the_body_layer_budget_fails() {
        use crate::context::SpreadBudget;
        let sf = slab_model(0.4, Vec::new());
        let file = ChituboxFile::parse(Path::new("spread_budget.chitubox"), &sf.to_bytes())
            .expect("parse");
        let options = AuditOptions {
            spread_budget: SpreadBudget {
                max_body_layer_evaluations: 2,
            },
            ..AuditOptions::default()
        };
        let audit = AuditContext {
            file: &file,
            options: &options,
            sidecar: None,
            sidecar_absence: None,
        };
        let ctx = ModelContext::build(&audit, 0);
        let o = ArrangementSpread.run(&ctx);
        assert_eq!(o.status(), Status::Fail);
        assert!(o.indeterminate > 0);
        assert!(o
            .failures
            .iter()
            .any(|f| f.message.contains(R_BUDGET_EXCEEDED)));
    }

    /// §3.7, the anti-vacuity rule, asserted on the instrument itself: a model
    /// whose raster cannot be built fails with a named reason. A check whose
    /// scope is zero passes vacuously, and this project has shipped that
    /// defect once already.
    #[test]
    fn a_model_that_cannot_be_rastered_fails_and_never_passes_vacuously() {
        // One triangle: a mesh that is not closed has no interior to raster.
        let sf = SynthFile::new().with_model(SynthModel {
            name: "open".into(),
            translate_mm: [0.0, 0.0, 5.0],
            mesh: vec![[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]],
            records: Vec::new(),
        });
        let file =
            ChituboxFile::parse(Path::new("spread_open.chitubox"), &sf.to_bytes()).expect("parse");
        let options = AuditOptions::default();
        let audit = AuditContext {
            file: &file,
            options: &options,
            sidecar: None,
            sidecar_absence: None,
        };
        let ctx = ModelContext::build(&audit, 0);
        let o = ArrangementSpread.run(&ctx);
        assert_eq!(o.status(), Status::Fail);
        assert_eq!(o.checked, o.indeterminate);
        assert!(o.checked > 0, "an aborted check still reports its scope");
        assert!(!o.is_not_implemented(), "this invariant is implemented");
    }

    // -- the run-based primitives ------------------------------------------

    #[test]
    fn overlap_len_is_the_shared_column_count() {
        let a = [Run { c0: 0, c1: 10 }, Run { c0: 20, c1: 30 }];
        let b = [Run { c0: 5, c1: 25 }];
        assert_eq!(overlap_len(&a, &b), 5 + 5);
        assert_eq!(overlap_len(&a, &[Run { c0: 40, c1: 50 }]), 0);
    }

    #[test]
    fn four_connectivity_splits_what_eight_connectivity_would_fuse() {
        // Two pixels touching only at a corner.
        let lr = LayerRuns {
            runs: vec![(0, Run { c0: 0, c1: 1 }), (1, Run { c0: 1, c1: 2 })],
            groups: vec![(0, 0, 1), (1, 1, 2)],
        };
        let four = inplane_labels(&lr, false);
        assert_ne!(four[0], four[1], "4-connectivity must split them");
        let eight = inplane_labels(&lr, true);
        assert_eq!(eight[0], eight[1], "8-connectivity must fuse them");
    }
}

// ---------------------------------------------------------------------------
// Real emitted bytes
// ---------------------------------------------------------------------------

#[cfg(test)]
mod real_bytes {
    use super::*;
    use crate::context::{AuditContext, AuditOptions};
    use crate::reader::ChituboxFile;
    use std::path::{Path, PathBuf};

    /// Run I10, and nothing else, over one artifact.
    fn i10_on(path: &Path) -> Vec<(String, CheckOutcome)> {
        let bytes = std::fs::read(path).expect("read");
        let file = ChituboxFile::parse(path, &bytes).expect("parse");
        let options = AuditOptions::default();
        let audit = AuditContext {
            file: &file,
            options: &options,
            sidecar: None,
            sidecar_absence: None,
        };
        (0..file.models.len())
            .map(|i| {
                let ctx = ModelContext::build(&audit, i);
                let name = file.models[i].name.clone();
                (name, ArrangementSpread.run(&ctx))
            })
            .collect()
    }

    fn num(o: &CheckOutcome, path: &[&str]) -> f64 {
        let mut cur = &o.measured;
        for k in path {
            let Some(next) = cur.get(k) else {
                return f64::NAN;
            };
            cur = next;
        }
        cur.as_f64().unwrap_or(f64::NAN)
    }

    fn report(tag: &str, name: &str, o: &CheckOutcome) {
        eprintln!(
            "{tag} :: {name} :: status={} checked={} passed={} failed={} indeterminate={} \
             bodies={} bodies8={} sens_c={} contacts={}/{} :: {}",
            o.status().as_str(),
            o.checked,
            num(o, &["passed"]),
            o.failed,
            o.indeterminate,
            num(o, &["bodies", "count"]),
            num(o, &["sensitivity", "a_bodies_8_connectivity"]),
            num(o, &["sensitivity", "c_line_of_action_vs_contact_points"]),
            num(o, &["bodies", "contacts_held_by_some_body"]),
            num(o, &["bodies", "contacts_total"]),
            o.headline(),
        );
        let mut by: Vec<String> = Vec::new();
        if let Some(Json::Obj(rows)) = o.measured.get("by_code") {
            for (k, v) in rows {
                by.push(format!("{k}={}", v.as_f64().unwrap_or(0.0)));
            }
        }
        if !by.is_empty() {
            eprintln!("{tag} ::   by_code {}", by.join(" "));
        }
        for f in o.failures.iter().take(3) {
            eprintln!("{tag} ::   {}", f.message);
        }
    }

    /// **The §3.7 anti-vacuity rule, asserted against real bytes rather than
    /// trusted.** A check whose scope is zero passes vacuously, and in the run
    /// that built this crate the fixture harness reported green because it had
    /// globbed a path where no fixtures were. `checked == 0` is never evidence.
    ///
    /// The treant is excluded by name, not by accident: it is 429,120 faces
    /// and 91 mm tall, and the baseline of 2026-08-30 records that it was
    /// deliberately not put through a full audit.
    #[test]
    fn i10_is_never_vacuous_on_the_emitted_corpus() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("exports");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            eprintln!("no exports/ at {}; skipping", dir.display());
            return;
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "chitubox"))
            .filter(|p| !p.to_string_lossy().contains("treant"))
            .collect();
        files.sort();
        assert!(!files.is_empty(), "the corpus glob found nothing -- that is the defect this test exists for");
        let mut any_pass = 0usize;
        for f in &files {
            for (name, o) in i10_on(f) {
                report("exports", &format!("{}/{name}", f.file_name().unwrap().to_string_lossy()), &o);
                assert!(
                    o.checked > 0,
                    "{}: I10 examined nothing; checked == 0 is never evidence",
                    f.display()
                );
                assert!(
                    num(&o, &["bodies", "count"]) > 0.0 || o.is_not_implemented(),
                    "{}: no prefix-connected body was found at all",
                    f.display()
                );
                if num(&o, &["passed"]) > 0.0 {
                    any_pass += 1;
                }
            }
        }
        // §7: a gate that refuses 100 % of evaluations on every corpus file is
        // reporting a defect in the instrument, not a finding about the plan.
        assert!(
            any_pass > 0,
            "no body-layer evaluation passed anywhere in the corpus; \
             diagnose the instrument before quoting its results"
        );
    }

    /// Optional sweep over an external corpus of real miniatures, named by
    /// `PLINTH_AUDIT_SPREAD_CORPUS`. Prints numbers; asserts only the
    /// anti-vacuity rule, because what the plan actually contains is the
    /// finding, not something to assert.
    #[test]
    fn i10_over_an_external_corpus_when_one_is_named() {
        let Ok(root) = std::env::var("PLINTH_AUDIT_SPREAD_CORPUS") else {
            eprintln!("PLINTH_AUDIT_SPREAD_CORPUS unset; skipping");
            return;
        };
        let mut files: Vec<PathBuf> = Vec::new();
        collect(Path::new(&root), &mut files);
        files.sort();
        assert!(!files.is_empty(), "named corpus {root} contained no .chitubox");
        for f in &files {
            for (name, o) in i10_on(f) {
                report("corpus", &format!("{}/{name}", f.file_name().unwrap().to_string_lossy()), &o);
                assert!(o.checked > 0, "{}: checked == 0", f.display());
            }
        }
    }

    fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                collect(&p, out);
            } else if p.extension().is_some_and(|x| x == "chitubox") {
                out.push(p);
            }
        }
    }
}
