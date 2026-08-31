//! report metrics -- the non-gating quality report of `docs/specs/audit-invariants.md` §4.
//!
//! **This check never gates.** Its outcome always carries zero failures and
//! zero indeterminates -- `docs/specs/audit-invariants.md` §4 says its verdict
//! field is literally `"disclosed"`, which is not a passing value anywhere in
//! the schema, and `src/checks/mod.rs::report()` keeps it out of
//! `print_candidate` entirely. What this module owes instead is honesty about
//! what each number is and how far it can be trusted: every section carries an
//! `established` flag, and a section that could not be computed says why
//! rather than reporting a zero that looks like a real measurement.
//!
//! ## The seven sections, and where each one's raw material comes from
//!
//! * `spacing` -- nearest-neighbour 3D distance between contact centres
//!   (`ModelContext::seats`, `Seat::centre`), RECONSTRUCTED.
//! * `arm_radius` -- narrowest section of every type-3 `Member` record, taken
//!   as `min(ra, rb)` of its reconstructed [`Primitive::Frustum`] (a straight
//!   frustum's narrowest point is always one of its two ends).
//! * `scar_orientation` -- each contact's `entry_normal.z` component
//!   (`Seat::entry_normal`), bucketed against the plate normal, weighted by
//!   `A_owned` from the shared ownership partition (`ModelContext::ownership`).
//! * `resin_split` -- cured volume of the shared model and support rasters.
//! * `overhang` -- the cantilever exposure I7 does not gate (§3.7): pixel
//!   columns cured at layer `k` whose own column was not cured at `k-1`,
//!   summed and spanned directly from the model raster's run-length rows.
//! * `raster_erosion_sensitivity` -- how many contacts would flip verdict
//!   under a pixel-centre fill rule instead of full containment. **Only I2's
//!   flavour of this is computed here**, by a local proxy explained in that
//!   section's own `i2_method` field; I3 and I6 are reported `established:
//!   false` with a reason, because their predicates live in sibling check
//!   modules this file may not import and reimplementing them here would
//!   grade a different thing than they grade, not cross-check it.
//! * `junction_turn_angles_deg` -- pairwise turn angle between type-3 members
//!   that share a reconstructed connective-point cluster (a union-find over
//!   `SupportGraph::joins`, restricted to `Member` points).
//!
//! Every corpus reference number named in the specification (median spacing
//! 1.03 mm, 36-54 % of arms at 0.35 mm, 28-31 % of scars not down-facing) is a
//! **reference line**, quoted in this module's doc comments only -- it is
//! never compared against as a threshold, because `SPACING_FLOOR_MM` and
//! `TARGET_SPACING_MM` are both `Placeholder` (`docs/specs/audit-invariants.md`
//! §1.3) and this check does not gate regardless.

use crate::check::{Check, CheckOutcome};
use crate::checks::interval::{self, Verdict};
use crate::consts as c;
use crate::context::ModelContext;
use crate::json::Json;
use crate::primitives::Primitive;
use crate::raster::{RasterGrid, Run, RunRaster};
use crate::reader::RecordKind;
use crate::seat::Seat;
use crate::vec3::{self as v, V3};
use std::collections::{BTreeMap, BTreeSet};

pub struct ReportMetrics;

impl Check for ReportMetrics {
    fn id(&self) -> &'static str {
        "report"
    }

    fn title(&self) -> &'static str {
        "report metrics"
    }

    fn scope(&self, _ctx: &ModelContext) -> usize {
        1
    }

    fn run(&self, ctx: &ModelContext) -> CheckOutcome {
        let mut outcome = CheckOutcome::new(1);

        let spacing = spacing_section(ctx);
        let arm_radius = arm_radius_section(ctx);
        let scar_orientation = scar_orientation_section(ctx);
        let resin_split = resin_split_section(ctx);
        let overhang = overhang_section(ctx);
        let raster_erosion_sensitivity = raster_erosion_sensitivity_section(ctx);
        let junction_turn_angles_deg = junction_turn_angles_section(ctx);

        let headline = build_headline(&spacing, &arm_radius, &scar_orientation, &resin_split, &overhang);

        let mut measured = Json::obj();
        measured.set("spacing", spacing);
        measured.set("arm_radius", arm_radius);
        measured.set("scar_orientation", scar_orientation);
        measured.set("resin_split", resin_split);
        measured.set("overhang", overhang);
        measured.set("raster_erosion_sensitivity", raster_erosion_sensitivity);
        measured.set("junction_turn_angles_deg", junction_turn_angles_deg);
        measured.set("headline", Json::str(headline));

        outcome.measured = measured;
        outcome
    }
}

// ---------------------------------------------------------------------------
// Small shared helpers
// ---------------------------------------------------------------------------

/// Linear-interpolation percentile ("R type 7") over an ASCENDING-sorted
/// slice. `p` in `[0, 1]`.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let pos = p.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let frac = pos - lo as f64;
        sorted[lo].mul_add(1.0 - frac, sorted[hi] * frac)
    }
}

/// A histogram over `values` with fixed-width bins starting at zero. Empty
/// bins are OMITTED to keep the array compact when a handful of outliers
/// would otherwise force thousands of empty rows -- a presentational choice
/// the specification does not pin, disclosed here rather than left implicit.
fn histogram(values: &[f64], bin_width: f64, unit: &str) -> Json {
    if values.is_empty() || bin_width <= 0.0 || bin_width.is_nan() {
        return Json::Arr(Vec::new());
    }
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let n_bins = ((max / bin_width).floor() as usize + 1).max(1);
    let mut counts = vec![0usize; n_bins];
    for &val in values {
        let idx = if val <= 0.0 {
            0
        } else {
            (val / bin_width).floor() as usize
        };
        counts[idx.min(n_bins - 1)] += 1;
    }
    let lo_key = format!("bin_lo_{unit}");
    let hi_key = format!("bin_hi_{unit}");
    Json::Arr(
        counts
            .iter()
            .enumerate()
            .filter(|&(_, &cnt)| cnt > 0)
            .map(|(i, &cnt)| {
                let lo = i as f64 * bin_width;
                let mut o = Json::obj();
                o.set(&lo_key, Json::num(lo));
                o.set(&hi_key, Json::num(lo + bin_width));
                o.set("count", Json::usize(cnt));
                o
            })
            .collect(),
    )
}

fn get_num(o: &Json, key: &str) -> Option<f64> {
    o.get(key).and_then(Json::as_f64)
}

fn build_headline(spacing: &Json, arm: &Json, scar: &Json, resin: &Json, overhang: &Json) -> String {
    let mut parts: Vec<String> = Vec::new();
    match (get_num(spacing, "median_mm"), get_num(spacing, "min_mm")) {
        (Some(med), Some(min)) => parts.push(format!("spacing median {med:.2}mm (min {min:.3}mm)")),
        _ => parts.push("spacing not established (fewer than two contacts)".to_string()),
    }
    if let Some(frac) = get_num(arm, "fraction_at_min_shoot_r") {
        parts.push(format!(
            "arms {:.0}% at {:.3}mm",
            frac * 100.0,
            c::MIN_SHOOT_R_MM
        ));
    }
    if let (Some(vert), Some(up)) = (
        get_num(scar, "fraction_vertical"),
        get_num(scar, "fraction_upward"),
    ) {
        parts.push(format!(
            "scars {:.0}% near-vertical or upward",
            (vert + up).min(1.0) * 100.0
        ));
    }
    if let Some(sf) = get_num(resin, "support_fraction") {
        parts.push(format!("support resin {:.0}%", sf * 100.0));
    }
    if let (Some(area), Some(span)) = (
        get_num(overhang, "total_unattached_area_mm2"),
        get_num(overhang, "max_unattached_run_span_mm"),
    ) {
        parts.push(format!("cantilever {area:.1}mm2, worst span {span:.2}mm"));
    }
    if parts.is_empty() {
        "no report metrics could be established on this model".to_string()
    } else {
        parts.join(" | ")
    }
}

// ---------------------------------------------------------------------------
// report.spacing
// ---------------------------------------------------------------------------

fn spacing_section(ctx: &ModelContext) -> Json {
    let mut o = Json::obj();
    let centres: Vec<V3> = ctx.seats.iter().map(|s| s.centre).collect();
    let n = centres.len();
    o.set("contact_count", Json::usize(n));
    if n < 2 {
        o.set("established", Json::Bool(false));
        o.set("reason", Json::str("fewer_than_two_contacts"));
        return o;
    }

    let mut nn: Vec<f64> = Vec::with_capacity(n);
    for i in 0..n {
        let mut best = f64::INFINITY;
        for (j, &cj) in centres.iter().enumerate() {
            if i == j {
                continue;
            }
            best = best.min(v::dist(centres[i], cj));
        }
        nn.push(best);
    }
    nn.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // EXEMPT (`audit-interval-policy.md` §1.9, §4): these are counts against
    // REFERENCE LINES, not thresholds. `SPACING_MINIMUM_REF_MM` and
    // `TARGET_SPACING_REF_MM` are both `Placeholder` and are documented
    // "reference lines, never thresholds" (`audit-invariants.md` §4);
    // nothing here is pass/fail, so there is no requirement for an interval
    // to straddle. Wrapping them in a tolerance would invent a precision
    // claim the numbers do not have.
    let below_floor = nn.iter().filter(|&&x| x < c::SPACING_MINIMUM_REF_MM).count();
    let below_target = nn.iter().filter(|&&x| x < c::TARGET_SPACING_REF_MM).count();

    o.set("established", Json::Bool(true));
    o.set("min_mm", Json::num(nn[0]));
    o.set("p01_mm", Json::num(percentile(&nn, 0.01)));
    o.set("p05_mm", Json::num(percentile(&nn, 0.05)));
    o.set("median_mm", Json::num(percentile(&nn, 0.5)));
    o.set("p95_mm", Json::num(percentile(&nn, 0.95)));
    o.set("max_mm", Json::num(nn[nn.len() - 1]));
    o.set("count_below_2.5mm", Json::usize(below_floor));
    o.set("count_below_4.0mm", Json::usize(below_target));
    o.set("spacing_floor_ref_mm", Json::num(c::SPACING_MINIMUM_REF_MM));
    o.set("target_spacing_ref_mm", Json::num(c::TARGET_SPACING_REF_MM));
    o.set(
        "reference_line_note",
        Json::str(
            "SPACING_FLOOR_MM and TARGET_SPACING_MM are both Placeholder; quoted here as \
             reference lines only, never as thresholds -- this check does not gate",
        ),
    );
    o.set("histogram_bin_width_mm", Json::num(0.1));
    o.set("histogram", histogram(&nn, 0.1, "mm"));
    o
}

// ---------------------------------------------------------------------------
// report.arm_radius
// ---------------------------------------------------------------------------

fn arm_radius_section(ctx: &ModelContext) -> Json {
    let mut o = Json::obj();
    let mut radii: Vec<f64> = Vec::new();
    let mut degenerate = 0usize;
    let mut member_count = 0usize;

    for (i, rec) in ctx.model.records.iter().enumerate() {
        if rec.kind != RecordKind::Member {
            continue;
        }
        member_count += 1;
        match ctx.primitive(i) {
            Ok(p) => {
                if let Primitive::Frustum { ra, rb, .. } = *p {
                    radii.push(ra.min(rb));
                } else {
                    degenerate += 1;
                }
            }
            Err(_) => degenerate += 1,
        }
    }

    o.set("member_count", Json::usize(member_count));
    o.set("degenerate_excluded", Json::usize(degenerate));
    if radii.is_empty() {
        o.set("established", Json::Bool(false));
        o.set("reason", Json::str("no_usable_member_records"));
        return o;
    }

    radii.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // EXEMPT, EQUALITY-BAND (`audit-interval-policy.md` §1.9, §3.4): a
    // COINCIDENCE count -- "how many arms sit at exactly the placeholder
    // radius" -- comparing a stored f32 against a constant at the evidence's
    // own resolution. Non-gating, and there is no delivered-versus-required
    // relation to give it a conservative side.
    let tol = c::tol_geom_mag(c::MIN_SHOOT_R_MM);
    let count_at_min_shoot_r = radii
        .iter()
        .filter(|&&r| (r - c::MIN_SHOOT_R_MM).abs() <= tol)
        .count();

    o.set("established", Json::Bool(true));
    o.set("min_mm", Json::num(radii[0]));
    o.set("median_mm", Json::num(percentile(&radii, 0.5)));
    o.set("max_mm", Json::num(radii[radii.len() - 1]));
    o.set("count_at_min_shoot_r", Json::usize(count_at_min_shoot_r));
    #[allow(clippy::cast_precision_loss)]
    let fraction = count_at_min_shoot_r as f64 / radii.len() as f64;
    o.set("fraction_at_min_shoot_r", Json::num(fraction));
    o.set("min_shoot_r_ref_mm", Json::num(c::MIN_SHOOT_R_MM));
    o.set(
        "min_shoot_r_note",
        Json::str("MIN_SHOOT_R_MM is an unmeasured placeholder; count_at_min_shoot_r discloses how often the plan hits it, never gates on it"),
    );
    o.set("histogram_bin_width_mm", Json::num(0.01));
    o.set("histogram", histogram(&radii, 0.01, "mm"));
    o
}

// ---------------------------------------------------------------------------
// report.scar_orientation
// ---------------------------------------------------------------------------

fn scar_orientation_section(ctx: &ModelContext) -> Json {
    let mut o = Json::obj();
    let total = ctx.seats.len();
    o.set("contact_count", Json::usize(total));

    let ownership_reliable = ctx.ownership().budget_exceeded.is_none();

    let mut established = 0usize;
    let mut indeterminate = 0usize;
    let (mut down_n, mut vert_n, mut up_n) = (0usize, 0usize, 0usize);
    let (mut down_area, mut vert_area, mut up_area) = (0.0f64, 0.0f64, 0.0f64);

    for s in &ctx.seats {
        let Some(n) = s.entry_normal else {
            indeterminate += 1;
            continue;
        };
        established += 1;
        let nz = n[2];
        let area = ctx.a_owned_mm2(s.contact_id).unwrap_or(0.0);
        if nz < 0.0 {
            down_n += 1;
            down_area += area;
        }
        if nz.abs() <= 0.1 {
            vert_n += 1;
            vert_area += area;
        }
        if nz > 0.1 {
            up_n += 1;
            up_area += area;
        }
    }

    o.set("established_count", Json::usize(established));
    o.set("indeterminate_count", Json::usize(indeterminate));
    o.set("ownership_reliable", Json::Bool(ownership_reliable));
    if !ownership_reliable {
        o.set(
            "ownership_note",
            Json::str("the shared ownership partition hit its sample budget; scar_area figures below are a partial sum, not the true A_owned"),
        );
    }

    if established == 0 {
        o.set("established", Json::Bool(false));
        o.set(
            "reason",
            Json::str("no contact has an established entry_normal"),
        );
        return o;
    }

    #[allow(clippy::cast_precision_loss)]
    let f = established as f64;
    o.set("established", Json::Bool(true));
    o.set("fraction_downward", Json::num(down_n as f64 / f));
    o.set("fraction_vertical", Json::num(vert_n as f64 / f));
    o.set("fraction_upward", Json::num(up_n as f64 / f));
    o.set("scar_area_downward_mm2", Json::num(down_area));
    o.set("scar_area_vertical_mm2", Json::num(vert_area));
    o.set("scar_area_upward_mm2", Json::num(up_area));
    o.set(
        "bucket_note",
        Json::str(
            "implemented exactly as audit-invariants.md §4 states it: fraction_downward is \
             n.z<0, fraction_vertical is |n.z|<=0.1, fraction_upward is n.z>0.1. These overlap \
             on (-0.1, 0) by construction -- a contact there counts toward both downward and \
             vertical. Implemented as written rather than silently repartitioned.",
        ),
    );
    o.set(
        "area_basis",
        Json::str("A_owned per contact from the shared Voronoi ownership partition (audit-invariants.md §2.2)"),
    );
    o
}

// ---------------------------------------------------------------------------
// report.resin_split
// ---------------------------------------------------------------------------

fn resin_split_section(ctx: &ModelContext) -> Json {
    let mut o = Json::obj();
    let support = ctx.support_raster();
    let model = ctx.model_raster();
    match (support, model) {
        (Ok(s), Ok(m)) => {
            let support_ml = s.volume_ml();
            let model_ml = m.volume_ml();
            let total = support_ml + model_ml;
            o.set("established", Json::Bool(true));
            o.set("support_ml", Json::num(support_ml));
            o.set("model_ml", Json::num(model_ml));
            o.set(
                "support_fraction",
                Json::num(if total > 0.0 { support_ml / total } else { 0.0 }),
            );
        }
        (s, m) => {
            o.set("established", Json::Bool(false));
            let mut reasons = Vec::new();
            if let Err(e) = s {
                reasons.push(format!("support_raster: {e}"));
            }
            if let Err(e) = m {
                reasons.push(format!("model_raster: {e}"));
            }
            o.set("reason", Json::str(reasons.join("; ")));
        }
    }
    o
}

// ---------------------------------------------------------------------------
// report.overhang -- verdict "disclosed", never "pass"
// ---------------------------------------------------------------------------

fn overhang_section(ctx: &ModelContext) -> Json {
    let mut o = Json::obj();
    o.set("verdict", Json::str("disclosed"));

    let model_raster = match ctx.model_raster() {
        Ok(r) => r,
        Err(e) => {
            o.set("established", Json::Bool(false));
            o.set("reason", Json::str(format!("model_raster: {e}")));
            return o;
        }
    };
    let Some((min_k, _max_k)) = model_raster.layer_range() else {
        o.set("established", Json::Bool(false));
        o.set("reason", Json::str("model raster has no cured layers"));
        return o;
    };

    let grid = model_raster.grid;
    let pixel_area = grid.pixel_area_mm2();
    let support_raster = ctx.support_raster().ok();

    let mut total_unattached_area_mm2 = 0.0f64;
    let mut max_span_mm = 0.0f64;
    let mut max_span_no_support_mm = 0.0f64;
    let mut layers_with_unattached: BTreeSet<i64> = BTreeSet::new();

    for layer in &model_raster.layers {
        if layer.k == min_k {
            // The very first cured layer has no "previous model layer" to be
            // attached to at all; excluded by analogy with I7's own exclusion
            // of the first layer above the plate (audit-invariants.md §3.7).
            continue;
        }
        let prev_layer = model_raster.layer(layer.k - 1);
        for (&row, runs) in &layer.rows {
            let prev_runs: &[Run] = prev_layer.map_or(&[][..], |l| l.runs_in_row(row));
            for r in runs {
                for ur in subtract_runs(*r, prev_runs) {
                    let len = ur.len();
                    if len <= 0 {
                        continue;
                    }
                    #[allow(clippy::cast_precision_loss)]
                    let len_f = len as f64;
                    total_unattached_area_mm2 += len_f * pixel_area;
                    let span_mm = len_f * grid.pitch_x_mm;
                    max_span_mm = max_span_mm.max(span_mm);
                    layers_with_unattached.insert(layer.k);
                    let layer_has_support = support_raster
                        .and_then(|sr| sr.layer(layer.k))
                        .is_some_and(|l| !l.rows.is_empty());
                    if !layer_has_support {
                        max_span_no_support_mm = max_span_no_support_mm.max(span_mm);
                    }
                }
            }
        }
    }

    o.set("established", Json::Bool(true));
    o.set("support_raster_available", Json::Bool(support_raster.is_some()));
    if support_raster.is_none() {
        o.set(
            "support_raster_note",
            Json::str("support raster failed to build; max_unattached_span_no_support_layer_mm is therefore 0.0 by construction, not a measured absence of support"),
        );
    }
    o.set("total_unattached_area_mm2", Json::num(total_unattached_area_mm2));
    o.set("max_unattached_run_span_mm", Json::num(max_span_mm));
    o.set(
        "max_unattached_span_no_support_layer_mm",
        Json::num(max_span_no_support_mm),
    );
    o.set(
        "layers_with_unattached_area",
        Json::usize(layers_with_unattached.len()),
    );
    o.set("excluded_base_layer_k", Json::int(min_k));
    o.set(
        "note",
        Json::str(
            "unattached = a model pixel column cured at layer k whose OWN column was not cured \
             at k-1 (audit-invariants.md §3.7). This is the cantilever exposure I7 does not \
             gate: a ledge, a cavity ceiling or a receding wall all show up here and nowhere \
             else. A 'run span' is the raw run-length of contiguous unattached columns within \
             one pixel row, in mm; it is NOT geodesic reach across a component.",
        ),
    );
    o
}

/// `run` minus every interval in `cuts`, all in column-index space.
fn subtract_runs(run: Run, cuts: &[Run]) -> Vec<Run> {
    let mut out = vec![run];
    for cut in cuts {
        if out.is_empty() {
            break;
        }
        let mut next = Vec::with_capacity(out.len() + 1);
        for r in &out {
            if cut.c1 <= r.c0 || cut.c0 >= r.c1 {
                next.push(*r);
                continue;
            }
            if r.c0 < cut.c0 {
                next.push(Run { c0: r.c0, c1: cut.c0 });
            }
            if cut.c1 < r.c1 {
                next.push(Run { c0: cut.c1, c1: r.c1 });
            }
        }
        out = next;
    }
    out
}

// ---------------------------------------------------------------------------
// report.raster_erosion_sensitivity
// ---------------------------------------------------------------------------

/// The TIP record's own primitive plus its adjacent TAPER's, when unique --
/// the solid a centre-sample rule would test against for that contact's own
/// patch.
fn tip_group_primitives(ctx: &ModelContext, s: &Seat) -> Vec<Primitive> {
    let mut out = Vec::new();
    if let Ok(p) = ctx.primitive(s.contact_id) {
        out.push(*p);
    }
    if let Some(t) = s.taper.unique() {
        if let Ok(p) = ctx.primitive(t) {
            out.push(*p);
        }
    }
    out
}

/// Cured pixel count of the FULL-CONTAINMENT raster at layer `k`, within a
/// square window (in mm, half-extent `window`) centred on `centre` in-plane.
fn count_full_pixels_near(raster: &RunRaster, k: i64, centre: V3, window: f64) -> usize {
    let Some(layer) = raster.layer(k) else {
        return 0;
    };
    let g = raster.grid;
    let r_lo = ((centre[1] - window) / g.pitch_y_mm).floor() as i64;
    let r_hi = ((centre[1] + window) / g.pitch_y_mm).floor() as i64;
    let c_lo = ((centre[0] - window) / g.pitch_x_mm).floor() as i64;
    let c_hi = ((centre[0] + window) / g.pitch_x_mm).floor() as i64;
    let mut n = 0usize;
    for (&row, runs) in &layer.rows {
        if row < r_lo || row > r_hi {
            continue;
        }
        for r in runs {
            let lo = r.c0.max(c_lo);
            let hi = (r.c1 - 1).min(c_hi);
            if hi >= lo {
                n += (hi - lo + 1) as usize;
            }
        }
    }
    n
}

/// Pixel-CENTRE containment count over the same window, tested directly
/// against `prims` -- the local alternate raster this report builds for
/// itself, because `raster.rs` hard-codes full containment
/// (`audit-invariants.md` §1.6) and this is the one place the report needs
/// the other rule.
fn count_centre_pixels_near(
    grid: RasterGrid,
    k: i64,
    centre: V3,
    window: f64,
    prims: &[Primitive],
) -> usize {
    if prims.is_empty() {
        return 0;
    }
    let z = grid.layer_z_mm(k);
    let r_lo = ((centre[1] - window) / grid.pitch_y_mm).floor() as i64;
    let r_hi = ((centre[1] + window) / grid.pitch_y_mm).floor() as i64;
    let c_lo = ((centre[0] - window) / grid.pitch_x_mm).floor() as i64;
    let c_hi = ((centre[0] + window) / grid.pitch_x_mm).floor() as i64;
    let mut n = 0usize;
    for row in r_lo..=r_hi {
        let y = (row as f64 + 0.5) * grid.pitch_y_mm;
        for col in c_lo..=c_hi {
            let x = (col as f64 + 0.5) * grid.pitch_x_mm;
            let p = [x, y, z];
            if prims.iter().any(|prim| prim.contains(p)) {
                n += 1;
            }
        }
    }
    n
}

fn raster_erosion_sensitivity_section(ctx: &ModelContext) -> Json {
    let mut o = Json::obj();
    let x_allowed = ctx.inputs().x_allowed;
    let mut checked = 0usize;
    let mut flips = 0usize;
    let mut unresolved = 0usize;
    // Contacts whose interval straddles the requirement under each fill
    // rule. Non-gating disclosure, like everything else in this section.
    let mut full_band = 0usize;
    let mut centre_band = 0usize;

    if let Ok(support) = ctx.support_raster() {
        for s in &ctx.seats {
            let (Some(h), Some(u)) = (s.h_delivered_mm, s.u) else {
                unresolved += 1;
                continue;
            };
            if h <= 0.0 {
                unresolved += 1;
                continue;
            }
            let Some(a_owned) = ctx.a_owned_mm2(s.contact_id) else {
                unresolved += 1;
                continue;
            };
            let required = a_owned / x_allowed;
            let tol = c::tol_area(a_owned, x_allowed);
            let axis_point = v::add(s.p0, v::mul(u, h));
            let k = support.grid.layer_at_or_below(axis_point[2]);
            let window = (s.r_tip_mm * 3.0).max(support.grid.pitch_x_mm * 4.0);

            let full_pixels = count_full_pixels_near(support, k, axis_point, window);
            let full_area = full_pixels as f64 * support.grid.pixel_area_mm2();

            let group = tip_group_primitives(ctx, s);
            let centre_pixels = count_centre_pixels_near(support.grid, k, axis_point, window, &group);
            let centre_area = centre_pixels as f64 * support.grid.pixel_area_mm2();

            checked += 1;
            // This metric exists to disclose how many contacts would change
            // I2's verdict under a different fill rule, so its predicate
            // must be I2's real one -- otherwise the disclosed sensitivity
            // is measuring the wrong gate. I2's predicate is now the
            // conservative MINIMUM-type form of
            // `audit-interval-policy.md` §3.2 (rows 17-18, mirrors of
            // row 8), so this is too: it replaces
            // `area >= required - tol` on both fill rules.
            //
            // With three outcomes, "flip" is a change in VERDICT, not a
            // change in a boolean: pass->indeterminate and
            // indeterminate->fail are both real sensitivities to the fill
            // rule and both used to be invisible when the two states were
            // collapsed into `bool`.
            let full_verdict = interval::min_type(full_area, required, tol);
            let centre_verdict = interval::min_type(centre_area, required, tol);
            if full_verdict != centre_verdict {
                flips += 1;
            }
            if full_verdict == Verdict::Indeterminate {
                full_band += 1;
            }
            if centre_verdict == Verdict::Indeterminate {
                centre_band += 1;
            }
        }
    } else {
        unresolved = ctx.seats.len();
    }

    o.set("i2_contact_area_checked", Json::usize(checked));
    o.set("i2_contact_area_flip_count", Json::usize(flips));
    o.set("i2_contact_area_unresolved", Json::usize(unresolved));
    o.set("i2_full_rule_band_indeterminate", Json::usize(full_band));
    o.set("i2_centre_rule_band_indeterminate", Json::usize(centre_band));
    o.set(
        "i2_method",
        Json::str(
            "local proxy, NOT I2's own patch-component algorithm (which does not exist in this \
             codebase yet): full-containment pixel count from the shared support raster, versus \
             a pixel-centre containment test run directly against the contact's own tip-group \
             primitives (its TIP ball and, when unique, its adjacent TAPER frustum), both \
             counted in a small window around the axis point at the delivered seat depth. Read \
             this as a directional signal, not I2's exact flip count.",
        ),
    );
    o.set("i3_min_section_flip_count", Json::Null);
    o.set("i3_established", Json::Bool(false));
    o.set(
        "i3_reason",
        Json::str(
            "I3's load-path walk and per-station narrowest-section search live in \
             narrowest_section.rs, a sibling check module this file may not import and which \
             was itself unimplemented at the time this report was written. Reimplementing that \
             algorithm here would duplicate it, not cross-check it, and a duplicate that drifts \
             from the original is worse than an honest gap.",
        ),
    );
    o.set("i6_no_intersection_flip_count", Json::Null);
    o.set("i6_established", Json::Bool(false));
    o.set(
        "i6_reason",
        Json::str(
            "I6's seat-zone volume test lives in intersection.rs, a sibling check module this \
             file may not import; same limitation as I3 above.",
        ),
    );
    o
}

// ---------------------------------------------------------------------------
// report.junction_turn_angles_deg
// ---------------------------------------------------------------------------

fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[ra] = rb;
    }
}

fn junction_turn_angles_section(ctx: &ModelContext) -> Json {
    let mut o = Json::obj();
    let graph = &ctx.graph;
    let n_points = graph.points.len();
    if n_points == 0 {
        o.set("established", Json::Bool(false));
        o.set("reason", Json::str("no connective points reconstructed"));
        return o;
    }

    // Union-find over connective points via the reconstructed joins, so a
    // junction is "every point that coincides", independent of which record
    // pairs happened to fall within tolerance of each other directly.
    let mut parent: Vec<usize> = (0..n_points).collect();
    for j in &graph.joins {
        let ia = graph.point_index[j.a.0].0 + j.a.1;
        let ib = graph.point_index[j.b.0].0 + j.b.1;
        union(&mut parent, ia, ib);
    }

    let mut clusters: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, pt) in graph.points.iter().enumerate() {
        if ctx.model.records[pt.record].kind != RecordKind::Member {
            continue;
        }
        let root = find(&mut parent, i);
        clusters.entry(root).or_default().push(i);
    }

    let mut angles: Vec<f64> = Vec::new();
    let mut junction_count = 0usize;
    for pts in clusters.values() {
        if pts.len() < 2 {
            continue;
        }
        let mut dirs: Vec<V3> = Vec::new();
        for &pi in pts {
            let cp = &graph.points[pi];
            let rec = &ctx.model.records[cp.record];
            let other = if cp.slot == 0 { rec.end_mm } else { rec.start_mm };
            if let Some(d) = v::normalize(v::sub(other, cp.pos)) {
                dirs.push(d);
            }
        }
        if dirs.len() < 2 {
            continue;
        }
        junction_count += 1;
        for i in 0..dirs.len() {
            for dj in dirs.iter().skip(i + 1) {
                let cosang = v::dot(dirs[i], *dj).clamp(-1.0, 1.0);
                let between_deg = cosang.acos().to_degrees();
                angles.push(180.0 - between_deg);
            }
        }
    }

    o.set("junction_count", Json::usize(junction_count));
    o.set("pairwise_angle_count", Json::usize(angles.len()));
    if angles.is_empty() {
        o.set("established", Json::Bool(false));
        o.set(
            "reason",
            Json::str("no reconstructed junction joins two or more type-3 Member records"),
        );
        return o;
    }

    let max = angles.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    o.set("established", Json::Bool(true));
    o.set("max_deg", Json::num(max));
    o.set("histogram_bin_width_deg", Json::num(5.0));
    o.set("histogram", histogram(&angles, 5.0, "deg"));
    o.set(
        "definition",
        Json::str(
            "per junction (a union-find cluster of connective points joined by \
             SupportGraph::joins, restricted to type-3 Member records), the turn angle for \
             EVERY pairwise combination of members meeting there: 180 deg minus the angle \
             between their outward axis directions, so 0 deg is straight-through and 90 deg is \
             a right-angle elbow. A branch point with more than two members therefore \
             contributes one data point per pair, not one per junction -- \
             audit-invariants.md §4 does not define the branching case, and averaging or \
             picking one pair would hide exactly the exposure I3 names in its own elbow \
             discussion (§3.3).",
        ),
    );
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_matches_hand_worked_values() {
        let v = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert!((percentile(&v, 0.0) - 1.0).abs() < 1e-12);
        assert!((percentile(&v, 1.0) - 5.0).abs() < 1e-12);
        assert!((percentile(&v, 0.5) - 3.0).abs() < 1e-12);
        // Linear interpolation at p=0.25 over 5 points: position 1.0 -> 2.0.
        assert!((percentile(&v, 0.25) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn percentile_on_a_single_value_never_panics() {
        assert!((percentile(&[7.0], 0.5) - 7.0).abs() < 1e-12);
    }

    #[test]
    fn percentile_on_an_empty_slice_is_the_unmeasurable_case() {
        // The boundary case a caller must guard: an empty distribution has no
        // percentile, and this returns NaN rather than a number that looks
        // like data. Every section above checks `is_empty()` BEFORE calling
        // this, precisely so a NaN never reaches the JSON writer (which
        // panics on one, by design -- see json.rs).
        assert!(percentile(&[], 0.5).is_nan());
    }

    #[test]
    fn histogram_bins_are_half_open_and_empty_bins_are_omitted() {
        let h = histogram(&[0.05, 0.15, 0.15, 0.95], 0.1, "mm");
        let arr = h.as_array().expect("array");
        // Values fall in bins [0,0.1), [0.1,0.2), [0.9,1.0): three non-empty
        // bins out of ten possible, and the two occupants of [0.1,0.2) merge.
        assert_eq!(arr.len(), 3, "{arr:?}");
        let counts: Vec<i64> = arr
            .iter()
            .map(|o| o.get("count").and_then(Json::as_f64).unwrap() as i64)
            .collect();
        assert_eq!(counts.iter().sum::<i64>(), 4);
    }

    #[test]
    fn histogram_on_no_values_is_an_empty_array_not_a_panic() {
        assert_eq!(histogram(&[], 0.1, "mm").as_array().map(<[Json]>::len), Some(0));
    }

    #[test]
    fn subtract_runs_removes_only_the_covered_columns() {
        let base = Run { c0: 0, c1: 10 };
        let cuts = [Run { c0: 3, c1: 5 }];
        let out = subtract_runs(base, &cuts);
        assert_eq!(out, vec![Run { c0: 0, c1: 3 }, Run { c0: 5, c1: 10 }]);
    }

    #[test]
    fn subtract_runs_on_full_coverage_leaves_nothing() {
        let base = Run { c0: 2, c1: 6 };
        let cuts = [Run { c0: 0, c1: 10 }];
        assert!(subtract_runs(base, &cuts).is_empty());
    }

    #[test]
    fn subtract_runs_on_disjoint_cuts_is_unchanged() {
        let base = Run { c0: 2, c1: 6 };
        let cuts = [Run { c0: 10, c1: 20 }];
        assert_eq!(subtract_runs(base, &cuts), vec![base]);
    }

    #[test]
    fn union_find_merges_transitively() {
        let mut p: Vec<usize> = (0..5).collect();
        union(&mut p, 0, 1);
        union(&mut p, 1, 2);
        union(&mut p, 3, 4);
        assert_eq!(find(&mut p, 0), find(&mut p, 2));
        assert_ne!(find(&mut p, 0), find(&mut p, 3));
    }
}
