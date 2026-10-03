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
    /// THE OWNER'S CHITUBOX GEOMETRY (2026-09-30), published as path hits:
    /// every tip's contact-sphere factor read off his profiles by tip size,
    /// in arm order, and every plate root boss's foot-to-root factor (his
    /// Heavy profile's, for every root), with the root it was drawn at
    /// ([`plinth_support::SweepLowering::tip_ball_factor`] and siblings).
    pub tip_ball_factor: Vec<f64>,
    pub root_boss_factor: Vec<f64>,
    pub root_boss_root_radius_mm: Vec<f64>,
    /// Members drawn in more than one straight span because one taper would
    /// have dipped below the area law's profile (every trunk under a mid-shaft
    /// junction), and the `Member` frusta that added.
    pub arms_drawn_in_spans: usize,
    pub profile_member_frusta: usize,
    /// `Member` spans below a taper that reach the model side of their arm's
    /// seat plane and carry its seat trim
    /// ([`plinth_support::SweepLowering::shaft_members_trimmed_to_seat`]).
    pub shaft_members_trimmed_to_seat: usize,
    /// Arms that reached the lowering with no radius profile (a caller
    /// defect; zero on every sweep field).
    pub arms_without_profile: usize,
    /// The support resin as EMITTED, from the written records themselves, and
    /// the part of it in tapers and chained members. The sweep report's
    /// `support_volume_mm3` is the SIZING ledger and is published beside it.
    pub drawn_volume_mm3: f64,
    pub drawn_shaft_volume_mm3: f64,
    /// THE RAFT (fix/sweep-raft, 2026-09-29): its resin, pads (one under every
    /// plate foot), MST bridges, pads grown past the native diameter to cover
    /// their foot, and feet sharing a pad
    /// ([`plinth_support::SweepLowering::raft_pads`] and siblings).
    pub raft_volume_mm3: f64,
    pub raft_pads: usize,
    pub raft_bridges: usize,
    pub raft_pads_grown: usize,
    pub raft_feet_sharing_a_pad: usize,
    /// Members whose drawn profile the JUNCTION CLAUSE raised above their
    /// ledger, from the junction down to their root, because a thicker branch
    /// (in practice a brace, sized by its own slenderness minimum) joins them.
    pub members_raised_to_a_branch: usize,
    /// Junctions where a support ARM's own ledger end is wider than its host's
    /// ledger at the joint — a ledger inconsistency, zero on a consistent
    /// ledger. Raises that only propagate a brace's raise down a tree of arms
    /// are not counted here.
    pub arm_junctions_inverted_in_ledger: usize,
    /// Load steps on support arms drawn as a cone above their junction
    /// ([`LoadStepTaper`], 2026-10-02), and the layers those cones raised
    /// above the ledger: the rule's path hits on the emitted field.
    pub load_steps_tapered: usize,
    pub load_step_layers_raised: usize,
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
        tip_ball_factor,
        root_boss_factor,
        root_boss_root_radius_mm,
        arms_drawn_in_spans,
        profile_member_frusta,
        shaft_members_trimmed_to_seat,
        arms_without_profile,
        drawn_volume_mm3,
        drawn_shaft_volume_mm3,
        raft_volume_mm3,
        raft_pads,
        raft_bridges,
        raft_pads_grown,
        raft_feet_sharing_a_pad,
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
        tip_ball_factor,
        root_boss_factor,
        root_boss_root_radius_mm,
        arms_drawn_in_spans,
        profile_member_frusta,
        shaft_members_trimmed_to_seat,
        arms_without_profile,
        drawn_volume_mm3,
        drawn_shaft_volume_mm3,
        raft_volume_mm3,
        raft_pads,
        raft_bridges,
        raft_pads_grown,
        raft_feet_sharing_a_pad,
        members_raised_to_a_branch: raised.members,
        arm_junctions_inverted_in_ledger: raised.arm_ledger_inversions,
        load_steps_tapered: raised.load_steps_tapered,
        load_step_layers_raised: raised.load_step_layers_raised,
    })
}

/// **A load step is drawn as a cone, not as a shoulder**
/// (fix/tapered-load-steps, 2026-10-02).
///
/// Where a support arm lands part-way down another support arm, the host
/// carries both loads below the junction, and the area law sizes it larger
/// there. That size is right. Until this rule the drawn profile went from the
/// large radius to the thin one in one layer: a sharp shoulder with D/d up to
/// 3.7 on the Cylinder (member 6: 0.25 mm above, 0.92 mm below, where two
/// branches land; `v11-feedback-diagnosis.md` item 2). A sharp shoulder
/// concentrates stress in the thin member exactly where it joins, against a
/// margin of 25 / 18 = 1.39 between the field snap ratio and `X_ALLOWED`.
///
/// **The rule.** Above such a junction the drawn radius narrows by at most
/// [`Self::per_layer_mm`] per layer, from the radius just below the junction
/// down to the host's own ledger, so the shoulder becomes a cone that is thick
/// at the junction and thin above it. The cone only ADDS section: the drawn
/// profile is never thinner than the ledger at any height, and it stays
/// non-decreasing downward, so no ledge faces down.
///
/// **The step is the lean law's** ([`crate::lean::LeanLaw::max_step_mm`]): the
/// largest sideways move one layer of a printed member may make, `h ·
/// cot(θ_sat)`, with `θ_sat` = 45° − 25°/2 = 32.5° from horizontal at the
/// registered overhang centre and transition: 1.5697 · h, 0.0785 mm at 0.05
/// mm layers. So the cone's wall lies no flatter than the surface of the most
/// leaned arm this engine draws, and its length follows from the two radii
/// alone: `(R − r) / step` layers. No new number. (An up-facing cone prints
/// at any angle, so the bound is not about printing it; it is the registered
/// limit on how flat a member's printed surface may lie.)
///
/// **What it does not touch.** Only a step made by CARRIED LOAD — an arm
/// landing on an arm — is tapered. The brace-junction raise (a host raised to
/// a brace's radius from the joint down to its root, [`drawn_profiles`]) is
/// left as it is: a brace carries no load, and that rule waits on a force
/// analysis and the owner. The cone never enters the host's CONTACT slot (its
/// top slot, sized from its own peel alone); the head-join rule grades the
/// cone's sections against the host's seat plane
/// ([`crate::head_join`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoadStepTaper {
    per_layer_mm: f64,
}

impl LoadStepTaper {
    /// No taper: every load step is drawn as the shoulder this engine drew
    /// before 2026-10-02. **Sabotage and comparison only.**
    pub const OFF: Self = Self { per_layer_mm: 0.0 };

    /// The taper at the lean law's own largest step per layer.
    #[must_use]
    pub const fn of(law: &crate::lean::LeanLaw) -> Self {
        Self {
            per_layer_mm: law.max_step_mm(),
        }
    }

    /// The taper of the REGISTERED lean law at `layer_height_mm`: the same two
    /// registered angles [`crate::SweepPolicy::from_registry`] reads, for the
    /// public measurements that see a strut set without its policy
    /// ([`crate::slenderness::unbraced_runs`]). An unreadable registry gives
    /// [`Self::OFF`]: nothing is refused on a missing number.
    #[must_use]
    pub fn registered(layer_height_mm: f64) -> Self {
        plinth_core::m0_constants().map_or(Self::OFF, |constants| {
            let placeholders = constants.placeholders();
            Self::of(&crate::lean::LeanLaw::new(
                layer_height_mm,
                placeholders.overhang_angle_deg().diagnostic().get(),
                placeholders
                    .overhang_weight_transition_deg()
                    .diagnostic()
                    .get(),
            ))
        })
    }

    /// The most the drawn radius may narrow from one layer to the next going
    /// UP from a load step, in mm; `0` when the taper is off.
    #[must_use]
    pub const fn per_layer_mm(&self) -> f64 {
        self.per_layer_mm
    }

    fn tapers(self) -> bool {
        self.per_layer_mm.is_finite() && self.per_layer_mm > 0.0
    }
}

/// One cone the taper draws above a load step on a support arm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LoadCone {
    /// The host's index in the strut slice.
    pub host: usize,
    /// The ABSOLUTE layer the landers' load is booked into (the junction; the
    /// cone starts one layer above it).
    pub booked: usize,
    /// The highest absolute layer the cone raised.
    pub top: usize,
}

/// **Every cone the taper draws**, from the LEDGER (never from a raised
/// profile, so a brace's raise is never tapered): for each support arm that
/// lands on a support arm, the host's ledger radius `R` in the slot its load
/// is booked into (`head_join::booked_slot`, the down-sweep's booking), then
/// `R − j · step` at `j` layers above it, for as long as that is wider than
/// the host's own ledger there and the layer is below the host's contact slot.
/// Landers booked into the same slot share one cone. One pass over the struts;
/// sorted by host, then layer.
pub(crate) fn load_step_cones(
    struts: &[crate::SweepStrut],
    taper: LoadStepTaper,
) -> Vec<LoadCone> {
    if !taper.tapers() {
        return Vec::new();
    }
    let step = taper.per_layer_mm();
    let mut junctions: Vec<(usize, usize)> = struts
        .iter()
        .enumerate()
        .filter(|(_, strut)| strut.kind == crate::StrutKind::Arm)
        .filter_map(|(index, strut)| {
            let Landing::Strut { id, layer } = strut.landing else {
                return None;
            };
            let host_index = id as usize;
            let host = struts.get(host_index)?;
            (host_index != index
                && host.id == id
                && host.kind == crate::StrutKind::Arm
                && host.k_bottom != usize::MAX
                && host.k_top > host.k_bottom
                && layer >= host.k_bottom)
                .then(|| (host_index, crate::head_join::booked_slot(host, layer)))
        })
        .collect();
    junctions.sort_unstable();
    junctions.dedup();
    junctions
        .into_iter()
        .filter_map(|(host_index, booked)| {
            let host = &struts[host_index];
            let ledger = &host.radius_mm;
            let booked_slot = booked - host.k_bottom;
            let wide = *ledger.get(booked_slot)?;
            // The contact slot is the last one; the cone stops under it.
            let contact = ledger.len().saturating_sub(1);
            let mut top = None;
            for (slot, &ledger_radius) in ledger
                .iter()
                .enumerate()
                .take(contact)
                .skip(booked_slot + 1)
            {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a slot count bounded by the plan's own layer count is exact in f64"
                )]
                let cone = wide - (slot - booked_slot) as f64 * step;
                if cone <= ledger_radius {
                    break;
                }
                top = Some(host.k_bottom + slot);
            }
            top.map(|top| LoadCone {
                host: host_index,
                booked,
                top,
            })
        })
        .collect()
}

/// What the junction clause did to the drawn profiles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct JunctionRaises {
    /// Members whose drawn profile was raised anywhere.
    members: usize,
    /// Load steps drawn as a cone ([`LoadStepTaper`]), and the layers the
    /// cones raised above the ledger: the rule's path hits.
    load_steps_tapered: usize,
    load_step_layers_raised: usize,
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
///
/// Before the junction clause, every LOAD step on a support arm is drawn as a
/// cone above its junction ([`LoadStepTaper`], 2026-10-02). Both are maxima
/// over the ledger from independent sources (the cone from the host's ledger,
/// a raise from a branch's drawn end, which no cone touches: a cone never
/// reaches a member's root slot or a brace), so the order does not matter.
fn junction_profiles(plan: &SweepPlan) -> (Vec<Vec<f64>>, JunctionRaises) {
    drawn_profiles(&plan.struts, plan.load_step_taper)
}

/// [`junction_profiles`] over a bare strut set: the profile every member is
/// DRAWN at, slot by slot from its own `k_bottom`.
///
/// The slenderness lane measures its runs on exactly this (owner ruling
/// 2026-09-29: "measure runs on the DRAWN profile"), so the section the lane
/// grades and the section the file prints are one function, not two.
pub(crate) fn drawn_profiles(
    struts: &[crate::SweepStrut],
    taper: LoadStepTaper,
) -> (Vec<Vec<f64>>, JunctionRaises) {
    let mut profiles: Vec<Vec<f64>> = struts.iter().map(|strut| strut.radius_mm.clone()).collect();
    let cones = load_step_cones(struts, taper);
    let mut load_step_layers_raised = 0_usize;
    for cone in &cones {
        let host = &struts[cone.host];
        let from = cone.booked - host.k_bottom;
        let wide = host.radius_mm[from];
        let profile = &mut profiles[cone.host];
        for (slot, drawn) in profile
            .iter_mut()
            .enumerate()
            .take(cone.top - host.k_bottom + 1)
            .skip(from + 1)
        {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a slot count bounded by the plan's own layer count is exact in f64"
            )]
            let radius = wide - (slot - from) as f64 * taper.per_layer_mm();
            if *drawn < radius {
                *drawn = radius;
                load_step_layers_raised += 1;
            }
        }
    }
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
    //
    // A landing on a support arm's HEAD is booked one slot lower still: the
    // top slot is the arm's contact, sized from its own peel alone, and the
    // lander's load goes into the shaft under it (`down_sweep::run`,
    // 2026-09-29). So at a head joint the slot the load was booked into is
    // the one below the contact, where the host has one.
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
            let top = host_ledger.len().saturating_sub(1);
            let booked = if struts[parent].kind == crate::StrutKind::Arm && slot >= top {
                top.saturating_sub(1)
            } else {
                slot.min(top)
            };
            let there = host_ledger.get(booked);
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
        load_steps_tapered: cones.len(),
        load_step_layers_raised,
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

/// **The two ends a member is DRAWN between**, before any footprint move:
/// its seat over its own seat column at its top layer, and its base over its
/// base cell at its bottom layer. The one derivation the lowering's arms
/// ([`arms_of`]) and the footprint rule's move checks
/// ([`crate::footprint`]) both read, so a move is judged on the geometry the
/// file gets.
#[expect(
    clippy::cast_precision_loss,
    reason = "a layer index bounded by the plan's own layer count is exact in f64"
)]
pub(crate) fn drawn_ends(
    grid: &crate::grid::Grid,
    plate_z_mm: f64,
    layer_height_mm: f64,
    strut: &crate::SweepStrut,
) -> ([f64; 3], [f64; 3]) {
    let x = strut.center_mm[0];
    let y = strut.center_mm[1];
    let seat_z = plate_z_mm + strut.k_top as f64 * layer_height_mm;
    let base_z = plate_z_mm + strut.k_bottom as f64 * layer_height_mm;
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
    ([x, y, seat_z], [base_x, base_y, base_z])
}

/// How far a member's DRAWN shaft line stands from the line it was planned
/// on: `(at its seat, at its base)`, each a horizontal displacement in mm.
/// Linear in between, because both lines are straight.
pub type LineOffset = ([f64; 3], [f64; 3]);

/// **A member's drawn shaft is where its junctions go** (fix/morel-long-posts,
/// 2026-10-02): the displacement of `arm`'s drawn shaft line
/// ([`plinth_support::SweepRootFlare::plate_shaft_of`], the lowering's own
/// line) from the planned line between `arm.seat_mm` and `arm.base_mm`, at the
/// seat's height and at the base's; `None` where the two lines are the same
/// line, which is every member except one that stands on the plate and LEANS.
///
/// A leaning plate post is planned from its seat to its base ON THE PLATE,
/// but its boss stands vertically over its foot, so the file draws its shaft
/// from the seat to the boss top over the foot: a steeper line, which leaves
/// the planned one by up to `lateral × boss_top / (seat_height − boss_top)`.
/// A diagonal or a branch placed on the PLANNED line therefore ended up to
/// 0.585 mm off the drawn post (Morel base, for-craig-v10: brace records
/// 1334-1337 on post 578, 0.08-0.26 mm of air between brace end and post
/// surface; 24 such ends on the Cylinder, 8 on the laid tube, 3 on the
/// Sphere). The offset is what moves such a junction onto the drawn line,
/// exactly as a footprint move of the post's seat moves it
/// ([`crate::footprint::followed_shifts_drawn`]).
///
/// At a planned point a fraction `t` of the way from base to seat the drawn
/// line is displaced by `t · seat + (1 − t) · base` of the returned pair,
/// because the drawn point AT THE SAME HEIGHT is linear in that height, and
/// the height is linear in `t`.
#[must_use]
pub fn drawn_line_offset(
    flare: &plinth_support::SweepRootFlare,
    arm: &SweepArm,
    plate_z_mm: f64,
) -> Option<LineOffset> {
    if arm.landing != SweepArmLanding::Plate {
        return None;
    }
    // A vertical member's drawn line is its planned line (the boss stands
    // under its own seat); nothing moves, bit for bit.
    let same = |a: f64, b: f64| a.partial_cmp(&b) == Some(std::cmp::Ordering::Equal);
    if same(arm.seat_mm[0], arm.base_mm[0]) && same(arm.seat_mm[1], arm.base_mm[1]) {
        return None;
    }
    let (top, bottom) = flare.plate_shaft_of(arm, plate_z_mm)?;
    let rise = top[2] - bottom[2];
    if !(rise.is_finite() && rise > 0.0) {
        return None;
    }
    let drawn_at = |z: f64| -> [f64; 3] {
        let t = (top[2] - z) / rise;
        [
            top[0] + t * (bottom[0] - top[0]),
            top[1] + t * (bottom[1] - top[1]),
            z,
        ]
    };
    let off = |planned: [f64; 3]| -> [f64; 3] {
        let drawn = drawn_at(planned[2]);
        [drawn[0] - planned[0], drawn[1] - planned[1], 0.0]
    };
    Some((off(arm.seat_mm), off(arm.base_mm)))
}

/// [`drawn_line_offset`] for a member as it stands in the plan, before any
/// seat face is measured (the lowering's fallback normal, straight down, as
/// [`crate::slenderness`]'s free-shaft floor reads it too), for the rules
/// that place junctions on it while the field is still being built.
#[must_use]
pub(crate) fn planned_line_offset(
    flare: &plinth_support::SweepRootFlare,
    grid: &crate::grid::Grid,
    layer_height_mm: f64,
    strut: &crate::SweepStrut,
) -> Option<LineOffset> {
    if strut.landing != Landing::Plate
        || strut.kind == crate::StrutKind::Brace
        || strut.k_bottom == usize::MAX
        || !strut.is_leaned()
    {
        return None;
    }
    let (seat, base) = drawn_ends(grid, 0.0, layer_height_mm, strut);
    let arm = sweep_arm(strut, seat, base, None, Vec::new());
    drawn_line_offset(flare, &arm, 0.0)
}

/// The displacement [`LineOffset`] gives at `layer` of `strut` (linear in the
/// layer between its base and its seat).
#[must_use]
pub(crate) fn offset_at(offset: &LineOffset, strut: &crate::SweepStrut, layer: usize) -> [f64; 3] {
    let span = strut.k_top.saturating_sub(strut.k_bottom);
    #[expect(
        clippy::cast_precision_loss,
        reason = "layer indices bounded by the plan's own layer count are exact in f64"
    )]
    let t = if span == 0 {
        1.0
    } else {
        (layer.saturating_sub(strut.k_bottom) as f64 / span as f64).clamp(0.0, 1.0)
    };
    std::array::from_fn(|axis| t * offset.0[axis] + (1.0 - t) * offset.1[axis])
}

/// The lowering's view of one member, drawn between `seat_mm` and `base_mm`
/// with the drawn radius `profile`. The one construction [`arms_of`] and the
/// sweep's head-join rule ([`crate::head_join`]) both use, so the rule measures
/// the member the file gets.
pub(crate) fn sweep_arm(
    strut: &crate::SweepStrut,
    seat_mm: [f64; 3],
    base_mm: [f64; 3],
    seat_face_point_mm: Option<[f64; 3]>,
    profile: Vec<f64>,
) -> SweepArm {
    SweepArm {
        seat_mm,
        base_mm,
        tip_radius_mm: strut.tip_radius_mm(),
        // THE WHOLE LEDGER, not its two ends (2026-09-28). Until then
        // only `(tip, root)` crossed here and the arm was drawn as one
        // straight taper between them, which leaves every trunk under
        // a mid-shaft junction thinner than its own ledger and thinner
        // than the branch landing on it. The lowering draws the fewest
        // straight spans that stay on or above this profile.
        // Raised where a thicker branch joins (`junction_profiles`).
        radius_profile_mm: profile,
        // Both measured by the sweep at its own finished seat
        // (`run_sweep_with_drains`); nothing is re-derived here. The
        // lowering drives the tip ball into the model along the arm to
        // the law's FULL depth and TRIMS the ball and the first shaft
        // segment to this face's plane. The point moves with the seat,
        // so it stays on the same plane.
        seat_normal_mm: strut.seat_normal,
        seat_face_point_mm,
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
}

fn arms_and_junction_raises(plan: &SweepPlan) -> (Vec<SweepArm>, JunctionRaises) {
    let height = plan.layer_height_mm;
    let plate = plan.plate_z_mm;
    let grid = &plan.grid;
    let (mut profiles, raised) = junction_profiles(plan);
    // THE FOOTPRINT RULE's move (fix/tip-trim, 2026-09-28): a seat whose
    // contact disc hung off its face slides ALONG the face; a member joined to
    // a moved axis follows it at the junction (`footprint::followed_shifts`).
    // Applied only where non-zero, so every member nothing moved is
    // bit-identical.
    let build = |strut: &crate::SweepStrut,
                 followed: crate::footprint::Followed,
                 profile: Vec<f64>| {
        let (seat, base) = drawn_ends(grid, plate, height, strut);
        let crate::footprint::Followed {
            seat_mm: seat_shift,
            base_mm: base_shift,
        } = followed;
        let moved = |point: [f64; 3], by: Option<[f64; 3]>| -> [f64; 3] {
            by.map_or(point, |by| std::array::from_fn(|axis| point[axis] + by[axis]))
        };
        // The face point moves with the arm's OWN seat move only.
        let own_shift = (strut.seat_face.footprint == crate::Footprint::MovedOntoFace)
            .then_some(strut.seat_face.shift_mm);
        sweep_arm(
            strut,
            moved(seat, seat_shift),
            moved(base, base_shift),
            strut
                .seat_face
                .point_mm
                .map(|point| moved(point, own_shift)),
            profile,
        )
    };
    let followed = crate::footprint::followed_shifts(&plan.struts);
    // A JUNCTION IS ON THE DRAWN SHAFT (fix/morel-long-posts, 2026-10-02): a
    // leaning plate post's file shaft runs to its boss top over its foot, off
    // the line it was planned on (`drawn_line_offset`), and every branch and
    // diagonal joined to it follows the drawn line as it follows a footprint
    // move. Each post's offset is read on the post as the file draws it (its
    // own seat move applied); no post has a host at its base, so that is its
    // final seat. A field with no leaning plate post is bit-identical.
    let offsets: Vec<Option<LineOffset>> = match plinth_support::SweepRootFlare::registered(height)
    {
        Ok(flare) => plan
            .struts
            .iter()
            .zip(&followed)
            .map(|(strut, followed)| {
                (strut.landing == Landing::Plate
                    && strut.kind != crate::StrutKind::Brace
                    && strut.is_leaned())
                .then(|| drawn_line_offset(&flare, &build(strut, *followed, Vec::new()), plate))
                .flatten()
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    let followed = if offsets.iter().any(Option::is_some) {
        crate::footprint::followed_shifts_drawn(&plan.struts, &offsets)
    } else {
        followed
    };
    let arms = plan
        .struts
        .iter()
        .zip(profiles.iter_mut())
        .zip(followed)
        .map(|((strut, profile), followed)| build(strut, followed, std::mem::take(profile)))
        .collect();
    (arms, raised)
}

#[cfg(test)]
mod tests {
    //! The load-step taper on a hand-built field (fix/tapered-load-steps,
    //! 2026-10-02). Every expected number is derived here from the inputs and
    //! the registered angles, not read from the code under test.
    #![allow(
        clippy::cast_precision_loss,
        reason = "slot counts of a 40-layer fixture are exact in f64"
    )]

    use super::{drawn_profiles, load_step_cones, LoadStepTaper};
    use crate::{Landing, StrutKind, SweepStrut};

    const H: f64 = 0.05;

    /// One straight member in its own column, `ledger` from `k_bottom` up.
    fn member(id: u32, kind: StrutKind, k_bottom: usize, landing: Landing, ledger: Vec<f64>) -> SweepStrut {
        SweepStrut {
            id,
            cell: id as usize,
            center_mm: [f64::from(id), 0.0],
            kind,
            min_radius_mm: 0.0,
            thickened_mm: Vec::new(),
            seat_on_strut: None,
            k_top: k_bottom + ledger.len(),
            k_bottom,
            landing,
            interior: false,
            carried_mm2: vec![0.0; ledger.len()],
            radius_mm: ledger,
            covered_demand: Vec::new(),
            inserted_by_load: false,
            path: Vec::new(),
            seat_face: crate::SeatFace::default(),
            seat_normal: [0.0; 3],
            straight_fallback: None,
            fallback_path: Vec::new(),
        }
    }

    /// The step at the registered 45° centre and 25° transition: the shallow
    /// end of the band is 32.5° from horizontal, so h · cot(32.5°).
    fn step() -> f64 {
        H / 32.5_f64.to_radians().tan()
    }

    fn taper() -> LoadStepTaper {
        LoadStepTaper::of(&crate::lean::LeanLaw::new(H, 45.0, 25.0))
    }

    /// A 40-layer plate post, 0.90 mm in slots 0..=19 (it carries a branch
    /// booked into slot 19) and 0.25 mm above, its own contact included.
    fn host() -> SweepStrut {
        let mut ledger = vec![0.90; 20];
        ledger.extend(vec![0.25; 20]);
        member(0, StrutKind::Arm, 0, Landing::Plate, ledger)
    }

    #[test]
    fn a_load_step_is_drawn_as_a_cone_no_steeper_than_the_lean_law() {
        let struts = vec![
            host(),
            member(1, StrutKind::Arm, 19, Landing::Strut { id: 0, layer: 19 }, vec![0.85; 30]),
        ];
        let (profiles, raised) = drawn_profiles(&struts, taper());
        let drawn = &profiles[0];
        let ledger = &struts[0].radius_mm;
        // Hand-derived: 0.90 − j · step stays above 0.25 while
        // j < 0.65 / step = 8.28, so layers 20..=27 are raised, 8 of them.
        let d = step();
        assert!((d - 0.078_485).abs() < 1e-6, "the lean law's step at 0.05 mm: {d}");
        assert_eq!(raised.load_steps_tapered, 1);
        assert_eq!(raised.load_step_layers_raised, 8);
        for slot in 0..drawn.len() {
            assert!(drawn[slot] >= ledger[slot], "slot {slot} drawn under its ledger");
        }
        for (j, &radius) in drawn[20..=27].iter().enumerate() {
            let slot = 20 + j;
            let expected = 0.90 - (slot - 19) as f64 * d;
            assert!((radius - expected).abs() < 1e-12, "slot {slot}: {radius}");
        }
        assert!((drawn[28] - 0.25).abs() < 1e-12, "the cone ends on the ledger");
        // The rule itself: going up, the drawn radius never narrows by more
        // than one step per layer anywhere on the host, and never widens.
        for slot in 1..drawn.len() {
            let narrowing = drawn[slot - 1] - drawn[slot];
            assert!(narrowing >= -1e-12, "slot {slot} wider than the slot under it");
            assert!(narrowing <= d * (1.0 + 1e-9), "slot {slot} narrows by {narrowing}");
        }
        // The contact slot keeps its own-load section.
        assert_eq!(drawn.last(), ledger.last());
    }

    #[test]
    fn with_the_taper_off_the_shoulder_comes_back() {
        let struts = vec![
            host(),
            member(1, StrutKind::Arm, 19, Landing::Strut { id: 0, layer: 19 }, vec![0.85; 30]),
        ];
        let (profiles, raised) = drawn_profiles(&struts, LoadStepTaper::OFF);
        assert_eq!(raised.load_steps_tapered, 0);
        assert_eq!(profiles[0], struts[0].radius_mm, "OFF is the ledger, bit for bit");
        assert!(load_step_cones(&struts, LoadStepTaper::OFF).is_empty());
    }

    /// A brace carries no load: the raise it makes (the host at the brace's
    /// radius from the joint down to its root) keeps its shoulder, pending a
    /// force analysis and the owner. Only the load step below it tapers.
    #[test]
    fn a_brace_raise_is_not_tapered() {
        let mut brace = member(2, StrutKind::Brace, 30, Landing::Strut { id: 0, layer: 30 }, vec![0.40; 5]);
        brace.min_radius_mm = 0.40;
        let struts = vec![
            host(),
            member(1, StrutKind::Arm, 19, Landing::Strut { id: 0, layer: 19 }, vec![0.85; 30]),
            brace,
        ];
        let (profiles, raised) = drawn_profiles(&struts, taper());
        let drawn = &profiles[0];
        assert_eq!(raised.load_steps_tapered, 1, "the brace's landing makes no cone");
        // Raised to 0.40 at and below the joint (slot 30), then straight back
        // to the 0.25 mm ledger: the brace's shoulder, untouched.
        assert!((drawn[30] - 0.40).abs() < 1e-12);
        assert!((drawn[31] - 0.25).abs() < 1e-12);
    }

    /// A lander on the host's head is booked one slot under its contact; the
    /// cone would have to start in the contact slot, which keeps its own
    /// section, so no cone is drawn there.
    #[test]
    fn no_cone_enters_the_contact_slot() {
        let mut ledger = vec![0.60; 39];
        ledger.push(0.10);
        let struts = vec![
            member(0, StrutKind::Arm, 0, Landing::Plate, ledger.clone()),
            member(1, StrutKind::Arm, 39, Landing::Strut { id: 0, layer: 39 }, vec![0.30; 10]),
        ];
        let (profiles, raised) = drawn_profiles(&struts, taper());
        assert_eq!(raised.load_steps_tapered, 0);
        assert_eq!(raised.load_step_layers_raised, 0);
        // Whatever the junction clause already does there (it raises the host
        // to the lander's root up to the landing layer) is unchanged.
        let (shoulder, _) = drawn_profiles(&struts, LoadStepTaper::OFF);
        assert_eq!(profiles[0], shoulder[0]);
    }
}
