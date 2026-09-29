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
//! depth the contact-area term asks for. The cured thickness this module hands
//! it in [`plinth_support::SweepArm::available_seat_depth_mm`] no longer
//! shortens that seat; since the owner's ruling of 2026-09-05 it is a CLIP
//! PLANE on the emitted ball, so a tip in a thin skin keeps its contact area
//! and simply stops at the far face.
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
    /// Tips whose emitted BALL was CLIPPED at the far face of the cured run it
    /// strikes. The SEAT is at full law depth in every one of these cases
    /// (owner ruling 2026-09-05: clip the geometry, never retract the tip);
    /// what the thin wall removed is the part of the ball that would have
    /// stood proud on the far side.
    pub tips_truncated_by_thin_wall: usize,
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
    let arms = arms_of(plan);
    let SweepLowering {
        primitives,
        arms_dropped_zero_length,
        arms_without_root_flare,
        arms_dropped_invalid_geometry,
        tips_truncated_by_thin_wall,
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
        tips_truncated_by_thin_wall,
        tips_at_law_minimum,
        tip_seat_depth_mm,
        tip_cut_radius_mm,
        tip_radius_mm,
        arms_drawn_in_spans,
        profile_member_frusta,
        arms_without_profile,
        drawn_volume_mm3,
        drawn_shaft_volume_mm3,
    })
}

/// One batch pass over the strut vector. No per-arm mesh query, no per-arm
/// registry read: the seat normal is the arm's own axis, exactly as
/// `lower_stub` uses the member direction rather than a face normal
/// (`crates/plinth-support/src/lower.rs`, `lower_stub`).
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a layer index bounded by the plan's own layer count is exact in f64"
)]
pub fn arms_of(plan: &SweepPlan) -> Vec<SweepArm> {
    let height = plan.layer_height_mm;
    let plate = plan.plate_z_mm;
    let grid = &plan.grid;
    plan.struts
        .iter()
        .map(|strut| {
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
            SweepArm {
                seat_mm: [x, y, seat_z],
                base_mm: [base_x, base_y, base_z],
                tip_radius_mm: strut.tip_radius_mm(),
                // THE WHOLE LEDGER, not its two ends (2026-09-28). Until then
                // only `(tip, root)` crossed here and the arm was drawn as one
                // straight taper between them, which leaves every trunk under
                // a mid-shaft junction thinner than its own ledger and thinner
                // than the branch landing on it. The lowering draws the fewest
                // straight spans that stay on or above this profile.
                radius_profile_mm: strut.radius_mm.clone(),
                // Both measured by the sweep at its own finished seat
                // (`run_sweep_with_drains`); nothing is re-derived here. The
                // lowering drives the tip ball into the model along the arm to
                // the law's FULL depth and CLIPS the ball at this thickness.
                seat_normal_mm: strut.seat_normal,
                available_seat_depth_mm: f64::from(strut.seat_solid_layers) * height,
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
        .collect()
}
