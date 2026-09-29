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
pub mod congestion;
pub mod down_sweep;
pub mod drain;
pub mod grid;
pub mod lean;
pub mod limits;
pub mod model_landing;
pub mod orient_demand;
pub mod peel;
pub mod raster;
pub mod reach;
pub mod report;
pub mod slenderness;
pub mod tip_landing;
pub mod up_sweep;

use plinth_core::{m0_constants, LayerPlan};
use plinth_mesh::Mesh;
use plinth_support::perception::{slice_stack, SliceStackLimits};
use thiserror::Error;

pub use adapter::{arms_of, to_primitives, SweepEmission};
pub use congestion::{CongestionCensus, CongestionOutcome, CongestionReport};
pub use drain::{DrainKeepOut, DrainSpec, DrainVerdict, EscapeReport};
pub use limits::SweepLimits;
pub use model_landing::{model_landing_scar, model_landings, ModelLanding, ModelLandingScar};
pub use orient_demand::{burden_cm2, SweepDemand};
pub use peel::{PeelFieldMode, PeelFieldStats};
pub use report::{PathHits, SweepRunReport};
pub use slenderness::{
    unbraced_runs, SlendernessContext, SlendernessLaw, SlendernessOutcome, UnbracedRun,
    UnbracedWarning,
};
pub use tip_landing::{tip_contact_scar, tip_landings, TipCharge, TipContactScar, TipLanding};

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

/// What ROLE a placed member has, which decides what the emit boundary builds
/// for it and whether the model ever sees it.
///
/// The slenderness lane (2026-09-06) adds two members that are not support
/// arms: a diagonal that ties two shafts together, and a plate-rooted post that
/// exists only to be tied to. Both are ordinary struts in the load ledger and
/// in the descent — no new record type, exactly as the plan of record asks —
/// but neither TOUCHES THE MODEL, so neither may be given a tip ball, and
/// neither leaves a scar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StrutKind {
    /// A support arm: it seats into the model and carries peel load.
    #[default]
    Arm,
    /// One diagonal of an `|X|` cross-brace. Its top seats ON another arm's
    /// shaft and its bottom lands on a second arm's shaft; it carries no peel
    /// load of its own beyond its own material, and it gets no tip ball and no
    /// root flare at emit.
    Brace,
    /// A plate-rooted post dropped where nothing else was within brace reach,
    /// so that a slender arm has something to brace TO. Its top stands in free
    /// space, so it gets no tip ball; its bottom is the plate, so it keeps the
    /// plate-root chain and its flare.
    Mast,
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
    /// What this member IS. `Arm` for every strut the reach and the area law
    /// place; `Brace` and `Mast` only ever come from the slenderness lane.
    pub kind: StrutKind,
    /// A radius this member may never be sized BELOW, in mm, whatever load the
    /// area law routes to it.
    ///
    /// `0` for every support arm: an arm's section is the area law's answer and
    /// nothing else. A brace and a mast carry almost no peel load, so the area
    /// law would give them the smallest printable tip and their OWN slenderness
    /// would then be violated by construction — a 4 mm diagonal at a 0.05 mm
    /// radius is 80:1. This field is that self-consistency, applied at the one
    /// place the radius is chosen (`down_sweep::run`), and it is a MINIMUM, so
    /// a brace that some load does reach still grows under the area law.
    pub min_radius_mm: f64,
    /// For a brace, the arm its TOP seats on. `None` for everything else.
    ///
    /// The bottom end of a brace is its `landing`, which the ledger already
    /// records; the top end is a lateral tie on a second shaft that no other
    /// field names, and the whole point of an `|X|` is that both shafts are
    /// tied at both ends of the band. Recording it here is what lets the
    /// unbraced-run measurement be recomputed from the finished strut set
    /// alone, by a test or by the run report, rather than trusted from a
    /// counter kept while the braces were being placed.
    pub seat_on_strut: Option<u32>,

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
    /// The cell this arm's axis occupies at each layer, indexed exactly as
    /// `radius_mm` is (`slot = layer - k_bottom`).
    ///
    /// **EMPTY MEANS STRAIGHT DOWN**, which is every arm the descent found no
    /// cheaper lean for, and every arm this engine produced before 2026-09-04.
    /// A straight arm allocates nothing here, so the common case pays no
    /// memory and every reader that wants the cell at a layer goes through
    /// [`SweepStrut::cell_at`] rather than reading `cell` directly.
    ///
    /// While a down-sweep arm is still growing the path is written TOP-DOWN
    /// beside its ledger, and is reversed with the ledger the moment it lands.
    pub path: Vec<u32>,
    /// The straight-down descent this arm WOULD have taken, recorded at
    /// placement, and `None` for an arm that did not lean.
    ///
    /// It is recorded rather than recomputed because [`up_sweep::ColumnHistory`]
    /// is a RUNNING state, not a record: by the end of the up-sweep every
    /// column's `top_model` holds the topmost model layer in that column
    /// overall — which may be far ABOVE this arm's seat — and the arm has
    /// stamped its own seat column with its own `k_top + 1`. Re-descending
    /// against that history lands the arm on ITSELF at its own seat layer, a
    /// zero-height stub. That defect was latent from 2026-09-04 (it fired zero
    /// times on eight models, which is unmeasured, not safe) and this field is
    /// the fix.
    pub straight_fallback: Option<(Landing, usize)>,
    /// The descent path that went with `straight_fallback` when a forced
    /// merge recorded it, so an un-merge restores the member the placement
    /// chose (possibly leaned) rather than a vertical column it never used.
    pub fallback_path: Vec<u32>,
    /// Cured model laminae standing at and above this arm's own `k_top`, in
    /// its own tip column — the thickness the tip has to seat INTO.
    ///
    /// Filled by [`seat_solid_layers`] once the strut set is final, because
    /// the down-sweep both adds and removes arms and a value captured earlier
    /// would belong to a field nobody prints. `0` means "unmeasured", and the
    /// lowering then seats nothing rather than guessing: an arm that reads
    /// zero is counted in `tips_truncated_by_thin_wall`, never dropped.
    ///
    /// It is DERIVED from the model and the arm's own seat, so it is
    /// deliberately absent from [`digest`]: the plan digest names the
    /// PLACEMENT, and this field moves no arm.
    pub seat_solid_layers: u32,
    /// Outward unit normal of the model face this tip carries, filled by
    /// [`tip_seat_normals`] once the strut set is final. `[0, 0, 0]` means
    /// unmeasured, and the lowering falls back to the arm's own axis.
    ///
    /// Derived, like [`Self::seat_solid_layers`], and likewise not digested.
    pub seat_normal: [f64; 3],
}

impl SweepStrut {
    /// Radius at the tip — the narrowest section, and the one the area law is
    /// evaluated at.
    #[must_use]
    pub fn tip_radius_mm(&self) -> f64 {
        self.radius_mm.last().copied().unwrap_or(0.0)
    }

    /// The cell the axis occupies at `layer`.
    ///
    /// The seat cell for a straight arm, and the leaned path's own entry
    /// otherwise. Out of range reads fall back to the seat cell rather than
    /// panicking: a reader outside `[k_bottom, k_top)` is asking about a layer
    /// this arm is not alive on, and the seat is the honest answer for the
    /// interior and spacing tests that do exactly that.
    #[must_use]
    pub fn cell_at(&self, layer: usize) -> usize {
        if self.path.is_empty() || self.k_bottom == usize::MAX || layer < self.k_bottom {
            return self.cell;
        }
        self.path
            .get(layer - self.k_bottom)
            .map_or(self.cell, |cell| *cell as usize)
    }

    /// The cell the axis occupies where it lands.
    #[must_use]
    pub fn base_cell(&self) -> usize {
        self.path.first().map_or(self.cell, |cell| *cell as usize)
    }

    /// Whether the descent leaned this arm rather than dropping it straight.
    #[must_use]
    pub fn is_leaned(&self) -> bool {
        !self.path.is_empty() && self.base_cell() != self.cell
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

    /// The resin this arm costs, in mm³: `π r²` integrated along its own axis.
    ///
    /// One term per ledger slot, because the radius is constant within a layer
    /// and the ledger is indexed by exactly that. A leaned layer is measured
    /// along its SLANT — `hypot(h, sideways)` — because that is the material a
    /// leaning layer actually needs; measuring it by its rise would understate
    /// the very descents the lean law is being graded on. The top slot has no
    /// layer above it to lean toward and is measured at `h`.
    ///
    /// This is the arm alone. The tip ball, the boss and the root flare are the
    /// lowering's primitives and are not counted here, which is stated so the
    /// number is never mistaken for the emitted file's own volume.
    #[must_use]
    pub fn volume_mm3(&self, grid: &grid::Grid, layer_height_mm: f64) -> f64 {
        let mut total = 0.0;
        for (slot, radius) in self.radius_mm.iter().enumerate() {
            let here = self.k_bottom.saturating_add(slot);
            let sideways = if self.path.is_empty() {
                0.0
            } else {
                cell_distance_mm(grid, self.cell_at(here), self.cell_at(here + 1))
            };
            let segment = if slot + 1 == self.radius_mm.len() {
                layer_height_mm
            } else {
                layer_height_mm.hypot(sideways)
            };
            total += core::f64::consts::PI * radius * radius * segment;
        }
        total
    }

    /// `π r_root²` — the area this arm presents where it stops.
    ///
    /// Read at the ROOT and not the tip: the root is the widest section (the
    /// carried load is non-decreasing downward), and it is what a plate landing
    /// asks the plate to hold.
    #[must_use]
    pub fn root_contact_area_mm2(&self) -> f64 {
        let radius = self.radius_mm.first().copied().unwrap_or(0.0);
        core::f64::consts::PI * radius * radius
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
    /// `OVERHANG_ANGLE_DEG`, the centre of the logistic overhang weight the
    /// descent prices a lean by.
    pub overhang_centre_deg: f64,
    /// `OVERHANG_WEIGHT_TRANSITION_DEG`, that logistic's 10-to-90 width.
    pub overhang_transition_deg: f64,
    /// Whether a descent may lean at all. **Sabotage knob**: with it false
    /// every arm drops straight down from its seat, which is exactly how this
    /// engine behaved before 2026-09-04.
    pub lean: bool,
    /// Whether the reach potential prices a wall's creep by the registered
    /// overhang weight of the mesh surface it crosses
    /// ([`reach::SurfaceCreep`], field report 2026-09-28). **On in
    /// production.** The knob exists so the drill can put the full-cost rule
    /// back and watch Round 50's drafted rim draw its minimum-tip arms again;
    /// nothing in the pipeline turns it off.
    pub creep_weight: bool,
    /// Whether a descent may land part-way down another arm's SHAFT, and not
    /// only on its top. **On in production.** The knob exists so a test can
    /// show that a mid-shaft junction is what a given arm chose, by putting the
    /// engine back on the top-only rule and watching the same arm go to the
    /// plate; nothing in the pipeline turns it off.
    pub shaft_landings: bool,
    /// Whether placement must route around every drain's escape cylinder.
    /// **On in production.** The knob exists so the sabotage drill can delete
    /// the keep-out and show that the drain then reads ENCLOSED -- a test that
    /// asserts the check FIRES rather than that it is merely present.
    pub drain_keep_out: bool,
    /// Whether the removal check must prove that EVERY layer of an arm's
    /// carried span stays covered before it deletes the arm, rather than its
    /// seat layer alone. **On in production.** The knob exists so the drill
    /// can put the seat-only rule back and watch the same arm be deleted with
    /// a gap opening under it (`refuse_uncovered_spans`).
    pub removal_span_check: bool,
    /// Whether an arm offers the down-sweep's routing field the column its own
    /// axis occupies ONE LAYER UP, and not only the column it occupies here.
    /// **On in production.** For a straight arm the two columns are the same
    /// cell and the flag decides nothing; for a LEANED arm they differ, and the
    /// column one layer up is the one the up-sweep rooted when it placed the
    /// arm. The knob exists so the drill can put the seat-blind accounting back
    /// and watch the load the arm's own tip holds go uncarried
    /// (`down_sweep::run`).
    pub tip_column_roots: bool,
    /// `SLENDERNESS_LIMIT_RATIO`, the largest unbraced-run length over radius a
    /// member may run before a lateral tie is required.
    pub slenderness_ratio: f64,
    /// `FLARE_R_FACTOR`, the registered factor the emitter applies to a plate
    /// arm's root radius when it lays the root flare down. The congestion
    /// census measures the disc the emitter ACTUALLY puts on the plate, so it
    /// reads the same number the lowering does rather than the bare radius.
    pub flare_r_factor: f64,
    /// Whether the congestion lane runs at all. **Sabotage knob**: with it
    /// false no footprint is measured for crowding, no root is re-landed and
    /// no flank arm is inserted, which is the field this engine placed before
    /// 2026-09-06.
    pub congestion: bool,
    /// Whether remedy 1 -- re-landing a stacked plate root on clear plate
    /// outside the silhouette -- may fire. **On in production.** The drill
    /// turns it off and watches the footprint stay congested.
    pub congestion_outward_lean: bool,
    /// Whether remedy 2 -- new arms seated on the body's flanks -- may fire.
    /// **On in production.** The drill turns it off and watches the warning
    /// survive, with NEVER fewer arms than before.
    pub congestion_flank_arms: bool,
    /// Whether the slenderness lane runs at all. **Sabotage knob**: with it
    /// false no run is measured, no remedy is applied and the field is exactly
    /// the field this engine placed before 2026-09-06, which is what the
    /// byte-identity comparison for the flat discs is taken against.
    pub slenderness: bool,
    /// Whether remedy 1 -- re-choosing a descent onto a reachable shaft -- may
    /// fire. **On in production.** The drill turns it off and watches the same
    /// arms be tied by `|X|` braces instead.
    pub slenderness_merge: bool,
    /// Whether remedy 2 -- the `|X|` cross-brace -- may fire. **On in
    /// production.** The drill turns it off and watches masts appear.
    pub slenderness_brace: bool,
    /// Whether remedy 3 -- dropping a new plate-rooted mast -- may fire.
    /// **On in production.** The drill turns it off and watches the runs come
    /// out as warnings, with NEVER fewer arms than before.
    pub slenderness_mast: bool,
    /// Whether a mast's plate cell must CLEAR the flared root discs already on
    /// the plate. **On in production.** The drill turns it off, which restores
    /// the pre-2026-09-06 per-cell occupancy test, and watches overlapping root
    /// pairs reappear on the finished field.
    pub slenderness_mast_root_clearance: bool,
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
            overhang_centre_deg: placeholders.overhang_angle_deg().diagnostic().get(),
            overhang_transition_deg: placeholders
                .overhang_weight_transition_deg()
                .diagnostic()
                .get(),
            lean: true,
            creep_weight: true,
            shaft_landings: true,
            drain_keep_out: true,
            removal_span_check: true,
            tip_column_roots: true,
            slenderness_ratio: placeholders.slenderness_limit_ratio().diagnostic().get(),
            flare_r_factor: placeholders.flare_r_factor().diagnostic().get(),
            congestion: true,
            congestion_outward_lean: true,
            congestion_flank_arms: true,
            slenderness: true,
            slenderness_merge: true,
            slenderness_brace: true,
            slenderness_mast: true,
            slenderness_mast_root_clearance: true,
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

    /// Descend straight down from every seat, the way this engine did before
    /// the owner's 2026-09-03 note ("all of the supports only descend directly
    /// vertically -- none angle down to merge trunks"). **Sabotage only**: it
    /// is what the merge and gallery tests delete to prove the lean is what
    /// merges the trunks and what keeps a drain gallery's arms alive, and
    /// nothing in the pipeline sets it.
    #[must_use]
    pub const fn without_lean(mut self) -> Self {
        self.lean = false;
        self
    }

    /// Land only on an arm's TOP, never part-way down its shaft -- the rule
    /// this engine had between 2026-09-04 and the owner's 2026-09-05 ruling
    /// that "the base needs to land ANYWHERE on the support arm". **Sabotage
    /// only**: it is what the mid-shaft junction test deletes to show that the
    /// same arm then falls to the plate, and nothing in the pipeline sets it.
    #[must_use]
    pub const fn without_shaft_landings(mut self) -> Self {
        self.shaft_landings = false;
        self
    }

    /// Decide removability from the arm's SEAT layer alone -- the rule this
    /// engine had until 2026-09-06, and the one that let five Morel-base arms
    /// go with 23.22 mm2 of load left uncarried under them. **Sabotage
    /// only**; nothing in the pipeline sets it.
    #[must_use]
    pub const fn with_seat_only_removal(mut self) -> Self {
        self.removal_span_check = false;
        self
    }

    /// Let an arm root only the column it stands in AT this layer, never the
    /// column its own axis occupies one layer up -- the accounting this engine
    /// had until 2026-09-06, and the one that left 23.22 mm2 of the Morel
    /// base's layer-108 load with no carrier at all. **Sabotage only**;
    /// nothing in the pipeline sets it.
    #[must_use]
    pub const fn without_tip_column_roots(mut self) -> Self {
        self.tip_column_roots = false;
        self
    }

    /// Measure no unbraced run and apply no remedy -- the engine as it stood
    /// before the owner's 2026-09-05 ruling that long thin verticals "with no
    /// lateral support will be very unlikely to stay straight". **Sabotage and
    /// comparison only**; nothing in the pipeline sets it.
    #[must_use]
    pub const fn without_slenderness(mut self) -> Self {
        self.slenderness = false;
        self
    }

    /// Price every creep step at a full lamina whatever the wall's angle -- the
    /// rule this engine had before the 2026-09-28 field report. **Sabotage and
    /// comparison only**: with it set, Round 50's drafted rim must draw
    /// minimum-tip arms again (`tests/it/wall_creep.rs`).
    #[must_use]
    pub const fn without_creep_weight(mut self) -> Self {
        self.creep_weight = false;
        self
    }

    /// Refuse remedy 1. **Sabotage only**: the drill sets it and the same arms
    /// must come out braced instead of merged.
    #[must_use]
    pub const fn without_slenderness_merge(mut self) -> Self {
        self.slenderness_merge = false;
        self
    }

    /// Refuse remedies 1 and 2. **Sabotage only**: the drill sets it and the
    /// same arms must come out with masts dropped beside them.
    #[must_use]
    pub const fn without_slenderness_brace(mut self) -> Self {
        self.slenderness_merge = false;
        self.slenderness_brace = false;
        self
    }

    /// Refuse every remedy but the warning. **Sabotage only**: the drill sets
    /// it and the runs must come out as named warnings on an emitted file,
    /// with NEVER fewer arms than the field had before.
    #[must_use]
    pub const fn without_slenderness_mast(mut self) -> Self {
        self.slenderness_merge = false;
        self.slenderness_brace = false;
        self.slenderness_mast = false;
        self
    }

    /// Let a mast stand on a cell that is merely EMPTY instead of one that is
    /// CLEAR — the field this engine placed before 2026-09-06, where a mast's
    /// root flare was free to lap over the root beside it. **Sabotage only**:
    /// the drill sets it and overlapping root pairs must reappear on the
    /// finished field.
    #[must_use]
    pub const fn without_mast_root_clearance(mut self) -> Self {
        self.slenderness_mast_root_clearance = false;
        self
    }

    /// Measure the plate's congestion but change nothing — the field this
    /// engine placed before 2026-09-06. **Sabotage and comparison only**: it is
    /// what the "the lane is a no-op on a disc" byte-identity check is taken
    /// against, and nothing in the pipeline sets it.
    #[must_use]
    pub const fn without_congestion(mut self) -> Self {
        self.congestion = false;
        self
    }

    /// Refuse remedy 1, the outward lean. **Sabotage only**: the drill sets it
    /// and the footprint must stay congested, or the flank arms must be forced
    /// to carry the relief instead.
    #[must_use]
    pub const fn without_congestion_outward_lean(mut self) -> Self {
        self.congestion_outward_lean = false;
        self
    }

    /// Refuse remedy 2, the flank arms. **Sabotage only**: the drill sets it
    /// and a still-congested footprint must come out as a WARNING on an
    /// emitted file, with NEVER fewer arms than the field had before.
    #[must_use]
    pub const fn without_congestion_flank_arms(mut self) -> Self {
        self.congestion_flank_arms = false;
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
    let mut arms_leaned = 0_usize;
    let mut arms_merged = 0_usize;
    let mut lean_paths_blocked = 0_usize;
    let mut junctions_mid_shaft_descent = 0_usize;
    let mut lean_candidates_priced = 0_u64;
    // The angle law, evaluated once for this plan's layer height and then read
    // by every descent. Disabled by the sabotage knob, which reduces the
    // descent to the straight-down descent this engine had before.
    let law = if policy.lean {
        lean::LeanLaw::new(height, policy.overhang_centre_deg, policy.overhang_transition_deg)
    } else {
        lean::LeanLaw::disabled(height)
    };
    let mut rasteriser_entries = 0_u64;
    // The mesh's own surface, banded layer by layer, for the overhang weight a
    // wall's creep is priced at. Disabled by the sabotage knob, which prices
    // every creep at a full lamina as before.
    let mut creep = if policy.creep_weight {
        reach::SurfaceCreep::new(
            mesh,
            &grid,
            policy.overhang_centre_deg,
            policy.overhang_transition_deg,
        )
    } else {
        reach::SurfaceCreep::disabled(&grid)
    };

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
        // The band this layer's material is born in: from the slicing plane
        // below to its own.
        creep.load_layer(&grid, planes[layer] - height, planes[layer]);
        reach::recompute_potential(
            &grid,
            &masks.cover,
            policy.kappa,
            height,
            &[],
            &creep,
            &mut reach,
        );
        let outcome = up_sweep::place_layer(
            &grid,
            layer,
            &masks.cover,
            &keep_out,
            &law,
            policy.shaft_landings,
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
        arms_leaned += outcome.arms_leaned;
        arms_merged += outcome.arms_merged;
        lean_paths_blocked += outcome.lean_paths_blocked;
        junctions_mid_shaft_descent += outcome.junctions_mid_shaft;
        lean_candidates_priced += outcome.lean_candidates_priced;
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
    set_interior(&mut struts, &history);

    // -------------------------------------------------------------- downward
    let sizing = down_sweep::SizingPolicy {
        x_allowed: policy.x_allowed,
        tip_radius_min_mm: policy.tip_radius_min_mm,
        tip_radius_max_mm: policy.tip_radius_max_mm,
        owner_growth_ceiling_mm: policy.owner_growth_ceiling_mm,
        alpha: peel::alpha(policy.peel_field_mode),
        lamina_reach_mm,
        tip_column_roots: policy.tip_column_roots,
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
    let mut removals_refused_by_span = 0_u64;
    let mut removal_passes_rolled_back = 0_u64;
    for _ in 0..policy.strut_removal_passes {
        // THE PASS IS UNDONE AS A WHOLE IF IT MAKES THE FIELD WORSE. The
        // span test below is a coverage argument over arms alone; the
        // down-sweep is the only thing that actually knows what the load did.
        // Keeping the previous field beside the new one costs one clone per
        // pass and turns "never if its removal raises `uncarried_load`" from
        // an intention into a property the run can be graded on.
        let before_struts = struts.clone();
        let before_outcome = outcome.clone();
        let removed = remove_under_loaded(
            &grid,
            &mut struts,
            minimum_load,
            lamina_reach_mm,
            layers,
            policy.removal_span_check,
            &mut removals_refused_by_span,
        );
        if removed == 0 {
            break;
        }
        outcome = down_sweep::run(&grid, &slices, &mut struts, sizing, insertion, &keep_out);
        let worse = outcome.uncarried_load_mm2
            > before_outcome.uncarried_load_mm2
                + 1.0e-9 * before_outcome.uncarried_load_mm2.abs().max(1.0);
        if worse {
            struts = before_struts;
            outcome = before_outcome;
            removal_passes_rolled_back += 1;
            break;
        }
        removals += removed as u64;
    }
    // The interior test again, now that the down-sweep has added arms of its
    // own. Same test, same column history; running it twice is cheaper than
    // giving the down-sweep a reason to know about the up-sweep's history.
    set_interior(&mut struts, &history);

    // --- merges the trunk cannot carry are undone -------------------------
    //
    // A merge hands the parent the SUM of both loads, and the area law sizes
    // the parent from that sum. Where the sum needs more section than the
    // registered maximum tip radius allows, the merge is not admissible: the
    // owner's instruction is "if the merged radius would exceed the 1.0 mm
    // maximum, do not merge". The check is here rather than in the descent
    // because the descent runs before any load is routed and cannot know the
    // sum. Un-merging is MONOTONE -- an arm put back straight has an empty
    // path and the descent never runs again -- so this loop cannot oscillate,
    // and it is bounded by the same registered pass count the removal loop
    // uses.
    let mut merges_refused_by_radius = 0_usize;
    for _ in 0..policy.strut_removal_passes {
        let undone = unmerge_overgrown(&mut struts, policy.tip_radius_max_mm);
        if undone == 0 {
            break;
        }
        merges_refused_by_radius += undone;
        outcome = down_sweep::run(&grid, &slices, &mut struts, sizing, insertion, &keep_out);
    }

    // AND ONCE MORE, ON THE FINISHED FIELD. `unmerge_overgrown` moves an arm
    // off its trunk and onto whatever the descent recorded as its straight
    // fallback, so it CHANGES LANDINGS — and until 2026-09-06 the interior
    // census was never recomputed after it, leaving every un-merged arm
    // flagged from the landing it no longer has. On the cavity oracle that
    // was 12 of 49 model landings reading `interior = false` while sitting on
    // the cavity floor. Merges were rare before shaft landings, so the hole
    // was there and never bit.
    set_interior(&mut struts, &history);

    // --- congestion: spread the footprint, then shed load to the flanks ----
    //
    // BEFORE the slenderness lane, deliberately. This lane MOVES a root
    // sideways and INSERTS long leaning arms, both of which change what is
    // slender and what is tied; running it afterwards would leave every member
    // it touched un-measured against `S` until the next run of the engine.
    // Slenderness is the last word on the geometry, so it goes last.
    //
    // Bounded by the SAME round count the down-sweep's load insertion is
    // bounded by (`InsertionPolicy::max_rounds`), with the same down-sweep
    // re-run after every round, because a moved root and an inserted arm are
    // both un-sized until it runs.
    let congestion_context = congestion::CongestionContext {
        grid: &grid,
        history: &history,
        keep_out: &keep_out,
        lean: &law,
        layer_height_mm: height,
        layers,
        flare_factor: policy.flare_r_factor,
        tip_radius_min_mm: policy.tip_radius_min_mm,
        hex_separation_mm: up_sweep::hex_separation_mm(lamina_reach_mm),
        x_allowed: policy.x_allowed,
        outward_lean: policy.congestion_outward_lean,
        flank_arms: policy.congestion_flank_arms,
        max_struts: policy.limits.max_struts,
    };
    let mut congestion_outcome = congestion::CongestionOutcome {
        enabled: policy.congestion,
        before: congestion::measure(&grid, &history, &struts, policy.flare_r_factor),
        ..congestion::CongestionOutcome::default()
    };
    if policy.congestion {
        for _ in 0..congestion::rounds_allowed(insertion) {
            let census = congestion::measure(&grid, &history, &struts, policy.flare_r_factor);
            if !census.all.congested() {
                break;
            }
            let moved =
                congestion::apply_remedies(&congestion_context, &mut struts, &mut congestion_outcome);
            if moved == 0 {
                break;
            }
            congestion_outcome.rounds_used += 1;
            outcome = down_sweep::run(&grid, &slices, &mut struts, sizing, insertion, &keep_out);
            set_interior(&mut struts, &history);
            // WHAT THE PRE-SIZING ESTIMATE GOT WRONG, measured rather than
            // assumed. A flank arm's plate cell is chosen against an ESTIMATE
            // of its root radius, because the arm does not exist yet when the
            // cell is picked; the down-sweep that just ran is the first pass
            // that knows the real one. Any inserted root whose flared disc
            // still overlaps a neighbour is an ordinary stacked plate root
            // (`StrutKind::Arm`, `Landing::Plate`, `k_bottom == 0`), so the
            // NEXT round's `stacked_ids` names it and remedy 1 leans it out —
            // no separate path, and this counter is what says whether any were
            // left when the rounds ran out.
            congestion_outcome.flank_roots_overlapping_after_sizing =
                congestion::roots_overlapping_after_sizing(
                    &grid,
                    &history,
                    &struts,
                    policy.flare_r_factor,
                    &congestion_outcome.inserted_ids,
                );
        }
    }
    // What THIS lane left, before slenderness has its say. Published beside the
    // finished-field census so the two are never confused: masts are
    // plate-rooted too, and the difference between these two numbers is what
    // the slenderness lane put on the plate afterwards.
    congestion_outcome.at_lane_exit =
        congestion::measure(&grid, &history, &struts, policy.flare_r_factor);

    // --- slenderness: merge, then |X|, then a mast, then a warning ---------
    //
    // Here, and not inside `down_sweep::run`, for the reason `slenderness.rs`
    // states at length: the constraint needs SIZED radii and whole-arm ties,
    // and remedy 1 re-chooses a descent against the up-sweep's own column
    // history. This is the iterate that already re-runs the down-sweep after
    // it moves arms, so a brace is sized and its junction splits its shaft's
    // ledger by the same pass that sizes everything else.
    let slenderness_law = if policy.slenderness {
        slenderness::SlendernessLaw::new(
            policy.slenderness_ratio,
            policy.tip_radius_min_mm,
            policy.tip_radius_max_mm,
            policy.overhang_centre_deg - 0.5 * policy.overhang_transition_deg,
        )
    } else {
        slenderness::SlendernessLaw::disabled()
    };
    let slenderness_context = slenderness::SlendernessContext {
        grid: &grid,
        history: &history,
        keep_out: &keep_out,
        lean: &law,
        law: &slenderness_law,
        shaft_landings: policy.shaft_landings,
        layer_height_mm: height,
        layers,
        merge: policy.slenderness_merge,
        brace: policy.slenderness_brace,
        mast: policy.slenderness_mast,
        flare_factor: policy.flare_r_factor,
        mast_root_clearance: policy.slenderness_mast_root_clearance,
        max_struts: policy.limits.max_struts,
    };
    let mut slenderness_merges_undone = 0_usize;
    let struts_before_slenderness = struts.len();
    let slenderness_outcome = slenderness::apply_remedies(&slenderness_context, &mut struts);
    if slenderness_outcome.merges_forced > 0 || struts.len() > struts_before_slenderness {
        // Anything the lane moved or added has a stale ledger; the same
        // down-sweep that sized the field sizes the braces and the masts, and
        // splits both shafts at every junction the crosses made.
        outcome = down_sweep::run(&grid, &slices, &mut struts, sizing, insertion, &keep_out);
        // AND THE SAME RADIUS CEILING THE OWNER RULED ON, APPLIED TO THE
        // MERGES THIS LANE FORCED. `unmerge_overgrown` ran before the lane, so
        // it never saw them; a forced merge hands its trunk the SUM of both
        // loads, and where the area law cannot grow the trunk far enough the
        // merge is not admissible however slender the member was. Measured on
        // `make_overhang_table`, 2026-09-06: nine forced merges drove 1828
        // overloaded arm-layers, against zero before the lane. The arm goes
        // back to the descent it recorded, and its run is then reported as a
        // warning — emit and warn, never refuse.
        for _ in 0..policy.strut_removal_passes {
            let undone = unmerge_overgrown(&mut struts, policy.tip_radius_max_mm);
            if undone == 0 {
                break;
            }
            merges_refused_by_radius += undone;
            slenderness_merges_undone += undone;
            outcome = down_sweep::run(&grid, &slices, &mut struts, sizing, insertion, &keep_out);
        }
        set_interior(&mut struts, &history);
    }
    // Measured on the FINISHED field, after the re-run, so the published run
    // lengths belong to the arms this file actually contains.
    let slenderness_after =
        slenderness::unbraced_runs(&grid, &struts, height, &slenderness_law);
    let mut slenderness_outcome = slenderness_outcome;
    slenderness_outcome.longest_run_mm = 0.0;
    slenderness_outcome.longest_run_limit_mm = 0.0;
    slenderness_outcome.merges_undone_by_radius = slenderness_merges_undone;
    // WHAT THE MAST'S PLACEMENT-TIME RADIUS GOT WRONG, measured on the sized
    // field by the same function that measures the flank arms' equivalent. A
    // mast's cell is chosen against `r_m = max(r_min, H_first/S)`; the
    // down-sweep then sizes it from what the cross ties into it, and two roots
    // that cleared at placement can grow into one another. Published, never
    // asserted away.
    let mast_ids: Vec<u32> = struts
        .iter()
        .filter(|strut| strut.kind == StrutKind::Mast)
        .map(|strut| strut.id)
        .collect();
    slenderness_outcome.mast_roots_overlapping_after_sizing =
        congestion::roots_overlapping_after_sizing(
            &grid,
            &history,
            &struts,
            policy.flare_r_factor,
            &mast_ids,
        );
    slenderness_outcome.warnings.clear();
    for run in &slenderness_after {
        if run.length_mm > slenderness_outcome.longest_run_mm {
            slenderness_outcome.longest_run_mm = run.length_mm;
            slenderness_outcome.longest_run_limit_mm = run.limit_mm;
        }
        if run.over_limit() {
            slenderness_outcome
                .warnings
                .push(slenderness::UnbracedWarning {
                    strut_id: run.strut_id,
                    k_lo: run.k_lo,
                    k_top: run.k_top,
                    length_mm: run.length_mm,
                    limit_mm: run.limit_mm,
                });
        }
    }

    // The congestion census AFTER, from the same function that measured it
    // before, read off the FINISHED field — so what is published describes the
    // footprint this file actually lays on the plate rather than the one the
    // lane left mid-iterate. The load the flank arms moved is read the same
    // way: their own carried ledger where they meet the plate.
    congestion_outcome.after = congestion::measure(&grid, &history, &struts, policy.flare_r_factor);
    congestion_outcome.still_congested = congestion_outcome.after.all.congested();
    congestion_outcome.flank_load_still_at_plate_mm2 = congestion_outcome
        .inserted_ids
        .iter()
        .filter_map(|id| struts.get(*id as usize))
        .filter(|strut| matches!(strut.landing, Landing::Plate))
        .filter_map(|strut| strut.carried_mm2.first().copied())
        .sum();
    // AND THE ONES THIS SUM CANNOT SEE. A later pass — a forced merge, or the
    // down-sweep's own descent — can re-land an inserted arm on a trunk, and
    // such an arm no longer meets the plate at all. It is not lost load; it is
    // load that reaches the plate down somebody else's shaft. Publishing the
    // count is what stops the sum above from reading as "less load moved".
    congestion_outcome.flank_arms_moved_off_plate = congestion_outcome
        .inserted_ids
        .iter()
        .filter_map(|id| struts.get(*id as usize))
        .filter(|strut| !matches!(strut.landing, Landing::Plate))
        .count();

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
        removals_refused_by_span,
        removal_passes_rolled_back,
    );
    report.drain_keep_out_cells = keep_out.blocked_cells();
    report.drain_keep_out_refusals = keep_out.refusals();
    report.drain_seat_refusals = keep_out_refusals;
    report.escape = escape;
    report.arms_leaned = arms_leaned;
    report.arms_merged = arms_merged;
    report.lean_paths_blocked = lean_paths_blocked;
    report.merges_refused_by_radius = merges_refused_by_radius;
    report.lean_max_step_mm = law.max_step_mm();
    report.descent_junctions_mid_shaft = junctions_mid_shaft_descent;
    // SHAFT SPANS AS THE FINISHED FIELD HAS THEM, not as the up-sweep left
    // them. `history` is the UP-SWEEP's column index and nothing has written to
    // it since: the descent, the congestion lane and the slenderness lane each
    // build their own index over the field they are working on, so this counter
    // has been reporting a pre-descent number ever since those lanes landed.
    // Rebuilt here from the strut set that is actually emitted.
    report.shaft_spans_recorded = {
        let mut final_index = up_sweep::ColumnHistory::new(&grid);
        congestion::stamp_all(&mut final_index, &struts);
        final_index.shaft_spans.len()
    };
    report.slenderness_lane = slenderness_outcome;
    report.congestion_lane = congestion_outcome;
    report.counters.lean_candidates_priced = lean_candidates_priced;

    // ------------------------------------------------ what each tip seats INTO
    //
    // Both of these are properties of the MODEL at an arm's finished seat, so
    // they are measured once here, after insertion, removal and un-merging
    // have settled the strut set, and never during a pass that is still moving
    // arms around. Neither moves an arm; both are read by the lowering when it
    // drives the tip ball into the model.
    let solid_layers = seat_solid_layers(&grid, &slices, &struts, layers);
    let seat_normals =
        tip_landing::tip_seat_normals(mesh, &grid, &struts, plate_z, height);
    for (index, strut) in struts.iter_mut().enumerate() {
        strut.seat_solid_layers = solid_layers.get(index).copied().unwrap_or(0);
        strut.seat_normal = seat_normals.get(index).copied().unwrap_or([0.0; 3]);
    }

    Ok(SweepPlan {
        grid,
        layers,
        layer_height_mm: height,
        plate_z_mm: plate_z,
        struts,
        report,
    })
}

/// How many cured laminae stand at and above each arm's own `k_top`, in its
/// own tip column — the thickness a tip has to seat into, in layers.
///
/// **Why it is measured and not assumed.** The seat plane is the BOTTOM of
/// lamina `k_top`, so an arm under a 0.15 mm skin has three laminae of model
/// above it at a 0.05 mm layer height and nothing more. Driving the tip ball
/// deeper than that would push its pole out through the far face, which the
/// land-at-strike ruling forbids at either end. The lowering truncates to this
/// number and counts the truncation; it never retracts and never drops the arm.
///
/// **Batching.** ONE rasterisation pass over the layer stack, in ascending
/// order, with every arm still counting advanced together — no per-arm slice
/// query and no second stack. An arm is opened at its own `k_top`, counts
/// while its tip column stays covered, and closes at the first gap, so the
/// whole pass is `O(layers · raster + Σ run)`.
///
/// **Which mask, and which way the error runs.** It counts `cover`, the
/// any-overlap mask, because `cover` is what decided this arm's `k_top` in the
/// first place and a bound measured against a different mask than the seat
/// would be answering about a different column. `cover` rounds OUTWARD, so at
/// the very top of a run it can credit one lamina that is only partly there —
/// an over-statement bounded by one layer height (0.05 mm at the registered
/// default). `carry` would round the other way, but it is eroded by one cell in
/// Y and would read ZERO for every rim column, which is the defect this whole
/// change exists to end, returned at the rim.
fn seat_solid_layers(
    grid: &Grid,
    slices: &[Vec<plinth_support::perception::SliceSolid>],
    struts: &[SweepStrut],
    layers: usize,
) -> Vec<u32> {
    let mut runs = vec![0_u32; struts.len()];
    if struts.is_empty() {
        return runs;
    }
    // Arms bucketed by the layer they open at, so the pass opens them in one
    // indexed lookup rather than scanning the strut vector per layer.
    let mut opening_at: Vec<Vec<usize>> = vec![Vec::new(); layers];
    for (index, strut) in struts.iter().enumerate() {
        if let Some(bucket) = opening_at.get_mut(strut.k_top) {
            bucket.push(index);
        }
    }
    let mut scratch = RasterScratch::new(grid);
    let mut masks = LayerMasks::default();
    let mut open: Vec<usize> = Vec::new();
    let mut still_open: Vec<usize> = Vec::new();
    for (layer, solids) in slices.iter().enumerate().take(layers) {
        if let Some(bucket) = opening_at.get(layer) {
            open.extend_from_slice(bucket);
        }
        if open.is_empty() {
            continue;
        }
        rasterize_into(grid, solids, &mut scratch, &mut masks);
        still_open.clear();
        for &index in &open {
            let strut = &struts[index];
            let cell = strut.cell_at(strut.k_top);
            if masks.cover.get(cell).copied().unwrap_or(false) {
                runs[index] += 1;
                still_open.push(index);
            }
        }
        std::mem::swap(&mut open, &mut still_open);
    }
    runs
}

/// Put back straight any arm that merged into a trunk the area law cannot grow
/// far enough to carry the sum.
///
/// The test is the registered maximum tip radius itself, read at the layer the
/// child lands on: the parent's radius there is `size_arm`'s clamped value, so
/// a parent sitting AT the maximum is one the law wanted to make bigger and
/// could not. Only arms the UP-SWEEP leaned are put back, because only they
/// have a straight-down descent in the column history to be put back to; a
/// load-inserted arm's descent is grown by the down-sweep and is not in this
/// history at all.
fn unmerge_overgrown(struts: &mut [SweepStrut], tip_radius_max_mm: f64) -> usize {
    let overgrown: Vec<(usize, Landing, usize)> = struts
        .iter()
        .enumerate()
        .filter_map(|(index, strut)| {
            if strut.inserted_by_load || strut.path.is_empty() {
                return None;
            }
            let Landing::Strut { id, layer } = strut.landing else {
                return None;
            };
            let parent = struts.get(id as usize)?;
            if parent.k_bottom == usize::MAX || layer < parent.k_bottom {
                return None;
            }
            let radius = *parent.radius_mm.get(layer - parent.k_bottom)?;
            let (landing, k_bottom) = strut.straight_fallback?;
            // A fallback that is not strictly below the seat would be a
            // zero-height stub, which is the defect this field exists to
            // prevent; refuse the un-merge rather than emit one.
            (radius >= tip_radius_max_mm && k_bottom < strut.k_top)
                .then_some((index, landing, k_bottom))
        })
        .collect();
    for &(index, landing, k_bottom) in &overgrown {
        struts[index].landing = landing;
        struts[index].k_bottom = k_bottom;
        struts[index].path = std::mem::take(&mut struts[index].fallback_path);
        // `carried_mm2` and `radius_mm` are now STALE: they describe a member
        // that stopped at the trunk. The caller's loop (lib.rs:616) re-runs
        // `down_sweep::run` after every non-zero return, which clears and
        // refills both from `ledger_len`, so the invariant is "an un-merge is
        // never observed from outside that loop".
    }
    overgrown.len()
}

/// Flag the arms whose scar nobody will ever see.
///
/// **The question is asked at the LANDING, not at the seat**, and that is a
/// correction of 2026-09-06 rather than a rephrasing. A scar is worn where an
/// arm's base touches the model, so "is it hidden?" is "does model stand above
/// the point where this arm stopped?" — `column_max_model` at the arm's own
/// LANDING column, against the layer it landed on. The seat form this replaces
/// asked whether model stood above the arm's SEAT, which is true of every arm
/// under every overhang by construction (that is what makes a face
/// down-facing), so it answered "interior" for exterior scars and, once the
/// descent could lean, answered about a column the arm's base is not even in.
fn set_interior(struts: &mut [SweepStrut], history: &up_sweep::ColumnHistory) {
    for strut in struts.iter_mut() {
        strut.interior = match strut.landing {
            Landing::Model { layer } => {
                usize::try_from(history.column_max_model[strut.base_cell()]).unwrap_or(0)
                    > layer + 1
            }
            Landing::Plate | Landing::Strut { .. } => false,
        };
    }
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
    layers: usize,
    span_check: bool,
    refused_by_span: &mut u64,
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
    // An arm ANOTHER arm lands on is never a removal candidate. Before the
    // descent could lean, a `Landing::Strut` was rare and the orphan fallback
    // below quietly re-pointed the child at the plate while leaving its
    // `k_bottom` at the vanished parent's top -- an arm claiming a plate
    // landing while stopping in mid-air. Merging makes that landing common, so
    // the parent is protected instead of the child being falsified.
    let mut is_a_parent = vec![false; struts.len()];
    for strut in struts.iter() {
        if let Landing::Strut { id, .. } = strut.landing {
            if let Some(flag) = is_a_parent.get_mut(id as usize) {
                *flag = true;
            }
        }
    }
    for &candidate in &candidates {
        if is_a_parent[candidate] {
            continue;
        }
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
    if span_check {
        *refused_by_span += refuse_uncovered_spans(grid, struts, &mut doomed, reach_mm, layers);
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
        // The recorded fallback names a strut id too, and a stale id there
        // would put an un-merge onto a vanished parent.
        if let Some((Landing::Strut { id, layer }, k_bottom)) = strut.straight_fallback {
            let mapped = remap[id as usize];
            strut.straight_fallback = if mapped == u32::MAX {
                None
            } else {
                Some((Landing::Strut { id: mapped, layer }, k_bottom))
            };
        }
    }
    *struts = kept;
    removed
}

/// Un-doom every candidate whose removal would leave a layer of its own
/// CARRIED SPAN with no carrier in reach, and say how many it saved.
///
/// # The defect this closes
///
/// `remove_under_loaded` decided removability from the arm's SEAT layer alone:
/// "are this arm's demand cells still within reach of another arm seated at
/// the same layer?". An arm is a carrier on EVERY layer it passes through, not
/// only the one it seats on, and the down-sweep routes load into it at each of
/// them. Once the descent could lean, load re-split, five more Morel-base arms
/// fell under the minimum-load line, and deleting them opened a gap on layers
/// BELOW their seats that the seat test cannot see: `uncarried_load` went
/// 0.00 → 23.22 mm² with the seat test reporting every removal as safe.
///
/// # What is asked instead
///
/// For every layer of a candidate's span, some SURVIVING arm must stand within
/// one lamina's reach of the candidate's own axis at that layer, in the same
/// 8-neighbour chamfer metric the down-sweep routes in
/// (`down_sweep::route_within_cover`). If any layer has none, the candidate is
/// kept.
///
/// # Batch-first, and why the interaction between candidates is not a hole
///
/// The survivor occupancy is built ONCE per removal pass, over the set that
/// would remain if EVERY tentatively doomed arm went at once, and all
/// candidates are tested against that one index. So the joint counterfactual
/// — "removing A and B together uncovers what removing either alone would
/// not" — is the case actually evaluated, not a per-arm one that misses it.
/// Un-dooming afterwards only ADDS carriers back, so every coverage claim
/// proved against the smaller survivor set still holds against the larger one:
/// the pass is conservative in the safe direction (it can keep an arm that
/// would in fact have been fine) and can never delete one it proved necessary.
/// No re-pass is needed for soundness, and the caller's registered pass count
/// still bounds the loop.
///
/// # What it deliberately does NOT count as a carrier
///
/// Model material. A candidate running down beside a wall may well have its
/// load taken by that wall, and this test will keep it anyway. That is the
/// over-support direction, it costs at most a hairline arm, and the alternative
/// is re-rasterising the whole slice stack once per removal pass to answer a
/// question that the down-sweep's own `uncarried_load` guard (in `run_sweep`)
/// already answers exactly.
fn refuse_uncovered_spans(
    grid: &Grid,
    struts: &[SweepStrut],
    doomed: &mut [bool],
    reach_mm: f64,
    layers: usize,
) -> u64 {
    if layers == 0 || !doomed.iter().any(|flag| *flag) {
        return 0;
    }
    // One index for the whole pass: the cells every SURVIVING arm occupies, by
    // layer. Built from the set that would remain if every doomed arm went.
    let mut survivors_by_layer: Vec<Vec<u32>> = vec![Vec::new(); layers];
    for (index, strut) in struts.iter().enumerate() {
        if doomed[index] || strut.k_bottom == usize::MAX {
            continue;
        }
        let top = strut.k_top.min(layers - 1);
        for (layer, here) in survivors_by_layer
            .iter_mut()
            .enumerate()
            .take(top + 1)
            .skip(strut.k_bottom)
        {
            here.push(u32::try_from(strut.cell_at(layer)).unwrap_or(u32::MAX));
        }
    }
    let mut refused = 0_u64;
    for index in 0..struts.len() {
        if !doomed[index] {
            continue;
        }
        let strut = &struts[index];
        if strut.k_bottom == usize::MAX {
            continue;
        }
        let top = strut.k_top.min(layers - 1);
        let covered_everywhere = (strut.k_bottom..=top).all(|layer| {
            let here = strut.cell_at(layer);
            survivors_by_layer[layer]
                .iter()
                .any(|cell| chamfer_distance_mm(grid, here, *cell as usize) <= reach_mm)
        });
        if !covered_everywhere {
            doomed[index] = false;
            refused += 1;
        }
    }
    refused
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

pub(crate) fn cell_distance_mm(grid: &Grid, a: usize, b: usize) -> f64 {
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
    removals_refused_by_span: u64,
    removal_passes_rolled_back: u64,
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
    // THE SCAR IS WHAT THE MODEL WEARS, so a member that never touches the
    // model contributes none. A brace seats on an arm and a mast stands in
    // free space; neither is given a tip ball at emit, and counting either
    // here would report a scar on a surface nothing touched.
    let total_scar: f64 = struts
        .iter()
        .filter(|strut| strut.kind == crate::StrutKind::Arm)
        .map(|strut| {
            let radius = strut.tip_radius_mm();
            core::f64::consts::PI * radius * radius
        })
        .sum();
    // The three quantities the 2026-09-05 shaft-landing lane is graded on, all
    // read off the FINISHED field in one pass rather than accumulated during
    // placement, so un-merging and removal cannot leave them describing a field
    // nobody prints.
    let mut support_volume_mm3 = 0.0;
    let mut plate_contact_area_mm2 = 0.0;
    let mut junctions_at_parent_top = 0_usize;
    let mut junctions_mid_shaft = 0_usize;
    for strut in struts {
        support_volume_mm3 += strut.volume_mm3(grid, height);
        // THE JUNCTION CENSUS IS ABOUT THE DESCENT. An `|X|` diagonal lands on
        // a shaft BY CONSTRUCTION -- that is what a brace IS -- so counting one
        // here would report the slenderness lane as a descent decision and
        // would survive the descent's own sabotage knob. The braces are
        // published in their own right on `slenderness_lane`.
        let censused = strut.kind == crate::StrutKind::Arm;
        match strut.landing {
            Landing::Plate => plate_contact_area_mm2 += strut.root_contact_area_mm2(),
            Landing::Strut { id, layer } if censused => {
                let at_top = struts
                    .get(id as usize)
                    .is_some_and(|parent| layer >= parent.k_top);
                if at_top {
                    junctions_at_parent_top += 1;
                } else {
                    junctions_mid_shaft += 1;
                }
            }
            Landing::Model { .. } | Landing::Strut { .. } => {}
        }
    }

    SweepRunReport {
        // Filled by the caller, which is the only place that knows the drains:
        // this function is handed the sweep's own measurements and nothing else.
        drain_keep_out_cells: 0,
        drain_keep_out_refusals: 0,
        drain_seat_refusals: 0,
        escape: crate::drain::EscapeReport::default(),
        arms_leaned: 0,
        arms_merged: 0,
        lean_paths_blocked: 0,
        merges_refused_by_radius: 0,
        lean_max_step_mm: 0.0,
        descent_junctions_mid_shaft: 0,
        shaft_spans_recorded: 0,
        slenderness_lane: crate::slenderness::SlendernessOutcome::default(),
        congestion_lane: crate::congestion::CongestionOutcome::default(),
        junctions_at_parent_top,
        junctions_mid_shaft,
        support_volume_mm3,
        plate_contact_area_mm2,
        tip_column_roots_offered: outcome.tip_column_roots_offered,
        tip_column_root_load_mm2: outcome.tip_column_root_load_mm2,
        misrouted_load_mm2: outcome.misrouted_load_mm2,
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
        // ARMS, not members: a brace and a mast carry the same protective flag
        // so the removal pass leaves them alone, and counting them here would
        // report the slenderness lane as load-driven insertion.
        arms_inserted_by_load: struts
            .iter()
            .filter(|strut| strut.inserted_by_load && strut.kind == crate::StrutKind::Arm)
            .count(),
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
            creep_cells_weighted: reach.creep_cells_weighted,
            creep_steps_taken: reach.creep_steps_taken,
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
            strut_removals_refused_by_span: removals_refused_by_span,
            removal_passes_rolled_back,
            lean_candidates_priced: 0,
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
        // The leaned path, so a descent that moved sideways can never share a
        // digest with the straight descent it replaced. A straight arm's path
        // is empty and absorbs nothing, which is what keeps every unleaned
        // field's digest exactly what it was.
        for cell in &strut.path {
            absorb(u64::from(*cell));
        }
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
    use super::{plan_grid, remove_under_loaded, unmerge_overgrown, Landing, SweepStrut};

    fn arm(id: u32, cell: usize, k_top: usize, k_bottom: usize) -> SweepStrut {
        SweepStrut {
            id,
            cell,
            center_mm: [0.0, 0.0],
            kind: crate::StrutKind::Arm,
            min_radius_mm: 0.0,
            seat_on_strut: None,
            k_top,
            k_bottom,
            landing: Landing::Plate,
            interior: false,
            radius_mm: Vec::new(),
            carried_mm2: Vec::new(),
            covered_demand: Vec::new(),
            inserted_by_load: false,
            path: Vec::new(),
            seat_solid_layers: 0,
            seat_normal: [0.0; 3],
            straight_fallback: None,
            fallback_path: Vec::new(),
        }
    }

    /// DEFECT 2, 2026-09-04. `unmerge_overgrown` used to re-descend against the
    /// FINAL column history, in which the arm has stamped its own seat column
    /// with its own `k_top + 1` — so the re-descent read that stamp and landed
    /// the arm on ITSELF at its own seat layer, a zero-height stub. It had
    /// fired zero times on eight models, which is unmeasured rather than safe.
    /// The un-merge now reads the straight descent RECORDED AT PLACEMENT.
    #[test]
    fn an_unmerged_arm_lands_below_its_own_seat_and_never_on_itself() {
        let tip_radius_max_mm = 1.0;
        // The trunk, clamped at the maximum radius at the layer the child lands
        // on: the area law wanted more section there and could not have it.
        let mut parent = arm(0, 0, 40, 10);
        parent.radius_mm = vec![tip_radius_max_mm; 30];
        parent.carried_mm2 = vec![50.0; 30];
        // The child: seated at 60, merged onto the trunk's shaft at 30.
        let mut child = arm(1, 7, 60, 30);
        child.landing = Landing::Strut { id: 0, layer: 30 };
        child.path = vec![7; 30];
        child.straight_fallback = Some((Landing::Plate, 0));
        let mut struts = vec![parent, child];

        let undone = unmerge_overgrown(&mut struts, tip_radius_max_mm);

        assert_eq!(undone, 1, "the merge onto a clamped trunk must be undone");
        let child = &struts[1];
        assert_eq!(child.landing, Landing::Plate);
        assert_eq!(child.k_bottom, 0);
        assert!(
            child.k_bottom < child.k_top,
            "an un-merged arm must still have height; {} .. {} is a stub",
            child.k_bottom,
            child.k_top
        );
        assert!(
            !matches!(child.landing, Landing::Strut { id, .. } if id == child.id),
            "an un-merged arm must never land on itself"
        );
        assert!(child.path.is_empty(), "an un-merged arm descends straight");
    }

    /// The counterpart: a trunk that is NOT clamped keeps its child, and an arm
    /// with no recorded fallback is never un-merged into a stub.
    #[test]
    fn a_trunk_with_room_to_grow_keeps_its_child_and_a_fallbackless_arm_is_left_alone() {
        let mut parent = arm(0, 0, 40, 10);
        parent.radius_mm = vec![0.4; 30];
        let mut child = arm(1, 7, 60, 30);
        child.landing = Landing::Strut { id: 0, layer: 30 };
        child.path = vec![7; 30];
        child.straight_fallback = Some((Landing::Plate, 0));
        let mut struts = vec![parent.clone(), child.clone()];
        assert_eq!(unmerge_overgrown(&mut struts, 1.0), 0);
        assert_eq!(struts[1].landing, Landing::Strut { id: 0, layer: 30 });

        parent.radius_mm = vec![1.0; 30];
        child.straight_fallback = None;
        let mut struts = vec![parent, child];
        assert_eq!(
            unmerge_overgrown(&mut struts, 1.0),
            0,
            "with no recorded straight descent there is nothing to put the arm back to"
        );
    }

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
            kind: crate::StrutKind::Arm,
            min_radius_mm: 0.0,
            seat_on_strut: None,
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
            path: Vec::new(),
            seat_solid_layers: 0,
            seat_normal: [0.0; 3],
            straight_fallback: None,
            fallback_path: Vec::new(),
        }];

        let mut refused = 0_u64;
        let removed =
            remove_under_loaded(&grid, &mut struts, minimum_load_mm2, 5.0, 6, true, &mut refused);

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

    /// One under-loaded arm at `cell 0`, and one healthy neighbour whose span
    /// the caller chooses. `neighbour_bottom == 0` puts a carrier beside the
    /// candidate on every layer it occupies; anything higher leaves the bottom
    /// of the candidate's span with nothing in reach.
    fn removal_pair(neighbour_bottom: usize) -> Vec<SweepStrut> {
        let bare = |id: u32, cell: usize, k_bottom: usize, carried: f64| SweepStrut {
            id,
            cell,
            center_mm: [0.0, 0.0],
            kind: crate::StrutKind::Arm,
            min_radius_mm: 0.0,
            seat_on_strut: None,
            k_top: 5,
            k_bottom,
            landing: Landing::Plate,
            interior: false,
            radius_mm: vec![0.1],
            carried_mm2: vec![carried],
            covered_demand: Vec::new(),
            inserted_by_load: false,
            path: Vec::new(),
            seat_solid_layers: 0,
            seat_normal: [0.0; 3],
            straight_fallback: None,
            fallback_path: Vec::new(),
        };
        // The candidate reads below the minimum load; the neighbour reads well
        // above it, so only one of the two is ever a candidate.
        vec![bare(0, 0, 0, 0.5), bare(1, 1, neighbour_bottom, 10.0)]
    }

    /// The counterpart: a REACH-placed arm (`inserted_by_load: false`) below
    /// the threshold, with no demand cells to protect it, is still removed —
    /// the fix must not blanket-protect every low-load arm, only the
    /// load-placed ones. Its neighbour stands beside it on every layer of its
    /// span, so the span test of 2026-09-06 has nothing to object to.
    #[test]
    fn a_reach_placed_arm_with_no_demand_to_protect_it_is_still_removed() {
        let (grid, _) = plan_grid(0.0, 0.0, 10.0, 10.0, 1, 1.0, 1.0, 1_000_000);
        let minimum_load_mm2 = 1.0;
        let mut struts = removal_pair(0);

        let mut refused = 0_u64;
        let removed =
            remove_under_loaded(&grid, &mut struts, minimum_load_mm2, 5.0, 6, true, &mut refused);

        assert_eq!(removed, 1, "a reach-placed arm below the threshold with nothing to protect it should still be removed");
        assert_eq!(refused, 0, "nothing was left uncovered, so nothing was refused");
        assert_eq!(struts.len(), 1);
        assert_eq!(struts[0].cell, 1, "the healthy neighbour is what survives");
    }

    /// **THE MOREL DRILL, and its sabotage in the same test.**
    ///
    /// The same pair, with the neighbour's own base three layers higher. The
    /// candidate's SEAT layer is still covered — layer 5, one cell away — so
    /// the seat-only rule of 2026-09-04 deletes it and leaves layers 0, 1 and
    /// 2 of its carried span with no carrier within reach. That is the shape
    /// of the Morel-base regression: `uncarried_load` 0.00 → 23.22 mm² with
    /// every removal reported safe.
    ///
    /// With the span test the arm is KEPT and counted in
    /// `strut_removals_refused_by_span`; with `span_check` off — the sabotage,
    /// which is the production knob `SweepPolicy::with_seat_only_removal`
    /// reads — the same arm on the same fixture is deleted. A fix that merely
    /// protected everything would fail the previous test; one that changed
    /// nothing fails this one.
    #[test]
    fn an_arm_whose_span_loses_its_carrier_below_the_seat_is_kept_and_the_seat_only_rule_drops_it()
    {
        let (grid, _) = plan_grid(0.0, 0.0, 10.0, 10.0, 1, 1.0, 1.0, 1_000_000);
        let minimum_load_mm2 = 1.0;

        let mut struts = removal_pair(3);
        let mut refused = 0_u64;
        let removed =
            remove_under_loaded(&grid, &mut struts, minimum_load_mm2, 5.0, 6, true, &mut refused);
        assert_eq!(
            removed, 0,
            "layers 0..2 of this arm's span have no carrier within reach once it goes; it must \
             be kept"
        );
        assert_eq!(refused, 1, "and the keep must be counted, not silent");
        assert_eq!(struts.len(), 2);

        let mut sabotaged = removal_pair(3);
        let mut sabotaged_refused = 0_u64;
        let sabotaged_removed = remove_under_loaded(
            &grid,
            &mut sabotaged,
            minimum_load_mm2,
            5.0,
            6,
            false,
            &mut sabotaged_refused,
        );
        assert_eq!(
            sabotaged_removed, 1,
            "the seat-only rule must still delete it, or this drill is not exercising the \
             difference the fix makes"
        );
        assert_eq!(sabotaged_refused, 0);
        assert_eq!(sabotaged.len(), 1);
    }
}
