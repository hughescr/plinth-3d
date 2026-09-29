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
//!    truss and holds only in its own plane**, and the owner's ruling is that
//!    bracing must hold in EVERY direction: a run is measured across the plane
//!    of every cross that ties its member ([`unbraced_runs`]), so a tie needs
//!    crosses in two planes at least 45° apart. Every partner in reach is
//!    tried, nearest first, not only the nearest.
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
    /// The plane this run is measured ACROSS, or `None` when no cross ties
    /// the member at all and every direction sees the same ties.
    ///
    /// An `|X|` holds only in its own plane. A member tied by crosses is
    /// measured once across each of their planes (grid steps from one post to
    /// the other, sign-normalised), counting every tie except the crosses
    /// within 45° of that plane (`holds_across`); the run it names is the one
    /// that sways in the direction that plane cannot hold.
    pub across: Option<[i64; 2]>,
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
    /// Remedy 2: crosses built as a PAIR in two planes 45° or more apart for a run
    /// no cross tied yet, and single crosses that completed a second plane on
    /// a run one plane already held.
    pub cross_pairs_added: usize,
    pub crosses_completing_a_plane: usize,
    /// Crosses built to a partner that was NOT the nearest in reach — the
    /// nearest's band was refused or its plane was too close to one the run
    /// already has. A path-hit
    /// witness for "try more than one partner".
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
    /// The narrowest angle, in degrees, between the two planes of a tie the
    /// lane closed with crosses; `None` when it closed none. Never below 45°
    /// by construction (`holds_across`); published so the owner can see how
    /// close to that bound the ties sit.
    pub narrowest_tie_plane_deg: Option<f64>,
    /// Remedy 3: members whose root the lane raised (tip kept), and the
    /// refusals: a root past the maximum radius, or a grown plate root whose
    /// flared disc would touch another root.
    pub members_thickened: usize,
    pub thicken_refused_radius: usize,
    pub thicken_refused_disc: usize,
    /// Trunks raised under a thickened member, so no member stands on one
    /// thinner than itself.
    pub trunks_raised_under_thickening: usize,
    /// Remedy 4: plate cells refused because the braced panel a mast there
    /// would make cannot hold the arm to one pixel even at the maximum mast
    /// radius; masts EXTENDED up to a later tie instead of a new mast; and
    /// half ties (one plane) placed because nothing better was admissible.
    pub mast_cells_refused_by_panel: usize,
    pub masts_extended: usize,
    pub half_ties: usize,
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
/// plane, so the member is measured once across each distinct plane of the
/// crosses that tie it, counting only the ties that hold across that plane —
/// every landing and end, and every cross at least 45° from it (see
/// `holds_across` and [`UnbracedRun::across`]). A member no cross ties is measured once. Runs that
/// come out identical in two directions are reported once.
#[must_use]
pub fn unbraced_runs(
    grid: &Grid,
    struts: &[SweepStrut],
    layer_height_mm: f64,
    law: &SlendernessLaw,
) -> Vec<UnbracedRun> {
    let (profiles, _) = crate::adapter::drawn_profiles(struts);
    let ties = tie_lists(grid, struts);
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

/// Exact integer test: two planes are the same line. Used only to list the
/// distinct directions a member is measured across.
fn parallel(a: Plane, b: Plane) -> bool {
    a[0] * b[1] - a[1] * b[0] == 0
}

/// Whether a cross in plane `tie` holds a post ACROSS plane `across`: the
/// cross's own direction projects at least as far across `across` as along
/// it, `|across × tie| ≥ |across · tie|` — the two planes stand at least 45°
/// apart.
///
/// No tuned number: this is the bisector of the two projections, the point at
/// which a cross stops being mostly the SAME tie as `across` and starts being
/// mostly a different one. A rigid-tie reading (any plane the grid can tell
/// apart counts) closed ties on the Cylinder with planes 0.1° apart, which hold
/// nothing across either; the measured narrowest angle is still published.
/// Integer arithmetic on grid steps, so it has no float noise either.
fn holds_across(tie: Plane, across: Plane) -> bool {
    let cross = (across[0] * tie[1] - across[1] * tie[0]).abs();
    let dot = (across[0] * tie[0] + across[1] * tie[1]).abs();
    cross >= dot
}

#[expect(
    clippy::cast_precision_loss,
    reason = "grid steps are bounded by the grid's own dimensions and exact in f64"
)]
fn plane_angle_deg(a: Plane, b: Plane) -> f64 {
    let (ax, ay, bx, by) = (a[0] as f64, a[1] as f64, b[0] as f64, b[1] as f64);
    let cosine = ((ax * bx + ay * by).abs() / (ax.hypot(ay) * bx.hypot(by))).min(1.0);
    cosine.acos().to_degrees()
}

/// One lateral tie on one member: the layer, and the plane it holds in
/// (`None`: every direction, as a landing or the member's own end does).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Tie {
    layer: usize,
    plane: Option<Plane>,
}

fn live(strut: &SweepStrut) -> bool {
    strut.k_bottom != usize::MAX && strut.k_top > strut.k_bottom
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
fn tie_lists(grid: &Grid, struts: &[SweepStrut]) -> Vec<Vec<Tie>> {
    let mut ties: Vec<Vec<Tie>> = vec![Vec::new(); struts.len()];
    for strut in struts {
        if !live(strut) {
            continue;
        }
        let plane = if strut.kind == StrutKind::Brace {
            brace_plane(grid, struts, strut)
        } else {
            None
        };
        if let Landing::Strut { id, layer } = strut.landing {
            if let Some(list) = ties.get_mut(id as usize) {
                list.push(Tie { layer, plane });
            }
        }
        if let Some(parent) = strut.seat_on_strut {
            if let Some(list) = ties.get_mut(parent as usize) {
                list.push(Tie {
                    layer: strut.k_top,
                    plane,
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
    let mut planes: Vec<Plane> = Vec::new();
    for tie in ties {
        if let Some(plane) = tie.plane {
            if !planes.iter().any(|known| parallel(*known, plane)) {
                planes.push(plane);
            }
        }
    }
    let directions: Vec<Option<Plane>> = if planes.is_empty() {
        vec![None]
    } else {
        planes.into_iter().map(Some).collect()
    };
    let mut runs: Vec<UnbracedRun> = Vec::new();
    let mut layers: Vec<usize> = Vec::with_capacity(ties.len() + 2);
    for across in directions {
        layers.clear();
        layers.push(strut.k_bottom);
        layers.push(strut.k_top);
        for tie in ties {
            let holds = match (tie.plane, across) {
                (Some(plane), Some(across)) => holds_across(plane, across),
                _ => true,
            };
            if holds && tie.layer >= strut.k_bottom && tie.layer <= strut.k_top {
                layers.push(tie.layer);
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
                across,
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
    pub thicken: bool,
    pub mast: bool,
    pub max_struts: usize,
    /// `X_ALLOWED`, the registered area-law ratio. Read only to keep a trunk
    /// under a forced merge sized for the load it was just handed, with the
    /// down-sweep's own rule, until the down-sweep re-sizes the field.
    pub x_allowed: f64,
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
    let mut queued = vec![false; struts.len()];
    // BRANCHES BEFORE THEIR TRUNKS. Thickening a member raises every trunk it
    // stands on, from the junction down to that trunk's root; a trunk tied
    // BEFORE one of its branches was thickened could be left carrying a cross
    // that no longer holds anything. So the members that stand deepest in the
    // landing tree go first, and a trunk is measured after every raise its
    // branches will hand it. `depth` is one walk down the landings per
    // member, bounded by the strut count.
    let depth_of = |strut: &SweepStrut| {
        let mut depth = 0_usize;
        let mut at = strut.landing;
        while let Landing::Strut { id, .. } = at {
            depth += 1;
            if depth > struts.len() {
                break;
            }
            at = struts.get(id as usize).map_or(Landing::Plate, |host| host.landing);
        }
        depth
    };
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
            first.push((depth_of(strut), strut.id));
            queued[index] = true;
        }
    }
    first.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let mut queue: VecDeque<u32> = first.into_iter().map(|(_, id)| id).collect();
    let mut exhausted: BTreeSet<RunKey> = BTreeSet::new();
    let mut applied = 0_usize;    'members: while let Some(id) = queue.pop_front() {
        let index = id as usize;
        if let Some(flag) = queued.get_mut(index) {
            *flag = false;
        }
        for _attempt in 0..=context.max_struts {
            if struts.len() + 2 > context.max_struts {
                break 'members;
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
            // THE HIGHEST RUN FIRST. Thickening a run raises the member all
            // the way down to its foot, so a cross placed first on a run
            // below it could be left holding nothing; nothing done to a
            // lower run changes a higher one.
            .max_by_key(|run| (run.k_lo, run.k_top));
            let Some(run) = run else {
                continue 'members;
            };            // The first of the ties the run needs, evenly spaced; the member is
            // measured again before the next one.
            let k = tie_layer(&run, 1, extra_ties(&run).max(1));
            let mut one_plane: Option<CrossPlan> = None;
            if context.brace {
                if let Some(k) = k {
                    match tie_by_crosses(context, &mut state, struts, &run, k, outcome) {
                        Ok(()) => {
                            applied += 1;
                            continue;
                        }
                        Err(half) => one_plane = half,
                    }
                }
            }
            if context.thicken && thicken(context, &mut state, struts, &run, outcome) {
                applied += 1;
                continue;
            }
            if context.mast {
                if let Some(k) = k {
                    if let Some(new_mast) =
                        tie_by_mast(context, &mut state, struts, &run, k, one_plane, outcome)
                    {
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
            // No admissible remedy: the run stays, and the census at the end
            // of the sweep names it as a warning.
            exhausted.insert(run_key(&run));
        }
    }
    prune_unneeded_crosses(context, &state, struts, first_new, outcome);
    applied
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
            && one.seat_on_strut.is_some_and(|seat| {
                matches!(two.landing, Landing::Strut { id, .. } if id == seat)
            });
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
                if let Some(position) = list.iter().position(|tie| *tie == Tie { layer, plane }) {
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
    let mut new_id: Vec<u32> = (0..len).map(|id| u32::try_from(id).unwrap_or(u32::MAX)).collect();
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

type RunKey = (u32, usize, usize, Option<Plane>);

fn run_key(run: &UnbracedRun) -> RunKey {
    (run.strut_id, run.k_lo, run.k_top, run.across)
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
}

impl TieState {
    fn build(context: &SlendernessContext<'_>, struts: &[SweepStrut]) -> Self {
        let mut index = ColumnHistory::new(context.grid);
        stamp_all(&mut index, struts);
        let (profiles, _) = crate::adapter::drawn_profiles(struts);
        let ties = tie_lists(context.grid, struts);
        let mut bands: BTreeMap<(u32, u32), Vec<(usize, usize)>> = BTreeMap::new();
        let mut masts_of: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for strut in struts {
            if strut.kind != StrutKind::Brace || !live(strut) {
                continue;
            }
            let (Some(seat), Landing::Strut { id: base, .. }) = (strut.seat_on_strut, strut.landing)
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
        let placed_roots = crate::congestion::root_discs(
            context.grid,
            context.history,
            struts,
            context.flare_factor,
        );
        Self {
            index,
            profiles,
            ties,
            bands,
            masts_of,
            placed_roots,
        }
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

fn note_plane_angle(outcome: &mut SlendernessOutcome, one: Option<Plane>, two: Option<Plane>) {
    if let (Some(one), Some(two)) = (one, two) {
        let angle = plane_angle_deg(one, two);
        outcome.narrowest_tie_plane_deg = Some(
            outcome
                .narrowest_tie_plane_deg
                .map_or(angle, |known| known.min(angle)),
        );
    }
}

/// Remedy 2: tie the run's member at layer `k` with crosses that hold it in
/// every direction.
///
/// A run measured ACROSS a plane already needs one more cross, in a plane at
/// least 45° from that one. A run no cross ties yet needs TWO, in planes at
/// least 45° apart, built together or not at all — one cross alone holds its
/// own plane only, and a tie the lane would then have to finish by thickening
/// would leave the cross redundant. Every partner in reach is tried, nearest
/// first (the stilt plate's arm 81: its nearest band crossed a leg, and arm 65,
/// 0.008 mm further away, was never tried).
///
/// On failure, the first cross that WOULD build is handed back, so the mast
/// rung can use it as one plane if nothing better is admissible.
fn tie_by_crosses(
    context: &SlendernessContext<'_>,
    state: &mut TieState,
    struts: &mut Vec<SweepStrut>,
    run: &UnbracedRun,
    k: usize,
    outcome: &mut SlendernessOutcome,
) -> Result<(), Option<CrossPlan>> {
    let id = run.strut_id;
    let Some(here) = struts.get(id as usize).map(|strut| strut.cell_at(k)) else {
        return Err(None);
    };
    let candidates = partner_candidates(context, struts, id, k);
    let mut planned: Vec<CrossPlan> = Vec::new();
    for (rank, &(partner, cell)) in candidates.iter().enumerate() {
        let Some(plane) = plane_between(context.grid, here, cell) else {
            continue;
        };
        if run.across.is_some_and(|across| !holds_across(plane, across)) {
            continue;
        }
        outcome.cross_bands_planned += 1;
        let Some(plan) = plan_cross(context, &state.index, struts, &state.bands, id, partner, k, outcome)
        else {
            continue;
        };
        let plan_plane = plan.plane.or(Some(plane));
        if run.across.is_some() {
            note_plane_angle(outcome, run.across, plan_plane);
            if rank > 0 {
                outcome.crosses_to_a_later_partner += 1;
            }
            commit_cross(context, state, struts, plan);
            outcome.braces_added += 1;
            outcome.crosses_completing_a_plane += 1;
            return Ok(());
        }
        let second_plane = planned.iter().position(|earlier| {
            matches!((earlier.plane, plan_plane), (Some(one), Some(two)) if holds_across(one, two))
                && !paths_collide(earlier, &plan)
        });
        if let Some(position) = second_plane {
            let first = planned.swap_remove(position);
            note_plane_angle(outcome, first.plane, plan_plane);
            outcome.crosses_to_a_later_partner += 1;
            commit_cross(context, state, struts, first);
            commit_cross(context, state, struts, plan);
            outcome.braces_added += 2;
            outcome.cross_pairs_added += 1;
            return Ok(());
        }
        planned.push(plan);
    }
    Err(planned.into_iter().next())
}

/// Whether any diagonal of `second` shares a cell AT THE SAME LAYER with a
/// diagonal of `first` — the test the shaft index would have made had `first`
/// been built before `second` was planned.
fn paths_collide(first: &CrossPlan, second: &CrossPlan) -> bool {
    let cell_at = |plan: &CrossPlan, path: &[u32], layer: usize| {
        layer
            .checked_sub(plan.k_lo)
            .and_then(|slot| path.get(slot))
            .copied()
    };
    for path in [&second.first, &second.second] {
        for (slot, cell) in path.iter().enumerate().skip(1) {
            let layer = second.k_lo + slot;
            for other in [&first.first, &first.second] {
                if cell_at(first, other, layer) == Some(*cell) {
                    return true;
                }
            }
        }
    }
    false
}

/// Build a planned cross and bring every list the tie rungs read up to date.
fn commit_cross(
    context: &SlendernessContext<'_>,
    state: &mut TieState,
    struts: &mut Vec<SweepStrut>,
    plan: CrossPlan,
) {
    let grid = context.grid;
    let span = plan.k - plan.k_lo;
    let (a, b, k, k_lo, radius, plane) = (plan.a, plan.b, plan.k, plan.k_lo, plan.radius_mm, plan.plane);
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
            list.push(Tie { layer: k, plane });
            list.push(Tie { layer: k_lo, plane });
        }
        // The junction clause the lowering applies: the post is drawn at
        // least as thick as the diagonal, from the joint down to its root.
        if let (Some(strut), Some(profile)) =
            (struts.get(post as usize), state.profiles.get_mut(post as usize))
        {
            let top = k.saturating_sub(strut.k_bottom);
            for radius_here in profile.iter_mut().take(top + 1) {
                *radius_here = radius_here.max(radius);
            }
        }
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
    let disc_growth = {
        let strut = &struts[foot];
        if matches!(strut.landing, Landing::Plate) && strut.kind != StrutKind::Brace {
            let current = drawn_at(strut, &state.profiles[foot], strut.k_bottom).unwrap_or(0.0);
            if root > current {
                let flared = root * flare_of(context);
                let slot = crate::congestion::disc_slot(&state.placed_roots, strut.id);
                let grows_past_own = slot
                    .and_then(|slot| crate::congestion::disc_radius_mm(&state.placed_roots, slot))
                    .is_none_or(|known| flared > known);
                if grows_past_own
                    && !crate::congestion::clear_of_placed(
                        grid,
                        strut.base_cell(),
                        flared,
                        &state.placed_roots,
                        slot,
                    )
                {
                    outcome.thicken_refused_disc += 1;
                    return false;
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
    let couple = gap_mm * gap_mm
        / (h * (1.0 / arm_a + 1.0 / mast_a) + diagonal / (2.0 * brace_a * sin_sq));
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

/// Remedy 4: a mast, tied to the arm by a cross.
///
/// * A run no cross ties yet takes the one plane the crosses COULD give first
///   (`one_plane`), because it costs no new member; the member is then
///   measured across that plane and comes back here only if nothing else
///   closes it.
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
fn tie_by_mast(
    context: &SlendernessContext<'_>,
    state: &mut TieState,
    struts: &mut Vec<SweepStrut>,
    run: &UnbracedRun,
    k: usize,
    one_plane: Option<CrossPlan>,
    outcome: &mut SlendernessOutcome,
) -> Option<Option<u32>> {
    let grid = context.grid;
    let law = context.law;
    let id = run.strut_id;
    if run.across.is_none() {
        if let Some(plan) = one_plane {
            commit_cross(context, state, struts, plan);
            outcome.braces_added += 1;
            outcome.half_ties += 1;
            return Some(None);
        }
    }
    let here = struts.get(id as usize)?.cell_at(k);
    let admissible = |plane: Option<Plane>| match (plane, run.across) {
        (Some(plane), Some(across)) => holds_across(plane, across),
        (Some(_), None) => true,
        (None, _) => false,
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
        let (old_top, cell) = (standing.k_top, standing.cell);
        let spans_before = state.index.shaft_spans.len();
        if old_top < k {
            let blocked = (old_top + 1..=k).any(|layer| {
                !context.keep_out.admits(cell, layer)
                    || state.index.shaft_blocks(cell, layer, None)
                    || context.history.model_blocks(cell, layer)
            });
            if blocked {
                continue;
            }
            state.index.stamp_shaft(mast, old_top + 1, k, cell, &[]);
            struts[mast as usize].k_top = k;
        }
        if let Some(plan) =
            plan_cross(context, &state.index, struts, &state.bands, id, mast, k, outcome)
        {
            if old_top < k {
                let profile = &mut state.profiles[mast as usize];
                let root = profile.first().copied().unwrap_or(0.0);
                profile.resize(k + 1, root);
                outcome.masts_extended += 1;
            }
            commit_cross(context, state, struts, plan);
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
            // THE MAST'S OWN RADIUS at this candidate: its slenderness has to
            // hold from the plate up to the bottom of the band the cross to
            // THIS cell spans, `r_m = max(r_min, H_first / S)`.
            let depth = context.lean.layers_to_reach(distance_mm);
            if depth == 0 || depth > k {
                continue;
            }
            #[expect(
                clippy::cast_precision_loss,
                reason = "a layer count bounded by the plan's own layer count is exact in f64"
            )]
            let first_brace_mm = (k - depth) as f64 * context.layer_height_mm;
            candidates.push((distance_mm, cell, law.brace_radius_mm(first_brace_mm)));
        }
    }
    candidates.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let flare = flare_of(context);
    let chosen = {
        let plate = crate::congestion::PlacedIndex::build(&state.placed_roots, None);
        let mut chosen = None;
        for (distance_mm, cell, own_radius) in candidates {
            let sway = |mast_mm: f64| {
                panel_sway_over_pixel(law, run.length_mm, run.radius_mm, tie_mm, mast_mm, distance_mm)
            };
            if sway(law.r_max_mm) > 1.0 {
                outcome.mast_cells_refused_by_panel += 1;
                continue;
            }
            let radius = if sway(own_radius) <= 1.0 {
                own_radius
            } else {
                let (mut low, mut high) = (own_radius, law.r_max_mm);
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
            if context.mast_root_clearance && !plate.clear(grid, cell, radius * flare) {
                continue;
            }
            if (0..=k).any(|layer| !context.keep_out.admits(cell, layer)) {
                continue;
            }
            chosen = Some((cell, radius));
            break;
        }
        chosen
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
        seat_solid_layers: 0,
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
    if let Some(plan) = plan_cross(context, &state.index, struts, &state.bands, id, mast_id, k, outcome)
    {
        commit_cross(context, state, struts, plan);
        crate::congestion::push_disc(
            &mut state.placed_roots,
            grid,
            mast_id,
            cell,
            radius_mm * flare,
            radius_mm,
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
#[expect(
    clippy::too_many_arguments,
    reason = "a band needs the context, the live index and struts, the standing bands, both posts, \
          its layer and the outcome it counts refusals in"
)]
fn plan_cross(
    context: &SlendernessContext<'_>,
    index: &ColumnHistory,
    struts: &[SweepStrut],
    bands: &BTreeMap<(u32, u32), Vec<(usize, usize)>>,
    a: u32,
    b: u32,
    k: usize,
    outcome: &mut SlendernessOutcome,
) -> Option<CrossPlan> {
    let grid = context.grid;
    let (a_cell_top, b_cell_top) = {
        let (Some(one), Some(two)) = (struts.get(a as usize), struts.get(b as usize)) else {
            return None;
        };
        (one.cell_at(k), two.cell_at(k))
    };
    let gap_mm = crate::cell_distance_mm(grid, a_cell_top, b_cell_top);
    if gap_mm <= 0.0 {
        return None;
    }
    let depth = context.lean.layers_to_reach(gap_mm);
    if depth == 0 {
        return None;
    }
    let Some(k_lo) = k.checked_sub(depth) else {
        outcome.brace_bands_refused += 1;
        return None;
    };
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
        seat_solid_layers: 0,
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
            across: None,
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
            assert!((r - 0.4).abs() < 1.0e-12, "{n} layers of 0.4 mm read as {r}");
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
}
