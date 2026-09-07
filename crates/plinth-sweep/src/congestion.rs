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
//! radius the EMITTER actually puts there, which is the arm's root radius times
//! the registered `FLARE_R_FACTOR`, not the bare arm radius. Then
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
#[derive(Clone, Copy, Debug, PartialEq)]
struct RootDisc {
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

/// One census of a set of root discs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CongestionCensus {
    /// Plate-rooted members counted.
    pub roots: usize,
    /// `Σ` rasterised disc area, mm².
    pub disc_area_mm2: f64,
    /// Area at least one disc covers, mm².
    pub union_area_mm2: f64,
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
    /// Load, in mm² of reference peel, that the flank arms are carrying WHERE
    /// THEY MEET THE PLATE, read off the finished field. This is the load that
    /// actually moved onto the new paths.
    pub flank_load_at_plate_mm2: f64,
    /// Searches that found no admissible plate cell outside the silhouette.
    pub outward_cells_refused: usize,
    /// Flank seats the path check or the non-overlap rule refused.
    pub flank_seats_refused: usize,
    /// Ids of the arms this lane inserted, so the driver can read their carried
    /// load off the FINISHED field rather than remember it from placement.
    pub inserted_ids: Vec<u32>,
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
    /// `FLARE_R_FACTOR` — the registered factor the emitter's root flare uses.
    pub flare_factor: f64,
    /// `TIP_RADIUS_DEFAULT_MM` — the smallest printable radius, used as the
    /// placeholder section of an arm the down-sweep has not sized yet.
    pub tip_radius_min_mm: f64,
    /// Nearest-neighbour spacing of the hex covering the up-sweep placed at.
    pub hex_separation_mm: f64,
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

/// Every plate-rooted member's root disc, as the emitter will lay it down.
///
/// A `Brace` is excluded because it gets no root flare and never lands on the
/// plate; an `Arm` and a `Mast` both keep the plate-root chain, so both are
/// counted.
fn root_discs(
    grid: &Grid,
    history: &ColumnHistory,
    struts: &[SweepStrut],
    flare_factor: f64,
) -> Vec<RootDisc> {
    let factor = if flare_factor.is_finite() && flare_factor > 0.0 {
        flare_factor
    } else {
        1.0
    };
    let mut discs = Vec::with_capacity(struts.len());
    for strut in struts {
        if !matches!(strut.landing, Landing::Plate) || strut.kind == StrutKind::Brace {
            continue;
        }
        let Some(&root) = strut.radius_mm.first() else {
            continue;
        };
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
    let (stamped, unioned) = (stamps as f64, union_cells as f64);
    out.disc_area_mm2 = stamped * cell_area;
    out.union_area_mm2 = unioned * cell_area;
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
    let mut pairs = 0_usize;
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
                pairs += 1;
            }
        }
    }
    pairs
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
    stamp_all(&mut index, struts);
    let mut placed = root_discs(grid, context.history, struts, context.flare_factor);

    let mut moved = 0_usize;
    if context.outward_lean {
        moved += lean_congested_roots(context, &mut index, struts, &mut placed, outcome);
    }

    // Remedy 2 only when remedy 1 could not clear the stack. The census is
    // re-taken from the moved field rather than predicted from the counter.
    let after_lean = census(grid, &placed);
    if context.flank_arms && after_lean.congested() {
        moved += insert_flank_arms(context, &mut index, struts, &mut placed, outcome);
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
        index.stamp_shaft(id, 0, strut.k_top, strut.cell, &strut.path);
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
fn insert_flank_arms(
    context: &CongestionContext<'_>,
    index: &mut ColumnHistory,
    struts: &mut Vec<SweepStrut>,
    placed: &mut Vec<RootDisc>,
    outcome: &mut CongestionOutcome,
) -> usize {
    let shelves = underside_layers(context.history, context.layers);
    let radius = context.tip_radius_min_mm * context.flare_factor;
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
        let seats = flank_seats(context, struts, shelf);
        if seats.is_empty() {
            continue;
        }
        outcome.shelf_layer = shelf;
        outcome.shelf_cover_cells = cover_cells;
        moved = seat_flank_arms(context, index, struts, placed, outcome, shelf, radius, &seats);
        if moved > 0 {
            break;
        }
    }
    moved
}

/// Put one arm under each accepted flank seat, leaning outward to clear plate.
#[expect(
    clippy::too_many_arguments,
    reason = "the seating step reads the context, the column index, the strut set, the placed \
          discs, the outcome it reports into, its own shelf layer, the placeholder radius and \
          the seats; a struct would move the argument list rather than shorten it"
)]
fn seat_flank_arms(
    context: &CongestionContext<'_>,
    index: &mut ColumnHistory,
    struts: &mut Vec<SweepStrut>,
    placed: &mut Vec<RootDisc>,
    outcome: &mut CongestionOutcome,
    shelf: usize,
    radius: f64,
    seats: &[usize],
) -> usize {
    let grid = context.grid;
    let mut moved = 0_usize;
    for seat in seats.iter().copied() {
        if struts.len() + 1 > context.max_struts {
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
            seat_solid_layers: 0,
            seat_normal: [0.0; 3],
            straight_fallback: None,
            fallback_path: Vec::new(),
        });
        let arm = &struts[id as usize];
        index.stamp_shaft(id, 0, shelf, seat, &arm.path);
        placed.push(RootDisc {
            id,
            cell,
            x_mm: grid.center_x_mm(grid.col_of(cell)),
            y_mm: grid.center_y_mm(grid.row_of(cell)),
            radius_mm: radius,
            section_mm2: core::f64::consts::PI
                * context.tip_radius_min_mm
                * context.tip_radius_min_mm,
            under_silhouette: false,
        });
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
        for row in row_lo..=row_hi {
            let edge_row = row == seat_row.saturating_sub(ring) || row == seat_row + ring;
            let base = row * cols;
            for col in col_lo..=col_hi {
                // The ring, not the box: everything strictly inside was visited
                // by an earlier ring.
                if !edge_row && col != seat_col.saturating_sub(ring) && col != seat_col + ring {
                    continue;
                }
                let cell = base + col;
                // OUTSIDE THE SILHOUETTE, and standing clear.
                if context.history.column_max_model.get(cell).copied().unwrap_or(1) != 0 {
                    continue;
                }
                if index.shaft_head.get(cell).copied().unwrap_or(u32::MAX) != u32::MAX {
                    continue;
                }
                let lateral = crate::cell_distance_mm(grid, seat_cell, cell);
                if lateral <= 0.0 || lateral > cap_mm {
                    continue;
                }
                let drop = context.lean.layers_to_reach(lateral);
                if drop == 0 || drop > k_top {
                    continue;
                }
                if context.lean.descent_cost_mm(k_top, lateral).is_none() {
                    continue;
                }
                ring_cells.push((lateral, cell));
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
            if !clear_of_placed(grid, cell, radius_mm, placed, skip) {
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
fn clear_of_placed(
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
        let reach = radius_mm + disc.radius_mm;
        let dx = disc.x_mm - x;
        let dy = disc.y_mm - y;
        if dx * dx + dy * dy < reach * reach {
            return false;
        }
    }
    true
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

/// Stamp every finished member's shaft into a fresh column index.
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
