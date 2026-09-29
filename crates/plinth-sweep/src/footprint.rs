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
//! bound, `2 · pitch · tan(slope)`, and never more than the ball's own
//! diameter. Every term is a resolution of the raster itself, not a new
//! physical number.
//!
//! # The one pruning bound (a disclosed Placeholder, it decides no outcome)
//!
//! `SEARCH_RADIUS_BALL_DIAMETERS = 1`: the seat may move at most one ball
//! diameter (`2R`) along the face. It only bounds how far the search looks. A
//! move further than the tip's own ball carries it off the feature the reach
//! criterion placed it for; beyond it the tip is left where it is and warned
//! about rather than moved somewhere nobody asked for support.
//!
//! # Batching
//!
//! Every sample of every tip is graded in ONE ascending pass over the layer
//! stack (each needed layer rasterised once); the search lattice for the tips
//! that fail is graded in one more such pass; every candidate move is
//! re-derived through ONE batched [`plinth_support::tip_contact_discs`] call
//! per round. No per-tip raster query, no per-tip registry read.

use plinth_core::LayerPlan;
use plinth_support::perception::SliceSolid;
use plinth_support::{SweepArm, TipContactDisc};

use crate::grid::Grid;
use crate::raster::{rasterize_into, LayerMasks, RasterScratch};
use crate::{Footprint, SweepStrut};

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
    /// Disc samples graded, lattice points searched, layers rasterised, and
    /// candidate rounds re-derived: the path hits.
    pub disc_samples: usize,
    pub search_samples: usize,
    pub layers_rasterised: usize,
    pub rounds: usize,
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
    let rise = (2.0 * height + 2.0 * pitch * frame.tan_slope).min(2.0 * frame.ball_radius);
    let lamina = |z: f64| -> usize {
        let k = ((z - plate_z) / height).floor();
        if k.is_finite() && k > 0.0 {
            (k as usize).min(layers.saturating_sub(1))
        } else {
            0
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
pub(crate) fn apply(
    grid: &Grid,
    slices: &[Vec<SliceSolid>],
    struts: &mut [SweepStrut],
    arms: &[SweepArm],
    layer_plan: &LayerPlan,
    plate_z: f64,
    height: f64,
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
    let mut failing: Vec<usize> = Vec::new();
    for arm in 0..arms.len() {
        if total[arm] == 0 {
            continue;
        }
        outcome.tips_graded += 1;
        if on[arm] == total[arm] {
            outcome.tips_on_face += 1;
            struts[arm].seat_face.footprint = Footprint::OnFace;
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
        let search_mm = SEARCH_RADIUS_BALL_DIAMETERS * 2.0 * frame.ball_radius;
        let disc_reach = offsets
            .iter()
            .map(|&(u, v)| u.abs().max(v.abs()))
            .max()
            .unwrap_or(0);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a search radius of one ball diameter over the grid pitch is small"
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
        let mut resolved = vec![false; searches.len()];
        for ((slot, _), (moved, disc)) in proposals.iter().zip(moved_arms.iter().zip(&rederived)) {
            let search = &searches[*slot];
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
                accepted[*slot] = Some(geom_kernel::vec3::sub(
                    moved.seat_mm,
                    arms[search.arm].seat_mm,
                ));
                resolved[*slot] = true;
            }
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
                outcome.tips_moved_onto_face += 1;
                outcome.max_move_mm = outcome.max_move_mm.max(length);
                struts[arm].seat_face.shift_mm = shift;
                struts[arm].seat_face.footprint = Footprint::MovedOntoFace;
            }
            None => {
                #[expect(clippy::cast_precision_loss, reason = "sample counts are small")]
                let fraction = on[arm] as f64 / total[arm].max(1) as f64;
                outcome.knife_edges.push((arm, fraction));
                struts[arm].seat_face.footprint = Footprint::KnifeEdge {
                    disc_on_face: fraction,
                };
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
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        New,
        Busy,
        Done,
    }
    let mut out = vec![Followed::default(); struts.len()];
    if !struts
        .iter()
        .any(|strut| strut.seat_face.footprint == Footprint::MovedOntoFace)
    {
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
            let own = (strut.seat_face.footprint == Footprint::MovedOntoFace)
                .then_some(strut.seat_face.shift_mm);
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
