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
//! 5. **Warn.** Named, counted, emitted. Never a dropped or shortened arm.
//!
//! **Every remedy is attempted only for a run that is over the limit NOW.**
//! The member's runs are re-measured from the live strut set after every
//! remedy that touches it, so no partner builds a second cross on a pair an
//! earlier cross already tied, and no mast is dropped beside a post that is
//! already held.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::drain::DrainKeepOut;
use crate::grid::Grid;
use crate::lean::LeanLaw;
use crate::up_sweep::ColumnHistory;
use crate::{Landing, StrutKind, SweepStrut};
use plinth_support::SweepRootFlare;

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
    /// Thickenings APPLIED whose root disc already lapped another root before
    /// the growth (and laps it more after): the growth made no new lap, so
    /// the rung is not refused (orchestrator ruling 2026-09-29, the stilt
    /// plate's post 81). The standing lap is the congestion lane's warning; this
    /// counts where the rung grew one. A path-hit witness for the rule.
    pub thicken_over_existing_lap: usize,
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
    /// warning).
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
    /// Remedy-1 candidates NOT re-landed because another member rests on them
    /// or is tied to them. Moving such an arm moves the ground from under its
    /// dependants — the daisy chains of 2026-09-28. The run falls to the brace,
    /// the mast, or a warning. A path-hit witness for the rule.
    pub merges_refused_carrying: usize,
    /// Shaft landings the index offered and the LIVE field refused: the named
    /// parent no longer stands at that cell and layer, because an earlier
    /// merge in the same loop moved it. A path-hit witness for the rule.
    pub merges_refused_stale_shaft: usize,
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
#[must_use]
pub fn unbraced_runs(
    grid: &Grid,
    struts: &[SweepStrut],
    layer_height_mm: f64,
    law: &SlendernessLaw,
) -> Vec<UnbracedRun> {
    let (profiles, _) = crate::adapter::drawn_profiles(struts);
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

/// The ties of one member grouped by layer: per layer, whether something
/// there holds every direction (a landing, an end), and the DISTINCT planes
/// of the crosses there. One sort, no map.
fn ties_by_layer(ties: &[Tie]) -> Vec<(usize, bool, Vec<Plane>)> {
    let mut sorted: Vec<Tie> = ties.to_vec();
    sorted.sort_unstable_by_key(|tie| tie.layer);
    let mut groups: Vec<(usize, bool, Vec<Plane>)> = Vec::new();
    for tie in sorted {
        if groups.last().is_none_or(|group| group.0 != tie.layer) {
            groups.push((tie.layer, false, Vec::new()));
        }
        let Some(group) = groups.last_mut() else {
            continue;
        };
        match tie.plane {
            None => group.1 = true,
            Some(plane) => {
                if !group.2.iter().any(|known| parallel(*known, plane)) {
                    group.2.push(plane);
                }
            }
        }
    }
    groups
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
    plane: Option<Plane>,
    /// Whether this is a cross's band TOP on the member (a diagonal SEATS on
    /// it here). A cross ties nothing while printing until its top is cured,
    /// so only a band top is a layer a further plane may share.
    band_top: bool,
}

fn live(strut: &SweepStrut) -> bool {
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
fn free_shaft_floor(
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
            if let Some(list) = ties.get_mut(id as usize).filter(|_| credited) {
                list.push(Tie {
                    layer,
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
    if !live(strut) {
        return Vec::new();
    }
    // CROSSES AT ONE LAYER ARE CREDITED TOGETHER (`joint_holds`); a layer
    // with a landing or an end holds every direction whatever crosses it has.
    let groups = ties_by_layer(ties);
    let open: Vec<&[Plane]> = groups
        .iter()
        .filter(|(_, all, _)| !*all)
        .map(|(_, _, planes)| planes.as_slice())
        .collect();
    let mut runs: Vec<UnbracedRun> = Vec::new();
    let mut layers: Vec<usize> = Vec::with_capacity(groups.len() + 2);
    for sway_deg in sway_directions_by_layer(&open) {
        layers.clear();
        layers.push(strut.k_bottom);
        layers.push(strut.k_top);
        for (layer, all, planes) in &groups {
            let held = *all || sway_deg.is_none_or(|sway| joint_holds(planes, sway));
            if held && *layer >= strut.k_bottom && *layer <= strut.k_top {
                layers.push(*layer);
            }
        }
        layers.sort_unstable();
        layers.dedup();
        for pair in layers.windows(2) {
            let (k_lo, k_top) = (pair[0], pair[1]);
            if runs
                .iter()
                .any(|run| run.k_lo == k_lo && run.k_top == k_top)
            {
                continue;
            }
            let length_mm = run_length(grid, strut, k_lo, k_top, layer_height_mm);
            let radius_mm = run_radius(strut, profile, k_lo, k_top);
            runs.push(UnbracedRun {
                strut_id: strut.id,
                k_lo,
                k_top,
                length_mm,
                radius_mm,
                limit_mm: law.max_unbraced_mm(radius_mm),
                sway_deg,
            });
        }
    }
    runs
}

/// The drawn radius at an absolute layer, or the member's own minimum where
/// the profile has no entry yet (a member this pass added and the down-sweep
/// has not sized).
fn drawn_at(strut: &SweepStrut, profile: &[f64], layer: usize) -> Option<f64> {
    let slot = layer.checked_sub(strut.k_bottom)?;
    let here = profile.get(slot).copied().unwrap_or(0.0);
    let minimum = strut
        .min_radius_mm
        .max(strut.thickened_mm.get(layer).copied().unwrap_or(0.0));
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

fn run_radius(strut: &SweepStrut, profile: &[f64], k_lo: usize, k_top: usize) -> f64 {
    equivalent_radius(k_top.saturating_sub(k_lo), |j| {
        drawn_at(strut, profile, k_lo + j)
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
    let span = (strut.k_top - strut.k_bottom) as f64;
    let lateral_mm = crate::cell_distance_mm(grid, strut.cell, strut.base_cell());
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

    let runs = unbraced_runs(context.grid, struts, context.layer_height_mm, law);
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
        stamp_all(&mut index, struts);
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
pub fn apply_ties(
    context: &SlendernessContext<'_>,
    struts: &mut Vec<SweepStrut>,
    outcome: &mut SlendernessOutcome,
) -> usize {
    let law = context.law;
    if !law.binds() {
        return 0;
    }
    let grid = context.grid;
    let height = context.layer_height_mm;
    let first_new = struts.len();
    let mut state = TieState::build(context, struts);
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
                // The first of the ties the run needs, evenly spaced; the
                // member is measured again before the next one.
                let k = tie_layer(&run, 1, extra_ties(&run).max(1));
                // A FURTHER PLANE GOES TO A LAYER THE MEMBER IS ALREADY TIED
                // AT, where that layer alone cuts the run into pieces inside
                // the limit: its planes are then credited together there
                // (`joint_holds`). Until 2026-09-29 each direction took its own
                // mid-run layer, and a post collected crosses at five levels
                // where two would do (the stilt plate's post 125).
                let shared = shared_tie_layer(
                    grid,
                    strut,
                    &state.ties[index],
                    &state.profiles[index],
                    &run,
                    height,
                    law,
                    k,
                );
                if context.brace {
                    if let Some(at) = shared {
                        if tie_by_crosses(context, &mut state, struts, &run, at, false, outcome) {
                            outcome.crosses_at_an_existing_tie_layer += 1;
                            applied += 1;
                            continue;
                        }
                    }
                    if let Some(k) = k {
                        if tie_by_crosses(context, &mut state, struts, &run, k, true, outcome) {
                            applied += 1;
                            continue;
                        }
                    }
                }
                if context.thicken && thicken(context, &mut state, struts, &run, outcome) {
                    applied += 1;
                    continue;
                }
                if context.mast {
                    if let Some(k) = k {
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
        length <= law.max_unbraced_mm(run_radius(strut, profile, k_lo, k_top))
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
/// The drawn profiles are read as they stand. Where a removed diagonal was
/// THICKER than its post, the post's profile still carries the junction raise
/// that diagonal made, so the post is judged a little stiffer than it will
/// print; the brace rule makes such a diagonal rare, and the down-sweep and
/// the next pass measure the field again from scratch, so a run this leaves
/// over the limit is tied then or named as a warning.
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
    let mut ties = state.ties.clone();
    let mut removed = vec![false; len];
    let mut taken = 0_usize;
    for &(one, two) in crosses.iter().rev() {
        let brace = &struts[one];
        let (Some(a), Landing::Strut { id: b, .. }) = (brace.seat_on_strut, brace.landing) else {
            continue;
        };
        let (k, k_lo) = (brace.k_top, brace.k_bottom);
        let plane = brace_plane(grid, struts, brace);
        let without = |post: u32| {
            let mut list = ties[post as usize].clone();
            for layer in [k, k_lo] {
                if let Some(position) = list
                    .iter()
                    .position(|tie| tie.layer == layer && tie.plane == plane)
                {
                    list.remove(position);
                }
            }
            list
        };
        // THE POST AS IT WOULD BE DRAWN WITHOUT THIS CROSS: its ledger, raised
        // by every branch that still joins it (the lowering's junction clause,
        // one post at a time), and not by the two diagonals being judged. A
        // diagonal thicker than its post — common where the area law leaves a
        // post at the minimum tip — raised the post it joined, and judging the
        // post on that raise would take out a cross the post still needs.
        let drawn_without = |post: u32| {
            let host = &struts[post as usize];
            let mut profile = if host.radius_mm.is_empty() {
                state.profiles[post as usize].clone()
            } else {
                host.radius_mm.clone()
            };
            for (index, branch) in struts.iter().enumerate() {
                if removed[index] || index == one || index == two || !live(branch) {
                    continue;
                }
                let mut joints: Vec<(usize, Option<f64>)> = Vec::new();
                if matches!(branch.landing, Landing::Strut { id, .. } if id == post) {
                    joints.push((branch.k_bottom, state.profiles[index].first().copied()));
                }
                if branch.seat_on_strut == Some(post) {
                    joints.push((branch.k_top, state.profiles[index].last().copied()));
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
        let needed = [a, b].iter().any(|&post| {
            member_runs(
                grid,
                &struts[post as usize],
                &without(post),
                &drawn_without(post),
                height,
                law,
            )
            .iter()
            .any(|run| run.over_limit() && run.k_lo < k && k_lo < run.k_top)
        });
        if needed {
            continue;
        }
        let (list_a, list_b) = (without(a), without(b));
        ties[a as usize] = list_a;
        ties[b as usize] = list_b;
        removed[one] = true;
        removed[two] = true;
        taken += 1;
        outcome.braces_added = outcome.braces_added.saturating_sub(1);
        outcome.crosses_pruned += 1;
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
    /// Set by [`plan_cross`] when it drops a band whose bottom fell inside a
    /// flare because the band could not move within its posts: how many layers
    /// the band had to rise. The mast rung reads it to try again with a mast
    /// that tall (a new mast is only as tall as the layer it is planned at).
    flare_shortfall: std::cell::Cell<Option<usize>>,
}

impl TieState {
    fn build(context: &SlendernessContext<'_>, struts: &[SweepStrut]) -> Self {
        let mut index = ColumnHistory::new(context.grid);
        stamp_all(&mut index, struts);
        let (profiles, _) = crate::adapter::drawn_profiles(struts);
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
        Self {
            index,
            profiles,
            ties,
            bands,
            masts_of,
            placed_roots,
            root_flare,
            flare_shortfall: std::cell::Cell::new(None),
        }
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
fn tie_by_crosses(
    context: &SlendernessContext<'_>,
    state: &mut TieState,
    struts: &mut Vec<SweepStrut>,
    run: &UnbracedRun,
    k: usize,
    may_move: bool,
    outcome: &mut SlendernessOutcome,
) -> bool {
    let id = run.strut_id;
    let Some(here) = struts.get(id as usize).map(|strut| strut.cell_at(k)) else {
        return false;
    };
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
        return true;
    }
    false
}

/// The junction clause from one raised joint DOWN: `member`'s drawn profile is
/// raised to at least `radius` from `layer` to its root, then each trunk it
/// stands on is raised to the member's drawn root from the member's landing to
/// ITS root, and so on to the member that meets the plate or the model — the
/// same clause, on the same slots, that [`crate::adapter::drawn_profiles`]
/// applies for the lowering. Returns the index of that last member (the foot).
/// A landing cycle, which placement never builds, stops the walk.
fn raise_to_the_foot(
    struts: &[SweepStrut],
    profiles: &mut [Vec<f64>],
    member: usize,
    layer: usize,
    radius: f64,
) -> usize {
    let (mut at, mut layer, mut radius) = (member, layer, radius);
    for _ in 0..=struts.len() {
        let Some(strut) = struts.get(at) else {
            return at;
        };
        if let Some(profile) = profiles.get_mut(at) {
            let top = layer.saturating_sub(strut.k_bottom);
            for radius_here in profile.iter_mut().take(top + 1) {
                *radius_here = radius_here.max(radius);
            }
            radius = profile.first().copied().unwrap_or(radius);
        }
        let Landing::Strut { id, .. } = strut.landing else {
            return at;
        };
        let host = id as usize;
        if host == at || host >= struts.len() {
            return at;
        }
        layer = strut.k_bottom;
        at = host;
    }
    at
}

/// Grow a plate foot's disc in `state.placed_roots` to the root it is now
/// DRAWN with ([`plinth_support::drawn_root_radius_mm`] of its live profile),
/// counting the growth and whether the grown disc laps another root.
fn grow_foot_disc(
    context: &SlendernessContext<'_>,
    state: &mut TieState,
    struts: &[SweepStrut],
    foot: usize,
    outcome: &mut SlendernessOutcome,
) {
    let Some(strut) = struts.get(foot) else {
        return;
    };
    if !matches!(strut.landing, Landing::Plate) || strut.kind == StrutKind::Brace {
        return;
    }
    let Some(profile) = state.profiles.get(foot) else {
        return;
    };
    let root = plinth_support::drawn_root_radius_mm(profile, strut.tip_radius_mm());
    let flared = root * flare_of(context);
    let Some(slot) = crate::congestion::disc_slot(&state.placed_roots, strut.id) else {
        return;
    };
    if crate::congestion::disc_radius_mm(&state.placed_roots, slot)
        .is_some_and(|known| flared > known)
    {
        crate::congestion::grow_disc(&mut state.placed_roots, slot, flared, root);
        outcome.cross_roots_grown += 1;
        if !crate::congestion::clear_of_placed(
            context.grid,
            strut.base_cell(),
            flared,
            &state.placed_roots,
            Some(slot),
        ) {
            outcome.cross_roots_lapping += 1;
        }
    }
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
    stamp_last_two(&mut state.index, struts);
    for post in [a, b] {
        if let Some(list) = state.ties.get_mut(post as usize) {
            list.push(Tie {
                layer: k,
                plane,
                band_top: true,
            });
            list.push(Tie {
                layer: k_lo,
                plane,
                band_top: false,
            });
        }
        // The junction clause the lowering applies: the post is drawn at
        // least as thick as the diagonal, from the joint down to its root —
        // and every trunk it stands on in turn, down to the plate or the model
        // (`adapter::drawn_profiles` repeats the clause to a fixed point).
        let foot = raise_to_the_foot(struts, &mut state.profiles, post as usize, k, radius);
        // THE FOOT ON THE PLATE GROWS WITH IT (review-fixes 2026-09-29): the
        // lowering flares the drawn root, so the disc every later mast and
        // thickening is cleared against is grown to it here. A foot that now
        // laps another root is counted: a warning on the emitted file.
        grow_foot_disc(context, state, struts, foot, outcome);
    }
    for _ in 0..2 {
        state.ties.push(Vec::new());
        state.profiles.push(vec![radius; span + 1]);
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
/// * where that chain meets the PLATE, the grown root's flared disc clears
///   every other root disc — the same rule, the same function, as a mast's.
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
    let grid = context.grid;
    let member = run.strut_id as usize;
    let n = run.k_top.saturating_sub(run.k_lo);
    if n == 0 {
        return false;
    }
    let (k_lo, k_top) = (run.k_lo, run.k_top);
    #[expect(
        clippy::cast_precision_loss,
        reason = "layer counts bounded by the plan's own layer count are exact in f64"
    )]
    let n_f = n as f64;
    // The drawn radius over the run, bottom first, and the top it keeps.
    let (drawn, top) = {
        let Some(strut) = struts.get(member) else {
            return false;
        };
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
    let taper = |root: f64, j: usize| root + (top - root) * (j as f64) / n_f;
    let holds = |root: f64| {
        equivalent_radius(n, |j| {
            let wanted = taper(root, j);
            Some(drawn[j].map_or(wanted, |here| here.max(wanted)))
        })
        .is_some_and(|radius| run.length_mm <= law.max_unbraced_mm(radius))
    };
    if !holds(law.r_max_mm) {
        outcome.thicken_refused_radius += 1;
        return false;
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
                    let (new_lap, existing) = crate::congestion::growth_laps(
                        grid,
                        strut.base_cell(),
                        known.unwrap_or(0.0),
                        flared,
                        &state.placed_roots,
                        slot,
                    );
                    if new_lap {
                        outcome.thicken_refused_disc += 1;
                        return false;
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

    for (position, &(index, highest)) in chain.iter().enumerate() {
        let strut = &mut struts[index];
        let k_bottom = strut.k_bottom;
        let upto = if position == 0 { k_top } else { highest + 1 };
        if strut.thickened_mm.len() < upto {
            strut.thickened_mm.resize(upto, 0.0);
        }
        let mut raised = false;
        for layer in k_bottom..upto {
            let wanted = if position == 0 && layer >= k_lo {
                taper(root, layer - k_lo)
            } else {
                root
            };
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
    if let Some((slot, flared)) = disc_growth {
        crate::congestion::grow_disc(&mut state.placed_roots, slot, flared, root);
    }
    if over_existing_lap {
        outcome.thicken_over_existing_lap += 1;
    }
    outcome.members_thickened += 1;
    true
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

    // The arm's own mast, extended.
    let own: Vec<u32> = state.masts_of.get(&id).cloned().unwrap_or_default();
    for mast in own {
        let Some(standing) = struts.get(mast as usize) else {
            continue;
        };
        if !admissible(plane_between(grid, here, standing.cell)) {
            continue;
        }
        let (old_top, cell, old_radius) = (standing.k_top, standing.cell, standing.min_radius_mm);
        let spans_before = state.index.shaft_spans.len();
        // A taller mast needs the radius its whole new height asks for, and its
        // grown root has to clear every other root, as a new mast's would.
        #[expect(
            clippy::cast_precision_loss,
            reason = "a layer index bounded by the plan's own layer count is exact in f64"
        )]
        let grown = law
            .brace_radius_mm(k as f64 * context.layer_height_mm)
            .max(old_radius);
        let slot = crate::congestion::disc_slot(&state.placed_roots, mast);
        if old_top < k {
            let blocked = (old_top + 1..=k).any(|layer| {
                !context.keep_out.admits(cell, layer)
                    || state.index.shaft_blocks(cell, layer, None)
                    || context.history.model_blocks(cell, layer)
            });
            let crowded = grown > old_radius
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
            state.index.stamp_shaft(mast, old_top + 1, k, cell, &[]);
            struts[mast as usize].k_top = k;
        }
        if let Some(plan) = plan_cross(context, state, struts, id, mast, k, run, true, outcome) {
            if old_top < k {
                let profile = &mut state.profiles[mast as usize];
                profile.resize(k + 1, grown);
                for radius in profile.iter_mut() {
                    *radius = radius.max(grown);
                }
                struts[mast as usize].min_radius_mm = grown;
                if let Some(slot) = slot {
                    crate::congestion::grow_disc(
                        &mut state.placed_roots,
                        slot,
                        grown * flare_of(context),
                        grown,
                    );
                }
                outcome.masts_extended += 1;
            }
            commit_cross(context, state, struts, plan, outcome);
            outcome.braces_added += 1;
            return Some(None);
        }
        if old_top < k {
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
            // 15.6 mm limit, re-opened by the very next pass).
            candidates.push((distance_mm, cell, law.brace_radius_mm(mast_height_mm)));
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
    });
    state.ties.push(Vec::new());
    state.profiles.push(vec![radius_mm; k + 1]);
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
            if index.shaft_blocks_excluding(cell, here, except) {
                return None;
            }
        }
        path.push(u32::try_from(cell).unwrap_or(u32::MAX));
    }
    Some(path)
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
                // brace, which holds nothing: only shafts are partners.
                && other.kind != StrutKind::Brace
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
/// where the moved band no longer ties `run` (its bottom is not below the
/// run's top), or where its plane no longer holds the direction `run` sways in
/// at the new layer.
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
    let (floor, top_limit) = {
        let (Some(one), Some(two)) = (struts.get(a as usize), struts.get(b as usize)) else {
            return None;
        };
        (
            state
                .free_shaft_floor(grid, one, h)
                .max(state.free_shaft_floor(grid, two, h)),
            one.k_top.min(two.k_top),
        )
    };
    let dropped = |outcome: &mut SlendernessOutcome| {
        outcome.brace_bands_refused += 1;
        outcome.brace_bands_dropped_in_flare += 1;
    };
    let planned = k;
    let mut k = k;
    let mut moved = false;
    // Each turn either breaks or raises `k` strictly, and `k` may not pass
    // `top_limit`, so the loop ends.
    let (a_cell_top, b_cell_top, depth, k_lo) = loop {
        let (a_top, b_top) = {
            let (Some(one), Some(two)) = (struts.get(a as usize), struts.get(b as usize)) else {
                return None;
            };
            (one.cell_at(k), two.cell_at(k))
        };
        let gap_mm = crate::cell_distance_mm(grid, a_top, b_top);
        if gap_mm <= 0.0 {
            return None;
        }
        let depth = context.lean.layers_to_reach(gap_mm);
        if depth == 0 {
            return None;
        }
        let Some(k_lo) = k.checked_sub(depth) else {
            if moved {
                dropped(outcome);
            } else {
                outcome.brace_bands_refused += 1;
            }
            return None;
        };
        if k_lo >= floor {
            break (a_top, b_top, depth, k_lo);
        }
        let raised = k + (floor - k_lo);
        if !may_move || raised > top_limit {
            if may_move {
                // How far above the layer it was planned at the band's top
                // would have to stand.
                state.flare_shortfall.set(Some(raised - planned));
            }
            dropped(outcome);
            return None;
        }
        k = raised;
        moved = true;
    };
    if moved {
        let plane_holds = plane_between(grid, a_cell_top, b_cell_top).is_some_and(|plane| {
            let ties = state.ties.get(a as usize).map_or(&[][..], Vec::as_slice);
            new_plane_holds(ties, k, plane, run.sway_deg)
        });
        if k_lo >= run.k_top || !plane_holds {
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
        outcome.brace_bands_refused += 1;
        return None;
    }
    // Each diagonal's own lateral move, measured between the cells it actually
    // joins rather than assumed equal to the gap at the top of the band.
    let first_lateral = crate::cell_distance_mm(grid, a_cell_top, b_cell_lo);
    let second_lateral = crate::cell_distance_mm(grid, b_cell_top, a_cell_lo);
    if context.lean.descent_cost_mm(depth, first_lateral).is_none()
        || context.lean.descent_cost_mm(depth, second_lateral).is_none()
    {
        outcome.brace_bands_refused += 1;
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
        return None;
    }
    let Some(first) = segment_path(context, index, a_cell_top, k, b_cell_lo, k_lo, Some(b)) else {
        outcome.brace_bands_refused += 1;
        return None;
    };
    let Some(second) = segment_path(context, index, b_cell_top, k, a_cell_lo, k_lo, Some(a)) else {
        outcome.brace_bands_refused += 1;
        return None;
    };
    if context.member_clearance {
        let h = context.layer_height_mm;
        let point =
            |cell: usize, layer: usize| crate::member_clearance::axis_point(grid, h, cell, layer);
        let diagonals = [
            (point(a_cell_top, k), point(b_cell_lo, k_lo)),
            (point(b_cell_top, k), point(a_cell_lo, k_lo)),
        ];
        for (top, bottom) in diagonals {
            if crate::member_clearance::model_intrusion(
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
                outcome.brace_bands_refused += 1;
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
                outcome.brace_bands_refused += 1;
                outcome.brace_bands_refused_by_member += 1;
                return None;
            }
        }
    }
    if moved {
        outcome.brace_bands_moved_above_flare += 1;
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
    })
}

/// **Member against member**: whether the solid of radius `radius_mm` about
/// `from`–`to` stays out of every standing member's DRAWN solid, other than
/// the `posts` it joins and the members joined to those posts (a branch that
/// lands on one, a diagonal that seats or lands on one, the trunk one stands
/// on): those meet it at a planned joint. One pass over the members, each an
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
    let joined = |other: u32| {
        if posts.contains(&other) {
            return true;
        }
        let Some(member) = struts.get(other as usize) else {
            return true;
        };
        posts.iter().any(|&post| {
            rests_on(member, post)
                || struts
                    .get(post as usize)
                    .is_some_and(|host| rests_on(host, other))
        })
    };
    let solids = struts
        .iter()
        .enumerate()
        .filter(|(_, s)| live(s))
        .map(|(index, strut)| crate::member_clearance::MemberSolid {
            id: strut.id,
            from: crate::member_clearance::axis_point(grid, h, strut.base_cell(), strut.k_bottom),
            to: crate::member_clearance::axis_point(grid, h, strut.cell, strut.k_top),
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
    });
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
        }
    }

    /// REVIEW-FIXES 2026-09-29: a cross raises its post's DRAWN root, and the
    /// lane's placed root disc grows with it, so a later mast or thickening
    /// is cleared against the foot the file will draw. The reviewer's case: a
    /// 0.05 mm plate arm 0.50 mm from a 0.20 mm root (flare 1.6: 0.40 mm, clear)
    /// takes a 0.20 mm cross to a third post; its drawn root is then the
    /// brace's 0.20 mm (the junction clause), so its disc is 0.32 mm and it
    /// laps its neighbour (0.64 mm > 0.50 mm). The expected numbers are
    /// hand-derived from the inputs; the invariant is checked against the
    /// discs rebuilt from scratch by `congestion::root_discs`.
    #[test]
    fn a_cross_grows_its_posts_plate_disc_to_the_drawn_root() {
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
            shaft_landings: true,
            merge: true,
            brace: true,
            thicken: true,
            mast: true,
            max_struts: 1_000,
            x_allowed: 12.0,
            seat_faces: crate::tip_landing::SeatFaceRule::from_overhang_band(45.0, 25.0),
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
        };
        let mut outcome = super::SlendernessOutcome::default();
        super::commit_cross(&context, &mut state, &mut struts, plan, &mut outcome);

        assert!(
            (disc(&state, 0) - 0.20 * flare).abs() < 1e-12,
            "the braced post's disc is {} mm; the file draws its root at 0.20 mm, a {} mm foot",
            disc(&state, 0),
            0.20 * flare
        );
        assert_eq!(outcome.cross_roots_grown, 1, "only P's root grew");
        assert_eq!(outcome.cross_roots_lapping, 1, "P's grown foot laps Q's");
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
            shaft_landings: true,
            merge: true,
            brace: true,
            thicken: true,
            mast: true,
            max_struts: 1_000,
            x_allowed: 12.0,
            seat_faces: crate::tip_landing::SeatFaceRule::from_overhang_band(45.0, 25.0),
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
        assert_eq!(outcome.brace_bands_moved_above_flare, 1);
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
        let (profiles, _) = crate::adapter::drawn_profiles(std::slice::from_ref(post));
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
            shaft_landings: true,
            merge: true,
            brace: true,
            thicken: true,
            mast: true,
            max_struts: 1_000,
            x_allowed: 12.0,
            seat_faces: crate::tip_landing::SeatFaceRule::from_overhang_band(45.0, 25.0),
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
        assert_eq!(
            outcome.brace_bands_moved_above_flare, 1,
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
        let (profiles, _) = crate::adapter::drawn_profiles(&struts);
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
}
