//! How crowded the plate is under a model, and what the engine does about a
//! footprint that is crowded.
//!
//! # Why the bottom of a laid-over cylinder is a solid mass
//!
//! A laid-over cylinder's whole peel load is routed down through its own body
//! to the line where the body is lowest, and the area law then puts
//! `peak / X_ALLOWED` of arm SECTION on that line, whatever the arms'
//! arrangement. **Merging cannot thin it.** A merge re-routes load, it never
//! removes it, and the section a given load needs is `π r² = load / X_ALLOWED`
//! — an invariant of the load, not of the number of members carrying it. Two
//! arms merged into one trunk give one root of the summed area, which is the
//! same area on the same patch of plate.
//!
//! Only two things can thin that patch, and neither is a merge:
//!
//! 1. **Put some of the section on plate OUTSIDE the model's silhouette.** The
//!    same total area, spread over a wider patch, so no two roots overlap.
//! 2. **Route part of the load to the plate by a DIFFERENT path** — arms seated
//!    higher on the body's flanks that lean outward, so the load never reaches
//!    the lowest line at all.
//!
//! The owner's ruling of 2026-09-06: "the only way to resolve the cylinder
//! would be to shed more load via the supports that land higher-up on the
//! model … spread some of the load back upwards to higher-landing supports."
//!
//! # The measurement
//!
//! At the plate layer, rasterise every plate-rooted member's ROOT DISC — the
//! arm's drawn root radius times the registered `FLARE_R_FACTOR`, not the bare
//! arm radius. Until 2026-09-30 that was the radius the EMITTER put there (its
//! flare foot). Since then the emitter stands every root on the owner's
//! ChituBox boss scaled to the root (`plinth_support::RootBoss`: the root
//! times his Heavy profile's boss-to-shaft ratio, 2.2 / 1.5) and the factor is
//! kept, unchanged, as the census's crowding measure;
//! `SweepPolicy::flare_r_factor` says why and what the open question is. Then
//!
//! ```text
//! congestion = Σ (cells one disc covers) / (cells at least one disc covers)
//! ```
//!
//! Exactly `1.0` when no two discs touch, and above `1.0` by however much they
//! stack. Both halves are counted on the SAME raster, so a disc smaller than a
//! cell cannot make the ratio disagree with itself; the analytic `Σ π r²` is
//! published beside it as a cross-check rather than used as the numerator.
//!
//! `root_overlap_pairs` counts the unordered pairs whose discs actually
//! intersect (a geometric test on the centres and radii, not a raster artefact)
//! and `root_stack_max` counts the CELLS that three or more discs cover — the
//! thing the owner saw in ChituBox as "chock-full of overlapping thick
//! supports". The deepest stack any one cell carries is published too.
//!
//! # Where the remedies run, and why not inside the down-sweep
//!
//! For the same reason [`crate::slenderness`] states: congestion is a property
//! of SIZED roots, and the down-sweep is what sizes them; and remedy 1
//! re-chooses a DESCENT, which needs the up-sweep's
//! [`crate::up_sweep::ColumnHistory`]. So this runs in the sweep driver's own
//! iterate, bounded by [`crate::down_sweep::InsertionPolicy::max_rounds`], with
//! the same down-sweep re-run after each round.
//!
//! # What the descent does today, measured rather than assumed
//!
//! A plate cell outside the model's silhouette **is** already a candidate in
//! [`crate::up_sweep`]'s own descent: a cell with no shaft head and no model
//! top yields `Landing::Plate` at `k_bottom = 0`, and nothing there consults a
//! footprint. What forbids the outward move is the COST test, twice over — the
//! sound lower bound `descent_layers · h ≥ straight_cost`, and then
//! `cost_mm ≥ straight_cost`. For an arm that already descends straight to the
//! plate, `straight_cost = layer · h` and every plate candidate crosses exactly
//! the same `layer` layers, so its bare bound EQUALS the straight cost and it
//! is dropped before it is ever priced. **A plate-rooted straight arm can never
//! lean outward under the up-sweep's descent**, not because the silhouette is
//! fenced but because leaning is strictly more expensive.
//!
//! That is what remedy 1 here overrides, deliberately and only where a root
//! disc is actually stacked on another: it buys a longer arm to stop two roots
//! sharing plate. The lean law, the path check (model, shafts, drain keep-out)
//! and the radius clamps are all unchanged — only the cost ORDER is set aside.

use crate::down_sweep::InsertionPolicy;
use crate::drain::DrainKeepOut;
use crate::grid::Grid;
use crate::lean::LeanLaw;
use crate::up_sweep::ColumnHistory;
use crate::{Landing, StrutKind, SweepStrut};

/// The congestion threshold is a DEFINITION, not a constant: `1.0` is the
/// ratio a set of discs has when no two of them overlap. There is nothing to
/// register and nothing to tune.
const NO_OVERLAP: f64 = 1.0;

/// One plate root, as the emitter will actually lay it down.
///
/// `pub(crate)` because the SLENDERNESS lane has to answer the same question
/// this lane answers — would this plate cell's flared disc touch a root that is
/// already there — and the rule has to be the same rule. The fields stay
/// private so the only ways to make one are [`root_discs`] and [`push_disc`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RootDisc {
    id: u32,
    cell: usize,
    x_mm: f64,
    y_mm: f64,
    /// The FLARED radius — what touches the plate.
    radius_mm: f64,
    /// The bare arm section at the root, `π r_root²`. What the area law owns.
    section_mm2: f64,
    under_silhouette: bool,
}

impl RootDisc {
    /// The member whose root this is (the lip-seat rule drops a disc when it
    /// re-lands its member off the plate or onto another cell).
    pub(crate) const fn id(&self) -> u32 {
        self.id
    }

    /// The BARE root radius this disc was flared from, read back from the
    /// section it holds (`section_mm2 = π r_root²` wherever a disc is made or
    /// grown: [`root_discs_drawn`], [`push_disc`], [`grow_disc`]). A test that
    /// measures roots at the DRAWN boss foot rather than at the crowding
    /// factor reads it ([`growth_laps_drawn`]).
    pub(crate) fn bare_radius_mm(&self) -> f64 {
        (self.section_mm2 / core::f64::consts::PI).max(0.0).sqrt()
    }
}

/// One census of a set of root discs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CongestionCensus {
    /// Plate-rooted members counted.
    pub roots: usize,
    /// `Σ` rasterised disc area, mm².
    pub disc_area_mm2: f64,
    /// Area at least one disc covers, mm².
    pub union_area_mm2: f64,
    /// The STACKED area, `(Σ disc cells − union cells) · cell area`, mm². Taken
    /// from the integer cell counts and not as `disc_area − union_area`: two
    /// censuses with the same stacked cells must read the same area to the
    /// bit, whatever else the field holds, because remedy 2 compares them.
    pub stacked_area_mm2: f64,
    /// `Σ π r²` analytically, mm² — published as a cross-check on the raster.
    pub analytic_disc_area_mm2: f64,
    /// `disc_area / union_area`. `1.0` means no two discs overlap; `0.0` means
    /// there was nothing to measure.
    pub ratio: f64,
    /// Unordered pairs of discs that actually intersect.
    pub overlap_pairs: usize,
    /// CELLS three or more discs cover. A count of cells, not a depth — named
    /// the way the 2026-09-06 brief names it, and stated here so it is never
    /// read as a depth.
    pub stack_max: usize,
    /// The deepest stack any one cell carries.
    pub stack_depth_max: usize,
}

impl CongestionCensus {
    /// Whether this footprint is congested at all.
    #[must_use]
    pub fn congested(&self) -> bool {
        self.ratio > NO_OVERLAP
    }
}

/// The census over the whole plate and over the model's own footprint.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CongestionReport {
    pub all: CongestionCensus,
    /// The same measurement over the roots that stand UNDER the model's
    /// silhouette alone — the patch the owner is looking at.
    pub under_silhouette: CongestionCensus,
}

/// What the lane did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CongestionOutcome {
    /// Whether the lane was allowed to run at all.
    pub enabled: bool,
    pub before: CongestionReport,
    /// The census as THIS LANE left it, before the slenderness lane runs. The
    /// difference between this and `after` is what the braces and masts added,
    /// which is a fact about that lane and not about this one.
    pub at_lane_exit: CongestionReport,
    pub after: CongestionReport,
    /// Insertion rounds this lane actually consumed.
    pub rounds_used: usize,
    /// Remedy 1: plate roots re-landed outside the silhouette.
    pub arms_leaned: usize,
    /// Remedy 2: new flank-seated arms inserted.
    pub arms_inserted: usize,
    /// The layer the flank shelf was found at, and the widest cover it carried.
    pub shelf_layer: usize,
    pub shelf_cover_cells: usize,
    /// `Σ π r_root²` of the arm sections whose ROOT moved from inside the
    /// silhouette to outside it. The area law's own quantity, not the flare.
    pub section_moved_outside_silhouette_mm2: f64,
    /// Load, in mm² of reference peel, carried by the flank arms THAT STILL
    /// LAND ON THE PLATE on the finished field, read at their own root.
    ///
    /// **The name says exactly what it counts, and it is not all of them.** An
    /// inserted arm the later down-sweep re-landed on a trunk (`Landing::Strut`)
    /// contributes NOTHING here, because it no longer meets the plate at all;
    /// how many of them there are is [`Self::flank_arms_moved_off_plate`]. The
    /// field was called `flank_load_at_plate_mm2` until 2026-09-06 and silently
    /// dropped those arms, so a re-landed field read as "less load moved" when
    /// the load had in fact moved further.
    pub flank_load_still_at_plate_mm2: f64,
    /// Inserted flank arms whose landing a later pass moved OFF the plate, so
    /// they are outside `flank_load_still_at_plate_mm2` by construction.
    pub flank_arms_moved_off_plate: usize,
    /// Inserted flank roots whose FLARED disc still overlaps another plate root
    /// once the round's down-sweep has sized the field — the error the
    /// pre-sizing radius estimate left behind. Non-zero is a WARNING; the next
    /// round sees these as ordinary stacked plate roots and remedy 1 applies.
    pub flank_roots_overlapping_after_sizing: usize,
    /// The two pre-sizing radius estimates, both computed every run so the
    /// choice between them is auditable from the report rather than from the
    /// source. Bare (un-flared) radii, mm.
    pub flank_radius_mean_placed_mm: f64,
    pub flank_radius_area_law_mm: f64,
    /// The one actually registered, bare, mm.
    pub flank_radius_used_mm: f64,
    /// The STACKED flared plate area remedy 2 was called to relieve, one entry
    /// per round in which it ran: `Σ disc cells − union cells` on the census
    /// taken after remedy 1 moved its roots, in mm². What remedy 2's arm count
    /// is sized from (see [`insert_flank_arms`]).
    pub flank_stacked_area_mm2: Vec<f64>,
    /// Flank arms the stacked area asked for, summed over the rounds:
    /// `⌈stacked / one flank arm's own flared disc⌉` per round.
    pub flank_arms_wanted: usize,
    /// Rounds in which remedy 2 was NOT run because the flank arms of the
    /// round before left the stack no smaller (see [`apply_remedies`]).
    pub flank_rounds_not_relieving: usize,
    /// Searches that found no admissible plate cell outside the silhouette.
    pub outward_cells_refused: usize,
    /// Stacked plate roots the lean left alone because the arm is HEADED: a
    /// head is frozen at placement and nothing re-routes a headed arm.
    pub headed_kept: usize,
    /// Flank seats the path check or the non-overlap rule refused.
    pub flank_seats_refused: usize,
    /// Ids of the arms this lane inserted, so the driver can read their carried
    /// load off the FINISHED field rather than remember it from placement.
    /// Arms the flank-relief rule withdrew are no longer listed.
    pub inserted_ids: Vec<u32>,
    /// Whether the flank-relief rule compared the field with and without the
    /// lane's flank arms (2026-09-29; `withdraw_idle_flank_arms` in the sweep
    /// driver). The path-hit witness: false when no flank arm was left to test.
    pub flank_relief_evaluated: bool,
    /// The comparison it made: `[stacked flared root area mm², overloaded
    /// arm-layers, uncarried load mm²]` with the flank arms, and without them.
    /// The arms stay only if some entry is larger without them.
    pub flank_relief_with: [f64; 3],
    pub flank_relief_without: [f64; 3],
    /// Flank arms withdrawn because the finished field did not need them.
    pub flank_arms_withdrawn: usize,
    /// True when congestion is still above `1.0` after the rounds are spent.
    /// A WARNING on an emitted file, never a refusal.
    pub still_congested: bool,
}

/// Everything the lane reads.
pub struct CongestionContext<'a> {
    pub grid: &'a Grid,
    pub history: &'a ColumnHistory,
    pub keep_out: &'a DrainKeepOut,
    pub lean: &'a LeanLaw,
    pub layer_height_mm: f64,
    pub layers: usize,
    /// `FLARE_R_FACTOR` — the census's root-crowding disc factor
    /// (`SweepPolicy::flare_r_factor`).
    pub flare_factor: f64,
    /// `TIP_RADIUS_DEFAULT_MM` — the smallest printable radius, used as the
    /// placeholder section of an arm the down-sweep has not sized yet.
    pub tip_radius_min_mm: f64,
    /// Nearest-neighbour spacing of the hex covering the up-sweep placed at.
    pub hex_separation_mm: f64,
    /// `X_ALLOWED` — the peel a unit of arm section may legally carry. The area
    /// law's own divisor, read here so a flank arm can be registered at the
    /// radius the LAW asks for instead of at the smallest printable tip.
    pub x_allowed: f64,
    /// Sabotage knob: remedy 1.
    pub outward_lean: bool,
    /// Sabotage knob: remedy 2.
    pub flank_arms: bool,
    pub max_struts: usize,
}

/// Measure the plate's congestion on a SIZED field.
///
/// **Batch-first**: one pass over the struts to gather the discs, one stamping
/// pass over the cells each disc covers, and one sweep-line pass for the
/// overlapping pairs. Nothing here is a per-arm scan of the grid.
#[must_use]
pub fn measure(
    grid: &Grid,
    history: &ColumnHistory,
    struts: &[SweepStrut],
    flare_factor: f64,
) -> CongestionReport {
    let discs = root_discs(grid, history, struts, flare_factor);
    let all = census(grid, &discs);
    let inside: Vec<RootDisc> = discs
        .iter()
        .copied()
        .filter(|disc| disc.under_silhouette)
        .collect();
    CongestionReport {
        all,
        under_silhouette: census(grid, &inside),
    }
}

/// Every plate-rooted member's root disc: its drawn root times the crowding
/// factor (`FLARE_R_FACTOR`).
///
/// A `Brace` is excluded because it gets no root boss and never lands on the
/// plate; an `Arm` and a `Mast` both keep the plate-root chain, so both are
/// counted.
///
/// **Not the emitted foot since 2026-09-30.** The emitter stands every plate
/// root on the owner's ChituBox boss, scaled to the root by his Heavy
/// profile's boss-to-shaft ratio ([`plinth_support::RootBoss`]). This disc
/// keeps the pre-change
/// crowding measure so placement is unchanged by the geometry lane; see
/// `SweepPolicy::flare_r_factor` for the measured alternatives and the open
/// question.
///
/// **The disc is the DRAWN root's** (review-fixes 2026-09-29). Until then it
/// was sized from the ledger's `radius_mm[0]`, while the lowering flares the
/// root of the profile the member is DRAWN at — the ledger raised by the
/// junction clause wherever a thicker branch (a brace) joins it
/// ([`crate::adapter::drawn_profiles`]). A braced plate arm then printed a
/// wider foot than every clearance decision and the census had seen. The root
/// is now [`plinth_support::drawn_root_radius_mm`] of that drawn profile: the
/// lowering's own rule on the lowering's own profile.
pub(crate) fn root_discs(
    grid: &Grid,
    history: &ColumnHistory,
    struts: &[SweepStrut],
    flare_factor: f64,
) -> Vec<RootDisc> {
    // Only the ROOT of each profile is read here, and the load-step taper
    // never reaches a root: a cone starts one layer above a junction, and the
    // junction raises it could feed come from a branch's root or a brace's
    // top, which it never touches (`adapter::drawn_profiles`). So the root is
    // the same with or without it, and no policy is needed to read it.
    let (profiles, _) = crate::adapter::drawn_profiles(struts, crate::adapter::LoadStepTaper::OFF);
    root_discs_drawn(grid, history, struts, &profiles, flare_factor)
}

/// [`root_discs`] over drawn profiles the caller already holds (one per
/// strut, in strut order), so a pass that keeps them live does not re-derive
/// them.
pub(crate) fn root_discs_drawn(
    grid: &Grid,
    history: &ColumnHistory,
    struts: &[SweepStrut],
    profiles: &[Vec<f64>],
    flare_factor: f64,
) -> Vec<RootDisc> {
    let factor = if flare_factor.is_finite() && flare_factor > 0.0 {
        flare_factor
    } else {
        1.0
    };
    let mut discs = Vec::with_capacity(struts.len());
    for (strut, profile) in struts.iter().zip(profiles) {
        if !matches!(strut.landing, Landing::Plate) || strut.kind == StrutKind::Brace {
            continue;
        }
        let root = plinth_support::drawn_root_radius_mm(profile, strut.tip_radius_mm());
        if !(root.is_finite() && root > 0.0) {
            continue;
        }
        let cell = strut.base_cell();
        let Some(under) = history.column_max_model.get(cell) else {
            continue;
        };
        discs.push(RootDisc {
            id: strut.id,
            cell,
            x_mm: grid.center_x_mm(grid.col_of(cell)),
            y_mm: grid.center_y_mm(grid.row_of(cell)),
            radius_mm: root * factor,
            section_mm2: core::f64::consts::PI * root * root,
            under_silhouette: *under != 0,
        });
    }
    discs
}

/// The census of one set of discs.
fn census(grid: &Grid, discs: &[RootDisc]) -> CongestionCensus {
    let mut out = CongestionCensus {
        roots: discs.len(),
        ..CongestionCensus::default()
    };
    if discs.is_empty() {
        return out;
    }
    let pitch = grid.pitch_mm();
    let cell_area = pitch * pitch;
    // One fresh count raster per census. `u16` because a plate cell under a
    // stacked field carries tens of discs, never tens of thousands, and the
    // saturating add says so rather than wrapping.
    let mut counts = vec![0_u16; grid.cells()];
    let mut stamps = 0_usize;
    let mut union_cells = 0_usize;
    let mut three_or_more = 0_usize;
    let mut deepest = 0_usize;
    for disc in discs {
        out.analytic_disc_area_mm2 += core::f64::consts::PI * disc.radius_mm * disc.radius_mm;
        for cell in disc_cells(grid, disc) {
            let count = &mut counts[cell];
            *count = count.saturating_add(1);
            stamps += 1;
            match *count {
                1 => union_cells += 1,
                3 => three_or_more += 1,
                _ => {}
            }
            deepest = deepest.max(*count as usize);
        }
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "cell counts bounded by the grid's own cell count are exact in f64"
    )]
    let (stamped, unioned, stacked) = (
        stamps as f64,
        union_cells as f64,
        (stamps - union_cells) as f64,
    );
    out.disc_area_mm2 = stamped * cell_area;
    out.union_area_mm2 = unioned * cell_area;
    out.stacked_area_mm2 = stacked * cell_area;
    out.ratio = if union_cells == 0 { 0.0 } else { stamped / unioned };
    out.stack_max = three_or_more;
    out.stack_depth_max = deepest;
    out.overlap_pairs = overlap_pairs(discs);
    out
}

/// The cells one disc covers, by cell centre.
fn disc_cells(grid: &Grid, disc: &RootDisc) -> Vec<usize> {
    let pitch = grid.pitch_mm();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a finite non-negative radius divided by the grid pitch, ceiled to a loop bound"
    )]
    let span = (disc.radius_mm / pitch).ceil().max(0.0) as usize;
    // The scan box is derived from the disc's own CENTRE and not from its cell
    // index. The two agree in production — both come from the same landing cell
    // — and deriving it from the coordinates is what keeps the two from being
    // able to disagree at all.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a grid coordinate clamped into the grid's own row and column range"
    )]
    let row0 = grid
        .row_coord(disc.y_mm)
        .floor()
        .clamp(0.0, (grid.rows() - 1) as f64) as usize;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a grid coordinate clamped into the grid's own row and column range"
    )]
    let col0 = grid
        .col_coord(disc.x_mm)
        .floor()
        .clamp(0.0, (grid.cols() - 1) as f64) as usize;
    let cols = grid.cols();
    let r2 = disc.radius_mm * disc.radius_mm;
    let mut cells = Vec::with_capacity((2 * span + 1) * (2 * span + 1));
    for row in row0.saturating_sub(span)..=(row0 + span).min(grid.rows() - 1) {
        let y = grid.center_y_mm(row) - disc.y_mm;
        let base = row * cols;
        for col in col0.saturating_sub(span)..=(col0 + span).min(cols - 1) {
            let x = grid.center_x_mm(col) - disc.x_mm;
            if x * x + y * y <= r2 {
                cells.push(base + col);
            }
        }
    }
    // A disc smaller than one cell still covers the cell it stands in, so the
    // ratio never divides by a union of zero while discs exist.
    if cells.is_empty() {
        cells.push(grid.index(row0, col0));
    }
    cells
}

/// Unordered pairs of discs whose circles intersect.
///
/// A sweep line in `x`: the discs are sorted by centre, and each is compared
/// only with the discs whose centre is within its own diameter plus the widest
/// radius in the set. That is near-linear on a field of any size and needs no
/// second data structure.
fn overlap_pairs(discs: &[RootDisc]) -> usize {
    let mut pairs = 0_usize;
    for_each_overlap(discs, |_, _| pairs += 1);
    pairs
}

/// Every overlapping pair of `discs` by member id, `(lower id, higher id)`,
/// sorted: the same sweep line as [`overlap_pairs`], so the lip-seat rule
/// can tell a pair its re-landings made from one the field already had.
pub(crate) fn overlap_pair_ids(discs: &[RootDisc]) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for_each_overlap(discs, |a, b| {
        let (x, y) = (discs[a].id, discs[b].id);
        out.push((x.min(y), x.max(y)));
    });
    out.sort_unstable();
    out
}

/// The sweep line behind [`overlap_pairs`]: `visit(a, b)` once per
/// intersecting unordered pair of slots.
fn for_each_overlap(discs: &[RootDisc], mut visit: impl FnMut(usize, usize)) {
    let mut order: Vec<usize> = (0..discs.len()).collect();
    order.sort_unstable_by(|a, b| {
        discs[*a]
            .x_mm
            .total_cmp(&discs[*b].x_mm)
            .then_with(|| discs[*a].id.cmp(&discs[*b].id))
    });
    let widest = discs
        .iter()
        .map(|disc| disc.radius_mm)
        .fold(0.0_f64, f64::max);
    for (slot, first) in order.iter().enumerate() {
        let a = discs[*first];
        for second in &order[slot + 1..] {
            let b = discs[*second];
            if b.x_mm - a.x_mm > a.radius_mm + widest {
                break;
            }
            let reach = a.radius_mm + b.radius_mm;
            let dx = b.x_mm - a.x_mm;
            let dy = b.y_mm - a.y_mm;
            if dx * dx + dy * dy < reach * reach {
                visit(*first, *second);
            }
        }
    }
}

/// Run the congestion remedies over a SIZED field, in place, for ONE round.
///
/// Returns how many members it moved or added. The caller re-runs the
/// down-sweep whenever that is non-zero, because an arm with a new descent and
/// an arm with no ledger entry are both un-sized until it does.
pub fn apply_remedies(
    context: &CongestionContext<'_>,
    struts: &mut Vec<SweepStrut>,
    outcome: &mut CongestionOutcome,
) -> usize {
    let grid = context.grid;
    if !context.lean.leans() {
        return 0;
    }
    let mut index = ColumnHistory::new(grid);
    stamp_all(&mut index, struts, context.layer_height_mm);
    let mut placed = root_discs(grid, context.history, struts, context.flare_factor);

    let mut moved = 0_usize;
    if context.outward_lean {
        moved += lean_congested_roots(context, &mut index, struts, &mut placed, outcome);
    }

    // Remedy 2 only when remedy 1 could not clear the stack. The census is
    // re-taken from the moved field rather than predicted from the counter.
    let after_lean = census(grid, &placed);
    //
    // AND ONLY WHILE IT IS RELIEVING. The last entry of
    // `flank_stacked_area_mm2` is the stack the previous round's flank arms
    // were inserted for; this round's census is taken after the down-sweep
    // sized them. If the stack is no smaller, those arms relieved nothing,
    // and more of the same buys members and no relief. Measured on Round 25
    // flipped, 2026-09-29: 0.0575 mm² stacked in each of four rounds, one
    // flank arm per round, and the four brought 10 |X| braces and 7 masts.
    let stacked_mm2 = after_lean.stacked_area_mm2;
    let relieving = outcome
        .flank_stacked_area_mm2
        .last()
        .is_none_or(|previous| stacked_mm2 < *previous);
    if context.flank_arms && after_lean.congested() && !relieving {
        outcome.flank_rounds_not_relieving += 1;
    }
    if context.flank_arms && after_lean.congested() && relieving {
        moved += insert_flank_arms(
            context,
            &mut index,
            struts,
            &mut placed,
            outcome,
            stacked_mm2,
        );
    }
    moved
}

/// Remedy 1: re-land every stacked plate root on clear plate outside the
/// silhouette.
fn lean_congested_roots(
    context: &CongestionContext<'_>,
    index: &mut ColumnHistory,
    struts: &mut [SweepStrut],
    placed: &mut [RootDisc],
    outcome: &mut CongestionOutcome,
) -> usize {
    let congested = stacked_ids(placed);
    // MOVING A PARENT BREAKS ITS CHILDREN. A member that another member lands
    // on mid-shaft, or that a brace seats on, holds that junction at a named
    // cell and layer; re-routing its path would leave the child attached to
    // air. Those members are simply not candidates — an arm with a dependant is
    // load-bearing structure, and the relief is taken from the leaves instead.
    let mut carries_someone = vec![false; struts.len()];
    for strut in struts.iter() {
        if let Landing::Strut { id, .. } = strut.landing {
            if let Some(flag) = carries_someone.get_mut(id as usize) {
                *flag = true;
            }
        }
        if let Some(parent) = strut.seat_on_strut {
            if let Some(flag) = carries_someone.get_mut(parent as usize) {
                *flag = true;
            }
        }
    }
    let mut moved = 0_usize;
    for id in congested {
        // A HEADED arm is never re-routed: its head is frozen at placement.
        if struts.get(id as usize).is_some_and(|strut| strut.head.is_some()) {
            outcome.headed_kept += 1;
            continue;
        }
        if carries_someone.get(id as usize).copied().unwrap_or(true) {
            continue;
        }
        let Some(slot) = placed.iter().position(|disc| disc.id == id) else {
            continue;
        };
        let disc = placed[slot];
        let Some(strut) = struts.get(id as usize) else {
            continue;
        };
        if strut.kind != StrutKind::Arm || !matches!(strut.landing, Landing::Plate) {
            continue;
        }
        if strut.k_bottom != 0 || strut.k_top == 0 {
            continue;
        }
        let Some((cell, path)) = nearest_clear_plate_cell(
            context,
            index,
            strut.cell,
            strut.k_top,
            disc.radius_mm,
            placed,
            Some(slot),
        ) else {
            outcome.outward_cells_refused += 1;
            continue;
        };
        let strut = &mut struts[id as usize];
        // WHERE THIS ARM WAS BEFORE THE LANE MOVED IT, recorded for exactly the
        // reason the slenderness lane records it: every later pass that puts an
        // arm back needs somewhere to put it back TO.
        strut.straight_fallback = Some((strut.landing, strut.k_bottom));
        strut.fallback_path = core::mem::take(&mut strut.path);
        strut.path = path;
        index.stamp_strut(strut, context.layer_height_mm);
        placed[slot].cell = cell;
        placed[slot].x_mm = context.grid.center_x_mm(context.grid.col_of(cell));
        placed[slot].y_mm = context.grid.center_y_mm(context.grid.row_of(cell));
        let was_inside = placed[slot].under_silhouette;
        placed[slot].under_silhouette = false;
        if was_inside {
            outcome.section_moved_outside_silhouette_mm2 += disc.section_mm2;
        }
        outcome.arms_leaned += 1;
        moved += 1;
    }
    moved
}

/// Remedy 2: new arms seated on the body's flanks, leaning outward to clear
/// plate.
///
/// The shelf is DERIVED FROM THE COVER RASTER and never from an angle: the
/// widest layer the model has is where its body is broadest, and the columns
/// whose cured material BEGINS at that layer are its down-facing flank. On a
/// laid-over cylinder that is the two lines along the body at the height of its
/// own axis — the highest seats that still face downward and outward.
///
/// # How many: the relief is sized by the stack it relieves
///
/// A flank arm relieves the stack by carrying load that would otherwise reach
/// the stacked roots. At best it takes its own section's worth, `π r_f² ·
/// X_ALLOWED`, off them, and the area law then lets their section shrink by
/// the same `π r_f²` — by `π (flare · r_f)²` of FLARED plate area, which is
/// exactly the disc the flank arm is registered at. So a stacked flared area
/// `E` needs at least `⌈E / π (flare · r_f)²⌉` flank arms, and every arm
/// beyond that buys relief the plate has no stack for. That is the count.
/// No constant: `E` is this round's census after the lean, and the disc is the
/// placeholder radius this function already registers every flank arm at.
///
/// Measured before this rule (2026-09-29): Round 25 flipped had ONE
/// overlapping root pair (congestion 1.0006, about 0.06 mm² stacked) and got
/// 49 flank arms at shelf layer 144, which brought 83 |X| braces and 58
/// masts; the laid tube at its production pose, 0.0004 stacked after the
/// lean, got 84.
///
/// # Which: nearest the stack
///
/// When the shelf offers more seats than are wanted, the wanted ones are the
/// seats nearest (in plan) to a root that is still stacked, because a flank
/// arm's load leaves the body where it is seated and relieves the roots under
/// that part of the body. When every seat is wanted, the seats keep the
/// shelf's own raster order, so this rule changes nothing on a footprint whose
/// stack outweighs the whole shelf.
fn insert_flank_arms(
    context: &CongestionContext<'_>,
    index: &mut ColumnHistory,
    struts: &mut Vec<SweepStrut>,
    placed: &mut Vec<RootDisc>,
    outcome: &mut CongestionOutcome,
    stacked_mm2: f64,
) -> usize {
    let shelves = underside_layers(context.history, context.layers);
    let bare = flank_root_radius_mm(context, &placed[..], outcome);
    let radius = bare * context.flare_factor;
    let disc_mm2 = core::f64::consts::PI * radius * radius;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a finite non-negative area ratio, ceiled to an arm count"
    )]
    let wanted = if disc_mm2 > 0.0 && stacked_mm2.is_finite() {
        ((stacked_mm2 / disc_mm2).ceil().max(1.0)) as usize
    } else {
        usize::MAX
    };
    outcome.flank_stacked_area_mm2.push(stacked_mm2);
    outcome.flank_arms_wanted = outcome.flank_arms_wanted.saturating_add(wanted);
    let stack: Vec<[f64; 2]> = {
        let ids = stacked_ids(placed);
        placed
            .iter()
            .filter(|disc| ids.binary_search(&disc.id).is_ok())
            .map(|disc| [disc.x_mm, disc.y_mm])
            .collect()
    };
    let mut moved = 0_usize;
    // THE SHELF IS THE HIGHEST LAYER WHOSE COVER ADMITS THE HEX PITCH, and it
    // is found by walking DOWN from the top of the model rather than by naming
    // an angle. A column joins this list at the layer its own cured material
    // BEGINS — where the model's underside is — so the highest such layer on a
    // body lying on its side is the line where the body is widest, its flank.
    // Above that line the silhouette only narrows and no new underside appears,
    // which is why "the widest layer" and "the highest underside" are the same
    // place on the shape this lane exists for, and why nothing here has to know
    // that it is a cylinder.
    for (shelf, cover_cells) in shelves.into_iter().rev() {
        if shelf == 0 {
            break;
        }
        let mut seats = flank_seats(context, struts, shelf);
        if seats.is_empty() {
            continue;
        }
        if wanted < seats.len() {
            nearest_the_stack_first(context.grid, &mut seats, &stack);
        }
        outcome.shelf_layer = shelf;
        outcome.shelf_cover_cells = cover_cells;
        moved = seat_flank_arms(
            context, index, struts, placed, outcome, shelf, radius, bare, &seats, wanted,
        );
        if moved > 0 {
            break;
        }
    }
    moved
}

/// Reorder `seats` so the ones nearest (in plan) to a still-stacked root come
/// first; ties keep the shelf's raster order. One distance per seat and stacked
/// root, once per round — the seats and the stack are both small lists, and
/// nothing here touches the grid.
fn nearest_the_stack_first(grid: &Grid, seats: &mut [usize], stack: &[[f64; 2]]) {
    if stack.is_empty() {
        return;
    }
    let mut keyed: Vec<(f64, usize)> = seats
        .iter()
        .map(|&cell| {
            let x = grid.center_x_mm(grid.col_of(cell));
            let y = grid.center_y_mm(grid.row_of(cell));
            let nearest = stack
                .iter()
                .map(|p| (p[0] - x) * (p[0] - x) + (p[1] - y) * (p[1] - y))
                .fold(f64::INFINITY, f64::min);
            (nearest, cell)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (slot, (_, cell)) in seats.iter_mut().zip(keyed) {
        *slot = cell;
    }
}

/// Which pre-sizing estimate of a flank root's radius is in force.
///
/// **Why an estimate is needed at all.** A flank arm is created here and sized
/// by the down-sweep at the END of the round, so at the moment its plate cell
/// is chosen its radius does not exist yet. Until 2026-09-06 the placeholder
/// was `TIP_RADIUS_DEFAULT_MM`, the SMALLEST radius any arm can have, which
/// makes the clearance test optimistic by construction: every disc is
/// registered too small, the search accepts cells that are too close, and the
/// sizing pass then grows the discs into one another. That is the same defect
/// the mast search had, in a different lane.
///
/// Both candidates below are derived — neither is a number chosen for the
/// purpose — and both are computed every run and published, so which one is in
/// force is a fact readable off the report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "exactly one variant is in force at a time and the other is the alternative it was \
          measured against; deleting the loser would delete the record of the choice, and \
          re-measuring means flipping FLANK_RADIUS_ESTIMATE and nothing else"
)]
enum FlankRadiusEstimate {
    /// **(a) the field's own answer.** The mean sized root radius of the plate
    /// roots this model already has. It asks no physics and needs no divisor;
    /// it says a new root will look like the roots already there.
    MeanPlacedRoot,
    /// **(b) the area law's answer.** One hex cell of the covering is the patch
    /// one seat is responsible for, so its share of the layer's peel event is
    /// its own area; `A = share / X_ALLOWED` and `r = sqrt(A/π)` is then the
    /// section the law asks that arm to have.
    HexCellAreaLaw,
}

/// **The estimate in force, chosen by MEASUREMENT and not by argument.** Both
/// were built, both were run through the production CLI on `make_laid_tube` and
/// the `Cylinder`, and the numbers are in the lane report beside this commit.
const FLANK_RADIUS_ESTIMATE: FlankRadiusEstimate = FlankRadiusEstimate::MeanPlacedRoot;

/// Both pre-sizing estimates of a flank root's BARE radius, with the one in
/// force returned and all three recorded on the outcome.
fn flank_root_radius_mm(
    context: &CongestionContext<'_>,
    placed: &[RootDisc],
    outcome: &mut CongestionOutcome,
) -> f64 {
    let minimum = context.tip_radius_min_mm;
    // (a) the mean SIZED root radius already on this plate. `RootDisc` carries
    // the flared radius, so the flare is divided back out to get the arm's own.
    let flare = if context.flare_factor.is_finite() && context.flare_factor > 0.0 {
        context.flare_factor
    } else {
        1.0
    };
    let mean = if placed.is_empty() {
        minimum
    } else {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a root count bounded by the strut count is exact in f64"
        )]
        let count = placed.len() as f64;
        (placed.iter().map(|disc| disc.radius_mm).sum::<f64>() / flare / count).max(minimum)
    };
    // (b) the area law on one hex cell. A hexagon whose nearest-neighbour
    // spacing is `s` has area `(√3/2)·s²` — the Voronoi cell of the covering
    // the up-sweep actually placed at, so this is the patch one seat owns.
    let separation = context.hex_separation_mm;
    let cell_area = 0.5 * 3.0_f64.sqrt() * separation * separation;
    let area_law = if context.x_allowed.is_finite() && context.x_allowed > 0.0 && cell_area > 0.0 {
        (cell_area / context.x_allowed / core::f64::consts::PI)
            .sqrt()
            .max(minimum)
    } else {
        minimum
    };
    outcome.flank_radius_mean_placed_mm = mean;
    outcome.flank_radius_area_law_mm = area_law;
    let used = match FLANK_RADIUS_ESTIMATE {
        FlankRadiusEstimate::MeanPlacedRoot => mean,
        FlankRadiusEstimate::HexCellAreaLaw => area_law,
    };
    outcome.flank_radius_used_mm = used;
    used
}

/// Put one arm under each accepted flank seat, in the order given, leaning
/// outward to clear plate, until `wanted` are seated. A refused seat passes the
/// want to the next seat in the list.
#[expect(
    clippy::too_many_arguments,
    reason = "the seating step reads the context, the column index, the strut set, the placed \
          discs, the outcome it reports into, its own shelf layer, the placeholder radius, \
          the seats and how many of them are wanted; a struct would move the argument list \
          rather than shorten it"
)]
fn seat_flank_arms(
    context: &CongestionContext<'_>,
    index: &mut ColumnHistory,
    struts: &mut Vec<SweepStrut>,
    placed: &mut Vec<RootDisc>,
    outcome: &mut CongestionOutcome,
    shelf: usize,
    radius: f64,
    bare: f64,
    seats: &[usize],
    wanted: usize,
) -> usize {
    let grid = context.grid;
    let mut moved = 0_usize;
    for seat in seats.iter().copied() {
        if moved >= wanted || struts.len() + 1 > context.max_struts {
            break;
        }
        let Some((cell, path)) =
            nearest_clear_plate_cell(context, index, seat, shelf, radius, placed, None)
        else {
            outcome.flank_seats_refused += 1;
            continue;
        };
        let id = u32::try_from(struts.len()).unwrap_or(u32::MAX);
        struts.push(SweepStrut {
            id,
            cell: seat,
            center_mm: [
                grid.center_x_mm(grid.col_of(seat)),
                grid.center_y_mm(grid.row_of(seat)),
            ],
            kind: StrutKind::Arm,
            min_radius_mm: 0.0,
            thickened_mm: Vec::new(),
            seat_on_strut: None,
            k_top: shelf,
            k_bottom: 0,
            landing: Landing::Plate,
            interior: false,
            radius_mm: Vec::new(),
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(),
            // A flank arm roots no demand cell — it exists to take load off a
            // crowded footprint, not to cover reach — so the removal pass's
            // coverage test would be vacuously true for it. The same flag the
            // load-inserted arms carry keeps it out of that pass entirely.
            inserted_by_load: true,
            path,
            seat_face: crate::SeatFace::default(),
            seat_normal: [0.0; 3],
            straight_fallback: None,
            fallback_path: Vec::new(),
            head: None,
        });
        let arm = &struts[id as usize];
        index.stamp_strut(arm, context.layer_height_mm);
        push_disc(placed, grid, id, cell, radius, bare, false);
        outcome.arms_inserted += 1;
        outcome.inserted_ids.push(id);
        moved += 1;
    }
    moved
}

/// Every layer at which some column's cured material BEGINS, with how many
/// columns begin there, in increasing layer order.
///
/// The model's own underside, read off the cover raster and nothing else — one
/// pass over the span arena, one bucket per layer. No angle, no normal, no
/// second slice of the mesh.
fn underside_layers(history: &ColumnHistory, layers: usize) -> Vec<(usize, usize)> {
    if layers == 0 {
        return Vec::new();
    }
    let mut births = vec![0_usize; layers];
    for &head in &history.model_head {
        if head == u32::MAX {
            continue;
        }
        let mut slot = head;
        let mut lowest = usize::MAX;
        while slot != u32::MAX {
            let span = history.model_spans[slot as usize];
            slot = span.next;
            lowest = lowest.min(span.k_bottom as usize);
        }
        if let Some(count) = births.get_mut(lowest) {
            *count += 1;
        }
    }
    births
        .into_iter()
        .enumerate()
        .filter(|(_, count)| *count > 0)
        .collect()
}

/// The flank seat columns at `shelf`, thinned to the hex pitch.
///
/// A candidate is a column whose cured material BEGINS at `shelf` — so the
/// model's own underside is at that layer and an arm can seat under it. The
/// thinning is against the up-sweep's own hex separation, and against the seats
/// the field already has at that layer, so this never doubles an existing arm.
fn flank_seats(context: &CongestionContext<'_>, struts: &[SweepStrut], shelf: usize) -> Vec<usize> {
    let grid = context.grid;
    let history = context.history;
    let separation = context.hex_separation_mm;
    let mut candidates = Vec::new();
    for (cell, &head) in history.model_head.iter().enumerate() {
        if head == u32::MAX {
            continue;
        }
        let mut slot = head;
        let mut lowest = usize::MAX;
        while slot != u32::MAX {
            let span = history.model_spans[slot as usize];
            slot = span.next;
            lowest = lowest.min(span.k_bottom as usize);
        }
        if lowest == shelf {
            candidates.push(cell);
        }
    }
    // The seats the field already has near this shelf. An arm whose own seat is
    // within one hex pitch of a candidate already holds that patch.
    let taken: Vec<usize> = struts
        .iter()
        .filter(|strut| strut.kind == StrutKind::Arm && strut.k_top.abs_diff(shelf) <= 1)
        .map(|strut| strut.cell)
        .collect();
    let mut accepted: Vec<usize> = Vec::new();
    for cell in candidates {
        if !context.keep_out.admits(cell, shelf) {
            continue;
        }
        if taken
            .iter()
            .any(|other| crate::cell_distance_mm(grid, cell, *other) < separation)
        {
            continue;
        }
        if accepted
            .iter()
            .any(|other| crate::cell_distance_mm(grid, cell, *other) < separation)
        {
            continue;
        }
        accepted.push(cell);
    }
    accepted
}

/// The ids whose root disc overlaps another placed root, lowest first.
fn stacked_ids(discs: &[RootDisc]) -> Vec<u32> {
    let mut order: Vec<usize> = (0..discs.len()).collect();
    order.sort_unstable_by(|a, b| discs[*a].x_mm.total_cmp(&discs[*b].x_mm));
    let widest = discs
        .iter()
        .map(|disc| disc.radius_mm)
        .fold(0.0_f64, f64::max);
    let mut hit = vec![false; discs.len()];
    for (slot, first) in order.iter().enumerate() {
        let a = discs[*first];
        for second in &order[slot + 1..] {
            let b = discs[*second];
            if b.x_mm - a.x_mm > a.radius_mm + widest {
                break;
            }
            let reach = a.radius_mm + b.radius_mm;
            let dx = b.x_mm - a.x_mm;
            let dy = b.y_mm - a.y_mm;
            if dx * dx + dy * dy < reach * reach {
                hit[*first] = true;
                hit[*second] = true;
            }
        }
    }
    let mut ids: Vec<u32> = discs
        .iter()
        .zip(hit.iter())
        .filter_map(|(disc, stacked)| stacked.then_some(disc.id))
        .collect();
    ids.sort_unstable();
    ids
}

/// The nearest plate cell OUTSIDE the model's silhouette whose root disc would
/// touch no placed root, with the descent path that reaches it.
///
/// Ring by ring outward from the seat, so "nearest" is a fact about the search
/// and not about a scan order; the rings stop as soon as no unvisited cell can
/// be closer than the best already found. The search is capped at the furthest
/// the LEAN LAW itself admits over this arm's own height — `k_top · max_step` —
/// which is a derivation from the registered overhang angles, not a bound
/// chosen for it.
fn nearest_clear_plate_cell(
    context: &CongestionContext<'_>,
    index: &ColumnHistory,
    seat_cell: usize,
    k_top: usize,
    radius_mm: f64,
    placed: &[RootDisc],
    skip: Option<usize>,
) -> Option<(usize, Vec<u32>)> {
    let grid = context.grid;
    let pitch = grid.pitch_mm();
    #[expect(
        clippy::cast_precision_loss,
        reason = "a layer count bounded by the plan's own layer count is exact in f64"
    )]
    let cap_mm = k_top as f64 * context.lean.max_step_mm();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a finite non-negative length divided by the grid pitch, floored to a loop bound"
    )]
    let cap_cells = (cap_mm / pitch).floor().max(0.0) as usize;
    if cap_cells == 0 {
        return None;
    }
    let cols = grid.cols();
    let rows = grid.rows();
    let seat_row = grid.row_of(seat_cell);
    let seat_col = grid.col_of(seat_cell);
    // `placed` cannot change during this search, so the plate is indexed once
    // here and every candidate asks the index rather than scanning every root
    // on the plate (see `PlacedIndex`: the same test, disc by disc).
    let plate = PlacedIndex::build(placed, skip);
    let mut best: Option<(f64, usize, Vec<u32>)> = None;
    let mut ring_cells: Vec<(f64, usize)> = Vec::new();
    for ring in 1..=cap_cells {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a ring index bounded by the grid is exact in f64"
        )]
        let ring_floor_mm = ring as f64 * pitch;
        if best
            .as_ref()
            .is_some_and(|(distance, _, _)| ring_floor_mm > *distance)
        {
            break;
        }
        ring_cells.clear();
        let row_lo = seat_row.saturating_sub(ring);
        let row_hi = (seat_row + ring).min(rows - 1);
        let col_lo = seat_col.saturating_sub(ring);
        let col_hi = (seat_col + ring).min(cols - 1);
        // The ring's own cells, in row-major order: a whole row on the ring's
        // top and bottom edges, and only its two edge columns in between.
        // Everything strictly inside was visited by an earlier ring. (This is
        // the set the loop used to find by walking the whole box and skipping
        // the interior, which made the search cubic in the cap.)
        let right_col = seat_col + ring;
        let mut consider = |cell: usize| {
            // OUTSIDE THE SILHOUETTE, and standing clear.
            if context
                .history
                .column_max_model
                .get(cell)
                .copied()
                .unwrap_or(1)
                != 0
            {
                return;
            }
            if index.shaft_head.get(cell).copied().unwrap_or(u32::MAX) != u32::MAX {
                return;
            }
            let lateral = crate::cell_distance_mm(grid, seat_cell, cell);
            if lateral <= 0.0 || lateral > cap_mm {
                return;
            }
            let drop = context.lean.layers_to_reach(lateral);
            if drop == 0 || drop > k_top {
                return;
            }
            if context.lean.descent_cost_mm(k_top, lateral).is_none() {
                return;
            }
            ring_cells.push((lateral, cell));
        };
        for row in row_lo..=row_hi {
            let edge_row = row == seat_row.saturating_sub(ring) || row == seat_row + ring;
            let base = row * cols;
            if edge_row {
                for col in col_lo..=col_hi {
                    consider(base + col);
                }
            } else {
                consider(base + col_lo);
                if right_col <= col_hi {
                    consider(base + right_col);
                }
            }
        }
        ring_cells.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        for (lateral, cell) in ring_cells.iter().copied() {
            if best
                .as_ref()
                .is_some_and(|(distance, _, _)| lateral >= *distance)
            {
                break;
            }
            if !plate.clear(grid, cell, radius_mm) {
                continue;
            }
            let Some(path) = segment_path(context, index, seat_cell, k_top, cell) else {
                continue;
            };
            best = Some((lateral, cell, path));
            break;
        }
    }
    best.map(|(_, cell, path)| (cell, path))
}

/// Whether a disc of `radius_mm` at `cell` would touch no placed root.
///
/// `radius_mm` and every `placed` radius are FLARED radii — what actually
/// touches the plate — so the test is `r_candidate + r_placed <= |Δcentre|`.
/// The slenderness lane's mast search calls THIS function rather than its own
/// copy: two copies of a clearance rule are two rules.
pub(crate) fn clear_of_placed(
    grid: &Grid,
    cell: usize,
    radius_mm: f64,
    placed: &[RootDisc],
    skip: Option<usize>,
) -> bool {
    let x = grid.center_x_mm(grid.col_of(cell));
    let y = grid.center_y_mm(grid.row_of(cell));
    for (slot, disc) in placed.iter().enumerate() {
        if Some(slot) == skip {
            continue;
        }
        if disc_touches(x, y, radius_mm, disc) {
            return false;
        }
    }
    true
}

/// Every placed root the disc of `radius_mm` at `cell` touches, as
/// `(member id, centre distance mm)`, nearest first (ties by the lower id).
/// Empty exactly when [`clear_of_placed`] says clear: the same
/// [`disc_touches`] rule, asked of every disc instead of stopping at the
/// first. The stranded-seat branch rule ([`crate::stranded_branch`]) needs to
/// know WHICH roots a disc laps, because it lands the member on one of them.
pub(crate) fn lapped_roots(
    grid: &Grid,
    cell: usize,
    radius_mm: f64,
    placed: &[RootDisc],
    skip: Option<usize>,
) -> Vec<(u32, f64)> {
    let x = grid.center_x_mm(grid.col_of(cell));
    let y = grid.center_y_mm(grid.row_of(cell));
    let mut out: Vec<(u32, f64)> = placed
        .iter()
        .enumerate()
        .filter(|(slot, disc)| Some(*slot) != skip && disc_touches(x, y, radius_mm, disc))
        .map(|(_, disc)| (disc.id, (disc.x_mm - x).hypot(disc.y_mm - y)))
        .collect();
    out.sort_unstable_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    out
}

/// What growing the disc at `cell` from `old_radius_mm` to `new_radius_mm`
/// does to the discs around it (flared radii, as [`clear_of_placed`]):
/// `(laps_a_new_root, grows_an_existing_lap)`. A disc it laps only after the
/// growth is a NEW lap; a disc it already lapped before is an existing one,
/// which the growth deepens. The same [`disc_touches`] rule, asked twice.
pub(crate) fn growth_laps(
    grid: &Grid,
    cell: usize,
    old_radius_mm: f64,
    new_radius_mm: f64,
    placed: &[RootDisc],
    skip: Option<usize>,
) -> (bool, bool) {
    let x = grid.center_x_mm(grid.col_of(cell));
    let y = grid.center_y_mm(grid.row_of(cell));
    let (mut new_lap, mut existing) = (false, false);
    for (slot, disc) in placed.iter().enumerate() {
        if Some(slot) == skip || !disc_touches(x, y, new_radius_mm, disc) {
            continue;
        }
        if disc_touches(x, y, old_radius_mm, disc) {
            existing = true;
        } else {
            new_lap = true;
        }
    }
    (new_lap, existing)
}

/// [`growth_laps`] with every root measured at the foot the file DRAWS under
/// it, `foot(bare root)`, instead of at the crowding factor the discs in
/// `placed` were stored at: the grown root (`old_bare_mm` to `new_bare_mm`)
/// and every placed root alike. The same [`centres_touch`] rule, asked twice,
/// so the two tests can differ only in the radii they are handed.
///
/// The thicken rung asks this with the lowering's own boss
/// ([`plinth_support::RootBoss::foot_radius_mm`], fix/mast-tie-and-overlap-factor
/// 2026-10-02), so the lap that refuses a thickening is the lap the file
/// would print.
pub(crate) fn growth_laps_drawn(
    grid: &Grid,
    cell: usize,
    old_bare_mm: f64,
    new_bare_mm: f64,
    placed: &[RootDisc],
    skip: Option<usize>,
    foot: impl Fn(f64) -> f64,
) -> (bool, bool) {
    let x = grid.center_x_mm(grid.col_of(cell));
    let y = grid.center_y_mm(grid.row_of(cell));
    let (old_mm, new_mm) = (foot(old_bare_mm), foot(new_bare_mm));
    let (mut new_lap, mut existing) = (false, false);
    for (slot, disc) in placed.iter().enumerate() {
        if Some(slot) == skip {
            continue;
        }
        let other_mm = foot(disc.bare_radius_mm());
        if !centres_touch(x, y, new_mm, disc.x_mm, disc.y_mm, other_mm) {
            continue;
        }
        if centres_touch(x, y, old_mm, disc.x_mm, disc.y_mm, other_mm) {
            existing = true;
        } else {
            new_lap = true;
        }
    }
    (new_lap, existing)
}

/// THE clearance rule, for one placed disc: whether a disc of `radius_mm`
/// centred at `(x, y)` would touch it. [`clear_of_placed`] and
/// [`PlacedIndex::clear`] both ask exactly this, so there is one rule.
fn disc_touches(x: f64, y: f64, radius_mm: f64, disc: &RootDisc) -> bool {
    centres_touch(x, y, radius_mm, disc.x_mm, disc.y_mm, disc.radius_mm)
}

/// Two discs touch when their centres are nearer than the sum of their radii.
/// The one arithmetic behind [`disc_touches`] and [`growth_laps_drawn`].
fn centres_touch(x: f64, y: f64, radius_mm: f64, other_x: f64, other_y: f64, other_mm: f64) -> bool {
    let reach = radius_mm + other_mm;
    let dx = other_x - x;
    let dy = other_y - y;
    dx * dx + dy * dy < reach * reach
}

/// A bucket index over the root discs on the plate, for a search that asks
/// [`clear_of_placed`]'s question of many candidate cells while the plate
/// stands still (`nearest_clear_plate_cell`).
///
/// It returns exactly what [`clear_of_placed`] returns, by asking
/// [`disc_touches`] of every disc that COULD touch and skipping only discs that
/// cannot: a disc whose centre is more than `|r| + max|R|` away along either
/// axis has `|Δ| > |r + R|`, so `Δx² + Δy² < (r + R)²` is false for it in
/// floating point as well (squaring and adding non-negative terms never round
/// below a term), and the query widens its bucket range by one bucket on each
/// side so a rounding in the bucket arithmetic can only ever ADD a disc to the
/// tested set. Any non-finite centre or radius sends every query to the plain
/// scan.
///
/// Why it exists: the congestion lane's outward search tests candidate cells in
/// distance order until one is clear, and on a crowded plate most of the near
/// cells are not; each of those tests scanned every root on the plate, which
/// made that one search the largest cost of a Round 100 run after the mast fix.
pub(crate) struct PlacedIndex<'a> {
    placed: &'a [RootDisc],
    skip: Option<usize>,
    buckets: Option<DiscBuckets>,
}

struct DiscBuckets {
    origin_x_mm: f64,
    origin_y_mm: f64,
    size_mm: f64,
    nx: usize,
    ny: usize,
    /// Bucket `b` holds `slots[starts[b]..starts[b + 1]]`.
    starts: Vec<u32>,
    slots: Vec<u32>,
    max_abs_radius_mm: f64,
}

impl<'a> PlacedIndex<'a> {
    pub(crate) fn build(placed: &'a [RootDisc], skip: Option<usize>) -> Self {
        let scan = Self {
            placed,
            skip,
            buckets: None,
        };
        let live = || {
            placed
                .iter()
                .enumerate()
                .filter(move |(slot, _)| Some(*slot) != skip)
        };
        let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
        let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut max_abs_radius_mm = 0.0_f64;
        let mut count = 0_usize;
        for (_, disc) in live() {
            if !(disc.x_mm.is_finite() && disc.y_mm.is_finite() && disc.radius_mm.is_finite()) {
                return scan;
            }
            min_x = min_x.min(disc.x_mm);
            min_y = min_y.min(disc.y_mm);
            max_x = max_x.max(disc.x_mm);
            max_y = max_y.max(disc.y_mm);
            max_abs_radius_mm = max_abs_radius_mm.max(disc.radius_mm.abs());
            count += 1;
        }
        if count == 0 || u32::try_from(placed.len()).is_err() {
            return scan;
        }
        // About four discs per bucket side-by-side, and never finer than one
        // disc diameter, so a query reads a handful of buckets.
        #[expect(
            clippy::cast_precision_loss,
            reason = "a disc count, far below 2^53, as a bucket-sizing ratio"
        )]
        let spread = ((max_x - min_x) * (max_y - min_y) / count as f64).sqrt();
        let size_mm = (2.0 * max_abs_radius_mm).max(2.0 * spread).max(1.0e-6);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a finite, non-negative extent over a positive bucket size, floored"
        )]
        let axis = |extent: f64| (extent / size_mm).floor() as usize + 1;
        let (nx, ny) = (axis(max_x - min_x), axis(max_y - min_y));
        let Some(buckets) = nx.checked_mul(ny).filter(|&n| n <= 4 * count + 64) else {
            return scan;
        };
        let bucket_of = |disc: &RootDisc| {
            let bx = Self::axis_index(disc.x_mm - min_x, size_mm, nx);
            let by = Self::axis_index(disc.y_mm - min_y, size_mm, ny);
            by * nx + bx
        };
        let mut starts = vec![0_u32; buckets + 1];
        for (_, disc) in live() {
            starts[bucket_of(disc) + 1] += 1;
        }
        for bucket in 0..buckets {
            starts[bucket + 1] += starts[bucket];
        }
        let mut fill = starts.clone();
        let mut slots = vec![0_u32; count];
        for (slot, disc) in live() {
            let bucket = bucket_of(disc);
            slots[fill[bucket] as usize] = u32::try_from(slot).unwrap_or(u32::MAX);
            fill[bucket] += 1;
        }
        Self {
            placed,
            skip,
            buckets: Some(DiscBuckets {
                origin_x_mm: min_x,
                origin_y_mm: min_y,
                size_mm,
                nx,
                ny,
                starts,
                slots,
                max_abs_radius_mm,
            }),
        }
    }

    /// The bucket along one axis holding offset `offset_mm` from the origin,
    /// clamped into `0..n` (clamping only ever merges edge buckets).
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a finite offset over a positive bucket size, floored and clamped to 0..n"
    )]
    fn axis_index(offset_mm: f64, size_mm: f64, n: usize) -> usize {
        let at = (offset_mm / size_mm).floor();
        if at <= 0.0 {
            0
        } else {
            (at as usize).min(n - 1)
        }
    }

    /// Exactly [`clear_of_placed`]`(grid, cell, radius_mm, placed, skip)`.
    pub(crate) fn clear(&self, grid: &Grid, cell: usize, radius_mm: f64) -> bool {
        let bound = radius_mm.abs()
            + self
                .buckets
                .as_ref()
                .map_or(f64::INFINITY, |buckets| buckets.max_abs_radius_mm);
        let Some(buckets) = self.buckets.as_ref().filter(|_| bound.is_finite()) else {
            return clear_of_placed(grid, cell, radius_mm, self.placed, self.skip);
        };
        let x = grid.center_x_mm(grid.col_of(cell));
        let y = grid.center_y_mm(grid.row_of(cell));
        let span = |centre: f64, origin: f64, n: usize| {
            let lo = Self::axis_index(centre - bound - origin, buckets.size_mm, n);
            let hi = Self::axis_index(centre + bound - origin, buckets.size_mm, n);
            (lo.saturating_sub(1), (hi + 1).min(n - 1))
        };
        let (bx_lo, bx_hi) = span(x, buckets.origin_x_mm, buckets.nx);
        let (by_lo, by_hi) = span(y, buckets.origin_y_mm, buckets.ny);
        for by in by_lo..=by_hi {
            for bx in bx_lo..=bx_hi {
                let bucket = by * buckets.nx + bx;
                let from = buckets.starts[bucket] as usize;
                let to = buckets.starts[bucket + 1] as usize;
                for &slot in &buckets.slots[from..to] {
                    if disc_touches(x, y, radius_mm, &self.placed[slot as usize]) {
                        return false;
                    }
                }
            }
        }
        true
    }
}

/// The cells a straight descent from `(seat_cell, k_top)` to `(base_cell, 0)`
/// occupies, or `None` when it cannot have them.
///
/// The same interpolation and the same three refusals — model material, another
/// member, a drain's escape cylinder — the slenderness lane's own `segment_path`
/// uses, and for the same reason: this pass runs AFTER the sweep, so the model
/// question has to be `model_blocks` (is there model HERE) and never `top_model`
/// (is there model at or above here), which by then is true of every column
/// under the piece.
fn segment_path(
    context: &CongestionContext<'_>,
    index: &ColumnHistory,
    seat_cell: usize,
    k_top: usize,
    base_cell: usize,
) -> Option<Vec<u32>> {
    let grid = context.grid;
    let cols = grid.cols();
    if k_top == 0 {
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
    let span_f = k_top as f64;
    let mut path = Vec::with_capacity(k_top);
    for slot in 0..k_top {
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
        if !context.keep_out.admits(cell, slot) {
            return None;
        }
        // The seat layer itself is exempt from the occupancy test: striking one
        // lamina into the surface is what a seat IS.
        if slot > 0 {
            if context.history.model_blocks(cell, slot) {
                return None;
            }
            if index.shaft_blocks(cell, slot, None) {
                return None;
            }
        }
        path.push(u32::try_from(cell).unwrap_or(u32::MAX));
    }
    Some(path)
}

/// Register a root disc a later pass has just placed but the down-sweep has
/// not sized yet, at the FLARED radius the caller estimates for it.
///
/// The slenderness lane uses this to make each mast it drops visible to the
/// clearance test the next mast runs, so two masts can never be placed on top
/// of one another inside a single run of the lane.
pub(crate) fn push_disc(
    placed: &mut Vec<RootDisc>,
    grid: &Grid,
    id: u32,
    cell: usize,
    flared_radius_mm: f64,
    bare_radius_mm: f64,
    under_silhouette: bool,
) {
    placed.push(RootDisc {
        id,
        cell,
        x_mm: grid.center_x_mm(grid.col_of(cell)),
        y_mm: grid.center_y_mm(grid.row_of(cell)),
        radius_mm: flared_radius_mm,
        section_mm2: core::f64::consts::PI * bare_radius_mm * bare_radius_mm,
        under_silhouette,
    });
}

/// Where member `id`'s own root disc sits in `placed`, if it has one.
pub(crate) fn disc_slot(placed: &[RootDisc], id: u32) -> Option<usize> {
    placed.iter().position(|disc| disc.id == id)
}

/// The FLARED radius of the disc in `slot`.
pub(crate) fn disc_radius_mm(placed: &[RootDisc], slot: usize) -> Option<f64> {
    placed.get(slot).map(|disc| disc.radius_mm)
}

/// Grow the disc in `slot` to a new bare root radius, flared by the same
/// factor the emitter uses. The slenderness lane's thicken rung calls this
/// once it has shown the grown disc clears every other root, so the next
/// clearance test sees the root as it will be printed.
pub(crate) fn grow_disc(placed: &mut [RootDisc], slot: usize, flared_radius_mm: f64, bare_radius_mm: f64) {
    if let Some(disc) = placed.get_mut(slot) {
        disc.radius_mm = disc.radius_mm.max(flared_radius_mm);
        disc.section_mm2 = disc
            .section_mm2
            .max(core::f64::consts::PI * bare_radius_mm * bare_radius_mm);
    }
}

/// How many of `ids` have a root disc that overlaps ANY other plate root on the
/// finished, SIZED field.
///
/// **Batch-first**: one pass to gather every disc, one sweep-line pass for the
/// overlapping pairs, one membership test. Nothing here is a per-arm scan.
#[must_use]
pub fn roots_overlapping_after_sizing(
    grid: &Grid,
    history: &ColumnHistory,
    struts: &[SweepStrut],
    flare_factor: f64,
    ids: &[u32],
) -> usize {
    if ids.is_empty() {
        return 0;
    }
    let discs = root_discs(grid, history, struts, flare_factor);
    let stacked = stacked_ids(&discs);
    ids.iter().filter(|id| stacked.contains(id)).count()
}

/// Stamp every finished member's shaft into a fresh column index.
pub(crate) fn stamp_all(index: &mut ColumnHistory, struts: &[SweepStrut], layer_height_mm: f64) {
    for strut in struts {
        if strut.k_bottom == usize::MAX || strut.k_top < strut.k_bottom {
            continue;
        }
        index.stamp_strut(strut, layer_height_mm);
    }
}

/// How many rounds this lane may consume, taken from the insertion policy the
/// down-sweep already runs on rather than from a number of its own.
#[must_use]
pub const fn rounds_allowed(insertion: InsertionPolicy) -> usize {
    insertion.max_rounds
}

#[cfg(test)]
mod tests {
    use super::{census, overlap_pairs, RootDisc, NO_OVERLAP};
    use crate::grid::Grid;

    fn disc(id: u32, x: f64, y: f64, radius: f64) -> RootDisc {
        RootDisc {
            id,
            cell: 0,
            x_mm: x,
            y_mm: y,
            radius_mm: radius,
            section_mm2: core::f64::consts::PI * radius * radius,
            under_silhouette: false,
        }
    }

    fn grid() -> Grid {
        crate::grid::lay_out_at(0.0, 0.0, 0.05, 200, 200)
    }

    /// Discs that do not touch measure exactly the no-overlap ratio, whatever
    /// their radii — the definition, not a tolerance.
    #[test]
    fn disjoint_discs_measure_exactly_one() {
        let grid = grid();
        for radius in [0.08, 0.2, 0.5] {
            let discs = vec![
                disc(0, 2.0, 2.0, radius),
                disc(1, 6.0, 2.0, radius),
                disc(2, 2.0, 6.0, radius),
            ];
            let out = census(&grid, &discs);
            assert!(
                (out.ratio - NO_OVERLAP).abs() < 1.0e-12,
                "three disjoint {radius} mm discs measured {}",
                out.ratio
            );
            assert_eq!(out.overlap_pairs, 0);
            assert_eq!(out.stack_max, 0);
            assert_eq!(out.stack_depth_max, 1);
        }
    }

    /// The same stacked pair reads the SAME stacked area to the bit, however
    /// many disjoint roots the rest of the plate holds. Remedy 2 stops when a
    /// round's stack is not smaller than the last one's, so two equal stacks
    /// must compare equal; `disc_area − union_area` did not (Round 25 flipped,
    /// 2026-09-29: the same 23 stacked cells read a hair smaller in round 2 and
    /// bought a second flank arm that relieved nothing).
    #[test]
    fn the_same_stack_reads_the_same_stacked_area_whatever_else_is_on_the_plate() {
        let grid = grid();
        let pair = [disc(0, 5.0, 5.0, 0.4), disc(1, 5.5, 5.0, 0.4)];
        let alone = census(&grid, &pair);
        assert!(alone.stacked_area_mm2 > 0.0, "the pair does not overlap");
        let mut differs_by_subtraction = 0;
        for extra in 1..=40_u32 {
            let mut discs = pair.to_vec();
            for k in 0..extra {
                let x = 0.5 + f64::from(k % 8) * 1.2;
                let y = 7.0 + f64::from(k / 8) * 0.6;
                discs.push(disc(10 + k, x, y, 0.13 + 0.01 * f64::from(k % 5)));
            }
            let out = census(&grid, &discs);
            assert_eq!(
                out.stacked_area_mm2.to_bits(),
                alone.stacked_area_mm2.to_bits(),
                "{extra} disjoint extra roots changed the stacked area {} -> {}",
                alone.stacked_area_mm2,
                out.stacked_area_mm2
            );
            if (out.disc_area_mm2 - out.union_area_mm2).to_bits()
                != (alone.disc_area_mm2 - alone.union_area_mm2).to_bits()
            {
                differs_by_subtraction += 1;
            }
        }
        // Anti-vacuity: the subtraction really does drift on these fields, so
        // this test would catch it coming back.
        assert!(
            differs_by_subtraction > 0,
            "the subtraction never drifted here, so this test cannot tell the two forms apart"
        );
    }

    /// Two coincident discs put twice the area on one patch, and the ratio says
    /// so. Anti-vacuity for the test above.
    #[test]
    fn coincident_discs_measure_the_number_of_them() {
        let grid = grid();
        let discs = vec![disc(0, 5.0, 5.0, 0.4), disc(1, 5.0, 5.0, 0.4)];
        let out = census(&grid, &discs);
        assert!(
            (out.ratio - 2.0).abs() < 1.0e-12,
            "two coincident discs measured {}",
            out.ratio
        );
        assert_eq!(out.overlap_pairs, 1);
        assert_eq!(out.stack_depth_max, 2);
    }

    /// Three stacked discs make cells that three discs cover; two do not.
    #[test]
    fn the_stack_count_needs_three_discs_over_one_cell() {
        let grid = grid();
        let two = census(&grid, &[disc(0, 5.0, 5.0, 0.4), disc(1, 5.1, 5.0, 0.4)]);
        assert_eq!(two.stack_max, 0);
        assert_eq!(two.stack_depth_max, 2);
        let three = census(
            &grid,
            &[
                disc(0, 5.0, 5.0, 0.4),
                disc(1, 5.1, 5.0, 0.4),
                disc(2, 5.05, 5.05, 0.4),
            ],
        );
        assert!(three.stack_max > 0);
        assert_eq!(three.stack_depth_max, 3);
    }

    /// The pair count is a fact about the circles, not about the raster: discs
    /// that touch at exactly one point do not overlap.
    #[test]
    fn tangent_discs_are_not_an_overlapping_pair() {
        let tangent = vec![disc(0, 1.0, 0.0, 0.5), disc(1, 2.0, 0.0, 0.5)];
        assert_eq!(overlap_pairs(&tangent), 0);
        let overlapping = vec![disc(0, 1.0, 0.0, 0.5), disc(1, 1.9, 0.0, 0.5)];
        assert_eq!(overlap_pairs(&overlapping), 1);
    }

    /// The sweep line's early break must not lose a pair: a long chain of
    /// overlapping discs measures every consecutive pair and nothing else.
    #[test]
    fn the_sweep_line_finds_every_overlapping_pair_in_a_chain() {
        let discs: Vec<RootDisc> = (0..20)
            .map(|index| disc(index, f64::from(index) * 0.9, 0.0, 0.5))
            .collect();
        assert_eq!(overlap_pairs(&discs), 19);
    }
}
