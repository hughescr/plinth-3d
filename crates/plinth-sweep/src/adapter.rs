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
//! same frame the emitter reads.
//!
//! ```text
//! layer k covers z in [plate_z + k*h, plate_z + (k+1)*h)
//! seat  = top face of the arm     = plate_z + k_top    * h
//! base  = bottom face of the arm  = plate_z + k_bottom * h
//! ```
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
}

impl SweepEmission {
    /// Arms that reached the file. **Emit and warn**: a dropped arm is a
    /// warning on an emitted file, never a refusal, so this can be smaller
    /// than `arms_offered` and the difference is named above.
    #[must_use]
    pub const fn arms_emitted(&self) -> usize {
        self.arms_offered - self.arms_dropped_zero_length - self.arms_dropped_invalid_geometry
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
    plan.struts
        .iter()
        .map(|strut| {
            let x = strut.center_mm[0];
            let y = strut.center_mm[1];
            let seat_z = plate + strut.k_top as f64 * height;
            let base_z = plate + strut.k_bottom as f64 * height;
            SweepArm {
                seat_mm: [x, y, seat_z],
                base_mm: [x, y, base_z],
                tip_radius_mm: strut.tip_radius_mm(),
                root_radius_mm: strut.radius_mm.first().copied().unwrap_or_default(),
                landing: match strut.landing {
                    Landing::Plate => SweepArmLanding::Plate,
                    Landing::Model { .. } | Landing::Strut { .. } => SweepArmLanding::Landed,
                },
            }
        })
        .collect()
}
