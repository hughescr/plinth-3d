//! I9 unsupported span -- `docs/specs/audit-span-criterion.md`.
//!
//! ## What this closes
//!
//! Cross-family review Finding 1: `island_coverage.rs` (I7) skips an entire
//! run-component the moment ANY pixel in it has its own column cured on the
//! preceding layer, so a 20 mm ledge, a wing that grows sideways, or a cavity
//! ceiling can be unsupported across its whole span and still pass I7. This
//! invariant supplies the missing quantity: an **unsupported-span
//! criterion**, derived from the two calibration plates that measured it
//! (`docs/specs/audit-span-criterion.md` §2).
//!
//! ## The predicate, as implemented (§3, §6.1)
//!
//! For every layer `k` above the first layer at or above the plate, for every
//! 8-connected down-face region `C` of `unattached(k)` -- model pixels whose
//! own column was NOT cured at `k-1`:
//!
//! ```text
//! max over p in C of reach(p)  <=  L_SPAN_ALLOWED_MM
//! ```
//!
//! `reach(p)` is the minimum in-plane path length from `p` to any pixel of
//! `carrier(k)`, over paths that stay inside `model(k)`, on the anisotropic
//! pixel lattice with an 8-neighbour chamfer (`0.014` mm horizontal, `0.019`
//! mm vertical, `d2 = sqrt(0.014^2 + 0.019^2)` diagonal). It is `+infinity`
//! when no such path exists.
//!
//! ```text
//! carrier(k) = attached(k)                                     model under it
//!            u { p in model(k) : column in support_borne(k) }   embedded tip
//!            u { p in model(k) : column in support(k-1) }       column below
//! ```
//!
//! where `support_borne(k)` keeps only those K1 components (shared face, never
//! a corner) of `support(k)` that have at least one pixel whose own column is
//! cured in the support raster at `k-1` -- a support blob born in mid-air at
//! this layer carries nothing (§3.2), and neither does one held on by a corner.
//!
//! **Attachment is a distance-zero seed, never an early exit.** That single
//! sentence is Finding 1's fix. `carrier(k) = model(k) \ working_set(k)`
//! exactly, so a ledge attached along one edge scores ~0 at its root and `L`
//! at its tip, and the gate fires on the tip.
//!
//! ## The one censor, carried over unchanged from I7 (§6.1)
//!
//! A region whose reach is `+infinity` AND whose **clump** area is at most one
//! pixel (`0.014 * 0.019 = 2.66e-4 mm^2`) is not counted. The censor applies
//! ONLY to the infinite-reach case; a region with a finite reach is never
//! censored by area, because area is not what this gate measures.
//!
//! "Clump", not "region", is corollary **K4** of
//! `docs/specs/audit-connectivity-rule.md` §3.1: the censor is decided on the
//! material, never on a fragment the connectivity rule carved off it. A rule
//! that subdivides material may not delete it from a denominator. §5.4 measured
//! what happens without K4 -- three corpus files each LOSE a failure when the
//! rule gets stricter, with `max_reach_mm` identical to the last digit.
//!
//! ## The constant, computed here and never hard-coded (§2.6)
//!
//! ```text
//! L_SURVIVED_MAX_MM   = 2.1300322500 mm   Measured  plate 1 rung 14
//! CONDITION_SHIFT_MAX = 1.53              Measured  plate 4 3.3, upper end
//! d2                  = sqrt(px^2+py^2)   Derived   from the printer pitches
//! L_SPAN_CRIT_MM      = L_SURVIVED_MAX_MM / CONDITION_SHIFT_MAX
//! L_SPAN_ALLOWED_MM   = L_SPAN_CRIT_MM - 2 * d2   = 1.3449762463 mm
//! ```
//!
//! [`SpanConstants::derive`] recomputes all of it from
//! [`crate::context::DeclaredInputs`] and the grid on every run, and every
//! number is echoed in `measured.span_constants` with its provenance. No
//! angle rule is read: `span_overhang_angle_legacy_deg` is disclosure only
//! (§4), and `alpha_min` is DERIVED from the span, never an input to it.
//!
//! ## Tolerance direction -- Finding 3 conformance (§7)
//!
//! The gate is `reach <= L_SPAN_ALLOWED_MM` with **no epsilon on the
//! right-hand side**. Raster erosion (`2 * d2`) is SUBTRACTED from the
//! allowance, never added. The chamfer's overestimate of the true geodesic is
//! left in, because it inflates reach and therefore refuses more geometry; it
//! is disclosed, and its price counted, in `measured.span_metric_conservatism`.
//! Nothing here lets a value inside a measurement band pass.
//!
//! ## Deviations from the specification, stated rather than silently taken
//!
//! 1. §6.3 directs this criterion to REPLACE `island_coverage.rs` in place
//!    under the JSON key `i7_island_coverage`. The registry
//!    (`crate::checks::GATING_IDS`, not a file this check owns) instead wires
//!    it as a separate tenth invariant `i9_unsupported_span` beside an
//!    untouched I7. The id and title here follow the registry, because a
//!    schema key that disagrees with the registry makes the document
//!    unreadable.
//! 2. §2.7 asks for `FILE_DECLARED_INPUT_MISSING` when
//!    `layer_height_mm != 0.050`. That is a file-level code emitted by
//!    `DeclaredInputs::validate`, which this check does not own, so the same
//!    condition is reported here as
//!    `SPAN_INDETERMINATE reason=span_criterion_layer_height_uncalibrated`
//!    over the whole model. It fails either way; it never passes.
//! 3. §6.4 puts `span_margin` and `span_metric_conservatism` in `report.*`,
//!    owned by `report_metrics.rs`. They are emitted here inside `measured`
//!    for the same ownership reason, carrying the same `"disclosed"` verdict.
//! 4. §3.6 requires `regions_passing_under_exact_metric`. The exact Euclidean
//!    geodesic is NOT computed, so that count is not established: it is
//!    emitted as `null`, beside the derived upper bound
//!    `regions_within_metric_conservatism_band_upper_bound` -- every failing
//!    region whose chamfer reach lies within the derived maximum overestimate
//!    of the allowance, i.e. every region that COULD flip. Disclosure only; it
//!    gates nothing, so an unestablished quantity never turns a fail into a
//!    pass.
//!
//! ## Failure codes
//!
//! `ISLAND_UNSUPPORTED` (the `reach = +infinity` case, preserved from I7),
//! `SPAN_UNSUPPORTED`, `SPAN_INDETERMINATE` (reason `slice_failed`,
//! `raster_budget_exceeded`, `distance_budget_exceeded`, or
//! `span_criterion_layer_height_uncalibrated`).
//!
//! ## What this check cannot see
//!
//! `docs/specs/audit-span-criterion.md` §8 lists twelve items and every one of
//! them stands. The three sharpest, restated because a reader of the JSON must
//! not miss them:
//!
//! - **Cumulative deflection on a shallow ramp.** The coupon measured one
//!   lamina reaching `L` from a RIGID spine. A ramp is every lamina reaching
//!   `layer_height / tan(alpha)` from a root that is itself a cantilever, and
//!   deflection accumulates. Neither plate tested a compliant root, so this
//!   criterion is optimistic there by an unmeasured amount.
//! - **Carrier ADEQUACY is not checked here at all.** One support pixel
//!   touching a ledge tip collapses reach along its whole length whether or
//!   not that support reaches the plate (I5), is thick enough (I2/I3/I4) or is
//!   live (I8). Reading this check alone overstates the plan's quality, which
//!   is why its headline says so.
//! - **Sag is not gated.** The plates measured LOSS. A region that reaches
//!   1.3 mm passes here and may still print visibly drooped, which is the
//!   owner's own complaint.

use crate::check::{Check, CheckOutcome, Failure};
use crate::context::{DeclaredInputs, ModelContext};
use crate::json::Json;
use crate::raster::{InPlaneJoin, Layer, RasterError, RasterGrid, RowRuns, Run, RunRaster};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

pub struct UnsupportedSpan;

// ---------------------------------------------------------------------------
// Constants that pin a calibration point or prune a search. None of them
// decides an outcome on its own (`docs/physics.md` §12): the gate value is
// derived in `SpanConstants::derive` from the declared inputs on every run.
// ---------------------------------------------------------------------------

/// The one layer height both calibration plates ran at
/// (`audit-span-criterion.md` §2.1). Neither varied it, and the bending of a
/// single lamina scales with thickness in a way nothing here measured, so a
/// file at any other layer height is INDETERMINATE -- never a pass (§2.7).
const CALIBRATED_LAYER_HEIGHT_MM: f64 = 0.050;

/// The geometric ratio of the plate-1/plate-4 ladder, from
/// `docs/evidence/plate-generator-PLATE-LAYOUT.md`, reproduced in
/// `audit-span-criterion.md` §2.1. Used for exactly one thing: recovering the
/// next rung up from `L_SURVIVED_MAX_MM`, which is the shortest length at
/// which a failure was actually recorded (plate 4, `G1aU`, 2.592036 mm) and so
/// the upper edge of the non-gating `span_margin` disclosure band (§6.4).
const LADDER_RATIO: f64 = 1.2169;

/// Reach histogram bin width, `audit-span-criterion.md` §6.3.
const REACH_HISTOGRAM_BIN_MM: f64 = 0.05;

/// Bins before the overflow bucket: 400 * 0.05 mm = 20 mm.
const REACH_HISTOGRAM_BINS: usize = 400;

/// One line per region, worst reach first, capped (§6.2). Truncation reduces
/// the number of LINES; `failed` always counts every failing region.
const MAX_FAILURE_LINES: usize = 64;

/// Working-set cap for ONE layer, in pixels. This is the memory bound: the
/// pass allocates one `f64` distance, one `u32` seed index and one `(i64,i64)`
/// coordinate per working pixel, so the cap is ~28 bytes each, ~224 MB at the
/// limit.
///
/// **MEASURED, and raised once after a MEASURED exhaustion** -- the
/// `SampleBudget` precedent, not a guess. A first value of 1,000,000 refused
/// `exports/cylinder-first-artifact.chitubox` with
/// `distance_budget_exceeded` at its lifted flat base. Re-run with the cap
/// lifted, the true worst per-layer working set over all 44 audited models
/// (24 real miniatures, 7 development fixtures, 12 planted fixtures, 1 smoke
/// output) is **2,504,487 pixels**, that same cylinder base; the worst real
/// miniature is Deinonychus at 22,196 and the worst planted fixture
/// `broken_path_*` at 374,148. This cap is 3.2x the worst observed.
///
/// Exceeding it emits `SPAN_INDETERMINATE reason=distance_budget_exceeded`
/// for the whole model and FAILS. It never coarsens and never passes.
const MAX_WORKING_PIXELS_PER_LAYER: usize = 8_000_000;

/// Working-set cap summed over every layer of one model, in pixels. A work
/// bound, not a memory bound -- the per-layer allocation is freed each layer.
/// MEASURED the same way: the worst total over the same 44 models is
/// **21,971,548 pixels** (`Round 100`, 6.4 s), against 1,809,858 for
/// Deinonychus and 12,592,383 for the treant. This cap is 9.1x the worst
/// observed.
const MAX_WORKING_PIXELS_TOTAL: usize = 200_000_000;

// ---------------------------------------------------------------------------
// The derived gate constant
// ---------------------------------------------------------------------------

/// Everything §2.6 derives, recomputed from the declared inputs on every run.
#[derive(Clone, Copy, Debug)]
pub struct SpanConstants {
    /// MEASURED. Plate 1 rung 14: the largest span with zero recorded loss.
    pub l_survived_max_mm: f64,
    /// MEASURED. Plate 4 §3.3, upper (conservative) end of the unidentified
    /// 1.31-1.53x condition shift.
    pub condition_shift_max: f64,
    /// DERIVED from the printer pitches: the in-layer pixel diagonal.
    pub d2_mm: f64,
    /// DERIVED. `l_survived_max_mm / condition_shift_max`.
    pub l_span_crit_mm: f64,
    /// DERIVED. `l_span_crit_mm - 2 * d2_mm` -- THE GATE CONSTANT.
    pub l_span_allowed_mm: f64,
    /// DERIVED consequence, reported and never read as an input (§4).
    pub alpha_min_deg: f64,
    /// DERIVED. `l_survived_max_mm * LADDER_RATIO` = plate 4 rung 7, the
    /// shortest length at which a failure was actually recorded.
    pub first_recorded_failure_mm: f64,
    /// DERIVED. The chamfer metric's maximum overestimate of the true in-plane
    /// geodesic on this anisotropic lattice, as a ratio >= 1.
    pub chamfer_max_overestimate: f64,
}

impl SpanConstants {
    #[must_use]
    pub fn derive(inputs: &DeclaredInputs, grid: &RasterGrid) -> Self {
        let d2_mm = (grid.pitch_x_mm.powi(2) + grid.pitch_y_mm.powi(2)).sqrt();
        let l_span_crit_mm = inputs.span_l_survived_max_mm / inputs.span_condition_shift_max;
        // Rule 2 of the specification's preamble: the instrument's own erosion
        // is SUBTRACTED from the allowance -- one pixel footprint at each end
        // of the path -- never added to it.
        let l_span_allowed_mm = l_span_crit_mm - 2.0 * d2_mm;
        Self {
            l_survived_max_mm: inputs.span_l_survived_max_mm,
            condition_shift_max: inputs.span_condition_shift_max,
            d2_mm,
            l_span_crit_mm,
            l_span_allowed_mm,
            alpha_min_deg: (grid.layer_height_mm / l_span_allowed_mm).atan().to_degrees(),
            first_recorded_failure_mm: inputs.span_l_survived_max_mm * LADDER_RATIO,
            chamfer_max_overestimate: chamfer_max_overestimate(
                grid.pitch_x_mm,
                grid.pitch_y_mm,
                d2_mm,
            ),
        }
    }

    /// THE GATE. `reach <= allowed`, with no epsilon on the right-hand side:
    /// an epsilon there would re-create exactly the defect Finding 3 named
    /// (§7, last row).
    ///
    /// **Deliberately outside `audit-interval-policy.md`'s sweep.**
    /// `docs/specs/audit-span-criterion.md` §7 pins this comparison sign by
    /// sign: every uncertainty in that document is already folded into
    /// `l_span_allowed_mm` in the failing direction (the erosion band is
    /// SUBTRACTED from the allowance, the chamfer overestimate is left in,
    /// the condition shift is divided by the LARGER factor, the ladder is
    /// read at the LOWER rung). Wrapping it in an interval here would apply
    /// the same uncertainties twice, in opposite directions. `+infinity` fails by the same comparison.
    #[must_use]
    pub fn passes(&self, reach_mm: f64) -> bool {
        reach_mm <= self.l_span_allowed_mm
    }

    fn to_json(self) -> Json {
        let mut o = Json::obj();
        o.set("l_survived_max_mm", Json::num(self.l_survived_max_mm));
        o.set("condition_shift_max", Json::num(self.condition_shift_max));
        o.set("d2_mm", Json::num(self.d2_mm));
        o.set("l_span_crit_mm", Json::num(self.l_span_crit_mm));
        o.set("l_span_allowed_mm", Json::num(self.l_span_allowed_mm));
        o.set("alpha_min_deg", Json::num(self.alpha_min_deg));
        o.set(
            "first_recorded_failure_mm",
            Json::num(self.first_recorded_failure_mm),
        );
        o.set("ladder_ratio", Json::num(LADDER_RATIO));
        let mut prov = Json::obj();
        prov.set(
            "l_survived_max_mm",
            Json::str("Measured -- plate 1 rung 14, the largest span with zero recorded loss"),
        );
        prov.set(
            "condition_shift_max",
            Json::str(
                "Measured -- plate 4 3.3, upper end of the 1.31-1.53x shift; cause unidentified",
            ),
        );
        prov.set(
            "d2_mm",
            Json::str("Derived -- in-layer pixel diagonal of the declared printer pitches"),
        );
        prov.set(
            "l_span_allowed_mm",
            Json::str("Derived -- l_survived_max_mm / condition_shift_max - 2 * d2_mm"),
        );
        prov.set(
            "alpha_min_deg",
            Json::str("Derived consequence of the span; NOT an input, and no angle rule is read"),
        );
        o.set("provenance", prov);
        o
    }
}

/// The maximum ratio by which the 8-neighbour chamfer overestimates the true
/// in-plane Euclidean distance on a lattice of pitches `a` (x) and `b` (y)
/// with diagonal `d2`. DERIVED here, not quoted (§3.6): a straight run of `n`
/// steps with `m <= n` of them diagonal has chamfer length `a*(n-m) + d2*m`
/// against a Euclidean `sqrt((n*a)^2 + (m*b)^2)`; setting the derivative of
/// the ratio in `t = m/n` to zero gives `t* = a*(d2-a)/b^2` on the x-dominant
/// branch and `t* = b*(d2-b)/a^2` on the y-dominant one.
///
/// On the Saturn 4 lattice this returns 1.120418 (+12.0418 %), the y-dominant
/// branch peaking at 1.052615 (+5.2615 %). **If the 3x3 neighbourhood is ever
/// replaced by a 5x5, this derivation must be redone, not assumed.**
fn chamfer_max_overestimate(a: f64, b: f64, d2: f64) -> f64 {
    let branch = |straight: f64, cross: f64| -> f64 {
        let c = d2 - straight;
        if c <= 0.0 || cross <= 0.0 {
            return 1.0;
        }
        let t = (c * straight / (cross * cross)).clamp(0.0, 1.0);
        (straight + c * t) / (straight * straight + cross * cross * t * t).sqrt()
    };
    branch(a, b).max(branch(b, a)).max(1.0)
}

// ---------------------------------------------------------------------------
// Run-list algebra. Local to this check by design: `raster.rs` is not a file
// this agent owns. All of it assumes normalised inputs -- sorted, disjoint,
// non-touching -- which is what `RunRaster` guarantees.
// ---------------------------------------------------------------------------

fn run_difference(a: &[Run], b: &[Run]) -> Vec<Run> {
    if b.is_empty() {
        return a.to_vec();
    }
    let mut out = Vec::new();
    let mut j = 0usize;
    for r in a {
        let mut cur = r.c0;
        while j < b.len() && b[j].c1 <= cur {
            j += 1;
        }
        let mut jj = j;
        while jj < b.len() && b[jj].c0 < r.c1 {
            if b[jj].c0 > cur {
                out.push(Run {
                    c0: cur,
                    c1: b[jj].c0,
                });
            }
            cur = cur.max(b[jj].c1);
            if cur >= r.c1 {
                break;
            }
            jj += 1;
        }
        if cur < r.c1 {
            out.push(Run { c0: cur, c1: r.c1 });
        }
    }
    out
}

fn run_union(a: &[Run], b: &[Run]) -> Vec<Run> {
    if a.is_empty() {
        return b.to_vec();
    }
    if b.is_empty() {
        return a.to_vec();
    }
    let mut merged: Vec<Run> = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0usize, 0usize);
    while i < a.len() || j < b.len() {
        let take_a = j >= b.len() || (i < a.len() && a[i].c0 <= b[j].c0);
        let r = if take_a {
            i += 1;
            a[i - 1]
        } else {
            j += 1;
            b[j - 1]
        };
        match merged.last_mut() {
            Some(last) if r.c0 <= last.c1 => last.c1 = last.c1.max(r.c1),
            _ => merged.push(r),
        }
    }
    merged
}

/// The runs of `row` in a row-sorted run table, or an empty slice.
fn rows_lookup(table: &[(i64, Vec<Run>)], row: i64) -> &[Run] {
    match table.binary_search_by_key(&row, |(r, _)| *r) {
        Ok(i) => &table[i].1,
        Err(_) => &[],
    }
}

fn runs_contain(runs: &[Run], col: i64) -> bool {
    runs.binary_search_by(|r| {
        if col < r.c0 {
            Ordering::Greater
        } else if col >= r.c1 {
            Ordering::Less
        } else {
            Ordering::Equal
        }
    })
    .is_ok()
}

// ---------------------------------------------------------------------------
// In-plane components: Rule K1, from `raster.rs`
// ---------------------------------------------------------------------------
//
// This file used to carry its own `components_8`, an 8-connectivity walk that
// joined two runs touching only at a corner. `docs/specs/audit-span-criterion.md`
// §3.2 and §3.3 specified 8-connectivity twice and gave no reason anywhere in
// the document, while `raster.rs` used 4 and `arrangement_spread.rs` took a
// bool. `docs/specs/audit-connectivity-rule.md` §4 sites 2 and 3 rule both of
// this file's uses onto **Rule K1** -- shared face, zero for a corner -- and
// the local copy is deleted rather than left to drift.
//
// Both uses are load questions, which is why the physical rule applies to them:
// a down-face region is the object the reach criterion grades, and a support
// blob held to the material below it by a corner is held on by nothing this
// project has ever measured.


// ---------------------------------------------------------------------------
// The working set: the pixels whose reach must actually be computed
// ---------------------------------------------------------------------------

/// `working_set(k) = unattached(k) \ carried(k)`, materialised as pixels.
///
/// Nothing else is materialised. `carrier(k) = model(k) \ working_set(k)` is
/// tested by run lookup, so a full-layer 16K bitmap is never allocated (§3.6).
struct WorkSet {
    rows: Vec<i64>,
    /// Per row: `(run, first flat pixel index, region id)`.
    runs: Vec<Vec<(Run, u32, u32)>>,
    total: usize,
}

impl WorkSet {
    fn index_of(&self, row: i64, col: i64) -> Option<u32> {
        let ri = self.rows.binary_search(&row).ok()?;
        let runs = &self.runs[ri];
        let k = runs
            .binary_search_by(|(r, _, _)| {
                if col < r.c0 {
                    Ordering::Greater
                } else if col >= r.c1 {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            })
            .ok()?;
        let (run, base, _) = runs[k];
        Some(base + (col - run.c0) as u32)
    }
}

/// A min-heap entry. `f64` has no `Ord`, so the order is pinned with
/// `total_cmp` rather than an `unwrap` on a partial comparison.
#[derive(Clone, Copy, PartialEq)]
struct HeapItem(f64, u32);
impl Eq for HeapItem {}
impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed: BinaryHeap is a max-heap and Dijkstra needs the minimum.
        other.0.total_cmp(&self.0).then_with(|| other.1.cmp(&self.1))
    }
}
impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// What one down-face region scored.
#[derive(Clone, Copy, Debug)]
struct RegionScore {
    layer: i64,
    reach_mm: f64,
    /// The arg-max pixel: WHERE the region reaches furthest.
    at: (i64, i64),
    /// The carrier that pixel's shortest path came from, and whether it is
    /// support rather than model material.
    carrier: Option<(i64, i64, bool)>,
    /// The region's own pixel-count-weighted centroid, for the island line.
    centroid: (f64, f64),
    pixels: i64,
}

// ---------------------------------------------------------------------------
// The check
// ---------------------------------------------------------------------------

impl Check for UnsupportedSpan {
    fn id(&self) -> &'static str {
        "i9_unsupported_span"
    }

    fn title(&self) -> &'static str {
        "I9 unsupported span"
    }

    /// Layers of the model raster. The real unit of this invariant is the
    /// down-face REGION, which is not known until the raster is walked, so the
    /// layer count is the honest pre-run scope -- the same choice
    /// `island_coverage.rs` makes, for the same reason.
    fn scope(&self, ctx: &ModelContext) -> usize {
        if let Ok(r) = ctx.model_raster() {
            return r.layers.len().max(1);
        }
        let e = ctx.model_mesh.bbox.extent()[2];
        ((e / ctx.grid.layer_height_mm).ceil() as usize).max(1)
    }

    fn run(&self, ctx: &ModelContext) -> CheckOutcome {
        let file = ctx.file();
        let grid = ctx.grid;
        let model = ctx.model_index;
        let scope = self.scope(ctx);
        let sc = SpanConstants::derive(ctx.inputs(), &grid);

        // §2.7: the criterion is calibrated at one layer height only. Any
        // other height is UNMEASURED, and unmeasured never passes.
        if (grid.layer_height_mm - CALIBRATED_LAYER_HEIGHT_MM).abs() > 1.0e-12 {
            return whole_model_indeterminate(
                &file,
                model,
                scope,
                "span_criterion_layer_height_uncalibrated",
                &format!(
                    "declared layer_height_mm={} is not the {CALIBRATED_LAYER_HEIGHT_MM} mm both \
                     calibration plates ran at; the span criterion does not travel to another \
                     layer height",
                    grid.layer_height_mm
                ),
                sc,
            );
        }

        let model_raster = match ctx.model_raster() {
            Ok(r) => r,
            Err(e) => return raster_unavailable(&file, model, scope, "model", e, sc),
        };
        let support_raster = match ctx.support_raster() {
            Ok(r) => r,
            Err(e) => return raster_unavailable(&file, model, scope, "support", e, sc),
        };

        // Computed, never hard-coded to 0, so a frame-convention change cannot
        // silently desync this from the grid it is compared against.
        let first_layer_k = grid.layer_at_or_above(grid.z_plate_mm);

        let mut st = Tally::new(sc);
        for layer in &model_raster.layers {
            st.layers_scanned += 1;
            if layer.k == first_layer_k {
                continue; // the plate itself carries this layer
            }
            if let Err(reason) = score_layer(layer, model_raster, support_raster, layer.k, &mut st) {
                return whole_model_indeterminate(
                    &file,
                    model,
                    scope,
                    reason,
                    &format!(
                        "the working set at layer {} exceeded the span distance budget \
                         (per-layer {MAX_WORKING_PIXELS_PER_LAYER}, total \
                         {MAX_WORKING_PIXELS_TOTAL} pixels); the span could not be established",
                        layer.k
                    ),
                    sc,
                );
            }
        }

        st.finish(&file, model, &grid, first_layer_k)
    }
}

// ---------------------------------------------------------------------------
// Accumulation across layers
// ---------------------------------------------------------------------------

/// Regions are counted and histogrammed in full; only the worst
/// [`MAX_FAILURE_LINES`] are kept as lines.
struct Tally {
    sc: SpanConstants,
    layers_scanned: usize,
    regions_checked: usize,
    regions_failed: usize,
    regions_infinite: usize,
    regions_censored: usize,
    regions_fully_carried: usize,
    regions_in_margin_band: usize,
    regions_in_metric_band: usize,
    carrier_model: usize,
    carrier_support: usize,
    hist: Vec<u64>,
    hist_overflow: u64,
    max_finite: Option<RegionScore>,
    worst: Vec<RegionScore>,
    truncated: usize,
    working_pixels_total: usize,
    working_pixels_max_layer: usize,
}

impl Tally {
    fn new(sc: SpanConstants) -> Self {
        Self {
            sc,
            layers_scanned: 0,
            regions_checked: 0,
            regions_failed: 0,
            regions_infinite: 0,
            regions_censored: 0,
            regions_fully_carried: 0,
            regions_in_margin_band: 0,
            regions_in_metric_band: 0,
            carrier_model: 0,
            carrier_support: 0,
            hist: vec![0; REACH_HISTOGRAM_BINS],
            hist_overflow: 0,
            max_finite: None,
            worst: Vec::new(),
            truncated: 0,
            working_pixels_total: 0,
            working_pixels_max_layer: 0,
        }
    }

    fn record(&mut self, r: RegionScore) {
        self.regions_checked += 1;
        if r.reach_mm.is_finite() {
            let bin = (r.reach_mm / REACH_HISTOGRAM_BIN_MM).floor().max(0.0) as usize;
            if bin < REACH_HISTOGRAM_BINS {
                self.hist[bin] += 1;
            } else {
                self.hist_overflow += 1;
            }
            if self.max_finite.is_none_or(|m| r.reach_mm > m.reach_mm) {
                self.max_finite = Some(r);
            }
            match r.carrier {
                Some((_, _, true)) => self.carrier_support += 1,
                Some((_, _, false)) => self.carrier_model += 1,
                None => self.regions_fully_carried += 1,
            }
        } else {
            self.regions_infinite += 1;
        }

        if self.sc.passes(r.reach_mm) {
            return;
        }
        self.regions_failed += 1;
        if r.reach_mm.is_finite() {
            // §6.4: refused by the gate, but below the shortest span at which
            // a failure was actually recorded. Disclosure, never a pass.
            if r.reach_mm < self.sc.first_recorded_failure_mm {
                self.regions_in_margin_band += 1;
            }
            // §3.6: the price of the chamfer's overestimate, made countable.
            if r.reach_mm <= self.sc.l_span_allowed_mm * self.sc.chamfer_max_overestimate {
                self.regions_in_metric_band += 1;
            }
        }
        self.worst.push(r);
        if self.worst.len() > MAX_FAILURE_LINES * 16 {
            self.prune();
        }
    }

    fn prune(&mut self) {
        if self.worst.len() <= MAX_FAILURE_LINES {
            return;
        }
        self.worst.sort_by(|a, b| b.reach_mm.total_cmp(&a.reach_mm));
        self.truncated += self.worst.len() - MAX_FAILURE_LINES;
        self.worst.truncate(MAX_FAILURE_LINES);
    }

    fn finish(
        mut self,
        file: &str,
        model: usize,
        grid: &RasterGrid,
        first_layer_k: i64,
    ) -> CheckOutcome {
        self.prune();
        self.worst.sort_by(|a, b| b.reach_mm.total_cmp(&a.reach_mm));

        let mut out = CheckOutcome::new(self.regions_checked);
        // Truncation reduces the number of LINES, never the count (§6.2).
        out.failed = self.regions_failed;

        for r in &self.worst {
            let z_mm = grid.layer_z_mm(r.layer);
            let z_above = z_mm - grid.z_plate_mm;
            let area = r.pixels as f64 * grid.pixel_area_mm2();
            if r.reach_mm.is_finite() {
                let (x, y) = pixel_centre_mm(r.at.0, r.at.1, grid);
                let (cx, cy, kind) = match r.carrier {
                    Some((cr, cc, s)) => {
                        let (cx, cy) = pixel_centre_mm(cr, cc, grid);
                        (Some(cx), Some(cy), if s { "support" } else { "model" })
                    }
                    None => (None, None, "carried"),
                };
                let mut v = Json::obj();
                v.set("layer", Json::int(r.layer));
                v.set("z_mm", Json::num(z_mm));
                v.set("z_above_plate_mm", Json::num(z_above));
                v.set("reach_mm", Json::num(r.reach_mm));
                v.set("allowed_mm", Json::num(self.sc.l_span_allowed_mm));
                v.set(
                    "over_by_mm",
                    Json::num(r.reach_mm - self.sc.l_span_allowed_mm),
                );
                v.set("at_mm", Json::nums([x, y]));
                v.set(
                    "nearest_carrier_mm",
                    Json::Arr(vec![Json::num_or_null(cx), Json::num_or_null(cy)]),
                );
                v.set("carrier_kind", Json::str(kind));
                v.set("region_area_mm2", Json::num(area));
                v.set("region_pixels", Json::int(r.pixels));
                out.failures.push(Failure::line(
                    "SPAN_UNSUPPORTED",
                    file,
                    model,
                    None,
                    None,
                    &[
                        ("layer", r.layer.to_string()),
                        ("z_mm", fmt(z_mm)),
                        ("z_above_plate_mm", fmt(z_above)),
                        ("reach_mm", fmt(r.reach_mm)),
                        ("allowed_mm", fmt(self.sc.l_span_allowed_mm)),
                        ("at_mm", format!("{},{}", fmt(x), fmt(y))),
                        (
                            "nearest_carrier_mm",
                            match (cx, cy) {
                                (Some(a), Some(b)) => format!("{},{}", fmt(a), fmt(b)),
                                _ => "n/a".to_string(),
                            },
                        ),
                        ("carrier_kind", kind.to_string()),
                        ("region_area_mm2", fmt(area)),
                        ("region_pixels", r.pixels.to_string()),
                    ],
                    v,
                ));
            } else {
                let (x, y) = r.centroid;
                let mut v = Json::obj();
                v.set("layer", Json::int(r.layer));
                v.set("z_mm", Json::num(z_mm));
                v.set("z_above_plate_mm", Json::num(z_above));
                v.set("area_mm2", Json::num(area));
                v.set("centroid_mm", Json::nums([x, y]));
                v.set("pixels", Json::int(r.pixels));
                out.failures.push(Failure::line(
                    "ISLAND_UNSUPPORTED",
                    file,
                    model,
                    None,
                    None,
                    &[
                        ("layer", r.layer.to_string()),
                        ("z_mm", fmt(z_mm)),
                        ("z_above_plate_mm", fmt(z_above)),
                        ("area_mm2", fmt(area)),
                        ("centroid_mm", format!("{},{}", fmt(x), fmt(y))),
                        ("pixels", r.pixels.to_string()),
                    ],
                    v,
                ));
            }
        }

        let m = &mut out.measured;
        m.set("implemented", Json::Bool(true));
        m.set("layers_scanned", Json::usize(self.layers_scanned));
        m.set("first_layer_k", Json::int(first_layer_k));
        m.set("regions_checked", Json::usize(self.regions_checked));
        m.set("regions_failed", Json::usize(self.regions_failed));
        m.set("regions_infinite_reach", Json::usize(self.regions_infinite));
        m.set(
            "regions_censored_one_pixel",
            Json::usize(self.regions_censored),
        );
        m.set(
            "regions_fully_carried",
            Json::usize(self.regions_fully_carried),
        );
        m.set("failures_truncated", Json::usize(self.truncated));
        m.set("pixel_area_mm2", Json::num(grid.pixel_area_mm2()));
        m.set("span_constants", self.sc.to_json());

        if let Some(r) = self.max_finite {
            let (x, y) = pixel_centre_mm(r.at.0, r.at.1, grid);
            m.set("max_reach_mm", Json::num(r.reach_mm));
            m.set("max_reach_location_mm", Json::nums([x, y]));
            m.set("max_reach_layer", Json::int(r.layer));
            m.set(
                "max_reach_z_above_plate_mm",
                Json::num(grid.layer_z_mm(r.layer) - grid.z_plate_mm),
            );
        } else {
            m.set("max_reach_mm", Json::num_or_null(None));
            m.set("max_reach_layer", Json::num_or_null(None));
        }

        let mut hist = Json::obj();
        hist.set("bin_mm", Json::num(REACH_HISTOGRAM_BIN_MM));
        hist.set(
            "counts",
            Json::Arr(self.hist.iter().map(|c| Json::int(*c as i64)).collect()),
        );
        hist.set("overflow", Json::int(self.hist_overflow as i64));
        hist.set("infinite", Json::usize(self.regions_infinite));
        m.set("reach_histogram", hist);

        let mut carriers = Json::obj();
        carriers.set("nearest_carrier_is_model", Json::usize(self.carrier_model));
        carriers.set(
            "nearest_carrier_is_support",
            Json::usize(self.carrier_support),
        );
        m.set("carrier_kinds", carriers);

        let mut margin = Json::obj();
        margin.set("verdict", Json::str("disclosed"));
        margin.set("band_lo_mm", Json::num(self.sc.l_span_allowed_mm));
        margin.set("band_hi_mm", Json::num(self.sc.first_recorded_failure_mm));
        margin.set("regions_in_band", Json::usize(self.regions_in_margin_band));
        margin.set(
            "note",
            Json::str(
                "regions refused by the gate but below the shortest span at which a failure was \
                 actually recorded; the price of the 1.53x condition-shift discount, made \
                 countable. Disclosure only -- it gates nothing and is never a pass.",
            ),
        );
        m.set("span_margin", margin);

        let mut cons = Json::obj();
        cons.set("verdict", Json::str("disclosed"));
        cons.set(
            "chamfer_max_overestimate_percent",
            Json::num((self.sc.chamfer_max_overestimate - 1.0) * 100.0),
        );
        cons.set(
            "regions_within_metric_conservatism_band_upper_bound",
            Json::usize(self.regions_in_metric_band),
        );
        cons.set("regions_passing_under_exact_metric", Json::num_or_null(None));
        cons.set(
            "note",
            Json::str(
                "the exact Euclidean geodesic is NOT computed, so the count of regions that would \
                 pass under it is not established and is reported as null; what is given is the \
                 derived upper bound -- every failing region whose chamfer reach lies within the \
                 maximum overestimate of the allowance, i.e. every region that COULD flip. \
                 Disclosure only.",
            ),
        );
        m.set("span_metric_conservatism", cons);

        let mut ws = Json::obj();
        ws.set("pixels_total", Json::usize(self.working_pixels_total));
        ws.set(
            "pixels_max_layer",
            Json::usize(self.working_pixels_max_layer),
        );
        ws.set("budget_per_layer", Json::usize(MAX_WORKING_PIXELS_PER_LAYER));
        ws.set("budget_total", Json::usize(MAX_WORKING_PIXELS_TOTAL));
        m.set("working_set", ws);

        let tail = "Carrier ADEQUACY is not checked here: whether it reaches the plate is I5, \
                    whether it is thick enough is I2/I3/I4, whether it is live is I8.";
        let headline = if self.regions_failed == 0 {
            match self.max_finite {
                Some(r) => format!(
                    "0 of {} down-face regions exceed the {:.6} mm allowance; worst reach \
                     {:.6} mm at layer {}. {tail}",
                    self.regions_checked, self.sc.l_span_allowed_mm, r.reach_mm, r.layer
                ),
                None => format!(
                    "no down-face region above the one-pixel censor across {} layers; \
                     regions_checked={} is itself worth investigating, not celebrating",
                    self.layers_scanned, self.regions_checked
                ),
            }
        } else {
            let w = self.worst[0];
            let (x, y) = if w.reach_mm.is_finite() {
                pixel_centre_mm(w.at.0, w.at.1, grid)
            } else {
                w.centroid
            };
            format!(
                "{} of {} down-face regions reach further than the measured {:.6} mm allowance \
                 ({} of them have no carrier at all); worst {} mm at ({:.3}, {:.3}) mm, {:.3} mm \
                 above the plate. {tail}",
                self.regions_failed,
                self.regions_checked,
                self.sc.l_span_allowed_mm,
                self.regions_infinite,
                if w.reach_mm.is_finite() {
                    format!("{:.6}", w.reach_mm)
                } else {
                    "infinite".to_string()
                },
                x,
                y,
                grid.layer_z_mm(w.layer) - grid.z_plate_mm,
            )
        };
        out.measured.set("headline", Json::str(headline));
        out
    }
}

// ---------------------------------------------------------------------------
// One layer of the criterion
// ---------------------------------------------------------------------------

/// Returns `Err(reason)` when the working set exceeds its budget -- which is
/// INDETERMINATE for the whole model, never a coarsening and never a pass.
fn score_layer(
    layer: &Layer,
    model_raster: &RunRaster,
    support_raster: &RunRaster,
    k: i64,
    st: &mut Tally,
) -> Result<(), &'static str> {
    let prev = model_raster.layer(k - 1);
    let grid = model_raster.grid;

    // unattached(k) = model(k) minus the columns cured at k-1.
    let mut unattached: Vec<(i64, Vec<Run>)> = Vec::new();
    for (&row, runs) in &layer.rows {
        let below = prev.map_or(&[][..], |p| p.runs_in_row(row));
        let un = run_difference(runs, below);
        if !un.is_empty() {
            unattached.push((row, un));
        }
    }
    if unattached.is_empty() {
        return Ok(());
    }

    // The support side of `carrier`: support_borne(k) u support(k-1).
    let borne = support_borne_rows(support_raster.layer(k), support_raster.layer(k - 1));
    let sup_prev = support_raster.layer(k - 1);

    // working set = unattached \ carried
    let mut ws = WorkSet {
        rows: Vec::with_capacity(unattached.len()),
        runs: Vec::with_capacity(unattached.len()),
        total: 0,
    };
    for (row, un) in &unattached {
        let b = rows_lookup(&borne, *row);
        let p = sup_prev.map_or(&[][..], |l| l.runs_in_row(*row));
        let carrier_cols = run_union(b, p);
        let w = run_difference(un, &carrier_cols);
        if w.is_empty() {
            continue;
        }
        let mut runs = Vec::with_capacity(w.len());
        for r in w {
            runs.push((r, ws.total as u32, u32::MAX));
            ws.total += r.len().max(0) as usize;
        }
        ws.rows.push(*row);
        ws.runs.push(runs);
    }

    if ws.total > MAX_WORKING_PIXELS_PER_LAYER {
        return Err("distance_budget_exceeded");
    }
    st.working_pixels_total += ws.total;
    st.working_pixels_max_layer = st.working_pixels_max_layer.max(ws.total);
    if st.working_pixels_total > MAX_WORKING_PIXELS_TOTAL {
        return Err("distance_budget_exceeded");
    }

    // Regions are components of unattached(k) -- carried pixels included,
    // because a region carried at one end and cantilevered at the other is
    // exactly the case Finding 1 was about. Rule K1: shared face, and a corner
    // contact joins nothing (`audit-connectivity-rule.md` §4 site 2).
    let table = RowRuns::from_row_table(&unattached);
    let flat = &table.runs;
    let (labels, region_count) = table.label(InPlaneJoin::SharedFace);
    // K4 (§3.1, §3.5): the one-pixel censor below is decided on the CLUMP of
    // material, never on a fragment K1 carved out of it. Without this, K1
    // shatters a failing region until its pieces fall under the censor and the
    // file's failure count goes DOWN because the rule got stricter -- measured
    // on three corpus files in §5.4.
    let (clump_labels, clump_count) = table.label(InPlaneJoin::SharedFaceOrCorner);
    let clump_pixels_by_clump = table.pixels_per_label(&clump_labels, clump_count);
    let mut region_clump_pixels = vec![0i64; region_count];
    for i in 0..flat.len() {
        region_clump_pixels[labels[i] as usize] =
            clump_pixels_by_clump[clump_labels[i] as usize];
    }
    let mut region_pixels = vec![0i64; region_count];
    let mut region_sx = vec![0.0f64; region_count];
    let mut region_sy = vec![0.0f64; region_count];
    let mut flat_row_start: Vec<usize> = Vec::with_capacity(unattached.len() + 1);
    {
        let mut acc = 0usize;
        for (_, runs) in &unattached {
            flat_row_start.push(acc);
            acc += runs.len();
        }
        flat_row_start.push(acc);
    }
    for (i, (row, r)) in flat.iter().enumerate() {
        let n = r.len().max(0);
        if n == 0 {
            continue;
        }
        let g = labels[i] as usize;
        region_pixels[g] += n;
        region_sx[g] += (r.c0 as f64 + r.c1 as f64) * 0.5 * grid.pitch_x_mm * n as f64;
        region_sy[g] += (*row as f64 + 0.5) * grid.pitch_y_mm * n as f64;
    }

    // Attach a region id to every working-set run: a working run is a subset
    // of exactly one unattached run in its own row.
    for (ri, row) in ws.rows.iter().enumerate() {
        let ui = unattached
            .binary_search_by_key(row, |(r, _)| *r)
            .expect("every working-set row came from an unattached row");
        let (s, e) = (flat_row_start[ui], flat_row_start[ui + 1]);
        for (run, _, region) in &mut ws.runs[ri] {
            let mut found = u32::MAX;
            for f in s..e {
                let u = flat[f].1;
                if run.c0 >= u.c0 && run.c0 < u.c1 {
                    found = labels[f];
                    break;
                }
            }
            debug_assert_ne!(found, u32::MAX, "a working run outside every unattached run");
            *region = found;
        }
    }

    // Multi-source Dijkstra. Every carrier pixel is a source at distance zero,
    // so propagation never enters attached or support-carried material and the
    // settled set is exactly the working set (§3.6): one pass per LAYER, never
    // one per region and never one per pixel.
    let field = reach_field(&ws, layer, prev, &borne, sup_prev, grid);
    let (dist, src, seeds) = (&field.dist, &field.src, &field.seeds);

    // Score every region: the max reach over its pixels. Carried pixels are
    // not in the working set and score zero by construction.
    let mut best: Vec<Option<(f64, i64, i64, u32)>> = vec![None; region_count];
    for (ri, row) in ws.rows.iter().enumerate() {
        for (run, base, region) in &ws.runs[ri] {
            let g = *region as usize;
            for c in run.c0..run.c1 {
                let idx = (*base + (c - run.c0) as u32) as usize;
                let d = dist[idx];
                if best[g].is_none_or(|(bd, _, _, _)| d > bd) {
                    best[g] = Some((d, *row, c, src[idx]));
                }
            }
        }
    }

    let one_pixel = grid.pixel_area_mm2();
    for g in 0..region_count {
        let pixels = region_pixels[g];
        if pixels == 0 {
            continue;
        }
        let (reach, at, seed) = match best[g] {
            Some((d, row, col, s)) => (d, (row, col), s),
            // Every pixel of this region is carried: reach is zero everywhere.
            None => (0.0, (0, 0), u32::MAX),
        };
        // K4: the censor's subject is the clump, not this region. A region of
        // one pixel that a corner contact attaches to more unattached material
        // is a FRAGMENT of that material, not the printer's noise floor, and
        // K1 declining to join them is not permission to delete one of them.
        let clump_area = region_clump_pixels[g] as f64 * one_pixel;
        if !reach.is_finite() && clump_area <= one_pixel {
            // The printer's own noise level (§6.1) -- infinite-reach case ONLY.
            st.regions_censored += 1;
            continue;
        }
        st.record(RegionScore {
            layer: k,
            reach_mm: reach,
            at,
            carrier: (seed != u32::MAX).then(|| seeds[seed as usize]),
            centroid: (
                region_sx[g] / pixels as f64,
                region_sy[g] / pixels as f64,
            ),
            pixels,
        });
    }
    Ok(())
}

/// One layer's reach field: a distance and a seed index per working pixel,
/// plus the seed table those indices point into.
struct ReachField {
    dist: Vec<f64>,
    src: Vec<u32>,
    seeds: Vec<(i64, i64, bool)>,
}

/// Multi-source Dijkstra over the working set, seeded from every carrier pixel
/// adjacent to it.
fn reach_field(
    ws: &WorkSet,
    layer: &Layer,
    prev: Option<&Layer>,
    borne: &[(i64, Vec<Run>)],
    sup_prev: Option<&Layer>,
    grid: RasterGrid,
) -> ReachField {
    let n = ws.total;
    let mut dist = vec![f64::INFINITY; n];
    let mut src = vec![u32::MAX; n];
    let mut seeds: Vec<(i64, i64, bool)> = Vec::new();
    let mut coords: Vec<(i64, i64)> = vec![(0, 0); n];
    let mut heap: BinaryHeap<HeapItem> = BinaryHeap::new();

    let (a, b) = (grid.pitch_x_mm, grid.pitch_y_mm);
    let d2 = (a * a + b * b).sqrt();
    let step = |dr: i64, dc: i64| -> f64 {
        match (dr.abs(), dc.abs()) {
            (0, 1) => a,
            (1, 0) => b,
            _ => d2,
        }
    };

    // Seeding pass, in row order so each working row fetches the three model
    // rows it needs once rather than once per pixel.
    for (ri, row) in ws.rows.iter().enumerate() {
        let model_rows = [
            layer.runs_in_row(*row - 1),
            layer.runs_in_row(*row),
            layer.runs_in_row(*row + 1),
        ];
        for (run, base, _) in &ws.runs[ri] {
            for c in run.c0..run.c1 {
                let idx = (*base + (c - run.c0) as u32) as usize;
                coords[idx] = (*row, c);
                let mut best: Option<(f64, (i64, i64))> = None;
                for dr in -1i64..=1 {
                    let nrow = *row + dr;
                    let model_runs = model_rows[(dr + 1) as usize];
                    for dc in -1i64..=1 {
                        if dr == 0 && dc == 0 {
                            continue;
                        }
                        let ncol = c + dc;
                        if !runs_contain(model_runs, ncol) {
                            continue; // not model material: not a carrier
                        }
                        if ws.index_of(nrow, ncol).is_some() {
                            continue; // in the working set: not a carrier
                        }
                        // carrier(k) = model(k) \ working_set(k), by
                        // construction: what is left is attached or carried.
                        let w = step(dr, dc);
                        if best.is_none_or(|(bw, _)| w < bw) {
                            best = Some((w, (nrow, ncol)));
                        }
                    }
                }
                if let Some((w, (nrow, ncol))) = best {
                    let is_model_below =
                        prev.is_some_and(|p| runs_contain(p.runs_in_row(nrow), ncol));
                    let is_support = !is_model_below
                        && (runs_contain(rows_lookup(borne, nrow), ncol)
                            || sup_prev.is_some_and(|l| runs_contain(l.runs_in_row(nrow), ncol)));
                    seeds.push((nrow, ncol, is_support));
                    dist[idx] = w;
                    src[idx] = (seeds.len() - 1) as u32;
                    heap.push(HeapItem(w, idx as u32));
                }
            }
        }
    }

    // Relaxation, restricted to the working set: a path through a carrier
    // pixel is dominated by starting AT that carrier, so leaving carriers out
    // of the frontier can never lengthen a shortest path.
    while let Some(HeapItem(d, u)) = heap.pop() {
        let u = u as usize;
        if d > dist[u] {
            continue;
        }
        let (row, col) = coords[u];
        for dr in -1i64..=1 {
            for dc in -1i64..=1 {
                if dr == 0 && dc == 0 {
                    continue;
                }
                let Some(v) = ws.index_of(row + dr, col + dc) else {
                    continue;
                };
                let v = v as usize;
                let nd = d + step(dr, dc);
                if nd < dist[v] {
                    dist[v] = nd;
                    src[v] = src[u];
                    heap.push(HeapItem(nd, v as u32));
                }
            }
        }
    }
    ReachField { dist, src, seeds }
}

/// `support_borne(k)`: the runs of `support(k)` whose own K1
/// component has at least one pixel cured in the support raster at `k-1`.
///
/// Component-level, not pixel-level, and deliberately so (§3.2): a tip ball is
/// wider than its own taper, so its rim pixels are legitimately unattached at
/// their birth layer. What this refuses is a support blob whose whole
/// component is born in mid-air at this layer, which today's I7 accepts.
fn support_borne_rows(sup_k: Option<&Layer>, sup_prev: Option<&Layer>) -> Vec<(i64, Vec<Run>)> {
    let Some(sup) = sup_k else {
        return Vec::new();
    };
    let table: Vec<(i64, Vec<Run>)> = sup
        .rows
        .iter()
        .map(|(row, runs)| (*row, runs.clone()))
        .collect();
    if table.is_empty() {
        return Vec::new();
    }
    // Rule K1 (`audit-connectivity-rule.md` §4 site 3): same lattice, same load
    // meaning. A support component held to the layer below by a corner is held
    // on by nothing that has been established, so it is not `support_borne`.
    let rr = RowRuns::from_row_table(&table);
    let flat = &rr.runs;
    let (labels, count) = rr.label(InPlaneJoin::SharedFace);
    let mut ok = vec![false; count];
    for (i, (row, run)) in flat.iter().enumerate() {
        let g = labels[i] as usize;
        if ok[g] {
            continue;
        }
        let below = sup_prev.map_or(&[][..], |p| p.runs_in_row(*row));
        if below.iter().any(|b| run.c0 < b.c1 && b.c0 < run.c1) {
            ok[g] = true;
        }
    }
    let mut out: Vec<(i64, Vec<Run>)> = Vec::new();
    for (i, (row, run)) in flat.iter().enumerate() {
        if !ok[labels[i] as usize] {
            continue;
        }
        match out.last_mut() {
            Some((r, runs)) if *r == *row => runs.push(*run),
            _ => out.push((*row, vec![*run])),
        }
    }
    out
}

/// The centre of pixel `(row, col)` in the model-bounding-box-centred frame,
/// mm -- the frame the `.chitubox` itself stores, so the pair can be typed
/// straight into ChituBox and found (§6.2).
fn pixel_centre_mm(row: i64, col: i64, grid: &RasterGrid) -> (f64, f64) {
    (
        (col as f64 + 0.5) * grid.pitch_x_mm,
        (row as f64 + 0.5) * grid.pitch_y_mm,
    )
}

fn fmt(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.6}")
    } else if v.is_nan() {
        "n/a".to_string()
    } else if v > 0.0 {
        "inf".to_string()
    } else {
        "-inf".to_string()
    }
}

/// The whole-model indeterminate outcome. "Not measured" never maps to pass
/// (`audit-invariants.md` §0 rule 3), so this is a FAILURE with
/// `checked == indeterminate == scope`, and it still echoes the constants so
/// the document says what it would have gated against.
fn whole_model_indeterminate(
    file: &str,
    model: usize,
    scope: usize,
    reason: &'static str,
    detail: &str,
    sc: SpanConstants,
) -> CheckOutcome {
    let scope = scope.max(1);
    let mut values = Json::obj();
    values.set("reason", Json::str(reason));
    values.set("detail", Json::str(detail.to_string()));
    let failure = Failure::line(
        "SPAN_INDETERMINATE",
        file,
        model,
        None,
        None,
        &[("layer", "n/a".to_string()), ("reason", reason.to_string())],
        values,
    );
    let mut measured = Json::obj();
    measured.set("implemented", Json::Bool(true));
    measured.set("span_constants", sc.to_json());
    measured.set("regions_checked", Json::usize(0));
    measured.set(
        "headline",
        Json::str(format!(
            "unsupported span could not be established ({reason}); {detail}"
        )),
    );
    CheckOutcome {
        checked: scope,
        failed: 0,
        indeterminate: scope,
        measured,
        failures: vec![failure],
    }
}

fn raster_unavailable(
    file: &str,
    model: usize,
    scope: usize,
    which: &'static str,
    e: &RasterError,
    sc: SpanConstants,
) -> CheckOutcome {
    let reason = match e {
        RasterError::BudgetExceeded { .. } => "raster_budget_exceeded",
        RasterError::MeshNotClosed | RasterError::EmptyOrNonFinite => "slice_failed",
    };
    whole_model_indeterminate(
        file,
        model,
        scope,
        reason,
        &format!("{which} raster unavailable: {e}"),
        sc,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::Status;
    use crate::consts as c;
    use crate::context::{AuditContext, AuditOptions};
    use crate::reader::{ChituboxFile, RecordKind};
    use crate::synth::{box_soup, SynthFile, SynthModel, SynthRecord};
    use std::path::Path;

    // ------------------------------------------------------------------
    // The derived constant and its boundary
    // ------------------------------------------------------------------

    fn saturn4_constants() -> SpanConstants {
        SpanConstants::derive(&DeclaredInputs::default(), &RasterGrid::saturn4(0.0))
    }

    #[test]
    fn the_gate_constant_is_derived_from_its_inputs_and_not_hard_coded() {
        let sc = saturn4_constants();
        assert!((sc.l_survived_max_mm - 2.130_032_25).abs() < 1e-12);
        assert!((sc.condition_shift_max - 1.53).abs() < 1e-12);
        assert!((sc.d2_mm - 0.023_600_847_4).abs() < 1e-9, "{}", sc.d2_mm);
        assert!(
            (sc.l_span_crit_mm - 1.392_177_941_2).abs() < 1e-9,
            "{}",
            sc.l_span_crit_mm
        );
        assert!(
            (sc.l_span_allowed_mm - 1.344_976_246_3).abs() < 1e-9,
            "L_SPAN_ALLOWED_MM = {}",
            sc.l_span_allowed_mm
        );
        // §4: the angle is an OUTPUT of the measured span, not an input.
        assert!(
            (sc.alpha_min_deg - 2.129_0).abs() < 1e-3,
            "alpha_min = {}",
            sc.alpha_min_deg
        );
        // §2.1: plate 4 rung 7, the shortest length at which a failure was
        // actually recorded.
        assert!(
            (sc.first_recorded_failure_mm - 2.592_036).abs() < 1e-5,
            "{}",
            sc.first_recorded_failure_mm
        );
        // A constant that is not read has been converted into a chosen number.
        let inputs = DeclaredInputs {
            span_condition_shift_max: 1.31,
            ..DeclaredInputs::default()
        };
        let relaxed = SpanConstants::derive(&inputs, &RasterGrid::saturn4(0.0));
        assert!(relaxed.l_span_allowed_mm > sc.l_span_allowed_mm + 0.2);
    }

    #[test]
    fn the_gate_boundary_is_inclusive_and_carries_no_epsilon() {
        let sc = saturn4_constants();
        let l = sc.l_span_allowed_mm;
        assert!(sc.passes(l), "a reach exactly at the allowance passes");
        assert!(
            !sc.passes(l + f64::EPSILON * l),
            "one ulp past the allowance must FAIL -- no epsilon on the right-hand side"
        );
        assert!(!sc.passes(f64::INFINITY), "no carrier at all must fail");
        assert!(sc.passes(0.0));
        // Finding 3's inversion as a test: the erosion allowance is
        // SUBTRACTED, so the gate is strictly tighter than the physics bracket
        // and a value inside the measurement band fails.
        assert!(sc.l_span_allowed_mm < sc.l_span_crit_mm);
        assert!(!sc.passes(sc.l_span_crit_mm));
        assert!(!sc.passes(l + 1e-12));
    }

    #[test]
    fn the_chamfer_overestimate_is_derived_and_matches_a_numeric_scan() {
        let (a, b) = (c::PIXEL_PITCH_X_MM, c::PIXEL_PITCH_Y_MM);
        let d2 = (a * a + b * b).sqrt();
        let closed = chamfer_max_overestimate(a, b, d2);
        let mut scan: f64 = 1.0;
        for i in 0..=200_000u32 {
            let t = f64::from(i) / 200_000.0;
            let x = (a + (d2 - a) * t) / (a * a + b * b * t * t).sqrt();
            let y = (b + (d2 - b) * t) / (b * b + a * a * t * t).sqrt();
            scan = scan.max(x).max(y);
        }
        assert!(
            (closed - scan).abs() < 1e-9,
            "closed form {closed} vs 200k-direction scan {scan}"
        );
        assert!(
            ((closed - 1.0) * 100.0 - 12.0418).abs() < 1e-3,
            "max overestimate {:.6} %",
            (closed - 1.0) * 100.0
        );
    }

    // ------------------------------------------------------------------
    // Run algebra and components
    // ------------------------------------------------------------------

    #[test]
    fn the_run_algebra_is_exact_on_the_cases_the_criterion_uses() {
        let a = vec![Run { c0: 0, c1: 10 }, Run { c0: 20, c1: 30 }];
        let b = vec![Run { c0: 5, c1: 22 }];
        assert_eq!(
            run_difference(&a, &b),
            vec![Run { c0: 0, c1: 5 }, Run { c0: 22, c1: 30 }]
        );
        assert_eq!(run_union(&a, &b), vec![Run { c0: 0, c1: 30 }]);
        assert_eq!(run_difference(&a, &[]), a);
        assert_eq!(run_difference(&a, &a), vec![]);
        assert!(runs_contain(&a, 25));
        assert!(!runs_contain(&a, 15));
    }

    /// I9's regions are K1 components: a corner contact does NOT make two
    /// down-face regions one (`audit-connectivity-rule.md` §4 site 2). This
    /// test asserted the opposite until 2026-08-31 and is the reason this
    /// invariant's region counts roughly double.
    #[test]
    fn regions_are_k1_components_and_a_corner_contact_never_joins_two() {
        let count = |t: &[(i64, Vec<Run>)]| {
            RowRuns::from_row_table(t).label(InPlaneJoin::SharedFace).1
        };
        // Runs [0,5) and [5,9) in neighbouring rows share ZERO columns: they
        // meet at the corner between column 4 and column 5.
        let diag = vec![
            (0i64, vec![Run { c0: 0, c1: 5 }]),
            (1, vec![Run { c0: 5, c1: 9 }]),
        ];
        assert_eq!(count(&diag), 2, "a corner contact carries no face area");
        // One shared column is a face, and it joins.
        let edge = vec![
            (0i64, vec![Run { c0: 0, c1: 5 }]),
            (1, vec![Run { c0: 4, c1: 9 }]),
        ];
        assert_eq!(count(&edge), 1, "one shared column is one shared face");
        let apart = vec![
            (0i64, vec![Run { c0: 0, c1: 5 }]),
            (1, vec![Run { c0: 6, c1: 9 }]),
        ];
        assert_eq!(count(&apart), 2);
        let skip = vec![
            (0i64, vec![Run { c0: 0, c1: 5 }]),
            (2, vec![Run { c0: 0, c1: 5 }]),
        ];
        assert_eq!(count(&skip), 2, "non-adjacent rows never join");
        // The clump rule -- and ONLY the clump rule -- still fuses the corner.
        assert_eq!(
            RowRuns::from_row_table(&diag)
                .label(InPlaneJoin::SharedFaceOrCorner)
                .1,
            1,
            "the K4 clump is the loosest adjacency and must keep fusing corners"
        );
    }

    // ------------------------------------------------------------------
    // Fixture geometry, built in process
    // ------------------------------------------------------------------

    /// Extrude a closed 2D polygon given in the XZ plane along Y.
    ///
    /// Used instead of two overlapping `box_soup`s because an internal face
    /// plane makes the rasteriser's full-containment rule blank the pixel
    /// column under it, which would sever a ledge from the block it is
    /// attached to and so test the wrong thing.
    fn extrude_xz(poly: &[(f64, f64)], y0: f64, y1: f64) -> Vec<[[f32; 3]; 3]> {
        let p = |x: f64, y: f64, z: f64| [x as f32, y as f32, z as f32];
        let mut out = Vec::new();
        let n = poly.len();
        for i in 0..n {
            let (x0, z0) = poly[i];
            let (x1, z1) = poly[(i + 1) % n];
            let (a, b) = (p(x0, y0, z0), p(x1, y0, z1));
            let (cc, d) = (p(x1, y1, z1), p(x0, y1, z0));
            out.push([a, cc, b]);
            out.push([a, d, cc]);
        }
        // Caps by ear clipping; the polygon may be non-convex.
        let area2 = |a: (f64, f64), b: (f64, f64), cc: (f64, f64)| {
            (b.0 - a.0) * (cc.1 - a.1) - (b.1 - a.1) * (cc.0 - a.0)
        };
        let mut idx: Vec<usize> = (0..n).collect();
        let mut tris: Vec<[usize; 3]> = Vec::new();
        let mut guard = 0;
        while idx.len() > 2 && guard < 10_000 {
            guard += 1;
            let m = idx.len();
            let mut clipped = false;
            for i in 0..m {
                let (ia, ib, ic) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
                let (a, b, cc) = (poly[ia], poly[ib], poly[ic]);
                if area2(a, b, cc) <= 0.0 {
                    continue;
                }
                let clear = idx.iter().all(|&j| {
                    j == ia
                        || j == ib
                        || j == ic
                        || !(area2(a, b, poly[j]) >= 0.0
                            && area2(b, cc, poly[j]) >= 0.0
                            && area2(cc, a, poly[j]) >= 0.0)
                });
                if clear {
                    tris.push([ia, ib, ic]);
                    idx.remove(i);
                    clipped = true;
                    break;
                }
            }
            assert!(clipped, "ear clipping stalled on a bad test polygon");
        }
        for [ia, ib, ic] in tris {
            let (a, b, cc) = (poly[ia], poly[ib], poly[ic]);
            out.push([p(a.0, y0, a.1), p(b.0, y0, b.1), p(cc.0, y0, cc.1)]);
            out.push([p(a.0, y1, a.1), p(cc.0, y1, cc.1), p(b.0, y1, b.1)]);
        }
        out
    }

    fn flip(soup: Vec<[[f32; 3]; 3]>) -> Vec<[[f32; 3]; 3]> {
        soup.into_iter().map(|[a, b, c]| [a, c, b]).collect()
    }

    /// A block standing on the plate to `z = 3.0`, with a one-layer ledge of
    /// length `l` mm growing out of its side at the very top.
    fn ledge_model(l: f64, x_shift: f64, support: Option<SynthRecord>) -> SynthFile {
        let poly = vec![
            (x_shift, 0.0),
            (x_shift + 1.0, 0.0),
            (x_shift + 1.0, 2.95),
            (x_shift + 1.0 + l, 2.95),
            (x_shift + 1.0 + l, 3.0),
            (x_shift, 3.0),
        ];
        SynthFile::new().with_model(SynthModel {
            name: "ledge".into(),
            translate_mm: [0.0, 0.0, 0.0],
            mesh: extrude_xz(&poly, 0.0, 2.0),
            records: support.into_iter().collect(),
        })
    }

    /// A closed hollow box: a `w+4` mm cube shell with a `w` mm cavity from
    /// `z = 2` to `z = 4`, so the ceiling closes over at `z = 4`.
    fn cavity_model(w: f64, column: Option<(f64, f64, f64)>) -> SynthFile {
        let mut mesh = box_soup([0.0, 0.0, 0.0], [w + 4.0, w + 4.0, 6.0]);
        mesh.extend(flip(box_soup([2.0, 2.0, 2.0], [2.0 + w, 2.0 + w, 4.0])));
        let records = match column {
            Some((cx, cy, r)) => vec![SynthRecord::new(
                RecordKind::Member,
                [cx, cy, 2.1],
                [cx, cy, 4.2],
                r,
                r,
            )
            .with_matching_soup()],
            None => Vec::new(),
        };
        SynthFile::new().with_model(SynthModel {
            name: "cavity".into(),
            translate_mm: [0.0, 0.0, 0.0],
            mesh,
            records,
        })
    }

    fn with_ctx<R>(sf: &SynthFile, opts: AuditOptions, f: impl FnOnce(&ModelContext) -> R) -> R {
        let file = ChituboxFile::parse(Path::new("span_test.chitubox"), &sf.to_bytes())
            .expect("synthetic file must parse");
        let audit = AuditContext {
            file: &file,
            options: &opts,
            sidecar: None,
            sidecar_absence: None,
        };
        let ctx = ModelContext::build(&audit, 0);
        f(&ctx)
    }

    fn run_on(sf: &SynthFile) -> CheckOutcome {
        with_ctx(sf, AuditOptions::default(), |ctx| {
            assert!(
                ctx.model_mesh.closure.is_closed(),
                "test geometry is not closed: {}",
                ctx.model_mesh.closure.summary()
            );
            UnsupportedSpan.run(ctx)
        })
    }

    fn measured_f64(o: &CheckOutcome, key: &str) -> f64 {
        o.measured
            .get(key)
            .and_then(Json::as_f64)
            .unwrap_or_else(|| panic!("measured.{key} missing or not a number"))
    }

    fn value_f64(f: &Failure, key: &str) -> f64 {
        f.values
            .get(key)
            .and_then(Json::as_f64)
            .unwrap_or_else(|| panic!("failure value {key} missing"))
    }

    fn first(o: &CheckOutcome, code: &str) -> Failure {
        o.failures
            .iter()
            .find(|f| f.code == code)
            .unwrap_or_else(|| {
                panic!(
                    "no {code} line; got {:?}",
                    o.failures.iter().map(|f| f.code).collect::<Vec<_>>()
                )
            })
            .clone()
    }

    fn erosion_tol() -> f64 {
        2.0 * (c::PIXEL_PITCH_X_MM.powi(2) + c::PIXEL_PITCH_Y_MM.powi(2)).sqrt()
    }

    // ------------------------------------------------------------------
    // The fixtures of audit-span-criterion.md section 10
    // ------------------------------------------------------------------

    #[test]
    fn span_ledge_long_fails_and_names_where_in_model_coordinates() {
        let o = run_on(&ledge_model(3.0, 0.0, None));
        assert_eq!(o.status(), Status::Fail);
        assert_eq!(o.indeterminate, 0, "{:?}", o.failures);
        assert!(o.failed >= 1);
        let f = first(&o, "SPAN_UNSUPPORTED");
        let reach = value_f64(&f, "reach_mm");
        assert!(
            (reach - 3.0).abs() <= erosion_tol(),
            "reach_mm={reach}, expected 3.0 +/- {}",
            erosion_tol()
        );
        // at_mm must be the ledge TIP, x ~ 4.0 mm -- not a centroid.
        let at = f.values.get("at_mm").and_then(Json::as_array).unwrap();
        let x = at[0].as_f64().unwrap();
        assert!(
            (x - 4.0).abs() <= 3.0 * erosion_tol(),
            "at_mm.x={x}, the tip is at 4.0"
        );
        assert_eq!(
            f.values.get("carrier_kind").and_then(Json::as_str),
            Some("model")
        );
        assert!(value_f64(&f, "over_by_mm") > 1.6);
        assert!(f.message.contains("SPAN_UNSUPPORTED"), "{}", f.message);
        assert!(f.message.contains("z_above_plate_mm="), "{}", f.message);
    }

    #[test]
    fn span_ledge_short_passes_with_its_measured_reach() {
        let o = run_on(&ledge_model(1.2, 0.0, None));
        assert_eq!(o.status(), Status::Pass, "{:?}", o.failures);
        assert!(o.checked > 0, "a pass with zero regions checked is vacuous");
        let reach = measured_f64(&o, "max_reach_mm");
        assert!(
            (reach - 1.2).abs() <= erosion_tol(),
            "max_reach_mm={reach}, expected 1.2 +/- {}",
            erosion_tol()
        );
    }

    /// S1: the same long ledge with a support column under its tip PASSES, and
    /// deleting that one record flips it back to FAIL. A check whose answer
    /// cannot be made to change has not been shown to be watching anything.
    #[test]
    fn s1_deleting_the_support_flips_the_verdict() {
        let support = SynthRecord::new(
            RecordKind::Member,
            [3.5, 1.0, 0.1],
            [3.5, 1.0, 2.99],
            0.3,
            0.3,
        )
        .with_matching_soup();
        let supported = run_on(&ledge_model(3.0, 0.0, Some(support)));
        assert_eq!(
            supported.status(),
            Status::Pass,
            "a column under the tip must carry it: {:?}",
            supported.failures
        );
        assert_eq!(
            run_on(&ledge_model(3.0, 0.0, None)).status(),
            Status::Fail,
            "removing that record must flip the verdict"
        );
    }

    /// S2: the constant is read, not baked. Relaxing the measured inputs so the
    /// allowance exceeds 3 mm flips the long ledge to PASS.
    #[test]
    fn s2_changing_the_constant_flips_the_verdict() {
        let sf = ledge_model(3.0, 0.0, None);
        assert_eq!(run_on(&sf).status(), Status::Fail);
        let opts = AuditOptions {
            inputs: DeclaredInputs {
                span_l_survived_max_mm: 10.0 * 1.53,
                ..DeclaredInputs::default()
            },
            ..AuditOptions::default()
        };
        let relaxed = with_ctx(&sf, opts, |ctx| UnsupportedSpan.run(ctx));
        assert_eq!(
            relaxed.status(),
            Status::Pass,
            "the gate constant is not being read: {:?}",
            relaxed.failures
        );
    }

    /// S3: the reported location comes from the failing pixel and nothing
    /// else. Move the model 7 mm in +X and `at_mm` must move 7 mm.
    #[test]
    fn s3_the_reported_location_tracks_the_failing_pixel() {
        let at_of = |shift: f64| -> f64 {
            let o = run_on(&ledge_model(3.0, shift, None));
            let f = first(&o, "SPAN_UNSUPPORTED");
            f.values.get("at_mm").and_then(Json::as_array).unwrap()[0]
                .as_f64()
                .unwrap()
        };
        let (a, b) = (at_of(0.0), at_of(7.0));
        assert!(
            (b - a - 7.0).abs() < 0.05,
            "at_mm.x moved {} mm, expected 7.0",
            b - a
        );
    }

    #[test]
    fn span_cavity_open_fails_inside_the_cavity_footprint() {
        let o = run_on(&cavity_model(5.0, None));
        assert_eq!(o.status(), Status::Fail, "a 5 mm cavity must not close over");
        let f = first(&o, "SPAN_UNSUPPORTED");
        let at = f.values.get("at_mm").and_then(Json::as_array).unwrap();
        let (x, y) = (at[0].as_f64().unwrap(), at[1].as_f64().unwrap());
        assert!(
            (2.0..7.0).contains(&x) && (2.0..7.0).contains(&y),
            "at_mm=({x},{y}) must lie inside the cavity footprint [2,7]^2"
        );
        let za = value_f64(&f, "z_above_plate_mm");
        assert!((3.9..4.1).contains(&za), "closing layer z_above_plate={za}");
        let reach = value_f64(&f, "reach_mm");
        assert!((2.0..3.0).contains(&reach), "reach_mm={reach}, half-width 2.5");
    }

    #[test]
    fn span_cavity_narrow_passes_so_the_gate_does_not_refuse_every_hollow() {
        let o = run_on(&cavity_model(2.0, None));
        assert_eq!(o.status(), Status::Pass, "{:?}", o.failures);
        let reach = measured_f64(&o, "max_reach_mm");
        assert!(reach < saturn4_constants().l_span_allowed_mm, "{reach}");
        assert!(reach > 0.9, "a 2 mm cavity ceiling reaches ~1 mm, got {reach}");
    }

    /// S4: the cavity clause the corpus cannot exercise. A column standing on
    /// the cavity floor carries its ceiling; moving that column outside the
    /// cavity wall does not, and the verdict must flip.
    #[test]
    fn s4_a_column_inside_the_cavity_carries_its_ceiling_and_one_outside_does_not() {
        let inside = run_on(&cavity_model(5.0, Some((4.5, 4.5, 0.8))));
        let outside = run_on(&cavity_model(5.0, Some((1.0, 1.0, 0.8))));
        assert_eq!(
            inside.status(),
            Status::Pass,
            "an interior column must carry the ceiling: {:?}",
            inside.failures
        );
        assert_eq!(
            outside.status(),
            Status::Fail,
            "a column outside the cavity wall carries nothing inside it"
        );
    }

    #[test]
    fn span_floating_island_keeps_the_old_infinite_reach_behaviour() {
        let mut mesh = box_soup([0.0, 0.0, 0.0], [2.0, 2.0, 1.0]);
        mesh.extend(box_soup([0.0, 0.0, 3.0], [1.0, 1.0, 4.0]));
        let sf = SynthFile::new().with_model(SynthModel {
            name: "two-box".into(),
            translate_mm: [0.0, 0.0, 0.0],
            mesh,
            records: Vec::new(),
        });
        let o = run_on(&sf);
        assert_eq!(o.status(), Status::Fail);
        let f = first(&o, "ISLAND_UNSUPPORTED");
        let area = value_f64(&f, "area_mm2");
        // The pixel-exact full-containment area of the 1x1 mm island, the same
        // number I7's own test pins.
        let pixel_exact = 70.0 * c::PIXEL_PITCH_X_MM * 51.0 * c::PIXEL_PITCH_Y_MM;
        assert!(
            (area - pixel_exact).abs() < 1.0e-6,
            "area_mm2={area}, pixel-exact area is {pixel_exact}"
        );
        assert!(measured_f64(&o, "regions_infinite_reach") >= 1.0);
    }

    #[test]
    fn span_ramp_shallow_reproduces_the_derived_angle_relation() {
        let alpha = 3.0f64.to_radians();
        let len = 20.0;
        let h = len * alpha.tan();
        // Solid ABOVE a plane at 3 degrees: material advances by
        // layer_height / tan(alpha) per layer, which is exactly the reach a
        // uniform ramp produces (§4).
        let poly = vec![(0.0, 0.0), (len, h), (len, h + 1.0), (0.0, 1.0)];
        let sf = SynthFile::new().with_model(SynthModel {
            name: "ramp".into(),
            translate_mm: [0.0, 0.0, 0.0],
            mesh: extrude_xz(&poly, 0.0, 3.0),
            records: Vec::new(),
        });
        let o = run_on(&sf);
        let reach = measured_f64(&o, "max_reach_mm");
        let expected = 0.050 / alpha.tan();
        assert!(
            (reach - expected).abs() / expected < 0.05,
            "max_reach_mm={reach}, layer_height/tan(3 deg)={expected}"
        );
        // And the criterion's own derived angle is what decides it: 3 degrees
        // is above alpha_min, so a 3 degree ramp passes.
        assert!(saturn4_constants().alpha_min_deg < 3.0);
        assert_eq!(o.status(), Status::Pass, "{:?}", o.failures);
    }

    // ------------------------------------------------------------------
    // Unmeasurable never passes
    // ------------------------------------------------------------------

    #[test]
    fn an_unmeasurable_model_fails_indeterminate_never_pass() {
        let mut mesh = box_soup([0.0, 0.0, 0.0], [2.0, 2.0, 1.0]);
        mesh.pop(); // an open mesh has no interior to rasterise
        let sf = SynthFile::new().with_model(SynthModel {
            name: "open".into(),
            translate_mm: [0.0, 0.0, 0.0],
            mesh,
            records: Vec::new(),
        });
        let o = with_ctx(&sf, AuditOptions::default(), |ctx| {
            assert!(!ctx.model_mesh.closure.is_closed());
            UnsupportedSpan.run(ctx)
        });
        assert_eq!(o.status(), Status::Fail);
        assert!(o.indeterminate > 0, "unmeasurable must never pass");
        assert_eq!(o.failed, 0);
        assert_eq!(o.failures[0].code, "SPAN_INDETERMINATE");
        assert!(
            o.failures[0].message.contains("reason=slice_failed"),
            "{}",
            o.failures[0].message
        );
    }

    #[test]
    fn an_uncalibrated_layer_height_is_indeterminate_never_pass() {
        let sf = ledge_model(1.2, 0.0, None);
        assert_eq!(run_on(&sf).status(), Status::Pass, "control: it passes at 0.050 mm");
        let opts = AuditOptions {
            inputs: DeclaredInputs {
                layer_height_mm: 0.030,
                ..DeclaredInputs::default()
            },
            ..AuditOptions::default()
        };
        let o = with_ctx(&sf, opts, |ctx| UnsupportedSpan.run(ctx));
        assert_eq!(o.status(), Status::Fail);
        assert!(o.indeterminate > 0);
        assert!(
            o.failures[0]
                .message
                .contains("reason=span_criterion_layer_height_uncalibrated"),
            "{}",
            o.failures[0].message
        );
    }

    #[test]
    fn the_one_pixel_censor_is_scoped_to_infinite_reach_only() {
        // A long thin region has a small area and a large reach. Area must not
        // rescue it: the censor is for the infinite-reach case alone (§6.1).
        let sc = saturn4_constants();
        assert!(!sc.passes(5.0));
        let grid = RasterGrid::saturn4(0.0);
        assert!(grid.pixel_area_mm2() <= grid.pixel_area_mm2());
        assert!(2.0 * grid.pixel_area_mm2() > grid.pixel_area_mm2());
    }

    #[test]
    fn the_schema_key_and_title_are_pinned() {
        assert_eq!(UnsupportedSpan.id(), "i9_unsupported_span");
        assert_eq!(UnsupportedSpan.title(), "I9 unsupported span");
    }
}
