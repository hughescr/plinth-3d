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
//! and rim, in one more batched raster pass per round. A sample is ON the face when the
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
    /// Proposed moves refused, by the rule the moved geometry (the arm, or a
    /// member joined to it) would have broken, and the member checks run: the
    /// path hits of the move check (review-fixes 2026-09-29).
    pub moves_refused_lean: usize,
    pub moves_refused_slenderness: usize,
    pub moves_refused_drain: usize,
    pub members_checked: usize,
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
        .map(|(disc, strut)| {
            if strut.kind == crate::StrutKind::Tie {
                return None;
            }
            let disc = disc.as_ref().filter(|disc| disc.radius_mm > 0.0)?;
            let frame = Frame::of(disc)?;
            Some((
                frame,
                disc_offsets(disc.radius_mm / pitch),
                rim_points(disc.radius_mm / pitch),
            ))
        })
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
            samples.push(sample_at(grid, plate_z, height, layers, frame, p));
            owner.push((arm, on_rim));
        }
    }
    outcome.disc_samples = samples.len();
    let hit = carried(grid, slices, &samples, &mut outcome.layers_rasterised);
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
        let Some((frame, offsets, _)) = &frames[arm] else {
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
        // One more pitch than the lattice offsets reach: the rim, read back at
        // its nearest lattice point, lies up to a pitch beyond them.
        let half_width = disc_reach + 1 + search_steps;
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
            let Some((_, offsets, rim)) = &frames[search.arm] else {
                continue;
            };
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
            while search.next < search.candidates.len() {
                let (i, j) = search.candidates[search.next];
                search.next += 1;
                if offsets
                    .iter()
                    .chain(&rim_cells)
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
                let shift =
                    frames[searches[slot].arm]
                        .as_ref()
                        .map_or([0.0; 3], |(frame, _, _)| {
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
        for (((slot, _), (moved, disc)), moved_bite) in proposals
            .iter()
            .zip(moved_arms.iter().zip(&rederived))
            .zip(&moved_bites)
        {
            let search = &searches[*slot];
            // A tip searched for its section alone moves only onto a seat
            // where the shaft keeps it. Its refusal is its own count, never a
            // "refused by the limits" (that names the lean law, an unbraced
            // run or the drain keep-out, and this tip keeps its seat anyway).
            if section_only[search.arm] && crate::lip_seat::cuts(moved_bite.as_ref()) {
                outcome.moves_refused_section += 1;
                continue;
            }
            let Some(disc) = disc.filter(|disc| disc.radius_mm > 0.0) else {
                continue;
            };
            let Some(real) = Frame::of(&disc) else {
                continue;
            };
            let first = verify_samples.len();
            let lattice = disc_offsets(disc.radius_mm / pitch)
                .into_iter()
                .map(|(u, v)| (f64::from(u), f64::from(v)));
            for (u, v) in lattice.chain(rim_points(disc.radius_mm / pitch)) {
                let p = real.point(pitch, u, v);
                verify_samples.push(sample_at(grid, plate_z, height, layers, &real, p));
            }
            verify_span.push((first, verify_samples.len()));
            verify.push((
                *slot,
                geom_kernel::vec3::sub(moved.seat_mm, arms[search.arm].seat_mm),
            ));
        }
        outcome.verify_samples += verify_samples.len();
        let verify_hit = carried(
            grid,
            slices,
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
        arms,
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

    // ---- publish: moves, their consequence for branches, and knife edges.
    for (slot, search) in searches.iter().enumerate() {
        let arm = search.arm;
        match accepted[slot] {
            Some(shift) => {
                let length = geom_kernel::vec3::norm(shift);
                if section_only[arm] {
                    outcome.tips_moved_for_section += 1;
                    outcome.moved_for_section.push(struts[arm].id);
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
