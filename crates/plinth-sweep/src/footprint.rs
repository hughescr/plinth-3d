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
//! grid's own pitch laid in its own plane. A sample is ON the face when the
//! model is carried in its column within the raster's own vertical
//! uncertainty above it: up to two laminae (the slice is taken at the lamina
//! mid-plane, so the first slice showing a face can be one lamina above it,
//! and one more for the carrier mask's inward rounding), plus the rise a
//! sloped face needs for the carrier mask to clear its own two-cell inward
//! bound, `2 · pitch · tan(slope)`. Every term is a resolution of the raster
//! itself, not a new physical number. (A first cut also capped the window at
//! the ball's own diameter; on a steep face under a minimum-radius tip that
//! cap is smaller than the raster can resolve and graded real face as edge,
//! so it was dropped. A column outside the silhouette is empty at every
//! height, so the wider window cannot pass a disc that hangs off an edge.)
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
//! # Batching
//!
//! Every sample of every tip is graded in ONE ascending pass over the layer
//! stack (each needed layer rasterised once); the search lattice for the tips
//! that fail is graded in one more such pass; every candidate move is
//! re-derived through ONE batched [`plinth_support::tip_contact_discs`] call
//! per round, and judged by ONE displacement pass over the field per round
//! (repeated only when a broken rule removes a move). No per-tip raster query,
//! no per-tip registry read.

use plinth_core::LayerPlan;
use plinth_support::perception::SliceSolid;
use plinth_support::{SweepArm, TipContactDisc};

use crate::drain::DrainKeepOut;
use crate::grid::Grid;
use crate::raster::{rasterize_into, LayerMasks, RasterScratch};
use crate::{Footprint, Landing, SweepStrut};

/// How far the seat may move along its face, in ball diameters. A Placeholder
/// that only prunes the search (see the module doc); it never decides whether
/// a disc is on its face.
pub const SEARCH_RADIUS_BALL_DIAMETERS: f64 = 1.0;

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
    /// Proposed moves refused, by the rule the moved geometry (the arm, or a
    /// member joined to it) would have broken, and the member checks run: the
    /// path hits of the move check (review-fixes 2026-09-29).
    pub moves_refused_lean: usize,
    pub moves_refused_slenderness: usize,
    pub moves_refused_drain: usize,
    pub members_checked: usize,
    /// THE LIP-SEAT RULE's remedy 2 (2026-09-29): tips whose disc was on its
    /// face but whose shaft the seat trim cut outside its ball, searched for a
    /// seat further along the same face; those moved there; and proposed
    /// slides refused because the moved shaft would be cut (deeper).
    pub tips_graded_for_section: usize,
    pub tips_moved_for_section: usize,
    pub moves_refused_section: usize,
    /// Disc samples graded, lattice points searched, layers rasterised, and
    /// candidate rounds re-derived: the path hits.
    pub disc_samples: usize,
    pub search_samples: usize,
    pub layers_rasterised: usize,
    pub rounds: usize,
}

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
            let before = crate::adapter::drawn_ends(grid, plate_z, height, strut);
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
}

/// A tip's disc plane and the lattice laid in it.
struct Frame {
    centre: [f64; 3],
    e1: [f64; 3],
    e2: [f64; 3],
    /// `tan` of the face's slope from horizontal, for the vertical window.
    tan_slope: f64,
    ball_radius: f64,
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
        })
    }

    fn point(&self, pitch: f64, u: f64, v: f64) -> [f64; 3] {
        std::array::from_fn(|axis| {
            self.centre[axis] + pitch * (u * self.e1[axis] + v * self.e2[axis])
        })
    }
}

/// A tip's disc frame and the lattice offsets its disc covers.
type DiscLattice = (Frame, Vec<(i32, i32)>);

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
) -> Sample {
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
    }
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

/// Grade every seated tip's disc, move the seats that hang off their face,
/// and warn about the knife edges. `arms` is the field as the adapter will
/// hand it to the lowering, in strut order.
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
    struts: &mut [SweepStrut],
    arms: &[SweepArm],
    layer_plan: &LayerPlan,
    plate_z: f64,
    height: f64,
    limits: &MoveLimits<'_>,
) -> FootprintOutcome {
    let mut outcome = FootprintOutcome::default();
    let Ok(discs) = plinth_support::tip_contact_discs(arms, layer_plan) else {
        return outcome;
    };
    let layers = slices.len();
    let pitch = grid.pitch_mm();

    // ---- stage 1: every graded tip's own disc, one pass.
    let frames: Vec<Option<DiscLattice>> = discs
        .iter()
        .map(|disc| {
            let disc = disc.as_ref().filter(|disc| disc.radius_mm > 0.0)?;
            let frame = Frame::of(disc)?;
            Some((frame, disc_offsets(disc.radius_mm / pitch)))
        })
        .collect();
    let mut samples = Vec::new();
    let mut owner: Vec<usize> = Vec::new();
    for (arm, entry) in frames.iter().enumerate() {
        let Some((frame, offsets)) = entry else {
            continue;
        };
        for &(u, v) in offsets {
            let p = frame.point(pitch, f64::from(u), f64::from(v));
            samples.push(sample_at(grid, plate_z, height, layers, frame, p));
            owner.push(arm);
        }
    }
    outcome.disc_samples = samples.len();
    let hit = carried(grid, slices, &samples, &mut outcome.layers_rasterised);
    let mut on = vec![0_usize; arms.len()];
    let mut total = vec![0_usize; arms.len()];
    for (index, &arm) in owner.iter().enumerate() {
        total[arm] += 1;
        on[arm] += usize::from(hit[index]);
    }
    // THE LIP-SEAT RULE's remedy 2 (2026-09-29): a tip whose disc is on its
    // face but whose shaft the seat trim cuts outside its ball joins the same
    // search, for a seat further along the same face where the moved arm keeps
    // its section; and no slide, for either reason, may cut a shaft deeper
    // than it was cut before. Measured by the lowering's own derivation.
    let bites_before = plinth_support::tip_shaft_seat_bites(arms, layer_plan)
        .unwrap_or_else(|_| vec![None; arms.len()]);
    let bite_of = |bite: Option<&plinth_support::ShaftSeatBite>| bite.map_or(0.0, |b| b.bite_mm);
    let mut section_only = vec![false; arms.len()];
    let mut failing: Vec<usize> = Vec::new();
    for arm in 0..arms.len() {
        if total[arm] == 0 {
            continue;
        }
        outcome.tips_graded += 1;
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
    }
    let mut searches: Vec<Search> = Vec::with_capacity(failing.len());
    let mut lattice: Vec<Sample> = Vec::new();
    for &arm in &failing {
        let Some((frame, offsets)) = &frames[arm] else {
            continue;
        };
        // How far the lean put the disc centre from the seat, in the plane.
        let offset = geom_kernel::vec3::sub(frame.centre, arms[arm].seat_mm);
        let normal = geom_kernel::vec3::cross(frame.e1, frame.e2);
        let in_plane = geom_kernel::vec3::sub(
            offset,
            geom_kernel::vec3::scale(normal, geom_kernel::vec3::dot(offset, normal)),
        );
        let search_mm = SEARCH_RADIUS_BALL_DIAMETERS * 2.0 * frame.ball_radius
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
        let half_width = disc_reach + search_steps;
        let first = lattice.len();
        for u in -half_width..=half_width {
            for v in -half_width..=half_width {
                let p = frame.point(pitch, f64::from(u), f64::from(v));
                lattice.push(sample_at(grid, plate_z, height, layers, frame, p));
            }
        }
        // Candidate moves nearest first; ties broken by the offset itself so
        // the answer never depends on iteration order.
        let mut candidates: Vec<(i32, i32)> = disc_offsets(f64::from(search_steps))
            .into_iter()
            .filter(|&(u, v)| (u, v) != (0, 0))
            .collect();
        candidates.sort_by_key(|&(u, v)| (u * u + v * v, u, v));
        searches.push(Search {
            arm,
            half_width,
            first,
            candidates,
            next: 0,
        });
    }
    outcome.search_samples = lattice.len();
    let lattice_hit = carried(grid, slices, &lattice, &mut outcome.layers_rasterised);
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

    // ---- stage 3: nearest candidate whose rigidly moved disc is all on the
    // face; then stage 4: re-derive the moved tips' discs through the
    // lowering (the axis changes when the seat moves), and keep a move only
    // if the disc the file will really get is on the face too. One batched
    // disc call per round.
    let mut pending: Vec<usize> = (0..searches.len()).collect();
    let mut accepted: Vec<Option<[f64; 3]>> = vec![None; searches.len()];
    // THE MOVE CHECK's state: every member's drawn ends before any move, the
    // own seat moves accepted so far (by member), and which search slot, if
    // any, each member's own move belongs to.
    let before_ends: Vec<([f64; 3], [f64; 3])> = struts
        .iter()
        .map(|strut| crate::adapter::drawn_ends(grid, plate_z, height, strut))
        .collect();
    let mut own_accepted: Vec<Option<[f64; 3]>> = vec![None; struts.len()];
    let mut slot_of_member = vec![usize::MAX; struts.len()];
    for (slot, search) in searches.iter().enumerate() {
        if let Some(entry) = slot_of_member.get_mut(search.arm) {
            *entry = slot;
        }
    }
    let mut refused_by_limits = vec![false; searches.len()];
    let search_arms: Vec<usize> = searches.iter().map(|search| search.arm).collect();
    while !pending.is_empty() {
        outcome.rounds += 1;
        let mut proposals: Vec<(usize, (i32, i32))> = Vec::new();
        for &slot in &pending {
            let search = &mut searches[slot];
            let Some((_, offsets)) = &frames[search.arm] else {
                continue;
            };
            while search.next < search.candidates.len() {
                let (i, j) = search.candidates[search.next];
                search.next += 1;
                if offsets
                    .iter()
                    .all(|&(u, v)| on_lattice(search, u + i, v + j))
                {
                    proposals.push((slot, (i, j)));
                    break;
                }
            }
        }
        if proposals.is_empty() {
            break;
        }
        let moved_arms: Vec<SweepArm> = proposals
            .iter()
            .map(|&(slot, (i, j))| {
                let arm = &arms[searches[slot].arm];
                // The move is a lattice step IN the disc's plane, so the seat
                // slides along the face and keeps its height above it.
                let shift = frames[searches[slot].arm]
                    .as_ref()
                    .map_or([0.0; 3], |(frame, _)| {
                        geom_kernel::vec3::sub(
                            frame.point(pitch, f64::from(i), f64::from(j)),
                            frame.centre,
                        )
                    });
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
        let rederived = plinth_support::tip_contact_discs(&moved_arms, layer_plan)
            .unwrap_or_else(|_| vec![None; moved_arms.len()]);
        // The same moves drawn with their own profiles, for the section test.
        let profiled: Vec<SweepArm> = proposals
            .iter()
            .zip(&moved_arms)
            .map(|(&(slot, _), moved)| SweepArm {
                radius_profile_mm: arms[searches[slot].arm].radius_profile_mm.clone(),
                ..moved.clone()
            })
            .collect();
        let moved_bites = plinth_support::tip_shaft_seat_bites(&profiled, layer_plan)
            .unwrap_or_else(|_| vec![None; profiled.len()]);
        let mut resolved = vec![false; searches.len()];
        let mut on_face: Vec<(usize, [f64; 3])> = Vec::with_capacity(proposals.len());
        for (((slot, _), (moved, disc)), moved_bite) in proposals
            .iter()
            .zip(moved_arms.iter().zip(&rederived))
            .zip(&moved_bites)
        {
            let search = &searches[*slot];
            let before = bite_of(bites_before[search.arm].as_ref());
            let after = bite_of(moved_bite.as_ref());
            // A tip searched for its section alone moves only onto a seat
            // where the shaft keeps it; any other slide may not cut deeper.
            let section_holds = if section_only[search.arm] {
                !crate::lip_seat::cuts(moved_bite.as_ref())
            } else {
                !crate::lip_seat::cuts_deeper(before, after)
            };
            if !section_holds {
                outcome.moves_refused_section += 1;
                refused_by_limits[*slot] = true;
                continue;
            }
            let Some((frame, _)) = &frames[search.arm] else {
                continue;
            };
            let Some(disc) = disc.filter(|disc| disc.radius_mm > 0.0) else {
                continue;
            };
            // The real disc, sampled on its own lattice and read back on the
            // search lattice at the nearest point.
            let real_ok = disc_offsets(disc.radius_mm / pitch).iter().all(|&(u, v)| {
                let p = geom_kernel::vec3::add(
                    disc.centre_mm,
                    geom_kernel::vec3::add(
                        geom_kernel::vec3::scale(frame.e1, pitch * f64::from(u)),
                        geom_kernel::vec3::scale(frame.e2, pitch * f64::from(v)),
                    ),
                );
                let d = geom_kernel::vec3::sub(p, frame.centre);
                let lu = geom_kernel::vec3::dot(d, frame.e1) / pitch;
                let lv = geom_kernel::vec3::dot(d, frame.e2) / pitch;
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "a lattice coordinate within the search window is small"
                )]
                let (lu, lv) = (lu.round() as i32, lv.round() as i32);
                on_lattice(search, lu, lv)
            });
            if real_ok {
                on_face.push((
                    *slot,
                    geom_kernel::vec3::sub(moved.seat_mm, arms[search.arm].seat_mm),
                ));
            }
        }
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
            own_accepted[searches[slot].arm] = Some(shift);
            resolved[slot] = true;
        }
        pending.retain(|slot| {
            !resolved[*slot] && searches[*slot].next < searches[*slot].candidates.len()
        });
    }

    // ---- publish: moves, their consequence for branches, and knife edges.
    for (slot, search) in searches.iter().enumerate() {
        let arm = search.arm;
        match accepted[slot] {
            Some(shift) => {
                let length = geom_kernel::vec3::norm(shift);
                if section_only[arm] {
                    outcome.tips_moved_for_section += 1;
                } else {
                    outcome.tips_moved_onto_face += 1;
                }
                outcome.max_move_mm = outcome.max_move_mm.max(length);
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

/// Everything the batched move check reads that does not change within a
/// round.
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
        let followed = followed_shifts_with(struts, &own);
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
    followed_shifts_with(struts, &own)
}

/// [`followed_shifts`] for a proposed set of own seat moves (`own[i]` is
/// member `i`'s own seat move, `None` where it has none), so the footprint
/// rule can judge a set of moves before it writes any of them.
fn followed_shifts_with(struts: &[SweepStrut], own_moves: &[Option<[f64; 3]>]) -> Vec<Followed> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        New,
        Busy,
        Done,
    }
    let mut out = vec![Followed::default(); struts.len()];
    if own_moves.iter().all(Option::is_none) {
        return out;
    }
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
    let along = |out: &[Followed], host: usize, layer: usize| -> Option<[f64; 3]> {
        let h = &struts[host];
        let Followed { seat_mm, base_mm } = out[host];
        if seat_mm.is_none() && base_mm.is_none() {
            return None;
        }
        let (seat, base) = (seat_mm.unwrap_or([0.0; 3]), base_mm.unwrap_or([0.0; 3]));
        if h.k_top <= h.k_bottom || h.k_bottom == usize::MAX {
            return Some(seat);
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "layer indices bounded by the plan's layer count are exact in f64"
        )]
        let t = ((layer.saturating_sub(h.k_bottom)) as f64 / (h.k_top - h.k_bottom) as f64)
            .clamp(0.0, 1.0);
        Some(std::array::from_fn(|axis| {
            t * seat[axis] + (1.0 - t) * base[axis]
        }))
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
            thickened_mm: Vec::new(),
            seat_on_strut: None,
            k_top: 100,
            k_bottom: 0,
            landing: Landing::Plate,
            interior: false,
            radius_mm: vec![0.1; 100],
            carried_mm2: vec![0.0; 100],
            covered_demand: Vec::new(),
            inserted_by_load: false,
            path: Vec::new(),
            straight_fallback: None,
            fallback_path: Vec::new(),
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
