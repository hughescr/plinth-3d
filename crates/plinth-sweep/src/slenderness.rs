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
//! # The remedies, in order (owner rulings of 2026-09-05 and 2026-09-29)
//!
//! 1. **Merge.** Re-choose the arm's descent so it lands on a reachable shaft
//!    inside its own `L_max`. Straight down past `L_max` is inadmissible when a
//!    shaft landing exists. The un-merge pass then puts back any merge whose
//!    trunk cannot grow to carry the sum, and ONLY THEN are the ties placed:
//!    "bracing runs after `unmerge_overgrown`", so no post the un-merge puts
//!    back on the plate is left unmeasured.
//! 2. **`|X|` brace.** Two crossing diagonals over one height band, each seated
//!    ON one shaft and landing on the other. Each is inside the overhang law by
//!    construction — its band is `LeanLaw::layers_to_reach(d)` layers deep — so
//!    no segment is horizontal and none has zero rise. **An `|X|` is a flat
//!    truss and holds only along its own plane**, and the owner's ruling is
//!    that bracing must hold in EVERY direction: a cross is credited with the
//!    sway directions within 45° of its plane, a member is measured in every
//!    direction its crosses tell apart ([`unbraced_runs`]), and a tie made of
//!    crosses needs planes whose arcs cover all of them. Crosses meeting a
//!    member at ONE layer are credited together (`Σ cos² ≥ ½`, `joint_holds`),
//!    so two planes 60°-120° apart hold every direction. Every partner in
//!    reach is tried, nearest first, not only the nearest; every diagonal and
//!    every mast is tested as the SOLID it is drawn, against the model (final
//!    gap) and against every member it is not joined to
//!    (`member_clearance.rs`, 2026-09-29). **Both ends of a band lie on the
//!    posts' free shafts** (2026-09-30): above the top of each post's root
//!    flare and above the raft top. A band whose bottom falls inside a flare
//!    moves up or is dropped, and a brace end inside a flare is never
//!    credited as a tie: the flare is already a rigid base, and a tie in it
//!    adds resin and no stiffness ([`free_shaft_floor`]).
//! 3. **Thicken.** Raise the member's ROOT, keep its tip: the drawn profile
//!    becomes a taper from the tip it had down to a thicker root, carried on
//!    down to the member's own foot and down every trunk it stands on. It holds
//!    in every direction and adds no member; the owner prefers it wherever the
//!    crosses cannot close a tie. The root may not pass the registered 1.0 mm
//!    maximum, and a grown plate root must still clear every other root disc.
//! 4. **Mast.** A new plate-rooted post, then an `|X|` to it. A cell is
//!    admissible only where the braced panel it makes holds the arm to one
//!    pixel in its plane (`panel_sway_over_pixel`), so a mast never stands at
//!    the minimum disc clearance merely because that cell is nearest. An arm's
//!    existing mast is extended before a second one is dropped.
//! 5. **Tie to the model** (D8, Craig ruling 2026-10-03). Only after the
//!    rungs above reach their fixed point: a run none of them held gets a set
//!    of ties from the post to the model's own nearby surface, each seated
//!    with a rounded tip like any support tip and counted through a frame in
//!    which only the model is fixed ([`crate::model_tie`], `tie_to_model`).
//! 6. **Warn.** Named, counted, emitted. Never a dropped or shortened arm.
//!
//! # Craig's rulings of 2026-10-01
//!
//! * `S` is 30 (interim, pending a slenderness coupon). **Superseded
//!   2026-10-02:** Craig ruled `S` = 40 (interim, pending the slenderness
//!   coupon) after the S = 30 sample showed the Cylinder and the Sphere
//!   heavily over-braced.
//! * **Tie timing.** A member that lands on a host — an arm branch or a
//!   brace diagonal — holds the host sideways only from the layer its own top
//!   is held (its seat prints). A run is the cantilever from the highest tie
//!   that HOLDS while a layer prints up to that layer ([`RunMeasure`]), so an
//!   `|X|` ties its posts from its band top, and a host carrying a branch is
//!   measured up to the branch's seat.
//! * **Keep one-direction crosses.** A cross holds its own plane; thickening
//!   or a mast covers the direction across it, and a thickening that also
//!   holds never makes the cross unneeded ([`prune_unneeded_crosses`]).
//! * Every brace-band refusal is counted under one reason ([`BandRefusals`]).
//!
//! # Craig's rulings of 2026-10-03 (interim, pending the slenderness coupon)
//!
//! From the bracing force study (`field-2026-09-28/bracing-mechanics.md`,
//! revision 2, and its cross-family challenge):
//!
//! * **R2.** A brace is never thicker than the thinner post it joins, so its
//!   reach is at most `S` times that post's radius ([`plan_cross`]). The
//!   brace-junction raise — the post drawn at the brace's radius from the
//!   joint down to its root — is deleted (`adapter::drawn_profiles`): it held
//!   up nothing, since a brace carries no peel load, and it drew a fat post
//!   with a thin top.
//! * **R7 + D5.** Brace inside a window, thicken outside it: a cross is built
//!   only where it costs less resin than thickening the run
//!   ([`cross_charge_mm3`]), so a run a little over its limit is thickened
//!   and a long skinny one is braced. The window's edges are that comparison
//!   for the actual pair, not a constant.
//! * **D9.** Past a pair's two-post ceiling ([`pair_ceiling_ratio`]) no band
//!   is added between that pair: a wider partner is tried, or the run is
//!   thickened into the window ([`thicken_into_the_window`]).
//! * **R6 + D4.** A band is not a fixed point. A run is braced by a SET of
//!   bands to one partner, counted through a two-post frame in which every
//!   band sways ([`crate::frame_bands`], the study's `bands2` panel, held to
//!   its numbers), laid evenly over the run and built together or not at all
//!   ([`tie_by_band_set`]); the prune judges a set as one. Past the count
//!   that fits, the pair is past its ceiling (D9 as the frame meets it).
//! * **R9 + D6.** A construction-stage shake check on the finished field, a
//!   WARNING only, at a stated, unmeasured brace peel pressure
//!   ([`crate::stage_shake`]).
//! * Unchanged: the brace angle (the saturated overhang angle, 57.5° from
//!   vertical) and "every direction before seating".
//!
//! **Every remedy is attempted only for a run that is over the limit NOW.**
//! The member's runs are re-measured from the live strut set after every
//! remedy that touches it, so no partner builds a second cross on a pair an
//! earlier cross already tied, and no mast is dropped beside a post that is
//! already held.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rayon::prelude::*;

use crate::drain::DrainKeepOut;
use crate::grid::Grid;
use crate::lean::LeanLaw;
use crate::up_sweep::ColumnHistory;
use crate::{Landing, StrutKind, SweepStrut};
use plinth_support::{RootBoss, SweepRootFlare};

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
        // `L / S` can round DOWN, and then `S · r` falls one ulp short of `L`
        // and the diagonal this sized reads as over its own limit (measured
        // 2026-09-29: 4.0537020117418585 mm against 4.053702011741858). Float
        // noise, not physics: the radius is nudged up by one relative epsilon
        // exactly when that happens.
        let radius = length_mm / self.ratio;
        let radius = if self.ratio * radius < length_mm {
            radius * (1.0 + f64::EPSILON)
        } else {
            radius
        };
        radius.max(self.r_min_mm)
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
    /// The EQUIVALENT radius of the run as DRAWN: the uniform radius whose
    /// cantilever sways exactly as far as the drawn profile does.
    ///
    /// For a cantilever of varying section the one-pixel law is the integral
    /// `δ/p = (4/S⁴) ∫₀ᴸ (L−x)³ / r(x)⁴ dx`, with `x` measured up from the
    /// lower tie; a uniform post gives `(L/(S r))⁴`. So the run is over its
    /// limit exactly when `L > S · r_eq` with
    /// `r_eq⁻⁴ = Σ w_j r_j⁻⁴`, `w_j = ((n−j)⁴ − (n−j−1)⁴) / n⁴` over its `n`
    /// layers, bottom first. The weights sum to one and fall towards the top:
    /// the root holds a cantilever, the tip barely matters. Until 2026-09-29
    /// this was the NARROWEST ledger radius on the run, which is neither the
    /// section that is printed nor the one that sways.
    pub radius_mm: f64,
    /// `S · radius_mm`.
    pub limit_mm: f64,
    /// The direction of sway this run is measured in, in degrees in
    /// `[0, 180)` in grid (row, column) steps, or `None` when no cross ties
    /// the member and every direction sees the same ties.
    ///
    /// An `|X|` holds only along its own plane. A member tied by crosses is
    /// measured in every sway direction its crosses tell apart (`sway_directions`),
    /// counting every landing and end, and each cross whose plane lies within
    /// 45° of that direction (`holds`).
    pub sway_deg: Option<f64>,
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
    /// on could not grow far enough to carry the sum. Their runs are measured
    /// again and tied like any other; the arm is never dropped.
    ///
    /// **Only merges THIS LANE forced.** Until 2026-09-29 this counted every
    /// un-merge after the lane, descent merges included (8 on the stilt plate,
    /// where none of the lane's own was undone); those are
    /// [`Self::descent_merges_put_back`] now.
    pub merges_undone_by_radius: usize,
    /// The ids behind [`Self::merges_undone_by_radius`], in id order.
    pub merges_put_back_ids: Vec<u32>,
    /// Merges the DESCENT made that the same un-merge pass put back, because
    /// a forced merge had loaded their trunk past the maximum radius.
    pub descent_merges_put_back: usize,
    /// Remedy 2: crosses built for a run no cross held yet, and crosses built
    /// for a run that sways in a direction the crosses already on its member
    /// do not hold (a further plane).
    pub crosses_first_plane: usize,
    pub crosses_further_plane: usize,
    /// Crosses built to a partner that was NOT the nearest in reach — the
    /// nearest's band was refused or its plane does not hold the direction the
    /// run sways in. A path-hit witness for "try more than one partner".
    pub crosses_to_a_later_partner: usize,
    /// Bands planned in all (every partner tried, built or not).
    pub cross_bands_planned: usize,
    /// Bands refused because a cross on the same pair of posts already
    /// stands over an overlapping height band: one tie is never built twice.
    pub crosses_refused_overlapping: usize,
    /// Crosses built in a pass and taken out at its end because a later cross
    /// left both posts inside their limits without them (every cross must be
    /// needed). Not included in `braces_added`.
    pub crosses_pruned: usize,
    /// Remedy 3: members whose root the lane raised (tip kept), and the
    /// refusals: a root past the maximum radius, or a grown plate root whose
    /// flared disc would touch another root.
    pub members_thickened: usize,
    pub thicken_refused_radius: usize,
    pub thicken_refused_disc: usize,
    /// Thickenings refused because the raised solid, of the member or of a
    /// trunk raised with it, would enter the model's final clearance gap where
    /// the solid as drawn now does not (R2, 2026-10-04). A path-hit witness.
    pub thicken_refused_model: usize,
    /// The refusal's path hits (challenger round 3, 2026-10-05): raised slots
    /// the raster flagged within the gap, and of those the ones the census's
    /// predicate measured clear as committed, and the ones it proved inside
    /// the gap already as drawn (excused, not the plan's doing).
    pub thicken_slots_flagged: usize,
    pub thicken_slots_measured_clear: usize,
    pub thicken_slots_inside_already: usize,
    /// Flagged slots whose committed read was an undecided upper bound at or
    /// above the gap, which refuses nothing (rule A, 2026-10-05): the path
    /// hit of the rule "a refusal needs proof".
    pub thicken_slots_undecided: usize,
    /// Raised members the refusal skipped because they stand in a hidden
    /// cavity, where the final gap does not apply (rule H, Craig 2026-10-05):
    /// the path hit of that ruling.
    pub thicken_members_hidden: usize,
    /// Thickenings APPLIED whose root disc already lapped another root before
    /// the growth (and laps it more after): the growth made no new lap, so
    /// the rung is not refused (orchestrator ruling 2026-09-29, the stilt
    /// plate's post 81). The standing lap is the congestion lane's warning; this
    /// counts where the rung grew one. A path-hit witness for the rule.
    pub thicken_over_existing_lap: usize,
    /// Thickenings the root-lap test ADMITTED on the feet the file draws
    /// (the lowering's root boss, 2.2 / 1.5 × root) where the census's
    /// crowding discs (`FLARE_R_FACTOR` × root) would have made a new lap and
    /// refused them, the rule until 2026-10-02 (fix/mast-tie-and-overlap-factor).
    /// A path-hit witness: zero means the drawn-foot test decided every
    /// thickening as the crowding test did.
    pub thicken_admitted_where_the_crowding_disc_laps: usize,
    /// Plate root discs a cross GREW, because the junction clause raised the
    /// root the file draws for its post (or the post's trunk, or a new mast)
    /// past the disc its placement was cleared against (review-fixes
    /// 2026-09-29); and of those, how many then lap another root. The second
    /// is a WARNING on the emitted file, never a refusal. Both are path-hit
    /// witnesses for keeping the lane's discs equal to the drawn roots.
    pub cross_roots_grown: usize,
    pub cross_roots_lapping: usize,
    /// Trunks raised under a thickened member, so no member stands on one
    /// thinner than itself.
    pub trunks_raised_under_thickening: usize,
    /// Remedy 4: plate cells refused because the braced panel a mast there
    /// would make cannot hold the arm to one pixel even at the maximum mast
    /// radius; and masts EXTENDED up to a later tie instead of a new mast.
    pub mast_cells_refused_by_panel: usize,
    pub masts_extended: usize,
    /// MAST TIE HEIGHT (fix/mast-tie-and-overlap-factor, 2026-10-02): mast
    /// attempts planned at a layer ABOVE the crosses' first evenly spaced tie
    /// (the layer every mast was planned at until this date), and of all mast
    /// attempts, those whose tie had to rise above the highest evenly spaced
    /// tie for the piece above to fit its own limit ([`mast_tie_layer`]).
    /// Path-hit witnesses: both zero means the rule planned every mast where
    /// the old one did.
    pub mast_ties_planned_above_the_first_tie: usize,
    pub mast_ties_raised_for_the_piece_above: usize,
    /// MEMBER CLEARANCE (fix/member-clearance, 2026-09-29): bands refused
    /// because a diagonal's drawn solid would come within the final gap of the
    /// model, or would enter a member it is not joined to; and mast cells (and
    /// mast extensions) refused for the same two reasons. Path-hit witnesses.
    pub brace_bands_refused_by_model: usize,
    pub brace_bands_refused_by_member: usize,
    /// FREE SHAFT (fix/brace-not-in-flare, 2026-09-30): bands whose bottom
    /// fell inside a post's root flare or the raft and that were MOVED UP until
    /// both ends lie on the posts' free shafts (counted when the moved band is
    /// built); and bands that could not move (the posts end first, the moved
    /// band no longer ties the run, or the band had to stay at a shared tie
    /// layer) and were DROPPED. Path-hit witnesses.
    pub brace_bands_moved_above_flare: usize,
    pub brace_bands_dropped_in_flare: usize,
    /// Masts built after their band was dropped for dipping into a flare, by
    /// planning the mast again with the tie moved up by the shortfall (a new
    /// mast is only as tall as its tie, so its band cannot move on its own).
    pub masts_raised_above_flare: usize,
    pub mast_cells_refused_by_model: usize,
    pub mast_cells_refused_by_member: usize,
    /// The member test's verdicts where the drawn-span rule (round-3 review
    /// fix, 2026-09-29) and the retired closest-axes reading DISAGREE, both
    /// ways: refused here and admitted by the retired reading (a wider span
    /// away from the closest axis point), and admitted here and refused by it
    /// (it read the slot above the closest point too). Path-hit witnesses; both
    /// zero means the rule decided every test on this field as before.
    pub member_refusals_beyond_the_closest_axes: usize,
    pub member_admissions_the_closest_axes_refused: usize,
    /// Mast cells whose braced panel would hold only with a mast THICKER than
    /// the member it braces: a second, stiffer post is not a brace, so the
    /// cell is refused (and, when no cell is left, the run goes to the
    /// warning). Since 2026-10-03 (R2) also cells whose diagonal would be
    /// thicker than the member at the tie: the mast is sized up to its
    /// diagonal, the member is not.
    pub mast_cells_refused_thicker_than_member: usize,
    /// Masts placed at a cell that needed a thinner mast than the nearest
    /// admissible cell would have: the smallest mast wins, not the nearest.
    pub masts_moved_to_a_thinner_cell: usize,
    /// Crosses built at a layer where the member already had a cross, because
    /// that layer alone cuts the run into pieces inside the limit: the planes
    /// meet at one layer and are credited together.
    pub crosses_at_an_existing_tie_layer: usize,
    /// Tie passes run (each followed by a down-sweep), and whether the last
    /// one changed nothing — the lane's fixed point.
    pub tie_passes: usize,
    pub fixed_point: bool,
    /// The tie passes run AGAIN after the clearance re-route moved an arm
    /// (rule D, 2026-10-05), and the remedies they applied (crosses,
    /// thickenings, masts, model ties): the path hit of that rule. Both are
    /// also counted in `tie_passes`.
    pub retie_passes_after_reroute: usize,
    pub retie_remedies_after_reroute: usize,
    /// Remedy-1 candidates NOT re-landed because another member rests on them
    /// or is tied to them. Moving such an arm moves the ground from under its
    /// dependants — the daisy chains of 2026-09-28. The run falls to the brace,
    /// the mast, or a warning. A path-hit witness for the rule.
    pub merges_refused_carrying: usize,
    /// Over-limit runs of a HEADED arm the forced-merge pass left alone: a
    /// head is frozen at placement and a merge would re-route its post.
    pub headed_kept: usize,
    /// Shaft landings the index offered and the LIVE field refused: the named
    /// parent no longer stands at that cell and layer, because an earlier
    /// merge in the same loop moved it. A path-hit witness for the rule.
    pub merges_refused_stale_shaft: usize,
    /// Crosses the prune KEPT only because they hold their own plane on the
    /// posts as they stood before this pass's thickenings (Craig ruling
    /// 2026-10-01: keep one-direction crosses). Without the ruling each of
    /// these would have been taken out. A path-hit witness.
    pub crosses_kept_in_their_own_plane: usize,
    /// Landing ties, on the field the LAST tie pass measured, that hold their
    /// host only from a layer ABOVE the landing (the branch's seat, or a
    /// diagonal's band top; Craig ruling 2026-10-01, tie timing). A path-hit
    /// witness: zero means the timing rule changed no run.
    pub landing_ties_held_late: usize,
    /// Every brace band refused, by its reason (`brace_bands_refused` is
    /// their sum). See [`BandRefusals`].
    pub brace_band_refusals: BandRefusals,
    /// Bands BUILT after the fix of 2026-10-01 to refusals that were not
    /// geometric: deepened until both diagonals fit the overhang law (posts
    /// leaning apart), and moved up because the band at its planned top
    /// reached below the plate. Path-hit witnesses.
    pub brace_bands_deepened_for_their_diagonals: usize,
    pub brace_bands_moved_up_off_the_plate: usize,
    /// **BRACE WINDOW** (R7 + D5, Craig ruling 2026-10-03): crosses that
    /// passed every geometric test and were NOT built because thickening the
    /// run costs less resin than the crosses it would need
    /// ([`cross_charge_mm3`]); runs then thickened with a cross refused that
    /// way; and crosses built with BOTH posts needing the tie (the cross is
    /// charged half to each). Path-hit witnesses.
    pub crosses_refused_thicken_cheaper: usize,
    pub thickened_where_cheaper_than_bracing: usize,
    pub crosses_shared_by_both_posts: usize,
    /// The lowest `L / limit` at which a cross was built while thickening
    /// was admissible (the window's lower edge as the field met it), and the
    /// highest at which a run was thickened because that was cheaper. `0`
    /// where none.
    pub window_lowest_braced_ratio: f64,
    pub window_highest_thickened_ratio: f64,
    /// **TWO-POST CEILING** (D9, 2026-10-03): crosses refused because the run
    /// is longer than its pair can hold with any number of bands
    /// ([`pair_ceiling_ratio`]); and runs thickened only INTO the window of
    /// such a pair (to its ceiling), to be braced after. Path-hit witnesses.
    pub crosses_refused_past_pair_ceiling: usize,
    pub members_thickened_into_the_window: usize,
    /// **R2 on the finished field** (2026-10-03): braces drawn thicker than a
    /// post at one of their joints. The band rule makes this zero when the
    /// posts' drawn sections do not shrink after the band is planned; it is
    /// counted on the emitted profiles, never assumed. A WARNING quantity.
    pub braces_thicker_than_a_post: usize,
    /// **FRAME BANDS** (R6 + D4, 2026-10-03): band sets built (one set ties
    /// one run to one partner, all its bands at once), the crosses in them,
    /// and how many of those the fixed-point count `⌈L / limit⌉ − 1` would
    /// NOT have built (the bands the frame added). Path-hit witnesses.
    pub frame_band_sets: usize,
    pub frame_bands_built: usize,
    pub frame_bands_beyond_fixed_point: usize,
    /// The most bands in one set, and the highest `L / limit` a set braced.
    pub frame_most_bands_in_a_set: usize,
    pub frame_highest_ratio_braced: f64,
    /// Pairs refused because no band count that fits the run holds it (the
    /// two-post ceiling as the frame meets it, D9), and sets refused because
    /// one of their bands was refused (geometry, clearance, a band that grew
    /// into its neighbour, or a frame that no longer held once the bands were
    /// planned on the posts' real cells).
    pub frame_pairs_past_ceiling: usize,
    pub frame_sets_refused_incomplete: usize,
    /// Pairs where not even the fixed-point count of bands fits the run at
    /// that pair's band depth (above both posts' flares, below the run's
    /// top): geometry, counted apart from the ceiling.
    pub frame_pairs_without_room: usize,
    /// Bands the field refused at their laid layer and that were built a
    /// little lower or higher between their neighbours (the frame solved
    /// again on the bands as built), and sets built with more bands than
    /// first counted because a band found no place at the closer count.
    pub frame_bands_moved_from_their_layer: usize,
    pub frame_sets_respaced: usize,
    /// Crosses the prune took out as whole sets (each set judged as one).
    pub frame_sets_pruned: usize,
    /// **CONSTRUCTION-STAGE SHAKE** (R9 + D6, 2026-10-03): a WARNING-only
    /// check of the finished field ([`crate::stage_shake`]). Never changes
    /// the field.
    pub stage_shake: crate::stage_shake::StageShake,
    /// **MODEL TIES** (D8, Craig ruling 2026-10-03; [`crate::model_tie`]):
    /// the last rung before a warning. Runs that reached it, tie sets built
    /// (one set ties one run, all its ties at once), the ties in them, and
    /// the ties' ids on the finished field.
    pub model_tie_runs_tried: usize,
    pub model_tie_sets: usize,
    pub model_ties_built: usize,
    pub model_tie_ids: Vec<u32>,
    /// Ties built thicker than their own slenderness radius because the
    /// frame needed it (never past the post, R2); sets built with more ties
    /// than the fixed-point count `⌈L / limit⌉ − 1`; and the worst
    /// tied-post sway a built set left, in pixels (at most 1).
    pub model_ties_thickened_for_the_frame: usize,
    pub model_tie_sets_beyond_fixed_point: usize,
    pub model_tie_worst_sway_px: f64,
    /// Why runs that reached the rung got no tie set: no model surface in
    /// reach of the post at some tie layer, no candidate whose geometry,
    /// path, solid clearance (the finished-field census itself) or member
    /// clearance passed, or a frame that no tie count holds.
    pub model_tie_refused_no_surface: usize,
    pub model_tie_refused_no_candidate: usize,
    pub model_tie_refused_frame: usize,
    /// Candidate ties refused, by reason: the joint below the post's free
    /// shaft or the run's base, a tie thicker than the post (R2), the
    /// centreline path (through a drain's escape, model material, or another
    /// member's axis), the drawn solid within the final gap of the model
    /// outside its own seat zone, its contact disc not wholly on the model,
    /// or the solid entering a member.
    pub model_tie_candidates: usize,
    pub model_tie_refused_off_shaft: usize,
    pub model_tie_refused_thicker_than_post: usize,
    pub model_tie_refused_path_drain: usize,
    pub model_tie_refused_path_model: usize,
    pub model_tie_refused_path_member: usize,
    pub model_tie_refused_solid_model: usize,
    /// Ties chosen that stand wholly in a hidden cavity, where the final gap
    /// is not read (Craig ruling 2026-10-05): the path hit of that ruling in
    /// the tie screen.
    pub model_tie_candidates_hidden: usize,
    /// Candidates whose seat trim would cut the shaft outside the tip ball
    /// (the lip-seat rule: the tie meets its face too side-on).
    pub model_tie_refused_trim_bite: usize,
    /// Tie layers planned again with a thicker tip because the nearest
    /// contact's seat trim cut its shaft at the thinner one (the lip-seat
    /// rule read as contact sizing; never past the post, R2).
    pub model_tie_tips_sized_for_the_trim: usize,
    /// Ties seated a few layers off their evenly spaced layer because no
    /// admissible tie seats there (tried a layer at a time, up to half the
    /// spacing either way).
    pub model_ties_moved_from_their_layer: usize,
    pub model_tie_refused_disc_off_face: usize,
    pub model_tie_refused_solid_member: usize,
}

/// Why a planned `|X|` band was refused, one count per reason; every refusal
/// [`plan_cross`] makes is counted under exactly one of them (until 2026-10-01
/// only the model gap and the member test were itemised, so on the brace pair
/// 7 refusals of 9 planned bands showed 1 reason).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BandRefusals {
    /// The two posts stand in one cell at the band top, or the gap needs no
    /// layer: there is no diagonal to draw.
    pub no_gap: usize,
    /// The band would start below layer 0 (the plate).
    pub below_plate: usize,
    /// The band bottom fell in a root flare or the raft and could not move up
    /// (`brace_bands_dropped_in_flare`).
    pub in_flare: usize,
    /// The band leaves one of the posts: its bottom is below a post's own
    /// foot, or its top above a post's own top.
    pub off_a_post: usize,
    /// A diagonal's own lateral move (between the cells it joins, for leaning
    /// posts) is longer than the overhang law admits over the band's depth.
    pub too_flat: usize,
    /// The band bottom is within one brace radius of the plate, so the
    /// diagonal's end ring would be drawn below it.
    pub ring_under_plate: usize,
    /// A diagonal's centreline crosses a drain's escape cylinder, model
    /// material, or another member's axis (`segment_path`).
    pub path_drain: usize,
    pub path_model: usize,
    pub path_member: usize,
    /// A diagonal's drawn SOLID comes within the final gap of the model, or
    /// enters a member it is not joined to (`brace_bands_refused_by_model`,
    /// `brace_bands_refused_by_member`).
    pub solid_model: usize,
    pub solid_member: usize,
    /// **R2** (2026-10-03): a diagonal of the band would be thicker than the
    /// thinner of the two posts at one of its joints, i.e. it would reach
    /// farther than `S` times that post's radius.
    pub thicker_than_post: usize,
}

impl BandRefusals {
    /// Every refusal counted.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.thicker_than_post
            + self.no_gap
            + self.below_plate
            + self.in_flare
            + self.off_a_post
            + self.too_flat
            + self.ring_under_plate
            + self.path_drain
            + self.path_model
            + self.path_member
            + self.solid_model
            + self.solid_member
    }
}

/// Every unbraced run in a finished field, measured from the strut set alone,
/// on the profile each member is DRAWN at, in every direction.
///
/// **Batch-first**: one pass for the drawn profiles (the lowering's own
/// junction clause, [`crate::adapter::drawn_profiles`]), one pass over the
/// struts to collect every tie, one pass to cut the runs. Nothing here scans
/// the grid.
///
/// A member's ties are its own two ends — its seat and its landing, both of
/// which are held laterally by the thing they are attached to — plus every
/// layer at which another member lands on it, plus every layer at which a brace
/// SEATS on it. The last of those is why [`SweepStrut::seat_on_strut`] exists:
/// a brace's bottom is in the ledger as a landing, its top is not in the ledger
/// at all, and an `|X|` ties both shafts at both ends of its band.
///
/// **A brace's tie is a tie in ONE PLANE** (owner ruling 2026-09-29: bracing
/// must hold in every direction). A flat truss holds nothing across its own
/// plane, so a member tied by crosses is measured in every direction of sway
/// its crosses tell apart, counting only the ties that hold in that direction
/// — every landing and end, and every cross within 45° of it (see `holds` and
/// [`UnbracedRun::sway_deg`]). A member no cross ties is measured once. Runs that
/// come out identical in two directions are reported once.
///
/// The drawn profile here carries the REGISTERED load-step taper
/// ([`crate::adapter::LoadStepTaper::registered`]), the production default; the
/// sweep itself measures with its policy's taper ([`unbraced_runs_drawn`]).
#[must_use]
pub fn unbraced_runs(
    grid: &Grid,
    struts: &[SweepStrut],
    layer_height_mm: f64,
    law: &SlendernessLaw,
) -> Vec<UnbracedRun> {
    unbraced_runs_drawn(
        grid,
        struts,
        layer_height_mm,
        law,
        crate::adapter::LoadStepTaper::registered(layer_height_mm),
    )
}

/// [`unbraced_runs`] on the profile drawn with `taper`.
pub(crate) fn unbraced_runs_drawn(
    grid: &Grid,
    struts: &[SweepStrut],
    layer_height_mm: f64,
    law: &SlendernessLaw,
    taper: crate::adapter::LoadStepTaper,
) -> Vec<UnbracedRun> {
    let (profiles, _) = crate::adapter::drawn_profiles(struts, taper);
    let ties = tie_lists(
        grid,
        struts,
        &free_shaft_floors(grid, struts, layer_height_mm),
    );
    let mut runs = Vec::new();
    for (index, strut) in struts.iter().enumerate() {
        runs.extend(member_runs(
            grid,
            strut,
            &ties[index],
            &profiles[index],
            layer_height_mm,
            law,
        ));
    }
    runs
}

/// The horizontal direction of a tie's plane, in grid steps, with its sign
/// normalised so that a plane and its reverse are one plane. `None` for two
/// cells that are the same.
type Plane = [i64; 2];

fn plane_between(grid: &Grid, from: usize, to: usize) -> Option<Plane> {
    let step = |a: usize, b: usize| i64::try_from(b).unwrap_or(0) - i64::try_from(a).unwrap_or(0);
    let mut plane = [
        step(grid.row_of(from), grid.row_of(to)),
        step(grid.col_of(from), grid.col_of(to)),
    ];
    if plane == [0, 0] {
        return None;
    }
    if plane[0] < 0 || (plane[0] == 0 && plane[1] < 0) {
        plane = [-plane[0], -plane[1]];
    }
    Some(plane)
}

/// Exact integer test: two planes are the same line. Used only to list a
/// member's distinct planes once.
fn parallel(a: Plane, b: Plane) -> bool {
    a[0] * b[1] - a[1] * b[0] == 0
}

/// The direction of a plane, in degrees in `[0, 180)`, in grid (row, column)
/// steps. A plane and its reverse are one direction.
#[expect(
    clippy::cast_precision_loss,
    reason = "grid steps are bounded by the grid's own dimensions and exact in f64"
)]
fn plane_deg(plane: Plane) -> f64 {
    (plane[1] as f64)
        .atan2(plane[0] as f64)
        .to_degrees()
        .rem_euclid(180.0)
}

/// The angle between two undirected directions, in `[0, 90]`.
fn apart_deg(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(180.0);
    d.min(180.0 - d)
}

/// Whether a cross in plane `tie` holds the post against sway in direction
/// `sway_deg`: the sway lies within 45° of the cross's own plane, so the
/// cross resists at least as much of it as it lets through (a flat truss is
/// stiff along its plane and free across it, and 45° is where the two
/// projections are equal — the bisector, not a tuned number).
fn holds(tie: Plane, sway_deg: f64) -> bool {
    apart_deg(plane_deg(tie), sway_deg) <= 45.0
}

/// Float noise on the joint sum, and nothing more: `Σ cos²` of planes read
/// from integer grid steps lands a few ulps either side of an exact ½.
const JOINT_NOISE: f64 = 1.0e-9;

/// **Crosses meeting a post at ONE layer are credited TOGETHER**
/// (fix/member-clearance, 2026-09-29). A cross along `θ` resists the part
/// `cos²(φ − θ)` of a sway in direction `φ`, in units of one cross along its
/// own plane; the lane's own reason for the 45° arc is that one cross there
/// resists exactly half (`cos² 45° = ½`). Crosses at one layer act in
/// parallel, so they hold `φ` when `Σ cos²(φ − θᵢ) ≥ ½` — the same ½, no new
/// number. One plane gives the ±45° arc exactly as [`holds`] does; two planes
/// 60° to 120° apart hold every direction (their minimum is `1 − |cos Δ|`),
/// and two planes 0.1° apart hold nothing across them. Distinct planes only:
/// two crosses in one plane are one flat truss.
fn joint_holds(planes: &[Plane], sway_deg: f64) -> bool {
    match planes {
        [] => false,
        [one] => holds(*one, sway_deg),
        many => {
            let sum: f64 = many
                .iter()
                .map(|plane| (sway_deg - plane_deg(*plane)).to_radians().cos().powi(2))
                .sum();
            sum >= 0.5 - JOINT_NOISE
        }
    }
}

/// The directions where the joint hold of `planes` (one layer) starts or
/// stops, or none when they hold every direction.
///
/// `Σ cos²(φ − θᵢ) = n/2 + (A/2)·cos(2φ − ψ)` with `A·e^{iψ} = Σ e^{2iθᵢ}`,
/// so the held set is one arc about `ψ/2` of half-width `½·acos((1 − n)/A)`,
/// or the whole half-turn when `(1 − n)/A ≤ −1`.
fn joint_breaks(planes: &[Plane]) -> Vec<f64> {
    if planes.len() == 1 {
        let deg = plane_deg(planes[0]);
        return vec![
            (deg - 45.0).rem_euclid(180.0),
            (deg + 45.0).rem_euclid(180.0),
        ];
    }
    let (mut sx, mut sy) = (0.0_f64, 0.0_f64);
    for plane in planes {
        let twice = (2.0 * plane_deg(*plane)).to_radians();
        sx += twice.cos();
        sy += twice.sin();
    }
    let amplitude = sx.hypot(sy);
    #[expect(
        clippy::cast_precision_loss,
        reason = "a plane count bounded by the member's tie count is exact in f64"
    )]
    let n = planes.len() as f64;
    if amplitude <= 0.0 {
        return Vec::new();
    }
    let c = (1.0 - n) / amplitude;
    if c <= -1.0 + JOINT_NOISE {
        return Vec::new();
    }
    let centre = 0.5 * sy.atan2(sx).to_degrees();
    let half = 0.5 * c.clamp(-1.0, 1.0).acos().to_degrees();
    vec![
        (centre - half).rem_euclid(180.0),
        (centre + half).rem_euclid(180.0),
    ]
}

/// The sway directions a member has to be measured in, given the planes of
/// its crosses grouped by LAYER: none when no layer's crosses leave any
/// direction unheld (every tie then holds every way), otherwise one
/// direction inside every arc between consecutive BREAKPOINTS, where some
/// layer's crosses start or stop holding ([`joint_breaks`]). Between two
/// breakpoints the set of layers that hold is the same, so the arcs'
/// midpoints are every case there is; and a new cross only ever ADDS holding
/// to a direction, so the measurement is monotone in the ties.
fn sway_directions_by_layer(groups: &[&[Plane]]) -> Vec<Option<f64>> {
    let mut breaks: Vec<f64> = groups
        .iter()
        .filter(|planes| !planes.is_empty())
        .flat_map(|planes| joint_breaks(planes))
        .collect();
    if breaks.is_empty() {
        return vec![None];
    }
    breaks.sort_by(f64::total_cmp);
    breaks.dedup_by(|a, b| (*a - *b).abs() < 1.0e-9);
    let count = breaks.len();
    (0..count)
        .map(|at| {
            let from = breaks[at];
            let to = if at + 1 < count {
                breaks[at + 1]
            } else {
                breaks[0] + 180.0
            };
            Some((0.5 * (from + to)).rem_euclid(180.0))
        })
        .collect()
}

/// [`sway_directions_by_layer`] for planes that each stand at a layer of
/// their own (no two credited together).
#[cfg(test)]
fn sway_directions(planes: &[Plane]) -> Vec<Option<f64>> {
    let groups: Vec<&[Plane]> = planes.iter().map(core::slice::from_ref).collect();
    sway_directions_by_layer(&groups)
}

/// The ties of one member at one layer.
#[derive(Clone, Debug)]
struct LayerTies {
    layer: usize,
    /// The earliest layer from which something here holds EVERY direction (a
    /// landing, an end), or `None` when nothing here does. A landing branch
    /// holds from its seat ([`Tie::from`]), so this can lie above `layer`.
    all_from: Option<usize>,
    /// The DISTINCT planes of the crosses here, each with the layer it holds
    /// from (a band top at once; a band bottom from its band top), sorted by
    /// that layer.
    planes: Vec<(Plane, usize)>,
}

impl LayerTies {
    /// Whether something here holds every direction from its own layer on.
    fn all_at_once(&self) -> bool {
        self.all_from.is_some_and(|from| from <= self.layer)
    }

    /// The plane sets this layer has over time: the planes holding by each
    /// plane's own start, earliest first (crosses here act together,
    /// `joint_holds`, but only once each has started to hold).
    /// One set per distinct start layer, so planes that start together are
    /// one set, exactly as before tie timing.
    fn plane_sets(&self) -> impl Iterator<Item = (usize, Vec<Plane>)> + '_ {
        (1..=self.planes.len())
            .filter(|&count| {
                self.planes
                    .get(count)
                    .is_none_or(|next| next.1 != self.planes[count - 1].1)
            })
            .map(|count| {
                (
                    self.planes[count - 1].1,
                    self.planes[..count].iter().map(|(plane, _)| *plane).collect(),
                )
            })
    }

    /// The layer from which this layer's ties hold the sway `sway_deg`
    /// (`None`: no direction is told apart, so a plane set holds only where
    /// it holds every direction), or `None` if they never do.
    fn holds_from(&self, sway_deg: Option<f64>) -> Option<usize> {
        let planes = self.plane_sets().find_map(|(from, planes)| {
            let held = match sway_deg {
                Some(sway) => joint_holds(&planes, sway),
                None => joint_breaks(&planes).is_empty(),
            };
            held.then_some(from)
        });
        match (self.all_from, planes) {
            (Some(all), Some(planes)) => Some(all.min(planes)),
            (one, two) => one.or(two),
        }
    }
}

/// The ties of one member grouped by layer: per layer, from when something
/// there holds every direction (a landing, an end), and the DISTINCT planes
/// of the crosses there. One sort, no map.
fn ties_by_layer(ties: &[Tie]) -> Vec<LayerTies> {
    let mut sorted: Vec<Tie> = ties.to_vec();
    sorted.sort_unstable_by_key(|tie| tie.layer);
    let mut groups: Vec<LayerTies> = Vec::new();
    for tie in sorted {
        if groups.last().is_none_or(|group| group.layer != tie.layer) {
            groups.push(LayerTies {
                layer: tie.layer,
                all_from: None,
                planes: Vec::new(),
            });
        }
        let Some(group) = groups.last_mut() else {
            continue;
        };
        match tie.plane {
            None => {
                group.all_from = Some(group.all_from.map_or(tie.from, |known| known.min(tie.from)));
            }
            Some(plane) => {
                match group
                    .planes
                    .iter_mut()
                    .find(|(known, _)| parallel(*known, plane))
                {
                    Some((_, from)) => *from = (*from).min(tie.from),
                    None => group.planes.push((plane, tie.from)),
                }
            }
        }
    }
    for group in &mut groups {
        group.planes.sort_by_key(|&(_, from)| from);
    }
    groups
}

/// The sway directions a member's grouped ties tell apart. A layer whose
/// every-direction tie holds AT ONCE needs no direction; one whose
/// every-direction tie holds only later (a landing branch not yet seated)
/// still has its crosses tell directions apart until then. Every plane set a
/// layer passes through ([`LayerTies::plane_sets`]) contributes its
/// breakpoints, so between two breakpoints every layer's held set is the same
/// at every moment of the print.
fn sway_directions_of(groups: &[LayerTies]) -> Vec<Option<f64>> {
    let sets: Vec<Vec<Plane>> = groups
        .iter()
        .filter(|group| !group.all_at_once())
        .flat_map(|group| group.plane_sets().map(|(_, planes)| planes))
        .collect();
    let open: Vec<&[Plane]> = sets.iter().map(Vec::as_slice).collect();
    sway_directions_by_layer(&open)
}

/// The distinct planes of the crosses tying a member at exactly `layer`.
fn planes_at(ties: &[Tie], layer: usize) -> Vec<Plane> {
    let mut planes: Vec<Plane> = Vec::new();
    for tie in ties {
        if tie.layer != layer {
            continue;
        }
        if let Some(plane) = tie.plane {
            if !planes.iter().any(|known| parallel(*known, plane)) {
                planes.push(plane);
            }
        }
    }
    planes
}

/// Whether a new cross in `plane` at `layer`, credited together with the
/// crosses already tying the member there, holds the direction `sway_deg`
/// (`None`: a run no cross ties yet, which any plane serves).
fn new_plane_holds(ties: &[Tie], layer: usize, plane: Plane, sway_deg: Option<f64>) -> bool {
    let Some(sway) = sway_deg else {
        return true;
    };
    let mut planes = planes_at(ties, layer);
    if !planes.iter().any(|known| parallel(*known, plane)) {
        planes.push(plane);
    }
    joint_holds(&planes, sway)
}

/// One lateral tie on one member: the layer, and the plane it holds in
/// (`None`: every direction, as a landing or the member's own end does).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Tie {
    layer: usize,
    /// **The layer from which the tie HOLDS** (Craig ruling 2026-10-01, tie
    /// timing). A member that lands on a host at `layer` — an arm branch or a
    /// brace diagonal — is a free cantilever off the host until its own top
    /// is held, the layer its seat prints; only from then does it hold the
    /// host sideways. `from == layer` for a seat on the host (a diagonal's
    /// band top), whose other end has already printed.
    from: usize,
    plane: Option<Plane>,
    /// Whether this is a cross's band TOP on the member (a diagonal SEATS on
    /// it here). A cross ties nothing while printing until its top is cured,
    /// so only a band top is a layer a further plane may share.
    band_top: bool,
}

pub(crate) fn live(strut: &SweepStrut) -> bool {
    strut.k_bottom != usize::MAX && strut.k_top > strut.k_bottom
}

/// Float noise on a height compared against a layer plane, so that a boss top
/// that falls exactly on a layer boundary (1.7 mm = 34 × 0.05 mm) does not
/// read a hair below it. NUMERICAL, not physical: it decides no geometry.
const HEIGHT_NOISE_MM: f64 = 1.0e-9;

/// **The lowest layer at which a tie on `strut` lies on its FREE SHAFT**
/// (fix/brace-not-in-flare, 2026-09-30): strictly above the top of its root
/// flare, for a plate-rooted member, and strictly above the raft top, for
/// every member.
///
/// A root flare is already a rigid base: it is a cone `FLARE_R_FACTOR` times
/// the root wide at its foot, standing on its raft pad. A brace that ties a
/// member inside it adds resin and a record below the raft top and holds
/// nothing the flare does not already hold (measured on the laid tube's `|X|`
/// pair 177/178: lower ends 0.25 mm over the plate, inside flares whose boss
/// top is 1.7 mm up; `oversupport-attribution.md`).
///
/// The flare's top is the lowering's own boss top
/// ([`plinth_support::SweepRootFlare`], which answers with the arithmetic
/// that draws the `RootFlare` record), read for the member AS THE LOWERING
/// WILL DRAW IT: the arm the adapter builds from its drawn ends
/// ([`crate::adapter::drawn_ends`], [`crate::adapter::sweep_arm`]) over a
/// plate at z = 0, so the boss is measured from the SEATED terminal, the
/// seat pushed into the model by the seat law, exactly as the lowering
/// measures it (round-6 review, 2026-09-30). Until then it was read at the
/// seat surface `k_top · h`: on a post shorter than twice the boss height
/// above the raft the boss top is half-way up the run, so the unseated
/// reading put it half a seat depth low, and a tie one layer inside the drawn
/// flare was accepted (under the legacy 1.5 mm flare length, a 3.05 mm post
/// with a 0.05 mm tip: 1.625 mm read, 1.6595 mm drawn, layer 33 at 1.65 mm
/// accepted). The sweep's boss height is the owner's ChituBox `heightofboss`,
/// 1.0 mm, since 2026-09-30, so such a post is now under 2.2 mm.
///
/// The seat normal is the member's own as it stands: the carried face is
/// measured after the ties (`run_sweep_with_drains`), and until then the
/// lowering's own fallback, straight down, is what it would draw.
///
/// `None` for the flare geometry (an unreadable registry) leaves only the
/// member's own foot, as before this rule: the band is still built and
/// nothing is refused on a missing number.
pub(crate) fn free_shaft_floor(
    flare: Option<&SweepRootFlare>,
    grid: &Grid,
    strut: &SweepStrut,
    h: f64,
) -> usize {
    let Some(flare) = flare else {
        return 0;
    };
    if !(h.is_finite() && h > 0.0) {
        return 0;
    }
    let mut top_mm = flare.raft_top_above_plate_mm();
    if strut.landing == Landing::Plate && strut.kind != StrutKind::Brace {
        // The radius profile is not read by the boss arithmetic; only the
        // two ends, the tip, the seat normal and whether the member seats.
        let (seat, base) = crate::adapter::drawn_ends(grid, 0.0, h, strut);
        let arm = crate::adapter::sweep_arm(strut, seat, base, None, Vec::new());
        if let Some(boss_mm) = flare.top_above_plate_of(&arm, 0.0) {
            top_mm = top_mm.max(boss_mm);
        }
    }
    first_layer_above(top_mm, h)
}

/// The lowest layer whose plane stands strictly above `height_mm`.
fn first_layer_above(height_mm: f64, h: f64) -> usize {
    if !(height_mm.is_finite() && height_mm >= 0.0) {
        return 0;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a finite non-negative height over a positive layer height, floored"
    )]
    let mut layer = (height_mm / h).floor() as usize;
    #[expect(
        clippy::cast_precision_loss,
        reason = "a layer index bounded by the plan's own layer count is exact in f64"
    )]
    while layer as f64 * h <= height_mm + HEIGHT_NOISE_MM {
        layer += 1;
    }
    layer
}

/// [`free_shaft_floor`] for every member, in one pass.
fn free_shaft_floors(grid: &Grid, struts: &[SweepStrut], h: f64) -> Vec<usize> {
    let flare = SweepRootFlare::registered(h).ok();
    struts
        .iter()
        .map(|strut| free_shaft_floor(flare.as_ref(), grid, strut, h))
        .collect()
}

/// The plane of the `|X|` a brace belongs to: from its seat post to its
/// landing post, both read AT THE TOP OF THE BAND, so both diagonals of one
/// cross name exactly one plane even when the posts lean.
fn brace_plane(grid: &Grid, struts: &[SweepStrut], brace: &SweepStrut) -> Option<Plane> {
    let seat = brace.seat_on_strut.and_then(|id| struts.get(id as usize));
    let base = match brace.landing {
        Landing::Strut { id, .. } => struts.get(id as usize),
        Landing::Plate | Landing::Model { .. } => None,
    };
    match (seat, base) {
        (Some(seat), Some(base)) => {
            plane_between(grid, seat.cell_at(brace.k_top), base.cell_at(brace.k_top))
        }
        _ => plane_between(grid, brace.cell, brace.base_cell()),
    }
}

/// Every tie on every member other than its own two ends. One pass.
///
/// **A brace is credited only where it ties the member's FREE SHAFT**
/// (fix/brace-not-in-flare, 2026-09-30): a brace end below the member's
/// `floors` entry ([`free_shaft_floor`]) lies inside its root flare or the
/// raft, which already hold that height rigidly, so it cuts no run. Landings
/// of other members are ties wherever they land, as before.
///
/// **A landing BRANCH holds its host only from the layer its own seat prints**
/// (Craig ruling 2026-10-01, tie timing; `stilt-and-brace-pair-diagnosis.md`
/// item 2). While the layers between the junction and the branch's seat
/// print, the branch is free at its upper end and holds nothing; the host
/// runs as a cantilever from its last tie below up to the layer printing. So
/// the tie sits at the landing layer and HOLDS FROM the branch's top
/// ([`Tie::from`]). **A diagonal is such a branch too**: it lands on one post
/// at the band bottom and seats on the other at the band top, and holds
/// nothing until that seat prints; so an `|X|` ties each post from its band
/// TOP, which is what the member-clearance lane's print-time guard
/// (`every_run_holds_with_crosses_credited_from_their_band_top_on_the_stilt_plate`)
/// already measured.
fn tie_lists(grid: &Grid, struts: &[SweepStrut], floors: &[usize]) -> Vec<Vec<Tie>> {
    let mut ties: Vec<Vec<Tie>> = vec![Vec::new(); struts.len()];
    let on_free_shaft = |host: u32, layer: usize| {
        floors
            .get(host as usize)
            .is_none_or(|&floor| layer >= floor)
    };
    for strut in struts {
        if !live(strut) {
            continue;
        }
        let brace = strut.kind == StrutKind::Brace;
        let plane = if brace {
            brace_plane(grid, struts, strut)
        } else {
            None
        };
        if let Landing::Strut { id, layer } = strut.landing {
            let credited = !brace || on_free_shaft(id, layer);
            // The branch's top is its seat (a diagonal's is its band top on
            // the other post): it holds the host from there.
            let from = strut.k_top.max(layer);
            if let Some(list) = ties.get_mut(id as usize).filter(|_| credited) {
                list.push(Tie {
                    layer,
                    from,
                    plane,
                    band_top: false,
                });
            }
        }
        if let Some(parent) = strut.seat_on_strut {
            let credited = !brace || on_free_shaft(parent, strut.k_top);
            if let Some(list) = ties.get_mut(parent as usize).filter(|_| credited) {
                list.push(Tie {
                    layer: strut.k_top,
                    from: strut.k_top,
                    plane,
                    band_top: plane.is_some(),
                });
            }
        }
    }
    ties
}

/// One member's runs, in every direction its ties distinguish.
fn member_runs(
    grid: &Grid,
    strut: &SweepStrut,
    ties: &[Tie],
    profile: &[f64],
    layer_height_mm: f64,
    law: &SlendernessLaw,
) -> Vec<UnbracedRun> {
    let measure = RunMeasure {
        grid,
        strut,
        thickened: &strut.thickened_mm,
        profile,
        layer_height_mm,
        law,
    };
    let mut runs: Vec<UnbracedRun> = Vec::new();
    if !live(strut) {
        return runs;
    }
    // CROSSES AT ONE LAYER ARE CREDITED TOGETHER (`joint_holds`); a layer
    // with a landing or an end holds every direction whatever crosses it has,
    // from the layer that tie holds from.
    let groups = ties_by_layer(ties);
    for sway_deg in sway_directions_of(&groups) {
        measure.push_runs(&groups, sway_deg, &mut runs);
    }
    runs
}

/// What one member's runs are measured on: the member, the per-layer minimum
/// its thickening set (normally its own [`SweepStrut::thickened_mm`]; the
/// prune passes the one the pass started from), and its drawn profile.
struct RunMeasure<'a> {
    grid: &'a Grid,
    strut: &'a SweepStrut,
    thickened: &'a [f64],
    profile: &'a [f64],
    layer_height_mm: f64,
    law: &'a SlendernessLaw,
}

impl RunMeasure<'_> {
    /// The member's runs in ONE sway direction, appended to `runs` (a run
    /// already listed, by its two layers, is not listed twice).
    ///
    /// **TIE TIMING** (Craig ruling 2026-10-01). A run is a cantilever from
    /// the highest tie that HOLDS while a layer prints up to that layer. A
    /// tie that holds from its own layer (a cross, an end) ends a run there,
    /// as before; a landing branch, which holds only from its seat
    /// ([`Tie::from`]), ends the run that was printing until then, the one
    /// from the tie below it up to the seat's layer (or the member's own top,
    /// whichever comes first), and starts the run above it only after that.
    /// With every tie holding at once this is exactly the old cut between
    /// consecutive ties. One sort, one sweep.
    fn push_runs(&self, groups: &[LayerTies], sway_deg: Option<f64>, runs: &mut Vec<UnbracedRun>) {
        let strut = self.strut;
        // A HEADED member's runs end at its JOINABLE top, not at `k_top`: a
        // band, a tie or an `|X|` never attaches above it, so the run's top
        // layer is the highest layer any of them may use.
        let Some(top) = strut.joinable_top_layer(self.layer_height_mm) else {
            return;
        };
        // (layer, holds from), inside the member, in this direction.
        let mut held: Vec<(usize, usize)> = groups
            .iter()
            .filter(|group| group.layer >= strut.k_bottom && group.layer <= top)
            .filter_map(|group| group.holds_from(sway_deg).map(|from| (group.layer, from)))
            .collect();
        held.sort_unstable_by_key(|&(_, from)| from);
        let mut ends: Vec<usize> = held
            .iter()
            .map(|&(_, from)| from.min(top))
            .filter(|&end| end > strut.k_bottom)
            .collect();
        ends.push(top);
        ends.sort_unstable();
        ends.dedup();
        let (mut base, mut next) = (strut.k_bottom, 0_usize);
        for k_top in ends {
            // Every tie that holds BEFORE this layer prints is a base the
            // cantilever can stand on; the highest of them is.
            while next < held.len() && held[next].1 < k_top {
                base = base.max(held[next].0);
                next += 1;
            }
            let k_lo = base;
            if k_lo >= k_top
                || runs
                    .iter()
                    .any(|run| run.k_lo == k_lo && run.k_top == k_top)
            {
                continue;
            }
            let length_mm = run_length(self.grid, strut, k_lo, k_top, self.layer_height_mm);
            let radius_mm = run_radius(strut, self.thickened, self.profile, k_lo, k_top);
            runs.push(UnbracedRun {
                strut_id: strut.id,
                k_lo,
                k_top,
                length_mm,
                radius_mm,
                limit_mm: self.law.max_unbraced_mm(radius_mm),
                sway_deg,
            });
        }
    }
}

/// The drawn radius at an absolute layer, or the member's own minimum where
/// the profile has no entry yet (a member this pass added and the down-sweep
/// has not sized).
pub(crate) fn drawn_at(strut: &SweepStrut, profile: &[f64], layer: usize) -> Option<f64> {
    drawn_at_with(strut, &strut.thickened_mm, profile, layer)
}

/// [`drawn_at`] with the thickening minimum given rather than the member's
/// own.
fn drawn_at_with(
    strut: &SweepStrut,
    thickened: &[f64],
    profile: &[f64],
    layer: usize,
) -> Option<f64> {
    let slot = layer.checked_sub(strut.k_bottom)?;
    let here = profile.get(slot).copied().unwrap_or(0.0);
    let minimum = strut
        .min_radius_mm
        .max(thickened.get(layer).copied().unwrap_or(0.0));
    let radius = here.max(minimum);
    (radius.is_finite() && radius > 0.0).then_some(radius)
}

/// `r_eq` of `n` layers, bottom first, from a radius per layer: the uniform
/// radius whose cantilever sways as far (see [`UnbracedRun::radius_mm`]).
/// Layers with no radius are left out and the weights re-normalised.
fn equivalent_radius(n: usize, radius_at: impl Fn(usize) -> Option<f64>) -> Option<f64> {
    if n == 0 {
        return None;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "layer counts bounded by the plan's own layer count are exact in f64"
    )]
    let n_f = n as f64;
    let mut compliance = 0.0;
    let mut weight = 0.0;
    let (mut narrowest, mut widest) = (f64::INFINITY, 0.0_f64);
    for j in 0..n {
        let Some(radius) = radius_at(j) else {
            continue;
        };
        narrowest = narrowest.min(radius);
        widest = widest.max(radius);
        #[expect(
            clippy::cast_precision_loss,
            reason = "a slot index bounded by the layer count is exact in f64"
        )]
        let (upper, lower) = (((n - j) as f64) / n_f, ((n - j - 1) as f64) / n_f);
        let w = upper.powi(4) - lower.powi(4);
        weight += w;
        compliance += w / radius.powi(4);
    }
    // A weighted power mean lies between its narrowest and widest input; the
    // clamp keeps float noise from reading a uniform post as a hair thinner
    // than itself.
    (compliance > 0.0 && weight > 0.0)
        .then(|| (weight / compliance).powf(0.25).clamp(narrowest, widest))
}

fn run_radius(
    strut: &SweepStrut,
    thickened: &[f64],
    profile: &[f64],
    k_lo: usize,
    k_top: usize,
) -> f64 {
    equivalent_radius(k_top.saturating_sub(k_lo), |j| {
        drawn_at_with(strut, thickened, profile, k_lo + j)
    })
    .unwrap_or_else(|| strut.tip_radius_mm())
}

/// The length of one run along the member's own axis.
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
fn run_length(
    grid: &Grid,
    strut: &SweepStrut,
    k_lo: usize,
    k_top: usize,
    layer_height_mm: f64,
) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "layer counts bounded by the plan's own layer count are exact in f64"
    )]
    // The chord of the member's POST: from the joint `K` for a headed arm,
    // from the seat otherwise.
    let (top_layer, top_cell) = match &strut.head {
        Some(head) => (head.k_joint, head.joint_cell as usize),
        None => (strut.k_top, strut.cell),
    };
    let span = top_layer.saturating_sub(strut.k_bottom) as f64;
    let lateral_mm = crate::cell_distance_mm(grid, top_cell, strut.base_cell());
    let member_mm = (span * layer_height_mm).hypot(lateral_mm);
    #[expect(
        clippy::cast_precision_loss,
        reason = "layer counts bounded by the plan's own layer count are exact in f64"
    )]
    let fraction = (k_top - k_lo) as f64 / span;
    member_mm * fraction
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
    /// `FLARE_R_FACTOR` — the congestion census's root-crowding disc factor
    /// (`congestion::root_discs_drawn`). A mast is plate-rooted, so the mast
    /// search has to answer the congestion lane's question before it lands
    /// one. Until 2026-09-30 this was also the emitter's flare foot; the
    /// emitter now stands every root on the owner's ChituBox boss instead.
    pub flare_factor: f64,
    /// Sabotage knob: whether a mast's cell must CLEAR the placed root discs,
    /// or merely be an empty cell as it was before 2026-09-06.
    pub mast_root_clearance: bool,
    /// `FINAL_CLEARANCE_GAP_MM`: a brace's or a mast's drawn solid clears the
    /// model by this much (the owner's 0.3 mm final hard limit, 2026-08-22).
    pub final_clearance_mm: f64,
    /// Sabotage knob: whether a brace's or a mast's DRAWN SOLID is tested
    /// against the model and against every member it is not joined to, or
    /// only its centreline cell, as before 2026-09-29.
    pub member_clearance: bool,
    /// The model's surface, in the mesh's frame (the plate at
    /// `plate_z_mm`), where the run built it: the thickening refusal
    /// ([`ThickenRefusal::Model`]) takes each member's contact zones from the
    /// faces its ends stand against and decides every slot with the census's
    /// own clearance predicate on the drawn frustum (challenger round 3,
    /// 2026-10-05). `None`: the raster reads alone, with the longest zones,
    /// and no slot is ever proved inside the gap already (so a raise the
    /// raster flags is refused).
    pub model_surface: Option<&'a geom_kernel::NativeSurface>,
    /// The plate's z in the mesh's frame (the raster's frame has it at 0).
    pub plate_z_mm: f64,
    /// Whether a member may land part-way down another member's shaft at all.
    /// Remedy 1 IS a shaft landing, so the descent knob that forbids one must
    /// forbid the other; otherwise the sabotage drill measures a rule it did
    /// not actually delete.
    pub shaft_landings: bool,
    /// Sabotage knobs, one per rung of the ladder.
    pub merge: bool,
    pub brace: bool,
    pub thicken: bool,
    pub mast: bool,
    pub max_struts: usize,
    /// `X_ALLOWED`, the registered area-law ratio. Read only to keep a trunk
    /// under a forced merge sized for the load it was just handed, with the
    /// down-sweep's own rule, until the down-sweep re-sizes the field.
    pub x_allowed: f64,
    /// Which faces a tip may carry, so the lip-seat and head-join passes
    /// measure every arm against the same face the finished plan seats it on
    /// ([`crate::tip_landing::SeatFaceRule`]).
    pub seat_faces: crate::tip_landing::SeatFaceRule,
    /// How a load step on a support arm is drawn
    /// ([`crate::adapter::LoadStepTaper`]): the lane, the lip-seat rule and the
    /// head-join rule read the same drawn profile the file gets.
    pub load_step_taper: crate::adapter::LoadStepTaper,
    /// **R2** (Craig ruling 2026-10-03, interim pending the slenderness
    /// coupon): a brace is never thicker than the thinner post it joins, so
    /// its reach is at most `S` times that post's radius. **On in
    /// production.** Sabotage knob: off, a band is not refused for a diagonal
    /// thicker than a post (and, the junction raise being deleted, such a
    /// diagonal is then drawn thicker than its post).
    pub brace_cap: bool,
    /// **R7 + D5 and D9** (Craig ruling 2026-10-03, interim pending the
    /// slenderness coupon): a cross is built only inside the window where it
    /// costs less resin than thickening, and never past the two-post ceiling
    /// of its pair; past the ceiling the run is thickened into the window.
    /// **On in production.** Sabotage knob: off, every admissible cross is
    /// built, as before 2026-10-03.
    pub brace_window: bool,
    /// **R6 + D4** (Craig ruling 2026-10-03, interim pending the slenderness
    /// coupon): a run is braced by a SET of bands to one partner, as many as
    /// the two-post frame needs with every band free to sway
    /// ([`crate::frame_bands`]), laid evenly over the run and built together;
    /// never by one band at a time credited as a fixed point. **On in
    /// production.** Sabotage knob: off, the ladder places one band at a time
    /// and counts `⌈L / limit⌉ − 1`, as before this ruling.
    pub frame_bands: bool,
    /// Whether the brace frame classifies EVERY pair it refuses (no room for
    /// the bands; past its two-post ceiling; a count that holds but no band
    /// set that can be planned on it) and counts it, exactly as the CLI's
    /// `brace frame:` line reports. **On for the emitted field.** Off, a pair
    /// whose bands cannot be planned at any count is classified only where
    /// the ladder's next rung reads the class (the pair a run is thickened
    /// into the window of) and its count is left out of the ledger; the
    /// orientation search's demand runs read no such count. Every decision,
    /// and so every strut built, is the same either way
    /// (`tests/it/frame_ledger.rs`).
    pub frame_ledger: bool,
    /// **D8** (Craig ruling 2026-10-03): where no other rung holds a run, the
    /// post is TIED TO THE MODEL's own nearby surface ([`crate::model_tie`]),
    /// the last rung before a warning. **On in production.** Sabotage knob:
    /// off, those runs are warnings again. It needs the model surface, so it
    /// fires only through [`apply_ties_with_model`].
    pub model_tie: bool,
}

/// What the model-tie rung needs beyond the slenderness context: the model
/// surface its candidates' solids are measured against (the finished-field
/// census's own), and the frame the lowering draws in.
#[derive(Clone, Copy)]
pub(crate) struct ModelTies<'a> {
    pub surface: &'a geom_kernel::NativeSurface,
    pub layer_plan: &'a plinth_core::LayerPlan,
    pub plate_z_mm: f64,
}

/// Remedy 1 of the ladder, on a sized field, in place: the forced merges.
///
/// Returns what it did and, for every arm it re-landed, the landing it gave
/// it — so the caller, which then re-sizes the field and runs the un-merge
/// pass, can say exactly which of THESE merges that pass put back. The ties
/// ([`apply_ties`]) run only after that pass (owner ruling 2026-09-29).
pub fn apply_merges(
    context: &SlendernessContext<'_>,
    struts: &mut [SweepStrut],
) -> (SlendernessOutcome, Vec<(u32, Landing)>) {
    let mut forced: Vec<(u32, Landing)> = Vec::new();
    let law = context.law;
    let mut outcome = SlendernessOutcome {
        ratio: law.ratio(),
        thin_brace_reach_mm: law.thinnest_brace_reach_mm(),
        thick_brace_reach_mm: law.thickest_brace_reach_mm(),
        ..SlendernessOutcome::default()
    };
    if !law.binds() {
        return (outcome, forced);
    }

    let runs = unbraced_runs_drawn(
        context.grid,
        struts,
        context.layer_height_mm,
        law,
        context.load_step_taper,
    );
    let mut over: Vec<UnbracedRun> = runs.into_iter().filter(UnbracedRun::over_limit).collect();
    outcome.runs_over_limit_before = over.len();
    let mut named: Vec<u32> = over.iter().map(|run| run.strut_id).collect();
    named.sort_unstable();
    named.dedup();
    outcome.arms_over_limit_before = named.len();
    if over.is_empty() {
        return (outcome, forced);
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
    let index = {
        let mut index = ColumnHistory::new(context.grid);
        stamp_all(&mut index, struts, context.layer_height_mm);
        index
    };

    // Remedy 1 first, for every run it can serve, because a merge costs no new
    // member at all.
    //
    // SUPPORT COMES BEFORE WHAT RESTS ON IT (2026-09-28). The index above is
    // stamped once, and this loop moves arms, so two rules keep every landing
    // it makes standing on something that is still there when the loop ends:
    //
    // * an arm that CARRIES SOMEONE — another member lands on it, or a brace
    //   seats on it — is never re-landed, because re-landing it cuts away the
    //   part its dependants rest on; and an arm that is merged ONTO becomes
    //   such an arm at once, so it is frozen for the rest of the loop;
    // * a shaft the index offers is taken only if its member STANDS THERE NOW
    //   ([`crate::landing_check::stands_at`], `O(1)` against the live field),
    //   because the index still shows the old column of every arm this loop
    //   has already moved.
    //
    // Before this, the loop ran in id order and built daisy chains: a child
    // was merged onto a parent's shaft at layer L, the parent — over its own
    // limit — was then merged onto a third arm at L' > L, and the child's base
    // was left in the air. 157 such members on the Cylinder, 28 on the Sphere.
    // The refused links fall to the brace, the mast, or a warning.
    let mut frozen = crate::landing_check::carries_someone(struts);
    if context.merge && context.shaft_landings {
        for run in &over {
            let Some(strut) = struts.get(run.strut_id as usize) else {
                continue;
            };
            // A HEADED arm is never merged: its head is frozen at placement
            // and a merge re-routes the whole post.
            if strut.head.is_some() {
                outcome.headed_kept += 1;
                continue;
            }
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
            if frozen.get(run.strut_id as usize).copied().unwrap_or(true) {
                outcome.merges_refused_carrying += 1;
                continue;
            }
            let Some((landing, k_bottom, path)) = merge_candidate(
                context,
                &index,
                struts,
                run.strut_id,
                run.limit_mm,
                &mut outcome.merges_refused_stale_shaft,
            ) else {
                continue;
            };
            if let Landing::Strut { id: parent, .. } = landing {
                if let Some(flag) = frozen.get_mut(parent as usize) {
                    *flag = true;
                }
            }
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
            // The arm's own load at the root it just left: what it now hands
            // to the trunk it landed on.
            let handed_mm2 = strut.carried_mm2.first().copied().unwrap_or(0.0);
            hand_load_down(context, struts, landing, handed_mm2);
            outcome.merges_forced += 1;
            forced.push((run.strut_id, landing));
        }
    }
    (outcome, forced)
}

/// Rungs 2 to 5 of the ladder — `|X|`, thicken, mast, warn — on a field the
/// merges and the un-merge pass have settled, in place.
///
/// **Every remedy is attempted only for a run that is over the limit NOW.**
/// Members are taken from a worklist; each member's runs are cut again from
/// the LIVE strut set before every attempt, and every remedy updates the tie
/// lists and drawn profiles of exactly the members it touched (`TieState`).
/// Until 2026-09-29 the list of runs was measured once, before any tie, and
/// walked to the end: a partner that an earlier cross had already tied still
/// built its own cross (the double `|X|`), or, when that band hit the first
/// cross, dropped a mast beside a post that was already held.
///
/// Remedies only ever ADD ties or section, so no member's runs get worse and
/// nothing already done is re-queued; a new mast is. Each attempt either
/// applies a remedy (bounded: members by `max_struts`, thickenings by the
/// runs) or marks that run as having no admissible remedy, so the loop ends.
///
/// Returns how many remedies it applied. The caller re-sizes the field with the
/// down-sweep whenever that is not zero and calls again, to a fixed point.
///
/// This entry has no model surface, so the model-tie rung (D8) does not fire
/// here. The sweep runs this to its fixed point first, and only then
/// [`apply_ties_with_model`] with the other rungs off, so a post is tied to
/// the model only where nothing else held it after every pass.
pub fn apply_ties(
    context: &SlendernessContext<'_>,
    struts: &mut Vec<SweepStrut>,
    outcome: &mut SlendernessOutcome,
) -> usize {
    apply_ties_with_model(context, struts, outcome, None)
}

/// [`apply_ties`] with the model surface, so that a run no other rung holds
/// may be TIED TO THE MODEL (D8, Craig ruling 2026-10-03), the last rung
/// before a warning ([`tie_to_model`]).
pub(crate) fn apply_ties_with_model(
    context: &SlendernessContext<'_>,
    struts: &mut Vec<SweepStrut>,
    outcome: &mut SlendernessOutcome,
    model: Option<&ModelTies<'_>>,
) -> usize {
    let law = context.law;
    if !law.binds() {
        return 0;
    }
    let grid = context.grid;
    let height = context.layer_height_mm;
    let first_new = struts.len();
    let mut state = TieState::build(context, struts);
    outcome.landing_ties_held_late = state
        .ties
        .iter()
        .flatten()
        .filter(|tie| tie.from > tie.layer)
        .count();
    let mut applied = 0_usize;
    // ROUNDS, TO A FIXED POINT INSIDE THE PASS. A run with no admissible
    // remedy is set aside for the rest of a round; a remedy applied later in
    // the same round (a new mast, a new cross, a thicker trunk) can give it a
    // partner or room it did not have, so the round is run again until one
    // applies nothing. Every round that continues applied at least one remedy,
    // so the rounds are bounded by the same bound the remedies are.
    'rounds: for _round in 0..=context.max_struts {
        let before = applied;
        let mut queued = vec![false; struts.len()];
        // BRANCHES BEFORE THEIR TRUNKS. Thickening a member raises every trunk
        // it stands on, from the junction down to that trunk's root; a trunk
        // tied BEFORE one of its branches was thickened could be left carrying
        // a cross that no longer holds anything. So the members that stand
        // deepest in the landing tree go first, and a trunk is measured after
        // every raise its branches will hand it.
        let mut first: Vec<(usize, u32)> = Vec::new();
        for (index, strut) in struts.iter().enumerate() {
            let over = member_runs(
                grid,
                strut,
                &state.ties[index],
                &state.profiles[index],
                height,
                law,
            )
            .iter()
            .any(UnbracedRun::over_limit);
            if over {
                first.push((landing_depth(struts, strut), strut.id));
                queued[index] = true;
            }
        }
        first.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let mut queue: VecDeque<u32> = first.into_iter().map(|(_, id)| id).collect();
        let mut exhausted: BTreeSet<RunKey> = BTreeSet::new();
        'members: while let Some(id) = queue.pop_front() {
            let index = id as usize;
            if let Some(flag) = queued.get_mut(index) {
                *flag = false;
            }
            for _attempt in 0..=context.max_struts {
                if struts.len() + 2 > context.max_struts {
                    break 'rounds;
                }
                let Some(strut) = struts.get(index) else {
                    continue 'members;
                };
                let run = member_runs(
                    grid,
                    strut,
                    &state.ties[index],
                    &state.profiles[index],
                    height,
                    law,
                )
                .into_iter()
                .filter(|run| run.over_limit() && !exhausted.contains(&run_key(run)))
                // THE HIGHEST RUN FIRST. Thickening a run raises the member
                // all the way down to its foot, so a cross placed first on a
                // run below it could be left holding nothing; nothing done to
                // a lower run changes a higher one.
                .max_by_key(|run| (run.k_lo, run.k_top));
                let Some(run) = run else {
                    continue 'members;
                };
                // A CROSS takes the first of the ties the run needs, evenly
                // spaced; the member is measured again before the next one.
                // A mast plans its own layer ([`mast_tie_layer`]).
                let k = tie_layer(&run, 1, extra_ties(&run).max(1));
                // A FURTHER PLANE GOES TO A LAYER THE MEMBER IS ALREADY TIED
                // AT, where that layer alone cuts the run into pieces inside
                // the limit: its planes are then credited together there
                // (`joint_holds`). Until 2026-09-29 each direction took its own
                // mid-run layer, and a post collected crosses at five levels
                // where two would do (the stilt plate's post 125). Under R6 a
                // further plane is a band set like the first, laid on the same
                // evenly spaced layers wherever its count is the same.
                let shared = if context.frame_bands {
                    None
                } else {
                    shared_tie_layer(
                        grid,
                        strut,
                        &state.ties[index],
                        &state.profiles[index],
                        &run,
                        height,
                        law,
                        k,
                    )
                };
                let mut miss = CrossMiss::default();
                // R6 (2026-10-03): the run is tied by a SET of bands to one
                // partner, counted through the frame and built at once; the
                // one-band-at-a-time ladder below is the sabotage path.
                if context.brace && context.frame_bands {
                    if let Some(k) = k {
                        match tie_by_band_set(context, &mut state, struts, &run, k, outcome) {
                            Ok(()) => {
                                applied += 1;
                                continue;
                            }
                            Err(missed) => miss.merge(missed),
                        }
                    }
                } else if context.brace {
                    if let Some(at) = shared {
                        match tie_by_crosses(context, &mut state, struts, &run, at, false, outcome)
                        {
                            Ok(()) => {
                                outcome.crosses_at_an_existing_tie_layer += 1;
                                applied += 1;
                                continue;
                            }
                            Err(missed) => miss.merge(missed),
                        }
                    }
                    if let Some(k) = k {
                        match tie_by_crosses(context, &mut state, struts, &run, k, true, outcome) {
                            Ok(()) => {
                                applied += 1;
                                continue;
                            }
                            Err(missed) => miss.merge(missed),
                        }
                    }
                }
                if context.thicken {
                    // PAST THE TWO-POST CEILING (D9): no more bands between
                    // that pair; thicken the run INTO the pair's window, then
                    // brace, where that costs less resin than thickening it
                    // all the way (or where full thickening is refused).
                    if let Some(past) = miss.past_ceiling {
                        if thicken_into_the_window(context, &mut state, struts, &run, past, outcome)
                        {
                            applied += 1;
                            continue;
                        }
                    }
                    if thicken(context, &mut state, struts, &run, outcome) {
                        if miss.thicken_cheaper {
                            outcome.thickened_where_cheaper_than_bracing += 1;
                            let ratio = run.length_mm / run.limit_mm;
                            outcome.window_highest_thickened_ratio =
                                outcome.window_highest_thickened_ratio.max(ratio);
                        }
                        applied += 1;
                        continue;
                    }
                }
                if context.mast {
                    // A MAST HAS ITS OWN TIE LAYER (fix/mast-tie-and-overlap-factor,
                    // 2026-10-02): where the piece above it is within its own
                    // limit, not the crosses' first evenly spaced layer.
                    let mast_k = mast_tie_layer(
                        grid,
                        &struts[index],
                        &state.profiles[index],
                        &run,
                        height,
                        law,
                    );
                    if let Some(tie) = mast_k {
                        if k.is_some_and(|k| tie.layer > k) {
                            outcome.mast_ties_planned_above_the_first_tie += 1;
                        }
                        if tie.raised {
                            outcome.mast_ties_raised_for_the_piece_above += 1;
                        }
                    }
                    if let Some(k) = mast_k.map(|tie| tie.layer) {
                        if let Some(new_mast) = tie_by_mast_above_the_flares(
                            context, &mut state, struts, &run, k, outcome,
                        ) {
                            applied += 1;
                            if let Some(mast) = new_mast {
                                queued.resize(struts.len(), false);
                                if !queued[mast as usize] {
                                    queued[mast as usize] = true;
                                    queue.push_back(mast);
                                }
                            }
                            continue;
                        }
                    }
                }
                // D8 (Craig ruling 2026-10-03): THE LAST RUNG BEFORE A
                // WARNING. No cross, thickening or mast holds the run, so the
                // post is tied to the model's own nearby surface, by ties
                // seated like any support tip and counted through a frame.
                // The sweep reaches it only in its own passes after the
                // ladder's fixed point, with the other rungs off.
                if context.model_tie {
                    if let Some(model) = model {
                        if tie_to_model(context, model, &mut state, struts, &run, outcome) {
                            applied += 1;
                            continue;
                        }
                    }
                }
                // No admissible remedy this round: the run is set aside, and
                // if no later round finds it one, the census at the end of the
                // sweep names it as a warning.
                exhausted.insert(run_key(&run));
            }
        }
        if applied == before {
            break;
        }
    }
    prune_unneeded_crosses(context, &state, struts, first_new, outcome);
    let reads = state.thicken_reads.get();
    outcome.thicken_slots_flagged += reads.flagged;
    outcome.thicken_slots_measured_clear += reads.measured_clear;
    outcome.thicken_slots_inside_already += reads.inside_already;
    outcome.thicken_slots_undecided += reads.undecided;
    outcome.thicken_members_hidden += reads.hidden_members;
    applied
}

/// A layer inside `run` at which a cross already ties the member and which,
/// on its own, cuts the run into two pieces each inside its limit; the one
/// nearest `planned` (the evenly spaced layer), or `None`. Measured with the
/// production run length and drawn equivalent radius, one pair of pieces per
/// candidate layer.
#[expect(
    clippy::too_many_arguments,
    reason = "the member, its ties and drawn profile, the run, the layer height, the law and the \
              planned layer are each needed and none derives from another"
)]
fn shared_tie_layer(
    grid: &Grid,
    strut: &SweepStrut,
    ties: &[Tie],
    profile: &[f64],
    run: &UnbracedRun,
    layer_height_mm: f64,
    law: &SlendernessLaw,
    planned: Option<usize>,
) -> Option<usize> {
    let planned = planned?;
    let fits = |k_lo: usize, k_top: usize| {
        let length = run_length(grid, strut, k_lo, k_top, layer_height_mm);
        length <= law.max_unbraced_mm(run_radius(strut, &strut.thickened_mm, profile, k_lo, k_top))
    };
    let mut best: Option<usize> = None;
    for tie in ties {
        let layer = tie.layer;
        if !tie.band_top || layer <= run.k_lo || layer >= run.k_top || layer == planned {
            continue;
        }
        if best.is_some_and(|known| known.abs_diff(planned) <= layer.abs_diff(planned)) {
            continue;
        }
        if fits(run.k_lo, layer) && fits(layer, run.k_top) {
            best = Some(layer);
        }
    }
    best
}

/// How many landings down a member stands from the plate or the model: `0` for
/// a member that lands on either, one more for each trunk below. One walk
/// down the landings, bounded by the strut count.
fn landing_depth(struts: &[SweepStrut], strut: &SweepStrut) -> usize {
    let mut depth = 0_usize;
    let mut at = strut.landing;
    while let Landing::Strut { id, .. } = at {
        depth += 1;
        if depth > struts.len() {
            break;
        }
        at = struts
            .get(id as usize)
            .map_or(Landing::Plate, |host| host.landing);
    }
    depth
}

/// **Every cross must be needed** — closed at the end of each pass.
///
/// The worklist ties one member at a time, so a cross that closed a member's
/// tie can be made unneeded by a cross a LATER member builds to the same post
/// (measured on the stilt plate: arm 86's pair at layer 350 was joined by a
/// third plane from a neighbour, and its cross to 87 then held nothing). The
/// rule is the field report's test 2 applied by the lane itself: each cross
/// built in this pass, latest first, is taken out if neither post it ties has
/// a run over the limit through its band without it. Latest first, and each
/// against the set as it stands, so every cross that is kept is needed given
/// every other one kept. A mast left with no cross goes with it.
///
/// **Needed in its own plane is needed** (Craig ruling 2026-10-01): a cross
/// the first test finds unneeded is judged again on the posts as they stood
/// before this pass's thickenings, in the directions it holds; a thickening
/// sized for the direction across the cross never takes the cross out.
///
/// The drawn profiles are read as they stand; a diagonal raises no post
/// (R2, 2026-10-03), so taking one out leaves every post's section as it is.
///
/// Members this pass added sit at the tail of `struts` and nothing older
/// refers to them, so the tail is compacted in place and its ids remapped.
/// Returns how many crosses (and masts) it took out.
fn prune_unneeded_crosses(
    context: &SlendernessContext<'_>,
    state: &TieState,
    struts: &mut Vec<SweepStrut>,
    first_new: usize,
    outcome: &mut SlendernessOutcome,
) -> usize {
    let grid = context.grid;
    let law = context.law;
    let height = context.layer_height_mm;
    let len = struts.len();
    if first_new >= len {
        return 0;
    }
    // The crosses of this pass: `commit_cross` pushes the two diagonals of
    // one cross back to back.
    let mut crosses: Vec<(usize, usize)> = Vec::new();
    let mut at = first_new;
    while at + 1 < len {
        let (one, two) = (&struts[at], &struts[at + 1]);
        let twins = one.kind == StrutKind::Brace
            && two.kind == StrutKind::Brace
            && one.k_top == two.k_top
            && one.k_bottom == two.k_bottom
            && one
                .seat_on_strut
                .is_some_and(|seat| matches!(two.landing, Landing::Strut { id, .. } if id == seat));
        if twins {
            crosses.push((at, at + 1));
            at += 2;
        } else {
            at += 1;
        }
    }
    // A BAND SET IS JUDGED AS ONE (R6, 2026-10-03): its bands were counted
    // together through the frame, and taking one out leaves the frame short
    // even where the fixed-point measure below would call that band
    // redundant. Every other cross is a group of one, as before.
    let mut groups: Vec<Vec<(usize, usize)>> = Vec::new();
    for &cross in &crosses {
        let set = state
            .band_sets
            .iter()
            .find(|&&(first, count)| cross.0 >= first && cross.0 < first + 2 * count);
        let joins_last = set.is_some_and(|&(first, _)| {
            groups
                .last()
                .and_then(|group| group.first())
                .is_some_and(|&(head, _)| head >= first && cross.0 > head)
        });
        match groups.last_mut() {
            Some(group) if joins_last => group.push(cross),
            _ => groups.push(vec![cross]),
        }
    }
    let mut ties = state.ties.clone();
    let mut removed = vec![false; len];
    let mut taken = 0_usize;
    for group in groups.iter().rev() {
        let Some(&(head, _)) = group.first() else {
            continue;
        };
        let brace = &struts[head];
        let (Some(a), Landing::Strut { id: b, .. }) = (brace.seat_on_strut, brace.landing) else {
            continue;
        };
        // Every band of the group: (band top, band bottom, plane).
        let bands: Vec<(usize, usize, Option<Plane>)> = group
            .iter()
            .map(|&(one, _)| {
                let diagonal = &struts[one];
                (
                    diagonal.k_top,
                    diagonal.k_bottom,
                    brace_plane(grid, struts, diagonal),
                )
            })
            .collect();
        let in_group = |index: usize| group.iter().any(|&(one, two)| index == one || index == two);
        let without = |post: u32| {
            let mut list = ties[post as usize].clone();
            for &(k, k_lo, plane) in &bands {
                for layer in [k, k_lo] {
                    if let Some(position) = list
                        .iter()
                        .position(|tie| tie.layer == layer && tie.plane == plane)
                    {
                        list.remove(position);
                    }
                }
            }
            list
        };
        // THE POST AS IT WOULD BE DRAWN WITHOUT THIS CROSS: its ledger, raised
        // by every branch that still joins it (the lowering's junction clause,
        // one post at a time). A brace raises nothing since 2026-10-03 (R2:
        // it is never thicker than its posts), so no diagonal, the two being
        // judged or any other, enters the clause, as in
        // `adapter::drawn_profiles`. `profiles` is `state.profiles` (as drawn
        // now) or `state.unthickened` (as drawn without this pass's
        // thickenings).
        let drawn_without = |post: u32, profiles: &[Vec<f64>]| {
            let host = &struts[post as usize];
            let mut profile = if host.radius_mm.is_empty() {
                profiles[post as usize].clone()
            } else {
                host.radius_mm.clone()
            };
            for (index, branch) in struts.iter().enumerate() {
                if removed[index]
                    || in_group(index)
                    || !live(branch)
                    || branch.kind == StrutKind::Brace
                {
                    continue;
                }
                let mut joints: Vec<(usize, Option<f64>)> = Vec::new();
                if matches!(branch.landing, Landing::Strut { id, .. } if id == post) {
                    joints.push((branch.k_bottom, profiles[index].first().copied()));
                }
                if branch.seat_on_strut == Some(post) {
                    joints.push((branch.k_top, profiles[index].last().copied()));
                }
                for (layer, end) in joints {
                    let (Some(end), Some(top)) = (end, layer.checked_sub(host.k_bottom)) else {
                        continue;
                    };
                    for radius in profile.iter_mut().take(top + 1) {
                        *radius = radius.max(end);
                    }
                }
            }
            profile
        };
        let through_band = |run: &UnbracedRun| {
            run.over_limit()
                && bands
                    .iter()
                    .any(|&(k, k_lo, _)| run.k_lo < k && k_lo < run.k_top)
        };
        let needed = [a, b].iter().any(|&post| {
            member_runs(
                grid,
                &struts[post as usize],
                &without(post),
                &drawn_without(post, &state.profiles),
                height,
                law,
            )
            .iter()
            .any(through_band)
        });
        if needed {
            continue;
        }
        // **KEEP ONE-DIRECTION CROSSES** (Craig ruling 2026-10-01). A cross
        // that ties two columns holds the sway in its own plane; thickening
        // (or a mast) covers the direction across it, and a thickening that
        // ALSO holds the cross's plane does not make the cross unneeded. So
        // the cross is judged a second time on the posts as they were before
        // this pass thickened anything (`state.unthickened`, and each post's
        // thickening minimum as the pass found it), and only in the
        // directions the cross itself holds: in each sway direction its
        // member's ties tell apart WITH the cross, where the crosses at its
        // band top (credited together, `joint_holds`) hold that direction, is
        // a run through the band over the limit WITHOUT it? A cross another
        // cross or a landing makes redundant still goes: there, nothing is
        // over without it in its own directions.
        let own_plane_needed = [a, b].iter().any(|&post| {
            let index = post as usize;
            let strut = &struts[index];
            let profile = drawn_without(post, &state.unthickened);
            let measure = RunMeasure {
                grid,
                strut,
                thickened: state
                    .thickened_at_start
                    .get(index)
                    .map_or(&[][..], Vec::as_slice),
                profile: &profile,
                layer_height_mm: height,
                law,
            };
            let with = ties_by_layer(&ties[index]);
            let without_groups = ties_by_layer(&without(post));
            let planes_here: Vec<(Option<Plane>, Vec<Plane>)> = bands
                .iter()
                .map(|&(k, _, plane)| (plane, planes_at(&ties[index], k)))
                .collect();
            sway_directions_of(&with).into_iter().any(|sway| {
                let held_by_the_cross = planes_here.iter().any(|(plane, here)| {
                    plane.is_none() || sway.is_none_or(|sway| joint_holds(here, sway))
                });
                if !held_by_the_cross {
                    return false;
                }
                let mut runs = Vec::new();
                measure.push_runs(&without_groups, sway, &mut runs);
                runs.iter().any(through_band)
            })
        });
        if own_plane_needed {
            outcome.crosses_kept_in_their_own_plane += 1;
            continue;
        }
        let (list_a, list_b) = (without(a), without(b));
        ties[a as usize] = list_a;
        ties[b as usize] = list_b;
        for &(one, two) in group {
            removed[one] = true;
            removed[two] = true;
        }
        taken += group.len();
        outcome.braces_added = outcome.braces_added.saturating_sub(group.len());
        outcome.crosses_pruned += group.len();
        if group.len() > 1 || state.band_sets.iter().any(|&(first, _)| first == head) {
            outcome.frame_sets_pruned += 1;
            outcome.frame_bands_built = outcome.frame_bands_built.saturating_sub(group.len());
            outcome.frame_band_sets = outcome.frame_band_sets.saturating_sub(1);
        }
    }
    if taken == 0 {
        return 0;
    }
    // A mast of this pass left with nothing tied to it goes too.
    for index in first_new..len {
        if struts[index].kind != StrutKind::Mast {
            continue;
        }
        let mast = struts[index].id;
        let tied = (first_new..len).any(|other| {
            !removed[other]
                && struts[other].kind == StrutKind::Brace
                && (struts[other].seat_on_strut == Some(mast)
                    || matches!(struts[other].landing, Landing::Strut { id, .. } if id == mast))
        });
        if !tied {
            removed[index] = true;
            outcome.masts_dropped = outcome.masts_dropped.saturating_sub(1);
            taken += 1;
        }
    }
    let mut new_id: Vec<u32> = (0..len)
        .map(|id| u32::try_from(id).unwrap_or(u32::MAX))
        .collect();
    let tail: Vec<SweepStrut> = struts.drain(first_new..).collect();
    for (offset, member) in tail.into_iter().enumerate() {
        let old = first_new + offset;
        if removed[old] {
            new_id[old] = u32::MAX;
            continue;
        }
        new_id[old] = u32::try_from(struts.len()).unwrap_or(u32::MAX);
        struts.push(member);
    }
    let remap = |id: u32| new_id.get(id as usize).copied().unwrap_or(id);
    for member in &mut struts[first_new..] {
        member.id = remap(member.id);
        member.seat_on_strut = member.seat_on_strut.map(remap);
        if let Landing::Strut { id, layer } = member.landing {
            member.landing = Landing::Strut {
                id: remap(id),
                layer,
            };
        }
    }
    taken
}

type RunKey = (u32, usize, usize, Option<i64>);

/// A run's identity for the "no admissible remedy" set. The sway direction is
/// an arc midpoint computed from integer grid steps, so it is keyed to a
/// micro-degree, which is far finer than any two distinct arcs.
fn run_key(run: &UnbracedRun) -> RunKey {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a direction in [0, 180) degrees times 1e6 fits an i64 exactly enough to key it"
    )]
    let sway = run.sway_deg.map(|deg| (deg * 1.0e6).round() as i64);
    (run.strut_id, run.k_lo, run.k_top, sway)
}

/// What the tie rungs keep current as they edit the field, so that no member
/// is ever measured from a stale list.
struct TieState {
    /// Every member's shaft, stamped once and then extended member by member.
    index: ColumnHistory,
    /// The DRAWN profile of every member (ledger plus junction raises), raised
    /// where a thickening or a diagonal's junction raises it.
    profiles: Vec<Vec<f64>>,
    /// The same profiles raised by everything EXCEPT this pass's thickenings,
    /// and every member's thickening minimum as the pass found it. The prune
    /// judges a cross on these (Craig ruling 2026-10-01): a thickening never
    /// makes a cross that holds its own plane unneeded.
    unthickened: Vec<Vec<f64>>,
    thickened_at_start: Vec<Vec<f64>>,
    /// Every member's ties other than its own two ends.
    ties: Vec<Vec<Tie>>,
    /// The height bands of the crosses on each unordered pair of posts.
    bands: BTreeMap<(u32, u32), Vec<(usize, usize)>>,
    /// The masts each arm is tied to.
    masts_of: BTreeMap<u32, Vec<u32>>,
    /// The root discs on the plate, as the congestion lane measures them;
    /// grown as roots are thickened and extended as masts are dropped.
    placed_roots: Vec<crate::congestion::RootDisc>,
    /// The lowering's root-flare geometry, so a band's ends are kept on the
    /// posts' free shafts (`None`: the registry was unreadable, and only the
    /// posts' own feet bound a band, as before 2026-09-30).
    root_flare: Option<SweepRootFlare>,
    /// The lowering's root boss ([`plinth_support::RootBoss`]: the owner's
    /// Heavy boss, its foot the root times 2.2 / 1.5), so the thicken rung's
    /// root-lap test measures the feet the file draws
    /// (fix/mast-tie-and-overlap-factor, 2026-10-02). `None`: the registry
    /// was unreadable, and the test falls back to the crowding discs, as
    /// before this date.
    root_boss: Option<RootBoss>,
    /// Every member's drawn shaft line off its planned one, as `(at its
    /// seat, at its base)`: a leaning plate post's own
    /// (`adapter::planned_line_offset`, its file shaft runs to its boss top
    /// over its foot), plus what a member joined to one follows
    /// (`footprint::followed_shifts_drawn`); `None` where the two lines
    /// coincide. Members added during the pass have no entry
    /// ([`Self::member_end_off`] reads their hosts).
    drawn_offsets: Vec<Option<crate::adapter::LineOffset>>,
    /// Set by [`plan_cross`] when it drops a band whose bottom fell inside a
    /// flare because the band could not move within its posts: how many layers
    /// the band had to rise. The mast rung reads it to try again with a mast
    /// that tall (a new mast is only as tall as the layer it is planned at).
    flare_shortfall: std::cell::Cell<Option<usize>>,
    /// The band sets built in this pass (R6): `(index of its first diagonal,
    /// crosses)`. A set's diagonals are pushed back to back, two per cross,
    /// and the prune judges a set as one ([`prune_unneeded_crosses`]).
    band_sets: Vec<(usize, usize)>,
    /// The thickening refusal's reads over the pass (path hits), summed
    /// into the outcome when the pass ends.
    thicken_reads: std::cell::Cell<ThickenReads>,
}

/// What the thickening refusal read ([`thickening_enters_the_gap`]): raised
/// slots the raster flagged, and of those the ones the census's predicate
/// measured clear when committed, and the ones it proved inside the gap
/// already as drawn.
#[derive(Clone, Copy, Debug, Default)]
struct ThickenReads {
    flagged: usize,
    measured_clear: usize,
    inside_already: usize,
    /// Flagged slots whose committed read was an upper bound at or above the
    /// gap: undecided, and so not a refusal (rule A, 2026-10-05).
    undecided: usize,
    /// Raised members the refusal did not read because they stand in a
    /// hidden cavity (rule H, 2026-10-05).
    hidden_members: usize,
}

impl TieState {
    fn build(context: &SlendernessContext<'_>, struts: &[SweepStrut]) -> Self {
        let mut index = ColumnHistory::new(context.grid);
        stamp_all(&mut index, struts, context.layer_height_mm);
        let (profiles, _) = crate::adapter::drawn_profiles(struts, context.load_step_taper);
        let root_flare = SweepRootFlare::registered(context.layer_height_mm).ok();
        let floors: Vec<usize> = struts
            .iter()
            .map(|strut| {
                free_shaft_floor(
                    root_flare.as_ref(),
                    context.grid,
                    strut,
                    context.layer_height_mm,
                )
            })
            .collect();
        let ties = tie_lists(context.grid, struts, &floors);
        let mut bands: BTreeMap<(u32, u32), Vec<(usize, usize)>> = BTreeMap::new();
        let mut masts_of: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for strut in struts {
            if strut.kind != StrutKind::Brace || !live(strut) {
                continue;
            }
            let (Some(seat), Landing::Strut { id: base, .. }) =
                (strut.seat_on_strut, strut.landing)
            else {
                continue;
            };
            let band = (strut.k_bottom, strut.k_top);
            let list = bands.entry((seat.min(base), seat.max(base))).or_default();
            if !list.contains(&band) {
                list.push(band);
            }
            for (post, other) in [(seat, base), (base, seat)] {
                if struts
                    .get(other as usize)
                    .is_some_and(|member| member.kind == StrutKind::Mast)
                {
                    let masts = masts_of.entry(post).or_default();
                    if !masts.contains(&other) {
                        masts.push(other);
                    }
                }
            }
        }
        // THE ROOT DISCS ALREADY ON THE PLATE. A mast is plate-rooted and lays
        // a root flare, and a thickened plate root lays a wider one, so either
        // placed on top of a root that is already there re-creates exactly the
        // stacking the congestion lane just spent its rounds relieving
        // (measured at 3ff52c7: `make_laid_tube` finished at 1.0926 with 155
        // overlapping pairs from masts alone). Built once per pass from the
        // sized field; every mast and every grown root updates it.
        // Sized from the DRAWN profiles held just above, the ones the lowering
        // flares (review-fixes 2026-09-29), and kept in step with them as
        // crosses and thickenings raise roots (`commit_cross`, `thicken`).
        let placed_roots = crate::congestion::root_discs_drawn(
            context.grid,
            context.history,
            struts,
            &profiles,
            context.flare_factor,
        );
        let unthickened = profiles.clone();
        let thickened_at_start = struts
            .iter()
            .map(|strut| strut.thickened_mm.clone())
            .collect();
        // Each member's DRAWN line against its planned one: a leaning plate
        // post's own (its shaft to its boss top over its foot), plus, for a
        // member joined to such a post, the displacement it follows at the
        // ends it is joined by, transitively (a branch on the post, a
        // diagonal on the branch), exactly as the adapter draws them.
        let kinks: Vec<Option<crate::adapter::LineOffset>> = struts
            .iter()
            .map(|strut| {
                root_flare.as_ref().and_then(|flare| {
                    crate::adapter::planned_line_offset(
                        flare,
                        context.grid,
                        context.plate_z_mm,
                        context.layer_height_mm,
                        strut,
                    )
                })
            })
            .collect();
        let drawn_offsets: Vec<Option<crate::adapter::LineOffset>> =
            if kinks.iter().any(Option::is_some) {
                crate::footprint::followed_shifts_drawn(struts, &kinks)
                    .into_iter()
                    .zip(&kinks)
                    .map(|(followed, kink)| {
                        let add = |a: Option<[f64; 3]>, b: Option<[f64; 3]>| match (a, b) {
                            (None, None) => None,
                            (a, b) => {
                                let (a, b) = (a.unwrap_or([0.0; 3]), b.unwrap_or([0.0; 3]));
                                Some(std::array::from_fn(|k| a[k] + b[k]))
                            }
                        };
                        let seat = add(followed.seat_mm, kink.map(|kink| kink.0));
                        let base = add(followed.base_mm, kink.map(|kink| kink.1));
                        (seat.is_some() || base.is_some())
                            .then(|| (seat.unwrap_or([0.0; 3]), base.unwrap_or([0.0; 3])))
                    })
                    .collect()
            } else {
                vec![None; struts.len()]
            };
        Self {
            index,
            profiles,
            unthickened,
            thickened_at_start,
            ties,
            bands,
            masts_of,
            placed_roots,
            root_flare,
            root_boss: RootBoss::registered().ok(),
            drawn_offsets,
            flare_shortfall: std::cell::Cell::new(None),
            band_sets: Vec::new(),
            thicken_reads: std::cell::Cell::new(ThickenReads::default()),
        }
    }

    /// Where member `id`'s DRAWN shaft stands at `layer`, off the axis cell
    /// the plan keeps for it (`[0; 3]` for every member whose drawn line is
    /// its planned one): a diagonal tied to it is measured, and drawn, there
    /// (fix/morel-long-posts, 2026-10-02; `adapter::drawn_line_offset`).
    fn drawn_off(&self, struts: &[SweepStrut], id: u32, layer: usize) -> [f64; 3] {
        let index = id as usize;
        match (self.drawn_offsets.get(index).copied().flatten(), struts.get(index)) {
            (Some(offset), Some(strut)) => crate::adapter::offset_at(&offset, strut, layer),
            _ => [0.0; 3],
        }
    }

    /// Where `strut`'s own seat (`at_seat`) or base end is drawn, off its
    /// axis cell: its own drawn-line displacement where the pass started with
    /// it, and for a member added during the pass (a diagonal), the drawn
    /// line of the member that end is joined to.
    fn member_end_off(&self, struts: &[SweepStrut], strut: &SweepStrut, at_seat: bool) -> [f64; 3] {
        let layer = if at_seat { strut.k_top } else { strut.k_bottom };
        if (strut.id as usize) < self.drawn_offsets.len() {
            return self.drawn_off(struts, strut.id, layer);
        }
        let host = if at_seat {
            strut.seat_on_strut
        } else {
            match strut.landing {
                Landing::Strut { id, .. } => Some(id),
                Landing::Plate | Landing::Model { .. } => None,
            }
        };
        host.map_or([0.0; 3], |host| self.drawn_off(struts, host, layer))
    }

    /// [`free_shaft_floor`] of a member as it stands NOW. Read on demand
    /// rather than held per member, because a mast is pushed, extended and
    /// withdrawn inside the pass and its seat height moves with it.
    fn free_shaft_floor(&self, grid: &Grid, strut: &SweepStrut, layer_height_mm: f64) -> usize {
        free_shaft_floor(self.root_flare.as_ref(), grid, strut, layer_height_mm)
    }
}

/// One planned `|X|`: both diagonals, checked against the live field, not yet
/// built.
#[derive(Clone, Debug)]
struct CrossPlan {
    a: u32,
    b: u32,
    k: usize,
    k_lo: usize,
    a_top: usize,
    b_top: usize,
    radius_mm: f64,
    first: Vec<u32>,
    second: Vec<u32>,
    plane: Option<Plane>,
    /// The two diagonals' resin, `π r_b² (ℓ₁ + ℓ₂)`: what the brace window
    /// weighs against thickening.
    volume_mm3: f64,
    /// How the band got where it is (counted when it is built, not when it
    /// is planned: a planned band the window refuses is not a built one).
    widened: bool,
    moved: bool,
    moved_off_the_plate: bool,
}

/// Remedy 2: tie the run's member at layer `k` with ONE cross whose plane
/// holds the direction the run sways in.
///
/// A run measured in a sway direction takes a cross whose plane lies within
/// 45° of that direction; a run no cross ties yet takes a cross in any plane.
/// One cross holds only the arc of directions around its own plane, so the
/// member is measured again at once and comes back for a further plane (or
/// for the thicken rung, if no further plane is admissible). A cross that a
/// later remedy — a further plane, or the member's own thickening — leaves
/// holding nothing is taken out at the end of the pass
/// ([`prune_unneeded_crosses`]), so building one at a time never leaves a
/// cross that is not needed. Every partner in reach is tried, nearest first
/// (the stilt plate's arm 81: its nearest band crossed a leg, and arm 65,
/// 0.008 mm further away, was never tried).
///
/// **THE BRACE WINDOW** (R7 + D5 and D9 of `bracing-mechanics.md`, Craig
/// ruling 2026-10-03, interim pending the slenderness coupon). A cross that
/// passes every geometric test is still not built when
///
/// * the run is longer than the pair's TWO-POST CEILING
///   ([`pair_ceiling_ratio`]): however many bands two posts share, they bend
///   together as one beam, and past that length no band count holds them.
///   The partner is skipped (a farther or wider partner has a higher
///   ceiling: "brace to a wider frame"), and the pair is returned so the run
///   can be thickened into its window ([`thicken_into_the_window`]);
/// * thickening the run costs less resin than the crosses it would need,
///   charged to this run ([`cross_charge_mm3`]) against the resin the thicken
///   rung would add for it (the same plan the rung would commit,
///   [`plan_thicken`]). Where thickening is refused (past the maximum radius,
///   or a root lap) or switched off, there is nothing to compare and the
///   cross is built.
///
/// Neither is a tuned number: the ceiling is the two posts' composite
/// section, and the lower edge is where the two resins are equal for THIS
/// pair (the study's 1.5× for a shared cross and 2.5× for a cross one post
/// needs are what that comparison gives for its ideal geometry).
///
/// Returns `Ok(())` when a cross was built, or what the window refused.
fn tie_by_crosses(
    context: &SlendernessContext<'_>,
    state: &mut TieState,
    struts: &mut Vec<SweepStrut>,
    run: &UnbracedRun,
    k: usize,
    may_move: bool,
    outcome: &mut SlendernessOutcome,
) -> Result<(), CrossMiss> {
    let id = run.strut_id;
    let mut miss = CrossMiss::default();
    let Some(here) = struts.get(id as usize).map(|strut| strut.cell_at(k)) else {
        return Err(miss);
    };
    let ratio = run.length_mm / run.limit_mm;
    // The resin thickening this run would add, planned once and only when a
    // cross gets far enough to be compared with it; `None` where the rung is
    // off or refused.
    let mut thicken_mm3: Option<Option<f64>> = None;
    let candidates = partner_candidates(context, struts, id, k);
    for (rank, &(partner, cell)) in candidates.iter().enumerate() {
        let Some(plane) = plane_between(context.grid, here, cell) else {
            continue;
        };
        // The new plane is credited TOGETHER with the crosses already on the
        // member at this layer (`joint_holds`), not alone.
        if !new_plane_holds(&state.ties[id as usize], k, plane, run.sway_deg) {
            continue;
        }
        outcome.cross_bands_planned += 1;
        let Some(plan) = plan_cross(
            context, state, struts, id, partner, k, run, may_move, outcome,
        ) else {
            continue;
        };
        let mut shared = false;
        if context.brace_window {
            let partner_mm = struts
                .get(partner as usize)
                .and_then(|other| drawn_at(other, &state.profiles[partner as usize], plan.k))
                .unwrap_or(0.0);
            let gap_mm = crate::cell_distance_mm(context.grid, plan.a_top, plan.b_top);
            let ceiling = pair_ceiling_ratio(run.radius_mm, partner_mm, gap_mm);
            shared = partner_needs_the_tie(context, state, struts, partner, plan.k, plan.plane);
            let planes = if run.sway_deg.is_none() { 2 } else { 1 };
            if ratio > ceiling * (1.0 + RATIO_NOISE) {
                outcome.crosses_refused_past_pair_ceiling += 1;
                let past = PastCeiling {
                    gap_mm,
                    partner_mm,
                    ceiling,
                    cross_mm3: plan.volume_mm3,
                    shared,
                    planes,
                    frame: None,
                };
                if miss.past_ceiling.is_none_or(|known| past.ceiling > known.ceiling) {
                    miss.past_ceiling = Some(past);
                }
                continue;
            }
            let thicken_cost = *thicken_mm3.get_or_insert_with(|| {
                if !context.thicken {
                    return None;
                }
                let fits = |radius: f64| run.length_mm <= context.law.max_unbraced_mm(radius);
                match plan_thicken(context, state, struts, run, &fits) {
                    Some(Ok(plan)) => Some(plan.added_mm3),
                    _ => None,
                }
            });
            if let Some(thicken_cost) = thicken_cost {
                let charge = cross_charge_mm3(plan.volume_mm3, shared, planes, extra_ties(run).max(1));
                if charge > thicken_cost {
                    outcome.crosses_refused_thicken_cheaper += 1;
                    miss.thicken_cheaper = true;
                    continue;
                }
                if outcome.window_lowest_braced_ratio <= 0.0
                    || ratio < outcome.window_lowest_braced_ratio
                {
                    outcome.window_lowest_braced_ratio = ratio;
                }
            }
        }
        if shared {
            outcome.crosses_shared_by_both_posts += 1;
        }
        if rank > 0 {
            outcome.crosses_to_a_later_partner += 1;
        }
        if run.sway_deg.is_some() {
            outcome.crosses_further_plane += 1;
        } else {
            outcome.crosses_first_plane += 1;
        }
        commit_cross(context, state, struts, plan, outcome);
        outcome.braces_added += 1;
        return Ok(());
    }
    Err(miss)
}

/// **R6 + D4: tie the run with a SET of bands to one partner** (Craig ruling
/// 2026-10-03, interim pending the slenderness coupon;
/// `bracing-mechanics.md` revision 2, §3 and rule R6).
///
/// A band is not a fixed point: two posts and the bands between them sway
/// together as one frame. So, for each partner in reach (nearest first, the
/// plane holding the direction the run sways in, as [`tie_by_crosses`]):
///
/// 1. **Count** the bands through the pair's frame ([`frame_bands::count_bands`]):
///    the fewest that hold the run's top within one pixel, every band free to
///    sway, starting from the fixed-point count `⌈L / limit⌉ − 1` (a frame
///    never needs fewer). The bands are laid as they will be built: tops at
///    the run's evenly spaced tie layers, each as deep as the overhang law
///    needs for the gap, raised above both posts' root flares.
/// 2. **Past the ceiling** (no count that fits holds; D9): the pair is
///    skipped, and kept so the run can be thickened until the frame holds it
///    ([`thicken_into_the_window`]).
/// 3. **Plan every band** ([`plan_cross`], the same geometric and clearance
///    tests as one cross). If ANY band is refused, the set is refused and
///    the next partner is tried: a partial set leaves the frame short. Bands
///    whose posts lean apart are planned deeper than laid; the frame is then
///    solved again on the bands as planned and must still hold.
/// 4. **The window** (R7 + D5): the set's resin, charged to this run, against
///    thickening the run ([`cross_charge_mm3`], with `n` the set itself).
/// 5. **Build** every band of the set together; the prune judges the set as
///    one ([`prune_unneeded_crosses`]).
///
/// Returns `Ok(())` when a set was built, or what the window and the ceiling
/// refused.
fn tie_by_band_set(
    context: &SlendernessContext<'_>,
    state: &mut TieState,
    struts: &mut Vec<SweepStrut>,
    run: &UnbracedRun,
    k: usize,
    outcome: &mut SlendernessOutcome,
) -> Result<(), CrossMiss> {
    use crate::frame_bands::{count_bands, panel_drag, BandCount, Panel};
    let id = run.strut_id;
    let grid = context.grid;
    let h = context.layer_height_mm;
    let mut miss = CrossMiss::default();
    let Some(here) = struts.get(id as usize).map(|strut| strut.cell_at(k)) else {
        return Err(miss);
    };
    let ratio = run.length_mm / run.limit_mm;
    let fixed_point = extra_ties(run).max(1);
    let planes = if run.sway_deg.is_none() { 2 } else { 1 };
    let mut thicken_mm3: Option<Option<f64>> = None;
    let candidates = partner_candidates(context, struts, id, k);
    // THE PAIRS THE LEDGER OFF LEAVES UNCLASSIFIED (`frame_ledger`): those
    // with room for bands but no band set that can be planned at any count.
    // Such a pair can never be built on, so the only thing its class can
    // change is the pair a run is thickened into the window of (the past-
    // ceiling pair with the highest ceiling, the earliest of equals). They
    // are classified after the loop, highest ceiling first, until one is
    // past its ceiling, and only while one could still be that pair.
    let mut deferred: Vec<DeferredPair> = Vec::new();
    // The rank of the candidate `miss.past_ceiling` came from.
    let mut past_rank = usize::MAX;
    for (rank, &(partner, cell)) in candidates.iter().enumerate() {
        let Some(plane) = plane_between(grid, here, cell) else {
            continue;
        };
        if !new_plane_holds(&state.ties[id as usize], k, plane, run.sway_deg) {
            continue;
        }
        let gap_mm = crate::cell_distance_mm(grid, here, cell);
        let depth = context.lean.layers_to_reach(gap_mm);
        let Some(partner_mm) = partner_radius_over(state, struts, partner, run) else {
            continue;
        };
        if depth == 0 || gap_mm <= 0.0 {
            continue;
        }
        let floor = match (struts.get(id as usize), struts.get(partner as usize)) {
            (Some(one), Some(two)) => state
                .free_shaft_floor(grid, one, h)
                .max(state.free_shaft_floor(grid, two, h)),
            _ => continue,
        };
        let brace_mm = diagonal_radius_mm(context, gap_mm);
        let frame_past = |most_bands: usize| FramePast {
            partner,
            gap_mm,
            depth,
            brace_mm,
            floor,
            most_bands,
        };
        let cross_mm3 = core::f64::consts::PI
            * brace_mm
            * brace_mm
            * 2.0
            * gap_mm.hypot(layers_mm(depth, h));
        let ceiling = pair_ceiling_ratio(run.radius_mm, partner_mm, gap_mm);
        // Whether this pair became the one the run is thickened into the
        // window of (a strictly higher ceiling than the one known).
        let past_here = |miss: &mut CrossMiss, most_bands: usize| {
            let past = PastCeiling {
                gap_mm,
                partner_mm,
                ceiling,
                cross_mm3,
                shared: false,
                planes,
                frame: Some(frame_past(most_bands)),
            };
            let beats = miss.past_ceiling.is_none_or(|known| past.ceiling > known.ceiling);
            if beats {
                miss.past_ceiling = Some(past);
            }
            beats
        };
        let panel = Panel {
            post_radius_mm: run.radius_mm,
            partner_radius_mm: partner_mm,
            gap_mm,
            height_mm: run.length_mm,
            bands: Vec::new(),
        };
        let layout = |n: usize| {
            band_set_layers(run, n, depth, floor).map(|layers| panel_bands(run, &layers, brace_mm))
        };
        // D9's infinite-band ceiling first: no band count beats it, and it
        // costs no solve.
        if context.brace_window
            && ratio > pair_ceiling_ratio(run.radius_mm, partner_mm, gap_mm) * (1.0 + RATIO_NOISE)
        {
            outcome.crosses_refused_past_pair_ceiling += 1;
            let most = most_bands_that_fit(run, depth, floor);
            if most > 0 && past_here(&mut miss, most) {
                past_rank = rank;
            }
            continue;
        }
        if !context.frame_ledger {
            // No room for the bands at the fixed-point count: what the count
            // below would find at once, without a solve.
            if band_set_layers(run, fixed_point, depth, floor).is_none() {
                continue;
            }
            // A pair no band set can be planned on, at any count the ladder
            // below could try, is refused whatever the frame says of it: set
            // it aside. The counts it could try are the fixed-point count up
            // to the end of the run of counts that fit from there, or the
            // most that fit from one band, whichever is more (a superset of
            // `found`'s loop). The plans are tried on a throwaway outcome;
            // the pair is planned again, for real, only if one of them
            // stands.
            let mut tried = SlendernessOutcome::default();
            let from_here = (fixed_point..)
                .take_while(|&count| band_set_layers(run, count, depth, floor).is_some())
                .last()
                .unwrap_or(fixed_point);
            let most = most_bands_that_fit(run, depth, floor).max(from_here);
            let plannable = (fixed_point..=most)
                .filter_map(|count| band_set_layers(run, count, depth, floor))
                .any(|layers| {
                    plan_band_set(
                        context, state, struts, run, partner, plane, &layers, &mut tried,
                    )
                    .is_some()
                });
            if !plannable {
                deferred.push(DeferredPair {
                    rank,
                    partner,
                    gap_mm,
                    partner_mm,
                    depth,
                    floor,
                    brace_mm,
                    cross_mm3,
                    ceiling,
                });
                continue;
            }
        }
        let counted = match count_bands(&panel, context.law.ratio(), fixed_point, &layout) {
            BandCount::Holds { bands, .. } => bands,
            // Not even the fixed-point count of bands fits the run with this
            // pair's band depth (above both flares, below the run's top):
            // there is no room for a set, which is geometry, not a ceiling.
            BandCount::PastCeiling { most_bands: 0, .. } => {
                outcome.frame_pairs_without_room += 1;
                continue;
            }
            BandCount::PastCeiling { most_bands, .. } => {
                outcome.frame_pairs_past_ceiling += 1;
                if past_here(&mut miss, most_bands) {
                    past_rank = rank;
                }
                continue;
            }
        };
        // EVERY BAND OF THE SET, OR NONE. A band the field refuses where it
        // was laid is tried a little lower or higher, between its
        // neighbours ([`plan_band_set`]); the frame is then solved again on
        // the bands as planned and must still hold. Where it does not, or a
        // band finds no place, one more band is laid (closer spacing), up to
        // as many as fit.
        let holds = |plans: &[CrossPlan]| {
            let planned = Panel {
                bands: plans
                    .iter()
                    .map(|plan| crate::frame_bands::PanelBand {
                        bottom_mm: height_in_run(run, plan.k_lo),
                        top_mm: height_in_run(run, plan.k),
                        brace_radius_mm: plan.radius_mm,
                    })
                    .collect(),
                ..panel.clone()
            };
            panel_drag(&planned, context.law.ratio())
                .is_some_and(|drag| drag <= 1.0 + RATIO_NOISE)
        };
        let most = most_bands_that_fit(run, depth, floor).max(counted);
        let mut found: Option<Vec<CrossPlan>> = None;
        for count in counted..=most {
            let Some(layers) = band_set_layers(run, count, depth, floor) else {
                break;
            };
            let Some((set, moved)) =
                plan_band_set(context, state, struts, run, partner, plane, &layers, outcome)
            else {
                continue;
            };
            let as_laid = set.iter().zip(&layers).all(|(plan, &(bottom, top))| {
                plan.k_lo == bottom && plan.k == top && plan.radius_mm <= brace_mm
            });
            if as_laid || holds(&set) {
                if count > counted {
                    outcome.frame_sets_respaced += 1;
                }
                outcome.frame_bands_moved_from_their_layer += moved;
                found = Some(set);
                break;
            }
        }
        let Some(plans) = found else {
            outcome.frame_sets_refused_incomplete += 1;
            continue;
        };
        let bands = plans.len();
        let set_mm3: f64 = plans.iter().map(|plan| plan.volume_mm3).sum();
        let mut shared = false;
        if context.brace_window {
            shared = partner_needs_the_tie(context, state, struts, partner, plans[0].k, plans[0].plane);
            let thicken_cost = *thicken_mm3.get_or_insert_with(|| {
                if !context.thicken {
                    return None;
                }
                let fits = |radius: f64| run.length_mm <= context.law.max_unbraced_mm(radius);
                match plan_thicken(context, state, struts, run, &fits) {
                    Some(Ok(plan)) => Some(plan.added_mm3),
                    _ => None,
                }
            });
            if let Some(thicken_cost) = thicken_cost {
                if cross_charge_mm3(set_mm3, shared, planes, 1) > thicken_cost {
                    outcome.crosses_refused_thicken_cheaper += 1;
                    miss.thicken_cheaper = true;
                    continue;
                }
                if outcome.window_lowest_braced_ratio <= 0.0
                    || ratio < outcome.window_lowest_braced_ratio
                {
                    outcome.window_lowest_braced_ratio = ratio;
                }
            }
        }
        if shared {
            outcome.crosses_shared_by_both_posts += bands;
        }
        if rank > 0 {
            outcome.crosses_to_a_later_partner += bands;
        }
        if run.sway_deg.is_some() {
            outcome.crosses_further_plane += bands;
        } else {
            outcome.crosses_first_plane += bands;
        }
        let first = struts.len();
        for plan in plans {
            commit_cross(context, state, struts, plan, outcome);
        }
        state.band_sets.push((first, bands));
        outcome.braces_added += bands;
        outcome.frame_band_sets += 1;
        outcome.frame_bands_built += bands;
        outcome.frame_bands_beyond_fixed_point += bands.saturating_sub(fixed_point);
        outcome.frame_most_bands_in_a_set = outcome.frame_most_bands_in_a_set.max(bands);
        outcome.frame_highest_ratio_braced = outcome.frame_highest_ratio_braced.max(ratio);
        return Ok(());
    }
    // The pairs set aside: the highest ceiling first, the earliest of equals,
    // each only while it could still be the pair the run is thickened into
    // the window of; the first of them past its ceiling is that pair.
    //
    // The counts are classified a batch at a time, one pair a thread, because
    // each is a pure function of its pair: the batch that holds the first
    // pair past its ceiling finds the same pair in any thread count (the
    // scan below takes the first in the sorted order), and a batch's pairs
    // after it are the only work the early stop would have spared.
    deferred.sort_by(|a, b| b.ceiling.total_cmp(&a.ceiling).then(a.rank.cmp(&b.rank)));
    let ratio_s = context.law.ratio();
    let batch_len = rayon::current_num_threads().max(1);
    let mut at = 0;
    'scan: while at < deferred.len() {
        let batch: Vec<DeferredPair> = deferred[at..]
            .iter()
            .take(batch_len)
            .take_while(|pair| {
                // Every pair after one that cannot be the pair has a lower
                // ceiling, or an equal one and a later rank.
                miss.past_ceiling.is_none_or(|known| {
                    // (Ceilings are finite and positive: `total_cmp` orders
                    // them as `>` does.)
                    match pair.ceiling.total_cmp(&known.ceiling) {
                        core::cmp::Ordering::Greater => true,
                        core::cmp::Ordering::Equal => pair.rank < past_rank,
                        core::cmp::Ordering::Less => false,
                    }
                })
            })
            .copied()
            .collect();
        if batch.is_empty() {
            break;
        }
        let counted: Vec<BandCount> = batch
            .par_iter()
            .map(|pair| {
                let panel = Panel {
                    post_radius_mm: run.radius_mm,
                    partner_radius_mm: pair.partner_mm,
                    gap_mm: pair.gap_mm,
                    height_mm: run.length_mm,
                    bands: Vec::new(),
                };
                let layout = |n: usize| {
                    band_set_layers(run, n, pair.depth, pair.floor)
                        .map(|layers| panel_bands(run, &layers, pair.brace_mm))
                };
                count_bands(&panel, ratio_s, fixed_point, &layout)
            })
            .collect();
        for (pair, count) in batch.iter().zip(counted) {
            if let BandCount::PastCeiling { most_bands, .. } = count {
                if most_bands > 0 {
                    miss.past_ceiling = Some(PastCeiling {
                        gap_mm: pair.gap_mm,
                        partner_mm: pair.partner_mm,
                        ceiling: pair.ceiling,
                        cross_mm3: pair.cross_mm3,
                        shared: false,
                        planes,
                        frame: Some(FramePast {
                            partner: pair.partner,
                            gap_mm: pair.gap_mm,
                            depth: pair.depth,
                            brace_mm: pair.brace_mm,
                            floor: pair.floor,
                            most_bands,
                        }),
                    });
                    break 'scan;
                }
            }
        }
        if batch.len() < batch_len.min(deferred.len() - at) {
            // The batch ended where a pair could no longer be the one.
            break;
        }
        at += batch.len();
    }
    Err(miss)
}

/// A pair the brace frame set aside unclassified ([`tie_by_band_set`], the
/// ledger off): what it takes to count its bands and, if it is past its
/// ceiling, to name it as the pair a run is thickened into the window of.
#[derive(Clone, Copy)]
struct DeferredPair {
    rank: usize,
    partner: u32,
    gap_mm: f64,
    partner_mm: f64,
    depth: usize,
    floor: usize,
    brace_mm: f64,
    cross_mm3: f64,
    ceiling: f64,
}

/// Plan every band of a set laid on `layers` (ascending `(bottom, top)`),
/// between `run`'s member and `partner`, or `None` where a band finds no
/// place. Each band is planned at its laid top first; where the field refuses
/// it there (the model, a member, a drain on a diagonal's path, a post that
/// ends), at tops stepped half a band's depth at a time down and up from it,
/// nearest first, while it stays above the band planned below it and inside
/// the run. Planned with `may_move` false: the laid tops
/// already stand above both posts' flares. Each attempt is one
/// [`plan_cross`] and is counted as a band planned (and, if refused, under
/// its one reason).
#[expect(
    clippy::too_many_arguments,
    reason = "the set needs the field, the run, its partner and plane, the laid layers and the \
              outcome its attempts are counted in"
)]
fn plan_band_set(
    context: &SlendernessContext<'_>,
    state: &TieState,
    struts: &[SweepStrut],
    run: &UnbracedRun,
    partner: u32,
    plane: Plane,
    layers: &[(usize, usize)],
    outcome: &mut SlendernessOutcome,
) -> Option<(Vec<CrossPlan>, usize)> {
    let id = run.strut_id;
    let first_floor = layers.first().map_or(0, |&(bottom, _)| bottom);
    let mut plans: Vec<CrossPlan> = Vec::with_capacity(layers.len());
    let mut moved = 0_usize;
    for &(bottom, top) in layers {
        let depth = top - bottom;
        // The band's top stays where its bottom is above the band planned
        // below (or the first laid bottom, which is above the flares), and
        // inside the run. A band that must climb past its laid layer pushes
        // the ones above it up too (each starts above the one below), so a
        // set can close up above an obstacle; the frame then judges the
        // layout as built.
        let low = plans
            .last()
            .map_or(first_floor + depth, |below| below.k + depth + 1)
            .max(first_floor + depth);
        let high = run.k_top.saturating_sub(1);
        let laid = top;
        let top = top.max(low);
        let step = (depth / 2).max(1);
        let mut tops: Vec<usize> = Vec::new();
        if (low..=high).contains(&top) {
            tops.push(top);
        }
        for i in 1.. {
            let down = top.checked_sub(i * step).filter(|t| (low..=high).contains(t));
            let up = Some(top + i * step).filter(|t| (low..=high).contains(t));
            if down.is_none() && up.is_none() {
                break;
            }
            tops.extend(down);
            tops.extend(up);
        }
        let mut placed = false;
        for at in tops {
            if !new_plane_holds(&state.ties[id as usize], at, plane, run.sway_deg) {
                continue;
            }
            outcome.cross_bands_planned += 1;
            let Some(plan) = plan_cross(context, state, struts, id, partner, at, run, false, outcome)
            else {
                continue;
            };
            if plans.last().is_some_and(|below| plan.k_lo <= below.k) {
                continue;
            }
            if at != laid {
                moved += 1;
            }
            plans.push(plan);
            placed = true;
            break;
        }
        if !placed {
            return None;
        }
    }
    Some((plans, moved))
}

/// `layers × h` in mm.
fn layers_mm(layers: usize, layer_height_mm: f64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a layer count bounded by the plan's own layer count is exact in f64"
    )]
    let count = layers as f64;
    count * layer_height_mm
}

/// How high `layer` stands above the run's base, along the run (its chord
/// length per layer); `0` at or below the base.
fn height_in_run(run: &UnbracedRun, layer: usize) -> f64 {
    let span = run.k_top.saturating_sub(run.k_lo);
    if span == 0 {
        return 0.0;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "layer counts bounded by the plan's own layer count are exact in f64"
    )]
    let fraction = layer.saturating_sub(run.k_lo) as f64 / span as f64;
    run.length_mm * fraction
}

/// **The layers of a set of `n` bands on `run`**: tops at the run's evenly
/// spaced tie layers ([`tie_layer`], the layers the fixed-point ladder used),
/// each band `depth` layers deep, a band whose bottom would fall below
/// `floor` (a root flare or the raft) raised until it does not. `None` where
/// `n` bands do not fit: a top at or past the run's top, or a band reaching
/// down into the one below (one tie is never built twice).
fn band_set_layers(
    run: &UnbracedRun,
    n: usize,
    depth: usize,
    floor: usize,
) -> Option<Vec<(usize, usize)>> {
    let mut layers: Vec<(usize, usize)> = Vec::with_capacity(n);
    for step in 1..=n {
        let top = tie_layer(run, step, n)?.max(floor + depth);
        if top >= run.k_top {
            return None;
        }
        let bottom = top - depth;
        if layers.last().is_some_and(|&(_, below)| bottom <= below) {
            return None;
        }
        layers.push((bottom, top));
    }
    Some(layers)
}

/// The most bands [`band_set_layers`] can lay on `run` (no solve).
fn most_bands_that_fit(run: &UnbracedRun, depth: usize, floor: usize) -> usize {
    let mut n = 0;
    while band_set_layers(run, n + 1, depth, floor).is_some() {
        n += 1;
    }
    n
}

/// The panel bands for `layers`, every diagonal `brace_mm`.
fn panel_bands(
    run: &UnbracedRun,
    layers: &[(usize, usize)],
    brace_mm: f64,
) -> Vec<crate::frame_bands::PanelBand> {
    layers
        .iter()
        .map(|&(bottom, top)| crate::frame_bands::PanelBand {
            bottom_mm: height_in_run(run, bottom),
            top_mm: height_in_run(run, top),
            brace_radius_mm: brace_mm,
        })
        .collect()
}

/// The partner's drawn equivalent radius over the part of `run` it stands
/// beside, or `None` where it stands beside none of it.
fn partner_radius_over(
    state: &TieState,
    struts: &[SweepStrut],
    partner: u32,
    run: &UnbracedRun,
) -> Option<f64> {
    let index = partner as usize;
    let other = struts.get(index)?;
    let profile = state.profiles.get(index)?;
    let lo = run.k_lo.max(other.k_bottom);
    let top = run.k_top.min(other.k_top);
    (top > lo).then(|| run_radius(other, &other.thickened_mm, profile, lo, top))
}

/// Whether the FRAME holds `run` once its root is thickened to `radius_mm`,
/// against the pair `past` with the bands that fit (R6 + D9, for
/// [`thicken_into_the_window`]).
fn frame_holds_at(
    context: &SlendernessContext<'_>,
    state: &TieState,
    struts: &[SweepStrut],
    run: &UnbracedRun,
    past: FramePast,
    radius_mm: f64,
) -> bool {
    use crate::frame_bands::{count_bands, BandCount, Panel};
    let Some(partner_mm) = partner_radius_over(state, struts, past.partner, run) else {
        return false;
    };
    let thicker = UnbracedRun {
        radius_mm,
        limit_mm: context.law.max_unbraced_mm(radius_mm),
        ..*run
    };
    let panel = Panel {
        post_radius_mm: radius_mm,
        partner_radius_mm: partner_mm,
        gap_mm: past.gap_mm,
        height_mm: run.length_mm,
        bands: Vec::new(),
    };
    let layout = |n: usize| {
        band_set_layers(run, n, past.depth, past.floor)
            .map(|layers| panel_bands(run, &layers, past.brace_mm))
    };
    matches!(
        count_bands(&panel, context.law.ratio(), extra_ties(&thicker).max(1), &layout),
        BandCount::Holds { .. }
    )
}

/// Relative float noise on a comparison of two quantities a construction
/// makes equal: a run thickened INTO a pair's window by bisection lands
/// exactly on its ceiling, and a diagonal sized to its post exactly meets
/// it; neither must read as a hair past when measured again. Not a physical
/// tolerance.
const RATIO_NOISE: f64 = 1.0e-9;

/// What the brace window refused for one run, so the ladder can pick the
/// remedy the rules name for it.
#[derive(Clone, Copy, Debug, Default)]
struct CrossMiss {
    /// A cross was refused ONLY because thickening costs less resin.
    thicken_cheaper: bool,
    /// The pair with the HIGHEST ceiling among those the run is past: the one
    /// it can be thickened into the window of with the least resin.
    past_ceiling: Option<PastCeiling>,
}

impl CrossMiss {
    fn merge(&mut self, other: Self) {
        self.thicken_cheaper |= other.thicken_cheaper;
        if let Some(past) = other.past_ceiling {
            if self.past_ceiling.is_none_or(|known| past.ceiling > known.ceiling) {
                self.past_ceiling = Some(past);
            }
        }
    }
}

/// A pair a run is past the two-post ceiling of (D9), as measured when its
/// cross was refused.
#[derive(Clone, Copy, Debug)]
struct PastCeiling {
    /// The gap between the posts' axes at the band top, and the partner's
    /// drawn radius there.
    gap_mm: f64,
    partner_mm: f64,
    /// The ceiling, as `L / limit`, at the run's radius then.
    ceiling: f64,
    /// The refused cross's own resin, whether both posts needed it, and the
    /// planes the run needs: what bracing the pair would charge the run once
    /// it is inside the window.
    cross_mm3: f64,
    shared: bool,
    planes: usize,
    /// Under R6, the pair as the frame saw it, so the run can be thickened
    /// until the FRAME holds it with the bands it can carry, rather than to
    /// the infinite-band ceiling; `None` on the fixed-point ladder.
    frame: Option<FramePast>,
}

/// A pair the frame found past its ceiling (R6 + D9): what
/// [`thicken_into_the_window`] needs to ask the frame again at a thicker
/// root.
#[derive(Clone, Copy, Debug)]
struct FramePast {
    partner: u32,
    gap_mm: f64,
    depth: usize,
    brace_mm: f64,
    floor: usize,
    /// The most bands that fit the run: what bracing it at the ceiling takes.
    most_bands: usize,
}

/// **D9, the two-post ceiling**: the longest run, as a multiple of its own
/// limit `S · r_a`, that two posts can hold together with ANY number of
/// bands between them.
///
/// However many bands they share, two posts at most bend as ONE composite
/// beam: sections `I = π r⁴ / 4`, areas `A = π r²`, axes `s` apart, so
/// `I_c = I_a + I_b + (A_a A_b / (A_a + A_b)) s²`. Both carry the S-law's
/// drag, `2q` together, so the pair sways `2q L⁴ / (8 E I_c)` against the
/// post's own `q L⁴ / (8 E I_a)`, and the one-pixel length grows by
///
/// ```text
/// λ_ceiling = (I_c / (2 I_a))^(1/4)  =  (1 + s² / r²)^(1/4)  ≈  √(s / r)   (equal posts)
/// ```
///
/// (`bracing-mechanics.md` §3, "a two-post ceiling": 3.8 for its thin pair,
/// r 0.15 mm at 2.2 mm, and 3.5 for its mid pair, r 0.4 mm at 5 mm). This
/// is the INFINITE-band bound; with the bands the engine actually places the
/// study's frame holds 3.3-3.6, so this ceiling errs toward bracing. No
/// constant: `S`, `E` and `q` cancel.
#[must_use]
pub fn pair_ceiling_ratio(run_radius_mm: f64, partner_radius_mm: f64, gap_mm: f64) -> f64 {
    let (a, b) = (run_radius_mm, partner_radius_mm);
    if !(a.is_finite() && a > 0.0 && b.is_finite() && b > 0.0 && gap_mm.is_finite()) {
        return 1.0;
    }
    let (a2, b2) = (a * a, b * b);
    // `I / (π/4)` and `A / π`: the common factors cancel in the ratio.
    let composite = a2 * a2 + b2 * b2 + 4.0 * gap_mm * gap_mm * a2 * b2 / (a2 + b2);
    (composite / (2.0 * a2 * a2)).powf(0.25)
}

/// **R7 + D5**: the resin one cross charges the run it ties
/// (`bracing-mechanics.md` §5): `c · P · n` diagonals' worth, where one
/// diagonal is half the cross.
///
/// * `c` is 1 when the partner ALSO needs the tie (each post pays for one
///   diagonal) and 2 when only this run does;
/// * `P` is the planes the run needs: 2 for a run no cross holds yet (every
///   direction), 1 for a further plane;
/// * `n` is the bands the run needs, `⌈L / limit⌉ − 1` ([`extra_ties`]).
///
/// Compared with the resin the thicken rung would add for the same run; the
/// cross is built only where it is not dearer.
#[must_use]
pub fn cross_charge_mm3(cross_mm3: f64, shared: bool, planes: usize, bands: usize) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "plane and band counts are small integers, exact in f64"
    )]
    let count = (planes * bands) as f64;
    let per_post = if shared { 0.5 } else { 1.0 };
    cross_mm3 * per_post * count
}

/// The radius of the diagonal an `|X|` across `gap_mm` between two straight
/// posts needs: its band is the overhang law's fewest layers for that move
/// (`LeanLaw::layers_to_reach`), its length the hypotenuse, and its radius
/// the brace rule's ([`SlendernessLaw::brace_radius_mm`]) — what
/// [`plan_cross`] computes for posts that do not lean.
fn diagonal_radius_mm(context: &SlendernessContext<'_>, gap_mm: f64) -> f64 {
    let depth = context.lean.layers_to_reach(gap_mm);
    #[expect(
        clippy::cast_precision_loss,
        reason = "a layer count bounded by the plan's own layer count is exact in f64"
    )]
    let rise_mm = depth as f64 * context.layer_height_mm;
    context.law.brace_radius_mm(rise_mm.hypot(gap_mm))
}

/// **R2 on a finished field**: how many braces are drawn thicker than a post
/// at one of their joints — the host they seat on, at their top layer, or
/// the member they land on, at their bottom layer — on the drawn `profiles`
/// (`adapter::drawn_profiles`, slot by slot from each member's own foot).
/// One pass over the struts.
pub(crate) fn braces_thicker_than_a_post(struts: &[SweepStrut], profiles: &[Vec<f64>]) -> usize {
    let at = |id: u32, layer: usize| {
        let index = id as usize;
        let strut = struts.get(index)?;
        drawn_at(strut, profiles.get(index)?, layer)
    };
    struts
        .iter()
        .enumerate()
        .filter(|(_, strut)| strut.kind == StrutKind::Brace && live(strut))
        .filter(|&(index, strut)| {
            let own = profiles
                .get(index)
                .and_then(|profile| profile.iter().copied().reduce(f64::max))
                .unwrap_or(0.0)
                .max(strut.min_radius_mm);
            let seat = strut.seat_on_strut.and_then(|host| at(host, strut.k_top));
            let base = match strut.landing {
                Landing::Strut { id, .. } => at(id, strut.k_bottom),
                Landing::Plate | Landing::Model { .. } => None,
            };
            [seat, base]
                .into_iter()
                .flatten()
                .any(|post| own > post * (1.0 + RATIO_NOISE))
        })
        .count()
}

/// Whether `partner` itself needs a tie at `k` in `plane`: one of its runs is
/// over its limit, has `k` strictly inside it, and sways in a direction the
/// plane holds (or is measured in every direction). Then the cross serves
/// both posts and is charged half to each (`c = 1`).
fn partner_needs_the_tie(
    context: &SlendernessContext<'_>,
    state: &TieState,
    struts: &[SweepStrut],
    partner: u32,
    k: usize,
    plane: Option<Plane>,
) -> bool {
    let index = partner as usize;
    let Some(strut) = struts.get(index) else {
        return false;
    };
    member_runs(
        context.grid,
        strut,
        &state.ties[index],
        &state.profiles[index],
        context.layer_height_mm,
        context.law,
    )
    .iter()
    .any(|run| {
        run.over_limit()
            && run.k_lo < k
            && k < run.k_top
            && run
                .sway_deg
                .is_none_or(|sway| plane.is_none_or(|plane| holds(plane, sway)))
    })
}

/// Build a planned cross and bring every list the tie rungs read up to date.
fn commit_cross(
    context: &SlendernessContext<'_>,
    state: &mut TieState,
    struts: &mut Vec<SweepStrut>,
    plan: CrossPlan,
    outcome: &mut SlendernessOutcome,
) {
    let grid = context.grid;
    let span = plan.k - plan.k_lo;
    if plan.widened {
        outcome.brace_bands_deepened_for_their_diagonals += 1;
    }
    if plan.moved_off_the_plate {
        outcome.brace_bands_moved_up_off_the_plate += 1;
    } else if plan.moved {
        outcome.brace_bands_moved_above_flare += 1;
    }
    let (a, b, k, k_lo, radius, plane) = (
        plan.a,
        plan.b,
        plan.k,
        plan.k_lo,
        plan.radius_mm,
        plan.plane,
    );
    push_brace(
        grid,
        struts,
        plan.a_top,
        k,
        k_lo,
        Landing::Strut { id: b, layer: k_lo },
        a,
        radius,
        plan.first,
    );
    push_brace(
        grid,
        struts,
        plan.b_top,
        k,
        k_lo,
        Landing::Strut { id: a, layer: k_lo },
        b,
        radius,
        plan.second,
    );
    stamp_last_two(&mut state.index, struts, context.layer_height_mm);
    for post in [a, b] {
        if let Some(list) = state.ties.get_mut(post as usize) {
            list.push(Tie {
                layer: k,
                from: k,
                plane,
                band_top: true,
            });
            // The diagonal landing here holds only once its seat, at the
            // band top on the other post, has printed (tie timing).
            list.push(Tie {
                layer: k_lo,
                from: k,
                plane,
                band_top: false,
            });
        }
        // NO JUNCTION RAISE (R2, 2026-10-03): the diagonal is no thicker than
        // either post at its band (`plan_cross` refuses it otherwise), so the
        // post's drawn profile, and its root disc on the plate, stay as they
        // are. Until that date the post was raised to the diagonal's radius
        // from the joint down to its root, and every trunk under it, and its
        // plate disc grown to match (`adapter::drawn_profiles`).
    }
    for _ in 0..2 {
        state.ties.push(Vec::new());
        state.profiles.push(vec![radius; span + 1]);
        state.unthickened.push(vec![radius; span + 1]);
    }
    state
        .bands
        .entry((a.min(b), a.max(b)))
        .or_default()
        .push((k_lo, k));
}

fn flare_of(context: &SlendernessContext<'_>) -> f64 {
    if context.flare_factor.is_finite() && context.flare_factor > 0.0 {
        context.flare_factor
    } else {
        1.0
    }
}

/// Remedy 3: THICKEN — raise the member's root, keep its tip (owner ruling
/// 2026-09-29).
///
/// The drawn profile over the run becomes `max(drawn, taper)`, the taper
/// running from the drawn radius the run has at its top (kept) down to a root
/// `R` at the run's lower tie, and `R` from there to the member's own foot —
/// a member is never narrower below than above. `R` is the SMALLEST root that
/// brings the run inside the one-pixel law as the taper integral measures it
/// ([`UnbracedRun::radius_mm`]); it is found by bisection, because the law is
/// monotone in `R`, and nothing about it is a tuned number.
///
/// Admissible only when:
/// * `R` is within the registered maximum radius (1.0 mm); a run that the
///   maximum cannot hold goes to the mast;
/// * every trunk the member stands on is raised to `R` from the junction down
///   to its own root (the junction rule: a trunk is at least as thick as the
///   branch where it joins it), and
/// * where that chain meets the PLATE, the grown root's drawn boss foot makes
///   no new lap of another root's drawn boss foot — the feet the file prints,
///   from the lowering's own [`RootBoss`] (since 2026-10-02; until then the
///   census's crowding discs, `FLARE_R_FACTOR` × root), with the same
///   touch rule the mast search uses.
///
/// It holds in every direction, which is why the owner prefers it wherever
/// crosses cannot close a tie. The minimum is written to
/// [`SweepStrut::thickened_mm`], so the down-sweep that re-sizes the field
/// keeps it.
fn thicken(
    context: &SlendernessContext<'_>,
    state: &mut TieState,
    struts: &mut [SweepStrut],
    run: &UnbracedRun,
    outcome: &mut SlendernessOutcome,
) -> bool {
    let law = context.law;
    let fits = |radius: f64| run.length_mm <= law.max_unbraced_mm(radius);
    thicken_to(context, state, struts, run, &fits, outcome)
}

/// **D9: thicken INTO THE WINDOW of a pair the run is past the ceiling of**
/// (Craig ruling 2026-10-03, interim pending the slenderness coupon).
///
/// "Past the ceiling, two thin posts cannot hold each other. Thicken until
/// `λ` drops into the window and then brace" (`bracing-mechanics.md` §5).
/// The root is the smallest that brings the run's `L / (S · r_eq)` down to
/// the pair's ceiling at that `r_eq` ([`pair_ceiling_ratio`] falls as the run
/// thickens, more slowly than `λ` does, so the test is monotone and the same
/// bisection finds it); the next attempt on the member then braces the pair.
///
/// Applied only where it costs less resin than thickening the run all the
/// way, counting the crosses the pair will then need ([`cross_charge_mm3`] at
/// the band count the ceiling leaves), or where full thickening is refused.
/// A partial root that adds nothing is not applied (the loop must end).
fn thicken_into_the_window(
    context: &SlendernessContext<'_>,
    state: &mut TieState,
    struts: &mut [SweepStrut],
    run: &UnbracedRun,
    past: PastCeiling,
    outcome: &mut SlendernessOutcome,
) -> bool {
    if !context.brace_window {
        return false;
    }
    let law = context.law;
    // Under R6 the window is where the FRAME holds the run with the bands
    // that fit it; otherwise the infinite-band composite ceiling.
    let into = |radius: f64| match past.frame {
        Some(frame) => frame_holds_at(context, state, struts, run, frame, radius),
        None => {
            run.length_mm
                <= law.max_unbraced_mm(radius)
                    * pair_ceiling_ratio(radius, past.partner_mm, past.gap_mm)
        }
    };
    let Some(Ok(partial)) = plan_thicken(context, state, struts, run, &into) else {
        return false;
    };
    if partial.added_mm3 <= 0.0 {
        return false;
    }
    let full = |radius: f64| run.length_mm <= law.max_unbraced_mm(radius);
    if let Some(Ok(whole)) = plan_thicken(context, state, struts, run, &full) {
        // The bands the run needs once it stands at the pair's ceiling.
        let at_ceiling = UnbracedRun {
            length_mm: run.limit_mm * past.ceiling,
            ..*run
        };
        let bands = past
            .frame
            .map_or_else(|| extra_ties(&at_ceiling).max(1), |frame| frame.most_bands.max(1));
        let braces = cross_charge_mm3(past.cross_mm3, past.shared, past.planes, bands);
        if whole.added_mm3 <= partial.added_mm3 + braces {
            return false;
        }
    }
    commit_thicken(state, struts, &partial, outcome);
    outcome.members_thickened_into_the_window += 1;
    true
}

/// [`thicken`] to the smallest root at which `fits(r_eq)` holds, counting a
/// refusal by its reason.
fn thicken_to(
    context: &SlendernessContext<'_>,
    state: &mut TieState,
    struts: &mut [SweepStrut],
    run: &UnbracedRun,
    fits: &dyn Fn(f64) -> bool,
    outcome: &mut SlendernessOutcome,
) -> bool {
    match plan_thicken(context, state, struts, run, fits) {
        None => false,
        Some(Err(ThickenRefusal::Radius)) => {
            outcome.thicken_refused_radius += 1;
            false
        }
        Some(Err(ThickenRefusal::Disc)) => {
            outcome.thicken_refused_disc += 1;
            false
        }
        Some(Err(ThickenRefusal::Model)) => {
            outcome.thicken_refused_model += 1;
            false
        }
        Some(Ok(plan)) => {
            commit_thicken(state, struts, &plan, outcome);
            true
        }
    }
}

/// Why the thicken rung refused a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThickenRefusal {
    /// The root it needs is past the registered maximum radius.
    Radius,
    /// The grown plate root's drawn foot would make a new lap of another root.
    Disc,
    /// The raised solid, of the member or of a trunk raised with it, would
    /// enter the model's final clearance gap where it does not now.
    Model,
}

/// Whether raising the members of `plan` puts a raised stretch of any of them
/// inside the model's final gap where that stretch as drawn now stands clear
/// ([`ThickenRefusal::Model`]).
///
/// Each member's raised stretch, from its foot to the layer the plan raises
/// it to, is cut where the plan changes shape (a constant root below the
/// run, a linear taper over it), and walked SLOT BY SLOT (challenger round 2,
/// 2026-10-04): the frustum a slot has when the plan is committed (the larger
/// of the profile drawn now and the plan's minimum, at each of its two
/// layers) against the frustum drawn there now. A slot that clears now and
/// enters the gap committed refuses the plan; one that is inside the gap
/// already, or that the plan does not raise, does not. A wider section or an
/// existing violation elsewhere in the piece excuses nothing.
///
/// Challenger round 3 (2026-10-05), the two reads it named:
///
/// * **Each solid is excused its OWN contact zones, no longer ones.** A
///   member's seat zone is where that solid first stands `gap` clear of the
///   face its seat stands against, and on the model its landing zone is the
///   same at its landing face ([`crate::member_clearance::contact_zone_mm`],
///   the form the up-sweep's descent test uses), each at the solid's own
///   radius at that end (the committed solid's for the committed read, the
///   drawn one's for the drawn read). The read it replaces excused every
///   member `zone_factor · (r + gap)` at both ends: a 0.6 mm trunk under a
///   flat seat raised to 0.15 mm was exempt whole (0.837 mm), so its growth
///   toward a wall beside its foot was never read.
/// * **"Inside the gap already" is PROVED, never flagged.** The raster only
///   flags (its reach carries half a cell's diagonal, so a flagged slot can
///   be clear); a slot the raster flags committed is decided by the census's
///   own clearance predicate on the drawn frustum
///   ([`geom_kernel::NativeSurface::segment_frustum_clearance_classified`]),
///   once for the committed solid and once for the drawn one, all such slots
///   of the plan in ONE batch. The committed slot enters the gap unless the
///   predicate MEASURES it clear; the drawn slot is inside the gap already
///   only where the predicate reads it under the gap (a measured value, or an
///   upper bound under the gap). Without a model surface nothing can be
///   proved, so a slot the raster flags committed refuses the plan.
///
/// Challenger round 5 (2026-10-05): **an unknown face exempts nothing.**
/// Without a surface, or where the query names no face for an end, that
/// end's zone is ZERO ([`crate::member_clearance::contact_zone_mm`]), so the
/// raster reads the solid up to its contact and the flag reaches the refusal
/// above. The retired read gave the longest zone there, which exempted a
/// short trunk whole: nothing was flagged, and the plan was admitted before
/// the no-surface refusal could run. A member with an unknown face is never
/// proved inside the gap already either (its zero zone reads its own contact
/// as inside the gap, which proves nothing about the plan).
fn thickening_enters_the_gap(
    context: &SlendernessContext<'_>,
    state: &TieState,
    struts: &[SweepStrut],
    plan: &ThickenPlan,
) -> bool {
    use crate::member_clearance::{contact_faces, contact_zone_mm, inset_segment_intrusion, ContactFace};
    let (grid, history, h) = (context.grid, context.history, context.layer_height_mm);
    let gap = context.final_clearance_mm;
    let factor = context.lean.face_zone_factor();
    let lift = |point: [f64; 3]| [point[0], point[1], point[2] + context.plate_z_mm];
    // The members the plan raises, with their two ends on the raster's frame:
    // (position in the chain, strut index, raised up to layer, seat, base).
    type Raised = (usize, usize, usize, [f64; 3], [f64; 3]);
    // **THE FINAL GAP DOES NOT APPLY INSIDE A HIDDEN CAVITY** (Craig ruling
    // 2026-10-05, on his 2026-08-31 ruling that supports hidden in a cavity
    // are never removed or cleaned): a member whose whole axis stands in
    // enclosed air ([`crate::cavity::HiddenCavity`], whatever it lands on:
    // the plate cannot be, a model floor or another support can) is not
    // read, so a post on a cavity floor may grow within the gap of the
    // cavity it stands in. Not the scar flag ([`crate::SweepStrut::interior`]),
    // which also holds for a recess open to the outside. Outside a cavity
    // nothing changes. The path hit is `ThickenReads::hidden_members`.
    //
    // **THE PROPOSED SOLID, not the centreline** (challenger, 2026-10-05):
    // the member is asked with the radius the plan would draw it at (the
    // widest of the drawn profile and the plan's own, along its whole length),
    // so a raise that would carry the solid out of the hidden voxels (through
    // a thin wall, say, to an exterior face) is read as before.
    let hidden = &history.hidden;
    let proposed_hidden = |position: usize, index: usize, highest: usize| -> bool {
        let Some(strut) = struts.get(index) else {
            return false;
        };
        let upto = plan.upto(position, highest).min(strut.k_top + 1);
        let radius_at = |layer: usize| {
            let drawn = drawn_at(strut, &state.profiles[index], layer)
                .unwrap_or(0.0)
                .max(strut.min_radius_mm);
            if layer < upto {
                drawn.max(plan.wanted(position, layer))
            } else {
                drawn
            }
        };
        hidden.span_hidden(strut.k_bottom, strut.k_top, |layer| strut.cell_at(layer), radius_at)
    };
    let chain_hidden: Vec<bool> = plan
        .chain
        .iter()
        .enumerate()
        .map(|(position, &(index, highest))| proposed_hidden(position, index, highest))
        .collect();
    let mut reads = state.thicken_reads.get();
    reads.hidden_members += chain_hidden.iter().filter(|hidden| **hidden).count();
    state.thicken_reads.set(reads);
    let members: Vec<Raised> = plan
        .chain
        .iter()
        .enumerate()
        .filter_map(|(position, &(index, highest))| {
            let strut = struts.get(index)?;
            if chain_hidden[position] {
                return None;
            }
            let upto = plan.upto(position, highest).min(strut.k_top + 1);
            if strut.k_bottom == usize::MAX || strut.k_top <= strut.k_bottom || upto <= strut.k_bottom + 1 {
                return None;
            }
            let seat = crate::member_clearance::axis_point(grid, h, strut.cell, strut.k_top);
            let base = crate::member_clearance::axis_point(grid, h, strut.base_cell(), strut.k_bottom);
            Some((position, index, upto, seat, base))
        })
        .collect();
    // The face each member's seat, and each model landing, stands against:
    // one nearest-point batch for the plan.
    let (seat_faces, landing_faces): (Vec<Option<ContactFace>>, Vec<Option<ContactFace>>) =
        match context.model_surface {
            Some(surface) => {
                let mut points: Vec<[f64; 3]> = members.iter().map(|member| lift(member.3)).collect();
                points.extend(members.iter().map(|member| lift(member.4)));
                let mut faces = contact_faces(surface, &points);
                let landings = faces.split_off(members.len());
                let landings = landings
                    .into_iter()
                    .zip(&members)
                    .map(|(face, member)| {
                        face.filter(|_| matches!(struts[member.1].landing, Landing::Model { .. }))
                    })
                    .collect();
                (faces, landings)
            }
            None => (vec![None; members.len()], vec![None; members.len()]),
        };
    // Every slot the raster flags committed: (top, bottom) on the raster's
    // frame, then the committed and the drawn frustum, each as
    // (radius at top, radius at bottom, inset at top, inset at bottom).
    // A member whose seat face, or model landing face, is unknown: its zones
    // there are zero, and nothing about it can be proved inside the gap.
    let faces_known: Vec<bool> = members
        .iter()
        .enumerate()
        .map(|(slot_of, member)| {
            seat_faces[slot_of].is_some()
                && (!matches!(struts[member.1].landing, Landing::Model { .. })
                    || landing_faces[slot_of].is_some())
        })
        .collect();
    type Read = (f64, f64, f64, f64);
    // (top, bottom, committed read, drawn read, faces known).
    type Flagged = ([f64; 3], [f64; 3], Read, Read, bool);
    let mut flagged: Vec<Flagged> = Vec::new();
    for (slot_of, &(position, index, upto, seat, base)) in members.iter().enumerate() {
        let strut = &struts[index];
        let top_layer = upto - 1;
        // Piece boundaries, foot first.
        let mut cuts = vec![strut.k_bottom];
        if position == 0 && plan.k_lo > strut.k_bottom && plan.k_lo < top_layer {
            cuts.push(plan.k_lo);
        }
        if top_layer > *cuts.last().unwrap_or(&strut.k_bottom) {
            cuts.push(top_layer);
        }
        let span = (strut.k_top - strut.k_bottom) as f64;
        let at =|layer: usize| -> [f64; 3] {
            let t = (layer - strut.k_bottom) as f64 / span;
            std::array::from_fn(|axis| base[axis] + t * (seat[axis] - base[axis]))
        };
        // The radius drawn now at a layer, and the radius it is drawn at if
        // the plan is committed (the plan only raises, and only below `upto`).
        let current = |layer: usize| {
            drawn_at(strut, &state.profiles[index], layer)
                .unwrap_or(0.0)
                .max(strut.min_radius_mm)
        };
        let committed = |layer: usize| {
            if layer < upto {
                current(layer).max(plan.wanted(position, layer))
            } else {
                current(layer)
            }
        };
        // Only a slot the plan really raises can be its doing.
        let grows = |layer: usize| committed(layer) > current(layer) + 1.0e-12;
        // A solid's radius at an end: the end layer's, or the slot below it
        // where the profile stops one short of the seat.
        let end_radius = |radius: &dyn Fn(usize) -> f64, layer: usize| {
            let here = radius(layer);
            if here > 0.0 || layer == 0 {
                here
            } else {
                radius(layer - 1)
            }
        };
        let length = geom_kernel::vec3::dist(seat, base);
        if !(length.is_finite() && length > 0.0) {
            continue;
        }
        let down = geom_kernel::vec3::scale(geom_kernel::vec3::sub(base, seat), 1.0 / length);
        let up = geom_kernel::vec3::scale(down, -1.0);
        let on_model = matches!(strut.landing, Landing::Model { .. });
        // A solid's radius `s` mm along its axis from its seat end
        // (`from_seat`) or its landing end: the wider of the two layers the
        // point stands between.
        let radius_along = |radius: &dyn Fn(usize) -> f64, s: f64, from_seat: bool| -> f64 {
            let steps = (s / length).clamp(0.0, 1.0) * span;
            let layer = if from_seat {
                strut.k_top as f64 - steps
            } else {
                strut.k_bottom as f64 + steps
            };
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a layer between the member's own two end layers, floored and ceiled"
            )]
            let (below, above) = (layer.floor() as usize, layer.ceil() as usize);
            let at = |k: usize| {
                if k >= strut.k_top {
                    end_radius(radius, strut.k_top)
                } else {
                    radius(k)
                }
            };
            at(below).max(at(above))
        };
        // A solid's own zone at one end: where ITS surface first stands `gap`
        // clear of that end's face plane. A tapered solid is wider at the
        // zone's edge than at its end, so the zone is read at the radius the
        // solid has at its edge, to a fixed point (a few rounds: the zone only
        // grows, and never past the longest the lean law admits).
        let own_zone = |face: Option<ContactFace>,
                        end: [f64; 3],
                        into: [f64; 3],
                        radius: &dyn Fn(usize) -> f64,
                        from_seat: bool|
         -> f64 {
            let mut zone = contact_zone_mm(face, end, into, radius_along(radius, 0.0, from_seat), gap, factor);
            for _ in 0..8 {
                let wider = contact_zone_mm(face, end, into, radius_along(radius, zone, from_seat), gap, factor);
                if wider <= zone + 1.0e-12 {
                    break;
                }
                zone = wider;
            }
            zone
        };
        // Each solid's own zones: (seat zone, landing zone).
        let zones = |radius: &dyn Fn(usize) -> f64| -> (f64, f64) {
            let seat_zone = own_zone(seat_faces[slot_of], lift(seat), down, radius, true);
            let landing_zone = if on_model {
                own_zone(landing_faces[slot_of], lift(base), up, radius, false)
            } else {
                0.0
            };
            (seat_zone, landing_zone)
        };
        let committed_zones = zones(&committed);
        let drawn_zones = zones(&current);
        // The zones run from the member's own ends, not the slot's.
        let insets = |layers: (usize, usize), zones: (f64, f64)| -> (f64, f64) {
            let (top, bottom) = (at(layers.1), at(layers.0));
            (
                (zones.0 - geom_kernel::vec3::dist(seat, top)).max(0.0),
                (zones.1 - geom_kernel::vec3::dist(bottom, base)).max(0.0),
            )
        };
        let raster_flags = |layers: (usize, usize), radii: (f64, f64)| {
            let (inset_top, inset_bottom) = insets(layers, committed_zones);
            inset_segment_intrusion(
                grid,
                history,
                h,
                at(layers.1),
                at(layers.0),
                inset_top,
                inset_bottom,
                radii,
                gap,
            )
            .is_some()
        };
        for pair in cuts.windows(2) {
            let (low, high) = (pair[0], pair[1]);
            if !(low..high).any(|layer| grows(layer) || grows(layer + 1)) {
                continue;
            }
            // One raster read of the whole piece at its widest committed
            // radius, outside the committed solid's own zones, bounds every
            // slot of it: a piece that clears it clears the plan.
            let widest = (low..=high).map(committed).fold(0.0, f64::max);
            if !raster_flags((low, high), (widest, widest)) {
                continue;
            }
            for layer in low..high {
                if !(grows(layer) || grows(layer + 1)) {
                    continue;
                }
                let wanted = (committed(layer + 1), committed(layer));
                if !raster_flags((layer, layer + 1), wanted) {
                    continue;
                }
                let (wanted_top, wanted_bottom) = insets((layer, layer + 1), committed_zones);
                let (drawn_top, drawn_bottom) = insets((layer, layer + 1), drawn_zones);
                flagged.push((
                    at(layer + 1),
                    at(layer),
                    (wanted.0, wanted.1, wanted_top, wanted_bottom),
                    (current(layer + 1), current(layer), drawn_top, drawn_bottom),
                    faces_known[slot_of],
                ));
            }
        }
    }
    if flagged.is_empty() {
        return false;
    }
    let mut reads = state.thicken_reads.get();
    reads.flagged += flagged.len();
    state.thicken_reads.set(reads);
    // Without a surface the flag cannot be confirmed, and no slot can be
    // proved inside the gap already: the raise is refused.
    let Some(surface) = context.model_surface else {
        return true;
    };
    // ONE batch: every flagged slot's committed frustum, then its drawn one.
    let rows = flagged.len();
    let mut p0s: Vec<[f64; 3]> = Vec::with_capacity(2 * rows);
    let mut p1s: Vec<[f64; 3]> = Vec::with_capacity(2 * rows);
    let (mut r0s, mut r1s, mut i0s, mut i1s) = (
        Vec::with_capacity(2 * rows),
        Vec::with_capacity(2 * rows),
        Vec::with_capacity(2 * rows),
        Vec::with_capacity(2 * rows),
    );
    for drawn in [false, true] {
        for row in &flagged {
            let (r_top, r_bottom, inset_top, inset_bottom) = if drawn { row.3 } else { row.2 };
            p0s.push(lift(row.0));
            p1s.push(lift(row.1));
            r0s.push(r_top);
            r1s.push(r_bottom);
            i0s.push(inset_top);
            i1s.push(inset_bottom);
        }
    }
    let Ok(verdicts) = surface.segment_frustum_clearance_classified(&p0s, &p1s, &r0s, &r1s, 0.0, &i0s, &i1s)
    else {
        return true;
    };
    let mut enters = false;
    for (row, slot) in flagged.iter().enumerate() {
        // **A REFUSAL NEEDS PROOF** (lane cylinder-bracing, rule A,
        // 2026-10-05). The committed slot clears where the predicate
        // MEASURES it clear, and is undecided where it reads an upper bound
        // at or above the gap: that bound says the true clearance is AT MOST
        // the number (the kernel could not decide a near-parallel face), so
        // it proves nothing about entering. The census counts a span only
        // where a read is under the gap, so the refusal may not be stricter
        // than the measurement it protects. Only a read UNDER the gap, a
        // measured value or an upper bound, proves the raise enters it (an
        // undecided read is counted in `ThickenReads::undecided`).
        let undecided = matches!(
            verdicts.get(row),
            Some(geom_kernel::SegmentClearanceClassified::UpperBound { clearance_mm, .. })
                if *clearance_mm >= gap - THICKEN_CLEARANCE_SLACK_MM
        );
        let committed_clears = undecided
            || matches!(
                verdicts.get(row),
                Some(geom_kernel::SegmentClearanceClassified::Measured { clearance_mm, .. })
                    if *clearance_mm >= gap - THICKEN_CLEARANCE_SLACK_MM
            );
        reads.undecided += usize::from(undecided);
        // An unknown face's zero zone reads the contact itself as inside the
        // gap: that proves nothing, so such a member is never excused.
        let drawn_inside_already = slot.4
            && matches!(
                verdicts.get(rows + row),
                Some(
                    geom_kernel::SegmentClearanceClassified::Measured { clearance_mm, .. }
                        | geom_kernel::SegmentClearanceClassified::UpperBound { clearance_mm, .. }
                ) if *clearance_mm < gap - THICKEN_CLEARANCE_SLACK_MM
            );
        reads.measured_clear += usize::from(committed_clears && !undecided);
        reads.inside_already += usize::from(!committed_clears && drawn_inside_already);
        enters |= !committed_clears && !drawn_inside_already;
    }
    state.thicken_reads.set(reads);
    enters
}

/// Float noise on a clearance compared against the final gap: the kernel's
/// distance at a zone's own edge, where the solid stands exactly `gap` from
/// its face by construction. NUMERICAL, not physical.
const THICKEN_CLEARANCE_SLACK_MM: f64 = 1.0e-9;

/// One thickening, planned against the live field and not yet applied
/// ([`commit_thicken`] applies it). Planning changes nothing, so the brace
/// window can weigh it against a cross.
#[derive(Clone, Debug)]
struct ThickenPlan {
    k_lo: usize,
    k_top: usize,
    /// The root found, and the tip radius the taper keeps, over `steps`.
    root: f64,
    top: f64,
    steps: f64,
    /// The member, then every trunk it stands on: (member index, the highest
    /// layer at which it must be at least `root` thick).
    chain: Vec<(usize, usize)>,
    /// The plate disc to grow, and to what.
    disc_growth: Option<(usize, f64)>,
    over_existing_lap: bool,
    admitted_on_drawn_feet: bool,
    /// The resin the thickening adds over the profiles as drawn now, mm³.
    added_mm3: f64,
}

impl ThickenPlan {
    /// The minimum radius the plan writes at `layer` of the chain member at
    /// `position` (the member itself is `0`).
    fn wanted(&self, position: usize, layer: usize) -> f64 {
        if position == 0 && layer >= self.k_lo {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a slot index bounded by the layer count is exact in f64"
            )]
            let t = ((layer - self.k_lo) as f64) / self.steps;
            self.root * (1.0 - t) + self.top * t
        } else {
            self.root
        }
    }

    /// The layer (exclusive) up to which the plan writes the chain member at
    /// `position`, from that member's own foot.
    fn upto(&self, position: usize, highest: usize) -> usize {
        if position == 0 {
            self.k_top
        } else {
            highest + 1
        }
    }
}

/// Plan [`thicken`]'s root for `run` against the live field: the smallest
/// root at which `fits(r_eq)` holds, the trunks it raises, and whether its
/// plate root may grow. `None` where the run has no layer or no member;
/// `Some(Err(..))` for a refusal. Changes nothing and counts nothing.
fn plan_thicken(
    context: &SlendernessContext<'_>,
    state: &TieState,
    struts: &[SweepStrut],
    run: &UnbracedRun,
    fits: &dyn Fn(f64) -> bool,
) -> Option<Result<ThickenPlan, ThickenRefusal>> {
    let law = context.law;
    let grid = context.grid;
    let member = run.strut_id as usize;
    let n = run.k_top.saturating_sub(run.k_lo);
    if n == 0 {
        return None;
    }
    let (k_lo, k_top) = (run.k_lo, run.k_top);
    #[expect(
        clippy::cast_precision_loss,
        reason = "layer counts bounded by the plan's own layer count are exact in f64"
    )]
    let steps = n.saturating_sub(1).max(1) as f64;
    // The drawn radius over the run, bottom first, and the top it keeps.
    let (drawn, top) = {
        let strut = struts.get(member)?;
        let profile = &state.profiles[member];
        let drawn: Vec<Option<f64>> = (0..n).map(|j| drawn_at(strut, profile, k_lo + j)).collect();
        let top = drawn
            .last()
            .copied()
            .flatten()
            .unwrap_or_else(|| strut.tip_radius_mm());
        (drawn, top)
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "a slot index bounded by the layer count is exact in f64"
    )]
    // KEEP THE TIP: the run's top layer (`j = n − 1`, where `top` was read)
    // is written exactly `top`, and the bottom (`j = 0`) exactly `root`. The
    // weights are `1 − t` and `t`, so both ends are exact in floating point.
    // Until 2026-10-01 the taper reached `top` at `j = n`, one layer ABOVE
    // the run, and wrote the tip layer `root + (top − root)·(n−1)/n`: the tip
    // came out thicker than drawn (overhang table, X 12, S 30: arm 19's tip
    // 0.38969 → 0.39087 mm). A one-layer run has no tip to keep apart from
    // its root, and takes the root, as before.
    let taper = |root: f64, j: usize| {
        let t = (j as f64) / steps;
        root * (1.0 - t) + top * t
    };
    let holds = |root: f64| {
        equivalent_radius(n, |j| {
            let wanted = taper(root, j);
            Some(drawn[j].map_or(wanted, |here| here.max(wanted)))
        })
        .is_some_and(fits)
    };
    if !holds(law.r_max_mm) {
        return Some(Err(ThickenRefusal::Radius));
    }
    let (mut low, mut high) = (top.min(law.r_max_mm), law.r_max_mm);
    for _ in 0..64 {
        let middle = 0.5 * (low + high);
        if holds(middle) {
            high = middle;
        } else {
            low = middle;
        }
    }
    let root = high;

    // The member, then every trunk it stands on, down to the one that meets
    // the plate or the model: (member index, the highest layer at which it
    // must be at least `root` thick).
    let mut chain: Vec<(usize, usize)> = vec![(member, k_lo)];
    let mut at = struts[member].landing;
    for _ in 0..struts.len() {
        let Landing::Strut { id, layer } = at else {
            break;
        };
        let host = id as usize;
        let Some(trunk) = struts.get(host) else {
            break;
        };
        if chain.iter().any(|(known, _)| *known == host) {
            break;
        }
        chain.push((host, layer));
        at = trunk.landing;
    }
    let foot = chain.last().map_or(member, |(index, _)| *index);
    let mut over_existing_lap = false;
    let mut admitted_on_drawn_feet = false;
    let disc_growth = {
        let strut = &struts[foot];
        if matches!(strut.landing, Landing::Plate) && strut.kind != StrutKind::Brace {
            let current = drawn_at(strut, &state.profiles[foot], strut.k_bottom).unwrap_or(0.0);
            if root > current {
                let flared = root * flare_of(context);
                let slot = crate::congestion::disc_slot(&state.placed_roots, strut.id);
                let known = slot
                    .and_then(|slot| crate::congestion::disc_radius_mm(&state.placed_roots, slot));
                let grows_past_own = known.is_none_or(|known| flared > known);
                // REFUSED ONLY FOR A LAP THE GROWTH MAKES (orchestrator ruling
                // 2026-09-29). Until then any growth of a root that already
                // lapped a neighbour was refused, so the stilt plate's post 81
                // (lapping by 0.051 mm before anything grew) took a mast
                // instead. A lap that stood before the growth is the
                // congestion lane's published warning, not a veto here.
                if grows_past_own {
                    // THE LAP IS MEASURED ON THE FEET THE FILE DRAWS
                    // (fix/mast-tie-and-overlap-factor, 2026-10-02): every
                    // root, the grown one and its neighbours, at the
                    // lowering's own boss foot (`RootBoss::foot_radius_mm`,
                    // the root times the owner's Heavy ratio 2.2 / 1.5; Craig
                    // ruling 2026-09-30), not at the census's crowding factor
                    // `FLARE_R_FACTOR` = 1.6 the placed discs are stored at.
                    // Until this date a thickening was refused for a lap no
                    // file prints (the lonely peak's arm 1 against post 0:
                    // 0.001 mm at 1.6, clear by 0.15 mm at the drawn foot).
                    // A root too short for a boss draws no foot wider than
                    // itself, so the boss foot is the WIDEST the lowering can
                    // draw under a root: the test can only err toward a
                    // refusal. The disc stored after the growth stays at the
                    // crowding factor, which the census and the mast search
                    // still read (`SweepPolicy::flare_r_factor`).
                    let census = || {
                        crate::congestion::growth_laps(
                            grid,
                            strut.base_cell(),
                            known.unwrap_or(0.0),
                            flared,
                            &state.placed_roots,
                            slot,
                        )
                    };
                    let (new_lap, existing) = match state.root_boss {
                        Some(boss) => {
                            let own_bare = slot
                                .and_then(|slot| state.placed_roots.get(slot))
                                .map_or(0.0, crate::congestion::RootDisc::bare_radius_mm);
                            let drawn = crate::congestion::growth_laps_drawn(
                                grid,
                                strut.base_cell(),
                                own_bare,
                                root,
                                &state.placed_roots,
                                slot,
                                |bare| boss.foot_radius_mm(bare),
                            );
                            admitted_on_drawn_feet = !drawn.0 && census().0;
                            drawn
                        }
                        None => census(),
                    };
                    if new_lap {
                        return Some(Err(ThickenRefusal::Disc));
                    }
                    over_existing_lap = existing;
                }
                slot.map(|slot| (slot, flared))
            } else {
                None
            }
        } else {
            None
        }
    };

    let mut plan = ThickenPlan {
        k_lo,
        k_top,
        root,
        top,
        steps,
        chain,
        disc_growth,
        over_existing_lap,
        admitted_on_drawn_feet,
        added_mm3: 0.0,
    };
    // **THICKENING MAY NOT PUSH A MEMBER INTO THE FINAL GAP** (R2 of
    // `clearance-census-diagnosis.md`, 2026-10-04). The root it needs was
    // found against the unbraced-length law alone; the overhang table's post
    // 30 stood 0.325 mm from a leg and was raised to 0.770 mm, 0.374 mm inside
    // it. A member, or a trunk raised with it, whose raised solid enters the
    // gap where its solid as drawn now does not, refuses the rung; the run
    // goes on to the cross, the mast, the model tie, then the warning.
    if context.member_clearance && thickening_enters_the_gap(context, state, struts, &plan) {
        return Some(Err(ThickenRefusal::Model));
    }
    // THE RESIN IT ADDS: on every layer it raises, `π (wanted² − drawn²)`
    // times that member's own length per layer (its chord, as `run_length`
    // measures it). One pass over the layers the commit would write.
    let height = context.layer_height_mm;
    let mut added = 0.0;
    for (position, &(index, highest)) in plan.chain.iter().enumerate() {
        let strut = &struts[index];
        let per_layer_mm = if strut.k_top > strut.k_bottom {
            run_length(grid, strut, strut.k_bottom, strut.k_bottom + 1, height)
        } else {
            height
        };
        for layer in strut.k_bottom..plan.upto(position, highest) {
            let wanted = plan.wanted(position, layer);
            let now = drawn_at(strut, &state.profiles[index], layer).unwrap_or(0.0);
            if wanted > now {
                added += core::f64::consts::PI * (wanted * wanted - now * now) * per_layer_mm;
            }
        }
    }
    plan.added_mm3 = added;
    Some(Ok(plan))
}

/// Apply a [`ThickenPlan`]: the minimum radius is written to each chain
/// member's [`SweepStrut::thickened_mm`] (the down-sweep keeps it) and to its
/// live drawn profile, and the plate disc grows.
fn commit_thicken(
    state: &mut TieState,
    struts: &mut [SweepStrut],
    plan: &ThickenPlan,
    outcome: &mut SlendernessOutcome,
) {
    for (position, &(index, highest)) in plan.chain.iter().enumerate() {
        let strut = &mut struts[index];
        let k_bottom = strut.k_bottom;
        let upto = plan.upto(position, highest);
        if strut.thickened_mm.len() < upto {
            strut.thickened_mm.resize(upto, 0.0);
        }
        let mut raised = false;
        for layer in k_bottom..upto {
            let wanted = plan.wanted(position, layer);
            let entry = &mut strut.thickened_mm[layer];
            *entry = entry.max(wanted);
            if let Some(drawn_here) = state.profiles[index].get_mut(layer - k_bottom) {
                if *drawn_here < wanted {
                    *drawn_here = wanted;
                    raised = true;
                }
            }
        }
        if position > 0 && raised {
            outcome.trunks_raised_under_thickening += 1;
        }
    }
    if let Some((slot, flared)) = plan.disc_growth {
        crate::congestion::grow_disc(&mut state.placed_roots, slot, flared, plan.root);
    }
    if plan.admitted_on_drawn_feet {
        outcome.thicken_admitted_where_the_crowding_disc_laps += 1;
    }
    if plan.over_existing_lap {
        outcome.thicken_over_existing_lap += 1;
    }
    outcome.members_thickened += 1;
}

/// How far an arm's top sways, in PIXELS, when a mast `gap_mm` away is joined
/// to it at `tie_mm` above the run's lower tie by one braced `|X|` panel — the
/// in-plane frame of `braces-masts.md` Q4 (`braces-masts-probe/frame.ts`,
/// ported term for term).
///
/// Both posts are clamped at the run's base; the panel makes them sway
/// together at the tie AND resists their rotation there through a couple, an
/// axial push-pull of the two posts `d` apart with stiffness
/// `k_c = d² / [H (1/A_a + 1/A_m) + ℓ / (2 A_b sin² θ)]`, `ℓ = d sec θ`. Every
/// member carries the same uniform lateral load, and `E` and `q` cancel into
/// `S`, so the answer is in pixels: `1.0` is the one-pixel law. The brace is
/// sized by the lane's own brace rule; `θ` is the saturated overhang angle.
/// The mast is taken as tall as the arm's tie, which is exact for a plate
/// arm's first run and optimistic above a lower tie (a longer mast is softer);
/// stated rather than hidden.
fn panel_sway_over_pixel(
    law: &SlendernessLaw,
    run_mm: f64,
    arm_mm: f64,
    tie_mm: f64,
    mast_mm: f64,
    gap_mm: f64,
) -> f64 {
    use core::f64::consts::PI;
    if !(run_mm > 0.0 && arm_mm > 0.0 && tie_mm > 0.0 && mast_mm > 0.0 && gap_mm > 0.0) {
        return f64::INFINITY;
    }
    let cos_t = 1.0 / law.secant_sat;
    let sin_sq = (1.0 - cos_t * cos_t).max(0.0);
    let diagonal = gap_mm * law.secant_sat;
    let brace = law.brace_radius_mm(diagonal);
    let arm_i = PI * arm_mm.powi(4) / 4.0;
    let mast_i = PI * mast_mm.powi(4) / 4.0;
    let (arm_a, mast_a, brace_a) = (
        PI * arm_mm * arm_mm,
        PI * mast_mm * mast_mm,
        PI * brace * brace,
    );
    let h = tie_mm.min(run_mm);
    let both_i = arm_i + mast_i;
    let couple =
        gap_mm * gap_mm / (h * (1.0 / arm_a + 1.0 / mast_a) + diagonal / (2.0 * brace_a * sin_sq));
    let k11 = 12.0 * both_i / h.powi(3);
    let k12 = -6.0 * both_i / (h * h);
    let k22 = 4.0 * both_i / h + couple;
    let upper = run_mm - h;
    // Fixed-end loads of both lower segments at the tie, plus the arm's
    // upper segment as a cantilever load.
    let force = h + upper;
    let moment = -(h * h) / 6.0 + upper * upper / 2.0;
    // The determinant of the symmetric 2x2 stiffness `[[k11, k12], [k12, k22]]`.
    let det = k11 * k22 - k12.powi(2);
    let sway = (force * k22 - k12 * moment) / det;
    let rotation = (k11 * moment - k12 * force) / det;
    let top = sway + rotation * upper + upper.powi(4) / (8.0 * arm_i);
    top * 2.0 * PI / law.ratio.powi(4)
}

/// Remedy 4: a mast, tied to the arm by a cross whose plane holds the
/// direction the run sways in.
///
/// * **One mast per arm where it will do**: an arm's own mast is extended up to
///   the new tie before a second one is dropped (the Cylinder's ladders of up
///   to six masts on one arm).
/// * **A cell is admissible only where the braced panel holds**: the arm's
///   top sway with the panel built must be within one pixel
///   ([`panel_sway_over_pixel`]). A cell that fails at the mast's own
///   slenderness radius may still pass with a thicker mast, up to the maximum
///   radius; the smallest that passes is used. Among admissible cells, the
///   nearest wins. Until 2026-09-29 the nearest cell whose disc merely cleared
///   won, so every mast stood at the minimum clearance (0.18 mm on the
///   Cylinder) — a second post, not a brace.
///
/// Returns `None` when no mast is admissible; `Some(Some(id))` for a new mast.
/// Remedy 4 at layer `k`, and, where its band had to dip into a root flare,
/// AGAIN WITH THE TIE MOVED UP (fix/brace-not-in-flare, 2026-09-30).
///
/// A new mast is only as tall as the layer it is planned at, so its band
/// cannot move up inside [`plan_cross`] the way a band between two standing
/// posts does: it is dropped, and the shortfall is recorded. The mast is then
/// planned afresh at the raised layer, where every test of the mast search
/// (cell, radius, panel, keep-out, clearance) is made at the new height. Each
/// retry raises the layer strictly, and the tie must stay inside the run, so
/// the loop ends.
fn tie_by_mast_above_the_flares(
    context: &SlendernessContext<'_>,
    state: &mut TieState,
    struts: &mut Vec<SweepStrut>,
    run: &UnbracedRun,
    k: usize,
    outcome: &mut SlendernessOutcome,
) -> Option<Option<u32>> {
    let mut at = k;
    loop {
        state.flare_shortfall.set(None);
        if let Some(done) = tie_by_mast(context, state, struts, run, at, outcome) {
            if at > k {
                outcome.masts_raised_above_flare += 1;
            }
            return Some(done);
        }
        let raise = state.flare_shortfall.take()?;
        let next = at + raise.max(1);
        if next >= run.k_top {
            return None;
        }
        at = next;
    }
}

fn tie_by_mast(
    context: &SlendernessContext<'_>,
    state: &mut TieState,
    struts: &mut Vec<SweepStrut>,
    run: &UnbracedRun,
    k: usize,
    outcome: &mut SlendernessOutcome,
) -> Option<Option<u32>> {
    let grid = context.grid;
    let law = context.law;
    let id = run.strut_id;
    let here = struts.get(id as usize)?.cell_at(k);
    let arm_ties = state.ties.get(id as usize).cloned().unwrap_or_default();
    let admissible = |plane: Option<Plane>| {
        plane.is_some_and(|plane| new_plane_holds(&arm_ties, k, plane, run.sway_deg))
    };
    // A MAST IS A PARTNER, NOT A SECOND, STIFFER POST (fix/member-clearance,
    // 2026-09-29): it is never drawn thicker than the member it braces. The
    // brace pair's mast was 0.7786 mm against the 0.3883 mm arm it tied —
    // 16x its bending stiffness — because the nearest cell won and its radius
    // was then grown until the panel held.
    let member_cap = state
        .profiles
        .get(id as usize)
        .map_or(0.0, |profile| profile.iter().copied().fold(0.0, f64::max))
        .max(struts.get(id as usize).map_or(0.0, |arm| arm.min_radius_mm));
    let h = context.layer_height_mm;
    let column_clear = |state: &TieState,
                        struts: &[SweepStrut],
                        cell: usize,
                        radius: f64,
                        mast: Option<u32>,
                        outcome: &mut SlendernessOutcome|
     -> bool {
        if !context.member_clearance {
            return true;
        }
        let bottom = crate::member_clearance::axis_point(grid, h, cell, 0);
        let top = crate::member_clearance::axis_point(grid, h, cell, k);
        if crate::member_clearance::model_intrusion(
            grid,
            context.history,
            h,
            bottom,
            top,
            radius,
            context.final_clearance_mm,
        )
        .is_some()
        {
            outcome.mast_cells_refused_by_model += 1;
            return false;
        }
        let posts: Vec<u32> = mast.into_iter().collect();
        if !member_solids_clear(context, state, struts, bottom, top, radius, &posts, outcome) {
            outcome.mast_cells_refused_by_member += 1;
            return false;
        }
        true
    };

    // R2 FOR A MAST'S CROSS (2026-10-03): the diagonals may be no thicker
    // than the mast, so a mast is sized at least as thick as the diagonal its
    // cell needs. The mast is the one post this rung sizes itself; the arm
    // is not grown, and a cell whose diagonal is thicker than the arm at the
    // tie is refused (`mast_cells_refused_thicker_than_member`).
    let arm_at_tie = struts
        .get(id as usize)
        .and_then(|arm| drawn_at(arm, &state.profiles[id as usize], k))
        .unwrap_or(0.0);
    let diagonal_for = |gap_mm: f64| {
        if context.brace_cap {
            diagonal_radius_mm(context, gap_mm)
        } else {
            0.0
        }
    };

    // The arm's own mast, extended (and, under R2, widened to its diagonal).
    let own: Vec<u32> = state.masts_of.get(&id).cloned().unwrap_or_default();
    for mast in own {
        let Some(standing) = struts.get(mast as usize) else {
            continue;
        };
        if !admissible(plane_between(grid, here, standing.cell)) {
            continue;
        }
        let (old_top, cell, old_radius) = (standing.k_top, standing.cell, standing.min_radius_mm);
        let needed = diagonal_for(crate::cell_distance_mm(grid, here, cell));
        if needed > arm_at_tie {
            outcome.mast_cells_refused_thicker_than_member += 1;
            continue;
        }
        let spans_before = state.index.shaft_spans.len();
        // A taller mast needs the radius its whole new height asks for, and its
        // grown root has to clear every other root, as a new mast's would.
        #[expect(
            clippy::cast_precision_loss,
            reason = "a layer index bounded by the plan's own layer count is exact in f64"
        )]
        let grown = law
            .brace_radius_mm(k as f64 * context.layer_height_mm)
            .max(old_radius)
            .max(needed);
        let (extend, widen) = (old_top < k, grown > old_radius);
        let slot = crate::congestion::disc_slot(&state.placed_roots, mast);
        if extend || widen {
            let blocked = extend
                && (old_top + 1..=k).any(|layer| {
                    !context.keep_out.admits(cell, layer)
                        || state.index.shaft_blocks(cell, layer, None)
                        || context.history.model_blocks(cell, layer)
                });
            let crowded = widen
                && context.mast_root_clearance
                && !crate::congestion::clear_of_placed(
                    grid,
                    cell,
                    grown * flare_of(context),
                    &state.placed_roots,
                    slot,
                );
            if blocked || crowded {
                continue;
            }
            if grown > member_cap {
                outcome.mast_cells_refused_thicker_than_member += 1;
                continue;
            }
            if !column_clear(state, struts, cell, grown, Some(mast), outcome) {
                continue;
            }
            if extend {
                state.index.stamp_shaft(mast, old_top + 1, k, cell, &[]);
                struts[mast as usize].k_top = k;
            }
            // The band reads the mast at the radius it will stand at.
            struts[mast as usize].min_radius_mm = grown;
        }
        if let Some(plan) = plan_cross(context, state, struts, id, mast, k, run, true, outcome) {
            if extend || widen {
                for profiles in [&mut state.profiles, &mut state.unthickened] {
                    let profile = &mut profiles[mast as usize];
                    if profile.len() < k + 1 {
                        profile.resize(k + 1, grown);
                    }
                    for radius in profile.iter_mut() {
                        *radius = radius.max(grown);
                    }
                }
                if let Some(slot) = slot {
                    crate::congestion::grow_disc(
                        &mut state.placed_roots,
                        slot,
                        grown * flare_of(context),
                        grown,
                    );
                }
                if extend {
                    outcome.masts_extended += 1;
                }
            }
            commit_cross(context, state, struts, plan, outcome);
            outcome.braces_added += 1;
            return Some(None);
        }
        struts[mast as usize].min_radius_mm = old_radius;
        if extend {
            state.index.unstamp_straight_shaft(cell, spans_before);
            struts[mast as usize].k_top = old_top;
        }
    }

    // A new mast.
    let reach_mm = law.thickest_brace_reach_mm();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a finite non-negative length divided by the grid pitch, floored to a bound"
    )]
    let reach_cells = (reach_mm / grid.pitch_mm()).floor().max(0.0) as usize;
    if reach_cells == 0 || run.k_top <= run.k_lo {
        return None;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "layer counts bounded by the plan's own layer count are exact in f64"
    )]
    let tie_mm = run.length_mm * ((k - run.k_lo) as f64) / ((run.k_top - run.k_lo) as f64);
    #[expect(
        clippy::cast_precision_loss,
        reason = "a layer index bounded by the plan's own layer count is exact in f64"
    )]
    let mast_height_mm = k as f64 * context.layer_height_mm;
    let cols = grid.cols();
    let row0 = grid.row_of(here);
    let col0 = grid.col_of(here);
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
            // A mast may not stand in the model's own footprint, nor on a
            // shaft, nor in a plane the run is not measured across.
            if context.history.column_max_model[cell] != 0 {
                continue;
            }
            if state.index.shaft_head[cell] != u32::MAX {
                continue;
            }
            if !admissible(plane_between(grid, here, cell)) {
                continue;
            }
            let depth = context.lean.layers_to_reach(distance_mm);
            if depth == 0 || depth > k {
                continue;
            }
            // THE MAST'S OWN RADIUS: `r_m = max(r_min, H / S)` over its WHOLE
            // height. Until 2026-09-29 it was sized up to the bottom of its
            // first band, but the cross that ties it holds only along its own
            // plane, so across that plane the mast stands unbraced from the
            // plate to its top (measured on the Cylinder: 16.05 mm against a
            // 15.6 mm limit, re-opened by the very next pass). Under R2
            // (2026-10-03) it is also at least as thick as the diagonal that
            // ties it, and a cell whose diagonal would be thicker than the arm
            // is refused. The diagonal grows with the distance, so a farther
            // cell never needs a thinner mast and the scan below may still
            // stop at the first cell that holds at its own radius.
            let needed = diagonal_for(distance_mm);
            if needed > arm_at_tie {
                outcome.mast_cells_refused_thicker_than_member += 1;
                continue;
            }
            candidates.push((
                distance_mm,
                cell,
                law.brace_radius_mm(mast_height_mm).max(needed),
            ));
        }
    }
    candidates.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let flare = flare_of(context);
    // THE CELL THAT NEEDS THE SMALLEST MAST WINS, the nearest among equals
    // (fix/member-clearance, 2026-09-29). The panel's couple grows with the
    // gap, so a farther cell can hold the arm with a much thinner mast
    // (brace pair: 1.868 mm needs r 0.779, 6 mm needs 0.396); until then the
    // nearest admissible cell won and its mast was grown until it held. No
    // mast may be thicker than the member it braces (`member_cap`). The scan
    // stops at the first cell that holds at the mast's own slenderness radius,
    // which no cell can beat.
    let chosen = {
        let plate = crate::congestion::PlacedIndex::build(&state.placed_roots, None);
        let mut best: Option<(usize, f64)> = None;
        let mut nearest_radius: Option<f64> = None;
        for (distance_mm, cell, own_radius) in candidates {
            if best.is_some_and(|(_, radius)| radius <= own_radius) {
                break;
            }
            let sway = |mast_mm: f64| {
                panel_sway_over_pixel(
                    law,
                    run.length_mm,
                    run.radius_mm,
                    tie_mm,
                    mast_mm,
                    distance_mm,
                )
            };
            if sway(law.r_max_mm) > 1.0 {
                outcome.mast_cells_refused_by_panel += 1;
                continue;
            }
            let ceiling = best.map_or(member_cap.min(law.r_max_mm), |(_, radius)| radius);
            if own_radius > ceiling || sway(ceiling) > 1.0 {
                if best.is_none() {
                    outcome.mast_cells_refused_thicker_than_member += 1;
                }
                continue;
            }
            let radius = if sway(own_radius) <= 1.0 {
                own_radius
            } else {
                let (mut low, mut high) = (own_radius, ceiling);
                for _ in 0..64 {
                    let middle = 0.5 * (low + high);
                    if sway(middle) <= 1.0 {
                        high = middle;
                    } else {
                        low = middle;
                    }
                }
                high
            };
            if best.is_some_and(|(_, known)| radius >= known) {
                continue;
            }
            if context.mast_root_clearance && !plate.clear(grid, cell, radius * flare) {
                continue;
            }
            if (0..=k).any(|layer| !context.keep_out.admits(cell, layer)) {
                continue;
            }
            if !column_clear(state, struts, cell, radius, None, outcome) {
                continue;
            }
            if nearest_radius.is_none() {
                nearest_radius = Some(radius);
            }
            best = Some((cell, radius));
            if radius <= own_radius {
                break;
            }
        }
        if let (Some((_, radius)), Some(nearest)) = (best, nearest_radius) {
            if radius < nearest {
                outcome.masts_moved_to_a_thinner_cell += 1;
            }
        }
        best
    };
    // NO ADMISSIBLE CELL MEANS THE MAST IS REFUSED AND COUNTED, never placed
    // overlapping or too close to hold. The run comes back as a warning.
    let Some((cell, radius_mm)) = chosen else {
        outcome.mast_cells_refused += 1;
        return None;
    };
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
        thickened_mm: Vec::new(),
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
        head: None,
    });
    state.ties.push(Vec::new());
    state.profiles.push(vec![radius_mm; k + 1]);
    state.unthickened.push(vec![radius_mm; k + 1]);
    // Stamped last, so the index is exactly what a fresh `stamp_all` would
    // build; taken back out if the cross will not build.
    let spans_before = state.index.shaft_spans.len();
    state.index.stamp_shaft(mast_id, 0, k, cell, &[]);
    if let Some(plan) = plan_cross(context, state, struts, id, mast_id, k, run, true, outcome) {
        commit_cross(context, state, struts, plan, outcome);
        // The mast's disc at the root it is DRAWN with once its cross is on
        // it (the cross can raise it past `radius_mm`), not the radius its
        // cell was chosen against.
        let drawn_root = state
            .profiles
            .get(mast_id as usize)
            .map_or(radius_mm, |profile| {
                plinth_support::drawn_root_radius_mm(profile, radius_mm)
            });
        if drawn_root > radius_mm {
            outcome.cross_roots_grown += 1;
            if !crate::congestion::clear_of_placed(
                grid,
                cell,
                drawn_root * flare,
                &state.placed_roots,
                None,
            ) {
                outcome.cross_roots_lapping += 1;
            }
        }
        crate::congestion::push_disc(
            &mut state.placed_roots,
            grid,
            mast_id,
            cell,
            drawn_root * flare,
            drawn_root,
            false,
        );
        state.masts_of.entry(id).or_default().push(mast_id);
        outcome.masts_dropped += 1;
        outcome.braces_added += 1;
        return Some(Some(mast_id));
    }
    // The cross would not build. EMIT AND WARN: the mast is withdrawn rather
    // than left standing with nothing tied to it, and the run is reported.
    state.index.unstamp_straight_shaft(cell, spans_before);
    struts.pop();
    state.ties.pop();
    state.profiles.pop();
    state.unthickened.pop();
    outcome.mast_cells_refused += 1;
    None
}

/// Keep every trunk under a forced merge SIZED for the load the merge just
/// handed it, until the down-sweep re-sizes the field.
///
/// A merge moves load, and the members below the junction grow: the down-sweep
/// adds the child's load to its trunk from the junction downward
/// ([`crate::down_sweep`]'s `combine_carried`) and sizes each layer by the area
/// law. Until it runs, the trunk's ledger still describes the thinner member it
/// was, and everything this lane decides after the merge loop reads that
/// ledger: the unbraced-run census, and the ROOT DISCS a mast must clear. A mast
/// placed against a root that then grows laps it — measured 2026-09-28 on
/// `make_laid_tube`, where refusing daisy chains concentrated merges onto
/// frozen trunks and left four masts 0.001-0.015 mm inside a root that grew
/// after sizing.
///
/// So the load is handed down the chain now, with the down-sweep's own sizing
/// rule: `r = clamp(sqrt(L / (X_ALLOWED·π)), r_min, r_max)`, never below the
/// member's own `min_radius_mm`. Adding the child's whole root load to every
/// layer below the junction is an UPPER bound on what `combine_carried` adds
/// (it takes a max before it sums), so the estimate errs thick — the direction
/// that keeps a mast clear. The down-sweep that runs after the lane overwrites
/// all of it. `O(chain · span)`, one pass per merge.
fn hand_load_down(
    context: &SlendernessContext<'_>,
    struts: &mut [SweepStrut],
    landing: Landing,
    handed_mm2: f64,
) {
    if !(handed_mm2.is_finite() && handed_mm2 > 0.0) {
        return;
    }
    let law = context.law;
    let x_allowed = context.x_allowed;
    if !(x_allowed.is_finite() && x_allowed > 0.0) {
        return;
    }
    let mut at = landing;
    for _ in 0..struts.len() {
        let Landing::Strut { id, layer } = at else {
            return;
        };
        let Some(trunk) = struts.get_mut(id as usize) else {
            return;
        };
        let Some(top_slot) = layer.checked_sub(trunk.k_bottom) else {
            return;
        };
        let slots = (top_slot + 1)
            .min(trunk.carried_mm2.len())
            .min(trunk.radius_mm.len());
        for slot in 0..slots {
            trunk.carried_mm2[slot] += handed_mm2;
            let radius = (trunk.carried_mm2[slot] / x_allowed / core::f64::consts::PI)
                .sqrt()
                .clamp(law.r_min_mm, law.r_max_mm)
                .max(trunk.min_radius_mm);
            trunk.radius_mm[slot] = trunk.radius_mm[slot].max(radius);
        }
        at = trunk.landing;
    }
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

/// **The layer a MAST ties `run` at** (fix/mast-tie-and-overlap-factor,
/// 2026-10-02; `v11-feedback-diagnosis.md` item 4).
///
/// The run needs `n = ceil(L / limit) − 1` ties, which cut it into pieces
/// inside the limit. A mast's braced panel is judged on the arm's TOP sway,
/// and the arm above the tie stands there as a free cantilever, so the mast
/// takes the HIGHEST of those ties, the one whose piece above is the run's
/// top piece: `n / (n + 1)` of the way up. If that top piece is still over its
/// OWN limit (its own equivalent radius, thinner than the run's where the arm
/// tapers to its tip), the tie rises layer by layer until it is not. The
/// pieces below are tied by the next attempts on the same member (the mast's
/// own extension path, or any rung), each again measured on its own.
///
/// Until this date the mast took the crosses' FIRST evenly spaced layer,
/// `1 / (n + 1)` of the way up, and left `n / (n + 1)` of the run standing
/// above it: on a run over twice its limit the free cantilever alone was
/// `(2/3 · L / limit)⁴ > 3` pixels, so the panel refused every cell at every
/// mast radius (the lonely peak: 35,172 cells, 3 masts refused). A run under
/// twice its limit needs one tie, where the first and the highest are the
/// same layer, so nothing changes there unless its top piece is over its
/// own limit.
///
/// Measured with the production run measure ([`run_length`], [`run_radius`]);
/// one equivalent-radius integral per layer tried, and the first layer tried
/// is usually the answer. `None` where the run is too short to hold a tie or
/// no layer inside it leaves a piece above within its limit.
fn mast_tie_layer(
    grid: &Grid,
    strut: &SweepStrut,
    profile: &[f64],
    run: &UnbracedRun,
    layer_height_mm: f64,
    law: &SlendernessLaw,
) -> Option<MastTie> {
    let wanted = extra_ties(run).max(1);
    let highest = tie_layer(run, wanted, wanted)?;
    let layer = (highest..run.k_top).find(|&k| {
        let piece_mm = run_length(grid, strut, k, run.k_top, layer_height_mm);
        let radius_mm = run_radius(strut, &strut.thickened_mm, profile, k, run.k_top);
        piece_mm <= law.max_unbraced_mm(radius_mm)
    })?;
    Some(MastTie {
        layer,
        raised: layer > highest,
    })
}

/// Where [`mast_tie_layer`] planned a mast's tie, and whether it had to rise
/// above the highest evenly spaced tie for the piece above to fit its own
/// limit (a path-hit witness).
#[derive(Clone, Copy, Debug)]
struct MastTie {
    layer: usize,
    raised: bool,
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

pub(crate) fn stamp_all(index: &mut ColumnHistory, struts: &[SweepStrut], layer_height_mm: f64) {
    for strut in struts {
        if strut.k_bottom == usize::MAX || strut.k_top < strut.k_bottom {
            continue;
        }
        index.stamp_strut(strut, layer_height_mm);
    }
}

fn stamp_last_two(index: &mut ColumnHistory, struts: &[SweepStrut], layer_height_mm: f64) {
    stamp_tail(index, struts, 2, layer_height_mm);
}

fn stamp_tail(
    index: &mut ColumnHistory,
    struts: &[SweepStrut],
    count: usize,
    layer_height_mm: f64,
) {
    let from = struts.len().saturating_sub(count);
    for strut in &struts[from..] {
        if strut.k_bottom == usize::MAX || strut.k_top < strut.k_bottom {
            continue;
        }
        index.stamp_strut(strut, layer_height_mm);
    }
}

/// Remedy 1: a shaft landing this arm can reach INSIDE its own limit.
///
/// The search is the one the up-sweep descent already does, with one extra
/// admissibility test: a candidate whose descent would leave a run longer than
/// `limit_mm` is not admissible, however cheap it is. Straight down past the
/// limit is therefore not admissible either — it is simply not in this
/// candidate set.
///
/// **The index is a snapshot and the field is not.** The index was stamped
/// before the merge loop began, and the loop has moved arms since, so every
/// answer it gives is checked against the parent AS IT STANDS NOW before it is
/// accepted; a stale answer is refused and counted in `stale`.
fn merge_candidate(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    struts: &[SweepStrut],
    id: u32,
    limit_mm: f64,
    stale: &mut usize,
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
            // THE PARENT MUST STAND HERE NOW, not merely have stood here when
            // the index was stamped.
            if !struts
                .get(parent as usize)
                .is_some_and(|live| crate::landing_check::stands_at(live, candidate, at))
            {
                *stale += 1;
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
///
/// The head-join rule ([`crate::head_join`]) re-lands a member with this same
/// test, so a join moved down a trunk is held to the refusals a remedy-1 merge
/// is.
pub(crate) fn segment_path(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    seat_cell: usize,
    k_top: usize,
    base_cell: usize,
    k_bottom: usize,
    except: Option<u32>,
) -> Option<Vec<u32>> {
    segment_path_excluding(
        context,
        index,
        seat_cell,
        k_top,
        base_cell,
        k_bottom,
        except.as_slice(),
    )
}

/// [`segment_path`] with several members the path may pass through: the one
/// it lands on, and (for a member being RE-landed) its own shaft as the index
/// still has it and the shafts of the members that move with it
/// (`lip_seat.rs`, 2026-09-29). `segment_path` is this with at most one.
pub(crate) fn segment_path_excluding(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    seat_cell: usize,
    k_top: usize,
    base_cell: usize,
    k_bottom: usize,
    except: &[u32],
) -> Option<Vec<u32>> {
    segment_path_or_block(context, index, seat_cell, k_top, base_cell, k_bottom, except).ok()
}

/// Why [`segment_path_or_block`] found no path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PathBlock {
    /// No layer to draw on (the segment does not rise).
    NoRise,
    Drain,
    Model,
    Member,
}

/// [`segment_path_excluding`], naming the refusal.
pub(crate) fn segment_path_or_block(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    seat_cell: usize,
    k_top: usize,
    base_cell: usize,
    k_bottom: usize,
    except: &[u32],
) -> Result<Vec<u32>, PathBlock> {
    segment_path_sized(context, index, seat_cell, k_top, base_cell, k_bottom, except, None)
}

/// [`segment_path_or_block`] for a member whose drawn radius is KNOWN: with
/// `Some(radius)` the drain test is the keep-out's own capsule at that radius
/// ([`DrainKeepOut::blocks_member`]) rather than at the registered maximum arm
/// radius the keep-out had to assume before anything was sized. `None` is
/// exactly [`segment_path_or_block`].
#[expect(
    clippy::too_many_arguments,
    reason = "the segment's two ends, the index, the members it may pass and the member's own \
          radius; a struct would move the list rather than shorten it"
)]
pub(crate) fn segment_path_sized(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    seat_cell: usize,
    k_top: usize,
    base_cell: usize,
    k_bottom: usize,
    except: &[u32],
    member_radius_mm: Option<f64>,
) -> Result<Vec<u32>, PathBlock> {
    let grid = context.grid;
    let cols = grid.cols();
    let span = k_top.checked_sub(k_bottom).ok_or(PathBlock::NoRise)?;
    if span == 0 {
        return Err(PathBlock::NoRise);
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
        let drained = match member_radius_mm {
            None => !context.keep_out.admits(cell, here),
            Some(radius) => context.keep_out.blocks_member(grid, cell, here, radius),
        };
        if drained {
            return Err(PathBlock::Drain);
        }
        if slot > 0 {
            if context.history.model_blocks(cell, here) {
                return Err(PathBlock::Model);
            }
            if index.shaft_blocks_excluding(cell, here, except) {
                return Err(PathBlock::Member);
            }
        }
        path.push(u32::try_from(cell).unwrap_or(u32::MAX));
    }
    Ok(path)
}

/// Every shaft, other than this arm's own, that stands at layer `k` within the
/// thickest brace's reach, with the cell its axis occupies there, NEAREST FIRST
/// (ties broken by the lower cell index, then the lower id, so the order does
/// not depend on the scan). R3 of the field report: a partner is any
/// admissible shaft in reach, not only the nearest one.
///
/// **One pass over the members, not over the grid.** The reach box of the
/// thickest brace is 42 mm across on the production law, about 2.8 million
/// cells at a 0.05 mm pitch, and the partner search runs once per attempted
/// tie; a member standing at layer `k` occupies exactly `cell_at(k)`, which is
/// the span the index would have been asked for, so reading the members
/// directly is the same answer at `O(struts)`. It also names every member at
/// a shared cell, where the cell scan named only the lowest id.
fn partner_candidates(
    context: &SlendernessContext<'_>,
    struts: &[SweepStrut],
    id: u32,
    k: usize,
) -> Vec<(u32, usize)> {
    let grid = context.grid;
    let Some(strut) = struts.get(id as usize) else {
        return Vec::new();
    };
    let here = strut.cell_at(k);
    let reach_mm = context.law.thickest_brace_reach_mm();
    let mut ordered: Vec<(f64, usize, u32)> = struts
        .iter()
        .filter(|other| {
            other.id != id
                // A brace to a member of an `|X|` would tie a brace to a
                // brace, which holds nothing: only shafts are partners. A
                // model tie (D8) is a short member to the model, not a shaft.
                && other.kind != StrutKind::Brace
                && other.kind != StrutKind::Tie
                && other.k_bottom != usize::MAX
                && other.k_bottom <= k
                && k <= other.k_top
        })
        .filter_map(|other| {
            let cell = other.cell_at(k);
            let distance_mm = crate::cell_distance_mm(grid, here, cell);
            (distance_mm > 0.0 && distance_mm <= reach_mm).then_some((distance_mm, cell, other.id))
        })
        .collect();
    ordered.sort_unstable_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    ordered
        .into_iter()
        .map(|(_, cell, member)| (member, cell))
        .collect()
}

/// Plan the two crossing diagonals of an `|X|` between `a` and `b` with its
/// top at layer `k`, against the live field, or refuse. Nothing is built here
/// ([`commit_cross`] builds).
///
/// Two diagonals over ONE height band: one seated on `A` at the top of the band
/// and landing on `B` at its bottom, and its twin seated on `B` at the top and
/// landing on `A` at the bottom. They cross in the middle and NOTHING IS
/// HORIZONTAL: the band is `LeanLaw::layers_to_reach(d)` layers deep, which is
/// the fewest layers the overhang law admits for a lateral move of `d`, so
/// every diagonal rises.
///
/// **One tie is never built twice**: a band that overlaps the band of a cross
/// already standing on the same two posts is refused and counted.
///
/// **Each diagonal is tested as the SOLID it is drawn** (fix/member-clearance,
/// 2026-09-29): its radius about its straight axis must clear the model by
/// the final gap, and must not enter any standing member it is not joined to
/// ([`member_solids_clear`]). The centreline-cell test stays in front of it as
/// the cheap first refusal.
///
/// **Both ends of the band lie on the posts' FREE SHAFTS**
/// (fix/brace-not-in-flare, 2026-09-30): the band bottom `k_lo` stands above
/// the top of each post's root flare and above the raft top
/// ([`free_shaft_floor`]; the band top is higher still). A band whose bottom
/// falls short MOVES UP by the shortfall, re-planned at the new top because a
/// leaning post changes the gap and so the depth, until it clears; it is
/// DROPPED (refused and counted) where it may not move (`may_move` false: it
/// was planned at a layer the member is already tied at, which is the whole
/// reason for that layer), where the moved top passes either post's top,
/// where the moved band no longer ties `run` (its top, from which it holds,
/// is not below the run's top), or where its plane no longer holds the
/// direction `run` sways in at the new layer.
///
/// **Refusals that were not geometry are gone** (Craig ruling 2026-10-01,
/// item 4): a band whose bottom would fall below the plate moves up exactly
/// as one in a flare does, and a band between posts that lean apart is
/// deepened until its diagonals fit the overhang law. What is still refused
/// is geometry and clearance: a band that cannot move (a post ends first), a
/// band below a branch partner's foot (moved onto the foot, its diagonal
/// ends inside the partner's host trunk), the model or a member or a drain
/// on a diagonal's path, a solid within the final gap. Each refusal is
/// counted under one reason ([`BandRefusals`]).
#[expect(
    clippy::too_many_arguments,
    reason = "the band needs the field, the two posts and the planned layer, and the run it ties \
              and whether it may move, to keep its ends on the free shafts"
)]
fn plan_cross(
    context: &SlendernessContext<'_>,
    state: &TieState,
    struts: &[SweepStrut],
    a: u32,
    b: u32,
    k: usize,
    run: &UnbracedRun,
    may_move: bool,
    outcome: &mut SlendernessOutcome,
) -> Option<CrossPlan> {
    let index = &state.index;
    let bands = &state.bands;
    let grid = context.grid;
    let h = context.layer_height_mm;
    // THE LOWEST LAYER THE BAND BOTTOM MAY TAKE: on both posts' free shafts
    // (above the flares and the raft). A band below a BRANCH partner's foot
    // is NOT moved up onto it: there its diagonal ends in the branch's
    // junction with its host trunk, which the member test admits as joined
    // (measured 2026-10-01, stilt plate: diagonal 96 inside post 73 by
    // 0.724 mm), so that refusal stays (`off_a_post`).
    let (floor, top_limit) = {
        let (Some(one), Some(two)) = (struts.get(a as usize), struts.get(b as usize)) else {
            return None;
        };
        // The band's top may not pass either post's JOINABLE top: a diagonal
        // seated on a headed post must keep clear of its joint sphere
        // (`joinable_at`); for a straight post this is `k_top`, as before.
        let (Some(one_top), Some(two_top)) = (
            one.joinable_top_layer(h),
            two.joinable_top_layer(h),
        ) else {
            return None;
        };
        (
            state
                .free_shaft_floor(grid, one, h)
                .max(state.free_shaft_floor(grid, two, h)),
            one_top.min(two_top),
        )
    };
    // EVERY REFUSAL IS COUNTED UNDER ONE REASON (2026-10-01).
    let refuse = |outcome: &mut SlendernessOutcome, reason: fn(&mut BandRefusals) -> &mut usize| {
        outcome.brace_bands_refused += 1;
        *reason(&mut outcome.brace_band_refusals) += 1;
    };
    let dropped = |outcome: &mut SlendernessOutcome| {
        refuse(outcome, |why| &mut why.in_flare);
        outcome.brace_bands_dropped_in_flare += 1;
    };
    let planned = k;
    let mut k = k;
    let mut moved = false;
    let mut moved_off_the_plate = false;
    let mut widened = false;
    // Each turn either breaks or raises `k` strictly, and `k` may not pass
    // `top_limit`, so the loop ends.
    let (a_cell_top, b_cell_top, depth, k_lo) = loop {
        let (one, two) = match (struts.get(a as usize), struts.get(b as usize)) {
            (Some(one), Some(two)) => (one, two),
            _ => return None,
        };
        let (a_top, b_top) = (one.cell_at(k), two.cell_at(k));
        let gap_mm = crate::cell_distance_mm(grid, a_top, b_top);
        let mut depth = context.lean.layers_to_reach(gap_mm);
        if gap_mm <= 0.0 || depth == 0 {
            refuse(outcome, |why| &mut why.no_gap);
            return None;
        }
        // THE BAND IS AS DEEP AS ITS DIAGONALS NEED (2026-10-01). Sized from
        // the gap at its top, a band between posts that lean apart has
        // diagonals whose own lateral moves are longer than that gap and
        // that the overhang law then refuses as too flat; the band is
        // deepened to the move its longer diagonal makes, until both fit.
        // The depth only grows, and never past `k`, so this ends.
        while let Some(k_lo) = k.checked_sub(depth) {
            let lateral = crate::cell_distance_mm(grid, a_top, two.cell_at(k_lo))
                .max(crate::cell_distance_mm(grid, b_top, one.cell_at(k_lo)));
            let needed = context.lean.layers_to_reach(lateral);
            if needed <= depth {
                break;
            }
            depth = needed;
            widened = true;
        }
        // A BAND THAT DOES NOT FIT BELOW `k` FITS HIGHER (2026-10-01). Its
        // bottom must stand at or above `floor` — the posts' free shafts, and
        // so above the plate — and where it falls short the whole band moves
        // up by the shortfall: reaching below the plate is no more a reason
        // to refuse it than reaching into a flare is. Until then it was
        // refused outright (Cylinder: 3,915 refusals below the plate).
        let bottom = k.checked_sub(depth).filter(|&k_lo| k_lo >= floor);
        if let Some(k_lo) = bottom {
            // The band TOP as planned (or moved) must also stand inside
            // both posts' joinable range: a band that already fits is not
            // exempt from the headed post's joint (`top_limit`).
            if k > top_limit {
                refuse(outcome, |why| &mut why.off_a_post);
                return None;
            }
            break (a_top, b_top, depth, k_lo);
        }
        let raised = floor + depth;
        if !may_move || raised > top_limit {
            if may_move {
                // How far above the layer it was planned at the band's top
                // would have to stand.
                state.flare_shortfall.set(Some(raised - planned));
            }
            // The binding reason, for the count: the plate, or a flare.
            if k < depth {
                refuse(outcome, |why| &mut why.below_plate);
            } else {
                dropped(outcome);
            }
            return None;
        }
        if k < depth {
            moved_off_the_plate = true;
        }
        k = raised.max(k + 1);
        moved = true;
    };
    if moved {
        let plane_holds = plane_between(grid, a_cell_top, b_cell_top).is_some_and(|plane| {
            let ties = state.ties.get(a as usize).map_or(&[][..], Vec::as_slice);
            new_plane_holds(ties, k, plane, run.sway_deg)
        });
        // TIE TIMING: a cross holds its posts from its band TOP, so a moved
        // band ties `run` only if its top still stands inside it.
        if k >= run.k_top || !plane_holds {
            dropped(outcome);
            return None;
        }
    }
    if bands
        .get(&(a.min(b), a.max(b)))
        .is_some_and(|standing| standing.iter().any(|&(lo, top)| lo <= k && k_lo <= top))
    {
        outcome.crosses_refused_overlapping += 1;
        return None;
    }
    let (a_cell_lo, b_cell_lo, floor_ok) = {
        let (Some(one), Some(two)) = (struts.get(a as usize), struts.get(b as usize)) else {
            return None;
        };
        (
            one.cell_at(k_lo),
            two.cell_at(k_lo),
            k_lo >= one.k_bottom && k_lo >= two.k_bottom && k <= one.k_top && k <= two.k_top,
        )
    };
    if !floor_ok {
        refuse(outcome, |why| &mut why.off_a_post);
        return None;
    }
    // Each diagonal's own lateral move, measured between the cells it actually
    // joins rather than assumed equal to the gap at the top of the band.
    let first_lateral = crate::cell_distance_mm(grid, a_cell_top, b_cell_lo);
    let second_lateral = crate::cell_distance_mm(grid, b_cell_top, a_cell_lo);
    if context.lean.descent_cost_mm(depth, first_lateral).is_none()
        || context.lean.descent_cost_mm(depth, second_lateral).is_none()
    {
        refuse(outcome, |why| &mut why.too_flat);
        return None;
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
    // **R2: A BRACE IS NEVER THICKER THAN THE THINNER POST IT JOINS** (Craig
    // ruling 2026-10-03, interim pending the slenderness coupon;
    // `bracing-mechanics.md` §2, decision D1). With `r_b = max(r_min, ℓ / S)`
    // this caps the diagonal's reach at `S` times the thinner post's radius.
    // The posts are read as DRAWN at the four joints (each diagonal's seat at
    // the band top and its landing at the bottom), so the junction raise that
    // used to make a post meet a thicker diagonal (a fat post with a thin
    // top) is never needed and is deleted. A design guard, not a strength
    // proof: the construction-stage check (R9) is what bounds the rest.
    if context.brace_cap {
        let thinner = [(a, k), (a, k_lo), (b, k), (b, k_lo)]
            .into_iter()
            .filter_map(|(post, layer)| {
                let strut = struts.get(post as usize)?;
                drawn_at(strut, state.profiles.get(post as usize)?, layer)
            })
            .fold(f64::INFINITY, f64::min);
        if radius_mm > thinner {
            refuse(outcome, |why| &mut why.thicker_than_post);
            return None;
        }
    }
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
        refuse(outcome, |why| &mut why.ring_under_plate);
        return None;
    }
    let path = |seat: usize, base: usize, lands_on: u32, outcome: &mut SlendernessOutcome| {
        match segment_path_or_block(context, index, seat, k, base, k_lo, &[lands_on]) {
            Ok(path) => Some(path),
            Err(block) => {
                let reason: fn(&mut BandRefusals) -> &mut usize = match block {
                    PathBlock::NoRise => |why| &mut why.no_gap,
                    PathBlock::Drain => |why| &mut why.path_drain,
                    PathBlock::Model => |why| &mut why.path_model,
                    PathBlock::Member => |why| &mut why.path_member,
                };
                refuse(outcome, reason);
                None
            }
        }
    };
    let first = path(a_cell_top, b_cell_lo, b, outcome)?;
    let second = path(b_cell_top, a_cell_lo, a, outcome)?;
    if context.member_clearance {
        let h = context.layer_height_mm;
        // Each end ON ITS POST'S DRAWN SHAFT (fix/morel-long-posts,
        // 2026-10-02): the axis cell the plan keeps, moved by the post's
        // drawn-line offset there, exactly where the adapter draws it
        // (`footprint::followed_shifts_drawn`). On a leaning plate post that
        // is up to 0.585 mm from the cell (Morel base, for-craig-v10).
        let point = |post: u32, cell: usize, layer: usize| {
            let at = crate::member_clearance::axis_point(grid, h, cell, layer);
            let off = state.drawn_off(struts, post, layer);
            [at[0] + off[0], at[1] + off[1], at[2] + off[2]]
        };
        let diagonals = [
            (point(a, a_cell_top, k), point(b, b_cell_lo, k_lo)),
            (point(b, b_cell_top, k), point(a, a_cell_lo, k_lo)),
        ];
        // The final gap does not apply inside a hidden cavity (Craig ruling
        // 2026-10-05): a band whose two diagonals, each with the brace's own
        // solid about it, stand wholly in hidden voxels ([`crate::cavity`])
        // is not read against the model (the path test above still refuses a
        // diagonal that crosses model material).
        let hidden = diagonals
            .iter()
            .all(|&(top, bottom)| context.history.hidden.segment_hidden(top, bottom, radius_mm));
        for (top, bottom) in diagonals {
            if !hidden
                && crate::member_clearance::model_intrusion(
                    grid,
                    context.history,
                    h,
                    top,
                    bottom,
                    radius_mm,
                    context.final_clearance_mm,
                )
                .is_some()
            {
                refuse(outcome, |why| &mut why.solid_model);
                outcome.brace_bands_refused_by_model += 1;
                return None;
            }
        }
        for (top, bottom) in diagonals {
            if !member_solids_clear(
                context,
                state,
                struts,
                top,
                bottom,
                radius_mm,
                &[a, b],
                outcome,
            ) {
                refuse(outcome, |why| &mut why.solid_member);
                outcome.brace_bands_refused_by_member += 1;
                return None;
            }
        }
    }
    Some(CrossPlan {
        a,
        b,
        k,
        k_lo,
        a_top: a_cell_top,
        b_top: b_cell_top,
        radius_mm,
        first,
        second,
        plane: plane_between(grid, a_cell_top, b_cell_top),
        volume_mm3: core::f64::consts::PI * radius_mm * radius_mm * (first_len + second_len),
        widened,
        moved,
        moved_off_the_plate,
    })
}

/// **Member against member**: whether the solid of radius `radius_mm` about
/// `from`–`to` stays out of every standing member's DRAWN solid, other than
/// the `posts` it joins and the members joined to those posts (a branch that
/// lands on one, a diagonal that seats or lands on one): those meet it at a
/// planned joint. The trunk a post stands on is NOT exempt (2026-10-01). One pass over the members, each an
/// O(1) bounding-box test and, where that passes, one segment distance; a
/// member whose WIDEST drawn radius does not clear is walked interval by
/// interval against its drawn profile (`member_clearance::member_intrusion`).
/// A verdict that differs from the retired closest-axes reading's is counted,
/// by direction, in `outcome.member_refusals_beyond_the_closest_axes` and
/// `outcome.member_admissions_the_closest_axes_refused`.
#[expect(
    clippy::too_many_arguments,
    reason = "the member test needs the field, the candidate's axis and radius, the posts it \
              joins, and the outcome its path-hit witness is counted in"
)]
fn member_solids_clear(
    context: &SlendernessContext<'_>,
    state: &TieState,
    struts: &[SweepStrut],
    from: [f64; 3],
    to: [f64; 3],
    radius_mm: f64,
    posts: &[u32],
    outcome: &mut SlendernessOutcome,
) -> bool {
    let grid = context.grid;
    let h = context.layer_height_mm;
    let rests_on = |member: &SweepStrut, post: u32| {
        member.seat_on_strut == Some(post)
            || matches!(member.landing, Landing::Strut { id, .. } if id == post)
    };
    // NOT the trunk a post stands on (2026-10-01): a diagonal meets its post
    // at the band, not at the post's foot, so a trunk is a member like any
    // other. Until then it was admitted as joined, and a band deepened to
    // reach a branch post's foot entered the trunk under it (stilt plate:
    // diagonal 96 inside post 73 by 0.724 mm, measured 2026-10-01).
    let joined = |other: u32| {
        if posts.contains(&other) {
            return true;
        }
        let Some(member) = struts.get(other as usize) else {
            return true;
        };
        posts.iter().any(|&post| rests_on(member, post))
    };
    let solids = struts
        .iter()
        .enumerate()
        .filter(|(_, s)| live(s))
        .map(|(index, strut)| crate::member_clearance::MemberSolid {
            id: strut.id,
            // Each member as the file draws it (fix/morel-long-posts,
            // 2026-10-02): a leaning plate post on its drawn line, and a
            // member joined to one at the end it joins it by, on that post's
            // drawn line too (the adapter draws it there,
            // `footprint::followed_shifts_drawn`). Measured on the Cylinder
            // before the joined ends followed: two diagonals tied to such
            // posts overlapped a third by up to 0.412 mm in the file.
            from: {
                let at =
                    crate::member_clearance::axis_point(grid, h, strut.base_cell(), strut.k_bottom);
                let off = state.member_end_off(struts, strut, false);
                [at[0] + off[0], at[1] + off[1], at[2] + off[2]]
            },
            to: {
                let at = crate::member_clearance::axis_point(grid, h, strut.cell, strut.k_top);
                let off = state.member_end_off(struts, strut, true);
                [at[0] + off[0], at[1] + off[1], at[2] + off[2]]
            },
            profile: state.profiles.get(index).map_or(&[][..], Vec::as_slice),
            k_bottom: strut.k_bottom,
            k_top: strut.k_top,
            fallback_radius_mm: strut.min_radius_mm,
            tip_radius_mm: strut.tip_radius_mm(),
        });
    let verdict = crate::member_clearance::member_intrusion(solids, from, to, radius_mm, joined);
    if verdict.refused_beyond_the_closest_axes() {
        outcome.member_refusals_beyond_the_closest_axes += 1;
    }
    if verdict.admitted_where_the_retired_reading_refused() {
        outcome.member_admissions_the_closest_axes_refused += 1;
    }
    verdict.entered.is_none()
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
        thickened_mm: Vec::new(),
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
        head: None,
    });
}

/// One planned model tie (D8): it lands on the post at `k_bottom`, rises to
/// the model cell `contact` and seats there at `k_top`. Not yet built.
#[derive(Clone, Debug)]
struct TiePlan {
    contact: usize,
    k_top: usize,
    k_bottom: usize,
    /// Its own slenderness radius `max(r_min, ℓ / S)`, and (R2) the post's
    /// drawn radius at the joint, which it may never pass.
    radius_mm: f64,
    cap_mm: f64,
    path: Vec<u32>,
}

/// Why no tie could be planned at one tie layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TieMiss {
    /// No model surface within the post's reach at that layer.
    NoSurface,
    /// Surface in reach, but no candidate passed every test.
    NoCandidate,
}

/// **D8: TIE THE POST TO THE MODEL** (Craig ruling 2026-10-03; the last rung
/// before a warning; [`crate::model_tie`] has the mechanics).
///
/// The run gets a SET of ties, planned in full and built together or not at
/// all, like a band set (R6): `n` ties, from the fixed-point count
/// `⌈L / limit⌉ − 1` up, their SEATS at the run's evenly spaced tie layers
/// (a tie holds from its seat, tie timing). Each tie is the shortest
/// candidate that passes every test ([`plan_model_tie`]). The set is built
/// only where
///
/// * every piece of the run, measured as the finished field will measure it
///   (from each tie's junction up to the next tie's seat), is inside its own
///   limit, and
/// * the post with its ties, counted through a frame in which every tie
///   bends and only the model is fixed ([`crate::model_tie::tied_post_sway`]),
///   sways at most one pixel in its WORST direction.
///
/// Each tie starts at its own slenderness radius. If the frame does not hold,
/// the ties are thickened together, by bisection, as far as the frame needs
/// and never past the post (R2); a thickened tie is screened again at its new
/// radius. If even the post's radius does not hold, one more tie is laid, as
/// long as one more still lowers the sway. A run no set holds stays a WARNING.
fn tie_to_model(
    context: &SlendernessContext<'_>,
    model: &ModelTies<'_>,
    state: &mut TieState,
    struts: &mut Vec<SweepStrut>,
    run: &UnbracedRun,
    outcome: &mut SlendernessOutcome,
) -> bool {
    let grid = context.grid;
    let h = context.layer_height_mm;
    let post_index = run.strut_id as usize;
    let Some(post) = struts.get(post_index) else {
        return false;
    };
    if !matches!(post.kind, StrutKind::Arm | StrutKind::Mast) || !live(post) {
        return false;
    }
    outcome.model_tie_runs_tried += 1;
    let floor = state.free_shaft_floor(grid, post, h).max(post.k_bottom);
    let fixed_point = extra_ties(run).max(1);
    // The post's DRAWN axis, in the sweep's frame (plate at z = 0).
    let on_post = |struts: &[SweepStrut], layer: usize| -> [f64; 3] {
        let at = crate::member_clearance::axis_point(grid, h, struts[post_index].cell_at(layer), layer);
        let off = state.drawn_off(struts, run.strut_id, layer);
        [at[0] + off[0], at[1] + off[1], at[2] + off[2]]
    };
    let (base_mm, top_mm) = (on_post(struts, run.k_lo), on_post(struts, run.k_top));
    let mut failing_sway = f64::INFINITY;
    let mut n = fixed_point;
    loop {
        if struts.len() + n > context.max_struts {
            return false;
        }
        let seats: Vec<usize> = (1..=n).filter_map(|step| tie_layer(run, step, n)).collect();
        if seats.len() != n || seats.windows(2).any(|pair| pair[0] >= pair[1]) {
            // No room for one more tie in the run.
            outcome.model_tie_refused_frame += 1;
            return false;
        }
        // Each tie at its evenly spaced seat; a seat with no admissible tie
        // there is tried a layer at a time lower and higher, up to half the
        // spacing either way (between its neighbours, as a refused band is
        // in R6). The pieces are judged on the seats as found.
        let spacing = (run.k_top - run.k_lo) / (n + 1);
        let mut plans: Vec<TiePlan> = Vec::with_capacity(n);
        for &seat in &seats {
            let mut found: Result<TiePlan, TieMiss> = Err(TieMiss::NoSurface);
            for shift in 0..=spacing / 2 {
                let tries = if shift == 0 {
                    [Some(seat), None]
                } else {
                    [seat.checked_sub(shift), Some(seat + shift)]
                };
                for at in tries.into_iter().flatten().filter(|&at| at > run.k_lo && at < run.k_top) {
                    let attempt =
                        plan_model_tie(context, model, state, struts, post_index, at, run, floor, None, outcome);
                    // A miss for want of any candidate outranks one for want
                    // of surface, for the count below.
                    match (&found, attempt) {
                        (_, Ok(plan)) => found = Ok(plan),
                        (Err(TieMiss::NoSurface), Err(miss)) => found = Err(miss),
                        _ => {}
                    }
                    if found.is_ok() {
                        break;
                    }
                }
                if found.is_ok() {
                    if shift > 0 {
                        outcome.model_ties_moved_from_their_layer += 1;
                    }
                    break;
                }
            }
            match found {
                Ok(plan) => plans.push(plan),
                Err(TieMiss::NoSurface) => {
                    outcome.model_tie_refused_no_surface += 1;
                    return false;
                }
                Err(TieMiss::NoCandidate) => {
                    outcome.model_tie_refused_no_candidate += 1;
                    return false;
                }
            }
        }
        plans.sort_by_key(|plan| plan.k_top);
        if !tie_pieces_fit(context, &struts[post_index], &state.profiles[post_index], run, &plans) {
            n += 1;
            continue;
        }
        // THE FRAME: every tie bends, only the model is fixed. The ties start
        // at their own slenderness radius and are thickened together, by
        // bisection, only as far as the frame needs, never past the post.
        let set_sway = |plans: &[TiePlan], radii: &[f64]| {
            tie_set_sway(context, base_mm, top_mm, run.radius_mm, plans, radii)
        };
        let holds = |sway: Option<f64>| sway.is_some_and(|px| px <= 1.0 + RATIO_NOISE);
        // TO A FIXED POINT: the frame asks for radii; a tie thickened past its
        // own slenderness radius is PLANNED AGAIN at the radius it will be
        // drawn at (its wider ball may hang off the face it had, or its wider
        // solid come within the gap, so it may seat elsewhere at the same
        // layer); the set as re-planned is judged again. Radii only grow, and
        // stop at the post's radius (R2). A round that moves no tie settles
        // the set. The round count is a numerical bound on a move that
        // cycles, like the bisection's, and a set that does not settle is
        // refused.
        let mut radii: Vec<f64> = plans.iter().map(|plan| plan.radius_mm).collect();
        let mut thickened = vec![false; plans.len()];
        let mut settled: Option<f64> = None;
        let mut short_by: Option<f64> = None;
        let mut pieces_broke = false;
        for _ in 0..64 {
            let low: Vec<f64> = plans
                .iter()
                .zip(&radii)
                .map(|(plan, &radius)| radius.max(plan.radius_mm))
                .collect();
            if holds(set_sway(&plans, &low)) {
                radii = low;
                settled = set_sway(&plans, &radii);
                break;
            }
            let cap: Vec<f64> = plans
                .iter()
                .zip(&low)
                .map(|(plan, &radius)| plan.cap_mm.max(radius))
                .collect();
            let at_cap = set_sway(&plans, &cap);
            if !holds(at_cap) {
                short_by = Some(at_cap.unwrap_or(f64::INFINITY));
                break;
            }
            let between = |t: f64| -> Vec<f64> {
                low.iter()
                    .zip(&cap)
                    .map(|(&from, &to)| from + t * (to - from))
                    .collect()
            };
            let (mut below, mut above) = (0.0_f64, 1.0_f64);
            for _ in 0..64 {
                let middle = 0.5 * (below + above);
                if holds(set_sway(&plans, &between(middle))) {
                    above = middle;
                } else {
                    below = middle;
                }
            }
            radii = between(above);
            let mut moved = false;
            for (index, plan) in plans.iter_mut().enumerate() {
                if radii[index] <= plan.radius_mm * (1.0 + RATIO_NOISE) {
                    continue;
                }
                thickened[index] = true;
                let Ok(again) = plan_model_tie(
                    context,
                    model,
                    state,
                    struts,
                    post_index,
                    plan.k_top,
                    run,
                    floor,
                    Some(radii[index]),
                    outcome,
                ) else {
                    outcome.model_tie_refused_no_candidate += 1;
                    return false;
                };
                moved |= again.contact != plan.contact || again.k_bottom != plan.k_bottom;
                *plan = again;
            }
            if !moved {
                settled = set_sway(&plans, &radii);
                break;
            }
            if !tie_pieces_fit(context, &struts[post_index], &state.profiles[post_index], run, &plans) {
                pieces_broke = true;
                break;
            }
        }
        if pieces_broke {
            // A moved tie left a piece over its limit: lay one more, as when
            // the pieces did not fit before the frame.
            n += 1;
            continue;
        }
        if let Some(at) = short_by {
            // Even the post's own radius does not hold: one more tie, while
            // one more still lowers the sway.
            if at >= failing_sway {
                outcome.model_tie_refused_frame += 1;
                return false;
            }
            failing_sway = at;
            n += 1;
            continue;
        }
        let Some(sway) = settled.filter(|&px| holds(Some(px))) else {
            outcome.model_tie_refused_frame += 1;
            return false;
        };
        outcome.model_ties_thickened_for_the_frame += thickened.iter().filter(|&&t| t).count();
        outcome.model_tie_worst_sway_px = outcome.model_tie_worst_sway_px.max(sway);
        outcome.model_tie_sets += 1;
        if n > extra_ties(run) {
            outcome.model_tie_sets_beyond_fixed_point += 1;
        }
        commit_model_ties(
            grid,
            context.layer_height_mm,
            state,
            struts,
            run.strut_id,
            &plans,
            &radii,
            outcome,
        );
        return true;
    }
}

/// Whether every piece of `run` a planned tie set leaves is inside its own
/// limit, as the finished field's run measure will cut it (tie timing): from
/// the run's base to the first tie's seat, from each tie's junction to the
/// next tie's seat, and from the last junction to the run's top; each
/// junction above the one before. Measured with the production run length
/// and drawn equivalent radius.
fn tie_pieces_fit(
    context: &SlendernessContext<'_>,
    post: &SweepStrut,
    profile: &[f64],
    run: &UnbracedRun,
    plans: &[TiePlan],
) -> bool {
    let (grid, h, law) = (context.grid, context.layer_height_mm, context.law);
    let fits = |lo: usize, hi: usize| {
        hi > lo
            && run_length(grid, post, lo, hi, h)
                <= law.max_unbraced_mm(run_radius(post, &post.thickened_mm, profile, lo, hi))
    };
    let mut lo = run.k_lo;
    for plan in plans {
        if plan.k_bottom <= lo || !fits(lo, plan.k_top) {
            return false;
        }
        lo = plan.k_bottom;
    }
    fits(lo, run.k_top)
}

/// The tied post's worst-direction top sway in pixels
/// ([`crate::model_tie::tied_post_sway`]): the run's drawn chord from `base`
/// to `top` at its equivalent radius, each planned tie at its radius from its
/// junction on the post to its seat cell, fixed in the model.
fn tie_set_sway(
    context: &SlendernessContext<'_>,
    base: [f64; 3],
    top: [f64; 3],
    post_radius_mm: f64,
    plans: &[TiePlan],
    radii: &[f64],
) -> Option<f64> {
    let (grid, h) = (context.grid, context.layer_height_mm);
    #[expect(
        clippy::cast_precision_loss,
        reason = "a layer index bounded by the plan's own layer count is exact in f64"
    )]
    let members: Vec<crate::model_tie::TieMember> = plans
        .iter()
        .zip(radii)
        .map(|(plan, &radius_mm)| crate::model_tie::TieMember {
            junction_z_mm: plan.k_bottom as f64 * h,
            seat_mm: crate::member_clearance::axis_point(grid, h, plan.contact, plan.k_top),
            radius_mm,
        })
        .collect();
    crate::model_tie::tied_post_sway(base, top, post_radius_mm, &members, context.law.ratio())
}

/// **Plan one model tie** whose seat is at layer `seat`, from the post at
/// `post_index`, against the live field: the shortest candidate that passes
/// every test, or why none did. With `thicker = Some(radius)` every candidate
/// is planned and tested at that radius (a tie the frame thickened), never
/// below its own slenderness radius.
///
/// The candidates are the model's SURFACE cells at the seat layer within the
/// post's reach ([`crate::model_tie::surface_contacts`]), nearest first. For
/// each:
///
/// * **the joint**: the tie rises at the overhang law's saturated lean, the
///   brace angle (decision D3), so its junction on the post is the fewest
///   layers below the seat the law admits for the move it makes, from where
///   the post stands at that layer; the junction lies on the post's free
///   shaft and inside the run;
/// * **R2**: its radius `max(r_min, ℓ / S)` is no more than the post's drawn
///   radius at the joint;
/// * **the centreline**: no drain, model or other member on its cells
///   ([`segment_path_or_block`], the test every brace and merge uses);
///
/// and then, all candidates in one batch, as the file will draw them (the
/// tip ball seated by the lowering's own seat law on the face it meets, the
/// nearest face of any slope):
///
/// * **its solid clears the model by the final gap outside its own seat
///   zone**: the finished-field census itself
///   ([`crate::member_clearance::model_clearance_census`]), confirmed alone
///   on the chosen tie so a span the kernel could not decide never passes;
/// * **its contact disc lies wholly on the model** ([`disc_on_model`]);
/// * **its solid enters no member** it is not joined to
///   ([`member_solids_clear`]).
#[expect(
    clippy::too_many_arguments,
    reason = "one tie needs the field, the model, the post, its seat layer, the run it ties, the \
              post's free shaft and an optional thicker radius"
)]
fn plan_model_tie(
    context: &SlendernessContext<'_>,
    model: &ModelTies<'_>,
    state: &TieState,
    struts: &[SweepStrut],
    post_index: usize,
    seat: usize,
    run: &UnbracedRun,
    floor: usize,
    thicker: Option<f64>,
    outcome: &mut SlendernessOutcome,
) -> Result<TiePlan, TieMiss> {
    let grid = context.grid;
    let h = context.layer_height_mm;
    let law = context.law;
    let post = &struts[post_index];
    let profile = &state.profiles[post_index];
    let Some(at_seat) = drawn_at(post, profile, seat) else {
        return Err(TieMiss::NoSurface);
    };
    let centre = post.cell_at(seat);
    // R2 bounds the reach: a tie no thicker than the post reaches at most `S`
    // times the post's radius along itself.
    let reach = law.brace_reach_mm(at_seat);
    let contacts: Vec<usize> =
        crate::model_tie::surface_contacts(grid, context.history, centre, seat, reach)
            .into_iter()
            .map(|(_, cell)| cell)
            .collect();
    if contacts.is_empty() {
        return Err(TieMiss::NoSurface);
    }
    let mut candidates: Vec<TiePlan> = Vec::new();
    for contact in contacts {
        outcome.model_tie_candidates += 1;
        // The fewest layers the overhang law admits for the move the tie
        // makes from where the post stands at its joint; deepened until the
        // move from that joint fits (a leaning post moves under it). The
        // depth only grows and the joint stays above layer 0, so this ends.
        let mut depth = context
            .lean
            .layers_to_reach(crate::cell_distance_mm(grid, centre, contact));
        let joint = loop {
            if depth == 0 {
                break None;
            }
            let Some(k_lo) = seat.checked_sub(depth) else {
                break None;
            };
            let lateral = crate::cell_distance_mm(grid, contact, post.cell_at(k_lo));
            let need = context.lean.layers_to_reach(lateral);
            if need == 0 {
                break None;
            }
            if need <= depth {
                break Some((k_lo, lateral));
            }
            depth = need;
        };
        let Some((k_lo, lateral)) = joint.filter(|&(k_lo, _)| k_lo >= floor && k_lo > run.k_lo)
        else {
            outcome.model_tie_refused_off_shaft += 1;
            continue;
        };
        #[expect(
            clippy::cast_precision_loss,
            reason = "a layer count bounded by the plan's own layer count is exact in f64"
        )]
        let length = (depth as f64 * h).hypot(lateral);
        let own = law.brace_radius_mm(length);
        let cap = drawn_at(post, profile, k_lo).unwrap_or(0.0);
        let radius = thicker.map_or(own, |radius| radius.max(own));
        if context.brace_cap && radius > cap * (1.0 + RATIO_NOISE) {
            outcome.model_tie_refused_thicker_than_post += 1;
            continue;
        }
        let base_cell = post.cell_at(k_lo);
        let path =
            match segment_path_or_block(context, &state.index, contact, seat, base_cell, k_lo, &[post.id]) {
                Ok(path) => path,
                Err(block) => {
                    *match block {
                        PathBlock::Drain => &mut outcome.model_tie_refused_path_drain,
                        PathBlock::Model | PathBlock::NoRise => &mut outcome.model_tie_refused_path_model,
                        PathBlock::Member => &mut outcome.model_tie_refused_path_member,
                    } += 1;
                    continue;
                }
            };
        candidates.push(TiePlan {
            contact,
            k_top: seat,
            k_bottom: k_lo,
            radius_mm: radius,
            cap_mm: cap,
            path,
        });
    }
    if candidates.is_empty() {
        return Err(TieMiss::NoCandidate);
    }
    match screen_model_ties(context, model, state, struts, post_index, &candidates, outcome) {
        Screened::Passed(index) => Ok(candidates.swap_remove(index)),
        // The contacts the seat trim would cut are sized up until it does
        // not (the nearest that then also keeps its disc on the model), and
        // the layer is planned again at that radius. The radius only grows
        // and the ladder is finite, so this ends.
        Screened::Failed { bitten } if !bitten.is_empty() => {
            let current = candidates[bitten[0]].radius_mm;
            let sized: Vec<TiePlan> = bitten.iter().map(|&index| candidates[index].clone()).collect();
            match radius_the_trim_keeps(context, model, state, struts, post.id, &sized) {
                Some(radius) if radius > current => {
                    outcome.model_tie_tips_sized_for_the_trim += 1;
                    plan_model_tie(
                        context,
                        model,
                        state,
                        struts,
                        post_index,
                        seat,
                        run,
                        floor,
                        Some(radius),
                        outcome,
                    )
                }
                _ => Err(TieMiss::NoCandidate),
            }
        }
        Screened::Failed { .. } => Err(TieMiss::NoCandidate),
    }
}

/// A planned tie as the member it would be, at `radius_mm` (its ledger filled
/// so the census draws it), and its seat normal where measured.
fn tie_member(grid: &Grid, id: u32, post: u32, plan: &TiePlan, radius_mm: f64) -> SweepStrut {
    let span = plan.k_top - plan.k_bottom;
    SweepStrut {
        id,
        cell: plan.contact,
        center_mm: [
            grid.center_x_mm(grid.col_of(plan.contact)),
            grid.center_y_mm(grid.row_of(plan.contact)),
        ],
        kind: StrutKind::Tie,
        min_radius_mm: radius_mm,
        thickened_mm: Vec::new(),
        seat_on_strut: None,
        k_top: plan.k_top,
        k_bottom: plan.k_bottom,
        landing: Landing::Strut {
            id: post,
            layer: plan.k_bottom,
        },
        interior: false,
        radius_mm: vec![radius_mm; span],
        carried_mm2: vec![0.0; span],
        covered_demand: Vec::new(),
        // A tie roots no demand cell, so the removal pass's coverage test
        // would be vacuously true for it; the flag that protects a
        // load-inserted arm protects it, for the same reason.
        inserted_by_load: true,
        path: plan.path.clone(),
        seat_face: crate::SeatFace::default(),
        seat_normal: [0.0; 3],
        straight_fallback: None,
        fallback_path: Vec::new(),
        head: None,
    }
}

/// Planned ties as the members and the arms the lowering would draw them as:
/// each at its own radius, its seat normal and face point from the nearest
/// face of any slope to its tip point (`tip_landing::tip_seat_faces` for a
/// tie), one surface projection for the batch. Member ids are batch indices.
fn tie_arms(
    context: &SlendernessContext<'_>,
    model: &ModelTies<'_>,
    state: &TieState,
    struts: &[SweepStrut],
    post_id: u32,
    candidates: &[TiePlan],
) -> (Vec<SweepStrut>, Vec<Option<[f64; 3]>>, Vec<plinth_support::SweepArm>) {
    let grid = context.grid;
    let h = context.layer_height_mm;
    let plate_z = model.plate_z_mm;
    let mut members: Vec<SweepStrut> = candidates
        .iter()
        .enumerate()
        .map(|(index, plan)| {
            tie_member(grid, u32::try_from(index).unwrap_or(u32::MAX), post_id, plan, plan.radius_mm)
        })
        .collect();
    // THE FACE EACH TIP MEETS: the nearest face of any slope to its tip point
    // (`tip_landing::tip_seat_faces` for a tie), one projection.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a layer index bounded by the plan's own layer count is exact in f64"
    )]
    let points: Vec<[f64; 3]> = members
        .iter()
        .map(|member| {
            [
                member.center_mm[0],
                member.center_mm[1],
                plate_z + (member.k_top as f64 + 0.5) * h,
            ]
        })
        .collect();
    let (_, closest, faces, normals) = model.surface.project(&points);
    let mut face_points: Vec<Option<[f64; 3]>> = Vec::with_capacity(members.len());
    for (index, member) in members.iter_mut().enumerate() {
        let normal = (faces.get(index).copied().unwrap_or(-1) >= 0)
            .then(|| geom_kernel::vec3::normalize(normals[index]))
            .flatten();
        member.seat_normal = normal.unwrap_or([0.0; 3]);
        face_points.push(normal.map(|_| closest[index]));
    }
    let off = |member: &SweepStrut| state.drawn_off(struts, post_id, member.k_bottom);
    let arms: Vec<plinth_support::SweepArm> = members
        .iter()
        .zip(&face_points)
        .map(|(member, face)| {
            let (seat, base) = crate::adapter::drawn_ends(grid, plate_z, h, member);
            let shift = off(member);
            let base = [base[0] + shift[0], base[1] + shift[1], base[2] + shift[2]];
            crate::adapter::sweep_arm(member, seat, base, *face, member.radius_mm.clone())
        })
        .collect();
    (members, face_points, arms)
}

/// What screening found: the first candidate (in order) that passes every
/// drawn test, or, where none does, the candidates (in order) that cleared
/// the model and were refused because the seat trim would cut their shaft
/// (contacts that a thicker tip may seat).
enum Screened {
    Passed(usize),
    Failed { bitten: Vec<usize> },
}

/// The first of `candidates` (in their order) that passes the drawn tests.
/// One surface projection, one contact-disc derivation, one seat-trim
/// derivation and one census for the whole batch; the member test and the
/// lone confirming census only for candidates the batch passed, in order,
/// until one holds.
fn screen_model_ties(
    context: &SlendernessContext<'_>,
    model: &ModelTies<'_>,
    state: &TieState,
    struts: &[SweepStrut],
    post_index: usize,
    candidates: &[TiePlan],
    outcome: &mut SlendernessOutcome,
) -> Screened {
    let grid = context.grid;
    let h = context.layer_height_mm;
    let plate_z = model.plate_z_mm;
    let post_id = struts[post_index].id;
    let (members, face_points, arms) = tie_arms(context, model, state, struts, post_id, candidates);
    let mut bitten: Vec<usize> = Vec::new();
    let census = crate::member_clearance::model_clearance_census(
        Some(model.surface),
        &members,
        &arms,
        model.layer_plan,
        plate_z,
        context.final_clearance_mm,
        &context.history.hidden,
    );
    let near: BTreeSet<u32> = census.within_gap.iter().map(|row| row.member).collect();
    let discs = plinth_support::tip_contact_discs(&arms, model.layer_plan)
        .unwrap_or_else(|_| vec![None; arms.len()]);
    // THE LIP-SEAT RULE (fix/lip-seat): the seat trim may cut the tip ball's
    // cap and never the shaft outside it, measured by the lowering's own
    // derivation. A tie that meets its face too side-on is refused here, as
    // the field's census would warn about it.
    let bites = plinth_support::tip_shaft_seat_bites(&arms, model.layer_plan)
        .unwrap_or_else(|_| vec![None; arms.len()]);
    for (index, member) in members.iter().enumerate() {
        if near.contains(&member.id) || face_points[index].is_none() {
            outcome.model_tie_refused_solid_model += 1;
            continue;
        }
        if crate::lip_seat::cuts(bites.get(index).and_then(Option::as_ref)) {
            outcome.model_tie_refused_trim_bite += 1;
            bitten.push(index);
            continue;
        }
        let Some(disc) = discs.get(index).copied().flatten() else {
            outcome.model_tie_refused_disc_off_face += 1;
            continue;
        };
        if !crate::model_tie::disc_on_model(grid, context.history, h, plate_z, &disc) {
            outcome.model_tie_refused_disc_off_face += 1;
            continue;
        }
        // The member test in the sweep's own frame (plate at z = 0), from the
        // ball centre the shaft starts at down to its junction on the post.
        let from = [
            disc.ball_centre_mm[0],
            disc.ball_centre_mm[1],
            disc.ball_centre_mm[2] - plate_z,
        ];
        let base = arms[index].base_mm;
        let to = [base[0], base[1], base[2] - plate_z];
        if !member_solids_clear(
            context,
            state,
            struts,
            from,
            to,
            member.min_radius_mm,
            &[post_id],
            outcome,
        ) {
            outcome.model_tie_refused_solid_member += 1;
            continue;
        }
        // CONFIRMED ALONE: a span the kernel could not decide is not in the
        // batch's list of near members, and must not pass for one that clears.
        let alone = crate::member_clearance::model_clearance_census(
            Some(model.surface),
            std::slice::from_ref(member),
            std::slice::from_ref(&arms[index]),
            model.layer_plan,
            plate_z,
            context.final_clearance_mm,
            &context.history.hidden,
        );
        if !tie_confirmed_clear(&alone) {
            outcome.model_tie_refused_solid_model += 1;
            continue;
        }
        outcome.model_tie_candidates_hidden += alone.members_hidden;
        return Screened::Passed(index);
    }
    Screened::Failed { bitten }
}

/// Whether the lone census of ONE planned tie confirms it clear of the
/// model: measured with no span undecided and none inside the gap, OR left
/// out of the census because the tie stands wholly in a hidden cavity (Craig
/// ruling 2026-10-05; [`crate::cavity`]: a tie from an interior post to its
/// cavity wall is classified by its own proposed geometry, foot on the post
/// and contact on the wall), where the final gap does not apply.
fn tie_confirmed_clear(alone: &crate::report::MemberModelClearance) -> bool {
    let clear = alone.within_gap.is_empty() && alone.members_unmeasured == 0;
    clear && (alone.members_measured == 1 || alone.members_hidden == 1)
}

/// **The contact sized up until the seat trim keeps its shaft** (the
/// lip-seat rule read as contact sizing): the smallest radius, between the
/// candidate's own and the post's (R2), at which the lowering's own
/// shaft-bite derivation finds no cut outside the tip ball. The limit angle
/// grows with the tip (`ShaftSeatBite::limit_deg`): a tie at the brace angle
/// meets a vertical wall about 33 deg off its normal, just past the 32.4 deg
/// a minimum tip admits and well inside the 43.9 deg of a 0.106 mm tip
/// (measured on the lonely peak). For the bitten contacts in their order
/// (nearest first), a ladder of radii evenly spaced at the resolution the
/// frame's bisection uses (64 steps) up to the post's: the first contact with
/// a rung whose shaft the trim keeps AND whose wider disc still lies wholly
/// on the model gives that rung's radius. Batched: one arm derivation, one
/// bite derivation and one disc derivation per group of contacts (a batching
/// size, not a rule: the first contact in order wins whatever the grouping).
/// `None` where no contact has such a rung.
fn radius_the_trim_keeps(
    context: &SlendernessContext<'_>,
    model: &ModelTies<'_>,
    state: &TieState,
    struts: &[SweepStrut],
    post_id: u32,
    bitten: &[TiePlan],
) -> Option<f64> {
    const STEPS: u32 = 64;
    const GROUP: usize = 8;
    for group in bitten.chunks(GROUP) {
        let ladder: Vec<TiePlan> = group
            .iter()
            .filter(|plan| plan.cap_mm > plan.radius_mm)
            .flat_map(|plan| {
                (1..=STEPS).map(move |step| TiePlan {
                    radius_mm: plan.radius_mm
                        + (plan.cap_mm - plan.radius_mm) * f64::from(step) / f64::from(STEPS),
                    ..plan.clone()
                })
            })
            .collect();
        if ladder.is_empty() {
            continue;
        }
        let (_, _, arms) = tie_arms(context, model, state, struts, post_id, &ladder);
        let bites = plinth_support::tip_shaft_seat_bites(&arms, model.layer_plan).ok()?;
        let discs = plinth_support::tip_contact_discs(&arms, model.layer_plan).ok()?;
        let found = ladder.iter().zip(bites.iter().zip(&discs)).find(|(_, (bite, disc))| {
            bite.as_ref().is_some_and(|bite| !crate::lip_seat::over_limit(bite))
                && disc.as_ref().is_some_and(|disc| {
                    crate::model_tie::disc_on_model(
                        context.grid,
                        context.history,
                        context.layer_height_mm,
                        model.plate_z_mm,
                        disc,
                    )
                })
        });
        if let Some((rung, _)) = found {
            return Some(rung.radius_mm);
        }
    }
    None
}

/// Build a planned tie set on the post `post`, at `radii`, and keep the
/// pass's state current: each tie is a landing on the post that holds it in
/// every direction from the layer its seat prints (tie timing), exactly as
/// the finished field's run measure credits it ([`tie_lists`]).
fn commit_model_ties(
    grid: &Grid,
    layer_height_mm: f64,
    state: &mut TieState,
    struts: &mut Vec<SweepStrut>,
    post: u32,
    plans: &[TiePlan],
    radii: &[f64],
    outcome: &mut SlendernessOutcome,
) {
    for (plan, &radius) in plans.iter().zip(radii) {
        let id = u32::try_from(struts.len()).unwrap_or(u32::MAX);
        let mut member = tie_member(grid, id, post, plan, radius);
        // Sized by the down-sweep that follows the pass, like every member
        // the ladder adds.
        member.radius_mm = Vec::new();
        member.carried_mm2 = Vec::new();
        struts.push(member);
        if let Some(list) = state.ties.get_mut(post as usize) {
            list.push(Tie {
                layer: plan.k_bottom,
                from: plan.k_top,
                plane: None,
                band_top: false,
            });
        }
        let span = plan.k_top - plan.k_bottom;
        state.ties.push(Vec::new());
        state.profiles.push(vec![radius; span + 1]);
        state.unthickened.push(vec![radius; span + 1]);
        outcome.model_ties_built += 1;
    }
    stamp_tail(
        &mut state.index,
        struts,
        plans.len(),
        layer_height_mm,
    );
}

#[cfg(test)]
mod tests {
    use super::{extra_ties, SlendernessLaw, UnbracedRun};

    /// D9: the two-post ceiling reproduces the study's two pairs
    /// (`bracing-mechanics.md` §3: "3.8 for the thin pair and 3.5 for the mid
    /// pair") and its equal-post closed form `(1 + s²/r²)^(1/4)`, grows with
    /// a wider partner ("brace to a wider frame"), and falls as the run's own
    /// post thickens, more slowly than `λ` does (so thickening INTO the
    /// window terminates).
    #[test]
    fn the_two_post_ceiling_is_the_composite_beam() {
        let thin = super::pair_ceiling_ratio(0.15, 0.15, 2.2);
        let mid = super::pair_ceiling_ratio(0.4, 0.4, 5.0);
        assert!((thin - 3.834).abs() < 1e-3, "thin pair {thin}");
        assert!((mid - 3.541).abs() < 1e-3, "mid pair {mid}");
        for (r, s) in [(0.05, 0.3), (0.2, 1.0), (0.7, 9.0)] {
            let closed = (1.0_f64 + s * s / (r * r)).powf(0.25);
            assert!((super::pair_ceiling_ratio(r, r, s) - closed).abs() < 1e-12);
        }
        assert!(super::pair_ceiling_ratio(0.15, 0.6, 2.2) > thin, "a wider partner holds more");
        // λ ∝ 1/r, the ceiling falls more slowly: λ / ceiling falls as r grows.
        let over = |r: f64| (0.15 / r) * 4.0 / super::pair_ceiling_ratio(r, 0.15, 2.2);
        assert!(over(0.2) < over(0.15) && over(0.3) < over(0.2));
        assert!((super::pair_ceiling_ratio(0.15, 0.15, 0.0) - 1.0).abs() < 1e-12);
    }

    /// D9 on a field: two 20 mm, 0.05 mm plate posts 2.0 mm apart, each 8
    /// limits long (S = 50). Their pair's ceiling is `(1 + 40²)^(1/4)` = 6.33,
    /// so no band is built between them as they stand. A post is thickened
    /// INTO the window (to the pair's ceiling at its new radius: root 0.484
    /// mm, 5.31 mm³, measured 2026-10-03) because full thickening (`r_eq`
    /// 0.4 mm over 20 mm) costs more resin than that plus the crosses, and
    /// then the pair is braced. (At 1.0 mm apart every growth laps the other
    /// post's drawn foot, so the fixture stands them 2.0 mm apart.) With the
    /// window off AND the frame off (the fixed-point ladder before
    /// 2026-10-03) the same pass builds bands between the pair at once and
    /// nothing is thickened into a window.
    ///
    /// RE-PINNED 2026-10-03 (R6, fix/brace-frame-bands): until then the
    /// window-off run alone was the sabotage, and it built bands between the
    /// pair. Under R6 the FRAME counts the bands, and no count that fits holds
    /// a run 8 limits long between these posts: the ceiling is now physics
    /// the frame meets, not a rule of the window's. So the window-off run with
    /// the frame on builds no band between the pair (the frame refuses it,
    /// counted), and the old drill is the run with both rules off.
    #[test]
    fn a_pair_past_its_ceiling_is_thickened_into_the_window_then_braced() {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let history = crate::up_sweep::ColumnHistory::new(&grid);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let tall = |id: u32, col: usize| {
            let mut post = plate_post(&grid, id, col, 0.05);
            post.k_top = 400;
            post.radius_mm = vec![0.05; 400];
            post.carried_mm2 = vec![0.0; 400];
            post
        };
        let run = |window: bool, frame: bool| {
            let context = super::SlendernessContext {
                grid: &grid,
                history: &history,
                keep_out: &keep_out,
                lean: &lean,
                law: &law,
                layer_height_mm: 0.05,
                layers: 500,
                flare_factor: 1.6,
                mast_root_clearance: true,
                final_clearance_mm: 0.3,
                member_clearance: true,
                model_surface: None,
                plate_z_mm: 0.0,
                shaft_landings: true,
                merge: true,
                brace: true,
                thicken: true,
                mast: false,
                max_struts: 1_000,
                x_allowed: 12.0,
                seat_faces: crate::tip_landing::SeatFaceRule::from_overhang_band(45.0, 25.0),
                load_step_taper: crate::adapter::LoadStepTaper::of(&lean),
                brace_cap: true,
                brace_window: window,
                frame_bands: frame,
                frame_ledger: true,
                model_tie: true,
            };
            let mut struts = vec![tall(0, 100), tall(1, 140)];
            let mut outcome = super::SlendernessOutcome::default();
            super::apply_ties(&context, &mut struts, &mut outcome);
            outcome
        };
        assert!(
            (super::pair_ceiling_ratio(0.05, 0.05, 2.0) - 1601.0_f64.powf(0.25)).abs() < 1e-12
        );
        let on = run(true, true);
        println!(
            "window on: {} past the ceiling, {} thickened into the window, {} thickened, {} \
             crosses ({} sets), {} refused for resin",
            on.crosses_refused_past_pair_ceiling,
            on.members_thickened_into_the_window,
            on.members_thickened,
            on.braces_added,
            on.frame_band_sets,
            on.crosses_refused_thicken_cheaper
        );
        assert!(on.crosses_refused_past_pair_ceiling > 0, "path hit: {on:?}");
        assert!(on.members_thickened_into_the_window > 0, "path hit: {on:?}");
        assert!(on.braces_added > 0, "the pair is braced once inside its window: {on:?}");
        assert!(on.frame_band_sets > 0, "and braced by a frame band set: {on:?}");
        let fixed_point = run(false, false);
        assert_eq!(fixed_point.crosses_refused_past_pair_ceiling, 0);
        assert_eq!(fixed_point.members_thickened_into_the_window, 0);
        assert!(fixed_point.braces_added > 0, "{fixed_point:?}");
        let frame_only = run(false, true);
        assert_eq!(frame_only.members_thickened_into_the_window, 0);
        assert!(frame_only.frame_pairs_past_ceiling > 0, "{frame_only:?}");
        assert_eq!(frame_only.braces_added, 0, "{frame_only:?}");
    }

    /// R7 + D5: a cross is charged one diagonal per post when both need it
    /// (`c` = 1) and the whole cross when only the run does (`c` = 2), per
    /// plane and per band (`bracing-mechanics.md` §5).
    #[test]
    fn a_cross_is_charged_per_plane_per_band_and_shared_halves_it() {
        assert!((super::cross_charge_mm3(2.0, true, 2, 3) - 6.0).abs() < 1e-12);
        assert!((super::cross_charge_mm3(2.0, false, 2, 3) - 12.0).abs() < 1e-12);
        assert!((super::cross_charge_mm3(2.0, false, 1, 1) - 2.0).abs() < 1e-12);
    }

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
            sway_deg: None,
        };
        assert_eq!(extra_ties(&run(4.0, 5.0)), 0);
        assert_eq!(extra_ties(&run(6.0, 5.0)), 1);
        assert_eq!(extra_ties(&run(10.0, 5.0)), 1);
        assert_eq!(extra_ties(&run(11.0, 5.0)), 2);
    }

    /// The taper integral reads a uniform post as its own radius, whatever
    /// the layer count: `(L/(S r))⁴` is the uniform cantilever.
    #[test]
    fn a_uniform_post_is_its_own_equivalent_radius() {
        for n in [1_usize, 2, 7, 700] {
            let r = super::equivalent_radius(n, |_| Some(0.4)).expect("a radius");
            assert!(
                (r - 0.4).abs() < 1.0e-12,
                "{n} layers of 0.4 mm read as {r}"
            );
        }
    }

    /// A cantilever is held by its ROOT: thickening the bottom half of a post
    /// stiffens it more than thickening the top half by the same amount.
    #[test]
    fn the_root_holds_a_cantilever_and_the_tip_barely_matters() {
        let n = 100;
        let thick_root = super::equivalent_radius(n, |j| Some(if j < n / 2 { 0.8 } else { 0.4 }))
            .expect("a radius");
        let thick_tip = super::equivalent_radius(n, |j| Some(if j < n / 2 { 0.4 } else { 0.8 }))
            .expect("a radius");
        // Bottom half weighs 1 - (1/2)^4 = 0.9375: r_eq = 0.677 and 0.406.
        assert!(
            thick_root > 0.65 && thick_tip < 0.45,
            "root-thick {thick_root}, tip-thick {thick_tip}"
        );
    }

    /// One cross holds the quarter-turn either side of its own plane and
    /// nothing else: the breakpoints fall 45° either side of its plane, and two
    /// perpendicular planes hold every direction between them.
    #[test]
    fn one_cross_holds_the_arc_around_its_own_plane_and_no_more() {
        let along = [0_i64, 10];
        let other = [10_i64, 0];
        assert!(super::holds(along, 90.0));
        assert!(super::holds(along, 135.0));
        assert!(!super::holds(along, 136.0));
        assert!(!super::holds(along, 0.0));
        let one = super::sway_directions(&[along]);
        assert_eq!(one.len(), 2, "{one:?}");
        let held = one
            .iter()
            .filter(|sway| super::holds(along, sway.expect("a direction")))
            .count();
        assert_eq!(held, 1, "one arc held and one not: {one:?}");
        let both = super::sway_directions(&[along, other]);
        assert!(
            both.iter().all(|sway| {
                let sway = sway.expect("a direction");
                super::holds(along, sway) || super::holds(other, sway)
            }),
            "two perpendicular planes hold every direction: {both:?}"
        );
    }

    /// The port of the field report's panel frame reproduces the numbers that
    /// frame recorded (`braces-masts.md` Q4, lonely peak arm 2 / mast 11:
    /// 1.56 px in-plane at the built 1.217 mm gap, one pixel at 2.43 mm).
    #[test]
    fn the_panel_frame_reproduces_the_recorded_lonely_peak_numbers() {
        let law = law();
        let at_built = super::panel_sway_over_pixel(&law, 33.45, 0.5271, 16.3, 0.310, 1.217);
        let at_minimum = super::panel_sway_over_pixel(&law, 33.45, 0.5271, 16.3, 0.310, 2.43);
        assert!((at_built - 1.56).abs() < 0.01, "at 1.217 mm: {at_built}");
        assert!((at_minimum - 1.0).abs() < 0.01, "at 2.43 mm: {at_minimum}");
    }

    #[test]
    fn the_disabled_law_admits_every_run() {
        let law = SlendernessLaw::disabled();
        assert!(!law.binds());
        assert!(law.max_unbraced_mm(0.05).is_infinite());
    }

    fn plate_post(grid: &crate::grid::Grid, id: u32, col: usize, radius: f64) -> crate::SweepStrut {
        let row = 100;
        let cell = grid.index(row, col);
        crate::SweepStrut {
            id,
            cell,
            center_mm: [grid.center_x_mm(col), grid.center_y_mm(row)],
            kind: crate::StrutKind::Arm,
            min_radius_mm: 0.0,
            thickened_mm: Vec::new(),
            seat_on_strut: None,
            k_top: 80,
            k_bottom: 0,
            landing: crate::Landing::Plate,
            interior: false,
            radius_mm: vec![radius; 80],
            carried_mm2: vec![0.0; 80],
            covered_demand: Vec::new(),
            inserted_by_load: false,
            path: Vec::new(),
            seat_face: crate::SeatFace::default(),
            seat_normal: [0.0; 3],
            straight_fallback: None,
            fallback_path: Vec::new(),
            head: None,
        }
    }

    /// The lane's placed root discs stay equal to the feet the file draws
    /// when a cross is committed (review-fixes 2026-09-29), checked against
    /// the discs rebuilt from scratch by `congestion::root_discs`.
    ///
    /// **Re-pinned 2026-10-03 (rule R2)** from
    /// `a_cross_grows_its_posts_plate_disc_to_the_drawn_root`, which pinned
    /// the deleted junction raise: a 0.05 mm plate arm 0.50 mm from a 0.20 mm
    /// root took a 0.20 mm cross, was drawn at the brace's 0.20 mm root, and
    /// its disc grew to 0.32 mm and lapped its neighbour. A brace now raises
    /// nothing (the lane refuses a diagonal thicker than its post, and this
    /// plan, built by hand, bypasses that refusal on purpose), so the arm's
    /// drawn root, and its disc, stay at 0.05 mm: no growth, no lap.
    #[test]
    fn a_cross_leaves_its_posts_plate_disc_as_drawn() {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let history = crate::up_sweep::ColumnHistory::new(&grid);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let flare = 1.6;
        let context = super::SlendernessContext {
            grid: &grid,
            history: &history,
            keep_out: &keep_out,
            lean: &lean,
            law: &law,
            layer_height_mm: 0.05,
            layers: 100,
            flare_factor: flare,
            mast_root_clearance: true,
            final_clearance_mm: 0.3,
            member_clearance: true,
            model_surface: None,
            plate_z_mm: 0.0,
            shaft_landings: true,
            merge: true,
            brace: true,
            thicken: true,
            mast: true,
            max_struts: 1_000,
            x_allowed: 12.0,
            seat_faces: crate::tip_landing::SeatFaceRule::from_overhang_band(45.0, 25.0),
            load_step_taper: crate::adapter::LoadStepTaper::of(&lean),
            brace_cap: true,
            brace_window: true,
            frame_bands: true,
            frame_ledger: true,
            model_tie: true,
        };
        // P at col 100, Q 10 cells (0.50 mm) away, R 40 cells the other way.
        let mut struts = vec![
            plate_post(&grid, 0, 100, 0.05),
            plate_post(&grid, 1, 110, 0.20),
            plate_post(&grid, 2, 60, 0.20),
        ];
        let mut state = super::TieState::build(&context, &struts);
        let disc = |state: &super::TieState, id: u32| {
            let slot = crate::congestion::disc_slot(&state.placed_roots, id).expect("a disc");
            crate::congestion::disc_radius_mm(&state.placed_roots, slot).expect("a radius")
        };
        assert!((disc(&state, 0) - 0.05 * flare).abs() < 1e-12);
        let plan = super::CrossPlan {
            a: 0,
            b: 2,
            k: 40,
            k_lo: 20,
            a_top: struts[0].cell,
            b_top: struts[2].cell,
            radius_mm: 0.20,
            first: Vec::new(),
            second: Vec::new(),
            plane: None,
            volume_mm3: 0.0,
            widened: false,
            moved: false,
            moved_off_the_plate: false,
        };
        let mut outcome = super::SlendernessOutcome::default();
        super::commit_cross(&context, &mut state, &mut struts, plan, &mut outcome);

        assert!(
            (disc(&state, 0) - 0.05 * flare).abs() < 1e-12,
            "the braced post's disc is {} mm; the file draws its root at 0.05 mm, a {} mm foot",
            disc(&state, 0),
            0.05 * flare
        );
        assert_eq!(outcome.cross_roots_grown, 0, "no root grew");
        assert_eq!(outcome.cross_roots_lapping, 0, "no foot laps another");
        // The maintained discs equal the discs rebuilt from the field once the
        // down-sweep has sized the new diagonals (at their own minimum, as it
        // does for a brace no load reaches).
        for strut in &mut struts {
            if strut.kind == crate::StrutKind::Brace {
                strut.radius_mm = vec![strut.min_radius_mm; strut.k_top - strut.k_bottom];
            }
        }
        let rebuilt = crate::congestion::root_discs(&grid, &history, &struts, flare);
        for id in 0..3 {
            let slot = crate::congestion::disc_slot(&rebuilt, id).expect("a rebuilt disc");
            let rebuilt_radius =
                crate::congestion::disc_radius_mm(&rebuilt, slot).expect("a radius");
            assert!(
                (disc(&state, id) - rebuilt_radius).abs() < 1e-12,
                "post {id}: the lane holds {} mm, the field draws {rebuilt_radius} mm",
                disc(&state, id)
            );
        }
    }

    /// The lowest layer on the free shaft of an 80-layer (4.0 mm) plate post
    /// at 0.05 mm layers, HAND-DERIVED from the registered numbers: the raft
    /// is 0.2 mm, the run above it 3.8 mm, so the boss rises
    /// `max(0.5 − 0.2, min(CHITUBOX_BOSS_HEIGHT_MM 1.0, 3.8 / 2)) = 1.0` mm
    /// over the raft to 1.2 mm, which is layer 24's plane; the first layer
    /// strictly above it is 25. (35 until the sweep's boss became the owner's
    /// ChituBox `heightofboss`, 1.0 mm, in place of the legacy `FLARE_LEN_MM`
    /// 1.5 mm: fix/chitubox-geometry, 2026-09-30.)
    const FREE_SHAFT_FLOOR_OF_A_4_MM_POST: usize = 25;

    /// FREE SHAFT (fix/brace-not-in-flare, 2026-09-30): the floor the lane
    /// reads is the lowering's own boss top, and it is the hand-derived one.
    #[test]
    fn the_free_shaft_of_a_plate_post_starts_above_its_boss_top() {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let post = plate_post(&grid, 0, 100, 0.2);
        let floors = super::free_shaft_floors(&grid, &[post], 0.05);
        assert_eq!(floors, vec![FREE_SHAFT_FLOOR_OF_A_4_MM_POST]);
    }

    /// FREE SHAFT: a brace end inside a post's root flare is NOT a tie: the
    /// post's runs are cut where the same brace lands on its free shaft, and
    /// are not cut where it lands inside the flare.
    #[test]
    fn a_brace_end_inside_a_root_flare_is_not_credited_as_a_tie() {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let law = law();
        let cuts_at = |landing_layer: usize| {
            let mut struts = vec![
                plate_post(&grid, 0, 100, 0.2),
                plate_post(&grid, 1, 120, 0.2),
            ];
            let (seat, base) = (struts[0].cell, struts[1].cell);
            super::push_brace(
                &grid,
                &mut struts,
                seat,
                60,
                landing_layer,
                crate::Landing::Strut {
                    id: 1,
                    layer: landing_layer,
                },
                0,
                0.1,
                vec![u32::try_from(base).expect("a cell index")],
            );
            super::unbraced_runs(&grid, &struts, 0.05, &law)
                .iter()
                .filter(|run| run.strut_id == 1)
                .any(|run| run.k_lo == landing_layer || run.k_top == landing_layer)
        };
        assert!(
            cuts_at(FREE_SHAFT_FLOOR_OF_A_4_MM_POST + 5),
            "anti-vacuity: a brace landing on the free shaft must cut the post's run"
        );
        assert!(
            !cuts_at(20),
            "a brace landing at layer 20 (1.0 mm), inside the 1.2 mm flare, cut the post's run"
        );
        assert!(
            !cuts_at(FREE_SHAFT_FLOOR_OF_A_4_MM_POST - 1),
            "a brace landing on the boss top's own layer cut the post's run"
        );
    }

    /// FREE SHAFT: a band whose bottom would fall inside a flare moves up by
    /// the shortfall and is built with its bottom on the first free-shaft
    /// layer; where it may not move it is dropped; a band already clear is
    /// untouched. Two vertical 4.0 mm posts 1.0 mm apart.
    #[test]
    fn a_band_whose_bottom_falls_in_a_flare_moves_up_or_is_dropped() {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let history = crate::up_sweep::ColumnHistory::new(&grid);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let context = super::SlendernessContext {
            grid: &grid,
            history: &history,
            keep_out: &keep_out,
            lean: &lean,
            law: &law,
            layer_height_mm: 0.05,
            layers: 100,
            flare_factor: 1.6,
            mast_root_clearance: true,
            final_clearance_mm: 0.3,
            member_clearance: true,
            model_surface: None,
            plate_z_mm: 0.0,
            shaft_landings: true,
            merge: true,
            brace: true,
            thicken: true,
            mast: true,
            max_struts: 1_000,
            x_allowed: 12.0,
            seat_faces: crate::tip_landing::SeatFaceRule::from_overhang_band(45.0, 25.0),
            load_step_taper: crate::adapter::LoadStepTaper::of(&lean),
            brace_cap: true,
            brace_window: true,
            frame_bands: true,
            frame_ledger: true,
            model_tie: true,
        };
        let struts = vec![
            plate_post(&grid, 0, 100, 0.2),
            plate_post(&grid, 1, 120, 0.2),
        ];
        let state = super::TieState::build(&context, &struts);
        let run = UnbracedRun {
            strut_id: 0,
            k_lo: 0,
            k_top: 80,
            length_mm: 4.0,
            radius_mm: 0.2,
            limit_mm: 10.0,
            sway_deg: None,
        };
        let depth = lean.layers_to_reach(1.0);
        // Topped 8 layers short of a band that would just clear the flare
        // (40 when the boss was the legacy 1.5 mm and the floor 35; the
        // floor is now 25, fix/chitubox-geometry 2026-09-30).
        let low = FREE_SHAFT_FLOOR_OF_A_4_MM_POST + depth - 8;
        assert!(
            low - depth < FREE_SHAFT_FLOOR_OF_A_4_MM_POST,
            "anti-vacuity: a band topped at {low} is {depth} layers deep and must dip into the flare"
        );

        let mut outcome = super::SlendernessOutcome::default();
        let plan = super::plan_cross(
            &context,
            &state,
            &struts,
            0,
            1,
            low,
            &run,
            true,
            &mut outcome,
        )
        .expect("the band moves up and is built");
        assert_eq!(plan.k_lo, FREE_SHAFT_FLOOR_OF_A_4_MM_POST, "{plan:?}");
        assert_eq!(plan.k, FREE_SHAFT_FLOOR_OF_A_4_MM_POST + depth, "{plan:?}");
        // Path hit, carried by the plan: the counter is bumped when a band is
        // BUILT (`commit_cross`, 2026-10-03), so a band the brace window
        // refuses is not counted as moved.
        assert!(plan.moved && !plan.moved_off_the_plate, "{plan:?}");
        assert_eq!(outcome.brace_bands_dropped_in_flare, 0);

        let mut outcome = super::SlendernessOutcome::default();
        assert!(
            super::plan_cross(
                &context,
                &state,
                &struts,
                0,
                1,
                low,
                &run,
                false,
                &mut outcome
            )
            .is_none(),
            "a band held to its layer may not move, and is dropped"
        );
        assert_eq!(outcome.brace_bands_dropped_in_flare, 1);
        assert_eq!(outcome.brace_bands_moved_above_flare, 0);

        let high = FREE_SHAFT_FLOOR_OF_A_4_MM_POST + depth + 3;
        let mut outcome = super::SlendernessOutcome::default();
        let plan = super::plan_cross(
            &context,
            &state,
            &struts,
            0,
            1,
            high,
            &run,
            true,
            &mut outcome,
        )
        .expect("a band already on the free shafts is built where it was planned");
        assert_eq!((plan.k, plan.k_lo), (high, high - depth));
        assert_eq!(outcome.brace_bands_moved_above_flare, 0);
        assert_eq!(outcome.brace_bands_dropped_in_flare, 0);
    }

    /// The reviewer's SHORT post (round-6 review, 2026-09-30): a vertical
    /// plate arm seated at layer 61 (3.05 mm at 0.05 mm layers) with a 0.05 mm
    /// tip, over its 2.5 mm unbraced limit. Under the legacy 1.5 mm flare
    /// length its run above the raft was shorter than twice the flare, so its
    /// boss top was half-way up that run and moved with the seat. Under the
    /// owner's 1.0 mm boss (fix/chitubox-geometry, 2026-09-30) it stands on
    /// the full boss, top 1.2 mm, free shaft from layer 25
    /// ([`FREE_SHAFT_FLOOR_OF_THE_SHORT_POST`]); the seat-moved case is
    /// [`SEAT_MOVED_POST_TOP`]. No post that short needs a tie at all: a run
    /// under `2 × 1.0 + 0.2` mm is inside the 2.5 mm limit of the minimum
    /// 0.05 mm radius.
    const SHORT_POST_TOP: usize = 61;
    const SHORT_POST_TIP_MM: f64 = 0.05;
    /// The free-shaft floor of the 3.05 mm post on the owner's full 1.0 mm
    /// boss: `max(0.3, min(1.0, (3.119 − 0.2) / 2)) = 1.0` mm over the
    /// 0.2 mm raft, top 1.2 mm (layer 24's plane), first layer above it 25.
    const FREE_SHAFT_FLOOR_OF_THE_SHORT_POST: usize = 25;

    /// A post short enough for its SEAT to move its boss under the sweep's
    /// boss height, the owner's ChituBox `heightofboss` 1.0 mm
    /// (fix/chitubox-geometry, 2026-09-30): its run above the raft is under
    /// `2 × 1.0` mm, so its boss top is half-way up that run. Seated at layer
    /// 41 (2.05 mm), the reviewer's 3.05 mm post 1.0 mm lower, for the same
    /// 0.5 mm the boss height fell by twice. (The reviewer's 3.05 mm post now
    /// stands on the full 1.0 mm boss, whatever its seat depth.)
    const SEAT_MOVED_POST_TOP: usize = 41;

    /// The lowest layer on the free shaft of that seat-moved post,
    /// HAND-DERIVED from the registered numbers and the seat law the lowering
    /// applies: the tip is driven into the model by
    /// `max(layer + pixel = 0.05 + 0.019, layer = 0.05, ball term) = 0.069`
    /// mm (straight down, the unmeasured-normal fallback), so the terminal
    /// is 2.119 mm up; the boss rises `max(0.3, min(1.0, (2.119 − 0.2) / 2))
    /// = 0.9595` mm over the 0.2 mm raft, to 1.1595 mm. Layer 23's plane
    /// (1.15 mm) is INSIDE that flare; layer 24's (1.20 mm) is the first
    /// above it. Reading the boss at the unseated 2.05 mm gives 1.125 mm and
    /// layer 23, which is the defect. (Under the legacy 1.5 mm flare length
    /// this was the 3.05 mm post, boss top 1.6595 mm, floor 34.)
    const FREE_SHAFT_FLOOR_OF_THE_SEAT_MOVED_POST: usize = 24;

    fn short_plate_post(grid: &crate::grid::Grid, id: u32, col: usize) -> crate::SweepStrut {
        post_seated_at(grid, id, col, SHORT_POST_TOP)
    }

    fn post_seated_at(
        grid: &crate::grid::Grid,
        id: u32,
        col: usize,
        top: usize,
    ) -> crate::SweepStrut {
        let mut post = plate_post(grid, id, col, SHORT_POST_TIP_MM);
        post.k_top = top;
        post.radius_mm = vec![SHORT_POST_TIP_MM; top];
        post.carried_mm2 = vec![0.0; top];
        post
    }

    /// The boss top THE LOWERING DRAWS for one plate-rooted member, in mm over
    /// the plate: the `RootFlare` record's top, read from
    /// `plinth_support::lower_sweep_arms` over the arm the adapter builds for
    /// the file. Nothing here computes a seat or a boss.
    fn emitted_boss_top_mm(grid: &crate::grid::Grid, post: &crate::SweepStrut, h: f64) -> f64 {
        let (seat, base) = crate::adapter::drawn_ends(grid, 0.0, h, post);
        let (profiles, _) = crate::adapter::drawn_profiles(
            std::slice::from_ref(post),
            crate::adapter::LoadStepTaper::registered(h),
        );
        let arm = crate::adapter::sweep_arm(post, seat, base, None, profiles[0].clone());
        let layers = plinth_core::LayerPlan::new(0.0, vec![h; 200]).expect("a uniform plan");
        let frame = plinth_support::BuildFrame::new(
            plinth_core::Finite::new(0.0).expect("a finite plate"),
            plinth_core::PositiveFinite::new(5.0).expect("a positive lift"),
        );
        let lowering = plinth_support::lower_sweep_arms(&[arm], frame, &layers, 100_000, 1 << 30)
            .expect("the lowering refuses only about the request");
        let flares: Vec<f64> = lowering
            .primitives
            .iter()
            .filter(|primitive| {
                primitive.segment_kind == plinth_support::SupportSegmentKind::RootFlare
            })
            .map(|flare| flare.start_mm[2].get())
            .collect();
        assert_eq!(flares.len(), 1, "one plate post draws one root flare");
        flares[0]
    }

    /// ROUND-6 REVIEW (2026-09-30): the free shaft of a SHORT plate post
    /// starts above the boss top the LOWERING DRAWS, which it reads at the
    /// SEATED terminal. Graded against the emitted `RootFlare` record, and
    /// against the hand-derived layer.
    #[test]
    fn a_short_plate_posts_free_shaft_starts_above_the_boss_top_the_file_draws() {
        let h = 0.05;
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let post = post_seated_at(&grid, 0, 100, SEAT_MOVED_POST_TOP);
        let boss_mm = emitted_boss_top_mm(&grid, &post, h);
        let floor = super::free_shaft_floors(&grid, std::slice::from_ref(&post), h)[0];
        #[expect(
            clippy::cast_precision_loss,
            reason = "a layer index far below 2^52 is exact in f64"
        )]
        let plane = |layer: usize| layer as f64 * h;
        assert!(
            plane(floor) > boss_mm,
            "the free shaft starts at layer {floor} ({:.4} mm), inside the drawn flare whose boss \
             top is {boss_mm:.4} mm",
            plane(floor)
        );
        assert!(
            plane(floor - 1) <= boss_mm,
            "layer {} ({:.4} mm) is already above the drawn boss top {boss_mm:.4} mm",
            floor - 1,
            plane(floor - 1)
        );
        assert_eq!(
            floor, FREE_SHAFT_FLOOR_OF_THE_SEAT_MOVED_POST,
            "boss top {boss_mm:.4} mm"
        );
    }

    /// ROUND-6 REVIEW (2026-09-30), the reviewer's cross: two such short posts
    /// 0.8 mm apart. A band planned low dips into the flares and moves up;
    /// its lower ties must then stand above the boss top the file draws for
    /// BOTH posts, and the band must still fit inside them.
    #[test]
    fn a_cross_between_short_posts_ties_them_above_the_flares_the_file_draws() {
        let h = 0.05;
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let history = crate::up_sweep::ColumnHistory::new(&grid);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let context = super::SlendernessContext {
            grid: &grid,
            history: &history,
            keep_out: &keep_out,
            lean: &lean,
            law: &law,
            layer_height_mm: h,
            layers: 100,
            flare_factor: 1.6,
            mast_root_clearance: true,
            final_clearance_mm: 0.3,
            member_clearance: true,
            model_surface: None,
            plate_z_mm: 0.0,
            shaft_landings: true,
            merge: true,
            brace: true,
            thicken: true,
            mast: true,
            max_struts: 1_000,
            x_allowed: 12.0,
            seat_faces: crate::tip_landing::SeatFaceRule::from_overhang_band(45.0, 25.0),
            load_step_taper: crate::adapter::LoadStepTaper::of(&lean),
            brace_cap: true,
            brace_window: true,
            frame_bands: true,
            frame_ledger: true,
            model_tie: true,
        };
        // 16 cells at 0.05 mm: 0.8 mm between the posts.
        let struts = vec![
            short_plate_post(&grid, 0, 100),
            short_plate_post(&grid, 1, 116),
        ];
        let state = super::TieState::build(&context, &struts);
        let run = UnbracedRun {
            strut_id: 0,
            k_lo: 0,
            k_top: SHORT_POST_TOP,
            length_mm: 3.05,
            radius_mm: SHORT_POST_TIP_MM,
            limit_mm: 2.5,
            sway_deg: None,
        };
        let depth = lean.layers_to_reach(0.8);
        let low = 30;
        assert!(
            low - depth < FREE_SHAFT_FLOOR_OF_THE_SHORT_POST
                && FREE_SHAFT_FLOOR_OF_THE_SHORT_POST + depth <= SHORT_POST_TOP,
            "anti-vacuity: a band {depth} layers deep topped at {low} must dip into the flares and \
             fit inside the posts once raised"
        );
        let mut outcome = super::SlendernessOutcome::default();
        let plan = super::plan_cross(
            &context,
            &state,
            &struts,
            0,
            1,
            low,
            &run,
            true,
            &mut outcome,
        )
        .expect("the band moves up and is built");
        // Counted when built (`commit_cross`); the plan carries the hit.
        assert!(
            plan.moved && !plan.moved_off_the_plate,
            "path hit: {plan:?}"
        );
        #[expect(
            clippy::cast_precision_loss,
            reason = "a layer index far below 2^52 is exact in f64"
        )]
        let tie_mm = plan.k_lo as f64 * h;
        for post in &struts {
            let boss_mm = emitted_boss_top_mm(&grid, post, h);
            assert!(
                tie_mm > boss_mm,
                "the band's lower ties at layer {} ({tie_mm:.4} mm) lie inside post {}'s drawn \
                 flare, whose boss top is {boss_mm:.4} mm",
                plan.k_lo,
                post.id
            );
        }
        assert!(
            plan.k <= SHORT_POST_TOP,
            "the raised band passes the posts' tops: {plan:?}"
        );
    }

    /// ROUND-6 REVIEW (2026-09-30), THE FIELD AS THE FILE HAS IT: two short
    /// 0.05 mm posts 0.8 mm apart (each 3.05 mm, over its 2.5 mm limit) are
    /// tied by the production pass, [`super::apply_ties`], and the whole
    /// field is then lowered by `plinth_support::lower_sweep_arms` over the
    /// arms the adapter builds. Every brace end, at the point the adapter
    /// draws it, must lie outside every `RootFlare` record the lowering
    /// emits. The flares come from the file, never from the rule.
    #[test]
    fn tying_two_short_posts_leaves_no_brace_end_inside_a_drawn_flare() {
        let h = 0.05;
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let history = crate::up_sweep::ColumnHistory::new(&grid);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let context = super::SlendernessContext {
            grid: &grid,
            history: &history,
            keep_out: &keep_out,
            lean: &lean,
            law: &law,
            layer_height_mm: h,
            layers: 100,
            flare_factor: 1.6,
            mast_root_clearance: true,
            final_clearance_mm: 0.3,
            member_clearance: true,
            model_surface: None,
            plate_z_mm: 0.0,
            shaft_landings: true,
            merge: true,
            brace: true,
            // The brace rung alone: a thickened post needs no tie, and a mast
            // is a third post; neither is the path graded here.
            thicken: false,
            mast: false,
            max_struts: 1_000,
            x_allowed: 12.0,
            seat_faces: crate::tip_landing::SeatFaceRule::from_overhang_band(45.0, 25.0),
            load_step_taper: crate::adapter::LoadStepTaper::of(&lean),
            brace_cap: true,
            brace_window: true,
            frame_bands: true,
            frame_ledger: true,
            model_tie: true,
        };
        let mut struts = vec![
            short_plate_post(&grid, 0, 100),
            short_plate_post(&grid, 1, 116),
        ];
        let mut outcome = super::SlendernessOutcome::default();
        let applied = super::apply_ties(&context, &mut struts, &mut outcome);
        let braces: Vec<&crate::SweepStrut> = struts
            .iter()
            .filter(|strut| strut.kind == crate::StrutKind::Brace && super::live(strut))
            .collect();
        assert!(
            applied > 0 && !braces.is_empty(),
            "anti-vacuity: the pass tied nothing ({applied} applied, {outcome:?})"
        );

        // The whole field, lowered exactly as `to_primitives` lowers a plan.
        let (profiles, _) = crate::adapter::drawn_profiles(&struts, context.load_step_taper);
        let arms: Vec<plinth_support::SweepArm> = struts
            .iter()
            .zip(profiles)
            .map(|(strut, profile)| {
                let (seat, base) = crate::adapter::drawn_ends(&grid, 0.0, h, strut);
                crate::adapter::sweep_arm(strut, seat, base, None, profile)
            })
            .collect();
        let layers = plinth_core::LayerPlan::new(0.0, vec![h; 200]).expect("a uniform plan");
        let frame = plinth_support::BuildFrame::new(
            plinth_core::Finite::new(0.0).expect("a finite plate"),
            plinth_core::PositiveFinite::new(5.0).expect("a positive lift"),
        );
        let lowering = plinth_support::lower_sweep_arms(&arms, frame, &layers, 100_000, 1 << 30)
            .expect("the lowering refuses only about the request");
        // (boss top, foot, top radius, foot radius) of every drawn flare.
        let flares: Vec<([f64; 3], [f64; 3], f64, f64)> = lowering
            .primitives
            .iter()
            .filter(|primitive| {
                primitive.segment_kind == plinth_support::SupportSegmentKind::RootFlare
            })
            .map(|flare| {
                (
                    flare.start_mm.map(plinth_core::Finite::get),
                    flare.end_mm.map(plinth_core::Finite::get),
                    flare.start_radius_mm.get(),
                    flare.end_radius_mm.get(),
                )
            })
            .collect();
        assert_eq!(flares.len(), 2, "anti-vacuity: both posts draw a flare");
        // Inside a frustum: between its foot and its top, and inside its
        // radius at that height (linear from the foot to the top).
        let inside = |point: [f64; 3]| {
            flares.iter().find(|(top, foot, top_r, foot_r)| {
                if point[2] < foot[2] || point[2] > top[2] {
                    return false;
                }
                let t = (point[2] - foot[2]) / (top[2] - foot[2]);
                let axis = [
                    foot[0] + t * (top[0] - foot[0]),
                    foot[1] + t * (top[1] - foot[1]),
                ];
                (point[0] - axis[0]).hypot(point[1] - axis[1]) < foot_r + t * (top_r - foot_r)
            })
        };
        let mut found: Vec<String> = Vec::new();
        for brace in &braces {
            let (seat, base) = crate::adapter::drawn_ends(&grid, 0.0, h, brace);
            for (end, point) in [("seat", seat), ("landing", base)] {
                if let Some((top, ..)) = inside(point) {
                    found.push(format!(
                        "brace {} {end} at {:.4} mm, inside a flare whose boss top is {:.4} mm",
                        brace.id, point[2], top[2]
                    ));
                }
            }
        }
        println!(
            "two short posts: {} braces, bands moved up {}, dropped {}; {} brace ends inside a \
             drawn flare",
            braces.len(),
            outcome.brace_bands_moved_above_flare,
            outcome.brace_bands_dropped_in_flare,
            found.len()
        );
        assert!(found.is_empty(), "{}", found.join("\n"));
    }

    /// The runs of post 0 (a 4.0 mm plate post) when a branch lands on it at
    /// layer 30 and SEATS at `branch_top`, as `(k_lo, k_top)` pairs, sorted.
    fn host_runs_under_a_branch(branch_top: usize) -> Vec<(usize, usize)> {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let host = plate_post(&grid, 0, 100, 0.2);
        let mut branch = plate_post(&grid, 1, 120, 0.2);
        branch.landing = crate::Landing::Strut { id: 0, layer: 30 };
        branch.k_bottom = 30;
        branch.k_top = branch_top;
        branch.radius_mm = vec![0.2; branch_top - 30];
        branch.carried_mm2 = vec![0.0; branch_top - 30];
        let mut runs: Vec<(usize, usize)> = super::unbraced_runs(&grid, &[host, branch], 0.05, &law())
            .iter()
            .filter(|run| run.strut_id == 0)
            .map(|run| (run.k_lo, run.k_top))
            .collect();
        runs.sort_unstable();
        runs.dedup();
        runs
    }

    /// TIE TIMING (Craig ruling 2026-10-01): a branch that lands on a host
    /// holds the host sideways only once the branch is held at ITS top, the
    /// layer its seat prints. Until then the branch is a free cantilever off
    /// the host and the host runs from its last tie below up to the layer
    /// being printed. A host carrying a branch that lands at layer 30 and
    /// seats at layer 70 is therefore measured over 0..70 (printed before the
    /// seat holds) and 30..80 (after it); the landing layer alone ends no run.
    /// A branch that seats ABOVE the host's own top never holds it while the
    /// host prints: one run, 0..80.
    #[test]
    fn a_landing_branch_ties_its_host_only_once_its_seat_has_printed() {
        assert_eq!(
            host_runs_under_a_branch(70),
            vec![(0, 70), (30, 80)],
            "a branch landing at 30 and seating at 70"
        );
        assert_eq!(
            host_runs_under_a_branch(95),
            vec![(0, 80)],
            "a branch landing at 30 and seating at 95, above the host's own top at 80"
        );
    }

    /// TIE TIMING for a cross: each diagonal lands on one post at the band
    /// bottom and seats on the other at the band top, so the `|X|` holds
    /// either post only once its top has printed. Two 4.0 mm posts with a
    /// cross over 30..60 (its bottom on the free shaft, above the 1.2 mm
    /// boss top): in the cross's plane each post runs 0..60 and 60..80; the
    /// band bottom ends no run (until 2026-10-01: 0..30, 30..60, 60..80).
    #[test]
    fn a_cross_ties_its_posts_from_its_band_top() {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let mut struts = vec![
            plate_post(&grid, 0, 100, 0.2),
            plate_post(&grid, 1, 140, 0.2),
        ];
        let (a, b) = (struts[0].cell, struts[1].cell);
        for (seat_cell, seat_on, lands_on, base) in [(a, 0, 1, b), (b, 1, 0, a)] {
            super::push_brace(
                &grid,
                &mut struts,
                seat_cell,
                60,
                30,
                crate::Landing::Strut {
                    id: lands_on,
                    layer: 30,
                },
                seat_on,
                0.1,
                vec![u32::try_from(base).expect("a cell index"); 30],
            );
        }
        for post in 0..2_u32 {
            let mut runs: Vec<(usize, usize)> = super::unbraced_runs(&grid, &struts, 0.05, &law())
                .iter()
                .filter(|run| run.strut_id == post)
                .map(|run| (run.k_lo, run.k_top))
                .collect();
            runs.sort_unstable();
            runs.dedup();
            assert!(
                runs.iter().all(|&(lo, top)| lo != 30 && top != 30),
                "post {post}: the band bottom cut a run: {runs:?}"
            );
            assert!(
                runs.contains(&(0, 60)) && runs.contains(&(60, 80)),
                "post {post}: {runs:?}"
            );
        }
    }

    /// REFUSALS THAT WERE NOT GEOMETRY (Craig ruling 2026-10-01, item 4). On
    /// two 4.0 mm posts 1.0 mm apart, two bands the lane used to refuse are
    /// built:
    /// * planned with its top lower than its own depth (the band "below the
    ///   plate"): moved up until its bottom is on the free shafts;
    /// * between posts that lean apart, where the depth sized from the gap at
    ///   the top leaves a diagonal "too flat": deepened until both fit.
    ///
    /// Red until 2026-10-01: refused as below the plate, and too flat.
    ///
    /// A band below a BRANCH partner's foot stays refused (`off_a_post`):
    /// moved onto the foot, its diagonal ends inside the trunk the branch
    /// lands on (measured on the stilt plate, 0.724 mm deep).
    #[test]
    fn a_band_that_fits_higher_or_deeper_is_built_not_refused() {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let history = crate::up_sweep::ColumnHistory::new(&grid);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let context = prune_context(&grid, &history, &keep_out, &lean, &law);
        let run = UnbracedRun {
            strut_id: 0,
            k_lo: 0,
            k_top: 80,
            length_mm: 4.0,
            radius_mm: 0.2,
            limit_mm: 10.0,
            sway_deg: None,
        };
        let depth = lean.layers_to_reach(1.0);
        let plan = |struts: &[crate::SweepStrut], k: usize| {
            let state = super::TieState::build(&context, struts);
            let mut outcome = super::SlendernessOutcome::default();
            let plan = super::plan_cross(&context, &state, struts, 0, 1, k, &run, true, &mut outcome);
            (plan, outcome)
        };

        // Below the plate.
        let posts = vec![
            plate_post(&grid, 0, 100, 0.2),
            plate_post(&grid, 1, 120, 0.2),
        ];
        assert!(depth > 2, "anti-vacuity: a 1.0 mm band is {depth} layers deep");
        let (built, outcome) = plan(&posts, depth - 2);
        let built = built.unwrap_or_else(|| panic!("refused: {:?}", outcome.brace_band_refusals));
        assert_eq!(built.k_lo, FREE_SHAFT_FLOOR_OF_A_4_MM_POST, "{built:?}");
        // Counted when built (`commit_cross`); the plan carries the hit.
        assert!(built.moved_off_the_plate, "{built:?}");

        // Below a branch partner's foot: post 1 lands on post 2 at layer 50.
        let mut posts = vec![
            plate_post(&grid, 0, 100, 0.2),
            plate_post(&grid, 1, 120, 0.2),
            plate_post(&grid, 2, 140, 0.2),
        ];
        posts[1].landing = crate::Landing::Strut { id: 2, layer: 50 };
        posts[1].k_bottom = 50;
        posts[1].radius_mm = vec![0.2; 30];
        posts[1].carried_mm2 = vec![0.0; 30];
        let k = 50 + depth - 5;
        assert!(k + 1 < run.k_top && k - depth >= FREE_SHAFT_FLOOR_OF_A_4_MM_POST);
        let (built, outcome) = plan(&posts, k);
        assert!(built.is_none(), "a band below a branch's foot was built: {built:?}");
        assert_eq!(outcome.brace_band_refusals.off_a_post, 1, "{:?}", outcome.brace_band_refusals);

        // Leaning apart: post 1's axis steps one cell outward every 4 layers
        // down, so a diagonal from post 0's top to post 1's bottom moves
        // further than the gap at the top.
        let mut posts = vec![
            plate_post(&grid, 0, 100, 0.2),
            plate_post(&grid, 1, 120, 0.2),
        ];
        posts[1].path = (0..80_usize)
            .map(|layer| {
                u32::try_from(grid.index(100, 120 + (80 - layer) / 4)).expect("a cell index")
            })
            .collect();
        let k = 70;
        let (built, outcome) = plan(&posts, k);
        let built = built.unwrap_or_else(|| panic!("refused: {:?}", outcome.brace_band_refusals));
        let lateral = crate::cell_distance_mm(&grid, posts[0].cell_at(k), posts[1].cell_at(built.k_lo));
        assert!(
            built.k - built.k_lo >= lean.layers_to_reach(lateral),
            "the band is {} layers deep for a {lateral} mm diagonal: {built:?}",
            built.k - built.k_lo
        );
        // Counted when built (`commit_cross`); the plan carries the hit.
        assert!(built.widened, "{built:?}");
    }

    /// A band that ALREADY fits is as bound by a headed post's joinable top
    /// as a raised one: a straight post 0 and a headed post 1 (k_top 200,
    /// joint 190, joinable 185). A band topped at 190 is refused (`off_a_post`),
    /// one topped at 180 is built. SABOTAGE: drop the `k > top_limit` refusal
    /// in `plan_cross` and the band at 190 is built, seated on the joint.
    #[test]
    fn a_band_that_fits_is_refused_above_a_headed_posts_joinable_top() {
        let grid = crate::head_carrier_tests::grid();
        let history = crate::up_sweep::ColumnHistory::new(&grid);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let context = prune_context(&grid, &history, &keep_out, &lean, &law);
        let posts = vec![
            crate::head_carrier_tests::straight(&grid, 0, 150, 100, 400, 0.2),
            crate::head_carrier_tests::headed(&grid, 1, 150, 120, 200, 190, 0.2),
        ];
        let run = UnbracedRun {
            strut_id: 0,
            k_lo: 0,
            k_top: 399,
            length_mm: 20.0,
            radius_mm: 0.2,
            limit_mm: 10.0,
            sway_deg: None,
        };
        let state = super::TieState::build(&context, &posts);
        let try_at = |k: usize| {
            let mut outcome = super::SlendernessOutcome::default();
            let plan = super::plan_cross(&context, &state, &posts, 0, 1, k, &run, true, &mut outcome);
            (plan, outcome)
        };
        let (low, outcome) = try_at(180);
        assert!(low.is_some(), "control: a band below the joinable top is built: {outcome:?}");
        let (high, outcome) = try_at(190);
        assert!(high.is_none(), "a band on the joint is refused: {high:?}");
        assert_eq!(outcome.brace_band_refusals.off_a_post, 1, "{outcome:?}");
    }

    fn prune_context<'a>(
        grid: &'a crate::grid::Grid,
        history: &'a crate::up_sweep::ColumnHistory,
        keep_out: &'a crate::drain::DrainKeepOut,
        lean: &'a crate::lean::LeanLaw,
        law: &'a SlendernessLaw,
    ) -> super::SlendernessContext<'a> {
        super::SlendernessContext {
            grid,
            history,
            keep_out,
            lean,
            law,
            layer_height_mm: 0.05,
            layers: 100,
            flare_factor: 1.6,
            mast_root_clearance: true,
            final_clearance_mm: 0.3,
            member_clearance: true,
            model_surface: None,
            plate_z_mm: 0.0,
            shaft_landings: true,
            merge: true,
            brace: true,
            thicken: true,
            mast: true,
            max_struts: 1_000,
            x_allowed: 12.0,
            seat_faces: crate::tip_landing::SeatFaceRule::from_overhang_band(45.0, 25.0),
            load_step_taper: crate::adapter::LoadStepTaper::of(lean),
            brace_cap: true,
            brace_window: true,
            frame_bands: true,
            frame_ledger: true,
            model_tie: true,
        }
    }

    /// Two 4.0 mm plate posts 2.0 mm apart at `radius`, one `|X|` between
    /// them over layers 20..40 built in this pass; when `thicken` is set, each
    /// post's remaining over-limit run (the direction straight across the
    /// cross's plane) is then THICKENED by the production rung. Returns the
    /// prune's count, and the members left.
    fn prune_after(radius: f64, thicken: bool) -> (usize, usize, usize) {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let history = crate::up_sweep::ColumnHistory::new(&grid);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let context = prune_context(&grid, &history, &keep_out, &lean, &law);
        let mut struts = vec![
            plate_post(&grid, 0, 100, radius),
            plate_post(&grid, 1, 140, radius),
        ];
        let mut state = super::TieState::build(&context, &struts);
        let first_new = struts.len();
        // Band 20..40: printed from its top, the cross cuts each post at 40,
        // into 0..40 and 40..80, 2.0 mm each against a 2.5 mm limit.
        let plan = super::CrossPlan {
            a: 0,
            b: 1,
            k: 40,
            k_lo: 20,
            a_top: struts[0].cell,
            b_top: struts[1].cell,
            radius_mm: radius,
            first: Vec::new(),
            second: Vec::new(),
            plane: super::plane_between(&grid, struts[0].cell, struts[1].cell),
            volume_mm3: 0.0,
            widened: false,
            moved: false,
            moved_off_the_plate: false,
        };
        let mut outcome = super::SlendernessOutcome::default();
        super::commit_cross(&context, &mut state, &mut struts, plan, &mut outcome);
        outcome.braces_added += 1;
        let mut thickened = 0;
        if thicken {
            for post in 0..2_usize {
                let runs = super::member_runs(
                    &grid,
                    &struts[post],
                    &state.ties[post],
                    &state.profiles[post],
                    0.05,
                    &law,
                );
                let over: Vec<UnbracedRun> =
                    runs.into_iter().filter(UnbracedRun::over_limit).collect();
                assert_eq!(
                    over.len(),
                    1,
                    "anti-vacuity: post {post} is over only straight across the cross: {over:?}"
                );
                assert!(
                    super::thicken(&context, &mut state, &mut struts, &over[0], &mut outcome),
                    "anti-vacuity: post {post} takes the thicken rung"
                );
                thickened += 1;
            }
        }
        let taken =
            super::prune_unneeded_crosses(&context, &state, &mut struts, first_new, &mut outcome);
        (taken, struts.len(), thickened)
    }

    /// KEEP ONE-DIRECTION CROSSES (Craig ruling 2026-10-01): a cross that
    /// ties two columns holds the sway in its own plane; thickening (or a
    /// mast) covers the direction across it. Thickening the posts for that
    /// other direction does NOT make the cross unneeded, so the prune keeps
    /// it. Red until 2026-10-01: the prune judged the cross on the THICKENED
    /// posts, found both inside the limit in every direction without it, and
    /// took it out (the brace pair's 2 built, 2 pruned, 0 kept).
    #[test]
    fn a_one_plane_cross_is_kept_when_thickening_holds_the_other_direction() {
        let (taken, left, thickened) = prune_after(0.05, true);
        assert_eq!(thickened, 2, "anti-vacuity: both posts thickened");
        assert_eq!(taken, 0, "the prune took out a cross that holds its own plane");
        assert_eq!(left, 4, "both posts and both diagonals stand");
    }

    /// The prune still takes out a cross that NOTHING needs: posts already
    /// inside their limit in every direction (0.2 mm, 4.0 mm against 10 mm)
    /// with no thickening at all. The guard that the rule above did not switch
    /// the prune off.
    #[test]
    fn a_cross_no_post_needs_is_still_taken_out() {
        let (taken, left, _) = prune_after(0.2, false);
        assert_eq!(taken, 1, "a cross on two posts inside their limits was kept");
        assert_eq!(left, 2, "only the two posts stand");
    }

    /// RAISE THE ROOT, KEEP THE TIP (owner ruling 2026-09-29, quoted on
    /// [`crate::SweepStrut::thickened_mm`]). One 4.0 mm plate post at 0.05 mm
    /// (limit 2.5 mm at S = 50), untied, takes the thicken rung over its whole
    /// run `0..80`. The minimum the rung writes at the run's top layer (79,
    /// the tip) must be the tip as drawn, and the root must be raised.
    ///
    /// Red until 2026-10-01 (round-10 milestone): the taper reached the kept
    /// top one layer ABOVE the run, so the tip layer was written
    /// `root + (top − root)·(n−1)/n` — above the tip. On the overhang table at
    /// X 12 and S 30 that raised arm 19's tip 0.38969 → 0.39087 mm, and
    /// `x_allowed_ruling::the_same_field_at_x_twelve_has_one_and_a_half_times_the_tip_section`
    /// read 1.509 where the law gives exactly 1.5.
    #[test]
    fn the_thicken_rung_raises_the_root_and_keeps_the_tip() {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let history = crate::up_sweep::ColumnHistory::new(&grid);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let context = prune_context(&grid, &history, &keep_out, &lean, &law);
        let tip = 0.05;
        let mut struts = vec![plate_post(&grid, 0, 100, tip)];
        let mut state = super::TieState::build(&context, &struts);
        let runs = super::member_runs(
            &grid,
            &struts[0],
            &state.ties[0],
            &state.profiles[0],
            0.05,
            &law,
        );
        let over: Vec<UnbracedRun> = runs.into_iter().filter(UnbracedRun::over_limit).collect();
        assert_eq!(
            over.len(),
            1,
            "anti-vacuity: the untied post is one over-limit run: {over:?}"
        );
        assert_eq!(
            (over[0].k_lo, over[0].k_top),
            (0, 80),
            "the run is the whole post"
        );
        let mut outcome = super::SlendernessOutcome::default();
        assert!(
            super::thicken(&context, &mut state, &mut struts, &over[0], &mut outcome),
            "anti-vacuity: the post takes the thicken rung"
        );
        let thickened = &struts[0].thickened_mm;
        assert_eq!(
            thickened.len(),
            80,
            "the rung writes every layer of the run"
        );
        assert!(
            thickened[0] > tip,
            "the root is raised: {} mm at layer 0 against a {tip} mm tip",
            thickened[0]
        );
        assert!(
            thickened[79] <= tip,
            "the tip is kept: the rung wrote {} mm at the tip layer of a {tip} mm tip",
            thickened[79]
        );
        assert!(
            thickened.windows(2).all(|pair| pair[1] <= pair[0]),
            "the taper never widens upward"
        );
    }

    /// **THICKENING MAY NOT PUSH A MEMBER INTO THE FINAL GAP** (R2,
    /// `clearance-census-diagnosis.md`, 2026-10-04): the overhang table's post
    /// 30 stood 0.325 mm from a leg face and the rung raised its root to 0.77
    /// mm, 0.374 mm inside the leg.
    ///
    /// A 4 mm plate post over its limit with a cured wall 0.4 mm from its
    /// axis over layers 0..=60 (its seat is at layer 80). At its drawn radius
    /// 0.05 mm it clears the 0.3 mm gap (0.35 + the raster's 0.035 mm < 0.4);
    /// the root the law asks for does not. Red before the rule: the rung
    /// thickened. Green: refused, counted `thicken_refused_model`, the post
    /// left as drawn. SABOTAGE: with `member_clearance` off it thickens again.
    #[test]
    fn thickening_refuses_a_root_that_would_enter_the_gap() {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let mut history = crate::up_sweep::ColumnHistory::new(&grid);
        for row in 90..=110 {
            let cell = grid.index(row, 108);
            history.model_head[cell] = u32::try_from(history.model_spans.len()).expect("a few spans");
            history.model_spans.push(crate::up_sweep::ShaftSpan {
                k_bottom: 0,
                k_top: 60,
                id: 0,
                next: u32::MAX,
                landable_end: u32::MAX,
            });
        }
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let attempt = |member_clearance: bool| {
            let context = super::SlendernessContext {
                member_clearance,
                ..prune_context(&grid, &history, &keep_out, &lean, &law)
            };
            let mut struts = vec![plate_post(&grid, 0, 100, 0.05)];
            let mut state = super::TieState::build(&context, &struts);
            let over: Vec<UnbracedRun> = super::member_runs(
                &grid,
                &struts[0],
                &state.ties[0],
                &state.profiles[0],
                0.05,
                &law,
            )
            .into_iter()
            .filter(UnbracedRun::over_limit)
            .collect();
            assert_eq!(over.len(), 1, "anti-vacuity: the untied post is one over-limit run");
            let mut outcome = super::SlendernessOutcome::default();
            let done = super::thicken(&context, &mut state, &mut struts, &over[0], &mut outcome);
            (done, outcome, struts)
        };
        let (done, outcome, struts) = attempt(true);
        assert!(!done, "the rung would push the post into the wall and must refuse");
        assert_eq!(outcome.thicken_refused_model, 1, "{outcome:?}");
        assert!(struts[0].thickened_mm.is_empty(), "the post is left as drawn");
        let (done, outcome, struts) = attempt(false);
        assert!(done, "SABOTAGE: without the clearance the rung thickens the post");
        assert_eq!(outcome.thicken_refused_model, 0);
        assert!(struts[0].thickened_mm[0] > 0.065, "into the wall: {}", struts[0].thickened_mm[0]);
    }

    /// **THE REFUSAL COMPARES THE PROFILES AT THE PLACES THEY DIFFER**
    /// (challenger round 2, 2026-10-04). A 4 mm plate post drawn 0.5 mm over
    /// its lowest 10 layers (thick where it carries, far from any model) and
    /// 0.06 mm above, and a cured wall 0.45 mm from its axis beside layers
    /// 30..=40 only. The plan raises the root to 0.3 mm and tapers it to the tip
    /// over the whole post, so at layers 30..=40 it writes about 0.2 mm: the
    /// raised solid enters the 0.3 mm gap (0.2 + 0.3 + 0.035 > 0.45) where the
    /// drawn 0.06 mm solid (0.395 mm) clears. The first reading drew the
    /// piece's WIDEST radius (0.5 mm, from the thick foot) everywhere to
    /// call "now", which already intersects the wall, and its `widest <= now`
    /// shortcut skipped the piece outright. Red before the fix: admitted.
    /// Green: refused. The control is the same plan with the wall 0.9 mm away
    /// (admitted), and the second wall, an EXISTING violation beside the thick
    /// foot, does not excuse the new intrusion above it.
    #[test]
    fn a_thin_stretch_raised_into_the_gap_is_refused_whatever_the_rest_of_the_piece_is() {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let wall_at = |history: &mut crate::up_sweep::ColumnHistory, col: usize, k0: u32, k1: u32| {
            for row in 90..=110 {
                let cell = grid.index(row, col);
                history.model_head[cell] = u32::try_from(history.model_spans.len()).expect("a few spans");
                history.model_spans.push(crate::up_sweep::ShaftSpan {
                    k_bottom: k0,
                    k_top: k1,
                    id: 0,
                    next: u32::MAX,
                    landable_end: u32::MAX,
                });
            }
        };
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let enters = |history: &crate::up_sweep::ColumnHistory| {
            let context = prune_context(&grid, history, &keep_out, &lean, &law);
            let mut post = plate_post(&grid, 0, 100, 0.06);
            post.radius_mm = (0..80).map(|layer| if layer < 10 { 0.5 } else { 0.06 }).collect();
            let struts = vec![post];
            let state = super::TieState::build(&context, &struts);
            let plan = super::ThickenPlan {
                k_lo: 0,
                k_top: 80,
                root: 0.3,
                top: 0.06,
                steps: 80.0,
                chain: vec![(0, 79)],
                disc_growth: None,
                over_existing_lap: false,
                admitted_on_drawn_feet: false,
                added_mm3: 0.0,
            };
            super::thickening_enters_the_gap(&context, &state, &struts, &plan)
        };
        let mut near = crate::up_sweep::ColumnHistory::new(&grid);
        wall_at(&mut near, 109, 30, 40);
        assert!(enters(&near), "the raised 0.2 mm solid enters a wall 0.45 mm away");
        let mut far = crate::up_sweep::ColumnHistory::new(&grid);
        wall_at(&mut far, 118, 30, 40);
        assert!(!enters(&far), "control: a wall 0.9 mm away is not entered");
        let mut both = crate::up_sweep::ColumnHistory::new(&grid);
        wall_at(&mut both, 107, 0, 8); // 0.35 mm beside the thick foot: already inside its gap
        wall_at(&mut both, 109, 30, 40);
        assert!(enters(&both), "an existing violation elsewhere excuses nothing here");
    }

    /// Cure, in `history`, every cell whose centre lies in the box
    /// `low..=high` (x, y), over the layers that box spans in z.
    fn cure_box(
        grid: &crate::grid::Grid,
        history: &mut crate::up_sweep::ColumnHistory,
        low: [f64; 3],
        high: [f64; 3],
    ) {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a small non-negative height over the layer height, in whole layers"
        )]
        let (k0, k1) = ((low[2] / 0.05).round() as u32, (high[2] / 0.05).round() as u32 - 1);
        for cell in 0..grid.cells() {
            let (x, y) = (grid.center_x_mm(grid.col_of(cell)), grid.center_y_mm(grid.row_of(cell)));
            if x < low[0] - 1.0e-9 || x > high[0] + 1.0e-9 || y < low[1] - 1.0e-9 || y > high[1] + 1.0e-9 {
                continue;
            }
            let slot = u32::try_from(history.model_spans.len()).expect("a few spans");
            history.model_spans.push(crate::up_sweep::ShaftSpan {
                k_bottom: k0,
                k_top: k1,
                id: 0,
                next: history.model_head[cell],
                landable_end: u32::MAX,
            });
            history.model_head[cell] = slot;
        }
    }

    /// **A TRUNK RAISED WITH A BRANCH IS EXCUSED ONLY ITS OWN SEAT ZONE**
    /// (challenger round 3, 2026-10-05, finding 1). A 0.6 mm plate trunk of
    /// radius 0.05 mm under a flat seat (a slab's underside at z = 0.6), raised
    /// to 0.15 mm as a TRUNK of a branch's plan (chain position 1), with a
    /// foreign wall 0.4 mm from its axis over its lowest 0.2 mm. Drawn now it
    /// clears the wall by 0.35 mm; raised, by 0.25 mm: inside the 0.3 mm gap.
    /// The flat seat's own zone at 0.15 mm is `gap + r` = 0.45 mm, so the
    /// lowest 0.15 mm is read and the raise is refused. SABOTAGE INSIDE: the
    /// retired zone, `zone_factor · (r + gap)` = 0.84 mm, covers the whole
    /// 0.6 mm trunk, so that read exempted all of it (the raster finds
    /// nothing outside it). CONTROL: the wall 0.85 mm away is not entered.
    ///
    /// **WITHOUT A SURFACE** (challenger round 5, 2026-10-05): the same slab,
    /// wall, raster and plan with `model_surface: None` (production reaches
    /// this when `head_join::build_surface` passes its memory cap or fails).
    /// Red before the fix: no face meant the longest zone, the trunk was
    /// exempt whole, nothing was flagged, and the raise was ADMITTED before
    /// the no-surface refusal could run. Green: an unknown face exempts
    /// nothing, so the raster flags the raised trunk and the refusal runs
    /// (path hit: flagged slots). Without a surface nothing is provable, so
    /// the 0.85 mm wall's raise is refused too (the trunk's own seat contact
    /// is flagged): conservative, and counted as a refusal.
    #[test]
    fn a_short_trunk_raised_with_a_branch_is_read_below_its_flat_seats_own_zone() {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let (cx, cy) = (grid.center_x_mm(100), grid.center_y_mm(100));
        let slab = ([cx - 1.0, cy - 1.0, 0.6], [cx + 1.0, cy + 1.0, 1.6]);
        // The branch whose plan raises the trunk stands 3 mm away, in the clear.
        let branch = plate_post(&grid, 0, 40, 0.05);
        let mut trunk = plate_post(&grid, 1, 100, 0.05);
        trunk.k_top = 12;
        trunk.radius_mm = vec![0.05; 12];
        trunk.carried_mm2 = vec![0.0; 12];
        let struts = vec![branch, trunk];
        let plan = super::ThickenPlan {
            k_lo: 0,
            k_top: 80,
            root: 0.15,
            top: 0.05,
            steps: 80.0,
            chain: vec![(0, 79), (1, 11)],
            disc_growth: None,
            over_existing_lap: false,
            admitted_on_drawn_feet: false,
            added_mm3: 0.0,
        };
        let enters_with = |wall_from: f64, with_surface: bool| {
            let wall = ([cx + wall_from, cy - 0.5, 0.0], [cx + wall_from + 0.5, cy + 0.5, 0.2]);
            let mut history = crate::up_sweep::ColumnHistory::new(&grid);
            cure_box(&grid, &mut history, slab.0, slab.1);
            cure_box(&grid, &mut history, wall.0, wall.1);
            let surface = crate::member_clearance::boxes_surface(&[slab, wall]);
            let context = super::SlendernessContext {
                model_surface: with_surface.then_some(&surface),
                plate_z_mm: 0.0,
                ..prune_context(&grid, &history, &keep_out, &lean, &law)
            };
            let state = super::TieState::build(&context, &struts);
            // The retired read: the longest zone exempts the whole trunk.
            let retired_zone = lean.face_zone_factor() * (0.15 + 0.3);
            let (seat, base) = (
                crate::member_clearance::axis_point(&grid, 0.05, struts[1].cell, 12),
                crate::member_clearance::axis_point(&grid, 0.05, struts[1].cell, 0),
            );
            let retired = crate::member_clearance::inset_segment_intrusion(
                &grid, &history, 0.05, seat, base, retired_zone, 0.0, (0.15, 0.15), 0.3,
            );
            let refused = super::thickening_enters_the_gap(&context, &state, &struts, &plan);
            (refused, retired, retired_zone, state.thicken_reads.get())
        };
        let enters = |wall_from: f64| {
            let (refused, retired, retired_zone, _) = enters_with(wall_from, true);
            (refused, retired, retired_zone)
        };
        let (refused, retired, retired_zone) = enters(0.4);
        assert!(retired_zone >= 0.6, "anti-vacuity: the retired zone covers the trunk: {retired_zone}");
        assert!(retired.is_none(), "SABOTAGE: the retired zone excused the whole trunk");
        assert!(refused, "the raised trunk enters the gap beside its foot, below its seat's own zone");
        let (refused, _, _) = enters(0.85);
        assert!(!refused, "control: a wall 0.85 mm away is not entered");
        // No surface: no face is known, and an unknown face exempts nothing.
        let (refused, retired, _, reads) = enters_with(0.4, false);
        assert!(
            retired.is_none(),
            "SABOTAGE: the retired no-face zone (the longest) left nothing for the raster to flag"
        );
        assert!(
            refused,
            "without a surface the raise beside the 0.4 mm wall is refused ({reads:?})"
        );
        assert!(reads.flagged > 0, "path hit: the raised trunk is flagged without a surface: {reads:?}");
        assert!(
            reads.measured_clear == 0 && reads.inside_already == 0,
            "nothing is measured or proved without a surface: {reads:?}"
        );
        let (refused, _, _, reads) = enters_with(0.85, false);
        assert!(
            refused && reads.flagged > 0,
            "without a surface nothing is provable: the flagged raise is refused ({reads:?})"
        );
        let unknown = crate::member_clearance::contact_zone_mm(
            None,
            [cx, cy, 0.6],
            [0.0, 0.0, -1.0],
            0.15,
            0.3,
            lean.face_zone_factor(),
        );
        assert!(unknown.abs() < 1.0e-12, "an unknown face's zone is zero: {unknown}");
    }

    /// **"INSIDE THE GAP ALREADY" IS PROVED ON THE SURFACE, NOT FLAGGED BY
    /// THE RASTER** (challenger round 3, 2026-10-05, finding 2). The
    /// challenger's shape: a 4 mm plate post of radius 0.05 mm under a flat
    /// seat, and a 0.01 mm square model bar over the post's whole height, its
    /// centre 0.35 mm across and 0.15 mm along from the axis (one cell centre,
    /// 0.3808 mm away). Its nearest surface is 0.3742 mm from the axis, so the
    /// drawn post clears it by 0.3242 mm; raised to a 0.1 mm root it clears by
    /// 0.2742 mm, inside the gap. SABOTAGE INSIDE: the raster flags the drawn
    /// slot (0.3808 < 0.05 + 0.3 + half a cell's diagonal = 0.3854), which the
    /// retired read took as an existing violation and excused the raise.
    /// Green: the predicate measures the drawn slot clear, so the raise is
    /// refused. CONTROLS: a bar whose surface stands 0.31 mm away (the drawn
    /// post is inside the gap there already, PROVED) excuses the raise; a bar
    /// 0.6 mm away is not entered; with no surface nothing can be proved and
    /// the raise is refused.
    #[test]
    fn a_raster_flag_is_not_proof_that_the_drawn_solid_is_inside_the_gap_already() {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let (cx, cy) = (grid.center_x_mm(100), grid.center_y_mm(100));
        let slab = ([cx - 1.0, cy - 1.0, 4.0], [cx + 1.0, cy + 1.0, 5.0]);
        let struts = vec![plate_post(&grid, 0, 100, 0.05)];
        let plan = super::ThickenPlan {
            k_lo: 0,
            k_top: 80,
            root: 0.1,
            top: 0.05,
            steps: 80.0,
            chain: vec![(0, 79)],
            disc_growth: None,
            over_existing_lap: false,
            admitted_on_drawn_feet: false,
            added_mm3: 0.0,
        };
        // A 0.01 mm bar centred on the cell centre `(dx, dy)` from the axis.
        let read = |dx: f64, dy: f64, with_surface: bool| {
            let (bx, by) = (cx + dx, cy + dy);
            let bar = ([bx - 0.005, by - 0.005, 0.0], [bx + 0.005, by + 0.005, 4.0]);
            let mut history = crate::up_sweep::ColumnHistory::new(&grid);
            cure_box(&grid, &mut history, slab.0, slab.1);
            cure_box(&grid, &mut history, bar.0, bar.1);
            let surface = crate::member_clearance::boxes_surface(&[slab, bar]);
            let context = super::SlendernessContext {
                model_surface: with_surface.then_some(&surface),
                plate_z_mm: 0.0,
                ..prune_context(&grid, &history, &keep_out, &lean, &law)
            };
            let state = super::TieState::build(&context, &struts);
            let foot = (
                crate::member_clearance::axis_point(&grid, 0.05, struts[0].cell, 1),
                crate::member_clearance::axis_point(&grid, 0.05, struts[0].cell, 0),
            );
            let drawn_flagged = crate::member_clearance::inset_segment_intrusion(
                &grid, &history, 0.05, foot.0, foot.1, 0.0, 0.0, (0.05, 0.05), 0.3,
            )
            .is_some();
            let refused = super::thickening_enters_the_gap(&context, &state, &struts, &plan);
            (refused, drawn_flagged, state.thicken_reads.get())
        };
        let (refused, drawn_flagged, reads) = read(0.35, 0.15, true);
        assert!(
            drawn_flagged,
            "SABOTAGE: the raster flags the drawn slot, which the retired read took as proof"
        );
        assert!(refused, "the drawn post clears the bar; the raise enters the gap: refused");
        assert!(reads.flagged > 0 && reads.inside_already == 0, "path hits: {reads:?}");
        let (refused, _, reads) = read(0.3, 0.1, true);
        assert!(!refused, "control: the drawn post is inside the gap there already, proved: excused");
        assert!(reads.inside_already > 0, "path hit: proved inside already: {reads:?}");
        let (refused, _, _) = read(0.6, 0.0, true);
        assert!(!refused, "control: a bar 0.6 mm away is not entered");
        let (refused, _, _) = read(0.35, 0.15, false);
        assert!(refused, "control: with no surface nothing is proved, and the flagged raise is refused");
    }

    /// A 10 mm plate post drawn 0.5 mm over its lowest 3 mm and 0.06 mm above
    /// (a load-sized arm: thick where it carries, thin at the tip), alone on
    /// a clear plate. Its run is 2.33 times its limit at S = 50, so it needs
    /// two ties.
    fn tapered_post(grid: &crate::grid::Grid) -> crate::SweepStrut {
        let mut post = plate_post(grid, 0, 150, 0.06);
        post.k_top = 200;
        post.radius_mm = (0..200).map(|layer| if layer < 60 { 0.5 } else { 0.06 }).collect();
        post.carried_mm2 = vec![0.0; 200];
        post
    }

    /// **A MAST IS TIED WHERE THE PIECE ABOVE ITS TIE IS WITHIN ITS OWN
    /// LIMIT** (fix/mast-tie-and-overlap-factor, 2026-10-02;
    /// `v11-feedback-diagnosis.md` item 4).
    ///
    /// The post below runs 2.33 limits, so two ties cut it into pieces inside
    /// the limit. Until this date the mast rung planned its tie at the FIRST
    /// of those evenly spaced layers (a third of the way up), leaving two
    /// thirds of the run standing above it; the braced panel judges the arm's
    /// top and that free upper cantilever alone is
    /// `(6.67 / 4.29)^4 ≈ 5.9` pixels, so every cell was refused by the panel
    /// at every mast radius and no mast could be placed on any run longer
    /// than twice its limit. Red until this date: 0 masts.
    ///
    /// Crosses have no partner here (one post) and the thicken rung is
    /// switched off, so the mast rung is the only one that can fire.
    #[test]
    fn a_mast_is_tied_where_the_piece_above_its_tie_is_within_its_limit() {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 300, 300);
        let history = crate::up_sweep::ColumnHistory::new(&grid);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let context = super::SlendernessContext {
            layers: 260,
            thicken: false,
            ..prune_context(&grid, &history, &keep_out, &lean, &law)
        };
        let mut struts = vec![tapered_post(&grid)];
        let before = super::unbraced_runs(&grid, &struts, 0.05, &law);
        assert_eq!(before.len(), 1, "anti-vacuity: one run, the whole post: {before:?}");
        assert!(
            before[0].over_limit() && extra_ties(&before[0]) == 2,
            "anti-vacuity: the run needs two ties: {:?}",
            before[0]
        );
        let mut outcome = super::SlendernessOutcome::default();
        super::apply_ties(&context, &mut struts, &mut outcome);
        let masts: Vec<&crate::SweepStrut> = struts
            .iter()
            .filter(|strut| strut.kind == crate::StrutKind::Mast)
            .collect();
        assert!(
            !masts.is_empty(),
            "no mast was placed: {} masts, {} cells refused by the panel, {} as thicker than \
             the member",
            outcome.masts_dropped,
            outcome.mast_cells_refused_by_panel,
            outcome.mast_cells_refused_thicker_than_member
        );
        // Every mast is tied at or above the layer the piece above needs: the
        // run measured from its top tie up to the post's top is inside its
        // own limit (the production measure, read through the public census).
        let after = super::unbraced_runs(&grid, &struts, 0.05, &law);
        for run in after.iter().filter(|run| run.strut_id == 0 && run.k_top == 200) {
            assert!(
                run.k_lo > 0,
                "the post's top run still starts at the plate in some direction: {run:?}"
            );
        }
        for mast in &masts {
            assert!(
                mast.min_radius_mm <= 0.5 + 1.0e-12,
                "a mast is never thicker than the member it braces: {} mm",
                mast.min_radius_mm
            );
        }
        assert!(
            outcome.mast_ties_planned_above_the_first_tie > 0,
            "path hit: the rule planned the mast above the crosses' first layer"
        );
        let over: Vec<&UnbracedRun> = after
            .iter()
            .filter(|run| run.strut_id == 0 && run.over_limit())
            .collect();
        assert!(
            over.is_empty(),
            "the post is left over its limit after {} masts and {} crosses: {over:?}",
            masts.len(),
            outcome.braces_added
        );
    }

    /// **A THICKENING IS REFUSED ONLY FOR A LAP OF THE FEET THE FILE DRAWS**
    /// (fix/mast-tie-and-overlap-factor, 2026-10-02).
    ///
    /// Since 2026-09-30 the lowering stands every plate root on the owner's
    /// Heavy boss, its foot the root times 2.2 / 1.5 ≈ 1.467
    /// ([`plinth_support::RootBoss`]). The thicken rung's root-lap test still
    /// measured both roots at the congestion census's crowding factor, 1.6,
    /// so it refused a thickening for a lap the file never prints (the
    /// lonely peak's arm 1 would lap post 0 by 0.001 mm at 1.6, and clear it
    /// by 0.15 mm at the drawn foot; `v11-feedback-diagnosis.md` item 4).
    ///
    /// Two plate posts: A, a 0.05 mm post that the rung thickens to a root
    /// `R` (measured first on A alone), and B, a 0.5 mm post set the first
    /// whole cell beyond the DRAWN feet's reach `1.467 (R + 0.5)` and inside
    /// the crowding reach `1.6 (R + 0.5)`. Red until this date:
    /// `thicken_refused_disc` = 1.
    #[test]
    fn a_thickening_is_refused_only_for_a_lap_of_the_feet_the_file_draws() {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let history = crate::up_sweep::ColumnHistory::new(&grid);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let context = prune_context(&grid, &history, &keep_out, &lean, &law);
        let thicken_post_zero = |struts: &mut Vec<crate::SweepStrut>| {
            let mut state = super::TieState::build(&context, struts);
            let over: Vec<UnbracedRun> = super::member_runs(
                &grid,
                &struts[0],
                &state.ties[0],
                &state.profiles[0],
                0.05,
                &law,
            )
            .into_iter()
            .filter(UnbracedRun::over_limit)
            .collect();
            assert_eq!(over.len(), 1, "anti-vacuity: post A is one over-limit run");
            let mut outcome = super::SlendernessOutcome::default();
            let done = super::thicken(&context, &mut state, struts, &over[0], &mut outcome);
            (done, outcome)
        };
        let mut alone = vec![plate_post(&grid, 0, 40, 0.05)];
        let (done, _) = thicken_post_zero(&mut alone);
        assert!(done, "anti-vacuity: post A alone takes the thicken rung");
        let root = alone[0].thickened_mm[0];
        let boss = plinth_support::RootBoss::registered()
            .expect("the registry holds the owner's root boss")
            .factor();
        let neighbour = 0.5;
        let (drawn_reach, census_reach) = (
            boss * (root + neighbour),
            context.flare_factor * (root + neighbour),
        );
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a short positive length over the grid pitch, rounded up to a cell count"
        )]
        let cells = (drawn_reach / grid.pitch_mm()).floor() as usize + 1;
        let apart = crate::cell_distance_mm(&grid, grid.index(100, 40), grid.index(100, 40 + cells));
        assert!(
            apart > drawn_reach && apart < census_reach,
            "anti-vacuity: B at {apart} mm lies between the drawn reach {drawn_reach} and the \
             crowding reach {census_reach} (root {root}, boss factor {boss})"
        );
        let mut pair = vec![
            plate_post(&grid, 0, 40, 0.05),
            plate_post(&grid, 1, 40 + cells, neighbour),
        ];
        let (done, outcome) = thicken_post_zero(&mut pair);
        assert!(
            done && outcome.thicken_refused_disc == 0,
            "the thickening was refused for a lap of the crowding discs only: refused by disc \
             {}, B {apart} mm away, drawn feet reach {drawn_reach} mm",
            outcome.thicken_refused_disc
        );
        assert!(
            (pair[0].thickened_mm[0] - root).abs() < 1.0e-12,
            "the neighbour changes whether the rung fires, never the root it draws"
        );
    }

    /// **A REFUSAL NEEDS PROOF** (lane cylinder-bracing, rule A, 2026-10-05;
    /// `cylinder-v15-regression.md`). The Cylinder's plate posts 134 and 136
    /// were refused a thickening on slots whose committed read was an
    /// `UpperBound` of 0.3009 to 0.3266 mm: the kernel could not decide a
    /// near-parallel face, so the true clearance is AT MOST that number, and
    /// the number is at or above the 0.3 mm gap. The refusal took that read as
    /// "enters the gap"; the census, on the same kind of read, does not count
    /// the span. Nothing had proved the raise entered the gap.
    ///
    /// The fixture: a 4 mm plate post of radius 0.05 mm under a flat slab, a
    /// wall box over its top 0.5 mm whose near face stands 0.38 mm from the
    /// axis, with that face tilted by ONE unit in the last place of one vertex
    /// (a facet of a round wall, never exactly parallel to the shaft, so the
    /// kernel's determinant guard reads the pierce candidate as
    /// `NearParallel`). The plan raises the root to 0.26 mm and keeps the tip,
    /// which is 0.076 mm at the wall's lowest layer, so the committed solid
    /// stands 0.304 mm from the face: clear, and read as an upper bound.
    ///
    /// Red before the rule: refused, on a slot whose committed read was an
    /// undecided upper bound at or above the gap (the path hit
    /// `ThickenReads::undecided`). Green: admitted.
    ///
    /// CONTROLS. The same tilted wall 0.36 mm from the axis stands 0.284 mm
    /// from the committed solid: the read is an upper bound UNDER the gap,
    /// which is proof, and the raise is refused. The same wall, exactly
    /// parallel, 0.38 mm away is MEASURED clear and admitted without an
    /// undecided read. SABOTAGE INSIDE: the retired reading (anything that is
    /// not a measured clear read enters) is computed here from the same
    /// kernel call and is shown to refuse the 0.38 mm case.
    #[test]
    fn an_undecided_read_beside_a_parallel_wall_does_not_refuse_a_raise() {
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let (cx, cy) = (grid.center_x_mm(100), grid.center_y_mm(100));
        let slab = ([cx - 1.0, cy - 1.0, 4.0], [cx + 1.0, cy + 1.0, 5.0]);
        let struts = vec![plate_post(&grid, 0, 100, 0.05)];
        let plan = super::ThickenPlan {
            k_lo: 0,
            k_top: 80,
            root: 0.26,
            top: 0.05,
            steps: 80.0,
            chain: vec![(0, 79)],
            disc_growth: None,
            over_existing_lap: false,
            admitted_on_drawn_feet: false,
            added_mm3: 0.0,
        };
        // (refused, the committed read of the wall's lowest slot, the reads).
        let read = |wall_from: f64, tilted: bool| {
            let wall = ([cx + wall_from, cy - 0.5, 3.5], [cx + wall_from + 0.5, cy + 0.5, 4.0]);
            let mut history = crate::up_sweep::ColumnHistory::new(&grid);
            cure_box(&grid, &mut history, slab.0, slab.1);
            cure_box(&grid, &mut history, wall.0, wall.1);
            let (mut verts, tris) = crate::member_clearance::boxes_mesh(&[slab, wall]);
            if tilted {
                // Corner 0 of the wall (vertex 8) one ulp across: the wall's
                // near face is no longer exactly parallel to the shaft.
                verts[8][0] = f64::from_bits(verts[8][0].to_bits() + 1);
            }
            let surface = crate::member_clearance::indexed_surface(&verts, &tris);
            let context = super::SlendernessContext {
                model_surface: Some(&surface),
                plate_z_mm: 0.0,
                ..prune_context(&grid, &history, &keep_out, &lean, &law)
            };
            let state = super::TieState::build(&context, &struts);
            // The committed frustum of the wall's lowest slot (layers 70, 71),
            // read as the refusal reads it.
            let at = |layer: usize| crate::member_clearance::axis_point(&grid, 0.05, struts[0].cell, layer);
            let wanted = |layer: usize| plan.wanted(0, layer);
            let verdict = surface
                .segment_frustum_clearance_classified(
                    &[at(71)],
                    &[at(70)],
                    &[wanted(71)],
                    &[wanted(70)],
                    0.0,
                    &[0.0],
                    &[0.0],
                )
                .expect("a read")[0];
            let refused = super::thickening_enters_the_gap(&context, &state, &struts, &plan);
            (refused, verdict, state.thicken_reads.get())
        };
        let (refused, verdict, reads) = read(0.38, true);
        assert!(
            matches!(
                verdict,
                geom_kernel::SegmentClearanceClassified::UpperBound { clearance_mm, .. }
                    if clearance_mm >= 0.3
            ),
            "anti-vacuity: the committed read is an undecided upper bound at or above the gap: {verdict:?}"
        );
        assert!(
            !matches!(verdict, geom_kernel::SegmentClearanceClassified::Measured { .. }),
            "SABOTAGE: the retired rule (only a measured clear read clears) refuses this slot"
        );
        assert!(reads.flagged > 0 && reads.undecided > 0, "path hits: {reads:?}");
        assert!(!refused, "an undecided read proves nothing: the raise is admitted ({reads:?})");
        let (refused, verdict, reads) = read(0.36, true);
        assert!(
            matches!(
                verdict,
                geom_kernel::SegmentClearanceClassified::UpperBound { clearance_mm, .. }
                    if clearance_mm < 0.3
            ),
            "control: the nearer wall reads an upper bound under the gap: {verdict:?}"
        );
        assert!(refused, "control: an upper bound under the gap PROVES the raise enters it ({reads:?})");
        let (refused, verdict, reads) = read(0.38, false);
        assert!(
            matches!(verdict, geom_kernel::SegmentClearanceClassified::Measured { clearance_mm, .. } if clearance_mm >= 0.3),
            "control: an exactly parallel wall is measured clear: {verdict:?}"
        );
        assert!(
            !refused && reads.undecided == 0 && reads.measured_clear > 0,
            "control: measured clear and admitted ({reads:?})"
        );
    }

    /// **THE FINAL GAP DOES NOT APPLY INSIDE A HIDDEN CAVITY** (Craig ruling
    /// 2026-10-05, on his 2026-08-31 ruling that supports hidden in a cavity
    /// are never removed or cleaned; `cylinder-v15-regression.md`: the
    /// Cylinder's interior posts 179, 195 and 203 stand on a cavity floor and
    /// were refused the thickening that held them, 0.23 to 0.30 mm from the
    /// cavity's wall).
    ///
    /// A 4 mm post of radius 0.05 mm standing on a model floor (landing
    /// `Model { layer: 0 }`) with a cured wall 0.4 mm from its axis over its
    /// whole height, as in `thickening_refuses_a_root_that_would_enter_the_gap`.
    ///
    /// The authority is the CAVITY MAP ([`crate::cavity`], built here from the
    /// slices of a real 8 mm box with 1.5 mm walls), not the scar flag
    /// ([`crate::SweepStrut::interior`], "model above the landing in the same
    /// column", which is also true of a recess open to the outside; challenger
    /// 2026-10-05). In all three fields the post's column has model above it,
    /// so the scar flag is TRUE for the post in all three (asserted: the
    /// anti-vacuity, since it is what admitted the pocket before).
    ///
    /// - SEALED box: the post stands in enclosed air. The rung thickens it
    ///   (path hit `thicken_members_hidden`).
    /// - open-sided POCKET (the same box with one wall left out, so a ceiling
    ///   stands over the same floor and the air reaches the outside): the rung
    ///   REFUSES the same raise. This is the negative control.
    /// - no cavity at all: refused, as before.
    ///
    /// SABOTAGE RUN (done by hand, not left in the code): the exemption read
    /// from `strut.interior` instead of the map lets the pocket thicken, and
    /// the pocket assertion fails.
    #[test]
    fn a_post_on_a_hidden_cavity_floor_may_thicken_within_the_gap_of_the_cavity() {
        use crate::cavity::fixtures::{cavity_box_sized, map_on};
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        #[derive(Clone, Copy)]
        enum Field {
            Sealed,
            Pocket,
            None,
        }
        let build = |field: Field| {
            let mut history = crate::up_sweep::ColumnHistory::new(&grid);
            for row in 90..=110 {
                let cell = grid.index(row, 108);
                history.model_head[cell] = u32::try_from(history.model_spans.len()).expect("a few spans");
                history.model_spans.push(crate::up_sweep::ShaftSpan {
                    k_bottom: 0,
                    k_top: 60,
                    id: 0,
                    next: u32::MAX,
                    landable_end: u32::MAX,
                });
            }
            // Model material over the post's column, above its seat layer, in
            // every field: what the scar flag reads.
            history.column_max_model[grid.index(100, 100)] = 200;
            history.hidden = match field {
                Field::Sealed => map_on(&cavity_box_sized(false, false, 0.0, 8.0, 1.5), &[], 0.0, &grid),
                Field::Pocket => map_on(&cavity_box_sized(true, false, 0.0, 8.0, 1.5), &[], 0.0, &grid),
                Field::None => crate::cavity::HiddenCavity::none(),
            };
            let mut struts = vec![plate_post(&grid, 0, 100, 0.05)];
            struts[0].landing = crate::Landing::Model { layer: 0 };
            crate::set_interior(&mut struts, &history);
            (history, struts)
        };
        let attempt = |field: Field| {
            let (history, mut struts) = build(field);
            assert!(struts[0].interior, "anti-vacuity: the scar flag calls the post interior in every field");
            let context = prune_context(&grid, &history, &keep_out, &lean, &law);
            let mut state = super::TieState::build(&context, &struts);
            let over: Vec<UnbracedRun> = super::member_runs(
                &grid,
                &struts[0],
                &state.ties[0],
                &state.profiles[0],
                0.05,
                &law,
            )
            .into_iter()
            .filter(UnbracedRun::over_limit)
            .collect();
            assert_eq!(over.len(), 1, "anti-vacuity: the untied post is one over-limit run");
            let mut outcome = super::SlendernessOutcome::default();
            let done = super::thicken(&context, &mut state, &mut struts, &over[0], &mut outcome);
            let hidden = state.thicken_reads.get().hidden_members;
            (done, outcome, struts, hidden)
        };
        assert!(build(Field::Sealed).0.hidden.census().enclosed_voxels > 0, "anti-vacuity: the box encloses air");
        assert_eq!(build(Field::Pocket).0.hidden.census().enclosed_voxels, 0, "anti-vacuity: the pocket does not");
        let (done, outcome, struts, hidden) = attempt(Field::Sealed);
        assert!(done, "inside a hidden cavity the final gap does not apply: the rung thickens ({outcome:?})");
        assert_eq!(outcome.thicken_refused_model, 0, "{outcome:?}");
        assert!(struts[0].thickened_mm[0] > 0.065, "into the cavity wall: {}", struts[0].thickened_mm[0]);
        assert!(hidden > 0, "path hit: the refusal skipped a hidden member");
        for (field, what) in [(Field::Pocket, "a pocket open to the outside"), (Field::None, "no cavity")] {
            let (done, outcome, struts, hidden) = attempt(field);
            assert!(!done, "{what}: the same raise enters the gap and is refused ({outcome:?})");
            assert_eq!(outcome.thicken_refused_model, 1, "{what}: {outcome:?}");
            assert!(struts[0].thickened_mm.is_empty(), "{what}: the post is left as drawn");
            assert_eq!(hidden, 0, "{what}: the member is read");
        }
    }

    /// **THE EXEMPTION IS FOR THE PROPOSED SOLID, NOT THE CENTRELINE**
    /// (challenger, 2026-10-05). A post 0.475 mm from the inner face of a
    /// sealed box's wall (1.5 mm, outer face at 9.5 mm) with a cured wall cell
    /// 0.4 mm from its axis, asked with `thickening_enters_the_gap`:
    ///
    /// - a raise to 0.7 mm stays in hidden voxels (it touches the cavity wall
    ///   0.23 mm deep, which the ruling permits): admitted, path hit
    ///   `hidden_members`;
    /// - a raise to 2.1 mm would carry the solid through the outer face at
    ///   x = 9.5: read as before, refused, though the CENTRELINE is in the same
    ///   hidden voxels as in the first case;
    /// - the controls: no cavity at all, and a box whose walls are 0.1 mm (the
    ///   thin wall of the challenger's example) refuse the 0.7 mm raise, and
    ///   the thin box encloses NO air (a wall that holds no whole cell does
    ///   not seal, so no member is ever hidden behind it).
    ///
    /// SABOTAGE RUN (by hand): the proposed radius read as 0 (the centreline)
    /// admits the 2.1 mm raise and the second assertion fails.
    #[test]
    fn a_raise_that_carries_the_solid_out_of_the_hidden_voxels_is_read_as_before() {
        use crate::cavity::fixtures::{cavity_box_sized, map_on};
        use crate::cavity::HiddenCavity;
        let grid = crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200);
        let keep_out = crate::drain::DrainKeepOut::none();
        let lean = crate::lean::LeanLaw::new(0.05, 45.0, 25.0);
        let law = law();
        let plan = |root: f64| super::ThickenPlan {
            k_lo: 0,
            k_top: 80,
            root,
            top: root,
            steps: 80.0,
            chain: vec![(0, 79)],
            disc_growth: None,
            over_existing_lap: false,
            admitted_on_drawn_feet: false,
            added_mm3: 0.0,
        };
        // (refused, members read as hidden, enclosed voxels of the map).
        let read = |map: Option<(f64, f64)>, root: f64| {
            let mut history = crate::up_sweep::ColumnHistory::new(&grid);
            for row in 90..=110 {
                let cell = grid.index(row, 158);
                history.model_head[cell] = u32::try_from(history.model_spans.len()).expect("a few spans");
                history.model_spans.push(crate::up_sweep::ShaftSpan {
                    k_bottom: 0,
                    k_top: 60,
                    id: 0,
                    next: u32::MAX,
                    landable_end: u32::MAX,
                });
            }
            history.hidden = match map {
                Some((outer, wall)) => map_on(&cavity_box_sized(false, false, 0.0, outer, wall), &[], 0.0, &grid),
                None => HiddenCavity::none(),
            };
            let mut struts = vec![plate_post(&grid, 0, 150, 0.05)];
            struts[0].landing = crate::Landing::Model { layer: 0 };
            let context = prune_context(&grid, &history, &keep_out, &lean, &law);
            let state = super::TieState::build(&context, &struts);
            let refused = super::thickening_enters_the_gap(&context, &state, &struts, &plan(root));
            (refused, state.thicken_reads.get().hidden_members, history.hidden.census().enclosed_voxels)
        };
        let (refused, hidden, enclosed) = read(Some((9.0, 1.5)), 0.7);
        assert!(enclosed > 0, "anti-vacuity: the 9 mm box with 1.5 mm walls encloses air");
        assert!(
            !refused && hidden == 1,
            "a raise that stays in hidden voxels, touching the cavity wall, is admitted (hidden {hidden})"
        );
        let (refused, hidden, _) = read(Some((9.0, 1.5)), 2.1);
        assert!(
            refused && hidden == 0,
            "a raise whose solid reaches the outer face is read as before (hidden {hidden})"
        );
        let (refused, hidden, _) = read(None, 0.7);
        assert!(refused && hidden == 0, "control: no cavity, the same raise is refused");
        let (refused, hidden, enclosed) = read(Some((9.0, 0.1)), 0.7);
        assert_eq!(enclosed, 0, "a 0.1 mm wall holds no whole cell: no enclosed air");
        assert!(refused && hidden == 0, "control: the thin-walled box hides nothing");
    }

    /// **A MODEL TIE IN A HIDDEN CAVITY IS CONFIRMED CLEAR** (Craig ruling
    /// 2026-10-05; challenger 2026-10-05: the tie screen's lone census
    /// demanded `members_measured == 1`, so a tie the census left out as
    /// hidden was refused at the gap anyway). The lone census of one tie
    /// confirms it when it measured it clear, or when it left it out because
    /// the tie stands wholly in a hidden cavity; never when a span is
    /// undecided, when it is inside the gap, or when nothing was read and
    /// nothing was hidden.
    #[test]
    fn a_tie_left_out_as_hidden_is_confirmed_clear_and_nothing_else_unread_is() {
        use crate::report::{MemberModelClearance, MemberNearModel};
        let census = |measured: usize, hidden: usize, unmeasured: usize, near: usize| MemberModelClearance {
            members_measured: measured,
            members_hidden: hidden,
            members_unmeasured: unmeasured,
            within_gap: (0..near)
                .map(|member| MemberNearModel {
                    member: u32::try_from(member).expect("a few"),
                    kind: crate::StrutKind::Tie,
                    landing: crate::Landing::Plate,
                    clearance_mm: 0.1,
                    upper_bound: false,
                    witness_mm: [0.0; 3],
                })
                .collect(),
            ..MemberModelClearance::default()
        };
        assert!(super::tie_confirmed_clear(&census(1, 0, 0, 0)), "measured clear");
        assert!(super::tie_confirmed_clear(&census(0, 1, 0, 0)), "left out as hidden");
        assert!(!super::tie_confirmed_clear(&census(0, 0, 0, 0)), "nothing read, nothing hidden");
        assert!(!super::tie_confirmed_clear(&census(0, 0, 1, 0)), "a span undecided");
        assert!(!super::tie_confirmed_clear(&census(1, 0, 0, 1)), "inside the gap");
    }
}
