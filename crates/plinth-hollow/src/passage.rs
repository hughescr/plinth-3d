//! Can a drilled bore still get out of the finished print?
//!
//! The drill stage proves a drain reaches the cavity. Nothing proved the other
//! half: that the resin, once through the bore, can leave the *outside* of the
//! model. It cannot if the support field has closed the mouth in — and that is
//! exactly what the owner saw in ChituBox on 2026-09-03, a hollow Sphere whose
//! drain outlet sat under "a clutter of thick overlapping supports at the
//! bottom". His words on the remedy name the invariant this module states:
//!
//! > we don't want the outlet to then be enclosed by solid supports — or we
//! > would have just in effect created ANOTHER hollow space which would need a
//! > drain.
//!
//! # The invariant
//!
//! From each drain outlet, the empty space — not model, not strut — must
//! connect to the outside of the build through a passage at least the drain's
//! own **diameter** wide. A thread-thin gap between two struts is not a
//! passage; resin does not leave through it.
//!
//! # How it is decided, and what is reused
//!
//! Exactly the machinery the drill stage already uses to decide where a bore
//! *fits*, asked about a different occupancy:
//!
//! * [`crate::frozen_distance_transform`] — the conservative voxel EDT whose
//!   soundness lemma (FU-53, that function's own doc) is what makes a
//!   threshold on it a sound prefilter.
//! * [`crate::flow::core_clearance_threshold_mm`] — the same discretization
//!   correction `r + pitch/2` the resin-flow rule applies, so "the bore fits
//!   here" means one thing in this crate and not two.
//! * `HOLLOW_PITCH_MM` — the caller passes the pitch, and the sweep passes the
//!   registered hollowing pitch, so the question is decided at the resolution
//!   the project already decided the identical question at. **No new constant
//!   is registered by this module.**
//!
//! What is **not** reused, and why: `crate::flow::flow_labels` and its
//! `downhill_flood`. That flood is multi-source from cavity *core* voxels and
//! may only travel **downhill in clearance**, because its job is to stop a
//! cavity annexing dead space through a throat. An escape path from an outlet
//! runs the other way — it starts pinched at the mouth and opens **uphill**
//! into free air — so a downhill-only flood cannot follow it, and its seeds
//! (cavity cores) are not the seeds this question has (an outlet, and the
//! outside of the build). Reusing it would have meant a flag that reverses its
//! one rule, which is a second flood wearing the first one's name. The plain
//! 6-connected wave below is the flood; the expensive, subtle part — the
//! clearance field and the threshold on it — is the shared part.
//!
//! # Batch-first
//!
//! One EDT and one flood for the whole model, however many drains it has; a
//! drain is then an `O(ball)` lookup into the flooded set. No per-drain field
//! is ever built.

use scirs2_core::ndarray::Array3;

use crate::flow::core_clearance_threshold_mm;
use crate::{frozen_distance_transform, HollowError};

/// The voxel lattice one passage question is decided on.
///
/// Voxel `[z, y, x]` is centred at `origin_mm + (index + 0.5) * pitch_mm`, the
/// same convention [`crate::frozen_distance_transform`] measures on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PassageGrid {
    /// `[nz, ny, nx]`, matching the EDT's own axis order.
    pub dims: [usize; 3],
    /// The low corner of voxel `[0, 0, 0]`, as `[x, y, z]` in mm.
    pub origin_mm: [f64; 3],
    pub pitch_mm: f64,
}

impl PassageGrid {
    #[must_use]
    pub const fn voxels(&self) -> usize {
        self.dims[0] * self.dims[1] * self.dims[2]
    }

    #[must_use]
    pub const fn index(&self, z: usize, y: usize, x: usize) -> usize {
        (z * self.dims[1] + y) * self.dims[2] + x
    }

    /// The voxel containing `point_mm`, or `None` when it is outside.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "each quotient is range-checked against its own dimension before the cast, and a \
              negative or non-finite quotient fails the `>= 0.0` test first"
    )]
    pub fn voxel_of(&self, point_mm: [f64; 3]) -> Option<[usize; 3]> {
        let mut out = [0_usize; 3];
        // axis 0 of `out` is z, to match `dims`.
        for (axis, dim_axis) in [(2_usize, 0_usize), (1, 1), (0, 2)] {
            let coord = (point_mm[axis] - self.origin_mm[axis]) / self.pitch_mm;
            if !coord.is_finite() || coord < 0.0 || coord >= self.dims[dim_axis] as f64 {
                return None;
            }
            out[dim_axis] = coord as usize;
        }
        Some(out)
    }

    /// Centre of voxel `[z, y, x]`, as `[x, y, z]` in mm.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "a voxel index bounded by the caller's own grid size is exact in f64"
    )]
    pub fn center_mm(&self, voxel: [usize; 3]) -> [f64; 3] {
        [
            self.origin_mm[0] + (voxel[2] as f64 + 0.5) * self.pitch_mm,
            self.origin_mm[1] + (voxel[1] as f64 + 0.5) * self.pitch_mm,
            self.origin_mm[2] + (voxel[0] as f64 + 0.5) * self.pitch_mm,
        ]
    }
}

/// One model's escape field: where a bore of the registered radius can stand,
/// and which of those places connect to the outside of the build.
#[derive(Clone, Debug)]
pub struct PassageField {
    grid: PassageGrid,
    bore_radius_mm: f64,
    required_clearance_mm: f64,
    /// Passable **and** 6-connected to the outside of the build.
    escapes: Vec<bool>,
    clearance_mm: Vec<f32>,
    passable_voxels: usize,
    escaping_voxels: usize,
    exterior_seeds: usize,
}

/// What the field says about one drain outlet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutletVerdict {
    /// The invariant: a bore-wide passage runs from this outlet to the outside.
    pub connected: bool,
    /// The largest free radius any probe around the outlet reads, in mm. Beside
    /// `required_clearance_mm` this is the margin — without it, `connected:
    /// false` says a verdict and nothing about how close it came.
    pub best_clearance_mm: f64,
    /// The best clearance among probes that ALSO reach the outside. Zero when
    /// none does.
    pub best_escaping_clearance_mm: f64,
    pub required_clearance_mm: f64,
    /// Voxels examined around the outlet. A path-hit witness: without it,
    /// `connected: false` cannot be told apart from "the probe ball fell
    /// outside the grid and nothing was looked at".
    pub probed_voxels: usize,
}

/// Build the escape field for one occupancy.
///
/// `solid[grid.index(z, y, x)]` is true where the finished print has material —
/// model, strut, or plate. Everything else is empty space.
///
/// # The padding, and why the outside of the build is declared rather than
/// measured
///
/// [`crate::frozen_distance_transform`] pads its input with one voxel of
/// background on every side, so a free voxel at the boundary of the array it is
/// handed reads a clearance of about one pitch rather than its true one. Left
/// alone that makes the outside of the build the least passable place in the
/// whole field and every drain reads enclosed.
///
/// So the field is built on an array larger than `grid`: a band of
/// `ceil(required / pitch) + 1` voxels of **free** space on ±x, ±y and +z, wide
/// enough that no voxel of `grid` can see the EDT's own border within the
/// clearance it has to prove. That band is the outside of the build, and it is
/// where the flood is seeded — declared open air rather than measured, because
/// measuring it is exactly what the padding makes impossible.
///
/// **The caller owns the one obligation this creates**: `grid` must be wide
/// enough that nothing solid — no model, no strut — reaches its boundary, so
/// that the declared-open band really is open. The sweep sizes its grid from
/// the model bounding box grown by the largest tip radius plus this same
/// clearance, which is that obligation discharged.
///
/// There is **no band below**: the build plate is there. `-z` is padded solid,
/// so no seed appears under the model and no escape is ever claimed downward
/// through the plate.
///
/// # Errors
///
/// Only the voxel-stack and grid-size refusals
/// [`crate::frozen_distance_transform`] itself raises. **This never refuses a
/// model**: it answers a question, and a caller that cannot get an answer
/// warns.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "one band width, a positive ratio of two positive lengths, ceil-ed and then used only \
          as an array margin"
)]
pub fn passage_field(
    grid: PassageGrid,
    solid: &[bool],
    bore_radius_mm: f64,
) -> Result<PassageField, HollowError> {
    let voxels = grid.voxels();
    if solid.len() != voxels {
        return Err(HollowError::VoxelStack);
    }
    let [nz, ny, nx] = grid.dims;
    let required = core_clearance_threshold_mm(bore_radius_mm, grid.pitch_mm);
    let band = (required / grid.pitch_mm).ceil().max(1.0) as usize + 1;

    // Padded lattice: free band on +-x, +-y and +z; nothing added below, where
    // the plate is.
    let pz = nz.checked_add(band).ok_or(HollowError::VoxelGridTooLarge)?;
    let py = ny
        .checked_add(2 * band)
        .ok_or(HollowError::VoxelGridTooLarge)?;
    let px = nx
        .checked_add(2 * band)
        .ok_or(HollowError::VoxelGridTooLarge)?;
    pz.checked_mul(py)
        .and_then(|value| value.checked_mul(px))
        .ok_or(HollowError::VoxelGridTooLarge)?;
    let padded_index = |z: usize, y: usize, x: usize| (z * py + y) * px + x;
    let in_grid = |z: usize, y: usize, x: usize| {
        (z < nz && y >= band && y < band + ny && x >= band && x < band + nx)
            .then(|| grid.index(z, y - band, x - band))
    };

    // The EDT measures each `true` voxel's distance to the nearest `false` one,
    // so free space is the foreground here and solid is the background: what
    // comes back is the free radius at every empty voxel.
    let free = Array3::from_shape_fn((pz, py, px), |(z, y, x)| {
        in_grid(z, y, x).is_none_or(|cell| !solid[cell])
    });
    let clearance = frozen_distance_transform(&free, grid.pitch_mm)?;

    let padded_voxels = pz * py * px;
    let mut passable = vec![false; padded_voxels];
    let mut clearance_mm = vec![0.0_f32; voxels];
    let mut passable_voxels = 0_usize;
    let mut frontier: Vec<u32> = Vec::new();
    let mut escapes = vec![false; padded_voxels];
    // One pass over the padded raster, not one query per voxel: it fills the
    // clearance readout, the passable set and the seed frontier together.
    for z in 0..pz {
        for y in 0..py {
            for x in 0..px {
                let padded = padded_index(z, y, x);
                let inside = in_grid(z, y, x);
                let Some(cell) = inside else {
                    // The declared-open band. Free by construction, and the
                    // seed set for the flood.
                    passable[padded] = true;
                    passable_voxels += 1;
                    escapes[padded] = true;
                    frontier.push(u32::try_from(padded).unwrap_or(u32::MAX));
                    continue;
                };
                if solid[cell] {
                    continue;
                }
                let measured = clearance[[z, y, x]];
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "a mm-scale clearance bounded by the grid diagonal is exact to 1e-5 mm \
                          in f32, and this field is reported, never compared for equality"
                )]
                {
                    clearance_mm[cell] = measured as f32;
                }
                if measured >= required {
                    passable[padded] = true;
                    passable_voxels += 1;
                }
            }
        }
    }
    drop(clearance);
    let exterior_seeds = frontier.len();

    // A plain 6-connected wave. Every voxel enters the frontier at most once,
    // so this is O(voxels) and holds at most one entry per voxel.
    let mut escaping_voxels = exterior_seeds;
    while let Some(padded) = frontier.pop() {
        let padded = padded as usize;
        let x = padded % px;
        let y = (padded / px) % py;
        let z = padded / (px * py);
        let mut visit = |cell: usize, escapes: &mut Vec<bool>, frontier: &mut Vec<u32>| {
            if passable[cell] && !escapes[cell] {
                escapes[cell] = true;
                escaping_voxels += 1;
                frontier.push(u32::try_from(cell).unwrap_or(u32::MAX));
            }
        };
        if x > 0 {
            visit(padded - 1, &mut escapes, &mut frontier);
        }
        if x + 1 < px {
            visit(padded + 1, &mut escapes, &mut frontier);
        }
        if y > 0 {
            visit(padded - px, &mut escapes, &mut frontier);
        }
        if y + 1 < py {
            visit(padded + px, &mut escapes, &mut frontier);
        }
        if z > 0 {
            visit(padded - px * py, &mut escapes, &mut frontier);
        }
        if z + 1 < pz {
            visit(padded + px * py, &mut escapes, &mut frontier);
        }
    }

    // Project the padded verdict back onto the caller's own lattice, so every
    // reader below indexes with `grid` and never has to know about the band.
    let mut grid_escapes = vec![false; voxels];
    for z in 0..nz {
        for y in 0..ny {
            for x in 0..nx {
                grid_escapes[grid.index(z, y, x)] =
                    escapes[padded_index(z, y + band, x + band)];
            }
        }
    }

    Ok(PassageField {
        grid,
        bore_radius_mm,
        required_clearance_mm: required,
        escapes: grid_escapes,
        clearance_mm,
        passable_voxels,
        escaping_voxels,
        exterior_seeds,
    })
}


impl PassageField {
    #[must_use]
    pub const fn required_clearance_mm(&self) -> f64 {
        self.required_clearance_mm
    }

    #[must_use]
    pub const fn passable_voxels(&self) -> usize {
        self.passable_voxels
    }

    #[must_use]
    pub const fn escaping_voxels(&self) -> usize {
        self.escaping_voxels
    }

    #[must_use]
    pub const fn exterior_seeds(&self) -> usize {
        self.exterior_seeds
    }

    #[must_use]
    pub const fn grid(&self) -> PassageGrid {
        self.grid
    }

    /// Does empty space at least a bore DIAMETER wide run from this outlet to
    /// the outside of the build?
    ///
    /// # The probe ball, and why its radius is the bore's own
    ///
    /// The mouth of a bore of radius `r` cut through a wall cannot itself read
    /// a free radius above `r`: the rim is exactly `r` away. One bore radius
    /// **outside** the wall, on the axis, the nearest solid is the wall face at
    /// distance `r` or the rim at `sqrt(2)·r`, so the clearance has risen to
    /// `r` and keeps rising. The probe therefore looks over the ball of radius
    /// `2·r` around the outlet, restricted to the outward side of the outlet
    /// plane, which is the smallest ball guaranteed to contain a point that
    /// clears the threshold when the mouth is genuinely open. `2·r` is the
    /// bore's own diameter, not a distance constant.
    #[must_use]
    pub fn outlet(&self, outlet_mm: [f64; 3], outward_axis: [f64; 3]) -> OutletVerdict {
        let probe_radius_mm = 2.0 * self.bore_radius_mm;
        let axis_len =
            (outward_axis[0].mul_add(outward_axis[0], outward_axis[1] * outward_axis[1])
                + outward_axis[2] * outward_axis[2])
                .sqrt();
        let axis = if axis_len > 0.0 {
            [
                outward_axis[0] / axis_len,
                outward_axis[1] / axis_len,
                outward_axis[2] / axis_len,
            ]
        } else {
            // No usable axis: probe the whole ball rather than refuse to look.
            [0.0, 0.0, 0.0]
        };
        let mut verdict = OutletVerdict {
            connected: false,
            best_clearance_mm: 0.0,
            best_escaping_clearance_mm: 0.0,
            required_clearance_mm: self.required_clearance_mm,
            probed_voxels: 0,
        };
        let [nz, ny, nx] = self.grid.dims;
        let pitch = self.grid.pitch_mm;
        let half_span = (probe_radius_mm / pitch).ceil();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a positive ratio of two positive lengths, ceil-ed; the loop bounds below \
                  clamp it into the grid"
        )]
        let span = half_span as usize;
        let Some(centre) = self.grid.voxel_of(outlet_mm) else {
            return verdict;
        };
        let z_lo = centre[0].saturating_sub(span);
        let z_hi = (centre[0] + span + 1).min(nz);
        let y_lo = centre[1].saturating_sub(span);
        let y_hi = (centre[1] + span + 1).min(ny);
        let x_lo = centre[2].saturating_sub(span);
        let x_hi = (centre[2] + span + 1).min(nx);
        for z in z_lo..z_hi {
            for y in y_lo..y_hi {
                for x in x_lo..x_hi {
                    let centre_mm = self.grid.center_mm([z, y, x]);
                    let delta = [
                        centre_mm[0] - outlet_mm[0],
                        centre_mm[1] - outlet_mm[1],
                        centre_mm[2] - outlet_mm[2],
                    ];
                    let distance = (delta[0].mul_add(delta[0], delta[1] * delta[1])
                        + delta[2] * delta[2])
                        .sqrt();
                    if distance > probe_radius_mm {
                        continue;
                    }
                    let outward = delta[0].mul_add(axis[0], delta[1] * axis[1]) + delta[2] * axis[2];
                    if axis_len > 0.0 && outward < 0.0 {
                        continue;
                    }
                    verdict.probed_voxels += 1;
                    let cell = self.grid.index(z, y, x);
                    let clearance = f64::from(self.clearance_mm[cell]);
                    if clearance > verdict.best_clearance_mm {
                        verdict.best_clearance_mm = clearance;
                    }
                    if self.escapes[cell] {
                        verdict.connected = true;
                        if clearance > verdict.best_escaping_clearance_mm {
                            verdict.best_escaping_clearance_mm = clearance;
                        }
                    }
                }
            }
        }
        verdict
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(nz: usize, ny: usize, nx: usize, pitch: f64) -> PassageGrid {
        PassageGrid {
            dims: [nz, ny, nx],
            origin_mm: [0.0, 0.0, 0.0],
            pitch_mm: pitch,
        }
    }

    /// A closed box with a slot in its lid, and nothing else. The only route
    /// out of the chamber is the slot, so the slot's width alone decides
    /// whether a bore of a given radius escapes.
    fn slotted_box(g: PassageGrid, slot_half_voxels: usize) -> Vec<bool> {
        let mut solid = vec![false; g.voxels()];
        let mid = g.dims[2] / 2;
        for z in 2..18 {
            for y in 8..32 {
                for x in 8..32 {
                    let inner = (4..16).contains(&z) && (10..30).contains(&y) && (10..30).contains(&x);
                    let slot = (16..18).contains(&z)
                        && (10..30).contains(&y)
                        && x + slot_half_voxels >= mid
                        && x < mid + slot_half_voxels;
                    solid[g.index(z, y, x)] = !inner && !slot;
                }
            }
        }
        solid
    }

    #[test]
    fn an_outlet_in_open_air_is_connected() {
        let g = grid(40, 40, 40, 0.5);
        // A plate slab at the bottom and nothing else.
        let mut solid = vec![false; g.voxels()];
        for y in 0..40 {
            for x in 0..40 {
                solid[g.index(0, y, x)] = true;
            }
        }
        let field = passage_field(g, &solid, 2.5).expect("field");
        let verdict = field.outlet([10.0, 10.0, 10.0], [0.0, 0.0, 1.0]);
        assert!(verdict.probed_voxels > 0, "the probe must look at something");
        assert!(verdict.connected, "open air must connect: {verdict:?}");
    }

    #[test]
    fn a_sealed_pocket_is_enclosed() {
        let g = grid(40, 40, 40, 0.5);
        let mut solid = vec![true; g.voxels()];
        // A 6 x 6 x 6 voxel bubble of air inside a solid block, no route out.
        for z in 17..23 {
            for y in 17..23 {
                for x in 17..23 {
                    solid[g.index(z, y, x)] = false;
                }
            }
        }
        let field = passage_field(g, &solid, 0.5).expect("field");
        let verdict = field.outlet([10.0, 10.0, 10.0], [0.0, 0.0, 0.0]);
        assert!(verdict.probed_voxels > 0, "the probe must look at something");
        assert!(
            !verdict.connected,
            "a sealed pocket must not connect: {verdict:?}"
        );
    }

    /// The flood must not squeeze through a gap narrower than the bore
    /// DIAMETER, and must go through one that is wide enough. Same geometry,
    /// two bores: this is the invariant, not an incidental property of one
    /// fixture.
    #[test]
    fn the_flood_does_not_pass_a_gap_narrower_than_the_bore_diameter() {
        let g = grid(40, 40, 40, 0.5);
        // Slot 4 voxels = 2.0 mm wide: the widest ball that fits has radius
        // 1.0 mm.
        let solid = slotted_box(g, 2);
        let outlet = [10.0, 10.0, 4.0];
        let wide = passage_field(g, &solid, 1.5).expect("field");
        let narrow = passage_field(g, &solid, 0.5).expect("field");
        let wide_verdict = wide.outlet(outlet, [0.0, 0.0, 1.0]);
        let narrow_verdict = narrow.outlet(outlet, [0.0, 0.0, 1.0]);
        assert!(
            !wide_verdict.connected,
            "a 3.0 mm bore must not pass a 2.0 mm slot: {wide_verdict:?}"
        );
        assert!(
            narrow_verdict.connected,
            "a 1.0 mm bore must pass a 2.0 mm slot: {narrow_verdict:?}"
        );
    }

    /// Widening the same slot flips the same bore's verdict. Without this the
    /// test above passes for a field that never connects anything.
    #[test]
    fn widening_the_slot_connects_the_bore_that_the_narrow_slot_refused() {
        let g = grid(40, 40, 40, 0.5);
        // Slot 10 voxels = 5.0 mm wide: a 1.5 mm bore needs 1.75 mm of
        // clearance and has 2.5 mm.
        let solid = slotted_box(g, 5);
        let field = passage_field(g, &solid, 1.5).expect("field");
        let verdict = field.outlet([10.0, 10.0, 4.0], [0.0, 0.0, 1.0]);
        assert!(
            verdict.connected,
            "a 3.0 mm bore must pass a 5.0 mm slot: {verdict:?}"
        );
    }
}
