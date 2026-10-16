//! **THE FOOTPRINT RULE: a tip's whole contact disc lies ON its seat face**
//! (fix/tip-trim, 2026-09-28).
//!
//! # The defect
//!
//! `Round 25` at the flip seats rim tips within 0.03 mm of the disc's knife
//! edge, on arms that lean OUTWARD (the congestion lane's lean). Their ball
//! centres ended up 0.02-0.07 mm OUTSIDE the silhouette and about 58 % of each
//! contact disc hung off the face (`plinth-evidence/slice-sweep-2026-09-02/
//! field-2026-09-28/tip-clip.md`, Q1). The area law's `a ≥ r_tip` guarantee is
//! evaluated without the face's edge, so it reported those tips as carrying
//! their full contact when they carried 42 % of it. Contact area is what
//! governs (owner ruling 2026-08-30); a disc half off the face is half a tip.
//!
//! # The rule
//!
//! The contact disc a tip DELIVERS — the section of its emitted ball in the
//! seat plane, which is exactly what the seat trim leaves
//! ([`plinth_support::tip_contact_discs`], the lowering's own derivation) —
//! must lie on the seat face. Where it does not, the seat moves ALONG the face
//! to the nearest position whose whole disc does. Where no such position lies
//! within the search radius, the face there is a true knife edge: the seat
//! stays, the trim still keeps every emitted solid on the support side and out
//! of the model, and a WARNING names the tip and the fraction of its disc that
//! is on the face. Emit and warn, never refuse.
//!
//! # How "on the face" is measured
//!
//! In the same layer rasters the sweep placed the tip with, using the ERODED
//! carrier mask (`carry`: full containment, eroded one cell in Y), so a
//! boundary cell never counts as face. The disc is sampled on a lattice of the
//! grid's own pitch laid in its own plane, AND ON ITS RIM (chitubox-geometry,
//! 2026-09-30): points on the disc's own circle no more than a quarter pitch
//! apart ([`RIM_SAMPLES_PER_PITCH`]). The lattice alone never looks past its
//! outermost point inside the circle, up to one pitch short of the rim, so a
//! rim that hung off the face by less than a pitch was graded on it: the
//! flipped `Round 25` under the owner's ChituBox ball ratio emitted one disc
//! 0.0153 mm off its face, graded on-face by every lattice point. A rim point
//! in a carried cell is on the face (the carrier mask is full containment);
//! between two rim points a quarter pitch apart the circle can cut a corner of
//! a cell no sample sits in, by at most its sagitta, `(pitch/4)² / 8r`, which
//! is 0.0004 mm at the smallest registered tip. Moves are verified the same
//! way: the disc the lowering re-derives for a moved seat is sampled, lattice
//! and rim, in one more batched pass per round.
//!
//! **A sample is ON the face when the REAL model surface is inside the drawn
//! ball over it** (Craig 2026-10-12: the tip is cut to the model mesh, so the
//! file has contact there exactly then). Along the seat normal through the
//! sample, the ball spans a chord; the sample is on the face when the model
//! surface crosses that chord, within one pixel (the printer cannot part two
//! solids closer than that). ONE batched ray cast against the model's surface
//! index grades every sample of a pass ([`touching`]); the search lattice, a
//! prefilter only, reads the ball's widest chord. Until 2026-10-12 a sample
//! was on the face when the model was carried in its raster column within two
//! laminae plus `2 · pitch · tan(slope)` above it: a disc hanging 0.1 mm
//! under a curved underside passed, and 14 of 29 Bat_Cat_Owl discs graded
//! "on their face" hung up to 0.17 mm under it. That raster window survives
//! only where the surface index is absent (its memory cap) or the cast is
//! refused ([`carried`]).
//!
//! # The one pruning bound (a disclosed Placeholder, it decides no outcome)
//!
//! The seat may move along the face by at most
//! `SEARCH_RADIUS_BALL_DIAMETERS · 2R + e + 2 · pitch`: the tip's own ball
//! diameter, plus `e`, how far the arm's lean put the disc centre from the
//! seat (the offset that pushed it off the face), plus the carrier mask's own
//! two-cell inward bound (a disc graded ON the face must sit that far inside
//! the raster's edge, whatever the tip's size). It only bounds how far the
//! search looks. A move further than that carries the tip off the feature the
//! reach criterion placed it for; beyond it the tip is left where it is and
//! warned about rather than moved somewhere nobody asked for support. (A first
//! cut used `2R` alone; on a minimum-radius tip that is smaller than the
//! raster's own two-cell margin, and every such rim tip was misreported as a
//! knife edge.)
//!
//! # A move is held to the rules of the finished field (review-fixes 2026-09-29)
//!
//! This rule runs after the lean law, the slenderness lane and the drain
//! keep-out have been satisfied, and a move changes the geometry the file
//! gets: the moved arm's axis and every member joined to it. So every round's
//! on-face moves, together with the moves already accepted, are judged on the
//! DRAWN ends of every member they displace ([`MoveLimits`]): lateral travel
//! per unit drop within the lean law, the drawn chord within its tightest
//! unbraced run's limit, and no new lamina in the drain keep-out — each "or no
//! worse than before the move" for a member already past it. A move a broken
//! rule traces back to is refused and its tip tries its next candidate; a tip
//! whose every on-face candidate is refused keeps its seat, trimmed, with a
//! WARNING (`Footprint::MoveRefused`), never a refusal. Measured on the
//! flipped `Round 25` fixture: 7 proposals refused by the lean law, every
//! tip still moved onto its face by a later candidate, and without the check
//! two members left leaning at 1.575 and 1.586 against the law's 1.5697.
//!
//! A move must also keep the support the tip was placed to give (joint review
//! P2, 2026-10-12): the moved ball must still hold the original seat, to
//! within one pixel, or the slide must not climb (the moved disc's lowest
//! point within one layer of the placed disc's) ([`keeps_seat`]). A disc
//! wholly on some face higher up is not
//! support for the material under the seat; a candidate that leaves it is
//! refused like a move that breaks a rule, and a tip with none left keeps its
//! seat as a [`Footprint::MoveRefused`].
//!
//! # Batching
//!
//! Every sample of every tip is graded in ONE ascending pass over the layer
//! stack (each needed layer rasterised once); the search lattice for the tips
//! that fail is graded in one more such pass; every candidate move is
//! re-derived through ONE batched [`plinth_support::tip_contact_discs`] call
//! per round, and judged by ONE displacement pass over the field per round
//! (repeated only when a broken rule removes a move). No per-tip raster query,
//! no per-tip registry read.

use geom_kernel::NativeSurface;
use plinth_core::LayerPlan;
use plinth_support::perception::SliceSolid;
use plinth_support::{SweepArm, TipContactDisc};
use rayon::prelude::*;

use crate::drain::DrainKeepOut;
use crate::grid::Grid;
use crate::raster::{rasterize_into, LayerMasks, RasterScratch};
use crate::{Footprint, Landing, SweepStrut};

/// How far the seat may move along its face, in ball diameters. A Placeholder
/// that only prunes the search (see the module doc); it never decides whether
/// a disc is on its face.
pub const SEARCH_RADIUS_BALL_DIAMETERS: f64 = 1.0;

/// Rim samples per grid pitch of the disc's circumference: a quarter-pitch
/// spacing, a resolution of the raster itself and not a physical number. The
/// circle cuts a cell corner no rim sample sits in by at most the sagitta of a
/// quarter-pitch chord (see the module doc).
pub const RIM_SAMPLES_PER_PITCH: f64 = 4.0;

/// What the footprint rule did to one field.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FootprintOutcome {
    /// Seated tips whose contact disc was graded.
    pub tips_graded: usize,
    /// Of those, discs already wholly on their face.
    pub tips_on_face: usize,
    /// Tips whose seat was moved along the face so its whole disc lies on it.
    pub tips_moved_onto_face: usize,
    /// The longest such move, in mm.
    pub max_move_mm: f64,
    /// Members joined to a moved member's axis (a branch landing on it, a
    /// brace topped on it, and so on down the tree) that were moved WITH it
    /// so they stay on it ([`followed_shifts`]), and the longest such move.
    pub members_following_a_moved_host: usize,
    pub max_follow_mm: f64,
    /// Tips on a true knife edge: no seat within the search radius puts the
    /// whole disc on the face. `(arm, fraction of disc samples on the face)`.
    pub knife_edges: Vec<(usize, f64)>,
    /// Tips whose disc hangs off its face where every on-face seat the search
    /// found would have broken a rule of the finished field ([`MoveLimits`]):
    /// `(arm, fraction of disc samples on the face)`. The tip keeps its seat,
    /// trimmed, and is warned about. Not a knife edge: the face is there.
    pub moves_refused: Vec<(usize, f64)>,
    /// HEADED tips whose disc hangs off its face: never slid (a head is frozen
    /// at placement), so they keep their seat as a [`Footprint::MoveRefused`]
    /// and are warned about with the fraction of the disc on the face. Also
    /// listed in `moves_refused`.
    pub headed_kept: usize,
    /// Proposed moves refused, by the rule the moved geometry (the arm, or a
    /// member joined to it) would have broken, and the member checks run: the
    /// path hits of the move check (review-fixes 2026-09-29).
    pub moves_refused_lean: usize,
    pub moves_refused_slenderness: usize,
    pub moves_refused_drain: usize,
    pub members_checked: usize,
    /// On-face candidates and re-derived moves refused because the moved
    /// ball no longer holds the seat the tip was placed for ([`keeps_seat`],
    /// joint review P2): the path hits of that rule.
    pub moves_refused_off_seat: usize,
    /// Of those, the ones refused because the moved ball would leave model
    /// cells the placed ball held over nothing ([`keeps_ground`], fix lane A
    /// 2026-10-12): the path hits of that check.
    pub moves_refused_off_ground: usize,
    /// THE LIP-SEAT RULE's remedy 2 (2026-09-29): tips whose disc was on its
    /// face but whose shaft the seat trim cut outside its ball, searched for a
    /// seat further along the same face; those moved there; and slides of
    /// such tips refused because the moved shaft would still be cut. A tip
    /// whose disc is OFF its face is never refused for its shaft: contact area
    /// is the only seat criterion.
    pub tips_graded_for_section: usize,
    pub tips_moved_for_section: usize,
    pub moves_refused_section: usize,
    /// The ids of the members moved for their section, in search order.
    pub moved_for_section: Vec<u32>,
    /// Section moves taken back because, drawn with every accepted move's
    /// follow applied, the shaft was still cut (`settle`).
    pub section_moves_taken_back: usize,
    /// Moves taken back AFTER a section move was taken back, because the
    /// field left behind broke the lean law, an unbraced run's limit or the
    /// drain keep-out for a member they displace (`settle`,
    /// round5-review-fixes 2026-09-29). Also counted in `moves_refused_lean`,
    /// `moves_refused_slenderness` or `moves_refused_drain` by rule. A tip
    /// whose disc was off its face then keeps its seat and is warned about in
    /// `moves_refused`; a tip moved for its section alone keeps its seat.
    pub moves_taken_back_limits: usize,
    /// Sets of moves judged again after a take-back: the path hits of that
    /// re-check.
    pub settle_rechecks: usize,
    /// Disc samples graded, lattice points searched, layers rasterised, and
    /// candidate rounds re-derived: the path hits.
    pub disc_samples: usize,
    pub search_samples: usize,
    /// Samples of the re-derived discs of proposed moves (lattice and rim),
    /// graded before a move is accepted (chitubox-geometry, 2026-09-30).
    pub verify_samples: usize,
    /// Tips whose every LATTICE sample is on the face but a RIM sample is
    /// not: the overhangs the lattice alone graded on-face. The path-hit
    /// witness for the rim samples.
    pub tips_off_face_at_rim_only: usize,
    pub layers_rasterised: usize,
    pub rounds: usize,
    /// THE TIP COVER (lane tc, 2026-10-16): what the slides did to the
    /// support the placed tips gave, and how it was put back ([`TipCover`]).
    pub cover: TipCoverOutcome,
}

/// **What the tip-cover check found and did** (lane tc, 2026-10-16).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TipCoverOutcome {
    /// Whether the check ran (a field with no slide never asks it).
    pub enabled: bool,
    /// Reach walks run over the moved field: the path hits.
    pub checks: usize,
    /// Slides standing at the last check whose ball still holds the seat the
    /// tip was placed for (`keeps_seat`'s first clause): their roots are
    /// where they were, so they are not walked.
    pub slides_holding_seat: usize,
    /// What the FIRST check found, before any repair: surface cells (a cell
    /// whose column was not stranded one layer down), all cells including
    /// the ones inheriting a stranded cell below, the surface area in mm², and
    /// the slides blamed. Zero on a field whose slides all keep their cover.
    pub found_surface_cells: usize,
    pub found_cells: usize,
    pub found_surface_mm2: f64,
    pub found_slides: usize,
    /// The largest finite potential among the found cells on each blamed
    /// slide's lowest stranded layer (`U = 1` is the placement's 2.13 mm
    /// reach; above it a cell carries its column's potential up and adds its
    /// own run, which is not the reach the surface was left at). Reporting
    /// only; zero where every such cell is out of reach altogether.
    pub found_max_u: f64,
    /// What the first check found, one row per blamed slide (its lowest
    /// stranded layer, the surface area there, the patch's centre, its worst
    /// potential). Reporting only: where the repairs happened.
    pub found_by_slide: Vec<CoverShortfall>,
    /// Slides taken back because the field they left stranded demand, and
    /// candidates the re-search passed over because their root would not
    /// reach the cells the slide had stranded (the re-search prefilter).
    pub slides_taken_back: usize,
    pub candidates_refused: usize,
    /// Re-search passes run (bounded by [`TipCover::passes`]).
    pub passes: usize,
    /// Tips whose slide stranded demand and that found ANOTHER slide that
    /// keeps every cell rooted (repair (a): the tip reseats itself), by arm
    /// (strut index, as the other footprint lists name them).
    pub reseated: Vec<usize>,
    /// Tips whose slide was undone (repair (c)), by arm: slides searched for
    /// the tip's section (its disc was on its face before the slide), so the
    /// tip keeps its placed seat and the lip-seat census warns about its
    /// shaft. (A slide the rule of the finished field then refuses leaves the
    /// tip as a [`crate::Footprint::MoveRefused`], warned.)
    pub undone: Vec<usize>,
    /// ONTO-FACE slides (the tip's disc hung off its face before the slide)
    /// that still strand demand and that no other on-face seat replaced, by
    /// arm: kept, because undoing them would take contact area from the tip
    /// (contact area governs, owner ruling 2026-08-30), and what they strand
    /// is in `left`, warned by area.
    pub kept: Vec<usize>,
    /// THE NEIGHBOUR REPAIR: tips searched to slide over the cells a kept
    /// slide strands (each held to every rule a slide is held to, and to its
    /// shaft not being cut), and those whose slides stand at the end, by arm.
    /// Helpers stand only where the kept slide then strands nothing; else
    /// every one of its helpers is taken back.
    pub helpers_searched: usize,
    pub helpers: Vec<usize>,
    /// Shortfalls left after every repair, one per slide: what the kept
    /// onto-face slides strand (and anything the backstop left). Emitted,
    /// and warned about by area.
    pub left: Vec<CoverShortfall>,
}

/// **Demand a slide left uncovered and nothing repaired** (warned, never
/// refused).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CoverShortfall {
    /// The moved member blamed.
    pub arm: usize,
    /// The lowest layer of the patch, and its surface area (every surface
    /// cell blamed on the slide, on any layer).
    pub layer: usize,
    pub surface_mm2: f64,
    /// The centre of the patch's surface cells, in the build frame.
    pub centre_mm: [f64; 2],
    /// The worst finite potential on the patch's lowest layer (`U = 1` is the
    /// reach; zero where no root reaches any of it).
    pub max_u: f64,
    /// How far the blamed slide moved the seat, in mm.
    pub slide_mm: f64,
}

/// **THE TIP COVER CHECK** (lane tc, 2026-10-16): a slide must leave every
/// demand cell the placed tips rooted still rooted by the tips as they are
/// drawn.
///
/// # The defect
///
/// [`keeps_seat`] passes a slide that does not climb, and on a flat ceiling
/// every slide is level, so any slide within the search radius was taken.
/// Nothing re-ran the reach after it. On the approved Cylinder file one tip on
/// the underside of the lid, inside the hollow, slid 1.25 mm for its section
/// and left about 2.3 mm² of the lid's first layer up to 2.57 mm from the
/// nearest root, past the 2.13 mm reach the tips were placed for (lane unroot,
/// `perf-2026-10-14/unroot/RESULTS.md`).
///
/// # The check
///
/// After the slides have settled (`settle`), every slide whose ball let go of
/// the seat it was placed for (passed by [`keeps_seat`]'s no-climb clause
/// alone: a slide whose ball still holds its seat, [`holds_seat`], leaves the
/// tip touching the cell it was rooted at) is walked: the up-sweep's own reach
/// potential over the field twice, placed roots against slid roots
/// ([`crate::demand_replay::stranded_by_moves`]); every cell the placed roots
/// rooted and the slid ones do not is blamed on the slide that was covering
/// it. No new limit: the reach is the placement's own. A field none of whose
/// slides let go of its seat is not walked at all.
///
/// # The repair, cheapest first
///
/// 1. **The tip reseats itself (a).** The blamed tip goes on down its own
///    candidate list (the nearest on-face seats first), held to every rule a
///    slide is held to, and to one more: its root must lie within one
///    lamina's straight reach of every cell its slide had stranded on its
///    seat layer (a prefilter; the next walk is the judge). No resin is added
///    and no other tip moves. Bounded by `passes`.
/// 2. **A SECTION slide is undone (c).** A tip moved for its section alone
///    had its whole disc on the face before it moved, so its placed seat
///    gives exactly the cover it was placed for at no resin and no contact;
///    it gives up only the shaft clearance the slide bought, and the lip-seat
///    census warns about its shaft, as it does for every tip no slide could
///    clear.
/// 3. **An ONTO-FACE slide with no replacement is kept, and its NEIGHBOURS
///    are tried (a).** Its tip's disc hung off its face; undoing the slide
///    would take contact area from the tip (on Deinonychus it left one tip
///    with no real contact at all), and contact area governs (owner ruling
///    2026-08-30). The slide the rule first chose stands. Every seated,
///    unheaded tip on its face, rooted on the layer the slide strands first,
///    whose search can reach the stranded cells, is searched as a failing tip
///    is (and as a section tip is: its slide must not cut its shaft), with
///    those cells as the prefilter's targets; the next walk judges them, a
///    helper that strands its own demand is taken back like a section slide,
///    and if the kept slide still strands anything every one of its helpers
///    is taken back (a helper moves only where it covers).
/// 4. **Only then is the area warned** ([`TipCoverOutcome::left`]), the slide
///    standing. A new small tip (b) is not made here: placing one by the
///    normal path (seat, load sizing, slenderness, heads, clearance) needs a
///    re-plan with the area seeded as demand.
///
/// Each turn ends the loop or takes back, replaces or keeps a blamed slide,
/// and re-search stops after `passes` turns, so the check ends; it runs
/// serially in slot order, so it never depends on thread timing. A field none
/// of whose slides strands anything is left exactly as it was.
pub(crate) struct TipCover<'a> {
    /// One lamina's straight reach, `κ·h`, in mm: the re-search prefilter.
    pub reach_mm: f64,
    /// Re-search passes before every slide still blamed is undone.
    pub passes: u32,
    /// The walk: the field and the slid seats in, the stranded cells out.
    pub stranded: &'a mut StrandedWalk<'a>,
}

/// The tip-cover walk ([`TipCover::stranded`]): the field and its slid seats
/// in, the demand cells they strand out
/// ([`crate::demand_replay::stranded_by_moves`] in production).
pub(crate) type StrandedWalk<'a> = dyn FnMut(
        &[SweepStrut],
        &[crate::demand_replay::MovedSeat],
    ) -> Vec<crate::demand_replay::StrandedCell>
    + 'a;

/// **What a footprint move may not break** (review-fixes 2026-09-29).
///
/// The footprint rule runs on the FINISHED field, after the lean law, the
/// slenderness lane and the drain keep-out have all been satisfied, and a
/// seat move changes the geometry the file gets: the moved arm's own axis, and
/// every member joined to it ([`followed_shifts`]). So every proposed set of
/// moves is judged on the DRAWN ends ([`crate::adapter::drawn_ends`] plus the
/// displacement each member would get) against the same three rules, and a
/// move that breaks one is refused and the search goes on to its next
/// candidate. A member already past a rule before any move is held to "no
/// worse than it was", so a move can never be refused for a violation it did
/// not cause. No new number: each bound is the rule's own.
pub(crate) struct MoveLimits<'a> {
    /// The lean law's steepest member, as lateral travel per unit of drop:
    /// `max_step / layer_height` (`lean.rs`, the saturated overhang angle).
    pub max_lean: f64,
    /// Per strut (index = id): the factor its drawn chord may grow by before
    /// its tightest unbraced run passes its own limit — `min(limit / length)`
    /// over its runs on the finished field (a run's length is its share of the
    /// chord, `slenderness::run_length`). Infinite where no run binds.
    pub chord_slack: Vec<f64>,
    /// The drain keep-out the placement honoured.
    pub keep_out: &'a DrainKeepOut,
}

/// The rule a proposed move would break.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Broken {
    Lean,
    Slenderness,
    Drain,
}

/// Float noise on a comparison the geometry designs for (an arm drawn
/// exactly at the lean law's limit), and nothing more.
const RULE_RELATIVE_SLACK: f64 = 1e-9;

/// Lateral travel per unit of drop of a member drawn from `seat` to `base`.
fn lean_of((seat, base): ([f64; 3], [f64; 3])) -> f64 {
    let drop = seat[2] - base[2];
    let lateral = (seat[0] - base[0]).hypot(seat[1] - base[1]);
    if drop > 0.0 {
        lateral / drop
    } else if lateral > 0.0 {
        f64::INFINITY
    } else {
        0.0
    }
}

fn chord_of((seat, base): ([f64; 3], [f64; 3])) -> f64 {
    geom_kernel::vec3::norm(geom_kernel::vec3::sub(seat, base))
}

/// A member's drawn ends after its [`Followed`] displacement.
fn displaced(before: ([f64; 3], [f64; 3]), followed: Followed) -> ([f64; 3], [f64; 3]) {
    let moved = |point: [f64; 3], by: Option<[f64; 3]>| {
        by.map_or(point, |by| geom_kernel::vec3::add(point, by))
    };
    (
        moved(before.0, followed.seat_mm),
        moved(before.1, followed.base_mm),
    )
}

/// The first rule member `index` breaks when drawn from `after` rather than
/// `before`, if any.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_arguments,
    reason = "layer indices bounded by the plan's layer count are exact in f64; grid coordinates \
              are range-checked as f64 before the cast; the frame, the rules, and one member's \
              ends before and after"
)]
fn broken_rule(
    grid: &Grid,
    plate_z: f64,
    height: f64,
    limits: &MoveLimits<'_>,
    strut: &SweepStrut,
    index: usize,
    before: ([f64; 3], [f64; 3]),
    after: ([f64; 3], [f64; 3]),
) -> Option<Broken> {
    let allowed_lean = lean_of(before).max(limits.max_lean);
    if lean_of(after) > allowed_lean * (1.0 + RULE_RELATIVE_SLACK) {
        return Some(Broken::Lean);
    }
    let (was, now) = (chord_of(before), chord_of(after));
    let slack = limits
        .chord_slack
        .get(index)
        .copied()
        .unwrap_or(f64::INFINITY)
        .max(1.0);
    if was > 0.0 && now > was * slack * (1.0 + RULE_RELATIVE_SLACK) {
        return Some(Broken::Slenderness);
    }
    if !limits.keep_out.is_empty() && strut.k_bottom != usize::MAX {
        // The column the drawn axis stands in at each lamina it passes
        // through, sampled at the lamina's mid-plane.
        let cell_at = |(seat, base): ([f64; 3], [f64; 3]), z: f64| -> Option<usize> {
            let rise = seat[2] - base[2];
            let t = if rise > 0.0 {
                ((z - base[2]) / rise).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let x = base[0] + (seat[0] - base[0]) * t;
            let y = base[1] + (seat[1] - base[1]) * t;
            let col = grid.col_coord(x).floor();
            let row = grid.row_coord(y).floor();
            (col >= 0.0
                && row >= 0.0
                && (col as usize) < grid.cols()
                && (row as usize) < grid.rows())
            .then(|| grid.index(row as usize, col as usize))
        };
        for layer in strut.k_bottom..strut.k_top {
            let z = plate_z + (layer as f64 + 0.5) * height;
            let blocked =
                |ends| cell_at(ends, z).is_some_and(|cell| limits.keep_out.blocks(cell, layer));
            if blocked(after) && !blocked(before) {
                return Some(Broken::Drain);
            }
        }
    }
    None
}

/// Every member's drawn chord after the footprint moves, over its chord
/// before them: `1` for a member nothing moved. The run lengths the
/// slenderness lane published are shares of the chord, so this is what
/// refreshes them on the field the file gets.
pub(crate) fn chord_scales(
    grid: &Grid,
    plate_z: f64,
    height: f64,
    struts: &[SweepStrut],
) -> Vec<f64> {
    struts
        .iter()
        .zip(followed_shifts(struts))
        .map(|(strut, followed)| {
            if followed.seat_mm.is_none() && followed.base_mm.is_none() {
                return 1.0;
            }
            // The chord of the member's POST (from the joint `K` for a headed
            // arm): that is the line a follower shift tilts.
            let before = crate::adapter::drawn_post_ends(grid, plate_z, height, strut);
            let was = chord_of(before);
            if was > 0.0 {
                chord_of(displaced(before, followed)) / was
            } else {
                1.0
            }
        })
        .collect()
}

/// One point of a lattice whose column is tested for carried material.
#[derive(Clone, Copy, Debug)]
struct Sample {
    cell: Option<usize>,
    k_lo: usize,
    k_hi: usize,
    /// The ball's chord through the sample along the seat normal, from its
    /// support-side end inward ([`touching`]): the sample is on the face when
    /// the REAL model surface crosses this chord.
    chord: Chord,
}

/// A ball's chord along the seat normal: from `origin` (on the ball, support
/// side) along `inward` (the inward unit normal) for `length` mm.
#[derive(Clone, Copy, Debug)]
struct Chord {
    origin: [f64; 3],
    inward: [f64; 3],
    length: f64,
}

/// How many searched tips (or proposals) a round needs before its per-tip
/// work runs on the rayon pool. Each task is a whole tip (a candidate walk
/// with its ground checks, or a verdict with its disc samples), not a cell or
/// a sample; below this the hand-off costs more than the work.
const PARALLEL_MIN_TIPS: usize = 16;

/// One pending tip's walk down its candidate list in a round, read-only on
/// the search state, applied in tip order afterwards.
#[derive(Debug)]
struct CandidateWalk {
    slot: usize,
    /// The index of the next candidate after the walk.
    next: usize,
    /// The first candidate that passed every prefilter.
    proposal: Option<(i32, i32)>,
    /// Candidates refused for the seat they were placed for, and (of those
    /// counted) the ones refused by the ground check.
    refused_off_seat: usize,
    refused_off_ground: usize,
    /// Candidates refused because their root would not reach the cells the
    /// tip's last slide stranded ([`TipCover`]'s re-search prefilter).
    refused_cover: usize,
}

/// One proposal's verdict after the lowering re-derived its disc.
#[derive(Debug)]
enum Verdict {
    /// Left out without a count (no disc, or no frame for it).
    Skip,
    /// Refused: the shaft is still cut at the moved seat.
    Section,
    /// Refused: the moved ball no longer holds the seat.
    OffSeat,
    /// Refused: the slide leaves ground over air.
    OffGround,
    /// Standing: the seat's shift and the samples of its real disc.
    Verify {
        shift: [f64; 3],
        samples: Vec<Sample>,
        /// The UNGROWN ball the file gets at the moved seat ([`Slid::ungrown`]):
        /// what the tip-cover walk roots the moved tip on ([`TipCover`]).
        ball: ([f64; 3], f64),
        /// Whether that ball still holds the seat the tip was placed for
        /// ([`holds_seat`]); a slide that does is never walked.
        holds_seat: bool,
    },
}

/// A tip's disc plane and the lattice laid in it.
struct Frame {
    centre: [f64; 3],
    e1: [f64; 3],
    e2: [f64; 3],
    /// `tan` of the face's slope from horizontal, for the vertical window.
    tan_slope: f64,
    ball_radius: f64,
    /// The UNGROWN ball's radius: the search radius is about the tip's own
    /// feature, not about how far its ball grew for contact.
    planned_ball_radius: f64,
    /// The seat's outward unit normal, and the drawn ball centre's height
    /// over the disc plane along it (`d`, negative under the plane).
    normal: [f64; 3],
    centre_height: f64,
}

impl Frame {
    fn of(disc: &TipContactDisc) -> Option<Self> {
        let normal = disc.outward_normal;
        let reference = if normal[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let along = geom_kernel::vec3::dot(reference, normal);
        let e1 = geom_kernel::vec3::normalize(geom_kernel::vec3::sub(
            reference,
            geom_kernel::vec3::scale(normal, along),
        ))?;
        let e2 = geom_kernel::vec3::cross(normal, e1);
        let horizontal = normal[0].hypot(normal[1]);
        let tan_slope = if normal[2].abs() > 0.0 {
            horizontal / normal[2].abs()
        } else {
            f64::INFINITY
        };
        Some(Self {
            centre: disc.centre_mm,
            e1,
            e2,
            tan_slope,
            ball_radius: disc.ball_radius_mm,
            planned_ball_radius: disc.planned_ball_radius_mm,
            normal,
            centre_height: geom_kernel::vec3::dot(
                geom_kernel::vec3::sub(disc.ball_centre_mm, disc.centre_mm),
                normal,
            ),
        })
    }

    /// The drawn ball's chord along the normal through `p` (a point of the
    /// disc plane). `exact`: the chord at `p`'s own distance from the disc
    /// centre; otherwise the ball's widest chord (the search prefilter, which
    /// grades a column for any disc position near it).
    fn chord(&self, p: [f64; 3], exact: bool) -> Chord {
        let rho = if exact {
            geom_kernel::vec3::dist(p, self.centre)
        } else {
            0.0
        };
        let half = (self.ball_radius * self.ball_radius - rho * rho).max(0.0).sqrt();
        Chord {
            origin: geom_kernel::vec3::add(
                p,
                geom_kernel::vec3::scale(self.normal, self.centre_height + half),
            ),
            inward: self.normal.map(|component| -component),
            length: 2.0 * half,
        }
    }

    fn point(&self, pitch: f64, u: f64, v: f64) -> [f64; 3] {
        std::array::from_fn(|axis| {
            self.centre[axis] + pitch * (u * self.e1[axis] + v * self.e2[axis])
        })
    }
}

/// A tip's disc frame, the lattice offsets its disc covers, and its rim
/// points (in pitches, about the disc centre).
type DiscLattice = (Frame, Vec<(i32, i32)>, Vec<(f64, f64)>);

/// Points ON a disc of `radius` pitches, at most `1 / RIM_SAMPLES_PER_PITCH`
/// of a pitch apart along the circle, in pitches about its centre; never
/// fewer than eight.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "a rim of a few pitches gives a small positive sample count"
)]
fn rim_points(radius_in_pitches: f64) -> Vec<(f64, f64)> {
    if !(radius_in_pitches.is_finite() && radius_in_pitches > 0.0) {
        return Vec::new();
    }
    let count = (std::f64::consts::TAU * radius_in_pitches * RIM_SAMPLES_PER_PITCH)
        .ceil()
        .max(8.0) as usize;
    (0..count)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / count as f64;
            (
                radius_in_pitches * angle.cos(),
                radius_in_pitches * angle.sin(),
            )
        })
        .collect()
}

/// The column cell and lamina window a point on the seat plane is graded in.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "row, column and lamina indices are range-checked as f64 before every cast"
)]
fn sample_at(
    grid: &Grid,
    plate_z: f64,
    height: f64,
    layers: usize,
    frame: &Frame,
    p: [f64; 3],
    exact: bool,
) -> Sample {
    let chord = frame.chord(p, exact);
    let col = grid.col_coord(p[0]).floor();
    let row = grid.row_coord(p[1]).floor();
    let cell =
        (col >= 0.0 && row >= 0.0 && (col as usize) < grid.cols() && (row as usize) < grid.rows())
            .then(|| grid.index(row as usize, col as usize));
    let pitch = grid.pitch_mm();
    // A vertical face (tan = inf) reads the whole column above; `lamina`
    // clamps the window to the stack.
    let rise = 2.0 * height + 2.0 * pitch * frame.tan_slope;
    let top = layers.saturating_sub(1);
    let lamina = |z: f64| -> usize {
        let k = ((z - plate_z) / height).floor();
        if k.is_nan() || k <= 0.0 {
            0
        } else if k >= top as f64 {
            top
        } else {
            k as usize
        }
    };
    Sample {
        cell,
        k_lo: lamina(p[2]),
        k_hi: lamina(p[2] + rise.max(height)),
        chord,
    }
}

/// **Grade samples on the REAL model surface** (Craig 2026-10-12): a sample is
/// on the face when the model surface crosses the drawn ball's chord through
/// it ([`touching`]), within one pixel. Where the model's surface index is
/// absent (its memory cap) or the batched cast is refused, the raster grade
/// ([`carried`]) stands in.
fn graded(
    grid: &Grid,
    slices: &[Vec<SliceSolid>],
    surface: Option<&NativeSurface>,
    samples: &[Sample],
    layers_rasterised: &mut usize,
) -> Vec<bool> {
    surface
        .and_then(|surface| touching(surface, samples, grid.pitch_mm()))
        .unwrap_or_else(|| carried(grid, slices, samples, layers_rasterised))
}

/// **The gap to the real surface, per sample, in ONE batched ray cast.** The
/// emitter keeps `ball ∖ model` (no plane), so the file has contact over a
/// sample exactly when the model surface lies inside the drawn ball along the
/// seat normal there: a crossing of the sample's chord, from its support-side
/// end, within its length plus one pixel (`tolerance_mm`: the printer cannot
/// part two solids closer than that). A ray whose crossing the arithmetic
/// could not decide, with no decided crossing, is graded off the face (the
/// side that warns). `None` when the cast is refused.
///
/// Until 2026-10-12 a sample was on the face when the model was carried in its
/// raster column within two laminae plus `2 · pitch · tan(slope)` above it,
/// so a disc 0.1 mm under a curved surface passed (14 of 29 Bat_Cat_Owl discs
/// graded on their face hung up to 0.17 mm under it).
fn touching(surface: &NativeSurface, samples: &[Sample], tolerance_mm: f64) -> Option<Vec<bool>> {
    if samples.is_empty() {
        return Some(Vec::new());
    }
    let origins: Vec<[f64; 3]> = samples.iter().map(|sample| sample.chord.origin).collect();
    let directions: Vec<[f64; 3]> = samples.iter().map(|sample| sample.chord.inward).collect();
    let reach = samples
        .iter()
        .map(|sample| sample.chord.length)
        .fold(0.0_f64, f64::max)
        + tolerance_mm;
    let cast = surface
        .cast_rays_classified(&origins, &directions, reach)
        .ok()?;
    let row = |index: usize| -> &[f64] {
        let lo = usize::try_from(cast.offsets.get(index).copied().unwrap_or(0)).unwrap_or(0);
        let hi = usize::try_from(cast.offsets.get(index + 1).copied().unwrap_or(0)).unwrap_or(0);
        cast.t_all.get(lo..hi).unwrap_or(&[])
    };
    Some(
        samples
            .iter()
            .enumerate()
            .map(|(index, sample)| {
                row(index)
                    .iter()
                    .any(|&t| t <= sample.chord.length + tolerance_mm)
            })
            .collect(),
    )
}

/// ONE ascending pass: for every sample, whether the carrier mask covers its
/// cell at any lamina of its window. Each needed layer is rasterised once.
fn carried(
    grid: &Grid,
    slices: &[Vec<SliceSolid>],
    samples: &[Sample],
    layers_rasterised: &mut usize,
) -> Vec<bool> {
    let mut hit = vec![false; samples.len()];
    let layers = slices.len();
    let mut opening: Vec<Vec<usize>> = vec![Vec::new(); layers];
    for (index, sample) in samples.iter().enumerate() {
        if sample.cell.is_some() && sample.k_lo < layers {
            opening[sample.k_lo].push(index);
        }
    }
    let mut scratch = RasterScratch::new(grid);
    let mut masks = LayerMasks::default();
    let mut open: Vec<usize> = Vec::new();
    let mut still: Vec<usize> = Vec::new();
    for (layer, solids) in slices.iter().enumerate() {
        open.extend_from_slice(&opening[layer]);
        if open.is_empty() {
            continue;
        }
        rasterize_into(grid, solids, &mut scratch, &mut masks);
        *layers_rasterised += 1;
        still.clear();
        for &index in &open {
            let sample = samples[index];
            let Some(cell) = sample.cell else { continue };
            if masks.carry.get(cell).copied().unwrap_or(false) {
                hit[index] = true;
            } else if sample.k_hi > layer {
                still.push(index);
            }
        }
        std::mem::swap(&mut open, &mut still);
    }
    hit
}

/// A slid tip's ball and the contact disc it cuts in its seat plane.
#[derive(Clone, Copy, Debug)]
struct Slid {
    ball_centre: [f64; 3],
    ball_radius: f64,
    /// The UNGROWN ball's radius (the owner's ball for the tip size).
    planned_ball_radius: f64,
    disc_centre: [f64; 3],
    disc_radius: f64,
    /// The seat plane's outward unit normal.
    normal: [f64; 3],
}

impl Slid {
    fn of(disc: &TipContactDisc) -> Self {
        Self {
            ball_centre: disc.ball_centre_mm,
            ball_radius: disc.ball_radius_mm,
            planned_ball_radius: disc.planned_ball_radius_mm,
            disc_centre: disc.centre_mm,
            disc_radius: disc.radius_mm,
            normal: disc.outward_normal,
        }
    }

    /// The UNGROWN ball: the lowering grows a ball by keeping its deepest
    /// point along the outward normal (`plinth_support`'s `seated_ball`), so
    /// the ungrown centre stands `(R - R0)` back along the normal. Every ball
    /// the finished field's contact growth draws CONTAINS this one, so what
    /// it holds the file holds.
    fn ungrown(&self) -> ([f64; 3], f64) {
        let planned = self.planned_ball_radius.min(self.ball_radius);
        if planned.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            return (self.ball_centre, self.ball_radius);
        }
        let back = self.ball_radius - planned;
        (
            std::array::from_fn(|axis| self.ball_centre[axis] - back * self.normal[axis]),
            planned,
        )
    }

    /// The contact disc's lowest point: its centre less its radius times the
    /// plane's tilt from level (the horizontal part of the unit normal).
    fn lowest_mm(&self) -> f64 {
        let tilt = self.normal[0].hypot(self.normal[1]).min(1.0);
        self.disc_centre[2] - self.disc_radius * tilt
    }
}

/// **A slide keeps the support it was placed to give** (joint review P2,
/// 2026-10-12). `seat` is the point the reach criterion placed the tip for
/// (the bottom of the first cured lamina there). The slide keeps it when
/// either
///
/// * the moved ball still HOLDS the seat, to within one pixel (`pitch`: the
///   printer cannot part two solids closer than that, the tolerance the grade
///   itself reads); the tip's own ball holds its seat by construction (the
///   seat law drives it INTO the model there); or
/// * the slide does not CLIMB: the moved contact disc's lowest point stands
///   no more than one layer (`height`) above the lowest point of the disc the
///   tip had before the move (`placed`). A slide across a level face, or
///   along a tilted face's strike, stays on the laminae the tip was placed
///   under (the rim tips of a flat disc slide inward this way; a section move
///   slides along its face).
///
/// What it refuses is a slide UP the layers onto a contact that no longer
/// meets them: a disc wholly on SOME face is not support for the material
/// the tip was placed under. Deinonychus's ball 310 slid 0.79 mm up its
/// slope (its centre 0.27 mm higher) onto a whole-disc contact, and the 829 +
/// 1356 pixels it had held at 7.575 and 7.625 mm were left over nothing
/// (`joint/out-fix/final`).
fn keeps_seat(seat: [f64; 3], placed: Slid, moved: Slid, pitch: f64, height: f64) -> bool {
    if holds_seat(seat, moved, pitch) {
        return true;
    }
    moved.lowest_mm() <= placed.lowest_mm() + height
}

/// [`keeps_seat`]'s first clause: the moved ball still HOLDS the seat the tip
/// was placed for, to within one pixel. Such a slide leaves the tip touching
/// the cell the planner rooted it at, so its root is where it was and the
/// tip-cover check ([`TipCover`]) never walks it; only a slide passed by the
/// no-climb clause alone moves a root.
fn holds_seat(seat: [f64; 3], moved: Slid, pitch: f64) -> bool {
    geom_kernel::vec3::dist(seat, moved.ball_centre) <= moved.ball_radius + pitch
}

/// The cured model around ONE searched tip, as the ground check reads it
/// ([`keeps_ground`]): the sweep's own two masks of each layer the tip's ball
/// spans (and the one under it), cut to a window of the grid. `cover` (any
/// overlap, rounded outward) says where model MAY be, so a cell it names is
/// one the slide must not leave over air; `carry` (full containment, eroded)
/// says where model surely is, so only it joins a cell to its neighbours or
/// stands under it. Both read in their safe direction: the check never claims
/// ground the raster is not sure of.
struct GroundWindow {
    row0: usize,
    col0: usize,
    rows: usize,
    cols: usize,
    layer0: usize,
    layers: usize,
    /// Bit 0 `cover`, bit 1 `carry`, per (layer, row, col) of the window.
    bits: Vec<u8>,
}

impl GroundWindow {
    const COVER: u8 = 1;
    const CARRY: u8 = 2;

    fn slot(&self, row: usize, col: usize, layer: usize) -> Option<usize> {
        let (r, c, k) = (
            row.checked_sub(self.row0)?,
            col.checked_sub(self.col0)?,
            layer.checked_sub(self.layer0)?,
        );
        (r < self.rows && c < self.cols && k < self.layers)
            .then_some((k * self.rows + r) * self.cols + c)
    }

    fn has(&self, row: usize, col: usize, layer: usize, bit: u8) -> bool {
        self.slot(row, col, layer)
            .is_some_and(|slot| self.bits[slot] & bit != 0)
    }
}

/// The windows the ground check reads, one per searched tip: ONE ascending
/// pass over the layers any of them spans, each layer rasterised once
/// ([`rasterize_into`], the sweep's own masks). `balls[i]` is the i-th tip's
/// UNGROWN placed ball and `reach[i]` the window's half-width in mm.
fn ground_windows(
    grid: &Grid,
    slices: &[Vec<SliceSolid>],
    plate_z: f64,
    height: f64,
    balls: &[([f64; 3], f64)],
    reach: &[f64],
) -> Vec<GroundWindow> {
    let pitch = grid.pitch_mm();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "raster coordinates and layer indices of a ball near the model are small and clamped at zero"
    )]
    let index_of = |value: f64| value.max(0.0).floor() as usize;
    let mut windows: Vec<GroundWindow> = balls
        .iter()
        .zip(reach)
        .map(|(&(centre, radius), &reach)| {
            let col0 = index_of(grid.col_coord(centre[0] - reach));
            let row0 = index_of(grid.row_coord(centre[1] - reach));
            let col1 = index_of(grid.col_coord(centre[0] + reach) + 1.0).min(grid.cols());
            let row1 = index_of(grid.row_coord(centre[1] + reach) + 1.0).min(grid.rows());
            let layer0 = index_of((centre[2] - radius - plate_z) / height).saturating_sub(2);
            let layer1 = (index_of((centre[2] + radius - plate_z) / height) + 2).min(slices.len());
            let (rows, cols) = (row1.saturating_sub(row0), col1.saturating_sub(col0));
            let layers = layer1.saturating_sub(layer0);
            GroundWindow {
                row0,
                col0,
                rows,
                cols,
                layer0,
                layers,
                bits: vec![0; rows * cols * layers],
            }
        })
        .collect();
    if !(pitch > 0.0 && height > 0.0) || windows.is_empty() {
        return windows;
    }
    let mut opening: Vec<Vec<usize>> = vec![Vec::new(); slices.len()];
    for (index, window) in windows.iter().enumerate() {
        for slot in opening.iter_mut().skip(window.layer0).take(window.layers) {
            slot.push(index);
        }
    }
    let mut scratch = RasterScratch::new(grid);
    let mut masks = LayerMasks::default();
    for (layer, solids) in slices.iter().enumerate() {
        if opening[layer].is_empty() {
            continue;
        }
        rasterize_into(grid, solids, &mut scratch, &mut masks);
        for &index in &opening[layer] {
            let window = &mut windows[index];
            for r in 0..window.rows {
                for c in 0..window.cols {
                    let cell = grid.index(window.row0 + r, window.col0 + c);
                    let bits = u8::from(masks.cover[cell]) * GroundWindow::COVER
                        + u8::from(masks.carry[cell]) * GroundWindow::CARRY;
                    let slot = ((layer - window.layer0) * window.rows + r) * window.cols + c;
                    window.bits[slot] = bits;
                }
            }
        }
    }
    windows
}

/// What the ground check reads for one tip ([`keeps_ground`]).
struct Ground<'a> {
    grid: &'a Grid,
    window: &'a GroundWindow,
    plate_z: f64,
    height: f64,
}

impl Ground<'_> {
    /// The height a slicer cures layer `layer` at: its mid-plane.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a layer index bounded by the plan's own layer count is exact in f64"
    )]
    fn layer_mid_z(&self, layer: usize) -> f64 {
        self.plate_z + (layer as f64 + 0.5) * self.height
    }

    fn centre_of(&self, cell: usize) -> [f64; 2] {
        [
            self.grid.center_x_mm(self.grid.col_of(cell)),
            self.grid.center_y_mm(self.grid.row_of(cell)),
        ]
    }

    /// Whether the ball `(centre, radius)` HOLDS the model cell `cell` at
    /// `layer`: its section at that layer's mid-plane, or at the mid-plane of
    /// the layer under it (where the cell is cured ONTO it), covers the whole
    /// cell, the cell's centre at least half a cell diagonal inside the
    /// circle. The raster knows the model only to within its cell, so a cell
    /// the ball only grazes is not one it is known to hold.
    fn holds(&self, (centre, radius): ([f64; 3], f64), cell: usize, layer: usize) -> bool {
        let [x, y] = self.centre_of(cell);
        let rho = (x - centre[0]).hypot(y - centre[1])
            + crate::member_clearance::raster_half_diagonal_mm(self.grid);
        [Some(layer), layer.checked_sub(1)]
            .into_iter()
            .flatten()
            .any(|k| {
                let dz = self.layer_mid_z(k) - centre[2];
                let section_sq = radius * radius - dz * dz;
                section_sq > 0.0 && rho * rho <= section_sq
            })
    }

    /// Model MAY stand in this cell at this layer (`cover`).
    fn may_be_model(&self, cell: usize, layer: usize) -> bool {
        self.window.has(
            self.grid.row_of(cell),
            self.grid.col_of(cell),
            layer,
            GroundWindow::COVER,
        )
    }

    /// Model surely stands in this cell at this layer (`carry`).
    fn is_model(&self, cell: usize, layer: usize) -> bool {
        self.window.has(
            self.grid.row_of(cell),
            self.grid.col_of(cell),
            layer,
            GroundWindow::CARRY,
        )
    }

    /// Whether the window holds this cell at all.
    fn in_store(&self, cell: usize) -> bool {
        let (row, col) = (self.grid.row_of(cell), self.grid.col_of(cell));
        row >= self.window.row0
            && row < self.window.row0 + self.window.rows
            && col >= self.window.col0
            && col < self.window.col0 + self.window.cols
    }
}

/// **A slide leaves nothing it held over air** (fix lane A, 2026-10-12).
///
/// [`keeps_seat`] asks about ONE point, the seat; but a ball holds every
/// cured cell its section covers at the layer the cell cures on, or the one
/// under it, and a slide that keeps the seat point can still take the ball
/// from under the material beside it. Measured on Deinonychus (`joint/
/// out-fix/final`, slicer view with a one-pixel lateral allowance): arm 101
/// slid 0.51 mm DOWN its slope (passing the no-climb clause) and the 121
/// pixels of its own seat island at 7.925 mm floated; arms 92, 60 and 133
/// slid 0.10-0.22 mm with the seat still inside the moved ball, and 131, 97
/// and 44 pixels beside their seats, which the unmoved balls had held,
/// floated.
///
/// So every cell where model may stand that the UNGROWN placed ball holds (in
/// the ball's whole height, layer by layer) and the ungrown moved ball does
/// not, must still be held by something else: it must join, through cells
/// of ITS OWN layer where model surely stands, a cell with model surely
/// under it (carried from below), a cell the moved ball holds, or the edge of
/// the window the check reads (material that runs further than both balls
/// and the slide is someone else's to carry, and is left to the rules that
/// placed it). The ungrown balls are compared because the finished field's
/// contact growth only ever grows a ball to one that contains its ungrown
/// ball. A first cut read the up-sweep's `cover` record for all three
/// questions; its outward rounding joined arm 101's seat island to the slope
/// beside it through a cell the slicer leaves open, and passed the slide.
///
/// Read on the sweep's own two masks ([`GroundWindow`]); no new number: the
/// window is the two balls and the slide, plus the raster's own two-cell edge.
fn keeps_ground(ground: &Ground<'_>, placed: Slid, moved: Slid) -> bool {
    let before = placed.ungrown();
    let after = moved.ungrown();
    let grid = ground.grid;
    let pitch = grid.pitch_mm();
    if !(pitch > 0.0 && ground.height > 0.0) || grid.cells() == 0 {
        return true;
    }
    let travel = (after.0[0] - before.0[0]).hypot(after.0[1] - before.0[1]);
    let reach = before.1 + after.1 + travel + 2.0 * pitch;
    let [cx, cy] = [before.0[0], before.0[1]];
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "raster coordinates of a window a few ball radii across are small, clamped to the grid"
    )]
    let span = |coord: f64, count: usize| -> (usize, usize) {
        let lo = (coord - reach / pitch).floor().max(0.0) as usize;
        let hi = ((coord + reach / pitch).ceil().max(0.0) as usize).min(count.saturating_sub(1));
        (lo, hi)
    };
    let (c0, c1) = span(grid.col_coord(cx), grid.cols());
    let (r0, r1) = span(grid.row_coord(cy), grid.rows());
    if c1 < c0 || r1 < r0 {
        return true;
    }
    let width = c1 - c0 + 1;
    let local = |row: usize, col: usize| (row - r0) * width + (col - c0);
    let in_window = |cell: usize| {
        let [x, y] = ground.centre_of(cell);
        let (row, col) = (grid.row_of(cell), grid.col_of(cell));
        (r0..=r1).contains(&row)
            && (c0..=c1).contains(&col)
            && (x - cx).hypot(y - cy) <= reach
            && ground.in_store(cell)
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "layer indices of a ball's height over the plate are small and clamped at zero"
    )]
    let layer_of = |z: f64| ((z - ground.plate_z) / ground.height).max(0.0) as usize;
    let lowest = layer_of(before.0[2] - before.1).saturating_sub(1);
    let highest = layer_of(before.0[2] + before.1) + 1;
    // Which walk reached each cell of the layer (0: none). Every walk that
    // ended before this one found ground (an ungrounded one returns at once),
    // so a cell an EARLIER walk reached is ground for this one.
    let mut seen = vec![0_u32; width * (r1 - r0 + 1)];
    let mut walk = 0_u32;
    let mut queue: Vec<usize> = Vec::new();
    // Only a cell whose centre lies within the placed ball's radius of its
    // centre can be one it holds (`Ground::holds`: the section is never wider
    // than the ball), so the scan for lost cells keeps to that square, in the
    // same row-major order; the walks still range over the whole window.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "raster coordinates of a ball near the model are small and clamped to the window"
    )]
    let held = |coord: f64, lo: usize, hi: usize| -> (usize, usize) {
        let half = before.1 / pitch;
        let from = ((coord - half).floor().max(0.0) as usize).max(lo);
        let to = ((coord + half).ceil().max(0.0) as usize).min(hi);
        (from, to)
    };
    let (scan_c0, scan_c1) = held(grid.col_coord(cx), c0, c1);
    let (scan_r0, scan_r1) = held(grid.row_coord(cy), r0, r1);
    for layer in lowest..=highest {
        seen.iter_mut().for_each(|stamp| *stamp = 0);
        for row in scan_r0..=scan_r1 {
            for col in scan_c0..=scan_c1 {
                let cell = grid.index(row, col);
                if seen[local(row, col)] != 0
                    || !ground.may_be_model(cell, layer)
                    || !ground.holds(before, cell, layer)
                    || ground.holds(after, cell, layer)
                {
                    continue;
                }
                // A cell the placed ball held and the moved one does not:
                // the component of its layer it lies in must be held some
                // other way. The whole component is walked, so a later lost
                // cell in it is already decided.
                let mut grounded = false;
                walk += 1;
                queue.clear();
                queue.push(cell);
                seen[local(row, col)] = walk;
                // A cell where model only MAY stand is the uncertain edge of
                // whatever surely stands within the raster's own rounding band
                // (`cover` rounds out one cell, `carry` in one and erodes one
                // more in Y): it belongs to that material, so the walk starts
                // from it too. Without this the rim ring of a flat disc's
                // first lamina joined nothing, and every rim slide of the
                // flipped Round 25 was refused.
                let band: isize = if ground.is_model(cell, layer) { 0 } else { 2 };
                for dr in -band..=band {
                    for dc in -band..=band {
                        let (Some(nr), Some(nc)) =
                            (row.checked_add_signed(dr), col.checked_add_signed(dc))
                        else {
                            continue;
                        };
                        if !((r0..=r1).contains(&nr) && (c0..=c1).contains(&nc)) {
                            continue;
                        }
                        let near = grid.index(nr, nc);
                        if !(ground.is_model(near, layer) && in_window(near)) {
                            continue;
                        }
                        match seen[local(nr, nc)] {
                            0 => {
                                seen[local(nr, nc)] = walk;
                                queue.push(near);
                            }
                            stamp if stamp != walk => grounded = true,
                            _ => {}
                        }
                    }
                }
                // The walk stops at its first ground. Every seed is stamped by
                // then, so each piece of material the walk started from holds
                // a cell stamped by it, and a later walk that meets any of
                // that material reaches one of its stamps and is grounded by
                // it, as it would have been by the whole walk (a walk that
                // finds no ground returns at once, so every earlier stamp is
                // a grounded walk's). The answer is the whole walk's.
                while !grounded {
                    let Some(at) = queue.pop() else {
                        break;
                    };
                    if (layer > 0 && ground.is_model(at, layer - 1)) || ground.holds(after, at, layer) {
                        grounded = true;
                    }
                    let (ar, ac) = (grid.row_of(at), grid.col_of(at));
                    for dr in -1_isize..=1 {
                        for dc in -1_isize..=1 {
                            if dr == 0 && dc == 0 {
                                continue;
                            }
                            let (Some(nr), Some(nc)) =
                                (ar.checked_add_signed(dr), ac.checked_add_signed(dc))
                            else {
                                continue;
                            };
                            if nr >= grid.rows() || nc >= grid.cols() {
                                continue;
                            }
                            let next = grid.index(nr, nc);
                            if !ground.is_model(next, layer) {
                                continue;
                            }
                            if !in_window(next) {
                                // It runs past everything the slide touched.
                                grounded = true;
                                continue;
                            }
                            let slot = local(nr, nc);
                            match seen[slot] {
                                0 => {
                                    seen[slot] = walk;
                                    queue.push(next);
                                }
                                stamp if stamp != walk => grounded = true,
                                _ => {}
                            }
                        }
                    }
                }
                if !grounded {
                    return false;
                }
            }
        }
    }
    true
}

/// One contact ball as the ground check reads it: the drawn ball, its planned
/// (ungrown) radius, and the seat plane's outward unit normal
/// ([`contacts_keep_ground`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ContactBall {
    pub centre_mm: [f64; 3],
    pub radius_mm: f64,
    pub planned_radius_mm: f64,
    pub normal: [f64; 3],
}

impl ContactBall {
    /// The ball of a tip's contact disc.
    #[must_use]
    pub(crate) fn of(disc: &TipContactDisc) -> Self {
        Self {
            centre_mm: disc.ball_centre_mm,
            radius_mm: disc.ball_radius_mm,
            planned_radius_mm: disc.planned_ball_radius_mm,
            normal: disc.outward_normal,
        }
    }

    fn slid(self) -> Slid {
        Slid {
            ball_centre: self.centre_mm,
            ball_radius: self.radius_mm,
            planned_ball_radius: self.planned_radius_mm,
            disc_centre: self.centre_mm,
            disc_radius: 0.0,
            normal: self.normal,
        }
    }
}

/// **Whether each old tip keeps the ground its placed contact held**, for a
/// whole field of tips whose contact ball changed (`SC-DESIGN.md` §4, "ground
/// lost"): `pairs[i] = (before, after)` is the ball a tip had in the finished
/// first plan and the ball it has in the edited field. The answer is
/// [`keeps_ground`]'s, on the sweep's own two masks, for each pair: the cells
/// the UNGROWN `before` ball held and the ungrown `after` ball does not must
/// still join ground (an island a shrunk contact leaves over air is lost).
///
/// Batch-first: one window per CHANGED pair, every layer any window spans
/// rasterised ONCE ([`ground_windows`]); a pair whose two balls are equal
/// (nothing moved or shrank) is kept without a window.
#[must_use]
pub(crate) fn contacts_keep_ground(
    grid: &Grid,
    slices: &[Vec<SliceSolid>],
    plate_z: f64,
    height: f64,
    pairs: &[(ContactBall, ContactBall)],
) -> Vec<bool> {
    let pitch = grid.pitch_mm();
    let mut kept = vec![true; pairs.len()];
    let changed: Vec<usize> = pairs
        .iter()
        .enumerate()
        .filter(|(_, (before, after))| before != after)
        .map(|(index, _)| index)
        .collect();
    if changed.is_empty() {
        return kept;
    }
    let balls: Vec<([f64; 3], f64)> = changed.iter().map(|&i| pairs[i].0.slid().ungrown()).collect();
    // The window `keeps_ground` reads is its own reach (both ungrown radii
    // and the travel, plus two pitches) and the raster's two-cell edge.
    let reach: Vec<f64> = changed
        .iter()
        .map(|&i| {
            let (before, after) = (pairs[i].0.slid().ungrown(), pairs[i].1.slid().ungrown());
            let travel = (after.0[0] - before.0[0]).hypot(after.0[1] - before.0[1]);
            before.1 + after.1 + travel + 4.0 * pitch
        })
        .collect();
    let windows = ground_windows(grid, slices, plate_z, height, &balls, &reach);
    for (&index, window) in changed.iter().zip(&windows) {
        let ground = Ground {
            grid,
            window,
            plate_z,
            height,
        };
        kept[index] = keeps_ground(&ground, pairs[index].0.slid(), pairs[index].1.slid());
    }
    kept
}

/// The shortfalls a tip-cover check left, one per blamed slide: its lowest
/// stranded layer, its surface area, the surface's centre and its worst
/// finite potential on its lowest layer. In member order.
fn shortfalls(grid: &Grid, stranded: &[crate::demand_replay::StrandedCell]) -> Vec<CoverShortfall> {
    let mut arms: Vec<usize> = stranded.iter().map(|cell| cell.blamed).collect();
    arms.sort_unstable();
    arms.dedup();
    arms.into_iter()
        .filter_map(|arm| {
            let cells: Vec<&crate::demand_replay::StrandedCell> =
                stranded.iter().filter(|cell| cell.blamed == arm).collect();
            let layer = cells.iter().map(|cell| cell.layer).min()?;
            let base: Vec<&&crate::demand_replay::StrandedCell> =
                cells.iter().filter(|cell| cell.surface).collect();
            #[expect(clippy::cast_precision_loss, reason = "cell counts are small")]
            let count = base.len() as f64;
            let centre = base.iter().fold([0.0, 0.0], |sum, cell| {
                [
                    sum[0] + grid.center_x_mm(grid.col_of(cell.cell)),
                    sum[1] + grid.center_y_mm(grid.row_of(cell.cell)),
                ]
            });
            Some(CoverShortfall {
                arm,
                layer,
                surface_mm2: count * grid.cell_area_mm2(),
                centre_mm: [centre[0] / count.max(1.0), centre[1] / count.max(1.0)],
                max_u: cells
                    .iter()
                    .filter(|cell| cell.layer == layer)
                    .map(|cell| f64::from(cell.u))
                    .filter(|u| u.is_finite())
                    .fold(0.0, f64::max),
                slide_mm: 0.0,
            })
        })
        .collect()
}

/// The integer lattice offsets inside a disc of `radius` pitches, centre first.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a lattice half-width bounded by a ball radius over the grid pitch is small"
)]
fn disc_offsets(radius_in_pitches: f64) -> Vec<(i32, i32)> {
    let reach = radius_in_pitches.max(0.0).floor() as i32;
    let limit = radius_in_pitches * radius_in_pitches;
    let mut offsets = Vec::new();
    for u in -reach..=reach {
        for v in -reach..=reach {
            if f64::from(u * u + v * v) <= limit {
                offsets.push((u, v));
            }
        }
    }
    offsets
}

/// **Grade again the discs of tips whose seat normal changed after they were
/// graded** (`which`, strut indices): the footprint normal is measured on the
/// emitted ball ([`crate::footprint_normal::remeasure_emitted`]), and a ball
/// seated along another normal cuts another disc, so the grade the move search
/// gave the earlier disc no longer describes the file. `arms` are the emitted
/// arms carrying the final normals. Graded exactly as stage 1 grades (the
/// lattice AND the rim, in the disc's own plane, one ascending raster pass).
///
/// Returns `(strut index, fraction of the disc's samples on the face)` for
/// every tip of `which` whose disc is not wholly on its face. The grade is
/// published and warned on and moves nothing: a re-grade never un-moves a
/// seat.
#[expect(
    clippy::too_many_arguments,
    reason = "the field, its rasters, its arms and the tips to grade again"
)]
pub(crate) fn regrade(
    grid: &Grid,
    slices: &[Vec<SliceSolid>],
    surface: Option<&NativeSurface>,
    struts: &[SweepStrut],
    arms: &[SweepArm],
    which: &[usize],
    layer_plan: &LayerPlan,
    plate_z: f64,
    height: f64,
) -> Vec<(usize, f64)> {
    if which.is_empty() {
        return Vec::new();
    }
    let subset: Vec<SweepArm> = which
        .iter()
        .filter_map(|&index| arms.get(index).cloned())
        .collect();
    let Ok(discs) = plinth_support::tip_contact_discs(&subset, layer_plan) else {
        return Vec::new();
    };
    let pitch = grid.pitch_mm();
    let layers = slices.len();
    let mut samples: Vec<Sample> = Vec::new();
    let mut owner: Vec<usize> = Vec::new();
    for (position, (&index, disc)) in which.iter().zip(&discs).enumerate() {
        let Some(strut) = struts.get(index) else { continue };
        let Some((frame, offsets, rim)) = disc_lattice(disc.as_ref(), strut, pitch) else {
            continue;
        };
        let lattice = offsets.iter().map(|&(u, v)| (f64::from(u), f64::from(v)));
        for (u, v) in lattice.chain(rim) {
            let p = frame.point(pitch, u, v);
            samples.push(sample_at(grid, plate_z, height, layers, &frame, p, true));
            owner.push(position);
        }
    }
    let mut layers_rasterised = 0;
    let hit = graded(grid, slices, surface, &samples, &mut layers_rasterised);
    let mut on = vec![0_usize; which.len()];
    let mut total = vec![0_usize; which.len()];
    for (&position, &is_hit) in owner.iter().zip(&hit) {
        total[position] += 1;
        on[position] += usize::from(is_hit);
    }
    (0..which.len())
        .filter(|&position| total[position] > 0 && on[position] < total[position])
        .map(|position| {
            #[expect(clippy::cast_precision_loss, reason = "sample counts are small")]
            let fraction = on[position] as f64 / total[position] as f64;
            (which[position], fraction)
        })
        .collect()
}

/// A graded tip's disc frame, lattice offsets and rim points
/// ([`DiscLattice`]); `None` for a tie (not graded, see [`apply`]), a missing
/// disc, or a disc with no radius.
fn disc_lattice(
    disc: Option<&TipContactDisc>,
    strut: &SweepStrut,
    pitch: f64,
) -> Option<DiscLattice> {
    if strut.kind == crate::StrutKind::Tie {
        return None;
    }
    let disc = disc.filter(|disc| disc.radius_mm > 0.0)?;
    let frame = Frame::of(disc)?;
    Some((
        frame,
        disc_offsets(disc.radius_mm / pitch),
        rim_points(disc.radius_mm / pitch),
    ))
}

/// Grade every seated tip's disc, move the seats that hang off their face,
/// and warn about the knife edges. `arms` is the field as the adapter will
/// hand it to the lowering, in strut order.
///
/// **A slide is validated on the geometry it will have.** Each proposed slide
/// is re-measured at the MOVED ball: `remeasure(arm indices, moved arms,
/// shifts)` returns the seat normal each moved arm gets there (the footprint
/// normal at the slid ball, [`crate::footprint_normal::remeasure_slid`]), and
/// the disc that is graded against the face, the section test and the
/// accepted move all use that normal.
#[expect(
    clippy::too_many_lines,
    reason = "one pass in four batched stages (grade, search lattice, search, verify); splitting \
              it would thread the same six buffers through four signatures"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "the field, its rasters, its arms, its frame and the rules a move is held to"
)]
pub(crate) fn apply(
    grid: &Grid,
    slices: &[Vec<SliceSolid>],
    surface: Option<&NativeSurface>,
    struts: &mut [SweepStrut],
    arms: &[SweepArm],
    layer_plan: &LayerPlan,
    plate_z: f64,
    height: f64,
    limits: &MoveLimits<'_>,
    ground_check: bool,
    mut cover: Option<TipCover<'_>>,
    mut remeasure: impl FnMut(&[usize], &[SweepArm], &[[f64; 3]]) -> Vec<[f64; 3]>,
) -> FootprintOutcome {
    let mut outcome = FootprintOutcome::default();
    let Ok(discs) = plinth_support::tip_contact_discs(arms, layer_plan) else {
        return outcome;
    };
    let layers = slices.len();
    let pitch = grid.pitch_mm();

    // ---- stage 1: every graded tip's own disc, one pass.
    //
    // A MODEL TIE (D8, 2026-10-03) is not graded here: its seat may be a
    // wall, and this rule's "on the face" is read in the carried-face
    // rasters, which hold down-facing faces only, so a disc on a wall reads
    // as hanging off its face and would be slid along the wall off the
    // model. A tie's disc is held wholly on the model where it is placed
    // (`model_tie::disc_on_model`), and its seat is not moved.
    let frames: Vec<Option<DiscLattice>> = discs
        .iter()
        .zip(struts.iter())
        .map(|(disc, strut)| disc_lattice(disc.as_ref(), strut, pitch))
        .collect();
    let mut samples = Vec::new();
    // (arm, whether the sample is on the rim)
    let mut owner: Vec<(usize, bool)> = Vec::new();
    for (arm, entry) in frames.iter().enumerate() {
        let Some((frame, offsets, rim)) = entry else {
            continue;
        };
        let lattice = offsets
            .iter()
            .map(|&(u, v)| (f64::from(u), f64::from(v), false));
        let rim = rim.iter().map(|&(u, v)| (u, v, true));
        for (u, v, on_rim) in lattice.chain(rim) {
            let p = frame.point(pitch, u, v);
            samples.push(sample_at(grid, plate_z, height, layers, frame, p, true));
            owner.push((arm, on_rim));
        }
    }
    outcome.disc_samples = samples.len();
    let hit = graded(grid, slices, surface, &samples, &mut outcome.layers_rasterised);
    // Cancelled poses (`crate::cancel`) leave between the stages with the
    // outcome as it stands, before anything is moved; the caller ends the pose
    // at its next safe point and reads none of it.
    if crate::cancel::cancelled() {
        return outcome;
    }
    let mut on = vec![0_usize; arms.len()];
    let mut total = vec![0_usize; arms.len()];
    let mut lattice_missed = vec![false; arms.len()];
    for (index, &(arm, on_rim)) in owner.iter().enumerate() {
        total[arm] += 1;
        on[arm] += usize::from(hit[index]);
        lattice_missed[arm] |= !on_rim && !hit[index];
    }
    outcome.tips_off_face_at_rim_only = (0..arms.len())
        .filter(|&arm| total[arm] > 0 && on[arm] < total[arm] && !lattice_missed[arm])
        .count();
    // THE LIP-SEAT RULE's remedy 2 (2026-09-29): a tip whose disc is on its
    // face but whose shaft the seat trim cuts outside its ball joins the same
    // search, for a seat further along the same face where the moved arm keeps
    // its section. Measured by the lowering's own derivation. A tip whose disc
    // is OFF its face is searched for contact area alone, as before this rule.
    let bites_before = plinth_support::tip_shaft_seat_bites(arms, layer_plan)
        .unwrap_or_else(|_| vec![None; arms.len()]);
    let mut section_only = vec![false; arms.len()];
    let mut failing: Vec<usize> = Vec::new();
    for arm in 0..arms.len() {
        if total[arm] == 0 {
            continue;
        }
        outcome.tips_graded += 1;
        // A HEADED arm is graded but never slid: its head is frozen at
        // placement. A disc off its face keeps its seat, as a refused move.
        if struts[arm].head.is_some() {
            if on[arm] == total[arm] {
                outcome.tips_on_face += 1;
                struts[arm].seat_face.footprint = Footprint::OnFace;
            } else {
                #[expect(clippy::cast_precision_loss, reason = "sample counts are small")]
                let fraction = on[arm] as f64 / total[arm].max(1) as f64;
                outcome.headed_kept += 1;
                outcome.moves_refused.push((arm, fraction));
                struts[arm].seat_face.footprint = Footprint::MoveRefused {
                    disc_on_face: fraction,
                };
            }
            continue;
        }
        if on[arm] == total[arm] {
            outcome.tips_on_face += 1;
            struts[arm].seat_face.footprint = Footprint::OnFace;
            if crate::lip_seat::cuts(bites_before[arm].as_ref()) {
                section_only[arm] = true;
                outcome.tips_graded_for_section += 1;
                failing.push(arm);
            }
        } else {
            failing.push(arm);
        }
    }
    if failing.is_empty() {
        return outcome;
    }

    // ---- stage 2: the search lattice around every failing tip, one pass.
    // Each failing tip gets a square lattice of half-width W pitches in its
    // own disc plane, W covering its disc moved by up to the search radius.
    struct Search {
        arm: usize,
        half_width: i32,
        first: usize,
        candidates: Vec<(i32, i32)>,
        next: usize,
        /// The ball and disc before any move: the prefilter moves them
        /// rigidly with each candidate ([`keeps_seat`]).
        slid: Slid,
    }
    let mut searches: Vec<Search> = Vec::with_capacity(failing.len());
    let mut lattice: Vec<Sample> = Vec::new();
    // One tip's search: its lattice appended to `lattice`, its candidates
    // nearest first. Also what the tip cover's NEIGHBOUR repair searches with
    // ([`TipCover`]), so a helper is searched exactly as a failing tip is.
    let search_for = |arm: usize, lattice: &mut Vec<Sample>| -> Option<Search> {
        let (frame, offsets, _) = frames[arm].as_ref()?;
        // How far the lean put the disc centre from the seat, in the plane.
        let offset = geom_kernel::vec3::sub(frame.centre, arms[arm].seat_mm);
        let normal = geom_kernel::vec3::cross(frame.e1, frame.e2);
        let in_plane = geom_kernel::vec3::sub(
            offset,
            geom_kernel::vec3::scale(normal, geom_kernel::vec3::dot(offset, normal)),
        );
        let search_mm = SEARCH_RADIUS_BALL_DIAMETERS * 2.0 * frame.planned_ball_radius
            + geom_kernel::vec3::norm(in_plane)
            + 2.0 * pitch;
        let disc_reach = offsets
            .iter()
            .map(|&(u, v)| u.abs().max(v.abs()))
            .max()
            .unwrap_or(0);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a search radius of about one ball diameter over the grid pitch is small"
        )]
        let search_steps = (search_mm / pitch).floor() as i32;
        // One more pitch than the lattice offsets reach: the rim, read back at
        // its nearest lattice point, lies up to a pitch beyond them.
        let half_width = disc_reach + 1 + search_steps;
        let first = lattice.len();
        for u in -half_width..=half_width {
            for v in -half_width..=half_width {
                let p = frame.point(pitch, f64::from(u), f64::from(v));
                lattice.push(sample_at(grid, plate_z, height, layers, frame, p, false));
            }
        }
        // Candidate moves nearest first; ties broken by the offset itself so
        // the answer never depends on iteration order.
        let mut candidates: Vec<(i32, i32)> = disc_offsets(f64::from(search_steps))
            .into_iter()
            .filter(|&(u, v)| (u, v) != (0, 0))
            .collect();
        candidates.sort_by_key(|&(u, v)| (u * u + v * v, u, v));
        Some(Search {
            arm,
            half_width,
            first,
            candidates,
            next: 0,
            slid: Slid {
                ball_centre: geom_kernel::vec3::add(
                    frame.centre,
                    geom_kernel::vec3::scale(frame.normal, frame.centre_height),
                ),
                ball_radius: frame.ball_radius,
                planned_ball_radius: frame.planned_ball_radius,
                disc_centre: frame.centre,
                disc_radius: discs[arm].map_or(0.0, |disc| disc.radius_mm),
                normal: frame.normal,
            },
        })
    };
    for &arm in &failing {
        if let Some(search) = search_for(arm, &mut lattice) {
            searches.push(search);
        }
    }
    outcome.search_samples = lattice.len();
    let mut lattice_hit = graded(grid, slices, surface, &lattice, &mut outcome.layers_rasterised);
    if crate::cancel::cancelled() {
        return outcome;
    }
    // THE GROUND EACH SEARCHED TIP HOLDS (`keeps_ground`): one window per
    // tip, wide enough for its ball moved anywhere the search may take it,
    // every layer rasterised once.
    let windows_for = |searches: &[Search]| -> Vec<GroundWindow> {
        let balls: Vec<([f64; 3], f64)> =
            searches.iter().map(|search| search.slid.ungrown()).collect();
        let reach: Vec<f64> = searches
            .iter()
            .map(|search| {
                let (_, radius) = search.slid.ungrown();
                f64::from(search.half_width) * pitch + 2.0 * radius + 4.0 * pitch
            })
            .collect();
        ground_windows(grid, slices, plate_z, height, &balls, &reach)
    };
    let mut windows: Vec<GroundWindow> =
        if ground_check { windows_for(&searches) } else { Vec::new() };

    // ---- stage 3: nearest candidate whose rigidly moved disc is all on the
    // face; then stage 4: re-derive the moved tips' discs through the
    // lowering (the axis changes when the seat moves), and keep a move only
    // if the disc the file will really get is on the face too. One batched
    // disc call per round.
    let mut pending: Vec<usize> = (0..searches.len()).collect();
    let mut accepted: Vec<Option<[f64; 3]>> = vec![None; searches.len()];
    // The seat normal measured at each slot's latest proposed ball, and the
    // one of the move it was accepted with.
    let mut proposal_normal: Vec<Option<[f64; 3]>> = vec![None; searches.len()];
    let mut accepted_normal: Vec<Option<[f64; 3]>> = vec![None; searches.len()];
    // The same for the ungrown ball at the moved seat (the tip cover roots a
    // moved tip on it).
    let mut proposal_ball: Vec<Option<([f64; 3], f64)>> = vec![None; searches.len()];
    let mut accepted_ball: Vec<Option<([f64; 3], f64)>> = vec![None; searches.len()];
    // And whether that ball still holds the placed seat ([`holds_seat`]).
    let mut proposal_holds_seat = vec![false; searches.len()];
    let mut accepted_holds_seat = vec![false; searches.len()];
    // THE MOVE CHECK's state: every member's drawn ends before any move, the
    // own seat moves accepted so far (by member), and which search slot, if
    // any, each member's own move belongs to.
    let before_ends: Vec<([f64; 3], [f64; 3])> = struts
        .iter()
        .map(|strut| crate::adapter::drawn_post_ends(grid, plate_z, height, strut))
        .collect();
    let mut own_accepted: Vec<Option<[f64; 3]>> = vec![None; struts.len()];
    let mut slot_of_member = vec![usize::MAX; struts.len()];
    for (slot, search) in searches.iter().enumerate() {
        if let Some(entry) = slot_of_member.get_mut(search.arm) {
            *entry = slot;
        }
    }
    let mut refused_by_limits = vec![false; searches.len()];
    let mut search_arms: Vec<usize> = searches.iter().map(|search| search.arm).collect();
    // THE NEIGHBOUR REPAIR's state ([`TipCover`]): per slot, the kept slide a
    // helper slot was added to cover for; per kept slot, whether its helpers
    // were added and whether they failed; per arm, whether it is a helper.
    // A helper is searched as a section tip is (its disc is on its face, and
    // a slide must not cut its shaft), so `section_only` is set for it.
    let mut cover_helper_of: Vec<Option<usize>> = vec![None; searches.len()];
    let mut cover_helpers_added = vec![false; searches.len()];
    let mut cover_helpers_failed = vec![false; searches.len()];
    let mut cover_retired = vec![false; searches.len()];
    let mut is_helper = vec![false; arms.len()];
    let mut cover_spawns = 0_usize;
    // THE TIP COVER's state ([`TipCover`]): per slot, the cells (in the plane,
    // on the tip's seat layer) a slide of it once stranded, which every later
    // candidate's root must reach; whether a slide of it was ever blamed; and
    // the re-search passes run.
    let mut cover_targets: Vec<Vec<[f64; 2]>> = vec![Vec::new(); searches.len()];
    let mut cover_blamed = vec![false; searches.len()];
    let mut cover_passes = 0_u32;
    let cover_reach = cover.as_ref().map_or(f64::INFINITY, |cover| cover.reach_mm);
    // Per slot: the slide the rule first chose (shift, normal, ball, whether
    // it holds its seat), kept from the first time the slide is blamed; and
    // whether an ONTO-FACE slide is kept, stranding and warned, because no
    // other on-face seat covers what it stranded.
    type Chosen = ([f64; 3], Option<[f64; 3]>, Option<([f64; 3], f64)>, bool);
    let mut cover_original: Vec<Option<Chosen>> = vec![None; searches.len()];
    let mut cover_kept = vec![false; searches.len()];
    // Each turn of this loop either ends it, or takes back, replaces or keeps
    // a blamed slide; re-search passes stop after `passes` turns, and after
    // them every blamed slide is undone or kept, so the number of turns is
    // bounded. The bound below is a backstop only.
    let cover_passes_max = cover.as_ref().map_or(0, |cover| cover.passes as usize);
    let mut cover_turns = 0_usize;
    loop {
    // Read afresh each turn: the tip cover's neighbour repair appends
    // searches, lattice grades and ground windows between turns.
    let ground_of = |slot: usize| {
        windows.get(slot).map(|window| Ground {
            grid,
            window,
            plate_z,
            height,
        })
    };
    let on_lattice = |search: &Search, u: i32, v: i32| -> bool {
        let w = search.half_width;
        if u.abs() > w || v.abs() > w {
            return false;
        }
        #[expect(
            clippy::cast_sign_loss,
            reason = "both terms are shifted into 0..=2W before the cast"
        )]
        let index = search.first + ((u + w) as usize) * ((2 * w + 1) as usize) + (v + w) as usize;
        lattice_hit.get(index).copied().unwrap_or(false)
    };
    while !pending.is_empty() {
        if crate::cancel::cancelled() {
            return outcome;
        }
        outcome.rounds += 1;
        let mut proposals: Vec<(usize, (i32, i32))> = Vec::new();
        // EACH PENDING TIP'S CANDIDATE WALK is a pure function of that tip's
        // own search, the frozen lattice grade and its own ground window:
        // read-only here, so the walks run side by side (one task per tip, a
        // whole walk each) and their results are applied in `pending` order,
        // the order the serial loop applied them in. The counters are sums.
        let walk_of = |slot: usize| -> Option<CandidateWalk> {
            let search = &searches[slot];
            let (frame, offsets, rim) = frames[search.arm].as_ref()?;
            // The rim read back at its nearest search-lattice point: a
            // pruning prefilter only. The disc the move really gets is graded
            // exactly, lattice and rim, after the lowering re-derives it.
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a rim coordinate of a few pitches is small"
            )]
            let rim_cells: Vec<(i32, i32)> = rim
                .iter()
                .map(|&(u, v)| (u.round() as i32, v.round() as i32))
                .collect();
            let seat = arms[search.arm].seat_mm;
            let mut walk = CandidateWalk {
                slot,
                next: search.next,
                proposal: None,
                refused_off_seat: 0,
                refused_off_ground: 0,
                refused_cover: 0,
            };
            let targets = &cover_targets[slot];
            let root = struts[search.arm].center_mm;
            while walk.next < search.candidates.len() {
                let (i, j) = search.candidates[walk.next];
                walk.next += 1;
                if !offsets
                    .iter()
                    .chain(&rim_cells)
                    .all(|&(u, v)| on_lattice(search, u + i, v + j))
                {
                    continue;
                }
                // An on-face candidate whose ball and disc, moved rigidly with
                // it, no longer keep the seat it was placed for (`keeps_seat`)
                // is a refused move; the ball the lowering re-derives is held
                // to the same rule below.
                let slide = geom_kernel::vec3::sub(
                    frame.point(pitch, f64::from(i), f64::from(j)),
                    frame.centre,
                );
                let rigid = Slid {
                    ball_centre: geom_kernel::vec3::add(search.slid.ball_centre, slide),
                    disc_centre: geom_kernel::vec3::add(search.slid.disc_centre, slide),
                    ..search.slid
                };
                if !keeps_seat(seat, search.slid, rigid, pitch, height) {
                    walk.refused_off_seat += 1;
                    continue;
                }
                if ground_of(slot).is_some_and(|ground| !keeps_ground(&ground, search.slid, rigid)) {
                    walk.refused_off_seat += 1;
                    walk.refused_off_ground += 1;
                    continue;
                }
                // THE TIP COVER's prefilter: a tip whose earlier slide
                // stranded demand only takes a seat whose root reaches those
                // cells again (the next reach walk is the judge).
                if !targets.is_empty() {
                    let at = [root[0] + slide[0], root[1] + slide[1]];
                    if targets
                        .iter()
                        .any(|cell| (cell[0] - at[0]).hypot(cell[1] - at[1]) > cover_reach)
                    {
                        walk.refused_cover += 1;
                        continue;
                    }
                }
                walk.proposal = Some((i, j));
                break;
            }
            Some(walk)
        };
        let walks: Vec<Option<CandidateWalk>> = if pending.len() >= PARALLEL_MIN_TIPS {
            pending.par_iter().map(|&slot| walk_of(slot)).collect()
        } else {
            pending.iter().map(|&slot| walk_of(slot)).collect()
        };
        if crate::footprint_normal::verify_footprint() {
            // THE SHADOW CHECK (`PLINTH_SWEEP_VERIFY_FOOTPRINT=1`): the same
            // walks, one after another in tip order.
            let serial: Vec<Option<CandidateWalk>> =
                pending.iter().map(|&slot| walk_of(slot)).collect();
            assert_eq!(
                format!("{walks:?}"),
                format!("{serial:?}"),
                "the candidate walks run side by side differ from the walks run in tip order"
            );
        }
        for walk in walks.into_iter().flatten() {
            searches[walk.slot].next = walk.next;
            outcome.moves_refused_off_seat += walk.refused_off_seat;
            outcome.moves_refused_off_ground += walk.refused_off_ground;
            outcome.cover.candidates_refused += walk.refused_cover;
            refused_by_limits[walk.slot] |= walk.refused_off_seat > 0;
            if let Some(step) = walk.proposal {
                proposals.push((walk.slot, step));
            }
        }
        if proposals.is_empty() {
            break;
        }
        // The move is a lattice step IN the disc's plane, so the seat slides
        // along the face and keeps its height above it.
        let slides: Vec<[f64; 3]> = proposals
            .iter()
            .map(|&(slot, (i, j))| {
                frames[searches[slot].arm]
                    .as_ref()
                    .map_or([0.0; 3], |(frame, _, _)| {
                        geom_kernel::vec3::sub(
                            frame.point(pitch, f64::from(i), f64::from(j)),
                            frame.centre,
                        )
                    })
            })
            .collect();
        let mut moved_arms: Vec<SweepArm> = proposals
            .iter()
            .zip(&slides)
            .map(|(&(slot, _), &shift)| {
                let arm = &arms[searches[slot].arm];
                SweepArm {
                    seat_mm: geom_kernel::vec3::add(arm.seat_mm, shift),
                    seat_face_point_mm: arm
                        .seat_face_point_mm
                        .map(|point| geom_kernel::vec3::add(point, shift)),
                    radius_profile_mm: Vec::new(),
                    ..arm.clone()
                }
            })
            .collect();
        // THE NORMAL AT THE MOVED BALL: the footprint under the slid tip is
        // another patch of the surface, so its average normal is another
        // normal, and the ball, the disc graded below and the trim all follow
        // it. The move is accepted on the geometry the file will have.
        let slid_members: Vec<usize> = proposals
            .iter()
            .map(|&(slot, _)| searches[slot].arm)
            .collect();
        let slid_normals = remeasure(&slid_members, &moved_arms, &slides);
        for ((arm, &normal), &(slot, _)) in moved_arms.iter_mut().zip(&slid_normals).zip(&proposals) {
            arm.seat_normal_mm = normal;
            proposal_normal[slot] = Some(normal);
        }
        let rederived = plinth_support::tip_contact_discs(&moved_arms, layer_plan)
            .unwrap_or_else(|_| vec![None; moved_arms.len()]);
        // The section test, for the tips searched for their section ALONE
        // (their disc is already on its face): each such move drawn with its
        // own profile, all of them in one lowering call. A tip searched
        // because its disc is OFF its face is not tested: contact area is the
        // only seat criterion, and a slide that puts the disc back on its
        // face is taken whatever it does to the shaft (the lip-seat census
        // then warns about the shaft). Challenger, 2026-09-29: a guard here on
        // every slide left Round_100's arm 195 with 22 % of its disc on the
        // face.
        let section_slots: Vec<usize> = proposals
            .iter()
            .enumerate()
            .filter(|(_, (slot, _))| section_only[searches[*slot].arm])
            .map(|(at, _)| at)
            .collect();
        let profiled: Vec<SweepArm> = section_slots
            .iter()
            .map(|&at| SweepArm {
                radius_profile_mm: arms[searches[proposals[at].0].arm]
                    .radius_profile_mm
                    .clone(),
                ..moved_arms[at].clone()
            })
            .collect();
        let measured = plinth_support::tip_shaft_seat_bites(&profiled, layer_plan)
            .unwrap_or_else(|_| vec![None; profiled.len()]);
        let mut moved_bites: Vec<Option<plinth_support::ShaftSeatBite>> =
            vec![None; proposals.len()];
        for (&at, bite) in section_slots.iter().zip(measured) {
            moved_bites[at] = bite;
        }
        let mut resolved = vec![false; searches.len()];
        // The real disc of every proposal still standing, sampled on its own
        // lattice AND its rim in its own plane, all of them graded in ONE
        // ascending raster pass: exactly how a disc is graded in stage 1, so
        // a move is accepted only onto a seat whose whole disc is on the face.
        let mut verify: Vec<(usize, [f64; 3])> = Vec::with_capacity(proposals.len());
        let mut verify_samples: Vec<Sample> = Vec::new();
        let mut verify_span: Vec<(usize, usize)> = Vec::with_capacity(proposals.len());
        // Each proposal's verdict (its refusals, the ground check, its disc's
        // samples) is a pure function of that proposal and the frozen search
        // state, so the verdicts are read side by side, one task per
        // proposal, and applied in proposal order: the order the samples are
        // laid out in, and the order the serial loop counted in.
        let verdict_of = |at: usize| -> Verdict {
            let (slot, _) = proposals[at];
            let (moved, disc) = (&moved_arms[at], &rederived[at]);
            let search = &searches[slot];
            // A tip searched for its section alone moves only onto a seat
            // where the shaft keeps it. Its refusal is its own count, never a
            // "refused by the limits" (that names the lean law, an unbraced
            // run or the drain keep-out, and this tip keeps its seat anyway).
            if section_only[search.arm] && crate::lip_seat::cuts(moved_bites[at].as_ref()) {
                return Verdict::Section;
            }
            let Some(disc) = disc.filter(|disc| disc.radius_mm > 0.0) else {
                return Verdict::Skip;
            };
            // THE SEAT IT WAS PLACED FOR (joint review P2): the ball the file
            // will get at the moved seat must still hold the seat the reach
            // criterion asked support for. A refusal is a refused move: the
            // tip tries its next candidate, and keeps its seat, warned, when
            // none is left.
            if !keeps_seat(
                arms[search.arm].seat_mm,
                search.slid,
                Slid::of(&disc),
                pitch,
                height,
            ) {
                return Verdict::OffSeat;
            }
            if ground_of(slot)
                .is_some_and(|ground| !keeps_ground(&ground, search.slid, Slid::of(&disc)))
            {
                return Verdict::OffGround;
            }
            let Some(real) = Frame::of(&disc) else {
                return Verdict::Skip;
            };
            let lattice = disc_offsets(disc.radius_mm / pitch)
                .into_iter()
                .map(|(u, v)| (f64::from(u), f64::from(v)));
            let samples: Vec<Sample> = lattice
                .chain(rim_points(disc.radius_mm / pitch))
                .map(|(u, v)| {
                    let p = real.point(pitch, u, v);
                    sample_at(grid, plate_z, height, layers, &real, p, true)
                })
                .collect();
            Verdict::Verify {
                shift: geom_kernel::vec3::sub(moved.seat_mm, arms[search.arm].seat_mm),
                samples,
                ball: Slid::of(&disc).ungrown(),
                holds_seat: holds_seat(arms[search.arm].seat_mm, Slid::of(&disc), pitch),
            }
        };
        let verdicts: Vec<Verdict> = if proposals.len() >= PARALLEL_MIN_TIPS {
            (0..proposals.len()).into_par_iter().map(verdict_of).collect()
        } else {
            (0..proposals.len()).map(verdict_of).collect()
        };
        if crate::footprint_normal::verify_footprint() {
            let serial: Vec<Verdict> = (0..proposals.len()).map(verdict_of).collect();
            assert_eq!(
                format!("{verdicts:?}"),
                format!("{serial:?}"),
                "the verdicts read side by side differ from the verdicts read in proposal order"
            );
        }
        for (&(slot, _), verdict) in proposals.iter().zip(verdicts) {
            match verdict {
                Verdict::Skip => {}
                Verdict::Section => outcome.moves_refused_section += 1,
                Verdict::OffSeat => {
                    outcome.moves_refused_off_seat += 1;
                    refused_by_limits[slot] = true;
                }
                Verdict::OffGround => {
                    outcome.moves_refused_off_seat += 1;
                    outcome.moves_refused_off_ground += 1;
                    refused_by_limits[slot] = true;
                }
                Verdict::Verify { shift, samples, ball, holds_seat } => {
                    proposal_ball[slot] = Some(ball);
                    proposal_holds_seat[slot] = holds_seat;
                    let first = verify_samples.len();
                    verify_samples.extend(samples);
                    verify_span.push((first, verify_samples.len()));
                    verify.push((slot, shift));
                }
            }
        }
        outcome.verify_samples += verify_samples.len();
        let verify_hit = graded(
            grid,
            slices,
            surface,
            &verify_samples,
            &mut outcome.layers_rasterised,
        );
        let on_face: Vec<(usize, [f64; 3])> = verify
            .into_iter()
            .zip(verify_span)
            .filter(|(_, (first, end))| verify_hit[*first..*end].iter().all(|&hit| hit))
            .map(|(entry, _)| entry)
            .collect();
        // THE MOVE CHECK, once per round over the whole batch: the moves
        // accepted so far plus this round's, every member they displace
        // judged on its drawn ends. A broken rule refuses every move of THIS
        // round that the broken member inherits a displacement from (itself
        // or a host, transitively), and the rest are judged again without
        // them. Each pass removes at least one move, so it ends.
        let admitted = check_moves(
            MoveCheck {
                grid,
                plate_z,
                height,
                limits,
                struts,
                before_ends: &before_ends,
                own_accepted: &own_accepted,
                slot_of_member: &slot_of_member,
                search_arms: &search_arms,
            },
            on_face,
            &mut refused_by_limits,
            &mut outcome,
        );
        for (slot, shift) in admitted {
            accepted[slot] = Some(shift);
            accepted_normal[slot] = proposal_normal[slot];
            accepted_ball[slot] = proposal_ball[slot];
            accepted_holds_seat[slot] = proposal_holds_seat[slot];
            own_accepted[searches[slot].arm] = Some(shift);
            resolved[slot] = true;
        }
        pending.retain(|slot| {
            !resolved[*slot] && searches[*slot].next < searches[*slot].candidates.len()
        });
    }

    // ---- settle: every section move measured again on the drawn field, and
    // what is left judged again against the rules after each take-back.
    let no_moves: Vec<Option<[f64; 3]>> = vec![None; struts.len()];
    // Each moved tip is measured with the seat normal of the ball it was moved
    // to (`accepted_normal`), not the one it had before the slide.
    let mut settle_arms: Vec<SweepArm> = arms.to_vec();
    for (slot, normal) in accepted_normal.iter().enumerate() {
        if let (Some(normal), Some(arm)) = (normal, settle_arms.get_mut(search_arms[slot])) {
            arm.seat_normal_mm = *normal;
        }
    }
    settle(
        MoveCheck {
            grid,
            plate_z,
            height,
            limits,
            struts,
            before_ends: &before_ends,
            own_accepted: &no_moves,
            slot_of_member: &slot_of_member,
            search_arms: &search_arms,
        },
        &settle_arms,
        &section_only,
        &mut accepted,
        &mut refused_by_limits,
        &mut outcome,
        |drawn| {
            plinth_support::tip_shaft_seat_bites(drawn, layer_plan)
                .unwrap_or_else(|_| vec![None; drawn.len()])
                .iter()
                .map(|bite| crate::lip_seat::cuts(bite.as_ref()))
                .collect()
        },
    );

    // ---- the tip cover ([`TipCover`]): the field the settled slides leave,
    // walked against the placed field; a slide that strands demand is taken
    // back and its tip searched again, or, past the passes, left at its seat.
    let Some(tip_cover) = cover.as_mut() else {
        break;
    };
    // Only a slide whose ball let go of its placed seat (passed by the
    // no-climb clause alone) moves a root; the others are not walked.
    outcome.cover.slides_holding_seat = (0..accepted.len())
        .filter(|&slot| accepted[slot].is_some() && accepted_holds_seat[slot])
        .count();
    let moved: Vec<crate::demand_replay::MovedSeat> = accepted
        .iter()
        .enumerate()
        // A helper is always walked: its slide is there to move its root.
        .filter(|(slot, _)| !accepted_holds_seat[*slot] || cover_helper_of[*slot].is_some())
        .filter_map(|(slot, shift)| {
            let shift = (*shift)?;
            let index = search_arms[slot];
            let centre = struts[index].center_mm;
            Some(crate::demand_replay::MovedSeat {
                index,
                xy_mm: [centre[0] + shift[0], centre[1] + shift[1]],
                contact_radius_mm: discs[index].map_or(0.0, |disc| disc.radius_mm),
                placed_ball: discs[index].map(|disc| Slid::of(&disc).ungrown()),
                ball: accepted_ball[slot],
            })
        })
        .collect();
    if moved.is_empty() {
        break;
    }
    outcome.cover.enabled = true;
    outcome.cover.checks += 1;
    let stranded = (tip_cover.stranded)(&*struts, &moved);
    if crate::cancel::cancelled() {
        return outcome;
    }
    if outcome.cover.checks == 1 {
        let found = &mut outcome.cover;
        found.found_cells = stranded.len();
        found.found_surface_cells = stranded.iter().filter(|cell| cell.surface).count();
        #[expect(clippy::cast_precision_loss, reason = "cell counts are small")]
        let surface = found.found_surface_cells as f64;
        found.found_surface_mm2 = surface * grid.cell_area_mm2();
        let mut blamed: Vec<usize> = stranded.iter().map(|cell| cell.blamed).collect();
        blamed.sort_unstable();
        blamed.dedup();
        found.found_slides = blamed.len();
        found.found_by_slide = shortfalls(grid, &stranded);
        found.found_max_u = found.found_by_slide.iter().map(|row| row.max_u).fold(0.0, f64::max);
        for row in &mut found.found_by_slide {
            row.slide_mm = slot_of_member
                .get(row.arm)
                .and_then(|&slot| accepted.get(slot).copied().flatten())
                .map_or(0.0, geom_kernel::vec3::norm);
        }
    }
    if stranded.is_empty() {
        break;
    }
    cover_turns += 1;
    // Each kept slide adds its helpers once and gives them fresh passes.
    if cover_turns > 2 * searches.len() + cover_passes_max * (1 + cover_spawns) + 4 {
        // The backstop: never expected (each turn takes a slide back). What
        // is left is emitted and warned about by area.
        outcome.cover.left = shortfalls(grid, &stranded);
        break;
    }
    // Every blamed slot, in slot order, except an onto-face slide already
    // kept: it is searched again from its next candidate while passes remain,
    // with the cells its slide stranded on its seat layer as the prefilter's
    // targets. A SECTION slide is taken back first (its disc was on its face
    // before it moved, so the placed seat costs no contact): undone if the
    // search finds nothing. An ONTO-FACE slide stands while it is searched
    // and is replaced only by a seat that is on the face too: undoing it
    // would hang the tip's disc off its face again, and contact area governs
    // (owner ruling 2026-08-30), so where nothing replaces it the slide the
    // rule first chose is kept and what it strands is warned about by area.
    let mut changed = false;
    // A kept slide whose helpers were added and that still strands: its
    // helpers failed (all of them or none: a helper's slide is only worth
    // its cost if the area is covered), so their slides are taken back.
    for kept in 0..searches.len() {
        if !(cover_kept[kept] && cover_helpers_added[kept] && !cover_helpers_failed[kept]) {
            continue;
        }
        let kept_arm = search_arms[kept];
        if !stranded.iter().any(|cell| cell.blamed == kept_arm) {
            continue;
        }
        cover_helpers_failed[kept] = true;
        for helper in 0..searches.len() {
            if cover_helper_of[helper] != Some(kept) {
                continue;
            }
            cover_retired[helper] = true;
            if accepted[helper].is_some() {
                accepted[helper] = None;
                accepted_normal[helper] = None;
                own_accepted[search_arms[helper]] = None;
                changed = true;
            }
        }
        pending.retain(|slot| !cover_retired[*slot]);
    }
    let mut blamed_slots: Vec<usize> = stranded
        .iter()
        .filter_map(|cell| slot_of_member.get(cell.blamed).copied())
        .filter(|&slot| {
            slot != usize::MAX
                && accepted.get(slot).is_some_and(Option::is_some)
                && !cover_kept[slot]
                && !cover_retired[slot]
        })
        .collect();
    blamed_slots.sort_unstable();
    blamed_slots.dedup();
    if blamed_slots.is_empty() && !changed {
        // Only kept onto-face slides strand anything, and their helpers were
        // tried: warned by area.
        outcome.cover.left = shortfalls(grid, &stranded);
        for row in &mut outcome.cover.left {
            row.slide_mm = slot_of_member
                .get(row.arm)
                .and_then(|&slot| accepted.get(slot).copied().flatten())
                .map_or(0.0, geom_kernel::vec3::norm);
        }
        break;
    }
    let research = cover_passes < tip_cover.passes;
    if research && !blamed_slots.is_empty() {
        cover_passes += 1;
        outcome.cover.passes += 1;
    }
    // Kept slides of this turn, whose helpers are added after the loop.
    let mut newly_kept: Vec<usize> = Vec::new();
    for &slot in &blamed_slots {
        let arm = search_arms[slot];
        cover_blamed[slot] = true;
        if cover_original[slot].is_none() {
            cover_original[slot] = accepted[slot].map(|shift| {
                (shift, accepted_normal[slot], accepted_ball[slot], accepted_holds_seat[slot])
            });
        }
        let can_search = research && searches[slot].next < searches[slot].candidates.len();
        if !section_only[arm] {
            if can_search {
                pending.push(slot);
            } else {
                // Kept: back to the slide the rule first chose, if a later
                // candidate replaced it; its neighbours are tried next.
                cover_kept[slot] = true;
                newly_kept.push(slot);
                if let Some((shift, normal, ball, holds)) = cover_original[slot] {
                    if accepted[slot] != Some(shift) {
                        accepted[slot] = Some(shift);
                        accepted_normal[slot] = normal;
                        accepted_ball[slot] = ball;
                        accepted_holds_seat[slot] = holds;
                        changed = true;
                    }
                }
            }
        } else {
            accepted[slot] = None;
            accepted_normal[slot] = None;
            own_accepted[arm] = None;
            refused_by_limits[slot] = true;
            outcome.cover.slides_taken_back += 1;
            changed = true;
            if can_search {
                pending.push(slot);
            }
        }
        let k_top = struts[arm].k_top;
        let on_seat_layer: Vec<[f64; 2]> = stranded
            .iter()
            .filter(|cell| cell.blamed == arm && cell.layer == k_top)
            .map(|cell| {
                [
                    grid.center_x_mm(grid.col_of(cell.cell)),
                    grid.center_y_mm(grid.row_of(cell.cell)),
                ]
            })
            .collect();
        cover_targets[slot].extend(on_seat_layer);
    }
    // THE NEIGHBOUR REPAIR (a): for each slide kept this turn, the tips that
    // can root the cells it strands try to slide to cover them. A helper is a
    // seated, unheaded arm on its face, not searched already, rooted on the
    // layer the kept slide strands first, whose search can reach those cells
    // (its root within one lamina's reach plus its search radius of the
    // farthest of them). It is searched exactly as a failing tip is
    // (`search_for`: its lattice graded, its ground window, every rule a slide
    // is held to), and as a section tip is (a slide must not cut its shaft),
    // with the stranded cells as the prefilter's targets; nearest first.
    for &kept in &newly_kept {
        let kept_arm = search_arms[kept];
        let Some(layer) = stranded
            .iter()
            .filter(|cell| cell.blamed == kept_arm)
            .map(|cell| cell.layer)
            .min()
        else {
            continue;
        };
        let targets: Vec<[f64; 2]> = stranded
            .iter()
            .filter(|cell| cell.blamed == kept_arm && cell.layer == layer)
            .map(|cell| {
                [
                    grid.center_x_mm(grid.col_of(cell.cell)),
                    grid.center_y_mm(grid.row_of(cell.cell)),
                ]
            })
            .collect();
        cover_helpers_added[kept] = true;
        let first_new = searches.len();
        let mut chosen: Vec<(f64, Search)> = Vec::new();
        for (arm, strut) in struts.iter().enumerate() {
            if strut.kind != crate::StrutKind::Arm
                || strut.head.is_some()
                || strut.k_top != layer
                || slot_of_member[arm] != usize::MAX
                || strut.seat_face.footprint != Footprint::OnFace
                || frames[arm].is_none()
            {
                continue;
            }
            let farthest = targets
                .iter()
                .map(|cell| (cell[0] - strut.center_mm[0]).hypot(cell[1] - strut.center_mm[1]))
                .fold(0.0, f64::max);
            let start = lattice.len();
            let Some(search) = search_for(arm, &mut lattice) else {
                continue;
            };
            if farthest > cover_reach + f64::from(search.half_width) * pitch {
                lattice.truncate(start);
                continue;
            }
            chosen.push((farthest, search));
        }
        // Graded in one pass, nearest helper first.
        let tail = graded(grid, slices, surface, &lattice[lattice_hit.len()..], &mut outcome.layers_rasterised);
        lattice_hit.extend(tail);
        chosen.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.arm.cmp(&b.1.arm)));
        let added: Vec<Search> = chosen.into_iter().map(|(_, search)| search).collect();
        if ground_check {
            windows.extend(windows_for(&added));
        }
        for search in added {
            let slot = searches.len();
            let arm = search.arm;
            slot_of_member[arm] = slot;
            search_arms.push(arm);
            searches.push(search);
            accepted.push(None);
            proposal_normal.push(None);
            accepted_normal.push(None);
            proposal_ball.push(None);
            accepted_ball.push(None);
            proposal_holds_seat.push(false);
            accepted_holds_seat.push(false);
            refused_by_limits.push(false);
            cover_targets.push(targets.clone());
            cover_blamed.push(false);
            cover_original.push(None);
            cover_kept.push(false);
            cover_helper_of.push(Some(kept));
            cover_helpers_added.push(false);
            cover_helpers_failed.push(false);
            cover_retired.push(false);
            section_only[arm] = true;
            is_helper[arm] = true;
            pending.push(slot);
        }
        outcome.cover.helpers_searched += searches.len() - first_new;
        if searches.len() > first_new {
            // The helpers get passes of their own.
            cover_spawns += 1;
            cover_passes = 0;
        } else {
            cover_helpers_failed[kept] = true;
        }
    }
    pending.sort_unstable();
    pending.dedup();
    // A slide taken back (or put back) changes what the members joined to it
    // follow: every slide left is judged again on the drawn ends, as `settle`
    // does after a section take-back.
    let left: Vec<(usize, [f64; 3])> = accepted
        .iter()
        .enumerate()
        .filter_map(|(slot, shift)| shift.map(|shift| (slot, shift)))
        .collect();
    if changed && !left.is_empty() {
        let mut kept = vec![false; accepted.len()];
        let admitted = check_moves(
            MoveCheck {
                grid,
                plate_z,
                height,
                limits,
                struts,
                before_ends: &before_ends,
                own_accepted: &no_moves,
                slot_of_member: &slot_of_member,
                search_arms: &search_arms,
            },
            left.clone(),
            &mut refused_by_limits,
            &mut outcome,
        );
        for (slot, _) in admitted {
            kept[slot] = true;
        }
        for (slot, _) in left {
            if !kept[slot] {
                accepted[slot] = None;
                accepted_normal[slot] = None;
                own_accepted[search_arms[slot]] = None;
                outcome.moves_taken_back_limits += 1;
            }
        }
    }
    // The re-search reads the moves standing (`settle` may have taken some
    // back without the round state knowing).
    own_accepted.fill(None);
    for (slot, shift) in accepted.iter().enumerate() {
        own_accepted[search_arms[slot]] = *shift;
    }
    }
    for (slot, blamed) in cover_blamed.iter().enumerate() {
        if cover_helper_of[slot].is_some() {
            if accepted[slot].is_some() {
                outcome.cover.helpers.push(search_arms[slot]);
            }
            continue;
        }
        if *blamed {
            let arm = search_arms[slot];
            match (accepted[slot].is_some(), cover_kept[slot]) {
                (true, true) => outcome.cover.kept.push(arm),
                (true, false) => outcome.cover.reseated.push(arm),
                (false, _) => outcome.cover.undone.push(arm),
            }
        }
    }

    // ---- publish: moves, their consequence for branches, and knife edges.
    for (slot, search) in searches.iter().enumerate() {
        let arm = search.arm;
        match accepted[slot] {
            Some(shift) => {
                let length = geom_kernel::vec3::norm(shift);
                if is_helper[arm] {
                    // Counted in `cover.helpers`.
                } else if section_only[arm] {
                    outcome.tips_moved_for_section += 1;
                    outcome.moved_for_section.push(struts[arm].id);
                } else {
                    outcome.tips_moved_onto_face += 1;
                }
                outcome.max_move_mm = outcome.max_move_mm.max(length);
                // The strut carries the normal of the ball it was moved to.
                if let Some(normal) = accepted_normal[slot] {
                    struts[arm].seat_normal = normal;
                }
                struts[arm].seat_face.shift_mm = shift;
                struts[arm].seat_face.footprint = Footprint::MovedOntoFace;
            }
            // A disc already on its face keeps its seat and its status; the
            // lip-seat census warns about its shaft.
            None if section_only[arm] => {}
            None => {
                #[expect(clippy::cast_precision_loss, reason = "sample counts are small")]
                let fraction = on[arm] as f64 / total[arm].max(1) as f64;
                if refused_by_limits[slot] {
                    outcome.moves_refused.push((arm, fraction));
                    struts[arm].seat_face.footprint = Footprint::MoveRefused {
                        disc_on_face: fraction,
                    };
                } else {
                    outcome.knife_edges.push((arm, fraction));
                    struts[arm].seat_face.footprint = Footprint::KnifeEdge {
                        disc_on_face: fraction,
                    };
                }
            }
        }
    }
    // A moved seat moves its member's drawn axis (seat to base, straight), so
    // every member joined to that axis FOLLOWS it (`followed_shifts`): a
    // branch's landing and a brace's top move with the host's axis at their
    // own height, and never come off it. Counted here.
    for (strut, followed) in struts.iter().zip(followed_shifts(struts)) {
        let own = strut.seat_face.footprint == Footprint::MovedOntoFace;
        let top = followed.seat_mm.filter(|_| !own);
        if followed.base_mm.is_some() || top.is_some() {
            outcome.members_following_a_moved_host += 1;
            let moved = [followed.base_mm, top]
                .into_iter()
                .flatten()
                .map(geom_kernel::vec3::norm)
                .fold(0.0, f64::max);
            outcome.max_follow_mm = outcome.max_follow_mm.max(moved);
        }
    }
    outcome
}

/// **A tip moved for its section keeps it on the geometry the file draws.**
/// The search measured each such move with its own seat shifted and its base
/// where it stood; but the moves are taken together, and a member whose host
/// moved in the same set has its base FOLLOW the host's axis
/// ([`followed_shifts`]), which re-tilts its shaft. Measured on the stilt
/// plate: member 66 moved for its section and came out cut 0.0013 mm on the
/// emitted field. So every section move is measured again on the drawn ends
/// with every accepted shift applied (the lowering's own measurement, all of
/// them in one call), and a move still cut is taken back: its disc was on its
/// face before the move, so it keeps its seat, as a tip no slide held would.
/// Taking one back can change what its dependants follow, so this repeats
/// until nothing is taken back (each round removes at least one move).
///
/// **And what is left still holds the rules** (round5-review-fixes,
/// 2026-09-29). The search admitted each move on the field WITH every other
/// accepted move ([`check_moves`]), so a branch's seat slide could pass the
/// lean law only because its base followed its host's slide. Taking the
/// host's slide back takes that base displacement away, and the branch then
/// leans by its own slide alone: a branch with 1 mm of drop and 1.55 mm of
/// lateral travel, both ends moved +0.10 mm, stays at 1.55; with the host's
/// slide taken back it is at 1.65, past the lean law's ~1.57. So after every
/// take-back, every move left is judged again by the same move check on the
/// same drawn ends ([`MoveLimits`]: the lean law, the unbraced run's limit,
/// the drain keep-out, each "no worse than it was" before any move), and a
/// move whose displaced members break a rule is taken back too. A tip whose
/// disc was off its face then keeps its seat and is warned about, as a tip
/// whose every slide broke a rule is. Each round removes at least one move,
/// so this ends.
///
/// `cut` is the section measurement, one call per round over every section
/// move drawn with the round's follows: the lowering's own
/// [`plinth_support::tip_shaft_seat_bites`] read by [`crate::lip_seat::cuts`]
/// in production.
fn settle(
    check: MoveCheck<'_>,
    arms: &[SweepArm],
    section_only: &[bool],
    accepted: &mut [Option<[f64; 3]>],
    refused_by_limits: &mut [bool],
    outcome: &mut FootprintOutcome,
    mut cut: impl FnMut(&[SweepArm]) -> Vec<bool>,
) {
    let struts = check.struts;
    let mut own: Vec<Option<[f64; 3]>> = vec![None; struts.len()];
    loop {
        own.fill(None);
        for (&member, shift) in check.search_arms.iter().zip(accepted.iter()) {
            own[member] = *shift;
        }
        let section: Vec<usize> = (0..accepted.len())
            .filter(|&slot| accepted[slot].is_some() && section_only[check.search_arms[slot]])
            .collect();
        if section.is_empty() {
            break;
        }
        let followed = followed_shifts_with(struts, &own, &[]);
        let drawn: Vec<SweepArm> = section
            .iter()
            .map(|&slot| {
                let index = check.search_arms[slot];
                let arm = &arms[index];
                let Followed { seat_mm, base_mm } = followed[index];
                let shift = own[index].unwrap_or([0.0; 3]);
                SweepArm {
                    seat_mm: seat_mm
                        .map_or(arm.seat_mm, |by| geom_kernel::vec3::add(arm.seat_mm, by)),
                    base_mm: base_mm
                        .map_or(arm.base_mm, |by| geom_kernel::vec3::add(arm.base_mm, by)),
                    seat_face_point_mm: arm
                        .seat_face_point_mm
                        .map(|point| geom_kernel::vec3::add(point, shift)),
                    ..arm.clone()
                }
            })
            .collect();
        let cuts = cut(&drawn);
        let mut taken_back = false;
        for (&slot, &still_cut) in section.iter().zip(&cuts) {
            if still_cut {
                accepted[slot] = None;
                outcome.section_moves_taken_back += 1;
                taken_back = true;
            }
        }
        if !taken_back {
            break;
        }
        // THE MOVE CHECK again, on every move left: the whole set is this
        // round's (`check.own_accepted` holds no move), so every member any of
        // them displaces is judged on the ends the file will draw.
        let left: Vec<(usize, [f64; 3])> = accepted
            .iter()
            .enumerate()
            .filter_map(|(slot, shift)| shift.map(|shift| (slot, shift)))
            .collect();
        if left.is_empty() {
            break;
        }
        outcome.settle_rechecks += 1;
        let mut kept = vec![false; accepted.len()];
        for (slot, _) in check_moves(check, left.clone(), refused_by_limits, outcome) {
            kept[slot] = true;
        }
        for (slot, _) in left {
            if !kept[slot] {
                accepted[slot] = None;
                outcome.moves_taken_back_limits += 1;
            }
        }
    }
}

/// Everything the batched move check reads that does not change within a
/// round.
#[derive(Clone, Copy)]
struct MoveCheck<'a> {
    grid: &'a Grid,
    plate_z: f64,
    height: f64,
    limits: &'a MoveLimits<'a>,
    struts: &'a [SweepStrut],
    before_ends: &'a [([f64; 3], [f64; 3])],
    own_accepted: &'a [Option<[f64; 3]>],
    /// Per member: the search slot its own move belongs to, `usize::MAX` for
    /// a member the footprint rule never moves itself.
    slot_of_member: &'a [usize],
    /// Per search slot: the member it moves.
    search_arms: &'a [usize],
}

/// Judge one round's on-face moves ([`MoveLimits`]) and return the ones
/// admitted. A refused move's slot is flagged in `refused_by_limits`, and the
/// search goes on to that tip's next candidate.
fn check_moves(
    check: MoveCheck<'_>,
    mut round: Vec<(usize, [f64; 3])>,
    refused_by_limits: &mut [bool],
    outcome: &mut FootprintOutcome,
) -> Vec<(usize, [f64; 3])> {
    let struts = check.struts;
    let hosts = |index: usize| -> [Option<usize>; 2] {
        let strut = &struts[index];
        let seat_host = strut
            .seat_on_strut
            .map(|id| id as usize)
            .filter(|id| *id != index && *id < struts.len());
        let base_host = match strut.landing {
            Landing::Strut { id, .. } => {
                Some(id as usize).filter(|id| *id != index && *id < struts.len())
            }
            _ => None,
        };
        [seat_host, base_host]
    };
    let mut in_round = vec![false; check.search_arms.len()];
    let mut blamed: Vec<Option<Broken>> = vec![None; check.search_arms.len()];
    let mut walk: Vec<usize> = Vec::new();
    let mut seen: Vec<usize> = Vec::new();
    while !round.is_empty() {
        let mut own = check.own_accepted.to_vec();
        in_round.fill(false);
        blamed.fill(None);
        for &(slot, shift) in &round {
            own[check.search_arms[slot]] = Some(shift);
            in_round[slot] = true;
        }
        let followed = followed_shifts_with(struts, &own, &[]);
        for (index, moved) in followed.iter().enumerate() {
            if moved.seat_mm.is_none() && moved.base_mm.is_none() {
                continue;
            }
            outcome.members_checked += 1;
            let before = check.before_ends[index];
            let Some(rule) = broken_rule(
                check.grid,
                check.plate_z,
                check.height,
                check.limits,
                &struts[index],
                index,
                before,
                displaced(before, *moved),
            ) else {
                continue;
            };
            // Blame every move of this round the member inherits its
            // displacement from: itself, and its hosts, transitively.
            walk.clear();
            seen.clear();
            walk.push(index);
            while let Some(member) = walk.pop() {
                if seen.contains(&member) || seen.len() > struts.len() {
                    continue;
                }
                seen.push(member);
                let slot = check.slot_of_member[member];
                if slot != usize::MAX && in_round[slot] && blamed[slot].is_none() {
                    blamed[slot] = Some(rule);
                }
                walk.extend(hosts(member).into_iter().flatten());
            }
        }
        let before = round.len();
        round.retain(|(slot, _)| blamed[*slot].is_none());
        if round.len() == before {
            break;
        }
        for (slot, rule) in blamed.iter().enumerate() {
            let Some(rule) = rule else { continue };
            refused_by_limits[slot] = true;
            match rule {
                Broken::Lean => outcome.moves_refused_lean += 1,
                Broken::Slenderness => outcome.moves_refused_slenderness += 1,
                Broken::Drain => outcome.moves_refused_drain += 1,
            }
        }
    }
    round
}

/// How one member moves because a seat moved: its seat and its base
/// displacements, each `None` where that end does not move at all (so an
/// untouched member is bit-identical, not moved by zero).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Followed {
    pub seat_mm: Option<[f64; 3]>,
    pub base_mm: Option<[f64; 3]>,
}

/// **Every member joined to a moved axis follows it** (fix/tip-trim,
/// 2026-09-28). Per strut, its seat and base displacements ([`Followed`]).
///
/// A member is drawn as one straight line from its seat to its base. When the
/// footprint rule slides a seat along its face, that line moves: at a height
/// a fraction `t` of the way from base to seat it moves by
/// `t · seat_move + (1 − t) · base_move`. A branch whose landing is on that
/// member (`Landing::Strut`), and a brace whose top seats on it
/// (`seat_on_strut`), were placed ON the old line; each is moved by the
/// line's own displacement at its junction height, so it lands on the new
/// line exactly as it landed on the old one. A branch that moves moves its own
/// line in turn, so the displacements are resolved hosts first (an iterative
/// depth-first walk, no recursion). A dependency cycle, which placement never
/// builds, is broken by reading the member still being resolved as unmoved.
///
/// Every entry is `Followed::default()` — nothing moves — when no seat moved.
#[must_use]
pub fn followed_shifts(struts: &[SweepStrut]) -> Vec<Followed> {
    let own: Vec<Option<[f64; 3]>> = struts
        .iter()
        .map(|strut| {
            (strut.seat_face.footprint == Footprint::MovedOntoFace)
                .then_some(strut.seat_face.shift_mm)
        })
        .collect();
    followed_shifts_with(struts, &own, &[])
}

/// [`followed_shifts`], with each member's DRAWN line offset from its planned
/// one (`line_offsets[i]`, [`crate::adapter::drawn_line_offset`]; an empty
/// slice or `None` means the two coincide) added to the displacement a
/// member joined to it follows (fix/morel-long-posts, 2026-10-02).
///
/// The offset moves only the members JOINED to a member — the branch that
/// lands on it, the diagonal that seats or lands on it — never the member's
/// own ends: the lowering already draws a leaning plate post along its drawn
/// line, and what was off that line was everything placed on the planned
/// one. A member nothing is joined to, or joined only to members whose two
/// lines coincide, follows exactly what [`followed_shifts`] gives it.
#[must_use]
pub fn followed_shifts_drawn(
    struts: &[SweepStrut],
    line_offsets: &[Option<crate::adapter::LineOffset>],
) -> Vec<Followed> {
    let own: Vec<Option<[f64; 3]>> = struts
        .iter()
        .map(|strut| {
            (strut.seat_face.footprint == Footprint::MovedOntoFace)
                .then_some(strut.seat_face.shift_mm)
        })
        .collect();
    followed_shifts_with(struts, &own, line_offsets)
}

/// [`followed_shifts`] for a proposed set of own seat moves (`own[i]` is
/// member `i`'s own seat move, `None` where it has none), so the footprint
/// rule can judge a set of moves before it writes any of them; and with the
/// hosts' drawn-line offsets of [`followed_shifts_drawn`] (empty: none).
fn followed_shifts_with(
    struts: &[SweepStrut],
    own_moves: &[Option<[f64; 3]>],
    line_offsets: &[Option<crate::adapter::LineOffset>],
) -> Vec<Followed> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        New,
        Busy,
        Done,
    }
    let mut out = vec![Followed::default(); struts.len()];
    if own_moves.iter().all(Option::is_none) && line_offsets.iter().all(Option::is_none) {
        return out;
    }
    // A host's drawn line off its planned one, at `layer` (`None` where the
    // two coincide).
    let drawn_off = |host: usize, layer: usize| -> Option<[f64; 3]> {
        line_offsets
            .get(host)
            .copied()
            .flatten()
            .map(|offset| crate::adapter::offset_at(&offset, &struts[host], layer))
    };
    let hosts = |index: usize| -> [Option<usize>; 2] {
        let strut = &struts[index];
        let seat_host = strut
            .seat_on_strut
            .map(|id| id as usize)
            .filter(|id| *id != index && *id < struts.len());
        let base_host = match strut.landing {
            crate::Landing::Strut { id, .. } => {
                Some(id as usize).filter(|id| *id != index && *id < struts.len())
            }
            _ => None,
        };
        [seat_host, base_host]
    };
    // The host's axis displacement at `layer`; `None` for an unmoved host.
    let moved_along = |out: &[Followed], host: usize, layer: usize| -> Option<[f64; 3]> {
        let h = &struts[host];
        let Followed { seat_mm, base_mm } = out[host];
        if seat_mm.is_none() && base_mm.is_none() {
            return None;
        }
        let (seat, base) = (seat_mm.unwrap_or([0.0; 3]), base_mm.unwrap_or([0.0; 3]));
        // The displaced axis runs from the base to the member's POST top: its
        // frozen joint K for a headed arm (the head is never moved), its
        // `k_top` for a straight one.
        let post_top = h.post_top_layer();
        if post_top <= h.k_bottom || h.k_bottom == usize::MAX {
            return Some(seat);
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "layer indices bounded by the plan's layer count are exact in f64"
        )]
        let t = ((layer.saturating_sub(h.k_bottom)) as f64 / (post_top - h.k_bottom) as f64)
            .clamp(0.0, 1.0);
        Some(std::array::from_fn(|axis| {
            t * seat[axis] + (1.0 - t) * base[axis]
        }))
    };
    // The host's axis displacement at `layer`, its drawn-line offset there
    // added; `None` for a host that neither moved nor draws off its plan.
    let along = |out: &[Followed], host: usize, layer: usize| -> Option<[f64; 3]> {
        let moved = moved_along(out, host, layer);
        match (moved, drawn_off(host, layer)) {
            (moved, None) => moved,
            (None, Some(off)) => Some(off),
            (Some(moved), Some(off)) => Some(std::array::from_fn(|axis| moved[axis] + off[axis])),
        }
    };
    let mut state = vec![State::New; struts.len()];
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..struts.len() {
        stack.push(start);
        while let Some(&index) = stack.last() {
            if state[index] == State::Done {
                stack.pop();
                continue;
            }
            if state[index] == State::New {
                state[index] = State::Busy;
                let pending: Vec<usize> = hosts(index)
                    .into_iter()
                    .flatten()
                    .filter(|host| state[*host] == State::New)
                    .collect();
                if !pending.is_empty() {
                    stack.extend(pending);
                    continue;
                }
            }
            let strut = &struts[index];
            let [seat_host, base_host] = hosts(index);
            let own = own_moves.get(index).copied().flatten();
            let seat_mm = match seat_host {
                Some(host) if state[host] == State::Done => along(&out, host, strut.k_top),
                _ => own,
            };
            let base_mm = match (base_host, strut.landing) {
                (Some(host), crate::Landing::Strut { layer, .. }) if state[host] == State::Done => {
                    along(&out, host, layer)
                }
                _ => None,
            };
            out[index] = Followed { seat_mm, base_mm };
            state[index] = State::Done;
            stack.pop();
        }
    }
    out
}

#[cfg(test)]
mod ground_tests {
    //! [`keeps_ground`] on hand-built masks: the Deinonychus arm-101 slide
    //! (0.51 mm down its face, the seat island at 7.925 mm left over air),
    //! and the two kinds of slide the check must keep admitting.

    use super::{keeps_ground, keeps_seat, Ground, GroundWindow, Slid};
    use crate::grid::lay_out_at;

    const PITCH: f64 = 0.05;
    const H: f64 = 0.05;
    const SIDE: usize = 100;
    /// The layer the island's first lamina cures on.
    const K: usize = 50;

    /// A 5 x 5 mm window of the grid over the ball's whole height and two
    /// layers either side (`K - 14 ..= K + 2`), model (cover and carry)
    /// wherever `model(row, col, layer)` says.
    fn window(model: impl Fn(usize, usize, usize) -> bool) -> GroundWindow {
        let (layer0, layers) = (K - 14, 17);
        let mut bits = vec![0; SIDE * SIDE * layers];
        for k in 0..layers {
            for r in 0..SIDE {
                for c in 0..SIDE {
                    if model(r, c, layer0 + k) {
                        bits[(k * SIDE + r) * SIDE + c] = GroundWindow::COVER | GroundWindow::CARRY;
                    }
                }
            }
        }
        GroundWindow {
            row0: 0,
            col0: 0,
            rows: SIDE,
            cols: SIDE,
            layer0,
            layers,
            bits,
        }
    }

    /// A ball of radius 0.4 mm centred at `(x, 2.5)`, its centre 0.2 mm under
    /// the island's first lamina: its section under the lamina holds the
    /// cells within about 0.33 mm of its axis.
    fn ball(x: f64) -> Slid {
        #[expect(clippy::cast_precision_loss, reason = "a small layer index")]
        let lamina = K as f64 * H;
        Slid {
            ball_centre: [x, 2.5, lamina - 0.2],
            ball_radius: 0.4,
            planned_ball_radius: 0.4,
            disc_centre: [x, 2.5, lamina - 0.2],
            disc_radius: 0.3,
            normal: [0.0, 0.0, -1.0],
        }
    }

    /// The island: model from layer `K` up, over a 0.3 mm square at (2.5, 2.5).
    fn island(r: usize, c: usize, k: usize) -> bool {
        k >= K && (47..53).contains(&r) && (47..53).contains(&c)
    }

    #[test]
    fn a_slide_that_takes_the_ball_from_under_an_island_is_refused() {
        let grid = lay_out_at(0.0, 0.0, PITCH, SIDE, SIDE);
        let masks = window(island);
        let ground = Ground {
            grid: &grid,
            window: &masks,
            plate_z: 0.0,
            height: H,
        };
        let (placed, moved) = (ball(2.5), ball(3.0));
        // The premise, and the defect: the old rule passes this slide (it
        // does not climb), and the island is left over nothing.
        assert!(
            keeps_seat([2.5, 2.5, 2.5], placed, moved, PITCH, H),
            "premise: the pre-2026-10-12 rule admits the slide"
        );
        assert!(!keeps_ground(&ground, placed, moved), "the island must keep its ball");
        // Control: no slide holds what it held.
        assert!(keeps_ground(&ground, placed, placed));
    }

    #[test]
    fn a_slide_along_one_lamina_keeps_its_ground() {
        // A level face: the island's first lamina runs the whole window, so
        // the cells the slide uncovers join, in their own layer, cells the
        // moved ball still holds (the flat discs' rim slides).
        let grid = lay_out_at(0.0, 0.0, PITCH, SIDE, SIDE);
        let masks = window(|_, _, k| k >= K);
        let ground = Ground {
            grid: &grid,
            window: &masks,
            plate_z: 0.0,
            height: H,
        };
        assert!(keeps_ground(&ground, ball(2.5), ball(2.8)));
    }

    #[test]
    fn a_rim_slide_onto_a_flat_lamina_keeps_its_ground() {
        // A flat disc's first lamina as the sweep rasterises it: model surely
        // stands (`carry`) up to column 49, and may stand (`cover`) two cells
        // further, the raster's rounding band. A ball on the rim slides inward:
        // the ring cells it leaves are the uncertain edge of the lamina and
        // join it (the flipped Round 25's rim tips).
        let grid = lay_out_at(0.0, 0.0, PITCH, SIDE, SIDE);
        let mut masks = window(|_, c, k| k >= K && c < 50);
        for k in 0..masks.layers {
            if masks.layer0 + k < K {
                continue;
            }
            for r in 0..SIDE {
                for c in 50..52 {
                    masks.bits[(k * SIDE + r) * SIDE + c] = GroundWindow::COVER;
                }
            }
        }
        let ground = Ground {
            grid: &grid,
            window: &masks,
            plate_z: 0.0,
            height: H,
        };
        assert!(keeps_ground(&ground, ball(2.6), ball(2.3)));
    }

    #[test]
    fn material_with_model_under_it_is_not_the_balls_to_keep() {
        // The "island" is the end of a column of model that runs on down
        // past the ball: it is carried from below, whatever the ball does.
        let grid = lay_out_at(0.0, 0.0, PITCH, SIDE, SIDE);
        let masks = window(|r, c, _| island(r, c, K));
        let ground = Ground {
            grid: &grid,
            window: &masks,
            plate_z: 0.0,
            height: H,
        };
        assert!(keeps_ground(&ground, ball(2.5), ball(3.0)));
    }
}

#[cfg(test)]
#[path = "footprint_ground_tests.rs"]
mod ground_oracle_tests;

#[cfg(test)]
#[path = "footprint_cover_tests.rs"]
mod cover_tests;

#[cfg(test)]
mod move_check_tests {
    use super::{broken_rule, Broken, MoveLimits};
    use crate::drain::{DrainKeepOut, DrainSpec};
    use crate::grid::{lay_out_at, Grid};
    use crate::{Landing, SeatFace, StrutKind, SweepStrut};

    const HEIGHT: f64 = 0.05;
    const PLATE: f64 = 0.0;
    /// The lean law at the registered 45° centre and 25° transition: the
    /// saturated 32.5°, `cot 32.5° = 1.5697` lateral per unit drop.
    fn max_lean() -> f64 {
        1.0 / 32.5_f64.to_radians().tan()
    }

    fn grid() -> Grid {
        lay_out_at(0.0, 0.0, 0.05, 400, 400)
    }

    /// A plate arm from layer 0 to layer 100 (a 5 mm drop).
    fn arm() -> SweepStrut {
        SweepStrut {
            id: 0,
            cell: 0,
            center_mm: [0.0, 0.0],
            kind: StrutKind::Arm,
            min_radius_mm: 0.0,
            thickened_mm: Vec::new(), foot_radius_mm: None,
            seat_on_strut: None,
            k_top: 100,
            k_bottom: 0,
            landing: Landing::Plate,
            interior: false,
            radius_mm: vec![0.1; 100],
            carried_mm2: vec![0.0; 100],
            covered_demand: Vec::new(), held_births: Vec::new(), twin_host: false,
            inserted_by_load: false,
            path: Vec::new(),
            straight_fallback: None,
            fallback_path: Vec::new(),
            head: None,
            seat_face: SeatFace::default(),
            seat_normal: [0.0; 3],
        }
    }

    fn judge(
        grid: &Grid,
        limits: &MoveLimits<'_>,
        before: ([f64; 3], [f64; 3]),
        after: ([f64; 3], [f64; 3]),
    ) -> Option<Broken> {
        broken_rule(grid, PLATE, HEIGHT, limits, &arm(), 0, before, after)
    }

    /// The reviewer's case: 5 mm drop, 7.5 mm lateral (1.50, inside the law);
    /// a 0.95 mm move that adds lateral travel makes it 8.45 mm (1.69).
    #[test]
    fn a_move_that_leans_an_arm_past_the_law_is_refused() {
        let grid = grid();
        let keep_out = DrainKeepOut::none();
        let limits = MoveLimits {
            max_lean: max_lean(),
            chord_slack: vec![f64::INFINITY],
            keep_out: &keep_out,
        };
        let base = [10.0, 10.0, 0.0];
        let before = ([17.5, 10.0, 5.0], base);
        let outward = ([18.45, 10.0, 5.0], base);
        let inward = ([16.55, 10.0, 5.0], base);
        assert_eq!(judge(&grid, &limits, before, outward), Some(Broken::Lean));
        assert_eq!(judge(&grid, &limits, before, inward), None);
        // Already past the law (by a placement this rule did not make): held
        // to no worse, never refused for what it already was.
        let steep = ([18.5, 10.0, 5.0], base);
        let steeper = ([18.6, 10.0, 5.0], base);
        let gentler = ([18.4, 10.0, 5.0], base);
        assert_eq!(judge(&grid, &limits, steep, steeper), Some(Broken::Lean));
        assert_eq!(judge(&grid, &limits, steep, gentler), None);
    }

    /// A member whose tightest run is exactly at its limit may not grow its
    /// chord; one with 10 % of slack may grow it by up to 10 %.
    #[test]
    fn a_move_that_lengthens_a_run_past_its_limit_is_refused() {
        let grid = grid();
        let keep_out = DrainKeepOut::none();
        let base = [10.0, 10.0, 0.0];
        let before = ([10.0, 10.0, 5.0], base);
        // Chord 5 mm -> 5.0990 mm (+1.98 %).
        let moved = ([11.0, 10.0, 5.0], base);
        let at_limit = MoveLimits {
            max_lean: max_lean(),
            chord_slack: vec![1.0],
            keep_out: &keep_out,
        };
        assert_eq!(
            judge(&grid, &at_limit, before, moved),
            Some(Broken::Slenderness)
        );
        let with_slack = MoveLimits {
            max_lean: max_lean(),
            chord_slack: vec![1.1],
            keep_out: &keep_out,
        };
        assert_eq!(judge(&grid, &with_slack, before, moved), None);
    }

    /// **The reviewer's case for `settle`** (round5-review-fixes 2026-09-29):
    /// host H, a plate arm moved for its section alone; branch B landing on
    /// H's axis at H's top (so B's base follows H's slide in full), moved for
    /// contact area. B drops 1 mm with 1.55 mm of lateral travel. Both slid
    /// +0.10 mm in x: B stays at 1.55, inside the lean law, and the move check
    /// admits the pair. H's shaft is still cut on the drawn field, so its
    /// slide is taken back; B's own slide alone puts it at 1.65, past the
    /// law, so B's slide must go too, and B is marked refused by the limits.
    #[test]
    fn a_section_take_back_judges_the_moves_left_again() {
        use super::{check_moves, settle, FootprintOutcome, MoveCheck};
        let grid = grid();
        let keep_out = DrainKeepOut::none();
        let limits = MoveLimits {
            max_lean: max_lean(),
            chord_slack: vec![f64::INFINITY; 2],
            keep_out: &keep_out,
        };
        let (row, host_col, branch_col) = (200, 100, 131);
        let host_cell = grid.index(row, host_col);
        let branch_cell = grid.index(row, branch_col);
        let host = SweepStrut {
            id: 0,
            cell: host_cell,
            center_mm: [grid.center_x_mm(host_col), grid.center_y_mm(row)],
            ..arm()
        };
        let branch = SweepStrut {
            id: 1,
            cell: branch_cell,
            center_mm: [grid.center_x_mm(branch_col), grid.center_y_mm(row)],
            k_top: 120,
            k_bottom: 100,
            landing: Landing::Strut { id: 0, layer: 100 },
            radius_mm: vec![0.1; 20],
            carried_mm2: vec![0.0; 20],
            path: vec![
                u32::try_from(host_cell).expect("small grid"),
                u32::try_from(branch_cell).expect("small grid"),
            ],
            ..arm()
        };
        let struts = vec![host, branch];
        let before_ends: Vec<([f64; 3], [f64; 3])> = struts
            .iter()
            .map(|strut| crate::adapter::drawn_ends(&grid, PLATE, HEIGHT, strut))
            .collect();
        let lean = |(seat, base): ([f64; 3], [f64; 3])| {
            (seat[0] - base[0]).hypot(seat[1] - base[1]) / (seat[2] - base[2])
        };
        assert!((lean(before_ends[1]) - 1.55).abs() < 1e-9);
        let arms: Vec<plinth_support::SweepArm> = struts
            .iter()
            .zip(&before_ends)
            .map(|(strut, &(seat, base))| {
                crate::adapter::sweep_arm(strut, seat, base, None, vec![0.1])
            })
            .collect();
        let no_moves = vec![None; 2];
        let slot_of_member = [0, 1];
        let search_arms = [0, 1];
        let check = MoveCheck {
            grid: &grid,
            plate_z: PLATE,
            height: HEIGHT,
            limits: &limits,
            struts: &struts,
            before_ends: &before_ends,
            own_accepted: &no_moves,
            slot_of_member: &slot_of_member,
            search_arms: &search_arms,
        };
        let slide = [0.10, 0.0, 0.0];
        // The premise: the pair together passes the move check.
        let mut refused = vec![false; 2];
        let mut outcome = FootprintOutcome::default();
        let admitted = check_moves(
            check,
            vec![(0, slide), (1, slide)],
            &mut refused,
            &mut outcome,
        );
        assert_eq!(admitted.len(), 2, "the pair together holds the lean law");
        assert_eq!(refused, [false, false]);

        let section_only = [true, false];
        // Control: nothing cut, nothing taken back.
        let mut accepted = vec![Some(slide); 2];
        let mut outcome = FootprintOutcome::default();
        settle(
            check,
            &arms,
            &section_only,
            &mut accepted,
            &mut refused,
            &mut outcome,
            |drawn| vec![false; drawn.len()],
        );
        assert_eq!(accepted, vec![Some(slide); 2]);
        assert_eq!(outcome.section_moves_taken_back, 0);

        // H's shaft still cut: H's slide goes, and B's with it.
        let mut outcome = FootprintOutcome::default();
        let mut measured = 0;
        settle(
            check,
            &arms,
            &section_only,
            &mut accepted,
            &mut refused,
            &mut outcome,
            |drawn| {
                measured += drawn.len();
                vec![true; drawn.len()]
            },
        );
        println!(
            "settle: {measured} section arms measured, {} section moves taken back, {} moves \
             taken back by the limits (lean {}), {} re-checks, {} member checks",
            outcome.section_moves_taken_back,
            outcome.moves_taken_back_limits,
            outcome.moves_refused_lean,
            outcome.settle_rechecks,
            outcome.members_checked,
        );
        assert_eq!(outcome.section_moves_taken_back, 1);
        assert_eq!(
            accepted,
            vec![None, None],
            "the branch keeps a slide that leans it past the law once its host's is taken back"
        );
        assert_eq!(outcome.moves_taken_back_limits, 1);
        assert_eq!(outcome.moves_refused_lean, 1);
        assert_eq!(refused, [false, true]);
    }

    /// A downward drain at (15, 10): its keep-out column is refused to an arm
    /// that moves into it, never to one that was already there.
    #[test]
    fn a_move_into_a_drain_keep_out_is_refused() {
        let grid = grid();
        let drain = DrainSpec {
            outlet_mm: [15.0, 10.0, 5.0],
            outward_axis: [0.0, 0.0, -1.0],
            radius_mm: 0.5,
        };
        let keep_out = DrainKeepOut::build(&grid, &[drain], 0.2, 0.3, 120, PLATE, HEIGHT);
        assert!(!keep_out.is_empty());
        let limits = MoveLimits {
            max_lean: max_lean(),
            chord_slack: vec![f64::INFINITY],
            keep_out: &keep_out,
        };
        let clear = ([12.0, 10.0, 5.0], [12.0, 10.0, 0.0]);
        let into = ([15.0, 10.0, 5.0], [12.0, 10.0, 0.0]);
        assert_eq!(judge(&grid, &limits, clear, into), Some(Broken::Drain));
        assert_eq!(judge(&grid, &limits, into, into), None);
        assert_eq!(
            keep_out.refusals(),
            0,
            "the move check never counts a refusal"
        );
    }
}

/// [`contacts_keep_ground`] on real slices: the batched ground check of SC-0.
#[cfg(test)]
mod contacts_ground_tests {
    use super::{contacts_keep_ground, ContactBall};
    use crate::grid::lay_out_at;
    use plinth_support::perception::{slice_stack, SliceStackLimits};

    const H: f64 = 0.05;
    const PITCH: f64 = 0.05;

    /// A ball of radius `r` centred at `(x, y)`, 0.05 mm under the island's
    /// underside at z = 1.0, looking down.
    fn ball(x: f64, y: f64, r: f64) -> ContactBall {
        ContactBall {
            centre_mm: [x, y, 0.95],
            radius_mm: r,
            planned_radius_mm: r,
            normal: [0.0, 0.0, -1.0],
        }
    }

    fn verdicts(with_slab: bool, pairs: &[(ContactBall, ContactBall)]) -> Vec<bool> {
        let mut boxes = vec![([0.0, 0.0, 1.0], [0.8, 0.8, 1.5])];
        if with_slab {
            boxes.push(([-0.5, -0.5, 0.0], [1.3, 1.3, 1.0]));
        }
        let (verts, tris) = crate::member_clearance::boxes_mesh(&boxes);
        let mesh = plinth_mesh::Mesh::new(verts, tris).expect("closed boxes");
        let planes: Vec<f64> = (0..40).map(|layer| (f64::from(layer) + 0.5) * H).collect();
        let slices = slice_stack(&mesh, &planes, SliceStackLimits::production()).expect("slices");
        let grid = lay_out_at(-1.0, -1.0, PITCH, 120, 120);
        contacts_keep_ground(&grid, &slices, 0.0, H, pairs)
    }

    /// A floating island held by a 0.4 mm ball under its middle. Moving the
    /// ball 3 mm away leaves the island's first laminae over air: ground
    /// lost. The same move under an island that stands on a slab keeps it
    /// (the slab carries it), and an unmoved ball, or a slightly smaller one
    /// whose lost rim still joins the cells it holds, keeps it too. Pairs are
    /// answered in one batch and in order.
    #[test]
    fn a_contact_moved_off_its_island_loses_the_ground_and_a_carried_one_does_not() {
        let held = ball(0.4, 0.4, 0.4);
        let pairs = [
            (held, ball(3.0, 3.0, 0.4)),
            (held, held),
            (held, ball(0.4, 0.4, 0.38)),
        ];
        assert_eq!(verdicts(false, &pairs), vec![false, true, true]);
        assert_eq!(
            verdicts(true, &pairs),
            vec![true, true, true],
            "an island standing on a slab is carried from below"
        );
    }
}
