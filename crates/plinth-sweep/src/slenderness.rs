//! How long a support member may run with nothing holding it sideways, and
//! what the engine does about a run that is longer than that.
//!
//! The owner's ruling of 2026-09-05: long thin verticals "with no lateral
//! support will be very unlikely to stay straight"; the fix is "a rule about
//! continuous length/radius allowable before lateral support is required";
//! the remedy is "merging first", then cross-bracing "but ANGLED, as an X, so
//! there is `|X|`" and no unsupported horizontal span, and "if there's NO
//! nearby other support to brace to, then we would need to drop a NEW support
//! and brace to that".
//!
//! # The constraint
//!
//! An unbraced member is a cantilever fixed at its last lateral tie. The
//! lateral load is resin drag on the retract/return stroke plus the shear of
//! the tilt release: a distributed `q` per unit length. For a cylinder of
//! radius `r` and modulus `E`, `I = π r⁴ / 4` and the tip deflection is
//!
//! ```text
//! δ = q L⁴ / (8 E I) = q L⁴ / (2π E r⁴)
//! ```
//!
//! A layer printed on a swayed arm lands where the arm is, not where it should
//! be, so the deflection that matters is ONE PIXEL. Solving `δ ≤ p`:
//!
//! ```text
//! L_max(r) = (2π E r⁴ p / q)^(1/4)  ∝  r      →      L_max = S · r
//! ```
//!
//! `S` is a pure ratio, which is the whole content of the derivation: the
//! admissible length scales with the radius and with nothing else. Its
//! MAGNITUDE is the registered Placeholder `SLENDERNESS_LIMIT_RATIO`, whose
//! note carries the order-of-magnitude inputs and the 40-to-70 bracket they
//! give. Nothing in this module declares a physical constant of its own; every
//! other number here is derived at the site from `S`, from the registered
//! minimum and maximum tip radii, and from the overhang law the descent
//! already obeys.
//!
//! # Where it is evaluated, and why here rather than inside the down-sweep
//!
//! The constraint needs SIZED radii, and the down-sweep is what sizes them; it
//! also needs to know an arm's ties, and a tie is a whole-arm property. The
//! down-sweep's per-layer insertion round is neither: it sees one layer's
//! routed load in one cell and has no way to ask "how far is it from here to
//! this arm's last tie". Worse, remedy 1 RE-CHOOSES A DESCENT, and a descent is
//! chosen against the up-sweep's [`crate::up_sweep::ColumnHistory`], which
//! `down_sweep::run` is not given and could not be given without handing the
//! sizing pass the placement's state.
//!
//! So this pass runs in the sweep driver's own iterate — the same place the
//! removal passes and the un-merge passes already re-run the down-sweep — after
//! a completed down-sweep has sized every arm, and the down-sweep is re-run
//! after it so that every brace and every mast is sized and every junction it
//! made splits its shaft's ledger. The two stay consistent because the pass
//! never invents a radius: it changes the strut set and then asks the same
//! down-sweep the same question again.
//!
//! # The remedies, in order
//!
//! 1. **Merge.** Re-choose the arm's descent so it lands on a reachable shaft
//!    inside its own `L_max`. Straight down past `L_max` is inadmissible when a
//!    shaft landing exists.
//! 2. **`|X|` brace.** Two crossing diagonals over one height band, each seated
//!    ON one shaft and landing on the other. Each is inside the overhang law by
//!    construction — its band is `LeanLaw::layers_to_reach(d)` layers deep — so
//!    no segment is horizontal and none has zero rise.
//! 3. **Mast.** A new plate-rooted post at the nearest admissible plate cell,
//!    then an `|X|` to it.
//! 4. **Warn.** Named, counted, emitted. Never a dropped or shortened arm.

use crate::drain::DrainKeepOut;
use crate::grid::Grid;
use crate::lean::LeanLaw;
use crate::up_sweep::{ColumnHistory, ShaftSpan};
use crate::{Landing, StrutKind, SweepStrut};

/// The slenderness law, evaluated once for one run and then read by everything
/// the lane does.
///
/// `S` is the registered ratio; every other quantity here is derived from it,
/// from the registered radius clamps, and from the overhang law's own saturated
/// angle. Nothing in this struct is a second constant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlendernessLaw {
    ratio: f64,
    r_min_mm: f64,
    r_max_mm: f64,
    /// `sec(θ_sat)` — the length of a diagonal per unit of GAP at the shallowest
    /// angle the overhang law admits. `√(1 + tan² θ_sat)`, written as the
    /// secant because that is what it is.
    secant_sat: f64,
}

impl SlendernessLaw {
    /// Build the law from the registered ratio, the registered radius clamps
    /// and the overhang law's own saturated angle.
    ///
    /// A non-finite or non-positive input yields [`Self::disabled`] rather than
    /// a panic: emit and warn, never refuse.
    #[must_use]
    pub fn new(ratio: f64, r_min_mm: f64, r_max_mm: f64, saturated_deg: f64) -> Self {
        let sane = ratio.is_finite()
            && ratio > 0.0
            && r_min_mm.is_finite()
            && r_min_mm > 0.0
            && r_max_mm.is_finite()
            && r_max_mm >= r_min_mm
            && saturated_deg.is_finite()
            && saturated_deg > 0.0
            && saturated_deg < 90.0;
        if !sane {
            return Self::disabled();
        }
        let tangent = saturated_deg.to_radians().tan();
        Self {
            ratio,
            r_min_mm,
            r_max_mm,
            secant_sat: (1.0 + tangent * tangent).sqrt(),
        }
    }

    /// The law that constrains nothing. **Sabotage and comparison only**: with
    /// it in force every unbraced run is admissible and the lane is a no-op, so
    /// a field placed under it is the field this engine placed before
    /// 2026-09-06.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            ratio: 0.0,
            r_min_mm: 0.0,
            r_max_mm: 0.0,
            secant_sat: 1.0,
        }
    }

    /// Whether this law constrains anything at all.
    #[must_use]
    pub fn binds(&self) -> bool {
        self.ratio > 0.0
    }

    /// `S`.
    #[must_use]
    pub const fn ratio(&self) -> f64 {
        self.ratio
    }

    /// `L_max(r) = S · r` — the longest run of member at radius `r` that may
    /// stand with nothing holding it sideways.
    #[must_use]
    pub fn max_unbraced_mm(&self, radius_mm: f64) -> f64 {
        if !self.binds() || !radius_mm.is_finite() || radius_mm <= 0.0 {
            return f64::INFINITY;
        }
        self.ratio * radius_mm
    }

    /// `r_b = max(r_min, ℓ / S)` — the radius a diagonal of length `ℓ` needs so
    /// that it satisfies its OWN slenderness limit.
    ///
    /// The area law does not size a brace: a brace carries no peel event of its
    /// own. Its own straightness does, and this is that requirement read as a
    /// radius rather than as a length.
    #[must_use]
    pub fn brace_radius_mm(&self, length_mm: f64) -> f64 {
        if !self.binds() {
            return self.r_min_mm;
        }
        (length_mm / self.ratio).max(self.r_min_mm)
    }

    /// `d_max(r) = S · r / √(1 + tan² θ_sat)` — how far sideways a brace of
    /// radius `r` can reach.
    ///
    /// The geometry closes on itself: a diagonal across a gap `d` must rise at
    /// least `d · tan θ_sat` to stay inside the overhang law, so its length is
    /// at least `d · sec θ_sat`, and its length may not exceed `S · r`. The
    /// reach is that inequality solved for `d`.
    #[must_use]
    pub fn brace_reach_mm(&self, radius_mm: f64) -> f64 {
        if !self.binds() {
            return 0.0;
        }
        self.ratio * radius_mm / self.secant_sat
    }

    /// The reach of the THINNEST brace the printer can make — the number the
    /// plan of record calls `d_max`.
    #[must_use]
    pub fn thinnest_brace_reach_mm(&self) -> f64 {
        self.brace_reach_mm(self.r_min_mm)
    }

    /// The reach of the THICKEST brace the registry admits.
    ///
    /// The plan's own words are that `d_max` is the thinnest brace's reach,
    /// "larger if a thicker brace is accepted". A thicker brace IS accepted
    /// here — `brace_radius_mm` grows the diagonal until it satisfies its own
    /// limit — so this, not the thin figure, is the distance at which the
    /// search stops looking for a partner. Both are published.
    #[must_use]
    pub fn thickest_brace_reach_mm(&self) -> f64 {
        self.brace_reach_mm(self.r_max_mm)
    }
}

/// One run of member with nothing holding it sideways.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnbracedRun {
    /// The member this run belongs to.
    pub strut_id: u32,
    /// The lower tie's layer, and the upper tie's.
    pub k_lo: usize,
    pub k_top: usize,
    /// The run measured along the member's own axis, slant by slant.
    pub length_mm: f64,
    /// The NARROWEST radius anywhere on the run. The arm tapers, and a
    /// cantilever fails at its narrowest section.
    pub radius_mm: f64,
    /// `S · radius_mm`.
    pub limit_mm: f64,
}

impl UnbracedRun {
    /// Whether this run is longer than its own limit.
    #[must_use]
    pub fn over_limit(&self) -> bool {
        self.length_mm > self.limit_mm
    }
}

/// A member the lane could not tie, named so the run report can warn about it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnbracedWarning {
    pub strut_id: u32,
    pub k_lo: usize,
    pub k_top: usize,
    pub length_mm: f64,
    pub limit_mm: f64,
}

/// What one slenderness pass did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SlendernessOutcome {
    /// `S` in force. `0` means the lane was disabled.
    pub ratio: f64,
    /// `d_max` at the thinnest brace, and at the thickest.
    pub thin_brace_reach_mm: f64,
    pub thick_brace_reach_mm: f64,
    /// Members with at least one over-limit run BEFORE any remedy.
    pub arms_over_limit_before: usize,
    /// Over-limit RUNS before any remedy.
    pub runs_over_limit_before: usize,
    /// Remedy 1: descents re-chosen onto a shaft inside the arm's own limit.
    pub merges_forced: usize,
    /// Remedy 2: `|X|` cross-braces added. Each is TWO diagonals.
    pub braces_added: usize,
    /// Remedy 3: plate-rooted masts dropped because nothing was in reach.
    pub masts_dropped: usize,
    /// Remedy 4: runs that stayed over the limit, one entry each.
    pub warnings: Vec<UnbracedWarning>,
    /// The worst run left standing, and the limit it is measured against.
    pub longest_run_mm: f64,
    pub longest_run_limit_mm: f64,
    /// Partner searches that found a shaft but could not use it because the
    /// band did not fit inside both members. A WARNING quantity.
    pub brace_bands_refused: usize,
    /// Mast searches that found no admissible plate cell — no cell inside
    /// `d_max` that was outside the model, free of a shaft, out of a drain's
    /// way AND clear of every root flare already on the plate.
    pub mast_cells_refused: usize,
    /// Masts whose FLARED root disc still laps another plate root once the
    /// final down-sweep has sized the field.
    ///
    /// **This is not a placement failure and it is published rather than
    /// asserted away.** The cell is chosen against `r_m = max(r_min, H_first/S)`
    /// — the mast's own slenderness radius — because that is the only radius
    /// that exists at the moment of choosing. The down-sweep afterwards sizes
    /// the mast from what the cross actually ties into it, and the area law can
    /// hand it more section than `r_m`; two roots that cleared at placement can
    /// then grow into one another. Measured 2026-09-07: `make_lonely_peak`
    /// leaves 2 such pairs with the rule on AND with it off — identical fields,
    /// so the rule is not what fails there — while `make_laid_tube` goes from
    /// 127 lapping mast pairs with the rule deleted to 0 with it in force.
    pub mast_roots_overlapping_after_sizing: usize,
    /// Forced merges the un-merge pass put BACK because the trunk they landed
    /// on could not grow far enough to carry the sum. Their runs return as
    /// warnings; the arm is never dropped.
    pub merges_undone_by_radius: usize,
}

/// Every unbraced run in a finished field, measured from the strut set alone.
///
/// **Batch-first**: one pass over the struts to collect every tie, one pass to
/// cut the runs. Nothing here scans the grid and nothing is per-layer.
///
/// A member's ties are its own two ends — its seat and its landing, both of
/// which are held laterally by the thing they are attached to — plus every
/// layer at which another member lands on it, plus every layer at which a brace
/// SEATS on it. The last of those is why [`SweepStrut::seat_on_strut`] exists:
/// a brace's bottom is in the ledger as a landing, its top is not in the ledger
/// at all, and an `|X|` ties both shafts at both ends of its band.
#[must_use]
pub fn unbraced_runs(
    grid: &Grid,
    struts: &[SweepStrut],
    layer_height_mm: f64,
    law: &SlendernessLaw,
) -> Vec<UnbracedRun> {
    let mut ties: Vec<Vec<usize>> = vec![Vec::new(); struts.len()];
    for strut in struts {
        if strut.k_bottom == usize::MAX || strut.k_top <= strut.k_bottom {
            continue;
        }
        if let Landing::Strut { id, layer } = strut.landing {
            if let Some(list) = ties.get_mut(id as usize) {
                list.push(layer);
            }
        }
        if let Some(parent) = strut.seat_on_strut {
            if let Some(list) = ties.get_mut(parent as usize) {
                list.push(strut.k_top);
            }
        }
    }

    let mut runs = Vec::new();
    for (index, strut) in struts.iter().enumerate() {
        if strut.k_bottom == usize::MAX || strut.k_top <= strut.k_bottom {
            continue;
        }
        let list = &mut ties[index];
        list.push(strut.k_bottom);
        list.push(strut.k_top);
        list.sort_unstable();
        list.dedup();
        for pair in list.windows(2) {
            let (k_lo, k_top) = (pair[0], pair[1]);
            if k_top <= k_lo {
                continue;
            }
            let (length_mm, radius_mm) =
                run_length_and_radius(grid, strut, k_lo, k_top, layer_height_mm);
            runs.push(UnbracedRun {
                strut_id: strut.id,
                k_lo,
                k_top,
                length_mm,
                radius_mm,
                limit_mm: law.max_unbraced_mm(radius_mm),
            });
        }
    }
    runs
}

/// The length of one run along the member's own axis, and its narrowest radius.
///
/// **The length is the STRAIGHT CHORD, not the sum of the rasterised path's
/// per-layer slants, and the difference is not cosmetic.** What the emit
/// boundary builds for a member is ONE straight frustum from its seat point to
/// its landing point (`plinth_support::lower_sweep_arms`); the per-layer path
/// is how the placement rasterises that line onto the working grid, and its
/// steps round independently in row and column, so the polyline is up to about
/// 40 % longer than the line it approximates. Measuring the polyline made the
/// lane report runs that the member it had just built could not physically
/// have — 2.939 mm on a 23-layer arm whose angle law admits at most 2.14 mm
/// (Sphere, 2026-09-06) — and it made the remedy's own admissibility test
/// disagree with the measurement that grades it. The chord is what is printed
/// and the chord is what is measured.
fn run_length_and_radius(
    grid: &Grid,
    strut: &SweepStrut,
    k_lo: usize,
    k_top: usize,
    layer_height_mm: f64,
) -> (f64, f64) {
    #[expect(
        clippy::cast_precision_loss,
        reason = "layer counts bounded by the plan's own layer count are exact in f64"
    )]
    let span = (strut.k_top - strut.k_bottom) as f64;
    let lateral_mm = crate::cell_distance_mm(grid, strut.cell, strut.base_cell());
    let member_mm = (span * layer_height_mm).hypot(lateral_mm);
    #[expect(
        clippy::cast_precision_loss,
        reason = "layer counts bounded by the plan's own layer count are exact in f64"
    )]
    let fraction = (k_top - k_lo) as f64 / span;
    let length = member_mm * fraction;

    let mut radius = f64::INFINITY;
    for layer in k_lo..k_top {
        if let Some(slot) = layer.checked_sub(strut.k_bottom) {
            if let Some(here) = strut.radius_mm.get(slot) {
                if *here > 0.0 {
                    radius = radius.min(*here);
                }
            }
        }
    }
    if !radius.is_finite() {
        radius = strut.tip_radius_mm();
    }
    (length, radius)
}

/// Everything the remedy ladder needs that is not the strut set itself.
#[derive(Clone, Copy)]
pub struct SlendernessContext<'a> {
    pub grid: &'a Grid,
    pub history: &'a ColumnHistory,
    pub keep_out: &'a DrainKeepOut,
    pub lean: &'a LeanLaw,
    pub law: &'a SlendernessLaw,
    pub layer_height_mm: f64,
    pub layers: usize,
    /// `FLARE_R_FACTOR` — the registered factor the emitter's root flare uses.
    /// A mast is plate-rooted and gets a root flare like any arm, so the mast
    /// search has to answer the congestion lane's question before it lands one.
    pub flare_factor: f64,
    /// Sabotage knob: whether a mast's cell must CLEAR the placed root discs,
    /// or merely be an empty cell as it was before 2026-09-06.
    pub mast_root_clearance: bool,
    /// Whether a member may land part-way down another member's shaft at all.
    /// Remedy 1 IS a shaft landing, so the descent knob that forbids one must
    /// forbid the other; otherwise the sabotage drill measures a rule it did
    /// not actually delete.
    pub shaft_landings: bool,
    /// Sabotage knobs, one per rung of the ladder.
    pub merge: bool,
    pub brace: bool,
    pub mast: bool,
    pub max_struts: usize,
}

/// Apply the remedy ladder to a sized field, in place.
///
/// Returns what it did. The caller re-runs the down-sweep when anything was
/// added or moved, because a brace with no radius and a junction with no
/// ledger entry is not a brace.
pub fn apply_remedies(
    context: &SlendernessContext<'_>,
    struts: &mut Vec<SweepStrut>,
) -> SlendernessOutcome {
    let law = context.law;
    let mut outcome = SlendernessOutcome {
        ratio: law.ratio(),
        thin_brace_reach_mm: law.thinnest_brace_reach_mm(),
        thick_brace_reach_mm: law.thickest_brace_reach_mm(),
        ..SlendernessOutcome::default()
    };
    if !law.binds() {
        return outcome;
    }

    let runs = unbraced_runs(context.grid, struts, context.layer_height_mm, law);
    let mut over: Vec<UnbracedRun> = runs.into_iter().filter(UnbracedRun::over_limit).collect();
    outcome.runs_over_limit_before = over.len();
    let mut named: Vec<u32> = over.iter().map(|run| run.strut_id).collect();
    named.sort_unstable();
    named.dedup();
    outcome.arms_over_limit_before = named.len();
    if over.is_empty() {
        return outcome;
    }
    // Deterministic order, and it does not depend on floating point: the arm
    // with the lowest id first, then the lowest run.
    over.sort_unstable_by(|a, b| {
        a.strut_id
            .cmp(&b.strut_id)
            .then_with(|| a.k_lo.cmp(&b.k_lo))
    });

    // The shaft index over the FINISHED field. `ColumnHistory::shaft_spans` as
    // the up-sweep left it names only the arms the up-sweep placed; the
    // down-sweep has added its own since, and a brace may tie to any of them.
    let mut index = ColumnHistory::new(context.grid);
    stamp_all(&mut index, struts);

    // Remedy 1 first, for every run it can serve, because a merge costs no new
    // member at all. It moves an arm's base, so the shaft index is rebuilt
    // afterwards rather than patched.
    let mut merged_any = false;
    if context.merge && context.shaft_landings {
        for run in &over {
            let Some(strut) = struts.get(run.strut_id as usize) else {
                continue;
            };
            // Only a run that reaches the arm's own base can be cured by
            // re-choosing where that base lands.
            // `k_lo` is the run's LOWER tie and `k_top` its upper one; the arm's
            // own ends are `k_bottom` and `k_top`. The pairing looks mismatched
            // and is not: only a run that spans the WHOLE arm can be cured by
            // re-choosing where that arm lands.
            #[expect(
                clippy::suspicious_operation_groupings,
                reason = "the run's lower end is named k_lo and the arm's is named k_bottom"
            )]
            let spans_whole_arm = run.k_lo == strut.k_bottom && run.k_top == strut.k_top;
            if !spans_whole_arm {
                continue;
            }
            if matches!(strut.landing, Landing::Strut { .. }) {
                continue;
            }
            let Some((landing, k_bottom, path)) =
                merge_candidate(context, &index, struts, run.strut_id, run.limit_mm)
            else {
                continue;
            };
            let strut = &mut struts[run.strut_id as usize];
            // WHERE THIS ARM WAS BEFORE THE CONSTRAINT MOVED IT. The un-merge
            // pass puts an arm back when the trunk it landed on cannot grow far
            // enough to carry the sum (owner: "if the merged radius would exceed
            // the 1.0 mm maximum, do not merge"), and it can only do that for an
            // arm that recorded somewhere to go back TO. Measured on
            // `make_overhang_table`, 2026-09-06: without this the nine merges
            // this lane forces drove 1828 overloaded arm-layers that nothing
            // could undo.
            strut.straight_fallback = Some((strut.landing, strut.k_bottom));
            strut.fallback_path = std::mem::take(&mut strut.path);
            strut.landing = landing;
            strut.k_bottom = k_bottom;
            strut.path = path;
            outcome.merges_forced += 1;
            merged_any = true;
        }
    }
    if merged_any {
        index = ColumnHistory::new(context.grid);
        stamp_all(&mut index, struts);
        let runs = unbraced_runs(context.grid, struts, context.layer_height_mm, law);
        over = runs.into_iter().filter(UnbracedRun::over_limit).collect();
        over.sort_unstable_by(|a, b| {
            a.strut_id
                .cmp(&b.strut_id)
                .then_with(|| a.k_lo.cmp(&b.k_lo))
        });
    }

    // THE ROOT DISCS ALREADY ON THE PLATE, as the congestion lane measures
    // them. A mast is plate-rooted and lays a root flare, so dropping one on
    // top of a root that is already there re-creates exactly the stacking the
    // congestion lane just spent four rounds relieving. Measured at 3ff52c7:
    // `make_laid_tube` left the congestion lane at 1.0004 with 2 overlapping
    // pairs and its FINISHED file read 1.0926 with 155, and the Sphere — where
    // the congestion lane ran zero rounds — still finished at 1.0068 with 8,
    // entirely from masts.
    //
    // The list is built ONCE here, from the sized field, and each mast this
    // lane drops is appended, so masts cannot be stacked on each other either.
    // A merge that has just taken an arm OFF the plate leaves a stale disc
    // behind; that is the conservative direction (it only ever refuses a cell)
    // and it is left standing rather than guessed at.
    let mut placed_roots =
        crate::congestion::root_discs(context.grid, context.history, struts, context.flare_factor);

    // Remedies 2 and 3, run tie point by tie point up each surviving run.
    for run in &over {
        let wanted = extra_ties(run);
        for step in 1..=wanted {
            if struts.len() + 2 > context.max_struts {
                break;
            }
            let Some(k) = tie_layer(run, step, wanted) else {
                continue;
            };
            let tied = context.brace
                && try_brace(context, &index, struts, run.strut_id, k, &mut outcome);
            if tied {
                stamp_last_two(&mut index, struts);
                continue;
            }
            if context.mast
                && try_mast(
                    context,
                    &mut index,
                    struts,
                    &mut placed_roots,
                    run.strut_id,
                    k,
                    &mut outcome,
                )
            {
                // `try_mast` stamped the mast itself before tying the cross to
                // it; the cross's two members follow it, in member order.
                stamp_last_two(&mut index, struts);
                continue;
            }
        }
    }

    // The measurement AFTER the lane, from the same function that measured it
    // before: a warning is a run the ladder did not close, read off the
    // finished set rather than remembered from the attempt.
    let runs = unbraced_runs(context.grid, struts, context.layer_height_mm, law);
    for run in &runs {
        if run.length_mm > outcome.longest_run_mm {
            outcome.longest_run_mm = run.length_mm;
            outcome.longest_run_limit_mm = run.limit_mm;
        }
        if run.over_limit() {
            outcome.warnings.push(UnbracedWarning {
                strut_id: run.strut_id,
                k_lo: run.k_lo,
                k_top: run.k_top,
                length_mm: run.length_mm,
                limit_mm: run.limit_mm,
            });
        }
    }
    outcome
}

/// How many extra ties one over-limit run needs.
///
/// `n` ties cut a run into `n + 1` pieces, and every piece must fit inside the
/// limit, so `n = ceil(length / limit) - 1`.
fn extra_ties(run: &UnbracedRun) -> usize {
    if !(run.limit_mm.is_finite() && run.limit_mm > 0.0) || !run.length_mm.is_finite() {
        return 0;
    }
    let pieces = (run.length_mm / run.limit_mm).ceil();
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "a ratio of two positive lengths, ceil-ed, and the run is known over-limit so it \
              is at least 2"
    )]
    let pieces = pieces.max(1.0) as usize;
    pieces.saturating_sub(1)
}

/// The layer of the `step`-th of `wanted` evenly spaced ties inside a run.
fn tie_layer(run: &UnbracedRun, step: usize, wanted: usize) -> Option<usize> {
    let span = run.k_top.checked_sub(run.k_lo)?;
    if span < 2 || wanted == 0 {
        return None;
    }
    let at = run.k_lo + span * step / (wanted + 1);
    (at > run.k_lo && at < run.k_top).then_some(at)
}

fn stamp_all(index: &mut ColumnHistory, struts: &[SweepStrut]) {
    for strut in struts {
        if strut.k_bottom == usize::MAX || strut.k_top < strut.k_bottom {
            continue;
        }
        index.stamp_shaft(
            strut.id,
            strut.k_bottom,
            strut.k_top,
            strut.cell,
            &strut.path,
        );
    }
}

fn stamp_last_two(index: &mut ColumnHistory, struts: &[SweepStrut]) {
    stamp_tail(index, struts, 2);
}

fn stamp_tail(index: &mut ColumnHistory, struts: &[SweepStrut], count: usize) {
    let from = struts.len().saturating_sub(count);
    for strut in &struts[from..] {
        if strut.k_bottom == usize::MAX || strut.k_top < strut.k_bottom {
            continue;
        }
        index.stamp_shaft(
            strut.id,
            strut.k_bottom,
            strut.k_top,
            strut.cell,
            &strut.path,
        );
    }
}

/// Remedy 1: a shaft landing this arm can reach INSIDE its own limit.
///
/// The search is the one the up-sweep descent already does, with one extra
/// admissibility test: a candidate whose descent would leave a run longer than
/// `limit_mm` is not admissible, however cheap it is. Straight down past the
/// limit is therefore not admissible either — it is simply not in this
/// candidate set.
fn merge_candidate(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    struts: &[SweepStrut],
    id: u32,
    limit_mm: f64,
) -> Option<(Landing, usize, Vec<u32>)> {
    let strut = struts.get(id as usize)?;
    let grid = context.grid;
    let lean = context.lean;
    if !lean.leans() {
        return None;
    }
    // The furthest sideways any admissible descent can travel: no descent is
    // shorter than the straight line it covers, so a landing further away than
    // the limit itself can never be inside it.
    let radius_cells = {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a finite non-negative length divided by the grid pitch, floored to a bound"
        )]
        let cells = (limit_mm / grid.pitch_mm()).floor().max(0.0) as usize;
        cells
    };
    if radius_cells == 0 {
        return None;
    }
    let cols = grid.cols();
    let seat_row = grid.row_of(strut.cell);
    let seat_col = grid.col_of(strut.cell);
    let row_lo = seat_row.saturating_sub(radius_cells);
    let row_hi = (seat_row + radius_cells).min(grid.rows() - 1);
    let col_lo = seat_col.saturating_sub(radius_cells);
    let col_hi = (seat_col + radius_cells).min(cols - 1);

    let mut best: Option<(f64, usize, u32, usize)> = None;
    for row in row_lo..=row_hi {
        let base = row * cols;
        for col in col_lo..=col_hi {
            let candidate = base + col;
            if candidate == strut.cell {
                continue;
            }
            let lateral_mm = crate::cell_distance_mm(grid, strut.cell, candidate);
            if lateral_mm > limit_mm {
                continue;
            }
            let drop = lean.layers_to_reach(lateral_mm);
            let Some(ceiling) = strut.k_top.checked_sub(drop) else {
                continue;
            };
            if drop == 0 {
                continue;
            }
            let Some((parent, at, _)) = index.shaft_landing(candidate, ceiling) else {
                continue;
            };
            if parent == id || descends_through(struts, parent, id) {
                continue;
            }
            let layers = strut.k_top.saturating_sub(at);
            if layers == 0 {
                continue;
            }
            #[expect(
                clippy::cast_precision_loss,
                reason = "a layer count bounded by the plan's own layer count is exact in f64"
            )]
            let rise_mm = layers as f64 * context.layer_height_mm;
            let length_mm = rise_mm.hypot(lateral_mm);
            if length_mm > limit_mm {
                continue;
            }
            if lean.descent_cost_mm(layers, lateral_mm).is_none() {
                continue;
            }
            // Shortest first, then the lowest cell index, so the winner does
            // not depend on the scan order.
            let key = (length_mm, candidate, parent, at);
            if best.is_none_or(|(best_len, best_cell, _, _)| {
                (length_mm.total_cmp(&best_len), candidate) < (core::cmp::Ordering::Equal, best_cell)
            }) {
                best = Some(key);
            }
        }
    }
    let (_, cell, parent, at) = best?;
    let path = segment_path(context, index, strut.cell, strut.k_top, cell, at, Some(parent))?;
    Some((Landing::Strut { id: parent, layer: at }, at, path))
}

/// Whether `from` reaches `target` by following landings downward.
///
/// A member may not land on something that already lands on it: the ledger's
/// own Kahn ordering would find the cycle and report it, and a cycle in the
/// load path is not a structure. The chain is short by construction and is
/// bounded here anyway.
fn descends_through(struts: &[SweepStrut], from: u32, target: u32) -> bool {
    let mut at = from;
    for _ in 0..struts.len().min(64) {
        if at == target {
            return true;
        }
        let Some(strut) = struts.get(at as usize) else {
            return false;
        };
        match strut.landing {
            Landing::Strut { id, .. } => at = id,
            Landing::Plate | Landing::Model { .. } => return false,
        }
    }
    false
}

/// The cells a straight segment from `(base_cell, k_bottom)` up to
/// `(seat_cell, k_top)` occupies, or `None` when it cannot have them.
///
/// The same interpolation, the same per-layer sampling and the same three
/// refusals [`crate::up_sweep`]'s own `lean_path` uses — model material,
/// another member, a drain's escape cylinder — with one difference that is
/// stated rather than hidden: the model test here is
/// [`ColumnHistory::model_blocks`], which asks whether model stands at THIS
/// layer, because this pass runs after the sweep and `top_model` by then names
/// each column's topmost model layer overall.
fn segment_path(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    seat_cell: usize,
    k_top: usize,
    base_cell: usize,
    k_bottom: usize,
    except: Option<u32>,
) -> Option<Vec<u32>> {
    let grid = context.grid;
    let cols = grid.cols();
    let span = k_top.checked_sub(k_bottom)?;
    if span == 0 {
        return None;
    }
    let seat_row = grid.row_of(seat_cell);
    let seat_col = grid.col_of(seat_cell);
    let base_row = grid.row_of(base_cell);
    let base_col = grid.col_of(base_cell);
    #[expect(
        clippy::cast_precision_loss,
        reason = "a layer count bounded by the plan's own layer count is exact in f64"
    )]
    let span_f = span as f64;
    let mut path = Vec::with_capacity(span);
    for slot in 0..span {
        let here = k_bottom + slot;
        #[expect(
            clippy::cast_precision_loss,
            reason = "a slot index bounded by the layer count is exact in f64"
        )]
        let fraction = slot as f64 / span_f;
        #[expect(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "an interpolation between two in-range grid indices, rounded back onto the \
                  grid and then clamped by construction"
        )]
        let row = ((base_row as f64) + ((seat_row as f64) - (base_row as f64)) * fraction)
            .round()
            .max(0.0) as usize;
        #[expect(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "an interpolation between two in-range grid indices, rounded back onto the \
                  grid and then clamped by construction"
        )]
        let col = ((base_col as f64) + ((seat_col as f64) - (base_col as f64)) * fraction)
            .round()
            .max(0.0) as usize;
        let row = row.min(grid.rows() - 1);
        let col = col.min(cols - 1);
        let cell = row * cols + col;
        if !context.keep_out.admits(cell, here) {
            return None;
        }
        if slot > 0 {
            if context.history.model_blocks(cell, here) {
                return None;
            }
            if index.shaft_blocks(cell, here, except) {
                return None;
            }
        }
        path.push(u32::try_from(cell).unwrap_or(u32::MAX));
    }
    Some(path)
}

/// Remedy 2: tie arm `id` at layer `k` to a neighbouring shaft with an `|X|`.
///
/// Two diagonals over ONE height band: one seated on `A` at the top of the band
/// and landing on `B` at its bottom, and its twin seated on `B` at the top and
/// landing on `A` at the bottom. They cross in the middle, so the pair braces
/// the pair in both directions and NOTHING IS HORIZONTAL: the band is
/// `LeanLaw::layers_to_reach(d)` layers deep, which is the fewest layers the
/// overhang law admits for a lateral move of `d`, so every diagonal rises.
fn try_brace(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    struts: &mut Vec<SweepStrut>,
    id: u32,
    k: usize,
    outcome: &mut SlendernessOutcome,
) -> bool {
    let Some(partner) = nearest_partner(context, index, struts, id, k) else {
        return false;
    };
    let (partner_id, _) = partner;
    if build_cross(context, index, struts, id, partner_id, k, outcome) {
        outcome.braces_added += 1;
        return true;
    }
    false
}

/// The nearest shaft, other than this arm's own, that stands at layer `k`
/// within the thickest brace's reach.
fn nearest_partner(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    struts: &[SweepStrut],
    id: u32,
    k: usize,
) -> Option<(u32, f64)> {
    let grid = context.grid;
    let strut = struts.get(id as usize)?;
    let here = strut.cell_at(k);
    let reach_mm = context.law.thickest_brace_reach_mm();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a finite non-negative length divided by the grid pitch, floored to a bound"
    )]
    let reach_cells = (reach_mm / grid.pitch_mm()).floor().max(0.0) as usize;
    if reach_cells == 0 {
        return None;
    }
    let cols = grid.cols();
    let row0 = grid.row_of(here);
    let col0 = grid.col_of(here);
    let mut best: Option<(u32, f64, usize)> = None;
    for row in row0.saturating_sub(reach_cells)..=(row0 + reach_cells).min(grid.rows() - 1) {
        let base = row * cols;
        for col in col0.saturating_sub(reach_cells)..=(col0 + reach_cells).min(cols - 1) {
            let cell = base + col;
            if cell == here {
                continue;
            }
            let distance_mm = crate::cell_distance_mm(grid, here, cell);
            if distance_mm > reach_mm || distance_mm <= 0.0 {
                continue;
            }
            let Some(candidate) = spanning_member(index, cell, k) else {
                continue;
            };
            if candidate == id {
                continue;
            }
            let Some(other) = struts.get(candidate as usize) else {
                continue;
            };
            // A brace to a member of this same `|X|` family would tie a brace
            // to a brace, which holds nothing: only load-bearing shafts are
            // partners.
            if other.kind == StrutKind::Brace {
                continue;
            }
            if best.is_none_or(|(_, best_distance, best_cell)| {
                (distance_mm.total_cmp(&best_distance), cell) < (core::cmp::Ordering::Equal, best_cell)
            }) {
                best = Some((candidate, distance_mm, cell));
            }
        }
    }
    best.map(|(candidate, distance_mm, _)| (candidate, distance_mm))
}

/// The member whose shaft stands in this column at exactly this layer, if any.
fn spanning_member(index: &ColumnHistory, cell: usize, layer: usize) -> Option<u32> {
    let mut slot = *index.shaft_head.get(cell)?;
    let mut best: Option<u32> = None;
    while slot != u32::MAX {
        let span: ShaftSpan = index.shaft_spans[slot as usize];
        slot = span.next;
        if (span.k_bottom as usize) <= layer && layer <= span.k_top as usize {
            best = Some(best.map_or(span.id, |current| current.min(span.id)));
        }
    }
    best
}

/// Build the two crossing diagonals, or leave the field untouched.
fn build_cross(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    struts: &mut Vec<SweepStrut>,
    a: u32,
    b: u32,
    k: usize,
    outcome: &mut SlendernessOutcome,
) -> bool {
    let grid = context.grid;
    let (a_cell_top, b_cell_top) = {
        let (Some(one), Some(two)) = (struts.get(a as usize), struts.get(b as usize)) else {
            return false;
        };
        (one.cell_at(k), two.cell_at(k))
    };
    let gap_mm = crate::cell_distance_mm(grid, a_cell_top, b_cell_top);
    if gap_mm <= 0.0 {
        return false;
    }
    let depth = context.lean.layers_to_reach(gap_mm);
    if depth == 0 {
        return false;
    }
    let Some(k_lo) = k.checked_sub(depth) else {
        outcome.brace_bands_refused += 1;
        return false;
    };
    let (a_cell_lo, b_cell_lo, floor_ok) = {
        let (Some(one), Some(two)) = (struts.get(a as usize), struts.get(b as usize)) else {
            return false;
        };
        (
            one.cell_at(k_lo),
            two.cell_at(k_lo),
            k_lo >= one.k_bottom && k_lo >= two.k_bottom && k <= one.k_top && k <= two.k_top,
        )
    };
    if !floor_ok {
        outcome.brace_bands_refused += 1;
        return false;
    }
    // Each diagonal's own lateral move, measured between the cells it actually
    // joins rather than assumed equal to the gap at the top of the band.
    let first_lateral = crate::cell_distance_mm(grid, a_cell_top, b_cell_lo);
    let second_lateral = crate::cell_distance_mm(grid, b_cell_top, a_cell_lo);
    if context.lean.descent_cost_mm(depth, first_lateral).is_none()
        || context.lean.descent_cost_mm(depth, second_lateral).is_none()
    {
        outcome.brace_bands_refused += 1;
        return false;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "a layer count bounded by the plan's own layer count is exact in f64"
    )]
    let rise_mm = depth as f64 * context.layer_height_mm;
    let first_len = rise_mm.hypot(first_lateral);
    let second_len = rise_mm.hypot(second_lateral);
    let radius_mm = context
        .law
        .brace_radius_mm(first_len.max(second_len));
    // **A DIAGONAL MAY NOT REACH THE PLATE.** A brace's frustum rings stand
    // perpendicular to its own tilted axis, so the ring at its base extends up
    // to one brace radius BELOW the axis point; a band whose bottom sits closer
    // to the plate than that puts emitted geometry under the plate, which the
    // emitted-file audit refuses outright (`plinth_emit`'s world-bounds check,
    // `min[2] < 0.0`) and which no printer could make in any case. The test is
    // the geometry's own: the bottom of the band, measured from the plate, must
    // clear the radius the ring is drawn at. Measured on the Sphere, 2026-09-06:
    // without it the whole file was refused at the emit boundary.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a layer index bounded by the plan's own layer count is exact in f64"
    )]
    let band_base_mm = k_lo as f64 * context.layer_height_mm;
    if band_base_mm <= radius_mm {
        outcome.brace_bands_refused += 1;
        return false;
    }
    let Some(first_path) =
        segment_path(context, index, a_cell_top, k, b_cell_lo, k_lo, Some(b))
    else {
        outcome.brace_bands_refused += 1;
        return false;
    };
    let Some(second_path) =
        segment_path(context, index, b_cell_top, k, a_cell_lo, k_lo, Some(a))
    else {
        outcome.brace_bands_refused += 1;
        return false;
    };
    push_brace(
        grid,
        struts,
        a_cell_top,
        k,
        k_lo,
        Landing::Strut { id: b, layer: k_lo },
        a,
        radius_mm,
        first_path,
    );
    push_brace(
        grid,
        struts,
        b_cell_top,
        k,
        k_lo,
        Landing::Strut { id: a, layer: k_lo },
        b,
        radius_mm,
        second_path,
    );
    true
}

#[expect(
    clippy::too_many_arguments,
    reason = "one diagonal genuinely needs the grid, the strut set, its seat cell and layer, its \
          base layer, its landing, the shaft it seats on, its radius and its path"
)]
fn push_brace(
    grid: &Grid,
    struts: &mut Vec<SweepStrut>,
    seat_cell: usize,
    k_top: usize,
    k_bottom: usize,
    landing: Landing,
    seat_on: u32,
    radius_mm: f64,
    path: Vec<u32>,
) {
    let id = u32::try_from(struts.len()).unwrap_or(u32::MAX);
    struts.push(SweepStrut {
        id,
        cell: seat_cell,
        center_mm: [
            grid.center_x_mm(grid.col_of(seat_cell)),
            grid.center_y_mm(grid.row_of(seat_cell)),
        ],
        kind: StrutKind::Brace,
        min_radius_mm: radius_mm,
        seat_on_strut: Some(seat_on),
        k_top,
        k_bottom,
        landing,
        interior: false,
        radius_mm: Vec::new(),
        carried_mm2: Vec::new(),
        covered_demand: Vec::new(),
        // A brace roots no demand cell, so the removal pass's coverage test
        // would be vacuously true for it; the same flag that protects a
        // load-inserted arm protects this one, for the same reason.
        inserted_by_load: true,
        path,
        seat_face: crate::SeatFace::default(),
        seat_normal: [0.0; 3],
        straight_fallback: None,
        fallback_path: Vec::new(),
    });
}

/// Remedy 3: drop a plate-rooted mast and brace to it.
///
/// **The cell has to be CLEAR, not merely empty.** The occupancy test used to
/// be a single per-cell question — is there a shaft head here — which is a
/// question about one cell and says nothing about the disc that lands on it. A
/// mast lays a root flare of `FLARE_R_FACTOR × r_m`, so a cell one pitch from a
/// 0.6 mm root is empty and not clear. The search now carries the candidate's
/// own flared radius and asks [`crate::congestion::clear_of_placed`] — the same
/// function, not a copy of the rule — against every root already on the plate.
fn try_mast(
    context: &SlendernessContext<'_>,
    index: &mut ColumnHistory,
    struts: &mut Vec<SweepStrut>,
    placed_roots: &mut Vec<crate::congestion::RootDisc>,
    id: u32,
    k: usize,
    outcome: &mut SlendernessOutcome,
) -> bool {
    let grid = context.grid;
    let Some(strut) = struts.get(id as usize) else {
        return false;
    };
    let here = strut.cell_at(k);
    let reach_mm = context.law.thickest_brace_reach_mm();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a finite non-negative length divided by the grid pitch, floored to a bound"
    )]
    let reach_cells = (reach_mm / grid.pitch_mm()).floor().max(0.0) as usize;
    if reach_cells == 0 {
        return false;
    }
    let cols = grid.cols();
    let row0 = grid.row_of(here);
    let col0 = grid.col_of(here);
    let flare = if context.flare_factor.is_finite() && context.flare_factor > 0.0 {
        context.flare_factor
    } else {
        1.0
    };
    // Nearest admissible cell wins: the least `(distance, cell)`, compared
    // lexicographically, among the cells that pass every test below. The box
    // is the same box `d_max` bounds, and every test is a refusal of THIS
    // cell, never of the search.
    //
    // The cheap per-cell tests run over the whole box; the one expensive test
    // -- the flared disc against every placed root, a scan of the whole plate
    // -- runs only in `(distance, cell)` order, and only until one cell passes
    // it. That first passing cell IS the least passing cell, so this picks
    // exactly the cell the one-pass nearest-so-far scan picked, without paying
    // the plate scan for every cell of a box up to 169 cells across.
    let mut candidates: Vec<(f64, usize, f64)> = Vec::new();
    for row in row0.saturating_sub(reach_cells)..=(row0 + reach_cells).min(grid.rows() - 1) {
        let base = row * cols;
        for col in col0.saturating_sub(reach_cells)..=(col0 + reach_cells).min(cols - 1) {
            let cell = base + col;
            if cell == here {
                continue;
            }
            let distance_mm = crate::cell_distance_mm(grid, here, cell);
            if distance_mm > reach_mm || distance_mm <= 0.0 {
                continue;
            }
            // A mast may not stand in the model's own footprint, and it may not
            // stand where the resin has to leave.
            if context.history.column_max_model[cell] != 0 {
                continue;
            }
            if index.shaft_head[cell] != u32::MAX {
                continue;
            }
            // THE MAST'S OWN RADIUS, at this candidate. Its slenderness has to
            // hold over the run from the plate up to its FIRST brace, which is
            // the bottom of the band the cross about to be built spans, and
            // that band depends on the gap to THIS cell. So the radius is a
            // property of the candidate and is computed per candidate:
            // `r_m = max(r_min, H_first / S)`.
            let depth = context.lean.layers_to_reach(distance_mm);
            if depth == 0 || depth > k {
                continue;
            }
            #[expect(
                clippy::cast_precision_loss,
                reason = "a layer count bounded by the plan's own layer count is exact in f64"
            )]
            let first_brace_mm = (k - depth) as f64 * context.layer_height_mm;
            let radius_mm = context.law.brace_radius_mm(first_brace_mm);
            candidates.push((distance_mm, cell, radius_mm));
        }
    }
    candidates.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    // AND ITS FLARED DISC HAS TO CLEAR EVERY ROOT ALREADY ON THE PLATE. The
    // plate does not change during this search, so it is indexed once and each
    // candidate asks the index (`congestion::PlacedIndex`: the same test as
    // `clear_of_placed`, disc by disc, over every disc that could touch).
    let best = {
        let plate = crate::congestion::PlacedIndex::build(placed_roots, None);
        candidates.into_iter().find(|&(_, cell, radius_mm)| {
            !context.mast_root_clearance || plate.clear(grid, cell, radius_mm * flare)
        })
    };
    // NO CLEAR CELL INSIDE `d_max` MEANS THE MAST IS REFUSED AND COUNTED, never
    // placed overlapping. The run it would have served comes back as a warning
    // from the census at the end of the lane — emit and warn, never refuse.
    let Some((_, cell, radius_mm)) = best else {
        outcome.mast_cells_refused += 1;
        return false;
    };
    if (0..=k).any(|layer| !context.keep_out.admits(cell, layer)) {
        outcome.mast_cells_refused += 1;
        return false;
    }
    let mast_id = u32::try_from(struts.len()).unwrap_or(u32::MAX);
    struts.push(SweepStrut {
        id: mast_id,
        cell,
        center_mm: [
            grid.center_x_mm(grid.col_of(cell)),
            grid.center_y_mm(grid.row_of(cell)),
        ],
        kind: StrutKind::Mast,
        min_radius_mm: radius_mm,
        seat_on_strut: None,
        k_top: k,
        k_bottom: 0,
        landing: Landing::Plate,
        interior: false,
        radius_mm: Vec::new(),
        carried_mm2: Vec::new(),
        covered_demand: Vec::new(),
        inserted_by_load: true,
        path: Vec::new(),
        seat_face: crate::SeatFace::default(),
        seat_normal: [0.0; 3],
        straight_fallback: None,
        fallback_path: Vec::new(),
    });
    // The mast has to be visible to the cross that ties to it. `index` already
    // holds every other member, stamped in member order (the caller builds it
    // with `stamp_all` and stamps each member it adds as it adds it), so
    // stamping the mast last gives exactly the index a fresh `stamp_all` over
    // `struts` would build -- the same spans, in the same order, in the same
    // slots -- without re-walking every shaft of the plan for each attempt.
    // If the cross is refused the mast's one span is taken back out, so the
    // caller's index again matches `struts` after the `pop` below.
    let spans_before = index.shaft_spans.len();
    index.stamp_shaft(mast_id, 0, k, cell, &[]);
    if build_cross(context, index, struts, id, mast_id, k, outcome) {
        // The mast is now a root on the plate, so the NEXT mast search has to
        // clear it too.
        crate::congestion::push_disc(
            placed_roots,
            grid,
            mast_id,
            cell,
            radius_mm * flare,
            radius_mm,
            false,
        );
        outcome.masts_dropped += 1;
        outcome.braces_added += 1;
        return true;
    }
    // The cross would not build. EMIT AND WARN: the mast is withdrawn rather
    // than left standing with nothing tied to it, and the run is reported.
    index.unstamp_straight_shaft(cell, spans_before);
    struts.pop();
    outcome.mast_cells_refused += 1;
    false
}

#[cfg(test)]
mod tests {
    use super::{extra_ties, SlendernessLaw, UnbracedRun};

    fn law() -> SlendernessLaw {
        SlendernessLaw::new(50.0, 0.05, 1.0, 32.5)
    }

    #[test]
    fn the_limit_is_the_ratio_times_the_radius_and_nothing_else() {
        let law = law();
        assert!((law.max_unbraced_mm(0.2) - 10.0).abs() < 1.0e-12);
        assert!((law.max_unbraced_mm(0.05) - 2.5).abs() < 1.0e-12);
    }

    /// The brace radius closes the geometry on itself: a diagonal sized by this
    /// rule always satisfies its OWN limit, at every length.
    #[test]
    fn a_brace_is_never_more_slender_than_the_limit_it_enforces() {
        let law = law();
        for tenths in 1..=200 {
            let length = f64::from(tenths) * 0.1;
            let radius = law.brace_radius_mm(length);
            assert!(
                length <= law.max_unbraced_mm(radius) + 1.0e-12,
                "a {length} mm diagonal at radius {radius} exceeds its own limit"
            );
        }
    }

    /// The reach is the same inequality read the other way: a partner exactly
    /// at `d_max(r)` needs a diagonal of exactly `S · r`.
    #[test]
    fn the_reach_and_the_radius_are_one_inequality_read_two_ways() {
        let law = law();
        for radius in [0.05, 0.2, 0.5, 1.0] {
            let d = law.brace_reach_mm(radius);
            let rise = d * 32.5_f64.to_radians().tan();
            let diagonal = d.hypot(rise);
            assert!(
                (diagonal - law.max_unbraced_mm(radius)).abs() < 1.0e-9,
                "at r = {radius} the reach {d} implies a diagonal of {diagonal}, not {}",
                law.max_unbraced_mm(radius)
            );
        }
    }

    #[test]
    fn a_run_needs_one_tie_per_whole_limit_it_overshoots() {
        let run = |length: f64, limit: f64| UnbracedRun {
            strut_id: 0,
            k_lo: 0,
            k_top: 100,
            length_mm: length,
            radius_mm: 0.1,
            limit_mm: limit,
        };
        assert_eq!(extra_ties(&run(4.0, 5.0)), 0);
        assert_eq!(extra_ties(&run(6.0, 5.0)), 1);
        assert_eq!(extra_ties(&run(10.0, 5.0)), 1);
        assert_eq!(extra_ties(&run(11.0, 5.0)), 2);
    }

    #[test]
    fn the_disabled_law_admits_every_run() {
        let law = SlendernessLaw::disabled();
        assert!(!law.binds());
        assert!(law.max_unbraced_mm(0.05).is_infinite());
    }
}
