//! **R9 + D6: the construction-stage shake check** (Craig ruling 2026-10-03,
//! interim pending the slenderness coupon; `field-2026-09-28/
//! bracing-mechanics.md`, revision 2, §2 "Shaking" and rule R9). A WARNING
//! only: it reads the finished field and never changes it.
//!
//! # What it checks
//!
//! A diagonal grows off its lower post like a diving board until it meets its
//! twin. Each new layer of it peels off the film, and that pull acts at the
//! diagonal's growing tip, a lever away from its root: a torque on the post,
//! which bends the post and everything under it that the frame does not
//! hold. The S law sees only drag; this is the load it does not see.
//!
//! At every STAGE — the last layer printed before a cross's two diagonals
//! meet, where their levers are longest (or, for a diagonal that never meets
//! its twin, the last layer before its own top) — the field as printed so far
//! is built as one beam frame ([`crate::frame`]), exactly as the study's
//! `bracing-mechanics/tools/stage.ts` builds it from the files:
//!
//! * every member cut at that layer, drawn at its own drawn radius;
//! * every finished diagonal, through its meeting point, is a frame member,
//!   so every lower band is compliant and nothing is a clamp;
//! * every diagonal still growing is a cantilever cut at that layer, with its
//!   newest layer's peel `F = p_b π r_b² / cos φ` pulling up at its tip, all
//!   of them at once;
//! * a branch stands on its trunk; a plate post is fixed at the top of its
//!   root flare (the boss the file draws), a member on the model at its foot;
//! * the tops still printing are free (the model's seats are not credited,
//!   which is softer than the real field).
//!
//! The answer is the sideways move of every printing top, in pixels:
//! `δ / px = u · 2π / (q S⁴)` with `u` the frame's move at `E = 1`, because
//! the slenderness law's own `S` is `L / r` at one pixel under drag `q`
//! (`E = q S⁴ / (2π px)`; the pixel cancels).
//!
//! # The assumed pressure, stated in every warning
//!
//! How hard a brace layer peels is NOT KNOWN. [`BRACE_PEEL_KPA`] is the
//! measured value for a 0.05 mm slender wall; a brace layer is an ellipse of
//! 0.015-1.07 mm², between that wall and the flat discs of 67-101 kPa, and
//! nobody has measured it (coupon F2 will). Every figure here is proportional
//! to `p_b / q`, so the warning states `p_b` and the pressure at which the
//! worst top reaches one pixel.
//!
//! # Three phases, and why the answer cannot depend on the thread count
//!
//! 1. [`ShakeInput::capture`] reads the field once, into flat arrays: nothing
//!    after it touches a [`SweepStrut`]. (A seal that reuses a winner plan
//!    stores the input and runs phase 3 later.)
//! 2. Each stage's frame is built from the input alone, in parallel
//!    (`ShakeInput::build_stage` is a pure function of the input and the stage
//!    number), and collected by stage position.
//! 3. The frames are grouped, in stage order, into batches whose skyline
//!    memory fits [`SOLVE_BUDGET_BYTES`]; each batch is solved in parallel
//!    and collected by position, and a SERIAL fold in ascending stage order
//!    reads the results. No float is summed across stages or threads, and a
//!    batch boundary depends only on integer sizes and a constant, so the
//!    answer is the same at every thread count. Each stage is still one
//!    serial factorisation.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use geom_kernel::vec3;
use plinth_support::SweepRootFlare;
use rayon::prelude::*;

use crate::frame::Frame;
use crate::grid::Grid;
use crate::slenderness::{drawn_at, free_shaft_floor, live, SlendernessLaw};
use crate::{Landing, StrutKind, SweepStrut};

/// `p_b`, the brace-layer peel pressure the check ASSUMES, kPa: the measured
/// effective peel pressure of a 0.05 mm slender wall on this printer, "at
/// most 1.24 kPa (plate 1, saturated bound)"
/// (`docs/plans/load-model-physics-first.md` §1.1 (a)). UNMEASURED FOR A
/// BRACE SECTION: a brace layer is 3-17 times wider than that wall, and may
/// peel harder (`bracing-mechanics.md` §0). Interim value by Craig's ruling
/// of 2026-10-03 (D6: "p_b = 1.24 kPa wall value, flagged as unmeasured"),
/// until coupon F2 replaces it. It decides nothing: it scales a warning.
pub const BRACE_PEEL_KPA: f64 = 1.24;

/// `q`, the S-law's lateral drag per unit length, N/mm: the order-of-
/// magnitude input the registered `SLENDERNESS_LIMIT_RATIO` is derived from
/// ("q ~ 4 pi mu v ... so q ~ 5e-3 N/m", that entry's note), the same `q`
/// the study's stage check uses. Only the ratio `p_b / q` enters a figure
/// here. A placeholder, as `S` is.
pub const DRAG_N_PER_MM: f64 = 5.0e-6;

/// Float noise on a height divided by the layer height (14.0 / 0.05 reads
/// 280.00000000000006). Numerical, not physical.
const LAYER_NOISE: f64 = 1.0e-9;

/// The most skyline memory the stages of one batch may hold at once, bytes.
/// A RESOURCE budget (the machine has 128 GB and a run is held under 100),
/// not physics: it decides how many stages are factored side by side and
/// nothing else. A stage whose own skyline is larger is solved alone, never
/// refused.
pub(crate) const SOLVE_BUDGET_BYTES: usize = 8 << 30;

/// What the check found on one field.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StageShake {
    /// `p_b` assumed, kPa (`0` where the check did not run).
    pub pressure_kpa: f64,
    /// Diagonals on the field, and those whose twin's axis never passes
    /// within the larger of the two radii (they grow to their full length).
    pub diagonals: usize,
    pub unjoined: usize,
    /// Stages solved, stages where some printing top moved over one pixel,
    /// and stages with a loaded piece the solver could not carry (a piece
    /// with nothing fixed): counted, never guessed.
    pub stages: usize,
    pub stages_over_pixel: usize,
    pub unsolved_stages: usize,
    /// Every printing top that moves over one pixel at some stage, at its
    /// worst stage, worst first.
    pub tops: Vec<ShakenTop>,
    /// The worst move of any printing top, in pixels, where, and the
    /// pressure at which it would be exactly one pixel (`p_b / worst`).
    pub worst_px: f64,
    pub worst_member: Option<u32>,
    pub worst_layer: usize,
    pub pressure_for_one_pixel_kpa: f64,
    /// Work counters, read by no printer: stage frames put to the solver,
    /// the largest skyline of any one of them (`f64`s), and the batches they
    /// were solved in. All three depend on the field and the budget only.
    pub frames_solved: usize,
    pub skyline_entries_max: usize,
    pub batches: usize,
}

/// One printing top that moves over one pixel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShakenTop {
    pub member: u32,
    pub layer: usize,
    pub px: f64,
}

/// One diagonal as the check sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Diagonal {
    /// The post it grows off (its landing) and the post it seats on.
    root_post: usize,
    seat_post: usize,
    bottom: usize,
    top: usize,
    radius_mm: f64,
    /// Where it meets its twin, if it does.
    meet: Option<[f64; 3]>,
}

/// Everything the check reads off a finished field, as flat arrays (phase 1).
/// Cheap to keep (a few MB on a big field): a seal that reuses an
/// orientation plan stores it and runs [`ShakeInput::check`] on the winner.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShakeInput {
    /// `false` where the check does not apply (no binding law, a bad layer
    /// height or no pressure): `check` then returns the empty result.
    active: bool,
    pressure_kpa: f64,
    layer_height_mm: f64,
    /// `2π / (q S⁴)`: pixels per unit of move at `E = 1`.
    scale: f64,
    /// `p_b` in N/mm².
    pressure: f64,
    /// Per member: its line (the post, from the joint for a headed arm), its
    /// kind, whether it is live, its layers, and what it lands on.
    ends: Vec<([f64; 3], [f64; 3])>,
    is_brace: Vec<bool>,
    is_live: Vec<bool>,
    k_bottom: Vec<usize>,
    k_top: Vec<usize>,
    /// `Some((trunk, layer))` for a member that lands on another.
    parent: Vec<Option<(usize, usize)>>,
    min_radius_mm: Vec<f64>,
    /// The drawn radius of each live non-brace member at each layer of
    /// `k_bottom..=k_top`, flat; member `m` owns
    /// `radii[radius_start[m]..radius_start[m + 1]]`. (Braces are drawn at
    /// their own `radius_mm` and never asked for a layer radius.)
    radii: Vec<f64>,
    radius_start: Vec<usize>,
    /// Every plate post's fixed height: the top of its root flare.
    fixed_layer: Vec<usize>,
    /// Whether a member stands on something fixed.
    base_fixed: Vec<bool>,
    /// The live non-brace members landing on each member.
    branches_of: Vec<Vec<usize>>,
    diagonals: Vec<Diagonal>,
    /// For each member, the ascending indices of the diagonals that have it
    /// as root or seat (each diagonal once).
    diagonals_at: Vec<Vec<usize>>,
    /// The stages, ascending.
    stages: Vec<usize>,
}

/// One stage's frame, built and ready to solve.
struct StageFrame {
    stage: usize,
    frame: Frame,
    loads: Vec<(usize, [f64; 3])>,
    /// `(member, its top node)` for every member still printing.
    tops: Vec<(usize, usize)>,
    /// The skyline of this frame's solve, `f64`s.
    skyline: usize,
}

/// One stage's answer.
struct StageResult {
    stage: usize,
    unsolved: bool,
    /// The sideways move in pixels of every solved printing top, in the order
    /// of the stage's members.
    tops: Vec<(u32, f64)>,
}

impl ShakeInput {
    /// Read a finished field once (phase 1). `profiles` are the drawn
    /// profiles (`adapter::drawn_profiles`).
    #[must_use]
    pub fn capture(
        grid: &Grid,
        struts: &[SweepStrut],
        profiles: &[Vec<f64>],
        layer_height_mm: f64,
        law: &SlendernessLaw,
        pressure_kpa: f64,
    ) -> Self {
        let h = layer_height_mm;
        let mut input = Self {
            pressure_kpa,
            layer_height_mm: h,
            ..Self::default()
        };
        if !(law.binds() && h.is_finite() && h > 0.0 && pressure_kpa.is_finite() && pressure_kpa > 0.0)
        {
            return input;
        }
        input.active = true;
        let flare = SweepRootFlare::registered(h).ok();
        input.ends = struts
            .iter()
            // A member's LINE: its post, from the joint `K` for a headed arm.
            .map(|strut| crate::adapter::drawn_post_ends(grid, 0.0, h, strut))
            .collect();
        input.is_brace = struts.iter().map(|s| s.kind == StrutKind::Brace).collect();
        input.is_live = struts.iter().map(live).collect();
        input.k_bottom = struts.iter().map(|s| s.k_bottom).collect();
        input.k_top = struts.iter().map(|s| s.k_top).collect();
        input.min_radius_mm = struts.iter().map(|s| s.min_radius_mm).collect();
        input.parent = struts
            .iter()
            .map(|strut| match strut.landing {
                Landing::Strut { id, layer } => Some((id as usize, layer)),
                Landing::Plate | Landing::Model { .. } => None,
            })
            .collect();
        input.radius_start.push(0);
        for (member, strut) in struts.iter().enumerate() {
            if input.is_live[member] && !input.is_brace[member] {
                for layer in strut.k_bottom..=strut.k_top {
                    input.radii.push(
                        profiles
                            .get(member)
                            .and_then(|profile| drawn_at(strut, profile, layer))
                            .unwrap_or(strut.min_radius_mm),
                    );
                }
            }
            input.radius_start.push(input.radii.len());
        }
        // The diagonals, their twins and meeting points.
        for strut in struts {
            if strut.kind != StrutKind::Brace || !live(strut) {
                continue;
            }
            let (Some(seat_post), Landing::Strut { id: root_post, .. }) =
                (strut.seat_on_strut, strut.landing)
            else {
                continue;
            };
            let radius_mm = strut
                .radius_mm
                .iter()
                .copied()
                .fold(strut.min_radius_mm, f64::max);
            input.diagonals.push(Diagonal {
                root_post: root_post as usize,
                seat_post: seat_post as usize,
                bottom: strut.k_bottom,
                top: strut.k_top,
                radius_mm,
                meet: None,
            });
        }
        let mut by_band: BTreeMap<(usize, usize, usize, usize), Vec<usize>> = BTreeMap::new();
        for (index, d) in input.diagonals.iter().enumerate() {
            let pair = (d.root_post.min(d.seat_post), d.root_post.max(d.seat_post));
            by_band
                .entry((pair.0, pair.1, d.bottom, d.top))
                .or_default()
                .push(index);
        }
        for list in by_band.values() {
            let [one, two] = list[..] else {
                continue;
            };
            let (a0, a1) = input.axis(&input.diagonals[one]);
            let (b0, b1) = input.axis(&input.diagonals[two]);
            let (distance, p, q) = geom_kernel::segment_segment_distance_min(a0, a1, b0, b1);
            if distance <= input.diagonals[one].radius_mm.max(input.diagonals[two].radius_mm) {
                let meet = vec3::scale(vec3::add(p, q), 0.5);
                input.diagonals[one].meet = Some(meet);
                input.diagonals[two].meet = Some(meet);
            }
        }
        // The stages: the last layer below each meeting point (or each unjoined
        // diagonal's own top).
        let mut stages: BTreeSet<usize> = BTreeSet::new();
        for d in &input.diagonals {
            let closes = input.closes_at(d);
            // The first layer plane at or above the meeting point (a meeting
            // point ON a plane, up to float noise, is that plane).
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a finite non-negative height over a positive layer height, ceiled"
            )]
            let above = (closes / h - LAYER_NOISE).ceil().max(0.0) as usize;
            if let Some(stage) = above.checked_sub(1).filter(|&stage| stage > d.bottom) {
                stages.insert(stage);
            }
        }
        input.stages = stages.into_iter().collect();
        input.fixed_layer = struts
            .iter()
            .map(|strut| match strut.landing {
                Landing::Plate if strut.kind != StrutKind::Brace => {
                    free_shaft_floor(flare.as_ref(), grid, strut, h).max(strut.k_bottom)
                }
                _ => strut.k_bottom,
            })
            .collect();
        // Members joined by landings, for the per-stage reach.
        input.branches_of = vec![Vec::new(); struts.len()];
        for (index, strut) in struts.iter().enumerate() {
            if strut.kind == StrutKind::Brace || !live(strut) {
                continue;
            }
            if let Landing::Strut { id, .. } = strut.landing {
                if let Some(list) = input.branches_of.get_mut(id as usize) {
                    list.push(index);
                }
            }
        }
        // A branch that lands on its trunk inside the trunk's rigid base (the
        // root flare, or the trunk's own fixed foot) stands on something fixed.
        input.base_fixed = struts
            .iter()
            .map(|strut| match strut.landing {
                Landing::Plate | Landing::Model { .. } => true,
                Landing::Strut { id, layer } => input
                    .fixed_layer
                    .get(id as usize)
                    .is_none_or(|&floor| layer <= floor),
            })
            .collect();
        // Diagonals by post, so the reach never scans them all.
        input.diagonals_at = vec![Vec::new(); struts.len()];
        for (index, d) in input.diagonals.iter().enumerate() {
            if let Some(list) = input.diagonals_at.get_mut(d.root_post) {
                list.push(index);
            }
            if d.seat_post != d.root_post {
                if let Some(list) = input.diagonals_at.get_mut(d.seat_post) {
                    list.push(index);
                }
            }
        }
        input.scale = 2.0 * core::f64::consts::PI / (DRAG_N_PER_MM * law.ratio().powi(4));
        input.pressure = pressure_kpa * 1.0e-3;
        input
    }

    fn height(&self, layer: usize) -> f64 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a layer index bounded by the plan's own layer count is exact in f64"
        )]
        let at = layer as f64 * self.layer_height_mm;
        at
    }

    /// A member's axis point at height `z`: on its straight drawn chord.
    fn point(&self, member: usize, z: f64) -> [f64; 3] {
        let (seat, base) = self.ends[member];
        let span = seat[2] - base[2];
        let t = if span > 0.0 {
            ((z - base[2]) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        vec3::add(base, vec3::scale(vec3::sub(seat, base), t))
    }

    /// The drawn radius of a live non-brace member at a layer (clamped to its
    /// own layers, as the profile is read).
    fn radius(&self, member: usize, layer: usize) -> f64 {
        let owned = &self.radii[self.radius_start[member]..self.radius_start[member + 1]];
        if owned.is_empty() {
            return self.min_radius_mm[member];
        }
        let clamped = layer.clamp(self.k_bottom[member], self.k_top[member]);
        owned[clamped - self.k_bottom[member]]
    }

    fn axis(&self, d: &Diagonal) -> ([f64; 3], [f64; 3]) {
        (
            self.point(d.root_post, self.height(d.bottom)),
            self.point(d.seat_post, self.height(d.top)),
        )
    }

    fn closes_at(&self, d: &Diagonal) -> f64 {
        d.meet.map_or_else(|| self.height(d.top), |meet| meet[2])
    }

    /// One stage's frame (phase 2): the members joined to a growing diagonal,
    /// their levels, the finished diagonals and the growing ones' loads.
    /// `None` where nothing grows or nothing is loaded.
    fn build_stage(&self, stage: usize) -> Option<StageFrame> {
        let members = self.ends.len();
        let z = self.height(stage);
        let growing: Vec<bool> = self
            .diagonals
            .iter()
            .map(|d| d.bottom < stage && z < self.closes_at(d))
            .collect();
        if !growing.contains(&true) {
            return None;
        }
        crate::perf::add(crate::PerfWork::ShakeStages, 1);
        // A diagonal JOINS its two posts at this stage once it is finished,
        // or once its lower half has met its twin's.
        let joins = |d: &Diagonal| {
            d.bottom < stage && (d.top <= stage || d.meet.is_some_and(|m| m[2] <= z))
        };
        // THE MEMBERS THAT CAN FEEL IT: joined to a growing diagonal's root
        // through trunks, branches and the bands joined by this stage.
        let mut reach = vec![false; members];
        let mut queue: VecDeque<usize> = VecDeque::new();
        for (d, _) in self.diagonals.iter().zip(&growing).filter(|(_, &grows)| grows) {
            if !reach[d.root_post] {
                reach[d.root_post] = true;
                queue.push_back(d.root_post);
            }
        }
        while let Some(member) = queue.pop_front() {
            let mut next: Vec<usize> = Vec::new();
            if let Some((trunk, _)) = self.parent[member] {
                next.push(trunk);
            }
            next.extend(
                self.branches_of[member]
                    .iter()
                    .copied()
                    .filter(|&branch| self.k_bottom[branch] < stage),
            );
            for d in self.diagonals_at[member].iter().map(|&i| &self.diagonals[i]) {
                if !joins(d) {
                    continue;
                }
                if d.root_post == member {
                    next.push(d.seat_post);
                } else if d.seat_post == member {
                    next.push(d.root_post);
                }
            }
            for other in next {
                if other < members && !reach[other] && self.is_live[other] {
                    reach[other] = true;
                    queue.push_back(other);
                }
            }
        }
        // Every member's levels up to the stage: its fixed base (or its foot
        // on its trunk), every branch and diagonal end on it, and its cut.
        let mut levels: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
        for (member, _) in reach.iter().enumerate().filter(|(_, &reached)| reached) {
            if self.is_brace[member] || self.fixed_layer[member] >= stage {
                continue;
            }
            let list = levels.entry(member).or_default();
            list.insert(self.fixed_layer[member]);
            list.insert(self.k_top[member].min(stage));
        }
        for (member, parent) in self.parent.iter().enumerate() {
            if let (true, Some((trunk, layer))) = (levels.contains_key(&member), *parent) {
                if !self.base_fixed[member] {
                    if let Some(list) = levels.get_mut(&trunk) {
                        list.insert(layer);
                    }
                }
            }
        }
        for d in self.diagonals.iter().filter(|d| d.bottom < stage) {
            for (post, layer) in [(d.root_post, d.bottom), (d.seat_post, d.top)] {
                if layer <= stage {
                    if let Some(list) = levels.get_mut(&post) {
                        list.insert(layer);
                    }
                }
            }
        }
        let mut frame = Frame::new();
        let mut node_of: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        let mut tops: Vec<(usize, usize)> = Vec::new();
        for (&member, list) in &levels {
            let mut previous: Option<(usize, usize)> = None;
            for &layer in list.range(self.fixed_layer[member]..=stage) {
                let fixed = previous.is_none() && self.base_fixed[member];
                let node = frame.node(self.point(member, self.height(layer)), fixed);
                node_of.insert((member, layer), node);
                if let Some((below, below_layer)) = previous {
                    frame.member(below, node, self.radius(member, (layer + below_layer) / 2));
                }
                previous = Some((node, layer));
            }
            if self.k_top[member] >= stage {
                if let Some((top, _)) = previous {
                    tops.push((member, top));
                }
            }
        }
        // Branches stand on their trunks: a stub from the trunk's axis point
        // to the branch's own foot point (the two axes meet within a cell),
        // drawn at the branch's root radius.
        for (&member, list) in &levels {
            let Some((trunk, layer)) = self.parent[member] else {
                continue;
            };
            if self.base_fixed[member] {
                continue;
            }
            let own = list
                .iter()
                .next()
                .and_then(|first| node_of.get(&(member, *first)));
            if let (Some(&own), Some(&trunk)) = (own, node_of.get(&(trunk, layer))) {
                frame.member(trunk, own, self.radius(member, self.k_bottom[member]));
            }
        }
        // The diagonals as printed by this stage.
        let mut loads: Vec<(usize, [f64; 3])> = Vec::new();
        for (index, d) in self.diagonals.iter().enumerate() {
            if d.bottom >= stage {
                continue;
            }
            let Some(&root) = node_of.get(&(d.root_post, d.bottom)) else {
                continue;
            };
            let (a, b) = self.axis(d);
            if d.top <= stage {
                if let Some(&seat) = node_of.get(&(d.seat_post, d.top)) {
                    match d.meet {
                        Some(meet) => {
                            let middle = frame.node(meet, false);
                            frame.member(root, middle, d.radius_mm);
                            frame.member(middle, seat, d.radius_mm);
                        }
                        None => frame.member(root, seat, d.radius_mm),
                    }
                }
                continue;
            }
            if !growing[index] {
                // The lower half has met its twin's; the upper half grows off
                // the fused cross, and its load is not counted (as the study).
                if let Some(meet) = d.meet {
                    let middle = frame.node(meet, false);
                    frame.member(root, middle, d.radius_mm);
                }
                continue;
            }
            let span = b[2] - a[2];
            if span <= 0.0 || span.is_nan() {
                continue;
            }
            let t = ((z - a[2]) / span).clamp(0.0, 1.0);
            let tip_point = vec3::add(a, vec3::scale(vec3::sub(b, a), t));
            let tip = frame.node(tip_point, false);
            frame.member(root, tip, d.radius_mm);
            // The newest layer's section is the diagonal's ellipse,
            // `π r_b² / cos φ`, pulled up by the film.
            let secant = vec3::dist(a, b) / span;
            let force =
                self.pressure * core::f64::consts::PI * d.radius_mm * d.radius_mm * secant;
            loads.push((tip, [0.0, 0.0, force]));
        }
        if loads.is_empty() {
            return None;
        }
        let skyline = frame.skyline_entries(&loads);
        Some(StageFrame {
            stage,
            frame,
            loads,
            tops,
            skyline,
        })
    }

    /// Solve one stage's frame and read the move of its printing tops.
    fn solve_stage(&self, built: StageFrame) -> StageResult {
        let StageFrame {
            stage,
            frame,
            loads,
            tops,
            ..
        } = built;
        let solution = frame.solve(&loads);
        let unsolved = loads.iter().any(|&(node, _)| !solution.solved[node]);
        let mut moved_px: Vec<(u32, f64)> = Vec::new();
        for &(member, node) in &tops {
            if !solution.solved[node] {
                continue;
            }
            let moved = solution.moved[node];
            let px = moved[0].hypot(moved[1]) * self.scale;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "member ids are u32 by construction"
            )]
            let id = member as u32;
            moved_px.push((id, px));
        }
        StageResult {
            stage,
            unsolved,
            tops: moved_px,
        }
    }

    /// Run the check (phases 2 and 3). The same answer for every thread
    /// count; see the module docs.
    #[must_use]
    pub fn check(&self) -> StageShake {
        self.check_with_budget(SOLVE_BUDGET_BYTES)
    }

    /// [`ShakeInput::check`] with another solve budget (tests force several
    /// batches with a small one). The answer does not depend on it.
    #[must_use]
    pub(crate) fn check_with_budget(&self, budget_bytes: usize) -> StageShake {
        let _perf_span = crate::perf::span(crate::perf::Span::StageShake);
        crate::perf::add(crate::PerfWork::ShakeChecks, 1);
        let mut out = StageShake {
            pressure_kpa: self.pressure_kpa,
            ..StageShake::default()
        };
        if !self.active {
            return out;
        }
        out.diagonals = self.diagonals.len();
        out.unjoined = self.diagonals.iter().filter(|d| d.meet.is_none()).count();
        // Phase 2: every stage's frame, collected by stage position.
        let built: Vec<StageFrame> = self
            .stages
            .par_iter()
            .map(|&stage| self.build_stage(stage))
            .collect::<Vec<_>>()
            .into_iter()
            .flatten()
            .collect();
        out.frames_solved = built.len();
        out.skyline_entries_max = built.iter().map(|b| b.skyline).max().unwrap_or(0);
        // Phase 3: batches in stage order, each within the budget (a frame
        // over it goes alone), solved in parallel and collected by position.
        let budget_entries = budget_bytes / size_of::<f64>();
        let mut results: Vec<StageResult> = Vec::with_capacity(built.len());
        let mut batch: Vec<StageFrame> = Vec::new();
        let mut held = 0_usize;
        let mut skyline_total = 0_usize;
        let mut flush = |batch: &mut Vec<StageFrame>, results: &mut Vec<StageResult>| {
            if batch.is_empty() {
                return;
            }
            let frames = core::mem::take(batch);
            results.extend(
                frames
                    .into_par_iter()
                    .map(|built| self.solve_stage(built))
                    .collect::<Vec<_>>(),
            );
            out.batches += 1;
        };
        for frame in built {
            skyline_total = skyline_total.saturating_add(frame.skyline);
            if !batch.is_empty() && held.saturating_add(frame.skyline) > budget_entries {
                flush(&mut batch, &mut results);
                held = 0;
            }
            held = held.saturating_add(frame.skyline);
            batch.push(frame);
        }
        flush(&mut batch, &mut results);
        crate::perf::add(crate::PerfWork::ShakeFramesSolved, crate::perf::count(out.frames_solved));
        crate::perf::add(crate::PerfWork::ShakeBatches, crate::perf::count(out.batches));
        crate::perf::add(crate::PerfWork::ShakeSkylineEntries, crate::perf::count(skyline_total));
        // The fold, serial, in ascending stage order and each stage's top
        // order: the only order-sensitive steps are the strict `>` ties.
        let mut worst_of: BTreeMap<usize, ShakenTop> = BTreeMap::new();
        for result in results {
            let stage = result.stage;
            out.stages += 1;
            if result.unsolved {
                out.unsolved_stages += 1;
            }
            let mut over = false;
            for &(id, px) in &result.tops {
                let member = id as usize;
                if px > out.worst_px {
                    out.worst_px = px;
                    out.worst_member = Some(id);
                    out.worst_layer = stage;
                }
                if px > 1.0 {
                    over = true;
                    let entry = worst_of.entry(member).or_insert(ShakenTop {
                        member: id,
                        layer: stage,
                        px,
                    });
                    if px > entry.px {
                        *entry = ShakenTop {
                            member: id,
                            layer: stage,
                            px,
                        };
                    }
                }
            }
            if over {
                out.stages_over_pixel += 1;
            }
        }
        let mut tops: Vec<ShakenTop> = worst_of.into_values().collect();
        tops.sort_by(|a, b| b.px.total_cmp(&a.px).then(a.member.cmp(&b.member)));
        out.tops = tops;
        out.pressure_for_one_pixel_kpa = if out.worst_px > 0.0 {
            self.pressure_kpa / out.worst_px
        } else {
            f64::INFINITY
        };
        out
    }
}

/// Run the check on a finished field. `profiles` are the drawn profiles
/// (`adapter::drawn_profiles`). Batch-first: the members, the diagonals,
/// their twins and meeting points are read once ([`ShakeInput::capture`]);
/// each stage builds only the members joined, through landings and the bands
/// begun by then, to a diagonal growing at that stage, and solves only the
/// pieces that carry a load, the stages side by side.
#[must_use]
pub fn check(
    grid: &Grid,
    struts: &[SweepStrut],
    profiles: &[Vec<f64>],
    layer_height_mm: f64,
    law: &SlendernessLaw,
    pressure_kpa: f64,
) -> StageShake {
    ShakeInput::capture(grid, struts, profiles, layer_height_mm, law, pressure_kpa).check()
}

#[cfg(test)]
mod tests {
    use super::{check, ShakeInput, StageShake, DRAG_N_PER_MM};
    use crate::grid::{lay_out_at, Grid};
    use crate::slenderness::SlendernessLaw;
    use crate::{Landing, StrutKind, SweepStrut};

    const H: f64 = 0.05;

    fn member(grid: &Grid, id: u32, col: usize, k_bottom: usize, k_top: usize) -> SweepStrut {
        let row = 100;
        let cell = grid.index(row, col);
        SweepStrut {
            id,
            cell,
            center_mm: [grid.center_x_mm(col), grid.center_y_mm(row)],
            kind: StrutKind::Arm,
            min_radius_mm: 0.0,
            thickened_mm: Vec::new(),
            seat_on_strut: None,
            k_top,
            k_bottom,
            landing: Landing::Plate,
            interior: false,
            radius_mm: Vec::new(),
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(), held_births: Vec::new(), twin_host: false,
            inserted_by_load: false,
            path: Vec::new(),
            seat_face: crate::SeatFace::default(),
            seat_normal: [0.0; 3],
            straight_fallback: None,
            fallback_path: Vec::new(),
            head: None,
        }
    }

    /// Two 0.3 mm plate posts 2.2 mm apart, 20 mm tall, and one `|X|`
    /// between them over layers 260-301 (its diagonals cross at layer
    /// 280.5), its diagonals 0.06 mm.
    fn field(grid: &Grid) -> Vec<SweepStrut> {
        let (a, b) = (80, 124);
        let post = |id: u32, col: usize| {
            let mut post = member(grid, id, col, 0, 400);
            post.radius_mm = vec![0.3; 401];
            post
        };
        let diagonal = |id: u32, seat: u32, seat_col: usize, root: u32| {
            let mut brace = member(grid, id, seat_col, 260, 301);
            brace.kind = StrutKind::Brace;
            brace.min_radius_mm = 0.06;
            brace.seat_on_strut = Some(seat);
            brace.landing = Landing::Strut {
                id: root,
                layer: 260,
            };
            brace
        };
        vec![post(0, a), post(1, b), diagonal(2, 1, b, 0), diagonal(3, 0, a, 1)]
    }

    fn profiles(struts: &[SweepStrut]) -> Vec<Vec<f64>> {
        struts.iter().map(|strut| strut.radius_mm.clone()).collect()
    }

    /// Six 0.3 mm plate posts 1.1 mm apart and seventeen diagonals between
    /// them: eight crosses `|X|` at staggered layers (posts `a`-`b`, layers
    /// `bottom`-`top`; the last three span two gaps), and one diagonal with
    /// no twin that grows to its full length.
    fn many_stage_field(grid: &Grid) -> Vec<SweepStrut> {
        let cols = [60, 82, 104, 126, 148, 170];
        let mut struts: Vec<SweepStrut> = Vec::new();
        for (id, &col) in cols.iter().enumerate() {
            let mut post = member(grid, u32::try_from(id).unwrap(), col, 0, 400);
            post.radius_mm = vec![0.3; 401];
            struts.push(post);
        }
        let mut diagonal = |root: usize, seat: usize, bottom: usize, top: usize| {
            let id = u32::try_from(struts.len()).unwrap();
            let mut brace = member(grid, id, cols[seat], bottom, top);
            brace.kind = StrutKind::Brace;
            brace.min_radius_mm = 0.06;
            brace.seat_on_strut = Some(u32::try_from(seat).unwrap());
            brace.landing = Landing::Strut {
                id: u32::try_from(root).unwrap(),
                layer: bottom,
            };
            struts.push(brace);
        };
        for (a, b, bottom, top) in [
            (0, 1, 200, 241),
            (1, 2, 220, 261),
            (2, 3, 240, 281),
            (3, 4, 260, 301),
            (4, 5, 280, 321),
            (0, 2, 300, 341),
            (2, 4, 320, 361),
            (3, 5, 340, 381),
        ] {
            diagonal(a, b, bottom, top);
            diagonal(b, a, bottom, top);
        }
        diagonal(1, 5, 150, 391);
        struts
    }

    /// Every figure of a result as exact bits, in one line (the work
    /// counters are not part of the answer and are left out).
    fn digest(shake: &StageShake) -> String {
        let tops: Vec<String> = shake
            .tops
            .iter()
            .map(|top| format!("({},{},{:016x})", top.member, top.layer, top.px.to_bits()))
            .collect();
        format!(
            "p={:016x} diagonals={} unjoined={} stages={} over={} unsolved={} worst={:016x} \
             member={:?} layer={} one_px={:016x} tops=[{}]",
            shake.pressure_kpa.to_bits(),
            shake.diagonals,
            shake.unjoined,
            shake.stages,
            shake.stages_over_pixel,
            shake.unsolved_stages,
            shake.worst_px.to_bits(),
            shake.worst_member,
            shake.worst_layer,
            shake.pressure_for_one_pixel_kpa.to_bits(),
            tops.join(","),
        )
    }

    /// The answer of the serial, stage-by-stage check as it stood before the
    /// stages were solved side by side (recorded from that code, 2026-10-09),
    /// on [`many_stage_field`] at 124 kPa: 9 stages, 6 tops over a pixel.
    const GOLDEN: &str = "p=405f000000000000 diagonals=17 unjoined=1 stages=9 over=9 unsolved=0 \
        worst=401e46bf122795c0 member=Some(1) layer=240 one_px=403061e66d20a1c7 \
        tops=[(1,240,401e46bf122795c0),(5,300,4018c8e7be4afe5b),(0,320,4015e0bea719633e),\
        (4,280,40154ad740c9b9a7),(3,260,40121040c3184b6f),(2,240,400e32488a6d463f)]";

    /// **Golden bits, at every width and every batching.** The stages are
    /// solved side by side; the answer is the serial one to the last bit, in
    /// rayon pools of 1, 4 and 16, whether one batch holds every stage or each
    /// stage is its own.
    #[test]
    fn the_stages_solved_side_by_side_give_the_serial_answer_to_the_last_bit() {
        let grid = lay_out_at(0.0, 0.0, H, 200, 200);
        let struts = many_stage_field(&grid);
        let law = SlendernessLaw::new(40.0, 0.05, 1.0, 32.5);
        let input = ShakeInput::capture(&grid, &struts, &profiles(&struts), H, &law, 124.0);
        let widest = input.check().skyline_entries_max;
        assert!(widest > 0, "the stages have a skyline");
        let mut counters = Vec::new();
        for width in [1_usize, 4, 16] {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(width).build().unwrap();
            for budget in [crate::stage_shake::SOLVE_BUDGET_BYTES, 8, 8 * widest * 2] {
                let shake = pool.install(|| input.check_with_budget(budget));
                assert_eq!(digest(&shake), GOLDEN, "width {width}, budget {budget}");
                assert_eq!(shake.frames_solved, shake.stages);
                counters.push((shake.frames_solved, shake.skyline_entries_max, shake.batches, budget));
            }
        }
        // Path hits: the field solved 9 stages; one batch under the default
        // budget, one per stage under a budget of one `f64`, and some count
        // under the middle one (two of the largest frames); the largest
        // skyline is the same whatever the width or the batching.
        assert!(counters.iter().all(|c| c.0 == 9 && c.1 == widest));
        for &(_, _, batches, budget) in &counters {
            match budget {
                8 => assert_eq!(batches, 9, "a one-f64 budget: one stage per batch"),
                b if b == crate::stage_shake::SOLVE_BUDGET_BYTES => assert_eq!(batches, 1),
                _ => assert!((1..=9).contains(&batches), "{batches} batches"),
            }
        }
        // The free function is the same check.
        let direct = check(&grid, &struts, &profiles(&struts), H, &law, 124.0);
        assert_eq!(digest(&direct), GOLDEN);
        // Capturing twice gives the same input.
        assert_eq!(input, ShakeInput::capture(&grid, &struts, &profiles(&struts), H, &law, 124.0));
    }

    /// **The study's own synthetic check, as a closed form.** At the last
    /// layer before the two diagonals meet (layer 280), each grows off its
    /// post as a cantilever with its newest layer's peel `F = p π r_b² sec φ`
    /// at its tip, a lever `x` from the post. Below the root the post bends
    /// under `M = F x`; above it the post is straight, so the printing top,
    /// `b` above the post's fixed base with the root `a` above it, moves
    /// `M a (b − a/2) / (E I)` (`bracing-mechanics.md` §2). In pixels that is
    /// `· 2π / (q S⁴)` at `E = 1`. The check must reproduce it exactly: the
    /// beam elements are exact under nodal loads.
    #[test]
    fn a_growing_diagonal_shakes_its_post_by_the_closed_form() {
        let grid = lay_out_at(0.0, 0.0, H, 200, 200);
        let struts = field(&grid);
        let law = SlendernessLaw::new(40.0, 0.05, 1.0, 32.5);
        let shake = check(&grid, &struts, &profiles(&struts), H, &law, 1.24);
        assert_eq!(shake.diagonals, 2);
        assert_eq!(shake.unjoined, 0, "the two diagonals cross at mid-gap");
        assert_eq!(shake.stages, 1, "{shake:?}");
        assert_eq!(shake.worst_layer, 280);
        let flare = plinth_support::SweepRootFlare::registered(H).ok();
        let floor = crate::slenderness::free_shaft_floor(flare.as_ref(), &grid, &struts[0], H);
        let (z_fix, z_root, z_seat, z) =
            (f64::from(u32::try_from(floor).unwrap()) * H, 260.0 * H, 301.0 * H, 280.0 * H);
        let gap = crate::cell_distance_mm(&grid, struts[0].cell, struts[1].cell);
        assert!((gap - 2.2).abs() < 1e-9, "gap {gap}");
        let t = (z - z_root) / (z_seat - z_root);
        let lever = t * gap;
        let secant = gap.hypot(z_seat - z_root) / (z_seat - z_root);
        let force = 1.24e-3 * core::f64::consts::PI * 0.06 * 0.06 * secant;
        let second = core::f64::consts::PI * 0.3_f64.powi(4) / 4.0;
        let (a, b) = (z_root - z_fix, z - z_fix);
        let moved = force * lever * a * (b - 0.5 * a) / second;
        let px = moved * 2.0 * core::f64::consts::PI / (DRAG_N_PER_MM * 40.0_f64.powi(4));
        assert!(
            (shake.worst_px - px).abs() <= 1.0e-9 * px,
            "check {} px against the closed form {px} px",
            shake.worst_px
        );
        assert!((shake.pressure_for_one_pixel_kpa - 1.24 / px).abs() <= 1.0e-9 * (1.24 / px));
    }

    /// The figure is linear in the stated pressure (it is a warning AT that
    /// pressure, nothing more), and a pressure of zero switches it off.
    #[test]
    fn the_shake_scales_with_the_stated_pressure_and_zero_is_off() {
        let grid = lay_out_at(0.0, 0.0, H, 200, 200);
        let struts = field(&grid);
        let law = SlendernessLaw::new(40.0, 0.05, 1.0, 32.5);
        let one = check(&grid, &struts, &profiles(&struts), H, &law, 1.24);
        let two = check(&grid, &struts, &profiles(&struts), H, &law, 2.48);
        assert!(one.worst_px > 0.0);
        assert!((two.worst_px - 2.0 * one.worst_px).abs() <= 1.0e-9 * two.worst_px);
        assert!((two.pressure_for_one_pixel_kpa - one.pressure_for_one_pixel_kpa).abs() < 1e-9);
        let off = check(&grid, &struts, &profiles(&struts), H, &law, 0.0);
        assert_eq!(off.stages, 0);
        assert!(off.tops.is_empty());
        // Over one pixel only where the figure says so.
        let high = check(&grid, &struts, &profiles(&struts), H, &law, 1.24 * 2.0 / one.worst_px);
        assert_eq!(high.tops.len(), 2, "both posts' tops move about 2 px: {high:?}");
    }
}
