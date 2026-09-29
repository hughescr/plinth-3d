//! Drain connectivity: the support field must not wall a drain outlet in.
//!
//! The hollow stage drills a bore so resin can leave the cavity. Placement then
//! ran with no knowledge that the bore exists, and the owner opened the result
//! in ChituBox on 2026-09-03 and found the Sphere's outlet "completely covered
//! by a clutter of thick overlapping supports at the bottom". His own statement
//! of the remedy is the invariant:
//!
//! > a clearance check need only happen to a limited distance — though we don't
//! > want the outlet to then be enclosed by solid supports — or we would have
//! > just in effect created ANOTHER hollow space which would need a drain.
//!
//! This module does two things with one set of drain records:
//!
//! 1. **A keep-out** ([`DrainKeepOut`]) the placement reads, so no arm is
//!    seated in, or descends through, the cylinder that the resin has to leave
//!    by. Radius `drain radius + TIP_RADIUS_MAX_LEGACY_MM + STANDOFF_MM`: the
//!    bore itself, the largest an arm may be sized to (the keep-out is built
//!    before anything is sized, and an arm is measured to its SURFACE), and the
//!    registered placement-family gap every other placement search demands.
//! 2. **An emit-time check** ([`check_escape`]), which floods the empty space
//!    from each outlet with the struts rasterised into the solid set and
//!    publishes `connected` / `enclosed` per drain. A drain that is enclosed is
//!    a loud WARNING on an emitted file, never a refusal (owner ruling
//!    2026-09-01).
//!
//! The flood itself is `plinth_hollow::passage`, which is built out of the
//! drill stage's own conservative EDT and its own `r + pitch/2` clearance
//! correction, run at the registered hollowing pitch. The check therefore
//! decides "does the bore fit through here" with the same machinery, the same
//! rule and the same resolution the drill stage decided the identical question
//! with. See that module for what could not be reused and why.

use std::sync::atomic::{AtomicU64, Ordering};

use plinth_hollow::passage::{passage_field, OutletVerdict, PassageGrid};

use crate::grid::Grid;
use crate::raster::{rasterize_into, LayerMasks, RasterScratch};
use crate::SweepStrut;
use plinth_support::perception::SliceSolid;

/// One drilled drain, in the same frame as the mesh the sweep is placing on.
///
/// The hollow stage runs after orientation, so its `DrainRecord` coordinates
/// and this sweep's mesh are already the same frame; the pipeline converts one
/// to the other with no transform, and this type exists so `plinth-sweep`'s
/// public surface does not force a `plinth-hollow` type on its callers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrainSpec {
    /// The bore's mouth on the OUTSIDE of the model, `[x, y, z]` in mm.
    pub outlet_mm: [f64; 3],
    /// Unit-ish direction pointing OUT of the model at the outlet. Normalized
    /// on use; a zero vector means "no usable axis" and is handled, not
    /// refused.
    pub outward_axis: [f64; 3],
    pub radius_mm: f64,
}

impl DrainSpec {
    /// Build one from an entrance/exit pair: the axis points from the cavity
    /// side to the outside, which is the direction the resin leaves by.
    #[must_use]
    pub fn from_bore(entrance_mm: [f64; 3], exit_mm: [f64; 3], radius_mm: f64) -> Self {
        Self {
            outlet_mm: exit_mm,
            outward_axis: [
                exit_mm[0] - entrance_mm[0],
                exit_mm[1] - entrance_mm[1],
                exit_mm[2] - entrance_mm[2],
            ],
            radius_mm,
        }
    }
}

/// Where no arm may stand, because the resin leaves that way.
///
/// One `u32` per working cell: the LOWEST sweep layer at which some drain's
/// escape cylinder covers that cell, or `u32::MAX` for a cell no cylinder ever
/// touches. A strut seated at layer `k` in cell `c` is refused exactly when
/// `lowest_blocked[c] <= k`, because descent is straight down and such a strut
/// either stands in the cylinder or passes through it on the way to its
/// landing.
///
/// **The direction of the error, stated rather than discovered.** The test
/// refuses a strut that would have terminated on a carrier *above* the cylinder
/// without ever entering it. That over-refuses in one case (a drain whose axis
/// points sideways or upward, with a legitimate landing above the bore) and
/// never under-refuses. Struts BELOW the cylinder are not refused, which is
/// what keeps a sideways drain from carving the whole column out of the field.
///
/// `min` over drains is exact for the union: some cylinder covers `c` at or
/// below `k` iff the smallest such layer is at or below `k`.
#[derive(Debug, Default)]
pub struct DrainKeepOut {
    lowest_blocked: Vec<u32>,
    blocked_cells: usize,
    /// How many times a placement candidate was refused by this keep-out. A
    /// path-hit witness: without it, "the drains are all connected" cannot be
    /// told apart from "the keep-out never ran". Atomic rather than a `Cell`
    /// only so the type stays `Sync`: `orient_demand` drives whole sweeps from
    /// a rayon pool, and a keep-out that could not cross a thread boundary
    /// would silently constrain where the sweep may be called from.
    refusals: AtomicU64,
}

impl Clone for DrainKeepOut {
    fn clone(&self) -> Self {
        Self {
            lowest_blocked: self.lowest_blocked.clone(),
            blocked_cells: self.blocked_cells,
            refusals: AtomicU64::new(self.refusals()),
        }
    }
}

impl DrainKeepOut {
    /// An empty keep-out: no drains, nothing refused, no allocation.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Stamp every drain's escape cylinder into the working raster.
    ///
    /// The cylinder runs from the outlet along the outward axis until it leaves
    /// the layer stack, with radius `drain radius + standoff`. It is stamped by
    /// marching the axis in half-pitch steps and, at each sample, writing the
    /// lowest layer the ball of that radius reaches over each covered cell —
    /// `z_sample - sqrt(R^2 - d^2)` for a cell at horizontal distance `d`. Half
    /// a pitch between samples is below the cell size, so consecutive balls
    /// overlap and the marched union is the capsule.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "cell and layer indices are bounded by the working raster the caller already \
              budgeted; every quotient below is clamped into that range before it is used as an \
              index, and a layer index is exact in f64 far beyond the budget"
    )]
    pub fn build(
        grid: &Grid,
        drains: &[DrainSpec],
        standoff_mm: f64,
        max_arm_radius_mm: f64,
        layers: usize,
        plate_z_mm: f64,
        layer_height_mm: f64,
    ) -> Self {
        if drains.is_empty() || layers == 0 {
            return Self::none();
        }
        let mut lowest_blocked = vec![u32::MAX; grid.cells()];
        let top_z = plate_z_mm + (layers as f64) * layer_height_mm;
        let step = 0.5 * grid.pitch_mm();
        // One stamp of the ball of `radius` centred on `point`: for every cell
        // the ball covers, the LOWEST z it reaches over that cell, as a layer.
        let stamp = |point: [f64; 3], radius: f64, lowest: &mut [u32]| {
            let lo_col = grid.col_coord(point[0] - radius).floor().max(0.0) as usize;
            let hi_col = (grid.col_coord(point[0] + radius).ceil().max(0.0) as usize)
                .min(grid.cols().saturating_sub(1));
            let lo_row = grid.row_coord(point[1] - radius).floor().max(0.0) as usize;
            let hi_row = (grid.row_coord(point[1] + radius).ceil().max(0.0) as usize)
                .min(grid.rows().saturating_sub(1));
            if lo_col > hi_col || lo_row > hi_row {
                return;
            }
            for row in lo_row..=hi_row {
                let dy = grid.center_y_mm(row) - point[1];
                for col in lo_col..=hi_col {
                    let dx = grid.center_x_mm(col) - point[0];
                    let horizontal = dx.mul_add(dx, dy * dy);
                    if horizontal > radius * radius {
                        continue;
                    }
                    let drop = (radius * radius - horizontal).sqrt();
                    let z = point[2] - drop;
                    let layer = ((z - plate_z_mm) / layer_height_mm).floor().max(0.0) as usize;
                    if layer >= layers {
                        continue;
                    }
                    let cell = grid.index(row, col);
                    let value = layer as u32;
                    if value < lowest[cell] {
                        lowest[cell] = value;
                    }
                }
            }
        };

        // How far a ray can run before it is certainly outside the stack in
        // every direction. The working volume's diagonal bounds it, and
        // marching past the edge costs nothing because every stamp is clipped.
        let span_x = grid.pitch_mm() * grid.cols() as f64;
        let span_y = grid.pitch_mm() * grid.rows() as f64;
        let span_z = top_z - plate_z_mm;
        let diagonal = (span_x.mul_add(span_x, span_y * span_y) + span_z * span_z).sqrt();
        let samples = (diagonal / step).ceil().max(1.0) as usize;

        for drain in drains {
            // The cylinder is measured to an arm's AXIS, but the passage is
            // measured to its SURFACE, so the arm's own radius has to be in
            // here or the keep-out clears a hole the arms then close again.
            // `max_arm_radius_mm` is the registered maximum tip radius: the
            // down-sweep has not sized anything when this is built, and the
            // largest an arm may become is the only bound that is true before
            // it is sized. `standoff_mm` on top is STANDOFF_MM, the registered
            // placement-family surface-to-surface gap.
            let radius = drain.radius_mm + max_arm_radius_mm + standoff_mm;
            if !radius.is_finite() || radius <= 0.0 {
                continue;
            }
            let axis_len = (drain.outward_axis[0].mul_add(
                drain.outward_axis[0],
                drain.outward_axis[1] * drain.outward_axis[1],
            ) + drain.outward_axis[2] * drain.outward_axis[2])
                .sqrt();
            // No axis is not a reason to skip the keep-out: stamp the ball at
            // the outlet alone, which is the part of the cylinder that is
            // certainly needed.
            let axis = if axis_len > 0.0 {
                [
                    drain.outward_axis[0] / axis_len,
                    drain.outward_axis[1] / axis_len,
                    drain.outward_axis[2] / axis_len,
                ]
            } else {
                [0.0, 0.0, 0.0]
            };
            let ray_samples = if axis_len > 0.0 { samples } else { 0 };
            for sample in 0..=ray_samples {
                let t = sample as f64 * step;
                let point = [
                    drain.outlet_mm[0] + axis[0] * t,
                    drain.outlet_mm[1] + axis[1] * t,
                    drain.outlet_mm[2] + axis[2] * t,
                ];
                if point[2] - radius > top_z {
                    break;
                }
                stamp(point, radius, &mut lowest_blocked);
            }

            // --- what is deliberately NOT here: a gallery --------------------
            //
            // A cylinder along the axis is not, on its own, enough to make a
            // DOWNWARD drain drain. Its cylinder ends at the plate, still under
            // the model, and the resin then has to cross the arm field sideways
            // to leave the build. At the registered spacing it cannot: arms `s`
            // apart with radius `a` leave a free radius of `s/2 - a` between
            // them, below what the bore needs. The cleared cylinder is a POCKET
            // with no way out, which is the owner's own description of the
            // defect.
            //
            // A horizontal channel from the foot of the cylinder out past the
            // silhouette WAS built and measured on this branch, and it is not
            // shipped, because the measurement said what it costs. Every arm in
            // that channel's columns descends straight down through it and so
            // has to go, at every layer:
            //
            //   Sphere    139 -> 102 arms, residual demand 0 -> 3199.69 mm2,
            //             uncarried load 0 -> 400.49 mm2
            //   Cylinder  408 -> 377 arms, residual demand 0 -> 5091.55 mm2
            //
            // A drain that vents at the price of leaving thousands of mm2 of
            // model unsupported is not a better file. The channel becomes
            // affordable exactly when a descending arm can LEAN around it
            // instead of being deleted -- angled descent and trunk merging,
            // which this engine does not have yet. Until then the honest
            // outcome is the one below: keep the mouth itself clear, measure
            // the escape on the finished field, and WARN when the field still
            // encloses it. See `check_escape`.
        }
        let blocked_cells = lowest_blocked
            .iter()
            .filter(|value| **value != u32::MAX)
            .count();
        Self {
            lowest_blocked,
            blocked_cells,
            refusals: AtomicU64::new(0),
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.lowest_blocked.is_empty()
    }

    #[must_use]
    pub const fn blocked_cells(&self) -> usize {
        self.blocked_cells
    }

    /// How many placement candidates this keep-out has refused.
    #[must_use]
    pub fn refusals(&self) -> u64 {
        self.refusals.load(Ordering::Relaxed)
    }

    /// Whether the keep-out covers `cell` at `layer` — [`Self::admits`]'s
    /// answer, negated, WITHOUT counting a refusal. For a check on a finished
    /// field (the footprint rule's move check) rather than a placement
    /// candidate, so the published refusal count keeps meaning "placement
    /// candidates refused".
    #[must_use]
    pub fn blocks(&self, cell: usize, layer: usize) -> bool {
        self.lowest_blocked.get(cell).is_some_and(|&lowest| {
            lowest != u32::MAX && usize::try_from(lowest).unwrap_or(usize::MAX) <= layer
        })
    }

    /// May an arm be seated in `cell` at `layer`?
    ///
    /// Counts its own refusals, so a run can prove the keep-out was consulted.
    #[must_use]
    pub fn admits(&self, cell: usize, layer: usize) -> bool {
        let Some(&lowest) = self.lowest_blocked.get(cell) else {
            return true;
        };
        if lowest == u32::MAX || usize::try_from(lowest).unwrap_or(usize::MAX) > layer {
            return true;
        }
        self.refusals.fetch_add(1, Ordering::Relaxed);
        false
    }
}

/// What the escape flood said about one drain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrainVerdict {
    pub outlet_mm: [f64; 3],
    pub radius_mm: f64,
    pub connected: bool,
    pub best_clearance_mm: f64,
    pub required_clearance_mm: f64,
    pub probed_voxels: usize,
}

impl DrainVerdict {
    fn from_outlet(spec: &DrainSpec, verdict: OutletVerdict) -> Self {
        Self {
            outlet_mm: spec.outlet_mm,
            radius_mm: spec.radius_mm,
            connected: verdict.connected,
            best_clearance_mm: verdict.best_clearance_mm,
            required_clearance_mm: verdict.required_clearance_mm,
            probed_voxels: verdict.probed_voxels,
        }
    }
}

/// Everything one escape check measured, including the witnesses that say it
/// ran at all.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EscapeReport {
    pub verdicts: Vec<DrainVerdict>,
    pub voxel_pitch_mm: f64,
    pub voxels: usize,
    pub solid_voxels: usize,
    pub passable_voxels: usize,
    pub escaping_voxels: usize,
}

impl EscapeReport {
    #[must_use]
    pub fn enclosed(&self) -> usize {
        self.verdicts
            .iter()
            .filter(|verdict| !verdict.connected)
            .count()
    }
}

/// Flood the empty space around the finished field and say, per drain, whether
/// the resin can get out.
///
/// The occupancy handed to the flood is model **plus struts plus plate**, which
/// is the point: a check that ignored the struts could never see the defect it
/// exists to find.
///
/// # Batch-first
///
/// One rasterisation pass builds the whole occupancy, one EDT and one flood
/// cover the whole model, and each drain is then a ball lookup. Nothing here is
/// per-drain except the lookup.
#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "the occupancy is genuinely the working raster, the slice stack, the strut list, the \
          frame's plate and height, the drains and the two registered lengths; a struct would \
          rename the list rather than shorten it"
)]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "voxel counts and indices bounded by the coarse passage lattice this function lays \
          out itself; every quotient is clamped into its own dimension before use"
)]
pub fn check_escape(
    grid: &Grid,
    slices: &[Vec<SliceSolid>],
    struts: &[SweepStrut],
    drains: &[DrainSpec],
    plate_z_mm: f64,
    layer_height_mm: f64,
    voxel_pitch_mm: f64,
    tip_radius_max_mm: f64,
) -> EscapeReport {
    let mut report = EscapeReport {
        voxel_pitch_mm,
        ..EscapeReport::default()
    };
    if drains.is_empty() || slices.is_empty() || voxel_pitch_mm <= 0.0 {
        return report;
    }
    // The passage lattice must extend past anything solid on every side but
    // the bottom, which is `plinth_hollow::passage`'s one obligation on its
    // caller. A strut centre lies inside the working raster and its disc can
    // reach one maximum tip radius beyond it, so one radius plus one voxel of
    // slack is enough, and the bottom row sits below the plate on purpose.
    let pad = tip_radius_max_mm + voxel_pitch_mm;
    let origin_x = grid.center_x_mm(0) - 0.5 * grid.pitch_mm() - pad;
    let origin_y = grid.center_y_mm(0) - 0.5 * grid.pitch_mm() - pad;
    let origin_z = plate_z_mm - voxel_pitch_mm;
    let span_x = grid.pitch_mm() * grid.cols() as f64 + 2.0 * pad;
    let span_y = grid.pitch_mm() * grid.rows() as f64 + 2.0 * pad;
    let span_z = (slices.len() as f64) * layer_height_mm + voxel_pitch_mm + pad;
    let nx = ((span_x / voxel_pitch_mm).ceil().max(1.0) as usize).max(1);
    let ny = ((span_y / voxel_pitch_mm).ceil().max(1.0) as usize).max(1);
    let nz = ((span_z / voxel_pitch_mm).ceil().max(1.0) as usize).max(1);
    let passage = PassageGrid {
        dims: [nz, ny, nx],
        origin_mm: [origin_x, origin_y, origin_z],
        pitch_mm: voxel_pitch_mm,
    };
    let mut solid = vec![false; passage.voxels()];

    // The plate: every voxel whose centre is below the plate plane. The bottom
    // row is entirely below it by construction, so the flood can never claim an
    // escape downward.
    for z in 0..nz {
        let centre_z = origin_z + (z as f64 + 0.5) * voxel_pitch_mm;
        if centre_z >= plate_z_mm {
            break;
        }
        for y in 0..ny {
            for x in 0..nx {
                solid[passage.index(z, y, x)] = true;
            }
        }
    }

    // The model. Every print layer is OR-ed into the voxel row that contains
    // its plane, so a feature thinner than one voxel is still solid — under-
    // stating the solid would over-state the passage, which is the one
    // direction this check must never err in.
    let coarse = crate::grid::lay_out_at(origin_x, origin_y, voxel_pitch_mm, nx, ny);
    let mut scratch = RasterScratch::new(&coarse);
    let mut masks = LayerMasks::default();
    for (layer, solids) in slices.iter().enumerate() {
        let Some(rows) =
            voxel_rows_for_layer(layer, plate_z_mm, layer_height_mm, origin_z, voxel_pitch_mm, nz)
        else {
            continue;
        };
        rasterize_into(&coarse, solids, &mut scratch, &mut masks);
        for z in rows {
            for row in 0..coarse.rows() {
                for col in 0..coarse.cols() {
                    if masks.cover[coarse.index(row, col)] {
                        solid[passage.index(z, row, col)] = true;
                    }
                }
            }
        }
    }

    // The struts. Per voxel row, the largest radius the arm has anywhere in
    // that row, stamped as a disc: an arm flares downward, so taking the
    // maximum over the row is the conservative read.
    for strut in struts {
        if strut.k_bottom == usize::MAX || strut.radius_mm.is_empty() {
            continue;
        }
        for (slot, &radius) in strut.radius_mm.iter().enumerate() {
            if !radius.is_finite() || radius <= 0.0 {
                continue;
            }
            let layer = strut.k_bottom + slot;
            let Some(rows) =
                voxel_rows_for_layer(layer, plate_z_mm, layer_height_mm, origin_z, voxel_pitch_mm, nz)
            else {
                continue;
            };
            // Any-overlap, the same conservative rule the model raster uses: a
            // voxel counts as solid when the arm's disc touches its square at
            // all, not only when it covers the centre. Marking on centres alone
            // was measured to under-state every arm by up to half a voxel on
            // each side, which widens every gap in the field by a whole voxel
            // and let a bore-wide ball through a throat that is really 0.45 mm.
            // Over-stating solid is the direction this check must err in.
            let radius = radius + 0.5 * core::f64::consts::SQRT_2 * voxel_pitch_mm;
            // The arm's centre AT THIS LAYER. A leaned arm is a different
            // column at every layer, and stamping its seat column all the way
            // down would both wall off a passage the resin really has and open
            // one it really does not.
            let center_mm = if strut.path.is_empty() {
                strut.center_mm
            } else {
                let here = strut.cell_at(layer);
                [
                    grid.center_x_mm(grid.col_of(here)),
                    grid.center_y_mm(grid.row_of(here)),
                ]
            };
            let lo_x = (((center_mm[0] - radius - origin_x) / voxel_pitch_mm).floor())
                .max(0.0) as usize;
            let hi_x = ((((center_mm[0] + radius - origin_x) / voxel_pitch_mm).ceil())
                .max(0.0) as usize)
                .min(nx.saturating_sub(1));
            let lo_y = (((center_mm[1] - radius - origin_y) / voxel_pitch_mm).floor())
                .max(0.0) as usize;
            let hi_y = ((((center_mm[1] + radius - origin_y) / voxel_pitch_mm).ceil())
                .max(0.0) as usize)
                .min(ny.saturating_sub(1));
            for y in lo_y..=hi_y {
                let dy = origin_y + (y as f64 + 0.5) * voxel_pitch_mm - center_mm[1];
                for x in lo_x..=hi_x {
                    let dx = origin_x + (x as f64 + 0.5) * voxel_pitch_mm - center_mm[0];
                    if dx.mul_add(dx, dy * dy) <= radius * radius {
                        for z in rows.clone() {
                            solid[passage.index(z, y, x)] = true;
                        }
                    }
                }
            }
        }
    }

    report.voxels = passage.voxels();
    report.solid_voxels = solid.iter().filter(|value| **value).count();
    // The registered bore radius is the same for every drain the drill stage
    // places, so one field serves them all; a model whose drains differ gets
    // one field per distinct radius, still not one per drain.
    let mut radii: Vec<f64> = drains.iter().map(|drain| drain.radius_mm).collect();
    radii.sort_by(f64::total_cmp);
    radii.dedup_by(|a, b| (*a - *b).abs() < f64::EPSILON);
    for radius in radii {
        let Ok(field) = passage_field(passage, &solid, radius) else {
            // The EDT refused the lattice. That is a fact about this machine's
            // budget, not a verdict on the model: say nothing about the drains
            // rather than say something false, and emit.
            continue;
        };
        report.passable_voxels = report.passable_voxels.max(field.passable_voxels());
        report.escaping_voxels = report.escaping_voxels.max(field.escaping_voxels());
        for drain in drains {
            if (drain.radius_mm - radius).abs() >= f64::EPSILON {
                continue;
            }
            report.verdicts.push(DrainVerdict::from_outlet(
                drain,
                field.outlet(drain.outlet_mm, drain.outward_axis),
            ));
        }
    }
    report
}

#[cfg(test)]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test-only cell lookups on a grid these tests lay out themselves; every coordinate is \
          inside it by construction"
)]
mod tests {
    use super::*;
    use crate::grid::plan_grid;

    #[test]
    fn a_keep_out_refuses_the_column_the_resin_leaves_by_and_admits_the_rest() {
        let (grid, _) = plan_grid(-10.0, -10.0, 10.0, 10.0, 100, 0.5, 1.0, 2_000_000_000);
        let drain = DrainSpec {
            outlet_mm: [0.0, 0.0, 5.0],
            outward_axis: [0.0, 0.0, -1.0],
            radius_mm: 1.25,
        };
        let keep_out = DrainKeepOut::build(&grid, &[drain], 0.6, 1.0, 100, 0.0, 0.05);
        assert!(keep_out.blocked_cells() > 0, "the cylinder must cover cells");
        // Straight under the outlet, at any layer: refused.
        let under = grid.index(
            grid.row_coord(0.0).floor() as usize,
            grid.col_coord(0.0).floor() as usize,
        );
        assert!(!keep_out.admits(under, 99));
        assert!(!keep_out.admits(under, 0));
        // Well outside the cylinder: admitted.
        let away = grid.index(
            grid.row_coord(8.0).floor() as usize,
            grid.col_coord(8.0).floor() as usize,
        );
        assert!(keep_out.admits(away, 99));
        assert!(keep_out.refusals() >= 2, "refusals must be counted");
    }

    #[test]
    fn an_empty_keep_out_admits_everything_and_allocates_nothing() {
        let keep_out = DrainKeepOut::none();
        assert!(keep_out.is_empty());
        assert!(keep_out.admits(0, 0));
        assert!(keep_out.admits(usize::MAX, usize::MAX));
        assert_eq!(keep_out.refusals(), 0);
    }
}

/// The voxel rows a print layer occupies: every row its slab
/// `[z_lo, z_hi)` touches, not only the row that contains its mid-plane. With
/// the registered 0.05 mm layer under a 0.5 mm voxel the two agree, but a
/// layer taller than a voxel would otherwise leave whole rows unstamped — a
/// horizontal tunnel through solid material that the flood would then report
/// as a passage. Under-stating the solid is the one direction this check must
/// never err in, so the range is the rule and the mid-plane is not.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "both values are floored, finite, clamped to >= 0 and bounded by nz before the cast"
)]
fn voxel_rows_for_layer(
    layer: usize,
    plate_z_mm: f64,
    layer_height_mm: f64,
    origin_z: f64,
    voxel_pitch_mm: f64,
    nz: usize,
) -> Option<core::ops::RangeInclusive<usize>> {
    let z_lo = plate_z_mm + layer as f64 * layer_height_mm;
    let z_hi = z_lo + layer_height_mm;
    let lo = ((z_lo - origin_z) / voxel_pitch_mm).floor();
    // The slab's top edge lies exactly on a row boundary when it is a whole
    // number of pitches up; that row is not touched, hence the open interval.
    let hi = ((z_hi - origin_z) / voxel_pitch_mm - f64::EPSILON).floor();
    if !lo.is_finite() || !hi.is_finite() || hi < 0.0 || nz == 0 {
        return None;
    }
    let lo = lo.max(0.0) as usize;
    let hi = (hi.max(0.0) as usize).min(nz - 1);
    (lo <= hi).then_some(lo..=hi)
}

#[cfg(test)]
mod voxel_row_tests {
    use super::voxel_rows_for_layer;

    #[test]
    fn a_layer_thinner_than_a_voxel_lands_in_one_row() {
        // 0.05 mm layers under 0.5 mm voxels: layer 7 spans 0.35..0.40, row 0.
        assert_eq!(voxel_rows_for_layer(7, 0.0, 0.05, 0.0, 0.5, 10), Some(0..=0));
        // Layer 10 spans 0.50..0.55, row 1 — the boundary is not double-counted.
        assert_eq!(voxel_rows_for_layer(10, 0.0, 0.05, 0.0, 0.5, 10), Some(1..=1));
    }

    #[test]
    fn a_layer_taller_than_a_voxel_stamps_every_row_it_spans() {
        // A 1.2 mm layer under 0.5 mm voxels spans 0.0..1.2: rows 0, 1 and 2.
        assert_eq!(voxel_rows_for_layer(0, 0.0, 1.2, 0.0, 0.5, 10), Some(0..=2));
        // A layer exactly two voxels tall touches two rows, not three.
        assert_eq!(voxel_rows_for_layer(0, 0.0, 1.0, 0.0, 0.5, 10), Some(0..=1));
    }

    #[test]
    fn rows_outside_the_volume_are_clamped_or_absent() {
        assert_eq!(voxel_rows_for_layer(0, 0.0, 1.0, 0.0, 0.5, 1), Some(0..=0));
        assert_eq!(voxel_rows_for_layer(50, 0.0, 0.05, 0.0, 0.5, 3), None);
        assert_eq!(voxel_rows_for_layer(0, -2.0, 0.05, 0.0, 0.5, 3), None);
    }
}
