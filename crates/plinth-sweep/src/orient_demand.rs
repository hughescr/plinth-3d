//! The concrete `plinth_orient::demand::SupportDemandMeasurer`, wired to the
//! real slice sweep (LANE 3, `docs/plans/slice-sweep-core-2026-09-02.md`
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
//! depends on both crates, wires a [`SweepDemand`] into
//! `plinth_orient::search_and_orient`.
//!
//! # What the burden IS, and why it is the arm count
//!
//! The objective's support term asks one question: how much support does
//! this pose cost the person holding the print? Three candidate answers were
//! measured side by side on the discs, at the production pitch
//! (`tests/pose_demand_probe.rs` re-measures all three in-suite so this
//! paragraph cannot go stale):
//!
//! | pose | demand mm² | arms | scar mm² |
//! |---|---|---|---|
//! | Round 50 flat | 1812.41 | 243 | 163.017 |
//! | Round 50 flipped 180° | 1778.12 | 283 | 162.837 |
//! | Round 50 tilted 10° | 21.29 | 495 | 165.080 |
//! | Round 25 flat | 416.85 | 70 | 40.596 |
//! | Round 25 flipped 180° | 417.39 | 88 | 40.518 |
//! | Round 25 tilted 3.6° | 29.84 | 117 | 40.956 |
//!
//! * **`demand_area_mm2` is refuted.** It counts model area with no rooted
//!   carrier AT ITS OWN BIRTH LAYER. A flat down-face is born all at once
//!   with nothing under it, so all of it enters the demand set; a sloped one
//!   is born crescent by crescent, each within one lateral step of the
//!   already-rooted crescent below, so almost none of it does — while the
//!   greedy then places an arm per tiny patch and ends up with TWICE the
//!   arms. It is anti-correlated with the burden across the tilt axis, which
//!   is why substituting it tilted every disc in the acceptance set and laid
//!   the overhang table on its side.
//! * **`total_scar_mm2` cannot rank poses, by the area law itself.**
//!   `SweepPlan::total_scar_mm2`'s own invariant is
//!   `Σ π r² = injected_load / X_ALLOWED` (equal to `peak_layer_area /
//!   X_ALLOWED` only on flat-bottomed models), *independent of how many arms
//!   share the load*: the total tip contact area is set by the load that has
//!   to be carried, not by the pose that carries it. The table shows exactly
//!   that — Round 25's four poses span 40.518 to 41.232 mm², a 1.8 % band,
//!   while their arm counts span 70 to 121, a 73 % one — and inside that
//!   narrow band it ranks the 180° flip BACKWARDS (40.518 flipped against
//!   40.596 flat) on both discs. A quantity the physics holds nearly
//!   constant across poses is not a quantity that can choose one.
//! * **The arm count ranks every measured comparison correctly** at the
//!   production pitch: 70 < 88 < 117 < 121 on Round 25, 243 < 283 < 495 on
//!   Round 50. It is also the burden the owner actually pays: each arm is a
//!   separate contact to clip, a separate scar to sand and a separate chance
//!   to mar the model, and the number of them is the free variable the total
//!   scar area is not.
//!
//! # The unit bridge, and the ruling that is NOT being pre-empted
//!
//! The objective's support weight `ORIENT_WEIGHT_SUPPORT_AREA_CM2` is
//! registered against an AREA in cm², so a raw count cannot be handed to it
//! without silently redefining the weight's unit. The burden is therefore
//! reported as the scar-equivalent area
//!
//! ```text
//! burden_cm2 = arms * pi * TIP_RADIUS_DEFAULT_MM^2 / 100
//! ```
//!
//! — the scar this pose would wear if every one of its arms were the
//! smallest tip the generator can make. Three things about that, stated
//! rather than buried:
//!
//! 1. It is a UNIT CONVERSION, not a weight. The factor is the same positive
//!    constant for every pose of every model, so it cannot change which pose
//!    ranks ahead of which; it can only change the units the count arrives
//!    in. `TIP_RADIUS_DEFAULT_MM` supplies it because it is the registered
//!    generation radius — the per-arm scar the engine starts from — and the
//!    smallest per-arm scar that is physically available, so the term claims
//!    the least authority the count's own ordering can be stated in.
//! 2. The obvious alternative bridge, `arms * mean tip area`, is
//!    ARITHMETICALLY IDENTICAL to `total_scar_mm2` (`Σ π r²` is the count
//!    times the mean of `π r²` by definition), so it would hand back the
//!    quantity the area law holds nearly constant. It is not available as a
//!    bridge; that is a fact about the two quantities, not a preference.
//! 3. **What remains open is the EXCHANGE RATE, and it is not decided here.**
//!    No measurement in this project relates "one support arm to remove" to
//!    "one millimetre of model height" or "one Newton of peel", so the
//!    relative authority of the support term against `w_h` and the peel term
//!    is a ruling nobody has made. The choice above deliberately makes the
//!    support term SMALL rather than picking a factor that would make it
//!    win; where it does decide a pose it decides one the other terms are
//!    indifferent to (a 180° flip moves neither the height nor the peak),
//!    and where it does not, `weight_flips` already publishes the value of
//!    each weight at which the winner flips. No constant was tuned to make
//!    an acceptance check pass.
//!
//! # Two rasters, and why the cheap one may not crown a pose
//!
//! `Prefilter` runs at `working_pitch * DEMAND_PITCH_MULTIPLIER`, `Production`
//! at the registered `SWEEP_WORKING_PITCH_MM` itself. The layer height is
//! never coarsened in either — it sets `kappa * height`, so moving it would
//! distort the reach physics rather than merely make it cheaper — and
//! coarsening the XY pitch cuts `cells_per_layer` roughly with the square of
//! the pitch ratio while leaving the layer count, the true driver of wall
//! clock on a tall pose, alone.
//!
//! The coarse raster is a real measurement and a WRONG ranker: measured at
//! 0.5 mm, Round 25's flat pose takes 102 arms and its 180° flip takes 88 —
//! the reverse of the production-pitch order. That is the whole reason the
//! caller re-measures its finalists here; a prefilter that ranked correctly
//! would have made the second stage unnecessary.
//! [`DEMAND_PITCH_MULTIPLIER`] is registered nowhere: it is a hand-picked
//! cost/fidelity trade on a stage that now only prunes, stated here as a
//! deviation from a literal "run the production sweep on every candidate"
//! reading of the design rather than hidden inside a constant that would
//! misleadingly claim registry provenance for a pure performance knob.
//!
//! # The production pass also prices where the arms LAND, and what they CARRY
//!
//! A production measurement carries two quantities the prefilter does not:
//! the MODEL-LANDING SCAR, `Σ π r_seat² × (fragility + D)` over the arms
//! that seat on the model, measured by [`crate::model_landing`] against the
//! very `OrientFaceTable` the objective was scored with; and the TIP-CONTACT
//! SCAR, the same product over the faces the arms' TIPS carry (excluding one
//! the analytic down-facing fold already counts — [`crate::tip_landing`]'s
//! own doc has the choice and the measurement behind it), measured by
//! [`crate::tip_landing`] against the same table. Both go back through
//! [`MeasuredSupportDemand::landing_scar_raw_mm2`] and
//! [`MeasuredSupportDemand::tip_scar_raw_mm2`] and are folded into the
//! objective's existing scar term at the existing registered weight; this
//! module adds no constant of its own for either.
//!
//! Both are measured at [`DemandFidelity::Production`] and nowhere else, for
//! the same reason the burden may only be crowned there: a coarse raster's
//! arms are not the emitted field's arms, so where they land or what they
//! carry says nothing about where anything will land or carry. A prefilter
//! row therefore carries a stated absence, not a zero.

use std::time::Instant;

use plinth_core::{m0_constants, LayerPlan};
use plinth_mesh::Mesh;
use plinth_orient::demand::{DemandFidelity, MeasuredSupportDemand, SupportDemandMeasurer};
use plinth_orient::{ObjectiveWeights, OrientFaceTable, UpDirection};
use rayon::prelude::*;

use crate::{model_landing, run_sweep, tip_landing, StrutKind, SweepPolicy};

/// See the module doc's "Two rasters" section.
const DEMAND_PITCH_MULTIPLIER: f64 = 10.0;

/// Whether a production measurement prices where its arms LAND on the model.
///
/// **A policy, not a constant.** [`Self::Measured`] is the production
/// behaviour and the [`Default`]; [`Self::Suppressed`] exists so a test can
/// ask the counterfactual question "which pose would this model take if the
/// model-landing scar were not in the objective at all" against the real
/// engine, without a constant to edit or a weight to zero. Suppressing it
/// yields a stated absence (`landing_scar_raw_mm2: None`), which is exactly
/// what a run that never measured one carries, so the suppressed path is a
/// path the search already knows how to take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModelLandingScarPolicy {
    #[default]
    Measured,
    Suppressed,
}

/// Whether a production measurement prices what an arm's TIP carries.
///
/// **A policy, not a constant**, symmetric to [`ModelLandingScarPolicy`] at
/// the other end of the arm. [`Self::Suppressed`] lets a test ask "which pose
/// would this model take if the tip-contact scar were not in the objective at
/// all" against the real engine, and yields a stated absence
/// (`tip_scar_raw_mm2: None`), never a zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TipContactScarPolicy {
    #[default]
    Measured,
    Suppressed,
}

/// The one production [`SupportDemandMeasurer`]. Every number it needs is read
/// fresh from the registry on each call, so two instances can never disagree
/// about a constant; the two things it carries are the policies above.
#[derive(Clone, Copy, Debug, Default)]
pub struct SweepDemand {
    landing_scar: ModelLandingScarPolicy,
    tip_scar: TipContactScarPolicy,
}

impl SweepDemand {
    /// The production measurer: real burden, the model-landing scar and the
    /// tip-contact scar, both measured on the production raster.
    #[must_use]
    pub const fn production() -> Self {
        Self {
            landing_scar: ModelLandingScarPolicy::Measured,
            tip_scar: TipContactScarPolicy::Measured,
        }
    }

    /// The same measurer with the model-landing scar withheld, so a test can
    /// measure what that term is worth by removing it rather than by
    /// re-deriving it. See [`ModelLandingScarPolicy`]. The tip-contact scar
    /// stays measured.
    #[must_use]
    pub const fn without_model_landing_scar() -> Self {
        Self {
            landing_scar: ModelLandingScarPolicy::Suppressed,
            tip_scar: TipContactScarPolicy::Measured,
        }
    }

    /// The same measurer with the tip-contact scar withheld, so a test can
    /// measure what THAT term is worth by removing it rather than by
    /// re-deriving it. See [`TipContactScarPolicy`]. The model-landing scar
    /// stays measured.
    #[must_use]
    pub const fn without_tip_contact_scar() -> Self {
        Self {
            landing_scar: ModelLandingScarPolicy::Measured,
            tip_scar: TipContactScarPolicy::Suppressed,
        }
    }
}

/// Every registry number one batch of measurements needs, read ONCE per
/// batch. Reading the registry per candidate would be a scalar call in the
/// hot path of a stage that runs up to sixteen sweeps.
struct DemandSettings {
    lift_mm: f64,
    layer_height_mm: f64,
    tip_radius_mm: f64,
    /// The same `-(OVERHANG_ANGLE_DEG.to_radians().sin())` the objective's
    /// down-facing fold evaluates (`plinth_orient::score::ObjectiveWeights::
    /// down_limit`), read once per batch so the tip-contact scar's
    /// "already counted" test agrees with the analytic fold's own registered
    /// number rather than a second read of it.
    down_limit: f64,
    policy: SweepPolicy,
}

impl DemandSettings {
    fn read(fidelity: DemandFidelity) -> Result<Self, String> {
        let constants = m0_constants().map_err(|error| error.to_string())?;
        let policy = SweepPolicy::from_registry().map_err(|error| error.to_string())?;
        let weights = ObjectiveWeights::registered().map_err(|error| error.to_string())?;
        let policy = match fidelity {
            DemandFidelity::Prefilter => {
                let coarse_pitch = policy.working_pitch_mm * DEMAND_PITCH_MULTIPLIER;
                policy.with_working_pitch_mm(coarse_pitch)
            }
            // The registered working pitch, untouched: this is the same
            // raster `sweep_and_emit` plans the emitted field on.
            DemandFidelity::Production => policy,
        };
        Ok(Self {
            lift_mm: constants.placeholders().lift_mm().diagnostic().get(),
            layer_height_mm: *constants.evidence().layer_height_default_mm().value(),
            tip_radius_mm: constants
                .placeholders()
                .tip_radius_default_mm()
                .diagnostic()
                .get(),
            down_limit: weights.down_limit(),
            policy,
        })
    }
}

impl SweepDemand {
    /// One candidate, one sweep. The batch entry point owns the loop so a
    /// single failing pose cannot end the batch.
    fn measure_one(
        mesh: &Mesh,
        table: &OrientFaceTable,
        candidate: UpDirection,
        fidelity: DemandFidelity,
        settings: &DemandSettings,
        landing_scar: ModelLandingScarPolicy,
        tip_scar: TipContactScarPolicy,
    ) -> Result<MeasuredSupportDemand, String> {
        // Oriented exactly as the eventual winner would be
        // (`plinth_orient::oriented_recentered_mesh` is the same
        // `transform_for`/`transform_mesh` pair `search_and_orient` applies
        // to the committed winner), so a candidate measured well here is
        // measured in the frame it would actually be built in.
        let transformed = plinth_orient::oriented_recentered_mesh(mesh, candidate)
            .map_err(|error| error.to_string())?;

        let layer_height_mm = settings.layer_height_mm;

        // `oriented_recentered_mesh` recenters translation so the
        // transformed mesh's own minimum Z is exactly 0
        // (`plinth-orient`'s `transform_mesh`: `translation = [.., -min[2]]`),
        // so the plate sits at `-lift_mm` for every candidate without
        // re-deriving its own minimum here.
        let plate_z = -settings.lift_mm;
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

        let plan =
            run_sweep(&transformed, &layers, &settings.policy).map_err(|error| error.to_string())?;

        // The model-landing scar, on the PRODUCTION raster only. Two reasons
        // it is not measured at `Prefilter`: the coarse raster's arms are not
        // the emitted field's arms, so where they land is not where anything
        // will land; and the fidelity precedence in `plinth_orient::search`
        // means a prefilter row may only prune, never crown, so a scar it
        // measured could not be allowed to decide anyway.
        //
        // The fields come from the table the objective was SCORED against,
        // never re-evaluated here, and the face order of `transformed` is the
        // face order of `mesh` (orienting rotates vertices and never reorders
        // triangles), which is the table's own order.
        let landing = match (fidelity, landing_scar) {
            (DemandFidelity::Production, ModelLandingScarPolicy::Measured) => {
                model_landing::model_landing_scar(
                    &transformed,
                    &plan,
                    table.delicacy(),
                    table.display(),
                )
            }
            _ => None,
        };
        // The tip-contact scar, under the identical fidelity precedence and
        // for the identical reason: a coarse raster's tips are not the
        // emitted field's tips.
        let tip = match (fidelity, tip_scar) {
            (DemandFidelity::Production, TipContactScarPolicy::Measured) => {
                tip_landing::tip_contact_scar(
                    &transformed,
                    &plan,
                    table.delicacy(),
                    table.display(),
                    settings.down_limit,
                )
            }
            _ => None,
        };
        // THE BURDEN IS WHAT THE OWNER PAYS IN CLIPS, so it counts only the
        // members that seat into the model and carry peel load. A brace or a
        // mast (the slenderness lane, 2026-09-05/06) touches no model face
        // and leaves no scar; counting either here would over-state the
        // burden by exactly the stiffening the lane added to make the arms
        // themselves legal (see the module doc's "What the burden IS" table
        // and `tests/pose_demand_probe.rs`).
        let arm_count = plan
            .struts
            .iter()
            .filter(|strut| strut.kind == StrutKind::Arm)
            .count();
        let brace_count = plan
            .struts
            .iter()
            .filter(|strut| strut.kind == StrutKind::Brace)
            .count();
        let mast_count = plan
            .struts
            .iter()
            .filter(|strut| strut.kind == StrutKind::Mast)
            .count();
        Ok(MeasuredSupportDemand {
            support_area_cm2: burden_cm2(arm_count, settings.tip_radius_mm),
            struts: arm_count,
            braces: brace_count,
            masts: mast_count,
            total_scar_mm2: plan.report.total_scar_mm2,
            achieved_pitch_mm: plan.report.achieved_pitch_mm,
            layers: plan.layers,
            residual_demand_cells: plan.report.residual_demand_cells,
            uncarried_load_mm2: plan.report.uncarried_load_mm2,
            model_landings: plan.report.landings.model,
            landing_scar_raw_mm2: landing.as_ref().map(|scar| scar.raw_mm2),
            tip_scar_raw_mm2: tip.as_ref().map(|scar| scar.raw_mm2),
            fidelity,
        })
    }
}

/// The unit bridge from an arm count to the cm² the registered support weight
/// is denominated in. See the module doc: one fixed positive factor for every
/// pose, so it converts and never ranks.
#[must_use]
pub fn burden_cm2(struts: usize, tip_radius_mm: f64) -> f64 {
    struts as f64 * core::f64::consts::PI * tip_radius_mm * tip_radius_mm / 100.0
}

impl SupportDemandMeasurer for SweepDemand {
    fn measure_support_demand(
        &self,
        mesh: &Mesh,
        table: &OrientFaceTable,
        candidates: &[UpDirection],
        fidelity: DemandFidelity,
    ) -> Vec<Result<MeasuredSupportDemand, String>> {
        let started = Instant::now();
        let settings = match DemandSettings::read(fidelity) {
            Ok(settings) => settings,
            // Emit and warn: one named failure per candidate, so the caller
            // keeps its proxy for all of them rather than the search refusing.
            Err(error) => return candidates.iter().map(|_| Err(error.clone())).collect(),
        };
        // The registry was already read ONCE above into `settings`; every
        // candidate in the batch shares that one read rather than a per-pose
        // registry call. Each pose's own sweep (`measure_one`) allocates its
        // own scratch and touches no state shared with any other pose, so the
        // batch is embarrassingly parallel: `into_par_iter` is index-parallel
        // and `collect` on a slice-backed rayon iterator preserves the
        // original candidate order, so the result vector lines up with
        // `candidates` exactly as the serial `.map` did (proved bit-identical
        // to the serial path by
        // `plinth-sweep/tests/orient_demand_parallel.rs`).
        let measured: Vec<Result<MeasuredSupportDemand, String>> = candidates
            .par_iter()
            .map(|candidate| {
                Self::measure_one(
                    mesh,
                    table,
                    *candidate,
                    fidelity,
                    &settings,
                    self.landing_scar,
                    self.tip_scar,
                )
            })
            .collect();
        // Disclosure, not a record: the cost of this stage is printed where a
        // reader of the run log will see it, and is deliberately kept OUT of
        // the sealed `DenseSweepRecord`, whose digest must not depend on a
        // clock.
        let poses: Vec<String> = candidates
            .iter()
            .zip(&measured)
            .map(|(candidate, result)| {
                let up = candidate.get();
                let (arms, stiffening, burden, landing, tip) = match result {
                    Ok(one) => (
                        one.struts.to_string(),
                        // Published beside the arm count so the pose's
                        // stiffening cost is visible without being allowed to
                        // decide the pose (neither is folded into `burden`).
                        format!("{} braces {} masts", one.braces, one.masts),
                        format!("{:.5}", one.support_area_cm2),
                        // A stated absence prints as one: "not measured" and
                        // "measured, nothing landed" must never look alike in
                        // the log a reader diagnoses a pose from.
                        one.landing_scar_raw_mm2.map_or_else(
                            || format!("{} model landings, scar unmeasured", one.model_landings),
                            |scar| {
                                format!(
                                    "{} model landings, landing scar {scar:.5} mm2",
                                    one.model_landings
                                )
                            },
                        ),
                        one.tip_scar_raw_mm2.map_or_else(
                            || "tip scar unmeasured".to_owned(),
                            |scar| format!("tip scar {scar:.5} mm2"),
                        ),
                    ),
                    Err(_) => (
                        "err".to_owned(),
                        "err".to_owned(),
                        "err".to_owned(),
                        "err".to_owned(),
                        "err".to_owned(),
                    ),
                };
                format!(
                    "({:+.4},{:+.4},{:+.4}) arms {arms} ({stiffening}) burden {burden} cm2, \
                     {landing}, {tip}",
                    up[0], up[1], up[2]
                )
            })
            .collect();
        eprintln!(
            "[orient-demand] {} sweep of {} pose(s) in {:.2} s: {}",
            fidelity.label(),
            candidates.len(),
            started.elapsed().as_secs_f64(),
            poses.join("; "),
        );
        measured
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
