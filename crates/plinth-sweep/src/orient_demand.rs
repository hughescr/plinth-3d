//! The concrete `plinth_orient::demand::SupportDemandMeasurer`, wired to the
//! real coarse-raster sweep (LANE 3, `docs/plans/slice-sweep-core-2026-09-02.md`
//! §12).
//!
//! This lives in `plinth-sweep`, not `plinth-orient`, because `plinth-orient`
//! cannot depend on `plinth-sweep` (`plinth_orient::demand`'s own module doc
//! names the cycle: `orient -> sweep -> support -> hollow -> orient`).
//! `plinth-sweep` depending on `plinth-orient` instead introduces no cycle —
//! nothing `plinth-orient` depends on (`plinth-core`, `plinth-decimate`,
//! `plinth-mesh`, `geom_kernel`) reaches back to `plinth-sweep` — so the
//! trait lives upstream and this implementation lives downstream, the usual
//! shape of a dependency-inversion seam. `plinth-pipeline`, which already
//! depends on both crates, wires a [`CoarseSweepDemand`] into
//! `plinth_orient::search_and_orient`.
//!
//! # Why coarser, and why that is a disclosed cost decision and not a physics one
//!
//! Running the sweep at the production `SWEEP_WORKING_PITCH_MM` for every
//! commit-tier candidate (bounded at `ORIENT_COMMIT_TOP_K` plus the escorted
//! legacy 13 — LANE 1/2's own measurement: up to ~16 candidates per model) is
//! affordable on the discs (single-digit seconds each) but not on a tall or
//! high-face-count model: LANE 2 measured 76.85 s for one production sweep of
//! the Weeping Treant, and [`CoarseSweepDemand`] runs one sweep per admitted
//! candidate, serially, inside the orientation stage of every prepare run.
//! Coarsening the XY raster pitch (never the Z layer height, which sets
//! `kappa * height` and must stay at the production value or the reach
//! physics itself would be distorted, not merely made cheaper) cuts
//! `cells_per_layer` roughly with the square of the pitch ratio while leaving
//! the layer count — the true driver of wall clock on a tall pose — alone.
//! [`DEMAND_PITCH_MULTIPLIER`] is registered nowhere: it is a hand-picked
//! cost/fidelity trade the pitch-halving invariance test
//! (`tests/pitch_invariance.rs`, a 2x pitch change moved a disc's strut count
//! 3.4%) licenses, stated here as a deviation from a literal "run the
//! production sweep on every candidate" reading of the design rather than
//! hidden inside a constant that would misleadingly claim registry
//! provenance for a pure performance knob.

use plinth_core::{m0_constants, LayerPlan};
use plinth_mesh::Mesh;
use plinth_orient::demand::{MeasuredSupportDemand, SupportDemandMeasurer};
use plinth_orient::UpDirection;

use crate::{run_sweep, SweepPolicy};

/// See the module doc's "Why coarser" section.
const DEMAND_PITCH_MULTIPLIER: f64 = 10.0;

/// The one production [`SupportDemandMeasurer`]. Zero-sized: every number it
/// needs is read fresh from the registry on each call, so two instances can
/// never disagree.
#[derive(Clone, Copy, Debug, Default)]
pub struct CoarseSweepDemand;

impl SupportDemandMeasurer for CoarseSweepDemand {
    fn measure_support_area_cm2(
        &self,
        mesh: &Mesh,
        candidate: UpDirection,
    ) -> Result<MeasuredSupportDemand, String> {
        // Oriented exactly as the eventual winner would be
        // (`plinth_orient::oriented_recentered_mesh` is the same
        // `transform_for`/`transform_mesh` pair `search_and_orient` applies
        // to the committed winner), so a candidate measured well here is
        // measured in the frame it would actually be built in.
        let transformed =
            plinth_orient::oriented_recentered_mesh(mesh, candidate).map_err(|error| error.to_string())?;

        let constants = m0_constants().map_err(|error| error.to_string())?;
        let lift_mm = constants.placeholders().lift_mm().diagnostic().get();
        let layer_height_mm = *constants.evidence().layer_height_default_mm().value();

        // `oriented_recentered_mesh` recenters translation so the
        // transformed mesh's own minimum Z is exactly 0
        // (`plinth-orient`'s `transform_mesh`: `translation = [.., -min[2]]`),
        // so the plate sits at `-lift_mm` for every candidate without
        // re-deriving its own minimum here.
        let plate_z = -lift_mm;
        let max_z = max_z_mm(&transformed)?;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a layer count is a finite positive quotient of two model-scale lengths, ceil-ed \
                  and clamped to at least one, exactly as crates/plinth-pipeline/src/prepare.rs's \
                  sweep_and_emit computes its own sweep layer plan"
        )]
        let layer_count = (((max_z - plate_z) / layer_height_mm).ceil().max(1.0) as usize)
            .saturating_add(1)
            .max(1);
        let layers = LayerPlan::new(plate_z, vec![layer_height_mm; layer_count])
            .map_err(|error| error.to_string())?;

        let policy = SweepPolicy::from_registry().map_err(|error| error.to_string())?;
        let coarse_pitch = policy.working_pitch_mm * DEMAND_PITCH_MULTIPLIER;
        let coarse_policy = policy.with_working_pitch_mm(coarse_pitch);

        let plan = run_sweep(&transformed, &layers, &coarse_policy).map_err(|error| error.to_string())?;
        Ok(MeasuredSupportDemand {
            // # This is the scar, and the scar is a WEAK proxy for demand
            //
            // §C2 states `total scar = peak_mm2 / X_ALLOWED` and the oracles
            // measure that identity at ratio 1.00-1.13, so scoring a pose by
            // its scar is very nearly scoring it by its peak cross-section —
            // the `w_peak * peak_xsec_cm2` term §12 retired, re-entering under
            // the name of the term that replaced it. Measured consequences, both
            // ways:
            //
            // * It cannot see a 180° flip at all. Round 25's identity pose
            //   (60 arms) and its flipped pose (83 arms) return the SAME peak
            //   (432.3175 mm²) and the SAME scar (40.5440 mm²) to four decimals.
            // * It rewards tilting, because tilting shrinks the peak: Round 100
            //   at 10° has a 682.85 mm² peak against 7630.94 mm² flat.
            //
            // **`plan.report.demand_area_mm2` — the model area the up-sweep
            // actually found unrooted — was built and substituted here, and it
            // is measurably WORSE.** It removed Round 25's flip, and then tilted
            // every model in the acceptance set by 10°: Round 25 to 4 arms,
            // Round 50 to 13, Round 100 to 48, and `make_overhang_table` off its
            // correct identity pose to 27 arms against 140. The reason is
            // upstream of this line: while `reach::weight` spends the cured
            // thickness `τ`, a tilted underside is reached from its low edge and
            // measures almost no demand, so ANY faithful demand measure prefers
            // the tilt. The demand quantity and the `τ` ruling are one decision,
            // not two, and the scar stays here until that ruling is taken.
            support_area_cm2: plan.total_scar_mm2() / 100.0,
            struts: plan.struts.len(),
            achieved_pitch_mm: plan.report.achieved_pitch_mm,
            layers: plan.layers,
            residual_demand_cells: plan.report.residual_demand_cells,
            uncarried_load_mm2: plan.report.uncarried_load_mm2,
        })
    }
}

/// One batched pass over the oriented mesh's own vertices.
fn max_z_mm(mesh: &Mesh) -> Result<f64, String> {
    let mut max_z = f64::NEG_INFINITY;
    for vertex in mesh.verts() {
        max_z = vertex[2].max(max_z);
    }
    if max_z.is_finite() {
        Ok(max_z)
    } else {
        Err("the oriented mesh has no finite Z extent".to_owned())
    }
}
