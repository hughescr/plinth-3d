//! **A lander joins a support arm far enough BELOW its head that the section
//! carrying the sum clears the seat** (head-join, 2026-09-29).
//!
//! # The defect
//!
//! A member that lands on a support arm's HEAD (at the arm's own seat layer,
//! or one under it) hands the arm its load right at the contact. Since the
//! contact-sizing rule (2026-09-29, `down_sweep::run`) that load is booked one
//! slot under the contact, so the ledger's contact is sized from the arm's own
//! peel. The GEOMETRY did not follow. The drawn shaft is sum-sized from one
//! slot (0.05 mm) under the seat plane, and on an arm that leans the ring of
//! that section is tilted: on the Round 25 ring stiffener (arms 39, 41 and 44,
//! about 53° from vertical) its upper edge rises about 0.52 mm and crosses the
//! seat plane. The seat trim cuts it at the plane, so the cut section lies on
//! the model face. The file's contact was the own-sized ball joined with a
//! trimmed sum-sized section, and on arms 39 and 44 that shaft was wider than
//! the ball (`lanes/contact-sizing.md` §5). The lander's own end sat at the
//! parent's seat point, inside the ball, and its solid lay on the face.
//!
//! # The rule
//!
//! A lander may join the load path under a support arm's head only where
//!
//! 1. the section that carries the SUM stands at least the registered final
//!    clearance (`FINAL_CLEARANCE_GAP_MM`, 0.3 mm, the owner's field ruling of
//!    2026-08-22) from the seat plane of the member it joins, on the support
//!    side, or from the model itself where the plane is not the model (below)
//!    — so above the join the head arm carries its own peel only, and
//!    everything of it in its seat zone is sized from its own load; and
//! 2. the lander's own solid, re-landed there, keeps the two-tier clearance
//!    from the model (the same ruling): the placement margin (`STANDOFF_MM`,
//!    0.6 mm) where any such join exists, else the 0.3 mm hard limit. A join
//!    that only moved the lander's contact with the model somewhere else would
//!    not be a fix.
//!
//! The joins tried are the head arm's own shaft under the booked slot, then
//! the member it lands on, and so on down its landing chain (support arms
//! only), highest first. The first that passes both tests at the better tier
//! wins. Measured on the Round 25 stiffener: no slot of arm 39 itself lets
//! lander 69 clear the ring's lower corner (its straight segment clips it by
//! 0.07 to 0.50 mm at every slot), and arm 39's own trunk, arm 4, does at and
//! below layer 57.
//!
//! **Test 1 is measured on the member as the file draws it.** The drawn shaft
//! starts at the tip ball's centre and runs straight to the base
//! (`plinth_support::lower_sweep_arms`). The ball centre and the seat plane
//! come from [`plinth_support::tip_contact_discs`], which is the lowering's own
//! derivation. The sum-sized span starts at the top of the slot the load is
//! booked into, or at the ball centre where that is lower. Its ring is normal
//! to the shaft axis `m`, so its closest point to the plane is
//! `d(centre) − R·√(1 − (n·m)²)` for a plane of outward normal `n`, with `R`
//! the DRAWN radius there (junction raises included).
//!
//! **Test 2 is the kernel's clearance predicate**
//! ([`geom_kernel::NativeSurface::segment_clearance_classified`], the one the
//! legacy engine's brace and rewire stages use): the lander's drawn segment
//! from its seat to the join, at its widest drawn radius, against the model
//! surface. The part of the segment inside the lander's own contact (its seat
//! to the far side of its tip ball) is not tested: that is where it is meant
//! to touch. A row the predicate could not decide ([`UpperBound`] or
//! [`Unmeasured`]) does not pass. Members that rest on a moved lander are
//! re-landed on its new axis and must be no nearer the model than they were,
//! up to the same tier.
//!
//! [`UpperBound`]: geom_kernel::SegmentClearanceClassified::UpperBound
//! [`Unmeasured`]: geom_kernel::SegmentClearanceClassified::Unmeasured
//!
//! A vertical arm under a flat face is untouched when its ball already puts the
//! sum-sized span deep enough (Round 25 arm 42: 0.468 mm, over 0.3 mm).
//!
//! **Every join on a support arm, graded by geometry, not by landing layer.**
//! A lander one or three layers under a leaned head is booked into the same
//! slot as one at the head (contact-sizing books a head landing one slot under
//! the contact, and any other landing at its own layer), so its sum-sized
//! section stands where a head join's does. `cb28bdd` looked at head landings
//! only, and on Round 50 at its committed flip four ~57° arms (66, 68, 83, 98)
//! kept a sum across the seat plane by 0.33 to 0.48 mm, unwarned.
//!
//! **The seat zone is where the seat plane is the model.** Test 1's plane is
//! the model face at the contact, and it is infinite: under a steep face it
//! passes below the model's lowest point (Round 25 arm 6, face 75° from
//! horizontal: its lander's sum-sized section, 1.25 mm under the seat,
//! crosses the plane with no model within reach). So a section is in the seat
//! zone only when it is within the clearance of the plane AND of the model
//! itself, measured on its disc by the kernel's clearance predicate
//! ([`sections_near_model`]); the same zone decides which joins move and which
//! candidates may take a lander. Where the model IS there, the join is the
//! rule's whatever the arm's lean: Round 50 arm 155 is vertical under a steep
//! face, and its lander's sum-sized section, 1.05 mm under the seat, crossed
//! the face (the lander's own solid 0.088 mm inside the model).
//!
//! # What moves, and what it is held to
//!
//! The lander is re-landed along the straight segment from its own seat, with
//! the axis refusals a remedy-1 merge uses (`slenderness::segment_path`: model
//! material, another member, a drain's escape cylinder) and the lean law, as
//! well as test 2. Members that rest on the lander are re-landed on its new
//! axis at their own landing layer, transitively, under the same tests.
//!
//! **Above the raft** (fix/round100-emit, 2026-09-29; the raft top since
//! fix/round11-milestone, 2026-10-02). Every join, a lander's or a
//! dependant's, keeps the member's drawn base disc at or above the raft top
//! ([`stays_above_raft`], [`join_floor_z_mm`]). The candidate walk reaches
//! each host's own bottom layer, and a plate arm's bottom layer is the plate:
//! on Round_100 a lander re-landed there dipped 0.2335 mm under it, and the
//! emitter's disk audit refused the file; on the procedural Round 25 a lander
//! moved into a plate post's flare reached through the post's raft pad.
//!
//! **Emit and warn.** A join that cannot move — no join passes, a cross
//! (brace) tied to a member that would have to move, a lander that is not a
//! support arm, or no model surface to measure against — is kept as it is and
//! counted by its reason in [`HeadJoinOutcome`]. The EMITTED field (after the
//! footprint rule, which may slide a seat and so re-tilt a shaft) is surveyed
//! again, and what still reaches a seat zone is published.
//!
//! No new physical constant: the two clearances are the registered ruling,
//! and every other number is the down-sweep's, the lowering's or the grid's.
//! One new resolution: a section's disc is measured as a FILLED circumscribed
//! 16-gon ([`SECTION_POLYGON_SIDES`]), which errs only toward "near" (by at
//! most 0.02 mm at the largest registered radius). Filled, not its edges and
//! spokes alone (round-3 review, 2026-09-29): a model feature between two
//! spokes, over the disc's interior, is within the clearance of the disc
//! while every spoke and rim edge clears it, so the model's own edges are
//! measured against the filled polygon too ([`sections_near_model`]).
//! The surface index's parity-grid pitch and byte cap are the registered
//! `DECIMATION_PROBE_GRID_PITCH_MM` / `_MAX_GRID_BYTES` (the only registered
//! pair for a distance-probe surface index); the segment clearance is a BVH
//! walk and does not depend on that pitch.
//!
//! Batch-first: one survey per pass (the drawn profiles, the seat faces of
//! every trunk with a lander in one closest-face query, every contact disc
//! the pass needs in one call, and every section the plane holds against the
//! model in one clearance batch), the surface index built once per run and
//! only when a section is near a seat plane, one section batch per pass over
//! every candidate the plane holds, and one clearance batch per lander over
//! all of its candidate joins.

use std::cell::OnceCell;

use geom_kernel::{NativeSurface, NativeSurfaceConfig, SegmentClearanceClassified};
use plinth_core::LayerPlan;
use plinth_mesh::Mesh;
use plinth_support::SweepArm;

use crate::slenderness::{segment_path, SlendernessContext};
use crate::up_sweep::ColumnHistory;
use crate::{Landing, StrutKind, SweepStrut};

/// What the head-join rule measured and did on one run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeadJoinOutcome {
    /// Whether the rule may move joins. `false` only under the sabotage knob;
    /// the emitted-field census still runs and publishes.
    pub enabled: bool,
    /// The clearance the sum-sized section must keep from the seat plane, and
    /// the hard limit for a moved member's solid, in mm (`FINAL_CLEARANCE_GAP_MM`).
    pub clearance_mm: f64,
    /// The placement margin a moved member's solid is searched at first, in mm
    /// (`STANDOFF_MM`).
    pub standoff_mm: f64,
    /// Joins on support arms (a member landed on a support arm, at any layer)
    /// examined by the first survey. The path-hit witness: zero means no
    /// member landed on a support arm at all.
    pub joins_examined: usize,
    /// Joins whose sum-sized section reached the host's seat zone (within the
    /// clearance of its seat plane AND of the model) when the rule first ran,
    /// before it moved anything.
    pub joins_in_seat_zone_before: usize,
    /// Of those, the joins that stood in a HEADED host's head span or on its
    /// joint sphere (above its joinable top): always in the zone, re-landed at
    /// the joint or below when a join exists (v19 fix, 2026-10-08). The
    /// path-hit witness for that rule.
    pub joins_in_head_span: usize,
    /// Joins, at that first survey, whose sum-sized section was within the
    /// clearance of the host's seat plane but clear of the model: the plane is
    /// not the model there (a steep face), and the join is left alone. The
    /// path-hit witness for the model half of the seat zone.
    pub joins_off_the_model: usize,
    /// Candidate joins the seat plane alone would have refused and the model
    /// test admitted, over every pass.
    pub candidates_off_the_model: usize,
    /// Section-against-model rows measured by the kernel's clearance
    /// predicate (the model half of test 1), over every pass.
    pub model_rows: usize,
    /// The interior term of that model test (round-3 review fix,
    /// 2026-09-29): model-edge rows measured against the FILLED sections, and
    /// the sections it alone found near (every rim edge and spoke clear, the
    /// centre outside), over every pass. Path-hit witnesses.
    pub interior_rows: usize,
    pub sections_near_by_interior: usize,
    /// Of the emitted field's census below, the joins the interior term alone
    /// put in the seat zone.
    pub census_near_by_interior: usize,
    /// THE LOAD-STEP CONE (fix/tapered-load-steps, 2026-10-02): joins the
    /// first survey held by the cone above their booked section alone, and on
    /// the emitted field's census the cone layers graded and the joins held by
    /// their cone alone. Path-hit witnesses of the cone half of test 1.
    pub joins_held_by_load_cone: usize,
    pub census_load_cone_layers: usize,
    pub census_held_by_load_cone: usize,
    /// Landers moved, over every pass.
    pub landers_moved: usize,
    /// Of those, the ones that went down the head arm's landing chain, onto a
    /// member below it, because no slot of the head arm itself passed.
    pub landers_moved_down_the_chain: usize,
    /// Of those, the ones placed at the 0.3 mm hard limit because no join
    /// passed the 0.6 mm placement margin.
    pub landers_at_hard_limit: usize,
    /// The largest single drop, in layers, from the old join to the new one.
    pub max_drop_layers: usize,
    /// The smallest clearance a moved lander's solid keeps from the model, in
    /// mm (`INFINITY` when nothing moved).
    pub min_lander_clearance_mm: f64,
    /// The load the moved landers hand the path below, in mm², summed.
    pub load_moved_mm2: f64,
    /// Members resting on a moved lander that were re-landed on its new axis.
    pub dependants_relanded: usize,
    /// Candidate joins measured by the clearance predicate (the path-hit
    /// witness for test 2).
    pub clearance_rows: usize,
    /// Candidate joins (a lander's, or a dependant's re-landing) refused
    /// because the member's drawn base disc would reach under the raft top
    /// (`stays_above_raft`; the build plate until fix/round11-milestone,
    /// 2026-10-02), over every pass. The path-hit witness for the plate rule
    /// (fix/round100-emit, 2026-09-29).
    pub candidates_below_plate: usize,
    /// Rule passes that moved something (each followed by a down-sweep).
    pub passes: usize,
    /// Joins kept in the seat zone at the last pass, by reason (warnings, and
    /// states rather than events: the last pass describes the field).
    pub kept_no_clear_join: usize,
    pub kept_carrying_a_tie: usize,
    pub kept_not_an_arm: usize,
    /// Joins kept because the lander is HEADED, or a member that would have
    /// to move with it is: a head is frozen at placement and nothing moves a
    /// headed arm.
    pub headed_kept: usize,
    pub kept_surface_unavailable: usize,
    /// Joins whose host's seat geometry could not be measured (no seat plane
    /// or a degenerate shaft), so the rule could not grade them.
    pub joins_unmeasured: usize,
    /// **The emitted field**: joins whose sum-sized section still reaches the
    /// host's seat zone, as `(lander id, trunk id)`. A warning; empty when the
    /// rule held everywhere.
    pub in_seat_zone_after: Vec<(u32, u32)>,
}

/// Everything the rule reads besides the strut set.
pub(crate) struct HeadJoinContext<'a> {
    pub slender: &'a SlendernessContext<'a>,
    pub mesh: &'a Mesh,
    pub layer_plan: &'a LayerPlan,
    pub plate_z_mm: f64,
    /// `FINAL_CLEARANCE_GAP_MM`: test 1, and the hard limit of test 2.
    pub clearance_mm: f64,
    /// `STANDOFF_MM`: test 2's placement margin.
    pub standoff_mm: f64,
    pub surface_pitch_mm: f64,
    pub surface_max_bytes: u64,
    /// The model surface, built on first need and kept for the run; `None`
    /// inside when it could not be built.
    pub surface: &'a OnceCell<Option<NativeSurface>>,
}

/// The model's surface index, or `None` when its memory estimate is over
/// `max_bytes` or it cannot be built. One builder for every reader: the
/// placement's descent test (`up_sweep`, which reads it before any arm
/// stands) and the head-join, lip-seat and census rules (which read it later
/// from the same cell).
pub(crate) fn build_surface(mesh: &Mesh, pitch_mm: f64, max_bytes: u64) -> Option<NativeSurface> {
    let verts = mesh.verts();
    let tris = mesh.tris();
    let estimate =
        NativeSurface::estimate_index_memory_from_indexed(verts, tris, pitch_mm).ok()?;
    let cap = usize::try_from(max_bytes).unwrap_or(usize::MAX);
    if estimate.peak_bytes() > cap {
        return None;
    }
    NativeSurface::from_indexed(
        verts,
        tris,
        NativeSurfaceConfig {
            grid_pitch_mm: pitch_mm,
            max_grid_bytes: estimate.max_axis_grid_bytes(),
        },
    )
    .ok()
}

impl HeadJoinContext<'_> {
    /// The model surface, built on first need (the lip-seat rule reads it
    /// too, for a re-landed member's solid clearance).
    pub(crate) fn surface(&self) -> Option<&NativeSurface> {
        self.surface
            .get_or_init(|| build_surface(self.mesh, self.surface_pitch_mm, self.surface_max_bytes))
            .as_ref()
    }
}

/// One member's measured tip: the drawn shaft from the ball centre `centre`
/// to the base `base` (unit direction `axis`), the seat plane (`plane_point`,
/// outward `normal`), the ring spread `√(1 − (n·m)²)` of a section normal to
/// that shaft, and how far its own contact reaches down its axis from the
/// drawn seat.
#[derive(Clone, Copy)]
struct Head {
    centre: [f64; 3],
    base: [f64; 3],
    axis: [f64; 3],
    plane_point: [f64; 3],
    normal: [f64; 3],
    spread: f64,
    seat: [f64; 3],
    contact_reach_mm: f64,
}

/// A join on a support arm, as the survey found it.
#[derive(Clone, Copy)]
struct Join {
    child: usize,
    parent: usize,
    layer: usize,
    /// The slot of the parent the lander's load is booked into.
    booked: usize,
    /// The lander stands on a headed parent's head span or joint sphere
    /// (above its joinable top): always in the rule's zone.
    in_head_span: bool,
}

/// One drawn section of a shaft: the centre of its ring, the shaft's unit
/// axis (the ring is normal to it) and its radius.
#[derive(Clone, Copy)]
struct Section {
    centre: [f64; 3],
    axis: [f64; 3],
    radius: f64,
}

impl Head {
    /// The section of radius `radius` whose top is the top of layer `slot`, or
    /// the ball centre where that is lower.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a layer index bounded by the plan's own layer count is exact in f64"
    )]
    fn section_at(
        &self,
        plate_z_mm: f64,
        layer_height_mm: f64,
        slot: usize,
        radius: f64,
    ) -> Section {
        let slot_top_z = plate_z_mm + (slot + 1) as f64 * layer_height_mm;
        let rise = self.centre[2] - self.base[2];
        let top_z = slot_top_z.min(self.centre[2]);
        let t = ((self.centre[2] - top_z) / rise).clamp(0.0, 1.0);
        Section {
            centre: std::array::from_fn(|axis| {
                self.centre[axis] + t * (self.base[axis] - self.centre[axis])
            }),
            axis: self.axis,
            radius,
        }
    }

    /// The clearance, in mm, between the seat plane and the nearest point of
    /// `section`'s ring. Negative is on the model side of the plane.
    fn plane_clearance(&self, section: &Section) -> f64 {
        let distance = geom_kernel::vec3::dot(
            geom_kernel::vec3::sub(section.centre, self.plane_point),
            self.normal,
        );
        distance - section.radius * self.spread
    }

    /// [`Self::plane_clearance`] of [`Self::section_at`].
    #[cfg(test)]
    fn clearance_at(&self, plate_z_mm: f64, layer_height_mm: f64, slot: usize, radius: f64) -> f64 {
        self.plane_clearance(&self.section_at(plate_z_mm, layer_height_mm, slot, radius))
    }
}

/// Sides of the polygon a section's ring is measured as, against the model.
///
/// **A resolution, not a threshold.** The polygon CIRCUMSCRIBES the ring (its
/// vertices at `R / cos(π/N)`), so the FILLED polygon covers the whole disc
/// and a disc that comes within the clearance of the model always has a
/// filled polygon that does too (measured filled: [`sections_near_model`]).
/// The only error is toward "near": the polygon stands at most
/// `R·(sec(π/16) − 1) = 0.0196·R` beyond the ring, under 0.02 mm at the
/// largest radius the registry allows (1 mm). It can only put a join INTO the
/// seat zone, never let one out.
const SECTION_POLYGON_SIDES: usize = 16;

/// What [`sections_near_model`] found: a verdict per section, and its
/// path-hit witnesses.
struct SectionsNear {
    near: Vec<bool>,
    /// Rim-edge and spoke rows measured against the model.
    rows: usize,
    /// Model-edge rows measured against the filled polygons (the interior
    /// term, round-3 review fix, 2026-09-29).
    interior_rows: usize,
    /// Sections found near ONLY by the interior term: every rim edge and
    /// spoke cleared, the centre was outside, and a model edge still came
    /// within the clearance of the filled polygon.
    near_by_interior: usize,
}

/// Whether each section's FILLED DISC comes within `clearance_mm` of the
/// model, measured on the filled circumscribed polygon, which covers the disc.
///
/// The distance between the filled polygon `P` and a model triangle `T` is
/// attained on an edge of one of them against the other, or it is zero where
/// they meet (and then an edge of one meets the other). So:
///
/// 1. `P`'s edges against the model: the rim edges and, harmlessly, the spokes
///    (the fan triangles' edges), as zero-radius segments, by the kernel's
///    clearance predicate in ONE batch;
/// 2. the model's edges against `P`: every model face within the rim's reach
///    plus the clearance of the centre, in ONE kernel band query, and each of
///    their edges against the sections' fan triangles, built into one small
///    surface, in ONE more. A model feature between two spokes (the round-3
///    review's post, its foot 0.29 mm over the disc and ≥ 0.302 mm from every
///    spoke and rim edge) is seen here and nowhere else;
/// 3. a disc wholly inside the model touches no surface: the centre, by the
///    kernel's inside test.
///
/// A row the predicate did not measure counts as near (the safe side), and so
/// does a section whose ring has no direction, and every section a failed
/// batch covered; a near-parallel bound in step 1 is read as the distance (see
/// the loop), and one in step 2 marks every section within reach of that
/// model edge near.
fn sections_near_model(
    surface: &NativeSurface,
    model_verts: &[[f64; 3]],
    model_tris: &[[u32; 3]],
    sections: &[Section],
    clearance_mm: f64,
) -> SectionsNear {
    if sections.is_empty() {
        return SectionsNear {
            near: Vec::new(),
            rows: 0,
            interior_rows: 0,
            near_by_interior: 0,
        };
    }
    let sides = SECTION_POLYGON_SIDES;
    #[expect(
        clippy::cast_precision_loss,
        reason = "a polygon side count of 16 is exact in f64"
    )]
    let half_angle = std::f64::consts::PI / sides as f64;
    let mut near = vec![false; sections.len()];
    let mut starts = Vec::with_capacity(sections.len() * 2 * sides);
    let mut ends = Vec::with_capacity(sections.len() * 2 * sides);
    let mut owner = Vec::with_capacity(sections.len() * 2 * sides);
    // Each section's filled polygon as a fan of `sides` triangles about its
    // centre: `(centre, v_i, v_{i+1})`, in section order (step 2).
    let mut fans: Vec<Option<Vec<[f64; 3]>>> = vec![None; sections.len()];
    for (index, section) in sections.iter().enumerate() {
        let reference = if section.axis[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let Some(e1) =
            geom_kernel::vec3::normalize(geom_kernel::vec3::cross(section.axis, reference))
        else {
            near[index] = true;
            continue;
        };
        let e2 = geom_kernel::vec3::cross(section.axis, e1);
        let reach = section.radius / half_angle.cos();
        let vertices: Vec<[f64; 3]> = (0..sides)
            .map(|step| {
                #[expect(clippy::cast_precision_loss, reason = "a vertex index below 16")]
                let angle = 2.0 * half_angle * step as f64;
                geom_kernel::vec3::add(
                    section.centre,
                    geom_kernel::vec3::add(
                        geom_kernel::vec3::scale(e1, reach * angle.cos()),
                        geom_kernel::vec3::scale(e2, reach * angle.sin()),
                    ),
                )
            })
            .collect();
        for step in 0..sides {
            starts.push(vertices[step]);
            ends.push(vertices[(step + 1) % sides]);
            owner.push(index);
            starts.push(section.centre);
            ends.push(vertices[step]);
            owner.push(index);
        }
        fans[index] = Some(vertices);
    }
    let rows = starts.len();
    let zero = vec![0.0; rows];
    match surface.segment_clearance_classified(&starts, &ends, &zero, &zero, 0.0, &zero, &zero) {
        Ok(found) => {
            for (row, &index) in found.into_iter().zip(&owner) {
                // AN UPPER-BOUND ROW IS DECIDED HERE, AND WHY THAT IS SAFE. The
                // kernel flags a row when some face's pierce test was
                // near-parallel (|det| at most `DET_EPS`, 1e-14) and keeps the
                // endpoint and edge distances as a one-sided bound. For a
                // ZERO-radius segment the bound is tight: a segment that pierces
                // a face while lying within that angle of its plane stays within
                // `length × sin ε` (under 1e-9 mm here) of the plane, so either
                // an endpoint lies over the face (the endpoint distance reads
                // ~0) or the segment crosses one of its edges in projection (the
                // edge distance reads ~0). Measured on a flat slab: horizontal
                // spokes 0.5 mm under it came back as bounds of exactly 0.5 mm,
                // flagged by the slab's far SIDE faces, which share the BVH leaf.
                // An unmeasured row stays near (the safe side).
                let gap = row.clearance_mm().or_else(|| row.upper_bound_mm());
                if gap.is_none_or(|gap| gap < clearance_mm - 1.0e-9) {
                    near[index] = true;
                }
            }
        }
        Err(_) => near.iter_mut().for_each(|flag| *flag = true),
    }
    let centres: Vec<[f64; 3]> = sections.iter().map(|section| section.centre).collect();
    for (flag, inside) in near.iter_mut().zip(surface.inside(&centres)) {
        *flag |= inside;
    }
    let before_interior = near.clone();
    let interior_rows = model_edges_near_fans(
        surface,
        model_verts,
        model_tris,
        sections,
        &fans,
        half_angle,
        clearance_mm,
        &mut near,
    );
    let near_by_interior = near
        .iter()
        .zip(&before_interior)
        .filter(|&(&after, &before)| after && !before)
        .count();
    SectionsNear {
        near,
        rows,
        interior_rows,
        near_by_interior,
    }
}

/// Step 2 of [`sections_near_model`]: the MODEL's edges against the filled
/// polygons of the sections not yet near. Marks a section near when a model
/// edge comes within `clearance_mm` of its fan; returns the model-edge rows
/// measured. Every failure marks the sections it covers near (the safe side).
#[expect(
    clippy::too_many_arguments,
    reason = "the model (surface and its indexed source), the sections with their fans and \
              polygon angle, the clearance, and the verdicts it updates"
)]
fn model_edges_near_fans(
    surface: &NativeSurface,
    model_verts: &[[f64; 3]],
    model_tris: &[[u32; 3]],
    sections: &[Section],
    fans: &[Option<Vec<[f64; 3]>>],
    half_angle: f64,
    clearance_mm: f64,
    near: &mut [bool],
) -> usize {
    let pending: Vec<usize> = (0..sections.len())
        .filter(|&index| !near[index] && fans[index].is_some())
        .collect();
    if pending.is_empty() {
        return 0;
    }
    // Each pending section's reach: its polygon lies within `R / cos(π/N)` of
    // its centre, so a model face within the clearance of the polygon lies
    // within that reach plus the clearance of the centre.
    let reach_of = |index: usize| sections[index].radius / half_angle.cos() + clearance_mm;
    let mark_pending = |near: &mut [bool]| pending.iter().for_each(|&index| near[index] = true);

    // (a) The model faces near each pending section, in ONE band query (a
    // zero-length segment at the centre: a ball).
    let centres: Vec<[f64; 3]> = pending
        .iter()
        .map(|&index| sections[index].centre)
        .collect();
    let within: Vec<f64> = pending.iter().map(|&index| reach_of(index)).collect();
    let limits = geom_kernel::NearFaceBatchLimits {
        max_rows: pending.len(),
        max_faces_per_row: surface.nfaces(),
        max_total_faces: usize::MAX,
    };
    let Ok(band) =
        surface.faces_near_segments_batch_classified(&centres, &centres, &within, limits)
    else {
        mark_pending(near);
        return 0;
    };
    // Their edges, once each (by vertex pair).
    let mut edges: Vec<(u32, u32)> = band
        .rows
        .iter()
        .flatten()
        .filter_map(|&face| model_tris.get(face as usize))
        .flat_map(|&[a, b, c]| [(a, b), (b, c), (c, a)])
        .map(|(a, b)| (a.min(b), a.max(b)))
        .collect();
    edges.sort_unstable();
    edges.dedup();
    let mut edge_starts = Vec::with_capacity(edges.len());
    let mut edge_ends = Vec::with_capacity(edges.len());
    for &(a, b) in &edges {
        let (Some(&start), Some(&end)) = (model_verts.get(a as usize), model_verts.get(b as usize))
        else {
            mark_pending(near);
            return 0;
        };
        edge_starts.push(start);
        edge_ends.push(end);
    }
    if edges.is_empty() {
        return 0;
    }

    // (b) The pending sections' fans as ONE small surface, face `f` owned by
    // section `fan_owner[f]`.
    let sides = SECTION_POLYGON_SIDES;
    let mut fan_verts: Vec<[f64; 3]> = Vec::with_capacity(pending.len() * (sides + 1));
    let mut fan_tris: Vec<[u32; 3]> = Vec::with_capacity(pending.len() * sides);
    let mut fan_owner: Vec<usize> = Vec::with_capacity(pending.len() * sides);
    for &index in &pending {
        let Some(vertices) = fans[index].as_ref() else {
            continue;
        };
        let Ok(base) = u32::try_from(fan_verts.len()) else {
            mark_pending(near);
            return 0;
        };
        fan_verts.push(sections[index].centre);
        fan_verts.extend_from_slice(vertices);
        for step in 0..sides {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a polygon vertex index below 16"
            )]
            let (one, two) = (step as u32 + 1, ((step + 1) % sides) as u32 + 1);
            fan_tris.push([base, base + one, base + two]);
            fan_owner.push(index);
        }
    }
    let pitch = surface.config().grid_pitch_mm;
    let fan_surface =
        NativeSurface::estimate_index_memory_from_indexed(&fan_verts, &fan_tris, pitch)
            .ok()
            .and_then(|estimate| {
                NativeSurface::from_indexed(
                    &fan_verts,
                    &fan_tris,
                    NativeSurfaceConfig {
                        grid_pitch_mm: pitch,
                        max_grid_bytes: estimate.max_axis_grid_bytes(),
                    },
                )
                .ok()
            });
    let Some(fan_surface) = fan_surface else {
        mark_pending(near);
        return 0;
    };

    // (c) Every model edge against every fan, in ONE band query at the
    // clearance: a fan face in an edge's band puts its section near.
    let band_mm = vec![(clearance_mm - 1.0e-9).max(0.0); edges.len()];
    let limits = geom_kernel::NearFaceBatchLimits {
        max_rows: edges.len(),
        max_faces_per_row: fan_tris.len(),
        max_total_faces: usize::MAX,
    };
    let Ok(found) = fan_surface.faces_near_segments_batch_classified(
        &edge_starts,
        &edge_ends,
        &band_mm,
        limits,
    ) else {
        mark_pending(near);
        return edges.len();
    };
    for (row, faces) in found.rows.iter().enumerate() {
        for &face in faces {
            if let Some(&index) = fan_owner.get(face as usize) {
                near[index] = true;
            }
        }
        // A face whose pierce the kernel could not decide is left out of the
        // band (its membership is proof-only); here an unproven pierce must
        // count as near, so every pending section within reach of this edge
        // is marked.
        if found.near_parallel.get(row).is_some_and(|&count| count > 0) {
            for &index in &pending {
                let centre = sections[index].centre;
                let (distance, _, _) = geom_kernel::segment_segment_distance_min(
                    edge_starts[row],
                    edge_ends[row],
                    centre,
                    centre,
                );
                if distance <= reach_of(index) {
                    near[index] = true;
                }
            }
        }
    }
    edges.len()
}

/// The slot of `host` a member landed at `layer` is booked into
/// (`down_sweep::run`): its landing layer, except that a load reaching the
/// CONTACT slot (a landing at the seat layer or the top alive layer) is booked
/// one slot under the contact where there is one. It is an absolute LAYER.
/// The load-step taper starts its cones here too
/// ([`crate::adapter::load_step_cones`]), so both read one booking.
pub(crate) fn booked_slot(host: &SweepStrut, layer: usize) -> usize {
    let contact = host.k_top.saturating_sub(1);
    let under_contact = if contact > host.k_bottom {
        contact - 1
    } else {
        contact
    };
    // A HEADED host's head span is not a place a load enters (v19 fix,
    // 2026-10-08): booked at the joint's layer at the highest.
    host.joint_hold_layer()
        .map_or(layer.min(under_contact), |joint| layer.min(under_contact).min(joint))
}

/// Every join on a support arm in `struts`: a member landed on a support arm,
/// at ANY layer. Which of them reaches a seat zone is geometry ([`grade`]),
/// not the landing layer: a lander one or three layers under a leaned head is
/// booked into the same slot as one at the head (Round 50 at its committed
/// flip: arms 66, 68, 83, 98, their sums across the seat plane by 0.33 to
/// 0.48 mm on `cb28bdd`, which looked at head landings only).
fn arm_joins(struts: &[SweepStrut], layer_height_mm: f64) -> Vec<Join> {
    let mut joins = Vec::new();
    for (child, strut) in struts.iter().enumerate() {
        let Landing::Strut { id, layer } = strut.landing else {
            continue;
        };
        let parent = id as usize;
        let Some(host) = struts.get(parent) else {
            continue;
        };
        if parent == child
            || host.id != id
            || host.kind != StrutKind::Arm
            || host.k_bottom == usize::MAX
            || host.k_top <= host.k_bottom
            || layer < host.k_bottom
        {
            continue;
        }
        // A lander ABOVE a headed host's joinable top stands in its head span
        // or on its joint sphere: a load that enters there does not pass
        // through the contact (v19 fix, 2026-10-08, ruling 2c). Such a join is
        // always re-landed at the joint or below when a join exists
        // ([`candidate_joins`]), whatever the seat plane says, and listed
        // when it cannot be. A head given up (`drawn_straight`) is a straight
        // arm again and joinable anywhere.
        let in_head_span = host.head.as_ref().is_some_and(|head| !head.drawn_straight)
            && host
                .joinable_top_layer(layer_height_mm)
                .is_none_or(|top| layer > top);
        joins.push(Join {
            child,
            parent,
            layer,
            booked: booked_slot(host, layer),
            in_head_span,
        });
    }
    joins
}

/// Indices, once each, in index order.
fn sorted_unique(mut indices: Vec<usize>) -> Vec<usize> {
    indices.sort_unstable();
    indices.dedup();
    indices
}

/// Each listed member's measured tip from its lowering input, by ONE batched
/// contact-disc call, scattered to strut index.
fn heads_of(
    count: usize,
    members: &[usize],
    arms: &[SweepArm],
    layer_plan: &LayerPlan,
) -> Vec<Option<Head>> {
    let mut heads = vec![None; count];
    let Ok(discs) = plinth_support::tip_contact_discs(arms, layer_plan) else {
        return heads;
    };
    for ((&index, arm), disc) in members.iter().zip(arms).zip(&discs) {
        let Some(disc) = disc else {
            continue;
        };
        // The shaft starts at the ball centre, or at the joint `K` of a headed
        // arm, whose neck and joint sphere are the contact.
        let centre = disc.post_top_mm;
        let base = arm.base_mm;
        if centre[2] - base[2] <= 0.0 {
            continue;
        }
        let Some(axis) = geom_kernel::vec3::normalize(geom_kernel::vec3::sub(base, centre)) else {
            continue;
        };
        let along = geom_kernel::vec3::dot(disc.outward_normal, axis);
        heads[index] = Some(Head {
            centre,
            base,
            axis,
            plane_point: disc.centre_mm,
            normal: disc.outward_normal,
            spread: (1.0 - along * along).max(0.0).sqrt(),
            seat: arm.seat_mm,
            contact_reach_mm: geom_kernel::vec3::norm(geom_kernel::vec3::sub(centre, arm.seat_mm))
                + disc.head_joint_radius_mm.unwrap_or(disc.ball_radius_mm),
        });
    }
    heads
}

/// Measure the tips of `members` on the LIVE field: each one's seat face now
/// (a seat does not move before the footprint rule), its arm built unshifted
/// with its drawn profile, and one contact-disc batch.
fn measure(
    context: &HeadJoinContext<'_>,
    struts: &[SweepStrut],
    profiles: &[Vec<f64>],
    members: &[usize],
) -> Vec<Option<Head>> {
    let arms = measured_arms(context, struts, profiles, members);
    heads_of(struts.len(), members, &arms, context.layer_plan)
}

/// The arms of `members` as the lowering will get them on the LIVE field: each
/// one's seat face now, its arm built unshifted with its drawn profile
/// (`profiles` is indexed by strut). One seat-face query, one footprint
/// normal batch. The graze census reads these too ([`crate::graze`]).
pub(crate) fn measured_arms(
    context: &HeadJoinContext<'_>,
    struts: &[SweepStrut],
    profiles: &[Vec<f64>],
    members: &[usize],
) -> Vec<SweepArm> {
    let grid = context.slender.grid;
    let height = context.slender.layer_height_mm;
    let mut tips: Vec<SweepStrut> = members.iter().map(|&index| struts[index].clone()).collect();
    let (mut faces, seat_triangles) = crate::tip_landing::tip_seat_faces_with_ids(
        context.mesh,
        grid,
        &tips,
        context.plate_z_mm,
        height,
        context.slender.seat_faces,
    );
    // The seat normal is the footprint's average, here as in the finished plan
    // (`footprint_normal`, Craig ruling 2026-10-05).
    crate::footprint_normal::refine(
        &crate::footprint_normal::FootprintEnv {
            mesh: context.mesh,
            surface: context.surface(),
            grid,
            plate_z_mm: context.plate_z_mm,
            layer_height_mm: height,
            layer_plan: context.layer_plan,
            rule: context.slender.seat_faces,
        },
        &tips,
        &mut faces,
        &seat_triangles,
    );
    tips.iter_mut()
        .zip(&faces)
        .zip(members)
        .map(|((tip, face), &index)| {
            tip.seat_normal = face.map_or([0.0; 3], |(normal, _)| normal);
            let (seat, base) = crate::adapter::drawn_ends(grid, context.plate_z_mm, height, tip);
            crate::adapter::sweep_arm(
                tip,
                seat,
                base,
                face.map(|(_, point)| point),
                profiles[index].clone(),
            )
        })
        .collect()
}

/// What one survey found.
struct Survey {
    examined: usize,
    unmeasured: usize,
    /// Joins whose sum-sized section is within the clearance of the seat
    /// plane but clear of the model: the plane is not the model there.
    off_the_model: usize,
    /// Section-against-model rows the survey measured.
    model_rows: usize,
    /// Model-edge rows measured against the filled sections, and the
    /// sections only that interior term found near.
    interior_rows: usize,
    near_by_interior: usize,
    /// Load-step cone layers graded above joins whose booked section cleared
    /// the plane, and the joins the plane held by their cone alone
    /// ([`crate::adapter::LoadStepTaper`]): path-hit witnesses.
    cone_sections_graded: usize,
    held_by_cone: usize,
    /// Joins that stand in a headed host's head span or on its joint sphere,
    /// listed in the zone without a geometry test.
    in_head_span: usize,
    in_zone: Vec<Join>,
    heads: Vec<Option<Head>>,
    profiles: Vec<Vec<f64>>,
}

/// Grade `joins` on the given drawn profiles (indexed by strut) and heads.
///
/// **A join is in its host's seat zone when the section that carries its sum
/// comes within the clearance of the host's seat plane AND of the model
/// itself.** The plane is the model only near the seat face; it is infinite,
/// and under a steep face it passes below the model's lowest point (Round 25
/// arm 6, face 75° from horizontal: its lander's sum-sized section, 1.25 mm
/// under the seat, crosses the plane with no model within reach). The model
/// test runs only on the joins the plane test holds, in one batch
/// ([`sections_near_model`]); with no model surface they stay in the zone.
fn grade(
    context: &HeadJoinContext<'_>,
    struts: &[SweepStrut],
    joins: Vec<Join>,
    heads: Vec<Option<Head>>,
    profiles: Vec<Vec<f64>>,
) -> Survey {
    let mut out = Survey {
        examined: joins.len(),
        unmeasured: 0,
        off_the_model: 0,
        model_rows: 0,
        interior_rows: 0,
        near_by_interior: 0,
        cone_sections_graded: 0,
        held_by_cone: 0,
        in_head_span: 0,
        in_zone: Vec::new(),
        heads,
        profiles,
    };
    let height = context.slender.layer_height_mm;
    // THE LOAD-STEP CONE (fix/tapered-load-steps, 2026-10-02): above a join the
    // drawn shaft narrows to the host's own section over a few layers instead
    // of in one, so it stands nearer the seat than the booked section does.
    // Its sections are graded by the same test, keyed by (host, booked layer).
    let cones = crate::adapter::load_step_cones(struts, context.slender.load_step_taper);
    let mut held: Vec<(Join, Section)> = Vec::new();
    for join in joins {
        // A lander in a headed host's head span is in the zone by definition
        // (v19 fix, 2026-10-08): its load must enter at the joint or below.
        if join.in_head_span {
            out.in_head_span += 1;
            out.in_zone.push(join);
            continue;
        }
        let host = &struts[join.parent];
        let Some(head) = out.heads.get(join.parent).copied().flatten() else {
            out.unmeasured += 1;
            continue;
        };
        let Some(&radius) =
            out.profiles[join.parent].get(join.booked.saturating_sub(host.k_bottom))
        else {
            out.unmeasured += 1;
            continue;
        };
        // Only a section WIDER than the arm's own contact can make the contact
        // more than own-load sized; a lander whose load does not widen the
        // shaft leaves nothing in the seat zone but the arm's own section.
        let own = host.tip_radius_mm();
        if radius <= own * (1.0 + 1.0e-9) {
            continue;
        }
        let section = head.section_at(context.plate_z_mm, height, join.booked, radius);
        if head.plane_clearance(&section) < context.clearance_mm - 1.0e-9 {
            held.push((join, section));
            continue;
        }
        // The booked section clears the plane; the cone above it may not. The
        // cone section nearest the plane is the one carried on.
        let cone_top = cones
            .binary_search_by(|cone| (cone.host, cone.booked).cmp(&(join.parent, join.booked)))
            .ok()
            .map(|at| cones[at].top);
        let Some(cone_top) = cone_top else {
            continue;
        };
        let nearest = (join.booked + 1..=cone_top)
            .filter_map(|layer| {
                let radius = *out.profiles[join.parent].get(layer - host.k_bottom)?;
                (radius > own * (1.0 + 1.0e-9))
                    .then(|| head.section_at(context.plate_z_mm, height, layer, radius))
            })
            .map(|section| (head.plane_clearance(&section), section))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        out.cone_sections_graded += cone_top - join.booked;
        if let Some((clearance, section)) = nearest {
            if clearance < context.clearance_mm - 1.0e-9 {
                out.held_by_cone += 1;
                held.push((join, section));
            }
        }
    }
    if held.is_empty() {
        return out;
    }
    let Some(surface) = context.surface() else {
        out.in_zone.extend(held.into_iter().map(|(join, _)| join));
        return out;
    };
    let sections: Vec<Section> = held.iter().map(|(_, section)| *section).collect();
    let found = sections_near_model(
        surface,
        context.mesh.verts(),
        context.mesh.tris(),
        &sections,
        context.clearance_mm,
    );
    out.model_rows = found.rows;
    out.interior_rows = found.interior_rows;
    out.near_by_interior = found.near_by_interior;
    for ((join, _), near) in held.into_iter().zip(found.near) {
        if near {
            out.in_zone.push(join);
        } else {
            out.off_the_model += 1;
        }
    }
    out
}

/// Survey the LIVE field inside the sweep.
fn survey(context: &HeadJoinContext<'_>, struts: &[SweepStrut]) -> Survey {
    let joins = arm_joins(struts, context.slender.layer_height_mm);
    if joins.is_empty() {
        return grade(context, struts, joins, Vec::new(), Vec::new());
    }
    let (profiles, _) = crate::adapter::drawn_profiles(struts, context.slender.load_step_taper);
    let parents = sorted_unique(joins.iter().map(|join| join.parent).collect());
    let heads = measure(context, struts, &profiles, &parents);
    grade(context, struts, joins, heads, profiles)
}

/// What the emitted field's [`census`] found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Census {
    /// Joins on support arms.
    pub examined: usize,
    /// Every one whose sum-sized section (or the load-step cone above it)
    /// still reaches its host's seat zone, as `(lander id, trunk id)`.
    pub listed: Vec<(u32, u32)>,
    /// Of those, the ones the filled-section interior term alone put there.
    pub near_by_interior: usize,
    /// Load-step cone layers graded, and joins held by their cone alone.
    pub cone_sections_graded: usize,
    pub held_by_cone: usize,
}

/// **The emitted field's census**: how many joins on support arms it has, and
/// every one whose sum-sized section still reaches its host's seat zone, as
/// `(lander id, trunk id)`, measured on the arms exactly as they go to the
/// lowering (`adapter::arms_of`, footprint moves included); and how many of
/// those the filled-section interior term alone put in the zone.
#[must_use]
pub(crate) fn census(
    context: &HeadJoinContext<'_>,
    struts: &[SweepStrut],
    arms: &[SweepArm],
) -> Census {
    let joins = arm_joins(struts, context.slender.layer_height_mm);
    let examined = joins.len();
    if joins.is_empty() || arms.len() != struts.len() {
        return Census {
            examined,
            ..Census::default()
        };
    }
    let parents = sorted_unique(joins.iter().map(|join| join.parent).collect());
    let trunk_arms: Vec<SweepArm> = parents.iter().map(|&index| arms[index].clone()).collect();
    let heads = heads_of(struts.len(), &parents, &trunk_arms, context.layer_plan);
    let profiles = arms
        .iter()
        .map(|arm| arm.radius_profile_mm.clone())
        .collect();
    let graded = grade(context, struts, joins, heads, profiles);
    let listed = graded
        .in_zone
        .iter()
        .map(|join| (struts[join.child].id, struts[join.parent].id))
        .collect();
    Census {
        examined,
        listed,
        near_by_interior: graded.near_by_interior,
        cone_sections_graded: graded.cone_sections_graded,
        held_by_cone: graded.held_by_cone,
    }
}

/// Why a join could not be moved.
enum Refused {
    NoClearJoin,
    CarryingTie,
    /// The lander, or a member resting on it, is headed: never moved.
    Headed,
}

/// One tentative move: `(strut index, landing, k_bottom, path)`.
type Move = (usize, Landing, usize, Vec<u32>);

/// The point a member's drawn base sits at when it lands in `cell` at `layer`
/// (`adapter::drawn_ends`: the cell's centre, at the layer's bottom face).
#[expect(
    clippy::cast_precision_loss,
    reason = "a layer index bounded by the plan's own layer count is exact in f64"
)]
pub(crate) fn base_point(context: &HeadJoinContext<'_>, cell: usize, layer: usize) -> [f64; 3] {
    let grid = context.slender.grid;
    [
        grid.center_x_mm(grid.col_of(cell)),
        grid.center_y_mm(grid.row_of(cell)),
        context.plate_z_mm + layer as f64 * context.slender.layer_height_mm,
    ]
}

/// **A join stands on the host's solid, above the raft top**
/// (fix/round100-emit, 2026-09-29; the raft top since fix/round11-milestone,
/// 2026-10-02).
///
/// A member is drawn straight from its tip ball's centre `centre` to its base
/// point `base`, and its base is a disc of radius `radius` NORMAL to that
/// axis. On a leaned member the disc tilts, and its lowest point stands
/// `radius · √(1 − a_z²)` under `base` (`a_z` the axis's vertical component).
/// A join is admissible only where that lowest point is at or above
/// `floor_z_mm`, which callers take from [`join_floor_z_mm`]: the RAFT TOP.
///
/// The defects it answers: on Round_100 at its committed flip the rule
/// re-landed lander 536 on plate arm 40 AT LAYER 0, on the plate itself; its
/// end disc dipped 0.2335 mm under the plate, and from `15e594d` the whole file
/// was refused. `candidate_joins` walks every host down to its own bottom
/// layer, and a plate arm's bottom layer is the plate. Since the sweep emits a
/// raft (fix/sweep-raft, 2026-09-29) the plate is no longer the floor: under
/// every plate root stands a pad up to the raft top, and only a FOOT ends in
/// it. Judged against the plate, the round-11 milestone (`f19ba85`) found the
/// head-join rule moving a lander on the procedural Round 25 down plate post
/// 9 to layer 7 (0.35 mm up, inside the post's root flare), and its emitted
/// end disc reached 0.087 mm below the raft top, through the pad
/// (`plate_contact::every_plate_root_of_round_25_stands_on_a_raft_pad_that_covers_its_foot`).
///
/// `radius` is the member's widest drawn radius, as test 2 uses. No new
/// constant: the raft height, the drawn axis and the drawn radius only.
pub(crate) fn stays_above_raft(
    centre: [f64; 3],
    base: [f64; 3],
    radius: f64,
    floor_z_mm: f64,
) -> bool {
    let reach_below = geom_kernel::vec3::normalize(geom_kernel::vec3::sub(base, centre))
        .map_or(radius, |axis| {
            radius * (1.0 - axis[2] * axis[2]).max(0.0).sqrt()
        });
    base[2] - reach_below >= floor_z_mm
}

/// **The floor every join's base disc stands on: the raft top** over a plate
/// at `plate_z_mm`, read through the lowering's own door
/// ([`plinth_support::SweepRootFlare::raft_top_above_plate_mm`], the height
/// the pads are drawn to), so the rule and the file cannot disagree. Nothing
/// but a raft record and a foot ending in its pad reaches below it, the same
/// floor the tie planner keeps every brace above
/// (`slenderness::free_shaft_floor`). An unreadable registry leaves the plate,
/// as before the raft: nothing is refused on a missing number.
pub(crate) fn join_floor_z_mm(plate_z_mm: f64, layer_height_mm: f64) -> f64 {
    plate_z_mm
        + plinth_support::SweepRootFlare::registered(layer_height_mm)
            .map_or(0.0, |flare| flare.raft_top_above_plate_mm())
}

/// How far down a member's axis `axis` (unit, seat to base) its own seat zone
/// reaches: its tip ball, or, where further, the point where the member's
/// solid first stands `tier` mm clear of its OWN seat plane. The kernel's
/// clearance treats the solid as a capsule (the axis's distance to the
/// surface less the radius), so that point is where the axis stands
/// `tier + r` from the plane: `(tier + r) / (n·m)`. The zone test 1 grants a
/// head arm's own section, granted to the member's own seat. `None` when the
/// axis does not leave the seat plane at all.
fn own_seat_zone_mm(head: &Head, axis: [f64; 3], radius: f64, tier: f64) -> Option<f64> {
    let along = geom_kernel::vec3::dot(head.normal, axis);
    if along <= 0.0 {
        return None;
    }
    Some(head.contact_reach_mm.max((tier + radius) / along))
}

/// Test 2 over a batch of segments of one member, from its drawn seat to each
/// of `bases`, at its widest drawn radius, with its own seat zone untested
/// ([`own_seat_zone_mm`]): the clearance in mm for each, `None` where the
/// predicate did not decide or nothing is left to test.
fn solid_clearances(
    surface: &NativeSurface,
    head: &Head,
    radius: f64,
    bases: &[[f64; 3]],
    tier: f64,
) -> Vec<Option<f64>> {
    let n = bases.len();
    let seats = vec![head.seat; n];
    let radii = vec![radius; n];
    let zero = vec![0.0; n];
    let insets: Vec<f64> = bases
        .iter()
        .map(|&base| {
            geom_kernel::vec3::normalize(geom_kernel::vec3::sub(base, head.seat))
                .and_then(|axis| own_seat_zone_mm(head, axis, radius, tier))
                .unwrap_or(f64::INFINITY)
        })
        .collect();
    surface
        .segment_clearance_classified(&seats, bases, &radii, &radii, 0.0, &insets, &zero)
        .map_or_else(
            |_| vec![None; n],
            |rows| {
                rows.into_iter()
                    .map(SegmentClearanceClassified::clearance_mm)
                    .collect()
            },
        )
}

/// The segment `strut` would take from its own seat down to `(base_cell,
/// k_bottom)`, under the lean law and `segment_path`'s refusals, or `None`.
fn relanded_path(
    context: &HeadJoinContext<'_>,
    index: &ColumnHistory,
    strut: &SweepStrut,
    base_cell: usize,
    k_bottom: usize,
    host_id: u32,
) -> Option<Vec<u32>> {
    let slender = context.slender;
    let layers = strut.k_top.checked_sub(k_bottom)?;
    if layers == 0 {
        return None;
    }
    let lateral_mm = crate::cell_distance_mm(slender.grid, strut.cell, base_cell);
    if lateral_mm > 0.0 && slender.lean.descent_cost_mm(layers, lateral_mm).is_none() {
        return None;
    }
    segment_path(
        slender,
        index,
        strut.cell,
        strut.k_top,
        base_cell,
        k_bottom,
        Some(host_id),
    )
}

/// The cell a member stands in at `layer` once it has `path` from `k_bottom`.
fn cell_on(strut: &SweepStrut, path: &[u32], k_bottom: usize, layer: usize) -> Option<usize> {
    if layer == strut.k_top {
        return Some(strut.cell);
    }
    let slot = layer.checked_sub(k_bottom)?;
    path.get(slot).map(|cell| *cell as usize)
}

/// What the move planner reads besides the join itself.
struct Planner<'a, 'c> {
    context: &'a HeadJoinContext<'c>,
    index: &'a ColumnHistory,
    struts: &'a [SweepStrut],
    dependants: &'a [Vec<usize>],
    tied: &'a [bool],
    heads: &'a [Option<Head>],
    profiles: &'a [Vec<f64>],
    surface: &'a NativeSurface,
    /// The raft top ([`join_floor_z_mm`]) a dependant's base disc keeps above.
    floor_z_mm: f64,
    /// Dependant re-landings refused by the plate rule, counted here and
    /// added to [`HeadJoinOutcome::candidates_below_plate`].
    below_plate: std::cell::Cell<usize>,
}

impl Planner<'_, '_> {
    fn widest(&self, member: usize) -> f64 {
        self.profiles[member]
            .iter()
            .copied()
            .fold(self.struts[member].tip_radius_mm(), f64::max)
    }

    /// Plan the lander's move onto `host` at `layer`, and every member resting
    /// on it re-landed on its new axis, transitively, each no nearer the model
    /// than it was, up to `tier` mm.
    fn plan(
        &self,
        child: usize,
        host: usize,
        layer: usize,
        tier: f64,
    ) -> Result<Vec<Move>, Refused> {
        let struts = self.struts;
        // A HEADED lander is never moved (its head is frozen at placement).
        if struts[child].head.is_some() {
            return Err(Refused::Headed);
        }
        let base_cell = struts[host].cell_at(layer);
        let path = relanded_path(
            self.context,
            self.index,
            &struts[child],
            base_cell,
            layer,
            struts[host].id,
        )
        .ok_or(Refused::NoClearJoin)?;
        let mut moves: Vec<Move> = vec![(
            child,
            Landing::Strut {
                id: struts[host].id,
                layer,
            },
            layer,
            path,
        )];
        let mut visited = vec![false; struts.len()];
        visited[child] = true;
        let mut cursor = 0;
        while cursor < moves.len() {
            let moved = moves[cursor].0;
            let moved_bottom = moves[cursor].2;
            let moved_path = moves[cursor].3.clone();
            cursor += 1;
            if self.tied[moved] {
                return Err(Refused::CarryingTie);
            }
            for &dependant in &self.dependants[moved] {
                let strut = &struts[dependant];
                // A cross (or a mast) resting on a member that must move was
                // placed for the geometry it has; it is not re-planned here.
                if strut.kind != StrutKind::Arm {
                    return Err(Refused::CarryingTie);
                }
                if visited[dependant] {
                    continue;
                }
                let Landing::Strut { layer: at, .. } = strut.landing else {
                    continue;
                };
                if cell_on(&struts[moved], &moved_path, moved_bottom, at) == Some(strut.base_cell())
                {
                    continue;
                }
                // A headed dependant would have to be re-landed: never.
                if strut.head.is_some() {
                    return Err(Refused::Headed);
                }
                // THE DEPENDANT FOLLOWS THE MEMBER IT RESTS ON: re-landed on its
                // new axis at its own landing layer, or, where that breaks the
                // lean law or a refusal, lower down the same member, highest
                // first — each no nearer the model than it was, up to the tier.
                let head = self.heads[dependant].ok_or(Refused::NoClearJoin)?;
                let dependant_widest = self.widest(dependant);
                let mut options: Vec<(usize, Vec<u32>, [f64; 3])> = Vec::new();
                for layer in (moved_bottom..=at).rev() {
                    let Some(cell) = cell_on(&struts[moved], &moved_path, moved_bottom, layer)
                    else {
                        continue;
                    };
                    let base = base_point(self.context, cell, layer);
                    // THE RAFT TOP, for a dependant as for its lander.
                    if !stays_above_raft(head.centre, base, dependant_widest, self.floor_z_mm) {
                        self.below_plate.set(self.below_plate.get() + 1);
                        continue;
                    }
                    if let Some(path) = relanded_path(
                        self.context,
                        self.index,
                        strut,
                        cell,
                        layer,
                        struts[moved].id,
                    ) {
                        options.push((layer, path, base));
                    }
                }
                let mut bases = vec![base_point(self.context, strut.base_cell(), strut.k_bottom)];
                bases.extend(options.iter().map(|option| option.2));
                let found = solid_clearances(self.surface, &head, dependant_widest, &bases, tier);
                let required = found[0].map_or(tier, |before| tier.min(before)) - 1.0e-9;
                let chosen = options
                    .into_iter()
                    .zip(found.into_iter().skip(1))
                    .find(|(_, after)| after.is_some_and(|after| after >= required));
                let Some(((layer, dependant_path, _), _)) = chosen else {
                    return Err(Refused::NoClearJoin);
                };
                visited[dependant] = true;
                moves.push((
                    dependant,
                    Landing::Strut {
                        id: struts[moved].id,
                        layer,
                    },
                    layer,
                    dependant_path,
                ));
            }
        }
        Ok(moves)
    }
}

/// The joins a lander may be moved to, highest first: the head arm's own
/// shaft under the booked slot, then each member below it on its landing
/// chain (support arms only) from the layer the one above lands at.
fn candidate_joins(struts: &[SweepStrut], join: &Join, layer_height_mm: f64) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut host = join.parent;
    let mut top = join.booked;
    let mut hops = 0;
    loop {
        let strut = &struts[host];
        let lowest = strut.k_bottom;
        // A HEADED host is joined only at or below its joinable top
        // (`k_top` for a straight host, so nothing changes there).
        let joinable = strut.joinable_top_layer(layer_height_mm);
        if top > lowest {
            let highest = if host == join.parent { top - 1 } else { top };
            if let Some(cap) = joinable {
                for layer in (lowest..=highest.min(cap)).rev() {
                    out.push((host, layer));
                }
            }
        } else if host != join.parent && top == lowest && joinable.is_some_and(|cap| lowest <= cap) {
            out.push((host, lowest));
        }
        let Landing::Strut { id, layer } = strut.landing else {
            break;
        };
        let next = id as usize;
        hops += 1;
        if hops > struts.len()
            || struts.get(next).is_none_or(|next_strut| {
                next_strut.id != id || next_strut.kind != StrutKind::Arm || next == join.child
            })
        {
            break;
        }
        host = next;
        top = layer;
    }
    out
}

/// One pass of the rule over `struts`: survey, then move every head join whose
/// sum-sized section reaches the seat zone to the highest admissible join.
/// Returns how many landers it moved; the caller re-sizes the field with the
/// down-sweep whenever that is not zero, and runs it again, to a fixed point.
#[expect(
    clippy::too_many_lines,
    reason = "one pass: survey, index, measure, then per join the candidate batch, the tiers \
              and the move; split further it would pass the same dozen locals around"
)]
pub(crate) fn apply(
    context: &HeadJoinContext<'_>,
    struts: &mut [SweepStrut],
    outcome: &mut HeadJoinOutcome,
    first: bool,
) -> usize {
    let found = survey(context, struts);
    // THE RAFT TOP every join keeps its base disc above (`stays_above_raft`),
    // read once for the pass.
    let floor_z_mm = join_floor_z_mm(context.plate_z_mm, context.slender.layer_height_mm);
    if first {
        outcome.joins_examined = found.examined;
        outcome.joins_in_seat_zone_before = found.in_zone.len();
        outcome.joins_in_head_span = found.in_head_span;
        outcome.joins_off_the_model = found.off_the_model;
        outcome.joins_held_by_load_cone = found.held_by_cone;
    }
    outcome.model_rows += found.model_rows;
    outcome.interior_rows += found.interior_rows;
    outcome.sections_near_by_interior += found.near_by_interior;
    outcome.joins_unmeasured = found.unmeasured;
    outcome.kept_no_clear_join = 0;
    outcome.kept_carrying_a_tie = 0;
    outcome.kept_not_an_arm = 0;
    outcome.headed_kept = 0;
    outcome.kept_surface_unavailable = 0;
    if found.in_zone.is_empty() {
        return 0;
    }
    let Some(surface) = context.surface() else {
        outcome.kept_surface_unavailable = found.in_zone.len();
        return 0;
    };
    let height = context.slender.layer_height_mm;
    let n = struts.len();
    let mut dependants: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut tied = vec![false; n];
    for (index, strut) in struts.iter().enumerate() {
        if let Landing::Strut { id, .. } = strut.landing {
            if let Some(list) = dependants.get_mut(id as usize) {
                list.push(index);
            }
        }
        if let Some(host) = strut.seat_on_strut {
            if let Some(flag) = tied.get_mut(host as usize) {
                *flag = true;
            }
        }
    }
    // Every member that may move this pass — the landers in the zone and what
    // rests on them — is left OUT of the obstacle index, so none blocks its
    // own new path; each accepted move is stamped back in as it lands.
    let mut may_move = vec![false; n];
    // A HEADED member never moves, so it stays in the obstacle index.
    let headed = |index: usize| struts[index].head.is_some();
    let mut stack: Vec<usize> = found
        .in_zone
        .iter()
        .map(|join| join.child)
        .filter(|&child| !headed(child))
        .collect();
    while let Some(index) = stack.pop() {
        if std::mem::replace(&mut may_move[index], true) {
            continue;
        }
        stack.extend(dependants[index].iter().copied().filter(|&member| !headed(member)));
    }
    let mut obstacles = ColumnHistory::new(context.slender.grid);
    for (position, strut) in struts.iter().enumerate() {
        if may_move[position] || strut.k_bottom == usize::MAX || strut.k_top < strut.k_bottom {
            continue;
        }
        obstacles.stamp_strut(strut, height);
    }
    // Every member whose tip this pass needs, in ONE measurement: the hosts on
    // every candidate chain (test 1), and the landers and what rests on them
    // (test 2's contact reach).
    let candidates: Vec<Vec<(usize, usize)>> = found
        .in_zone
        .iter()
        .map(|join| candidate_joins(struts, join, height))
        .collect();
    let mut wanted: Vec<usize> = candidates.iter().flatten().map(|&(host, _)| host).collect();
    wanted.extend((0..n).filter(|&index| may_move[index]));
    let wanted = sorted_unique(wanted);
    let mut heads = measure(context, struts, &found.profiles, &wanted);
    for (index, head) in found.heads.iter().enumerate() {
        if head.is_some() && heads[index].is_none() {
            heads[index] = *head;
        }
    }

    // TEST 1 over every candidate of every join: the seat plane by arithmetic,
    // and, for the candidates the plane holds, the model in ONE batch — the
    // same seat zone the survey grades by (`grade`).
    let mut verdicts: Vec<Vec<bool>> = Vec::with_capacity(candidates.len());
    let mut held: Vec<(usize, usize)> = Vec::new();
    let mut held_sections: Vec<Section> = Vec::new();
    for (position, (join, chain)) in found.in_zone.iter().zip(&candidates).enumerate() {
        let lander_top = struts[join.child].k_top;
        let mut row = Vec::with_capacity(chain.len());
        for (slot, &(host, layer)) in chain.iter().enumerate() {
            if layer >= lander_top {
                row.push(false);
                continue;
            }
            let Some(head) = heads[host] else {
                // A host below the head arm whose tip could not be measured:
                // its seat plane is unknown, and the join is not at its head.
                row.push(host != join.parent);
                continue;
            };
            let Some(&radius) = found.profiles[host].get(layer - struts[host].k_bottom) else {
                row.push(false);
                continue;
            };
            let section = head.section_at(context.plate_z_mm, height, layer, radius);
            if head.plane_clearance(&section) >= context.clearance_mm - 1.0e-9 {
                row.push(true);
            } else {
                row.push(false);
                held.push((position, slot));
                held_sections.push(section);
            }
        }
        verdicts.push(row);
    }
    let found_near = sections_near_model(
        surface,
        context.mesh.verts(),
        context.mesh.tris(),
        &held_sections,
        context.clearance_mm,
    );
    outcome.model_rows += found_near.rows;
    outcome.interior_rows += found_near.interior_rows;
    outcome.sections_near_by_interior += found_near.near_by_interior;
    for (&(position, slot), near) in held.iter().zip(found_near.near) {
        if !near {
            verdicts[position][slot] = true;
            outcome.candidates_off_the_model += 1;
        }
    }

    let mut moved_count = 0;
    let mut moved_already = vec![false; n];
    for ((join, chain), verdict) in found.in_zone.iter().zip(&candidates).zip(&verdicts) {
        if moved_already[join.child] {
            continue;
        }
        if struts[join.child].kind != StrutKind::Arm {
            outcome.kept_not_an_arm += 1;
            continue;
        }
        // A HEADED lander is kept where it is.
        if struts[join.child].head.is_some() {
            outcome.headed_kept += 1;
            continue;
        }
        let lander = &struts[join.child];
        let Some(lander_head) = heads[join.child] else {
            outcome.kept_no_clear_join += 1;
            continue;
        };
        let widest = found.profiles[join.child]
            .iter()
            .copied()
            .fold(lander.tip_radius_mm(), f64::max);
        // THE RAFT TOP (`stays_above_raft`), by arithmetic, before test 2.
        let mut admissible: Vec<(usize, usize)> = Vec::new();
        let mut bases: Vec<[f64; 3]> = Vec::new();
        for (&(host, layer), &clears) in chain.iter().zip(verdict) {
            if !clears {
                continue;
            }
            let base = base_point(context, struts[host].cell_at(layer), layer);
            if stays_above_raft(lander_head.centre, base, widest, floor_z_mm) {
                admissible.push((host, layer));
                bases.push(base);
            } else {
                outcome.candidates_below_plate += 1;
            }
        }
        // TEST 2, one batch over every candidate the arithmetic admitted.
        let tiers = [(context.standoff_mm, false), (context.clearance_mm, true)];
        let clearances: Vec<Vec<Option<f64>>> = tiers
            .iter()
            .map(|&(tier, _)| solid_clearances(surface, &lander_head, widest, &bases, tier))
            .collect();
        outcome.clearance_rows += bases.len() * tiers.len();
        let planner = Planner {
            context,
            index: &obstacles,
            struts,
            dependants: &dependants,
            tied: &tied,
            heads: &heads,
            profiles: &found.profiles,
            surface,
            floor_z_mm,
            below_plate: std::cell::Cell::new(0),
        };
        let mut refusal = Refused::NoClearJoin;
        let mut planned: Option<(usize, usize, f64, bool, Vec<Move>)> = None;
        'tiers: for (&(tier, at_hard_limit), measured) in tiers.iter().zip(&clearances) {
            for (&(host, layer), clearance) in admissible.iter().zip(measured) {
                let Some(clearance) = *clearance else {
                    continue;
                };
                if clearance < tier - 1.0e-9 {
                    continue;
                }
                match planner.plan(join.child, host, layer, tier) {
                    Ok(moves) => {
                        planned = Some((host, layer, clearance, at_hard_limit, moves));
                        break 'tiers;
                    }
                    Err(reason) => refusal = reason,
                }
            }
        }
        outcome.candidates_below_plate += planner.below_plate.get();
        let Some((host, layer, clearance, at_hard_limit, moves)) = planned else {
            match refusal {
                Refused::NoClearJoin => outcome.kept_no_clear_join += 1,
                Refused::CarryingTie => outcome.kept_carrying_a_tie += 1,
                Refused::Headed => outcome.headed_kept += 1,
            }
            continue;
        };
        outcome.max_drop_layers = outcome
            .max_drop_layers
            .max(join.layer.saturating_sub(layer));
        outcome.load_moved_mm2 += struts[join.child]
            .carried_mm2
            .first()
            .copied()
            .unwrap_or(0.0);
        outcome.dependants_relanded += moves.len() - 1;
        outcome.landers_moved_down_the_chain += usize::from(host != join.parent);
        outcome.landers_at_hard_limit += usize::from(at_hard_limit);
        outcome.min_lander_clearance_mm = outcome.min_lander_clearance_mm.min(clearance);
        for (member, landing, k_bottom, path) in moves {
            let strut = &mut struts[member];
            strut.landing = landing;
            strut.k_bottom = k_bottom;
            strut.path = path;
            // `carried_mm2` and `radius_mm` are now stale; the caller re-runs
            // the down-sweep, which re-sizes both from `ledger_len`.
            obstacles.stamp_strut(strut, height);
            moved_already[member] = true;
        }
        moved_count += 1;
    }
    outcome.landers_moved += moved_count;
    if moved_count > 0 {
        outcome.passes += 1;
    }
    moved_count
}

#[cfg(test)]
mod head_carrier_join_tests {
    use super::{candidate_joins, Join};
    use crate::head_carrier_tests::{grid, headed, straight, H};
    use crate::Landing;

    /// A lander on a HEADED host is offered join layers at or below the host's
    /// joinable top only (`k_joint - ceil(r_K / h)`), down its own shaft and
    /// then down the chain; the same host with no head is offered every layer.
    /// SABOTAGE: with the cap removed the headed host is offered its head.
    #[test]
    fn a_headed_host_is_offered_no_join_above_its_joint() {
        let grid = grid();
        let (k_top, k_joint) = (200, 190);
        let joinable = k_joint - 5;
        let lander = |host: crate::SweepStrut| {
            let mut child = straight(&grid, 1, 150, 160, k_top, 0.1);
            child.landing = Landing::Strut { id: 0, layer: k_top - 1 };
            child.k_bottom = k_top - 1;
            let struts = vec![host, child];
            let join = Join {
                child: 1,
                parent: 0,
                layer: k_top - 1,
                booked: k_top - 1,
                in_head_span: false,
            };
            candidate_joins(&struts, &join, H)
        };
        let on_head = lander(headed(&grid, 0, 150, 150, k_top, k_joint, 0.3));
        assert!(!on_head.is_empty(), "anti-vacuity: layers below the joint are offered");
        assert_eq!(on_head.iter().map(|&(_, layer)| layer).max(), Some(joinable));
        let on_straight = lander(straight(&grid, 0, 150, 150, k_top, 0.3));
        assert_eq!(on_straight.iter().map(|&(_, layer)| layer).max(), Some(k_top - 2));
    }

    /// **A lander in a headed host's head span is always in the rule's zone,
    /// and its load is booked at the joint's layer at the highest** (v19 fix,
    /// 2026-10-08). 0.25 mm under the contact (layer 195 of a head over layers
    /// 191..=200, joint at 190) and on the joint sphere (layer 187, over the
    /// joinable top 185) are in the head span; a landing at 185 is not. A head
    /// given up (`drawn_straight`) is a straight arm again.
    /// SABOTAGE: take `in_head_span` out of `arm_joins` and the first two
    /// assertions go red; take the joint out of `booked_slot` and the booked
    /// layers read 195 and 187.
    #[test]
    fn a_lander_in_the_head_span_is_listed_and_booked_at_the_joint() {
        use super::{arm_joins, booked_slot};
        let grid = grid();
        let (k_top, k_joint) = (200, 190);
        let joins_at = |layer: usize, drawn_straight: bool| {
            let mut host = headed(&grid, 0, 150, 150, k_top, k_joint, 0.3);
            host.head.as_mut().expect("a head").drawn_straight = drawn_straight;
            let mut child = straight(&grid, 1, 150, 160, k_top, 0.1);
            child.landing = Landing::Strut { id: 0, layer };
            child.k_bottom = layer;
            let struts = vec![host, child];
            let joins = arm_joins(&struts, H);
            assert_eq!(joins.len(), 1);
            (joins[0].in_head_span, joins[0].booked, booked_slot(&struts[0], layer))
        };
        assert_eq!(joins_at(195, false), (true, k_joint, k_joint), "in the head span");
        assert_eq!(joins_at(187, false), (true, 187, 187), "on the joint sphere");
        assert_eq!(joins_at(185, false), (false, 185, 185), "at the joinable top");
        assert_eq!(joins_at(195, true), (false, k_joint, k_joint), "a straight arm again");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        join_floor_z_mm, sections_near_model, stays_above_raft, Head, Section, SectionsNear,
    };
    use geom_kernel::{NativeSurface, NativeSurfaceConfig};

    /// A model: its surface and the indexed triangles it was built from.
    struct Model {
        surface: NativeSurface,
        verts: Vec<[f64; 3]>,
        tris: Vec<[u32; 3]>,
    }

    fn near_of(model: &Model, sections: &[Section], clearance_mm: f64) -> SectionsNear {
        sections_near_model(
            &model.surface,
            &model.verts,
            &model.tris,
            sections,
            clearance_mm,
        )
    }

    /// A closed axis-aligned box from `low` to `high`, as a model.
    fn boxed(low: [f64; 3], high: [f64; 3]) -> Model {
        let verts: Vec<[f64; 3]> = (0..8)
            .map(|corner| {
                std::array::from_fn(|k| {
                    if corner >> k & 1 == 0 {
                        low[k]
                    } else {
                        high[k]
                    }
                })
            })
            .collect();
        let tris: Vec<[u32; 3]> = vec![
            [0, 2, 1],
            [1, 2, 3],
            [4, 5, 6],
            [5, 7, 6],
            [0, 1, 4],
            [1, 5, 4],
            [2, 6, 3],
            [3, 6, 7],
            [0, 4, 2],
            [2, 4, 6],
            [1, 3, 5],
            [3, 7, 5],
        ];
        let pitch = 0.1;
        let estimate = NativeSurface::estimate_index_memory_from_indexed(&verts, &tris, pitch)
            .expect("an estimate");
        let surface = NativeSurface::from_indexed(
            &verts,
            &tris,
            NativeSurfaceConfig {
                grid_pitch_mm: pitch,
                max_grid_bytes: estimate.max_axis_grid_bytes(),
            },
        )
        .expect("a surface");
        Model {
            surface,
            verts,
            tris,
        }
    }

    fn flat(centre: [f64; 3], radius: f64) -> Section {
        Section {
            centre,
            axis: [0.0, 0.0, 1.0],
            radius,
        }
    }

    /// Hand-derived, against a clearance of 0.3 mm. A slab whose underside is
    /// `z = 0` and spans `x, y ∈ [−5, 5]`:
    /// * a 1 mm disc 0.2 mm under it is near, and 0.5 mm under it is not;
    /// * a 1 mm disc 0.2 mm under the slab's plane but 2 mm beyond its edge
    ///   (its nearest point 1 mm off, laterally) is NOT near: the plane is
    ///   not the model there.
    ///
    /// A post 0.1 mm wide, its foot 0.1 mm over a 1 mm disc's CENTRE, is
    /// within the clearance of the disc's interior only (the ring is 0.95 mm
    /// off): near, by the spokes. Sabotage: dropping the spokes reads it far.
    #[test]
    fn a_disc_is_near_the_model_where_the_model_is() {
        let slab = boxed([-5.0, -5.0, 0.0], [5.0, 5.0, 1.0]);
        let found = near_of(
            &slab,
            &[
                flat([0.0, 0.0, -0.2], 1.0),
                flat([0.0, 0.0, -0.5], 1.0),
                flat([8.0, 0.0, -0.2], 1.0),
            ],
            0.3,
        );
        assert_eq!(found.near, vec![true, false, false]);
        let post = boxed([-0.05, -0.05, 0.1], [0.05, 0.05, 3.0]);
        let found_post = near_of(&post, &[flat([0.0, 0.0, 0.0], 1.0)], 0.3);
        assert_eq!(found_post.near, vec![true]);
        assert_eq!(found.rows, 3 * 2 * super::SECTION_POLYGON_SIDES);
    }

    /// The cross-family review's case (round 3, 2026-09-29): a closed
    /// 0.1 × 0.1 mm post centred at `XY = (−0.1560722576, 0.7846282243)`, its
    /// foot at `z = 0.29`, over a 1 mm disc at the origin, axis `+z`. The
    /// post's footprint lies inside the disc, so the disc is 0.29 mm from the
    /// model: NEAR against 0.3 mm. The post stands halfway between two of the
    /// 16 spokes (at 101.25°), where the spokes and the rim are 0.302 mm or
    /// more from it, and the centre is outside it; only the FILLED disc sees
    /// it. Control: the same post with its foot at `z = 0.31` is 0.31 mm from
    /// the disc and must read clear (the fix is exact on the filled polygon,
    /// not a wider margin).
    #[test]
    fn a_post_between_two_spokes_is_near_the_filled_disc() {
        let (x, y, half) = (-0.156_072_257_6, 0.784_628_224_3, 0.05);
        let post_at = |foot: f64| boxed([x - half, y - half, foot], [x + half, y + half, 3.0]);
        let found = near_of(&post_at(0.29), &[flat([0.0, 0.0, 0.0], 1.0)], 0.3);
        assert_eq!(
            found.near,
            vec![true],
            "the post's foot is 0.29 mm over the disc's interior, inside the 0.3 mm clearance"
        );
        // The path-hit witness: the interior term, and only it, found the post.
        // Its rows are the edges of the post's faces within the rim's reach
        // plus the clearance (1.0196 + 0.3 mm) of the centre: the foot and the
        // four sides (the top, at z = 3, is out of reach), so the 4 foot
        // edges, 4 verticals, 4 top edges, 4 side diagonals and the foot's
        // diagonal: 17.
        assert_eq!(found.near_by_interior, 1);
        assert_eq!(found.interior_rows, 17);
        let clear = near_of(&post_at(0.31), &[flat([0.0, 0.0, 0.0], 1.0)], 0.3);
        assert_eq!(
            clear.near,
            vec![false],
            "0.31 mm over the disc is clear of 0.3 mm"
        );
        assert_eq!(clear.near_by_interior, 0);
    }

    /// A shaft leaning from a ball centre at the origin toward `(3, 0, −4)`
    /// (cos θ = 0.8 from vertical) under a flat face through `z = 0.2` with
    /// outward normal `−z`, one layer 0.05 mm high on a plate at `z = −10`.
    fn leaned() -> Head {
        Head {
            centre: [0.0, 0.0, 0.0],
            base: [3.0, 0.0, -4.0],
            axis: [0.6, 0.0, -0.8],
            plane_point: [0.0, 0.0, 0.2],
            normal: [0.0, 0.0, -1.0],
            spread: 0.6,
            seat: [0.0, 0.0, 0.2],
            contact_reach_mm: 0.0,
        }
    }

    /// Hand-derived: the section whose top is the top of layer 179 sits at
    /// `z = −10 + 180·0.05 = −1`, 1.2 mm below the plane; a 0.5 mm ring tilted
    /// by `sin θ = 0.6` rises 0.3 mm of that back, leaving 0.9 mm. Sabotage:
    /// dropping the ring's rise (`radius · spread`) reads 1.2 and goes red.
    #[test]
    fn a_tilted_ring_rises_toward_the_seat_by_its_radius_times_the_lean() {
        let clearance = leaned().clearance_at(-10.0, 0.05, 179, 0.5);
        assert!((clearance - 0.9).abs() < 1.0e-9, "{clearance}");
    }

    /// Above the ball centre the section cannot start: a slot whose top is
    /// above the centre is measured AT the centre (0.2 mm below the plane).
    #[test]
    fn a_section_never_starts_above_the_ball_centre() {
        let clearance = leaned().clearance_at(-10.0, 0.05, 250, 0.5);
        assert!((clearance - (0.2 - 0.3)).abs() < 1.0e-9, "{clearance}");
    }

    /// Hand-derived, the plate at z = 0. An axis leaned 3-4-5 (horizontal 3,
    /// down 4: `a_z = −0.8`) tilts a 0.5 mm base disc so it reaches
    /// `0.5·√(1 − 0.64) = 0.3` mm under the base point. So a base 0.29 mm over
    /// the plate is refused, 0.31 mm admitted; a VERTICAL member's disc is
    /// flat and may stand on the plate itself; a base ON the plate with any
    /// lean (the Round_100 case: lander 536 re-landed at plate arm 40's
    /// layer 0) is refused. Sabotage: dropping the disc's reach (`base_z ≥
    /// plate` only) admits the 0.29 mm and on-the-plate leaned cases.
    #[test]
    fn a_join_keeps_the_members_base_disc_above_the_plate() {
        let leaned =
            |base_z: f64| stays_above_raft([0.0, 0.0, base_z + 4.0], [3.0, 0.0, base_z], 0.5, 0.0);
        assert!(!leaned(0.29));
        assert!(leaned(0.31));
        assert!(!leaned(0.0));
        let vertical =
            |base_z: f64| stays_above_raft([0.0, 0.0, 4.0], [0.0, 0.0, base_z], 0.5, 0.0);
        assert!(vertical(0.0));
        assert!(!vertical(-0.01));
    }

    /// **The floor is the raft top the lowering draws the pads to**
    /// (fix/round11-milestone, 2026-10-02), read through the lowering's own
    /// door, over any plate height. The milestone's case, hand-derived from
    /// its numbers: a 0.3896 mm lander leaned with `√(1 − a_z²) = 0.4686`
    /// joined 0.35 mm over the plate reaches 0.1826 mm under its base point,
    /// so its disc's lowest point is 0.1674 mm over the plate. The plate
    /// admitted it (`f19ba85`); the raft top, 0.2 mm up, refuses it, and
    /// admits the same lander one layer higher (0.2174 mm). Sabotage: a floor
    /// of the plate itself admits the 0.35 mm join, and this test goes red.
    #[test]
    fn a_join_keeps_the_members_base_disc_above_the_raft_top() {
        let raft_mm = plinth_support::SweepRootFlare::registered(0.05)
            .expect("the registry")
            .raft_top_above_plate_mm();
        assert!(raft_mm > 0.0, "the sweep draws a raft: {raft_mm}");
        for plate in [0.0, -6.75, 7.3] {
            let floor = join_floor_z_mm(plate, 0.05);
            assert!(
                (floor - (plate + raft_mm)).abs() < 1.0e-12,
                "plate {plate}: floor {floor}, raft top {}",
                plate + raft_mm
            );
        }
        assert!(
            (raft_mm - 0.2).abs() < 1.0e-12,
            "the hand derivation reads a 0.2 mm raft"
        );
        // The lander's axis, as the milestone's probe read it (plate at 0).
        let centre = [
            -3.017_780_895_934_699_4,
            -7.324_502_427_876_349,
            7.448_434_970_962_137,
        ];
        let foot = [-4.525, -10.775];
        let radius = 0.389_635_896_481_705_6;
        let lander = |base_z: f64| {
            stays_above_raft(
                centre,
                [foot[0], foot[1], base_z],
                radius,
                join_floor_z_mm(0.0, 0.05),
            )
        };
        assert!(
            stays_above_raft(centre, [foot[0], foot[1], 0.35], radius, 0.0),
            "anti-vacuity: the plate floor admits the 0.35 mm join"
        );
        assert!(!lander(0.35), "0.35 mm up, the disc reaches into the pad");
        assert!(lander(0.40), "one layer higher it clears the raft top");
    }
}
