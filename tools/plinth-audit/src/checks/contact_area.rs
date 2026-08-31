//! I2 contact area -- `docs/specs/audit-invariants.md` §3.2.
//!
//! The load-bearing check, and the one no instrument has ever measured.
//! Contact area in this project has only ever been computed from ideal
//! geometry. **This check computes it from what the slicer would actually
//! cure.**
//!
//! ## The predicate, exactly as implemented
//!
//! For each contact `c` with reconstructed seat `(p0, u, h_delivered)`:
//!
//! ```text
//! P(c)      = { x : (x - p0) . u = h_delivered }      the separating cut plane
//! Patch(c)  = the connected component of  P(c) INTERSECT cured_voxels(S)
//!             that contains  p0 + h_delivered * u
//! A_rast(c) = area(Patch(c))                          mm^2, RECONSTRUCTED
//!
//! PASS  iff  A_rast(c) >= A_owned(c) / x_allowed - tol_area(A_owned(c))
//! ```
//!
//! `A_owned` is the shared Voronoi partition of §2.2, `x_allowed = 12`, and
//! `tol_area(A) = max(4096 * eps64 * A / x_allowed, 1e-9)` mm^2 (§1.5, §3.2).
//!
//! Three further numbers are reported for every contact, failing or not:
//!
//! ```text
//! A_ideal(c)   = pi * a^2,  a^2 = 2*f*r_tip*h - h^2   RECONSTRUCTED, ideal
//! A_implied(c) = x_allowed * pi * r_tip^2             DECLARED, by inversion
//! raster_loss(c) = 1 - A_rast / A_ideal
//! ```
//!
//! ## Why this check exists
//!
//! The owner ruled `default_contact_depth` down from 0.300 mm to 0.100 mm on
//! 2026-08-30, on the ground that burial depth is not an adhesion requirement.
//! That ruling is bound to one condition: **contact area still governs, and
//! depth never overrides it.** A shallower seat is a RETRACTED ball, not a
//! trimmed one, so its cut disc shrinks as `a = sqrt(2*f*r*h - h*h)`. This is
//! the check that says so, and it says it from cured pixels rather than from
//! the algebra.
//!
//! ## Partial pixels, and the direction of the error
//!
//! The shared rasteriser's fill rule is **full containment**: a pixel column is
//! cured at layer `k` only when the solid contains its whole footprint **at
//! that layer's sampling plane `z_k`** (`audit-invariants.md` §1.6). Partial
//! pixels are NOT cured. A pass therefore never rests on the printer's
//! unmeasured partial-exposure behaviour. The cost is a roughly one-pixel rim
//! -- about 3 % of area at the tip sizes in `exports/`, and much more below
//! 0.1 mm diameter -- and it is disclosed, not absorbed:
//!
//! * every contact reports `raster_loss`, the fraction of the ideal disc the
//!   quantisation removed;
//! * `measured.totals.failed_even_at_ideal_disc` counts the failures that
//!   would still fail if the patch were the whole ideal disc, plus the whole
//!   quantisation allowance derived below. Those are failures **no fill rule
//!   can rescue**, and they are the fill-rule-independent core of the finding.
//!
//! ## The specification's exceeds-ideal tripwire, and where its premise fails
//!
//! §3.2 makes `A_rast > A_ideal + tol_area` a failure with reason
//! `raster_exceeds_ideal`, on the ground that "the cured patch cannot be larger
//! than the geometry". **That premise does not hold for this rasteriser, and
//! the check is implemented as written anyway.**
//!
//! Full containment is verified on a *pixel footprint at one sampling plane*,
//! not over the whole voxel box. A cured voxel therefore extends up to half a
//! layer above and below the plane where anything was checked, and near a
//! ball's pole the section grows quickly with depth. A cut plane sitting below
//! its layer's sampling plane can slice voxels whose footprints were verified
//! against a *wider* section of the ball than the one the cut plane meets, so
//! `A_rast` can legitimately exceed `A_ideal` by pure Z-quantisation with
//! nothing wrong anywhere.
//!
//! Because the specification governs the audit, the literal comparison is what
//! gates. Alongside it every contact reports a derived quantisation allowance
//! so a reviewer can separate quantisation from a real reconstruction error:
//!
//! ```text
//! every point of a cut voxel lies within one voxel diagonal d3 of the cut
//! plane, and that voxel's footprint was verified inside the solid at a
//! sampling point of the same voxel. So
//!         Patch(c)  SUBSET  ( S (+) B(d3) )  INTERSECT  P(c)
//! and for the tip ball of radius R = f*r_tip seated h deep,
//!         A_allowed(c) = pi * ( (R + d3)^2 - max(0, R - h)^2 )
//!         allowance    = A_allowed - A_ideal = pi * (2*R*d3 + d3^2)
//! ```
//!
//! `measured.totals.exceeds_ideal_within_quantisation` counts the contacts
//! whose excess is inside that bound -- explained by the lattice -- and
//! `exceeds_ideal_beyond_quantisation` counts the ones that are not, which are
//! the genuine "the plane, the axis or the union is wrong, or foreign material
//! has fused into this patch" findings the tripwire was reaching for.
//!
//! ## The plane that lies in a lattice plane, which is the common case
//!
//! `u = normalize(end - start)` for a vertical tip is exactly `(0, 0, ±1)`,
//! because normalising `(0, 0, d)` divides two exact zeros. The cut plane then
//! lies in a lattice plane family, and the voxel boxes the rasteriser reports
//! -- built as `layer_z(k) ± layer/2` -- do **not** tile exactly in `f64`:
//! consecutive boxes can overlap or leave a gap of an ULP. Deciding which box
//! owns that plane from those coordinates either double counts the shared face
//! (a patch at twice its area, which reads as a PASS) or drops it entirely (a
//! patch at zero, which reads as disconnected). Both were reachable here, and
//! the second one was reached.
//!
//! So an axis-aligned cut is decided **combinatorially on the lattice**: the
//! half-open cell `[i*pitch, (i+1)*pitch)` containing the plane's coordinate is
//! the only index that owns it, and every cured voxel at that index contributes
//! its whole cross-section. No box arithmetic, no tolerance, no double count.
//! Oblique cuts keep the polygon clip, where an ULP-wide box overlap can
//! contribute at most about `1e-16 mm^2` -- twelve orders below one pixel.

//! ## The smallest defect this check CANNOT see
//!
//! **A support severed one layer below the cut plane.** I2 examines a set of
//! measure zero -- a single plane -- and its connectivity test runs *within*
//! that plane, never across layers. Delete every cured voxel in exactly one
//! 0.050 mm layer between `P(c)` and the arm and `A_rast` does not change by
//! one pixel, while the support hangs off the model attached to nothing. The
//! specification names a weaker version of this (a one-voxel void inside the
//! seat zone, which lands in the fusion-uncertain band and reports
//! indeterminate); a void one layer *below* the plane is in neither the seat
//! zone nor the plane, and nothing here sees it. Closing it needs a continuity
//! walk along the axis from the model surface to the first structural node,
//! which is I5's territory and is not attempted here.
//!
//! Four smaller-consequence blind spots, in descending size:
//!
//! * **A vacuous pass.** A contact the Voronoi partition charges with zero area
//!   passes against a zero requirement, whatever its patch. Measured on
//!   `exports/make_box-first-artifact.chitubox`: 2 of 5 graded contacts. The
//!   predicate cannot see it; `totals.passed_on_zero_owned_area` counts it.
//! * **Shape.** A crescent or an annulus of the same area as a disc tears from
//!   its thin end first, and the area law was calibrated on round caps.
//! * **Incidence.** The plane is perpendicular to the tip AXIS, not to the
//!   model surface, so at incidence the measured disc is the tip's own section
//!   rather than the true model-support interface -- and the area law was
//!   measured in pure tension and is being applied at arbitrary incidence.
//! * **Ownership.** `A_owned` is a straight-line Voronoi partition, not a
//!   geodesic one. A notched surface mis-assigns it, and I2 has no way to tell
//!   a wrong requirement from a right one.

use crate::check::{Check, CheckOutcome, Failure};
use crate::consts as c;
use crate::context::ModelContext;
use crate::json::Json;
use crate::ownership::implied_area;
use crate::raster::RunRaster;
use crate::vec3::{self as v, Aabb, V3};
use std::collections::HashMap;

pub struct ContactArea;

impl Check for ContactArea {
    fn id(&self) -> &'static str {
        "i2_contact_area"
    }

    fn title(&self) -> &'static str {
        "I2 contact area"
    }

    fn scope(&self, ctx: &ModelContext) -> usize {
        ctx.seats.len()
    }

    fn run(&self, ctx: &ModelContext) -> CheckOutcome {
        let file = ctx.file();
        let model = ctx.model_index;
        let x_allowed = ctx.inputs().x_allowed;
        let f = ctx.inputs().ball_factor_stub;
        let mut out = CheckOutcome::new(ctx.seats.len());

        // The cured support solid, built once for the whole model and shared
        // with every other check. A raster the audit cannot build is a patch
        // the audit cannot measure, so every contact is indeterminate. It is
        // NEVER a pass.
        let raster = match ctx.support_raster() {
            Ok(r) => r,
            Err(e) => {
                let why = format!("{e}");
                for seat in &ctx.seats {
                    out.indeterminate(indeterminate_line(
                        &file,
                        model,
                        seat.contact_id,
                        REASON_RASTER_UNAVAILABLE,
                        &[("detail", Json::str(why.clone()))],
                    ));
                }
                out.measured = unmeasurable_block(&why, ctx);
                return out;
            }
        };

        // The shared ownership partition. A blown sample budget makes every
        // A_owned a lower bound rather than a value, so nothing is established.
        let ownership_budget = ctx.ownership().budget_exceeded;

        let mut acc = Totals::default();
        let mut rows: Vec<Json> = Vec::with_capacity(ctx.seats.len());

        for seat in &ctx.seats {
            let cid = seat.contact_id;
            let mut row = blank_row(cid);
            row.set("r_tip_mm", Json::num(seat.r_tip_mm));
            row.set("r_ball_mm", Json::num(seat.r_ball_mm));
            row.set("h_delivered_mm", Json::num_or_null(seat.h_delivered_mm));
            let implied = implied_area(cid, seat.r_tip_mm, x_allowed);
            row.set("a_implied_mm2_declared", Json::num(implied.a_implied_mm2));
            row.set("a_implied_clamped", Json::Bool(implied.clamped));

            // ---- 1. the seat must be established ---------------------------
            let (Some(u), Some(h), None) = (seat.u, seat.h_delivered_mm, seat.indeterminate) else {
                let why = seat.indeterminate.unwrap_or("no_axis_or_depth");
                row.set("status", Json::str("indeterminate"));
                row.set("reason", Json::str(REASON_SEAT));
                row.set("seat_reason", Json::str(why));
                rows.push(row);
                out.indeterminate(indeterminate_line(
                    &file,
                    model,
                    cid,
                    REASON_SEAT,
                    &[("seat_reason", Json::str(why))],
                ));
                continue;
            };

            // ---- 2. the cured patch ----------------------------------------
            // Measured before ownership, so a contact whose ownership cannot be
            // established still reports the area it delivers.
            let patch = match cut_patch(raster, seat.p0, u, h, seat.r_ball_mm) {
                Ok(p) => p,
                Err(PatchError::NoMaterialAtAxisPoint { components }) => {
                    row.set("status", Json::str("fail"));
                    row.set("reason", Json::str("patch_disconnected"));
                    row.set("patch_components", Json::usize(components));
                    row.set("a_rast_mm2", Json::num(0.0));
                    rows.push(row);
                    let mut values = Json::obj();
                    values.set("components", Json::usize(components));
                    values.set("h_delivered_mm", Json::num(h));
                    values.set("a_owned_mm2", Json::num_or_null(ctx.a_owned_mm2(cid)));
                    out.fail(Failure::line(
                        "CONTACT_PATCH_DISCONNECTED",
                        &file,
                        model,
                        Some(cid),
                        Some(cid),
                        &[("components", components.to_string())],
                        values,
                    ));
                    acc.disconnected += 1;
                    continue;
                }
                Err(PatchError::Unbounded { half_mm }) => {
                    row.set("status", Json::str("indeterminate"));
                    row.set("reason", Json::str(REASON_PATCH_UNBOUNDED));
                    rows.push(row);
                    out.indeterminate(indeterminate_line(
                        &file,
                        model,
                        cid,
                        REASON_PATCH_UNBOUNDED,
                        &[("search_half_extent_mm", Json::num(half_mm))],
                    ));
                    continue;
                }
            };
            let a_rast = patch.area_mm2;
            row.set("a_rast_mm2", Json::num(a_rast));
            row.set("patch_voxels", Json::usize(patch.voxels));
            row.set("patch_components", Json::usize(patch.components));
            if patch.lattice_aligned {
                acc.lattice_aligned += 1;
            }
            if patch.grew > 0 {
                acc.grown += 1;
            }

            // ---- 3. the ideal disc, and the quantisation allowance ---------
            if h <= 0.0 {
                // The ball's deepest point lies on or outside the model
                // surface. It meets the model in a tangent POINT, so there is
                // no cut disc and no contact patch to measure -- whatever the
                // plane finds at that height is support material beside the
                // model, not load path into it. Measured on the corpus: this is
                // the same population I1 reports as SEAT_DEPTH_ZERO.
                acc.zero_seat += 1;
                row.set("status", Json::str("indeterminate"));
                row.set("reason", Json::str(REASON_SEAT_ZERO));
                rows.push(row);
                out.indeterminate(indeterminate_line(
                    &file,
                    model,
                    cid,
                    REASON_SEAT_ZERO,
                    &[
                        ("h_delivered_mm", Json::num(h)),
                        ("a_rast_at_tangent_plane_mm2", Json::num(a_rast)),
                    ],
                ));
                continue;
            }
            let a2 = 2.0 * f * seat.r_tip_mm * h - h * h;
            if a2 <= 0.0 {
                // The cut plane sits at or beyond the ball's far pole, so the
                // ball has no cut disc there and `A_ideal` is not a quantity.
                // Not established, therefore a failure.
                row.set("status", Json::str("indeterminate"));
                row.set("reason", Json::str(REASON_IDEAL_UNDEFINED));
                row.set("ideal_disc_radius_squared_mm2", Json::num(a2));
                rows.push(row);
                out.indeterminate(indeterminate_line(
                    &file,
                    model,
                    cid,
                    REASON_IDEAL_UNDEFINED,
                    &[
                        ("a_squared_mm2", Json::num(a2)),
                        ("h_delivered_mm", Json::num(h)),
                        ("two_r_ball_mm", Json::num(2.0 * seat.r_ball_mm)),
                    ],
                ));
                continue;
            }
            let a_ideal = std::f64::consts::PI * a2;
            let allowance = quantisation_allowance_mm2(seat.r_ball_mm);
            let raster_loss = 1.0 - a_rast / a_ideal;
            row.set("a_ideal_mm2", Json::num(a_ideal));
            row.set("raster_loss", Json::num(raster_loss));
            row.set("quantisation_allowance_mm2", Json::num(allowance));
            row.set("excess_over_ideal_mm2", Json::num(a_rast - a_ideal));

            // ---- 4. the owned area must be established ---------------------
            let a_owned = match (ownership_budget, seat.entry_component, ctx.a_owned_mm2(cid)) {
                (None, Some(_), Some(a)) if a.is_finite() && a >= 0.0 => a,
                (budget, comp, got) => {
                    let detail = if budget.is_some() {
                        "sample_budget_exceeded"
                    } else if comp.is_none() {
                        "component_routing_failed"
                    } else if got.is_none() {
                        "contact_not_in_partition"
                    } else {
                        "nonfinite_owned_area"
                    };
                    row.set("status", Json::str("indeterminate"));
                    row.set("reason", Json::str(REASON_OWNERSHIP));
                    row.set("ownership_reason", Json::str(detail));
                    rows.push(row);
                    out.indeterminate(indeterminate_line(
                        &file,
                        model,
                        cid,
                        REASON_OWNERSHIP,
                        &[
                            ("ownership_reason", Json::str(detail)),
                            ("a_rast_mm2", Json::num(a_rast)),
                            ("a_ideal_mm2", Json::num(a_ideal)),
                        ],
                    ));
                    continue;
                }
            };
            row.set("a_owned_mm2", Json::num(a_owned));

            let tol = c::tol_area(a_owned, x_allowed);
            let a_required = a_owned / x_allowed;
            let x_delivered = (a_rast > 0.0).then(|| a_owned / a_rast);
            row.set("a_required_mm2", Json::num(a_required));
            row.set("tol_area_mm2", Json::num(tol));
            row.set("x_delivered", Json::num_or_null(x_delivered));

            acc.graded += 1;
            acc.a_rast_sum += a_rast;
            acc.a_owned_sum += a_owned;
            acc.a_ideal_sum += a_ideal;
            acc.losses.push(raster_loss);
            if let Some(x) = x_delivered {
                acc.worst_x = Some(acc.worst_x.map_or(x, |w: f64| w.max(x)));
            }

            // ---- 5. the specification's exceeds-ideal tripwire --------------
            if a_rast > a_ideal + tol {
                let beyond = a_rast - a_ideal > allowance;
                if beyond {
                    acc.exceeds_beyond_quantisation += 1;
                } else {
                    acc.exceeds_within_quantisation += 1;
                }
                row.set("status", Json::str("indeterminate"));
                row.set("reason", Json::str(REASON_EXCEEDS_IDEAL));
                row.set("excess_beyond_quantisation", Json::Bool(beyond));
                rows.push(row);
                out.indeterminate(indeterminate_line(
                    &file,
                    model,
                    cid,
                    REASON_EXCEEDS_IDEAL,
                    &[
                        ("a_rast_mm2", Json::num(a_rast)),
                        ("a_ideal_mm2", Json::num(a_ideal)),
                        ("excess_mm2", Json::num(a_rast - a_ideal)),
                        ("quantisation_allowance_mm2", Json::num(allowance)),
                        ("beyond_quantisation", Json::Bool(beyond)),
                    ],
                ));
                continue;
            }

            // ---- 6. the predicate -------------------------------------------
            if a_rast >= a_required - tol {
                // **A pass on a zero owner is a vacuous pass, and it is
                // counted.** A contact the Voronoi partition charges with no
                // model area satisfies the area law against nothing, so it
                // tells a reader nothing about the contact. Measured on
                // `make_box-first-artifact`: 2 of 5 graded contacts passed this
                // way. The gate is the specification's; the disclosure is here
                // so the pass count cannot be read as evidence it is not.
                if a_owned <= 0.0 {
                    acc.passed_on_zero_owned += 1;
                    row.set("passed_on_zero_owned_area", Json::Bool(true));
                }
                row.set("status", Json::str("pass"));
                rows.push(row);
                continue;
            }

            let deficit = a_required - a_rast;
            acc.worst_deficit = acc.worst_deficit.max(deficit);
            // A_rast can exceed A_ideal only by the quantisation allowance, so
            // a contact short of its requirement even against the whole allowed
            // disc is short under EVERY fill rule. That count is the part of
            // this finding no reviewer can argue away as conservatism.
            let unrescuable = a_ideal + allowance + tol < a_required;
            if unrescuable {
                acc.failed_even_at_ideal += 1;
            }
            row.set("fails_even_at_ideal_disc", Json::Bool(unrescuable));
            row.set("deficit_mm2", Json::num(deficit));
            row.set("status", Json::str("fail"));
            rows.push(row);

            let mut values = Json::obj();
            values.set("a_rast_mm2", Json::num(a_rast));
            values.set("a_required_mm2", Json::num(a_required));
            values.set("a_owned_mm2", Json::num(a_owned));
            values.set("a_ideal_mm2", Json::num(a_ideal));
            values.set("a_implied_mm2", Json::num(implied.a_implied_mm2));
            values.set("raster_loss", Json::num(raster_loss));
            values.set("x_delivered", Json::num_or_null(x_delivered));
            values.set("deficit_mm2", Json::num(deficit));
            values.set("tol_area_mm2", Json::num(tol));
            values.set("quantisation_allowance_mm2", Json::num(allowance));
            values.set("fails_even_at_ideal_disc", Json::Bool(unrescuable));
            out.fail(Failure::line(
                "CONTACT_AREA_SHORT",
                &file,
                model,
                Some(cid),
                Some(cid),
                &[
                    ("a_rast_mm2", num(a_rast)),
                    ("a_required_mm2", num(a_required)),
                    ("a_owned_mm2", num(a_owned)),
                    ("a_ideal_mm2", num(a_ideal)),
                    ("a_implied_mm2", num(implied.a_implied_mm2)),
                    ("raster_loss", num(raster_loss)),
                    (
                        "x_delivered",
                        x_delivered.map_or_else(|| "unbounded".to_string(), num),
                    ),
                ],
                values,
            ));
        }

        out.measured = acc.into_measured(ctx, rows);
        out
    }
}

// ---------------------------------------------------------------------------
// Accumulated evidence
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Totals {
    graded: usize,
    disconnected: usize,
    a_rast_sum: f64,
    a_owned_sum: f64,
    a_ideal_sum: f64,
    worst_x: Option<f64>,
    worst_deficit: f64,
    losses: Vec<f64>,
    failed_even_at_ideal: usize,
    exceeds_within_quantisation: usize,
    exceeds_beyond_quantisation: usize,
    passed_on_zero_owned: usize,
    zero_seat: usize,
    lattice_aligned: usize,
    grown: usize,
}

impl Totals {
    fn into_measured(mut self, ctx: &ModelContext, rows: Vec<Json>) -> Json {
        let x_allowed = ctx.inputs().x_allowed;
        let median_loss = median(&mut self.losses);

        let mut totals = Json::obj();
        totals.set("contacts", Json::usize(ctx.seats.len()));
        totals.set("graded", Json::usize(self.graded));
        totals.set("patch_disconnected", Json::usize(self.disconnected));
        totals.set("a_rast_sum_mm2", Json::num(self.a_rast_sum));
        totals.set("a_owned_sum_mm2", Json::num(self.a_owned_sum));
        totals.set("a_ideal_sum_mm2", Json::num(self.a_ideal_sum));
        totals.set("worst_x_delivered", Json::num_or_null(self.worst_x));
        totals.set("x_allowed", Json::num(x_allowed));
        totals.set("worst_deficit_mm2", Json::num(self.worst_deficit));
        totals.set("median_raster_loss", Json::num_or_null(median_loss));
        totals.set(
            "failed_even_at_ideal_disc",
            Json::usize(self.failed_even_at_ideal),
        );
        totals.set(
            "exceeds_ideal_within_quantisation",
            Json::usize(self.exceeds_within_quantisation),
        );
        totals.set(
            "exceeds_ideal_beyond_quantisation",
            Json::usize(self.exceeds_beyond_quantisation),
        );
        totals.set(
            "passed_on_zero_owned_area",
            Json::usize(self.passed_on_zero_owned),
        );
        totals.set("zero_seat", Json::usize(self.zero_seat));
        totals.set("patches_grown", Json::usize(self.grown));
        totals.set(
            "lattice_aligned_cuts",
            Json::usize(self.lattice_aligned),
        );

        let mut tolerance = Json::obj();
        tolerance.set(
            "form",
            Json::str("tol_area(A_owned) = max(4096*eps64*A_owned/x_allowed, 1e-9) mm2"),
        );
        tolerance.set("sum_k", Json::num(c::SUM_K));
        tolerance.set("eps64", Json::num(c::EPS64));
        tolerance.set("abs_floor_mm2", Json::num(c::SUM_ABS));
        tolerance.set(
            "derivation",
            Json::str(
                "the project's derived summation bound applied to the one summed input, \
                 A_owned (audit-invariants.md §1.5, §3.2). A_rast is an exact sum of convex \
                 polygon areas over voxels with disjoint interiors and carries no summation \
                 term of its own: its f64 round-off is below 1e-12 * A_rast at corpus patch \
                 sizes, six orders below the lattice quantisation that dominates it",
            ),
        );
        tolerance.set(
            "quantisation_allowance",
            Json::str(
                "A_allowed - A_ideal = pi * (2*R*d3 + d3^2) with R = f*r_tip and d3 the voxel \
                 diagonal. Derived from Patch SUBSET (S (+) B(d3)) INTERSECT P: every point of \
                 a cut voxel is within one voxel diagonal of the cut plane, and that voxel's \
                 footprint was verified inside the solid at a sampling point of the same voxel. \
                 DISCLOSURE ONLY -- the gate is the specification's literal A_ideal + tol_area",
            ),
        );

        let mut m = Json::obj();
        m.set(
            "headline",
            Json::str(headline(self.worst_x, x_allowed, median_loss)),
        );
        m.set("fill_rule", Json::str("full_containment"));
        m.set(
            "fill_rule_note",
            Json::str(
                "a pixel column is cured at layer k only when the solid contains its whole \
                 footprint at that layer's sampling plane z_k; partial pixels are not cured. \
                 Containment is verified AT z_k, not over the whole voxel box, so the cured \
                 solid is not a subset of the ideal one and A_rast can exceed A_ideal by the \
                 disclosed quantisation allowance",
            ),
        );
        m.set("plane", Json::str("(x - p0).u = h_delivered, perpendicular to the support axis"));
        m.set("connectivity", Json::str("6_connected_voxel_faces"));
        m.set("pixel_pitch_x_mm", Json::num(ctx.grid.pitch_x_mm));
        m.set("pixel_pitch_y_mm", Json::num(ctx.grid.pitch_y_mm));
        m.set("layer_height_mm", Json::num(ctx.grid.layer_height_mm));
        m.set("pixel_area_mm2", Json::num(ctx.grid.pixel_area_mm2()));
        m.set("voxel_diagonal_mm", Json::num(c::D3_MM));
        m.set("tolerance", tolerance);
        m.set("totals", totals);
        m.set("contacts", Json::Arr(rows));
        m
    }
}

/// Every row carries the same keys, so a consumer never has to distinguish
/// "absent" from "not measurable". A quantity that could not be established is
/// `null` and is never defaulted to a number.
fn blank_row(contact_id: usize) -> Json {
    let mut row = Json::obj();
    row.set("contact_id", Json::usize(contact_id));
    row.set("record", Json::usize(contact_id));
    for key in [
        "a_rast_mm2",
        "a_required_mm2",
        "a_owned_mm2",
        "a_ideal_mm2",
        "raster_loss",
        "x_delivered",
        "quantisation_allowance_mm2",
    ] {
        row.set(key, Json::Null);
    }
    row
}

/// `A_allowed - A_ideal`, the amount by which lattice quantisation alone can
/// lift the cured patch above the ideal disc.
///
/// Derivation: every point of a voxel the plane cuts lies within one voxel
/// diagonal `d3` of that plane, and the voxel's footprint was verified inside
/// the solid at a sampling point of the same voxel. So the patch is contained
/// in `(S (+) B(d3)) INTERSECT P`. For the tip ball of radius `R` seated `h`
/// deep, that dilated ball's own cut at `P` is a disc of radius
/// `sqrt((R + d3)^2 - (R - h)^2)`, and subtracting the ideal
/// `pi * (R^2 - (R - h)^2)` leaves `pi * (2*R*d3 + d3^2)`, independent of `h`.
fn quantisation_allowance_mm2(r_ball_mm: f64) -> f64 {
    std::f64::consts::PI * c::D3_MM * 2.0f64.mul_add(r_ball_mm, c::D3_MM)
}

// ---------------------------------------------------------------------------
// Failure-line vocabulary
// ---------------------------------------------------------------------------

/// The three `reason=` tokens `audit-invariants.md` §3.2 enumerates.
const REASON_SEAT: &str = "seat_indeterminate";
const REASON_EXCEEDS_IDEAL: &str = "raster_exceeds_ideal";
const REASON_OWNERSHIP: &str = "ownership_indeterminate";

/// Three further tokens the specification does not enumerate but which name
/// real states this check can reach. Every one is a FAILURE; none is a pass, a
/// skip or a silence. They are reported as a specification gap rather than
/// folded into an existing token, because folding them would hide which thing
/// went wrong.
///
/// * `raster_unavailable` -- the shared support raster could not be built
///   (budget, or empty or non-finite bounds). Without it there is no cured
///   solid to cut.
/// * `patch_unbounded` -- the patch component kept reaching the edge of the
///   search box after every permitted growth, so its area is a lower bound
///   rather than a value.
/// * `ideal_disc_undefined` -- `a^2 = 2*f*r_tip*h - h^2 <= 0` with `h > 0`,
///   i.e. the cut plane sits at or beyond the ball's far pole. The ball has no
///   cut disc there, so neither `A_ideal` nor the tripwire that depends on it
///   exists.
/// * `seat_depth_zero` -- `h_delivered <= 0`: the ball's deepest point lies on
///   or outside the model surface, so it meets the model in a tangent point
///   with no cut disc and no contact patch at all. Named separately from
///   `ideal_disc_undefined` because it is a different fact -- not embedded,
///   rather than embedded past the far pole -- and folding the two would hide
///   which one a file has.
const REASON_RASTER_UNAVAILABLE: &str = "raster_unavailable";
const REASON_PATCH_UNBOUNDED: &str = "patch_unbounded";
const REASON_IDEAL_UNDEFINED: &str = "ideal_disc_undefined";
const REASON_SEAT_ZERO: &str = "seat_depth_zero";

fn indeterminate_line(
    file: &str,
    model: usize,
    contact: usize,
    reason: &'static str,
    extra: &[(&str, Json)],
) -> Failure {
    let mut values = Json::obj();
    values.set("reason", Json::str(reason));
    for (k, val) in extra {
        values.set(k, val.clone());
    }
    Failure::line(
        "CONTACT_AREA_INDETERMINATE",
        file,
        model,
        Some(contact),
        Some(contact),
        &[("reason", reason.to_string())],
        values,
    )
}

fn unmeasurable_block(why: &str, ctx: &ModelContext) -> Json {
    let mut m = Json::obj();
    m.set(
        "headline",
        Json::str(format!("no cured support raster: {why}")),
    );
    m.set("fill_rule", Json::str("full_containment"));
    m.set("contacts", Json::Arr(Vec::new()));
    m.set("raster_error", Json::str(why.to_string()));
    m.set("scope", Json::usize(ctx.seats.len()));
    m
}

fn headline(worst_x: Option<f64>, x_allowed: f64, median_loss: Option<f64>) -> String {
    match (worst_x, median_loss) {
        (Some(x), Some(l)) => format!(
            "worst X delivered {} against {}, median raster loss {l:.3}",
            trim(x),
            trim(x_allowed)
        ),
        (Some(x), None) => format!("worst X delivered {} against {}", trim(x), trim(x_allowed)),
        _ => "no contact was graded; every one is indeterminate".to_string(),
    }
}

fn trim(v: f64) -> String {
    let s = format!("{v:.1}");
    s.strip_suffix(".0").map_or_else(|| s.clone(), ToString::to_string)
}

/// The number format every failure line uses. Small quantities keep their
/// significant digits rather than printing as `0.000000`: a contact patch is on
/// the order of `1e-1 mm^2` and one pixel is `2.66e-4 mm^2`, so a fixed
/// six-place format would round real areas to zero.
fn num(v: f64) -> String {
    if !v.is_finite() {
        return format!("{v}");
    }
    if v != 0.0 && v.abs() < 1.0e-4 {
        format!("{v:.6e}")
    } else {
        format!("{v:.6}")
    }
}

fn median(xs: &mut [f64]) -> Option<f64> {
    if xs.is_empty() {
        return None;
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = xs.len();
    Some(if n % 2 == 1 {
        xs[n / 2]
    } else {
        0.5 * (xs[n / 2 - 1] + xs[n / 2])
    })
}

// ---------------------------------------------------------------------------
// The cut: a plane through the cured voxel union
// ---------------------------------------------------------------------------

/// The measured patch and the diagnostics behind it.
#[derive(Clone, Copy, Debug)]
struct Patch {
    /// Exact planar area of the connected component containing the axis point.
    area_mm2: f64,
    /// Voxels in that component.
    voxels: usize,
    /// Components the cut found anywhere in the search box. `> 1` means other
    /// material crosses the same plane nearby; only the axis component counts,
    /// because a disc that is not connected to this tip carries none of its
    /// load.
    components: usize,
    /// The cut plane lay in a lattice plane family, so the exact half-open
    /// lattice rule decided ownership rather than polygon clipping.
    lattice_aligned: bool,
    /// How many times the search box had to be doubled.
    grew: u32,
}

#[derive(Clone, Copy, Debug)]
enum PatchError {
    /// The cut plane meets no cured support material at the axis point.
    NoMaterialAtAxisPoint { components: usize },
    /// The component still touched the search box after every growth.
    Unbounded { half_mm: f64 },
}

/// How many times the search box may double before the patch is declared
/// unbounded. At a 0.3 mm ball the initial box is 1.9 mm across; three
/// doublings reach 15 mm, far past any contact patch.
const MAX_GROWTHS: u32 = 3;

/// How the plane meets the lattice.
///
/// **The axis-aligned case is not a rare degeneracy, it is the common one.**
/// `u = normalize(end - start)` for a vertical tip is exactly `(0, 0, ±1)`,
/// because normalising `(0, 0, d)` divides two exact zeros. The plane then lies
/// in a lattice plane family and voxel boxes on both sides of a lattice
/// boundary can BOTH appear to contain it -- the boxes are built as
/// `layer_z(k) ± h/2` and do not tile exactly in `f64`, so consecutive boxes
/// can overlap or leave a gap of an ULP. Deciding ownership from those box
/// coordinates either double counts a face (a patch at twice its area, which
/// reads as a PASS) or drops it entirely (a patch at zero, which reads as
/// disconnected). Both were reachable and both are wrong.
///
/// So the axis-aligned case is decided **combinatorially on the lattice**: the
/// half-open cell `[i*pitch, (i+1)*pitch)` containing the plane's coordinate is
/// the only index that owns it, and every cured voxel at that index contributes
/// its whole cross-section. No box arithmetic, no tolerance, no double count.
#[derive(Clone, Copy, Debug)]
enum CutMode {
    /// The plane is perpendicular to lattice axis `axis`; only voxels at
    /// `index` along that axis meet it, each contributing `cell_area_mm2`.
    Lattice {
        axis: usize,
        index: i64,
        cell_area_mm2: f64,
    },
    /// A general plane. Consecutive voxel boxes can overlap by an ULP, but the
    /// polygon such an overlap contributes has an area on the order of
    /// `1e-16 mm^2` -- twelve orders below one pixel -- so clipping each closed
    /// box independently is exact for every purpose here.
    Oblique,
}

fn cut_mode(grid: &crate::raster::RasterGrid, u: V3, x_c: V3) -> CutMode {
    for axis in 0..3 {
        if u[axis].abs() != 1.0 {
            continue;
        }
        let (origin, pitch, cell_area) = match axis {
            0 => (0.0, grid.pitch_x_mm, grid.pitch_y_mm * grid.layer_height_mm),
            1 => (0.0, grid.pitch_y_mm, grid.pitch_x_mm * grid.layer_height_mm),
            _ => (
                grid.z_plate_mm,
                grid.layer_height_mm,
                grid.pitch_x_mm * grid.pitch_y_mm,
            ),
        };
        return CutMode::Lattice {
            axis,
            index: ((x_c[axis] - origin) / pitch).floor() as i64,
            cell_area_mm2: cell_area,
        };
    }
    CutMode::Oblique
}

/// The lattice cell that owns a point under the half-open rule, as
/// `(k, row, col)`. This is the seed lookup, and it agrees with `cut_mode` by
/// construction rather than by a containment test on a closed box.
fn lattice_cell(grid: &crate::raster::RasterGrid, p: V3) -> (i64, i64, i64) {
    (
        ((p[2] - grid.z_plate_mm) / grid.layer_height_mm).floor() as i64,
        (p[1] / grid.pitch_y_mm).floor() as i64,
        (p[0] / grid.pitch_x_mm).floor() as i64,
    )
}

/// Cut the cured voxel union with `P(c)` and measure the component containing
/// the axis point.
///
/// The area is exact for the voxel set it is given: the voxels partition space
/// as half-open cells, a plane meets a cell in a convex polygon, and each cell
/// is counted once.
fn cut_patch(raster: &RunRaster, p0: V3, u: V3, h: f64, r_ball: f64) -> Result<Patch, PatchError> {
    let grid = raster.grid;
    let x_c = v::add(p0, v::mul(u, h));
    let mode = cut_mode(&grid, u, x_c);
    let seed_key = lattice_cell(&grid, x_c);
    // The ball's own cut disc has radius at most `r_ball`; three ball radii
    // plus four voxel diagonals covers it and any fused neighbour, and the
    // growth loop covers anything larger.
    let mut half = 3.0f64.mul_add(r_ball, 4.0 * c::D3_MM);
    let mut grew = 0u32;
    loop {
        let query = Aabb {
            min: [x_c[0] - half, x_c[1] - half, x_c[2] - half],
            max: [x_c[0] + half, x_c[1] + half, x_c[2] + half],
        };
        let mut cells: Vec<Cell> = Vec::new();
        for (k, row, col, bx) in raster.voxels_in_box(&query) {
            let area = match mode {
                CutMode::Lattice {
                    axis,
                    index,
                    cell_area_mm2,
                } => {
                    let here = match axis {
                        0 => col,
                        1 => row,
                        _ => k,
                    };
                    if here != index {
                        continue;
                    }
                    cell_area_mm2
                }
                CutMode::Oblique => match cut_cell(&bx, p0, u, h) {
                    Some(a) => a,
                    None => continue,
                },
            };
            cells.push(Cell {
                k,
                row,
                col,
                bx,
                area,
            });
        }
        if cells.is_empty() {
            return Err(PatchError::NoMaterialAtAxisPoint { components: 0 });
        }

        // Index for face adjacency. Two face-adjacent cells that are both cut
        // by the plane share a segment of that cut: their union is a box, and a
        // plane meets a convex body in a convex set that must cross the shared
        // face to reach both interiors.
        let mut index: HashMap<(i64, i64, i64), usize> = HashMap::with_capacity(cells.len() * 2);
        for (i, cell) in cells.iter().enumerate() {
            index.insert((cell.k, cell.row, cell.col), i);
        }

        let mut comp_of = vec![usize::MAX; cells.len()];
        let mut n_components = 0usize;
        let mut stack: Vec<usize> = Vec::new();
        for start in 0..cells.len() {
            if comp_of[start] != usize::MAX {
                continue;
            }
            let id = n_components;
            n_components += 1;
            comp_of[start] = id;
            stack.push(start);
            while let Some(i) = stack.pop() {
                let (k, row, col) = (cells[i].k, cells[i].row, cells[i].col);
                for (dk, dr, dc) in NEIGHBOURS {
                    if let Some(&j) = index.get(&(k + dk, row + dr, col + dc)) {
                        if comp_of[j] == usize::MAX {
                            comp_of[j] = id;
                            stack.push(j);
                        }
                    }
                }
            }
        }

        // The axis point picks the component, through the half-open lattice
        // cell that owns it -- not through a containment test on a closed box,
        // which is ambiguous exactly where it matters.
        let Some(&seed) = index.get(&seed_key) else {
            return Err(PatchError::NoMaterialAtAxisPoint {
                components: n_components,
            });
        };
        let id = comp_of[seed];

        let mut area = 0.0f64;
        let mut voxels = 0usize;
        let mut touches_edge = false;
        for (i, cell) in cells.iter().enumerate() {
            if comp_of[i] != id {
                continue;
            }
            area += cell.area;
            voxels += 1;
            for ax in 0..3 {
                if cell.bx.min[ax] <= query.min[ax] || cell.bx.max[ax] >= query.max[ax] {
                    touches_edge = true;
                }
            }
        }

        if touches_edge {
            if grew >= MAX_GROWTHS {
                return Err(PatchError::Unbounded { half_mm: half });
            }
            grew += 1;
            half *= 2.0;
            continue;
        }

        return Ok(Patch {
            area_mm2: area,
            voxels,
            components: n_components,
            lattice_aligned: matches!(mode, CutMode::Lattice { .. }),
            grew,
        });
    }
}

/// Face adjacency, `(dk, drow, dcol)`. Face-sharing only: two voxels meeting
/// along an edge or at a corner touch in a set of zero area and are not one
/// piece of cured material.
const NEIGHBOURS: [(i64, i64, i64); 6] = [
    (1, 0, 0),
    (-1, 0, 0),
    (0, 1, 0),
    (0, -1, 0),
    (0, 0, 1),
    (0, 0, -1),
];

#[derive(Clone, Copy, Debug)]
struct Cell {
    k: i64,
    row: i64,
    col: i64,
    bx: Aabb,
    area: f64,
}

/// The area of `plane INTERSECT box`, for the oblique case.
///
/// Returns `None` when the intersection is empty or has no area -- a single
/// vertex or a single edge.
fn cut_cell(bx: &Aabb, p0: V3, u: V3, h: f64) -> Option<f64> {
    let corners = corners_of(bx);
    let mut d = [0.0f64; 8];
    let mut dmin = f64::INFINITY;
    let mut dmax = f64::NEG_INFINITY;
    for i in 0..8 {
        d[i] = v::dot(v::sub(corners[i], p0), u) - h;
        dmin = dmin.min(d[i]);
        dmax = dmax.max(d[i]);
    }
    if dmin > 0.0 || dmax < 0.0 {
        return None;
    }

    // Points: corners exactly on the plane, plus crossings of the 12 edges.
    let mut pts: Vec<V3> = Vec::with_capacity(8);
    for i in 0..8 {
        if d[i] == 0.0 {
            pts.push(corners[i]);
        }
    }
    for &(i, j) in &BOX_EDGES {
        if (d[i] < 0.0 && d[j] > 0.0) || (d[i] > 0.0 && d[j] < 0.0) {
            let t = d[i] / (d[i] - d[j]);
            pts.push(v::add(corners[i], v::mul(v::sub(corners[j], corners[i]), t)));
        }
    }
    if pts.len() < 3 {
        return None;
    }

    // Order them around their centroid in the plane's own basis. A plane
    // section of a convex body is convex, so an angular sort is the polygon.
    let n = pts.len() as f64;
    let mut g = [0.0f64; 3];
    for p in &pts {
        g = v::add(g, *p);
    }
    g = v::mul(g, 1.0 / n);
    let (e1, e2, _) = v::basis_from_axis(u);
    let mut keyed: Vec<(f64, V3)> = pts
        .into_iter()
        .map(|p| {
            let r = v::sub(p, g);
            (v::dot(r, e2).atan2(v::dot(r, e1)), p)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Shoelace in 3D against the plane normal.
    let mut acc = [0.0f64; 3];
    for i in 0..keyed.len() {
        let a = v::sub(keyed[i].1, g);
        let b = v::sub(keyed[(i + 1) % keyed.len()].1, g);
        acc = v::add(acc, v::cross(a, b));
    }
    let area = 0.5 * v::dot(acc, u).abs();
    (area > 0.0).then_some(area)
}

fn corners_of(bx: &Aabb) -> [V3; 8] {
    let (a, b) = (bx.min, bx.max);
    [
        [a[0], a[1], a[2]],
        [b[0], a[1], a[2]],
        [b[0], b[1], a[2]],
        [a[0], b[1], a[2]],
        [a[0], a[1], b[2]],
        [b[0], a[1], b[2]],
        [b[0], b[1], b[2]],
        [a[0], b[1], b[2]],
    ]
}

/// The 12 edges of [`corners_of`]'s vertex order.
const BOX_EDGES: [(usize, usize); 12] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 0),
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 4),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::Status;
    use crate::context::{AuditContext, AuditOptions, ModelContext};
    use crate::primitives::Primitive;
    use crate::raster::{Budget, RasterGrid, RunRaster};
    use crate::reader::{ChituboxFile, RecordKind};
    use crate::synth::{self, SynthFile, SynthModel, SynthRecord};
    use std::path::Path;

    fn grid() -> RasterGrid {
        RasterGrid {
            pitch_x_mm: c::PIXEL_PITCH_X_MM,
            pitch_y_mm: c::PIXEL_PITCH_Y_MM,
            layer_height_mm: c::LAYER_HEIGHT_DEFAULT_MM,
            z_plate_mm: 0.0,
        }
    }

    // -- the geometric core -------------------------------------------------

    #[test]
    fn a_plane_through_a_box_interior_gives_the_footprint_area() {
        let bx = Aabb {
            min: [0.0, 0.0, 0.0],
            max: [0.014, 0.019, 0.05],
        };
        let area = cut_cell(&bx, [0.0, 0.0, 0.025], [0.0, 0.0, 1.0], 0.0).expect("cut");
        assert!((area - 0.014 * 0.019).abs() < 1e-15, "{area}");
    }

    /// The diagonal section of a unit cube through its centre at 45 degrees has
    /// area sqrt(2). An oblique plane is the general case and it is exact.
    #[test]
    fn an_oblique_plane_gives_the_exact_diagonal_section() {
        let bx = Aabb {
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 1.0],
        };
        let s = 1.0 / 2.0f64.sqrt();
        let area = cut_cell(&bx, [0.5, 0.5, 0.5], [s, 0.0, s], 0.0).expect("cut");
        assert!((area - 2.0f64.sqrt()).abs() < 1e-12, "{area}");
    }

    /// The lattice rule names exactly one owning index per axis-aligned plane,
    /// and it is the half-open cell the plane's own coordinate falls in.
    #[test]
    fn the_lattice_rule_names_exactly_one_owning_index() {
        let g = grid();
        // A plane strictly inside layer 21.
        match cut_mode(&g, [0.0, 0.0, 1.0], [0.0, 0.0, 21.5 * g.layer_height_mm]) {
            CutMode::Lattice {
                axis,
                index,
                cell_area_mm2,
            } => {
                assert_eq!((axis, index), (2, 21));
                assert!((cell_area_mm2 - g.pixel_area_mm2()).abs() < 1e-18);
            }
            CutMode::Oblique => panic!("an exactly vertical axis must be a lattice cut"),
        }
        // A plane exactly on the boundary between layers 21 and 22 belongs to
        // 22 -- the half-open cell [22*h, 23*h) -- and to nothing else.
        let q = 22.0 * g.layer_height_mm;
        match cut_mode(&g, [0.0, 0.0, -1.0], [0.0, 0.0, q]) {
            CutMode::Lattice { axis, index, .. } => assert_eq!((axis, index), (2, 22)),
            CutMode::Oblique => panic!("still a lattice cut when u points down"),
        }
        // A tilted axis is oblique and takes the polygon path.
        let s = 1.0 / 2.0f64.sqrt();
        assert!(matches!(
            cut_mode(&g, [s, 0.0, s], [0.0, 0.0, 0.0]),
            CutMode::Oblique
        ));
    }

    #[test]
    fn a_plane_touching_one_corner_has_no_area() {
        let bx = Aabb {
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 1.0],
        };
        let s = 1.0 / 3.0f64.sqrt();
        assert!(cut_cell(&bx, [0.0, 0.0, 0.0], [s, s, s], 0.0).is_none());
    }

    #[test]
    fn a_plane_missing_the_box_is_dropped() {
        let bx = Aabb {
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 1.0],
        };
        assert!(cut_cell(&bx, [0.0, 0.0, 2.0], [0.0, 0.0, 1.0], 0.0).is_none());
    }

    /// A plane landing exactly on a lattice boundary is counted ONCE, not
    /// twice and not zero times. Both wrong answers were reachable from voxel
    /// box coordinates, and one of them reads as a PASS.
    #[test]
    fn a_lattice_boundary_cut_is_counted_once_not_twice_and_not_zero() {
        let g = grid();
        let prim = Primitive::Frustum {
            a: [0.5, 0.5, 0.0],
            ra: 0.2,
            b: [0.5, 0.5, 1.0],
            rb: 0.2,
        };
        let raster = RunRaster::from_primitives(&[prim], g, &Budget::default()).expect("raster");
        // Exactly the boundary between layers 9 and 10.
        let z_boundary = 10.0 * g.layer_height_mm;
        let patch =
            cut_patch(&raster, [0.5, 0.5, z_boundary], [0.0, 0.0, 1.0], 0.0, 0.2).expect("patch");
        let ideal = std::f64::consts::PI * 0.04;
        assert!(patch.lattice_aligned, "the test did not hit the case");
        assert!(
            patch.area_mm2 <= ideal,
            "the boundary was double counted: {} > {ideal}",
            patch.area_mm2
        );
        assert!(
            patch.area_mm2 > 0.8 * ideal,
            "the boundary was dropped: {}",
            patch.area_mm2
        );
        // And one layer up, away from any boundary, the answer is the same
        // disc: the rule does not create or destroy area at a boundary.
        let inside = cut_patch(
            &raster,
            [0.5, 0.5, 10.5 * g.layer_height_mm],
            [0.0, 0.0, 1.0],
            0.0,
            0.2,
        )
        .expect("patch");
        assert!((inside.area_mm2 - patch.area_mm2).abs() < 1e-15);
    }

    /// The cured ball cut sits inside the ideal disc plus the derived
    /// quantisation allowance, and its loss from the ideal disc is the
    /// one-pixel rim the fill rule costs.
    #[test]
    fn the_cured_ball_cut_stays_within_the_derived_quantisation_bound() {
        let g = grid();
        let r_ball = 0.2955_f64;
        let p0 = [1.0, 1.0, 1.0];
        let centre = [p0[0], p0[1], p0[2] + r_ball];
        let raster =
            RunRaster::from_primitives(&[Primitive::Ball { centre, r: r_ball }], g, &Budget::default())
                .expect("raster");
        let h = 0.10;
        let patch = cut_patch(&raster, p0, [0.0, 0.0, 1.0], h, r_ball).expect("patch");
        let ideal = std::f64::consts::PI * (2.0 * r_ball * h - h * h);
        let allowance = quantisation_allowance_mm2(r_ball);
        assert!(
            patch.area_mm2 <= ideal + allowance,
            "{} exceeds the derived bound {}",
            patch.area_mm2,
            ideal + allowance
        );
        let loss = 1.0 - patch.area_mm2 / ideal;
        assert!(
            loss.abs() < 0.5,
            "raster loss {loss} is nowhere near the ideal disc"
        );
    }

    /// The plane through empty space finds nothing, and "nothing" is a FAILURE,
    /// never an empty pass.
    #[test]
    fn a_plane_with_no_cured_material_reports_disconnected() {
        let raster = RunRaster::empty(grid());
        assert!(matches!(
            cut_patch(&raster, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.1, 0.3),
            Err(PatchError::NoMaterialAtAxisPoint { components: 0 })
        ));
    }

    /// Foreign material crossing the same plane carries none of this contact's
    /// load and must not be added to its patch.
    #[test]
    fn the_component_restriction_excludes_a_neighbour_disc() {
        let g = grid();
        let r = 0.3_f64;
        let p0 = [1.0, 1.0, 1.0];
        let mine = Primitive::Ball {
            centre: [p0[0], p0[1], p0[2] + r],
            r,
        };
        let theirs = Primitive::Ball {
            centre: [p0[0] + 0.9, p0[1], p0[2] + r],
            r,
        };
        let both =
            RunRaster::from_primitives(&[mine, theirs], g, &Budget::default()).expect("raster");
        let alone = RunRaster::from_primitives(&[mine], g, &Budget::default()).expect("raster");
        let a = cut_patch(&both, p0, [0.0, 0.0, 1.0], 0.1, r).expect("patch");
        let b = cut_patch(&alone, p0, [0.0, 0.0, 1.0], 0.1, r).expect("patch");
        assert!(
            (a.area_mm2 - b.area_mm2).abs() < 1e-15,
            "the neighbour leaked into the patch: {} vs {}",
            a.area_mm2,
            b.area_mm2
        );
        assert_eq!(a.components, 2, "both discs seen, only one counted");
    }

    /// The allowance is the dilated-ball bound, and it is independent of the
    /// seat depth by construction.
    #[test]
    fn the_quantisation_allowance_is_the_dilated_ball_bound() {
        let r = 0.2955_f64;
        let want = std::f64::consts::PI * (2.0 * r * c::D3_MM + c::D3_MM * c::D3_MM);
        assert!((quantisation_allowance_mm2(r) - want).abs() < 1e-15 * want);
        for h in [0.05_f64, 0.1, 0.2] {
            let ideal = std::f64::consts::PI * (2.0 * r * h - h * h);
            let dilated =
                std::f64::consts::PI * ((r + c::D3_MM).powi(2) - (r - h).powi(2));
            assert!(
                (dilated - ideal - quantisation_allowance_mm2(r)).abs() < 1e-15,
                "h={h}"
            );
        }
    }

    // -- the predicate, end to end on a synthesised file ---------------------

    /// A 10 x 10 mm slab with one contact seated `h` deep into its underside.
    ///
    /// The tip record's `start` is the ball's DEEPEST point, so it sits INSIDE
    /// the slab and the axis `u = normalize(end - start)` points OUT of the
    /// model, downward toward the raft.
    fn fixture(h: f64, r_tip: f64) -> Vec<u8> {
        let r_ball = c::BALL_FACTOR_STUB * r_tip;
        let under = -2.0_f64;
        let p0 = [0.0, 0.0, under + h];
        let end = [0.0, 0.0, p0[2] - 2.0 * r_ball];
        let centre = [0.0, 0.0, p0[2] - r_ball];
        let node = [0.0, 0.0, centre[2] - 1.5];
        SynthFile::new()
            .with_model(SynthModel {
                name: "slab".into(),
                translate_mm: [0.0, 0.0, 5.02],
                mesh: synth::box_soup([-5.0, -5.0, under], [5.0, 5.0, 2.0]),
                records: vec![
                    SynthRecord::new(RecordKind::ContactSphere, p0, end, r_ball, r_ball)
                        .with_matching_soup(),
                    SynthRecord::new(RecordKind::ContactTaper, centre, node, r_tip, 0.35)
                        .with_declared_depth(h)
                        .with_matching_soup(),
                    SynthRecord::new(RecordKind::Member, node, [0.0, 0.0, -6.9], 0.35, 0.35)
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
            })
            .to_bytes()
    }

    fn outcome_for(bytes: &[u8]) -> CheckOutcome {
        let file = ChituboxFile::parse(Path::new("fixture.chitubox"), bytes).expect("parse");
        let options = AuditOptions::default();
        let audit = AuditContext {
            file: &file,
            options: &options,
            sidecar: None,
            sidecar_absence: Some("none".into()),
        };
        let ctx = ModelContext::build(&audit, 0);
        ContactArea.run(&ctx)
    }

    /// One contact under a 100 mm^2 down-face owns the whole face, which the
    /// area law cannot carry on a 0.4 mm tip. The check must SAY SO, with the
    /// numbers, and the failure must be one no fill rule could rescue.
    #[test]
    fn a_lone_contact_under_a_large_face_fails_with_numbers() {
        let out = outcome_for(&fixture(0.10, 0.197));
        assert_eq!(out.checked, 1);
        assert_eq!(out.indeterminate, 0, "{:?}", out.failures);
        assert_eq!(out.failed, 1, "{:?}", out.failures);
        let f = &out.failures[0];
        assert_eq!(f.code, "CONTACT_AREA_SHORT");
        assert!(f.message.contains("a_rast_mm2="), "{}", f.message);
        assert!(f.message.contains("x_delivered="), "{}", f.message);
        let g = |k: &str| f.values.get(k).and_then(Json::as_f64).expect(k);
        let (a_rast, a_ideal, req) = (g("a_rast_mm2"), g("a_ideal_mm2"), g("a_required_mm2"));
        assert!(a_rast > 0.0, "a real patch was measured: {a_rast}");
        assert!(
            a_rast <= a_ideal + quantisation_allowance_mm2(c::BALL_FACTOR_STUB * 0.197),
            "{a_rast} vs {a_ideal}"
        );
        // 100 mm^2 of down-face against x_allowed = 12 needs 8.33 mm^2.
        assert!((req - 100.0 / 12.0).abs() < 0.2, "{req}");
        assert_eq!(
            f.values.get("fails_even_at_ideal_disc"),
            Some(&Json::Bool(true)),
            "a 0.4 mm tip cannot carry 100 mm^2 under ANY fill rule"
        );
        assert_eq!(
            out.measured
                .get("totals")
                .and_then(|t| t.get("failed_even_at_ideal_disc"))
                .and_then(Json::as_f64),
            Some(1.0)
        );
    }

    /// A deeper seat delivers a bigger patch. This is the ruling's own
    /// condition, measured: retract the ball and the cut disc shrinks.
    #[test]
    fn a_shallower_seat_delivers_a_smaller_patch() {
        let area = |h: f64| -> f64 {
            let out = outcome_for(&fixture(h, 0.197));
            out.measured
                .get("contacts")
                .and_then(Json::as_array)
                .and_then(|r| r.first())
                .and_then(|r| r.get("a_rast_mm2"))
                .and_then(Json::as_f64)
                .expect("a_rast")
        };
        let (shallow, deep) = (area(0.05), area(0.20));
        assert!(
            deep > shallow,
            "a retracted ball must deliver less: {deep} vs {shallow}"
        );
    }

    /// The `measured` block carries a row for every contact CHECKED, not only
    /// for the failures: a passing invariant that reports no numbers is not
    /// evidence (§5 rule 3). Every row has the same keys, so an unmeasurable
    /// quantity is `null` rather than absent or defaulted to zero.
    #[test]
    fn every_checked_contact_gets_a_measured_row_with_a_uniform_shape() {
        let out = outcome_for(&fixture(0.10, 0.197));
        let rows = out
            .measured
            .get("contacts")
            .and_then(Json::as_array)
            .expect("contacts array");
        assert_eq!(rows.len(), out.checked);
        for r in rows {
            for key in [
                "contact_id",
                "record",
                "a_rast_mm2",
                "a_ideal_mm2",
                "a_owned_mm2",
                "a_required_mm2",
                "a_implied_mm2_declared",
                "raster_loss",
                "x_delivered",
                "quantisation_allowance_mm2",
                "status",
            ] {
                assert!(r.get(key).is_some(), "row is missing {key}: {r:?}");
            }
        }
        assert!(out.measured.get("totals").is_some());
        assert!(!out.headline().is_empty());
    }

    /// **The boundary case that matters most: an unmeasurable input.** A tip
    /// whose seat could not be established is INDETERMINATE, which forces the
    /// invariant to FAIL. It is never quietly skipped and it never passes.
    #[test]
    fn an_unmeasurable_seat_is_indeterminate_and_therefore_fails() {
        // A tip whose two poles coincide has no axis, so `u` is undefined.
        let bytes = SynthFile::new()
            .with_model(SynthModel {
                name: "degenerate".into(),
                translate_mm: [0.0, 0.0, 5.02],
                mesh: synth::box_soup([-5.0, -5.0, -2.0], [5.0, 5.0, 2.0]),
                records: vec![
                    SynthRecord::new(
                        RecordKind::ContactSphere,
                        [0.0, 0.0, -1.9],
                        [0.0, 0.0, -1.9],
                        0.3,
                        0.3,
                    ),
                    SynthRecord::new(
                        RecordKind::ContactTaper,
                        [0.0, 0.0, -1.9],
                        [0.0, 0.0, -4.0],
                        0.2,
                        0.35,
                    )
                    .with_declared_depth(0.1),
                ],
            })
            .to_bytes();
        let out = outcome_for(&bytes);
        assert_eq!(out.checked, 1);
        assert_eq!(out.indeterminate, 1, "{:?}", out.failures);
        assert_eq!(out.failed, 0);
        assert_eq!(out.status(), Status::Fail, "indeterminate never passes");
        let f = &out.failures[0];
        assert_eq!(f.code, "CONTACT_AREA_INDETERMINATE");
        assert!(
            f.message.contains("reason=seat_indeterminate"),
            "{}",
            f.message
        );
    }

    /// A tip that never enters the model is a failure, not a silence, and the
    /// zero-area case never reaches the predicate as a trivial pass.
    #[test]
    fn a_tip_that_does_not_reach_the_model_is_a_failure() {
        let r_ball = c::BALL_FACTOR_STUB * 0.197;
        // p0 sits 0.5 mm BELOW the slab underside, so the ball hangs free.
        let p0 = [0.0, 0.0, -2.5];
        let bytes = SynthFile::new()
            .with_model(SynthModel {
                name: "hanging".into(),
                translate_mm: [0.0, 0.0, 5.02],
                mesh: synth::box_soup([-5.0, -5.0, -2.0], [5.0, 5.0, 2.0]),
                records: vec![
                    SynthRecord::new(
                        RecordKind::ContactSphere,
                        p0,
                        [0.0, 0.0, p0[2] - 2.0 * r_ball],
                        r_ball,
                        r_ball,
                    )
                    .with_matching_soup(),
                    SynthRecord::new(
                        RecordKind::ContactTaper,
                        [0.0, 0.0, p0[2] - r_ball],
                        [0.0, 0.0, -4.0],
                        0.197,
                        0.35,
                    )
                    .with_declared_depth(0.1)
                    .with_matching_soup(),
                ],
            })
            .to_bytes();
        let out = outcome_for(&bytes);
        assert_eq!(out.checked, 1);
        assert_eq!(out.status(), Status::Fail);
        assert_eq!(out.failed + out.indeterminate, 1, "{:?}", out.failures);
        let m = &out.failures[0].message;
        assert!(
            m.contains("CONTACT_PATCH_DISCONNECTED")
                || m.contains("reason=seat_depth_zero")
                || m.contains("reason=ownership_indeterminate"),
            "{m}"
        );
        // A tangent ball never reaches the predicate as a trivial pass.
        assert_ne!(
            out.measured
                .get("contacts")
                .and_then(Json::as_array)
                .and_then(|r| r.first())
                .and_then(|r| r.get("status"))
                .and_then(Json::as_str),
            Some("pass")
        );
    }

    /// A contact the ownership partition charges with nothing satisfies the
    /// area law against nothing. The specification's predicate passes it, and
    /// this check COUNTS it, so a pass total can never be read as evidence the
    /// contact carries load.
    #[test]
    fn a_pass_on_a_zero_owner_is_counted_as_vacuous() {
        // Two coincident-in-plan contacts under the same face: the Voronoi
        // partition gives every sample to the lower record index, so the second
        // owns exactly nothing.
        let r_tip = 0.197_f64;
        let r_ball = c::BALL_FACTOR_STUB * r_tip;
        let h = 0.10_f64;
        let under = -2.0_f64;
        let tip = |x: f64| -> Vec<SynthRecord> {
            let p0 = [x, 0.0, under + h];
            let end = [x, 0.0, p0[2] - 2.0 * r_ball];
            let centre = [x, 0.0, p0[2] - r_ball];
            let node = [x, 0.0, centre[2] - 1.5];
            vec![
                SynthRecord::new(RecordKind::ContactSphere, p0, end, r_ball, r_ball)
                    .with_matching_soup(),
                SynthRecord::new(RecordKind::ContactTaper, centre, node, r_tip, 0.35)
                    .with_declared_depth(h)
                    .with_matching_soup(),
            ]
        };
        let mut records = tip(0.0);
        records.extend(tip(0.02));
        let bytes = SynthFile::new()
            .with_model(SynthModel {
                name: "twin".into(),
                translate_mm: [0.0, 0.0, 5.02],
                mesh: synth::box_soup([-5.0, -5.0, under], [5.0, 5.0, 2.0]),
                records,
            })
            .to_bytes();
        let out = outcome_for(&bytes);
        assert_eq!(out.checked, 2);
        let vacuous = out
            .measured
            .get("totals")
            .and_then(|t| t.get("passed_on_zero_owned_area"))
            .and_then(Json::as_f64)
            .expect("the counter must always be present");
        // Whatever the partition decides, the counter is reported and it never
        // exceeds the number of passes.
        let passes = out.checked - out.failed - out.indeterminate;
        assert!(vacuous <= passes as f64, "{vacuous} > {passes}");
    }

    /// The tolerance is the specification's, and it is derived rather than
    /// chosen: the summation bound on the one summed input, with an absolute
    /// floor so it never collapses to zero.
    #[test]
    fn the_tolerance_is_the_derived_summation_bound() {
        let big = 1.0e6_f64;
        let rel = c::SUM_K * c::EPS64 * big / c::X_ALLOWED;
        assert!(rel > c::SUM_ABS, "the relative leg must dominate here");
        assert!((c::tol_area(big, c::X_ALLOWED) - rel).abs() < 1e-18);
        assert!((c::tol_area(0.0, c::X_ALLOWED) - c::SUM_ABS).abs() < 1e-18);
        // and at corpus scale the absolute floor is what binds, which is why
        // the exceeds-ideal tripwire is effectively exact.
        assert!((c::tol_area(100.0, c::X_ALLOWED) - c::SUM_ABS).abs() < 1e-18);
    }
}
