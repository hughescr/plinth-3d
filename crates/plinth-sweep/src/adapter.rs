//! From a [`SweepPlan`] to the emit boundary's own primitives.
//!
//! This is the whole of LANE 2's geometry, and it is deliberately thin. The
//! primitive chain — tip ball, taper, boss column, root flare — is built by
//! [`plinth_support::lower_sweep_arms`], which lives beside the legacy
//! lowering and reuses its canonical ball poles, its boss-top arithmetic and
//! its registered flare factor. Nothing here re-derives any of that
//! (`docs/plans/slice-sweep-core-2026-09-02.md` §9.2).
//!
//! What this module DOES own is the frame arithmetic: turning an arm's
//! `(cell, k_top, k_bottom, landing)` into two points in millimetres, in the
//! same frame the emitter reads — and handing across the arm's WHOLE radius
//! ledger, one entry per layer, so the lowering can draw the section the area
//! law sized at every height rather than a straight line between its two ends.
//!
//! ```text
//! layer k covers z in [plate_z + k*h, plate_z + (k+1)*h)
//! seat  = top face of the arm     = plate_z + k_top    * h
//! base  = bottom face of the arm  = plate_z + k_bottom * h
//! ```
//!
//! `seat` is the model SURFACE — the bottom plane of the first cured model
//! lamina — and not the finished tip pole. Until 2026-09-05 it was both, so
//! the tip ball's pole sat exactly on that plane: a point contact carrying no
//! area, which is what the owner saw in ChituBox on the Sphere and the Morel
//! base. [`plinth_support::lower_sweep_arms`] now drives the pole INTO the
//! model from this plane by the seat law of `docs/physics.md` §9 — to the FULL
//! depth the contact-area term asks for. The carried face this module hands it
//! ([`plinth_support::SweepArm::seat_face_point_mm`] with the seat normal) is
//! the SEAT PLANE the emitted ball and the first shaft segment are trimmed to
//! (fix/tip-trim, 2026-09-28): a tip in a thin skin keeps its contact area,
//! and nothing of it stands inside the model or past the face.
//!
//! A `Landing::Plate` arm has `k_bottom = 0`, so its base is exactly the plate.
//! A `Landing::Model` or `Landing::Strut` arm has `k_bottom` at the HIGHEST
//! occupied layer below the seat, so it stops one lamina inside the strike
//! face and never exits the far side — the land-at-strike ruling
//! (`docs/physics.md`:284-324), realised by the layer index rather than by a
//! second ray cast.

use plinth_support::{lower_sweep_arms, SweepArm, SweepArmLanding, SweepLowering, SweepLoweringError};
use plinth_support::{BuildFrame, SupportPrimitive};
use plinth_core::LayerPlan;

use crate::{Landing, SweepPlan};

/// The emit-ready field, plus everything the lowering had to warn about.
#[derive(Clone, Debug, PartialEq)]
pub struct SweepEmission {
    pub primitives: Vec<SupportPrimitive>,
    pub arms_offered: usize,
    pub arms_dropped_zero_length: usize,
    pub arms_without_root_flare: usize,
    pub arms_dropped_invalid_geometry: usize,
    /// Tips handed to the emitter with a SEAT TRIM: their ball and the first
    /// segment of their shaft keep only the support side of the seat face,
    /// outside the model (fix/tip-trim, 2026-09-28). The seat is at full law
    /// depth in every case (owner ruling 2026-09-05: cut the geometry, never
    /// retract the tip). The second count is the tips whose plane is the
    /// raster seat's because no face was measured.
    pub tips_trimmed_to_seat: usize,
    pub tips_trimmed_without_a_measured_face: usize,
    /// Per-tip contact disc the trim leaves in the seat plane, and the raster
    /// seat's offset from that plane, in arm order
    /// ([`plinth_support::SweepLowering::tip_contact_disc_radius_mm`]).
    pub tip_contact_disc_radius_mm: Vec<f64>,
    pub tip_raster_seat_offset_mm: Vec<f64>,
    /// Tips the law seated at its own smallest term.
    pub tips_at_law_minimum: usize,
    /// Per-tip delivered seat depth, cut-disc radius and tip radius, in arm
    /// order — the evidence for the `a ≥ r_tip` guarantee, published rather
    /// than assumed.
    pub tip_seat_depth_mm: Vec<f64>,
    pub tip_cut_radius_mm: Vec<f64>,
    pub tip_radius_mm: Vec<f64>,
    /// Members drawn in more than one straight span because one taper would
    /// have dipped below the area law's profile (every trunk under a mid-shaft
    /// junction), and the `Member` frusta that added.
    pub arms_drawn_in_spans: usize,
    pub profile_member_frusta: usize,
    /// Arms that reached the lowering with no radius profile (a caller
    /// defect; zero on every sweep field).
    pub arms_without_profile: usize,
    /// The support resin as EMITTED, from the written records themselves, and
    /// the part of it in tapers and chained members. The sweep report's
    /// `support_volume_mm3` is the SIZING ledger and is published beside it.
    pub drawn_volume_mm3: f64,
    pub drawn_shaft_volume_mm3: f64,
    /// Members whose drawn profile the JUNCTION CLAUSE raised above their
    /// ledger, from the junction down to their root, because a thicker branch
    /// (in practice a brace, sized by its own slenderness minimum) joins them.
    pub members_raised_to_a_branch: usize,
    /// Junctions where a support ARM's own ledger end is wider than its host's
    /// ledger at the joint — a ledger inconsistency, zero on a consistent
    /// ledger. Raises that only propagate a brace's raise down a tree of arms
    /// are not counted here.
    pub arm_junctions_inverted_in_ledger: usize,
}

impl SweepEmission {
    /// Arms that reached the file. **Emit and warn**: a dropped arm is a
    /// warning on an emitted file, never a refusal, so this can be smaller
    /// than `arms_offered` and the difference is named above.
    #[must_use]
    pub const fn arms_emitted(&self) -> usize {
        self.arms_offered - self.arms_dropped_zero_length - self.arms_dropped_invalid_geometry
    }

    /// Tips whose DELIVERED cut disc came out narrower than their own tip —
    /// the `a ≥ r_tip` guarantee failing.
    ///
    /// Zero is the guarantee holding. A nonzero count is real information, not
    /// a defect to hide: on a wall too thin to seat into, the truncated seat
    /// genuinely cannot cut the disc the area law wants, and
    /// [`Self::tips_truncated_by_thin_wall`] says so beside it.
    /// The comparison carries the same tiny relative tolerance
    /// [`plinth_support::SweepLowering::tips_below_area_guarantee`] uses, and
    /// for the same reason: the ball clause designs `a` to EQUAL `r_tip` at its
    /// break-even depth, so exact equality is the common case and float
    /// rounding straddles it. A shortfall has to be real to be counted.
    #[must_use]
    pub fn tips_below_area_guarantee_count(&self) -> usize {
        let slack = 1.0 - plinth_support::SweepLowering::AREA_GUARANTEE_TOLERANCE;
        self.tip_cut_radius_mm
            .iter()
            .zip(&self.tip_radius_mm)
            .filter(|(cut, tip)| **cut < **tip * slack)
            .count()
    }
}

/// Turn one sweep plan into the primitives `plinth_emit::write` consumes.
///
/// `layers` is the pipeline's own [`LayerPlan`] — the lowering reads its layer
/// height for the root-boss no-bend run, and refuses a variable-height plan
/// the same way the legacy path does.
///
/// # Errors
///
/// Only about the request: an empty or variable-height layer plan, zero
/// limits, an unreadable registry, or a field larger than `max_primitives`.
pub fn to_primitives(
    plan: &SweepPlan,
    frame: BuildFrame,
    layers: &LayerPlan,
    max_primitives: usize,
    max_primitive_bytes: usize,
) -> Result<SweepEmission, SweepLoweringError> {
    let (arms, raised) = arms_and_junction_raises(plan);
    let SweepLowering {
        primitives,
        arms_dropped_zero_length,
        arms_without_root_flare,
        arms_dropped_invalid_geometry,
        tips_trimmed_to_seat,
        tips_trimmed_without_a_measured_face,
        tip_contact_disc_radius_mm,
        tip_raster_seat_offset_mm,
        tips_at_law_minimum,
        tip_seat_depth_mm,
        tip_cut_radius_mm,
        tip_radius_mm,
        arms_drawn_in_spans,
        profile_member_frusta,
        arms_without_profile,
        drawn_volume_mm3,
        drawn_shaft_volume_mm3,
    } = lower_sweep_arms(
        &arms,
        frame,
        layers,
        max_primitives,
        max_primitive_bytes,
    )?;
    Ok(SweepEmission {
        primitives,
        arms_offered: arms.len(),
        arms_dropped_zero_length,
        arms_without_root_flare,
        arms_dropped_invalid_geometry,
        tips_trimmed_to_seat,
        tips_trimmed_without_a_measured_face,
        tip_contact_disc_radius_mm,
        tip_raster_seat_offset_mm,
        tips_at_law_minimum,
        tip_seat_depth_mm,
        tip_cut_radius_mm,
        tip_radius_mm,
        arms_drawn_in_spans,
        profile_member_frusta,
        arms_without_profile,
        drawn_volume_mm3,
        drawn_shaft_volume_mm3,
        members_raised_to_a_branch: raised.members,
        arm_junctions_inverted_in_ledger: raised.arm_ledger_inversions,
    })
}

/// What the junction clause did to the drawn profiles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct JunctionRaises {
    /// Members whose drawn profile was raised anywhere.
    members: usize,
    /// Junctions where a support ARM's own LEDGER end is wider than its host's
    /// LEDGER at the joint. An arm's load is booked into its host below the
    /// landing, so this is zero on a consistent ledger; a nonzero count is a
    /// ledger inconsistency, published rather than hidden. (A raise that only
    /// PROPAGATES a brace's raise down a tree of arms is not one.)
    arm_ledger_inversions: usize,
}

/// **The drawn profile of every member**: the down-sweep's ledger, raised
/// where the JUNCTION CLAUSE needs it (2026-09-28).
///
/// The rule: a trunk is at least as thick as each branch where the branch
/// joins it. For a support ARM landing on a trunk the ledger already says so
/// — the branch's load is booked into the trunk below the landing, so the
/// trunk's section there is at least the branch's. A BRACE (and a mast, if one
/// is ever braced onto) is different: its section is set by its own
/// slenderness minimum, not by load, and the trunk's ledger never hears of it,
/// so a 0.08 mm brace can land on a 0.05 mm arm. For every junction — a
/// member's landing on another strut, and a brace's top seat on its host —
/// the host's drawn profile is raised to the branch's own drawn end radius
/// from the junction layer DOWN TO THE HOST'S ROOT. Down to the root, and not
/// only at the junction, because a printed member must never be narrower
/// below than above: a local collar would leave a down-facing ledge, which is
/// an unsupported overhang, and the ledger's own non-decreasing-downward shape
/// exists for the same reason. A raised root also widens that member's plate
/// flare, which is sized from the drawn root.
///
/// A raised member may itself be a branch of another, so the pass repeats
/// until nothing moves; every raise is monotone and bounded by the largest
/// radius in the field, so it terminates, and the pass count is bounded by the
/// strut count. One batch pass over the strut vector per round.
fn junction_profiles(plan: &SweepPlan) -> (Vec<Vec<f64>>, JunctionRaises) {
    let struts = &plan.struts;
    let mut profiles: Vec<Vec<f64>> = struts.iter().map(|strut| strut.radius_mm.clone()).collect();
    let max_id = struts
        .iter()
        .map(|strut| strut.id as usize)
        .max()
        .unwrap_or(0);
    let mut index_of = vec![usize::MAX; max_id + 1];
    for (index, strut) in struts.iter().enumerate() {
        index_of[strut.id as usize] = index;
    }
    let host = |id: u32| {
        index_of
            .get(id as usize)
            .copied()
            .filter(|index| *index != usize::MAX)
    };
    // (host index, host slot of the junction, branch index, true = at the
    // branch's ROOT, false = at its TOP)
    let mut joints: Vec<(usize, usize, usize, bool)> = Vec::new();
    for (index, strut) in struts.iter().enumerate() {
        if let Landing::Strut { id, .. } = strut.landing {
            if let Some(parent) = host(id).filter(|parent| *parent != index) {
                let slot = strut.k_bottom.saturating_sub(struts[parent].k_bottom);
                joints.push((parent, slot, index, true));
            }
        }
        if let Some(seat_host) = strut.seat_on_strut.and_then(host).filter(|h| *h != index) {
            let slot = strut.k_top.saturating_sub(struts[seat_host].k_bottom);
            joints.push((seat_host, slot, index, false));
        }
    }
    // THE LEDGER'S OWN CONSISTENCY, measured before anything is raised: a
    // support arm's ledger end wider than its host's LEDGER at the joint. The
    // load law says this cannot happen (the arm's load is booked into the host
    // below the landing), so it is counted, never assumed.
    let arm_ledger_inversions = joints
        .iter()
        .filter(|&&(parent, slot, branch, at_root)| {
            let branch_ledger = &struts[branch].radius_mm;
            let end = if at_root {
                branch_ledger.first()
            } else {
                branch_ledger.last()
            };
            let host_ledger = &struts[parent].radius_mm;
            let there = host_ledger.get(slot.min(host_ledger.len().saturating_sub(1)));
            struts[branch].kind == crate::StrutKind::Arm
                && matches!((end, there), (Some(end), Some(there)) if end > there)
        })
        .count();
    let mut raised = vec![false; struts.len()];
    for _round in 0..=struts.len() {
        let mut moved = false;
        for &(parent, slot, branch, at_root) in &joints {
            let end = if at_root {
                profiles[branch].first()
            } else {
                profiles[branch].last()
            };
            let Some(&branch_radius) = end else {
                continue;
            };
            let host_profile = &mut profiles[parent];
            let top = slot.min(host_profile.len().saturating_sub(1));
            for radius in host_profile.iter_mut().take(top + 1) {
                if *radius < branch_radius {
                    *radius = branch_radius;
                    moved = true;
                    raised[parent] = true;
                }
            }
        }
        if !moved {
            break;
        }
    }
    let summary = JunctionRaises {
        members: raised.iter().filter(|flag| **flag).count(),
        arm_ledger_inversions,
    };
    (profiles, summary)
}

/// One batch pass over the strut vector. No per-arm mesh query, no per-arm
/// registry read: the seat normal is the arm's own axis, exactly as
/// `lower_stub` uses the member direction rather than a face normal
/// (`crates/plinth-support/src/lower.rs`, `lower_stub`).
#[must_use]
pub fn arms_of(plan: &SweepPlan) -> Vec<SweepArm> {
    arms_and_junction_raises(plan).0
}

#[expect(
    clippy::cast_precision_loss,
    reason = "a layer index bounded by the plan's own layer count is exact in f64"
)]
fn arms_and_junction_raises(plan: &SweepPlan) -> (Vec<SweepArm>, JunctionRaises) {
    let height = plan.layer_height_mm;
    let plate = plan.plate_z_mm;
    let grid = &plan.grid;
    let (mut profiles, raised) = junction_profiles(plan);
    let followed = crate::footprint::followed_shifts(&plan.struts);
    let arms = plan
        .struts
        .iter()
        .zip(profiles.iter_mut())
        .enumerate()
        .map(|(index, (strut, profile))| {
            let x = strut.center_mm[0];
            let y = strut.center_mm[1];
            let seat_z = plate + strut.k_top as f64 * height;
            let base_z = plate + strut.k_bottom as f64 * height;
            // A leaned arm's base sits under a DIFFERENT column from its seat.
            // The emit boundary has always taken two arbitrary points, so a
            // leaning member needs no new primitive: the same tip ball, taper,
            // boss and root flare are lowered along the member direction, which
            // `lower_stub` already uses in place of a face normal.
            let base_cell = strut.base_cell();
            let (base_x, base_y) = if base_cell == strut.cell {
                (x, y)
            } else {
                (
                    grid.center_x_mm(grid.col_of(base_cell)),
                    grid.center_y_mm(grid.row_of(base_cell)),
                )
            };
            // THE FOOTPRINT RULE's move (fix/tip-trim, 2026-09-28): a seat
            // whose contact disc hung off its face slides ALONG the face; a
            // member joined to a moved axis follows it at the junction
            // (`footprint::followed_shifts`). Applied only where non-zero, so
            // every member nothing moved is bit-identical.
            let crate::footprint::Followed {
                seat_mm: seat_shift,
                base_mm: base_shift,
            } = followed[index];
            let moved = |point: [f64; 3], by: Option<[f64; 3]>| -> [f64; 3] {
                by.map_or(point, |by| {
                    std::array::from_fn(|axis| point[axis] + by[axis])
                })
            };
            // The face point moves with the arm's OWN seat move only.
            let own_shift = (strut.seat_face.footprint == crate::Footprint::MovedOntoFace)
                .then_some(strut.seat_face.shift_mm);
            SweepArm {
                seat_mm: moved([x, y, seat_z], seat_shift),
                base_mm: moved([base_x, base_y, base_z], base_shift),
                tip_radius_mm: strut.tip_radius_mm(),
                // THE WHOLE LEDGER, not its two ends (2026-09-28). Until then
                // only `(tip, root)` crossed here and the arm was drawn as one
                // straight taper between them, which leaves every trunk under
                // a mid-shaft junction thinner than its own ledger and thinner
                // than the branch landing on it. The lowering draws the fewest
                // straight spans that stay on or above this profile.
                // Raised where a thicker branch joins (`junction_profiles`).
                radius_profile_mm: std::mem::take(profile),
                // Both measured by the sweep at its own finished seat
                // (`run_sweep_with_drains`); nothing is re-derived here. The
                // lowering drives the tip ball into the model along the arm to
                // the law's FULL depth and TRIMS the ball and the first shaft
                // segment to this face's plane. The point moves with the seat,
                // so it stays on the same plane.
                seat_normal_mm: strut.seat_normal,
                seat_face_point_mm: strut
                    .seat_face
                    .point_mm
                    .map(|point| moved(point, own_shift)),
                landing: match strut.landing {
                    Landing::Plate => SweepArmLanding::Plate,
                    Landing::Model { .. } | Landing::Strut { .. } => SweepArmLanding::Landed,
                },
                // A brace seats on another arm and a mast tops out in free
                // space; neither may be given a tip ball. Every arm the reach
                // and the area law place seats into the model and is
                // unchanged.
                seats_into_model: strut.kind == crate::StrutKind::Arm,
            }
        })
        .collect();
    (arms, raised)
}
