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
//!
//! **The quick pass also skips bracing** (Craig ruling 2026-10-04). A prefilter
//! run places no `|X|` cross, thickening, mast or model tie
//! ([`SweepPolicy::without_slenderness_ties`]): it only ranks poses by the arms
//! they cost, its raster is too coarse to crown one, and the crosses' frame
//! solves were nearly all of its time since round 13. A prefilter row therefore
//! reports 0 braces and 0 masts, and its arm count holds no model tie. The
//! production measurement of the finalists runs every rung, as the emitted
//! field does. The slenderness merges (which move arms and so change the count)
//! still run in both.
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
//!
//! # A finalist that cannot win is stopped (perf lane B1, 2026-10-14)
//!
//! The finalists' production sweeps run in parallel and the batch used to
//! return when the slowest finished, even when the slowest was already
//! certain to lose. [`SupportDemandMeasurer::measure_production_race`] runs the
//! same sweeps against a [`ProductionRace`]: the moment a pose finishes it
//! tells the race, and the race sets the cancel token of every pose whose
//! lower bound (`plinth_orient::search::TermsRecord::production_lower_bound`,
//! proved there and in `plinth_orient::demand`) is STRICTLY above the finished
//! score. A set token ends that pose's sweep at its next safe point
//! ([`crate::cancel`]: between layers of the upward pass and of every
//! down-sweep, between the rounds of the long stages) and the pose is reported [`RaceOutcome::Stopped`] — with no score and
//! no retained plan. The pose that decides the batch is never stopped, and a
//! pose that is not stopped measures exactly what it measures without a race.
//! This module decides nothing about WHICH pose may stop; it carries the
//! token.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use geom_kernel::vec3;
use plinth_core::{m0_constants, LayerPlan};
use plinth_mesh::Mesh;
use plinth_orient::demand::{
    DemandFidelity, MeasuredSupportDemand, ProductionRace, RaceOutcome, SupportDemandMeasurer,
};
use plinth_orient::{ObjectiveWeights, OrientFaceTable, UpDirection};
use rayon::prelude::*;

use crate::{
    complete_for_emission, model_landing, perf, run_sweep_in_token_scope,
    run_sweep_with_holes_seeded_in_token_scope, tip_landing, DrainSpec, EditSeed, StrutKind,
    SweepPlan, SweepPolicy,
};

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
    /// `Some` when this batch keeps its plans for emission
    /// ([`RetainingSweepDemand`], production fidelity only): the policy those
    /// plans are swept under, [`production_measurement_policy`] of the
    /// registered one. `None` everywhere else.
    retained_policy: Option<SweepPolicy>,
    /// HOLLOW FIRST: the hollow request a retained production pose is drilled
    /// under ([`RetainingSweepDemand::hollow_first`]). `None` everywhere else.
    shells: Option<plinth_hollow::HollowRequest>,
}

impl DemandSettings {
    fn read(fidelity: DemandFidelity, retain: bool) -> Result<Self, String> {
        let constants = m0_constants().map_err(|error| error.to_string())?;
        let policy = SweepPolicy::from_registry().map_err(|error| error.to_string())?;
        let weights = ObjectiveWeights::registered().map_err(|error| error.to_string())?;
        // The registered policy as the emitted field is swept under, before any
        // demand-only switch below: the retained plans' policy is built from it.
        let retained_policy = (retain && matches!(fidelity, DemandFidelity::Production))
            .then(|| production_measurement_policy(policy));
        let policy = match fidelity {
            DemandFidelity::Prefilter => {
                let coarse_pitch = policy.working_pitch_mm * DEMAND_PITCH_MULTIPLIER;
                policy.with_working_pitch_mm(coarse_pitch)
            }
            // The registered working pitch, untouched: this is the same
            // raster `sweep_and_emit` plans the emitted field on.
            DemandFidelity::Production => policy,
        };
        // A demand run reads no count of the brace frame's refusals (it reads
        // the arms, the scar and the landings), so it does not classify a
        // pair no band set can be planned on: the same ladder decisions, the
        // same field (`plinth-sweep` `tests/it/frame_ledger.rs`), classifying
        // only the pairs a decision can read (`lanes/band-speed.md`).
        let policy = policy.without_frame_ledger();
        // THE QUICK PASS SKIPS BRACING (Craig ruling 2026-10-04). The prefilter
        // only ranks hundreds of poses to choose the few the production
        // measurement re-measures, and its raster is too coarse to crown one
        // anyway (module doc, "Two rasters"). Since round 13 the crosses'
        // frame solves were nearly all of its time (the toed slab's process
        // test: 765 s against 38 s at `a73e40a`), and the production stage
        // below still runs every rung exactly as the emitted field does, so
        // the pose that is built is judged with bracing in. Without the ties
        // a prefilter row reports 0 braces and 0 masts, and its arm count has
        // no model tie in it.
        // NOR DOES IT READ THE DRAWN SOLID OF A DESCENT
        // (fix/drawn-solid-clearance, 2026-10-04): the coarse raster's own
        // resolution term, half a cell's diagonal, is 0.35 mm at 0.5 mm pitch,
        // larger than the 0.3 mm final gap the test reads, so it would find
        // walls that are not there (the tall stilt plate's tilted coarse sweep
        // went from 349 arms to 302), and the prefilter ranks poses by the
        // arms they cost, which this rule must not change. The production
        // measurement of the finalists, at the registered pitch, reads it.
        let policy = match fidelity {
            DemandFidelity::Prefilter => policy
                .without_slenderness_ties()
                .without_drawn_solid_descent()
                .without_clearance_reroute()
                // NOR DOES IT AVERAGE THE SEAT NORMAL OVER A FOOTPRINT (Craig
                // ruling 2026-10-05, `footprint_normal`): it ranks poses by the
                // arms they cost and reads no seat normal; the finalists'
                // production measurement, below, runs the pass.
                .without_footprint_normal()
                // NOR DOES IT BUILD ARTICULATED HEADS (2026-10-06): a head
                // changes the post's start and the seat's cell, never the
                // demand the prefilter counts.
                .without_heads(),
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
            retained_policy,
            shells: None,
        })
    }
}

/// The policy a RETAINED production measurement is swept under: the emitted
/// policy with the shake check deferred ([`SweepPolicy::stage_shake_capture`]),
/// and nothing else changed. So the frame ledger and the rigid-share diagnostic
/// run as they do on the emitted field, and the retained plan is the plan the
/// seal would compute. None of the three moves a strut (`tests/it/frame_ledger.rs`;
/// the rigid share and the shake only read the field), which is why the
/// measured scalars and the ranking do not change.
#[must_use]
pub const fn production_measurement_policy(emitted: SweepPolicy) -> SweepPolicy {
    SweepPolicy {
        stage_shake_capture: true,
        rigid_share_capture: true,
        ..emitted
    }
}

/// A production measurement of one finalist pose, kept whole.
#[derive(Clone, Debug)]
pub struct RetainedPlan {
    /// The candidate direction this plan was measured for.
    pub up: UpDirection,
    /// The oriented mesh the sweep ran on. The reuse key compares its bits.
    pub mesh: Mesh,
    /// The layer plan the sweep ran under.
    pub layers: LayerPlan,
    /// The policy the sweep ran under ([`production_measurement_policy`]).
    pub policy: SweepPolicy,
    /// The plan, its shake check still deferred.
    pub plan: SweepPlan,
    /// The sweep's state at checkpoint C, kept so the finished plan can be
    /// edited, or re-planned with contact caps from its cap-independent prefix
    /// (SC-1, PX), without planning it again. Its
    /// identity key is normalized with [`production_measurement_policy`] on both
    /// sides, so a seed made here is valid for the emission policy the reuse
    /// key accepts. A pose that loses the race drops its seed with its plan.
    pub seed: Option<Arc<EditSeed>>,
    /// Wall seconds of the sweep. Evidence only; it is in no digest.
    pub sweep_seconds: f64,
    /// The drains the sweep ran with (hollow first only; else empty). The
    /// reuse key compares them bit for bit.
    pub drains: Vec<DrainSpec>,
    /// The vents the sweep ran with (hollow first only; else empty). The reuse
    /// key compares them bit for bit.
    pub vents: Vec<DrainSpec>,
    /// HOLLOW FIRST: the oriented SOLID this pose's drilled shell was made from,
    /// so [`RetainingSweepDemand::take_winner`] can match the pose the search
    /// committed. `None` when [`Self::mesh`] is itself that solid.
    pub pose_solid: Option<Mesh>,
}

impl RetainedPlan {
    /// The oriented solid of this pose: what the search's committed mesh is
    /// compared with.
    #[must_use]
    pub fn oriented_solid(&self) -> &Mesh {
        self.pose_solid.as_ref().unwrap_or(&self.mesh)
    }
}

/// Why a retained plan was not reused. The first failing check in key order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReuseMiss {
    /// Reuse is switched off for this run (`--no-cache`).
    Disabled,
    /// The orientation search kept no plan for the pose that won: the winner
    /// is not a production finalist, or the run did not search.
    NotRetained,
    /// The emitted mesh is not bit-equal to the measured one (a drilled shell,
    /// or a legacy pose whose committed rotation differs by ulps).
    MeshDiffers,
    /// The emitted layer plan is not bit-equal to the measured one.
    LayersDiffer,
    /// The emitted sweep policy differs from the measured one.
    PolicyDiffers,
    /// The emitted sweep has drains or vents; the measurement had none.
    HolesPresent,
    /// The measurement had drains or vents (hollow first), and the emitted
    /// sweep's are not bit-equal to them.
    HolesDiffer,
}

/// The reuse key: `None` when the retained plan is exactly what a fresh
/// `run_sweep_with_holes(mesh, layers, emitted, drains, vents)` would compute
/// (up to the deferred shake check), else the first thing that differs.
///
/// Key order: the mesh bit for bit; the layer plan bit for bit; the whole
/// policy; the drains and vents bit for bit against the ones the measurement
/// ran with (none, unless the search measured a drilled shell).
#[must_use]
pub fn reuse_miss(
    retained: &RetainedPlan,
    mesh: &Mesh,
    layers: &LayerPlan,
    emitted: &SweepPolicy,
    drains: &[DrainSpec],
    vents: &[DrainSpec],
) -> Option<ReuseMiss> {
    if !meshes_bit_equal(&retained.mesh, mesh) {
        return Some(ReuseMiss::MeshDiffers);
    }
    if !layers_bit_equal(&retained.layers, layers) {
        return Some(ReuseMiss::LayersDiffer);
    }
    if retained.policy != production_measurement_policy(*emitted) {
        return Some(ReuseMiss::PolicyDiffers);
    }
    if !holes_bit_equal(&retained.drains, drains) || !holes_bit_equal(&retained.vents, vents) {
        return Some(if retained.drains.is_empty() && retained.vents.is_empty() {
            ReuseMiss::HolesPresent
        } else {
            ReuseMiss::HolesDiffer
        });
    }
    None
}

fn holes_bit_equal(left: &[DrainSpec], right: &[DrainSpec]) -> bool {
    let bits = |hole: &DrainSpec| {
        [
            hole.outlet_mm[0].to_bits(),
            hole.outlet_mm[1].to_bits(),
            hole.outlet_mm[2].to_bits(),
            hole.outward_axis[0].to_bits(),
            hole.outward_axis[1].to_bits(),
            hole.outward_axis[2].to_bits(),
            hole.radius_mm.to_bits(),
        ]
    };
    left.len() == right.len() && left.iter().zip(right).all(|(a, b)| bits(a) == bits(b))
}

/// Reuse a retained plan for emission: on a hit, a copy of the plan with its
/// shake check run, else the reason it missed. Counts `cache.winner_plan.hit`
/// or `.miss`.
///
/// # Errors
///
/// A [`ReuseMiss`] when the key does not match; the caller then plans the field
/// as it always did.
pub fn reuse_for(
    retained: &RetainedPlan,
    mesh: &Mesh,
    layers: &LayerPlan,
    emitted: &SweepPolicy,
    drains: &[DrainSpec],
    vents: &[DrainSpec],
) -> Result<SweepPlan, ReuseMiss> {
    if let Some(miss) = reuse_miss(retained, mesh, layers, emitted, drains, vents) {
        perf::add(perf::Work::CacheWinnerPlanMiss, 1);
        return Err(miss);
    }
    perf::add(perf::Work::CacheWinnerPlanHit, 1);
    let mut plan = retained.plan.clone();
    complete_for_emission(&mut plan);
    Ok(plan)
}

/// [`reuse_for`], except that the rigid-body share reading stays DEFERRED on
/// the returned plan (`plan.deferred_rigid_share`): the caller runs it beside
/// other work (`deferred_rigid`) and stores the result. The shake check is
/// completed as in [`reuse_for`]. On a plan with no deferred reading this is
/// [`reuse_for`] exactly.
///
/// # Errors
///
/// A [`ReuseMiss`], as [`reuse_for`].
pub fn reuse_for_deferring_rigid_share(
    retained: &RetainedPlan,
    mesh: &Mesh,
    layers: &LayerPlan,
    emitted: &SweepPolicy,
    drains: &[DrainSpec],
    vents: &[DrainSpec],
) -> Result<SweepPlan, ReuseMiss> {
    if let Some(miss) = reuse_miss(retained, mesh, layers, emitted, drains, vents) {
        perf::add(perf::Work::CacheWinnerPlanMiss, 1);
        return Err(miss);
    }
    perf::add(perf::Work::CacheWinnerPlanHit, 1);
    let mut plan = retained.plan.clone();
    let rigid = plan.deferred_rigid_share.take();
    complete_for_emission(&mut plan);
    plan.deferred_rigid_share = rigid;
    Ok(plan)
}

fn meshes_bit_equal(left: &Mesh, right: &Mesh) -> bool {
    left.verts().len() == right.verts().len()
        && left.verts().iter().zip(right.verts()).all(|(a, b)| {
            a[0].to_bits() == b[0].to_bits()
                && a[1].to_bits() == b[1].to_bits()
                && a[2].to_bits() == b[2].to_bits()
        })
        && left.tris() == right.tris()
        && left.weld_policy() == right.weld_policy()
}

fn layers_bit_equal(left: &LayerPlan, right: &LayerPlan) -> bool {
    left.z0_mm().to_bits() == right.z0_mm().to_bits()
        && left.heights_mm().len() == right.heights_mm().len()
        && left
            .heights_mm()
            .iter()
            .zip(right.heights_mm())
            .all(|(a, b)| a.to_bits() == b.to_bits())
}

/// [`SweepDemand`] that also KEEPS the production plan of every finalist, so
/// the pipeline can emit the winner's plan instead of sweeping it again.
///
/// It measures exactly as [`SweepDemand::production`] does (same pose, same
/// layers, same registry read, same scars) and returns the same numbers; the
/// only difference is that the production sweep runs under
/// [`production_measurement_policy`] and the plan is stored. The prefilter
/// never retains.
pub struct RetainingSweepDemand {
    inner: SweepDemand,
    retained: Mutex<Vec<RetainedPlan>>,
    /// **Sabotage tests only.** When set, the production batch ignores the
    /// search's race, so no finalist is withdrawn and every finalist keeps its
    /// plan. Never set on a real run.
    keep_every_finalist: bool,
    /// HOLLOW FIRST (lane hf, 2026-10-16; `--hollow-first`): `Some` makes every
    /// PRODUCTION pose run the hollow stage on its oriented solid and measure
    /// the drilled shell, with its drains and vents, instead of the solid. A
    /// pose the hollow stage does not hollow is measured on the solid as
    /// before. The prefilter never hollows (`docs/plans/hollow-first-2026-10-16.md` §5).
    shells: Option<plinth_hollow::HollowRequest>,
}

impl RetainingSweepDemand {
    /// The production measurer, retaining.
    #[must_use]
    pub const fn production() -> Self {
        Self {
            inner: SweepDemand::production(),
            retained: Mutex::new(Vec::new()),
            keep_every_finalist: false,
            shells: None,
        }
    }

    /// This measurer, with every production pose measured on the drilled shell
    /// the hollow stage makes for it under `request` (hollow first).
    #[must_use]
    pub fn hollow_first(mut self, request: plinth_hollow::HollowRequest) -> Self {
        self.shells = Some(request);
        self
    }

    /// **Sabotage tests only.** [`Self::production`], except that the
    /// certified early stop is off: every finalist is measured to the end and
    /// keeps its plan, whatever order they finish in. Which certified losers
    /// the race has withdrawn by the time the batch returns depends on thread
    /// timing; the sabotage needs a loser's plan to exist, so it must not
    /// depend on that. The numbers measured and the winner are unchanged (the
    /// search recomputes its certified set from the winner's score alone), so
    /// the emitted file is the same as with the race on.
    #[doc(hidden)]
    #[must_use]
    pub const fn production_keeping_every_finalist() -> Self {
        Self {
            inner: SweepDemand::production(),
            retained: Mutex::new(Vec::new()),
            keep_every_finalist: true,
            shells: None,
        }
    }

    /// Drain the store: every kept plan, in candidate order.
    pub fn drain(&self) -> Vec<RetainedPlan> {
        std::mem::take(&mut *self.retained.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Drain the store and return the first plan whose oriented solid is
    /// bit-equal to `oriented` (the mesh the search committed). Every other
    /// plan is dropped. At most one finalist can match: the grid de-duplicates
    /// directions.
    pub fn take_winner(&self, oriented: &Mesh) -> Option<RetainedPlan> {
        self.drain()
            .into_iter()
            .find(|retained| meshes_bit_equal(retained.oriented_solid(), oriented))
    }

    /// **Sabotage only** (`WinnerPlanReuse::SabotageOtherPose`): drain the
    /// store and return the first plan that is NOT the winner's, relabelled
    /// with `oriented` as its mesh so the reuse key passes. The emitted field
    /// is then another pose's field, and the byte-identity checks must go red.
    /// `None` when the search kept no other pose.
    #[doc(hidden)]
    pub fn take_other_pose_relabelled(&self, oriented: &Mesh) -> Option<RetainedPlan> {
        self.drain()
            .into_iter()
            .find(|retained| !meshes_bit_equal(retained.oriented_solid(), oriented))
            .map(|mut retained| {
                retained.mesh = oriented.clone();
                retained.pose_solid = None;
                retained
            })
    }
}

/// Why one pose has no measurement: it failed, or the race withdrew it.
enum PoseError {
    Failed(String),
    /// The pose's cancel token was set ([`crate::cancel`]) and its sweep
    /// returned at a safe point.
    Stopped,
}

impl From<String> for PoseError {
    fn from(error: String) -> Self {
        Self::Failed(error)
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
    ) -> Result<(MeasuredSupportDemand, Option<RetainedPlan>), PoseError> {
        // Oriented exactly as the eventual winner would be
        // (`plinth_orient::oriented_recentered_mesh` is the same
        // `transform_for`/`transform_mesh` pair `search_and_orient` applies
        // to the committed winner), so a candidate measured well here is
        // measured in the frame it would actually be built in.
        let transformed = plinth_orient::oriented_recentered_mesh(mesh, candidate)
            .map_err(|error| error.to_string())?;

        // HOLLOW FIRST: a retained production pose is measured on the shell the
        // hollow stage drills for it, when it hollows (`pose_shell`).
        let shell = match (fidelity, &settings.shells, &settings.retained_policy) {
            (DemandFidelity::Production, Some(request), Some(_)) => {
                pose_shell(&transformed, request, candidate)
            }
            _ => None,
        };

        let layer_height_mm = settings.layer_height_mm;

        // `oriented_recentered_mesh` recenters translation so the
        // transformed mesh's own minimum Z is exactly 0
        // (`plinth-orient`'s `transform_mesh`: `translation = [.., -min[2]]`),
        // so the plate sits at `-lift_mm` for every candidate without
        // re-deriving its own minimum here. A drilled shell can lose the
        // solid's lowest or highest point to a bore, so its plate is placed
        // from its own extent, exactly as `sweep_and_emit` places it.
        let (plate_z, max_z) = match &shell {
            None => (-settings.lift_mm, max_z_mm(&transformed)?),
            Some(shell) => {
                let (min_z, max_z) = z_extent_mm(&shell.mesh)?;
                (min_z - settings.lift_mm, max_z)
            }
        };
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

        // A RETAINED measurement sweeps under the emitted policy (shake check
        // deferred) so the plan can be emitted as it is; every other
        // measurement is the pricing sweep, which skips the rigid-share
        // diagnostic and the frame ledger.
        // Names this pose's perf frame (write-only; the closure runs only
        // with `--perf`), so the winner's own spans can be told from the other
        // finalists' in the sidecar's `poses`.
        perf::pose_label(|| {
            let up = candidate.get();
            format!("{} ({:+.4},{:+.4},{:+.4})", fidelity.label(), up[0], up[1], up[2])
        });
        let started = Instant::now();
        let (swept, drains, vents) = match &shell {
            Some(shell) => (&shell.mesh, &shell.drains[..], &shell.vents[..]),
            None => (&transformed, &[][..], &[][..]),
        };
        // A RETAINED sweep keeps its state at checkpoint C beside the plan.
        let (plan, seed) = match &settings.retained_policy {
            Some(policy) => {
                run_sweep_with_holes_seeded_in_token_scope(swept, &layers, policy, drains, vents)
                    .map(|(plan, seed)| (plan, Some(seed)))
            }
            None => run_sweep_in_token_scope(&transformed, &layers, &settings.policy)
                .map(|plan| (plan, None)),
        }
        .map_err(|error| match error {
            crate::SweepError::Cancelled => PoseError::Stopped,
            other => PoseError::Failed(other.to_string()),
        })?;
        let sweep_seconds = started.elapsed().as_secs_f64();

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
        // triangles), which is the table's own order. A drilled shell has other
        // faces: its fields are the table's, carried face by face onto the
        // shell (`ShellFields`).
        let shell_fields = shell.as_ref().map(|shell| {
            ShellFields::carry(&transformed, &shell.mesh, table.delicacy(), table.display())
        });
        let (fragility, display) = shell_fields.as_ref().map_or_else(
            || (table.delicacy(), table.display()),
            |fields| (&fields.fragility[..], &fields.display[..]),
        );
        let landing = match (fidelity, landing_scar) {
            (DemandFidelity::Production, ModelLandingScarPolicy::Measured) => {
                model_landing::model_landing_scar(swept, &plan, fragility, display)
            }
            _ => None,
        };
        // The tip-contact scar, under the identical fidelity precedence and
        // for the identical reason: a coarse raster's tips are not the
        // emitted field's tips.
        let tip = match (fidelity, tip_scar) {
            (DemandFidelity::Production, TipContactScarPolicy::Measured) => {
                tip_landing::tip_contact_scar(
                    swept,
                    &plan,
                    fragility,
                    display,
                    settings.down_limit,
                )
            }
            _ => None,
        };
        if let (Some(shell), Some(fields)) = (&shell, &shell_fields) {
            let up = candidate.get();
            let scar = |raw: Option<f64>| {
                raw.map_or_else(|| "unmeasured".to_owned(), |value| format!("{value:.5}"))
            };
            eprintln!(
                "[orient-demand] hollow-first pose ({:+.4},{:+.4},{:+.4}): drilled shell {} faces \
                 (kept {} / {:.2} mm2, cut at a mouth {} / {:.2} mm2, cavity and bores priced 0 \
                 {} / {:.2} mm2), {} drain(s), {} vent(s); arms {}, model landings {} ({} \
                 interior), landing scar {}, tip scar {}",
                up[0],
                up[1],
                up[2],
                shell.mesh.tris().len(),
                fields.kept_faces,
                fields.kept_mm2,
                fields.touching_faces,
                fields.touching_mm2,
                fields.zero_faces,
                fields.zero_mm2,
                shell.drains.len(),
                shell.vents.len(),
                plan.struts.iter().filter(|strut| strut.kind.seats_on_model()).count(),
                plan.report.landings.model,
                landing.as_ref().map_or(0, |landing| landing.interior),
                scar(landing.as_ref().map(|landing| landing.raw_mm2)),
                scar(tip.as_ref().map(|tip| tip.raw_mm2)),
            );
        }
        drop(shell_fields);
        // THE BURDEN IS WHAT THE OWNER PAYS IN CLIPS, so it counts only the
        // members that seat into the model and carry peel load. A brace or a
        // mast (the slenderness lane, 2026-09-05/06) touches no model face
        // and leaves no scar; counting either here would over-state the
        // burden by exactly the stiffening the lane added to make the arms
        // themselves legal (see the module doc's "What the burden IS" table
        // and `tests/pose_demand_probe.rs`). A model tie (2026-10-03) DOES
        // seat a tip into the model, so it is a clip like an arm.
        let arm_count = plan
            .struts
            .iter()
            .filter(|strut| strut.kind.seats_on_model())
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
        let measured = MeasuredSupportDemand {
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
        };
        let (swept, pose_solid, drains, vents) = match shell {
            Some(shell) => (shell.mesh, Some(transformed), shell.drains, shell.vents),
            None => (transformed, None, Vec::new(), Vec::new()),
        };
        let retained = settings.retained_policy.map(|policy| RetainedPlan {
            up: candidate,
            mesh: swept,
            layers,
            policy,
            plan,
            seed,
            sweep_seconds,
            drains,
            vents,
            pose_solid,
        });
        Ok((measured, retained))
    }

    /// One batch of measurements. `retain` keeps the production plans (see
    /// [`RetainingSweepDemand`]); the numbers returned are the same either way.
    /// The plans come back in candidate order, one per measured pose.
    #[expect(
        clippy::too_many_arguments,
        reason = "the batch's own inputs plus the three per-measurer switches (policies, race, \
                  hollow request), each read once here; a bundling struct would only rename them"
    )]
    fn measure_batch(
        mesh: &Mesh,
        table: &OrientFaceTable,
        candidates: &[UpDirection],
        fidelity: DemandFidelity,
        retain: bool,
        policies: Self,
        race: Option<&ProductionRace>,
        shells: Option<&plinth_hollow::HollowRequest>,
    ) -> (Vec<RaceOutcome>, Vec<RetainedPlan>) {
        let (landing_scar, tip_scar) = (policies.landing_scar, policies.tip_scar);
        let started = Instant::now();
        let settings = match DemandSettings::read(fidelity, retain) {
            Ok(settings) => DemandSettings {
                shells: shells.cloned(),
                ..settings
            },
            // Emit and warn: one named failure per candidate, so the caller
            // keeps its proxy for all of them rather than the search refusing.
            Err(error) => {
                return (
                    candidates
                        .iter()
                        .map(|_| RaceOutcome::Failed(error.clone()))
                        .collect(),
                    Vec::new(),
                )
            }
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
        //
        // `with_max_len(1)`: one pose per job, so the 16 prefilter poses
        // balance by stealing on any width instead of in fixed pairs. It
        // changes only how the range is split; the indexed collect keeps
        // candidate order.
        //
        // THE RACE (perf lane B1). With a `race`, a pose whose token is set
        // is not started, or stops at its sweep's next safe point
        // ([`crate::cancel`]); a pose that finishes tells the race, which may
        // set other poses' tokens. Each pose's token is installed for that
        // pose's own `measure_one` only, and nothing else about a pose's
        // sweep changes, so a pose that is not stopped measures exactly what
        // it measures without a race.
        let raw: Vec<Result<(MeasuredSupportDemand, Option<RetainedPlan>), PoseError>> = candidates
            .par_iter()
            .enumerate()
            .with_max_len(1)
            .map(|(index, candidate)| {
                let measure = || {
                    Self::measure_one(
                        mesh,
                        table,
                        *candidate,
                        fidelity,
                        &settings,
                        landing_scar,
                        tip_scar,
                    )
                };
                let Some((race, token)) = race.and_then(|race| race.token(index).map(|t| (race, t)))
                else {
                    // Not a race pose, so it runs with NO token. The token is a
                    // thread-local, and this job may have been stolen by a
                    // worker that is waiting inside a raced pose of another
                    // search; it must not run under that pose's token.
                    return crate::cancel::without_token(measure);
                };
                if race.is_stopped(index) {
                    race.note_stopped(index);
                    return Err(PoseError::Stopped);
                }
                let result = crate::cancel::with_token(token, measure);
                match &result {
                    Ok((one, _)) => race.finished(index, one),
                    Err(PoseError::Stopped) => race.note_stopped(index),
                    Err(PoseError::Failed(_)) => {}
                }
                result
            })
            .collect();
        // Split serially, in candidate order, so the kept plans have a fixed
        // order whatever thread finished first.
        let mut measured = Vec::with_capacity(raw.len());
        let mut retained = Vec::new();
        for result in raw {
            match result {
                Ok((one, plan)) => {
                    measured.push(RaceOutcome::Measured(one));
                    retained.extend(plan);
                }
                Err(PoseError::Failed(error)) => measured.push(RaceOutcome::Failed(error)),
                Err(PoseError::Stopped) => measured.push(RaceOutcome::Stopped),
            }
        }
        Self::log_batch(candidates, &measured, fidelity, started);
        (measured, retained)
    }
}

/// The plain batch result: no pose is withdrawn without a race, so a stop
/// here cannot happen; it is reported as the failure it would be.
fn plain(outcomes: Vec<RaceOutcome>) -> Vec<Result<MeasuredSupportDemand, String>> {
    outcomes
        .into_iter()
        .map(|outcome| match outcome {
            RaceOutcome::Measured(one) => Ok(one),
            RaceOutcome::Failed(error) => Err(error),
            RaceOutcome::Stopped => Err("the sweep was stopped without a race".to_owned()),
        })
        .collect()
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
        plain(
            Self::measure_batch(
                mesh,
                table,
                candidates,
                fidelity,
                false,
                *self,
                None,
                None,
            )
            .0,
        )
    }

    fn measure_production_race(
        &self,
        mesh: &Mesh,
        table: &OrientFaceTable,
        candidates: &[UpDirection],
        race: &ProductionRace,
    ) -> Vec<RaceOutcome> {
        Self::measure_batch(
            mesh,
            table,
            candidates,
            DemandFidelity::Production,
            false,
            *self,
            Some(race),
            None,
        )
        .0
    }

    fn diagnostic_measurer(&self) -> Option<Box<dyn SupportDemandMeasurer>> {
        // A plain `SweepDemand` already retains nothing.
        Some(Box::new(*self))
    }
}

impl SupportDemandMeasurer for RetainingSweepDemand {
    fn measure_support_demand(
        &self,
        mesh: &Mesh,
        table: &OrientFaceTable,
        candidates: &[UpDirection],
        fidelity: DemandFidelity,
    ) -> Vec<Result<MeasuredSupportDemand, String>> {
        plain(self.measure(mesh, table, candidates, fidelity, None))
    }

    fn measure_production_race(
        &self,
        mesh: &Mesh,
        table: &OrientFaceTable,
        candidates: &[UpDirection],
        race: &ProductionRace,
    ) -> Vec<RaceOutcome> {
        if self.keep_every_finalist {
            // A race in which nothing can be stopped (sabotage tests only).
            let calm = ProductionRace::never_stops(candidates.len());
            return self.measure(mesh, table, candidates, DemandFidelity::Production, Some(&calm));
        }
        self.measure(mesh, table, candidates, DemandFidelity::Production, Some(race))
    }

    fn diagnostic_measurer(&self) -> Option<Box<dyn SupportDemandMeasurer>> {
        // The diagnostics must not touch the retained batch (a production
        // call REPLACES it, so a diagnostic batch would evict the winner's
        // plan): hand them the non-retaining measurer with the same policies.
        Some(Box::new(self.inner))
    }
}

impl RetainingSweepDemand {
    /// One batch, retaining the production plans of the poses that measured
    /// to the end. A pose the race withdrew leaves no plan, and the winner
    /// is never withdrawn, so the winner's plan is always here.
    fn measure(
        &self,
        mesh: &Mesh,
        table: &OrientFaceTable,
        candidates: &[UpDirection],
        fidelity: DemandFidelity,
        race: Option<&ProductionRace>,
    ) -> Vec<RaceOutcome> {
        let retain = matches!(fidelity, DemandFidelity::Production);
        let (measured, retained) = SweepDemand::measure_batch(
            mesh,
            table,
            candidates,
            fidelity,
            retain,
            self.inner,
            race,
            self.shells.as_ref(),
        );
        // A production batch REPLACES what the previous one left; the
        // prefilter keeps nothing and so leaves the store alone.
        if retain {
            *self.retained.lock().unwrap_or_else(PoisonError::into_inner) = retained;
        }
        measured
    }
}

impl SweepDemand {
    /// The one-line disclosure of a batch.
    fn log_batch(
        candidates: &[UpDirection],
        measured: &[RaceOutcome],
        fidelity: DemandFidelity,
        started: Instant,
    ) {
        // Disclosure, not a record: the cost of this stage is printed where a
        // reader of the run log will see it, and is deliberately kept OUT of
        // the sealed `DenseSweepRecord`, whose digest must not depend on a
        // clock.
        let poses: Vec<String> = candidates
            .iter()
            .zip(measured)
            .map(|(candidate, result)| {
                let up = candidate.get();
                let (arms, stiffening, burden, landing, tip) = match result {
                    RaceOutcome::Measured(one) => (
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
                    RaceOutcome::Failed(_) => (
                        "err".to_owned(),
                        "err".to_owned(),
                        "err".to_owned(),
                        "err".to_owned(),
                        "err".to_owned(),
                    ),
                    // Withdrawn by the race (certified loser): no score.
                    RaceOutcome::Stopped => (
                        "stopped".to_owned(),
                        "stopped".to_owned(),
                        "stopped".to_owned(),
                        "stopped".to_owned(),
                        "stopped".to_owned(),
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
    }
}

/// HOLLOW FIRST: the drilled shell one pose would print as, and the holes the
/// sweep must plan around, mapped exactly as `sweep_and_emit` maps the hollow
/// stage's report (drains from the drilled drains, vents from every planned
/// hole).
#[derive(Debug)]
pub struct PoseShell {
    pub mesh: Mesh,
    pub drains: Vec<DrainSpec>,
    pub vents: Vec<DrainSpec>,
}

/// Run the hollow stage on this pose's oriented solid
/// ([`plinth_hollow::drilled_shell_for_pose`], the body of `plinth_hollow::run`).
/// `None` when it does not hollow; the pose is then measured on the solid.
#[must_use]
pub fn pose_shell(
    oriented_solid: &Mesh,
    request: &plinth_hollow::HollowRequest,
    candidate: UpDirection,
) -> Option<PoseShell> {
    let Some(drilled) = plinth_hollow::drilled_shell_for_pose(oriented_solid, request) else {
        let up = candidate.get();
        eprintln!(
            "[orient-demand] hollow-first pose ({:+.4},{:+.4},{:+.4}): the hollow stage does not \
             hollow this pose; measured on the solid",
            up[0], up[1], up[2]
        );
        return None;
    };
    let drains = drilled
        .drain_bores()
        .map(|(entrance, exit, radius)| {
            DrainSpec::from_bore(
                [entrance[0].get(), entrance[1].get(), entrance[2].get()],
                [exit[0].get(), exit[1].get(), exit[2].get()],
                radius.get(),
            )
        })
        .collect();
    let vents = drilled
        .drain_plan()
        .holes()
        .iter()
        .map(|hole| DrainSpec {
            outlet_mm: [
                hole.outer_exit_mm[0].get(),
                hole.outer_exit_mm[1].get(),
                hole.outer_exit_mm[2].get(),
            ],
            outward_axis: [
                hole.direction[0].get(),
                hole.direction[1].get(),
                hole.direction[2].get(),
            ],
            radius_mm: hole.radius_mm.get(),
        })
        .collect();
    let (mesh, ..) = drilled.into_parts();
    Some(PoseShell {
        mesh,
        drains,
        vents,
    })
}

/// The orientation table's per-face fields carried onto a drilled shell.
///
/// Three kinds of shell face (`docs/plans/hollow-first-2026-10-16.md` §4):
///
/// * KEPT: its three vertices, in winding order, are bit-equal to a face of
///   the oriented solid. It is that face, kept by the Boolean, and takes its
///   fragility and display.
/// * TOUCHING: not kept, but it has a vertex bit-equal to a solid vertex: an
///   outer face cut again at a bore mouth. It takes the values of the solid
///   face at those vertices that faces most nearly its way.
/// * Every other face (the cavity surface, a bore wall) is priced 0 for both:
///   a seat there is inside the model or a bore, never seen, on a wall at
///   least the registered minimum thick.
///
/// An empty field stays empty (a stated absence).
struct ShellFields {
    fragility: Vec<f64>,
    display: Vec<f64>,
    kept_faces: usize,
    kept_mm2: f64,
    touching_faces: usize,
    touching_mm2: f64,
    zero_faces: usize,
    zero_mm2: f64,
}

type FaceKey = [[u64; 3]; 3];

impl ShellFields {
    fn carry(solid: &Mesh, shell: &Mesh, fragility: &[f64], display: &[f64]) -> Self {
        // One pass over the solid builds both maps; one pass over the shell
        // reads them.
        let mut by_face: std::collections::HashMap<FaceKey, usize> =
            std::collections::HashMap::with_capacity(solid.tris().len());
        let mut by_vertex: std::collections::HashMap<[u64; 3], Vec<usize>> =
            std::collections::HashMap::with_capacity(solid.verts().len());
        let solid_normals: Vec<[f64; 3]> = solid
            .tris()
            .iter()
            .map(|triangle| face_frame(solid, *triangle).0)
            .collect();
        for (face, triangle) in solid.tris().iter().enumerate() {
            by_face.entry(face_key(solid, *triangle)).or_insert(face);
            for index in triangle {
                by_vertex.entry(vertex_bits(solid, *index)).or_default().push(face);
            }
        }
        let faces = shell.tris().len();
        let mut fields = Self {
            fragility: Vec::with_capacity(if fragility.is_empty() { 0 } else { faces }),
            display: Vec::with_capacity(if display.is_empty() { 0 } else { faces }),
            kept_faces: 0,
            kept_mm2: 0.0,
            touching_faces: 0,
            touching_mm2: 0.0,
            zero_faces: 0,
            zero_mm2: 0.0,
        };
        for triangle in shell.tris() {
            let (normal, area) = face_frame(shell, *triangle);
            let source = if let Some(face) = by_face.get(&face_key(shell, *triangle)) {
                fields.kept_faces += 1;
                fields.kept_mm2 += area;
                Some(*face)
            } else {
                // An outer face the Boolean cut again at a bore mouth still
                // touches the solid's own vertices; the cavity surface and the
                // bore walls do not. Of the solid faces at those vertices, the
                // one facing most nearly the same way (the lowest index on a
                // tie) is the face it was cut from.
                let mut best: Option<(f64, usize)> = None;
                for index in triangle {
                    let at = by_vertex.get(&vertex_bits(shell, *index));
                    for &face in at.into_iter().flatten() {
                        let dot = vec3::dot(solid_normals[face], normal);
                        if best.is_none_or(|(best_dot, best_face)| {
                            dot.total_cmp(&best_dot).then(best_face.cmp(&face)).is_gt()
                        }) {
                            best = Some((dot, face));
                        }
                    }
                }
                if best.is_some() {
                    fields.touching_faces += 1;
                    fields.touching_mm2 += area;
                } else {
                    fields.zero_faces += 1;
                    fields.zero_mm2 += area;
                }
                best.map(|(_, face)| face)
            };
            if !fragility.is_empty() {
                let value = source.and_then(|face| fragility.get(face)).copied();
                fields.fragility.push(value.unwrap_or(0.0));
            }
            if !display.is_empty() {
                let value = source.and_then(|face| display.get(face)).copied();
                fields.display.push(value.unwrap_or(0.0));
            }
        }
        fields
    }
}

fn vertex_bits(mesh: &Mesh, index: u32) -> [u64; 3] {
    mesh.verts()[index as usize].map(f64::to_bits)
}

/// A face's unit normal (zero on a degenerate face) and its area, mm².
fn face_frame(mesh: &Mesh, triangle: [u32; 3]) -> ([f64; 3], f64) {
    let [a, b, c] = triangle.map(|index| mesh.verts()[index as usize]);
    let cross = vec3::cross(vec3::sub(b, a), vec3::sub(c, a));
    let length = vec3::norm(cross);
    if length.is_finite() && length > 0.0 {
        (vec3::scale(cross, 1.0 / length), 0.5 * length)
    } else {
        ([0.0; 3], 0.0)
    }
}

/// A face's three vertices as bits, rotated so the smallest comes first: the
/// same triangle with the same winding gives the same key whatever vertex a
/// mesh lists first; the reversed winding does not.
fn face_key(mesh: &Mesh, triangle: [u32; 3]) -> FaceKey {
    let [a, b, c] = triangle.map(|index| vertex_bits(mesh, index));
    let rotations = [[a, b, c], [b, c, a], [c, a, b]];
    rotations.into_iter().min().unwrap_or([a, b, c])
}

/// The lowest and highest vertex Z, folded exactly as `sweep_and_emit` folds
/// them, so a shell's plate and layer count are the emitted sweep's.
fn z_extent_mm(mesh: &Mesh) -> Result<(f64, f64), String> {
    let (min_z, max_z) = mesh
        .verts()
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), vertex| {
            (lo.min(vertex[2]), hi.max(vertex[2]))
        });
    if min_z.is_finite() && max_z.is_finite() {
        Ok((min_z, max_z))
    } else {
        Err("the drilled shell has no finite Z extent".to_owned())
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

#[cfg(test)]
mod shell_fields_tests {
    use super::{Mesh, ShellFields};

    /// A face kept by the Boolean (same three points, same winding, listed from
    /// another vertex and at another index) takes its solid face's values; a
    /// face that only touches solid vertices takes the value of the solid face
    /// there that faces most nearly its way; a face with no solid vertex is
    /// priced 0; an empty field stays empty.
    #[test]
    fn kept_faces_carry_their_values_and_every_other_face_is_priced_zero() {
        let solid = Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        )
        .expect("a tetrahedron");
        let fragility = [0.1, 0.2, 0.3, 0.4];
        let display = [1.0, 2.0, 3.0, 4.0];
        // The shell lists its vertices in another order and keeps faces 1 and 3
        // (face 3 rotated to start at another vertex). Face 0 reversed (normal
        // +Z) and a new face through two solid vertices (normal (1, 0, 1)/√2)
        // only touch the solid: both face most nearly the way solid face 3
        // (normal (1, 1, 1)/√3) does. The last face touches no solid vertex.
        let shell = Mesh::new(
            vec![
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.5, 0.5, 0.5],
                [0.2, 0.2, 0.2],
                [0.3, 0.2, 0.2],
            ],
            vec![[2, 1, 0], [3, 0, 1], [2, 1, 3], [0, 1, 4], [4, 5, 6]],
        )
        .expect("a shell-like soup");
        let fields = ShellFields::carry(&solid, &shell, &fragility, &display);
        assert_eq!(fields.fragility, vec![0.2, 0.4, 0.4, 0.4, 0.0]);
        assert_eq!(fields.display, vec![2.0, 4.0, 4.0, 4.0, 0.0]);
        assert_eq!(
            (fields.kept_faces, fields.touching_faces, fields.zero_faces),
            (2, 2, 1)
        );
        let empty = ShellFields::carry(&solid, &shell, &fragility, &[]);
        assert!(empty.display.is_empty(), "no display field stays no display field");
        assert_eq!(empty.fragility.len(), 5);
    }
}
