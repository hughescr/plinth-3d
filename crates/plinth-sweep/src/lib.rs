//! The slice-sweep support core.
//!
//! Slice the oriented, lifted model onto a square working raster, one mask per
//! print layer. Sweep **upward** carrying a dimensionless reach potential
//! through cured material; where it exceeds one, the model is unsupported and
//! a strut goes in. Sweep **downward** carrying a load raster in mm² of
//! reference peel, taking the maximum over peel events rather than their sum,
//! and size every arm from the registered area law
//! `π r² ≥ carried_area / X_ALLOWED`.
//!
//! # What this replaces, and why it exists
//!
//! `plinth-evidence/disc-diagnosis-2026-09-02/DISC-DIAGNOSIS.md` §0/§3: flat
//! discs failed because the legacy support stage keyed both its coverage raster
//! and its load ledger to **triangles**. A 98 mm sliver cost 65,536 refinement
//! cells (RC3), and each whole triangle was charged to the seat nearest its
//! centroid, so half a fan's seats carried nothing and emitted as hairline tips
//! while a fifth clamped at the 1.0 mm maximum, leaving load-bearing tips
//! 8.5–9.5 mm apart against a 2.6 mm measured reach (RC4). Nothing in this
//! crate looks at a triangle. Cost is proportional to boundary length and
//! filled area, and load is charged per raster cell.
//!
//! # Doctrine
//!
//! * **Emit and warn, never refuse.** There is no refusal path for a
//!   shortfall. Every one is a field on [`report::SweepRunReport`]. The only
//!   hard errors are internal-consistency assertions — a conservation residual,
//!   a non-monotone carried profile — because those are defects in this code,
//!   not outcomes about a model.
//! * **Batch-first.** Every per-layer step is a whole-array or scanline pass.
//!   Banned here by name: per-triangle refinement of any kind, per-cell
//!   point-in-polygon, per-seat mesh queries, and global rescoring inside the
//!   greedy loop.
//! * **A constant may prune, never decide.** The four Placeholders this crate
//!   reads coarsen a raster, bound a redistribution search that can never
//!   create a bare island, select a peel field whose default is the identity,
//!   and set a discretization that is reported and pinned by an invariance
//!   test.
//! * **Lift is always on.** A zero-lift pose is never a candidate and
//!   [`run_sweep`] refuses one on entry.

#![forbid(unsafe_code)]

pub mod adapter;
pub mod down_sweep;
pub mod drain;
pub mod grid;
pub mod limits;
pub mod orient_demand;
pub mod peel;
pub mod raster;
pub mod reach;
pub mod report;
pub mod up_sweep;

use plinth_core::{m0_constants, LayerPlan};
use plinth_mesh::Mesh;
use plinth_support::perception::{slice_stack, SliceStackLimits};
use thiserror::Error;

pub use adapter::{arms_of, to_primitives, SweepEmission};
pub use drain::{DrainKeepOut, DrainSpec, DrainVerdict, EscapeReport};
pub use limits::SweepLimits;
pub use orient_demand::{burden_cm2, SweepDemand};
pub use peel::{PeelFieldMode, PeelFieldStats};
pub use report::{PathHits, SweepRunReport};

use grid::{plan_grid, Grid};
use raster::{rasterize_into, LayerMasks, RasterScratch};
use reach::ReachState;
use up_sweep::{ColumnHistory, GreedyScratch};

/// Where a strut's bottom sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landing {
    /// The build plate. The sink for every load path that reaches it.
    Plate,
    /// Cured model material, seated at the STRIKE face and truncated there —
    /// it never exits the far side (`docs/physics.md`:284-324).
    Model { layer: usize },
    /// Another strut. The two frusta abut; v1 emits no joint sphere, and that
    /// is disclosed rather than hidden.
    Strut { id: u32, layer: usize },
}

/// One placed arm.
///
/// A strut is analytic and rasterised on demand; it never exists as a stored
/// voxel set. **It roots only its centre column, not its rasterised disc**, so
/// up-sweep rootedness never depends on a radius the down-sweep has not chosen
/// yet and shrinking an arm can never invalidate a placement that was already
/// proved. That under-states a fat arm's reach by up to its own radius, which
/// is the safe direction.
#[derive(Clone, Debug, PartialEq)]
pub struct SweepStrut {
    pub id: u32,
    pub cell: usize,
    pub center_mm: [f64; 2],
    pub k_top: usize,
    pub k_bottom: usize,
    pub landing: Landing,
    /// A landing whose scar nobody will ever see, because model material
    /// stands above the seat in the same column (owner ruling 2026-08-31).
    /// A re-entrant exterior pocket also tests true; that grants an undeserved
    /// bonus and never withholds a deserved one, and the test's form is stated
    /// here rather than left to be discovered.
    pub interior: bool,
    /// Radius at each layer from `k_bottom` upward.
    pub radius_mm: Vec<f64>,
    /// Carried load at each layer from `k_bottom` upward, in mm² of reference
    /// peel.
    pub carried_mm2: Vec<f64>,
    /// Demand cells this arm newly rooted when it went in. The removal pass
    /// asks whether a survivor still covers each of them.
    pub covered_demand: Vec<usize>,
    /// Placed by the down-sweep because the registered area law asked for the
    /// section, not by the up-sweep because reach asked for the cover. Such an
    /// arm roots no demand cell — `covered_demand` is empty by construction —
    /// so the removal pass's coverage test would be vacuously true for it and
    /// give it no protection at all; `remove_under_loaded` excludes any
    /// `inserted_by_load` arm from removal outright rather than let a
    /// vacuous coverage test decide it (see that function's doc for why).
    pub inserted_by_load: bool,
}

impl SweepStrut {
    /// Radius at the tip — the narrowest section, and the one the area law is
    /// evaluated at.
    #[must_use]
    pub fn tip_radius_mm(&self) -> f64 {
        self.radius_mm.last().copied().unwrap_or(0.0)
    }

    /// Total load this arm carries at its tip.
    #[must_use]
    pub fn tip_load_mm2(&self) -> f64 {
        self.carried_mm2.last().copied().unwrap_or(0.0)
    }

    #[must_use]
    pub fn length_mm(&self, layer_height_mm: f64) -> f64 {
        (self.k_top - self.k_bottom) as f64 * layer_height_mm
    }
}

/// The one plan a sweep produces.
#[derive(Clone, Debug)]
pub struct SweepPlan {
    pub grid: Grid,
    pub layers: usize,
    pub layer_height_mm: f64,
    pub plate_z_mm: f64,
    pub struts: Vec<SweepStrut>,
    pub report: SweepRunReport,
}

impl SweepPlan {
    /// `Σ π r²` over the tips — the scar the model wears.
    ///
    /// The invariant this is graded against is
    /// `Σ π r² = injected_load / X_ALLOWED` (equal to `peak_layer_area /
    /// X_ALLOWED` only on flat-bottomed models, where every arm is fed by one
    /// birth layer; `docs/physics.md`:100-118), independent of how many arms
    /// share the load.
    #[must_use]
    pub fn total_scar_mm2(&self) -> f64 {
        self.struts
            .iter()
            .map(|strut| {
                let radius = strut.tip_radius_mm();
                core::f64::consts::PI * radius * radius
            })
            .sum()
    }
}

/// The only failures a sweep can report. Every one of them is a refusal to
/// START, never a verdict on a model: a shortfall is a warning on an emitted
/// file.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum SweepError {
    #[error("the constants registry did not construct: {0}")]
    Registry(String),
    #[error("the layer plan is empty")]
    EmptyLayerPlan,
    #[error(
        "the layer plan is not uniform: layer {index} is {height_mm} mm against {first_mm} mm; \
         the sweep's working voxel is isotropic and assumes one layer height"
    )]
    NonUniformLayerPlan {
        index: usize,
        height_mm: String,
        first_mm: String,
    },
    #[error("the mesh has no finite bounding box")]
    DegenerateMesh,
    #[error(
        "lift is not applied: the plate is at {plate_z_mm} mm and the model starts at \
         {min_z_mm} mm. A zero-lift pose is never a candidate (elephant's-foot ruling)"
    )]
    ZeroLift {
        plate_z_mm: String,
        min_z_mm: String,
    },
    #[error("slicing failed: {0}")]
    Slice(String),
    #[error("the model needs more than {limit} struts")]
    StrutLimit { limit: usize },
}

/// Everything the sweep reads, gathered once from the registry.
#[derive(Clone, Copy, Debug)]
pub struct SweepPolicy {
    /// `LAMINA_REACH_RATIO`, the placement value.
    pub kappa: f64,
    /// `LAMINA_REACH_RATIO_FIRST_LOSS`. **Reporting only.**
    pub kappa_first_loss: f64,
    /// `LAMINA_REACH_RATIO_AUDIT_GATE`. **Reporting only.**
    pub kappa_audit_gate: f64,
    pub l_survived_max_mm: f64,
    pub x_allowed: f64,
    pub tip_radius_min_mm: f64,
    pub tip_radius_max_mm: f64,
    pub owner_growth_ceiling_mm: f64,
    pub target_spacing_mm_registered_but_unused: f64,
    pub beam_model_kappa: f64,
    pub working_pitch_mm: f64,
    /// `STANDOFF_MM`, the registered placement-family gap. Read here for the
    /// drain keep-out, which is a placement search like any other.
    pub standoff_mm: f64,
    /// `HOLLOW_PITCH_MM`. The drain-escape flood is decided at the SAME pitch
    /// the drill stage decides "does the bore fit here" at, so the two cannot
    /// disagree about one question.
    pub passage_voxel_pitch_mm: f64,
    pub strut_removal_passes: u32,
    pub peel_field_mode: PeelFieldMode,
    /// Whether the down-sweep may add arms the AREA LAW asks for, on top of
    /// the ones reach asked for. **On in production and in every oracle.** The
    /// knob exists so a test can measure what the insertion changes rather
    /// than assert that it does something; nothing in the pipeline turns it
    /// off.
    pub insert_by_load: bool,
    /// Whether placement must route around every drain's escape cylinder.
    /// **On in production.** The knob exists so the sabotage drill can delete
    /// the keep-out and show that the drain then reads ENCLOSED -- a test that
    /// asserts the check FIRES rather than that it is merely present.
    pub drain_keep_out: bool,
    pub limits: SweepLimits,
}

impl SweepPolicy {
    /// Read every number from the registry. **Nothing in this crate declares a
    /// physical constant of its own.**
    pub fn from_registry() -> Result<Self, SweepError> {
        let constants = m0_constants().map_err(|error| SweepError::Registry(error.to_string()))?;
        let evidence = constants.evidence();
        let placeholders = constants.placeholders();
        let kappa = *evidence.lamina_reach_ratio().value();
        // The cantilever-bending prediction from the registered Placeholder
        // pair, computed here only so the run report can print the 7.5×
        // disagreement rather than let it go unrecorded. Nothing reads it.
        let tensile_pa = placeholders.tensile_green_mpa().diagnostic().get() * 1.0e6;
        let peel_pa = placeholders.peel_pressure_pa().diagnostic().get();
        let beam_model_kappa = (tensile_pa / (3.0 * peel_pa)).sqrt();
        Ok(Self {
            kappa,
            kappa_first_loss: *evidence.lamina_reach_ratio_first_loss().value(),
            kappa_audit_gate: *evidence.lamina_reach_ratio_audit_gate().value(),
            l_survived_max_mm: *evidence.l_survived_max_mm().value(),
            x_allowed: *evidence.x_allowed().value(),
            tip_radius_min_mm: placeholders.tip_radius_default_mm().diagnostic().get(),
            tip_radius_max_mm: placeholders.tip_radius_max_legacy_mm().diagnostic().get(),
            owner_growth_ceiling_mm: *evidence.tip_radius_max_grown_scar_mm().value(),
            target_spacing_mm_registered_but_unused: placeholders
                .target_spacing_mm()
                .diagnostic()
                .get(),
            beam_model_kappa,
            working_pitch_mm: placeholders.sweep_working_pitch_mm().diagnostic().get(),
            standoff_mm: placeholders.standoff_mm().diagnostic().get(),
            passage_voxel_pitch_mm: placeholders.hollow_pitch_mm().diagnostic().get(),
            strut_removal_passes: u32::try_from(
                placeholders.sweep_strut_removal_passes().diagnostic().get(),
            )
            .unwrap_or(u32::MAX),
            peel_field_mode: PeelFieldMode::from_registry(placeholders),
            insert_by_load: true,
            drain_keep_out: true,
            limits: SweepLimits {
                cell_layer_budget: placeholders.sweep_cell_layer_budget().diagnostic().get(),
                ..SweepLimits::production()
            },
        })
    }

    /// Override the working pitch. The pitch-halving invariance test is what
    /// keeps this from silently deciding an outcome, so it is a first-class
    /// knob rather than a hidden one.
    #[must_use]
    pub const fn with_working_pitch_mm(mut self, pitch_mm: f64) -> Self {
        self.working_pitch_mm = pitch_mm;
        self
    }

    /// Place by reach ALONE, the way the core did before the owner's 2026-09-03
    /// ruling. For measurement only: it is what the load-driven insertion is
    /// graded against.
    #[must_use]
    pub const fn without_load_insertion(mut self) -> Self {
        self.insert_by_load = false;
        self
    }

    /// Place with no regard for where the resin leaves. **Sabotage only** -- it
    /// is what the drain-connectivity test deletes to prove the keep-out is
    /// what keeps the outlet open, and nothing in the pipeline sets it.
    #[must_use]
    pub const fn without_drain_keep_out(mut self) -> Self {
        self.drain_keep_out = false;
        self
    }
}

/// Place and size a support field for one oriented, lifted mesh.
///
/// `plan` supplies the plate: `plan.z0_mm()` is `min_z − lift`, exactly as
/// `crates/plinth-support/src/runner.rs`:2178-2186 computes it. The sweep
/// derives its own layer count and truncates at the model top, because the
/// pipeline's plan carries a +1.0 mm headroom and a 1,000-layer minimum that
/// belong to the sealed-run gates rather than to this engine.
pub fn run_sweep(
    mesh: &Mesh,
    plan: &LayerPlan,
    policy: &SweepPolicy,
) -> Result<SweepPlan, SweepError> {
    run_sweep_with_drains(mesh, plan, policy, &[])
}

/// Place and size a support field, knowing where this model's drains come out.
///
/// The only difference from [`run_sweep`] is that the field is placed around
/// every drain's escape cylinder rather than through it, and that the emitted
/// plan carries a per-drain `connected` / `enclosed` verdict. With an empty
/// `drains` slice the two are the same computation and produce the same bytes,
/// which is what `run_sweep` above relies on.
pub fn run_sweep_with_drains(
    mesh: &Mesh,
    plan: &LayerPlan,
    policy: &SweepPolicy,
    drains: &[DrainSpec],
) -> Result<SweepPlan, SweepError> {
    let (min_z, max_z, min_x, max_x, min_y, max_y) = bounds(mesh)?;
    let plate_z = plan.z0_mm();
    let height = uniform_height(plan)?;
    let mut lift_assertions = 0_u64;
    if plate_z >= min_z {
        return Err(SweepError::ZeroLift {
            plate_z_mm: format!("{plate_z}"),
            min_z_mm: format!("{min_z}"),
        });
    }
    lift_assertions += 1;

    let layers = layer_count(plate_z, max_z, height);
    let margin = policy.tip_radius_max_mm;
    let (grid, grid_plan) = plan_grid(
        min_x,
        min_y,
        max_x,
        max_y,
        layers,
        policy.working_pitch_mm,
        margin,
        policy.limits.cell_layer_budget,
    );

    let planes: Vec<f64> = (0..layers)
        .map(|layer| plate_z + (layer as f64 + 0.5) * height)
        .collect();
    let slices = slice_stack(mesh, &planes, slice_limits(policy))
        .map_err(|error| SweepError::Slice(error.to_string()))?;

    // The drain keep-out is built BEFORE any arm is placed, because a keep-out
    // consulted after placement is a repair, and a repair that deletes arms
    // leaves the demand it was covering with nothing. Empty when there are no
    // drains, and then it allocates nothing and admits everything.
    let keep_out = if policy.drain_keep_out {
        drain::DrainKeepOut::build(
            &grid,
            drains,
            policy.standoff_mm,
            policy.tip_radius_max_mm,
            layers,
            plate_z,
            height,
        )
    } else {
        drain::DrainKeepOut::none()
    };

    // ---------------------------------------------------------------- upward
    let lamina_reach_mm = policy.kappa * height;
    let mut reach = ReachState::new(&grid);
    let mut history = ColumnHistory::new(&grid);
    let mut greedy = GreedyScratch::new(&grid, lamina_reach_mm);
    let mut scratch = RasterScratch::new(&grid);
    let mut masks = LayerMasks::default();
    let mut struts: Vec<SweepStrut> = Vec::new();
    let mut demand_cells_total = 0_usize;
    let mut residual_demand_cells = 0_usize;
    let mut unrooted_landings = 0_usize;
    let mut keep_out_refusals = 0_usize;
    let mut rasteriser_entries = 0_u64;

    for (layer, solids) in slices.iter().enumerate().take(layers) {
        rasteriser_entries += 1;
        rasterize_into(&grid, solids, &mut scratch, &mut masks);
        if masks.cover.iter().all(|filled| !*filled) {
            // No model here. Reset the potential so a column that restarts
            // higher up does not inherit a stale value across the gap.
            reach.u.fill(f32::INFINITY);
            reach::commit_layer(&mut reach, &masks.carry);
            continue;
        }
        reach::advance_thickness(&grid, &masks.cover, &masks.carry, height, &mut reach);
        reach::recompute_potential(&grid, &masks.cover, policy.kappa, height, &[], &mut reach);
        let outcome = up_sweep::place_layer(
            &grid,
            layer,
            &masks.cover,
            &keep_out,
            policy.kappa,
            &mut reach,
            &mut history,
            &mut greedy,
            &mut struts,
            policy.limits.max_struts,
        );
        demand_cells_total += outcome.demand_cells;
        residual_demand_cells += outcome.residual_demand;
        unrooted_landings += outcome.unrooted_landings;
        keep_out_refusals += outcome.keep_out_refusals;
        up_sweep::commit_layer_history(layer, &masks.cover, &reach, &mut history);
        reach::commit_layer(&mut reach, &masks.carry);
    }
    if struts.len() >= policy.limits.max_struts {
        return Err(SweepError::StrutLimit {
            limit: policy.limits.max_struts,
        });
    }

    // The interior test looks UP, so it can only be answered once the upward
    // pass knows how high each column's model reaches.
    for strut in &mut struts {
        strut.interior = matches!(strut.landing, Landing::Model { .. })
            && usize::try_from(history.column_max_model[strut.cell]).unwrap_or(0)
                > strut.k_top + 1;
    }

    // -------------------------------------------------------------- downward
    let sizing = down_sweep::SizingPolicy {
        x_allowed: policy.x_allowed,
        tip_radius_min_mm: policy.tip_radius_min_mm,
        tip_radius_max_mm: policy.tip_radius_max_mm,
        owner_growth_ceiling_mm: policy.owner_growth_ceiling_mm,
        alpha: peel::alpha(policy.peel_field_mode),
        lamina_reach_mm,
    };
    let minimum_load =
        policy.x_allowed * core::f64::consts::PI * policy.tip_radius_min_mm.powi(2);
    // **One balanced pass, both directions.** The same loop adds arms the area
    // law asks for and drops arms it cannot justify, so a strut is neither
    // starved nor duplicated. The two criteria cannot fight over one arm AT
    // THE SAME MEASUREMENT: the insertion trigger is a routed load above
    // `ℓ_max = X_ALLOWED·π·r_max² = 37.6991 mm²` and the removal trigger is a
    // tip load below `ℓ_min = X_ALLOWED·π·r_min² = 0.0942 mm²`, which is 400x
    // smaller, so no arm can read both at once. That 400x gap does NOT by
    // itself prove the LOOP cannot oscillate across passes: each removal pass
    // re-runs the down-sweep with insertion still enabled, on a different
    // strut set, so an arm dropped this pass because ITS OWN load read low
    // could in principle come back next pass if the load it had been
    // carrying lands on a neighbour and pushes that one over `ℓ_max`. What
    // actually rules this out is `remove_under_loaded` refusing to remove any
    // `inserted_by_load` arm at all, regardless of its current load (see that
    // function's doc) — not the trigger gap. Re-entering the down-sweep with
    // the surviving inserted arms already in place is what stops it
    // DUPLICATING them: an arm that is now relieved no longer trips the
    // trigger, so the second call inserts nothing where the first one
    // already did. The loop is still bounded by `SWEEP_STRUT_REMOVAL_PASSES`
    // and `InsertionPolicy::max_rounds`, and hitting either bound is reported
    // (`insertion_rounds_exhausted`), not hidden.
    let insertion = if policy.insert_by_load {
        down_sweep::InsertionPolicy::new(policy.limits.max_struts)
    } else {
        down_sweep::InsertionPolicy::disabled()
    };
    let mut outcome = down_sweep::run(&grid, &slices, &mut struts, sizing, insertion, &keep_out);
    let mut removals = 0_u64;
    for _ in 0..policy.strut_removal_passes {
        let removed = remove_under_loaded(&grid, &mut struts, minimum_load, lamina_reach_mm);
        if removed == 0 {
            break;
        }
        removals += removed as u64;
        outcome = down_sweep::run(&grid, &slices, &mut struts, sizing, insertion, &keep_out);
    }
    // The interior test again, now that the down-sweep has added arms of its
    // own. Same test, same column history; running it twice is cheaper than
    // giving the down-sweep a reason to know about the up-sweep's history.
    for strut in &mut struts {
        strut.interior = matches!(strut.landing, Landing::Model { .. })
            && usize::try_from(history.column_max_model[strut.cell]).unwrap_or(0)
                > strut.k_top + 1;
    }

    // The escape check runs on the FINISHED field, struts and all, because a
    // check that ran before sizing would be answering about a field nobody
    // prints. Its verdict is published and warned on; it never withdraws an
    // arm and never refuses a model.
    let escape = drain::check_escape(
        &grid,
        &slices,
        &struts,
        drains,
        plate_z,
        height,
        policy.passage_voxel_pitch_mm,
        policy.tip_radius_max_mm,
    );

    let mut report = build_report(
        &grid,
        layers,
        grid_plan.coarsening_steps,
        policy,
        &reach,
        &struts,
        &outcome,
        demand_cells_total,
        residual_demand_cells,
        unrooted_landings,
        rasteriser_entries,
        lift_assertions,
        removals,
    );
    report.drain_keep_out_cells = keep_out.blocked_cells();
    report.drain_keep_out_refusals = keep_out.refusals();
    report.drain_seat_refusals = keep_out_refusals;
    report.escape = escape;
    Ok(SweepPlan {
        grid,
        layers,
        layer_height_mm: height,
        plate_z_mm: plate_z,
        struts,
        report,
    })
}

/// Drop arms carrying less than the smallest printable tip can justify, and
/// hand their load to their neighbours.
///
/// **This can never create a bare island**, which is what makes
/// `SWEEP_STRUT_REMOVAL_PASSES` unable to decide an outcome: a REACH-placed arm
/// is deleted only if EVERY demand cell it rooted is still within one lamina's
/// reach of a survivor at the same layer. A hairline arm is better than a bare
/// island, so otherwise it is kept and reported with its load.
///
/// A LOAD-placed arm (`inserted_by_load`) is never a candidate here, full
/// stop — it is excluded below rather than let the coverage test decide it.
/// It roots no demand cell, so `covered_demand` is empty by construction and
/// the coverage test above is vacuously true for it: nothing stops it being
/// dropped the moment its OWN tip load reads low, with no check on whether the
/// overload it exists to relieve is still there. That is a real gap, not a
/// hypothetical one: the down-sweep is re-run with insertion still enabled
/// after every removal pass (`sweep`'s loop, above), so a dropped load-arm can
/// come straight back the next pass if the load it was carrying lands on
/// another arm and pushes it over `ℓ_max` again — an insert/remove cycle
/// bounded only by `SWEEP_STRUT_REMOVAL_PASSES`, not by the 400x gap between
/// `ℓ_min` and `ℓ_max` (that gap only rules out one arm tripping both triggers
/// in the SAME measurement; it says nothing across two different down-sweep
/// calls with a different strut set in between). Excluding load-placed arms
/// here removes the cycle rather than merely bounding it: the area law placed
/// them for a reason a load-magnitude threshold cannot re-litigate.
fn remove_under_loaded(
    grid: &Grid,
    struts: &mut Vec<SweepStrut>,
    minimum_load_mm2: f64,
    reach_mm: f64,
) -> usize {
    let candidates: Vec<usize> = struts
        .iter()
        .enumerate()
        .filter(|(_, strut)| !strut.inserted_by_load && strut.tip_load_mm2() < minimum_load_mm2)
        .map(|(index, _)| index)
        .collect();
    if candidates.is_empty() {
        return 0;
    }
    let mut doomed = vec![false; struts.len()];
    for &candidate in &candidates {
        let covered = struts[candidate].covered_demand.clone();
        let layer = struts[candidate].k_top;
        let still_covered = covered.iter().all(|&cell| {
            struts.iter().enumerate().any(|(index, other)| {
                index != candidate
                    && !doomed[index]
                    && other.k_top == layer
                    && chamfer_distance_mm(grid, cell, other.cell) <= reach_mm
            })
        });
        if still_covered {
            doomed[candidate] = true;
        }
    }
    let removed = doomed.iter().filter(|flag| **flag).count();
    if removed == 0 {
        return 0;
    }
    let mut kept = Vec::with_capacity(struts.len() - removed);
    let mut remap = vec![u32::MAX; struts.len()];
    for (index, strut) in struts.iter().enumerate() {
        if doomed[index] {
            continue;
        }
        remap[index] = u32::try_from(kept.len()).unwrap_or(u32::MAX);
        kept.push(strut.clone());
    }
    for (fresh, strut) in kept.iter_mut().enumerate() {
        strut.id = u32::try_from(fresh).unwrap_or(u32::MAX);
        if let Landing::Strut { id, layer } = strut.landing {
            let mapped = remap[id as usize];
            // A parent that was itself removed cannot be landed on; the child
            // falls through to the plate rather than pointing at nothing.
            strut.landing = if mapped == u32::MAX {
                Landing::Plate
            } else {
                Landing::Strut { id: mapped, layer }
            };
        }
    }
    *struts = kept;
    removed
}

/// Distance in the SAME 8-neighbour chamfer metric the reach potential and the
/// spread's feature transform use.
///
/// The removal test must ask its question in the metric the engine actually
/// covers in. Asking it in the Euclidean metric — which is up to 8 % SHORTER
/// than the chamfer distance — would delete an arm whose demand cells are only
/// Euclidean-covered, and that is precisely how a pruning constant creates a
/// bare island.
pub(crate) fn chamfer_distance_mm(grid: &Grid, a: usize, b: usize) -> f64 {
    let rows = grid.row_of(a).abs_diff(grid.row_of(b));
    let cols = grid.col_of(a).abs_diff(grid.col_of(b));
    let (long, short) = if rows > cols { (rows, cols) } else { (cols, rows) };
    grid.pitch_mm() * ((long - short) as f64 + core::f64::consts::SQRT_2 * short as f64)
}

fn cell_distance_mm(grid: &Grid, a: usize, b: usize) -> f64 {
    let dx = grid.center_x_mm(grid.col_of(a)) - grid.center_x_mm(grid.col_of(b));
    let dy = grid.center_y_mm(grid.row_of(a)) - grid.center_y_mm(grid.row_of(b));
    dx.hypot(dy)
}

fn slice_limits(policy: &SweepPolicy) -> SliceStackLimits {
    SliceStackLimits {
        max_planes: policy.limits.max_planes,
        max_surface_grid_bytes: policy.limits.max_surface_grid_bytes,
        ..SliceStackLimits::production()
    }
}

fn bounds(mesh: &Mesh) -> Result<(f64, f64, f64, f64, f64, f64), SweepError> {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for vertex in mesh.verts() {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex[axis]);
            max[axis] = max[axis].max(vertex[axis]);
        }
    }
    if !(min[2].is_finite() && max[2].is_finite() && max[2] > min[2]) {
        return Err(SweepError::DegenerateMesh);
    }
    Ok((min[2], max[2], min[0], max[0], min[1], max[1]))
}

fn uniform_height(plan: &LayerPlan) -> Result<f64, SweepError> {
    let heights = plan.heights_mm();
    let first = *heights.first().ok_or(SweepError::EmptyLayerPlan)?;
    for (index, height) in heights.iter().enumerate() {
        if (height - first).abs() > 1.0e-12 {
            return Err(SweepError::NonUniformLayerPlan {
                index,
                height_mm: format!("{height}"),
                first_mm: format!("{first}"),
            });
        }
    }
    Ok(first)
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the layer count is a finite positive quotient of two model-scale lengths, ceil-ed and \
          then raised to at least one; the cast cannot truncate a reachable value or lose a sign"
)]
fn layer_count(plate_z: f64, max_z: f64, height: f64) -> usize {
    (((max_z - plate_z) / height).ceil().max(1.0) as usize).max(1)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the report is assembled from every stage's telemetry at once, on purpose: gathering it \
          into an intermediate struct would create a second place for a field to be forgotten"
)]
fn build_report(
    grid: &Grid,
    layers: usize,
    coarsening_steps: u32,
    policy: &SweepPolicy,
    reach: &ReachState,
    struts: &[SweepStrut],
    outcome: &down_sweep::DownSweepOutcome,
    demand_cells_total: usize,
    residual_demand_cells: usize,
    unrooted_landings: usize,
    rasteriser_entries: u64,
    lift_assertions: u64,
    removals: u64,
) -> SweepRunReport {
    let mut landings = report::LandingCounts::default();
    let mut radii: Vec<f64> = Vec::with_capacity(struts.len());
    let mut slenderness: Vec<f64> = Vec::with_capacity(struts.len());
    let height = policy.l_survived_max_mm / policy.kappa;
    for strut in struts {
        match strut.landing {
            Landing::Plate => landings.plate += 1,
            Landing::Model { .. } => landings.model += 1,
            Landing::Strut { .. } => landings.strut += 1,
        }
        landings.interior += usize::from(strut.interior);
        let tip = strut.tip_radius_mm();
        radii.push(tip);
        let length = strut.length_mm(height);
        let widest = strut.radius_mm.first().copied().unwrap_or(tip).max(tip);
        if widest > 0.0 {
            slenderness.push(length / (2.0 * widest));
        }
    }
    radii.sort_by(f64::total_cmp);
    slenderness.sort_by(f64::total_cmp);
    let tip_radius = report::TipRadiusStats {
        min_mm: radii.first().copied().unwrap_or(0.0),
        p50_mm: percentile(&radii, 0.50),
        max_mm: radii.last().copied().unwrap_or(0.0),
        count_at_min: radii
            .iter()
            .filter(|radius| **radius <= policy.tip_radius_min_mm + 1.0e-12)
            .count(),
        count_at_max: radii
            .iter()
            .filter(|radius| **radius >= policy.tip_radius_max_mm - 1.0e-12)
            .count(),
        count_above_owner_growth_ceiling: radii
            .iter()
            .filter(|radius| **radius > policy.owner_growth_ceiling_mm)
            .count(),
    };
    let seat_spacing = spacing_stats(grid, struts);
    let total_scar: f64 = radii
        .iter()
        .map(|radius| core::f64::consts::PI * radius * radius)
        .sum();

    SweepRunReport {
        // Filled by the caller, which is the only place that knows the drains:
        // this function is handed the sweep's own measurements and nothing else.
        drain_keep_out_cells: 0,
        drain_keep_out_refusals: 0,
        drain_seat_refusals: 0,
        escape: crate::drain::EscapeReport::default(),
        achieved_pitch_mm: grid.pitch_mm(),
        coarsening_steps,
        layers,
        cells_per_layer: grid.cells(),
        reach: report::ReachProvenance {
            kappa_used: policy.kappa,
            kappa_first_loss: policy.kappa_first_loss,
            kappa_audit_gate: policy.kappa_audit_gate,
            l_survived_max_mm: policy.l_survived_max_mm,
            layer_height_mm: height,
            beam_model_kappa: policy.beam_model_kappa,
            target_spacing_mm_registered_but_unused: policy
                .target_spacing_mm_registered_but_unused,
        },
        // The field was placed at the measured lower bound; the audit gates at
        // 2.5x denser. This is stated, not resolved -- owner ruling OR-2.
        will_pass_audit_span_check: seat_spacing.max_mm
            <= 2.0 * policy.kappa_audit_gate * height,
        max_tau_at_binding_reach_mm: reach.tau_at_binding_reach_max_mm,
        u_from_vertical_max: reach.u_from_vertical_max,
        tau_vertical_over_tau_used_max: reach.tau_vertical_over_used_max,
        chamfer_not_converged: reach.chamfer_not_converged,
        struts_total: struts.len(),
        landings,
        seat_spacing,
        tip_radius,
        slenderness_p50: percentile(&slenderness, 0.50),
        slenderness_max: slenderness.last().copied().unwrap_or(0.0),
        injected_load_mm2: outcome.injected_load_mm2,
        peak_layer_area_mm2: outcome.peak_layer_area_mm2,
        model_section_max_x: outcome.model_section_max_x,
        total_scar_mm2: total_scar,
        plate_reaction_mm2: outcome.plate_reaction_mm2,
        demand_area_mm2: demand_cells_total as f64 * grid.cell_area_mm2(),
        residual_demand_cells,
        residual_demand_mm2: residual_demand_cells as f64 * grid.cell_area_mm2(),
        uncarried_load_mm2: outcome.uncarried_load_mm2,
        uncarried_lowest_layer: outcome.uncarried_lowest_layer,
        uncarried_worst_gap_mm: outcome.uncarried_worst_gap_mm,
        sweeps_disagree: residual_demand_cells == 0 && outcome.uncarried_load_mm2 > 0.0,
        route_not_converged: outcome.route_not_converged,
        arms_inserted_by_load: struts.iter().filter(|strut| strut.inserted_by_load).count(),
        insertion_rounds_exhausted: outcome.insertion_rounds_exhausted,
        insertion_geometry_exhausted: outcome.insertion_geometry_exhausted,
        insertion_rounds: outcome.insertion_rounds,
        unrooted_landings,
        struts_at_minimum_radius: tip_radius.count_at_min,
        overloaded_struts: outcome.overloaded.clone(),
        model_section_overload: outcome.model_overload.clone(),
        conservation_residual: outcome.conservation_residual,
        peel_field_mode: policy.peel_field_mode,
        peel_field_stats: PeelFieldStats::observe(
            outcome.min_layer_area_mm2,
            outcome.peak_layer_area_mm2,
            314.0,
        ),
        sweep_digest: digest(struts),
        counters: PathHits {
            reach_tests: reach.reach_tests,
            vertical_inheritance_hits: reach.vertical_inheritance_hits,
            multi_carrier_split_hits: outcome.spread_search_hits,
            strut_share_reads: outcome.strut_share_reads,
            landing_injections: outcome.landing_injections,
            max_combine_hits: outcome.max_combine_hits,
            rasteriser_entries: rasteriser_entries + outcome.rasteriser_entries,
            coarsening_loop_hits: u64::from(coarsening_steps),
            // Both reach constants are read exactly once per run, from the
            // registry, in `SweepPolicy::from_registry`. Sabotage S8 swaps
            // them and `will_pass_audit_span_check` must flip.
            kappa_placement_reads: 1,
            kappa_audit_gate_reads: 1,
            lift_assertions,
            strut_removals: removals,
        },
    }
}

fn spacing_stats(grid: &Grid, struts: &[SweepStrut]) -> report::SpacingStats {
    if struts.len() < 2 {
        return report::SpacingStats::default();
    }
    let mut nearest: Vec<f64> = Vec::with_capacity(struts.len());
    for (index, strut) in struts.iter().enumerate() {
        let mut best = f64::INFINITY;
        for (other_index, other) in struts.iter().enumerate() {
            if other_index == index {
                continue;
            }
            best = best.min(cell_distance_mm(grid, strut.cell, other.cell));
        }
        if best.is_finite() {
            nearest.push(best);
        }
    }
    nearest.sort_by(f64::total_cmp);
    report::SpacingStats {
        mean_mm: nearest.iter().sum::<f64>() / nearest.len() as f64,
        p95_mm: percentile(&nearest, 0.95),
        max_mm: nearest.last().copied().unwrap_or(0.0),
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "an index into a sorted slice, produced by multiplying its length by a unit fraction \
          and then clamped to the last element"
)]
fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() as f64 - 1.0) * fraction).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

/// A stable identity for one placed field, in the role
/// `SupportVerificationSnapshot::digest` plays today.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a digest deliberately reinterprets a quantized mm or mm2 value as bits; truncation and \
          sign reinterpretation are the POINT of a hash mixer, not a hazard in it"
)]
fn digest(struts: &[SweepStrut]) -> String {
    let mut accumulator: u64 = 0xcbf2_9ce4_8422_2325;
    let mut absorb = |value: u64| {
        accumulator ^= value;
        accumulator = accumulator.wrapping_mul(0x0000_0100_0000_01b3);
    };
    for strut in struts {
        absorb(strut.cell as u64);
        absorb(strut.k_top as u64);
        absorb(strut.k_bottom as u64);
        absorb(match strut.landing {
            Landing::Plate => 1,
            Landing::Model { layer } => 2 ^ ((layer as u64) << 8),
            Landing::Strut { id, layer } => 3 ^ (u64::from(id) << 8) ^ ((layer as u64) << 32),
        });
        for radius in &strut.radius_mm {
            absorb((radius * 1.0e6).round() as i64 as u64);
        }
        for carried in &strut.carried_mm2 {
            absorb((carried * 1.0e6).round() as i64 as u64);
        }
    }
    format!("{accumulator:016x}")
}

#[cfg(test)]
mod removal_tests {
    use super::{plan_grid, remove_under_loaded, Landing, SweepStrut};

    /// Owner follow-up (2026-09-03, item 4 of the slice-sweep review): a
    /// LOAD-placed arm has empty `covered_demand` by construction, so the
    /// coverage test `remove_under_loaded` runs for a REACH-placed arm is
    /// vacuously true for it and gives it no protection at all. Before the
    /// fix this arm would have been removed the moment its own tip load read
    /// below `minimum_load_mm2`, with nothing checking whether the overload
    /// it exists to relieve was still there — an insert/remove cycle across
    /// removal passes, bounded only by `SWEEP_STRUT_REMOVAL_PASSES`. This
    /// proves the fix: a load-inserted arm below the removal threshold
    /// survives the pass regardless.
    #[test]
    fn an_inserted_arm_survives_the_removal_pass_even_when_its_own_load_reads_low() {
        let (grid, _) = plan_grid(0.0, 0.0, 10.0, 10.0, 1, 1.0, 1.0, 1_000_000);
        let minimum_load_mm2 = 1.0;
        let mut struts = vec![SweepStrut {
            id: 0,
            cell: 0,
            center_mm: [0.0, 0.0],
            k_top: 5,
            k_bottom: 0,
            landing: Landing::Plate,
            interior: false,
            radius_mm: vec![0.1],
            // Below minimum_load_mm2: this is exactly the state that made a
            // load-inserted arm a removal candidate before the fix.
            carried_mm2: vec![0.5],
            covered_demand: Vec::new(),
            inserted_by_load: true,
        }];

        let removed = remove_under_loaded(&grid, &mut struts, minimum_load_mm2, 5.0);

        assert_eq!(
            removed, 0,
            "a load-inserted arm below the removal threshold must never be a removal candidate"
        );
        assert_eq!(
            struts.len(),
            1,
            "the load-inserted arm must survive the removal pass"
        );
        assert!(struts[0].inserted_by_load);
    }

    /// The counterpart: a REACH-placed arm (`inserted_by_load: false`) below
    /// the threshold, with no demand cells to protect it, is still removed —
    /// the fix must not blanket-protect every low-load arm, only the
    /// load-placed ones.
    #[test]
    fn a_reach_placed_arm_with_no_demand_to_protect_it_is_still_removed() {
        let (grid, _) = plan_grid(0.0, 0.0, 10.0, 10.0, 1, 1.0, 1.0, 1_000_000);
        let minimum_load_mm2 = 1.0;
        let mut struts = vec![SweepStrut {
            id: 0,
            cell: 0,
            center_mm: [0.0, 0.0],
            k_top: 5,
            k_bottom: 0,
            landing: Landing::Plate,
            interior: false,
            radius_mm: vec![0.1],
            carried_mm2: vec![0.5],
            covered_demand: Vec::new(),
            inserted_by_load: false,
        }];

        let removed = remove_under_loaded(&grid, &mut struts, minimum_load_mm2, 5.0);

        assert_eq!(removed, 1, "a reach-placed arm below the threshold with nothing to protect it should still be removed");
        assert!(struts.is_empty());
    }
}
